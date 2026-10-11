use tidewater_formats::pud::Pud;
use tidewater_formats::tileset::Tileset;
use tidewater_formats::udta::{UnitStats, UnitTable, flags};

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
    /// Simulation ticks per second at normal speed.
    pub ticks_per_second: u32,
    /// Map pixels a moving unit covers per tick, by unit type.
    pub speed: Vec<u8>,
    /// Ticks a unit waits for a blocked tile before searching a new path.
    pub patience: u16,
}

impl Rules {
    pub fn placeholder(stats: &UnitTable) -> Self {
        let speed = stats
            .0
            .iter()
            .map(|s| {
                if s.flags & flags::BUILDING != 0 {
                    0
                } else if s.movement_class != 0 {
                    3
                } else {
                    2
                }
            })
            .collect();
        Self {
            ticks_per_second: 30,
            speed,
            patience: 10,
        }
    }
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
    /// Builds the starting state of a map. Start locations become [`Player::start`], not units.
    pub fn from_pud(pud: &Pud, tileset: &Tileset, stats: UnitTable) -> Result<Self, WorldError> {
        let want = pud.width as usize * pud.height as usize;
        if pud.tiles.len() != want {
            return Err(WorldError::BadMap {
                got: pud.tiles.len(),
                want,
            });
        }
        let terrain = Terrain::from_map(tileset, &pud.tiles, pud.width, pud.height);
        let rules = Rules::placeholder(&stats);
        let mut players = [Player::default(); PLAYERS];
        for (i, p) in players.iter_mut().enumerate() {
            let (g, l, o) = pud.start_resources(i);
            *p = Player {
                gold: g as u32,
                lumber: l as u32,
                oil: o as u32,
                race: pud.sides().get(i).copied().unwrap_or(2),
                start: None,
            };
        }
        let mut w = World {
            tick: 0,
            occupancy: std::array::from_fn(|_| vec![None; want]),
            terrain,
            stats,
            rules,
            players,
            units: Vec::new(),
            next_id: 1,
        };
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

    /// Places a unit with full hit points. Returns its id.
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
        let unit = Unit {
            id,
            kind,
            owner,
            tile,
            from: None,
            pos: centre(tile, size),
            facing: 4,
            hp: s.hit_points,
            layer,
            size,
            building,
            resources,
            order: Order::Idle,
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

    /// Advances one tick: applies `commands` in order, then moves every unit in id order.
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
            if u.owner != player || u.building {
                continue;
            }
            let order = match c {
                Command::Move { to, .. } => {
                    let path = self.plan(u, *to, false);
                    Order::Move {
                        goal: *to,
                        path,
                        wait: 0,
                    }
                }
                Command::Stop { .. } => Order::Idle,
            };
            self.units[i].order = order;
        }
    }

    /// A path for `u` to `goal`. With `avoid_units`, tiles held by other units count as blocked.
    fn plan(&self, u: &Unit, goal: (i32, i32), avoid_units: bool) -> Vec<(i32, i32)> {
        let (w, h) = (self.terrain.width as i32, self.terrain.height as i32);
        path::find(w, h, u.tile, goal, |x, y| {
            self.terrain_allows(u.layer, x, y)
                && match self.occupant(u.layer, x, y) {
                    None => true,
                    Some(o) => o == u.id || (!avoid_units && !self.unit(o).is_some_and(|b| b.building)),
                }
        })
    }

    fn advance(&mut self, i: usize) {
        let speed = self.rules.speed.get(self.units[i].kind as usize).copied().unwrap_or(0) as i32;
        let u = &self.units[i];
        if let Some(from) = u.from {
            // Walk towards the destination tile's centre.
            let target = centre(u.tile, u.size);
            let (dx, dy) = (target.0 - u.pos.0, target.1 - u.pos.1);
            let step = |d: i32| d.signum() * d.abs().min(speed.max(1));
            let pos = (u.pos.0 + step(dx), u.pos.1 + step(dy));
            let arrived = pos == target;
            let unit = self.units[i].clone();
            self.units[i].pos = pos;
            if arrived {
                self.units[i].from = None;
                self.mark(&unit, from, None);
                self.mark(&unit, unit.tile, Some(unit.id));
            }
            return;
        }
        let Order::Move { goal, path, wait } = &u.order else {
            return;
        };
        let Some(&next) = path.first() else {
            self.units[i].order = Order::Idle;
            return;
        };
        let (goal, wait) = (*goal, *wait);
        let blocked = self.occupant(u.layer, next.0, next.1).is_some_and(|o| o != u.id)
            || !self.terrain_allows(u.layer, next.0, next.1);
        if blocked {
            if wait >= self.rules.patience {
                let path = self.plan(u, goal, true);
                self.units[i].order = Order::Move { goal, path, wait: 0 };
            } else if let Order::Move { wait, .. } = &mut self.units[i].order {
                *wait += 1;
            }
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
        t[0x3a] = UnitStats {
            hit_points: 400,
            size: (2, 2),
            flags: flags::BUILDING,
            ..Default::default()
        };
        UnitTable(t)
    }

    /// An 8x8 ground map with a water column at x = 4 for y < 6.
    fn world() -> World {
        let tiles = (0..64)
            .map(|i| if i % 8 == 4 && i / 8 < 6 { 0x10 } else { 0x00 })
            .collect::<Vec<u16>>();
        let mut w = World {
            tick: 0,
            terrain: Terrain::from_map(&tileset(), &tiles, 8, 8),
            rules: Rules::placeholder(&stats()),
            stats: stats(),
            players: [Player::default(); PLAYERS],
            units: Vec::new(),
            next_id: 1,
            occupancy: std::array::from_fn(|_| vec![None; 64]),
        };
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
