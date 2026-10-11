use std::sync::Arc;

use tidewater_formats::pud::Pud;
use tidewater_formats::script::{Scripts, anim};
use tidewater_formats::tileset::Tileset;
use tidewater_formats::udta::{UnitStats, UnitTable, flags};

use crate::anim::{Modifiers, Script};
use crate::rng::Rng;
use crate::terrain::{Terrain, Tile};
use crate::{TILE_PX, path};

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("map has {got} tiles, expected {want}")]
    BadMap { got: usize, want: usize },
}

/// Stable unit handle; never reused within a game.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitId(pub u32);

/// Which occupancy plane a unit lives on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Layer {
    Land,
    Sea,
    Air,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Order {
    Idle,
    Move {
        goal: (i32, i32),
        /// Remaining tiles, next first.
        path: Vec<(i32, i32)>,
        /// Ticks spent waiting for a blocked tile.
        wait: u16,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Unit {
    pub id: UnitId,
    pub kind: u8,
    pub owner: u8,
    /// Top-left tile of the footprint (the destination tile while moving).
    pub tile: (i32, i32),
    /// Tile being left while moving; it stays reserved until the unit arrives.
    pub from: Option<(i32, i32)>,
    /// Centre in map pixels.
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
    /// Cycles before an idle unit looks around again.
    pub idle_wait: u16,
}

impl Unit {
    pub fn is_moving(&self) -> bool {
        self.from.is_some()
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
}

/// Numbers that specs will replace. Every field here is a placeholder of our own choosing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Rules {
    /// Cycles a unit waits for a blocked tile before searching a new path.
    pub patience: u16,
}

impl Default for Rules {
    fn default() -> Self {
        Self { patience: 10 }
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
    /// Sorted by id.
    pub units: Vec<Unit>,
    next_id: u32,
    /// Per layer, the unit standing on (or moving into or out of) each tile.
    occupancy: [Vec<Option<UnitId>>; 3],
}

fn layer_index(l: Layer) -> usize {
    match l {
        Layer::Land => 0,
        Layer::Sea => 1,
        Layer::Air => 2,
    }
}

fn centre(tile: (i32, i32), size: (u8, u8)) -> (i32, i32) {
    (
        tile.0 * TILE_PX + size.0 as i32 * TILE_PX / 2,
        tile.1 * TILE_PX + size.1 as i32 * TILE_PX / 2,
    )
}

fn facing(dx: i32, dy: i32) -> u8 {
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
        World {
            tick: 0,
            occupancy: std::array::from_fn(|_| vec![None; n]),
            terrain,
            stats,
            rules: Rules::default(),
            players: [Player::default(); PLAYERS],
            rng: Rng::new(seed),
            scripts,
            stride,
            units: Vec::new(),
            next_id: 1,
        }
    }

    /// Builds the starting state of a map. Start locations become [`Player::start`], not units. Map units are created
    /// in map order, each making its creation draws.
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
        let terrain = Terrain::from_map(tileset, &pud.tiles, pud.width, pud.height);
        let mut w = World::new(terrain, stats, scripts, seed);
        for (i, p) in w.players.iter_mut().enumerate() {
            let (g, l, o) = pud.start_resources(i);
            *p = Player {
                gold: g as u32,
                lumber: l as u32,
                oil: o as u32,
                race: pud.sides().get(i).copied().unwrap_or(2),
                start: None,
            };
        }
        for u in &pud.units {
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
            w.spawn(u.kind, u.owner, (u.x as i32, u.y as i32), resources);
        }
        Ok(w)
    }

    fn stats_of(&self, kind: u8) -> UnitStats {
        self.stats.0.get(kind as usize).copied().unwrap_or_default()
    }

    /// Creates a unit with full hit points, making the creation draws (timing-and-rng spec, "Draw sites"): a mobile
    /// unit's facing, a building's variation value, then extra cycles on the first animation wait. Returns its id.
    pub fn spawn(&mut self, kind: u8, owner: u8, tile: (i32, i32), resources: u32) -> UnitId {
        let s = self.stats_of(kind);
        let building = s.flags & flags::BUILDING != 0;
        let layer = if s.flags & flags::AIR != 0 {
            Layer::Air
        } else if s.flags & flags::SEA != 0 || (building && s.flags & flags::OIL_PATCH != 0) {
            Layer::Sea
        } else {
            Layer::Land
        };
        let size = (s.size.0.clamp(1, 8) as u8, s.size.1.clamp(1, 8) as u8);
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
            kind,
            owner,
            tile,
            from: None,
            pos: centre(tile, size),
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
            idle_wait: 0,
        };
        self.mark(&unit, unit.tile, Some(id));
        self.units.push(unit);
        id
    }

    fn mark(&mut self, unit: &Unit, tile: (i32, i32), v: Option<UnitId>) {
        let w = self.terrain.width as i32;
        let occ = &mut self.occupancy[layer_index(unit.layer)];
        for dy in 0..unit.size.1 as i32 {
            for dx in 0..unit.size.0 as i32 {
                let (x, y) = (tile.0 + dx, tile.1 + dy);
                if self.terrain.contains(x, y) {
                    occ[(y * w + x) as usize] = v;
                }
            }
        }
    }

    /// The unit occupying a tile on a layer.
    pub fn occupant(&self, layer: Layer, x: i32, y: i32) -> Option<UnitId> {
        if !self.terrain.contains(x, y) {
            return None;
        }
        self.occupancy[layer_index(layer)][(y * self.terrain.width as i32 + x) as usize]
    }

    fn terrain_allows(&self, layer: Layer, x: i32, y: i32) -> bool {
        let t = self.terrain.get(x, y);
        self.terrain.contains(x, y)
            && match layer {
                Layer::Land => t.has(Tile::LAND),
                Layer::Sea => t.has(Tile::SEA),
                Layer::Air => true,
            }
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units
            .binary_search_by_key(&id, |u| u.id)
            .ok()
            .map(|i| &self.units[i])
    }

    fn index(&self, id: UnitId) -> Option<usize> {
        self.units.binary_search_by_key(&id, |u| u.id).ok()
    }

    /// Runs one cycle: applies `commands` in order, then updates every unit in id order (phase 4 of the
    /// timing-and-rng spec; unit slot order is a placeholder until its spec lands).
    pub fn step(&mut self, commands: &[Command]) {
        for c in commands {
            self.apply(c);
        }
        for i in 0..self.units.len() {
            self.advance(i);
        }
        self.tick += 1;
    }

    fn apply(&mut self, c: &Command) {
        let (player, units) = match c {
            Command::Move { player, units, .. } | Command::Stop { player, units } => (*player, units),
        };
        for &id in units {
            let Some(i) = self.index(id) else { continue };
            let u = &self.units[i];
            if u.owner != player || u.building || self.stride_of(u.kind) == 0 {
                continue;
            }
            match c {
                Command::Move { to, .. } => {
                    let path = self.plan(u, *to, false);
                    self.units[i].order = Order::Move {
                        goal: *to,
                        path,
                        wait: 0,
                    };
                }
                Command::Stop { .. } => self.go_idle(i),
            }
        }
    }

    fn stride_of(&self, kind: u8) -> i32 {
        self.stride.get(kind as usize).copied().unwrap_or(0) as i32
    }

    /// Ends the unit's order, with the idle draw (timing-and-rng spec, "Unit goes idle").
    fn go_idle(&mut self, i: usize) {
        let wait = (self.rng.draw() & 15) as u16 + 44;
        let u = &mut self.units[i];
        u.order = Order::Idle;
        u.idle_wait = wait;
    }

    fn start_anim(&mut self, i: usize, a: u8) {
        let u = &mut self.units[i];
        let pc = self.scripts.unit(u.kind, a).unwrap_or(0);
        u.script.start(a, pc);
    }

    /// A path for `u` to `goal`. With `avoid_units`, tiles held by other units count as blocked. Units that move two
    /// tiles per step plan on the grid of tiles with their own tile's parity (a placeholder until the movement spec).
    fn plan(&self, u: &Unit, goal: (i32, i32), avoid_units: bool) -> Vec<(i32, i32)> {
        let s = self.stride_of(u.kind).max(1);
        let (px, py) = (u.tile.0.rem_euclid(s), u.tile.1.rem_euclid(s));
        let (w, h) = (self.terrain.width as i32, self.terrain.height as i32);
        let (cw, ch) = ((w - px + s - 1) / s, (h - py + s - 1) / s);
        let tile = move |c: (i32, i32)| (c.0 * s + px, c.1 * s + py);
        let start = ((u.tile.0 - px) / s, (u.tile.1 - py) / s);
        let goal = (
            ((goal.0 - px + s / 2) / s).clamp(0, cw - 1),
            ((goal.1 - py + s / 2) / s).clamp(0, ch - 1),
        );
        path::find(cw, ch, start, goal, |x, y| {
            let (x, y) = tile((x, y));
            self.terrain_allows(u.layer, x, y)
                && match self.occupant(u.layer, x, y) {
                    None => true,
                    Some(o) => o == u.id || (!avoid_units && !self.unit(o).is_some_and(|b| b.building)),
                }
        })
        .into_iter()
        .map(tile)
        .collect()
    }

    /// Phase 4 for one unit: advance its animation script, then its order.
    fn advance(&mut self, i: usize) {
        let scripts = Arc::clone(&self.scripts);
        let u = &mut self.units[i];
        u.idle_wait = u.idle_wait.saturating_sub(1);
        let m = Modifiers {
            haste: u.haste,
            carrying: false,
        };
        let fx = u.script.update(scripts.bytes(), u.facing, m);
        u.pos.0 += fx.dx;
        u.pos.1 += fx.dy;
        if fx.neutral {
            u.owner = 15;
        }
        self.order(i, fx.step_done);
    }

    /// The move order's reaction to the script (a placeholder until the movement spec): while the move animation
    /// runs, each "step done" signal ends the step under way and claims the next tile; a unit standing still starts
    /// the move animation once its next tile is free.
    fn order(&mut self, i: usize, step_done: bool) {
        let moving = self.units[i].script.anim == anim::MOVE;
        if moving && !step_done {
            return;
        }
        if let Some(from) = self.units[i].from.take() {
            let unit = self.units[i].clone();
            self.units[i].pos = centre(unit.tile, unit.size);
            self.mark(&unit, from, None);
            self.mark(&unit, unit.tile, Some(unit.id));
        }
        let u = &self.units[i];
        let Order::Move { goal, path, wait } = &u.order else {
            if moving {
                self.start_anim(i, anim::STILL);
            }
            return;
        };
        let Some(&next) = path.first() else {
            self.go_idle(i);
            self.start_anim(i, anim::STILL);
            return;
        };
        let (goal, wait) = (*goal, *wait);
        let blocked = self.occupant(u.layer, next.0, next.1).is_some_and(|o| o != u.id)
            || !self.terrain_allows(u.layer, next.0, next.1);
        if blocked {
            if moving {
                self.start_anim(i, anim::STILL);
            }
            if wait >= self.rules.patience {
                let path = self.plan(&self.units[i], goal, true);
                self.units[i].order = Order::Move { goal, path, wait: 0 };
            } else if let Order::Move { wait, .. } = &mut self.units[i].order {
                *wait += 1;
            }
            return;
        }
        if !moving {
            // The move script opens with "step done", which claims the tile on the next cycle.
            self.start_anim(i, anim::MOVE);
            return;
        }
        let unit = self.units[i].clone();
        let u = &mut self.units[i];
        u.facing = facing(next.0 - u.tile.0, next.1 - u.tile.1);
        u.from = Some(u.tile);
        u.tile = next;
        if let Order::Move { path, wait, .. } = &mut u.order {
            path.remove(0);
            *wait = 0;
        }
        self.mark(&unit, next, Some(unit.id));
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
        h.finish()
    }
}

#[cfg(test)]
mod tests {
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
        }
    }

    fn stats() -> UnitTable {
        let mut t = vec![UnitStats::default(); 110];
        t[0] = UnitStats {
            hit_points: 60,
            size: (1, 1),
            flags: flags::LAND,
            ..Default::default()
        };
        t[2] = UnitStats {
            hit_points: 100,
            size: (1, 1),
            flags: flags::AIR,
            ..Default::default()
        };
        t[0x3a] = UnitStats {
            hit_points: 400,
            size: (2, 2),
            flags: flags::BUILDING,
            ..Default::default()
        };
        UnitTable(t)
    }

    /// Synthetic scripts: every type halts in place, and its move loop covers 32 px in 8 cycles; type 2 moves
    /// 64 px per loop.
    pub(crate) fn scripts() -> Arc<Scripts> {
        use tidewater_formats::script::{MISSILE_KINDS, UNIT_TYPES};
        let mut b = vec![0u8; 512];
        let put = |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
        for i in 0..UNIT_TYPES + MISSILE_KINDS {
            put(&mut b, i * 2, if i == 2 { 320 } else { 300 });
        }
        for (table, mv) in [(300, 350), (320, 400)] {
            for a in 0..7 {
                put(&mut b, table + a * 2, if a == 3 { mv } else { 340 });
            }
        }
        b[340..343].copy_from_slice(&[2, 3, 0]);
        put(&mut b, 342, 340);
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
    fn world() -> World {
        let tiles = (0..64)
            .map(|i| if i % 8 == 4 && i / 8 < 6 { 0x10 } else { 0x00 })
            .collect::<Vec<u16>>();
        let mut w = World::new(Terrain::from_map(&tileset(), &tiles, 8, 8), stats(), scripts(), 0);
        w.spawn(0, 0, (0, 0), 0);
        w
    }

    fn run(w: &mut World, ticks: u32) {
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
        assert_eq!(u.pos, (7 * 32 + 16, 16));
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
            w.spawn(0, 0, (x, 7), 0);
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
        let b = w.spawn(0x3a, 0, (6, 6), 0);
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
        // Start the move animation; one cycle later "step done" claims (1, 0); the loop then takes 8 cycles.
        let at = |w: &World| w.unit(UnitId(1)).unwrap().pos;
        run(&mut w, 1);
        assert_eq!(w.unit(UnitId(1)).unwrap().tile, (1, 0));
        assert_eq!(at(&w), (16, 16));
        run(&mut w, 8);
        assert_eq!(at(&w), (48, 16));
        assert_eq!(w.occupant(Layer::Land, 0, 0), None);
    }

    #[test]
    fn flyers_move_two_tiles_per_step() {
        let mut w = world();
        let f = w.spawn(2, 0, (0, 2), 0);
        w.step(&[Command::Move {
            player: 0,
            units: vec![f],
            to: (6, 2),
        }]);
        run(&mut w, 2);
        assert_eq!(w.unit(f).unwrap().tile, (2, 2));
        run(&mut w, 100);
        let u = w.unit(f).unwrap();
        assert_eq!((u.tile, u.pos), ((6, 2), (6 * 32 + 16, 2 * 32 + 16)));
    }

    #[test]
    fn buildings_block_paths() {
        let mut w = world();
        w.spawn(0x3a, 0, (1, 0), 0);
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
