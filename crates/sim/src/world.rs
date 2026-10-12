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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

/// Which occupancy plane a unit lives on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
const CIRCLE_OF_POWER: u8 = 0x65;

/// A unit's order (orders spec, "The order list"); the spec's number is given on each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Order {
    /// 2: stop / idle.
    Idle,
    /// 3.
    Move { goal: (i32, i32) },
    /// 4: move, attacking on the way.
    MoveAttack { goal: (i32, i32) },
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
    /// Order 26: inside a building. `hall_sized` when goods were just delivered to a gold-accepting depot, which sizes
    /// the coming-out search as a town hall.
    Inside { building: UnitId, hall_sized: bool },
    /// Order 11: attack a wall tile (given by blocked row A of the movement spec).
    AttackWall { tile: (i32, i32) },
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
    Build { kind: u8, site: (i32, i32) },
    /// 22: a worker inside the building it is constructing.
    InsideSite { building: UnitId },
    /// Orders 38 to 56: casting `spell` (the order number) at a unit or, for the point spells, at `tile` (spells spec).
    Cast {
        spell: u8,
        target: Option<UnitId>,
        tile: (i32, i32),
    },
}

/// What a transport stores when given a command (transports spec, "What the transport stores"), and how it moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Gather {
    #[default]
    None,
    Gold,
    Lumber,
}

/// A patrol or attack-move remembered in Enhanced mode, to resume after a fight (orders spec, "Resuming patrol and
/// attack-move").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Remembered {
    Patrol { to: (i32, i32), back: (i32, i32) },
    AttackMove { goal: (i32, i32) },
}

/// Where a unit goes back to after delivering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Unit(UnitId),
    Tree((i32, i32)),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Unit {
    pub id: UnitId,
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
    /// Repair runs on this unit not yet paid for (repair spec: one payment per 2 runs, whoever repairs).
    pub repair_runs: u8,
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
    /// Set while a building is under construction.
    pub construction: Option<Construction>,
    /// A building's training or upgrade job.
    pub job: Option<Job>,
    /// Mana, and its regeneration counter (spells spec, "Timers"). A berserker counts its regeneration cycles in
    /// `mana`.
    pub mana: u8,
    pub mana_counter: u8,
    /// Invisibility spell timer; the unit is invisible while above 0 (the vision rules read it).
    pub invisibility: u16,
    /// Flame-shield timer: only blocks a second flame shield (spells spec, "Timers").
    pub flame_shield: u16,
    /// The per-unit "used" mark (spells spec, polymorph): a dead body that keeps it can't be raised.
    pub used: bool,
    /// Created during play with a decay rate: it decays (spells spec, "Summoned units and decay").
    pub decays: bool,
    /// Decay periods left before the unit starts losing hit points, and the cycles left in the current period.
    pub decay_period: u8,
    pub decay_count: u8,
    /// Players 0 to 7 that can see this unit at all (bit p), recomputed in phase 1 (vision spec, "Detection").
    pub detected_by: u8,
    /// Players 0 to 7 the fog hides this unit from (bit p), recomputed in phase 5 (vision spec, "Unit visibility").
    pub hidden_from: u8,
    /// The rescuable mark: the unit counts as rescuable (victory spec, "Terms").
    pub rescuable: bool,
    /// The held mark: placed by the map with a non-zero low byte in its data value; held units are left out of the
    /// computer's army counts (ai-scripts spec, "Conditions"). Nothing clears it (that spec's open question).
    pub held: bool,
    /// A computer worker's job, for the worker AI's bookkeeping.
    pub ai_job: crate::worker_ai::AiJob,
    /// The computer army's goal for this unit (attack-ai spec, "Unit goals").
    pub goal: crate::attack_ai::Goal,
    /// A computer building's think counter (training-research spec, "When a building thinks").
    pub think: u16,
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

    /// Alive and not inside a building: something other units can target.
    pub fn present(&self) -> bool {
        !self.dying && !self.inside
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Player {
    pub gold: u32,
    pub lumber: u32,
    pub oil: u32,
    /// Race from the map's `SIDE` section: 0 human, 1 orc, 2 neutral.
    pub race: u8,
    pub start: Option<(i32, i32)>,
    pub upgrades: Upgrades,
    pub kills: u32,
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
    /// Players this player shares its vision with, as a bit set (vision spec). Only read in Enhanced mode, where the
    /// player itself always counts too. Nothing sets it yet (diplomacy is a later spec).
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
    /// Players 0 to 7 this player is allied with (bit q); a player always counts as allied with itself.
    pub allies: u8,
    /// The allied-victory setting (victory spec, "The multiplayer rule").
    pub allied_victory: bool,
    /// A human player no longer connected.
    pub disconnected: bool,
    /// Units rescued, plus 0x1000 for a hero when the mission asks for one (victory spec, "Terms").
    pub rescued: u32,
    /// Some unit or building has changed owner to this player (mission mode F).
    pub received: bool,
    pub result: Option<crate::victory::Outcome>,
}

/// Animation number marking a unit whose slot was released this cycle (no real animation uses it).
pub(crate) const RELEASED: u8 = 0xff;

/// Numbers that specs will replace. Every field here is a placeholder of our own choosing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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

/// A player's instruction, applied at the start of the tick it is scheduled for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
    /// Command 29: a transport unloads at `tile` (transports spec).
    Unload {
        player: u8,
        units: Vec<UnitId>,
        tile: (i32, i32),
    },
    /// Cancel a building's job, or the building if it is under construction.
    Cancel {
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
}

pub const PLAYERS: usize = 16;

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
    /// The computer players' army: threat map, base boxes, pass counter (attack-ai spec).
    pub army: crate::attack_ai::Army,
    /// Upgrade data: the defaults, or the map's UGRD section (tech-tree spec, "Default upgrade data").
    pub upgrade_data: UpgradeTable,
    pub(crate) ai_file: Option<Arc<AiFile>>,
    next_id: u32,
    /// Fog maps and "seen" bits (vision spec).
    pub vision: Vision,
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
}

pub(crate) fn layer_index(l: Layer) -> usize {
    match l {
        Layer::Land | Layer::Sea => 0,
        Layer::Air => 1,
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
            army,
            upgrade_data: crate::tech::default_upgrades(),
            ai_file: None,
            missiles: vec![None; rules.effect_slots],
            rules,
            players: [Player::default(); PLAYERS],
            rng: Rng::new(seed),
            scripts,
            stride,
            units: Vec::new(),
            ylist: Vec::new(),
            runes: vec![None; crate::spells::RUNE_SLOTS],
            loading: false,
            sounds: Default::default(),
            next_id: 1,
            vision: Vision::new(terrain_w, terrain_h),
            victory: Default::default(),
        }
    }

    /// Builds the starting state of a map. Start locations become [`Player::start`], not units. Map units are created
    /// in map order, each making its creation draws; units of players marked "nobody" are skipped (unit-slots spec,
    /// "Starting a map"). Converting a unit to its owner's race isn't done yet. `upgrades` is the installation's
    /// default upgrade data (`upgrades.dat`), which the map's own data replaces when it has any.
    pub fn from_pud(
        pud: &Pud,
        tileset: &Tileset,
        stats: UnitTable,
        upgrades: UpgradeTable,
        scripts: Arc<Scripts>,
        seed: u32,
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
        let mut w = World::new(terrain, stats, scripts, seed);
        w.upgrade_data = upgrades;
        // The map's upgrade data replaces the defaults only when its "use default data" word is 0.
        if let Some(t) = pud.section(b"UGRD").map(UpgradeTable::from_ugrd).transpose()?.flatten() {
            w.upgrade_data = t;
        }
        // Without an ALOW section everything is allowed. Our reading: the two "being researched" words start at 0,
        // since the PUD specification calls them only good for saved games.
        let alow = pud.section(b"ALOW").map(Restrictions::parse).transpose()?;
        for (i, p) in w.players.iter_mut().enumerate() {
            let (g, l, o) = pud.start_resources(i);
            *p = Player {
                gold: g as u32,
                lumber: l as u32,
                oil: o as u32,
                race: pud.sides().get(i).copied().unwrap_or(2),
                controller: pud.owners().get(i).copied().unwrap_or(crate::ai::controller::NOBODY),
                ai_script: pud.ai_scripts().get(i).copied().unwrap_or(0),
                units_allowed: alow.as_ref().map_or(!0, |r| r.units[i]),
                spells_known: alow.as_ref().map_or(0, |r| r.spells_start[i]),
                spells_allowed: alow.as_ref().map_or(!0, |r| r.spells_allowed[i]),
                upgrades_allowed: alow.as_ref().map_or(!0, |r| r.upgrades_allowed[i]),
                ..Default::default()
            };
        }
        w.loading = true;
        for u in &pud.units {
            if pud.owners().get(u.owner as usize) == Some(&3) {
                continue;
            }
            if matches!(u.kind, 0x5e | 0x5f) {
                if let Some(p) = w.players.get_mut(u.owner as usize) {
                    p.start = Some((u.x as i32, u.y as i32));
                }
                continue;
            }
            let resources = if matches!(u.kind, 0x5c | 0x5d) {
                crate::harvest::trips(u.data)
            } else {
                0
            };
            // A unit that can't be created is left out, as the game does.
            let Ok(id) = w.spawn(u.kind, u.owner, (u.x as i32, u.y as i32), resources) else {
                continue;
            };
            if let Some(i) = w.index(id) {
                w.units[i].held = !matches!(u.kind, 0x5c | 0x5d) && u.data & 0xff != 0;
                w.held_goal(i);
            }
            if crate::economy::kind::is_hall(u.kind)
                && let Some(p) = w.players.get_mut(u.owner as usize)
            {
                p.had_hall = true;
            }
        }
        w.loading = false;
        Ok(w)
    }

    pub(crate) fn stats_of(&self, kind: u8) -> UnitStats {
        self.stats.0.get(kind as usize).copied().unwrap_or_default()
    }

    /// Creates a unit with full hit points in the front free slot (unit-slots spec), making the creation draws
    /// (timing-and-rng spec, "Draw sites"): a mobile unit's facing, a building's variation value, then extra cycles on
    /// the first animation wait. Ships and flyers are aligned to even tiles. Placement only checks the map bounds
    /// until the placement spec lands. Returns its id.
    pub fn spawn(&mut self, kind: u8, owner: u8, tile: (i32, i32), resources: u32) -> Result<UnitId, SpawnError> {
        let s = self.stats_of(kind);
        let building = s.flags & flags::BUILDING != 0;
        let layer = self.layer_of(kind);
        let size = (s.size.0.clamp(1, 8) as u8, s.size.1.clamp(1, 8) as u8);
        let slot = self.slots.take(owner).map_err(|_| SpawnError::TooMany)?;
        let tile = if self.stride_of(kind) == 2 {
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
        let (mut facing, mut variation) = (4, 0);
        // Spells spec, "Summoned units and decay": a decaying type created during play makes no facing draw.
        let decays = s.decay_rate != 0 && !self.loading && !building;
        if building {
            variation = (self.rng.draw() % 6) as u8;
        } else if !decays {
            facing = (self.rng.draw() & 7) as u8;
        }
        let mut script = Script::default();
        script.start(anim::STILL, self.scripts.unit(kind, anim::STILL).unwrap_or(0));
        script.wait += (self.rng.draw() & 7) as u8;
        let unit = Unit {
            id,
            slot,
            kind,
            owner,
            tile,
            pos: (tile.0 * TILE_PX, tile.1 * TILE_PX),
            facing,
            hp: s.hit_points,
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
            repair_runs: 0,
            bloodlust: 0,
            unholy_armor: 0,
            attacked_by: None,
            dying: false,
            removing: false,
            remains: false,
            construction: None,
            job: None,
            // Spells spec, "Timers": every newly created caster starts with 85 mana and its counter at 40.
            mana: if s.flags & flags::CASTER != 0 { 85 } else { 0 },
            mana_counter: if s.flags & flags::CASTER != 0 { 40 } else { 0 },
            invisibility: 0,
            flame_shield: 0,
            used: false,
            decays,
            decay_period: if decays { 255 } else { 0 },
            decay_count: if decays { s.decay_rate } else { 0 },
            detected_by: 0xff,
            hidden_from: 0,
            ai_job: Default::default(),
            goal: Default::default(),
            think: 0,
            rescuable: crate::ai::controller::is_rescue(self.players[owner as usize].controller),
            held: false,
        };
        self.mark(&unit, unit.tile, Some(id));
        self.insert_vertical(id, unit.pos.1);
        let at = self.units.partition_point(|u| u.slot < slot);
        self.units.insert(at, unit);
        self.slot_of.insert(id, slot);
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

    /// Whether a unit of `layer` may enter the tile as far as the terrain and buildings go: its movement word against
    /// the layer's mask, leaving out the unit marks of tiles a unit stands on (the callers judge units by pointer).
    /// Marks without a unit, such as the map's "cave" and "space" values, still block.
    pub(crate) fn terrain_allows(&self, layer: Layer, x: i32, y: i32) -> bool {
        let Some(n) = self.terrain.at(x, y) else { return false };
        let mut word = self.terrain.words[n];
        if self.occupancy[0][n].is_some() {
            word &= !bits::UNIT;
        }
        if self.occupancy[1][n].is_some() {
            word &= !bits::AIR_UNIT;
        }
        word & layer.mask() == 0
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.index(id).map(|i| &self.units[i])
    }

    pub(crate) fn index(&self, id: UnitId) -> Option<usize> {
        let slot = *self.slot_of.get(&id)?;
        self.units.binary_search_by_key(&slot, |u| u.slot).ok()
    }

    /// Moves a unit to another owner's list (unit-slots spec, "Changing owner"); the cap isn't checked.
    /// Whether player `a` counts player `b` as allied (its own player included).
    pub(crate) fn allied(&self, a: u8, b: u8) -> bool {
        a == b
            || self
                .players
                .get(a as usize)
                .is_some_and(|p| b < 8 && p.allies & (1 << b) != 0)
    }

    pub(crate) fn change_owner(&mut self, i: usize, to: u8) {
        self.note_owner_change(i, to);
        let u = &mut self.units[i];
        self.slots.change_owner(u.slot, u.owner, to);
        u.owner = to;
    }

    /// Runs one cycle: applies `commands` in order, then phase 4 of the timing-and-rng spec: every unit in ascending
    /// slot order, then every missile in effect slot order. Units whose slots were released leave the table at the end.
    pub fn step(&mut self, commands: &[Command]) {
        for c in commands {
            self.apply(c);
        }
        self.detection();
        self.step_ai();
        self.army_pass();
        let refresh = self.fog_refresh();
        for i in 0..self.units.len() {
            if refresh {
                self.apply_unit_sight(i);
            }
            self.advance(i);
        }
        self.advance_missiles();
        for i in 0..self.units.len() {
            if self.units[i].construction.is_some() {
                self.construct(i);
            }
        }
        self.timers_pass();
        self.sort_vertical();
        self.unit_visibility();
        self.victory_phase();
        let gone: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.script.anim == RELEASED)
            .map(|u| u.id)
            .collect();
        if !gone.is_empty() {
            self.ylist.retain(|id| !gone.contains(id));
        }
        for id in gone {
            self.slot_of.remove(&id);
        }
        self.units.retain(|u| u.script.anim != RELEASED);
        self.sound_tick();
        self.tick += 1;
    }

    fn apply(&mut self, c: &Command) {
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
            | Command::StandGround { player, units } => (*player, units),
            &Command::Train {
                player,
                building: id,
                kind,
            } => return self.building_command(player, id, Some(JobKind::Train(kind))),
            &Command::Upgrade {
                player,
                building: id,
                to,
            } => return self.building_command(player, id, Some(JobKind::Upgrade(to))),
            &Command::Research {
                player,
                building: id,
                upgrade,
            } => return self.building_command(player, id, Some(JobKind::Research(upgrade))),
            &Command::Spell {
                player,
                building: id,
                spell,
            } => return self.building_command(player, id, Some(JobKind::Spell(spell))),
            &Command::Cancel { player, unit } => return self.building_command(player, unit, None),
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
                if let Some(o) = self.build_command(i, kind, tile) {
                    self.give(i, o);
                }
                return;
            }
        };
        for &id in units {
            let Some(i) = self.index(id) else { continue };
            let u = &self.units[i];
            if u.owner != player || u.dying || u.building || self.stride_of(u.kind) == 0 {
                continue;
            }
            let Some(order) = self.command_order(i, c) else {
                continue;
            };
            self.give(i, order);
        }
    }

    /// A job for (or, with `None`, a cancel at) one of the player's buildings.
    fn building_command(&mut self, player: u8, id: UnitId, job: Option<JobKind>) {
        let Some(i) = self.index(id) else { return };
        if self.units[i].owner != player {
            return;
        }
        match job {
            Some(j) => self.start_job(i, j),
            None => self.cancel(i),
        }
    }

    pub(crate) fn stride_of(&self, kind: u8) -> i32 {
        self.stride.get(kind as usize).copied().unwrap_or(0) as i32
    }

    /// Phase 4 for one unit: advance its animation script, then its order.
    fn advance(&mut self, i: usize) {
        let scripts = Arc::clone(&self.scripts);
        let u = &mut self.units[i];
        let m = Modifiers {
            haste: u.haste,
            carrying: u.carrying,
        };
        let fx = u.script.update(scripts.bytes(), u.facing, m);
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
        let u = &mut self.units[i];
        u.resume = u.resume.saturating_sub(1);
        if u.building {
            if self.is_watching_tower(i) {
                self.tower(i, fx);
            } else {
                // Order 33's logic runs at a "step done" with no job; a job's take-over replaces it.
                let resting = fx.step_done && self.units[i].job.is_none();
                self.work(i, fx.step_done);
                if resting {
                    // Order 33's attacker handling, read as the orders spec's "reaction" ("Reacting to an
                    // attacker"): a building forgets its attacker, so its next hit calls for help again.
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
        }
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
        self.terrain.hash(&mut h);
        self.ai.hash(&mut h);
        self.army.hash(&mut h);
        self.victory.hash(&mut h);
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
