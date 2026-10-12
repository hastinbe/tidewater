//! Gathering gold, lumber and oil. Spec: `docs/specs/economy/harvest.md`.
//!
//! Not yet here:
//! - The AI hand-off of computer players' units coming out (ai spec to come); their units carry on as anyone else's.
//! - The chop sound draw (no audio yet; it uses the setup-shuffle generator, so the game state doesn't depend on it).
//! - Oil platforms built on patches.
//!
//! Our own reading: a unit that arrives at its mine, platform or depot starts order 25 in the same decision, rather
//! than waiting for another step done.

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::economy::kind;
use crate::movement::{Goal, Walk, building_goal, chebyshev};
use crate::terrain::{mask, region};
use crate::world::{Gather, Layer, Order, Source, Unit, UnitId, World, facing};
use crate::{TILE_PX, placement, slots};

/// One load of gold, lumber or oil.
const LOAD: u32 = 100;
const GOLD_KEEP: u32 = 110;
const GOLD_CASTLE: u32 = 120;
const LUMBER_MILL_LOAD: u32 = 125;
const REFINERY_LOAD: u32 = 125;
const CHOP_LIMIT: u8 = 50;
const INSIDE_WAIT: u8 = 150;
const OUT_WAIT: u8 = 25;
const TREE_RINGS: i32 = 6;
const RADIUS: i32 = 1;
/// Trips per unit of a map's amount byte, and the trips a 0 byte stands for.
const TRIPS_PER_MAP_UNIT: u32 = 25;
const TRIPS_FOR_ZERO: u32 = 120 * TRIPS_PER_MAP_UNIT;

const LUMBER_MILL: u8 = 0x4c;
const TROLL_LUMBER_MILL: u8 = 0x4d;
const REFINERY: u8 = 0x54;
const ORC_REFINERY: u8 = 0x55;

/// Trips held by a mine or patch with the map's amount byte (the low byte of the PUD unit value).
pub fn trips(map_value: u16) -> u32 {
    match map_value & 0xff {
        0 => TRIPS_FOR_ZERO,
        v => v as u32 * TRIPS_PER_MAP_UNIT,
    }
}

/// The tile of `b`'s footprint nearest to `from`.
fn nearest_in(from: (i32, i32), b: &Unit) -> (i32, i32) {
    (
        from.0.clamp(b.tile.0, b.tile.0 + b.size.0 as i32 - 1),
        from.1.clamp(b.tile.1, b.tile.1 + b.size.1 as i32 - 1),
    )
}

fn zone(b: &Unit) -> (i32, i32, i32, i32) {
    (
        b.tile.0 - 1,
        b.tile.1 - 1,
        b.tile.0 + b.size.0 as i32,
        b.tile.1 + b.size.1 as i32,
    )
}

impl World {
    fn flags_of(&self, k: u8) -> u32 {
        self.stats_of(k).flags
    }

    fn is_tanker(&self, u: &Unit) -> bool {
        self.flags_of(u.kind) & flags::TANKER != 0
    }

    /// Whether `u` can be told to harvest at `tile`: a worker at a gold mine or a tree, or a tanker at its own side's
    /// oil platform. The client uses this to turn a right click into a harvest.
    pub fn can_harvest(&self, u: &Unit, tile: (i32, i32)) -> bool {
        let f = self.flags_of(u.kind);
        let at = self.occupant(Layer::Land, tile.0, tile.1).and_then(|t| self.unit(t));
        let af = at.map_or(0, |t| self.flags_of(t.kind));
        if f & flags::PEON != 0 {
            af & flags::MINE != 0 || self.terrain.region(tile.0, tile.1).is_some_and(region::is_forest)
        } else if f & flags::TANKER != 0 {
            af & flags::OIL_SOURCE != 0 && at.is_some_and(|t| t.owner == u.owner)
        } else {
            false
        }
    }

    /// Whether `u` carries goods that building `depot` takes, so a right click on it returns them.
    pub fn can_return_to(&self, u: &Unit, depot: UnitId) -> bool {
        let Some(d) = self.unit(depot).filter(|d| d.present() && d.owner == u.owner) else {
            return false;
        };
        let f = self.flags_of(d.kind);
        u.carrying
            && if self.is_tanker(u) {
                f & flags::OIL_STORAGE != 0
            } else {
                f & flags::GOLD_STORAGE != 0 || (f & flags::WOOD_STORAGE != 0 && u.gather == Gather::Lumber)
            }
    }

    /// Command 23 ("Starting to harvest").
    pub(crate) fn harvest_command(&mut self, i: usize, target: Option<UnitId>, tile: (i32, i32)) -> Option<Order> {
        let u = &self.units[i];
        let f = self.flags_of(u.kind);
        if f & (flags::PEON | flags::TANKER) == 0 {
            return None;
        }
        let target = target.or_else(|| self.occupant(Layer::Land, tile.0, tile.1));
        let t = target
            .and_then(|t| self.unit(t))
            .filter(|t| t.alive())
            .map(|t| (t.id, t.owner, self.flags_of(t.kind)));
        let carrying = u.carrying;
        let owner = u.owner;
        let order = if f & flags::PEON != 0 {
            let mine = t.filter(|&(_, _, tf)| tf & flags::MINE != 0 && !carrying);
            let u = &mut self.units[i];
            u.chop = 0;
            u.gather = if mine.is_some() { Gather::Gold } else { Gather::Lumber };
            Order::Harvest {
                source: mine.map(|(id, ..)| id),
                tree: tile,
                chopping: false,
            }
        } else {
            let platform = t.filter(|&(_, o, tf)| tf & flags::OIL_SOURCE != 0 && o == owner && !carrying);
            self.units[i].chop = 0;
            match platform {
                Some((id, ..)) => Order::Harvest {
                    source: Some(id),
                    tree: tile,
                    chopping: false,
                },
                None => Order::Idle,
            }
        };
        Some(order)
    }

    /// Command 24 ("Returning goods").
    pub(crate) fn return_command(&mut self, i: usize) -> Option<Order> {
        if self.flags_of(self.units[i].kind) & (flags::PEON | flags::TANKER) == 0 {
            return None;
        }
        Some(match self.nearest_depot(&self.units[i]) {
            Some(d) => Order::ReturnGoods { depot: Some(d) },
            None => Order::Idle,
        })
    }

    fn idle(&mut self, i: usize) {
        self.go_idle(i);
        self.switch_anim(i, anim::STILL);
    }

    /// Order 23.
    pub(crate) fn harvest_order(&mut self, i: usize, source: Option<UnitId>, tree: (i32, i32), chopping: bool) {
        match source {
            Some(s) => self.gather_order(i, s),
            None if self.units[i].gather == Gather::Lumber => self.lumber_order(i, tree, chopping),
            None => self.idle(i),
        }
    }

    /// Order 23 at a mine or platform ("Gold and oil: going to the source").
    fn gather_order(&mut self, i: usize, s: UnitId) {
        let Some(si) = self.index(s).filter(|&si| self.units[si].present()) else {
            return self.idle(i);
        };
        if !self.walk_to_building(i, si) {
            return;
        }
        if self.units[si].construction.is_some() {
            return self.idle(i);
        }
        self.units[i].order = Order::Enter {
            building: s,
            stepped: false,
        };
        self.enter_order(i, s, false);
    }

    /// Moves toward building `bi` (arrival radius 1). Returns whether the unit has arrived; a unit with no path goes
    /// idle.
    fn walk_to_building(&mut self, i: usize, bi: usize) -> bool {
        let b = &self.units[bi];
        let g = Goal {
            tile: building_goal(self.units[i].tile, b),
            radius: RADIUS,
            range: false,
            zone: Some(zone(b)),
        };
        match self.move_toward(i, g) {
            Walk::Arrived => true,
            Walk::NoPath => {
                self.idle(i);
                false
            }
            Walk::Stepping | Walk::Blocked => false,
        }
    }

    /// Order 24.
    pub(crate) fn return_order(&mut self, i: usize, depot: Option<UnitId>) {
        let found = depot.and_then(|d| self.index(d)).filter(|&di| self.units[di].present());
        let di = match found {
            Some(di) => di,
            None => match self.nearest_depot(&self.units[i]).and_then(|d| self.index(d)) {
                Some(di) => {
                    self.units[i].order = Order::ReturnGoods {
                        depot: Some(self.units[di].id),
                    };
                    di
                }
                None => return self.idle(i),
            },
        };
        if !self.walk_to_building(i, di) {
            return;
        }
        if self.units[di].construction.is_some() {
            return self.idle(i);
        }
        let building = self.units[di].id;
        self.units[i].order = Order::Enter {
            building,
            stepped: false,
        };
        self.enter_order(i, building, false);
    }

    /// Order 25 ("Entering a mine, platform or depot").
    pub(crate) fn enter_order(&mut self, i: usize, building: UnitId, stepped: bool) {
        let Some(bi) = self.index(building).filter(|&bi| self.units[bi].alive()) else {
            if stepped {
                self.kill(i);
            } else {
                self.idle(i);
            }
            return;
        };
        let tanker = self.is_tanker(&self.units[i]);
        if !stepped {
            let aim = nearest_in(self.units[i].tile, &self.units[bi]);
            let u = self.units[i].clone();
            self.mark(&u, u.tile, None);
            let u = &mut self.units[i];
            u.facing = facing(aim.0 - u.tile.0, aim.1 - u.tile.1);
            u.path.clear();
            u.order = Order::Enter {
                building,
                stepped: true,
            };
            if !tanker {
                // One move loop carries the worker a tile toward the building; the logic runs again when it ends.
                self.switch_anim(i, anim::MOVE);
                return;
            }
        }
        self.go_in(i, bi);
    }

    /// Order 25, step 2: the unit goes inside, then delivers or takes a load.
    fn go_in(&mut self, i: usize, bi: usize) {
        let b = self.units[bi].clone();
        let bf = self.flags_of(b.kind);
        let tanker = self.is_tanker(&self.units[i]);
        // Vision spec, "When sight is applied", rule 3.
        let (worker, building) = (self.units[i].id, b.id);
        self.worker_entry_sight(worker, building);
        let u = &mut self.units[i];
        u.inside = true;
        u.tile = b.tile;
        u.pos = (b.tile.0 * TILE_PX, b.tile.1 * TILE_PX);
        u.attacked_by = None;
        let (gather, owner) = (u.gather, u.owner);
        let source = bf & (flags::MINE | flags::OIL_SOURCE) != 0;
        let accepts = if tanker {
            bf & flags::OIL_STORAGE != 0 && !source
        } else {
            match gather {
                Gather::Gold => bf & flags::GOLD_STORAGE != 0,
                Gather::Lumber => bf & (flags::GOLD_STORAGE | flags::WOOD_STORAGE) != 0,
                Gather::None => false,
            }
        };
        let hall_sized = accepts && !tanker && bf & flags::GOLD_STORAGE != 0;
        self.units[i].order = Order::Inside {
            building: b.id,
            hall_sized,
        };
        self.start_anim(i, anim::STILL);
        self.units[i].script.wait = INSIDE_WAIT;
        if accepts {
            self.deliver(i, owner, tanker);
        } else if source {
            let m = &mut self.units[bi];
            m.occupants = m.occupants.saturating_add(1);
            if m.resources > 0 {
                m.resources -= 1;
                let last = m.resources == 0;
                self.units[i].carrying = true;
                if last {
                    self.units[i].return_to = None;
                    self.collapse(bi);
                } else {
                    self.units[i].return_to = Some(Source::Unit(b.id));
                }
            }
        }
    }

    /// "Delivering goods".
    pub(crate) fn deliver(&mut self, i: usize, owner: u8, tanker: bool) {
        let u = &self.units[i];
        if self.rules.enhanced && !u.carrying {
            return;
        }
        let (res, amount) = if tanker {
            let refinery = self.has_complete(owner as usize, &[REFINERY, ORC_REFINERY]);
            (2, if refinery { REFINERY_LOAD } else { LOAD })
        } else {
            match u.gather {
                Gather::Gold => {
                    let amount = if self.has_complete(owner as usize, &[kind::CASTLE, kind::FORTRESS]) {
                        GOLD_CASTLE
                    } else if self.has_complete(owner as usize, &[kind::KEEP, kind::STRONGHOLD]) {
                        GOLD_KEEP
                    } else {
                        LOAD
                    };
                    (0, amount)
                }
                Gather::Lumber => {
                    let mill = self.has_complete(owner as usize, &[LUMBER_MILL, TROLL_LUMBER_MILL]);
                    (1, if mill { LUMBER_MILL_LOAD } else { LOAD })
                }
                Gather::None => return,
            }
        };
        let p = &mut self.players[owner as usize];
        match res {
            0 => p.gold += amount,
            1 => p.lumber += amount,
            _ => p.oil += amount,
        }
        p.gathered[res] += amount;
        self.units[i].carrying = false;
    }

    /// The last trip was taken: the mine or platform is destroyed, and units inside come out at their next chance.
    fn collapse(&mut self, bi: usize) {
        let id = self.units[bi].id;
        for u in &mut self.units {
            if let Order::Inside { building, .. } = u.order
                && building == id
            {
                u.script.wait = u.script.wait.min(1);
            }
        }
        self.kill(bi);
    }

    /// Order 26 when the wait inside ends ("Coming out").
    pub(crate) fn come_out(&mut self, i: usize, building: UnitId, hall_sized: bool) {
        self.go_idle(i);
        if let Some(bi) = self.index(building) {
            let carrying_from_source = self.units[i].carrying
                && self.units[i].return_to == Some(Source::Unit(building))
                && self.flags_of(self.units[bi].kind) & (flags::MINE | flags::OIL_SOURCE) != 0;
            if carrying_from_source {
                let b = &mut self.units[bi];
                b.occupants = b.occupants.saturating_sub(1);
            }
        }
        // A computer worker chooses its job here; a job given skips the usual return (worker-ai spec).
        let chosen = if self.chooses_jobs(i) {
            match self.worker_choice(i) {
                crate::worker_ai::Choice::Order(o) => Some(o),
                _ => None,
            }
        } else {
            None
        };
        // Where the unit heads decides which side it comes out on.
        let next = if chosen.is_some() {
            chosen
        } else if self.units[i].carrying {
            self.nearest_depot(&self.units[i])
                .map(|d| Order::ReturnGoods { depot: Some(d) })
        } else {
            match self.units[i].return_to.take() {
                Some(Source::Unit(s)) => {
                    let tile = self.unit(s).map_or(self.units[i].tile, |s| s.tile);
                    self.harvest_command(i, Some(s), tile)
                }
                Some(Source::Tree(t)) => self.harvest_command(i, None, t),
                None => None,
            }
        };
        let aim = match next {
            Some(Order::ReturnGoods { depot: Some(d) }) => self.unit(d).map(|d| d.tile),
            Some(Order::Harvest { source: Some(s), .. }) => self.unit(s).map(|s| s.tile),
            Some(Order::Harvest { tree, .. }) => Some(tree),
            _ => None,
        };
        let at = self.units[i].tile;
        let side = if hall_sized { 4 } else { 3 };
        let src = placement::Source::new(at.0, at.1, side, side);
        let Some(tile) = self.find_place(self.units[i].kind, src, aim.unwrap_or(at), placement::LIMIT_OUT) else {
            self.units[i].inside = false;
            self.kill(i);
            return;
        };
        let u = &mut self.units[i];
        u.inside = false;
        u.tile = tile;
        u.pos = (tile.0 * TILE_PX, tile.1 * TILE_PX);
        u.order = next.unwrap_or(Order::Idle);
        let u = u.clone();
        self.mark(&u, tile, Some(u.id));
        self.start_anim(i, anim::STILL);
        self.units[i].script.wait = OUT_WAIT;
    }

    /// Order 23 in lumber mode ("Lumber").
    fn lumber_order(&mut self, i: usize, tree: (i32, i32), chopping: bool) {
        let here = self.units[i].tile;
        if chebyshev(here, tree) > RADIUS {
            if self.move_toward(i, Goal::tile(tree, RADIUS)) == Walk::NoPath {
                self.idle(i);
            }
            return;
        }
        let reserved = self.terrain.region(tree.0, tree.1) == Some(region::RESERVED_FOREST);
        if !chopping {
            if self.terrain.reserve(tree.0, tree.1) {
                let u = &mut self.units[i];
                u.chop = 0;
                u.facing = facing(tree.0 - here.0, tree.1 - here.1);
                u.return_to = Some(Source::Tree(tree));
                u.order = Order::Harvest {
                    source: None,
                    tree,
                    chopping: true,
                };
                u.path.clear();
                self.switch_anim(i, anim::ATTACK);
            } else {
                self.next_tree(i);
            }
            return;
        }
        if !reserved {
            self.terrain.unreserve(tree.0, tree.1);
            return self.next_tree(i);
        }
        let u = &mut self.units[i];
        if u.chop < CHOP_LIMIT {
            u.chop += if self.rules.build_speed_cheat {
                CHOP_LIMIT - u.chop
            } else {
                1
            };
            return;
        }
        let area = self.terrain.region(here.0, here.1).unwrap_or(region::LAND_AREA);
        self.terrain.fell(tree.0, tree.1, area);
        self.units[i].carrying = true;
        match self.nearest_depot(&self.units[i]) {
            Some(d) => {
                self.units[i].order = Order::ReturnGoods { depot: Some(d) };
                self.return_order(i, Some(d));
            }
            None => self.idle(i),
        }
    }

    /// Sends a worker to the nearest tree from its own tile, or makes it idle.
    fn next_tree(&mut self, i: usize) {
        match self.nearest_tree(self.units[i].tile) {
            Some(t) => {
                self.units[i].order = Order::Harvest {
                    source: None,
                    tree: t,
                    chopping: false,
                };
                self.units[i].path.clear();
                if self.move_toward(i, Goal::tile(t, RADIUS)) == Walk::NoPath {
                    self.idle(i);
                } else if self.units[i].script.anim == anim::ATTACK {
                    self.switch_anim(i, anim::STILL);
                }
            }
            None => self.idle(i),
        }
    }

    /// "Finding the nearest tree".
    pub(crate) fn nearest_tree(&self, from: (i32, i32)) -> Option<(i32, i32)> {
        let a = self.terrain.region(from.0, from.1)?;
        for r in 1..=TREE_RINGS {
            let mut reachable = false;
            for (x, y) in ring(from, r) {
                let Some(v) = self.terrain.region(x, y) else { continue };
                if v == a {
                    reachable = true;
                } else if v == region::FOREST && self.tree_has_room(x, y, a) {
                    return Some((x, y));
                }
            }
            if !reachable {
                return None;
            }
        }
        None
    }

    fn tree_has_room(&self, x: i32, y: i32, a: u16) -> bool {
        (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                self.terrain.region(x + dx, y + dy) == Some(a) && self.terrain.word(x + dx, y + dy) & mask::LAND == 0
            })
        })
    }

    /// Whether a depot takes what unit `id` carries (the command panel's return-goods test; harvest spec, "Finding
    /// the nearest depot").
    pub fn has_depot_for(&self, id: UnitId) -> bool {
        self.unit(id).is_some_and(|u| self.nearest_depot(u).is_some())
    }

    /// "Finding the nearest depot": the owner's list in order, a later tie winning.
    pub(crate) fn nearest_depot(&self, u: &Unit) -> Option<UnitId> {
        let tanker = self.is_tanker(u);
        let here = self.terrain.region(u.tile.0, u.tile.1);
        let mut best: Option<(i32, UnitId)> = None;
        for slot in self.slots.list(u.owner) {
            let Ok(bi) = self.units.binary_search_by_key(&slot, |b| b.slot) else {
                continue;
            };
            let b = &self.units[bi];
            if !b.building || !b.alive() || b.construction.is_some() || u.owner == slots::NEUTRAL {
                continue;
            }
            let f = self.flags_of(b.kind);
            let ok = if tanker {
                f & flags::OIL_STORAGE != 0 && here.is_some_and(|r| self.touches_region(b, r))
            } else {
                here.is_some() && self.terrain.region(b.tile.0, b.tile.1) == here && {
                    f & flags::GOLD_STORAGE != 0 || (f & flags::WOOD_STORAGE != 0 && u.gather == Gather::Lumber)
                }
            };
            if !ok {
                continue;
            }
            let d = chebyshev(u.tile, building_goal(u.tile, b));
            if best.is_none_or(|(bd, _)| d <= bd) {
                best = Some((d, b.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Whether any tile just outside `b`'s footprint has region value `r`.
    fn touches_region(&self, b: &Unit, r: u16) -> bool {
        let (x0, y0, x1, y1) = zone(b);
        (y0..=y1).any(|y| {
            (x0..=x1).any(|x| (x == x0 || x == x1 || y == y0 || y == y1) && self.terrain.region(x, y) == Some(r))
        })
    }
}

/// The tiles of the square ring of radius `r` around `c`, clockwise from the top-left corner.
fn ring(c: (i32, i32), r: i32) -> impl Iterator<Item = (i32, i32)> {
    let (x0, y0, x1, y1) = (c.0 - r, c.1 - r, c.0 + r, c.1 + r);
    let top = (x0..=x1).map(move |x| (x, y0));
    let right = (y0 + 1..=y1).map(move |y| (x1, y));
    let bottom = (x0..x1).rev().map(move |x| (x, y1));
    let left = (y0 + 1..y1).rev().map(move |y| (x0, y));
    top.chain(right).chain(bottom).chain(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use crate::economy::kind;
    use crate::terrain::bits;
    use crate::world::tests::field;

    const MINE: u8 = 0x5c;

    /// A 24 x 24 field with a worker type (0), a 3 x 3 gold mine type, and halls and lumber mills that store goods.
    fn land() -> World {
        let mut w = field(24, 24);
        w.stats.0[0].flags |= flags::PEON;
        w.stats.0[MINE as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 25500,
            size: (3, 3),
            flags: flags::BUILDING | flags::MINE,
            ..Default::default()
        };
        for k in [kind::TOWN_HALL, kind::KEEP] {
            w.stats.0[k as usize].flags |= flags::GOLD_STORAGE | flags::WOOD_STORAGE;
        }
        w.stats.0[kind::CASTLE as usize] = w.stats.0[kind::FARM as usize];
        w.stats.0[LUMBER_MILL as usize] = w.stats.0[kind::FARM as usize];
        w.stats.0[LUMBER_MILL as usize].flags |= flags::WOOD_STORAGE;
        w
    }

    fn harvest(w: &mut World, units: Vec<UnitId>, target: Option<UnitId>, tile: (i32, i32)) {
        w.step(&[Command::Harvest {
            player: 0,
            units,
            target,
            tile,
        }]);
    }

    fn forest(w: &mut World, x: i32, y: i32) {
        let n = w.terrain.at(x, y).unwrap();
        w.terrain.regions[n] = region::FOREST;
        w.terrain.words[n] |= bits::OBSTACLE;
    }

    #[test]
    fn map_amounts_become_trips() {
        assert_eq!(trips(0), 3000);
        assert_eq!(trips(4), 100);
        assert_eq!(trips(0x0104), 100);
    }

    #[test]
    fn gold_round_trip() {
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 2).unwrap();
        let hall = w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let peasant = w.spawn(0, 0, (8, 5), 0).unwrap();
        harvest(&mut w, vec![peasant], Some(mine), (14, 4));
        let entered = (0..400)
            .find(|_| {
                w.step(&[]);
                w.unit(peasant).unwrap().inside
            })
            .expect("goes into the mine");
        let p = w.unit(peasant).unwrap();
        assert!(entered > 0 && p.carrying);
        assert_eq!(p.gather, Gather::Gold);
        assert_eq!(p.tile, (14, 4));
        assert_eq!(w.unit(mine).unwrap().resources, 1);
        assert_eq!(w.unit(mine).unwrap().occupants, 1);
        assert_eq!(w.occupant(Layer::Land, 13, 5), None);
        // Inside for about 150 cycles, then out and back to the hall, which pays 100 gold.
        let out = (0..400)
            .find(|_| {
                w.step(&[]);
                !w.unit(peasant).unwrap().inside
            })
            .unwrap();
        assert!((149..170).contains(&out), "{out}");
        assert_eq!(w.unit(mine).unwrap().occupants, 0);
        assert_eq!(w.unit(peasant).unwrap().order, Order::ReturnGoods { depot: Some(hall) });
        // It comes out on the hall's side of the mine.
        assert_eq!(w.unit(peasant).unwrap().tile.0, 13);
        for _ in 0..600 {
            w.step(&[]);
            if w.players[0].gold > 0 {
                break;
            }
        }
        assert_eq!((w.players[0].gold, w.players[0].gathered[0]), (100, 100));
        assert!(!w.unit(peasant).unwrap().carrying);
        // The second trip empties the mine, which collapses.
        for _ in 0..1200 {
            w.step(&[]);
        }
        assert!(w.unit(mine).is_none_or(|m| m.dying));
        assert_eq!(w.players[0].gold, 200);
    }

    #[test]
    fn delivery_bonuses() {
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let p = w.spawn(0, 0, (3, 4), 0).unwrap();
        // Spawning shifts indices, so they are looked up each time.
        let load = |w: &mut World, g: Gather| {
            let (pi, hi) = (w.index(p).unwrap(), w.index(hall).unwrap());
            let before = (w.players[0].gold, w.players[0].lumber);
            let u = &mut w.units[pi];
            (u.gather, u.carrying, u.inside) = (g, true, false);
            w.go_in(pi, hi);
            (w.players[0].gold - before.0, w.players[0].lumber - before.1)
        };
        assert_eq!(load(&mut w, Gather::Gold), (100, 0));
        assert_eq!(load(&mut w, Gather::Lumber), (0, 100));
        w.spawn(kind::KEEP, 0, (10, 10), 0).unwrap();
        assert_eq!(load(&mut w, Gather::Gold), (110, 0));
        w.spawn(kind::CASTLE, 0, (14, 14), 0).unwrap();
        assert_eq!(load(&mut w, Gather::Gold), (120, 0));
        // A lumber mill under construction doesn't count.
        let mill = w.place_building(LUMBER_MILL, 0, (18, 18)).unwrap();
        assert_eq!(load(&mut w, Gather::Lumber), (0, 100));
        let mi = w.index(mill).unwrap();
        w.units[mi].construction = None;
        assert_eq!(load(&mut w, Gather::Lumber), (0, 125));
        // Enhanced rules: no load, no delivery.
        let (pi, hi) = (w.index(p).unwrap(), w.index(hall).unwrap());
        let u = &mut w.units[pi];
        (u.carrying, u.inside) = (false, false);
        w.go_in(pi, hi);
        assert_eq!(w.players[0].gold, 330);
    }

    #[test]
    fn nearest_depot_by_goods_and_ties() {
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (16, 10), 0).unwrap();
        let mill = w.spawn(LUMBER_MILL, 0, (11, 10), 0).unwrap();
        let p = w.spawn(0, 0, (8, 10), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].gather = Gather::Lumber;
        assert_eq!(w.nearest_depot(&w.units[pi]), Some(mill));
        w.units[pi].gather = Gather::Gold;
        assert_eq!(w.nearest_depot(&w.units[pi]), Some(hall));
        // Two halls the same distance away: the one created first wins.
        let mut w = land();
        let first = w.spawn(kind::TOWN_HALL, 0, (2, 10), 0).unwrap();
        w.spawn(kind::TOWN_HALL, 0, (15, 10), 0).unwrap();
        let p = w.spawn(0, 0, (9, 10), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].gather = Gather::Gold;
        assert_eq!(w.nearest_depot(&w.units[pi]), Some(first));
    }

    #[test]
    fn tree_search_rings_and_islands() {
        let mut w = land();
        forest(&mut w, 12, 10);
        forest(&mut w, 10, 12);
        assert_eq!(w.nearest_tree((10, 10)), Some((12, 10)));
        // An island: every tile of the first ring is water or forest with no free land beside it.
        let mut w = land();
        for (x, y) in ring((10, 10), 1) {
            let n = w.terrain.at(x, y).unwrap();
            w.terrain.regions[n] = 0;
        }
        forest(&mut w, 12, 10);
        assert_eq!(w.nearest_tree((10, 10)), None);
    }

    #[test]
    fn reserved_trees_send_the_worker_elsewhere() {
        let mut w = land();
        forest(&mut w, 10, 8);
        forest(&mut w, 12, 8);
        assert!(w.terrain.reserve(10, 8));
        let p = w.spawn(0, 0, (10, 9), 0).unwrap();
        harvest(&mut w, vec![p], None, (10, 8));
        for _ in 0..40 {
            w.step(&[]);
        }
        let u = w.unit(p).unwrap();
        assert!(matches!(u.order, Order::Harvest { tree: (12, 8), .. }), "{:?}", u.order);
    }

    #[test]
    fn chopping_counts_once_per_attack_loop() {
        let mut w = land();
        forest(&mut w, 10, 8);
        let p = w.spawn(0, 0, (10, 9), 0).unwrap();
        harvest(&mut w, vec![p], None, (10, 8));
        for _ in 0..30 {
            w.step(&[]);
        }
        let u = w.unit(p).unwrap();
        assert_eq!(
            u.order,
            Order::Harvest {
                source: None,
                tree: (10, 8),
                chopping: true
            }
        );
        assert_eq!(u.script.anim, anim::ATTACK);
        assert_eq!(w.terrain.region(10, 8), Some(region::RESERVED_FOREST));
        let c0 = u.chop;
        for _ in 0..250 {
            w.step(&[]);
        }
        assert_eq!(w.unit(p).unwrap().chop - c0, 10);
    }

    #[test]
    fn a_mine_destroyed_while_stepping_in_kills_the_worker() {
        let mut w = land();
        let mine = w.spawn(MINE, 15, (10, 4), 5).unwrap();
        let p = w.spawn(0, 0, (9, 5), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].gather = Gather::Gold;
        w.units[pi].order = Order::Enter {
            building: mine,
            stepped: false,
        };
        w.enter_order(pi, mine, false);
        assert_eq!(
            w.units[pi].order,
            Order::Enter {
                building: mine,
                stepped: true
            }
        );
        let mi = w.index(mine).unwrap();
        w.kill(mi);
        w.enter_order(pi, mine, true);
        assert!(w.units[pi].dying);
    }
}
