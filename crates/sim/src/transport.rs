//! Transports: the command rules, landing, pushing off, unloading and boarding. Spec: `docs/specs/orders/transports.md`,
//! whose "The model" section names the stages: boarding (order 34), docked (32), pushing off (31), sailing (3 and 6),
//! landing (30) and unloading (29). Landing and pushing off are two parameter sets of the ordinary walk
//! ([`NearShore`]).
//!
//! Not yet here: a passenger dying aboard by other means than its transport's death (Enhanced mode empties its slot;
//! nothing but the transport's death kills a passenger yet).
//!
//! Our choices for the spec's open questions: the unload command's tile is used as given; a passenger is put ashore
//! facing as it was; a transport lost while a unit walks to it is step 1 of order 34 (V33, not the static reading in the
//! open question "A transport lost while the unit walks to it").

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::TILE_PX;
use crate::movement::{Goal, NoUsable, Walk, chebyshev, pulled_back};
use crate::path;
use crate::placement::{LIMIT_UNLOAD, Source, ring_walk};
use crate::terrain::{bits, mask};
use crate::world::{Layer, Order, Unit, UnitId, World};

/// Boarding distance on each axis.
const BOARD_REACH: i32 = 2;

/// "Pull-back usable test": the sea blocking mask without the ground-unit bit.
const PULL_BACK_MASK: u16 = mask::SEA & !bits::UNIT;

/// "Shore-target search" limit (4 rings).
const LIMIT_SHORE_TARGET: i32 = 8;

/// The points where landing and pushing off move the transport differently (transports spec, "Moving near the
/// shore"). Both then take the ordinary step, blocked rules and route search.
#[derive(Clone, Copy, Debug)]
struct NearShore {
    /// Movement class: `Some(true)` the landing class, `Some(false)` the sea class, `None` unchanged (pushing off
    /// keeps the landing class after a landing).
    landing_class: Option<bool>,
    /// Step size of this run: two-tile (landing: tile and goal even, or farther than 3 tiles; pushing off: tile even).
    two_tile: bool,
    /// Animation: whether a two-tile run starts the move animation afresh and runs it at once with the step, or lets
    /// a running one run on as any walking unit's step does (landing farther than 3 tiles out; round 70, M3).
    restart: bool,
    /// Whether a run that isn't two-tile makes the transport a one-tile mover (landing) or leaves its step size as it
    /// is (pushing off).
    one_tile_otherwise: bool,
    /// Route: whether the cached route is dropped before the step (landing within 3 tiles: a new route every step).
    fresh_route: bool,
    /// What no route does.
    no_route: NoRoute,
}

/// "Moving near the shore", "No route".
#[derive(Clone, Copy, Debug)]
enum NoRoute {
    /// Landing, step 6.
    LandingStep6,
    /// Pushing off: dock (order 32).
    Dock,
}

fn even(t: (i32, i32)) -> bool {
    t.0 & 1 == 0 && t.1 & 1 == 0
}

impl World {
    pub(crate) fn is_transport(&self, u: &Unit) -> bool {
        self.stats_of(u.kind).flags & flags::TRANSPORT != 0
    }

    fn shore_water(&self, t: (i32, i32)) -> bool {
        self.terrain.contains(t.0, t.1) && self.terrain.word(t.0, t.1) & bits::SHORE != 0
    }

    fn open_water(&self, t: (i32, i32)) -> bool {
        self.terrain.contains(t.0, t.1) && self.terrain.word(t.0, t.1) & bits::WATER != 0
    }

    fn region_at(&self, t: (i32, i32)) -> Option<u16> {
        self.terrain.region(t.0, t.1)
    }

    /// "The straight line": whether the walk from `s` toward `g` meets a land tile of `g`'s region before any
    /// open-water tile, or reaches `g` without meeting either.
    fn land_first(&self, s: (i32, i32), g: (i32, i32)) -> bool {
        let region = self.region_at(g);
        let mut at = s;
        let mut steps = path::line(s, g, 1).into_iter();
        while at != g {
            let w = self.terrain.word(at.0, at.1);
            if w & bits::LAND != 0 && self.region_at(at) == region {
                return true;
            }
            if w & bits::WATER != 0 {
                return false;
            }
            let Some(d) = steps.next() else { break };
            let v = path::DIRECTIONS[d as usize];
            at = (at.0 + v.0, at.1 + v.1);
        }
        true
    }

    /// "Land and shore targets", step 2: the target tile `g` of a transport on open water at `t`, pulled back toward
    /// `t` to shore water, or found by the shore-target ring search around `g`.
    pub(crate) fn shore_target(&self, t: (i32, i32), g: (i32, i32)) -> (i32, i32) {
        // Step 2.1: the pull-back with the transport's settings (pathfinding spec, "One pull-back, two settings").
        let home = self.region_at(t);
        let usable = |at: (i32, i32)| {
            self.terrain.contains(at.0, at.1)
                && self.region_at(at) == home
                && self.terrain.word(at.0, at.1) & PULL_BACK_MASK == 0
        };
        let k = pulled_back(t, g, usable, NoUsable::LastTile);
        // Step 2.2: the shore check, a third test (shore water, no bit of the landing mask, G's region), then the
        // shore-target ring search.
        let region = self.region_at(g);
        let ok = |at: (i32, i32)| {
            self.terrain.contains(at.0, at.1) && {
                let w = self.terrain.word(at.0, at.1);
                w & bits::SHORE != 0 && w & mask::LANDING == 0 && self.region_at(at) == region
            }
        };
        if ok(k) {
            return k;
        }
        ring_walk(
            Source::tile(g.0, g.1),
            g,
            LIMIT_SHORE_TARGET,
            self.terrain.width as i32,
            |x, y| ok((x, y)),
        )
        .unwrap_or(k)
    }

    /// "Land and shore targets": for a target tile `g` with the land or shore-water bit, the target unit is dropped
    /// and, for a transport on open water, the tile is pulled back to shore. This runs for every command given to a
    /// transport, not only the smart move and unload.
    pub(crate) fn land_and_shore(
        &self,
        i: usize,
        g: (i32, i32),
        target: Option<UnitId>,
    ) -> ((i32, i32), Option<UnitId>) {
        let s = self.units[i].tile;
        if !self.terrain.contains(g.0, g.1) || self.terrain.word(g.0, g.1) & (bits::LAND | bits::SHORE) == 0 {
            return (g, target);
        }
        let g = if self.open_water(s) { self.shore_target(s, g) } else { g };
        (g, None)
    }

    /// The goal tile and target unit of a command other than the smart move and unload. Giving a transport any command
    /// records the three things, after the land and shore steps ([`World::record_command`]; round 70, M7: every
    /// command, a computer player's move or attack included). Any other unit's command goes on as given.
    pub(crate) fn command_goal(
        &mut self,
        i: usize,
        target: Option<UnitId>,
        tile: (i32, i32),
    ) -> ((i32, i32), Option<UnitId>) {
        if self.is_transport(&self.units[i]) {
            self.record_command(i, target, tile)
        } else {
            (tile, target)
        }
    }

    /// "Clearing the mark" (round 71, traced: `human12-unload-then-move-900` n = 427, V39): every command a player
    /// gives a transport (from the command stream, a right-click or the buttons) first clears its unload-on-arrival
    /// mark, before anything else the command does; an unload command then sets it again ([`World::transport_command`]).
    /// The computer players' own routines don't come here, and neither does a player's command the transport doesn't
    /// take (round 75, traced: V40): the selection filter refuses it first, so the mark stays.
    pub(crate) fn players_command_clears_mark(&mut self, i: usize) {
        self.units[i].ship.unload = false;
    }

    /// A smart move (or, with `unload`, command 29) given to a transport ("Commands given to a transport"): the three
    /// things recorded with the land and shore steps ([`World::record_command`]), the unload-on-arrival mark, then the
    /// stage table ([`World::stage`]).
    pub(crate) fn transport_command(
        &mut self,
        i: usize,
        target: Option<UnitId>,
        tile: (i32, i32),
        unload: bool,
    ) -> Order {
        let (g, target) = self.record_command(i, target, tile);
        if unload && self.shore_water(g) {
            self.units[i].ship.unload = true;
        }
        self.stage(i, g, target)
    }

    /// The smart-move stage of a command that has already recorded the three things (transports spec, "What the
    /// transport stores": once per command, round 71): command 4 given to a transport, which as a non-combat unit goes
    /// on as a smart move ("Commands", command 4), takes the smart move's wait cap and its stage table on the target
    /// tile `g` the command recorded, without recording again or running the land and shore steps a second time.
    pub(crate) fn smart_stage(&mut self, i: usize, target: Option<UnitId>, g: (i32, i32)) -> Order {
        self.cap_smart_wait(i);
        self.stage(i, g, target)
    }

    /// "What the transport stores" and "Land and shore targets": the target unit, the target tile (the target unit's
    /// tile, or the command's) and the destination, after a land or shore-water target tile has dropped the target
    /// unit and, from open water, been pulled back to shore ([`World::land_and_shore`]). Returns the target tile and
    /// unit the command goes on with.
    fn record_command(&mut self, i: usize, target: Option<UnitId>, tile: (i32, i32)) -> ((i32, i32), Option<UnitId>) {
        let g = target.and_then(|t| self.unit(t)).map_or(tile, |t| t.tile);
        let (g, target) = self.land_and_shore(i, g, target);
        let u = &mut self.units[i];
        u.ship.target = target;
        u.ship.target_tile = g;
        u.ship.dest = g;
        // The stored goal a repair counts in (`economy/repair.md`, "Cost and rate"; traced, RP8): the command's target
        // tile, whatever order the stage then gives (a landing order doesn't carry a goal of its own here).
        u.goal_x = g.0 as i16;
        (g, target)
    }

    /// The stage table of a smart move or the unload command, first match wins, with G = `g` and S the transport's
    /// tile ("Commands given to a transport").
    fn stage(&mut self, i: usize, g: (i32, i32), target: Option<UnitId>) -> Order {
        let s = self.units[i].tile;
        let (shore_s, shore_g) = (self.shore_water(s), self.shore_water(g));
        // S is shore water, G in its region, and the straight line meets land of G's region first: G = S lands at
        // once (V01), anything else stays docked at S (V05, V26).
        if shore_s && self.region_at(g) == self.region_at(s) && self.land_first(s, g) {
            if g == s {
                return Order::Land { goal: s };
            }
            self.units[i].ship.target_tile = s;
            return Order::Docked;
        }
        // Orders 30, 31 and 32, or S shore water: push off first (V04).
        if matches!(self.units[i].order, Order::Land { .. } | Order::PushOff | Order::Docked) || shore_s {
            return Order::PushOff;
        }
        // G not shore water: follow a unit of the transport's own side that isn't a building, else move (V03).
        if !shore_g {
            let owner = self.units[i].owner;
            let own_unit = target
                .and_then(|t| self.unit(t))
                .filter(|t| t.owner == owner && !t.building);
            return match own_unit {
                Some(t) => Order::Follow { target: t.id },
                None => Order::Move { goal: g, target: None },
            };
        }
        // G shore water: land at G (V02, V25).
        Order::Land { goal: g }
    }

    /// Smart move by a land unit on its own side's transport ("Boarding", "The command").
    pub(crate) fn board_command(&mut self, i: usize, transport: UnitId) -> Order {
        if let Some(ti) = self.index(transport) {
            let t = &self.units[ti];
            let landing = matches!(t.order, Order::Land { .. }) || matches!(t.pending, Some(Order::Land { .. }));
            if !landing {
                let (id, tile) = (self.units[i].id, self.units[i].tile);
                let o = self.transport_command(ti, Some(id), tile, false);
                // The transport's smart move is a command: the command steps clear the blocked counter (movement spec),
                // forget a stale attacker and drop the cached route, as every command does (orders spec, "What every
                // command does").
                self.command_steps(ti);
                self.give(ti, o);
            }
        }
        Order::Board { transport }
    }

    /// Whether a smart move by `u` on `t` boards it: a land unit and a transport of its own side.
    pub(crate) fn boards(&self, u: &Unit, t: UnitId) -> bool {
        u.layer == Layer::Land
            && !u.building
            && self
                .unit(t)
                .is_some_and(|t| t.owner == u.owner && t.present() && self.is_transport(t))
    }

    /// The per-step logic of the transport orders. Returns false for other orders.
    pub(crate) fn run_transport(&mut self, i: usize) -> bool {
        let order = self.units[i].order;
        match order {
            Order::Land { goal } => self.landing(i, goal),
            Order::PushOff => self.push_off(i),
            Order::Unload => self.unload(i),
            Order::Board { transport } => self.board(i, transport),
            Order::Docked | Order::Aboard { .. } => {
                if self.units[i].script.anim != anim::STILL {
                    self.switch_anim(i, anim::STILL);
                }
            }
            _ => return false,
        }
        true
    }

    /// Order 30 ("Landing").
    fn landing(&mut self, i: usize, goal: (i32, i32)) {
        // Orders spec, "Order 30's radius": farther than 3 tiles the transport takes ordinary two-tile steps (move
        // animation); within 3 the arrival switch starts animation 6 and the landing logic runs.
        let far = chebyshev(self.units[i].tile, goal) > 3;
        let came_within = {
            let u = &self.units[i];
            !far && !u.ship.landing && u.script.anim == anim::MOVE
        };
        if came_within {
            // The landing logic takes over the moving: the ordinary steps' path is not reused.
            self.switch_anim(i, anim::WORK_B);
            self.units[i].path.clear();
        }
        let tile = self.units[i].tile;
        let landing = NearShore {
            landing_class: Some(!far),
            two_tile: far || (even(tile) && even(goal)),
            // Farther out the steps are ordinary ones, which don't restart the move animation at each step.
            restart: !far,
            one_tile_otherwise: true,
            // Step 5 (`human13-10000` n = 3820, V12): within the 3 tiles every run's arrival test discards the
            // planned route, so each step is planned afresh from the current tile.
            fresh_route: !far,
            no_route: NoRoute::LandingStep6,
        };
        // Step 4: arrived, order 29 if marked unload on arrival, else 32, set at once like an immediate order: the
        // still animation starts with a wait of 1 (orders spec, "Pending orders and immediate orders"; V24: docked at
        // n = 36 with wait 1).
        if self.near_shore(i, goal, landing) == Walk::Arrived {
            let u = &mut self.units[i];
            let next = if u.ship.unload { Order::Unload } else { Order::Docked };
            u.ship.unload = false;
            u.order = next;
            self.start_anim(i, anim::STILL);
        }
    }

    /// "Moving near the shore": one step of landing or pushing off, with `how` setting the points where the two
    /// differ; everything else is the ordinary walk ([`World::move_toward_as`]), its blocked rules restarting
    /// animation 6 ([`World::blocked`]). The end test is the goal tile exactly: radius 0, and no +1 in orders 30 and
    /// 31 ([`World::arrival_radius`]). Pushing off's own end test, an even open-water tile, runs before its tile pick
    /// ([`World::push_off`]).
    fn near_shore(&mut self, i: usize, goal: (i32, i32), how: NearShore) -> Walk {
        let u = &mut self.units[i];
        if let Some(landing) = how.landing_class {
            u.ship.landing = landing;
        }
        if how.two_tile {
            u.ship.one_tile = false;
        } else if how.one_tile_otherwise {
            u.ship.one_tile = true;
        }
        if how.fresh_route {
            u.path.clear();
        }
        // A two-tile run starts the move animation (afresh, if `restart`) and runs it at once with the step; a
        // one-tile run keeps animation 6, the order's own.
        let walk = self.move_toward_as(i, Goal::tile(goal, 0), how.two_tile && how.restart);
        if walk == Walk::NoPath {
            match how.no_route {
                NoRoute::LandingStep6 => self.landing_no_route(i),
                // Pushing off, step 5 (round 70, M4): order 32 as an immediate order (still animation, wait 1).
                NoRoute::Dock => self.immediately(i, Order::Docked),
            }
        }
        walk
    }

    /// "Landing (order 30)", step 6, no route: on shore water order 29 if marked unload on arrival, else 32;
    /// elsewhere on an even tile, the sea class as a two-tile mover (round 75, static study) and idle, so its next
    /// step done finds nothing to change; on an odd tile, 32. Each is set at once, still animation
    /// with wait 1 (open question 4 of the transports spec: the static study reads them as immediate orders too). The
    /// unload mark is cleared.
    fn landing_no_route(&mut self, i: usize) {
        let unload = std::mem::take(&mut self.units[i].ship.unload);
        let tile = self.units[i].tile;
        let next = if self.shore_water(tile) {
            if unload { Order::Unload } else { Order::Docked }
        } else if even(tile) {
            let ship = &mut self.units[i].ship;
            ship.landing = false;
            ship.one_tile = false;
            Order::Idle
        } else {
            Order::Docked
        };
        self.units[i].order = next;
        self.units[i].path.clear();
        self.start_anim(i, anim::STILL);
    }

    /// Order 31 ("Pushing off").
    fn push_off(&mut self, i: usize) {
        let tile = self.units[i].tile;
        if even(tile) && self.open_water(tile) {
            let u = &mut self.units[i];
            u.ship.landing = false;
            u.ship.one_tile = false;
            u.order = Order::Idle;
            let (owner, target, dest) = (u.owner, u.ship.target, u.ship.dest);
            let target = target.filter(|&t| !self.target_lost(owner, t));
            let o = self.transport_command(i, target, dest, false);
            // The smart move is a command (round 70, M6): the command steps clear the blocked counter, drop the cached
            // path and forget a stale attacker, as for the smart move a boarding unit gives its transport.
            self.command_steps(i);
            self.take_over(i, o);
            return;
        }
        // Step 2 (round 70, M4): with no tile found, order 32 is made pending and takes over in the same update (still
        // animation, wait 3).
        let Some(to) = self.find_push_off(tile, self.units[i].ship.target_tile) else {
            self.take_over(i, Order::Docked);
            return;
        };
        let u = &mut self.units[i];
        u.ship.target_tile = to;
        u.path.clear();
        u.blocked = 0;
        let owner = u.owner;
        if let Some(t) = u.ship.target
            && self.target_lost(owner, t)
        {
            self.units[i].ship.target = None;
            self.give(i, Order::Idle);
        }
        self.forget_lost_attacker(i);
        // Steps 4 and 5: on an even tile a two-tile mover, on an odd tile the step size unchanged; the class unchanged;
        // the tile pick above dropped the cached route, so the step searches afresh; no route docks.
        let pushing_off = NearShore {
            landing_class: None,
            two_tile: even(tile),
            restart: true,
            one_tile_otherwise: false,
            fresh_route: false,
            no_route: NoRoute::Dock,
        };
        self.near_shore(i, to, pushing_off);
    }

    /// Order 29 ("Unloading"): on shore water every passenger from slot 6 down to slot 1 is put ashore; off it, none
    /// is. Either way order 32 then takes over in the same update as a pending order does (still animation, wait 3;
    /// V27; off shore water round 70, M5, untraced).
    fn unload(&mut self, i: usize) {
        if self.shore_water(self.units[i].tile) {
            for slot in (0..self.units[i].passengers.len()).rev() {
                self.unload_one(i, slot);
            }
        }
        self.take_over(i, Order::Docked);
    }

    /// [`Command::UnloadOne`]: puts the passenger in `slot` of `transport` ashore, if `player` owns the transport.
    pub(crate) fn unload_one_command(&mut self, player: u8, transport: UnitId, slot: u8) {
        let Some(i) = self.index(transport) else { return };
        let t = &self.units[i];
        if t.owner != player || t.dying || usize::from(slot) >= t.passengers.len() {
            return;
        }
        self.unload_one(i, usize::from(slot));
    }

    /// Puts the passenger in `slot` ashore, if the transport is unloading or docked and a tile is free.
    pub fn unload_one(&mut self, i: usize, slot: usize) {
        let t = &self.units[i];
        if !matches!(t.order, Order::Unload | Order::Docked) {
            return;
        }
        let Some(p) = t.passengers[slot] else { return };
        let Some(pi) = self.index(p) else { return };
        let tile = t.tile;
        let class = self.layer_of(self.units[pi].kind).into();
        let region = self.region_at(tile);
        let Some(to) = self.find_place_class(class, Source::tile(tile.0, tile.1), tile, LIMIT_UNLOAD, region) else {
            return;
        };
        let u = &mut self.units[pi];
        u.tile = to;
        u.pos = (to.0 * TILE_PX, to.1 * TILE_PX);
        u.inside = false;
        u.order = Order::Idle;
        u.pending = None;
        u.commanded = false;
        u.path.clear();
        let unit = self.units[pi].clone();
        self.mark(&unit, to, Some(unit.id));
        // An immediate idle (V27 to V29): still animation with a wait of 1, no turn, and the idle logic not run
        // here. The passenger's own update, later in the cycle, runs it.
        self.start_anim(pi, anim::STILL);
        // Vision spec, "When sight is applied" (C3): a passenger put ashore reveals nothing as it lands; its first
        // movement step or the next fog refresh does.
        self.units[i].passengers[slot] = None;
    }

    /// The boarding walk of unit `i` to the transport at `ti` ("Order 34 (board)", step 2): the transport's tile when
    /// it is in the unit's region, else its destination tile, with arrival radius 1. A special case of the one walk
    /// goal ([`World::target_walk`]): the goal isn't always the target's tile, but the route search still has the
    /// transport as its target unit, so its goal zone is the transport's footprint widened by one tile (pathfinding
    /// spec, "Step 1"; V15).
    fn board_walk(&self, i: usize, ti: usize) -> Goal {
        let (t, u) = (&self.units[ti], &self.units[i]);
        let goal = if self.region_at(t.tile) == self.region_at(u.tile) {
            t.tile
        } else {
            t.ship.dest
        };
        Goal {
            zone: Some(crate::battle::goal_zone(t)),
            ..Goal::tile(goal, 1)
        }
    }

    /// Order 34 ("Order 34 (board)"): a walking order whose logic runs when the walk has ended.
    fn board(&mut self, i: usize, transport: UnitId) {
        // Step 1 (static study only; V33, which governs over the static reading in the open question "A
        // transport lost while the unit walks to it"): a transport that
        // is dying or gone, inside something or no longer detected by the unit's owner is forgotten, and so is the
        // recorded attacker if it is lost in the same way; idle becomes the pending order and takes over in the same
        // update. No reaction.
        let owner = self.units[i].owner;
        let Some(ti) = self.index(transport).filter(|_| !self.target_lost(owner, transport)) else {
            if self.units[i].attacked_by.is_some_and(|a| self.target_lost(owner, a)) {
                self.units[i].attacked_by = None;
            }
            self.idle_now(i);
            return;
        };
        let walk = self.board_walk(i, ti);
        let (t, u) = (&self.units[ti], &self.units[i]);
        let here = self.region_at(u.tile);
        // While the unit walks only the movement runs, so a unit of any player stays in order 34 (V10, V11). With no
        // route it restarts order 34 at once (still animation, wait 3) if the transport's destination tile is in its
        // region, and becomes idle at once otherwise.
        let dest_here = self.region_at(t.ship.dest) == here;
        let human = self.human_controlled(u.owner);
        let computer = self.computer_player(u.owner);
        // The reaction runs only in step 5: a move toward the transport, no route and boarding make none, so a unit
        // with an attacker recorded boards and doesn't turn on it (V31).
        match self.move_toward(i, walk) {
            Walk::Arrived => {}
            Walk::NoPath if dest_here => {
                // The restart: for a human player's unit it also tells the transport to come again unless it is
                // landing (V16 to V18); whether a computer player's does is open, so it doesn't.
                let o = if human {
                    self.board_command(i, transport)
                } else {
                    Order::Board { transport }
                };
                self.take_over(i, o);
                // The restarted logic runs in the same update and reaches step 3 (not arrived). A computer player's
                // unit gets command 4 on the transport, which takes over at once (V14, V34). The reviewers' note asks
                // to follow V10 (order 34 kept, wait 3) for a computer player's unit instead; V10 and V14 disagree,
                // and the traces pass as it is, so this stays a question for the spec.
                if computer {
                    let tile = self.units[i].tile;
                    self.command_4(i, Some(transport), tile);
                    if let Some(o) = self.units[i].pending.take() {
                        self.take_over(i, o);
                    }
                } else {
                    // Every other player's unit, passive and rescue included, gets the smart move, the boarding command
                    // again (static study): order 34 again, taken over in the same update with no visible effect,
                    // so the net result is the transport's call and the smart move's wait cap.
                    self.board_command(i, transport);
                    self.cap_smart_wait(i);
                }
                return;
            }
            Walk::NoPath => {
                self.idle_at_once(i);
                return;
            }
            Walk::Stepping | Walk::Blocked => return,
        }
        let (t, u) = (&self.units[ti], &self.units[i]);
        let close = (u.tile.0 - t.tile.0).abs() <= BOARD_REACH && (u.tile.1 - t.tile.1).abs() <= BOARD_REACH;
        let slot = t.passengers.iter().position(Option::is_none);
        if let (true, Order::Docked, Some(slot)) = (close, t.order, slot) {
            let unit = self.units[i].clone();
            let (tile, pos) = (self.units[ti].tile, self.units[ti].pos);
            self.mark(&unit, unit.tile, None);
            let u = &mut self.units[i];
            u.tile = tile;
            u.pos = pos;
            u.inside = true;
            u.attacked_by = None;
            u.order = Order::Aboard { transport };
            u.pending = None;
            u.path.clear();
            self.units[ti].passengers[slot] = Some(unit.id);
            self.start_anim(i, anim::STILL);
            self.boarded(i);
            return;
        }
        let dest_here = self.region_at(t.ship.dest) == here;
        let landing_here = matches!(t.order, Order::Land { .. }) && self.region_at(t.tile) == here;
        if landing_here && self.computer_player(u.owner) {
            // Step 5 (V13): a random nearby tile, x drawn first, each offset (r mod 12) - 6 and clamped to the map's
            // size (one size for both axes, so the width); a move there with no target unit takes over at once.
            let (here, w) = (u.tile, self.terrain.width as i32);
            let x = here.0 + (self.rng.draw() % 12) as i32 - 6;
            let y = here.1 + (self.rng.draw() % 12) as i32 - 6;
            let to = (x.clamp(0, w - 1), y.clamp(0, w - 1));
            self.smart_move(i, None, to);
            if let Some(o) = self.units[i].pending.take() {
                self.take_over(i, o);
            }
        } else if dest_here {
            // Step 5 (static study only; V07, V32): the reaction to an attacker runs, and with no new order the unit
            // keeps waiting.
            self.units[i].path.clear();
            if self.react_at_walk_end(i) {
                return;
            }
            if self.units[i].script.anim != anim::STILL {
                self.switch_anim(i, anim::STILL);
            }
        } else {
            // Step 5 (V08, V38): giving up makes idle the pending order, taken over in the same update (still
            // animation, wait 3).
            self.idle_now(i);
        }
    }

    /// Kills every passenger of a dying transport ("Passengers and the transport's death").
    pub(crate) fn kill_passengers(&mut self, i: usize) {
        let passengers = std::mem::take(&mut self.units[i].passengers);
        for p in passengers.into_iter().flatten() {
            if let Some(pi) = self.index(p)
                && !self.units[pi].dying
            {
                self.kill(pi);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tidewater_formats::udta::flags;

    use super::*;
    use crate::Terrain;
    use crate::ai::controller;
    use crate::terrain::region;
    use crate::world::tests::{run, scripts, stats, tileset};

    const SHIP: u8 = 0x1a;
    const FOOTMAN: u8 = 0;
    const LAND_REGION: u16 = region::LAND_AREA | 1;
    const SEA_REGION: u16 = 2;

    /// A 48 x 32 map: land for x < `shore`, shore water (in the land region, as coasts are) at `shore` and
    /// `shore` + 1, open water beyond.
    fn sea(shore: i32) -> World {
        coast(|x| match x {
            c if c < shore => bits::LAND,
            c if c < shore + 2 => bits::SHORE,
            _ => bits::WATER,
        })
    }

    /// A 48 x 32 map whose terrain word depends on the column only; water tiles are in the sea region, the others
    /// (shore water included, as coasts are) in the land region.
    fn coast(column: impl Fn(i32) -> u16) -> World {
        let (w, h) = (48i32, 32i32);
        let words: Vec<u16> = (0..w * h).map(|n| column(n % w)).collect();
        let regions: Vec<u16> = words
            .iter()
            .map(|&t| if t == bits::WATER { SEA_REGION } else { LAND_REGION })
            .collect();
        let tiles = vec![0u16; (w * h) as usize];
        // The ship gets its own animation table whose animation 6 is the still loop, as in the game's data, where it
        // is a "step done" every few cycles.
        let mut b = scripts().bytes().to_vec();
        let table = b.len();
        let shared = u16::from_le_bytes([b[SHIP as usize * 2], b[SHIP as usize * 2 + 1]]) as usize;
        let mut entries = b[shared..shared + 14].to_vec();
        let still = [entries[4], entries[5]];
        entries[12..14].copy_from_slice(&still);
        b.extend(entries);
        b[SHIP as usize * 2..SHIP as usize * 2 + 2].copy_from_slice(&(table as u16).to_le_bytes());
        let mut world = World::new(
            Terrain::from_map(
                &tileset(),
                &tiles,
                Some(&words),
                Some(&regions),
                w as u16,
                h as u16,
                false,
            ),
            stats(),
            std::sync::Arc::new(tidewater_formats::script::Scripts::parse(b).unwrap()),
            0,
        );
        world.players[0].controller = controller::HUMAN;
        world.players[1].controller = controller::COMPUTER;
        world.stats.0[SHIP as usize] = world.stats.0[2];
        world.stats.0[SHIP as usize].flags = flags::SEA | flags::TRANSPORT;
        world.stride[SHIP as usize] = 2;
        world
    }

    fn ship(w: &mut World, tile: (i32, i32), order: Order) -> usize {
        let id = w.spawn(SHIP, 0, (40, 0), 0).unwrap();
        let i = w.index(id).unwrap();
        let unit = w.units[i].clone();
        w.mark(&unit, unit.tile, None);
        let u = &mut w.units[i];
        u.tile = tile;
        u.pos = (tile.0 * TILE_PX, tile.1 * TILE_PX);
        u.order = order;
        u.ship.landing = tile.0 & 1 == 1;
        u.ship.one_tile = tile.0 & 1 == 1;
        let unit = w.units[i].clone();
        w.mark(&unit, tile, Some(unit.id));
        i
    }

    fn smart(w: &mut World, i: usize, tile: (i32, i32)) -> Order {
        let id = w.units[i].id;
        let c = crate::Command::Smart {
            player: 0,
            units: vec![id],
            target: None,
            tile,
        };
        w.command_order(i, &c).unwrap()
    }

    #[test]
    fn straight_line_tiles() {
        let w = sea(30);
        let mut at = (20, 20);
        let mut seen = vec![at];
        for d in path::line((20, 20), (25, 22), 1) {
            let v = path::DIRECTIONS[d as usize];
            at = (at.0 + v.0, at.1 + v.1);
            seen.push(at);
        }
        seen.pop();
        assert_eq!(seen, [(20, 20), (21, 21), (22, 21), (23, 21), (24, 22)]);
        assert!(w.land_first((20, 20), (25, 22)));
    }

    #[test]
    fn a_far_landing_takes_ordinary_two_tile_steps_until_within_3() {
        // The human12-transport vector mirrored: from (36, 14) to the shore tile (32, 9) (4 and 5 tiles away) the first step is an
        // ordinary two-tile step north-west in the move animation; within 3 tiles animation 6 carries one-tile steps.
        let mut w = sea(31);
        let i = ship(&mut w, (36, 14), Order::Idle);
        let id = w.units[i].id;
        let goal = (32, 9);
        w.give(i, Order::Land { goal });
        let mut tiles = vec![(36, 14)];
        for _ in 0..400 {
            w.step(&[]);
            let u = w.unit(id).unwrap();
            if u.tile != *tiles.last().unwrap() {
                let far = chebyshev(*tiles.last().unwrap(), goal) > 3;
                let want = if far { anim::MOVE } else { anim::WORK_B };
                assert_eq!(u.script.anim, want, "step to {:?}", u.tile);
                tiles.push(u.tile);
            }
            if u.pending.is_none() && !matches!(u.order, Order::Land { .. }) {
                break;
            }
        }
        assert_eq!(tiles[1], (34, 12), "{tiles:?}");
        // Within 3 tiles: a one-tile diagonal step (the trace's (70, 32) to (71, 31)), not the two-tile path's next step.
        assert_eq!(tiles[2], (33, 11), "{tiles:?}");
        assert_eq!(*tiles.last().unwrap(), goal, "{tiles:?}");
    }

    /// Transports spec, "Landing" step 5 (round-53 errata, `human13-10000` transport 1389, n = 3820): within 3 tiles
    /// of its goal the landing transport discards its planned route at every step and plans afresh from its tile;
    /// farther out it follows the route it has.
    #[test]
    fn a_landing_transport_within_3_tiles_plans_every_step() {
        for (from, goal, stale, want) in [
            ((23, 10), (21, 10), (22, 11), (22, 10)),
            ((30, 10), (21, 10), (28, 12), (28, 12)),
        ] {
            let mut w = sea(20);
            let i = ship(&mut w, from, Order::Land { goal });
            w.units[i].ship.landing = chebyshev(from, goal) <= 3;
            w.units[i].script.anim = if w.units[i].ship.landing {
                anim::WORK_B
            } else {
                anim::MOVE
            };
            w.units[i].path = vec![stale];
            w.landing(i, goal);
            assert_eq!(w.units[i].tile, want, "from {from:?}");
        }
    }

    #[test]
    fn landing_on_one_tile_steps_keeps_animation_6() {
        // Transports spec, round-33 vector (Human04 transport 1543, landing at (70, 87) from (68, 88)): the goal has an
        // odd coordinate, so the steps are one-tile steps carried by animation 6, the order's own; then order 32. A
        // two-tile landing starts the move animation instead.
        for (goal, steps_anim) in [((21, 11), anim::WORK_B), ((22, 10), anim::MOVE)] {
            let mut w = sea(20);
            let i = ship(&mut w, (26, 12), Order::Idle);
            let id = w.units[i].id;
            w.give(i, Order::Land { goal });
            let (mut tile, mut stepped, mut landing) = ((26, 12), 0, false);
            for _ in 0..300 {
                w.step(&[]);
                let u = w.unit(id).unwrap();
                let now = matches!(u.order, Order::Land { .. });
                if landing && !now {
                    break;
                }
                landing = now;
                if u.tile != tile {
                    // Round 36: farther than 3 tiles the steps are ordinary two-tile steps.
                    let want = if chebyshev(tile, goal) > 3 {
                        anim::MOVE
                    } else {
                        steps_anim
                    };
                    tile = u.tile;
                    stepped += 1;
                    assert_eq!(u.script.anim, want, "{goal:?} at {tile:?}");
                }
            }
            let u = w.unit(id).unwrap();
            assert!(stepped >= 2, "{goal:?}: {stepped} steps, {:?} at {:?}", u.order, u.tile);
            assert_eq!((u.order, u.tile), (Order::Docked, goal));
        }
    }

    #[test]
    fn commands_at_sea() {
        let mut w = sea(20);
        let i = ship(&mut w, (30, 10), Order::Idle);
        assert_eq!(smart(&mut w, i, (21, 12)), Order::Land { goal: (21, 12) });
        assert_eq!(
            smart(&mut w, i, (40, 12)),
            Order::Move {
                goal: (40, 12),
                target: None
            }
        );
    }

    #[test]
    fn docked_and_sent_inland_stays_docked() {
        let mut w = sea(20);
        let i = ship(&mut w, (20, 10), Order::Docked);
        assert_eq!(smart(&mut w, i, (15, 10)), Order::Docked);
        assert_eq!(
            (w.units[i].ship.target_tile, w.units[i].ship.dest),
            ((20, 10), (15, 10))
        );
    }

    /// The Human04 vector of "Land and shore targets", moved 60 tiles up and left: open water for x < 10, shore water
    /// at x = 10, land beyond.
    fn east_coast() -> World {
        coast(|x| match x {
            c if c < 10 => bits::WATER,
            10 => bits::SHORE,
            _ => bits::LAND,
        })
    }

    fn smart_on(w: &mut World, i: usize, target: UnitId) -> Order {
        let c = crate::Command::Smart {
            player: 0,
            units: vec![w.units[i].id],
            target: Some(target),
            tile: w.unit(target).unwrap().tile,
        };
        w.command_order(i, &c).unwrap()
    }

    #[test]
    fn a_land_target_from_open_water_is_pulled_back_to_the_shore() {
        let mut w = east_coast();
        // The target first: spawning shifts the unit indices.
        let hall = w.spawn(FOOTMAN, 0, (21, 24), 0).unwrap();
        let i = ship(&mut w, (8, 28), Order::Idle);
        // The walk back from (21, 24): (20, 24), (19, 25), (18, 25), (17, 25), (16, 26), (15, 26), (14, 26), (13, 27),
        // (12, 27), (11, 27), (10, 27), (9, 28). (9, 28) is the first usable tile (open water, the ship's region), so
        // K = (10, 27): shore water, landing mask clear, (21, 24)'s region.
        assert_eq!(w.shore_target((8, 28), (21, 24)), (10, 27));
        assert_eq!(smart_on(&mut w, i, hall), Order::Land { goal: (10, 27) });
        let ship = &w.units[i].ship;
        assert_eq!((ship.target, ship.target_tile, ship.dest), (None, (10, 27), (10, 27)));
    }

    #[test]
    fn a_docked_transport_sent_to_the_same_land_target_stays_docked() {
        let mut w = east_coast();
        // The target first: spawning shifts the unit indices.
        let hall = w.spawn(FOOTMAN, 0, (21, 24), 0).unwrap();
        let i = ship(&mut w, (10, 27), Order::Docked);
        // Not on open water, so G stays (21, 24); S is shore water in G's region and the line meets land first.
        assert_eq!(smart_on(&mut w, i, hall), Order::Docked);
        let ship = &w.units[i].ship;
        assert_eq!((ship.target, ship.target_tile, ship.dest), (None, (10, 27), (21, 24)));
    }

    #[test]
    fn a_shore_target_out_of_line_is_found_by_the_ring_search() {
        // No shore water on the line: the pull-back K is a land tile, so the ring search around G finds shore water.
        let mut w = coast(|x| match x {
            c if c < 10 => bits::WATER,
            _ => bits::LAND,
        });
        for y in 0..32 {
            let n = (y * 48 + 10) as usize;
            w.terrain.words[n] = if y == 22 { bits::SHORE } else { bits::LAND };
        }
        // Walk back from (12, 20): (11, 20), (10, 20), then (9, 20) is usable, so K = (10, 20), which is land. The
        // ring search around (12, 20) finds the only shore tile within 4 rings.
        assert_eq!(w.shore_target((8, 20), (12, 20)), (10, 22));
        // Out of its reach (limit 8, 4 rings), K stays.
        assert_eq!(w.shore_target((8, 14), (16, 14)), (10, 14));
    }

    #[test]
    fn pushing_off_from_an_odd_tile() {
        let mut w = sea(20);
        let i = ship(&mut w, (21, 20), Order::Docked);
        assert_eq!(smart(&mut w, i, (40, 20)), Order::PushOff);
        assert_eq!(w.find_push_off((21, 20), (40, 20)), Some((22, 20)));
        let id = w.units[i].id;
        w.give(i, Order::PushOff);
        for _ in 0..200 {
            w.step(&[]);
            if matches!(w.unit(id).unwrap().order, Order::Move { .. }) {
                break;
            }
        }
        let u = w.unit(id).unwrap();
        assert_eq!(
            u.order,
            Order::Move {
                goal: (40, 20),
                target: None
            }
        );
        assert_eq!(u.tile, (22, 20));
        assert!(!u.ship.landing && !u.ship.one_tile);
    }

    /// Orders spec, "Taking over" (round 71, V275): a transport whose pending idle takes over settles on an even tile
    /// that isn't shore water (order 2, sea class, two-tile mover) and otherwise docks (order 32): still animation,
    /// wait 3 either way. An idle transport's step done makes the same check before its idle logic.
    #[test]
    fn a_pending_idle_settles_or_docks_a_transport() {
        for (tile, want) in [
            ((31, 11), Order::Docked),
            ((30, 11), Order::Docked),
            ((20, 10), Order::Docked),
            ((30, 10), Order::Idle),
        ] {
            let mut w = sea(20);
            let i = ship(
                &mut w,
                tile,
                Order::Move {
                    goal: (40, 20),
                    target: None,
                },
            );
            let u = &mut w.units[i];
            (u.ship.landing, u.ship.one_tile) = (true, true);
            w.idle_now(i);
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait),
                (want, None, anim::STILL, 3),
                "{tile:?}"
            );
            if want == Order::Idle {
                assert!(!u.ship.landing && !u.ship.one_tile);
            }
            // The step done of an idle transport: a constructed state, idle on that tile.
            let u = &mut w.units[i];
            (u.order, u.ship.landing, u.ship.one_tile) = (Order::Idle, true, true);
            u.script.wait = 1;
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!(u.order, want, "{tile:?}");
            assert_eq!(!u.ship.landing && !u.ship.one_tile, want == Order::Idle, "{tile:?}");
        }
    }

    /// "Clearing the mark" (V39): a player's command clears the unload-on-arrival mark before anything else, so a
    /// smart move after an unload to the same shore tile lands without unloading; an unload command sets it again. A
    /// computer player's routine (command 4 here) leaves it.
    #[test]
    fn a_players_command_clears_the_unload_mark() {
        let mut w = sea(20);
        let t = ship(&mut w, (24, 10), Order::Idle);
        let tid = w.units[t].id;
        let unload = crate::Command::Unload {
            player: 0,
            units: vec![tid],
            tile: (20, 10),
        };
        let smart = crate::Command::Smart {
            player: 0,
            units: vec![tid],
            target: None,
            tile: (20, 10),
        };
        let stop = crate::Command::Stop {
            player: 0,
            units: vec![tid],
        };
        let mark = |w: &World| w.unit(tid).unwrap().ship.unload;
        w.turn(std::slice::from_ref(&unload));
        assert!(mark(&w));
        w.turn(std::slice::from_ref(&smart));
        assert!(!mark(&w), "the smart move cleared it");
        assert!(matches!(w.unit(tid).unwrap().pending, Some(Order::Land { .. })));
        w.turn(&[unload.clone(), stop]);
        assert!(!mark(&w), "a stop clears it too");
        w.turn(&[unload]);
        let ti = w.index(tid).unwrap();
        w.command_4(ti, None, (20, 10));
        assert!(mark(&w), "a computer routine's command keeps it");
    }

    /// "Clearing the mark" (round 75, V40, `human12-unload-then-refused-goals-900`): a player's command a transport
    /// doesn't take (command 4 from the buttons, or an attack-move, a patrol, a stand ground, an attack or a ground
    /// attack from the command stream) is refused before anything changes: the mark stays, the landing goes on, and
    /// the transport still unloads on arrival.
    #[test]
    fn a_refused_command_keeps_the_unload_mark() {
        let mut w = sea(20);
        let t = ship(&mut w, (24, 10), Order::Idle);
        let tid = w.units[t].id;
        let (player, units) = (0, vec![tid]);
        let mate = w.spawn(SHIP, 0, (40, 20), 0).unwrap();
        w.turn(&[crate::Command::Unload {
            player,
            units: units.clone(),
            tile: (20, 10),
        }]);
        w.step(&[]);
        let before = w.unit(tid).unwrap().clone();
        assert!(before.ship.unload);
        let refused = [
            crate::Command::Select {
                player,
                units: units.clone(),
            },
            crate::Command::Issue {
                player,
                command: 4,
                target: None,
                tile: (20, 10),
            },
            crate::Command::AttackMove {
                player,
                units: units.clone(),
                to: (20, 10),
            },
            crate::Command::Patrol {
                player,
                units: units.clone(),
                to: (20, 10),
            },
            crate::Command::StandGround {
                player,
                units: units.clone(),
            },
            crate::Command::Attack {
                player,
                units: units.clone(),
                target: mate,
            },
            crate::Command::AttackGround {
                player,
                units: units.clone(),
                tile: (20, 10),
            },
        ];
        w.turn(&refused);
        let u = w.unit(tid).unwrap();
        assert!(u.ship.unload, "the mark stays");
        let key = |u: &crate::Unit| (u.order, u.pending, u.ship.target_tile, u.ship.dest, u.attacked_by);
        assert_eq!(key(u), key(&before), "nothing about the transport changes");
        // It lands as told and unloads (order 29), where a cleared mark would dock it (order 32).
        let mut unloaded = false;
        for _ in 0..200 {
            w.step(&[]);
            if w.unit(tid).unwrap().order == Order::Unload {
                unloaded = true;
                break;
            }
        }
        assert!(unloaded, "{:?}", w.unit(tid).unwrap().order);
    }

    /// `Command::UnloadOne` puts one passenger ashore for the transport's owner only, and ignores a bad slot.
    #[test]
    fn the_unload_one_command_is_owner_checked() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        let p = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
        let pi = w.index(p).unwrap();
        let unit = w.units[pi].clone();
        w.mark(&unit, unit.tile, None);
        w.units[pi].inside = true;
        w.units[pi].order = Order::Aboard { transport: tid };
        let ti = w.index(tid).unwrap();
        w.units[ti].passengers[2] = Some(p);
        let one = |player, slot| crate::Command::UnloadOne {
            player,
            unit: tid,
            slot,
        };
        // Another player's request, a slot past the end and an empty slot do nothing.
        w.turn(&[one(1, 2), one(0, 200), one(0, 3)]);
        assert_eq!(w.unit(tid).unwrap().passengers[2], Some(p));
        w.turn(&[one(0, 2)]);
        assert_eq!(w.unit(tid).unwrap().passengers[2], None);
        assert!(!w.unit(p).unwrap().inside);
    }

    /// Vision spec, "When sight is applied" (C3): a passenger put ashore, by order 29 or by `Command::UnloadOne`,
    /// reveals nothing as it lands. The next fog refresh (or its first step) does.
    #[test]
    fn a_passenger_put_ashore_reveals_nothing_as_it_lands() {
        for one_command in [false, true] {
            let mut w = sea(20);
            w.players[0].controller = crate::ai::controller::HUMAN;
            let t = ship(&mut w, (20, 10), Order::Unload);
            let tid = w.units[t].id;
            let p = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
            let pi = w.index(p).unwrap();
            let unit = w.units[pi].clone();
            w.mark(&unit, unit.tile, None);
            w.units[pi].inside = true;
            w.units[pi].order = Order::Aboard { transport: tid };
            let ti = w.index(tid).unwrap();
            w.units[ti].passengers[1] = Some(p);
            w.vision.seen.fill(0xff);
            if one_command {
                w.units[ti].order = Order::Docked;
                w.turn(&[crate::Command::UnloadOne {
                    player: 0,
                    unit: tid,
                    slot: 1,
                }]);
            } else {
                w.unload(ti);
            }
            let u = w.unit(p).unwrap();
            assert!(!u.inside, "one_command {one_command}");
            assert!(
                w.vision.seen.iter().all(|&b| b == 0xff),
                "one_command {one_command}: landing revealed"
            );
            // The next refresh reveals from the passenger's tile.
            let tile = u.tile;
            w.vision.countdown = 1;
            w.step(&[]);
            assert!(
                !w.vision.unseen_by(0, tile.0, tile.1),
                "one_command {one_command}: the refresh reveals"
            );
        }
    }

    #[test]
    fn unloading_goes_from_the_last_slot() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        let mut ids = Vec::new();
        for slot in [0, 1, 4, 5] {
            let p = w.spawn(FOOTMAN, 0, (2, slot as i32 * 2), 0).unwrap();
            let pi = w.index(p).unwrap();
            let unit = w.units[pi].clone();
            w.mark(&unit, unit.tile, None);
            let u = &mut w.units[pi];
            u.inside = true;
            u.order = Order::Aboard { transport: tid };
            let ti = w.index(tid).unwrap();
            w.units[ti].passengers[slot] = Some(p);
            ids.push(p);
        }
        let c = crate::Command::Unload {
            player: 0,
            units: vec![tid],
            tile: (20, 10),
        };
        let ti = w.index(tid).unwrap();
        assert_eq!(w.command_order(ti, &c), Some(Order::Land { goal: (20, 10) }));
        assert!(w.units[ti].ship.unload);
        let first = w.find_place_class(
            crate::placement::PlaceClass::Land,
            Source::tile(20, 10),
            (20, 10),
            LIMIT_UNLOAD,
            Some(LAND_REGION),
        );
        w.step(&[c]);
        run(&mut w, 40);
        let t = w.unit(tid).unwrap();
        assert_eq!((t.order, t.passengers), (Order::Docked, [None; 6]));
        // Slot 6 is put ashore first, so it takes the first tile of the ring search.
        assert_eq!(Some(w.unit(ids[3]).unwrap().tile), first);
        for p in &ids {
            let u = w.unit(*p).unwrap();
            assert!(!u.inside && u.order == Order::Idle, "{u:?}");
            assert_eq!(w.occupant(Layer::Land, u.tile.0, u.tile.1), Some(*p));
            assert!(chebyshev(u.tile, (20, 10)) <= 3);
        }
    }

    /// Transports spec, "Unloading (order 29)", the round-39 `human12-transport` vectors: each passenger put ashore is
    /// an immediate idle (order 2, no pending order, still animation, wait 1) that keeps its facing and hasn't run its
    /// idle logic yet (no draw), and order 32 takes over for the transport in the same update with wait 3.
    #[test]
    fn unloading_makes_immediate_idles_and_order_32_takes_over() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Unload);
        let tid = w.units[t].id;
        let mut ids = Vec::new();
        for (slot, facing) in [(0, 5u8), (3, 2)] {
            let p = w.spawn(FOOTMAN, 0, (2, slot as i32 * 2), 0).unwrap();
            let pi = w.index(p).unwrap();
            let unit = w.units[pi].clone();
            w.mark(&unit, unit.tile, None);
            let u = &mut w.units[pi];
            u.inside = true;
            u.facing = facing;
            u.order = Order::Aboard { transport: tid };
            u.pending = Some(Order::Idle);
            let ti = w.index(tid).unwrap();
            w.units[ti].passengers[slot] = Some(p);
            ids.push((p, facing));
        }
        let rng = w.rng.state;
        let ti = w.index(tid).unwrap();
        w.unload(ti);
        let tr = &w.units[ti];
        assert_eq!(
            (tr.order, tr.script.anim, tr.script.wait),
            (Order::Docked, anim::STILL, 3)
        );
        for (p, facing) in ids {
            let u = w.unit(p).unwrap();
            assert!(!u.inside);
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait, u.facing),
                (Order::Idle, None, anim::STILL, 1, facing)
            );
        }
        assert_eq!(w.rng.state, rng, "no idle logic runs at the unload");
    }

    /// Orders spec, "What every command does" (round 68, code Q8): the transport's smart move from a boarding command
    /// drops its cached route and clears its blocked counter.
    #[test]
    fn the_board_command_drops_the_transports_route() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        w.units[t].path = vec![(21, 10)];
        w.units[t].blocked = 3;
        let f = w.spawn(FOOTMAN, 0, (14, 10), 0).unwrap();
        let fi = w.index(f).unwrap();
        assert_eq!(w.board_command(fi, tid), Order::Board { transport: tid });
        let tr = &w.units[w.index(tid).unwrap()];
        assert!(tr.pending.is_some());
        assert_eq!((tr.path.is_empty(), tr.blocked), (true, 0));
    }

    #[test]
    fn boarding_and_dying_aboard() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, (14, 10), 0).unwrap();
        let fi = w.index(f).unwrap();
        let c = crate::Command::Smart {
            player: 0,
            units: vec![f],
            target: Some(tid),
            tile: (20, 10),
        };
        assert_eq!(w.command_order(fi, &c), Some(Order::Board { transport: tid }));
        w.step(&[c]);
        for _ in 0..600 {
            w.step(&[]);
            if w.unit(f).unwrap().inside {
                break;
            }
        }
        let u = w.unit(f).unwrap();
        assert_eq!(u.order, Order::Aboard { transport: tid });
        assert_eq!(w.unit(tid).unwrap().passengers[0], Some(f));
        assert_eq!(w.occupant(Layer::Land, 19, 10), None);
        // The transport dies, and the passenger with it; neither's HP changes (unit-slots spec, round-39 errata).
        let fi = w.index(f).unwrap();
        w.units[fi].hp = 60;
        let ti = w.index(tid).unwrap();
        let hp = w.units[ti].hp;
        w.kill(ti);
        assert_eq!(w.unit(tid).unwrap().hp, hp);
        let u = w.unit(f).unwrap();
        assert_eq!((u.dying, u.hp), (true, 60));
    }

    #[test]
    fn a_boarder_with_no_route_takes_the_boarding_command_again() {
        // human12-transport: a human player's unit in order 34 whose move finds no route stays in order 34 (still
        // animation, wait 3) instead of going idle; boarding is a special case of "No route".
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, (14, 10), 0).unwrap();
        let fi = w.index(f).unwrap();
        for (x, y) in [
            (13, 9),
            (14, 9),
            (15, 9),
            (13, 10),
            (15, 10),
            (13, 11),
            (14, 11),
            (15, 11),
        ] {
            w.terrain.set_bits(x, y, 0x80);
        }
        w.take_over(fi, Order::Board { transport: tid });
        for _ in 0..3 {
            w.step(&[]);
        }
        let u = w.unit(f).unwrap();
        assert_eq!(u.order, Order::Board { transport: tid });
        assert_eq!((u.pending, u.script.anim, u.script.wait), (None, anim::STILL, 3));
        for _ in 0..30 {
            w.step(&[]);
            assert_eq!(w.unit(f).unwrap().order, Order::Board { transport: tid });
        }
    }

    /// Round-43 errata: a human player's no-route restart tells the docked transport to come again (its order 32
    /// becomes pending). Round-53 errata (`human13-10000` ogre 1383, n = 9154): the restarted order's logic runs in
    /// the same update and finds the unit not arrived. A computer player's unit takes command 4 on the transport at
    /// once (order 4, still animation, wait 3) and doesn't tell it; every other player's unit gets a smart move, the
    /// boarding command again, which tells it.
    #[test]
    fn a_no_route_restart_runs_the_logic_in_the_same_update() {
        for (kind, tells) in [
            (controller::HUMAN, true),
            (controller::PASSIVE, true),
            (controller::COMPUTER, false),
        ] {
            let mut w = sea(20);
            w.players[0].controller = kind;
            let t = ship(&mut w, (20, 10), Order::Docked);
            let tid = w.units[t].id;
            let f = w.spawn(FOOTMAN, 0, (14, 10), 0).unwrap();
            for (x, y) in [
                (13, 9),
                (14, 9),
                (15, 9),
                (13, 10),
                (15, 10),
                (13, 11),
                (14, 11),
                (15, 11),
            ] {
                w.terrain.set_bits(x, y, 0x80);
            }
            let fi = w.index(f).unwrap();
            w.units[fi].order = Order::Board { transport: tid };
            w.board(fi, tid);
            let u = &w.units[fi];
            assert_eq!(
                (u.script.anim, u.script.wait, u.pending),
                (anim::STILL, 3, None),
                "controller {kind}"
            );
            if tells {
                assert_eq!(u.order, Order::Board { transport: tid }, "controller {kind}");
            } else {
                assert!(
                    matches!(u.order, Order::MoveAttack { target: Some(t), .. } if t == tid),
                    "controller {kind}: {:?}",
                    u.order
                );
            }
            let ti = w.index(tid).unwrap();
            assert_eq!(w.units[ti].pending.is_some(), tells, "controller {kind}");
        }
    }

    /// A generator state whose next two draws are `a` and `b` mod 12.
    fn draws_mod_12(a: u32, b: u32) -> crate::Rng {
        (0u32..)
            .map(crate::Rng::new)
            .find(|start| {
                let mut r = *start;
                r.draw() % 12 == a && r.draw() % 12 == b
            })
            .unwrap()
    }

    /// Order 34 on a transport landing at `ship_at`, by a unit of `kind` at `unit_at` (arrived, can't board), with
    /// draws `a` and `b` mod 12 next. Returns the unit's order and whether it drew twice.
    fn landing_boarder(
        shore: i32,
        kind: u8,
        ship_at: (i32, i32),
        unit_at: (i32, i32),
        a: u32,
        b: u32,
    ) -> (Order, bool) {
        let mut w = sea(shore);
        w.players[0].controller = kind;
        let t = ship(&mut w, ship_at, Order::Land { goal: ship_at });
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, unit_at, 0).unwrap();
        let fi = w.index(f).unwrap();
        w.units[fi].order = Order::Board { transport: tid };
        w.rng = draws_mod_12(a, b);
        let mut after = w.rng;
        after.draw();
        after.draw();
        w.board(fi, tid);
        let u = &w.units[fi];
        if matches!(u.order, Order::Move { .. }) {
            assert_eq!((u.script.anim, u.script.wait, u.pending), (anim::STILL, 3, None));
        }
        (u.order, w.rng == after)
    }

    /// Round-53 errata (`orc14-10000` knight 1430, n = 5486): a computer player's unit that has arrived but can't
    /// board, its transport landing in its region, walks to a random nearby tile: x drawn first, then y, each offset
    /// (r mod 12) - 6 and clamped to 0 to the map's width - 1 (the width for both). The move has no target and takes
    /// over at once. A human player's unit keeps waiting in order 34.
    #[test]
    fn a_computer_boarder_whose_transport_is_landing_walks_to_a_random_tile() {
        let c = controller::COMPUTER;
        // The vector's offsets: -3 and +5.
        assert_eq!(
            landing_boarder(20, c, (20, 10), (19, 9), 3, 11),
            (
                Order::Move {
                    goal: (16, 14),
                    target: None
                },
                true
            )
        );
        let human = landing_boarder(20, controller::HUMAN, (20, 10), (19, 9), 3, 11);
        assert!(matches!(human, (Order::Board { .. }, false)), "{human:?}");
        // Clamped at 0.
        assert_eq!(
            landing_boarder(2, c, (2, 2), (1, 1), 0, 0),
            (
                Order::Move {
                    goal: (0, 0),
                    target: None
                },
                true
            )
        );
    }

    /// Round-52 errata (the `orc14-10000` knight 1433 vector at n = 2421, moved): a unit boarding a transport in
    /// another region walks to the transport's destination tile, but its route search's goal zone is the transport's
    /// footprint widened by one tile, not the 3 x 3 tiles around that goal.
    #[test]
    fn the_boarding_walk_aims_at_the_transports_widened_footprint() {
        let mut w = sea(20);
        let t = ship(&mut w, (22, 10), Order::Docked);
        w.units[t].ship.dest = (21, 9);
        assert_eq!(w.units[t].size, (1, 1));
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, (14, 4), 0).unwrap();
        let (fi, ti) = (w.index(f).unwrap(), w.index(tid).unwrap());
        let g = w.board_walk(fi, ti);
        assert_eq!((g.tile, g.radius, g.range), ((21, 9), 1, false));
        assert_eq!(g.zone, Some((21, 9, 23, 11)), "not (20, 8) to (22, 10)");
    }

    #[test]
    fn a_full_transport_turns_boarders_away() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        w.units[t].passengers = [Some(UnitId(999)); 6];
        w.units[t].ship.dest = (40, 10);
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, (19, 10), 0).unwrap();
        let fi = w.index(f).unwrap();
        w.units[fi].order = Order::Board { transport: tid };
        // The destination is in the sea region, not the unit's: it gives up. Round-64 errata: idle becomes the pending
        // order, taken over in the same update (still animation, wait 3).
        w.board(fi, tid);
        let u = &w.units[fi];
        assert_eq!((u.order, u.pending), (Order::Idle, None));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
    }

    /// Round-64 vectors (`orc12-orc12-board-attacked-1000` ogre 1439, n = 661): a unit with an attacker recorded that
    /// reaches its docked transport boards it and doesn't turn on its attacker.
    #[test]
    fn a_boarder_with_an_attacker_boards_without_reacting() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, (19, 10), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (18, 10), 0).unwrap();
        let gi = w.index(g).unwrap();
        w.units[gi].hidden_from = 0;
        let fi = w.index(f).unwrap();
        w.units[fi].order = Order::Board { transport: tid };
        w.units[fi].attacked_by = Some(g);
        w.start_anim(fi, anim::MOVE);
        let rng = w.rng;
        w.board(fi, tid);
        let u = &w.units[fi];
        assert_eq!(
            (u.order, u.attacked_by, u.inside),
            (Order::Aboard { transport: tid }, None, true)
        );
        assert_eq!(w.rng, rng);
    }

    /// Round-64 vectors (static study): a unit in order 34 that has reached its transport, which isn't docked, with
    /// the transport's destination in its region, runs the reaction: a combat unit whose search picks a target attacks
    /// it (order 12). With no attacker it keeps waiting in order 34.
    #[test]
    fn a_waiting_boarder_runs_the_reaction() {
        for attacked in [true, false] {
            let mut w = sea(20);
            let t = ship(&mut w, (20, 10), Order::Idle);
            w.units[t].ship.dest = (18, 12);
            let tid = w.units[t].id;
            let f = w.spawn(FOOTMAN, 0, (19, 10), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (17, 10), 0).unwrap();
            let gi = w.index(g).unwrap();
            w.units[gi].hidden_from = 0;
            let fi = w.index(f).unwrap();
            w.units[fi].order = Order::Board { transport: tid };
            w.units[fi].attacked_by = attacked.then_some(g);
            w.board(fi, tid);
            let u = &w.units[fi];
            if attacked {
                assert_eq!(u.order, Order::Chase { target: g });
            } else {
                assert_eq!((u.order, u.script.anim), (Order::Board { transport: tid }, anim::STILL));
            }
        }
    }

    /// Round-64 vectors (static study): a unit in order 34 whose transport dies while it walks forgets the transport
    /// at its next step boundary and idle becomes the pending order, taken over in the same update; no reaction. An
    /// attacker lost in the same way is forgotten too; a valid one is kept.
    #[test]
    fn a_lost_transport_makes_idle_pending_with_no_reaction() {
        for attacker_dies in [false, true] {
            let mut w = sea(20);
            let t = ship(&mut w, (20, 10), Order::Docked);
            let tid = w.units[t].id;
            let f = w.spawn(FOOTMAN, 0, (10, 10), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (9, 10), 0).unwrap();
            let gi = w.index(g).unwrap();
            w.units[gi].hidden_from = 0;
            if attacker_dies {
                w.kill(gi);
            }
            let fi = w.index(f).unwrap();
            w.units[fi].order = Order::Board { transport: tid };
            w.units[fi].attacked_by = Some(g);
            w.start_anim(fi, anim::MOVE);
            let ti = w.index(tid).unwrap();
            w.kill(ti);
            let fi = w.index(f).unwrap();
            let rng = w.rng;
            w.board(fi, tid);
            let u = &w.units[fi];
            assert_eq!((u.order, u.pending, u.tile), (Order::Idle, None, (10, 10)));
            assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
            assert_eq!(u.attacked_by, (!attacker_dies).then_some(g));
            assert_eq!(w.rng, rng, "no reaction");
        }
    }

    #[test]
    fn going_idle_on_the_shore_docks() {
        let mut w = sea(20);
        let i = ship(&mut w, (20, 10), Order::PushOff);
        w.stop(i);
        assert_eq!(w.units[i].order, Order::Docked);
        let j = ship(
            &mut w,
            (30, 10),
            Order::Move {
                goal: (40, 10),
                target: None,
            },
        );
        let j = w.index(w.units[j].id).unwrap();
        w.stop(j);
        assert_eq!(w.units[j].order, Order::Idle);
    }

    /// Orders spec, round-63 errata ("Walking orders react only when the walk has ended"; static study for order 34):
    /// a boarding footman hit while it walks to the transport keeps walking, and its attacker stays recorded.
    #[test]
    fn a_boarding_footman_hit_while_walking_doesnt_react() {
        let mut w = sea(20);
        let t = ship(&mut w, (20, 10), Order::Docked);
        let tid = w.units[t].id;
        let f = w.spawn(FOOTMAN, 0, (10, 10), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (9, 10), 0).unwrap();
        let fi = w.index(f).unwrap();
        // Seen by the footman's owner (round-53 errata: a hidden attacker doesn't seed the search).
        let gi = w.index(g).unwrap();
        w.units[gi].hidden_from = 0;
        w.units[fi].order = Order::Board { transport: tid };
        w.units[fi].attacked_by = Some(g);
        assert!(w.run_transport(fi));
        let u = &w.units[fi];
        assert_eq!((u.order, u.attacked_by), (Order::Board { transport: tid }, Some(g)));
        assert_eq!((u.tile, u.script.anim), ((11, 9), anim::MOVE), "it took a step");
    }

    /// Round-51 errata, orders spec "Fleeing" step 6 (`xorc8-10000` transport 1423, n = 6177): a transport flees with a
    /// smart move, so a flee tile on the shore makes it land: order 30, landing animation 6, wait 3.
    #[test]
    fn a_transport_fleeing_to_the_shore_lands() {
        let mut w = sea(30);
        let i = ship(&mut w, (34, 10), Order::Idle);
        let s = w.units[i].id;
        let a = w.spawn(FOOTMAN, 1, (20, 10), 0).unwrap();
        let ai = w.index(a).unwrap();
        w.units[ai].facing = 6;
        let i = w.index(s).unwrap();
        w.units[i].attacked_by = Some(a);
        assert!(w.flee(i));
        let u = &w.units[i];
        assert!(matches!(u.order, Order::Land { .. }), "{:?}", u.order);
        assert_eq!((u.script.anim, u.script.wait, u.attacked_by), (anim::WORK_B, 3, None));
    }

    /// Pins of the current behaviour on the paths the B-038 movement refactor touches (landing and pushing off as
    /// walks, the command rules and their stage table, the transport's pull-back, boarding) and of the coverage
    /// report's untraced transport regions, written before it so that it can't change them unseen. Each test states
    /// the outcome in a few fields and pins the whole simulation state with [`World::fingerprint`]. Where a pin differs
    /// from the spec's wording, its comment says so: those are open questions, not rules.
    mod pin {
        use super::*;

        /// Puts land (a ship can't enter it) on every tile at Chebyshev distance `r` from `c`.
        fn ring(w: &mut World, c: (i32, i32), r: i32) {
            for y in c.1 - r..=c.1 + r {
                for x in c.0 - r..=c.0 + r {
                    if chebyshev((x, y), c) == r && w.terrain.contains(x, y) {
                        let n = w.terrain.at(x, y).unwrap();
                        w.terrain.words[n] = bits::LAND;
                    }
                }
            }
        }

        /// The ship's order, animation, wait, tile, class and step size after one run.
        fn state(w: &World, i: usize) -> (Order, u8, u8, (i32, i32), bool, bool) {
            let u = &w.units[i];
            (
                u.order,
                u.script.anim,
                u.script.wait,
                u.tile,
                u.ship.landing,
                u.ship.one_tile,
            )
        }

        fn pinned(got: &[u64], want: &[u64]) {
            assert_eq!(got, want, "pinned fingerprints: {got:?}");
        }

        /// Order 30's logic from each kind of start: far out (an ordinary two-tile step), just come within 3 tiles
        /// in the move animation (the arrival switch), within 3 with an even and an odd goal, arrived (with and
        /// without the unload mark), and blocked within 3 tiles (animation 6 restarted).
        #[test]
        fn landing_runs() {
            let mut prints = vec![];
            for case in [
                "far",
                "came within",
                "even",
                "odd",
                "arrived",
                "arrived unload",
                "blocked",
            ] {
                let mut w = sea(20);
                let (from, goal) = match case {
                    "far" => ((30, 10), (21, 10)),
                    "came within" | "even" => ((24, 10), (22, 10)),
                    "odd" | "blocked" => ((23, 10), (21, 10)),
                    _ => ((21, 10), (21, 10)),
                };
                let i = ship(&mut w, from, Order::Land { goal });
                let id = w.units[i].id;
                let near = chebyshev(from, goal) <= 3;
                w.units[i].ship.landing = near && case != "came within";
                w.units[i].ship.one_tile = from.0 & 1 == 1;
                let a = if near && case != "came within" {
                    anim::WORK_B
                } else {
                    anim::MOVE
                };
                w.start_anim(i, a);
                w.units[i].ship.unload = case == "arrived unload";
                if case == "blocked" {
                    let b = w.spawn(SHIP, 0, (40, 30), 0).unwrap();
                    let bi = w.index(b).unwrap();
                    let unit = w.units[bi].clone();
                    w.mark(&unit, unit.tile, None);
                    w.units[bi].tile = (22, 10);
                    let unit = w.units[bi].clone();
                    w.mark(&unit, (22, 10), Some(b));
                    // A moving ally: the search walks through it, the step is refused (row I, as the landing
                    // transport is in animation 6, not the move animation that row F needs).
                    w.start_anim(bi, anim::MOVE);
                }
                let i = w.index(id).unwrap();
                let script = (w.units[i].script.pc, w.units[i].script.wait);
                w.landing(i, goal);
                prints.push(w.fingerprint());
                let s = state(&w, i);
                match case {
                    // Round 70, M3: far out an ordinary step lets the running move animation run on.
                    "far" => {
                        assert_ne!(s.3, from);
                        assert_eq!((w.units[i].script.pc, s.2), script);
                    }
                    "arrived" => assert_eq!(s.0, Order::Docked),
                    "arrived unload" => assert_eq!(s.0, Order::Unload),
                    "blocked" => assert_eq!((s.1, s.3, w.units[i].blocked, s.2), (anim::WORK_B, (23, 10), 1, 1)),
                    _ => assert_ne!(s.3, from, "{case}"),
                }
            }
            pinned(
                &prints,
                &[
                    7705891770862664557,
                    16935650280107691868,
                    16935650280107691868,
                    6920067489431265811,
                    11614432065551312411,
                    3163504729380280111,
                    15255577242155658033,
                ],
            );
        }

        /// Order 30 with no route ("Landing", step 6): on shore water, order 29 if marked unload on arrival, else 32;
        /// elsewhere on an even tile, idle in the sea class as a two-tile mover (round 75), even when it was landing
        /// one tile per step, so its next step done changes nothing; on an odd tile, 32.
        #[test]
        fn landing_with_no_route() {
            let mut prints = vec![];
            for (from, unload, want) in [
                ((21, 10), true, Order::Unload),
                ((21, 10), false, Order::Docked),
                ((26, 10), false, Order::Idle),
                ((27, 11), false, Order::Docked),
            ] {
                let mut w = sea(20);
                let goal = (from.0 - 3, from.1);
                let i = ship(&mut w, from, Order::Land { goal });
                w.units[i].ship.unload = unload;
                // Landing one tile per step in the landing class, whatever the tile.
                (w.units[i].ship.landing, w.units[i].ship.one_tile) = (true, true);
                ring(&mut w, from, 1);
                ring(&mut w, from, 2);
                w.landing(i, goal);
                let s = state(&w, i);
                assert_eq!((s.0, s.3), (want, from), "{from:?}");
                assert!(!w.units[i].ship.unload);
                let ship = &w.units[i].ship;
                assert_eq!(!ship.landing && !ship.one_tile, want == Order::Idle, "{from:?}");
                if want == Order::Idle {
                    prints.push(w.fingerprint());
                    // Its next step done finds nothing to change.
                    w.units[i].script.wait = 1;
                    assert!(w.run_order(i));
                    let ship = &w.units[i].ship;
                    assert_eq!(
                        (w.units[i].order, ship.landing, ship.one_tile),
                        (Order::Idle, false, false)
                    );
                    continue;
                }
                prints.push(w.fingerprint());
            }
            pinned(
                &prints,
                &[
                    57417322048109498,
                    2520618248462677285,
                    15838895517510761922,
                    13906116684569415798,
                ],
            );
        }

        /// Order 31's runs: done on an even open-water tile (its smart move to the destination, with the target unit
        /// kept or lost), no tile found (docks), a lost target (pending idle) and a lost attacker, steps from an odd
        /// and an even tile, and no route (docks).
        #[test]
        fn push_off_runs() {
            let mut prints = vec![];
            for case in [
                "done",
                "done lost target",
                "no tile",
                "lost target",
                "odd",
                "even",
                "no route",
            ] {
                let mut w = if case == "no tile" {
                    coast(|_| bits::LAND)
                } else {
                    sea(20)
                };
                let from = match case {
                    "done" | "done lost target" => (24, 10),
                    "odd" => (21, 11),
                    _ => (20, 10),
                };
                let mate = w.spawn(SHIP, 0, (40, 20), 0).unwrap();
                let foe = w.spawn(FOOTMAN, 1, (2, 2), 0).unwrap();
                let i = ship(&mut w, from, Order::PushOff);
                w.start_anim(i, anim::WORK_B);
                let u = &mut w.units[i];
                u.ship.target = Some(mate);
                u.ship.target_tile = (40, 10);
                u.ship.dest = (40, 10);
                u.attacked_by = Some(foe);
                u.blocked = 4;
                if case.contains("lost") {
                    let m = w.index(mate).unwrap();
                    w.kill(m);
                    let f = w.index(foe).unwrap();
                    w.kill(f);
                }
                if case == "no route" {
                    ring(&mut w, from, 2);
                    // Leave a passing tile for the ring search beyond the ring.
                }
                let i = w.index(w.units[i].id).unwrap();
                w.push_off(i);
                let s = state(&w, i);
                let u = &w.units[i];
                match case {
                    "done" => {
                        assert!(matches!(s.0, Order::Follow { .. } | Order::Move { .. }), "{s:?}");
                        // Round 70, M6: the smart move is a command, so the blocked counter is cleared.
                        assert_eq!(u.blocked, 0);
                    }
                    "done lost target" => assert_eq!((u.blocked, u.attacked_by), (0, None)),
                    // Round 70, M4: no tile docks as a pending order (wait 3), no route as an immediate one (wait 1).
                    "no tile" => assert_eq!((s.0, s.1, s.2), (Order::Docked, anim::STILL, 3)),
                    "no route" => assert_eq!((s.0, s.1, s.2, u.pending), (Order::Docked, anim::STILL, 1, None)),
                    "lost target" => assert_eq!((u.pending, u.attacked_by), (Some(Order::Idle), None)),
                    _ => {}
                }
                prints.push(w.fingerprint());
            }
            pinned(
                &prints,
                &[
                    10769073492478209763,
                    2220216196743237337,
                    3134487529215105021,
                    14733354621204349779,
                    671816985406506113,
                    8430039692419134989,
                    12333632964458456077,
                ],
            );
        }

        /// "Moving near the shore", docking sound (round 70, M2): pushing off tests the tile it leaves, so its step
        /// from shore water onto open water plays the sound and a step from open water doesn't; every other step tests
        /// the tile stepped onto.
        #[test]
        fn pushing_off_sounds_on_leaving_shore_water() {
            let docked = |w: &World| w.sounds.cues.iter().any(|c| c.sound == crate::sound::Sound::Dock);
            for (from, order, want) in [
                ((21, 11), Order::PushOff, true),
                ((23, 11), Order::PushOff, false),
                ((22, 11), Order::Land { goal: (21, 11) }, true),
            ] {
                let mut w = sea(20);
                let i = ship(&mut w, from, order);
                w.start_anim(i, anim::WORK_B);
                w.units[i].ship.target_tile = (40, 11);
                w.units[i].ship.dest = (40, 11);
                w.sounds.cues.clear();
                w.run_transport(i);
                let u = &w.units[i];
                assert_ne!(u.tile, from, "{order:?} from {from:?} took a step");
                assert_eq!(docked(&w), want, "{order:?} from {from:?} to {:?}", u.tile);
            }
        }

        /// Round 70, M5 (untraced): order 29 off shore water puts nobody ashore, and order 32 takes over as a pending
        /// order does (still animation, wait 3).
        #[test]
        fn unloading_off_shore_water_docks() {
            let mut w = sea(20);
            let i = ship(&mut w, (30, 10), Order::Unload);
            w.unload(i);
            let s = state(&w, i);
            assert_eq!((s.0, s.1, s.2), (Order::Docked, anim::STILL, 3));
            pinned(&[w.fingerprint()], &[10922877962506495687]);
        }

        /// The stage table, row by row, for a smart move and the unload command, from open water, shore water and
        /// orders 30, 31 and 32; with the three things stored.
        #[test]
        fn the_stage_table() {
            let mut prints = vec![];
            let mut orders = vec![];
            for (from, order, to, unload, own_target) in [
                ((20, 10), Order::Docked, (20, 10), true, false),
                ((20, 10), Order::Docked, (14, 10), false, false),
                ((20, 10), Order::Docked, (30, 10), false, false),
                ((30, 10), Order::Land { goal: (21, 10) }, (40, 10), false, false),
                ((30, 10), Order::PushOff, (40, 10), false, false),
                ((30, 10), Order::Idle, (40, 10), false, true),
                ((30, 10), Order::Idle, (40, 10), false, false),
                ((30, 10), Order::Idle, (21, 12), true, false),
                ((30, 10), Order::Idle, (14, 12), false, false),
            ] {
                let mut w = sea(20);
                let mate = w.spawn(SHIP, 0, to, 0).unwrap();
                let i = ship(&mut w, from, order);
                let target = own_target.then_some(mate);
                let o = w.transport_command(i, target, to, unload);
                let u = &w.units[i];
                orders.push(o);
                prints.push(
                    (
                        w.fingerprint(),
                        u.ship.target,
                        u.ship.target_tile,
                        u.ship.dest,
                        u.ship.unload,
                    )
                        .0,
                );
            }
            assert_eq!(
                orders,
                [
                    Order::Land { goal: (20, 10) },
                    Order::Docked,
                    Order::PushOff,
                    Order::PushOff,
                    Order::PushOff,
                    Order::Follow { target: UnitId(1) },
                    Order::Move {
                        goal: (40, 10),
                        target: None
                    },
                    Order::Land { goal: (21, 12) },
                    Order::Land { goal: (21, 11) },
                ]
            );
            pinned(
                &prints,
                &[
                    2053605494979400701,
                    15470943505722001256,
                    1086294026542919542,
                    6853327285718669543,
                    11959604757437237332,
                    5688853751936574971,
                    13936028312609698004,
                    10197535568248626268,
                    13769875678343717185,
                ],
            );
        }

        /// The transport's pull-back fallbacks: no usable tile gives the last tile walked (the unit's tile plus the
        /// line's last step), a target next to the transport has a one-tile walk, and a target on the transport's tile
        /// no walk at all (K = G). Then the shore check or the ring search.
        #[test]
        fn the_transport_pull_back_fallbacks() {
            let mut w = coast(|x| match x {
                c if c < 10 => bits::WATER,
                10 => bits::SHORE,
                _ => bits::LAND,
            });
            // Make the water around (4, 4) another region, so no tile on the way back is usable.
            for y in 0..32 {
                for x in 0..10 {
                    let n = w.terrain.at(x, y).unwrap();
                    w.terrain.regions[n] = if (x, y) == (4, 4) { 7 } else { SEA_REGION };
                }
            }
            // No usable tile on the way back: the last tile walked, (5, 4), fails the shore check, so the ring search
            // from it finds the shore.
            assert_eq!(w.shore_target((4, 4), (12, 4)), (10, 3));
            // A one-tile walk: nothing usable, K = G = (5, 4), and the ring search from it keeps K (pinned as is).
            assert_eq!(w.shore_target((4, 4), (5, 4)), (5, 4));
            assert_eq!(w.shore_target((10, 4), (10, 4)), (10, 4));
            assert_eq!(w.shore_target((9, 4), (10, 4)), (10, 4));
            // A non-transport's command goal is untouched; a transport's on shore water keeps its tile but drops the
            // target unit.
            let f = w.spawn(FOOTMAN, 0, (14, 4), 0).unwrap();
            let fi = w.index(f).unwrap();
            assert_eq!(w.command_goal(fi, Some(f), (20, 4)), ((20, 4), Some(f)));
            let i = ship(&mut w, (10, 6), Order::Docked);
            assert_eq!(w.land_and_shore(i, (20, 4), Some(f)), ((20, 4), None));
            assert_eq!(w.land_and_shore(i, (4, 6), Some(f)), ((4, 6), Some(f)));
        }

        /// "What the transport stores" (round 70, M7): every command given to a transport records its target unit,
        /// target tile and destination, after the land and shore steps: a player's move, and a computer player's
        /// command 4 and patrol. A player's attack-move and patrol are refused by the selection filter (no "can
        /// attack" flag; round 75, V40), so they record nothing.
        #[test]
        fn every_command_records_the_three_things() {
            for case in [
                "move",
                "attack-move",
                "patrol",
                "command 4",
                "ai patrol",
                "command 4 on land",
            ] {
                let mut w = sea(20);
                let i = ship(&mut w, (30, 10), Order::Idle);
                let id = w.units[i].id;
                let mate = w.spawn(SHIP, 0, (40, 20), 0).unwrap();
                let i = w.index(id).unwrap();
                let u = &mut w.units[i];
                u.ship.target = Some(mate);
                u.ship.target_tile = (1, 1);
                u.ship.dest = (1, 1);
                let to = (36, 14);
                let units = vec![id];
                let player = 0;
                match case {
                    "move" => w.step(&[crate::world::Command::Move { player, units, to }]),
                    "attack-move" => w.step(&[crate::world::Command::AttackMove { player, units, to }]),
                    "patrol" => w.step(&[crate::world::Command::Patrol { player, units, to }]),
                    "command 4" => w.command_4(i, Some(mate), to),
                    "ai patrol" => w.patrol(i, to, (30, 10)),
                    _ => w.command_4(i, None, (10, 10)),
                }
                let s = &w.unit(id).unwrap().ship;
                let want = match case {
                    "command 4" => (Some(mate), (40, 20), (40, 20)),
                    "attack-move" | "patrol" => (Some(mate), (1, 1), (1, 1)),
                    // A land tile from open water: no target unit, pulled back to the shore water.
                    "command 4 on land" => (None, s.target_tile, s.target_tile),
                    _ => (None, to, to),
                };
                assert_eq!((s.target, s.target_tile, s.dest), want, "{case}");
                if case == "command 4 on land" {
                    assert!(w.shore_water(s.target_tile), "{:?}", s.target_tile);
                }
            }
        }

        /// "What the transport stores" (round 71, Q3 and Q4): command 4 to a transport records the three things once.
        /// Its smart-move stage works from the recorded tile and isn't followed by a second recording, so docking at S
        /// (the first row of the stage table) leaves the target tile at S, with the destination the command's tile.
        #[test]
        fn command_4_records_once() {
            let mut w = sea(20);
            let i = ship(&mut w, (21, 10), Order::Docked);
            let id = w.units[i].id;
            w.units[i].script.wait = 30;
            w.command_4(i, None, (15, 10));
            let u = w.unit(id).unwrap();
            assert_eq!(
                (u.ship.target, u.ship.target_tile, u.ship.dest),
                (None, (21, 10), (15, 10))
            );
            assert_eq!(u.script.wait, 10, "the smart move's wait cap");
        }

        /// Order 34's runs, each pinned: a lost transport (V33), no route with the destination in the unit's region
        /// for a human, passive and computer player's unit (coverage: the computer unit's command 4 taking over),
        /// no route otherwise, boarding, a computer unit at a transport landing in its region, waiting, and giving up.
        #[test]
        fn boarding_runs() {
            let mut prints = vec![];
            for case in [
                "lost",
                "no route human",
                "no route passive",
                "no route computer",
                "no route away",
                "boards",
                "random tile",
                "waits",
                "gives up",
            ] {
                let mut w = sea(20);
                let kind = match case {
                    "no route passive" => controller::PASSIVE,
                    "no route computer" | "random tile" => controller::COMPUTER,
                    _ => controller::HUMAN,
                };
                w.players[0].controller = kind;
                let (order, dest) = match case {
                    "random tile" => (Order::Land { goal: (20, 10) }, (20, 10)),
                    "waits" => (Order::Idle, (18, 12)),
                    "gives up" | "no route away" => (Order::Docked, (40, 10)),
                    _ => (Order::Docked, (20, 10)),
                };
                let t = ship(&mut w, (20, 10), order);
                w.units[t].ship.dest = dest;
                if case == "gives up" {
                    w.units[t].passengers = [Some(UnitId(999)); 6];
                }
                let tid = w.units[t].id;
                let at = match case {
                    "boards" | "random tile" | "waits" | "gives up" => (19, 10),
                    _ => (14, 10),
                };
                let f = w.spawn(FOOTMAN, 0, at, 0).unwrap();
                if case.starts_with("no route") {
                    ring(&mut w, at, 1);
                    for (x, y) in [
                        (13, 9),
                        (14, 9),
                        (15, 9),
                        (13, 10),
                        (15, 10),
                        (13, 11),
                        (14, 11),
                        (15, 11),
                    ] {
                        w.terrain.set_bits(x, y, 0x80);
                    }
                }
                if case == "lost" {
                    let ti = w.index(tid).unwrap();
                    w.kill(ti);
                }
                let fi = w.index(f).unwrap();
                w.units[fi].order = Order::Board { transport: tid };
                w.board(fi, tid);
                prints.push(w.fingerprint());
            }
            pinned(
                &prints,
                &[
                    14143226004565989014,
                    17612710135938525923,
                    560996565627368457,
                    15230616325358557659,
                    11808286589870446420,
                    10579129746205022943,
                    14342187977763971119,
                    1319742456967632131,
                    3403160023535742859,
                ],
            );
        }

        /// Two whole ferry runs, a human and a computer player's: footmen board a docked transport, it is sent across
        /// to the far shore, unloads, and is sent back to open water.
        #[test]
        fn whole_ferry_runs() {
            let mut prints = vec![];
            for kind in [controller::HUMAN, controller::COMPUTER] {
                let mut w = coast(|x| match x {
                    c if c < 10 => bits::LAND,
                    10 | 11 => bits::SHORE,
                    c if c < 36 => bits::WATER,
                    36 | 37 => bits::SHORE,
                    _ => bits::LAND,
                });
                w.players[0].controller = kind;
                let t = ship(&mut w, (10, 10), Order::Docked);
                let tid = w.units[t].id;
                let feet: Vec<_> = [(5, 8), (6, 12), (4, 10)]
                    .iter()
                    .map(|&at| w.spawn(FOOTMAN, 0, at, 0).unwrap())
                    .collect();
                let smart = |units: Vec<UnitId>, target, tile| crate::Command::Smart {
                    player: 0,
                    units,
                    target,
                    tile,
                };
                w.step(&[smart(feet.clone(), Some(tid), (10, 10))]);
                run(&mut w, 300);
                w.step(&[smart(vec![tid], None, (36, 14))]);
                run(&mut w, 400);
                w.step(&[crate::Command::Unload {
                    player: 0,
                    units: vec![tid],
                    tile: w.unit(tid).unwrap().tile,
                }]);
                run(&mut w, 100);
                w.step(&[smart(vec![tid], None, (24, 4))]);
                run(&mut w, 300);
                let t = w.unit(tid).unwrap();
                assert_eq!((t.order, t.tile), (Order::Idle, (24, 4)));
                // Transports spec, "Clearing the mark" (V39): the stream's smart move clears the mark the computer
                // player's ferry set when its passengers boarded, so that transport docks at the far shore with them
                // aboard, and the ferry takes them back and unloads them on the near shore.
                for &f in &feet {
                    let x = w.unit(f).unwrap().tile.0;
                    if kind == controller::HUMAN {
                        assert_eq!(x, 38, "unloaded on the far shore");
                    } else {
                        assert!(x < 10, "unloaded on the near shore: {x}");
                    }
                }
                prints.push(w.fingerprint());
            }
            // The human run's pin changed with C3 (fog spec): the footmen put ashore no longer reveal as they land, so
            // the seen bits differ; the computer player's run, which applies no sight, is unchanged.
            pinned(&prints, &[15390870322679765947, 14821420497745263775]);
        }
    }
}
