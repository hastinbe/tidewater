use std::collections::BTreeMap;
use std::sync::Arc;

use tidewater_formats::aibin::AiFile;
use tidewater_formats::alow::Restrictions;
use tidewater_formats::pud::Pud;
use tidewater_formats::script::{Scripts, anim};
use tidewater_formats::tileset::Tileset;
use tidewater_formats::udta::{UnitStats, UnitTable, flags};
use tidewater_formats::ugrd::UpgradeTable;

use crate::TILE_PX;
use crate::ai::{AiPlayer, SCRIPT_PLAYERS};
use crate::anim::{Modifiers, Script};
use crate::battle::Missile;
use crate::combat::Upgrades;
use crate::economy::{Construction, Event, Job, JobKind};
use crate::rng::Rng;
use crate::setup::Start;
use crate::slots::{self, Slots};
use crate::terrain::{Terrain, bits, mask};
use crate::vision::Vision;

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("map has {got} tiles, expected {want}")]
    BadMap { got: usize, want: usize },
    #[error(transparent)]
    Upgrades(#[from] tidewater_formats::ugrd::UgrdError),
    #[error(transparent)]
    Restrictions(#[from] tidewater_formats::alow::AlowError),
}

/// Why a unit couldn't be created (unit-slots spec, "Creating a unit").
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SpawnError {
    #[error("too many units")]
    TooMany,
    #[error("can't place the unit there")]
    CantPlace,
}

/// Stable unit handle: the unit's creation number, never reused within a game. Slots are reused; ids aren't.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

/// Which occupancy plane a unit lives on.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Land,
    Sea,
    Air,
}

impl Layer {
    /// The movement-word bits that block this layer (occupancy spec, "Movement layers and blocking masks").
    pub fn mask(self) -> u16 {
        match self {
            Layer::Land => mask::LAND,
            Layer::Sea => mask::SEA,
            Layer::Air => mask::AIR,
        }
    }
}

/// Unit types that occupy tiles in their own way (occupancy spec, "Run-time marks"), in the PUD unit numbering.
const OIL_PATCH: u8 = 0x5d;
use crate::rescue::CIRCLE_OF_POWER;

/// A unit's order (orders spec, "The order list"); the spec's number is given on each.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Order {
    /// 2: stop / idle.
    Idle,
    /// Order 3. A smart move with a target unit that becomes the move keeps that unit as `target` until the move
    /// ends (orders spec, "Smart move", round-62 errata); every other move has none.
    Move { goal: (i32, i32), target: Option<UnitId> },
    /// 4: move, attacking on the way. A command 4 with a target unit walks to the target's tile and records the
    /// unit as `target` (orders spec, "Commands", round-49 errata).
    MoveAttack { goal: (i32, i32), target: Option<UnitId> },
    /// 5: patrol toward `to`; on arrival the ends swap.
    Patrol { to: (i32, i32), back: (i32, i32) },
    /// 6: follow a unit.
    Follow { target: UnitId },
    /// 7: follow and guard a unit.
    Guard { target: UnitId },
    /// 9: attack a unit.
    Attack { target: UnitId },
    /// 10: attack-move to a location.
    AttackMove { goal: (i32, i32) },
    /// 12: chase and attack a found target.
    Chase { target: UnitId },
    /// 13: stand ground.
    StandGround,
    /// 14: a tower watching; the target is found afresh each run.
    TowerWatch { target: Option<UnitId> },
    /// 15: attack a found target without moving.
    AttackInPlace { target: UnitId },
    /// 16: an idle unit's reaction: attack without moving.
    IdleReact { target: UnitId },
    /// Order 27: repair a building or transport (repair spec).
    Repair { target: UnitId },
    /// Order 23: gather from a gold mine or oil platform (`source`), or cut the tree at `tree` (harvest spec).
    Harvest {
        source: Option<UnitId>,
        tree: (i32, i32),
        chopping: bool,
    },
    /// Order 24: carry goods to a depot, found when `None`.
    ReturnGoods { depot: Option<UnitId> },
    /// Order 25: step into a mine, platform or depot; `stepped` once the step in has started.
    Enter { building: UnitId, stepped: bool },
    /// Order 26: inside a building; the coming-out search is sized by the unit's [`Unit::hall_exit`] mark. `None`
    /// once the mine or platform has collapsed, which clears the target (harvest spec, "Collapse").
    Inside { building: Option<UnitId> },
    /// Order 11: attack a wall tile (given by blocked row A of the movement spec).
    AttackWall { tile: (i32, i32) },
    /// 17: attack the ground at `tile` (attack-ground-demolish spec).
    AttackGround { tile: (i32, i32) },
    /// 18: move into range of `tile`, then attack the ground there. `goal` is the walk's goal: `tile` pulled back when
    /// the command is given (attack-ground-demolish spec, "Order 18", round-68 errata).
    AttackGroundMove { tile: (i32, i32), goal: (i32, i32) },
    /// 20: demolish at unit `at`, or at the obstacle `tile` when `at` is `None`.
    Demolish { at: Option<UnitId>, tile: (i32, i32) },
    /// 21: demolish at a location.
    DemolishAt { tile: (i32, i32) },
    /// 29: a transport putting its passengers ashore (transports spec).
    Unload,
    /// 30: a transport sailing to the shore-water tile `goal` in the landing class.
    Land { goal: (i32, i32) },
    /// 31: a transport leaving the shore for open water.
    PushOff,
    /// 32: a transport docked at the shore.
    Docked,
    /// 34: a land unit heading for a transport to board it.
    Board { transport: UnitId },
    /// 36: a passenger aboard a transport.
    Aboard { transport: UnitId },
    /// 28: a worker going to `site` (the top-left tile) to build `kind` (build-train spec, "Placing a building").
    /// `goal` is the build walk's goal, worked out when the command is given and then kept while it walks.
    Build {
        kind: u8,
        site: (i32, i32),
        goal: Option<(i32, i32)>,
    },
    /// 22: a worker inside the building it is constructing.
    /// The building is `None` for a builder let out of a cancelled site, whose target was cleared and whose pending idle
    /// has yet to take over (build-train spec, "Cancelling construction", V54).
    InsideSite { building: Option<UnitId> },
    /// Orders 38 to 56: casting `spell` (the order number) at a unit or, for the point spells, at `tile` (spells spec).
    Cast {
        spell: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
}

impl Order {
    /// The unit the order names as its target, as the unit's target field shows it: the unit attacked, followed,
    /// repaired, harvested from, delivered to, entered or boarded.
    pub fn target(self) -> Option<UnitId> {
        match self {
            Order::Follow { target }
            | Order::Guard { target }
            | Order::Attack { target }
            | Order::Chase { target }
            | Order::AttackInPlace { target }
            | Order::IdleReact { target }
            | Order::Repair { target } => Some(target),
            Order::TowerWatch { target }
            | Order::Cast { target, .. }
            | Order::MoveAttack { target, .. }
            | Order::Move { target, .. } => target,
            Order::Harvest { source, .. } => source,
            Order::ReturnGoods { depot } => depot,
            Order::Enter { building, .. } => Some(building),
            Order::InsideSite { building } => building,
            Order::Inside { building } => building,
            Order::Demolish { at, .. } => at,
            Order::Board { transport } | Order::Aboard { transport } => Some(transport),
            _ => None,
        }
    }
}

/// What a transport stores when given a command (transports spec, "What the transport stores"), and how it moves.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Ship {
    pub target: Option<UnitId>,
    pub target_tile: (i32, i32),
    /// The target tile as it was when the command was given; never changed afterwards.
    pub dest: (i32, i32),
    pub unload: bool,
    /// Moving in the landing class rather than the sea class.
    pub landing: bool,
    /// Moving one tile per step rather than two.
    pub one_tile: bool,
}

/// Passenger slots of a transport.
pub const PASSENGER_SLOTS: usize = 6;

/// What a worker gathers (harvest spec, "Gather mode and carried goods"). Tankers always gather oil.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Gather {
    #[default]
    None,
    Gold,
    Lumber,
}

/// A patrol or attack-move remembered in Enhanced mode, to resume after a fight (orders spec, "Resuming patrol and
/// attack-move").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Remembered {
    Patrol { to: (i32, i32), back: (i32, i32) },
    AttackMove { goal: (i32, i32) },
}

/// Where a unit goes back to after delivering.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Unit(UnitId),
    Tree((i32, i32)),
}

/// A unit's creation number. It doesn't join the sync hash: nothing in the simulation reads it (it only names the unit in
/// the trace), so keeping it out leaves every game's hash as it was.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(transparent)]
pub struct Serial(pub u32);

impl std::hash::Hash for Serial {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Unit {
    pub id: UnitId,
    /// The unit's creation number (`core/unit-slots.md`, "Creating a unit", step 3): 1 for the first unit of a map,
    /// then one more per created unit; a load renumbers every unit to its slot plus one. The trace format's `serial`.
    /// [`Unit::id`] stays the handle other state refers to, so a renumbering never has to rewrite a reference.
    pub serial: Serial,
    /// Place in the unit table; units update in ascending slot order.
    pub slot: u16,
    pub kind: u8,
    pub owner: u8,
    /// Top-left tile of the footprint; while stepping, already the destination tile (movement spec).
    pub tile: (i32, i32),
    /// Map pixel of the footprint's top-left corner: tile × 32 at rest, carried across by the move animation.
    pub pos: (i32, i32),
    /// 0 north, clockwise in eighths.
    pub facing: u8,
    pub hp: u16,
    pub layer: Layer,
    pub size: (u8, u8),
    pub building: bool,
    /// Trips left in a gold mine, oil patch or platform (harvest spec, "Mine and platform amounts").
    pub resources: u32,
    pub gather: Gather,
    /// Holding one load.
    pub carrying: bool,
    pub return_to: Option<Source>,
    /// Chop counter, 0 to 50.
    pub chop: u8,
    /// Inside a mine, platform or depot: hidden, untargetable and off the occupancy marks.
    pub inside: bool,
    /// Units inside this mine or platform; above 0 it shows its "active" look.
    pub occupants: u8,
    pub order: Order,
    pub script: Script,
    /// Haste/slow timer: positive is hasted, negative slowed.
    pub haste: i16,
    /// A building's stored `r mod 6` creation value (timing-and-rng spec; its use is not yet known).
    pub variation: u8,
    /// Cached route of up to 20 steps, as the tiles they reach, next first.
    pub path: Vec<(i32, i32)>,
    /// Blocked counter, 0 to 15 (movement spec, "Blocked rules").
    pub blocked: u16,
    /// A new order waiting for the next "step done" to take over.
    pub pending: Option<Order>,
    /// Enhanced mode's remembered patrol or attack-move, and the countdown before it may be resumed.
    pub remembered: Option<Remembered>,
    /// Transports only.
    pub ship: Ship,
    pub passengers: [Option<UnitId>; PASSENGER_SLOTS],
    pub resume: u8,
    /// Pathfinding spec, "Units seen by the search": the pending order came from a command and hasn't taken over.
    pub commanded: bool,
    /// The signed 16-bit counter the unit's "goal x" holds (repair spec, "Cost and rate"; build-train spec, "The
    /// construction pass"): a building's construction pass counter, kept after completion (12 x (T + 2), 0 for a
    /// building the map placed), and for a mobile unit the x of the tile its last command sent it to. A repair run
    /// adds 1 to it of whoever repairs, and pays when it is 2 or more, taking 2 off.
    pub goal_x: i16,
    /// Spell timers; running while above 0.
    pub bloodlust: u16,
    pub unholy_armor: u16,
    /// The last unit that attacked this one.
    pub attacked_by: Option<UnitId>,
    /// Playing its death animation; it no longer occupies tiles.
    pub dying: bool,
    /// Given to the neutral owner after dying; its slot is released at its next "step done".
    /// A dead body, wreck or ruin decaying (corpses spec); still `dying`.
    pub remains: bool,
    pub removing: bool,
    /// An oil patch killed under a new platform after its own update in that cycle: its removal starts at its next
    /// update (build-train spec, "Placing a building", round-65 errata).
    pub removal_due: bool,
    /// Set while a building is under construction.
    pub construction: Option<Construction>,
    /// A building's training or upgrade job.
    pub job: Option<Job>,
    /// The mana byte, and its regeneration counter (spells spec, "Timers"). It starts at the type's "magic" value
    /// (unit-slots spec, "Creating a unit", step 3). A berserker counts its regeneration cycles in it, a decaying unit
    /// its decay periods left, and a building its shown construction or job progress on a 0 to 255 scale
    /// (build-train spec).
    pub mana: u8,
    pub mana_counter: u8,
    /// Invisibility spell timer; the unit is invisible while above 0 (the vision rules read it).
    pub invisibility: u16,
    /// Flame-shield timer: only blocks a second flame shield (spells spec, "Timers").
    pub flame_shield: u16,
    /// The per-unit "used" mark (spells spec, polymorph): a dead body that keeps it can't be raised.
    pub used: bool,
    /// The caster AI's seven per-cycle target marks (`caster_ai::mark`), cleared by the timers pass.
    pub marks: u8,
    /// Created during play with a decay rate: it decays (spells spec, "Summoned units and decay").
    pub decays: bool,
    /// The cycles left in the current decay period; the periods left are in [`Unit::mana`].
    pub decay_count: u8,
    /// Players 0 to 7 that can see this unit at all (bit p), recomputed in phase 1 (vision spec, "Detection").
    pub detected_by: u8,
    /// Players 0 to 7 the fog hides this unit from (bit p), recomputed in phase 5 (vision spec, "Unit visibility").
    pub hidden_from: u8,
    /// The rescuable mark: the unit counts as rescuable (rescue spec, "Terms").
    pub rescuable: bool,
    /// The flash countdown after an owner change (rescue spec, "Flash"); not game state.
    pub flash: crate::rescue::Flash,
    /// The frozen state (rescue spec, "Frozen units").
    pub freeze: crate::rescue::Freeze,
    /// The held mark: placed by the map with a non-zero low byte in its data value; held units are left out of the
    /// computer's army counts (ai-scripts spec, "Conditions"). Nothing clears it (that spec's open question).
    pub held: bool,
    /// A computer worker's job, for the worker AI's bookkeeping.
    pub ai_job: crate::worker_ai::AiJob,
    /// The computer army's goal for this unit (attack-ai spec, "Unit goals").
    pub goal: crate::attack_ai::Goal,
    /// A computer building's think counter (training-research spec, "When a building thinks").
    pub think: u16,
    /// The colour player: the owner the unit was created with (minimap spec, "Dot colour"). It names a player, not a
    /// palette colour: the unit is drawn in that player's [`Player::colour`].
    pub colour: u8,
    /// The minimap's attack mark (minimap spec, "Attack mark"); not game state.
    pub attack_mark: crate::minimap::Mark,
    /// A building's remembered look for the local player (map-drawing spec, "Buildings and remembered buildings");
    /// display state, not game state.
    pub look: crate::minimap::Look,
    /// A ship's or flier's bob countdown and value (orders spec, "Idle look-around").
    pub bob: crate::idle::Bob,
    /// The attack cooldown in cycles (damage spec, "Attack timing (cooldown)"): set to the type's attack loop + 1 at
    /// each strike decision, counted down every cycle while above 0 (not for transports).
    pub cooldown: u16,
    /// The slot of a dying unit's target, kept through its death animation (damage spec, "Applying damage") and as
    /// remains. Reporting only: a dead unit keeps reporting this slot after the target's slot is released (trace-format
    /// spec, round-44 errata).
    pub dying_target: Option<u16>,
    /// A killer's forgotten target (damage spec, "Applying damage"): while the unit's order is still this one, the
    /// order has no target.
    pub forgot: Option<Order>,
    /// The hall-sized exit mark (harvest spec, "Coming out", step 6): set by every delivery at a gold-accepting
    /// building and for a hall's builder at completion; the next come-out uses the 4 × 4 source and clears it.
    pub hall_exit: bool,
    /// Set by order 12's no-route fallback (orders spec, "Order 12 with no route"): the next order-logic run skips its
    /// target search if its order is idle or an attack order, and the mark is cleared at the end of that run unless
    /// it ended in a take-over or a move step.
    pub skip_search: bool,
}

impl Unit {
    pub fn is_moving(&self) -> bool {
        self.script.anim == anim::MOVE
    }

    /// The footprint's centre in map pixels, where missiles start and aim (damage spec).
    pub fn centre_px(&self) -> (i32, i32) {
        (
            self.pos.0 + self.size.0 as i32 * TILE_PX / 2,
            self.pos.1 + self.size.1 as i32 * TILE_PX / 2,
        )
    }

    pub fn alive(&self) -> bool {
        !self.dying
    }

    /// The unit type whose picture draws this unit: during a building upgrade, the new type's, while the type stays
    /// the old one (build-train spec, "Starting"). Which frame of it the upgrade's frame 1 shows is that spec's open
    /// question "Upgrade picture"; we draw frame 1.
    pub fn picture(&self) -> u8 {
        match self.job {
            Some(crate::economy::Job {
                kind: crate::economy::JobKind::Upgrade(to),
                ..
            }) => to,
            _ => self.kind,
        }
    }

    /// Alive and not inside a building: something other units can target.
    pub fn present(&self) -> bool {
        !self.dying && !self.inside
    }

    /// Whether the unit stays in selections and groups ("Losing a unit from the selection" in the selecting spec):
    /// present, and not entering a mine, platform or depot. Order 25 counts from the update it is given, before the
    /// unit steps in (harvest spec, "Entering"; V93), and can't be interrupted, so the unit never comes back.
    pub fn stays_selected(&self) -> bool {
        self.present() && !matches!(self.order, Order::Enter { .. })
    }

    /// The target unit the unit records (trace-format spec, `target`): a command's target replaces the old one at
    /// once, even with none; a killer whose order is still the one whose target it forgot records none (damage spec,
    /// "Applying damage"); otherwise the pending order's target, or the order's.
    pub fn recorded_target(&self) -> Option<UnitId> {
        if let (true, Some(p)) = (self.commanded, self.pending) {
            p.target()
        } else if self.pending.is_none() && self.forgot == Some(self.order) {
            None
        } else {
            self.pending.and_then(Order::target).or_else(|| self.order.target())
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Player {
    pub gold: u32,
    pub lumber: u32,
    pub oil: u32,
    /// Race from the map's `SIDE` section: 0 human, 1 orc, 2 neutral.
    pub race: u8,
    pub start: Option<(i32, i32)>,
    pub upgrades: Upgrades,
    /// Enemy units (types below the first building type) this player's units killed (campaign-flow spec, "Mission
    /// score"); a kill of the player's own unit isn't counted.
    pub kills: u32,
    /// Enemy buildings this player's units destroyed: the razings.
    pub razings: u32,
    /// The mission score: the point values of the enemy units and buildings this player's units killed. The score
    /// screen adds the victory bonus (campaign-flow spec, "The score screen").
    pub score: u32,
    /// How many times the player's count of units (not buildings) went down, by a death or a change of owner.
    pub units_lost: u32,
    /// The same for the player's buildings.
    pub buildings_lost: u32,
    /// Whether the player has ever had a completed town hall, keep or castle (build-train spec, "fast first hall").
    pub had_hall: bool,
    pub under_construction: u16,
    pub in_training: u16,
    /// Controller from the map's `OWNR` section (values in [`crate::ai::controller`]).
    pub controller: u8,
    /// Gold, lumber and oil delivered over the game ("total gathered" statistics).
    pub gathered: [u32; 3],
    /// Computer script number from the map's `AIPL` section.
    pub ai_script: u8,
    /// Players this player shares its vision with, as a bit set (vision spec). Only read in Enhanced mode. Game setup
    /// starts it as the player alone (setup spec); diplomacy is a later spec.
    pub shared_vision: u8,
    /// Level of each upgrade line, groups 0 to 10 (tech-tree spec, "Upgrade lines and their effects").
    pub levels: [u8; crate::tech::LINES],
    /// The map's per-player words (tech-tree spec, "Per-player words from the map"): units and buildings allowed,
    /// spells known, spells allowed to research, spells being researched, upgrades allowed, upgrades being researched.
    pub units_allowed: u32,
    pub spells_known: u32,
    pub spells_allowed: u32,
    pub spells_researching: u32,
    pub upgrades_allowed: u32,
    pub upgrades_researching: u32,
    /// Players 0 to 15 this player counts as allied (bit q), from game setup's alliance table (setup spec); a player
    /// always counts as allied with itself.
    pub allies: u16,
    /// The player colour its units are drawn in: in single player, slot p uses colour p (setup spec, "Colours"). Units
    /// pick it through their colour player ([`Unit::colour`]), so a unit that changes owner keeps its creator's colour.
    pub colour: u8,
    /// The allied-victory setting (victory spec, "The multiplayer rule").
    pub allied_victory: bool,
    /// A human player no longer connected.
    pub disconnected: bool,
    /// Rescuable units brought to a circle of power, plus 0x1000 for a hero when the mission asks for one: an
    /// unsigned 16-bit value (victory spec, "Terms"; rescue spec).
    pub rescued: u16,
    /// The converted tally: units and buildings that changed owner to this player by rescue (rescue spec, "Owner
    /// change"); mission mode F reads it.
    pub converted: u16,
    pub result: Option<crate::victory::Outcome>,
}

/// Animation number marking a unit whose slot was released this cycle (no real animation uses it).
pub const RELEASED: u8 = 0xff;

/// Numbers that specs will replace. Every field here is a placeholder of our own choosing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rules {
    /// Unit and effect table sizes (unit-slots spec): Enhanced unless set otherwise.
    pub unit_slots: usize,
    pub effect_slots: usize,
    /// Enhanced rather than Classic mode, and a multiplayer game.
    pub enhanced: bool,
    pub multiplayer: bool,
    pub build_speed_cheat: bool,
    /// The fog-of-war option (vision spec, "Fog-of-war option and cheat"). Switch it during play with
    /// [`World::set_fog_of_war`].
    pub fog_of_war: bool,
    /// The reveal-map cheat: nothing is hidden and the whole minimap shows.
    pub reveal_map: bool,
    /// Whether training and building-upgrade requirements are checked when a command arrives (tech-tree spec, "When
    /// requirements are enforced"). Always on in single player and in our own multiplayer.
    pub enforce_requirements: bool,
    /// The instant-victory, instant-defeat and disable-victory cheats (victory spec).
    pub instant_victory: bool,
    pub instant_defeat: bool,
    pub disable_victory: bool,
    /// The all-spells cheat (spells spec, "Giving the order"): a spell command first sets the caster's mana to 255.
    pub all_spells_cheat: bool,
    /// The lumber cheat: each chop finishes the tree at once (harvest spec).
    pub lumber_cheat: bool,
    /// The invulnerability and damage cheat (damage spec, "Cheat").
    pub invulnerable_cheat: bool,
    /// The lobby option that disables cheats; it only matters in multiplayer games (cheats spec).
    pub cheats_disabled: bool,
    /// The optional improvements that touch the sim (`docs/design/extras.md`); all off by default. Stored after the
    /// world's own fields (`world::save`), not with the rules, so the stored layout stays what it was.
    #[serde(skip)]
    pub extras: crate::Extras,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            unit_slots: slots::ENHANCED_SLOTS,
            effect_slots: slots::ENHANCED_EFFECTS,
            enhanced: true,
            multiplayer: false,
            build_speed_cheat: false,
            fog_of_war: true,
            reveal_map: false,
            enforce_requirements: true,
            instant_victory: false,
            instant_defeat: false,
            disable_victory: false,
            all_spells_cheat: false,
            lumber_cheat: false,
            invulnerable_cheat: false,
            cheats_disabled: false,
            extras: crate::Extras::default(),
        }
    }
}

/// Tiles one loop of a unit type's move script covers: 1, 2 for ships and flyers, 0 if it has no move loop.
/// Measured by running the script facing east (animation-scripts spec, "Measured timings").
pub fn move_stride(scripts: &Scripts, kind: u8) -> u8 {
    let Some(pc) = scripts.unit(kind, anim::MOVE) else {
        return 0;
    };
    let mut s = Script::default();
    s.start(anim::MOVE, pc);
    let (mut dones, mut dx) = (0, 0);
    for _ in 0..1024 {
        let fx = s.update(scripts.bytes(), 2, Modifiers::default());
        if dones == 1 {
            dx += fx.dx;
        }
        if fx.step_done {
            dones += 1;
            if dones == 2 {
                return (dx / TILE_PX).clamp(0, 2) as u8;
            }
        }
    }
    0
}

/// Cycles between two strikes of a unit type's attack script (damage spec, "Attack timing (cooldown)", the "loop"
/// column, measured from the script with plain waits), or between two step dones for a script with no strike point;
/// 0 if it has no attack animation.
pub fn attack_loop(scripts: &Scripts, kind: u8) -> u16 {
    let Some(pc) = scripts.unit(kind, anim::ATTACK) else {
        return 0;
    };
    let mut s = Script::default();
    s.start(anim::ATTACK, pc);
    let (mut strikes, mut dones) = (vec![], vec![]);
    for c in 0..2048u16 {
        let fx = s.update(scripts.bytes(), 0, Modifiers::default());
        if fx.strikes > 0 {
            strikes.push(c);
        }
        if fx.step_done {
            dones.push(c);
        }
        if strikes.len() == 2 {
            return strikes[1] - strikes[0];
        }
    }
    if strikes.is_empty() && dones.len() >= 2 {
        return dones[1] - dones[0];
    }
    0
}

/// A player's instruction, applied at the start of the tick it is scheduled for. It is serialisable so that a network
/// turn can carry it (`docs/design/multiplayer-determinism.md`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    Move {
        player: u8,
        units: Vec<UnitId>,
        to: (i32, i32),
    },
    Stop {
        player: u8,
        units: Vec<UnitId>,
    },
    Attack {
        player: u8,
        units: Vec<UnitId>,
        target: UnitId,
    },
    /// Start training a unit at a building (tech-tree spec, "Starting a job").
    Train {
        player: u8,
        building: UnitId,
        kind: u8,
    },
    /// Start upgrading a building to another type.
    Upgrade {
        player: u8,
        building: UnitId,
        to: u8,
    },
    /// Start an upgrade job, upgrade numbers 0 to 51 (weapons, armour, training and so on).
    Research {
        player: u8,
        building: UnitId,
        upgrade: u8,
    },
    /// Start a spell job, upgrade numbers 0 to 51 (the spells, and the paladin and ogre-mage upgrades).
    Spell {
        player: u8,
        building: UnitId,
        spell: u8,
    },
    /// Command 27: workers repair a building or transport, or walk toward anything else.
    Repair {
        player: u8,
        units: Vec<UnitId>,
        target: UnitId,
    },
    /// Command 23: gather from `target` (or the ground unit on `tile`), or cut lumber at `tile`.
    Harvest {
        player: u8,
        units: Vec<UnitId>,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Command 24: carry goods to the nearest depot.
    ReturnGoods {
        player: u8,
        units: Vec<UnitId>,
    },
    /// Command 3, smart move (a right click): follow, harvest, return goods or move, by the target.
    Smart {
        player: u8,
        units: Vec<UnitId>,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Command 8 on a location: attack-move.
    AttackMove {
        player: u8,
        units: Vec<UnitId>,
        to: (i32, i32),
    },
    /// Command 5.
    Patrol {
        player: u8,
        units: Vec<UnitId>,
        to: (i32, i32),
    },
    /// Command 13.
    StandGround {
        player: u8,
        units: Vec<UnitId>,
    },
    /// Command 27 with a building type: a worker builds `kind` with its top-left tile at `tile`.
    Build {
        player: u8,
        unit: UnitId,
        kind: u8,
        tile: (i32, i32),
    },
    /// Command 17: attack the ground at `tile` (attack-ground-demolish spec).
    AttackGround {
        player: u8,
        units: Vec<UnitId>,
        tile: (i32, i32),
    },
    /// Command 19: demolish at `target`, or at `tile` when there is none.
    Demolish {
        player: u8,
        units: Vec<UnitId>,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Command 29: a transport unloads at `tile` (transports spec).
    Unload {
        player: u8,
        units: Vec<UnitId>,
        tile: (i32, i32),
    },
    /// Cancel construction: the building is taken down if it is under construction; otherwise nothing happens
    /// (build-train spec, "Cancelling construction", V69).
    Cancel {
        player: u8,
        unit: UnitId,
    },
    /// Cancel job: a working building's job is marked to be cancelled at its next work run; otherwise nothing
    /// happens (build-train spec, "Cancelling a job", V69).
    CancelJob {
        player: u8,
        unit: UnitId,
    },
    /// A spell command (spells spec, "Giving the order"): `spell` is the spell's order number, 38 to 56. Unit spells
    /// need `target`; point spells use `tile` only.
    Cast {
        player: u8,
        unit: UnitId,
        spell: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Replaces the player's synced selection with up to 9 units (selecting-and-commanding spec, "Two selections"), or
    /// up to the limit of the optional `selection-limit` improvement.
    Select {
        player: u8,
        units: Vec<UnitId>,
    },
    /// The right-click command for the player's synced selection: each unit picks a command by its type's right-click
    /// action ("What a right-click command does").
    RightClick {
        player: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Command `command` (numbered as in the orders spec) from target mode, the command buttons or the keys, for the
    /// units of the player's synced selection that pass the command filter ("Which units take part in a command").
    Issue {
        player: u8,
        command: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Fills control group `group` from the player's synced selection ("Control groups").
    SetGroup {
        player: u8,
        group: u8,
    },
    /// Tidies control group `group` and makes its units the player's synced selection.
    RecallGroup {
        player: u8,
        group: u8,
    },
    /// Sets the game speed (`net/lockstep.md`, "Speed"; `ui/selecting-and-commanding.md`, "Keys"). The command carries
    /// the speed to set, not a step: the + and - keys send the speed the sender's game is at, one higher or lower, and
    /// the Speeds dialog its slider's value. A value above the top speed is ignored (round 100: NC1, NC2).
    Speed {
        player: u8,
        speed: u8,
    },
    /// Pause (`ui/game-menu.md`, "Pausing"): does nothing if the game is already paused. In Enhanced multiplayer it
    /// spends one of the player's [`PAUSES_PER_PLAYER`] pauses, and does nothing for a player with none left.
    Pause {
        player: u8,
    },
    /// Info-panel spec, "Clicking an icon": the player puts the passenger in `slot` of transport `unit` ashore (the
    /// transports spec's one-passenger rule). Ignored unless the player owns the transport and it is unloading or
    /// docked.
    UnloadOne {
        player: u8,
        unit: UnitId,
        slot: u8,
    },
    /// A recognised cheat line (cheats spec): `flags` becomes the cheat flags (a [`crate::cheats::Flags`] value).
    Cheat {
        player: u8,
        flags: u32,
    },
    /// The game menu's Surrender (`ui/game-menu.md`, "Ending the scenario"): the player is defeated and the game is over
    /// at once. Single player only so far.
    ///
    /// TODO(multiplayer): `net/leave-and-drop.md`, "Surrender and quit (draw)": in a network game surrender is not a
    /// turn command. No command is sent and no result is set at that moment; the leaver's machine leaves the session
    /// as its game ends, the others see "left" (network entry 64) or "has left" (entry 111) and mark the slot out at
    /// their next victory check. Until the network phases land this variant is not sent in a multiplayer game.
    Surrender {
        player: u8,
    },
    /// The game menu's Quit (draw) (`ui/game-menu.md`; Battle.net, at most 120 s into the game). Locally exactly a
    /// surrender; the difference is only the result reported to Battle.net.
    ///
    /// TODO(multiplayer): `net/leave-and-drop.md`, "Surrender and quit (draw)" and the report table: a game that ends
    /// within 120 s of game time is reported as a draw for every player, whoever quit. That report is a later phase.
    QuitDraw {
        player: u8,
    },
    /// A player leaves a network game (`net/leave-and-drop.md`, "Chat-room game: carrying out a leave", steps 3 and
    /// 4): the player's slot becomes "nobody" and, under Enhanced rules, it leaves every shared-vision set. Its units
    /// keep their orders and no result is set. `dropped` only tells the notice apart ("left" or "was dropped"); it
    /// changes nothing in the world. The network session makes this command (the leaver's own last packet, or the
    /// drop every machine agrees on), never the interface. Ignored for a slot that is not a human.
    Leave {
        player: u8,
        dropped: bool,
    },
    /// Resume (`ui/game-menu.md`, "Resuming"): does nothing if the game is not paused. Uses no pause.
    Resume {
        player: u8,
    },
    /// Optional improvement (`rally.rs`): sets the rally point of the player's training building `building` to `target`
    /// (or `tile`); a target equal to the building clears it. Does nothing unless rally points are on.
    Rally {
        player: u8,
        building: UnitId,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
    /// Optional improvement (`waypoints.rs`): the wrapped unit command, given with Shift held, joins the order queue of
    /// each unit that takes it instead of replacing its order. `player` must be the wrapped command's own player.
    /// Does nothing unless shift-queued orders are on; a nested `Queued` is ignored.
    Queued {
        player: u8,
        command: Box<Command>,
    },
    /// The diplomacy command (`game/diplomacy.md`): the player's whole alliance, shared-vision and allied-victory
    /// settings, sent each time its diplomacy screen is confirmed. `alliances` holds one value for each player 0 to
    /// 7 (0 not allied, 1 to 3 allied; in Classic mode only 0 and 1), `vision` the new shared-vision set as a bit set
    /// of players 0 to 7 (unused in Classic mode). A packet acts as the slot it came from, so a command naming
    /// another player never reaches the world from the network (DP2).
    Diplomacy {
        player: u8,
        alliances: [u8; 8],
        vision: u8,
        allied_victory: bool,
    },
    /// Hold workers (`orders/orders.md`, "Holding and releasing workers"): the interface sends it when the player
    /// picks a building to place. The sender's selected workers are held: each finishes its step and stays frozen.
    HoldWorkers {
        player: u8,
    },
    /// Release workers (the same section): sent when placement mode ends without a placement. A unit command and a
    /// build command release the sender's selected workers themselves, before they act.
    ReleaseWorkers {
        player: u8,
    },
}

impl Command {
    /// The player the command acts as, which a network session checks against the slot a packet came from.
    pub fn player(&self) -> u8 {
        match *self {
            Command::Move { player, .. }
            | Command::Stop { player, .. }
            | Command::Attack { player, .. }
            | Command::Train { player, .. }
            | Command::Upgrade { player, .. }
            | Command::Research { player, .. }
            | Command::Spell { player, .. }
            | Command::Repair { player, .. }
            | Command::Harvest { player, .. }
            | Command::ReturnGoods { player, .. }
            | Command::Smart { player, .. }
            | Command::AttackMove { player, .. }
            | Command::Patrol { player, .. }
            | Command::StandGround { player, .. }
            | Command::Build { player, .. }
            | Command::AttackGround { player, .. }
            | Command::Demolish { player, .. }
            | Command::Unload { player, .. }
            | Command::Cancel { player, .. }
            | Command::CancelJob { player, .. }
            | Command::Cast { player, .. }
            | Command::Select { player, .. }
            | Command::RightClick { player, .. }
            | Command::Issue { player, .. }
            | Command::SetGroup { player, .. }
            | Command::RecallGroup { player, .. }
            | Command::Speed { player, .. }
            | Command::Pause { player }
            | Command::UnloadOne { player, .. }
            | Command::Cheat { player, .. }
            | Command::Surrender { player }
            | Command::QuitDraw { player }
            | Command::Leave { player, .. }
            | Command::Resume { player }
            | Command::Rally { player, .. }
            | Command::Queued { player, .. }
            | Command::Diplomacy { player, .. }
            | Command::HoldWorkers { player }
            | Command::ReleaseWorkers { player } => player,
        }
    }

    /// The command number of a unit command whose row of the selection filter (selecting-and-commanding spec,
    /// "Which units take part in a command") asks for flag bit 19 (can attack): attack (8), patrol (5; the filter's
    /// Enhanced exceptions included), stand ground (13), attack the ground (17) and demolish (19). `None` for the rest.
    pub(crate) fn needs_attacker(&self) -> Option<u8> {
        Some(match self {
            Command::Attack { .. } | Command::AttackMove { .. } => 8,
            Command::Patrol { .. } => 5,
            Command::StandGround { .. } => 13,
            Command::AttackGround { .. } => 17,
            Command::Demolish { .. } => 19,
            _ => return None,
        })
    }
}

pub const PLAYERS: usize = 16;
/// The pauses each player starts an Enhanced multiplayer game with (`ui/game-menu.md`, "Pausing").
pub const PAUSES_PER_PLAYER: u8 = 3;
/// Cycles run at once when a game starts (timing-and-rng spec, "Scheduler", rule 1).
pub const START_UP_CYCLES: u32 = 2;

#[derive(Clone, Debug)]
pub struct World {
    pub tick: u32,
    pub terrain: Terrain,
    pub stats: UnitTable,
    pub rules: Rules,
    pub players: [Player; PLAYERS],
    pub rng: Rng,
    pub scripts: Arc<Scripts>,
    /// Per unit type, from [`move_stride`].
    pub stride: Vec<u8>,
    /// Per unit type, from [`attack_loop`].
    pub attack_loop: Vec<u16>,
    /// Live units, sorted by slot.
    pub units: Vec<Unit>,
    /// The effect table: missiles by effect slot. The first slots hold the permanent effects and stay `None` here.
    pub missiles: Vec<Option<Missile>>,
    pub slots: Slots,
    slot_of: BTreeMap<UnitId, u16>,
    /// Messages for the player, drained by the client. Not game state.
    pub events: Vec<Event>,
    /// Computer player scripts (ai-scripts spec), one state per player 0 to 7.
    pub ai: [AiPlayer; SCRIPT_PLAYERS],
    /// Per computer player, the surplus on the footmen-to-submarines counts (`game/save-load.md` SL4 to
    /// SL6, SL13 to SL15): each load adds the counts at the save, and the rules add it to every later count. Stored after the rest of
    /// the world's state (save version 16: eleven groups; version 15 had four), and left out of [`World::fingerprint`] while it is all zero.
    pub(crate) group_surplus: [[u32; crate::training_ai::DOUBLED_GROUPS]; SCRIPT_PLAYERS],
    /// The computer players' army: threat map, base boxes, pass counter (attack-ai spec).
    pub army: crate::attack_ai::Army,
    /// Upgrade data: the defaults, or the map's UGRD section (tech-tree spec, "Default upgrade data").
    pub upgrade_data: UpgradeTable,
    pub(crate) ai_file: Option<Arc<AiFile>>,
    next_id: u32,
    /// The creation number the next unit gets (`core/unit-slots.md`, step 3). It follows `next_id` until a load
    /// renumbers the units; then it is one past the largest number in use (`game/save-load.md`, "Open questions").
    next_serial: u32,
    /// The rule that was in force when a check found an outcome, which replaced it ([`crate::victory::Rule::Outcome`]).
    /// A load loses the found outcome and puts this rule back (`game/save-load.md`, "Victory countdown").
    pub(crate) rule_before_outcome: Option<crate::victory::Rule>,
    /// Fog maps and "seen" bits (vision spec).
    pub vision: Vision,
    /// Players 0 to 7 whose own shared-vision set doesn't hold themselves, which only a diplomacy command can cause
    /// (`game/diplomacy.md`, "A player outside its own set"). Kept apart from [`Player`] so a game without such a
    /// command hashes and saves as it always has; see [`World::shared_set`].
    pub vision_outsiders: u8,
    /// The victory check (victory spec).
    pub victory: crate::victory::Victory,
    /// The ground-unit and air-unit pointer of each tile (occupancy spec, "Run-time marks"). Land units, ships and
    /// buildings share the ground plane.
    pub(crate) occupancy: [Vec<Option<UnitId>>; 2],
    /// Every unit in vertical order, re-sorted once per cycle (orders spec, "Vertical order").
    pub(crate) ylist: Vec<UnitId>,
    /// The runes on the map, 50 slots shared by all players (spells spec, "Runes on the map").
    pub runes: Vec<Option<crate::spells::Rune>>,
    /// Set while the map's units are being created: they get no decay countdown (spells spec, "Summoned units").
    pub(crate) loading: bool,
    /// Sound cues and the interface state behind them (sounds spec). Not game state: left out of the hash.
    pub sounds: crate::sound::Sounds,
    /// What released unit slots still hold (unit-slots spec, "Death, corpses and release"; caster-ai spec, open
    /// question "Free slots in the random search"). A slot missing here and not in [`World::units`] reads as never
    /// used: all zero, owner 14.
    pub(crate) stale: BTreeMap<u16, crate::caster_ai::Stale>,
    /// Spell counts for inspection tools. Not game state: left out of the hash.
    pub casts: crate::caster_ai::CastCounts,
    /// Each player's synced selection and control groups (selecting-and-commanding spec).
    pub selections: [crate::select::PlayerSelection; PLAYERS],
    /// The game speed, 0 to 8 (timing-and-rng spec, the 9 levels), and whether the game is paused.
    pub speed: u8,
    pub paused: bool,
    /// Each player's pauses left (`ui/game-menu.md`, "Pausing"). Only Enhanced multiplayer spends them, and only
    /// there do they join the state hash.
    pub pauses: [u8; PLAYERS],
    /// What the optional improvements keep: rally points, queued jobs, the selection beyond nine places
    /// (`docs/design/extras.md`). Empty while they are off. Hashed once an option is on, stored after the other fields.
    pub ext: crate::extras::State,
    /// The local player's remembered buildings for the minimap (minimap spec). Not game state: left out of the hash.
    pub remembered: crate::minimap::Memory,
    /// The starting view and sound set game setup chose. Not game state: left out of the hash.
    pub start: crate::setup::Started,
    /// The cheat flags (cheats spec); hashed once any is on.
    pub cheats: crate::cheats::Cheats,
    /// Order-12 restarts by the no-route fallback counted so far, to tell whether a move step ended in one (the skip
    /// mark). Not game state: not stored and left out of the hash.
    pub(crate) restarts: u32,
    /// Set while order 12's no-route fallback runs the order's logic once after its take-over (orders spec, "Order 12
    /// with no route", round-54 errata). Not game state: not stored and left out of the hash.
    pub(crate) fallback_run: bool,
    /// The slot whose update is running, during the cycle's walk of the unit table. Not game state: not stored and
    /// left out of the hash.
    pub(crate) updating: Option<u16>,
    /// Set from the detection phase to the end of a cycle: a unit created meanwhile starts with an empty detected-by
    /// set (fog spec, "A unit created during a cycle"). Not game state: always clear between cycles, so not stored
    /// and left out of the hash.
    pub(crate) in_cycle: bool,
    /// Which kind of player command is being carried out (`waypoints.rs`); `Off` between commands and always when
    /// shift-queued orders are off. Not game state: not stored and left out of the hash.
    pub(crate) queue_scope: crate::waypoints::Scope,
    /// The unit whose command was just queued, so the attacker it fights is not forgotten for a command that did not
    /// take over (`waypoints.rs`). Not game state.
    pub(crate) just_queued: Option<UnitId>,
    /// A network observer's machine (`docs/design/network.md`, "Observers"): the display shows the whole map, as the
    /// reveal-map cheat does, **without** the sim hiding anything differently. [`World::reveal_view`] is what the
    /// display reads; the sim reads `rules.reveal_map` only, so targeting and every other rule are the players'.
    /// Not game state: not stored and left out of the hash.
    pub observing: bool,
}

/// The hasher behind [`World::state_hash`]: FNV-1a over bytes, with `usize` and `isize` (vector lengths, enum
/// discriminants) hashed at a fixed 64-bit width so that a 32-bit machine and a 64-bit one agree.
struct Fnv(u64);

impl std::hash::Hasher for Fnv {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn write_usize(&mut self, n: usize) {
        self.write(&(n as u64).to_le_bytes());
    }
    fn write_isize(&mut self, n: isize) {
        self.write(&(n as i64).to_le_bytes());
    }
}

mod diff;
mod save;
pub use diff::Difference;
pub use save::RestoreError;

pub(crate) fn layer_index(l: Layer) -> usize {
    match l {
        Layer::Land | Layer::Sea => 0,
        Layer::Air => 1,
    }
}

/// Animation-scripts spec, "Facing a tile": the facing from tile `a` toward tile `b`. A straight facing unless the
/// smaller distance is at least half the larger one (rounded down, and at least 1); a unit facing its own tile faces
/// east.
pub(crate) fn face_tile(a: (i32, i32), b: (i32, i32)) -> u8 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (ax, ay) = (dx.abs(), dy.abs());
    let diagonal = |dx: i32, dy: i32| match (dx >= 0, dy >= 0) {
        (true, false) => 1,
        (true, true) => 3,
        (false, true) => 5,
        (false, false) => 7,
    };
    if ax < ay {
        if ax < (ay / 2).max(1) {
            if dy < 0 { 0 } else { 4 }
        } else {
            diagonal(dx, dy)
        }
    } else if ay < (ax / 2).max(1) {
        if dx < 0 { 6 } else { 2 }
    } else {
        diagonal(dx, dy)
    }
}

/// The tile B that "Facing a tile" aims at for a target unit: a building's top-left tile plus (1, 1), any other unit's
/// own (claimed) tile.
pub(crate) fn facing_tile_of(t: &Unit) -> (i32, i32) {
    if t.building {
        (t.tile.0 + 1, t.tile.1 + 1)
    } else {
        t.tile
    }
}

pub(crate) fn facing(dx: i32, dy: i32) -> u8 {
    match (dx.signum(), dy.signum()) {
        (0, -1) => 0,
        (1, -1) => 1,
        (1, 0) => 2,
        (1, 1) => 3,
        (0, 1) => 4,
        (-1, 1) => 5,
        (-1, 0) => 6,
        (-1, -1) => 7,
        _ => 4,
    }
}

impl World {
    /// An empty world on `terrain`, with the generator seeded.
    pub fn new(terrain: Terrain, stats: UnitTable, scripts: Arc<Scripts>, seed: u32) -> Self {
        let n = terrain.width as usize * terrain.height as usize;
        let stride = (0..tidewater_formats::script::UNIT_TYPES as u8)
            .map(|k| move_stride(&scripts, k))
            .collect();
        let attack_loop = (0..tidewater_formats::script::UNIT_TYPES as u8)
            .map(|k| attack_loop(&scripts, k))
            .collect();
        let rules = Rules::default();
        let (terrain_w, terrain_h) = (terrain.width as usize, terrain.height as usize);
        let army = crate::attack_ai::Army::new(&terrain);
        World {
            tick: 0,
            occupancy: std::array::from_fn(|_| vec![None; n]),
            terrain,
            stats,
            slots: Slots::new(rules.unit_slots),
            slot_of: BTreeMap::new(),
            events: Vec::new(),
            ai: Default::default(),
            group_surplus: Default::default(),
            army,
            upgrade_data: crate::tech::default_upgrades(),
            ai_file: None,
            missiles: vec![None; rules.effect_slots],
            rules,
            players: std::array::from_fn(|i| Player {
                colour: i as u8,
                ..Default::default()
            }),
            rng: Rng::new(seed),
            scripts,
            stride,
            attack_loop,
            units: Vec::new(),
            ylist: Vec::new(),
            runes: vec![None; crate::spells::RUNE_SLOTS],
            loading: false,
            sounds: Default::default(),
            stale: BTreeMap::new(),
            casts: Default::default(),
            restarts: 0,
            fallback_run: false,
            updating: None,
            in_cycle: false,
            queue_scope: Default::default(),
            just_queued: None,
            observing: false,
            selections: Default::default(),
            speed: crate::timing::NORMAL,
            paused: false,
            pauses: [PAUSES_PER_PLAYER; PLAYERS],
            ext: Default::default(),
            remembered: Default::default(),
            start: Default::default(),
            cheats: Default::default(),
            next_id: 1,
            next_serial: 1,
            rule_before_outcome: None,
            vision: Vision::new(terrain_w, terrain_h),
            vision_outsiders: 0,
            victory: Default::default(),
        }
    }

    /// Builds the starting state of a map for the players and choices of `start` ([`Start::choose`]), making steps 5
    /// to 9 of the setup spec's "Order of the steps": the starting alliances and shared vision, the map's units, the
    /// rescue players' new controllers, the starting view, the sound set and, on a campaign map, the mission start.
    /// `tileset` must be the one for `start.era`. `upgrades` is the installation's default upgrade data
    /// (`upgrades.dat`), which the map's own data replaces when its "use default" word is 0.
    pub fn from_pud(
        pud: &Pud,
        tileset: &Tileset,
        stats: UnitTable,
        upgrades: UpgradeTable,
        scripts: Arc<Scripts>,
        start: &Start,
    ) -> Result<Self, WorldError> {
        let want = pud.width as usize * pud.height as usize;
        if pud.tiles.len() != want {
            return Err(WorldError::BadMap {
                got: pud.tiles.len(),
                want,
            });
        }
        let terrain = Terrain::from_map(
            tileset,
            &pud.tiles,
            pud.movement().as_deref(),
            pud.regions().as_deref(),
            pud.width,
            pud.height,
            pud.section(b"OILM").is_some(),
        );
        let mut w = World::new(terrain, stats, scripts, 0);
        w.rng = start.rng;
        let setup = &start.setup;
        w.rules.enhanced = setup.enhanced;
        w.rules.multiplayer = start.multiplayer;
        // Unit-slots spec, "Table sizes": Classic mode has 600 unit slots and 200 effect slots.
        if !setup.enhanced {
            w.rules.unit_slots = slots::CLASSIC_SLOTS;
            w.rules.effect_slots = slots::CLASSIC_EFFECTS;
            w.slots = Slots::new(slots::CLASSIC_SLOTS);
            w.missiles = vec![None; slots::CLASSIC_EFFECTS];
        }
        if let Some(c) = setup.custom() {
            w.rules.fog_of_war = c.fog_of_war;
        }
        w.upgrade_data = upgrades;
        if let Some(t) = pud.section(b"UGRD").map(UpgradeTable::from_ugrd).transpose()?.flatten() {
            w.upgrade_data = t;
        }
        // An empty ALOW section changes nothing; a full one replaces all six words for every slot.
        let alow = pud
            .section(b"ALOW")
            .filter(|s| !s.is_empty())
            .map(Restrictions::parse)
            .transpose()?;
        let (units, spells, ups) = setup.campaign().map_or((!0, !0, !0), crate::setup::campaign_allowance);
        let allies = start.alliances();
        // Set for every slot at the start of a new game, except in Enhanced multiplayer (setup spec, "Ever had a hall").
        let had_hall = !(w.rules.enhanced && w.rules.multiplayer);
        for (i, p) in w.players.iter_mut().enumerate() {
            let (gold, lumber, oil) = start.resources[i];
            *p = Player {
                gold,
                lumber,
                oil,
                race: start.races[i],
                controller: start.controllers[i],
                ai_script: pud.ai_scripts().get(i).copied().unwrap_or(0),
                units_allowed: alow.as_ref().map_or(units, |r| r.units[i]),
                spells_known: alow.as_ref().map_or(crate::setup::SPELLS_KNOWN, |r| r.spells_start[i]),
                spells_allowed: alow.as_ref().map_or(spells, |r| r.spells_allowed[i]),
                spells_researching: alow.as_ref().map_or(0, |r| r.spells_researching[i]),
                upgrades_allowed: alow.as_ref().map_or(ups, |r| r.upgrades_allowed[i]),
                upgrades_researching: alow.as_ref().map_or(0, |r| r.upgrades_researching[i]),
                allies: allies[i],
                shared_vision: if i < 8 { 1 << i } else { 0 },
                colour: i as u8,
                had_hall,
                ..Default::default()
            };
        }
        w.vision.local = start.local;
        let view = w.create_map_units(pud, setup);
        // Rescue players become passive computer or computer players.
        for p in &mut w.players {
            p.controller = match p.controller {
                crate::ai::controller::RESCUE_PASSIVE => crate::ai::controller::PASSIVE,
                crate::ai::controller::RESCUE_ACTIVE => crate::ai::controller::COMPUTER,
                c => c,
            };
        }
        let view = w.starting_view(view);
        w.start = crate::setup::Started {
            view,
            saved_views: [view; 3],
            race_setting: start.race_setting,
            orc_sounds: start.race_setting == crate::setup::race::ORC || setup.campaign() == Some(48),
        };
        if let Some(entry) = setup.campaign() {
            w.start_campaign(entry);
        }
        Ok(w)
    }

    pub(crate) fn stats_of(&self, kind: u8) -> UnitStats {
        self.stats.0.get(kind as usize).copied().unwrap_or_default()
    }

    /// Creates a unit with full hit points in the front free slot (unit-slots spec), making the creation draws
    /// (timing-and-rng spec, "Draw sites"): a mobile unit's facing, a building's variation value, a computer land
    /// unit's post choice (its first goal, attack-ai spec), then extra cycles on the first animation wait. Ships and flyers are aligned to even tiles. Placement only checks the map bounds
    /// until the placement spec lands. Returns its id.
    pub fn spawn(&mut self, kind: u8, owner: u8, tile: (i32, i32), resources: u32) -> Result<UnitId, SpawnError> {
        let s = self.stats_of(kind);
        let building = s.flags & flags::BUILDING != 0;
        let layer = self.layer_of(kind);
        let size = (s.size.0.clamp(1, 8) as u8, s.size.1.clamp(1, 8) as u8);
        let slot = self.slots.take(owner).map_err(|_| SpawnError::TooMany)?;
        // Unit-slots spec, "Creating a unit", step 3: a reused slot's old contents are cleared now.
        self.stale.remove(&slot);
        // Unit-slots spec, "Creating a unit", step 4, says ships and flyers sit on an even tile. The round-99 trace
        // (`mp-r99-tests-…-5400`, dragon 1595 created from the map at (50, 5)) keeps a map flyer's odd row: while the
        // map's units are created only a flyer's column is aligned, the narrowest change that fits the trace. No
        // trace places a map flyer at an odd column, so whether the column is aligned is open (reported to research).
        // Units created during play are aligned as before.
        let tile = if self.stride_of(kind) == 2 && layer == Layer::Air && self.loading {
            (tile.0 & !1, tile.1)
        } else if self.stride_of(kind) == 2 {
            (tile.0 & !1, tile.1 & !1)
        } else {
            tile
        };
        let fits = self.terrain.contains(tile.0, tile.1)
            && self
                .terrain
                .contains(tile.0 + size.0 as i32 - 1, tile.1 + size.1 as i32 - 1);
        if !fits {
            self.slots.release(slot, owner);
            return Err(SpawnError::CantPlace);
        }
        let id = UnitId(self.next_id);
        self.next_id += 1;
        let serial = self.next_serial;
        self.next_serial += 1;
        // Unit-slots spec, "Creating a unit", step 3: facing 0; only mobile units then draw one.
        let (mut facing, mut variation) = (0, 0);
        // Spells spec, "Summoned units and decay": a decaying type created during play makes no facing draw.
        let decays = s.decay_rate != 0 && !self.loading && !building;
        let caster = s.flags & flags::CASTER != 0 && !building;
        if building {
            variation = (self.rng.draw() % 6) as u8;
        } else if !decays {
            facing = (self.rng.draw() & 7) as u8;
        }
        let mut script = Script::default();
        script.start(anim::STILL, self.scripts.unit(kind, anim::STILL).unwrap_or(0));
        let unit = Unit {
            id,
            serial: Serial(serial),
            slot,
            kind,
            owner,
            tile,
            pos: (tile.0 * TILE_PX, tile.1 * TILE_PX),
            facing,
            // Unit-slots spec, "Creating a unit", step 3: the type's hit points, or 1 if that is 0.
            hp: s.hit_points.max(1),
            layer,
            size,
            building,
            resources,
            gather: Gather::None,
            carrying: false,
            return_to: None,
            chop: 0,
            inside: false,
            occupants: 0,
            order: Order::Idle,
            script,
            haste: 0,
            variation,
            path: Vec::new(),
            blocked: 0,
            pending: None,
            remembered: None,
            ship: Ship::default(),
            passengers: [None; PASSENGER_SLOTS],
            resume: 0,
            commanded: false,
            goal_x: 0,
            bloodlust: 0,
            unholy_armor: 0,
            attacked_by: None,
            hall_exit: false,
            skip_search: false,
            dying: false,
            removing: false,
            removal_due: false,
            remains: false,
            construction: None,
            job: None,
            // Unit-slots spec, "Creating a unit", step 3: the type's "magic" value, then the spells spec's rules: a
            // unit that decays gets 255 (its decay periods), and a caster that isn't a building 85 with its counter
            // at 40.
            mana: if caster {
                85
            } else if decays {
                255
            } else {
                u8::from(s.magic)
            },
            mana_counter: if caster { 40 } else { 0 },
            invisibility: 0,
            flame_shield: 0,
            used: false,
            marks: 0,
            decays,
            decay_count: if decays { s.decay_rate } else { 0 },
            // Fog spec, "A unit created during a cycle": nobody detects it, its owner included, until the next
            // detection phase. One created outside a cycle (the map's units, a test's) is detected by everyone, as
            // that phase would make it.
            detected_by: if self.in_cycle { 0 } else { 0xff },
            // Unit-slots spec, "Creating a unit": hidden from every player, computers included, until the next
            // visibility pass (fog spec, "Unit visibility"), so no target search finds it before then.
            hidden_from: 0xff,
            ai_job: Default::default(),
            goal: Default::default(),
            think: 0,
            // Rescue spec, "Where the mark comes from": only units the map gives to a rescue player.
            rescuable: self.loading && crate::ai::controller::is_rescue(self.players[owner as usize].controller),
            flash: Default::default(),
            freeze: Default::default(),
            held: false,
            colour: owner,
            attack_mark: Default::default(),
            look: Default::default(),
            bob: Default::default(),
            cooldown: 0,
            dying_target: None,
            forgot: None,
        };
        // Orders spec, "Orders given when a unit is created": every unit starts in order 2; a building's first order
        // (14 for a tower, else 33 or 59, which we hold as `Idle`) is pending until its first step done.
        let first = if s.flags & flags::TOWER != 0 {
            Order::TowerWatch { target: None }
        } else {
            Order::Idle
        };
        let unit = Unit {
            pending: building.then_some(first),
            ..unit
        };
        self.mark(&unit, unit.tile, Some(id));
        self.insert_vertical(id, unit.pos.1);
        let at = self.units.partition_point(|u| u.slot < slot);
        self.units.insert(at, unit);
        // Orders spec, "A recorded attacker outlives its slot" (round-51 errata): a record naming a unit whose slot was
        // released names the slot's new unit once the slot is reused.
        let old: Vec<UnitId> = self
            .slot_of
            .iter()
            .filter(|&(&o, &s)| s == slot && o != id)
            .map(|(&o, _)| o)
            .collect();
        for o in old {
            self.slot_of.remove(&o);
            for u in &mut self.units {
                if u.attacked_by == Some(o) {
                    u.attacked_by = Some(id);
                }
            }
        }
        self.slot_of.insert(id, slot);
        // Unit-slots spec, "Creating a unit", step 6: the computer player's creation hook (the goal cleared, and the
        // attack-ai spec's first goal) runs after the first order is set and before the extra first-wait draw, which
        // always comes last.
        self.creation_hook(at);
        let extra = (self.rng.draw() & 7) as u8;
        self.units[at].script.wait += extra;
        Ok(id)
    }

    /// Sets (`v` is the unit) or clears (`None`) the unit's occupancy marks at `tile` (occupancy spec, "Run-time
    /// marks"). A mobile unit marks the one tile with the ground-unit or air-unit bit, and clearing zeroes the pointer
    /// only if it names this unit. A building marks its footprint; clearing zeroes every footprint pointer. An oil patch
    /// marks nothing, and a circle of power only forbids building on its footprint.
    pub(crate) fn layer_of(&self, kind: u8) -> Layer {
        let s = self.stats_of(kind);
        if s.flags & flags::AIR != 0 {
            Layer::Air
        } else if s.flags & flags::SEA != 0 || (s.flags & flags::BUILDING != 0 && s.flags & flags::OIL_PATCH != 0) {
            Layer::Sea
        } else {
            Layer::Land
        }
    }

    pub(crate) fn mark(&mut self, unit: &Unit, tile: (i32, i32), v: Option<UnitId>) {
        if !unit.building {
            let (plane, bit) = match unit.layer {
                Layer::Air => (1, bits::AIR_UNIT),
                _ => (0, bits::UNIT),
            };
            let Some(n) = self.terrain.at(tile.0, tile.1) else {
                return;
            };
            match v {
                Some(_) => self.terrain.words[n] |= bit,
                None => self.terrain.words[n] &= !bit,
            }
            let p = &mut self.occupancy[plane][n];
            if v.is_some() || *p == Some(unit.id) {
                *p = v;
            }
            return;
        }
        for dy in 0..unit.size.1 as i32 {
            for dx in 0..unit.size.0 as i32 {
                let Some(n) = self.terrain.at(tile.0 + dx, tile.1 + dy) else {
                    continue;
                };
                match (unit.kind, v) {
                    (OIL_PATCH, _) | (CIRCLE_OF_POWER, None) => {}
                    (CIRCLE_OF_POWER, Some(_)) => self.terrain.words[n] |= bits::NO_BUILD,
                    (_, Some(_)) => {
                        self.terrain.words[n] |= bits::BUILDING;
                        self.occupancy[0][n] = v;
                    }
                    (_, None) => {
                        self.terrain.words[n] &= !bits::BUILDING;
                        self.occupancy[0][n] = None;
                    }
                }
            }
        }
    }

    /// The unit occupying a tile on a layer.
    pub fn occupant(&self, layer: Layer, x: i32, y: i32) -> Option<UnitId> {
        let n = self.terrain.at(x, y)?;
        self.occupancy[layer_index(layer)][n]
    }

    /// Whether a unit moving in the class `mask` may enter the tile as far as the terrain and buildings go: its
    /// movement word against the class mask, leaving out the unit marks of tiles a unit stands on (the callers judge
    /// units by pointer). Marks without a unit, such as the map's "cave" and "space" values, still block.
    pub(crate) fn terrain_allows_mask(&self, mask: u16, x: i32, y: i32) -> bool {
        let Some(n) = self.terrain.at(x, y) else { return false };
        let mut word = self.terrain.words[n];
        if self.occupancy[0][n].is_some() {
            word &= !bits::UNIT;
        }
        if self.occupancy[1][n].is_some() {
            word &= !bits::AIR_UNIT;
        }
        word & mask == 0
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.index(id).map(|i| &self.units[i])
    }

    pub(crate) fn index(&self, id: UnitId) -> Option<usize> {
        let slot = *self.slot_of.get(&id)?;
        // A slot released and reused in the same cycle is held twice until the cycle's end: pick by id.
        let lo = self.units.partition_point(|u| u.slot < slot);
        self.units[lo..]
            .iter()
            .take_while(|u| u.slot == slot)
            .position(|u| u.id == id)
            .map(|k| lo + k)
    }

    /// The slot a unit was last in, for a unit whose slot has been released too, until the slot is reused (orders
    /// spec, "A recorded attacker outlives its slot").
    pub(crate) fn last_slot(&self, id: UnitId) -> Option<u16> {
        self.slot_of.get(&id).copied()
    }

    /// Whether player `a` counts player `b` as allied (its own player included), by the alliance table.
    pub fn allied(&self, a: u8, b: u8) -> bool {
        a == b
            || self
                .players
                .get(a as usize)
                .is_some_and(|p| b < 16 && p.allies & (1 << b) != 0)
    }

    /// Moves a unit to another owner's list (unit-slots spec, "Changing owner"); the cap isn't checked. Rescue's
    /// owner change is [`World::rescue_owner_change`].
    pub(crate) fn change_owner(&mut self, i: usize, to: u8) {
        let u = &mut self.units[i];
        self.slots.change_owner(u.slot, u.owner, to);
        u.owner = to;
    }

    /// Runs one cycle: applies `commands` in order, then phase 4 of the timing-and-rng spec: every unit in ascending
    /// slot order, then every missile in effect slot order. Units whose slots were released leave the table at the end.
    pub fn step(&mut self, commands: &[Command]) {
        self.turn(commands);
        // Timing-and-rng spec, scheduler rule 4: a paused cycle changes no game state, but the counter still counts.
        if self.paused {
            self.tick += 1;
            return;
        }
        self.run_order_queues();
        self.detection();
        self.in_cycle = true;
        // Phase 2: the computer players (`ai/computer-player.md`, "One cycle, in order").
        self.computer_players();
        let refresh = self.fog_refresh();
        // Ascending slot order. A unit created during another unit's update is inserted into the table, so the walk
        // goes by slot rather than by index: a new unit in a higher slot is reached this cycle, one in a lower slot
        // isn't, and no unit is updated twice.
        let mut i = 0;
        while i < self.units.len() {
            let slot = self.units[i].slot;
            if refresh {
                self.refresh_sight(i);
            }
            self.updating = Some(slot);
            self.advance(i);
            i = self.units.partition_point(|u| u.slot <= slot);
        }
        self.updating = None;
        self.advance_missiles();
        for i in 0..self.units.len() {
            if self.units[i].construction.is_some() {
                self.construct(i);
            }
        }
        self.timers_pass();
        self.sort_vertical();
        self.unit_visibility();
        self.remember_looks();
        self.forget_seen_buildings();
        self.flash_pass();
        self.victory_phase();
        self.prune_selections();
        self.prune_rally();
        self.prune_queue();
        let gone: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.script.anim == RELEASED)
            .map(|u| u.id)
            .collect();
        // A released unit's slot entry is kept until the slot is reused (`spawn`): attacker records may still name it.
        if !gone.is_empty() {
            self.ylist.retain(|id| !gone.contains(id));
        }
        self.units.retain(|u| u.script.anim != RELEASED);
        self.sound_tick();
        self.in_cycle = false;
        self.tick += 1;
    }

    /// Timing-and-rng spec, "Scheduler", rule 1: when a game starts, the two start-up cycles run at once as full
    /// cycles (units update and draw), then the cycle counter is reset, so the first normal cycle ends at 1.
    pub fn start_up(&mut self) {
        for _ in 0..START_UP_CYCLES {
            self.step(&[]);
        }
        self.end_start_up();
    }

    /// A command turn without a cycle: applies `commands` in order, as [`World::step`] does before its cycle. The
    /// oracle trace format (`docs/specs/oracle/trace-format.md`, "Records and the cycles they follow") records the
    /// state after a turn separately from the cycle that follows it, so a comparer runs the two apart.
    pub fn turn(&mut self, commands: &[Command]) {
        for c in commands {
            self.apply_scoped(c);
        }
    }

    /// The live unit in unit-table slot `slot`, if any.
    pub fn unit_in_slot(&self, slot: u16) -> Option<&Unit> {
        self.units
            .binary_search_by_key(&slot, |u| u.slot)
            .ok()
            .map(|i| &self.units[i])
    }

    /// The counter reset that ends the start-up cycles, for a caller that steps them itself.
    pub fn end_start_up(&mut self) {
        self.tick = 0;
    }

    pub(crate) fn apply(&mut self, c: &Command) {
        if let Command::Queued { player, command } = c {
            return self.apply_queued(*player, command);
        }
        if self.selection_command(c) {
            return;
        }
        // Orders spec, "Holding and releasing workers" (round 101: HW4 to HW6): a unit command, even one every unit
        // then refuses, and a build command first release the sender's selected workers.
        if let Command::Move { player, .. }
        | Command::Stop { player, .. }
        | Command::Attack { player, .. }
        | Command::Repair { player, .. }
        | Command::Harvest { player, .. }
        | Command::ReturnGoods { player, .. }
        | Command::Smart { player, .. }
        | Command::AttackMove { player, .. }
        | Command::Patrol { player, .. }
        | Command::Unload { player, .. }
        | Command::AttackGround { player, .. }
        | Command::Demolish { player, .. }
        | Command::StandGround { player, .. }
        | Command::Cast { player, .. }
        | Command::Build { player, .. } = c
        {
            self.release_workers(*player);
        }
        let (player, units) = match c {
            Command::Move { player, units, .. }
            | Command::Stop { player, units }
            | Command::Attack { player, units, .. }
            | Command::Repair { player, units, .. }
            | Command::Harvest { player, units, .. }
            | Command::ReturnGoods { player, units }
            | Command::Smart { player, units, .. }
            | Command::AttackMove { player, units, .. }
            | Command::Patrol { player, units, .. }
            | Command::Unload { player, units, .. }
            | Command::AttackGround { player, units, .. }
            | Command::Demolish { player, units, .. }
            | Command::StandGround { player, units } => (*player, units),
            &Command::Train {
                player,
                building: id,
                kind,
            } => return self.building_command(player, id, JobKind::Train(kind)),
            &Command::Upgrade {
                player,
                building: id,
                to,
            } => return self.building_command(player, id, JobKind::Upgrade(to)),
            &Command::Research {
                player,
                building: id,
                upgrade,
            } => return self.building_command(player, id, JobKind::Research(upgrade)),
            &Command::Spell {
                player,
                building: id,
                spell,
            } => return self.building_command(player, id, JobKind::Spell(spell)),
            &Command::Cancel { player, unit } => return self.cancel_command(player, unit, false),
            &Command::CancelJob { player, unit } => return self.cancel_command(player, unit, true),
            &Command::Cast {
                player,
                unit,
                spell,
                target,
                tile,
            } => return self.cast_command(player, unit, spell, target, tile),
            &Command::Build {
                player,
                unit,
                kind,
                tile,
            } => {
                let Some(i) = self
                    .index(unit)
                    .filter(|&i| self.units[i].owner == player && !self.units[i].dying)
                else {
                    return;
                };
                // Orders spec, "A player's command forgets the attacker" (round-56 static correction): a build command
                // takes its own path, which keeps the recorded attacker and leaves the remembered order as it is.
                // Round-60 errata: checked first, in the command turn ("Carrying out a player's build command").
                if let Some(o) = self.player_build_command(i, kind, tile) {
                    self.command(i, o);
                }
                return;
            }
            Command::Select { .. }
            | Command::RightClick { .. }
            | Command::Issue { .. }
            | Command::SetGroup { .. }
            | Command::RecallGroup { .. }
            | Command::Speed { .. }
            | Command::Pause { .. }
            | Command::Queued { .. } => return,
            &Command::Cheat { player, flags } => return self.cheat_command(player, crate::cheats::Flags(flags)),
            &Command::Surrender { player } | &Command::QuitDraw { player } => return self.surrender_command(player),
            &Command::Leave { player, .. } => return self.leave_command(player),
            Command::Resume { .. } => return,
            &Command::Rally {
                player,
                building,
                target,
                tile,
            } => return self.rally_command(player, building, target, tile),
            &Command::UnloadOne { player, unit, slot } => return self.unload_one_command(player, unit, slot),
            &Command::Diplomacy {
                player,
                alliances,
                vision,
                allied_victory,
            } => return self.diplomacy_command(player, alliances, vision, allied_victory),
            &Command::HoldWorkers { player } => return self.hold_workers(player),
            &Command::ReleaseWorkers { player } => return self.release_workers(player),
        };
        for &id in units {
            let Some(i) = self.index(id) else { continue };
            let u = &self.units[i];
            if u.owner != player || u.dying || u.building || self.stride_of(u.kind) == 0 {
                continue;
            }
            // Selecting-and-commanding spec, "Which units take part in a command", and transports spec, "Clearing the
            // mark" (round 75, traced: V40): a command a unit doesn't take for want of the "can attack" flag is
            // refused before anything changes, so a transport keeps its unload-on-arrival mark.
            if let Some(n) = c.needs_attacker()
                && !self.takes(u, n)
            {
                continue;
            }
            self.players_command_clears_mark(i);
            let Some(order) = self.command_order(i, c) else {
                continue;
            };
            self.command(i, order);
            // Orders spec, "A player's command forgets the attacker" (round-55 errata).
            self.forget_attacker_after_command(id);
        }
    }

    /// A job for (or, with `None`, a cancel at) one of the player's buildings.
    fn building_command(&mut self, player: u8, id: UnitId, job: JobKind) {
        let Some(i) = self.index(id) else { return };
        if self.units[i].owner == player && !self.queue_job(i, job) {
            self.start_job(i, job);
        }
    }

    /// The two cancel commands, for the sender's own building: cancel job (`job`) or cancel construction, each
    /// acting only in its own state (build-train spec, V69).
    fn cancel_command(&mut self, player: u8, id: UnitId, job: bool) {
        let Some(i) = self.index(id) else { return };
        if self.units[i].owner == player && !(job && self.unqueue_last(i)) {
            self.cancel(i, job);
        }
    }

    pub(crate) fn stride_of(&self, kind: u8) -> i32 {
        self.stride.get(kind as usize).copied().unwrap_or(0) as i32
    }

    /// Phase 4 for one unit: advance its animation script, then its order.
    fn advance(&mut self, i: usize) {
        use crate::rescue::Freeze;
        let scripts = Arc::clone(&self.scripts);
        // Damage spec, "Attack timing (cooldown)": the cooldown counts down every cycle, except a transport's.
        if self.units[i].cooldown > 0 && self.stats_of(self.units[i].kind).flags & flags::TRANSPORT == 0 {
            self.units[i].cooldown -= 1;
        }
        // Build-train spec, "Placing a building" (round-65 errata, `xhuman5-30000` patch 1530, n = 27633): a patch
        // killed in a lower slot than its builder starts its removal at its next update; the wait of 1 comes from
        // that update, as for a patch in a higher slot.
        if std::mem::take(&mut self.units[i].removal_due) {
            self.death_done(i);
            self.units[i].script.wait = 2;
        }
        let u = &mut self.units[i];
        // Rescue spec, "Frozen units": a frozen unit doesn't advance its animation at all.
        if u.freeze == Freeze::Frozen && !u.dying {
            return;
        }
        let m = Modifiers {
            haste: u.haste,
            carrying: u.carrying,
        };
        // Orders spec, "Holding and releasing workers": a release update doesn't advance the animation; it runs the
        // rescue check and the order logic at once, as at a step done (traced: HW2, HW3).
        let mut fx = if u.freeze == Freeze::Released {
            crate::anim::Effects {
                step_done: true,
                ..Default::default()
            }
        } else {
            u.script.update(scripts.bytes(), u.facing, m)
        };
        match u.freeze {
            Freeze::Freezing if fx.step_done => {
                u.freeze = Freeze::Frozen;
                fx.step_done = false;
            }
            Freeze::Released => u.freeze = Freeze::None,
            _ => {}
        }
        u.pos.0 += fx.dx;
        u.pos.1 += fx.dy;
        let dying = u.dying;
        if fx.sound {
            self.play_pending(i);
        }
        if fx.neutral && self.units[i].owner != slots::NEUTRAL {
            self.change_owner(i, slots::NEUTRAL);
        }
        if dying {
            self.dead_update(i, fx);
        }
        if dying {
            return;
        }
        // Rescue spec, "Joining": the check comes first at a "step done", before the pending order and the order
        // logic.
        if fx.step_done {
            self.joining_check(i);
        }
        let u = &mut self.units[i];
        // Orders spec, "Resuming patrol and attack-move" (round-62 errata, static study): the countdown runs only while
        // the unit remembers something.
        if u.remembered.is_some() {
            u.resume = u.resume.saturating_sub(1);
        }
        if u.building {
            // Orders spec, "Orders given when a unit is created": the first order takes over at the first step done
            // (wait 3), and nothing else runs in that update.
            if fx.step_done
                && let Some(first) = u.pending.take()
            {
                self.take_over(i, first);
                return;
            }
            if self.is_watching_tower(i) {
                self.tower(i, fx);
            } else if self.units[i].kind == CIRCLE_OF_POWER {
                // Order 59's logic is the rescue check (rescue spec, "Rescuing at a circle of power").
                if fx.step_done {
                    self.circle_check(i);
                }
            } else {
                // Order 33's logic runs at a "step done" with no job; a job's take-over (order 37) replaces it. The
                // circle of power rests in order 59 instead. Orders 37 and 59 keep the attacker.
                let resting = fx.step_done && self.units[i].job.is_none() && self.units[i].kind != CIRCLE_OF_POWER;
                self.work(i, fx.step_done);
                if resting {
                    // Orders spec, row 33: each time its logic runs it first forgets the building's attacker (for
                    // every owner), so later hits call for help again.
                    self.units[i].attacked_by = None;
                    self.building_think_tick(i);
                }
            }
            return;
        }
        self.order(i, fx);
    }

    /// FNV-1a over everything that defines the game state. Equal hashes on two machines mean they are in sync.
    pub fn state_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        self.tick.hash(&mut h);
        self.players.hash(&mut h);
        self.rng.hash(&mut h);
        self.units.hash(&mut h);
        self.ylist.hash(&mut h);
        self.missiles.hash(&mut h);
        self.runes.hash(&mut h);
        self.vision.countdown.hash(&mut h);
        self.slots.hash(&mut h);
        self.stale.hash(&mut h);
        self.terrain.hash(&mut h);
        self.ai.hash(&mut h);
        self.army.hash(&mut h);
        self.victory.hash(&mut h);
        self.selections.hash(&mut h);
        self.speed.hash(&mut h);
        self.paused.hash(&mut h);
        // The pause counts are game state only where they are spent.
        if self.rules.multiplayer && self.rules.enhanced {
            self.pauses.hash(&mut h);
        }
        // The options that touch the sim are game state, and so is what they keep; a game with every option off hashes
        // as it always has (`extras`).
        if !self.rules.extras.is_default() {
            self.rules.extras.hash(&mut h);
            self.ext.hash(&mut h);
        }
        // Cheat flags join the hash only when set, so a game without cheats hashes as it always has.
        if self.cheats.flags.0 != 0 {
            self.cheats.flags.hash(&mut h);
        }
        // The players left out of their own shared-vision sets join the hash only once there is one (a diplomacy
        // command). The sets and the allied-victory settings themselves are in `players`.
        if self.vision_outsiders != 0 {
            self.vision_outsiders.hash(&mut h);
        }
        // The "seen" bits are game state; the explored and visible maps only drive the display.
        self.vision.seen.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tidewater_formats::tileset::{Palette, TileGroup};

    /// A tileset whose group 0 is ground and group 1 is water.
    pub(crate) fn tileset() -> Tileset {
        let g = |flags, code| TileGroup {
            megatiles: [0; 16],
            terrain: [code; 4],
            flags,
        };
        Tileset {
            palette: Palette([[0; 3]; 256]),
            groups: vec![g(0x01, 0x606), g(0x40, 0x202)],
            megatiles: vec![[0; 16]],
            minitiles: vec![0; 64],
            changes: None,
        }
    }

    pub(crate) fn stats() -> UnitTable {
        let mut t = vec![UnitStats::default(); 110];
        // A footman-like melee unit, an archer-like shooter (type 8) and a catapult-like splash thrower (type 5).
        t[0] = UnitStats {
            hit_points: 60,
            size: (1, 1),
            flags: flags::LAND | flags::CAN_ATTACK,
            basic_damage: 6,
            piercing_damage: 3,
            armor: 2,
            attack_range: 1,
            missile: crate::combat::missile::NONE,
            can_target: 1,
            ..Default::default()
        };
        t[8] = UnitStats {
            hit_points: 40,
            size: (1, 1),
            flags: flags::LAND | flags::CAN_ATTACK,
            basic_damage: 3,
            piercing_damage: 6,
            attack_range: 4,
            missile: 0x0f,
            can_target: 7,
            ..Default::default()
        };
        t[5] = UnitStats {
            hit_points: 110,
            size: (1, 1),
            flags: flags::LAND | flags::CAN_ATTACK | 4,
            basic_damage: 80,
            attack_range: 8,
            missile: crate::combat::missile::CATAPULT,
            can_target: 3,
            ..Default::default()
        };
        t[2] = UnitStats {
            hit_points: 100,
            size: (1, 1),
            flags: flags::AIR,
            ..Default::default()
        };
        for k in [0x3a, 0x3e, 0x4a, 0x58] {
            t[k] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING,
                ..Default::default()
            };
        }
        UnitTable(t)
    }

    /// Synthetic scripts: every type halts in place, and its move loop covers 32 px in 8 cycles; type 2 moves
    /// 64 px per loop. Attacks strike on cycle 12 and every 25 cycles after; death takes 6 cycles.
    pub(crate) fn scripts() -> Arc<Scripts> {
        use tidewater_formats::script::{MISSILE_KINDS, UNIT_TYPES};
        let mut b = vec![0u8; 640];
        let put = |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        for i in 0..UNIT_TYPES + MISSILE_KINDS {
            put(&mut b, i * 2, if i == 2 { 320 } else { 300 });
        }
        for (table, mv) in [(300, 350), (320, 400)] {
            for a in 0..7 {
                let at = match a {
                    1 => 480,
                    2 => 343,
                    3 => mv,
                    4 => 460,
                    0 => 520,
                    _ => 340,
                };
                put(&mut b, table + a * 2, at);
            }
        }
        b[460..467].copy_from_slice(&[0, 1, 10, 11, 1, 14, 3]);
        put(&mut b, 467, 460);
        b[480..485].copy_from_slice(&[4, 1, 1, 5, 2]);
        b[340..343].copy_from_slice(&[2, 3, 0]);
        put(&mut b, 342, 340);
        // Buildings (types 58 to 99): still is a "step done" every 5 cycles; work waits 10, then goes still.
        for k in 58..100 {
            put(&mut b, k * 2, 490);
        }
        for (a, at) in [520, 480, 343, 340, 340, 504, 343].into_iter().enumerate() {
            put(&mut b, 490 + a * 2, at);
        }
        b[343..346].copy_from_slice(&[0, 1, 4]);
        b[346] = 3;
        put(&mut b, 347, 343);
        b[504..507].copy_from_slice(&[1, 10, 3]);
        put(&mut b, 507, 343);
        for (at, op) in [(350usize, 5u8), (400, 13)] {
            let mut code = vec![0];
            for _ in 0..7 {
                code.extend([op, 4, 7, 1]);
            }
            code.extend([op, 4, 3]);
            code.extend((at as u16).to_le_bytes());
            b[at..at + code.len()].copy_from_slice(&code);
        }
        // Every type's remove animation: halt, halt, step done (corpses spec).
        b[520..524].copy_from_slice(&[2, 2, 0, 2]);
        // Remains (types 0x69 to 0x6D), every decay animation: frame 0, wait 10, neutral, wait 10, step done.
        for k in 0x69..=0x6d {
            put(&mut b, k * 2, 540);
        }
        for a in 0..7 {
            put(&mut b, 540 + a * 2, if (1..=5).contains(&a) { 560 } else { 520 });
        }
        b[560..569].copy_from_slice(&[4, 0, 1, 10, 15, 1, 10, 0, 2]);
        Arc::new(Scripts::parse(b).unwrap())
    }

    /// An 8x8 ground map with a water column at x = 4 for y < 6.
    pub(crate) fn world() -> World {
        let tiles = (0..64)
            .map(|i| if i % 8 == 4 && i / 8 < 6 { 0x10 } else { 0x00 })
            .collect::<Vec<u16>>();
        let mut w = World::new(
            Terrain::from_map(&tileset(), &tiles, None, None, 8, 8, false),
            stats(),
            scripts(),
            0,
        );
        w.spawn(0, 0, (0, 0), 0).unwrap();
        w
    }

    /// An open ground map of `w` x `h` tiles with no units.
    pub(crate) fn field(w: u16, h: u16) -> World {
        let tiles = vec![0u16; w as usize * h as usize];
        World::new(
            Terrain::from_map(&tileset(), &tiles, None, None, w, h, false),
            stats(),
            scripts(),
            0,
        )
    }

    pub(crate) fn run(w: &mut World, ticks: u32) {
        for _ in 0..ticks {
            w.step(&[]);
        }
    }

    fn fnv(h: u64, bytes: &[u8]) -> u64 {
        bytes
            .iter()
            .fold(h, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
    }

    /// For pin tests: a text as one number.
    pub(crate) fn fnv_str(s: &str) -> u64 {
        fnv(0xcbf2_9ce4_8422_2325, s.as_bytes())
    }

    /// For pin tests: the whole simulation state ([`World::fingerprint`]) with the queued events and sound cues, as
    /// one number.
    pub(crate) fn pinned(w: &World) -> u64 {
        fnv_str(&format!("{}|{:?}|{:?}", w.fingerprint(), w.events, w.sounds.cues))
    }

    /// For pin tests: steps `n` cycles, folding [`pinned`] of every cycle.
    pub(crate) fn trail(w: &mut World, n: u32) -> u64 {
        let mut h = 0xcbf2_9ce4_8422_2325;
        for _ in 0..n {
            w.step(&[]);
            h = fnv(h, &pinned(w).to_le_bytes());
        }
        h
    }

    /// Animation-scripts spec, "Facing a tile", test vectors.
    #[test]
    fn facing_a_tile() {
        for (a, b, f) in [
            ((76, 78), (78, 78), 2),
            ((87, 80), (89, 78), 1),
            ((0, 0), (3, 1), 3),
            ((0, 0), (4, 1), 2),
            ((0, 0), (-2, -5), 7),
            ((0, 0), (-2, -6), 0),
            ((5, 5), (5, 5), 2),
            // orc4 slot 1584 on (9, 12) facing gold mine 1599 (top-left (12, 1), so B = (13, 2)): north.
            ((9, 12), (13, 2), 0),
            // human01-basics footman 1598 on (26, 10) striking grunt 1592, which has claimed (26, 11): south.
            ((26, 10), (26, 11), 4),
        ] {
            assert_eq!(face_tile(a, b), f, "{a:?} -> {b:?}");
        }
    }

    /// Damage spec, "Attack timing (cooldown)": the attack loop is the cycles between two strikes of the attack
    /// script (25 in the synthetic scripts); without a strike point (towers) it is the step-done period.
    #[test]
    fn attack_loop_from_the_scripts() {
        let s = scripts();
        assert_eq!(attack_loop(&s, 0), 25);
        assert_eq!(attack_loop(&s, 8), 25);
        let w = world();
        assert_eq!((w.attack_loop[0], w.attack_loop[5]), (25, 25));
        // Building types' attack animation in the synthetic scripts has neither a strike nor a step done.
        assert_eq!(attack_loop(&s, 0x60), 0);
    }

    /// The same rule for a strike: a footman faces a building at its top-left tile plus (1, 1), whatever animation
    /// it is already playing.
    #[test]
    fn a_strike_faces_a_building_at_its_top_left_plus_one() {
        let mut w = field(32, 32);
        let f = w.spawn(0, 0, (9, 12), 0).unwrap();
        let farm = w.spawn(0x3a, 1, (12, 1), 0).unwrap();
        let i = w.index(f).unwrap();
        let b = facing_tile_of(w.unit(farm).unwrap());
        assert_eq!(b, (13, 2));
        w.face_and_strike(i, b);
        assert_eq!((w.units[i].facing, w.units[i].script.anim), (0, 4));
        w.units[i].tile = (20, 6);
        // Damage spec, "Attack timing (cooldown)": past the cooldown the next strike decision is reached.
        w.units[i].cooldown = 0;
        w.face_and_strike(i, b);
        assert_eq!(w.units[i].facing, 7, "turns again while in animation 4");
    }

    #[test]
    fn walks_around_water_to_the_goal() {
        let mut w = world();
        w.step(&[Command::Move {
            player: 0,
            units: vec![UnitId(1)],
            to: (7, 0),
        }]);
        run(&mut w, 400);
        let u = w.unit(UnitId(1)).unwrap();
        assert_eq!(u.tile, (7, 0));
        assert_eq!(u.pos, (7 * 32, 0));
        assert_eq!(u.order, Order::Idle);
        assert_eq!(w.occupant(Layer::Land, 7, 0), Some(UnitId(1)));
        assert_eq!(w.occupant(Layer::Land, 0, 0), None);
    }

    #[test]
    fn ignores_other_players_commands() {
        let mut w = world();
        w.step(&[Command::Move {
            player: 1,
            units: vec![UnitId(1)],
            to: (3, 3),
        }]);
        assert_eq!(w.unit(UnitId(1)).unwrap().order, Order::Idle);
    }

    #[test]
    fn units_never_share_a_tile() {
        let mut w = world();
        for x in 0..3 {
            w.spawn(0, 0, (x, 7), 0).unwrap();
        }
        let all: Vec<_> = w.units.iter().map(|u| u.id).collect();
        w.step(&[Command::Move {
            player: 0,
            units: all,
            to: (2, 2),
        }]);
        for _ in 0..300 {
            w.step(&[]);
            let mut seen = std::collections::BTreeSet::new();
            for u in &w.units {
                assert!(seen.insert(u.tile), "two units on {:?} at tick {}", u.tile, w.tick);
            }
        }
    }

    #[test]
    fn same_commands_same_hash() {
        let cmds = [Command::Move {
            player: 0,
            units: vec![UnitId(1)],
            to: (7, 7),
        }];
        let (mut a, mut b) = (world(), world());
        a.step(&cmds);
        b.step(&cmds);
        run(&mut a, 50);
        run(&mut b, 50);
        assert_eq!(a.state_hash(), b.state_hash());
        b.step(&[Command::Stop {
            player: 0,
            units: vec![UnitId(1)],
        }]);
        a.step(&[]);
        assert_ne!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn the_hasher_takes_usize_at_64_bits() {
        use std::hash::{Hash, Hasher};
        let (mut a, mut b) = (Fnv(1), Fnv(1));
        a.write_usize(0x1234_5678);
        b.write(&0x1234_5678u64.to_le_bytes());
        assert_eq!(a.finish(), b.finish());
        let (mut a, mut b) = (Fnv(1), Fnv(1));
        a.write_isize(-3);
        b.write(&(-3i64).to_le_bytes());
        assert_eq!(a.finish(), b.finish());
        // A vector hashes its length as a 64-bit number, then its items.
        let (mut a, mut b) = (Fnv(1), Fnv(1));
        vec![7u8, 9].hash(&mut a);
        b.write(&2u64.to_le_bytes());
        b.write(&[7, 9]);
        assert_eq!(a.finish(), b.finish());
    }

    /// Pins the sync hash: a fixed world, a few commands and cycles, one constant. If this fails, the hash of every
    /// game changed, which breaks play between builds; change the constant only on purpose.
    #[test]
    fn the_state_hash_is_pinned() {
        let mut w = world();
        w.spawn(8, 1, (6, 6), 0).unwrap();
        w.step(&[Command::Move {
            player: 0,
            units: vec![UnitId(1)],
            to: (3, 5),
        }]);
        run(&mut w, 40);
        assert_eq!(w.state_hash(), 0xd61f75605e58c349);
    }

    #[test]
    fn commands_round_trip_through_serde() {
        let cmds = vec![
            Command::Move {
                player: 1,
                units: vec![UnitId(3), UnitId(9)],
                to: (-4, 12),
            },
            Command::Harvest {
                player: 2,
                units: vec![UnitId(5)],
                target: Some(UnitId(8)),
                tile: (1, 2),
            },
            Command::Smart {
                player: 0,
                units: vec![],
                target: None,
                tile: (0, 0),
            },
            Command::Cast {
                player: 3,
                unit: UnitId(4),
                spell: 40,
                target: None,
                tile: (6, 7),
            },
            Command::Speed { player: 0, speed: 6 },
            Command::HoldWorkers { player: 1 },
            Command::ReleaseWorkers { player: 2 },
            Command::Cheat {
                player: 1,
                flags: 0x8000_0001,
            },
            Command::Surrender { player: 2 },
            Command::QuitDraw { player: 3 },
            Command::Leave {
                player: 4,
                dropped: true,
            },
        ];
        let bytes = postcard::to_stdvec(&cmds).unwrap();
        let back: Vec<Command> = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, cmds);
    }

    /// What the game menu did before surrender was a command: it set the player's result and the game's end.
    fn surrendered_by_hand(w: &mut World, p: usize) {
        w.players[p].result = Some(crate::victory::Outcome::Defeat);
        w.victory.over = true;
    }

    #[test]
    fn surrender_and_quit_draw_defeat_the_player_and_end_the_game_as_the_menu_did() {
        for command in [Command::Surrender { player: 0 }, Command::QuitDraw { player: 0 }] {
            let (mut by_command, mut by_hand) = (world(), world());
            by_command.turn(std::slice::from_ref(&command));
            surrendered_by_hand(&mut by_hand, 0);
            assert_eq!(by_command.players[0].result, Some(crate::victory::Outcome::Defeat));
            assert!(by_command.victory.over);
            assert_eq!(by_command.state_hash(), by_hand.state_hash());
            // In a cycle, too: the same commands in a step.
            let (mut a, mut b) = (world(), world());
            a.step(&[command]);
            surrendered_by_hand(&mut b, 0);
            b.step(&[]);
            assert_eq!(a.state_hash(), b.state_hash());
        }
    }

    #[test]
    fn a_surrender_for_a_slot_that_is_not_a_player_is_ignored() {
        let (mut w, still) = (world(), world());
        w.turn(&[Command::Surrender { player: 200 }, Command::QuitDraw { player: 16 }]);
        assert!(!w.victory.over);
        assert_eq!(w.state_hash(), still.state_hash());
    }

    #[test]
    fn updates_run_in_reverse_creation_order() {
        let mut w = world();
        let a = w.spawn(0, 0, (0, 7), 0).unwrap();
        let b = w.spawn(0, 0, (1, 7), 0).unwrap();
        let order: Vec<_> = w.units.iter().map(|u| (u.slot, u.id)).collect();
        assert_eq!(order, [(1597, b), (1598, a), (1599, UnitId(1))]);
    }

    #[test]
    fn spawn_failures() {
        let mut w = world();
        assert_eq!(w.spawn(0, 0, (8, 0), 0), Err(SpawnError::CantPlace));
        assert_eq!(w.spawn(0x3a, 0, (7, 7), 0), Err(SpawnError::CantPlace));
        assert_eq!(w.slots.len(0), 1, "a failed placement returns the slot");
        assert_eq!(w.spawn(2, 0, (3, 3), 0).map(|id| w.unit(id).unwrap().tile), Ok((2, 2)));
        // The map's own flyers keep their row (round 99 trace: a dragon created at row 5).
        w.loading = true;
        assert_eq!(w.spawn(2, 0, (3, 3), 0).map(|id| w.unit(id).unwrap().tile), Ok((2, 3)));
        w.loading = false;
    }

    #[test]
    fn occupancy_marks_and_pointers() {
        let mut w = world();
        assert_eq!(w.terrain.word(0, 0), 0x0101);
        let b = w.spawn(0x3a, 0, (5, 6), 0).unwrap();
        for (x, y) in [(5, 6), (6, 6), (5, 7), (6, 7)] {
            assert_eq!((w.terrain.word(x, y), w.occupant(Layer::Sea, x, y)), (0x0801, Some(b)));
        }
        let f = w.spawn(2, 0, (2, 6), 0).unwrap();
        assert_eq!(w.terrain.word(2, 6), 0x0201);
        assert_eq!(
            (w.occupant(Layer::Air, 2, 6), w.occupant(Layer::Land, 2, 6)),
            (Some(f), None)
        );
        // A second ground unit leaving a shared tile clears the mark for both, but leaves the other's pointer.
        let mut other = w.unit(UnitId(1)).unwrap().clone();
        other.id = UnitId(99);
        w.mark(&other, (0, 0), None);
        assert_eq!(
            (w.terrain.word(0, 0), w.occupant(Layer::Land, 0, 0)),
            (0x0001, Some(UnitId(1)))
        );
        let bu = w.unit(b).unwrap().clone();
        w.mark(&bu, (5, 6), None);
        assert_eq!((w.terrain.word(6, 7), w.occupant(Layer::Land, 6, 7)), (0x0001, None));
    }

    #[test]
    fn strides_come_from_the_move_scripts() {
        let w = world();
        assert_eq!((w.stride[0], w.stride[2]), (1, 2));
    }

    #[test]
    fn creation_draws_follow_the_spec() {
        // Seed 0 draws 0, 346, 130, 10982: the first unit takes facing 0 and 346 & 7 = 2 extra wait cycles.
        let w = world();
        let u = w.unit(UnitId(1)).unwrap();
        assert_eq!((u.facing, u.script.wait), (0, 3));
        let mut w = world();
        let b = w.spawn(0x3a, 0, (6, 6), 0).unwrap();
        assert_eq!(w.unit(b).unwrap().variation, (130 % 6) as u8);
        assert_eq!(w.unit(b).unwrap().script.wait, 1 + (10982 & 7) as u8);
    }

    #[test]
    fn a_new_units_hit_points_are_its_types_or_1() {
        // Unit-slots spec, "Creating a unit": the type's hit points, or 1 if that is 0 (the Human04 oil patches);
        // other values are kept (the gold mine's 25500).
        let mut w = world();
        w.stats.0[0x3a].hit_points = 0;
        let patch = w.spawn(0x3a, 0, (5, 5), 0).unwrap();
        w.stats.0[0x3a].hit_points = 25500;
        let mine = w.spawn(0x3a, 0, (2, 2), 0).unwrap();
        assert_eq!((w.unit(patch).unwrap().hp, w.unit(mine).unwrap().hp), (1, 25500));
    }

    #[test]
    fn a_new_units_facing_and_mana_byte() {
        // Unit-slots spec, "Creating a unit", step 3 (round-33 errata): facing 0, drawn only for mobile units; the mana
        // byte is the type's "magic" value, 255 for a unit that decays and 85 for a caster that isn't a building.
        let mut w = field(16, 16);
        w.stats.0[0x3a].flags |= flags::CASTER;
        let b = w.spawn(0x3a, 0, (5, 5), 0).unwrap();
        assert_eq!(w.unit(b).unwrap().mana, 0, "a building gets no caster's mana");
        w.stats.0[0x3a].flags &= !flags::CASTER;
        run(&mut w, 50);
        let u = w.unit(b).unwrap();
        assert_eq!((u.facing, u.mana), (0, 0), "a building never turns");
        // Mobile units draw their facing: the critters of the Human04 vector show facings 1 to 7.
        let facings: std::collections::BTreeSet<u8> = (0..16)
            .map(|n| {
                let id = w.spawn(0, 1, (n % 8, 10 + n / 8), 0).unwrap();
                let u = w.unit(id).unwrap();
                assert_eq!(u.mana, 0);
                u.facing
            })
            .collect();
        assert!(facings.len() > 1, "{facings:?}");
        w.stats.0[0].magic = true;
        let m = w.spawn(0, 1, (0, 14), 0).unwrap();
        assert_eq!(w.unit(m).unwrap().mana, 1);
        w.stats.0[0].flags |= flags::CASTER;
        let c = w.spawn(0, 1, (1, 14), 0).unwrap();
        assert_eq!(w.unit(c).unwrap().mana, 85);
        // A decaying type: 255 during play, its "magic" value while the map loads.
        w.stats.0[8].decay_rate = 3;
        let d = w.spawn(8, 1, (2, 14), 0).unwrap();
        let u = w.unit(d).unwrap();
        assert_eq!((u.facing, u.mana), (0, 255), "no facing draw for a summoned unit");
        w.loading = true;
        let l = w.spawn(8, 1, (3, 14), 0).unwrap();
        assert_eq!(w.unit(l).unwrap().mana, 0);
    }

    #[test]
    fn a_new_buildings_first_order_is_pending() {
        // Orders spec, "Orders given when a unit is created": a building's resting order takes over at its first
        // step done, so it shows order 2 until then.
        let mut w = world();
        let b = w.spawn(0x3a, 0, (5, 5), 0).unwrap();
        let u = w.unit(b).unwrap();
        assert_eq!((u.order, u.pending), (Order::Idle, Some(Order::Idle)));
    }

    #[test]
    fn a_step_takes_one_move_loop() {
        let mut w = world();
        w.step(&[Command::Move {
            player: 0,
            units: vec![UnitId(1)],
            to: (3, 0),
        }]);
        // The pending order takes over at the still loop's "step done" (cycle 3, the creation wait), and the move
        // animation's first instruction runs 3 cycles later: the tile changes then, while the pixel position is still
        // on the old tile. The step's loop takes 8 cycles and carries the pixel position across.
        let at = |w: &World| w.unit(UnitId(1)).unwrap().pos;
        let tile = |w: &World| w.unit(UnitId(1)).unwrap().tile;
        run(&mut w, 4);
        assert_eq!(tile(&w), (0, 0));
        run(&mut w, 1);
        assert_eq!(tile(&w), (1, 0));
        assert_eq!(at(&w), (0, 0));
        assert_eq!(w.occupant(Layer::Land, 0, 0), None);
        assert_eq!((w.terrain.word(0, 0), w.terrain.word(1, 0)), (0x0001, 0x0101));
        run(&mut w, 7);
        assert_eq!((tile(&w), at(&w)), ((1, 0), (28, 0)));
        run(&mut w, 1);
        assert_eq!((tile(&w), at(&w)), ((2, 0), (32, 0)));
    }

    #[test]
    fn flyers_move_two_tiles_per_step() {
        let mut w = world();
        let f = w.spawn(2, 0, (0, 2), 0).unwrap();
        w.step(&[Command::Move {
            player: 0,
            units: vec![f],
            to: (6, 2),
        }]);
        // The first step lands two tiles over.
        for _ in 0..20 {
            if w.unit(f).unwrap().tile != (0, 2) {
                break;
            }
            w.step(&[]);
        }
        assert_eq!(w.unit(f).unwrap().tile, (2, 2));
        run(&mut w, 100);
        let u = w.unit(f).unwrap();
        assert_eq!((u.tile, u.pos), ((6, 2), (6 * 32, 2 * 32)));
    }

    #[test]
    fn buildings_block_paths() {
        let mut w = world();
        w.spawn(0x3a, 0, (1, 0), 0).unwrap();
        w.step(&[Command::Move {
            player: 0,
            units: vec![UnitId(1)],
            to: (3, 0),
        }]);
        run(&mut w, 300);
        let u = w.unit(UnitId(1)).unwrap();
        assert_eq!(u.tile, (3, 0));
    }
}
