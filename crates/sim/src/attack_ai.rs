//! The computer players' army: the threat map, attack waves, per-unit goals (hold, attack, ferry, roam, patrols,
//! escort), the all-out attack and the call for help. Spec: `docs/specs/ai/attack-ai.md`. Unit flag bits, "can
//! target" values and unit type numbers are those of the public PUD specification.
//!
//! Our readings where the spec leaves room:
//! - "Computer player" is the movement spec's computer player (computer slots and active rescue players), as in the
//!   worker AI; only players 0 to 7 have script slots, base boxes and goals that run.
//! - A "gold store" is a unit with flag bit 12 that is not gone (complete or not).
//! - The ferry's "last passenger in slot order" is the last filled passenger slot of the transport; a passenger
//!   without a goal tile lands at the transport's own tile.
//! - Command 4 with a target unit moves toward that unit's tile at the time of the command (order 4 keeps a tile).
//! - The weights are computed from the world's unit and animation data (the map's own unit data when it has some).
//! - Casters run the spell step of `ai/caster-ai.md` (`caster_ai.rs`) before their goal in pass B.

use tidewater_formats::script::{Op, Scripts, anim, decode};
use tidewater_formats::udta::{UnitTable, flags};

use crate::ai::{FERRY_SEARCH, SCRIPT_PLAYERS, controller};
use crate::anim::FACING_VECTOR;
use crate::orders::distance;
use crate::placement::{Source, ring_walk};
use crate::slots;
use crate::terrain::{Terrain, bits, mask};
use crate::world::{Command, Order, PLAYERS, Remembered, Unit, UnitId, World};

/// Cycles between two runs of the passes; the counter starts at 1, so the first run is in the first cycle.
pub const PERIOD: u32 = 50;
/// Side of a threat-map block, in tiles.
pub const BLOCK: i32 = 8;
/// Ring-search limits: the goal tile, the post.
const GOAL_RING: i32 = 16;
const POST_RING: i32 = 24;
/// Units a transport loads per trip.
const FERRY_LOAD: usize = 6;
/// Help radii, and the distance within which a held unit answers.
const HELP_COMPUTER: i32 = 13;
const HELP_SEA: i32 = 4;
const HELP_OTHER: i32 = 2;
const HELD_ANSWER: i32 = 4;
/// Types below this one have a weight.
const WEIGHTED_TYPES: usize = 0x3a;
/// Flag bits 0 to 4: a mobile unit.
const MOBILE: u32 = flags::LAND | flags::AIR | flags::EXPLODE_WHEN_KILLED | flags::SEA | flags::CRITTER;
/// Slot of the sea wave size, also read by the escort quirk.
const SEA_SIZE: usize = 0x0f;

/// PUD types.
mod t {
    pub const SHIPYARDS: [u8; 2] = [0x48, 0x49];
    pub const GOLD_MINE: u8 = 0x5c;
}

/// A unit's goal kind ("Unit goals").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GoalKind {
    #[default]
    None = 0,
    Hold = 1,
    Attack = 2,
    Ferry = 3,
    Roam = 4,
    BaseEdge = 5,
    SeaPatrol = 6,
    Escort = 7,
}

/// A unit's goal: kind, goal tile, goal land area and the needs-a-ferry mark with its assigned transport. The held
/// mark is [`Unit::held`].
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Goal {
    pub kind: GoalKind,
    pub tile: Option<(i32, i32)>,
    pub area: u16,
    pub needs_ferry: bool,
    pub transport: Option<UnitId>,
}

/// A threat-map block's terrain kind.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlockKind {
    Land,
    Water,
    #[default]
    Mixed,
}

/// One 8 x 8 block of the threat map.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Block {
    pub kind: BlockKind,
    pub dominant: Option<u8>,
    pub land: [i16; PLAYERS],
    pub sea: [i16; PLAYERS],
}

/// A player's base box, kept as signed 8-bit values ("The base box").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BaseBox {
    pub min_x: i8,
    pub min_y: i8,
    pub max_x: i8,
    pub max_y: i8,
}

impl BaseBox {
    /// The box before any building: minimums at the map width (as an 8-bit value), maximums at -1.
    pub fn empty(width: u16) -> Self {
        let w = width as u8 as i8;
        Self {
            min_x: w,
            min_y: w,
            max_x: -1,
            max_y: -1,
        }
    }

    fn contains(&self, (x, y): (i32, i32)) -> bool {
        (self.min_x as i32..=self.max_x as i32).contains(&x) && (self.min_y as i32..=self.max_y as i32).contains(&y)
    }
}

/// Builds a base box from the top-left tiles of the qualifying buildings in list order, then adds the margins
/// (steps 3 and 4 of "The base box").
pub fn base_box(width: u16, buildings: &[(i32, i32)]) -> BaseBox {
    let mut b = BaseBox::empty(width);
    for &(x, y) in buildings {
        if x < b.min_x as i32 {
            b.min_x = x as i8;
        } else if x > b.max_x as i32 {
            b.max_x = x as i8;
        }
        if y < b.min_y as i32 {
            b.min_y = y as i8;
        } else if y > b.max_y as i32 {
            b.max_y = y as i8;
        }
    }
    let w = width as u8 as i8;
    let lower = |v: i8| {
        let v = v.wrapping_sub(5);
        if v < 0 { 0 } else { v }
    };
    let raise = |v: i8| {
        let v = v.wrapping_add(8);
        if v >= w { w.wrapping_sub(1) } else { v }
    };
    b.min_x = lower(b.min_x);
    b.min_y = lower(b.min_y);
    b.max_x = raise(b.max_x);
    b.max_y = raise(b.max_y);
    b
}

/// The army state kept per game: the pass counter, the threat map, the base boxes and the base-edge side counters.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Army {
    pub countdown: u32,
    /// Blocks per row (map width / 8); rows from the top.
    pub per_row: usize,
    pub blocks: Vec<Block>,
    pub boxes: [BaseBox; SCRIPT_PLAYERS],
    pub sides: [u8; SCRIPT_PLAYERS],
    /// Wave rounds launched per player (land, sea, air) that sent at least one unit. Not from the spec: a count for
    /// inspection.
    pub launched: [[u32; 3]; SCRIPT_PLAYERS],
}

impl Army {
    /// The state at game start: the blocks' terrain kinds from the map ("The threat map", "Terrain kind").
    pub fn new(terrain: &Terrain) -> Self {
        let per_row = (terrain.width as i32 / BLOCK).max(0) as usize;
        let blocks = (0..per_row * per_row)
            .map(|b| {
                let (row, col) = ((b / per_row) as i32, (b % per_row) as i32);
                let (mut water, mut land) = (0, 0);
                for y in row * BLOCK..(row + 1) * BLOCK {
                    for x in col * BLOCK..(col + 1) * BLOCK {
                        if !terrain.contains(x, y) {
                            continue;
                        }
                        let w = terrain.word(x, y);
                        water += (w & bits::WATER != 0) as u32;
                        land += (w & bits::LAND != 0) as u32;
                    }
                }
                Block {
                    kind: match land.cmp(&water) {
                        std::cmp::Ordering::Greater => BlockKind::Land,
                        std::cmp::Ordering::Less => BlockKind::Water,
                        std::cmp::Ordering::Equal => BlockKind::Mixed,
                    },
                    ..Default::default()
                }
            })
            .collect();
        Self {
            countdown: 1,
            per_row,
            blocks,
            boxes: [BaseBox::empty(terrain.width); SCRIPT_PLAYERS],
            sides: [0; SCRIPT_PLAYERS],
            launched: [[0; 3]; SCRIPT_PLAYERS],
        }
    }

    /// The block holding a tile.
    pub fn block_at(&self, (x, y): (i32, i32)) -> Option<&Block> {
        if x < 0 || y < 0 {
            return None;
        }
        let (col, row) = ((x / BLOCK) as usize, (y / BLOCK) as usize);
        if col >= self.per_row {
            return None;
        }
        self.blocks.get(row * self.per_row + col)
    }
}

/// The attack duration D of an attack animation starting at `pc` ("Weight"): step-done counts 1, waits their
/// argument, continue-at jumps are followed, and the walk stops after the second step-done.
pub fn attack_duration(code: &[u8], pc: Option<u16>) -> u32 {
    let Some(mut pc) = pc.map(usize::from) else { return 0 };
    let (mut d, mut dones) = (0u32, 0);
    // Not from the spec: a guard against a script that loops without two step-dones.
    for _ in 0..4096 {
        let Some(op) = decode(code, pc) else { break };
        match op {
            Op::StepDone => {
                d += 1;
                dones += 1;
                if dones == 2 {
                    break;
                }
            }
            Op::Wait(n) | Op::WaitHasteSlow(n) | Op::WaitHaste(n) | Op::WaitSlow(n) | Op::WaitCarry(n) => d += n as u32,
            Op::Jump(to) => {
                pc = to as usize;
                continue;
            }
            _ => {}
        }
        pc += op.size();
    }
    d
}

/// rate(t) = floor(10 × D(0) / D(t)) kept to 8 bits; 0 when D(t) = 0.
pub fn rate(d0: u32, d: u32) -> u8 {
    (10 * d0).checked_div(d).map_or(0, |r| r as u8)
}

/// w(t) from the basic and piercing damage and the rates.
pub fn weight(basic: u8, piercing: u8, rate: u8, rate1: u8) -> u16 {
    if rate == 0 || rate1 == 0 {
        return 0;
    }
    let base = (basic as i32 - 1) / 2 + piercing as i32;
    ((base * rate as i32) as u16) / rate1 as u16
}

/// The weight of every type below 0x3A; 0 for the rest.
pub fn weights(stats: &UnitTable, scripts: &Scripts) -> Vec<u16> {
    let d: Vec<u32> = (0..WEIGHTED_TYPES as u8)
        .map(|k| attack_duration(scripts.bytes(), scripts.unit(k, anim::ATTACK)))
        .collect();
    let rates: Vec<u8> = d.iter().map(|&dt| rate(d[0], dt)).collect();
    (0..WEIGHTED_TYPES)
        .map(|k| {
            let s = stats.0.get(k).copied().unwrap_or_default();
            weight(s.basic_damage, s.piercing_damage, rates[k], rates[1])
        })
        .collect()
}

/// The roam tile from the unit's tile, the map width and the two draws ("Roam").
pub fn roam_tile(here: (i32, i32), width: i32, rx: u32, ry: u32) -> (i32, i32) {
    let half = (width / 2).max(1) as u32;
    let quarter = width / 4;
    (
        (here.0 + (rx % half) as i32 - quarter).clamp(0, width - 1),
        (here.1 + (ry % half) as i32 - quarter).clamp(0, width - 1),
    )
}

/// The walk from `from` toward `to` by the keep-clear line rule, returning the first tile outside `inside` (or
/// `from` itself if it is outside, or if the two tiles are the same: the spec's open question "Line walk").
fn line_walk(from: (i32, i32), to: (i32, i32), inside: impl Fn((i32, i32)) -> bool) -> (i32, i32) {
    if !inside(from) || from == to {
        return from;
    }
    let (ax, ay) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
    let (sx, sy) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    let x_major = ax >= ay;
    let (major, minor) = if x_major { (ax, ay) } else { (ay, ax) };
    let mut counter = (major / 2).max(1);
    let mut at = from;
    loop {
        counter -= minor;
        let step_minor = counter <= 0;
        if step_minor {
            counter += major;
        }
        if x_major {
            at.0 += sx;
            if step_minor {
                at.1 += sy;
            }
        } else {
            at.1 += sy;
            if step_minor {
                at.0 += sx;
            }
        }
        if !inside(at) {
            return at;
        }
    }
}

/// Which wave, or which target picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Wave {
    Land,
    Sea,
    Air,
}

/// The waves: kind, trigger, size and repeat slots ("Attack waves").
const WAVES: [(Wave, usize, usize, usize); 3] = [
    (Wave::Land, 0x09, 0x0d, 0x0e),
    (Wave::Sea, 0x0a, 0x0f, 0x10),
    (Wave::Air, 0x0b, 0x11, 0x12),
];

/// Strength mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Land,
    Sea,
}

/// Dying, dead, removed or inside something.
pub(crate) fn gone(u: &Unit) -> bool {
    u.dying || u.removing || u.inside
}

impl World {
    fn unit_flags(&self, u: &Unit) -> u32 {
        self.stats_of(u.kind).flags
    }

    fn mobile(&self, u: &Unit) -> bool {
        self.unit_flags(u) & MOBILE != 0
    }

    /// Indices of `owner`'s units in list order.
    fn list_of(&self, owner: u8) -> Vec<usize> {
        self.slots
            .list(owner)
            .filter_map(|s| self.units.binary_search_by_key(&s, |u| u.slot).ok())
            .collect()
    }

    /// Players 0 to 7 that run the army logic.
    fn army_player(&self, owner: u8) -> bool {
        (owner as usize) < SCRIPT_PLAYERS && self.computer_player(owner)
    }

    // ---- Timing -------------------------------------------------------------------------------------------------

    /// Phase 2, after the scripts: every 50 cycles from the first, pass A, the no-workers research pass, then pass B
    /// ("Timing").
    pub(crate) fn army_pass(&mut self) {
        self.army.countdown = self.army.countdown.saturating_sub(1);
        if self.army.countdown > 0 {
            return;
        }
        self.army.countdown = PERIOD;
        self.rebuild_threat_map();
        self.attack_waves();
        // Step 2: `ai/training-research.md`, "the no-workers pass" (training_ai.rs).
        self.no_workers_research_pass();
        for p in 0..SCRIPT_PLAYERS as u8 {
            if self.computer_player(p) {
                self.rebuild_base_box(p);
            }
        }
        self.run_goals();
    }

    // ---- The threat map -----------------------------------------------------------------------------------------

    /// A unit's strength in a mode ("Strength of a unit").
    fn strength(&self, u: &Unit, mode: Mode, w: &[u16], h: i32) -> i16 {
        let f = self.unit_flags(u);
        if u.owner == slots::NEUTRAL || gone(u) {
            return 0;
        }
        if f & flags::PEON != 0 {
            return -2;
        }
        if f & flags::GOLD_STORAGE != 0 {
            return -1;
        }
        if f & flags::TRANSPORT != 0 {
            return -5;
        }
        if f & flags::TANKER != 0 {
            return -2;
        }
        if f & MOBILE == 0 {
            return -1;
        }
        let bit = match mode {
            Mode::Land => 1,
            Mode::Sea => 2,
        };
        let wt = w.get(u.kind as usize).copied().unwrap_or(0) as i32;
        if self.stats_of(u.kind).can_target & bit != 0 && wt != 0 {
            (u.hp as i32 * wt / h) as i16
        } else {
            0
        }
    }

    /// Rebuilds the sums and dominant owners ("Rebuild", "Dominant owner").
    pub(crate) fn rebuild_threat_map(&mut self) {
        let w = weights(&self.stats, &self.scripts);
        let h = (self.stats_of(1).hit_points as i32).max(1);
        let n = self.army.per_row;
        for b in 0..self.army.blocks.len() {
            let (row, col) = ((b / n) as i32, (b % n) as i32);
            let (mut land, mut sea) = ([0i16; PLAYERS], [0i16; PLAYERS]);
            for y in row * BLOCK..(row + 1) * BLOCK {
                for x in col * BLOCK..(col + 1) * BLOCK {
                    let Some(t) = self.terrain.at(x, y) else { continue };
                    let Some(u) = self.occupancy[0][t].and_then(|id| self.unit(id)) else {
                        continue;
                    };
                    let o = u.owner as usize & (PLAYERS - 1);
                    land[o] = land[o].wrapping_add(self.strength(u, Mode::Land, &w, h));
                    sea[o] = sea[o].wrapping_add(self.strength(u, Mode::Sea, &w, h));
                }
            }
            let block = &mut self.army.blocks[b];
            block.land = land;
            block.sea = sea;
            let sums = match block.kind {
                BlockKind::Land => Some(&block.land),
                BlockKind::Water => Some(&block.sea),
                BlockKind::Mixed => None,
            };
            block.dominant = sums.and_then(|s| {
                let mut best: Option<(i16, u8)> = None;
                for (p, &v) in s.iter().enumerate().take(SCRIPT_PLAYERS) {
                    if v != 0 && v > -100 && best.is_none_or(|(b, _)| v > b) {
                        best = Some((v, p as u8));
                    }
                }
                best.map(|(_, p)| p)
            });
        }
    }

    // ---- Attack waves (pass A) ----------------------------------------------------------------------------------

    /// The human player 0 to 7 with the most non-building units, more than 0; the lowest number wins a tie.
    fn target_player(&self) -> Option<u8> {
        let mut best: Option<(usize, u8)> = None;
        for p in 0..SCRIPT_PLAYERS as u8 {
            if self.players[p as usize].controller != controller::HUMAN {
                continue;
            }
            let n = self
                .units
                .iter()
                .filter(|u| u.owner == p && !u.building && !u.dying && !u.removing)
                .count();
            if n > best.map_or(0, |(b, _)| b) {
                best = Some((n, p));
            }
        }
        best.map(|(_, p)| p)
    }

    fn in_wave(&self, wave: Wave, u: &Unit) -> bool {
        let f = self.unit_flags(u);
        let layer = match wave {
            Wave::Land => f & (flags::AIR | flags::SEA) == 0,
            Wave::Sea => f & flags::SEA != 0,
            Wave::Air => f & flags::AIR != 0,
        };
        let excluded = flags::PEON | flags::TANKER | flags::KILLED_BY_INVISIBILITY | flags::MAGE_LIKE;
        layer && f & flags::CAN_ATTACK != 0 && f & MOBILE != 0 && f & excluded == 0
    }

    pub(crate) fn attack_waves(&mut self) {
        for p in 0..SCRIPT_PLAYERS as u8 {
            if !self.computer_player(p) || self.list_of(p).is_empty() {
                continue;
            }
            for (wave, trigger, size, repeat) in WAVES {
                if self.ai[p as usize].slots[trigger] == 0 {
                    continue;
                }
                for _ in 0..self.ai[p as usize].slots[repeat] {
                    self.wave_round(p, wave, self.ai[p as usize].slots[size] as usize);
                }
                self.ai[p as usize].slots[trigger] = 0;
            }
        }
    }

    fn wave_round(&mut self, p: u8, wave: Wave, size: usize) {
        let Some(target) = self.target_player() else { return };
        if size == 0 {
            return;
        }
        let Some(tile) = self.pick(wave, target) else { return };
        let mut count = 0;
        for i in self.list_of(p) {
            if count >= size {
                break;
            }
            let u = &self.units[i];
            if !self.in_wave(wave, u) || u.held || u.goal.kind == GoalKind::Attack {
                continue;
            }
            self.set_goal(i, GoalKind::Attack, Some(tile));
            count += 1;
        }
        if count > 0 {
            self.army.launched[p as usize][wave as usize] += 1;
        }
    }

    // ---- Target pickers -----------------------------------------------------------------------------------------

    /// The player's gold store nearest unit `i`, among those in its region (any, for an air or sea unit).
    fn nearest_store(&self, owner: u8, i: usize) -> Option<usize> {
        let u = &self.units[i];
        let any = self.unit_flags(u) & (flags::AIR | flags::SEA) != 0;
        let region = self.terrain.region(u.tile.0, u.tile.1);
        let mut best: Option<(i32, usize)> = None;
        for s in self.list_of(owner) {
            let b = &self.units[s];
            if self.unit_flags(b) & flags::GOLD_STORAGE == 0 || gone(b) {
                continue;
            }
            if !any && self.terrain.region(b.tile.0, b.tile.1) != region {
                continue;
            }
            let d = distance(u.tile, b);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, s));
            }
        }
        best.map(|(_, s)| s)
    }

    /// The pickers' reference unit ("Reference unit").
    fn reference_unit(&self, target: u8, land: bool) -> Option<usize> {
        let list = self.list_of(target);
        let f = *list.first()?;
        if let Some(s) = self.nearest_store(target, f) {
            return Some(s);
        }
        if land && let Some(&b) = list.iter().find(|&&b| self.units[b].building) {
            return Some(b);
        }
        Some(f)
    }

    fn pick(&self, wave: Wave, target: u8) -> Option<(i32, i32)> {
        match wave {
            Wave::Land => self.land_picker(target),
            Wave::Sea => self.sea_picker(target),
            Wave::Air => self.air_picker(target),
        }
    }

    /// "Land picker".
    pub(crate) fn land_picker(&self, target: u8) -> Option<(i32, i32)> {
        let r = self.reference_unit(target, true)?;
        let reference = &self.units[r];
        let n = self.army.per_row;
        let mut best: Option<(i32, usize)> = None;
        for (b, block) in self.army.blocks.iter().enumerate() {
            if block.dominant != Some(target) || block.kind != BlockKind::Land {
                continue;
            }
            // The spec's coordinate swap: the block's row gives the point's x, its column the y.
            let (row, col) = ((b / n) as i32, (b % n) as i32);
            let point = (row * BLOCK + 4, col * BLOCK + 4);
            let d = distance(point, reference);
            if d > 0 && d > best.map_or(0, |(bd, _)| bd) {
                best = Some((d, b));
            }
        }
        let (row, col) = match best {
            Some((_, b)) => ((b / n) as i32, (b % n) as i32),
            None => (reference.tile.1 / BLOCK, reference.tile.0 / BLOCK),
        };
        let tile = (col * BLOCK + 4, row * BLOCK + 4);
        let mut near: Option<(i32, (i32, i32))> = None;
        for i in self.list_of(target) {
            let u = &self.units[i];
            let f = self.unit_flags(u);
            let air_fighter = f & flags::AIR != 0 && f & flags::CAN_ATTACK != 0;
            if gone(u) || f & flags::SEA != 0 || air_fighter || f & flags::OIL_SOURCE != 0 {
                continue;
            }
            let d = distance(tile, u);
            if near.is_none_or(|(nd, _)| d < nd) {
                near = Some((d, u.tile));
            }
        }
        Some(near.map_or(tile, |(_, t)| t))
    }

    /// "Sea picker".
    fn sea_picker(&self, target: u8) -> Option<(i32, i32)> {
        let r = self.reference_unit(target, false)?;
        let list = self.list_of(target);
        if let Some(&p) = list
            .iter()
            .find(|&&i| self.unit_flags(&self.units[i]) & flags::OIL_SOURCE != 0)
        {
            return Some(self.units[p].tile);
        }
        if let Some(&s) = list.iter().rev().find(|&&i| t::SHIPYARDS.contains(&self.units[i].kind)) {
            return Some(self.units[s].tile);
        }
        let from = self.units[r].tile;
        self.farthest(&list, from, |u| self.unit_flags(u) & flags::SEA != 0)
    }

    /// "Air picker".
    fn air_picker(&self, target: u8) -> Option<(i32, i32)> {
        let r = self.reference_unit(target, false)?;
        let list = self.list_of(target);
        let from = self.units[r].tile;
        let mut mine: Option<(i32, (i32, i32))> = None;
        for &i in &list {
            let u = &self.units[i];
            if u.kind == t::GOLD_MINE {
                let d = distance(from, u);
                if mine.is_none_or(|(md, _)| d < md) {
                    mine = Some((d, u.tile));
                }
            }
        }
        if let Some((_, tile)) = mine {
            return Some(tile);
        }
        if let Some(&s) = list
            .iter()
            .rev()
            .find(|&&i| self.unit_flags(&self.units[i]) & flags::GOLD_STORAGE != 0)
        {
            return Some(self.units[s].tile);
        }
        Some(self.farthest(&list, from, |_| true).unwrap_or(from))
    }

    /// The unit of `list` passing `keep` that is farthest from `from` (strictly farther replaces): its tile.
    fn farthest(&self, list: &[usize], from: (i32, i32), keep: impl Fn(&Unit) -> bool) -> Option<(i32, i32)> {
        let mut best: Option<(i32, (i32, i32))> = None;
        for &i in list {
            let u = &self.units[i];
            if !keep(u) {
                continue;
            }
            let d = distance(from, u);
            if best.is_none_or(|(bd, _)| d > bd) {
                best = Some((d, u.tile));
            }
        }
        best.map(|(_, t)| t)
    }

    // ---- The all-out attack -------------------------------------------------------------------------------------

    /// The losses rule of `ai/worker-ai.md`: after one of a computer player's buildings is destroyed, fewer than 3
    /// buildings other than oil platforms left starts the all-out attack.
    pub(crate) fn building_lost(&mut self, owner: u8) {
        if !self.army_player(owner) {
            return;
        }
        let left = self
            .list_of(owner)
            .into_iter()
            .filter(|&i| {
                let u = &self.units[i];
                u.building && !u.dying && !u.removing && self.unit_flags(u) & flags::OIL_SOURCE == 0
            })
            .count();
        if left < 3 {
            self.all_out_attack(owner);
        }
    }

    /// "The all-out attack".
    pub(crate) fn all_out_attack(&mut self, p: u8) {
        let list = self.list_of(p);
        if list.is_empty() {
            return;
        }
        let Some(target) = self.target_player() else { return };
        let Some(tile) = self.land_picker(target) else { return };
        for i in list {
            let u = &self.units[i];
            if !self.mobile(u) || gone(u) {
                continue;
            }
            self.set_goal(i, GoalKind::Attack, Some(tile));
            if self.unit_flags(&self.units[i]) & (flags::PEON | flags::TANKER) != 0 {
                self.smart_move(i, None, tile);
            }
        }
    }

    // ---- Goals --------------------------------------------------------------------------------------------------

    /// A ring search (limit `limit`) around the 1 x 1 rectangle at `at` for a tile passing `ok`.
    fn ring_find(&self, at: (i32, i32), limit: i32, ok: impl Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
        ring_walk(Source::tile(at.0, at.1), at, limit, self.terrain.width as i32, ok)
    }

    /// "Setting a goal": true on success.
    pub(crate) fn set_goal(&mut self, i: usize, kind: GoalKind, tile: Option<(i32, i32)>) -> bool {
        let u = &self.units[i];
        if !self.mobile(u) || gone(u) {
            return false;
        }
        let here = u.tile;
        let f = self.unit_flags(u);
        let Some(mut to) = tile else {
            let g = &mut self.units[i].goal;
            g.tile = Some(here);
            g.kind = kind;
            return true;
        };
        if f & flags::AIR != 0 {
            let g = &mut self.units[i].goal;
            g.tile = Some(to);
            g.kind = kind;
            return true;
        }
        let sea = f & flags::SEA != 0;
        let word = self.terrain.word(to.0, to.1);
        let found = if sea {
            (word & (bits::LAND | bits::SHORE) != 0).then(|| {
                self.ring_find(to, GOAL_RING, |x, y| {
                    let w = self.terrain.word(x, y);
                    w & bits::WATER != 0 && w & mask::SEA == 0
                })
            })
        } else {
            (word & mask::LAND != 0).then(|| {
                self.ring_find(to, GOAL_RING, |x, y| {
                    let w = self.terrain.word(x, y);
                    w & bits::LAND != 0 && w & mask::LAND == 0
                })
            })
        };
        if let Some(Some(t)) = found {
            to = t;
        }
        let there = self.terrain.region(to.0, to.1);
        if self.terrain.region(here.0, here.1) == there {
            let g = &mut self.units[i].goal;
            g.tile = Some(to);
            g.kind = kind;
            return true;
        }
        if !sea {
            let g = &mut self.units[i].goal;
            g.tile = Some(to);
            g.needs_ferry = true;
            g.area = there.unwrap_or(0);
            g.kind = kind;
        }
        false
    }

    /// Creation step 6 (`core/unit-slots.md`): the goal is cleared, and a computer player's mobile fighting unit gets
    /// its first goal ("When a unit is created", map units included).
    pub(crate) fn first_goal(&mut self, i: usize) {
        self.units[i].goal = Goal::default();
        let u = &self.units[i];
        let f = self.unit_flags(u);
        if !self.army_player(u.owner) || f & MOBILE == 0 || f & (flags::PEON | flags::TANKER) != 0 {
            return;
        }
        if f & flags::TRANSPORT != 0 {
            self.set_goal(i, GoalKind::Ferry, None);
        } else if f & flags::SEA != 0 {
            self.set_goal(i, GoalKind::SeaPatrol, None);
        } else if f & flags::AIR != 0 {
            self.set_goal(i, GoalKind::Roam, None);
        } else {
            let post = self.post(i);
            self.set_goal(i, GoalKind::Hold, Some(post));
        }
    }

    /// "Held units": a held unit that is mobile, can attack and is not a worker holds its own tile.
    pub(crate) fn held_goal(&mut self, i: usize) {
        let u = &self.units[i];
        let f = self.unit_flags(u);
        if u.held && f & MOBILE != 0 && f & flags::CAN_ATTACK != 0 && f & flags::PEON == 0 {
            self.set_goal(i, GoalKind::Hold, None);
        }
    }

    /// "Boarding": going inside a transport clears the needs-a-ferry mark and the assigned transport.
    pub(crate) fn boarded(&mut self, i: usize) {
        let g = &mut self.units[i].goal;
        g.needs_ferry = false;
        g.transport = None;
    }

    /// "Choosing a land unit's post". Draws twice only when the walk leaves the map.
    pub(crate) fn post(&mut self, i: usize) -> (i32, i32) {
        let (here, owner) = (self.units[i].tile, self.units[i].owner);
        let Some(store) = self.nearest_store(owner, i) else {
            return here;
        };
        let Some(enemy) = self.enemy_search(i).and_then(|e| self.unit(e)) else {
            return here;
        };
        let bx = self.army.boxes[owner as usize % SCRIPT_PLAYERS];
        let mut to = line_walk(self.units[store].tile, enemy.tile, |t| bx.contains(t));
        let w = self.terrain.width as i32;
        if to.0 < 0 || to.1 < 0 || to.0 >= w || to.1 >= w {
            let x = here.0 + (self.rng.draw() % 12) as i32 - 6;
            let y = here.1 + (self.rng.draw() % 12) as i32 - 6;
            to = (x.clamp(0, w - 1), y.clamp(0, w - 1));
        }
        if self.terrain.word(to.0, to.1) & mask::LAND != 0 {
            let region = self.terrain.region(to.0, to.1);
            let found = self.ring_find(to, POST_RING, |x, y| {
                let wd = self.terrain.word(x, y);
                wd & bits::LAND != 0 && wd & mask::LAND == 0 && self.terrain.region(x, y) == region
            });
            match found {
                Some(t) => to = t,
                None => return here,
            }
        }
        if self.terrain.region(to.0, to.1) != self.terrain.region(here.0, here.1) {
            return here;
        }
        to
    }

    /// "Enemy search": the nearest unit, anywhere on the map, of a player the unit's owner doesn't count as allied,
    /// that is alive and that the unit's acquisition would score above 0.
    pub(crate) fn enemy_search(&self, i: usize) -> Option<UnitId> {
        let u = &self.units[i];
        let mut best: Option<(i32, UnitId)> = None;
        for q in 0..SCRIPT_PLAYERS as u8 {
            if self.allied(u.owner, q) {
                continue;
            }
            for t in self.list_of(q) {
                let t = &self.units[t];
                if gone(t) || !self.candidate(u, t, u.order) {
                    continue;
                }
                let d = distance(u.tile, t);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, t.id));
                }
            }
        }
        best.map(|(_, id)| id)
    }

    // ---- The base box (pass B) ----------------------------------------------------------------------------------

    pub(crate) fn rebuild_base_box(&mut self, p: u8) {
        let width = self.terrain.width;
        let list = self.list_of(p);
        let has_store = list.first().is_some_and(|&f| self.nearest_store(p, f).is_some());
        let b = if has_store {
            let tiles: Vec<(i32, i32)> = list
                .iter()
                .map(|&i| &self.units[i])
                .filter(|u| {
                    let f = self.unit_flags(u);
                    f & flags::BUILDING != 0 && !gone(u) && f & (flags::OIL_SOURCE | flags::SHORE_BUILDING) == 0
                })
                .map(|u| u.tile)
                .collect();
            base_box(width, &tiles)
        } else {
            BaseBox::empty(width)
        };
        self.army.boxes[p as usize] = b;
    }

    // ---- Running goals (pass B) ---------------------------------------------------------------------------------

    pub(crate) fn run_goals(&mut self) {
        for i in 0..self.units.len() {
            let u = &self.units[i];
            if !self.army_player(u.owner) || gone(u) || u.goal.kind == GoalKind::None {
                continue;
            }
            // Casters first run the spell step (`ai/caster-ai.md`); if it acts, the goal is skipped this pass.
            if self.unit_flags(u) & flags::CASTER != 0 && self.caster_step(i) {
                continue;
            }
            match self.units[i].goal.kind {
                GoalKind::Hold => self.hold_goal(i),
                GoalKind::Attack => self.attack_goal(i),
                GoalKind::Ferry => self.ferry_goal(i),
                GoalKind::Roam => self.roam_goal(i),
                GoalKind::BaseEdge => self.base_edge_goal(i),
                GoalKind::SeaPatrol => self.sea_patrol_goal(i),
                GoalKind::Escort | GoalKind::None => {}
            }
        }
    }

    fn leash(&self, u: &Unit) -> i32 {
        self.stats_of(u.kind).react_range_computer as i32 / 2 + 4
    }

    /// "Hold (1)".
    fn hold_goal(&mut self, i: usize) {
        let u = &self.units[i];
        let f = self.unit_flags(u);
        if f & (flags::PEON | flags::TANKER) != 0 {
            return;
        }
        let idle = u.order == Order::Idle;
        let o = u.owner as usize;
        if let Some(b) = self.army.block_at(u.tile) {
            if b.dominant == Some(u.owner) {
                if !idle {
                    return;
                }
            } else {
                let sums = if f & flags::SEA != 0 { &b.sea } else { &b.land };
                // Open question "No dominant owner in hold": E is taken as 0.
                let e = b.dominant.map_or(0, |d| sums[d as usize]) as i32;
                if 10 * sums[o] as i32 >= 4 * e && !idle {
                    return;
                }
            }
        }
        let goal = u.goal.tile.unwrap_or(u.tile);
        if distance(goal, u) >= self.leash(u) {
            if self.set_goal(i, GoalKind::Hold, Some(goal)) {
                self.smart_move(i, None, goal);
            }
            self.units[i].attacked_by = None;
        }
    }

    /// "Attack (2)".
    fn attack_goal(&mut self, i: usize) {
        let u = &self.units[i];
        let attacking = matches!(
            u.order,
            Order::Attack { .. }
                | Order::AttackWall { .. }
                | Order::Chase { .. }
                | Order::AttackInPlace { .. }
                | Order::IdleReact { .. }
        );
        let goal = u.goal.tile.unwrap_or(u.tile);
        if distance(goal, u) >= self.leash(u) && !attacking {
            if self.set_goal(i, GoalKind::Attack, Some(goal)) {
                self.command_4(i, None, goal);
            }
            self.units[i].attacked_by = None;
            return;
        }
        if u.order != Order::Idle {
            return;
        }
        match self.enemy_search(i) {
            Some(t) => self.command_12(i, t),
            None => {
                self.set_goal(i, GoalKind::Roam, None);
            }
        }
    }

    /// "Roam (4)".
    fn roam_goal(&mut self, i: usize) {
        if self.units[i].order != Order::Idle {
            return;
        }
        let w = self.terrain.width as i32;
        let rx = self.rng.draw();
        let ry = self.rng.draw();
        let to = roam_tile(self.units[i].tile, w, rx, ry);
        self.command_4(i, None, to);
    }

    /// "Sea patrol (6)".
    fn sea_patrol_goal(&mut self, i: usize) {
        let u = &self.units[i];
        if u.order != Order::Idle {
            return;
        }
        let owner = u.owner;
        let list = self.list_of(owner);
        let tanker = list
            .iter()
            .copied()
            .find(|&k| self.unit_flags(&self.units[k]) & flags::TANKER != 0);
        let depot = tanker
            .and_then(|k| self.nearest_depot(&self.units[k]))
            .and_then(|d| self.unit(d));
        let anchor = depot.map_or(u.tile, |d| d.tile);
        let nearest = |units: &mut dyn Iterator<Item = &Unit>| {
            let mut best: Option<(i32, (i32, i32))> = None;
            for c in units {
                let d = distance(anchor, c);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, c.tile));
                }
            }
            best.map(|(_, t)| t)
        };
        let platform = nearest(
            &mut list
                .iter()
                .map(|&k| &self.units[k])
                .filter(|c| self.unit_flags(c) & flags::OIL_SOURCE != 0 && c.construction.is_none() && !gone(c)),
        );
        let neutral = self.list_of(slots::NEUTRAL);
        let far = platform.or_else(|| {
            nearest(
                &mut neutral
                    .iter()
                    .map(|&k| &self.units[k])
                    .filter(|c| self.unit_flags(c) & flags::OIL_PATCH != 0 && c.construction.is_none() && !gone(c)),
            )
        });
        let far = match far {
            Some(t) => t,
            None => {
                let w = self.terrain.width as i32;
                let x = anchor.0 - (self.rng.draw() % 5) as i32 - 2;
                let y = anchor.1 - (self.rng.draw() % 5) as i32 - 2;
                (x.clamp(0, w - 1), y.clamp(0, w - 1))
            }
        };
        self.patrol(i, anchor, far);
        self.units[i].goal.tile = Some(anchor);
    }

    /// "Base-edge patrol (5)".
    fn base_edge_goal(&mut self, i: usize) {
        let u = &self.units[i];
        if u.order != Order::Idle {
            return;
        }
        let p = u.owner as usize;
        let b = self.army.boxes[p];
        let w = self.terrain.width as i32;
        let (x0, y0, x1, y1) = (b.min_x as i32, b.min_y as i32, b.max_x as i32, b.max_y as i32);
        let exists = [y0 < w - 1, x1 > 0, y1 > 0, x0 < w - 1];
        let c = self.army.sides[p];
        self.army.sides[p] = (c + 1) % 4;
        let Some(side) = (0..4).map(|k| (c as usize + k) % 4).find(|&s| exists[s]) else {
            self.set_goal(i, GoalKind::Roam, None);
            return;
        };
        let (from, to) = match side {
            0 => ((x0, y0), (x1, y0)),
            1 => ((x1, y0), (x1, y1)),
            2 => ((x0, y1), (x1, y1)),
            _ => ((x0, y0), (x0, y1)),
        };
        let clamp = |(x, y): (i32, i32)| (x.clamp(0, w - 1), y.clamp(0, w - 1));
        self.patrol(i, clamp(from), clamp(to));
    }

    /// "Ferry (3)".
    fn ferry_goal(&mut self, ti: usize) {
        let t = &self.units[ti];
        if !matches!(t.order, Order::Idle | Order::Docked) {
            return;
        }
        let (tid, owner, here) = (t.id, t.owner, t.tile);
        let passengers: Vec<UnitId> = t.passengers.iter().flatten().copied().collect();
        let list = self.list_of(owner);
        // Waiting units.
        let waiting: Vec<usize> = list
            .iter()
            .copied()
            .filter(|&k| !gone(&self.units[k]) && self.units[k].goal.transport == Some(tid))
            .take(FERRY_LOAD)
            .collect();
        if !waiting.is_empty() {
            let mut best: Option<(i32, usize)> = None;
            for k in waiting {
                let d = distance(here, &self.units[k]);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, k));
                }
            }
            if let Some((_, k)) = best {
                self.smart_move(k, Some(tid), here);
            }
            return;
        }
        if !passengers.is_empty() {
            let mut g = passengers
                .last()
                .and_then(|&p| self.unit(p))
                .and_then(|p| p.goal.tile)
                .unwrap_or(here);
            if self.ai[owner as usize].slots[FERRY_SEARCH] != 0 {
                let r = self.rng.draw();
                g = self.landing_spot(g, r);
            }
            for e in 0..SCRIPT_PLAYERS {
                if e >= self.ai[e].slots[SEA_SIZE] as usize {
                    break;
                }
                let Some(k) = self.escort_for(owner, here) else { break };
                self.set_goal(k, GoalKind::Escort, Some(g));
                self.command_4(k, Some(tid), here);
            }
            self.smart_move(ti, None, g);
            self.units[ti].ship.unload = true;
            return;
        }
        // Gather.
        let mut trip: Option<u16> = None;
        let mut count = 0;
        let mut any = false;
        for k in list {
            let u = &self.units[k];
            if !self.mobile(u) || gone(u) || !u.goal.needs_ferry {
                continue;
            }
            let region = self.terrain.region(u.tile.0, u.tile.1);
            let served = u.goal.transport.and_then(|a| self.unit(a)).is_some_and(|a| {
                !gone(a)
                    && self.is_transport(a)
                    && a.owner == owner
                    && self.terrain.region(a.ship.target_tile.0, a.ship.target_tile.1) == region
            });
            if served {
                continue;
            }
            any = true;
            let area = u.goal.area;
            self.units[k].goal.transport = Some(tid);
            if area == *trip.get_or_insert(area) {
                self.smart_move(k, Some(tid), here);
                count += 1;
                // When 6 have been counted the walk stops: later units keep their assigned transport.
                if count == FERRY_LOAD {
                    break;
                }
            }
        }
        if !any && let Some(s) = self.nearest_store(owner, ti) {
            let (sid, stile) = (self.units[s].id, self.units[s].tile);
            self.smart_move(ti, Some(sid), stile);
        }
    }

    /// The landing-spot search from `g` with the draw `r` (step 3.1 of "Ferry").
    fn landing_spot(&self, g: (i32, i32), r: u32) -> (i32, i32) {
        let region = self.terrain.region(g.0, g.1);
        let d0 = (r % 8) as usize;
        for k in 0..8 {
            let v = FACING_VECTOR[(d0 + k) % 8];
            let mut at = (g.0 + v.0, g.1 + v.1);
            while self.terrain.contains(at.0, at.1) {
                if self.terrain.region(at.0, at.1) == region && self.terrain.word(at.0, at.1) & bits::SHORE != 0 {
                    return at;
                }
                at = (at.0 + v.0, at.1 + v.1);
            }
        }
        g
    }

    /// The escort for a round: the owner's unit nearest the transport that can attack, is an air or sea unit, is not
    /// held and is not already an escort.
    fn escort_for(&self, owner: u8, here: (i32, i32)) -> Option<usize> {
        let mut best: Option<(i32, usize)> = None;
        for k in self.list_of(owner) {
            let u = &self.units[k];
            let f = self.unit_flags(u);
            if gone(u)
                || f & flags::CAN_ATTACK == 0
                || f & (flags::AIR | flags::SEA) == 0
                || u.held
                || u.goal.kind == GoalKind::Escort
            {
                continue;
            }
            let d = distance(here, u);
            if best.is_none_or(|(bd, _)| d < bd) {
                best = Some((d, k));
            }
        }
        best.map(|(_, k)| k)
    }

    // ---- The call for help --------------------------------------------------------------------------------------

    /// "Call for help": unit `vi` is notified that it was attacked by `attacker`.
    pub(crate) fn call_for_help(&mut self, vi: usize, attacker: UnitId) {
        if let Some(old) = self.units[vi].attacked_by {
            if self.unit(old).is_some_and(|a| !gone(a)) {
                return;
            }
            self.units[vi].attacked_by = None;
        }
        let Some(a) = self.unit(attacker) else { return };
        let (a_owner, a_building) = (a.owner, a.building);
        let a_sub = self.unit_flags(a) & flags::SUBMARINE != 0;
        let v = &self.units[vi];
        let (v_id, v_owner, v_tile, v_size) = (v.id, v.owner, v.tile, v.size);
        if self.allied(v_owner, a_owner) || gone(v) {
            return;
        }
        if self.human_controlled(v_owner) && a_building {
            return;
        }
        self.units[vi].attacked_by = Some(attacker);
        let computer = self.computer_player(v_owner);
        if computer && a_sub {
            let mut best: Option<(i32, usize)> = None;
            for k in self.list_of(v_owner) {
                let s = &self.units[k];
                let f = self.unit_flags(s);
                if k == vi || gone(s) || s.attacked_by.is_some() || f & (flags::AIR | flags::SUBMARINE) == 0 {
                    continue;
                }
                let d = distance(v_tile, s);
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, k));
                }
            }
            if let Some((_, k)) = best {
                self.command_4(k, Some(v_id), v_tile);
            }
        }
        let r = if computer {
            HELP_COMPUTER
        } else if self.unit_flags(&self.units[vi]) & flags::SEA != 0 {
            HELP_SEA
        } else {
            HELP_OTHER
        };
        let (w, h) = (self.terrain.width as i32, self.terrain.height as i32);
        let x0 = (v_tile.0 - r).clamp(0, w - 1);
        let y0 = (v_tile.1 - r).clamp(0, h - 1);
        let x1 = (v_tile.0 + v_size.0 as i32 - 1 + r).clamp(0, w - 1);
        let y1 = (v_tile.1 + v_size.1 as i32 - 1 + r).clamp(0, h - 1);
        let answering: Vec<UnitId> = self
            .ylist
            .iter()
            .filter_map(|&id| self.unit(id))
            .filter(|s| {
                s.owner == v_owner
                    && s.tile.0 <= x1
                    && s.tile.0 + s.size.0 as i32 > x0
                    && s.tile.1 <= y1
                    && s.tile.1 + s.size.1 as i32 > y0
            })
            .map(|s| s.id)
            .collect();
        for sid in answering {
            let Some(k) = self.index(sid) else { continue };
            let Some(a) = self.unit(attacker) else { return };
            let s = &self.units[k];
            let f = self.unit_flags(s);
            let ready = matches!(s.order, Order::Idle | Order::Guard { .. }) && s.pending.is_none();
            let fighter = f & (flags::PEON | flags::TANKER | flags::KILLED_BY_INVISIBILITY | flags::MAGE_LIKE) == 0
                && f & flags::CAN_ATTACK != 0
                && f & flags::TOWER == 0;
            if s.dying || !ready || !fighter || s.attacked_by.is_some() || k == vi || !self.candidate(s, a, s.order) {
                continue;
            }
            if computer && s.held {
                if distance(s.tile, &self.units[vi]) > HELD_ANSWER {
                    continue;
                }
                self.set_goal(k, GoalKind::Attack, Some(self.units[k].tile));
            }
            self.command_12(k, attacker);
        }
    }

    // ---- Commands -----------------------------------------------------------------------------------------------

    /// Whether a unit takes commands at all (as for a player's command).
    fn takes_commands(&self, i: usize) -> bool {
        let u = &self.units[i];
        !u.dying && !u.building && self.stride_of(u.kind) != 0
    }

    /// Command 3, smart move.
    pub(crate) fn smart_move(&mut self, i: usize, target: Option<UnitId>, tile: (i32, i32)) {
        if !self.takes_commands(i) {
            return;
        }
        let c = Command::Smart {
            player: self.units[i].owner,
            units: vec![self.units[i].id],
            target,
            tile,
        };
        if let Some(o) = self.command_order(i, &c) {
            self.give(i, o);
        }
    }

    /// Command 4, move attacking on the way: a non-combat unit is first handled as a smart move, then the order
    /// becomes 4 (`orders/orders.md`, "Commands").
    fn command_4(&mut self, i: usize, target: Option<UnitId>, tile: (i32, i32)) {
        if !self.takes_commands(i) {
            return;
        }
        let goal = target.and_then(|t| self.unit(t)).map_or(tile, |t| t.tile);
        if self.non_combat(&self.units[i]) {
            let c = Command::Smart {
                player: self.units[i].owner,
                units: vec![self.units[i].id],
                target,
                tile: goal,
            };
            let _ = self.command_order(i, &c);
        }
        self.units[i].remembered = None;
        self.give(i, Order::MoveAttack { goal });
    }

    /// Command 12 against a unit.
    pub(crate) fn command_12(&mut self, i: usize, target: UnitId) {
        if self.takes_commands(i) {
            self.give(i, Order::Chase { target });
        }
    }

    /// A patrol command toward `from`, with the patrol ends `from` and `to`.
    fn patrol(&mut self, i: usize, from: (i32, i32), to: (i32, i32)) {
        if !self.takes_commands(i) {
            return;
        }
        self.units[i].remembered = self.rules.enhanced.then_some(Remembered::Patrol { to: from, back: to });
        self.give(i, Order::Patrol { to: from, back: to });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Layer;
    use crate::world::tests::{field, run};
    use tidewater_formats::udta::UnitStats;

    const FOOTMAN: u8 = 0x00;
    const GRUNT: u8 = 0x01;
    const PEASANT: u8 = 0x02;
    const KNIGHT: u8 = 0x06;
    const ARCHER: u8 = 0x08;
    const DESTROYER: u8 = 0x1e;
    const TRANSPORT: u8 = 0x1a;
    const SUBMARINE: u8 = 0x26;
    const GRYPHON: u8 = 0x2a;
    const FARM: u8 = 0x3a;
    const TOWN_HALL: u8 = 0x4a;
    const PLATFORM: u8 = 0x56;
    const SHIPYARD: u8 = 0x48;
    // Any type works: the tower behaviour comes from its flags.
    const TOWER: u8 = 0x66;

    fn melee(hp: u16, b: u8, p: u8, f: u32) -> UnitStats {
        UnitStats {
            hit_points: hp,
            size: (1, 1),
            flags: f | flags::CAN_ATTACK,
            basic_damage: b,
            piercing_damage: p,
            attack_range: 1,
            missile: crate::combat::missile::NONE,
            can_target: 1,
            react_range_computer: 6,
            react_range_human: 4,
            sight: 9,
            priority: 50,
            ..Default::default()
        }
    }

    fn building(size: u16, f: u32) -> UnitStats {
        UnitStats {
            hit_points: 400,
            size: (size, size),
            flags: flags::BUILDING | f,
            sight: 3,
            ..Default::default()
        }
    }

    /// A 64 x 64 field with default-like unit data; player 0 human, player 1 computer. The synthetic scripts give
    /// every type an attack duration of 26, so every rate is 10.
    fn game() -> World {
        let mut w = field(64, 64);
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        for p in 2..8 {
            w.players[p].controller = controller::NOBODY;
        }
        let s = &mut w.stats.0;
        s[FOOTMAN as usize] = melee(60, 6, 3, flags::LAND);
        s[GRUNT as usize] = melee(60, 6, 3, flags::LAND);
        s[KNIGHT as usize] = melee(90, 8, 4, flags::LAND);
        s[ARCHER as usize] = UnitStats {
            missile: 0x0f,
            attack_range: 4,
            can_target: 7,
            ..melee(40, 3, 6, flags::LAND)
        };
        s[PEASANT as usize] = UnitStats {
            flags: flags::LAND | flags::PEON,
            ..melee(30, 3, 2, 0)
        };
        s[DESTROYER as usize] = UnitStats {
            missile: 0x0f,
            can_target: 7,
            ..melee(100, 35, 0, flags::SEA)
        };
        s[SUBMARINE as usize] = UnitStats {
            missile: 0x0f,
            can_target: 2,
            ..melee(60, 50, 0, flags::SEA | flags::SUBMARINE)
        };
        s[TRANSPORT as usize] = UnitStats {
            flags: flags::SEA | flags::TRANSPORT,
            ..melee(150, 0, 0, 0)
        };
        s[GRYPHON as usize] = UnitStats {
            missile: 0x0f,
            can_target: 7,
            ..melee(100, 0, 16, flags::AIR)
        };
        s[FARM as usize] = building(2, 0);
        s[TOWN_HALL as usize] = building(4, flags::GOLD_STORAGE);
        s[SHIPYARD as usize] = building(3, flags::SHORE_BUILDING);
        s[PLATFORM as usize] = building(3, flags::OIL_SOURCE);
        s[TOWER as usize] = UnitStats {
            missile: 0x0f,
            can_target: 7,
            attack_range: 6,
            ..building(2, flags::TOWER | flags::CAN_ATTACK)
        };
        for k in [DESTROYER, SUBMARINE, TRANSPORT, GRYPHON] {
            w.stride[k as usize] = 2;
        }
        w.rules.fog_of_war = false;
        w.army = Army::new(&w.terrain);
        w
    }

    /// Spawns a unit and runs the units' visibility, as if a visibility pass had run since (a new unit is hidden
    /// from everyone until then). Unit indices move as units are added (new slots sort first), so tests hold ids.
    fn spawn(w: &mut World, kind: u8, owner: u8, tile: (i32, i32)) -> UnitId {
        let id = w.spawn(kind, owner, tile, 0).unwrap();
        w.unit_visibility();
        id
    }

    fn ix(w: &World, id: UnitId) -> usize {
        w.index(id).unwrap()
    }

    fn at(w: &World, id: UnitId) -> &Unit {
        w.unit(id).unwrap()
    }

    fn um(w: &mut World, id: UnitId) -> &mut Unit {
        let i = ix(w, id);
        &mut w.units[i]
    }

    /// Makes columns `x0..x1` of the map open water, in its own region.
    fn water(w: &mut World, x0: i32, x1: i32) {
        for y in 0..64 {
            for x in x0..x1 {
                let n = (y * 64 + x) as usize;
                w.terrain.words[n] = bits::WATER;
                w.terrain.regions[n] = 2;
            }
        }
        w.army = Army::new(&w.terrain);
    }

    // ---- Weights and strengths ----

    #[test]
    fn attack_duration_walks_to_the_second_step_done() {
        // Step done, wait 10, strike, wait 14, continue at the start: 1 + 10 + 14 + 1.
        let code = [0, 1, 10, 11, 1, 14, 3, 0, 0];
        assert_eq!(attack_duration(&code, Some(0)), 26);
        // Every wait kind counts; frames and moves are skipped with their arguments.
        let code = [4, 9, 0, 7, 3, 8, 4, 9, 5, 12, 6, 6, 1, 2, 0];
        assert_eq!(attack_duration(&code, Some(0)), 1 + 3 + 4 + 5 + 6 + 1);
        assert_eq!(attack_duration(&code, None), 0);
    }

    #[test]
    fn weights_of_the_default_data() {
        // The spec's table: (D, B, P, w).
        let rows: [(u32, u8, u8, u16); 12] = [
            (26, 6, 3, 5),
            (26, 3, 2, 3),
            (201, 80, 0, 3),
            (26, 8, 4, 7),
            (66, 3, 6, 2),
            (41, 0, 9, 5),
            (26, 4, 2, 3),
            (121, 35, 0, 3),
            (231, 130, 0, 6),
            (101, 50, 0, 4),
            (2, 0, 0, 0),
            (215, 0, 16, 1),
        ];
        let r1 = rate(26, 26);
        assert_eq!(r1, 10);
        for (d, b, p, want) in rows {
            assert_eq!(weight(b, p, rate(26, d), r1), want, "D {d} B {b} P {p}");
        }
        assert_eq!(rate(26, 2), 130);
        assert_eq!(rate(26, 0), 0);
        // From the world's data: every synthetic attack animation lasts 26, so the gryphon here weighs 0 + 16.
        let w = game();
        let ws = weights(&w.stats, &w.scripts);
        assert_eq!(
            (ws[FOOTMAN as usize], ws[KNIGHT as usize], ws[GRYPHON as usize]),
            (5, 7, 16)
        );
    }

    #[test]
    fn strengths_and_dominant_owners() {
        let mut w = game();
        // A computer grunt and a human footman at 30 HP in block 0.
        spawn(&mut w, GRUNT, 1, (1, 1));
        let f = spawn(&mut w, FOOTMAN, 0, (2, 1));
        um(&mut w, f).hp = 30;
        // Block 1: a town hall and 2 peasants of player 0.
        spawn(&mut w, TOWN_HALL, 0, (10, 2));
        spawn(&mut w, PEASANT, 0, (9, 1));
        spawn(&mut w, PEASANT, 0, (9, 2));
        // Block 2: farms of players 2 and 3.
        spawn(&mut w, FARM, 2, (17, 1));
        spawn(&mut w, FARM, 3, (20, 1));
        // Block 3: a knight at full HP and a gryphon rider (not on the ground layer).
        spawn(&mut w, KNIGHT, 1, (25, 1));
        spawn(&mut w, GRYPHON, 0, (26, 2));
        w.rebuild_threat_map();
        let b = &w.army.blocks;
        // Grunt 60 x 5 / 60; footman 30 x 5 / 60.
        assert_eq!((b[0].land[1], b[0].land[0], b[0].dominant), (5, 2, Some(1)));
        // A gold store counts -1 per tile (16 tiles), each peasant -2.
        assert_eq!((b[1].land[0], b[1].dominant), (-20, Some(0)));
        // Equal sums: the lower player.
        assert_eq!((b[2].land[2], b[2].land[3], b[2].dominant), (-4, -4, Some(2)));
        assert_eq!((b[3].land[1], b[3].land[0], b[3].dominant), (10, 0, Some(1)));
    }

    #[test]
    fn water_and_mixed_blocks() {
        let mut w = game();
        water(&mut w, 8, 16);
        // Block 1 is water, judged on sea strengths: a footman adds nothing there.
        spawn(&mut w, FOOTMAN, 0, (9, 1));
        let d = spawn(&mut w, DESTROYER, 1, (12, 2));
        assert_eq!(at(&w, d).layer, Layer::Sea);
        w.rebuild_threat_map();
        let b = &w.army.blocks[1];
        assert_eq!(b.kind, BlockKind::Water);
        // 100 x ((35 - 1) / 2) / 60.
        assert_eq!((b.sea[0], b.sea[1], b.dominant), (0, 28, Some(1)));
        // Half water, half land: mixed, no dominant owner.
        let mut w = game();
        water(&mut w, 4, 8);
        spawn(&mut w, FOOTMAN, 0, (1, 1));
        w.rebuild_threat_map();
        assert_eq!(
            (w.army.blocks[0].kind, w.army.blocks[0].dominant),
            (BlockKind::Mixed, None)
        );
    }

    // ---- Timing and waves ----

    #[test]
    fn passes_run_in_the_first_start_up_cycle_then_every_50() {
        // "Timing": the first start-up cycle (trace record n = 1), then the cycles ending with the counter at 49, 99,
        // ... (n = 51, 101, ...).
        let mut w = game();
        spawn(&mut w, GRUNT, 1, (40, 40));
        let trigger = |w: &mut World| w.ai[1].slots[0x09] = 1;
        trigger(&mut w);
        w.step(&[]);
        assert_eq!(w.ai[1].slots[0x09], 0, "n = 1");
        trigger(&mut w);
        run(&mut w, 49);
        assert_eq!(w.ai[1].slots[0x09], 1);
        w.step(&[]);
        assert_eq!(w.ai[1].slots[0x09], 0, "n = 51");
        trigger(&mut w);
        run(&mut w, 49);
        assert_eq!(w.ai[1].slots[0x09], 1);
        w.step(&[]);
        assert_eq!(w.ai[1].slots[0x09], 0, "n = 101");
    }

    fn wave_world(held: usize) -> (World, Vec<UnitId>) {
        let mut w = game();
        spawn(&mut w, FOOTMAN, 0, (5, 5));
        let grunts: Vec<UnitId> = (0..10)
            .map(|k| {
                let g = spawn(&mut w, GRUNT, 1, (40 + k, 50));
                um(&mut w, g).held = (k as usize) < held;
                g
            })
            .collect();
        let s = &mut w.ai[1].slots;
        s[0x09] = 1;
        s[0x0d] = 4;
        s[0x0e] = 2;
        w.rebuild_threat_map();
        (w, grunts)
    }

    fn attackers(w: &World, ids: &[UnitId]) -> usize {
        ids.iter().filter(|&&g| at(w, g).goal.kind == GoalKind::Attack).count()
    }

    #[test]
    fn a_land_wave_sends_size_times_repeat_in_list_order() {
        let (mut w, grunts) = wave_world(0);
        w.attack_waves();
        assert_eq!(w.ai[1].slots[0x09], 0);
        assert_eq!(attackers(&w, &grunts), 8);
        // List order: the newest units come first in the owner's list, so the two oldest stay home.
        let list: Vec<UnitId> = w.list_of(1).into_iter().map(|i| w.units[i].id).collect();
        assert_eq!(&list[8..], &[grunts[1], grunts[0]]);
        // They keep the first goal they got when created (hold).
        for g in &list[8..] {
            assert_eq!(at(&w, *g).goal.kind, GoalKind::Hold);
        }
    }

    #[test]
    fn held_units_stay_out_of_waves() {
        let (mut w, grunts) = wave_world(3);
        w.attack_waves();
        assert_eq!(attackers(&w, &grunts), 7);
        assert!(grunts[..3].iter().all(|g| at(&w, *g).goal.kind == GoalKind::Hold));
    }

    #[test]
    fn no_target_clears_the_trigger_and_sends_nobody() {
        let (mut w, grunts) = wave_world(0);
        let f = w.units.iter().position(|u| u.owner == 0).unwrap();
        w.kill(f);
        spawn(&mut w, FARM, 0, (2, 2));
        w.attack_waves();
        assert_eq!((w.ai[1].slots[0x09], attackers(&w, &grunts)), (0, 0));
    }

    #[test]
    fn the_target_is_the_lowest_of_the_biggest_human_players() {
        let mut w = game();
        w.players[2].controller = controller::HUMAN;
        for k in 0..7 {
            spawn(&mut w, FOOTMAN, 0, (k, 0));
            spawn(&mut w, FOOTMAN, 2, (k, 10));
        }
        assert_eq!(w.target_player(), Some(0));
        spawn(&mut w, FOOTMAN, 2, (8, 10));
        assert_eq!(w.target_player(), Some(2));
    }

    #[test]
    fn the_land_picker_swaps_row_and_column() {
        let mut w = game();
        // The reference unit: a town hall at (2, 10). Two dominant land blocks: (row 2, col 5) and (row 5, col 2).
        // The distance uses the swapped point: (20, 44) is 31 away, (44, 20) 39, so the farther is block (row 5,
        // col 2), whose tile is (2 x 8 + 4, 5 x 8 + 4) = (20, 44).
        spawn(&mut w, TOWN_HALL, 0, (2, 10));
        w.rebuild_threat_map();
        for b in w.army.blocks.iter_mut() {
            b.dominant = None;
        }
        w.army.blocks[2 * 8 + 5].dominant = Some(0);
        w.army.blocks[5 * 8 + 2].dominant = Some(0);
        assert_eq!(w.land_picker(0), Some((2, 10)), "the only unit: the hall");
        spawn(&mut w, FOOTMAN, 0, (22, 40));
        assert_eq!(w.land_picker(0), Some((22, 40)));
        // Ships are left out.
        water(&mut w, 30, 64);
        w.army.blocks[5 * 8 + 2].dominant = Some(0);
        w.army.blocks[2 * 8 + 5].dominant = Some(0);
        spawn(&mut w, DESTROYER, 0, (32, 44));
        assert_eq!(w.land_picker(0), Some((22, 40)));
    }

    #[test]
    fn the_sea_picker_prefers_an_oil_platform() {
        let mut w = game();
        spawn(&mut w, SHIPYARD, 0, (20, 20));
        spawn(&mut w, PLATFORM, 0, (30, 30));
        spawn(&mut w, SHIPYARD, 0, (40, 40));
        assert_eq!(w.sea_picker(0), Some((30, 30)));
        let mut w = game();
        spawn(&mut w, SHIPYARD, 0, (20, 20));
        spawn(&mut w, SHIPYARD, 0, (40, 40));
        // The last shipyard in list order: the oldest.
        assert_eq!(w.sea_picker(0), Some((20, 20)));
        assert_eq!(game().sea_picker(0), None);
    }

    // ---- Goals ----

    #[test]
    fn hold_returns_past_the_leash() {
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (30, 30));
        let foe = spawn(&mut w, FOOTMAN, 0, (60, 60));
        let u = um(&mut w, g);
        u.goal.kind = GoalKind::Hold;
        u.goal.tile = Some((20, 30));
        u.attacked_by = Some(foe);
        w.rebuild_threat_map();
        w.run_goals();
        assert_eq!(at(&w, g).pending, Some(Order::Move { goal: (20, 30) }));
        assert_eq!(at(&w, g).attacked_by, None);
        // Within the leash (6 / 2 + 4 = 7): nothing.
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (26, 30));
        let u = um(&mut w, g);
        u.goal.kind = GoalKind::Hold;
        u.goal.tile = Some((20, 30));
        w.rebuild_threat_map();
        w.run_goals();
        assert_eq!(at(&w, g).pending, None);
    }

    /// A computer grunt fighting (order 12) 10 tiles from its post, in block 0 where player 0 is dominant with 13.
    fn outmatched(own: i16) -> Option<Order> {
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (2, 2));
        let foe = spawn(&mut w, FOOTMAN, 0, (3, 3));
        let u = um(&mut w, g);
        u.goal.kind = GoalKind::Hold;
        u.goal.tile = Some((12, 2));
        u.order = Order::Chase { target: foe };
        let b = &mut w.army.blocks[0];
        b.kind = BlockKind::Land;
        b.dominant = Some(0);
        b.land[0] = 13;
        b.land[1] = own;
        w.run_goals();
        at(&w, g).pending
    }

    #[test]
    fn hold_gives_up_a_fight_when_badly_outmatched() {
        // 10 x 5 < 4 x 13; 10 x 6 >= 4 x 13.
        assert_eq!(outmatched(5), Some(Order::Move { goal: (12, 2) }));
        assert_eq!(outmatched(6), None);
    }

    #[test]
    fn attack_goal_rules() {
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        let foe = spawn(&mut w, FOOTMAN, 0, (50, 10));
        um(&mut w, g).goal.kind = GoalKind::Attack;
        um(&mut w, g).goal.tile = Some((10, 10));
        w.run_goals();
        assert_eq!(at(&w, g).pending, Some(Order::Chase { target: foe }));
        // In order 12, 20 tiles away: nothing.
        let u = um(&mut w, g);
        u.pending = None;
        u.order = Order::Chase { target: foe };
        u.goal.tile = Some((30, 10));
        w.run_goals();
        assert_eq!(at(&w, g).pending, None);
        // Moving (order 4), 2 tiles from the goal tile: within the leash, not idle.
        let u = um(&mut w, g);
        u.order = Order::MoveAttack { goal: (12, 10) };
        u.goal.tile = Some((12, 10));
        w.run_goals();
        assert_eq!((at(&w, g).pending, at(&w, g).goal.kind), (None, GoalKind::Attack));
        // Idle far from the goal tile: marches with command 4.
        let u = um(&mut w, g);
        u.order = Order::Idle;
        u.goal.tile = Some((30, 10));
        w.run_goals();
        assert_eq!(at(&w, g).pending, Some(Order::MoveAttack { goal: (30, 10) }));
        // Idle at the goal, no enemy left: becomes a roamer.
        let fi = ix(&w, foe);
        w.kill(fi);
        let u = um(&mut w, g);
        u.pending = None;
        u.goal.tile = Some((10, 10));
        w.run_goals();
        assert_eq!(at(&w, g).goal.kind, GoalKind::Roam);
    }

    #[test]
    fn roam_tile_vector() {
        assert_eq!(roam_tile((10, 10), 64, 5, 40), (0, 2));
    }

    #[test]
    fn roam_draws_twice_and_moves() {
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        um(&mut w, g).goal.kind = GoalKind::Roam;
        let mut r = w.rng;
        let want = roam_tile((10, 10), 64, r.draw(), r.draw());
        w.run_goals();
        assert_eq!(w.rng, r);
        assert_eq!(at(&w, g).pending, Some(Order::MoveAttack { goal: want }));
    }

    #[test]
    fn set_goal_needs_a_ferry_across_water() {
        let mut w = game();
        water(&mut w, 30, 34);
        for n in 0..64 * 64usize {
            if n % 64 >= 34 {
                w.terrain.regions[n] = 3;
            }
        }
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        assert!(!w.set_goal(ix(&w, g), GoalKind::Attack, Some((40, 10))));
        let goal = at(&w, g).goal;
        assert_eq!(
            (goal.kind, goal.tile, goal.needs_ferry, goal.area),
            (GoalKind::Attack, Some((40, 10)), true, 3)
        );
        // A land goal on water is moved to the nearest land tile first.
        let k = spawn(&mut w, GRUNT, 1, (12, 12));
        assert!(w.set_goal(ix(&w, k), GoalKind::Hold, Some((30, 12))));
        assert_eq!(at(&w, k).goal.tile, Some((29, 12)));
        // A ship can't take a goal in another water area, and nothing changes.
        let d = spawn(&mut w, DESTROYER, 1, (32, 10));
        um(&mut w, d).goal.kind = GoalKind::SeaPatrol;
        assert!(!w.set_goal(ix(&w, d), GoalKind::Escort, Some((10, 10))));
        assert_eq!(at(&w, d).goal.kind, GoalKind::SeaPatrol);
    }

    #[test]
    fn first_goals_on_completion() {
        let mut w = game();
        water(&mut w, 40, 64);
        // No gold store: the post is the unit's own tile, which its own mark occupies, so the goal-tile ring search
        // moves the goal to a free neighbour.
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        w.first_goal(ix(&w, g));
        let goal = at(&w, g).goal;
        assert_eq!(goal.kind, GoalKind::Hold);
        let t = goal.tile.unwrap();
        assert!(t != (10, 10) && (t.0 - 10).abs() <= 1 && (t.1 - 10).abs() <= 1);
        let t = spawn(&mut w, TRANSPORT, 1, (50, 10));
        let d = spawn(&mut w, DESTROYER, 1, (50, 20));
        let a = spawn(&mut w, GRYPHON, 1, (20, 20));
        let p = spawn(&mut w, PEASANT, 1, (11, 11));
        let h = spawn(&mut w, GRUNT, 0, (12, 12));
        for u in [t, d, a, p, h] {
            w.first_goal(ix(&w, u));
        }
        let kinds: Vec<GoalKind> = [t, d, a, p, h].iter().map(|&u| at(&w, u).goal.kind).collect();
        assert_eq!(
            kinds,
            [
                GoalKind::Ferry,
                GoalKind::SeaPatrol,
                GoalKind::Roam,
                GoalKind::None,
                GoalKind::None
            ]
        );
        assert_eq!(at(&w, a).goal.tile, Some((20, 20)));
    }

    #[test]
    fn the_post_lies_on_the_line_to_the_enemy_at_the_box_edge() {
        let mut w = game();
        // List order hall, farm (newest first): box x 5..28, y 5..18.
        spawn(&mut w, FARM, 1, (20, 10));
        spawn(&mut w, TOWN_HALL, 1, (10, 10));
        spawn(&mut w, FOOTMAN, 0, (50, 10));
        w.rebuild_base_box(1);
        assert_eq!(
            w.army.boxes[1],
            BaseBox {
                min_x: 5,
                min_y: 5,
                max_x: 28,
                max_y: 18
            }
        );
        // From the hall's tile east toward the footman: (29, 10) is the first tile outside.
        let g = spawn(&mut w, GRUNT, 1, (14, 16));
        let before = w.rng;
        assert_eq!(w.post(ix(&w, g)), (29, 10));
        assert_eq!(w.rng, before);
    }

    #[test]
    fn units_get_their_first_goal_when_created() {
        // Unit-slots spec, step 6: every created unit, map units included, with no call from outside.
        let mut w = game();
        water(&mut w, 40, 64);
        let kinds: Vec<GoalKind> = [
            (GRUNT, 1, (10, 10)),
            (TRANSPORT, 1, (50, 10)),
            (DESTROYER, 1, (50, 20)),
            (GRYPHON, 1, (20, 20)),
            (PEASANT, 1, (11, 11)),
            (GRUNT, 0, (12, 12)),
        ]
        .into_iter()
        .map(|(k, p, t)| {
            let u = spawn(&mut w, k, p, t);
            at(&w, u).goal.kind
        })
        .collect();
        use GoalKind::*;
        assert_eq!(kinds, [Hold, Ferry, SeaPatrol, Roam, None, None]);
    }

    #[test]
    fn an_off_map_post_draws_before_the_first_wait_draw() {
        // Timing spec, draw table: a mobile unit's creation draws its facing, then a computer land unit's post choice
        // off the map draws x then y, and the extra first-wait draw comes last.
        let mut w = game();
        // A box as large as the map: the walk west from the hall toward the footman leaves it beyond the left edge.
        spawn(&mut w, TOWN_HALL, 1, (4, 10));
        spawn(&mut w, FOOTMAN, 0, (0, 10));
        w.army.boxes[1] = BaseBox {
            min_x: 0,
            min_y: 0,
            max_x: 63,
            max_y: 63,
        };
        let mut r = w.rng;
        let facing = r.draw() & 7;
        let (dx, dy, extra) = (r.draw() % 12, r.draw() % 12, r.draw() & 7);
        let g = spawn(&mut w, GRUNT, 1, (30, 30));
        assert_eq!(w.rng, r, "four draws");
        let want = (30 + dx as i32 - 6, 30 + dy as i32 - 6);
        let u = at(&w, g);
        assert_eq!((u.facing as u32, u.goal.kind), (facing, GoalKind::Hold));
        if want != (30, 30) {
            assert_eq!(u.goal.tile, Some(want));
        }
        assert_eq!(u.script.wait as u32, 1 + extra);
    }

    #[test]
    fn line_walk_steps() {
        let inside = |(x, _): (i32, i32)| x < 3;
        assert_eq!(line_walk((0, 0), (10, 5), inside), (3, 2));
        assert_eq!(line_walk((5, 5), (10, 5), inside), (5, 5), "starts outside");
        assert_eq!(line_walk((0, 0), (0, 0), inside), (0, 0), "same tile");
        assert_eq!(line_walk((0, 0), (1, 8), |(_, y)| y < 4), (1, 4));
    }

    #[test]
    fn held_units_hold_their_tile() {
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        um(&mut w, g).held = true;
        w.held_goal(ix(&w, g));
        assert_eq!(
            (at(&w, g).goal.kind, at(&w, g).goal.tile),
            (GoalKind::Hold, Some((10, 10)))
        );
        let p = spawn(&mut w, PEASANT, 1, (11, 10));
        um(&mut w, p).held = true;
        w.held_goal(ix(&w, p));
        assert_eq!(at(&w, p).goal.kind, GoalKind::None);
    }

    // ---- Base box ----

    #[test]
    fn base_box_vectors() {
        let b = |min_x, min_y, max_x, max_y| BaseBox {
            min_x,
            min_y,
            max_x,
            max_y,
        };
        assert_eq!(base_box(64, &[(10, 12), (20, 25)]), b(5, 7, 28, 33));
        assert_eq!(base_box(64, &[(20, 25), (10, 12)]), b(5, 7, 7, 7));
        assert_eq!(base_box(64, &[(10, 12)]), b(5, 7, 7, 7));
        assert_eq!(base_box(64, &[(40, 40), (60, 50)]).max_x, 63);
        let wide = base_box(128, &[(10, 12), (20, 25)]);
        assert_eq!((wide.min_x, wide.min_y), (123, 123));
        assert_eq!(BaseBox::empty(64), b(64, 64, -1, -1));
    }

    #[test]
    fn no_gold_store_no_box_and_the_edge_patrol_roams() {
        let mut w = game();
        spawn(&mut w, FARM, 1, (10, 10));
        let g = spawn(&mut w, GRUNT, 1, (20, 20));
        w.rebuild_base_box(1);
        assert_eq!(w.army.boxes[1], BaseBox::empty(64));
        um(&mut w, g).goal.kind = GoalKind::BaseEdge;
        w.run_goals();
        assert_eq!(at(&w, g).goal.kind, GoalKind::Roam);
    }

    #[test]
    fn the_edge_patrol_turns_round_the_sides() {
        let mut w = game();
        // List order (10, 12) then (20, 25): box x 5..28, y 7..33.
        spawn(&mut w, FARM, 1, (20, 25));
        spawn(&mut w, TOWN_HALL, 1, (10, 12));
        let g = spawn(&mut w, GRUNT, 1, (15, 15));
        w.rebuild_base_box(1);
        um(&mut w, g).goal.kind = GoalKind::BaseEdge;
        w.army.sides[1] = 1;
        w.run_goals();
        assert_eq!(
            at(&w, g).pending,
            Some(Order::Patrol {
                to: (28, 7),
                back: (28, 33)
            })
        );
        assert_eq!(w.army.sides[1], 2);
    }

    // ---- Sea patrol and ferry ----

    #[test]
    fn sea_patrol_falls_back_to_a_random_far_end() {
        let mut w = game();
        water(&mut w, 32, 64);
        let d = spawn(&mut w, DESTROYER, 1, (40, 40));
        um(&mut w, d).goal.kind = GoalKind::SeaPatrol;
        let mut r = w.rng;
        let (a, b) = (r.draw(), r.draw());
        w.run_goals();
        assert_eq!(w.rng, r);
        let far = (40 - (a % 5) as i32 - 2, 40 - (b % 5) as i32 - 2);
        assert_eq!(
            at(&w, d).pending,
            Some(Order::Patrol {
                to: (40, 40),
                back: far
            })
        );
        // With an oil platform, it patrols to the platform without drawing.
        spawn(&mut w, PLATFORM, 1, (50, 50));
        um(&mut w, d).pending = None;
        let before = w.rng;
        w.run_goals();
        assert_eq!(w.rng, before);
        assert_eq!(
            at(&w, d).pending,
            Some(Order::Patrol {
                to: (40, 40),
                back: (50, 50)
            })
        );
    }

    /// A transport at (40, 10) on water (x 32 to 53, region 2), land west of it (region 1); for x >= 54 region 3:
    /// shore water at x 54 and 55, land beyond.
    fn harbour() -> (World, UnitId) {
        let mut w = game();
        water(&mut w, 32, 56);
        for y in 0..64usize {
            for x in 0..64usize {
                let n = y * 64 + x;
                if x < 32 {
                    w.terrain.regions[n] = 1;
                } else if x >= 54 {
                    w.terrain.regions[n] = 3;
                    w.terrain.words[n] = if x < 56 { bits::SHORE } else { bits::LAND };
                }
            }
        }
        let t = spawn(&mut w, TRANSPORT, 1, (40, 10));
        um(&mut w, t).goal.kind = GoalKind::Ferry;
        (w, t)
    }

    /// Eight grunts of player 1 that need a ferry to (60, 10), in player 1's list order.
    fn ferry_grunts(w: &mut World) -> Vec<UnitId> {
        let grunts: Vec<UnitId> = (0..8)
            .map(|k| {
                let g = spawn(w, GRUNT, 1, (10 + k, 10));
                w.set_goal(ix(w, g), GoalKind::Attack, Some((60, 10)));
                g
            })
            .collect();
        assert!(grunts.iter().all(|&g| at(w, g).goal.needs_ferry));
        w.list_of(1)
            .into_iter()
            .map(|i| w.units[i].id)
            .filter(|g| grunts.contains(g))
            .collect()
    }

    fn boards(w: &World, g: UnitId, t: UnitId) -> bool {
        matches!(at(w, g).pending, Some(Order::Board { transport }) if transport == t)
    }

    #[test]
    fn a_transport_gathers_six_and_stops() {
        let (mut w, t) = harbour();
        let list = ferry_grunts(&mut w);
        w.run_goals();
        // The first 6 in list order are assigned and told to board; the walk stops there, so the other 2 keep their
        // old assignment (none).
        for (n, g) in list.iter().enumerate() {
            let u = at(&w, *g);
            assert_eq!(u.goal.transport, (n < 6).then_some(t), "unit {n}");
            assert_eq!(boards(&w, *g, t), n < 6, "unit {n}: {:?}", u.pending);
        }
    }

    #[test]
    fn a_transport_gathers_for_the_first_candidates_area_only() {
        let (mut w, t) = harbour();
        // The land east of the water is split: area A (region 3) north of y 32, area B (region 4) south of it.
        for y in 32..64usize {
            for x in 54..64usize {
                w.terrain.regions[y * 64 + x] = 4;
            }
        }
        let list = ferry_grunts(&mut w);
        // The first in list order is bound for area B; the other 7 for area A.
        let first = ix(&w, list[0]);
        w.set_goal(first, GoalKind::Attack, Some((60, 40)));
        assert_eq!((at(&w, list[0]).goal.area, at(&w, list[1]).goal.area), (4, 3));
        w.run_goals();
        // The first fixes the trip area B and boards (count 1). The 7 area-A grunts are all assigned with no command:
        // only 1 was counted, so the walk goes on to the end of the list.
        assert_eq!(at(&w, list[0]).goal.transport, Some(t));
        assert!(boards(&w, list[0], t));
        for (n, g) in list.iter().enumerate().skip(1) {
            assert_eq!(at(&w, *g).goal.transport, Some(t), "unit {n}");
            assert!(!boards(&w, *g, t), "unit {n}: no command");
        }
    }

    #[test]
    fn a_loaded_transport_sails_to_the_last_passengers_goal_and_unloads() {
        let (mut w, t) = harbour();
        // Set at game start (after the opening run).
        w.ai[1].slots[FERRY_SEARCH] = 1;
        let a = spawn(&mut w, GRUNT, 1, (10, 10));
        let b = spawn(&mut w, GRUNT, 1, (11, 10));
        // Only the goal tiles matter here, not the first goals the grunts got when created.
        um(&mut w, a).goal = Goal::default();
        um(&mut w, b).goal = Goal::default();
        um(&mut w, a).goal.tile = Some((60, 5));
        um(&mut w, b).goal.tile = Some((60, 20));
        for (slot, p) in [a, b].into_iter().enumerate() {
            let unit = at(&w, p).clone();
            w.mark(&unit, unit.tile, None);
            um(&mut w, p).inside = true;
            um(&mut w, t).passengers[slot] = Some(p);
        }
        let mut r = w.rng;
        let d0 = r.draw();
        w.run_goals();
        assert_eq!(w.rng, r, "one draw for the landing spot");
        // From (60, 20), B's goal: the first shore tile of region 3 in direction d0 and on.
        let want = w.landing_spot((60, 20), d0);
        assert!(w.terrain.word(want.0, want.1) & bits::SHORE != 0);
        // The landing spot is shore water and the transport is on open water, so the command pulls the target tile
        // back toward the transport (transports spec, "Land and shore targets").
        let tr = at(&w, t);
        assert!(tr.ship.unload);
        assert!(tr.pending.is_some());
        assert_eq!(tr.ship.dest, w.shore_target(tr.tile, want));
    }

    #[test]
    fn the_landing_spot_search() {
        let (w, _) = harbour();
        // Facing 2 (east) from (60, 20) leaves the map with no shore; the search goes on clockwise, and the first
        // direction with a westward part reaches the shore.
        let spot = w.landing_spot((60, 20), 2);
        assert_eq!(w.terrain.region(spot.0, spot.1), Some(3));
        assert!(w.terrain.word(spot.0, spot.1) & bits::SHORE != 0);
        // No shore tile in the region at all: the goal itself.
        let mut w = game();
        water(&mut w, 32, 64);
        assert_eq!(w.landing_spot((10, 10), 0), (10, 10));
    }

    #[test]
    fn boarding_clears_the_ferry_marks() {
        let mut w = game();
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        um(&mut w, g).goal.needs_ferry = true;
        um(&mut w, g).goal.transport = Some(UnitId(99));
        w.boarded(ix(&w, g));
        assert!(!at(&w, g).goal.needs_ferry && at(&w, g).goal.transport.is_none());
    }

    // ---- Call for help ----

    #[test]
    fn neighbours_answer_a_computer_call() {
        let mut w = game();
        let v = spawn(&mut w, FOOTMAN, 1, (20, 20));
        let a = spawn(&mut w, ARCHER, 0, (24, 20));
        let near: Vec<UnitId> = (0..3).map(|k| spawn(&mut w, GRUNT, 1, (15 + k, 25))).collect();
        let far = spawn(&mut w, GRUNT, 1, (40, 20));
        w.step(&[]);
        for u in near.iter().chain([&far, &v]) {
            um(&mut w, *u).pending = None;
            um(&mut w, *u).order = Order::Idle;
        }
        w.damage(ix(&w, v), 1, a);
        assert_eq!(at(&w, v).attacked_by, Some(a));
        for k in near {
            assert_eq!(at(&w, k).pending, Some(Order::Chase { target: a }));
        }
        assert_eq!(at(&w, far).pending, None, "outside R = 13");
    }

    #[test]
    fn held_neighbours_answer_only_close_by() {
        let mut w = game();
        let v = spawn(&mut w, FOOTMAN, 1, (20, 20));
        let a = spawn(&mut w, ARCHER, 0, (24, 20));
        let close = spawn(&mut w, GRUNT, 1, (17, 20));
        let away = spawn(&mut w, GRUNT, 1, (12, 20));
        w.step(&[]);
        for u in [close, away] {
            let u = um(&mut w, u);
            u.held = true;
            u.pending = None;
            u.order = Order::Idle;
        }
        w.damage(ix(&w, v), 1, a);
        assert_eq!(at(&w, close).pending, Some(Order::Chase { target: a }));
        assert_eq!(at(&w, close).goal.kind, GoalKind::Attack);
        assert_eq!(at(&w, away).pending, None);
    }

    #[test]
    fn the_first_live_attacker_is_kept() {
        let mut w = game();
        let v = spawn(&mut w, FOOTMAN, 1, (20, 20));
        let a = spawn(&mut w, FOOTMAN, 0, (21, 20));
        let b = spawn(&mut w, FOOTMAN, 0, (19, 20));
        w.damage(ix(&w, v), 1, a);
        w.damage(ix(&w, v), 1, b);
        assert_eq!(at(&w, v).attacked_by, Some(a));
        um(&mut w, a).dying = true;
        w.damage(ix(&w, v), 1, b);
        assert_eq!(at(&w, v).attacked_by, Some(b));
    }

    #[test]
    fn human_help_radius_and_towers() {
        let mut w = game();
        let v = spawn(&mut w, FOOTMAN, 0, (20, 20));
        let friend = spawn(&mut w, FOOTMAN, 0, (23, 20));
        let a = spawn(&mut w, GRUNT, 1, (21, 21));
        w.step(&[]);
        um(&mut w, friend).pending = None;
        um(&mut w, friend).order = Order::Idle;
        w.damage(ix(&w, v), 1, a);
        assert_eq!(at(&w, friend).pending, None, "3 tiles: outside R = 2");
        // A human unit hit by a tower doesn't record it.
        let mut w = game();
        let v = spawn(&mut w, FOOTMAN, 0, (20, 20));
        let tw = spawn(&mut w, TOWER, 1, (22, 20));
        w.damage(ix(&w, v), 1, tw);
        assert_eq!(at(&w, v).attacked_by, None);
    }

    #[test]
    fn a_submarine_attacker_calls_air_units_and_submarines() {
        let mut w = game();
        water(&mut w, 0, 64);
        let v = spawn(&mut w, DESTROYER, 1, (20, 20));
        let sub = spawn(&mut w, SUBMARINE, 0, (22, 20));
        let busy = spawn(&mut w, SUBMARINE, 1, (24, 24));
        let gryphon = spawn(&mut w, GRYPHON, 1, (50, 50));
        um(&mut w, busy).attacked_by = Some(sub);
        w.damage(ix(&w, v), 1, sub);
        assert_eq!(at(&w, gryphon).pending, Some(Order::MoveAttack { goal: (20, 20) }));
        // A destroyer attacker: no submarine call.
        let mut w = game();
        water(&mut w, 0, 64);
        let v = spawn(&mut w, SUBMARINE, 1, (20, 20));
        let d = spawn(&mut w, DESTROYER, 0, (22, 20));
        let g = spawn(&mut w, GRYPHON, 1, (50, 50));
        w.damage(ix(&w, v), 1, d);
        assert_eq!(at(&w, g).pending, None);
    }

    #[test]
    fn a_resting_building_forgets_its_attacker() {
        let mut w = game();
        let farm = spawn(&mut w, FARM, 1, (20, 20));
        let a = spawn(&mut w, FOOTMAN, 0, (50, 50));
        w.damage(ix(&w, farm), 1, a);
        assert_eq!(at(&w, farm).attacked_by, Some(a));
        run(&mut w, 300);
        assert_eq!(at(&w, farm).attacked_by, None);
    }

    #[test]
    fn resting_orders_14_and_33_forget_the_attacker_37_and_59_keep_it() {
        use crate::economy::{Job, JobKind};
        use crate::rescue::CIRCLE_OF_POWER;
        let mut w = game();
        w.stats.0[CIRCLE_OF_POWER as usize] = building(2, 0);
        let a = spawn(&mut w, FOOTMAN, 0, (50, 50));
        // Order 33 for every owner: a human farm forgets too.
        let human_farm = spawn(&mut w, FARM, 0, (20, 20));
        // Order 14: a tower.
        let tower = spawn(&mut w, TOWER, 1, (30, 30));
        // Order 37: a building training a unit.
        let hall = spawn(&mut w, TOWN_HALL, 1, (10, 30));
        um(&mut w, hall).job = Some(Job {
            kind: JobKind::Train(PEASANT),
            steps: u16::MAX,
            progress: 0,
            running: true,
            paid: Default::default(),
        });
        // Order 59: the circle of power.
        let circle = spawn(&mut w, CIRCLE_OF_POWER, 1, (40, 10));
        for b in [human_farm, tower, hall, circle] {
            um(&mut w, b).attacked_by = Some(a);
        }
        run(&mut w, 300);
        assert_eq!(at(&w, human_farm).attacked_by, None, "order 33");
        assert_eq!(at(&w, tower).attacked_by, None, "order 14");
        assert_eq!(at(&w, hall).attacked_by, Some(a), "order 37");
        assert_eq!(at(&w, circle).attacked_by, Some(a), "order 59");
    }

    #[test]
    fn a_used_target_takes_nothing() {
        let mut w = game();
        let v = spawn(&mut w, FOOTMAN, 1, (20, 20));
        let a = spawn(&mut w, FOOTMAN, 0, (21, 20));
        um(&mut w, v).used = true;
        w.damage(ix(&w, v), 10, a);
        assert_eq!((at(&w, v).hp, at(&w, v).attacked_by), (60, None));
    }

    #[test]
    fn losing_buildings_starts_the_all_out_attack() {
        let mut w = game();
        spawn(&mut w, FOOTMAN, 0, (50, 50));
        let farms: Vec<UnitId> = (0..3).map(|k| spawn(&mut w, FARM, 1, (2 + 3 * k, 2))).collect();
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        let p = spawn(&mut w, PEASANT, 1, (11, 10));
        um(&mut w, g).held = true;
        w.rebuild_threat_map();
        // The target tile is the footman's; it occupies it, so the goal tile is moved to a free neighbour.
        let f = ix(&w, farms[0]);
        w.kill(f);
        let goal = at(&w, g).goal;
        assert_eq!(goal.kind, GoalKind::Attack, "held units go too");
        let t = goal.tile.unwrap();
        assert!((t.0 - 50).abs() <= 1 && (t.1 - 50).abs() <= 1);
        assert_eq!(at(&w, p).goal.kind, GoalKind::Attack);
        // The worker also gets command 3 to the tile (a smart command on an enemy attacks it).
        assert!(at(&w, p).pending.is_some());
        // With 3 buildings left, nothing.
        let mut w = game();
        spawn(&mut w, FOOTMAN, 0, (50, 50));
        let farms: Vec<UnitId> = (0..4).map(|k| spawn(&mut w, FARM, 1, (2 + 3 * k, 2))).collect();
        let g = spawn(&mut w, GRUNT, 1, (10, 10));
        let f = ix(&w, farms[0]);
        w.kill(f);
        assert_eq!(at(&w, g).goal.kind, GoalKind::Hold, "the first goal from creation");
    }
}
