//! Gathering gold, lumber and oil. Spec: `docs/specs/economy/harvest.md`.
//!
//! The code follows the spec's model ("The model"): four orders, each with its own function, and the shared rules,
//! each stated once.
//!
//! | Spec | Here |
//! | --- | --- |
//! | Order 23, "Going to a mine or platform" (tanker and gold worker) | [`World::source_order`] |
//! | Order 23, "Lumber" | [`World::lumber_order`] |
//! | Order 24, "Returning goods" | [`World::return_order`], [`World::return_in_reach`], [`World::return_target_lost`] |
//! | Order 25, "Entering a mine, platform or depot" | [`World::enter_order`], [`World::go_in`] |
//! | Order 26, "Coming out" | [`World::come_out`] |
//! | "Commands given by these orders" | [`World::harvest_command`], [`World::return_command`], [`World::command_at_once`] |
//! | "Walking orders" | the walk at the head of each order 23 and 24 function |
//! | "No route" | [`World::no_route`] |
//! | "Facing a target" | [`facing_tile`] |
//!
//! Where the code keeps an outcome that the spec words differently, or that a spec open question disputes, the
//! comment there says so and a test pins it; those are questions for the spec, not rules.
//!
//! Not yet here:
//! - The AI hand-off of computer players' units coming out (ai spec to come); their units carry on as anyone else's.
//!
//! A unit that arrives at its mine, platform or depot starts order 25 in the same decision, rather than waiting for
//! another step done (orders spec, "Same-update take-over").

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

/// "Facing a target": the tile a unit turns to when it steps in, starts to chop or comes out. A building target at
/// its top-left tile + (1, 1), any other target unit at its own tile, and with no target the unit's goal tile.
fn facing_tile(target: Option<&Unit>, goal: Option<(i32, i32)>) -> Option<(i32, i32)> {
    target.map(crate::world::facing_tile_of).or(goal)
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

    /// Command 23 ("Starting to harvest"), whoever gives it.
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
            // Step 2: a lumber worker's tree tile is the command's goal after the pull-back.
            let tree = if mine.is_some() { tile } else { self.pull_back(i, tile) };
            Order::Harvest {
                source: mine.map(|(id, ..)| id),
                tree,
                chopping: false,
            }
        } else {
            // Step 3: a platform placed in this cycle doesn't count while the oil patch under it is still to have its
            // own update this cycle; the tanker goes idle (V55, V56).
            let platform = t.filter(|&(_, o, tf)| tf & flags::OIL_SOURCE != 0 && o == owner && !carrying);
            self.units[i].chop = 0;
            match platform {
                Some((id, ..)) if self.platform_fresh(id) => Order::Idle,
                Some((id, ..)) => Order::Harvest {
                    source: Some(id),
                    tree: tile,
                    chopping: false,
                },
                // Step 3: otherwise the target is cleared and the order is still 23, aimed at the command's goal,
                // pulled back as any command's goal is ("A tanker with no target").
                None => Order::Harvest {
                    source: None,
                    tree: self.pull_back(i, tile),
                    chopping: false,
                },
            }
        };
        Some(order)
    }

    /// Whether `platform` was placed in this cycle and the oil patch under it hasn't had its own update yet: its slot
    /// is above the one being updated (harvest spec, "Starting to harvest", step 3).
    fn platform_fresh(&self, platform: UnitId) -> bool {
        self.fresh_platforms
            .iter()
            .any(|&(p, patch)| p == platform && self.updating.is_some_and(|s| patch > s))
    }

    /// Command 24 ("Returning goods", "The command") with no target unit: the nearest depot, or idle with none. Taken
    /// by a worker or tanker that is carrying; one with a target unit takes it as given (`select`'s command 24).
    pub(crate) fn return_command(&mut self, i: usize) -> Option<Order> {
        let u = &self.units[i];
        if self.flags_of(u.kind) & (flags::PEON | flags::TANKER) == 0 || !u.carrying {
            return None;
        }
        Some(match self.nearest_depot(&self.units[i]) {
            Some(d) => Order::ReturnGoods { depot: Some(d) },
            None => Order::Idle,
        })
    }

    /// Idle by an animation switch rather than a take-over: the outcome the code keeps for order 24 at a depot under
    /// construction and for order 23's unreachable no-target, non-lumber worker (both pinned).
    fn idle(&mut self, i: usize) {
        self.go_idle(i);
        self.switch_anim(i, anim::STILL);
    }

    /// Order 23 ("Walking orders": its logic runs only once the walk has ended, reaction first).
    ///
    /// A tanker and a gold-mode worker follow "Going to a mine or platform"; a lumber-mode worker "Lumber". A worker
    /// with no target unit that isn't in lumber mode is a state no command makes ("Starting to harvest" gives gold
    /// mode only with a mine): the code keeps its old outcome, the reaction and then idle by an animation switch.
    pub(crate) fn harvest_order(&mut self, i: usize, source: Option<UnitId>, tree: (i32, i32), chopping: bool) {
        if source.is_some() || self.is_tanker(&self.units[i]) {
            return self.source_order(i, source, tree);
        }
        if self.units[i].gather == Gather::Lumber {
            return self.lumber_order(i, tree, chopping);
        }
        if !self.react_at_walk_end(i) {
            self.idle(i);
        }
    }

    /// "Going to a mine or platform": order 23 for a tanker, and for a worker in gold mode, toward its target unit, or
    /// toward the command's goal tile for a tanker whose command's target was cleared ("A tanker with no target",
    /// V84, V88). The walk runs until it ends; then, after the reaction: no target, or one dying or gone, makes idle
    /// pending, taken over in the same update (step 1); at a completed mine or platform order 25 takes over (step 3);
    /// at one still under construction idle is made pending as in step 1 (step 4, V52). Step 2 (the logic run out of
    /// reach) is never reached: the walk ends only in reach, with no route, or with the target lost.
    ///
    /// A target found dying or gone ends the walk before it starts (step 1). A walk with no route takes "No route";
    /// the code runs the reaction first when there is a target, and not for a tanker with no target (V87).
    fn source_order(&mut self, i: usize, source: Option<UnitId>, goal: (i32, i32)) {
        let target = match source {
            Some(s) => match self.index(s).filter(|&si| self.units[si].present()) {
                Some(si) => Some(si),
                None => {
                    if !self.react_at_walk_end(i) {
                        self.idle_now(i);
                    }
                    return;
                }
            },
            None => None,
        };
        let g = match target {
            Some(si) => self.building_walk(i, si),
            None => Goal::tile(goal, RADIUS),
        };
        match self.move_toward(i, g) {
            Walk::Arrived => {}
            Walk::NoPath => {
                if target.is_none() || !self.react_at_walk_end(i) {
                    self.no_route(i);
                }
                return;
            }
            Walk::Stepping | Walk::Blocked => return,
        }
        if self.react_at_walk_end(i) {
            return;
        }
        match target {
            Some(si) if self.units[si].construction.is_none() => {
                // Order 25 takes over at once, without running its logic (orders spec, "Taking over"): its first
                // run comes at the next step done.
                let building = self.units[si].id;
                self.take_over(
                    i,
                    Order::Enter {
                        building,
                        stepped: false,
                    },
                );
            }
            _ => self.idle_now(i),
        }
    }

    /// The walk toward building (or unit) `bi`: the building goal rule from the unit's tile, arrival radius 1, the
    /// target's widened footprint as the goal zone.
    fn building_walk(&self, i: usize, bi: usize) -> Goal {
        let b = &self.units[bi];
        Goal {
            tile: building_goal(self.units[i].tile, b),
            radius: RADIUS,
            range: false,
            zone: Some(zone(b)),
        }
    }

    /// "No route" (under "Lumber"): a walk in any harvest order that finds no route takes the generic no-route idle
    /// (`orders/orders.md`: idle as an immediate order, wait 1, and the idle logic in the same update; for a computer
    /// player's unit the job choice). The one exception is the tree search: a unit with the peon flag (only that flag
    /// counts, so not a tanker) in order 23, in either gather mode, is given a new harvest command to the nearest tree
    /// from its own tile, and order 23's logic then runs in the same update ([`World::lumber_after_no_route`]). With no
    /// tree it takes the generic idle. The old order's own logic doesn't run, so neither does its reaction (V83, V87).
    fn no_route(&mut self, i: usize) {
        let peon = self.flags_of(self.units[i].kind) & flags::PEON != 0;
        let tree_search = peon && matches!(self.units[i].order, Order::Harvest { .. });
        if !tree_search || !self.aim_at_nearest_tree(i) {
            return self.idle_at_once(i);
        }
        self.lumber_after_no_route(i);
    }

    /// "No route", a tree found: the run of order 23's logic in the same update, which is "Lumber" again. Within 1
    /// tile of the tree, step 2 starts chopping at once, and the attack animation still restarts with wait 3 (V46).
    /// Further away, step 1 gives the harvest command again, to the goal the first command set, so the goal is pulled
    /// back a second time (V59, V47, V48).
    fn lumber_after_no_route(&mut self, i: usize) {
        let u = &self.units[i];
        let Order::Harvest {
            source: None,
            tree,
            chopping: false,
        } = u.order
        else {
            return;
        };
        if chebyshev(u.tile, tree) > RADIUS {
            return self.harvest_again(i, tree);
        }
        if self.start_chopping(i, tree) {
            self.start_anim(i, anim::ATTACK);
            self.units[i].script.wait = TAKEOVER_WAIT;
        }
    }

    /// Order 24 ("Returning goods"), a walking order: only the walk runs until it ends. It ends in reach of the
    /// depot, when the walk drops the target at a step boundary (dying, gone, inside something or undetected), or with
    /// no route ("No route"). Order 24's own test is narrower: only a target dying or gone counts as lost, and the
    /// step-boundary test isn't made on arrival, so a target inside something then is entered and order 25's target
    /// check drops it (V82). The three target tests stay separate.
    pub(crate) fn return_order(&mut self, i: usize, depot: Option<UnitId>) {
        let owner = self.units[i].owner;
        let alive = depot.and_then(|d| self.index(d)).filter(|&di| self.units[di].alive());
        let Some(di) = alive else {
            return self.return_target_lost(i);
        };
        let g = self.building_walk(i, di);
        if self.at_goal(i, g) {
            return self.return_in_reach(i, di);
        }
        if self.target_lost(owner, self.units[di].id) {
            return self.return_target_lost(i);
        }
        if self.move_toward(i, g) == Walk::NoPath {
            self.no_route(i);
        }
    }

    /// "Returning goods", step 3, after the reaction (step 1): at a completed depot order 25 takes over at once and
    /// first runs at the next step done; at one still under construction the unit goes idle. Kept as it is: that idle
    /// is set by an animation switch, not a take-over (pinned; "otherwise 2" in the spec).
    fn return_in_reach(&mut self, i: usize, di: usize) {
        self.units[i].path.clear();
        if self.react_at_walk_end(i) {
            return;
        }
        if self.units[di].construction.is_some() {
            return self.idle(i);
        }
        let building = self.units[di].id;
        self.take_over(
            i,
            Order::Enter {
                building,
                stepped: false,
            },
        );
    }

    /// "Returning goods", step 2, after the reaction (step 1): the target is lost, so the unit is sent to the nearest
    /// depot, taken over in the same update (wait 3, route dropped), or with none idle (V68, V77, V81).
    ///
    /// Kept as it is (pinned): this re-aim isn't made as an ordinary command, though "Commands given by these orders"
    /// says it is. The blocked counter is kept, a stale attacker isn't forgotten, and the idle with no depot may
    /// resume a remembered order. The reaction runs before the route is dropped.
    fn return_target_lost(&mut self, i: usize) {
        if self.react_at_walk_end(i) {
            return;
        }
        self.units[i].path.clear();
        match self.nearest_depot(&self.units[i]) {
            Some(d) => self.take_over(i, Order::ReturnGoods { depot: Some(d) }),
            None => self.idle_now(i),
        }
    }

    /// Order 25 ("Entering a mine, platform or depot"). Step 3, the target check, comes first: a target dying or gone,
    /// inside something or no longer detected is dropped ([`World::enter_target_lost`]). Then step 1 (face, step in)
    /// and step 2 ([`World::go_in`]).
    pub(crate) fn enter_order(&mut self, i: usize, building: UnitId, stepped: bool) {
        let owner = self.units[i].owner;
        let held = self
            .index(building)
            .filter(|&bi| self.units[bi].present() && Self::detected(&self.units[bi], owner));
        let Some(bi) = held else {
            return self.enter_target_lost(i, stepped);
        };
        let tanker = self.is_tanker(&self.units[i]);
        if !stepped {
            // Step 1: the unit faces its target ("Facing a target"; a unit that isn't a building, which command 24 can
            // name, at its own tile: V74).
            let aim = crate::world::facing_tile_of(&self.units[bi]);
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

    /// "Entering", step 3, with no target: a unit that already stepped in dies, its target showing as none (V69).
    /// One that hasn't has idle made pending, taken over in the same update (V82 as written). Kept apart: the open
    /// question "Order 25 losing its target before stepping in" finds an immediate idle here instead.
    fn enter_target_lost(&mut self, i: usize, stepped: bool) {
        if stepped {
            self.units[i].order = Order::Idle;
            self.kill(i);
        } else {
            self.idle_now(i);
        }
    }

    /// "Entering", step 2: the unit goes inside, then delivers ("Delivering goods") or takes a load.
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
        // Delivery branch: a worker at a building that stores gold or wood, or a tanker at any building but an oil
        // platform, even when the building doesn't accept what it carries; the delivery table then adds nothing and
        // the load is lost (V66). Source branch: anything else, a tanker at an oil platform (V67) and a worker at a
        // building that stores neither gold nor wood included; that worker keeps its load and its return-to source,
        // and the building's frame becomes 2 (V70, V72).
        let delivers = if tanker {
            bf & flags::OIL_SOURCE == 0
        } else {
            bf & (flags::GOLD_STORAGE | flags::WOOD_STORAGE) != 0
        };
        let accepts = tanker
            || match gather {
                Gather::Gold => bf & flags::GOLD_STORAGE != 0,
                Gather::Lumber => bf & (flags::GOLD_STORAGE | flags::WOOD_STORAGE) != 0,
                Gather::None => false,
            };
        // "Delivering goods": every delivery at a building that accepts gold sets the hall-sized exit mark, whatever
        // the gather mode and whether or not the unit carries anything, gather mode none included (V54); a ship's
        // delivery never sets it.
        if !source && !tanker && bf & flags::GOLD_STORAGE != 0 {
            self.units[i].hall_exit = true;
        }
        self.units[i].order = Order::Inside { building: Some(b.id) };
        self.start_anim(i, anim::STILL);
        self.units[i].script.wait = INSIDE_WAIT;
        if delivers {
            if accepts {
                self.deliver(i, owner, tanker);
            } else {
                // "Delivering goods": gold mode at a building that stores wood only, and anything else, adds nothing.
                self.units[i].carrying = false;
            }
        } else {
            // "Entering", step 2: the "active" look is the building's frame, 1 for a gold mine and 2 for any other
            // building, set only from frame 0. A building with no trips gives nothing, and since coming out lowers only
            // the return-to source's count, its count and frame are never lowered again.
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
        // Enhanced rules deliver only if the unit is carrying; Classic rules check only the gather mode.
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

    /// "Mine and platform amounts": the last trip was taken, so the mine or platform is destroyed, and every unit
    /// inside that has it as its target (the one that took the last trip included) loses that target and comes out at
    /// its next chance.
    fn collapse(&mut self, bi: usize) {
        let id = self.units[bi].id;
        for u in &mut self.units {
            if let Order::Inside { building } = &mut u.order
                && *building == Some(id)
            {
                *building = None;
                u.script.wait = u.script.wait.min(1);
            }
        }
        self.kill(bi);
    }

    /// "Coming out", step 6, facing: "Facing a target" from the exit tile, with the target unit of the order given
    /// (`next`: a return's depot, a harvest's source, a repair's building) and its goal tile (`aim`).
    ///
    /// Kept special case: an order 28 with no stored goal faces the building goal worked out from the exit tile. No
    /// command makes one now (`build_command` always stores its goal, V58), so this is never reached.
    fn come_out_face(&self, i: usize, next: Option<Order>, aim: Option<(i32, i32)>) -> Option<(i32, i32)> {
        let target = match next {
            Some(Order::Build { kind, site, goal: None }) => return Some(self.build_goal(i, kind, site)),
            Some(
                Order::ReturnGoods { depot: Some(t) }
                | Order::Harvest { source: Some(t), .. }
                | Order::Repair { target: t },
            ) => self.unit(t),
            _ => None,
        };
        facing_tile(target, aim)
    }

    /// "Coming out", step 6, aim: the goal tile the command of steps 3 to 5 stored ("Commands given by these orders"),
    /// so the unit comes out on that side. A target unit's top-left tile pulled back (a return's depot, V60; a
    /// harvest's source, V64, V65), a tree tile (already the pulled-back goal), a build job's stored goal (V58). With
    /// no command, none: the search is aimed at the unit's own tile.
    ///
    /// Kept special cases: a computer player's repair job aims at the building's top-left tile, not pulled back
    /// (V57; open question "Repair aim on coming out"); an order 28 with no stored goal (never made now) at its site.
    fn come_out_goal(&self, i: usize, next: Option<Order>) -> Option<(i32, i32)> {
        match next {
            Some(Order::ReturnGoods { depot: Some(t) } | Order::Harvest { source: Some(t), .. }) => {
                self.unit(t).map(|t| self.pull_back(i, t.tile))
            }
            Some(Order::Harvest { tree, .. }) => Some(tree),
            Some(Order::Build { goal: Some(g), .. }) => Some(g),
            Some(Order::Build { site, .. }) => Some(site),
            Some(Order::Repair { target }) => self.unit(target).map(|b| b.tile),
            _ => None,
        }
    }

    /// Order 26 when the wait inside ends ("Coming out"): step 1's immediate idle, step 2's count, the command of steps
    /// 3 to 5, and step 6's placement, aimed at the command's goal and facing by "Facing a target". The command is
    /// made pending and waits for the 25-cycle wait; with no command, a carrying unit (step 4, no depot) has idle
    /// pending (V73), and any other unit has nothing pending, so its idle logic runs when the wait ends (V85).
    pub(crate) fn come_out(&mut self, i: usize, _building: Option<UnitId>) {
        self.go_idle(i);
        self.leave_source(i);
        let next = self.come_out_command(i);
        let aim = self.come_out_goal(i, next);
        let at = self.units[i].tile;
        // Step 6: the hall-sized exit mark sizes the search's source as a town hall; this come-out clears it.
        let side = if std::mem::take(&mut self.units[i].hall_exit) {
            4
        } else {
            3
        };
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
        let face = self.come_out_face(i, next, aim);
        let u = &mut self.units[i];
        if let Some(f) = face.filter(|&f| f != tile) {
            u.facing = crate::world::face_tile(tile, f);
        }
        let carrying = u.carrying;
        u.order = Order::Idle;
        u.pending = next.or(carrying.then_some(Order::Idle));
        // "Commands given by these orders": the command marks the unit as about to move (pathfinding spec), clears
        // the blocked counter and forgets a stale attacker. Its route was dropped by step 1's idle. The command check
        // on a named target has nothing to drop here: a return's depot is one the depot search just found, and a
        // harvest command names no target the check covers (as for `command`). Kept special case: done here rather
        // than by `command`, whose pending idle (a tanker's command on a platform placed this cycle) would resume a
        // remembered order.
        u.commanded = next.is_some();
        if next.is_some() {
            u.blocked = 0;
            self.forget_stale_attacker(i);
        }
        let u = self.units[i].clone();
        self.mark(&u, tile, Some(u.id));
        self.start_anim(i, anim::STILL);
        self.units[i].script.wait = OUT_WAIT;
    }

    /// "Coming out", step 2: a carrying unit lowers the occupant count of its return-to source, if that is a mine or
    /// platform, whatever it comes out of (V71, V75). A count of 0 counts as 1; reaching 0 ends the "active" look.
    fn leave_source(&mut self, i: usize) {
        if self.units[i].carrying
            && let Some(Source::Unit(s)) = self.units[i].return_to
            && let Some(si) = self.index(s)
            && self.flags_of(self.units[si].kind) & (flags::MINE | flags::OIL_SOURCE) != 0
        {
            let b = &mut self.units[si];
            b.occupants = b.occupants.max(1) - 1;
            if b.occupants == 0 {
                b.script.frame = 0;
            }
        }
    }

    /// "Coming out", steps 3 to 5: a computer player's job choice (a job given skips the rest; worker-ai spec), else
    /// a return command to the nearest depot if carrying, else a harvest command back to the return-to source, which
    /// is forgotten. The harvest command is given whatever the source has become (V84). A source whose slot is gone
    /// gets the command at the unit's own tile (see the pinned test; a question for the spec).
    fn come_out_command(&mut self, i: usize) -> Option<Order> {
        if self.chooses_jobs(i)
            && let crate::worker_ai::Choice::Order(o) = self.worker_choice(i)
        {
            return Some(o);
        }
        if self.units[i].carrying {
            return self
                .nearest_depot(&self.units[i])
                .map(|d| Order::ReturnGoods { depot: Some(d) });
        }
        match self.units[i].return_to.take() {
            Some(Source::Unit(s)) => {
                let tile = self.unit(s).map_or(self.units[i].tile, |s| s.tile);
                self.harvest_command(i, Some(s), tile)
            }
            Some(Source::Tree(t)) => self.harvest_command(i, None, t),
            None => None,
        }
    }

    /// Order 23 in lumber mode ("Lumber"). More than 1 tile from the tree only the walk runs ("Walking orders"; step 1's
    /// new harvest command is given only by the same-update run after "No route", [`World::lumber_after_no_route`]).
    /// Once in reach, after the reaction: step 2 (start chopping, or the nearest tree), step 3 (count up, or fell and
    /// return) and step 4 (the reservation lost: the nearest tree).
    fn lumber_order(&mut self, i: usize, tree: (i32, i32), chopping: bool) {
        let here = self.units[i].tile;
        if chebyshev(here, tree) > RADIUS {
            if self.move_toward(i, Goal::tile(tree, RADIUS)) == Walk::NoPath && !self.react_at_walk_end(i) {
                self.no_route(i);
            }
            return;
        }
        // Orders spec, "The arrival switch": a worker still in its move animation is switched to the chop animation
        // before the logic runs, with or without an attacker, and starting to chop then doesn't start it again.
        let walked = self.units[i].script.anim == anim::MOVE;
        if self.react_at_walk_end(i) {
            return;
        }
        let reserved = self.terrain.region(tree.0, tree.1) == Some(region::RESERVED_FOREST);
        if !chopping {
            if self.start_chopping(i, tree) {
                if !walked {
                    self.switch_anim(i, anim::ATTACK);
                }
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
        // "Lumber", step 3: the worker carries lumber, its frame is set to 0, and it is given a return command to the
        // nearest depot (idle with none; the take-over looks it up).
        let u = &mut self.units[i];
        u.carrying = true;
        u.script.frame = 0;
        self.command_at_once(i, Order::ReturnGoods { depot: None });
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
        // "Facing a target": no target unit, so the tree tile.
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

    /// "Lumber", steps 2 and 4: a new harvest command to the nearest tree, or with none idle made pending and taken
    /// over in the same update (still animation, frame kept, wait 3), without running the idle logic (V53). "No route"
    /// has the generic immediate idle instead ([`World::no_route`]).
    fn next_tree(&mut self, i: usize) {
        if !self.aim_at_nearest_tree(i) {
            self.idle_now(i);
        }
    }

    /// "Finding the nearest tree" from the unit's own tile, and a new harvest command to it. Returns false, with the
    /// order no longer chopping, if there is none.
    fn aim_at_nearest_tree(&mut self, i: usize) -> bool {
        if let Order::Harvest { chopping, .. } = &mut self.units[i].order {
            *chopping = false;
        }
        let Some(t) = self.nearest_tree(self.units[i].tile) else {
            return false;
        };
        self.harvest_again(i, t);
        true
    }

    /// A new harvest command to tree tile `t`, given by the worker's own logic ("Lumber", "Every new aim is a new
    /// harvest command"). Only workers reach here, and a worker always gets a harvest order.
    fn harvest_again(&mut self, i: usize, t: (i32, i32)) {
        if let Some(o) = self.harvest_command(i, None, t) {
            self.command_at_once(i, o);
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

    /// "Lumber", step 2 (round-48 errata, `xhuman2-10000` peon 1530, n = 5820): a worker whose tree is taken and that
    /// finds no other tree gets idle as a pending order taking over in the same update: still animation, frame kept,
    /// wait 3.
    #[test]
    fn no_other_tree_takes_over_a_pending_idle_keeping_the_frame() {
        let mut w = land();
        forest(&mut w, 10, 8);
        assert!(w.terrain.reserve(10, 8));
        let p = w.spawn(0, 0, (10, 9), 0).unwrap();
        let i = w.index(p).unwrap();
        w.units[i].gather = Gather::Lumber;
        w.units[i].order = Order::Harvest {
            source: None,
            tree: (10, 8),
            chopping: false,
        };
        w.units[i].script.frame = 25;
        w.harvest_order(i, None, (10, 8), false);
        let u = &w.units[i];
        assert_eq!((u.order, u.pending), (Order::Idle, None));
        assert_eq!((u.script.anim, u.script.frame, u.script.wait), (anim::STILL, 25, 3));
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
        w.come_out(pi, Some(b));
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
        w.come_out(ti, Some(platform));
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

    /// Round-47 errata, "Coming out" step 6: every delivery at a gold-accepting building sets the hall-sized exit
    /// mark, carrying or not; a lumber mill doesn't; the next come-out from any building clears it.
    #[test]
    fn the_hall_sized_exit_mark_lasts_until_the_next_come_out() {
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let hall = w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let mill = w.spawn(LUMBER_MILL, 0, (4, 12), 0).unwrap();
        let p = w.spawn(0, 0, (12, 4), 0).unwrap();
        let mark = |w: &World| w.unit(p).unwrap().hall_exit;
        assert_eq!(w.unit(p).unwrap().gather, Gather::None);
        go_in_now(&mut w, p, hall);
        assert!(mark(&w), "set without carrying anything, gather mode none");
        come_out_now(&mut w, p, hall);
        assert!(!mark(&w));
        go_in_now(&mut w, p, hall);
        come_out_now(&mut w, p, hall);
        go_in_now(&mut w, p, mine);
        assert!(!mark(&w), "a mine doesn't set it");
        let pi = w.index(p).unwrap();
        w.units[pi].hall_exit = true;
        come_out_now(&mut w, p, mine);
        assert!(!mark(&w), "a mine's come-out clears it");
        let pi = w.index(p).unwrap();
        w.units[pi].gather = Gather::Lumber;
        let mi = w.index(mill).unwrap();
        w.go_in(pi, mi);
        assert!(!mark(&w), "a lumber mill doesn't set it");
    }

    /// Round-47 errata (`xhuman3-10000` tanker 1418, n = 8749): arriving at a mine still under construction makes idle
    /// the pending order, taken over at once with wait 3 and no idle logic.
    #[test]
    fn arriving_at_an_unfinished_mine_takes_over_a_pending_idle() {
        let mut w = land();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let mi = w.index(mine).unwrap();
        w.units[mi].construction = Some(Default::default());
        let p = w.spawn(0, 0, (13, 4), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].gather = Gather::Gold;
        w.units[pi].order = Order::Harvest {
            source: Some(mine),
            tree: (14, 4),
            chopping: false,
        };
        w.harvest_order(pi, Some(mine), (14, 4), false);
        let u = &w.units[pi];
        assert_eq!((u.order, u.script.anim, u.script.wait), (Order::Idle, anim::STILL, 3));
    }

    /// Round-52 errata, "No route" (the `xhuman12-10000` peon 1386 vector at n = 5598, moved to (10, 10)): the nearest
    /// tree (12, 14) is pulled back to (11, 13); the worker isn't within reach of it, so it is given the harvest command
    /// again and the goal is pulled back a second time, from (11, 13) to (10, 12). Facing kept, attack animation,
    /// wait 3, no step.
    #[test]
    fn a_worker_out_of_reach_of_the_new_tree_has_its_goal_pulled_back_twice() {
        let mut w = land();
        forest(&mut w, 12, 14);
        for (x, y) in [(11, 13), (10, 12)] {
            let n = w.terrain.at(x, y).unwrap();
            w.terrain.words[n] |= bits::OBSTACLE;
        }
        let p = w.spawn(0, 0, (10, 10), 0).unwrap();
        let i = w.index(p).unwrap();
        assert_eq!(w.pull_back(i, (12, 14)), (11, 13), "the first pull-back");
        w.units[i].gather = Gather::Lumber;
        w.units[i].facing = 6;
        w.units[i].order = Order::Harvest {
            source: None,
            tree: (20, 20),
            chopping: false,
        };
        w.no_route(i);
        let u = &w.units[i];
        let want = Order::Harvest {
            source: None,
            tree: (10, 12),
            chopping: false,
        };
        assert_eq!((u.order, u.pending), (want, None));
        assert_eq!(
            (u.tile, u.facing, u.script.anim, u.script.wait),
            ((10, 10), 6, anim::ATTACK, 3)
        );
        assert_eq!(w.terrain.region(12, 14), Some(region::FOREST), "not reserved");
    }

    /// Round-56 errata, "No route" (`orc11-25000` peasant 1441, n = 10428, in small): with no route and no tree, the
    /// worker goes idle as an immediate order (no pending idle, wait 1) and its idle logic runs in the same update.
    #[test]
    fn a_worker_with_no_route_and_no_tree_goes_idle_at_once() {
        let mut w = land();
        let p = w.spawn(0, 0, (10, 10), 0).unwrap();
        let i = w.index(p).unwrap();
        w.units[i].gather = Gather::Lumber;
        w.units[i].order = Order::Harvest {
            source: None,
            tree: (20, 20),
            chopping: false,
        };
        let before = w.rng.state;
        w.no_route(i);
        let u = &w.units[i];
        assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, None, 1));
        assert_ne!(
            w.rng.state, before,
            "the idle logic's look-around drew in the same update"
        );
    }

    /// Round-52 errata, "Coming out" (the `human13-10000` peon 1550 vector at n = 196, in small): a worker coming out
    /// of a mine with a return is placed toward the depot's top-left tile pulled back, (10, 10) here rather than the
    /// hall's (2, 18), so it comes out on (11, 9), not (11, 10); it still faces the hall's top-left tile plus (1, 1).
    #[test]
    fn a_return_comes_out_toward_the_pulled_back_goal_facing_the_depot() {
        let mut w = land();
        for x in 3..=10 {
            for y in 0..24 {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.words[n] |= bits::OBSTACLE;
            }
        }
        let hall = w.spawn(kind::TOWN_HALL, 0, (2, 18), 0).unwrap();
        let mine = w.spawn(MINE, 15, (12, 8), 100).unwrap();
        let p = w.spawn(0, 0, (11, 8), 0).unwrap();
        go_in_now(&mut w, p, mine);
        let pi = w.index(p).unwrap();
        assert!(w.units[pi].carrying);
        assert_eq!(w.pull_back(pi, (2, 18)), (10, 10));
        come_out_now(&mut w, p, mine);
        let u = &w.units[pi];
        assert_eq!(u.pending, Some(Order::ReturnGoods { depot: Some(hall) }));
        assert_eq!((u.order, u.tile), (Order::Idle, (11, 9)));
        // Toward (3, 19): south-west; toward the pulled-back goal's (11, 11) it would be south (4).
        assert_eq!(u.facing, 5);
    }

    /// Round-58 errata, "Coming out" step 6 (the `mp-rose-petal-bne-10000` peasant 1567 vector at n = 1731, moved
    /// by (-50, -74)): a worker leaving a town hall toward a gold mine aims at the mine's top-left tile pulled back.
    /// From the hall's (5, 10) toward the mine's (9, 4), the first tile tested, (8, 5), is forest and the next, (8, 6),
    /// is usable, so the goal is (8, 5): the worker comes out at (5, 9), facing 1 toward the mine's (10, 5). Aimed at
    /// (9, 4) it would come out at (9, 10) facing 0.
    #[test]
    fn leaving_toward_a_mine_aims_at_its_pulled_back_top_left_tile() {
        let mut w = land();
        forest(&mut w, 8, 5);
        let mine = w.spawn(MINE, 15, (9, 4), 100).unwrap();
        let hall = w.spawn(kind::TOWN_HALL, 0, (5, 10), 0).unwrap();
        let p = w.spawn(0, 0, (5, 14), 0).unwrap();
        go_in_now(&mut w, p, hall);
        let pi = w.index(p).unwrap();
        assert!(w.units[pi].hall_exit);
        w.units[pi].return_to = Some(Source::Unit(mine));
        assert_eq!(w.units[pi].tile, (5, 10));
        assert_eq!(w.pull_back(pi, (9, 4)), (8, 5));
        come_out_now(&mut w, p, hall);
        let u = &w.units[pi];
        assert!(matches!(u.pending, Some(Order::Harvest { source: Some(s), .. }) if s == mine));
        assert_eq!((u.order, u.tile, u.facing), (Order::Idle, (5, 9), 1));
    }

    /// Round-58 errata (the `mp-gold-rush-bne-6000` peon 1511 vector at n = 2353, moved by (0, -30)): the worker faces
    /// the mine's top-left tile plus (1, 1), (7, 14), not the pulled-back goal (8, 15) plus (1, 1).
    #[test]
    fn leaving_toward_a_mine_faces_its_top_left_tile_plus_one() {
        let mut w = land();
        // Forest in place of the trace's other ground keeps the exit off (10, 19).
        forest(&mut w, 10, 19);
        let mine = w.spawn(MINE, 15, (6, 13), 100).unwrap();
        let hall = w.spawn(kind::TOWN_HALL, 0, (11, 19), 0).unwrap();
        let p = w.spawn(0, 0, (16, 20), 0).unwrap();
        go_in_now(&mut w, p, hall);
        let pi = w.index(p).unwrap();
        w.units[pi].return_to = Some(Source::Unit(mine));
        assert_eq!(w.pull_back(pi, (6, 13)), (8, 15));
        come_out_now(&mut w, p, hall);
        // From the exit tile, the pulled-back goal's (9, 16) would give 0.
        let u = &w.units[pi];
        assert_eq!((u.tile, u.facing), ((10, 20), 7));
        assert_eq!(crate::world::face_tile(u.tile, (9, 16)), 0);
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

    /// Round-51 errata ("Coming out", step 6; harvest spec round-51 vectors): a worker coming out with a build job faces
    /// the build goal worked out when the command was given, while it was still inside at the hall's top-left tile, not
    /// one worked out again from its exit tile. The `xhuman9-10000` peon 1440 case: hall at (114, 109), exit (113, 110),
    /// a 2 x 2 site at (110, 109).
    #[test]
    fn a_build_job_coming_out_faces_the_goal_worked_out_inside() {
        let mut w = field(128, 128);
        w.stats.0[0].flags |= flags::PEON;
        w.stats.0[kind::FARM as usize].size = (2, 2);
        w.stats.0[kind::FARM as usize].flags |= flags::BUILDING;
        for (hall, exit, site, goal, facing) in [
            ((114, 109), (113, 110), (110, 109), (111, 109), 7),
            ((8, 8), (10, 10), (14, 10), (14, 10), 2),
        ] {
            let p = w.spawn(0, 0, hall, 0).unwrap();
            let i = w.index(p).unwrap();
            w.units[i].tile = hall;
            let next = w.build_command(i, kind::FARM, site);
            assert_eq!(
                next,
                Some(Order::Build {
                    kind: kind::FARM,
                    site,
                    goal: Some(goal)
                })
            );
            w.units[i].tile = exit;
            // Worked out again from the exit tile, the goal would differ in the first case: (111, 110).
            if hall == (114, 109) {
                assert_eq!(w.build_goal(i, kind::FARM, site), (111, 110));
            }
            let face = w.come_out_face(i, next, w.come_out_goal(i, next));
            assert_eq!(face, Some(goal));
            assert_eq!(crate::world::face_tile(exit, goal), facing);
        }
    }

    /// Round-54 errata, "Collapse" (`human12-25000` tanker 1410, n = 12948): the worker that takes the last trip loses
    /// its target with every other unit inside (wait 1), carries its load and has no return-to source; it comes out
    /// at its next step done and heads for a depot.
    #[test]
    fn a_collapse_clears_the_target_of_the_worker_that_took_the_last_trip() {
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (4, 4), 0).unwrap();
        let mine = w.spawn(MINE, 15, (12, 4), 2).unwrap();
        let a = w.spawn(0, 0, (11, 4), 0).unwrap();
        let b = w.spawn(0, 0, (11, 5), 0).unwrap();
        go_in_now(&mut w, a, mine);
        let ai = w.index(a).unwrap();
        assert_eq!(w.units[ai].order, Order::Inside { building: Some(mine) });
        assert_eq!(w.units[ai].recorded_target(), Some(mine));
        go_in_now(&mut w, b, mine);
        assert!(w.unit(mine).unwrap().dying, "the last trip collapses the mine");
        for id in [a, b] {
            let u = w.unit(id).unwrap();
            assert_eq!(
                (u.order, u.recorded_target(), u.script.wait),
                (Order::Inside { building: None }, None, 1),
                "{id:?}"
            );
        }
        let u = w.unit(b).unwrap();
        assert!(u.carrying);
        assert_eq!(u.return_to, None);
        let bi = w.index(b).unwrap();
        w.come_out(bi, None);
        let u = w.unit(b).unwrap();
        assert!(!u.inside);
        assert_eq!(u.pending, Some(Order::ReturnGoods { depot: Some(hall) }));
    }

    /// Round-62 vector (`orc9-orc09-return-goods-2200` peon 1482, n = 1482): a worker carrying gold into a lumber mill
    /// (wood storage only) takes the delivery branch; the table adds nothing and the load is lost.
    #[test]
    fn gold_taken_to_a_lumber_mill_is_lost() {
        let mut w = land();
        let mill = w.spawn(LUMBER_MILL, 0, (10, 4), 0).unwrap();
        let p = w.spawn(0, 0, (9, 5), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].carrying = true;
        go_in_now(&mut w, p, mill);
        let u = w.unit(p).unwrap();
        assert_eq!(u.order, Order::Inside { building: Some(mill) });
        assert!(!u.carrying, "the load is lost");
        assert_eq!((w.players[0].gold, w.players[0].gathered[0]), (0, 0));
        assert_eq!(mine_look(&w, mill), (0, 0), "a depot's look and count don't change");
    }

    /// Round-62 vector (`orc9-orc09-return-goods-2200` tanker 1473, n = 1287): a carrying tanker that enters its own
    /// oil platform takes the mine-or-platform branch and delivers nothing.
    #[test]
    fn a_tanker_at_its_own_platform_delivers_nothing() {
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
        w.units[ti].carrying = true;
        w.go_in(ti, pi);
        let u = w.unit(t).unwrap();
        assert!(u.carrying);
        assert_eq!(
            u.order,
            Order::Inside {
                building: Some(platform)
            }
        );
        assert_eq!(w.players[0].oil, 0);
        assert_eq!(mine_look(&w, platform), (1, 2));
    }

    /// Round-62 vector (`orc9-orc09-enter-dying-1800` peon 1482, n = 1435): a worker in order 24 whose depot starts
    /// dying is given return goods to the nearest depot as a pending order that takes over in the same update (still
    /// animation, wait 3, cached route dropped). With no depot left, idle takes over the same way.
    #[test]
    fn a_dying_depot_reissues_return_goods_at_once() {
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
        let mill = w.spawn(LUMBER_MILL, 0, (16, 4), 0).unwrap();
        let p = w.spawn(0, 0, (10, 10), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].carrying = true;
        w.units[pi].gather = Gather::Lumber;
        w.units[pi].order = Order::ReturnGoods { depot: Some(mill) };
        w.units[pi].path = vec![(11, 9)];
        let mi = w.index(mill).unwrap();
        w.kill(mi);
        w.return_order(pi, Some(mill));
        let u = &w.units[pi];
        assert_eq!((u.order, u.pending), (Order::ReturnGoods { depot: Some(hall) }, None));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
        assert!(u.path.is_empty());
        // No depot: idle.
        let hi = w.index(hall).unwrap();
        w.kill(hi);
        w.return_order(pi, Some(hall));
        let u = &w.units[pi];
        assert_eq!((u.order, u.script.anim, u.script.wait), (Order::Idle, anim::STILL, 3));
    }

    /// Round-62 vector (`orc9-orc09-enter-dying-1800` peon 1478, n = 1441): order 25 checks its target first, so a
    /// worker already stepping into a building that has started dying dies with no target; one that hasn't stepped in
    /// gets a pending idle that takes over in the same update.
    #[test]
    fn entering_a_dying_building_drops_it_first() {
        let mut w = land();
        let mill = w.spawn(LUMBER_MILL, 0, (10, 4), 0).unwrap();
        let p = w.spawn(0, 0, (9, 5), 0).unwrap();
        let q = w.spawn(0, 0, (9, 7), 0).unwrap();
        let (pi, qi) = (w.index(p).unwrap(), w.index(q).unwrap());
        w.units[pi].order = Order::Enter {
            building: mill,
            stepped: false,
        };
        w.enter_order(pi, mill, false);
        w.units[qi].order = Order::Enter {
            building: mill,
            stepped: false,
        };
        let mi = w.index(mill).unwrap();
        w.kill(mi);
        let pi = w.index(p).unwrap();
        w.enter_order(pi, mill, true);
        let u = w.unit(p).unwrap();
        assert!(u.dying);
        assert_eq!(u.dying_target, None, "it dies with no target");
        let qi = w.index(q).unwrap();
        w.enter_order(qi, mill, false);
        let u = w.unit(q).unwrap();
        assert!(!u.dying);
        assert_eq!((u.order, u.script.anim, u.script.wait), (Order::Idle, anim::STILL, 3));
    }

    const BLACKSMITH: u8 = 0x53;
    const SHIPYARD: u8 = 0x49;

    /// [`land`] with a building that stores nothing (a blacksmith) and one that stores oil only (a shipyard).
    fn land_with_shops() -> World {
        let mut w = land();
        for (k, f) in [(BLACKSMITH, 0), (SHIPYARD, flags::OIL_STORAGE)] {
            w.stats.0[k as usize] = w.stats.0[kind::FARM as usize];
            w.stats.0[k as usize].flags = flags::BUILDING | f;
        }
        w
    }

    /// Round-63 vectors (`orc9-orc09-enter-nothing-1900` n = 910 and 1367, n = 1060): a worker carrying gold into a
    /// building that stores neither gold nor wood (a blacksmith, or a shipyard that stores oil only) goes inside
    /// without delivering: it keeps its load and its return-to source, and the building's count goes up and its frame
    /// becomes 2. Coming out lowers the return-to mine's count (its frame back to 0 at 0), not the building's, which
    /// keeps frame 2.
    #[test]
    fn a_worker_keeps_its_load_at_a_building_storing_neither_gold_nor_wood() {
        for shop in [BLACKSMITH, SHIPYARD] {
            let mut w = land_with_shops();
            let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
            let b = w.spawn(shop, 0, (8, 12), 0).unwrap();
            w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
            let p = w.spawn(0, 0, (12, 4), 0).unwrap();
            go_in_now(&mut w, p, mine);
            come_out_now(&mut w, p, mine);
            // A second worker keeps the mine "active" (count 1, frame 1).
            let q = w.spawn(0, 0, (12, 6), 0).unwrap();
            go_in_now(&mut w, q, mine);
            assert_eq!(mine_look(&w, mine), (1, 1));
            go_in_now(&mut w, p, b);
            let u = w.unit(p).unwrap();
            assert_eq!(u.order, Order::Inside { building: Some(b) }, "{shop:#x}");
            assert!(u.carrying, "{shop:#x}: the load is kept");
            assert_eq!(u.return_to, Some(Source::Unit(mine)));
            assert_eq!((w.players[0].gold, w.players[0].oil), (0, 0));
            assert_eq!(mine_look(&w, b), (1, 2), "{shop:#x}");
            come_out_now(&mut w, p, b);
            assert_eq!(mine_look(&w, b), (1, 2), "{shop:#x}: never lowered");
            assert_eq!(mine_look(&w, mine), (0, 0), "{shop:#x}: the return-to mine's count");
        }
    }

    /// Round-63 vectors (`orc9-orc09-enter-nothing-1900` n = 1517 and 1542): a carrying worker that comes out with no
    /// depot stays idle with idle as its pending order, which takes over when the 25-cycle wait ends (still animation,
    /// wait 3). Round-65 errata: only this no-depot case makes idle pending.
    #[test]
    fn coming_out_with_no_depot_takes_over_a_pending_idle() {
        let mut w = land_with_shops();
        let b = w.spawn(SHIPYARD, 0, (8, 8), 0).unwrap();
        let p = w.spawn(0, 0, (7, 8), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].carrying = true;
        go_in_now(&mut w, p, b);
        come_out_now(&mut w, p, b);
        let u = w.unit(p).unwrap();
        assert!(!u.inside && u.carrying);
        assert_eq!((u.order, u.pending), (Order::Idle, Some(Order::Idle)));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, OUT_WAIT));
        let taken = (0..40)
            .find(|_| {
                w.step(&[]);
                w.unit(p).unwrap().pending.is_none()
            })
            .expect("the pending idle takes over");
        let u = w.unit(p).unwrap();
        assert_eq!(taken, OUT_WAIT as usize - 1);
        assert_eq!(
            (u.order, u.script.anim, u.script.wait),
            (Order::Idle, anim::STILL, TAKEOVER_WAIT)
        );
    }

    /// Round-65 vector (`xhuman5-30000` tanker 1495, n = 26786 to 26811): a unit that comes out given no command, not
    /// carrying and with no return-to source, has nothing pending; when the 25-cycle wait ends its idle logic runs
    /// with wait 1, with no take-over.
    #[test]
    fn coming_out_with_nothing_to_do_leaves_nothing_pending() {
        let mut w = land_with_shops();
        let b = w.spawn(SHIPYARD, 0, (8, 8), 0).unwrap();
        let p = w.spawn(0, 0, (7, 8), 0).unwrap();
        go_in_now(&mut w, p, b);
        let pi = w.index(p).unwrap();
        w.units[pi].return_to = None;
        come_out_now(&mut w, p, b);
        let u = w.unit(p).unwrap();
        assert!(!u.inside && !u.carrying);
        assert_eq!((u.order, u.pending), (Order::Idle, None));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, OUT_WAIT));
        for _ in 0..OUT_WAIT {
            w.step(&[]);
        }
        let u = w.unit(p).unwrap();
        assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, None, 1));
    }

    /// Round-63 vector (`orc9-orc09-return-nodepot-2200` peon 1482, n = 1196): stepping into a target that isn't a
    /// building faces the target's own tile, not its tile plus (1, 1).
    #[test]
    fn stepping_into_a_unit_faces_its_own_tile() {
        let mut w = land();
        let p = w.spawn(0, 0, (10, 10), 0).unwrap();
        let q = w.spawn(0, 0, (10, 9), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.enter_order(pi, q, false);
        let u = &w.units[pi];
        assert_eq!((u.facing, u.tile), (0, (10, 9)));
    }

    /// Round-63 vector (`orc9-orc09-return-nodepot-2200` peon 1482, n = 1374): command 24 given a target unit that is
    /// inside something fails the command check: no target, idle pending in place of the return, the order kept, and
    /// no depot looked up.
    #[test]
    fn return_goods_to_a_unit_inside_something_leaves_a_pending_idle() {
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let p = w.spawn(0, 0, (10, 10), 0).unwrap();
        let q = w.spawn(0, 0, (12, 4), 0).unwrap();
        go_in_now(&mut w, q, mine);
        let pi = w.index(p).unwrap();
        w.units[pi].carrying = true;
        w.units[pi].gather = Gather::Gold;
        w.units[pi].pending = Some(Order::ReturnGoods { depot: Some(hall) });
        w.command(pi, Order::ReturnGoods { depot: Some(q) });
        let u = &w.units[pi];
        assert_eq!(
            (u.order, u.pending, u.recorded_target()),
            (Order::Idle, Some(Order::Idle), None)
        );
        assert!(u.carrying);
    }

    /// Round-63 vectors (static study): order 24 runs the reaction when its walk has ended. A worker hit by an enemy
    /// worker as it reaches its depot fights back instead of entering; one hit by an archer while walking doesn't
    /// react, and on arrival doesn't flee (order 24 isn't one of the flee rule's orders) but enters, the attacker kept.
    #[test]
    fn a_returning_worker_reacts_only_on_arrival() {
        let mut w = land();
        w.players[0].controller = crate::ai::controller::HUMAN;
        w.players[1].controller = crate::ai::controller::COMPUTER;
        const ARCHER: u8 = 8;
        w.stats.0[0].flags |= flags::CAN_ATTACK;
        w.stats.0[ARCHER as usize] = w.stats.0[0];
        w.stats.0[ARCHER as usize].flags = flags::LAND | flags::CAN_ATTACK;
        let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
        let next_to = (2 + w.unit(hall).unwrap().size.0 as i32, 2);
        for (foe, reaches) in [(0, true), (ARCHER, true), (ARCHER, false)] {
            let p = w.spawn(0, 0, if reaches { next_to } else { (10, 10) }, 0).unwrap();
            let a = w.spawn(foe, 1, (12, 12), 0).unwrap();
            let ai = w.index(a).unwrap();
            w.units[ai].detected_by = 0xff;
            let pi = w.index(p).unwrap();
            w.units[pi].carrying = true;
            w.units[pi].gather = Gather::Gold;
            w.units[pi].order = Order::ReturnGoods { depot: Some(hall) };
            w.units[pi].attacked_by = Some(a);
            w.return_order(pi, Some(hall));
            let u = &w.units[pi];
            match (foe, reaches) {
                // The fight-back command is pending, as any command.
                (0, _) => assert_eq!(u.pending, Some(Order::Attack { target: a })),
                (_, true) => {
                    assert!(matches!(u.order, Order::Enter { .. }), "{:?}", u.order);
                    assert_eq!(u.attacked_by, Some(a));
                }
                _ => {
                    assert_eq!(
                        (u.order, u.attacked_by),
                        (Order::ReturnGoods { depot: Some(hall) }, Some(a))
                    );
                    assert_eq!(u.script.anim, anim::MOVE);
                }
            }
            let ai = w.index(a).unwrap();
            w.kill(ai);
            let pi = w.index(p).unwrap();
            w.kill(pi);
        }
    }

    /// Round-64 vectors (`orc9-orc09-return-inside-1200` peon 1482, n = 558): a worker returning goods to a unit that
    /// goes inside a mine while the worker walks drops it at its next step boundary and is given return goods to the
    /// nearest depot, taken over at once (still animation, wait 3). It doesn't walk on toward the unit.
    #[test]
    fn a_return_target_gone_inside_is_dropped_at_the_next_step_boundary() {
        let mut w = land();
        let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let p = w.spawn(0, 0, (10, 12), 0).unwrap();
        let q = w.spawn(0, 0, (12, 4), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].carrying = true;
        w.units[pi].gather = Gather::Gold;
        w.units[pi].order = Order::ReturnGoods { depot: Some(q) };
        w.start_anim(pi, anim::MOVE);
        w.return_order(pi, Some(q));
        assert_eq!(w.units[pi].tile, (11, 11), "a step toward the worker");
        go_in_now(&mut w, q, mine);
        w.return_order(pi, Some(q));
        let u = &w.units[pi];
        assert_eq!((u.order, u.pending), (Order::ReturnGoods { depot: Some(hall) }, None));
        assert_eq!((u.tile, u.script.anim, u.script.wait), ((11, 11), anim::STILL, 3));
        assert!(u.path.is_empty());
    }

    /// Round-64 vector (static study): a worker in order 24 whose target unit is already inside a mine when it arrives
    /// within reach starts order 25, and at its first run the target check drops the target: idle becomes the pending
    /// order.
    #[test]
    fn a_return_target_inside_on_arrival_is_dropped_by_order_25() {
        let mut w = land();
        w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
        let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
        let q = w.spawn(0, 0, (13, 4), 0).unwrap();
        go_in_now(&mut w, q, mine);
        let p = w.spawn(0, 0, (13, 3), 0).unwrap();
        let pi = w.index(p).unwrap();
        w.units[pi].carrying = true;
        w.units[pi].order = Order::ReturnGoods { depot: Some(q) };
        w.return_order(pi, Some(q));
        assert_eq!(
            w.units[pi].order,
            Order::Enter {
                building: q,
                stepped: false
            }
        );
        w.enter_order(pi, q, false);
        let u = &w.units[pi];
        assert_eq!((u.order, u.script.anim, u.script.wait), (Order::Idle, anim::STILL, 3));
        assert!(!u.inside && u.carrying);
    }

    const TANKER: u8 = 9;
    const PLATFORM: u8 = 0x56;

    /// [`land`] with a land-going tanker type and an oil platform type.
    fn land_with_oil() -> World {
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
        w
    }

    /// Round-64 vectors (`xhuman5-30000` tanker 1542, n = 26447 to 26642): a harvest command that clears a tanker's
    /// target leaves order 23 with no target, aimed at the command's goal. The tanker walks there with no other logic,
    /// and on arrival idle becomes the pending order, taken over at once (still animation, wait 3).
    #[test]
    fn a_tanker_with_no_target_walks_to_its_goal_then_goes_idle() {
        let mut w = land_with_oil();
        let t = w.spawn(TANKER, 0, (4, 10), 0).unwrap();
        let i = w.index(t).unwrap();
        let o = w.harvest_command(i, None, (8, 10)).unwrap();
        assert_eq!(
            o,
            Order::Harvest {
                source: None,
                tree: (8, 10),
                chopping: false
            }
        );
        w.units[i].order = o;
        w.harvest_order(i, None, (8, 10), false);
        let u = &w.units[i];
        assert_eq!((u.order, u.tile, u.script.anim), (o, (5, 10), anim::MOVE));
        w.units[i].tile = (7, 10);
        w.units[i].path.clear();
        w.harvest_order(i, None, (8, 10), false);
        let u = &w.units[i];
        assert_eq!((u.order, u.pending, u.tile), (Order::Idle, None, (7, 10)));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
    }

    /// Round-64 vectors (`xhuman5-30000` tanker 1542, n = 26422): a tanker coming out whose return-to platform is gone
    /// is given the harvest command toward it all the same: order 23 with no target is pending, aimed at the
    /// platform's top-left tile, and the tanker comes out on that side.
    #[test]
    fn a_tanker_whose_platform_collapsed_comes_out_toward_it() {
        let out = |collapsed: bool| {
            let mut w = land_with_oil();
            let yard = w.spawn(SHIPYARD, 0, (10, 12), 0).unwrap();
            let platform = w.spawn(PLATFORM, 0, (10, 2), 100).unwrap();
            let t = w.spawn(TANKER, 0, (9, 12), 0).unwrap();
            let (ti, yi) = (w.index(t).unwrap(), w.index(yard).unwrap());
            w.go_in(ti, yi);
            let ti = w.index(t).unwrap();
            w.units[ti].return_to = Some(Source::Unit(platform));
            w.units[ti].carrying = false;
            if collapsed {
                let pi = w.index(platform).unwrap();
                w.kill(pi);
            }
            come_out_now(&mut w, t, yard);
            let u = w.unit(t).unwrap();
            assert_eq!((u.order, u.script.wait), (Order::Idle, OUT_WAIT));
            (u.pending, u.tile, u.facing)
        };
        let (pending, tile, facing) = out(true);
        assert_eq!(
            pending,
            Some(Order::Harvest {
                source: None,
                tree: (10, 2),
                chopping: false
            })
        );
        let (_, alive_tile, alive_facing) = out(false);
        assert_eq!(
            (tile, facing),
            (alive_tile, alive_facing),
            "the same side as toward a platform still there"
        );
    }

    /// Pins of the current behaviour where no oracle trace reaches (the coverage report's untraced harvest regions),
    /// written before the B-038 refactor so that it can't change them unseen. Each test states the outcome in a few
    /// fields and pins the whole simulation state with [`World::fingerprint`]. Where a pin differs from the spec, its
    /// comment says so: those are open questions, not rules.
    mod pin {
        use super::*;
        use crate::ai::controller;

        const FOE: u8 = 8;

        /// [`land_with_oil`] with player 0 human, player 1 computer, workers that can attack and a foe type (an
        /// archer-like fighter).
        fn arena() -> World {
            let mut w = land_with_oil();
            w.players[0].controller = controller::HUMAN;
            w.players[1].controller = controller::COMPUTER;
            w.stats.0[0].flags |= flags::CAN_ATTACK | flags::LAND;
            w.stats.0[FOE as usize] = w.stats.0[0];
            w.stats.0[FOE as usize].flags = flags::LAND | flags::CAN_ATTACK;
            w
        }

        /// An enemy of `kind` far away, recorded as `p`'s attacker and seen by everyone.
        fn attacker(w: &mut World, p: UnitId, kind: u8) -> UnitId {
            let a = w.spawn(kind, 1, (22, 22), 0).unwrap();
            let ai = w.index(a).unwrap();
            w.units[ai].detected_by = 0xff;
            let pi = w.index(p).unwrap();
            w.units[pi].attacked_by = Some(a);
            a
        }

        /// Walls the tiles round `c` (radius 1) so that a walk from `c` finds no route.
        fn wall_round(w: &mut World, c: (i32, i32)) {
            for (x, y) in ring(c, 1) {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.words[n] |= bits::OBSTACLE;
            }
        }

        fn harvesting(w: &mut World, p: UnitId, source: Option<UnitId>, tree: (i32, i32), gather: Gather) -> usize {
            let i = w.index(p).unwrap();
            w.units[i].gather = gather;
            w.units[i].order = Order::Harvest {
                source,
                tree,
                chopping: false,
            };
            i
        }

        #[test]
        fn return_command_needs_a_carrying_worker_and_a_depot() {
            let mut w = arena();
            let p = w.spawn(0, 0, (10, 10), 0).unwrap();
            let f = w.spawn(FOE, 0, (12, 10), 0).unwrap();
            let (pi, fi) = (w.index(p).unwrap(), w.index(f).unwrap());
            assert_eq!(w.return_command(pi), None, "not carrying");
            w.units[fi].carrying = true;
            assert_eq!(w.return_command(fi), None, "neither a worker nor a tanker");
            w.units[pi].carrying = true;
            assert_eq!(w.return_command(pi), Some(Order::Idle), "no depot");
            let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
            let pi = w.index(p).unwrap();
            assert_eq!(w.return_command(pi), Some(Order::ReturnGoods { depot: Some(hall) }));
        }

        /// A worker in order 23 with no target unit and not in lumber mode: a state no command makes ("Starting to
        /// harvest" gives gold mode only with a mine). Pinned as it is: the reaction runs, then idle at once without a
        /// take-over (no wait of 3).
        #[test]
        fn a_worker_with_no_target_out_of_lumber_mode_goes_idle() {
            let mut w = arena();
            let p = w.spawn(0, 0, (10, 10), 0).unwrap();
            let i = harvesting(&mut w, p, None, (16, 10), Gather::Gold);
            w.units[i].facing = 3;
            w.harvest_order(i, None, (16, 10), false);
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.tile, u.script.anim),
                (Order::Idle, None, (10, 10), anim::STILL)
            );
            assert_eq!(w.fingerprint(), 391676241161655986);
        }

        /// "Going to a mine or platform", no route: a tanker with a target runs the reaction (which only forgets the
        /// attacker of a unit that can't attack) before the generic no-route idle; a tanker with no target doesn't
        /// (V87, static study), so its same-update idle logic finds the attacker still recorded.
        #[test]
        fn a_tanker_with_no_route_reacts_only_with_a_target() {
            let mut prints = vec![];
            for with_target in [true, false] {
                let mut w = arena();
                let platform = w.spawn(PLATFORM, 0, (16, 4), 100).unwrap();
                let t = w.spawn(TANKER, 0, (10, 10), 0).unwrap();
                wall_round(&mut w, (10, 10));
                attacker(&mut w, t, FOE);
                let source = with_target.then_some(platform);
                let i = harvesting(&mut w, t, source, (16, 4), Gather::None);
                w.start_anim(i, anim::MOVE);
                w.harvest_order(i, source, (16, 4), false);
                let u = &w.units[i];
                // With a target the reaction forgot the attacker and the idle stays (wait 1); with none the idle
                // logic still finds the attacker and flees.
                if with_target {
                    assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, None, 1));
                } else {
                    assert!(matches!(u.order, Order::Move { .. }), "{:?}", u.order);
                }
                assert_eq!(u.attacked_by, None);
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [11896481358704396292, 13214669665595594724]);
        }

        /// "Going to a mine or platform", step 1 with an attacker: a worker whose mine is gone reacts first; hit by a
        /// worker it fights back (pending attack) and no idle takes over.
        #[test]
        fn a_gold_worker_whose_mine_is_lost_reacts_first() {
            let mut prints = vec![];
            for hit in [true, false] {
                let mut w = arena();
                let mine = w.spawn(MINE, 15, (16, 4), 100).unwrap();
                let p = w.spawn(0, 0, (10, 10), 0).unwrap();
                let a = hit.then(|| attacker(&mut w, p, 0));
                let mi = w.index(mine).unwrap();
                w.kill(mi);
                let i = harvesting(&mut w, p, Some(mine), (16, 4), Gather::Gold);
                w.harvest_order(i, Some(mine), (16, 4), false);
                let u = &w.units[i];
                match a {
                    Some(a) => {
                        assert!(matches!(u.order, Order::Harvest { .. }));
                        assert_eq!(u.pending, Some(Order::Attack { target: a }));
                    }
                    None => assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, None, 3)),
                }
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [7427822006686717663, 8390441432806348842]);
        }

        /// Order 23 with no route and an attacker: the code runs the reaction before the no-route rule, so a gold
        /// worker (with its mine) or a lumber worker hit by a worker fights back and makes no tree search. The spec's
        /// "No route" reads as the tree search first, then order 23's logic (and its reaction) in the same update; the
        /// traced case (`orc9-orc09-flee-chop-walk-900` n = 573) doesn't separate the two.
        #[test]
        fn a_worker_with_no_route_and_an_attacker_reacts_before_the_tree_search() {
            let mut prints = vec![];
            for gold in [true, false] {
                let mut w = arena();
                let mine = w.spawn(MINE, 15, (16, 4), 100).unwrap();
                forest(&mut w, 12, 12);
                let p = w.spawn(0, 0, (10, 10), 0).unwrap();
                wall_round(&mut w, (10, 10));
                let a = attacker(&mut w, p, 0);
                let (source, goal, mode) = if gold {
                    (Some(mine), (16, 4), Gather::Gold)
                } else {
                    (None, (20, 20), Gather::Lumber)
                };
                let i = harvesting(&mut w, p, source, goal, mode);
                w.start_anim(i, anim::MOVE);
                w.harvest_order(i, source, goal, false);
                let u = &w.units[i];
                assert_eq!(u.pending, Some(Order::Attack { target: a }), "{gold}");
                assert_eq!(
                    (u.order, u.gather),
                    (
                        Order::Harvest {
                            source,
                            tree: goal,
                            chopping: false
                        },
                        mode
                    )
                );
                assert_eq!(w.terrain.region(12, 12), Some(region::FOREST));
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [4982595562378309529, 8848653804881294756]);
        }

        /// The same with no attacker, for a gold worker: the tree search (V30), and the same-update lumber run.
        #[test]
        fn a_gold_worker_with_no_route_switches_to_lumber() {
            let mut w = arena();
            let mine = w.spawn(MINE, 15, (16, 4), 100).unwrap();
            forest(&mut w, 12, 12);
            let p = w.spawn(0, 0, (10, 10), 0).unwrap();
            wall_round(&mut w, (10, 10));
            let i = harvesting(&mut w, p, Some(mine), (16, 4), Gather::Gold);
            w.start_anim(i, anim::MOVE);
            w.units[i].blocked = 4;
            w.harvest_order(i, Some(mine), (16, 4), false);
            let u = &w.units[i];
            assert!(matches!(u.order, Order::Harvest { source: None, .. }), "{:?}", u.order);
            assert_eq!((u.gather, u.blocked, u.script.wait), (Gather::Lumber, 0, 3));
            assert_eq!(w.fingerprint(), 14908006865186026271);
        }

        /// Order 24 arriving at a depot still under construction: idle (order 2) at once, by an animation switch, not a
        /// take-over.
        #[test]
        fn a_return_to_an_unfinished_depot_goes_idle() {
            let mut w = arena();
            let hall = w.place_building(kind::TOWN_HALL, 0, (4, 4)).unwrap();
            let p = w.spawn(0, 0, (3, 4), 0).unwrap();
            let pi = w.index(p).unwrap();
            w.units[pi].carrying = true;
            w.units[pi].order = Order::ReturnGoods { depot: Some(hall) };
            w.return_order(pi, Some(hall));
            let pi = w.index(p).unwrap();
            let u = &w.units[pi];
            assert_eq!((u.order, u.pending, u.script.anim), (Order::Idle, None, anim::STILL));
            assert_eq!(w.fingerprint(), 18180086034743565868);
        }

        /// Order 24 whose depot is lost, with an attacker: the reaction runs first. Hit by a worker, it fights back,
        /// and the cached route is kept (it is dropped only after a reaction that does nothing).
        #[test]
        fn a_return_whose_depot_is_lost_reacts_first() {
            let mut w = arena();
            w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
            let mill = w.spawn(LUMBER_MILL, 0, (16, 4), 0).unwrap();
            let p = w.spawn(0, 0, (10, 10), 0).unwrap();
            let a = attacker(&mut w, p, 0);
            let pi = w.index(p).unwrap();
            w.units[pi].carrying = true;
            w.units[pi].gather = Gather::Lumber;
            w.units[pi].order = Order::ReturnGoods { depot: Some(mill) };
            w.units[pi].path = vec![(11, 9)];
            let mi = w.index(mill).unwrap();
            w.kill(mi);
            let pi = w.index(p).unwrap();
            w.return_order(pi, Some(mill));
            let u = &w.units[pi];
            assert_eq!(u.pending, Some(Order::Attack { target: a }));
            assert_eq!(u.path, vec![(11, 9)]);
            assert_eq!(w.fingerprint(), 5083180129386180915);
        }

        /// Order 24's depot re-issue ("Returning goods", step 2) is made pending and taken over, but as the code
        /// stands it isn't a command: the blocked counter is kept (the spec's "Commands given by these orders" would
        /// clear it), and with no depot left the pending idle may resume a remembered order.
        #[test]
        fn a_return_whose_depot_is_lost_keeps_its_blocked_counter() {
            let mut w = arena();
            let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
            let mill = w.spawn(LUMBER_MILL, 0, (16, 4), 0).unwrap();
            let p = w.spawn(0, 0, (10, 10), 0).unwrap();
            let pi = w.index(p).unwrap();
            w.units[pi].carrying = true;
            w.units[pi].gather = Gather::Lumber;
            w.units[pi].blocked = 5;
            w.units[pi].order = Order::ReturnGoods { depot: Some(mill) };
            let mi = w.index(mill).unwrap();
            w.kill(mi);
            let pi = w.index(p).unwrap();
            w.return_order(pi, Some(mill));
            let u = &w.units[pi];
            assert_eq!((u.order, u.blocked), (Order::ReturnGoods { depot: Some(hall) }, 5));
            let first = w.fingerprint();
            // No depot left, a remembered patrol: the pending idle resumes it.
            w.units[pi].remembered = Some(crate::world::Remembered::Patrol {
                to: (20, 10),
                back: (10, 10),
            });
            w.units[pi].resume = 0;
            let hi = w.index(hall).unwrap();
            w.kill(hi);
            let pi = w.index(p).unwrap();
            w.return_order(pi, Some(hall));
            let u = &w.units[pi];
            assert!(matches!(u.order, Order::Patrol { .. }), "{:?}", u.order);
            assert_eq!(u.blocked, 0, "the resumed patrol starts afresh");
            assert_eq!([first, w.fingerprint()], [6996958735745534602, 5472781868606498703]);
        }

        /// Order 24 with no route: the generic no-route idle and the same-update idle logic, with no reaction first, so
        /// the idle logic still finds the attacker (a fighter it can't take on alone here) and flees.
        #[test]
        fn a_return_with_no_route_goes_idle_at_once() {
            let mut w = arena();
            let hall = w.spawn(kind::TOWN_HALL, 0, (2, 2), 0).unwrap();
            let p = w.spawn(0, 0, (12, 12), 0).unwrap();
            wall_round(&mut w, (12, 12));
            attacker(&mut w, p, FOE);
            let pi = w.index(p).unwrap();
            w.units[pi].carrying = true;
            w.units[pi].gather = Gather::Gold;
            w.units[pi].order = Order::ReturnGoods { depot: Some(hall) };
            w.start_anim(pi, anim::MOVE);
            w.return_order(pi, Some(hall));
            let u = &w.units[pi];
            assert!(matches!(u.order, Order::Move { .. }), "{:?}", u.order);
            assert_eq!((u.pending, u.attacked_by), (None, None));
            assert_eq!(w.fingerprint(), 10587118783921462611);
        }

        /// "Lumber", step 4: a chopping worker whose tile is no longer reserved drops the reservation and is sent to the
        /// nearest tree (here the same one, a forest tile again).
        #[test]
        fn a_chopping_worker_whose_tree_is_unreserved_looks_for_another() {
            let mut w = arena();
            forest(&mut w, 10, 8);
            forest(&mut w, 12, 8);
            let p = w.spawn(0, 0, (10, 9), 0).unwrap();
            let i = harvesting(&mut w, p, None, (10, 8), Gather::Lumber);
            w.units[i].order = Order::Harvest {
                source: None,
                tree: (10, 8),
                chopping: true,
            };
            w.units[i].chop = 7;
            w.harvest_order(i, None, (10, 8), true);
            let u = &w.units[i];
            assert_eq!(
                u.order,
                Order::Harvest {
                    source: None,
                    tree: (10, 8),
                    chopping: false
                }
            );
            assert_eq!((u.chop, u.script.anim, u.script.wait), (0, anim::ATTACK, 3));
            assert_eq!(w.terrain.region(10, 8), Some(region::FOREST));
            assert_eq!(w.fingerprint(), 5614337015796615819);
        }

        /// "Coming out", facing: an order 28 with no stored goal (none is made now; `build_command` stores one) faces
        /// the building goal worked out from the exit tile.
        #[test]
        fn a_build_order_without_a_stored_goal_faces_a_goal_worked_out_outside() {
            let mut w = field(32, 32);
            w.stats.0[0].flags |= flags::PEON;
            w.stats.0[kind::FARM as usize].size = (2, 2);
            w.stats.0[kind::FARM as usize].flags |= flags::BUILDING;
            let p = w.spawn(0, 0, (20, 20), 0).unwrap();
            let i = w.index(p).unwrap();
            w.units[i].tile = (13, 10);
            let next = Some(Order::Build {
                kind: kind::FARM,
                site: (10, 10),
                goal: None,
            });
            assert_eq!(w.come_out_face(i, next, Some((10, 10))), Some((11, 10)));
        }

        /// "Coming out", step 5, toward a source whose slot is gone (released, not a ruin): as the code stands the
        /// harvest command is given with no target unit at the unit's own tile, so a worker gets lumber mode there and
        /// a tanker order 23 with no target. The second reviewer's note reads the command check as dropping the gone
        /// target with a pending idle instead (static study; no trace has it).
        #[test]
        fn coming_out_toward_a_source_whose_slot_is_gone() {
            let mut prints = vec![];
            for tanker in [false, true] {
                let mut w = arena();
                let (src_kind, worker) = if tanker { (PLATFORM, TANKER) } else { (MINE, 0) };
                let hall = w.spawn(kind::TOWN_HALL, 0, (4, 12), 0).unwrap();
                let src = w.spawn(src_kind, 0, (16, 4), 100).unwrap();
                let si = w.index(src).unwrap();
                w.kill(si);
                assert!((0..5000).any(|_| {
                    w.step(&[]);
                    w.unit(src).is_none()
                }));
                let p = w.spawn(worker, 0, (3, 12), 0).unwrap();
                go_in_now(&mut w, p, hall);
                let pi = w.index(p).unwrap();
                w.units[pi].return_to = Some(Source::Unit(src));
                w.units[pi].carrying = false;
                come_out_now(&mut w, p, hall);
                let u = w.unit(p).unwrap();
                assert!(
                    matches!(u.pending, Some(Order::Harvest { source: None, tree, .. }) if tree == (4, 12)),
                    "{tanker}: {:?}",
                    u.pending
                );
                assert_eq!(u.gather, if tanker { Gather::Gold } else { Gather::Lumber });
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [11047150078815211316, 11680749226269963979]);
        }

        /// "Coming out" with no command and nothing pending: the facing is kept. The spec's table says it faces its
        /// goal tile (the building's top-left tile; static study only, untraced).
        #[test]
        fn coming_out_with_nothing_to_do_keeps_its_facing() {
            let mut w = land_with_shops();
            let b = w.spawn(SHIPYARD, 0, (8, 8), 0).unwrap();
            let p = w.spawn(0, 0, (7, 8), 0).unwrap();
            go_in_now(&mut w, p, b);
            let pi = w.index(p).unwrap();
            w.units[pi].return_to = None;
            w.units[pi].facing = 5;
            come_out_now(&mut w, p, b);
            let u = w.unit(p).unwrap();
            assert_eq!((u.pending, u.facing), (None, 5));
            assert_ne!(crate::world::face_tile(u.tile, (8, 8)), 5);
            assert_eq!(w.fingerprint(), 15173445877571300346);
        }

        /// Order 25 losing its target before stepping in (OQ-A): a pending idle, taken over at once (V82 as written).
        #[test]
        fn entering_a_target_lost_before_stepping_in_takes_over_a_pending_idle() {
            let mut w = arena();
            let mine = w.spawn(MINE, 15, (14, 4), 100).unwrap();
            let p = w.spawn(0, 0, (13, 5), 0).unwrap();
            let pi = w.index(p).unwrap();
            w.units[pi].order = Order::Enter {
                building: mine,
                stepped: false,
            };
            let mi = w.index(mine).unwrap();
            w.kill(mi);
            let pi = w.index(p).unwrap();
            w.enter_order(pi, mine, false);
            let u = &w.units[pi];
            assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, None, 3));
            assert_eq!(w.fingerprint(), 13517207483336127821);
        }

        /// Whole cycles: human workers mining, chopping (with a tree cut from under one of them), a tanker at its
        /// platform, and a computer player's workers, run for 3000 cycles with the state pinned every 250.
        #[test]
        fn harvest_cycles_are_pinned() {
            let mut w = arena();
            w.players[1].controller = controller::COMPUTER;
            let mine = w.spawn(MINE, 15, (16, 2), 4).unwrap();
            w.spawn(kind::TOWN_HALL, 0, (3, 3), 0).unwrap();
            w.spawn(LUMBER_MILL, 0, (3, 14), 0).unwrap();
            w.spawn(SHIPYARD, 0, (14, 14), 0).unwrap();
            for (x, y) in [
                (9, 18),
                (10, 18),
                (11, 18),
                (9, 19),
                (10, 19),
                (11, 19),
                (20, 10),
                (21, 10),
            ] {
                forest(&mut w, x, y);
            }
            let platform = w.spawn(PLATFORM, 0, (19, 18), 3).unwrap();
            let miners = [(10, 4), (11, 6), (12, 8)].map(|t| w.spawn(0, 0, t, 0).unwrap());
            let choppers = [(8, 15), (12, 15)].map(|t| w.spawn(0, 0, t, 0).unwrap());
            let tanker = w.spawn(TANKER, 0, (18, 14), 0).unwrap();
            harvest(&mut w, miners.to_vec(), Some(mine), (16, 2));
            harvest(&mut w, choppers.to_vec(), None, (10, 18));
            harvest(&mut w, vec![tanker], Some(platform), (19, 18));
            let mut prints = vec![];
            for n in 1..=3000 {
                w.step(&[]);
                if n == 900 {
                    // A depot lost under a returning worker: the hall goes.
                    if let Some(h) = w.units.iter().position(|u| u.kind == kind::TOWN_HALL) {
                        w.kill(h);
                    }
                }
                if n % 250 == 0 {
                    prints.push(w.fingerprint());
                }
            }
            assert_eq!(
                prints,
                [
                    7207997613274591782,
                    13476117557655880645,
                    5181640389991292329,
                    8929566194430708829,
                    3387284651410698369,
                    741518362166369064,
                    9344351753719453454,
                    7649109570281920205,
                    14449367595918039853,
                    5599906224186311679,
                    17197182959073010245,
                    12250845477640230121
                ]
            );
        }
    }
}
