//! Transports: the command rules, landing, pushing off, unloading and boarding. Spec: `docs/specs/orders/transports.md`.
//!
//! Not yet here: the docking sound, and a passenger dying aboard by other means than its transport's death (Enhanced
//! mode empties its slot; nothing but the transport's death kills a passenger yet).
//!
//! Our choices for the spec's open questions: a computer player's unit whose transport is landing in its region waits
//! in order 34; the unload command's tile is used as given; a passenger is put ashore facing as it was.

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::TILE_PX;
use crate::movement::{Goal, Walk, chebyshev};
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
        let home = self.region_at(t);
        let usable = |at: (i32, i32)| self.region_at(at) == home && self.terrain.word(at.0, at.1) & PULL_BACK_MASK == 0;
        // The walk back from G undoes the line's steps in their order: G, G - s1, G - s1 - s2, ..., stopping before T.
        let steps = path::line(t, g, 1);
        let mut walk = Vec::with_capacity(steps.len());
        let mut at = g;
        for &d in &steps {
            if at == t {
                break;
            }
            walk.push(at);
            let v = path::DIRECTIONS[d as usize];
            at = (at.0 - v.0, at.1 - v.1);
        }
        let k = match walk.iter().position(|&a| self.terrain.contains(a.0, a.1) && usable(a)) {
            Some(0) => g,
            Some(n) => walk[n - 1],
            None => walk.last().copied().unwrap_or(g),
        };
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
    /// and, for a transport on open water, the tile is pulled back to shore. Round-40 errata: this runs for every
    /// command given to a transport, not only the smart move and unload.
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

    /// The goal tile of a command other than the smart move and unload: [`World::land_and_shore`] for a transport,
    /// the tile itself for any other unit.
    pub(crate) fn transport_goal(&self, i: usize, to: (i32, i32)) -> (i32, i32) {
        if self.is_transport(&self.units[i]) {
            self.land_and_shore(i, to, None).0
        } else {
            to
        }
    }

    /// A smart move (or, with `unload`, command 29) given to a transport ("Commands given to a transport").
    pub(crate) fn transport_command(
        &mut self,
        i: usize,
        target: Option<UnitId>,
        tile: (i32, i32),
        unload: bool,
    ) -> Order {
        let g = target.and_then(|t| self.unit(t)).map_or(tile, |t| t.tile);
        let s = self.units[i].tile;
        let (g, target) = self.land_and_shore(i, g, target);
        let u = &mut self.units[i];
        u.ship.target = target;
        u.ship.target_tile = g;
        u.ship.dest = g;
        let docking = matches!(u.order, Order::Land { .. } | Order::PushOff | Order::Docked);
        if unload && self.shore_water(g) {
            self.units[i].ship.unload = true;
        }
        let (shore_s, shore_g) = (self.shore_water(s), self.shore_water(g));
        if shore_s && self.region_at(g) == self.region_at(s) && self.land_first(s, g) {
            if g == s {
                return Order::Land { goal: s };
            }
            self.units[i].ship.target_tile = s;
            return Order::Docked;
        }
        if docking || shore_s {
            return Order::PushOff;
        }
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
                // The transport's smart move is a command: it clears the blocked counter (movement spec).
                self.units[ti].blocked = 0;
                self.forget_stale_attacker(ti);
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
            Order::Board { .. } if self.react(i) => {}
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
        let u = &mut self.units[i];
        u.ship.landing = !far;
        u.ship.one_tile = !far && !(even(u.tile) && even(goal));
        // Step 5 (round-53 errata, `human13-10000` n = 3820): within the 3 tiles every run's arrival test discards
        // the planned route, so each step is planned afresh from the current tile.
        if !far {
            u.path.clear();
        }
        // Step 2: a two-tile mover starts its move animation and runs it at once; a one-tile mover keeps animation 6.
        let restart = !u.ship.one_tile;
        match self.move_toward_as(i, Goal::tile(goal, 0), restart) {
            Walk::Arrived => {
                let u = &mut self.units[i];
                let next = if u.ship.unload { Order::Unload } else { Order::Docked };
                u.ship.unload = false;
                u.order = next;
                // Set at once like an immediate order: the still animation starts with a wait of 1 (orders spec,
                // "Pending orders and immediate orders"; Human04 vector: docked at n = 36 with wait 1).
                self.start_anim(i, anim::STILL);
            }
            Walk::NoPath => {
                let unload = std::mem::take(&mut self.units[i].ship.unload);
                let tile = self.units[i].tile;
                let next = if self.shore_water(tile) {
                    if unload { Order::Unload } else { Order::Docked }
                } else if even(tile) {
                    self.units[i].ship.landing = false;
                    Order::Idle
                } else {
                    Order::Docked
                };
                self.units[i].order = next;
                self.units[i].path.clear();
                self.start_anim(i, anim::STILL);
            }
            Walk::Stepping | Walk::Blocked => {}
        }
    }

    /// Order 31 ("Pushing off").
    fn push_off(&mut self, i: usize) {
        let tile = self.units[i].tile;
        if even(tile) && self.open_water(tile) {
            let u = &mut self.units[i];
            u.ship.landing = false;
            u.ship.one_tile = false;
            u.order = Order::Idle;
            u.path.clear();
            let (owner, target, dest) = (u.owner, u.ship.target, u.ship.dest);
            let target = target.filter(|&t| self.holds(owner, t));
            let o = self.transport_command(i, target, dest, false);
            self.take_over(i, o);
            return;
        }
        let Some(to) = self.find_push_off(tile, self.units[i].ship.target_tile) else {
            self.dock(i);
            return;
        };
        let u = &mut self.units[i];
        u.ship.target_tile = to;
        u.path.clear();
        u.blocked = 0;
        let owner = u.owner;
        if let Some(t) = u.ship.target
            && !self.holds(owner, t)
        {
            self.units[i].ship.target = None;
            self.give(i, Order::Idle);
        }
        if let Some(a) = self.units[i].attacked_by
            && !self.holds(owner, a)
        {
            self.units[i].attacked_by = None;
        }
        // Step 4: on an even tile, a two-tile mover whose move animation is started and run at once with the step; on
        // an odd tile the transport keeps animation 6, the order's own.
        if even(tile) {
            self.units[i].ship.one_tile = false;
        }
        if matches!(self.move_toward_as(i, Goal::tile(to, 0), even(tile)), Walk::NoPath) {
            self.dock(i);
        }
    }

    fn dock(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.order = Order::Docked;
        u.path.clear();
        self.switch_anim(i, anim::STILL);
    }

    /// Order 29 ("Unloading"): every passenger from slot 6 down to slot 1, then order 32 takes over in the same update
    /// as a pending order does (still animation, wait 3; round-39 errata). Off shore water it just becomes order 32.
    fn unload(&mut self, i: usize) {
        if !self.shore_water(self.units[i].tile) {
            self.dock(i);
            return;
        }
        for slot in (0..self.units[i].passengers.len()).rev() {
            self.unload_one(i, slot);
        }
        self.take_over(i, Order::Docked);
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
        // An immediate idle (round-39 errata): still animation with a wait of 1, no turn, and the idle logic not run
        // here. The passenger's own update, later in the cycle, runs it.
        self.start_anim(pi, anim::STILL);
        self.apply_unit_sight(pi);
        self.units[i].passengers[slot] = None;
    }

    /// Order 34 ("Order 34 (board)").
    /// The boarding walk of unit `i` to the transport at `ti`: the transport's tile when it is in the unit's region,
    /// else its destination tile, with arrival radius 1. Round-52 errata (`orc14-10000` knight 1433, n = 2421): the
    /// walk's route search has the transport as its target unit, so its goal zone is the transport's footprint widened
    /// by one tile (pathfinding spec, "Step 1"), even when the goal isn't the transport's tile.
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

    fn board(&mut self, i: usize, transport: UnitId) {
        let Some(ti) = self.index(transport).filter(|&ti| self.units[ti].present()) else {
            self.stop(i);
            return;
        };
        let walk = self.board_walk(i, ti);
        let (t, u) = (&self.units[ti], &self.units[i]);
        let here = self.region_at(u.tile);
        // Round-42 errata: order 34 is a walking order. While the unit walks to its goal only the movement runs, so
        // a unit of any player stays in order 34 (`human10-24000` grunt 1473, `human12-7000` grunt 1407); the logic
        // runs on arrival. With no route the unit restarts order 34 at once (still animation, wait 3) if the
        // transport's destination tile is in its region, and becomes idle at once otherwise. Round-43 errata: for a
        // human player's unit the restart also tells the transport to come again unless it is landing
        // (`human12-transport` transport 1428, n = 46, wait 3). Whether a computer player's restart does is open, so
        // it only restarts order 34 (`human10-24000` n = 17216 and `human12-7000` n = 6454 leave the transport be).
        // Round-53 errata (`human13-10000` ogre 1383, n = 9154): the restarted order's logic then runs in the same
        // update and finds the unit not arrived (step 3). A computer player's unit gets command 4 on the transport,
        // which takes over at once (order 4, still animation, wait 3); every other player's unit gets a smart move,
        // the boarding command again, which tells the transport to come unless it is landing.
        // Round-54 errata (static study): that is every player that isn't a computer player, passive and rescue
        // included, and the smart move's wait cap (10) applies.
        let dest_here = self.region_at(t.ship.dest) == here;
        let human = self.human_controlled(u.owner);
        let computer = self.computer_player(u.owner);
        match self.move_toward(i, walk) {
            Walk::Arrived => {}
            Walk::NoPath if dest_here => {
                let o = if human {
                    self.board_command(i, transport)
                } else {
                    Order::Board { transport }
                };
                self.take_over(i, o);
                if computer {
                    let tile = self.units[i].tile;
                    self.command_4(i, Some(transport), tile);
                    if let Some(o) = self.units[i].pending.take() {
                        self.take_over(i, o);
                    }
                } else {
                    // Round-56 answer (transports spec, "No route", static study): the smart move sets order 34
                    // again and it takes over in the same update with no visible effect, so the unit stays in order
                    // 34 with nothing pending, and the net result is the transport's call and the wait cap.
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
            // Step 5 (round-53 errata, `orc14-10000` knight 1430, n = 5486): a random nearby tile, x drawn first, each
            // offset (r mod 12) - 6 and clamped to the map's size (round-54 errata: one size for both axes, so the
            // width); a move there with no target unit takes over at once.
            let (here, w) = (u.tile, self.terrain.width as i32);
            let x = here.0 + (self.rng.draw() % 12) as i32 - 6;
            let y = here.1 + (self.rng.draw() % 12) as i32 - 6;
            let to = (x.clamp(0, w - 1), y.clamp(0, w - 1));
            self.smart_move(i, None, to);
            if let Some(o) = self.units[i].pending.take() {
                self.take_over(i, o);
            }
        } else if dest_here {
            if self.units[i].script.anim != anim::STILL {
                self.switch_anim(i, anim::STILL);
            }
            self.units[i].path.clear();
        } else {
            self.stop(i);
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
        // The destination is in the sea region, not the unit's: it gives up.
        w.board(fi, tid);
        assert_eq!(w.units[fi].order, Order::Idle);
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

    #[test]
    fn a_boarding_footman_hit_turns_on_its_attacker() {
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
        assert_eq!(w.units[fi].order, Order::Chase { target: g });
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
}
