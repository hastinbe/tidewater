//! Tile steps, arrival and blocking (reviewed spec `docs/specs/movement/movement.md`), and the glue to the path
//! search (`docs/specs/movement/pathfinding.md`).
//!
//! The order logic runs only at an animation's "step done". A new order waits as a pending order until then, and
//! taking over sets the first wait to 3. A step changes the tile at its start: the old tile is left and the new one
//! entered at once, and the move animation then carries the pixel position across.

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::TILE_PX;
use crate::ai::controller;
use crate::anim::{Effects, Modifiers};
use crate::path;
use crate::terrain::bits;
use crate::world::{Layer, Order, Unit, UnitId, World, facing};

/// Blocks after which the counter gives up (row C).
const GIVE_UP: u16 = 15;
/// Blocks from which the path is dropped and the unit waits 15 cycles (row D).
const LONG_WAIT_FROM: u16 = 8;
const LONG_WAIT: u8 = 15;
/// First wait after a pending order takes over.
const TAKEOVER_WAIT: u8 = 3;
/// Movement-map bit on walls that computer units attack when blocked (occupancy spec).
const HUMAN_WALL: u16 = 0x04;

/// Result of one movement decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Walk {
    Arrived,
    Stepping,
    /// Blocked; the blocked rules have set the next decision.
    Blocked,
    /// The search found no steps.
    NoPath,
}

/// Where a moving unit is heading.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Goal {
    pub tile: (i32, i32),
    /// Arrival radius in tiles, before any +1.
    pub radius: i32,
    /// Whether the radius is an attack range (never +1).
    pub range: bool,
    /// The path search's goal zone, for a target unit or building: its footprint widened by one tile.
    pub zone: Option<(i32, i32, i32, i32)>,
}

impl Goal {
    pub(crate) fn tile(tile: (i32, i32), radius: i32) -> Self {
        Self {
            tile,
            radius,
            range: false,
            zone: None,
        }
    }
}

pub(crate) fn chebyshev(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The footprint tile a unit at `from` aims for on a building (pathfinding spec, "Building goal rule").
pub(crate) fn building_goal(from: (i32, i32), b: &Unit) -> (i32, i32) {
    footprint_goal(from, b.tile, (b.size.0 as i32, b.size.1 as i32))
}

/// The "Building goal rule" for a footprint of `size` tiles with its top-left tile at `tl`.
pub(crate) fn footprint_goal(from: (i32, i32), tl: (i32, i32), size: (i32, i32)) -> (i32, i32) {
    let axis = |u: i32, lo: i32, size: i32| {
        let hi = lo + size - 1;
        if u <= lo {
            lo
        } else if u >= hi {
            hi
        } else {
            lo + size / 2
        }
    };
    (axis(from.0, tl.0, size.0), axis(from.1, tl.1, size.1))
}

impl World {
    /// Whether a player is controlled by a person (movement spec, "Player kinds").
    pub(crate) fn human_controlled(&self, owner: u8) -> bool {
        self.players
            .get(owner as usize)
            .is_some_and(|p| p.controller == controller::HUMAN)
    }

    /// Whether a player counts as a computer player: computer slots, and active rescue players, which game setup
    /// turns into computer players (movement spec, "Player kinds").
    pub(crate) fn computer_player(&self, owner: u8) -> bool {
        self.players
            .get(owner as usize)
            .is_some_and(|p| matches!(p.controller, controller::COMPUTER | controller::RESCUE_ACTIVE))
    }

    /// Gives a unit a new order: it waits as the pending order until the next "step done".
    pub(crate) fn give(&mut self, i: usize, order: Order) {
        // Orders 25 and 26 can't be interrupted (harvest spec, "Entering"), and dying units take no orders.
        let inside = matches!(
            self.units[i].order,
            Order::Enter { .. } | Order::Inside { .. } | Order::Aboard { .. } | Order::InsideSite { .. }
        );
        if self.units[i].dying || inside {
            return;
        }
        // Orders spec, "Resuming patrol and attack-move": a pending idle may become the remembered order.
        // The resumed order is a command, so it carries the pending-command mark.
        let resumed = match order {
            Order::Idle => self.resumed(i),
            _ => None,
        };
        // Movement spec, "Blocked rules": an order made pending without a command keeps the
        // blocked counter; `command` clears it.
        let u = &mut self.units[i];
        u.pending = Some(resumed.unwrap_or(order));
        u.commanded = resumed.is_some();
    }

    /// A command's order for one unit, whoever gives it: the goal pulled back off ground the unit can't stand on
    /// (pathfinding spec, "Goal pulled back") and the blocked counter cleared for every order (movement spec), then
    /// the order stored as pending with the pending-command mark ("Units seen by the search"). Our reading of
    /// `xorc01-death-coil` (death knight 1491, n = 353): the command also drops the unit's cached route.
    pub(crate) fn command(&mut self, i: usize, order: Order) {
        let order = match order {
            // Pathfinding spec ("Goal pulled back"): a smart move with a target unit that becomes the
            // move stores the target's tile without pulling it back.
            Order::Move { goal, target: None } => Order::Move {
                goal: self.pull_back(i, goal),
                target: None,
            },
            Order::MoveAttack { goal, target } => Order::MoveAttack {
                goal: self.pull_back(i, goal),
                target,
            },
            Order::AttackMove { goal } => Order::AttackMove {
                goal: self.pull_back(i, goal),
            },
            Order::Patrol { to, back } => Order::Patrol {
                to: self.pull_back(i, to),
                back,
            },
            // Orders spec, "Order 11: attacking a wall tile": the wall test was made on the tile as
            // given; the goal is then pulled back like any other, to the wall tile nearest the unit along the line.
            Order::AttackWall { tile } => Order::AttackWall {
                tile: self.pull_back(i, tile),
            },
            o => o,
        };
        self.command_steps(i);
        // A target unit given with the command gets the same check. If it fails, the target is
        // dropped and idle becomes the pending order instead (with the resume rules); it still counts as a command.
        let owner = self.units[i].owner;
        // Traced (`xhuman10-slow-3000` mage 1481, n = 302): a spell command's target unit is checked too.
        // Harvest spec ("Returning goods"; `orc9-orc09-return-nodepot-2200` peon 1482, n = 1374):
        // command 24's target unit too, and a target inside something fails the check; no depot is looked up then.
        let named = match order {
            Order::MoveAttack { target, .. } | Order::Cast { target, .. } | Order::ReturnGoods { depot: target } => {
                target
            }
            o => crate::orders::target_of(o),
        };
        if named.is_some_and(|t| self.target_lost(owner, t)) {
            self.give(i, Order::Idle);
            let u = &mut self.units[i];
            u.commanded = u.pending.is_some();
            return;
        }
        self.give(i, order);
        let u = &mut self.units[i];
        u.commanded = u.pending == Some(order);
    }

    /// Orders spec, "What every command does", step 4 and the attacker part of step 5: the cached route is dropped,
    /// the blocked counter cleared (movement spec, "Blocked rules") and a stale attacker forgotten ("Commands forget a
    /// stale attacker").
    pub(crate) fn command_steps(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.blocked = 0;
        u.path.clear();
        self.forget_stale_attacker(i);
    }

    /// Orders spec, "Same-update take-over": a command the unit's own logic gives it ([`World::command`]), made pending
    /// and taken over at once (wait 3), without its logic running. Returns whether it took over: nothing does for a
    /// unit that can't take a pending order ("Setting a pending order"). Coming out of a building is the exception: its
    /// command waits for the 25-cycle wait (harvest spec, "Coming out"; [`World::come_out`]).
    pub(crate) fn command_at_once(&mut self, i: usize, o: Order) -> bool {
        self.command(i, o);
        let Some(o) = self.units[i].pending.take() else {
            return false;
        };
        self.take_over(i, o);
        true
    }

    /// "Goal pulled back": a goal tile the unit can't stand on (terrain or building bits of its mask, units ignored;
    /// another region for a unit that doesn't fly) is pulled back toward the unit. The line's k steps (one tile each,
    /// even for a two-tile mover) are subtracted from the goal in their order, first step first, and the first k − 1
    /// tiles so made are tested: at the first usable one the goal becomes the tile tested just before it (the goal
    /// itself for the first). With none usable the goal is kept, for transports too.
    pub(crate) fn pull_back(&self, i: usize, g: (i32, i32)) -> (i32, i32) {
        let u = &self.units[i];
        let mask = u.layer.mask() & !(bits::UNIT | bits::AIR_UNIT);
        let home = self.terrain.region(u.tile.0, u.tile.1);
        let air = u.layer == Layer::Air;
        let usable = |t: (i32, i32)| {
            self.terrain.contains(t.0, t.1)
                && self.terrain.word(t.0, t.1) & mask == 0
                && (air || self.terrain.region(t.0, t.1) == home)
        };
        if usable(g) {
            return g;
        }
        let steps = path::line(u.tile, g, 1);
        let mut before = g;
        for &d in steps.iter().take(steps.len().saturating_sub(1)) {
            let v = path::DIRECTIONS[d as usize];
            let t = (before.0 - v.0, before.1 - v.1);
            if usable(t) {
                return before;
            }
            before = t;
        }
        g
    }

    /// The goal tile of an order aimed at target `t` (movement spec, "Arriving"): a mobile unit's tile; a building's
    /// footprint tile by the building goal rule; for a unit that is neither a sea nor an air unit targeting a shore
    /// building, the shore corner goal (pathfinding spec, "Shore building"; `orc9-orc09-shore-build-3000`
    /// n = 343).
    pub(crate) fn target_goal(&self, u: &Unit, t: &Unit) -> (i32, i32) {
        if !t.building {
            t.tile
        } else if self.aims_at_shore_corner(u, t) {
            self.shore_goal(u.tile, t.tile, (t.size.0 as i32, t.size.1 as i32))
        } else {
            building_goal(u.tile, t)
        }
    }

    /// Whether `u` aims at shore building `t` by its shore corner: `u`'s type is neither a sea nor an air unit.
    pub(crate) fn aims_at_shore_corner(&self, u: &Unit, t: &Unit) -> bool {
        t.building
            && self.flags_of_unit(t) & flags::SHORE_BUILDING != 0
            && self.flags_of_unit(u) & (flags::SEA | flags::AIR) == 0
    }

    /// Ends the unit's order. Going idle makes no draw (timing-and-rng spec).
    pub(crate) fn go_idle(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.order = Order::Idle;
        u.path.clear();
    }

    pub(crate) fn start_anim(&mut self, i: usize, a: u8) {
        let u = &mut self.units[i];
        let pc = self.scripts.unit(u.kind, a).unwrap_or(0);
        u.script.start(a, pc);
    }

    /// Switches animation with the first instruction run in the same cycle (movement spec, "Which first wait
    /// applies when"). Its "step done", if any, is the decision just made, so it isn't acted on again.
    pub(crate) fn switch_anim(&mut self, i: usize, a: u8) {
        self.start_anim(i, a);
        let scripts = std::sync::Arc::clone(&self.scripts);
        let u = &mut self.units[i];
        let m = Modifiers {
            haste: u.haste,
            carrying: u.carrying,
        };
        let fx = u.script.update(scripts.bytes(), u.facing, m);
        u.pos.0 += fx.dx;
        u.pos.1 += fx.dy;
        if fx.sound {
            self.play_pending(i);
        }
    }

    /// The update at a "step done" (orders spec, "How an update runs"): a pending order takes over (step 1); otherwise
    /// the order's logic runs with its walk and arrival switch (steps 2 to 4, [`World::order_logic`]), and the skip
    /// mark is cleared at the end (step 5).
    pub(crate) fn order(&mut self, i: usize, fx: Effects) {
        // Animation-scripts spec ("No strike while a command waits"): while a command given to the
        // unit hasn't taken over, the strike does nothing at all (no reveal, draw, damage, launch or cooldown).
        let waiting = self.units[i].commanded && self.units[i].pending.is_some();
        if self.units[i].script.anim == anim::ATTACK && !waiting {
            for _ in 0..fx.strikes {
                self.strike(i);
            }
        }
        if !fx.step_done {
            return;
        }
        let u = &mut self.units[i];
        // At a boundary the unit is at rest on its tile.
        u.pos = (u.tile.0 * TILE_PX, u.tile.1 * TILE_PX);
        if let Some(o) = u.pending.take() {
            self.take_over(i, o);
            return;
        }
        let (marked, tile) = (self.units[i].skip_search, self.units[i].tile);
        let restarts = self.restarts;
        let move_step = marked && self.out_of_reach(i);
        self.order_logic(i);
        // Step 5, "Order 12 with no route" step 3, "Clearing" (V203, V219): a mark set when the update began is
        // cleared at its end unless the unit stepped. Out of reach of its order-12 target the unit takes a move step
        // (still order 12, no fallback restart), which leaves the mark; a take-over the run itself caused clears it,
        // and a mark first set in this update (a fallback restart in an unmarked update) lasts until the next one.
        let stepped = move_step && matches!(self.units[i].order, Order::Chase { .. }) && self.restarts == restarts;
        if marked && !stepped && self.units[i].tile == tile {
            self.units[i].skip_search = false;
        }
    }

    /// "How an update runs", steps 2 to 4: each order's logic, which walks, makes the arrival switch and runs the
    /// order's own part; a command it gives itself can take over at once ("Same-update take-over").
    fn order_logic(&mut self, i: usize) {
        if self.run_order(i) || self.run_transport(i) || self.run_spell(i) {
            return;
        }
        let order = self.units[i].order;
        match order {
            // Orders spec, "The reaction": orders 23, 24 and 27 run it in their logic once their walk has ended,
            // so each calls it itself.
            Order::Repair { target } => self.repair_order(i, target),
            Order::Harvest { source, tree, chopping } => self.harvest_order(i, source, tree, chopping),
            Order::ReturnGoods { depot } => self.return_order(i, depot),
            Order::Enter { building, stepped } => self.enter_order(i, building, stepped),
            Order::Inside { building } => self.come_out(i, building),
            Order::Build { kind, site, goal } => self.build_order(i, kind, site, goal),
            Order::InsideSite { .. } => self.inside_site(i),
            _ => {}
        }
    }

    /// A pending order replaces the current one: its animation starts with a first wait of 3 and nothing else
    /// happens this cycle (movement spec, "When decisions are made"). It doesn't clear the blocked counter
    /// ("Taking over"): only commands do.
    pub(crate) fn take_over(&mut self, i: usize, o: Order) {
        self.release_tree(i);
        // Orders spec, "Taking over": a return with no depot looks one up, or becomes idle.
        let o = match o {
            Order::ReturnGoods { depot: None } => match self.nearest_depot(&self.units[i]) {
                Some(d) => Order::ReturnGoods { depot: Some(d) },
                None => Order::Idle,
            },
            o => o,
        };
        let a = crate::orders::order_anim(o);
        let u = &mut self.units[i];
        u.order = o;
        u.commanded = false;
        self.start_anim(i, a);
        self.units[i].script.wait = TAKEOVER_WAIT;
    }

    /// Arrival radius after the +1 (movement spec, "The +1"): sea and air units get it, except in orders 30 and 31.
    /// Order 28 is a land unit's, so it never gets the +1.
    fn arrival_radius(&self, u: &Unit, g: &Goal) -> i32 {
        let docking = matches!(u.order, Order::Land { .. } | Order::PushOff);
        let bonus = !g.range && !docking && matches!(u.layer, Layer::Sea | Layer::Air);
        g.radius + bonus as i32
    }

    /// Whether the unit stands within the arrival radius of `g` (the arrival test of [`World::move_toward`]).
    pub(crate) fn at_goal(&self, i: usize, g: Goal) -> bool {
        let u = &self.units[i];
        chebyshev(u.tile, g.tile) <= self.arrival_radius(u, &g)
    }

    /// Orders spec, "Walking orders" step 4 with the plain "No route" rule ("Arriving and losing the target"): one
    /// movement decision toward `g`; with no route the unit goes idle as an immediate order and its idle logic runs in
    /// the same update, straight after. The orders with their own no-route case (order 12's fallback, the harvest
    /// tree search, transports, orders 17 and 18) call [`World::move_toward`] instead.
    pub(crate) fn walk(&mut self, i: usize, g: Goal) -> Walk {
        let w = self.move_toward(i, g);
        if w == Walk::NoPath {
            self.idle_at_once(i);
        }
        w
    }

    /// One movement decision toward `g`: arrive, step, or apply the blocked rules.
    pub(crate) fn move_toward(&mut self, i: usize, g: Goal) -> Walk {
        self.move_toward_as(i, g, false)
    }

    /// [`World::move_toward`]; with `restart`, a step starts the move animation and runs it at once even if it is
    /// already playing (a two-tile transport landing or pushing off, transports spec, "Landing" step 2 and "Pushing
    /// off" step 4).
    pub(crate) fn move_toward_as(&mut self, i: usize, g: Goal, restart: bool) -> Walk {
        let u = &self.units[i];
        if chebyshev(u.tile, g.tile) <= self.arrival_radius(u, &g) {
            self.units[i].path.clear();
            return Walk::Arrived;
        }
        if u.path.is_empty() {
            let path = self.plan(&self.units[i], g);
            if path.is_empty() {
                return Walk::NoPath;
            }
            self.units[i].path = path;
        }
        let u = &self.units[i];
        let next = u.path[0];
        let occupant = self.occupant(u.layer, next.0, next.1).filter(|&o| o != u.id);
        // Transports spec, "Landing": a landing transport moves in the landing class, which may enter shore water.
        let class = if u.ship.landing {
            crate::terrain::mask::LANDING
        } else {
            u.layer.mask()
        };
        if occupant.is_some() || !self.terrain_allows_mask(class, next.0, next.1) {
            self.blocked(i, next, occupant, g);
            return Walk::Blocked;
        }
        let unit = self.units[i].clone();
        self.mark(&unit, unit.tile, None);
        let u = &mut self.units[i];
        u.facing = facing(next.0 - u.tile.0, next.1 - u.tile.1);
        u.tile = next;
        u.path.remove(0);
        let unit = self.units[i].clone();
        self.mark(&unit, next, Some(unit.id));
        // Vision spec, "When sight is applied": a unit that moves onto a new tile reveals from it at once.
        self.apply_unit_sight(i);
        // A transport moving as a one-tile unit keeps its order's animation (6 for orders 30 and 31): only a two-tile
        // mover starts the move animation (transports spec, "Landing" step 2 and "Pushing off" step 4).
        if (restart || unit.script.anim != anim::MOVE) && !unit.ship.one_tile {
            self.switch_anim(i, anim::MOVE);
        }
        // Sounds spec, "Docking": a transport moving onto a shore-water tile.
        if self.stats_of(unit.kind).flags & tidewater_formats::udta::flags::TRANSPORT != 0
            && self.terrain.word(next.0, next.1) & crate::terrain::bits::SHORE != 0
        {
            self.sound_on(i, crate::sound::Sound::Dock);
        }
        Walk::Stepping
    }

    /// The blocked rules: the move animation restarts in place, the counter goes up, and the first matching row
    /// decides (movement spec, "Blocked rules").
    fn blocked(&mut self, i: usize, next: (i32, i32), blocker: Option<UnitId>, g: Goal) {
        // The walking animation restarts in place, which for orders 30 and 31 is their own (the
        // second walking animation).
        let walking = match self.units[i].order {
            Order::Land { .. } | Order::PushOff => crate::orders::order_anim(self.units[i].order),
            _ => anim::MOVE,
        };
        self.start_anim(i, walking);
        let u = &mut self.units[i];
        u.blocked = (u.blocked + 1).min(GIVE_UP);
        let u = &self.units[i];
        let wall = self.terrain.word(next.0, next.1) & HUMAN_WALL != 0;
        let computer = self.computer_player(u.owner);
        let invisible = false;
        let attacked = u.attacked_by.is_some();
        let b = blocker.and_then(|b| self.unit(b));
        let drop_path = |w: &mut World| w.units[i].path.clear();
        let wait = if computer && wall && !invisible && !attacked {
            // Row A: attack the wall tile.
            self.units[i].pending = Some(Order::AttackWall { tile: next });
            drop_path(self);
            1
        } else if computer && wall {
            drop_path(self); // B
            1
        } else if u.blocked >= GIVE_UP {
            // C. Build-train spec, "Placing a building": the idle is a pending order.
            self.units[i].blocked = 0;
            // Orders spec ("Giving up when blocked"; `xhuman8-25000` troll destroyer 1441,
            // n = 11284): a remembered patrol or attack-move that resumes is given as a command and made the pending
            // order, and the unit starts its still animation with wait 1 instead of 15.
            // Traced (`orc10-10000` tanker 1533, n = 3505; `xhuman12-10000` axe thrower 1521, n = 4144):
            // when nothing resumes, idle is pending and nothing else changes, so the unit keeps the walking animation
            // restarted above, with wait 15; only the resume case starts the still animation. A
            // remembered attack-move that is complete is forgotten only with the countdown at 0, as everywhere idle
            // becomes pending (`resumed`).
            if let Some(o) = self.resumed(i) {
                self.units[i].pending = Some(o);
                self.start_anim(i, anim::STILL);
                1
            } else {
                self.units[i].pending = Some(Order::Idle);
                LONG_WAIT
            }
        } else if u.blocked >= LONG_WAIT_FROM {
            drop_path(self); // D
            LONG_WAIT
        } else if b.is_some_and(|b| {
            // E: the blocker is dying or leaving play. F: this unit in its move animation after the restart (a
            // transport in a plain move waits, a landing one doesn't), and an ally walking away
            // (its next step isn't onto us).
            b.dying
                || b.removing
                || (u.script.anim == anim::MOVE
                    && self.allied(u.owner, b.owner)
                    && b.script.anim == anim::MOVE
                    && b.blocked == 0
                    && !b.inside
                    && b.path.first().is_some_and(|&t| t != u.tile))
        }) {
            LONG_WAIT
        } else {
            let near = if u.layer == Layer::Land { 1 } else { 2 };
            let (dx, dy) = ((u.tile.0 - g.tile.0).abs(), (u.tile.1 - g.tile.1).abs());
            if attacked && dx <= near && dy <= near {
                // G. Static study only: "stand still" becomes the pending order, so the resume rules
                // of the orders spec apply.
                self.give(i, Order::Idle);
            } else {
                drop_path(self); // H, I
            }
            1
        };
        self.units[i].script.wait = wait;
    }

    /// Whether the path search treats a ground unit on a tile as free (pathfinding spec, "Units seen by the
    /// search"): enemies are those of non-allied players, and moving allies include the unit's own player.
    fn ignored_by_search(&self, u: &Unit, other: UnitId) -> bool {
        let Some(o) = self.unit(other) else { return true };
        // The searcher's own tile is no exception: it is free only under the moving-ally case
        // below; `orc4-20000` peasant 1578 at n = 6811, just taken over (wait 3, not moving), blocks its own walk.
        if o.layer == Layer::Air {
            return false;
        }
        let s = self.stats_of(u.kind);
        // Traced (`xhuman11-10000` paladin 1510, n = 4443): the spell-caster exclusion is the mage-type units
        // only (mage and death knight types); paladins and ogre mages count as fighters.
        let fighter = s.flags & (flags::PEON | flags::TANKER | flags::MAGE_LIKE) == 0
            && !crate::orders::DEMOLITION.contains(&u.kind);
        // Pathfinding spec, "Units seen by the search": the orders with the "acquires" mark (2, 4, 5, 7, 12 to 16 and
        // 32), and attack-move (10) in Enhanced mode. Attacking a unit (9) or a wall (11) is not attack-type.
        let attack_type = match u.order {
            Order::Idle
            | Order::MoveAttack { .. }
            | Order::Patrol { .. }
            | Order::Guard { .. }
            | Order::Chase { .. }
            | Order::StandGround
            | Order::TowerWatch { .. }
            | Order::AttackInPlace { .. }
            | Order::IdleReact { .. }
            | Order::Docked => true,
            Order::AttackMove { .. } => self.rules.enhanced,
            _ => false,
        };
        let allied = self.allied(u.owner, o.owner);
        // A kept dying attacker doesn't count for the "no attacker" test (`xorc01-death-coil` ogre
        // 1489 at n = 886, whose attacker 1515 is dying, killed by 1496).
        if attack_type && !self.live_attacker(u) && fighter && !allied {
            return true;
        }
        allied && (o.script.anim == anim::MOVE || (o.commanded && o.pending.is_some())) && o.blocked == 0
    }

    /// Up to 20 tiles of route from `u` toward `g` (pathfinding spec). Empty means no path.
    pub(crate) fn plan(&self, u: &Unit, g: Goal) -> Vec<(i32, i32)> {
        let s = if u.ship.one_tile {
            1
        } else {
            self.stride_of(u.kind).max(1)
        };
        let (w, h) = (self.terrain.width as i32, self.terrain.height as i32);
        let mut req = path::Request {
            enhanced: self.rules.enhanced,
            ..path::Request::to_tile(w, h, u.tile, g.tile, s)
        };
        if let Some(z) = g.zone {
            req.zone = z;
        }
        // Units of players that aren't human-controlled drop bit 0x04 from their mask, unless invisible or attacked.
        let drop_wall = !self.human_controlled(u.owner) && u.attacked_by.is_none();
        let class = if u.ship.landing {
            crate::terrain::mask::LANDING
        } else {
            u.layer.mask()
        };
        let mask = class & if drop_wall { !HUMAN_WALL } else { !0 };
        struct Search<'a> {
            w: &'a World,
            u: &'a Unit,
            mask: u16,
        }
        impl Search<'_> {
            fn terrain(&self, x: i32, y: i32) -> bool {
                let Some(n) = self.w.terrain.at(x, y) else { return true };
                let mut word = self.w.terrain.words[n];
                if self.w.occupancy[0][n].is_some() {
                    word &= !bits::UNIT;
                }
                if self.w.occupancy[1][n].is_some() {
                    word &= !bits::AIR_UNIT;
                }
                word & self.mask != 0
            }
        }
        impl path::Map for Search<'_> {
            fn blocked(&self, x: i32, y: i32) -> bool {
                let (w, u) = (self.w, self.u);
                self.terrain(x, y) || w.occupant(u.layer, x, y).is_some_and(|o| !w.ignored_by_search(u, o))
            }
            fn blocked_strict(&self, x: i32, y: i32) -> bool {
                let (w, u) = (self.w, self.u);
                self.terrain(x, y) || w.occupant(u.layer, x, y).is_some_and(|o| o != u.id)
            }
        }
        let mut at = u.tile;
        path::search(&req, &Search { w: self, u, mask })
            .into_iter()
            .map(|d| {
                let v = path::DIRECTIONS[d as usize];
                at = (at.0 + v.0 * s, at.1 + v.1 * s);
                at
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use crate::world::tests::{field, run};

    fn order(w: &mut World, id: UnitId, o: Command) {
        let _ = id;
        w.step(&[o]);
    }

    fn mv(w: &mut World, id: UnitId, to: (i32, i32)) {
        let player = w.unit(id).unwrap().owner;
        order(
            w,
            id,
            Command::Move {
                player,
                units: vec![id],
                to,
            },
        );
    }

    /// A one-tile-wide corridor along y = 1.
    fn corridor(w: u16) -> World {
        let mut world = field(w, 3);
        for x in 0..w as i32 {
            world.terrain.set_bits(x, 0, 0x80);
            world.terrain.set_bits(x, 2, 0x80);
        }
        world
    }

    /// Steps until `f` holds, up to `max` cycles; the number of steps taken.
    fn until(w: &mut World, max: u32, f: impl Fn(&World) -> bool) -> u32 {
        for n in 0..max {
            if f(w) {
                return n;
            }
            w.step(&[]);
        }
        panic!("condition not met in {max} cycles");
    }

    #[test]
    fn the_search_treats_allies_as_allies() {
        // Pathfinding spec, "Units seen by the search": an attacking unit ignores units of non-allied players only,
        // and a moving unit of an allied player is ignored like one of the searcher's own.
        let mut w = field(12, 4);
        w.players[0].allies = 1 << 1;
        let a = w.spawn(0, 0, (1, 1), 0).unwrap();
        let ally = w.spawn(0, 1, (3, 1), 0).unwrap();
        let enemy = w.spawn(0, 2, (5, 1), 0).unwrap();
        let i = w.units.iter().position(|u| u.id == a).unwrap();
        w.units[i].order = Order::Chase { target: enemy };
        let u = w.units[i].clone();
        assert!(w.ignored_by_search(&u, enemy));
        assert!(!w.ignored_by_search(&u, ally));
        let j = w.units.iter().position(|u| u.id == ally).unwrap();
        w.units[j].script.anim = anim::MOVE;
        assert!(w.ignored_by_search(&u, ally));
    }

    #[test]
    fn attack_type_orders_are_the_acquiring_ones() {
        // Pathfinding spec, "Units seen by the search": orders 2, 4, 5, 7 and 12 to 16 (and 10 in Enhanced mode) go
        // through enemies; attacking a unit (9) or a wall (11) doesn't, and neither do demolition units.
        let mut w = field(12, 4);
        let a = w.spawn(0, 0, (1, 1), 0).unwrap();
        let enemy = w.spawn(0, 2, (5, 1), 0).unwrap();
        let i = w.index(a).unwrap();
        let seen = |w: &mut World, o: Order| {
            w.units[i].order = o;
            let u = w.units[i].clone();
            w.ignored_by_search(&u, enemy)
        };
        let goal = (8, 1);
        for o in [
            Order::Idle,
            Order::MoveAttack { goal, target: None },
            Order::Patrol { to: goal, back: (1, 1) },
            Order::Chase { target: enemy },
            Order::StandGround,
        ] {
            assert!(seen(&mut w, o), "{o:?}");
        }
        for o in [
            Order::Move { goal, target: None },
            Order::Attack { target: enemy },
            Order::AttackWall { tile: goal },
        ] {
            assert!(!seen(&mut w, o), "{o:?}");
        }
        w.rules.enhanced = true;
        assert!(seen(&mut w, Order::AttackMove { goal }));
        w.rules.enhanced = false;
        assert!(!seen(&mut w, Order::AttackMove { goal }));
        // An attacker, or a demolition unit, sees every unit.
        w.units[i].kind = crate::orders::DEMOLITION[0];
        assert!(!seen(&mut w, Order::Chase { target: enemy }));
        w.units[i].kind = 0;
        // Traced (`xhuman11-10000` paladin 1510): a caster that isn't mage-type (a paladin or an ogre mage)
        // counts as a fighter; a mage-type unit sees every unit.
        let plain = w.stats.0[0].flags;
        w.stats.0[0].flags = plain | flags::CASTER;
        assert!(seen(&mut w, Order::Chase { target: enemy }));
        w.stats.0[0].flags = plain | flags::CASTER | flags::MAGE_LIKE;
        assert!(!seen(&mut w, Order::Chase { target: enemy }));
        w.stats.0[0].flags = plain;
        w.units[i].attacked_by = Some(enemy);
        assert!(!seen(&mut w, Order::Chase { target: enemy }));
    }

    #[test]
    fn a_goal_on_a_building_is_pulled_back_toward_the_unit() {
        // Pathfinding spec, "Goal pulled back", with the xorc7 vectors' geometry on land: a 3 x 3 building covering
        // (6..8, 18..20). From (36, 64) the walk back meets (6, 18), (7, 19), (7, 20), then usable (8, 21): goal
        // (7, 20). From (58, 4): (6, 18), (7, 18), then (8, 17): goal (7, 18).
        let mut w = field(70, 70);
        w.stats.0[0x4a].size = (3, 3);
        w.stats.0[0x4a].flags |= flags::BUILDING;
        w.spawn(0x4a, 1, (6, 18), 0).unwrap();
        for (from, want) in [((36, 64), (7, 20)), ((58, 4), (7, 18))] {
            let f = w.spawn(0, 0, from, 0).unwrap();
            let i = w.index(f).unwrap();
            assert_eq!(w.pull_back(i, (6, 18)), want, "from {from:?}");
            // A usable goal is kept, and the command stores the pulled-back goal.
            assert_eq!(w.pull_back(i, (20, 20)), (20, 20));
            w.units[i].blocked = 5;
            w.command(
                i,
                Order::Move {
                    goal: (6, 18),
                    target: None,
                },
            );
            let u = &w.units[i];
            assert_eq!(
                (u.pending, u.blocked, u.commanded),
                (
                    Some(Order::Move {
                        goal: want,
                        target: None
                    }),
                    0,
                    true
                )
            );
        }
    }

    /// Pathfinding spec, "Goal pulled back", the `orc8` vectors: the line's steps are subtracted
    /// from the goal first step first. From (123, 96) to the tree (124, 90) the steps are N, N, NE, N, N, N; with a
    /// 3 x 3 church at (124, 91) the tested tiles are (124, 91) and (124, 92), buildings, then (123, 93), usable: goal
    /// (124, 92), not the (124, 93) that walking the line's own tiles back would give. From (123, 93) (steps NE, N,
    /// N) the first tested tile (123, 91) is usable, so the goal is kept.
    #[test]
    fn the_pull_back_subtracts_the_lines_steps_first_step_first() {
        let mut w = field(130, 100);
        w.stats.0[0x4a].size = (3, 3);
        w.stats.0[0x4a].flags |= flags::BUILDING;
        w.spawn(0x4a, 1, (124, 91), 0).unwrap();
        w.terrain.set_bits(124, 90, bits::OBSTACLE);
        w.terrain.set_bits(125, 90, bits::OBSTACLE);
        let f = w.spawn(0, 0, (123, 96), 0).unwrap();
        let i = w.index(f).unwrap();
        assert_eq!(
            path::line((123, 96), (124, 90), 1),
            vec![0, 0, 1, 0, 0, 0],
            "N, N, NE, N, N, N"
        );
        assert_eq!(w.pull_back(i, (124, 90)), (124, 92));
        let unit = w.units[i].clone();
        w.mark(&unit, unit.tile, None);
        w.units[i].tile = (123, 93);
        assert_eq!(w.pull_back(i, (124, 90)), (124, 90));
    }

    /// Pathfinding spec, the k − 1 vector: from (0, 0) to (0, 3) with (0, 1) to (0, 3) all unusable, only (0, 2) and
    /// (0, 1) are tested; none is usable, so the goal is kept, for a transport too (the fallback to
    /// the unit's tile plus the last step belongs to the transport's own pull-back).
    #[test]
    fn the_pull_back_tests_k_minus_1_tiles_and_keeps_the_goal() {
        let mut w = field(20, 20);
        for y in 1..=3 {
            w.terrain.set_bits(0, y, bits::OBSTACLE);
        }
        let f = w.spawn(0, 0, (0, 0), 0).unwrap();
        let i = w.index(f).unwrap();
        assert_eq!(w.pull_back(i, (0, 3)), (0, 3));
        w.stats.0[0].flags |= flags::TRANSPORT;
        assert_eq!(w.pull_back(i, (0, 3)), (0, 3));
    }

    /// Movement spec, "Blocked rules", and the pathfinding vector: a spell command (orders 38 to 56) with blocked
    /// counter 5 leaves it 0 and its tile is not pulled back; an order the unit's own logic gives clears it too.
    #[test]
    fn every_command_clears_the_blocked_counter_at_once() {
        let mut w = field(70, 70);
        w.stats.0[0x4a].size = (3, 3);
        w.stats.0[0x4a].flags |= flags::BUILDING;
        w.spawn(0x4a, 1, (6, 18), 0).unwrap();
        let f = w.spawn(0, 0, (36, 64), 0).unwrap();
        let i = w.index(f).unwrap();
        w.units[i].blocked = 5;
        let cast = Order::Cast {
            spell: crate::spells::spell::HASTE,
            target: None,
            tile: (6, 18),
        };
        w.command(i, cast);
        let u = &w.units[i];
        assert_eq!((u.pending, u.blocked), (Some(cast), 0));
        // An order made pending without a command keeps the counter.
        w.units[i].blocked = 5;
        w.give(i, Order::Idle);
        assert_eq!(w.units[i].blocked, 5);
    }

    /// Movement spec, "Blocked rules" (the `orc8-10000` n = 947 vector): a pending order taking over
    /// doesn't clear the blocked counter; only commands do.
    #[test]
    fn a_take_over_keeps_the_blocked_counter() {
        let mut w = field(12, 12);
        let p = w.spawn(0, 0, (6, 6), 0).unwrap();
        let i = w.index(p).unwrap();
        w.units[i].blocked = 2;
        w.take_over(
            i,
            Order::Move {
                goal: (1, 1),
                target: None,
            },
        );
        assert_eq!(w.units[i].blocked, 2);
        w.command(
            i,
            Order::Move {
                goal: (2, 2),
                target: None,
            },
        );
        assert_eq!(w.units[i].blocked, 0);
    }

    /// Movement spec, "Blocked rules", row F (traced by `human12-row-f-move` and
    /// `human12-row-f-landing`): the blocked unit itself must be in its move animation, which a transport in order 30
    /// or 31 never is (it restarts its own walking animation), so it doesn't wait for an ally walking away; a unit in
    /// an ordinary walk does.
    #[test]
    fn row_f_needs_the_blocked_unit_in_its_move_animation() {
        for (order, waits) in [
            (
                Order::Move {
                    goal: (5, 1),
                    target: None,
                },
                true,
            ),
            (Order::Land { goal: (5, 1) }, false),
        ] {
            let mut w = field(12, 4);
            let a = w.spawn(0, 0, (1, 1), 0).unwrap();
            let b = w.spawn(0, 0, (2, 1), 0).unwrap();
            let (i, j) = (w.index(a).unwrap(), w.index(b).unwrap());
            w.units[j].script.anim = anim::MOVE;
            w.units[j].path = vec![(3, 1)];
            w.units[i].order = order;
            w.blocked(i, (2, 1), Some(b), Goal::tile((5, 1), 0));
            let u = &w.units[i];
            assert_eq!(u.blocked, 1);
            assert_eq!(u.script.wait == LONG_WAIT, waits, "{order:?}");
            let walking = if waits { anim::MOVE } else { anim::WORK_B };
            assert_eq!(u.script.anim, walking);
        }
    }

    /// Pathfinding spec, the `orc4-20000` vectors: the searcher's own tile is free only under the
    /// moving-ally case (in its move animation with blocked counter 0, or with a command pending).
    #[test]
    fn the_searchers_own_tile_follows_the_unit_rules() {
        let mut w = field(12, 12);
        let p = w.spawn(0, 0, (6, 6), 0).unwrap();
        let i = w.index(p).unwrap();
        let free = |w: &World| w.ignored_by_search(&w.units[i], p);
        w.units[i].order = Order::Move {
            goal: (4, 3),
            target: None,
        };
        w.units[i].script.anim = anim::ATTACK;
        assert!(!free(&w), "just taken over, not moving: it blocks");
        w.units[i].script.anim = anim::MOVE;
        assert!(free(&w), "in its move animation with counter 0");
        w.units[i].blocked = 2;
        assert!(!free(&w), "counter 2");
        w.units[i].blocked = 0;
        w.units[i].script.anim = anim::STILL;
        w.command(
            i,
            Order::Move {
                goal: (1, 1),
                target: None,
            },
        );
        assert!(free(&w), "a command pending");
    }

    #[test]
    fn an_ally_with_a_command_pending_is_ignored_by_the_search() {
        // The orc4 vector: a peasant out of a mine, idle with a command pending, is "about to move"; once the order
        // has taken over (not moving), it blocks the search.
        let mut w = field(12, 4);
        let a = w.spawn(0, 0, (1, 1), 0).unwrap();
        let p = w.spawn(0, 0, (2, 1), 0).unwrap();
        let (i, j) = (w.index(a).unwrap(), w.index(p).unwrap());
        let u = w.units[i].clone();
        assert!(!w.ignored_by_search(&u, p));
        w.command(
            j,
            Order::Move {
                goal: (8, 2),
                target: None,
            },
        );
        assert!(w.ignored_by_search(&u, p));
        let o = w.units[j].pending.take().unwrap();
        w.take_over(j, o);
        assert!(!w.ignored_by_search(&u, p));
        // An internal pending order isn't a command.
        w.give(j, Order::Idle);
        assert!(!w.ignored_by_search(&u, p));
    }

    #[test]
    fn attacking_a_unit_walks_round_an_enemy_in_the_way() {
        // Order 9 is not attack-type: the search sees the idle enemy grunt on the line and walks round it, so the
        // footman is never blocked.
        let mut w = field(12, 5);
        let f = w.spawn(0, 0, (1, 2), 0).unwrap();
        w.spawn(0, 1, (2, 2), 0).unwrap();
        let foe = w.spawn(0, 1, (8, 2), 0).unwrap();
        order(
            &mut w,
            f,
            Command::Attack {
                player: 0,
                units: vec![f],
                target: foe,
            },
        );
        until(&mut w, 30, |w| w.unit(f).unwrap().tile != (1, 2));
        let u = w.unit(f).unwrap();
        assert_ne!(u.tile, (2, 2));
        assert_eq!(u.blocked, 0);
    }

    #[test]
    fn five_tiles_east_one_loop_per_step() {
        // The synthetic move loop is 8 cycles (the spec's footman takes 16).
        let mut w = field(12, 4);
        let f = w.spawn(0, 0, (1, 1), 0).unwrap();
        mv(&mut w, f, (6, 1));
        let (mut last, mut changes, mut idle) = ((1, 1), vec![], None);
        for _ in 0..100 {
            w.step(&[]);
            let u = w.unit(f).unwrap();
            if u.tile != last {
                last = u.tile;
                changes.push(w.tick);
            }
            if idle.is_none() && u.order == Order::Idle && u.pending.is_none() {
                idle = Some(w.tick);
            }
        }
        assert_eq!(changes.len(), 5);
        assert!(changes.windows(2).all(|c| c[1] - c[0] == 8), "{changes:?}");
        // The order takes over within the still loop (5 cycles), and the first step comes 3 cycles later.
        assert!(changes[0] <= 1 + 5 + 3, "{changes:?}");
        assert_eq!(idle.unwrap() - changes[4], 8);
        let u = w.unit(f).unwrap();
        assert_eq!((u.tile, u.pos), ((6, 1), (192, 32)));
    }

    #[test]
    fn a_new_order_waits_for_the_step_to_finish() {
        let mut w = field(12, 4);
        let f = w.spawn(0, 0, (1, 1), 0).unwrap();
        mv(&mut w, f, (8, 1));
        until(&mut w, 20, |w| w.unit(f).unwrap().tile == (2, 1));
        mv(&mut w, f, (0, 1));
        // The step east is finished first: the pixel position reaches tile 2 before the unit turns back.
        let n = until(&mut w, 20, |w| w.unit(f).unwrap().pos.0 == 64);
        assert!(n <= 8);
        assert_eq!(
            w.unit(f).unwrap().order,
            Order::Move {
                goal: (0, 1),
                target: None
            }
        );
        until(&mut w, 20, |w| w.unit(f).unwrap().tile == (1, 1));
    }

    #[test]
    fn pixel_positions_at_rest() {
        let mut w = field(16, 16);
        let f = w.spawn(0, 0, (5, 7), 0).unwrap();
        w.stats.0[0x4a].size = (4, 4);
        let h = w.spawn(0x4a, 0, (10, 10), 0).unwrap();
        assert_eq!(w.unit(f).unwrap().pos, (160, 224));
        assert_eq!(w.unit(h).unwrap().pos, (320, 320));
        run(&mut w, 50);
        assert_eq!(w.unit(h).unwrap().pos, (320, 320));
    }

    #[test]
    fn a_stationary_enemy_in_a_corridor_ends_the_move() {
        let mut w = corridor(10);
        let f = w.spawn(0, 0, (2, 1), 0).unwrap();
        w.spawn(5, 1, (3, 1), 0).unwrap();
        mv(&mut w, f, (7, 1));
        until(&mut w, 40, |w| {
            let u = w.unit(f).unwrap();
            u.order == Order::Idle && u.pending.is_none()
        });
        assert_eq!(w.unit(f).unwrap().tile, (2, 1));
    }

    #[test]
    fn attacking_through_a_blocker_gives_up_after_15_blocks() {
        let mut w = corridor(10);
        let f = w.spawn(0, 0, (2, 1), 0).unwrap();
        w.spawn(5, 1, (3, 1), 0).unwrap();
        let foe = w.spawn(0, 1, (6, 1), 0).unwrap();
        // Movement spec vector: chasing a found target (order 12) beyond the blocker. The search ignores it.
        let i = w.index(f).unwrap();
        w.take_over(i, Order::Chase { target: foe });
        let first = until(&mut w, 40, |w| w.unit(f).unwrap().blocked == 1);
        assert_eq!(w.unit(f).unwrap().order, Order::Chase { target: foe });
        // Blocks 1 to 7 retry the next cycle, blocks 8 to 14 wait 15 cycles, block 15 gives up: the idle is a pending
        // order.
        let more = until(&mut w, 200, |w| w.unit(f).unwrap().pending == Some(Order::Idle));
        let _ = first;
        assert_eq!(more, 7 + 7 * 15);
        let u = w.unit(f).unwrap();
        assert_eq!((u.blocked, u.tile), (0, (2, 1)));
        until(&mut w, 40, |w| w.unit(f).unwrap().order == Order::Idle);
    }

    /// Row C (orders spec, "Giving up when blocked"; the `xhuman8-25000` troll destroyer 1441
    /// vector, n = 11284): a unit that gives up at the 15th block while it remembers a patrol, its resume countdown 0,
    /// starts its still animation with wait 1 and gets the patrol as its pending order; with the countdown running it
    /// gives up as before (idle pending, wait 15).
    #[test]
    fn row_c_resumes_a_remembered_patrol_at_once() {
        for countdown in [0, 5] {
            let mut w = corridor(10);
            let f = w.spawn(0, 1, (2, 1), 0).unwrap();
            let b = w.spawn(5, 1, (3, 1), 0).unwrap();
            let i = w.index(f).unwrap();
            let u = &mut w.units[i];
            u.order = Order::Patrol {
                to: (7, 1),
                back: (1, 1),
            };
            u.remembered = Some(crate::world::Remembered::Patrol {
                to: (7, 1),
                back: (1, 1),
            });
            u.resume = countdown;
            u.blocked = 14;
            u.path = vec![(3, 1)];
            w.blocked(i, (3, 1), Some(b), Goal::tile((7, 1), 0));
            let u = &w.units[i];
            assert_eq!(u.blocked, 0);
            if countdown == 0 {
                assert!(matches!(u.pending, Some(Order::Patrol { .. })), "{:?}", u.pending);
                assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 1));
            } else {
                // Nothing resumes, so the restarted walking animation is kept, with wait 15.
                assert_eq!(u.pending, Some(Order::Idle));
                assert_eq!((u.script.anim, u.script.wait), (anim::MOVE, LONG_WAIT));
            }
        }
    }

    /// Row C with nothing to resume (orders spec, "Giving up when blocked"; the `orc10-10000` tanker
    /// 1533 and `xhuman12-10000` axe thrower 1521 vectors): idle pending, the walking animation (3) kept, wait 15. A
    /// remembered attack-move within 1 tile of the unit is complete and forgotten there; one further away is kept while
    /// the countdown runs. A complete one is forgotten only at countdown 0, and kept while it runs.
    #[test]
    fn row_c_with_nothing_to_resume_keeps_walking_and_forgets_a_complete_attack_move() {
        for (goal, countdown, kept) in [
            (None, 0, false),
            (Some((3, 2)), 0, false),
            (Some((3, 2)), 5, true),
            (Some((7, 1)), 5, true),
        ] {
            let mut w = corridor(10);
            w.rules.enhanced = true;
            let f = w.spawn(0, 1, (2, 1), 0).unwrap();
            let b = w.spawn(5, 1, (3, 1), 0).unwrap();
            let i = w.index(f).unwrap();
            let u = &mut w.units[i];
            u.order = Order::Move {
                goal: (7, 1),
                target: None,
            };
            u.remembered = goal.map(|goal| crate::world::Remembered::AttackMove { goal });
            u.resume = countdown;
            u.blocked = 14;
            u.path = vec![(3, 1)];
            w.blocked(i, (3, 1), Some(b), Goal::tile((7, 1), 0));
            let u = &w.units[i];
            assert_eq!(u.pending, Some(Order::Idle), "{goal:?}");
            assert_eq!((u.script.anim, u.script.wait), (anim::MOVE, LONG_WAIT), "{goal:?}");
            assert_eq!(u.remembered.is_some(), kept, "{goal:?}");
        }
    }

    /// Unit `f` mid-move at `at`, its cached path pointing at `next`.
    fn stepping_into(w: &mut World, f: UnitId, goal: (i32, i32), next: (i32, i32)) -> usize {
        let i = w.index(f).unwrap();
        w.units[i].order = Order::Move { goal, target: None };
        w.units[i].path = vec![next];
        i
    }

    #[test]
    fn computer_units_attack_human_walls() {
        let mut w = field(10, 3);
        w.terrain.set_bits(3, 1, 0x8d);
        w.players[1].controller = controller::COMPUTER;
        w.players[2].controller = controller::RESCUE_PASSIVE;
        let grunt = w.spawn(0, 1, (2, 1), 0).unwrap();
        let i = stepping_into(&mut w, grunt, (7, 1), (3, 1));
        assert_eq!(w.move_toward(i, Goal::tile((7, 1), 0)), Walk::Blocked);
        let u = &w.units[i];
        assert_eq!(u.pending, Some(Order::AttackWall { tile: (3, 1) }));
        assert_eq!((u.script.wait, u.path.len()), (1, 0));
        // A passive computer player's unit falls through to the rows below.
        let other = w.spawn(0, 2, (2, 0), 0).unwrap();
        let j = stepping_into(&mut w, other, (7, 0), (3, 1));
        w.move_toward(j, Goal::tile((7, 0), 0));
        assert_eq!(w.units[j].pending, None);
    }

    #[test]
    fn allies_walking_away_are_waited_for() {
        let mut w = field(10, 3);
        let f = w.spawn(0, 0, (2, 1), 0).unwrap();
        let ally = w.spawn(0, 0, (3, 1), 0).unwrap();
        let a = w.index(ally).unwrap();
        w.start_anim(a, anim::MOVE);
        w.units[a].path = vec![(4, 1)];
        let i = stepping_into(&mut w, f, (7, 1), (3, 1));
        w.move_toward(i, Goal::tile((7, 1), 0));
        let u = &w.units[i];
        assert_eq!((u.blocked, u.script.wait, u.path.clone()), (1, 15, vec![(3, 1)]));
        // Two allies stepping onto each other's tiles don't wait (row I).
        let a = w.index(ally).unwrap();
        w.units[a].path = vec![(2, 1)];
        let i = stepping_into(&mut w, f, (7, 1), (3, 1));
        w.move_toward(i, Goal::tile((7, 1), 0));
        let u = &w.units[i];
        assert_eq!((u.blocked, u.script.wait, u.path.len()), (2, 1, 0));
        // With a counter of 7, row D comes first.
        w.units[i].blocked = 7;
        let a = w.index(ally).unwrap();
        w.units[a].path = vec![(4, 1)];
        let i = stepping_into(&mut w, f, (7, 1), (3, 1));
        w.move_toward(i, Goal::tile((7, 1), 0));
        let u = &w.units[i];
        assert_eq!((u.blocked, u.script.wait, u.path.len()), (8, 15, 0));
    }

    #[test]
    fn attacked_units_stop_near_their_goal() {
        let mut w = field(10, 3);
        let f = w.spawn(0, 0, (2, 1), 0).unwrap();
        let foe = w.spawn(0, 1, (3, 1), 0).unwrap();
        let i = stepping_into(&mut w, f, (3, 1), (3, 1));
        w.units[i].attacked_by = Some(foe);
        let before = w.units[i].order;
        w.move_toward(i, Goal::tile((3, 1), 0));
        // Row G: idle becomes the pending order; the order is unchanged in that update, wait 1.
        let u = &w.units[i];
        assert_eq!((u.order, u.pending, u.script.wait), (before, Some(Order::Idle), 1));
    }

    /// Row G's pending idle goes through the resume rules: a remembered attack-move is given again,
    /// as a command.
    #[test]
    fn row_g_resumes_a_remembered_attack_move() {
        let mut w = field(10, 3);
        w.rules.enhanced = true;
        let f = w.spawn(0, 0, (2, 1), 0).unwrap();
        let foe = w.spawn(0, 1, (3, 1), 0).unwrap();
        let i = stepping_into(&mut w, f, (3, 1), (3, 1));
        w.units[i].attacked_by = Some(foe);
        w.units[i].remembered = Some(crate::world::Remembered::AttackMove { goal: (8, 1) });
        w.move_toward(i, Goal::tile((3, 1), 0));
        let u = &w.units[i];
        assert_eq!(u.pending, Some(Order::AttackMove { goal: (8, 1) }));
        assert_eq!((u.blocked, u.commanded), (0, true));
    }

    #[test]
    fn flyers_get_one_more_tile_of_radius() {
        let mut w = field(10, 6);
        let g = w.spawn(2, 0, (0, 2), 0).unwrap();
        mv(&mut w, g, (5, 3));
        until(&mut w, 100, |w| {
            let u = w.unit(g).unwrap();
            u.order == Order::Idle && u.pending.is_none()
        });
        assert_eq!(w.unit(g).unwrap().tile, (4, 2));
    }

    #[test]
    fn attack_in_range_doesnt_move() {
        let mut w = field(10, 3);
        let archer = w.spawn(8, 0, (1, 1), 0).unwrap();
        let foe = w.spawn(0, 1, (4, 1), 0).unwrap();
        order(
            &mut w,
            archer,
            Command::Attack {
                player: 0,
                units: vec![archer],
                target: foe,
            },
        );
        until(&mut w, 20, |w| w.unit(archer).unwrap().script.anim == anim::ATTACK);
        assert_eq!(w.unit(archer).unwrap().tile, (1, 1));
    }

    #[test]
    fn footman_attacking_a_town_hall() {
        let mut w = field(16, 16);
        w.stats.0[0x4a].size = (4, 4);
        let f = w.spawn(0, 0, (6, 11), 0).unwrap();
        let hall = w.spawn(0x4a, 1, (10, 10), 0).unwrap();
        let (u, h) = (w.unit(f).unwrap(), w.unit(hall).unwrap());
        assert_eq!(building_goal(u.tile, h), (10, 12));
        let g = Goal {
            tile: (10, 12),
            radius: 1,
            range: true,
            zone: Some((9, 9, 14, 14)),
        };
        assert_eq!(w.plan(u, g), vec![(7, 11), (8, 11), (9, 11)]);
        order(
            &mut w,
            f,
            Command::Attack {
                player: 0,
                units: vec![f],
                target: hall,
            },
        );
        // Order 9 starts its attack animation at takeover, so wait for the footman to stop walking.
        until(&mut w, 100, |w| w.unit(f).unwrap().tile == (9, 11));
        run(&mut w, 40);
        let u = w.unit(f).unwrap();
        assert_eq!((u.tile, u.script.anim), ((9, 11), anim::ATTACK));
    }
}
