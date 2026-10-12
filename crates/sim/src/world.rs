use std::collections::BTreeMap;
use std::sync::Arc;

use tidewater_formats::aibin::AiFile;
use tidewater_formats::pud::Pud;
use tidewater_formats::script::{Scripts, anim};
use tidewater_formats::tileset::Tileset;
use tidewater_formats::udta::{UnitStats, UnitTable, flags};

use crate::TILE_PX;
use crate::ai::{AiPlayer, SCRIPT_PLAYERS};
use crate::anim::{Modifiers, Script};
use crate::battle::Missile;
use crate::combat::Upgrades;
use crate::economy::{Construction, Event, Job, JobKind};
use crate::rng::Rng;
use crate::slots::{self, Slots};
use crate::terrain::{Terrain, bits, mask};

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("map has {got} tiles, expected {want}")]
    BadMap { got: usize, want: usize },
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Order {
    Idle,
    Move {
        goal: (i32, i32),
    },
    Attack {
        target: UnitId,
    },
    /// Order 27: repair a building or transport (repair spec).
    Repair {
        target: UnitId,
    },
    /// Order 11: attack a wall tile (given by blocked row A of the movement spec).
    AttackWall {
        tile: (i32, i32),
    },
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
    /// Gold or oil left, for mines and patches.
    pub resources: u32,
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
    pub removing: bool,
    /// Set while a building is under construction.
    pub construction: Option<Construction>,
    /// A building's training or upgrade job.
    pub job: Option<Job>,
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
    /// Computer script number from the map's `AIPL` section.
    pub ai_script: u8,
}

/// Animation number marking a unit whose slot was released this cycle (no real animation uses it).
const RELEASED: u8 = 0xff;

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
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            unit_slots: slots::ENHANCED_SLOTS,
            effect_slots: slots::ENHANCED_EFFECTS,
            enhanced: true,
            multiplayer: false,
            build_speed_cheat: false,
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
    /// Start training a unit at a building. Whether the building makes that unit isn't checked yet.
    Train {
        player: u8,
        building: UnitId,
        kind: u8,
    },
    /// Start upgrading a building to another type. Whether `to` is an upgrade of it isn't checked yet.
    Upgrade {
        player: u8,
        building: UnitId,
        to: u8,
    },
    /// Command 27: workers repair a building or transport, or walk toward anything else.
    Repair {
        player: u8,
        units: Vec<UnitId>,
        target: UnitId,
    },
    /// Cancel a building's job, or the building if it is under construction.
    Cancel {
        player: u8,
        unit: UnitId,
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
    pub(crate) ai_file: Option<Arc<AiFile>>,
    next_id: u32,
    /// The ground-unit and air-unit pointer of each tile (occupancy spec, "Run-time marks"). Land units, ships and
    /// buildings share the ground plane.
    pub(crate) occupancy: [Vec<Option<UnitId>>; 2],
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
        World {
            tick: 0,
            occupancy: std::array::from_fn(|_| vec![None; n]),
            terrain,
            stats,
            slots: Slots::new(rules.unit_slots),
            slot_of: BTreeMap::new(),
            events: Vec::new(),
            ai: Default::default(),
            ai_file: None,
            missiles: vec![None; rules.effect_slots],
            rules,
            players: [Player::default(); PLAYERS],
            rng: Rng::new(seed),
            scripts,
            stride,
            units: Vec::new(),
            next_id: 1,
        }
    }

    /// Builds the starting state of a map. Start locations become [`Player::start`], not units. Map units are created
    /// in map order, each making its creation draws; units of players marked "nobody" are skipped (unit-slots spec,
    /// "Starting a map"). Converting a unit to its owner's race isn't done yet.
    pub fn from_pud(
        pud: &Pud,
        tileset: &Tileset,
        stats: UnitTable,
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
        for (i, p) in w.players.iter_mut().enumerate() {
            let (g, l, o) = pud.start_resources(i);
            *p = Player {
                gold: g as u32,
                lumber: l as u32,
                oil: o as u32,
                race: pud.sides().get(i).copied().unwrap_or(2),
                controller: pud.owners().get(i).copied().unwrap_or(crate::ai::controller::NOBODY),
                ai_script: pud.ai_scripts().get(i).copied().unwrap_or(0),
                ..Default::default()
            };
        }
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
                u.data as u32 * 2500
            } else {
                0
            };
            // A unit that can't be created is left out, as the game does.
            if w.spawn(u.kind, u.owner, (u.x as i32, u.y as i32), resources).is_ok()
                && crate::economy::kind::is_hall(u.kind)
                && let Some(p) = w.players.get_mut(u.owner as usize)
            {
                p.had_hall = true;
            }
        }
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
        if building {
            variation = (self.rng.draw() % 6) as u8;
        } else {
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
            order: Order::Idle,
            script,
            haste: 0,
            variation,
            path: Vec::new(),
            blocked: 0,
            pending: None,
            repair_runs: 0,
            bloodlust: 0,
            unholy_armor: 0,
            attacked_by: None,
            dying: false,
            removing: false,
            construction: None,
            job: None,
        };
        self.mark(&unit, unit.tile, Some(id));
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
    pub(crate) fn change_owner(&mut self, i: usize, to: u8) {
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
        self.step_ai();
        for i in 0..self.units.len() {
            self.advance(i);
        }
        self.advance_missiles();
        for i in 0..self.units.len() {
            if self.units[i].construction.is_some() {
                self.construct(i);
            }
        }
        let gone: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.script.anim == RELEASED)
            .map(|u| u.id)
            .collect();
        for id in gone {
            self.slot_of.remove(&id);
        }
        self.units.retain(|u| u.script.anim != RELEASED);
        self.tick += 1;
    }

    fn apply(&mut self, c: &Command) {
        let (player, units) = match c {
            Command::Move { player, units, .. }
            | Command::Stop { player, units }
            | Command::Attack { player, units, .. }
            | Command::Repair { player, units, .. } => (*player, units),
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
            &Command::Cancel { player, unit } => return self.building_command(player, unit, None),
        };
        for &id in units {
            let Some(i) = self.index(id) else { continue };
            let u = &self.units[i];
            if u.owner != player || u.dying || u.building || self.stride_of(u.kind) == 0 {
                continue;
            }
            let order = match *c {
                Command::Move { to, .. } => Order::Move { goal: to },
                Command::Stop { .. } => Order::Idle,
                Command::Attack { target, .. } => {
                    if !self.can_attack(u, target) {
                        continue;
                    }
                    Order::Attack { target }
                }
                Command::Repair { target, .. } => {
                    if self.stats_of(u.kind).flags & flags::PEON == 0 {
                        continue;
                    }
                    match self.repair_command(i, target) {
                        Some(o) => o,
                        None => continue,
                    }
                }
                Command::Train { .. } | Command::Upgrade { .. } | Command::Cancel { .. } => continue,
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
            carrying: false,
        };
        let fx = u.script.update(scripts.bytes(), u.facing, m);
        u.pos.0 += fx.dx;
        u.pos.1 += fx.dy;
        let (dying, removing) = (u.dying, u.removing);
        if fx.neutral {
            self.change_owner(i, slots::NEUTRAL);
        }
        if dying && (fx.halted || fx.step_done) {
            // Unit-slots spec, "Death, corpses and release": at the death animation's step done the unit (corpses and
            // ruins are a later spec) goes to the neutral owner with the "remove" order, which releases its slot at the
            // next step done. Release order decides slot reuse, so the free list is updated now.
            if removing {
                let u = &self.units[i];
                self.slots.release(u.slot, u.owner);
                self.units[i].script.anim = RELEASED;
            } else {
                self.change_owner(i, slots::NEUTRAL);
                self.units[i].removing = true;
            }
        }
        if dying {
            return;
        }
        if self.units[i].building {
            self.work(i, fx.step_done);
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
        self.missiles.hash(&mut h);
        self.slots.hash(&mut h);
        self.terrain.hash(&mut h);
        self.ai.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tidewater_formats::tileset::{Palette, TileGroup};

    /// A tileset whose group 0 is ground and group 1 is water.
    fn tileset() -> Tileset {
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

    fn stats() -> UnitTable {
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
            ..Default::default()
        };
        t[5] = UnitStats {
            hit_points: 110,
            size: (1, 1),
            flags: flags::LAND | flags::CAN_ATTACK | 4,
            basic_damage: 80,
            attack_range: 8,
            missile: crate::combat::missile::CATAPULT,
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
        let mut b = vec![0u8; 512];
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
        // Buildings (types 0x3e, 0x4a and 0x58): still is a "step done" every 5 cycles; work waits 10, then goes still.
        for k in [0x3e, 0x4a, 0x58] {
            put(&mut b, k * 2, 490);
        }
        for (a, at) in [340, 480, 343, 340, 340, 504, 343].into_iter().enumerate() {
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
