//! Gathering gold, lumber and oil. Spec: `docs/specs/economy/harvest.md`.
//!
//! Not yet here:
//! - The AI hand-off of computer players' units coming out (ai spec to come); their units carry on as anyone else's.
//!
//! A unit that arrives at its mine, platform or depot starts order 25 in the same decision, rather than waiting for
//! another step done (orders spec, "Same-update take-over", confirmed by the round-33 errata).

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::economy::kind;
use crate::movement::{Goal, Walk, building_goal, chebyshev};
use crate::terrain::{mask, region};
use crate::world::{Gather, Layer, Order, Source, Unit, UnitId, World};
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
/// The wait a take-over starts with (orders spec, "Same-update take-over").
const TAKEOVER_WAIT: u8 = 3;
const TREE_RINGS: i32 = 6;
const RADIUS: i32 = 1;
/// Trips per unit of a map's amount byte, and the trips a 0 byte stands for.
const TRIPS_PER_MAP_UNIT: u32 = 25;
const TRIPS_FOR_ZERO: u32 = 120 * TRIPS_PER_MAP_UNIT;

/// The gold mine, whose "active" look is frame 1 (any other source shows frame 2).
const MINE: u8 = 0x5c;
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

/// The tile a unit faces when it turns to a building: its top-left tile plus (1, 1), as `economy/repair.md` and
/// `movement/placement.md` state for facing a target building.
fn facing_point(b: &Unit) -> (i32, i32) {
    (b.tile.0 + 1, b.tile.1 + 1)
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
            // A lumber worker's tree tile is the command's goal after the pull-back ("Starting to harvest", round-38
            // errata), whoever gives the command; `human12-7000` peon 1572: tree (105, 0) becomes (104, 1).
            let tree = if mine.is_some() { tile } else { self.pull_back(i, tile) };
            Order::Harvest {
                source: mine.map(|(id, ..)| id),
                tree,
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
        if !self.walk_to_building(i, si, true) {
            return;
        }
        if self.units[si].construction.is_some() {
            return self.idle(i);
        }
        // Order 25 takes over at once, in the same update, without running its logic (orders spec, "Taking over",
        // and its default for "Same-update take-over"): its first run comes at the next step done.
        self.take_over(
            i,
            Order::Enter {
                building: s,
                stepped: false,
            },
        );
    }

    /// Moves toward building `bi` (arrival radius 1). Returns whether the unit has arrived. A unit with no path goes
    /// idle or, in order 23 (`harvesting`), looks for the nearest tree ("No route").
    fn walk_to_building(&mut self, i: usize, bi: usize, harvesting: bool) -> bool {
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
                if harvesting {
                    self.no_route(i);
                } else {
                    // Round-41 errata (`xorc3` peon 1462, n = 8895): a return with no route goes idle at once, and
                    // its idle logic (a computer player's job choice) runs in the same update.
                    self.idle_at_once(i);
                }
                false
            }
            Walk::Stepping | Walk::Blocked => false,
        }
    }

    /// "Lumber", "No route" (round-39 errata): a unit with the peon flag in order 23, in either gather mode, whose
    /// route search finds no route looks for the nearest tree from its own tile and is given a new harvest command to
    /// it, which takes over in the same update; with no tree (or without the flag) it goes idle. `human5-10000`
    /// worker 1567 at n = 119 and `orc8-10000` worker 1499 from n = 754. Round-40 errata: only the peon flag counts, so
    /// a tanker goes idle as in "No route" of the orders spec (an immediate idle whose idle logic runs at once).
    ///
    /// Round-44 errata (`orc4-40000` peasant 1584, n = 23680): after the re-aim to a tree, order 23's logic runs once
    /// more in the same update. A worker within 1 tile of the new tree starts chopping at once (step 2: it faces the
    /// tree, reserves it and makes its chop-sound draw), and the attack animation still restarts with wait 3. A worker
    /// further away is left as the re-aim made it (facing kept, attack animation, wait 3).
    fn no_route(&mut self, i: usize) {
        if self.flags_of(self.units[i].kind) & flags::PEON == 0 {
            return self.idle_at_once(i);
        }
        self.next_tree(i);
        let u = &self.units[i];
        if let Order::Harvest {
            source: None,
            tree,
            chopping: false,
        } = u.order
            && u.gather == Gather::Lumber
            && chebyshev(u.tile, tree) <= RADIUS
            && self.start_chopping(i, tree)
        {
            self.start_anim(i, anim::ATTACK);
            self.units[i].script.wait = TAKEOVER_WAIT;
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
        if !self.walk_to_building(i, di, false) {
            return;
        }
        if self.units[di].construction.is_some() {
            return self.idle(i);
        }
        let building = self.units[di].id;
        // As at a mine: order 25 takes over at once and first runs at the next step done.
        self.take_over(
            i,
            Order::Enter {
                building,
                stepped: false,
            },
        );
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
            let aim = facing_point(&self.units[bi]);
            let u = self.units[i].clone();
            self.mark(&u, u.tile, None);
            let u = &mut self.units[i];
            u.facing = crate::world::face_tile(u.tile, aim);
            u.path.clear();
            u.order = Order::Enter {
                building,
                stepped: true,
            };
            if !tanker {
                // One move loop carries the worker a tile toward the building; the logic runs again when it ends. As
                // for any step, the tile position holds the destination tile from the step's start (movement spec).
                let v = crate::path::DIRECTIONS[u.facing as usize & 7];
                u.tile = (u.tile.0 + v.0, u.tile.1 + v.1);
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
            // "Entering", step 2: the "active" look is the building's frame, 1 for a gold mine and 2 for an oil
            // platform, set only from frame 0.
            let m = &mut self.units[bi];
            m.occupants = m.occupants.saturating_add(1);
            if m.script.frame == 0 {
                m.script.frame = if m.kind == MINE { 1 } else { 2 };
            }
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
                // A count of 0 counts as 1; reaching 0 ends the "active" look.
                let b = &mut self.units[bi];
                b.occupants = b.occupants.max(1) - 1;
                if b.occupants == 0 {
                    b.script.frame = 0;
                }
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
            // Round-40 errata: a build order aims at the site's top-left tile, not the build walk's goal tile.
            Some(Order::Build { site, .. }) => Some(site),
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
        // Placement spec: the unit faces its target; a target building is faced at its top-left tile plus (1, 1).
        // Round-42 errata (`xorc3` peon 1485, n = 9180): a worker with a build job faces its build goal tile, found
        // by the building goal rule from its exit tile.
        let face = match next {
            Some(Order::Harvest { tree, source: None, .. }) => Some(tree),
            Some(Order::Build { kind, site, .. }) => Some(self.build_goal(i, kind, site)),
            _ => aim.map(|t| (t.0 + 1, t.1 + 1)),
        };
        let u = &mut self.units[i];
        if let Some(f) = face.filter(|&f| f != tile) {
            u.facing = crate::world::face_tile(tile, f);
        }
        // Steps 3 to 5: the order given is pending; the unit stays idle (order 2) until its next step done, when the
        // order takes over with a wait of 3 (Human04 trace: order 2 with the depot as target, then order 24 after the
        // 25-cycle wait).
        u.order = Order::Idle;
        u.pending = next;
        // The order comes from a command (harvest or return), so the unit counts as about to move (pathfinding spec).
        u.commanded = next.is_some();
        // Movement spec, "Blocked rules" (round-41 errata): the order given on coming out is a command.
        if next.is_some() {
            u.blocked = 0;
        }
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
                self.no_route(i);
            }
            return;
        }
        let reserved = self.terrain.region(tree.0, tree.1) == Some(region::RESERVED_FOREST);
        if !chopping {
            if self.start_chopping(i, tree) {
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
            u.chop += if self.rules.lumber_cheat {
                CHOP_LIMIT - u.chop
            } else {
                1
            };
            self.pend_chop(i);
            return;
        }
        let area = self.terrain.region(here.0, here.1).unwrap_or(region::LAND_AREA);
        self.terrain.fell(tree.0, tree.1, area);
        // The worker carries lumber and its frame is set to 0; the return (or idle, with no depot) takes over in the
        // same update (orders spec, "Same-update take-over"). Taking over looks up the nearest depot. Round-43 errata:
        // as a command, the return clears the blocked counter, so blocks met on the way to the tree don't carry over.
        let u = &mut self.units[i];
        u.carrying = true;
        u.script.frame = 0;
        u.blocked = 0;
        self.take_over(i, Order::ReturnGoods { depot: None });
    }

    /// "Lumber", step 2: a worker next to a tree tile that is still unreserved forest starts chopping (counter 0, faces
    /// the tree, reserves it, takes it as its return-to source). Returns false, changing nothing, if the tile can't be
    /// reserved. The caller starts the chop animation.
    fn start_chopping(&mut self, i: usize, tree: (i32, i32)) -> bool {
        if !self.terrain.reserve(tree.0, tree.1) {
            return false;
        }
        let u = &mut self.units[i];
        u.chop = 0;
        u.facing = crate::world::face_tile(u.tile, tree);
        u.return_to = Some(Source::Tree(tree));
        u.order = Order::Harvest {
            source: None,
            tree,
            chopping: true,
        };
        u.path.clear();
        // Sounds spec, "Workers chopping": each chop's sound is played by the chop animation.
        self.pend_chop(i);
        true
    }

    /// Sends a worker to the nearest tree from its own tile, or makes it idle. "Lumber", round-38 errata: the new aim
    /// is a new harvest command to that tile, which takes over in the same update (order 23, its attack animation,
    /// wait 3) without running its logic; `human12-7000` peon 1572 at n = 215 and `human5-10000` peasant 1512 at
    /// n = 39.
    fn next_tree(&mut self, i: usize) {
        if let Order::Harvest { chopping, .. } = &mut self.units[i].order {
            *chopping = false;
        }
        let Some(t) = self.nearest_tree(self.units[i].tile) else {
            return self.idle(i);
        };
        match self.harvest_command(i, None, t) {
            Some(o) => {
                // A new harvest command: the route dropped and the blocked counter cleared (movement spec).
                self.units[i].path.clear();
                self.units[i].blocked = 0;
                self.give(i, o);
                if let Some(o) = self.units[i].pending.take() {
                    self.units[i].commanded = false;
                    self.take_over(i, o);
                }
            }
            None => self.idle(i),
        }
    }

    /// "Finding the nearest tree".
    pub(crate) fn nearest_tree(&self, from: (i32, i32)) -> Option<(i32, i32)> {
        self.nearest_tree_within(from, TREE_RINGS)
    }

    /// The same search with rings of radius 1 to `rings` (the computer job choice's larger limit, worker-ai spec,
    /// "Harvest").
    pub(crate) fn nearest_tree_within(&self, from: (i32, i32), rings: i32) -> Option<(i32, i32)> {
        let a = self.terrain.region(from.0, from.1)?;
        for r in 1..=rings {
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
        assert_eq!(
            w.unit(peasant).unwrap().pending,
            Some(Order::ReturnGoods { depot: Some(hall) })
        );
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

    /// "Starting to harvest", round-38 errata: the tree tile is the command's goal after the pull-back, a forest tile
    /// nearer the worker or even a building tile (as `human12-7000` peon 1572's (104, 1), a farm's).
    #[test]
    fn the_tree_tile_is_the_pulled_back_goal() {
        let mut w = land();
        forest(&mut w, 11, 10);
        forest(&mut w, 12, 10);
        let p = w.spawn(0, 0, (2, 10), 0).unwrap();
        let i = w.index(p).unwrap();
        let tree = |o: Option<Order>| match o {
            Some(Order::Harvest { source: None, tree, .. }) => tree,
            o => panic!("{o:?}"),
        };
        assert_eq!(tree(w.harvest_command(i, None, (12, 10))), (11, 10));
        let mut w = land();
        forest(&mut w, 12, 10);
        w.spawn(kind::FARM, 0, (10, 9), 0).unwrap();
        let p = w.spawn(0, 0, (2, 10), 0).unwrap();
        let i = w.index(p).unwrap();
        assert_eq!(tree(w.harvest_command(i, None, (12, 10))), (10, 10), "a farm tile");
    }

    /// "Lumber", round-38 errata, and the `human12-7000` vector (n = 215): a worker by a tree tile that isn't forest
    /// is given a new harvest command to the nearest tree, which takes over in the same update: order 23, attack
    /// animation, wait 3, blocked counter 0, no route yet.
    #[test]
    fn a_new_aim_is_a_harvest_command_taking_over_at_once() {
        let mut w = land();
        forest(&mut w, 12, 7);
        let p = w.spawn(0, 0, (10, 9), 0).unwrap();
        let i = w.index(p).unwrap();
        w.units[i].gather = Gather::Lumber;
        w.units[i].blocked = 2;
        w.units[i].order = Order::Harvest {
            source: None,
            tree: (10, 8),
            chopping: false,
        };
        w.harvest_order(i, None, (10, 8), false);
        let u = &w.units[i];
        let want = Order::Harvest {
            source: None,
            tree: (12, 7),
            chopping: false,
        };
        assert_eq!((u.order, u.pending, u.commanded), (want, None, false));
        assert_eq!(
            (u.script.anim, u.script.wait, u.blocked, u.tile),
            (anim::ATTACK, 3, 0, (10, 9))
        );
        assert!(u.path.is_empty());
    }

    /// "Lumber", "No route" (round-39 errata), the `human5-10000` worker 1567 vector at n = 119: a worker in gold mode
    /// heading for a mine it has no route to looks for the nearest tree and is given a lumber harvest command to it,
    /// which takes over in the same update (attack animation, wait 3, no target). With no tree it goes idle. Round-44
    /// errata (`orc4-40000` peasant 1584, n = 23680): order 23's logic then runs once more, so a worker next to the new
    /// tree turns to it, reserves it and starts chopping, still with wait 3; one further away keeps its facing.
    #[test]
    fn a_worker_with_no_route_looks_for_the_nearest_tree() {
        for tree in [None, Some((11, 10)), Some((12, 10))] {
            let mut w = land();
            // (10, 10) walled round; a tree at (11, 10) is part of the wall.
            for (x, y) in ring((10, 10), 1) {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.words[n] |= bits::OBSTACLE;
            }
            if let Some(t) = tree {
                forest(&mut w, t.0, t.1);
            }
            let mine = w.spawn(MINE, 15, (18, 10), 2).unwrap();
            let p = w.spawn(0, 0, (10, 10), 0).unwrap();
            let i = w.index(p).unwrap();
            w.units[i].gather = Gather::Gold;
            w.units[i].facing = 4;
            w.units[i].order = Order::Harvest {
                source: Some(mine),
                tree: (18, 10),
                chopping: false,
            };
            w.harvest_order(i, Some(mine), (18, 10), false);
            let u = &w.units[i];
            let Some(t) = tree else {
                assert_eq!(u.order, Order::Idle);
                continue;
            };
            let adjacent = t == (11, 10);
            let want = Order::Harvest {
                source: None,
                tree: t,
                chopping: adjacent,
            };
            assert_eq!((u.order, u.pending, u.gather), (want, None, Gather::Lumber));
            assert_eq!(
                (u.script.anim, u.script.frame, u.script.wait, u.tile),
                (anim::ATTACK, 0, 3, (10, 10))
            );
            let reserved = w.terrain.region(t.0, t.1) == Some(region::RESERVED_FOREST);
            assert_eq!(
                (u.facing, reserved),
                if adjacent { (2, true) } else { (4, false) },
                "{t:?}"
            );
        }
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
    fn felling_a_tree_takes_over_the_return_in_the_same_update() {
        // Human04 n = 1913: when the tree falls, order 24 takes over at once (wait 3, the still animation) and its
        // logic first runs at the next step done (orders spec, open question "Same-update take-over", default).
        let mut w = land();
        forest(&mut w, 10, 8);
        let hall = w.spawn(kind::TOWN_HALL, 0, (2, 10), 0).unwrap();
        let p = w.spawn(0, 0, (10, 9), 0).unwrap();
        harvest(&mut w, vec![p], None, (10, 8));
        for _ in 0..30 {
            w.step(&[]);
        }
        let pi = w.index(p).unwrap();
        w.units[pi].chop = 50;
        // Round-43 errata: blocks met on the way to the tree don't carry over into the walk back.
        w.units[pi].blocked = 9;
        // Harvest and animation-scripts specs, round-33 vectors (peon 1531): frame 25 before the felling, 0 after.
        let felled = (0..100).find(|_| {
            let pi = w.index(p).unwrap();
            w.units[pi].script.frame = 25;
            w.step(&[]);
            w.unit(p).unwrap().carrying
        });
        assert!(felled.is_some());
        let u = w.unit(p).unwrap();
        assert_eq!(
            (u.order, u.pending, u.script.anim, u.script.frame, u.script.wait, u.tile),
            (
                Order::ReturnGoods { depot: Some(hall) },
                None,
                anim::STILL,
                0,
                3,
                (10, 9)
            )
        );
        assert_eq!(u.blocked, 0);
    }

    #[test]
    fn felling_with_no_depot_takes_over_idle_in_the_same_update() {
        let mut w = land();
        forest(&mut w, 10, 8);
        let p = w.spawn(0, 0, (10, 9), 0).unwrap();
        harvest(&mut w, vec![p], None, (10, 8));
        for _ in 0..30 {
            w.step(&[]);
        }
        let pi = w.index(p).unwrap();
        w.units[pi].chop = 50;
        assert!((0..100).any(|_| {
            w.step(&[]);
            w.unit(p).unwrap().carrying
        }));
        let u = w.unit(p).unwrap();
        assert_eq!(
            (u.order, u.pending, u.script.anim, u.script.frame, u.script.wait),
            (Order::Idle, None, anim::STILL, 0, 3)
        );
    }

    /// Puts worker `p` (gold, as from mine `mine`) into building `b` ("Entering", step 2).
    fn go_in_now(w: &mut World, p: UnitId, b: UnitId) {
        let (pi, bi) = (w.index(p).unwrap(), w.index(b).unwrap());
        w.units[pi].gather = Gather::Gold;
        w.go_in(pi, bi);
    }

    /// Lets worker `p` out of `b` ("Coming out").
    fn come_out_now(w: &mut World, p: UnitId, b: UnitId) {
        let pi = w.index(p).unwrap();
        w.come_out(pi, b, false);
    }

    /// The mine's occupant count and frame.
    fn mine_look(w: &World, m: UnitId) -> (u8, u8) {
        let m = w.unit(m).unwrap();
        (m.occupants, m.script.frame)
    }

    #[test]
    fn a_gold_mine_shows_frame_1_while_workers_are_inside() {
        // Harvest spec, round-33 vectors: Human04 mine 1559, n = 1112 to 1914, with peons 1530 (a), 1548 (b) and 1529
        // (c).
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let [a, b, c] = [(12, 4), (12, 6), (12, 8)].map(|t| w.spawn(0, 0, t, 0).unwrap());
        assert_eq!(mine_look(&w, mine), (0, 0));
        let steps: [(bool, UnitId, (u8, u8)); 10] = [
            (true, a, (1, 1)),
            (true, b, (2, 1)),
            (false, a, (1, 1)),
            (false, b, (0, 0)),
            (true, c, (1, 1)),
            (true, a, (2, 1)),
            (false, c, (1, 1)),
            (true, b, (2, 1)),
            (false, a, (1, 1)),
            (false, b, (0, 0)),
        ];
        for (n, (going_in, p, look)) in steps.into_iter().enumerate() {
            if going_in {
                go_in_now(&mut w, p, mine);
            } else {
                come_out_now(&mut w, p, mine);
            }
            assert_eq!(mine_look(&w, mine), look, "step {n}");
        }
    }

    #[test]
    fn a_count_of_0_counts_as_1_when_coming_out() {
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let p = w.spawn(0, 0, (12, 4), 0).unwrap();
        go_in_now(&mut w, p, mine);
        let mi = w.index(mine).unwrap();
        w.units[mi].occupants = 0;
        come_out_now(&mut w, p, mine);
        assert_eq!(mine_look(&w, mine), (0, 0));
    }

    #[test]
    fn a_frame_already_set_is_kept_on_going_in() {
        // "If its frame is 0 ... Otherwise the frame doesn't change."
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let p = w.spawn(0, 0, (12, 4), 0).unwrap();
        let mi = w.index(mine).unwrap();
        w.units[mi].script.frame = 3;
        go_in_now(&mut w, p, mine);
        assert_eq!(mine_look(&w, mine), (1, 3));
    }

    #[test]
    fn a_depot_never_changes_frame() {
        // Harvest spec, round-33 vectors: Human04 great hall 1550, peons going in carrying gold.
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let p = w.spawn(0, 0, (7, 4), 0).unwrap();
        for _ in 0..3 {
            let pi = w.index(p).unwrap();
            w.units[pi].carrying = true;
            go_in_now(&mut w, p, hall);
            assert_eq!(mine_look(&w, hall), (0, 0));
            come_out_now(&mut w, p, hall);
            assert_eq!(mine_look(&w, hall), (0, 0));
        }
    }

    #[test]
    fn an_oil_platform_shows_frame_2_while_a_tanker_is_inside() {
        // Harvest spec, round-33 vectors: orc4 platform 1596 (type 0x56), frame 2 for one tanker visit of 150 cycles.
        const PLATFORM: u8 = 0x56;
        const TANKER: u8 = 9;
        let mut w = land();
        w.stats.0[PLATFORM as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 5000,
            size: (3, 3),
            flags: flags::BUILDING | flags::OIL_SOURCE,
            ..Default::default()
        };
        w.stats.0[TANKER as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 90,
            size: (1, 1),
            flags: flags::LAND | flags::TANKER,
            ..Default::default()
        };
        let platform = w.spawn(PLATFORM, 0, (14, 4), 100).unwrap();
        let t = w.spawn(TANKER, 0, (12, 4), 0).unwrap();
        let (ti, pi) = (w.index(t).unwrap(), w.index(platform).unwrap());
        w.go_in(ti, pi);
        assert!(w.unit(t).unwrap().carrying);
        assert_eq!(mine_look(&w, platform), (1, 2));
        let ti = w.index(t).unwrap();
        w.come_out(ti, platform, false);
        assert_eq!(mine_look(&w, platform), (0, 0));
    }

    #[test]
    fn entering_aims_at_the_top_left_tile_plus_1_1() {
        // Harvest spec, round-33 vectors, moved: peon 1531 at (76, 78) going into the lumber mill at (77, 77) faces
        // east (2), toward (78, 78), and steps to (77, 78); peon 1548 at (87, 80) going into the mine at (88, 77) faces
        // 1, toward (89, 78), and steps to (88, 79).
        for (b, at, facing, to) in [(LUMBER_MILL, (11, 10), 2, (11, 11)), (MINE, (14, 7), 1, (14, 9))] {
            let mut w = land();
            let building = w.spawn(b, 0, at, 100).unwrap();
            let start = if b == MINE { (13, 10) } else { (10, 11) };
            let p = w.spawn(0, 0, start, 0).unwrap();
            let pi = w.index(p).unwrap();
            w.enter_order(pi, building, false);
            let u = &w.units[pi];
            assert_eq!((u.facing, u.tile), (facing, to), "{b:#x}");
        }
    }

    #[test]
    fn the_order_given_on_coming_out_waits_for_the_25_cycle_wait() {
        // Harvest spec, "Coming out" (Human04 peon 1548): order 2 with the return pending through the 25-cycle wait,
        // then order 24 takes over with a wait of 3. The same holds for a person's units.
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let hall = w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let p = w.spawn(0, 0, (12, 4), 0).unwrap();
        go_in_now(&mut w, p, mine);
        come_out_now(&mut w, p, mine);
        let u = w.unit(p).unwrap();
        let ret = Order::ReturnGoods { depot: Some(hall) };
        assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, Some(ret), 25));
        let mut took_over = None;
        for n in 1..=40 {
            w.step(&[]);
            let u = w.unit(p).unwrap();
            if u.order == ret {
                took_over = Some((n, u.script.anim, u.script.wait));
                break;
            }
        }
        let (n, a, wait) = took_over.expect("the return takes over");
        assert!(n >= 25, "{n}");
        assert_eq!((a, wait), (anim::STILL, 3));
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
