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

/// The rows of the blocked rules (movement spec, "Blocked rules"), tested in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    /// A computer player's unit, a wall tile with bit 0x04, not invisible and no attacker: attack the wall.
    A,
    /// As A, but invisible or with an attacker: search again.
    B,
    /// Counter 15: give up.
    C,
    /// Counter 8 to 14: search again after 15 cycles.
    D,
    /// The blocker is dying or leaving play: wait 15 cycles.
    E,
    /// An ally walking away: wait 15 cycles.
    F,
    /// An attacker, near the goal: stand still.
    G,
    /// An attacker, a sea or air unit within 2 tiles of the goal on x but farther on y: search again.
    H,
    /// Anything else: search again next cycle.
    I,
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

/// The pull-back's fallback when no tile is usable (pathfinding spec, "One pull-back, two settings").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoUsable {
    /// The command's pull-back: the goal is kept.
    KeepGoal,
    /// The transport's pull-back: the tile tested last, the unit's tile plus the line's last step.
    LastTile,
}

/// One pull-back, two settings (pathfinding spec, "The model" and "Goal pulled back"; transports spec, "Land and shore
/// targets", step 2.1): the goal `g`, then `g` minus the steps of the line from `from` (one-tile steps, "Line rule"),
/// first step first, stopping before `from`, are tested in turn. At the first `usable` tile the goal becomes the tile
/// tested just before it (`g` itself when `g` is usable); with none usable, `none` decides.
pub(crate) fn pulled_back(
    from: (i32, i32),
    g: (i32, i32),
    usable: impl Fn((i32, i32)) -> bool,
    none: NoUsable,
) -> (i32, i32) {
    let (mut tested, mut before) = (g, g);
    // The line's k steps give k tiles to test: g and the first k - 1 subtractions (the k-th would be `from`).
    for d in path::line(from, g, 1) {
        if usable(tested) {
            return before;
        }
        before = tested;
        let v = path::DIRECTIONS[d as usize];
        tested = (tested.0 - v.0, tested.1 - v.1);
    }
    match none {
        NoUsable::KeepGoal => g,
        NoUsable::LastTile => before,
    }
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
        // Orders spec, "Resuming patrol and attack-move": a pending idle goes by the common path, which may resume
        // the remembered order as a command.
        if order == Order::Idle {
            self.pending_idle(i);
            return;
        }
        // Movement spec, "Blocked rules": an order made pending without a command keeps the
        // blocked counter; `command` clears it.
        let u = &mut self.units[i];
        u.pending = Some(order);
        u.commanded = false;
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
            // Attack-ground-demolish spec, "Command 17", step 3: order 18's goal is the target tile pulled back; the
            // target tile itself is kept (`orc9-orc09-ground-move-goals-1100` n = 22 and 42, V267 and V268).
            Order::AttackGroundMove { tile, goal } => Order::AttackGroundMove {
                tile,
                goal: self.pull_back(i, goal),
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

    /// The command's pull-back (pathfinding spec, "Goal pulled back"): [`pulled_back`] from the unit's tile with the
    /// command's settings. Usable for the pull-back ("Three tile tests"): no terrain or building bit of the unit's
    /// mask (units are ignored) and, for a unit that doesn't fly, the unit's region. With no tile usable the goal is
    /// kept, for transports too (V29).
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
        pulled_back(u.tile, g, usable, NoUsable::KeepGoal)
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

    /// One walk goal (pathfinding spec, "The model"; "G for a walk with a target unit"): the walk toward target unit
    /// `t`, set again from where `t` stands every time the walk runs. Its goal is [`World::target_goal`], which the
    /// arrival test measures to and the search alone rounds to even; its goal zone is `t`'s footprint widened by one
    /// tile ("Step 1").
    pub(crate) fn target_walk(&self, u: &Unit, t: &Unit, radius: i32) -> Goal {
        Goal {
            zone: Some(crate::battle::goal_zone(t)),
            ..Goal::tile(self.target_goal(u, t), radius)
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

    /// [`World::move_toward`]: the walk at a step boundary (movement spec, "The model"), its four steps in order. With
    /// `restart`, a step starts the move animation and runs it at once even if it is already playing (a two-tile
    /// transport landing or pushing off, transports spec, "Moving near the shore").
    pub(crate) fn move_toward_as(&mut self, i: usize, g: Goal, restart: bool) -> Walk {
        // 1. Arrival: measured to the stored goal, not the one the search rounds to even (pathfinding spec, "One walk
        // goal"; V20 there).
        if self.at_goal(i, g) {
            self.units[i].path.clear();
            return Walk::Arrived;
        }
        // 2. Next direction: from the cached path, searched first when it is used up; no steps is no route.
        let Some(next) = self.next_tile(i, g) else {
            return Walk::NoPath;
        };
        // 3. Blocked test.
        if let Some(blocker) = self.step_blocked(i, next) {
            self.blocked(i, next, blocker, g);
            return Walk::Blocked;
        }
        // 4. The step.
        self.take_step(i, next, restart);
        Walk::Stepping
    }

    /// The walk's step 2 ("Paths"): the next tile of the cached path, searched first if the cache is used up. `None`
    /// when the search returns no steps.
    fn next_tile(&mut self, i: usize, g: Goal) -> Option<(i32, i32)> {
        if self.units[i].path.is_empty() {
            let path = self.plan(&self.units[i], g);
            if path.is_empty() {
                return None;
            }
            self.units[i].path = path;
        }
        Some(self.units[i].path[0])
    }

    /// The movement class a unit moves in (occupancy spec): its layer's, or the landing class while a transport lands
    /// and after it has landed (transports spec, "Moving near the shore").
    fn movement_class(u: &Unit) -> u16 {
        if u.ship.landing {
            crate::terrain::mask::LANDING
        } else {
            u.layer.mask()
        }
    }

    /// The walk's step 3, the step's own tile test ("Paths"; pathfinding spec, "Three tile tests"): any other unit on
    /// the next tile blocks it, and so does terrain of the unit's movement class. `Some` with the blocking unit, if
    /// any, when the step is blocked.
    fn step_blocked(&self, i: usize, next: (i32, i32)) -> Option<Option<UnitId>> {
        let u = &self.units[i];
        let occupant = self.occupant(u.layer, next.0, next.1).filter(|&o| o != u.id);
        let free = occupant.is_none() && self.terrain_allows_mask(Self::movement_class(u), next.0, next.1);
        (!free).then_some(occupant)
    }

    /// The walk's step 4 ("Positions and steps"): the old tile is left and the new one entered at once, the unit
    /// faces the step, a human player's sight is applied from the new tile, and the move animation carries the pixel
    /// position across.
    fn take_step(&mut self, i: usize, next: (i32, i32), restart: bool) {
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
        // mover starts the move animation (transports spec, "Moving near the shore", "Animation").
        if (restart || unit.script.anim != anim::MOVE) && !unit.ship.one_tile {
            self.switch_anim(i, anim::MOVE);
        }
        // Sounds spec, "Docking": a transport moving onto a shore-water tile. Transports spec ("Moving near the
        // shore", "Docking sound") puts pushing off's sound on the step leaving shore water instead; we keep one rule
        // for both until the docking sound is implemented (open question there).
        if self.stats_of(unit.kind).flags & flags::TRANSPORT != 0
            && self.terrain.word(next.0, next.1) & bits::SHORE != 0
        {
            self.sound_on(i, crate::sound::Sound::Dock);
        }
    }

    /// The blocked rules (movement spec, "Blocked rules"): the walking animation restarts in place, the counter goes
    /// up, and the first matching row of the table decides ([`World::blocked_row`]) and sets the next decision.
    fn blocked(&mut self, i: usize, next: (i32, i32), blocker: Option<UnitId>, g: Goal) {
        // The walking animation restarts in place: the move animation, or animation 6 for orders 30 and 31, whose own
        // animation it is (V19, V20).
        let walking = match self.units[i].order {
            Order::Land { .. } | Order::PushOff => crate::orders::order_anim(self.units[i].order),
            _ => anim::MOVE,
        };
        self.start_anim(i, walking);
        let u = &mut self.units[i];
        u.blocked = (u.blocked + 1).min(GIVE_UP);
        let drop_path = |w: &mut World| w.units[i].path.clear();
        let wait = match self.blocked_row(i, next, blocker, g) {
            Row::A => {
                // Order 11 on the blocking tile as a pending order, set directly (orders spec, "Order 11", step 5;
                // V22). Movement spec open question 1 (static study): the program gives it as a command; kept as
                // the specs' rule until a trace decides.
                self.units[i].pending = Some(Order::AttackWall { tile: next });
                drop_path(self);
                1
            }
            Row::B | Row::H | Row::I => {
                drop_path(self);
                1
            }
            Row::C => {
                // Giving up clears the counter, and idle becomes the pending order (orders spec, "Giving up when
                // blocked"): a remembered patrol or attack-move that resumes instead is given as a command and starts
                // the still animation with wait 1 (V33); with nothing to resume the walking animation restarted above
                // is kept, with wait 15 (V35, V36).
                self.units[i].blocked = 0;
                if self.pending_idle(i) { 1 } else { LONG_WAIT }
            }
            Row::D => {
                drop_path(self);
                LONG_WAIT
            }
            Row::E | Row::F => LONG_WAIT,
            Row::G => {
                // Static study only (V24): "stand still" becomes the pending order, so the resume rules of the orders
                // spec apply.
                self.give(i, Order::Idle);
                1
            }
        };
        self.units[i].script.wait = wait;
    }

    /// The first row of the blocked rules that matches, the counter already increased. `next` is the blocked tile,
    /// `blocker` the unit on it, if any, and `g` the stored goal (row G measures to it).
    fn blocked_row(&self, i: usize, next: (i32, i32), blocker: Option<UnitId>, g: Goal) -> Row {
        let u = &self.units[i];
        // Rows A and B are for computer players only, unlike the search's bit-0x04 drop ("Player kinds", V23).
        let computer = self.computer_player(u.owner);
        let wall = self.terrain.word(next.0, next.1) & HUMAN_WALL != 0;
        // Invisibility isn't modelled yet: no unit is invisible.
        let invisible = false;
        let attacked = u.attacked_by.is_some();
        let b = blocker.and_then(|b| self.unit(b));
        let near = if u.layer == Layer::Land { 1 } else { 2 };
        let (dx, dy) = ((u.tile.0 - g.tile.0).abs(), (u.tile.1 - g.tile.1).abs());
        if computer && wall && !invisible && !attacked {
            Row::A
        } else if computer && wall {
            Row::B
        } else if u.blocked >= GIVE_UP {
            Row::C
        } else if u.blocked >= LONG_WAIT_FROM {
            Row::D
        } else if b.is_some_and(|b| b.dying || b.removing) {
            Row::E
        } else if b.is_some_and(|b| self.waits_for(u, b)) {
            Row::F
        } else if attacked && dx <= near && dy <= near {
            Row::G
        } else if attacked && u.layer != Layer::Land && dx <= 2 {
            Row::H
        } else {
            Row::I
        }
    }

    /// Row F: this unit is in its move animation after the restart (so never in orders 30 and 31, V17), and blocker
    /// `b` is an ally (or of the same player) walking away: in its move animation, counter 0, not inside anything, its
    /// next cached step not onto this unit's tile (V10, V11, V13, V16, V21). This is not the search's moving-ally rule
    /// ([`World::ignored_by_search`]): row F tests the move animation alone, not the pending-command mark.
    fn waits_for(&self, u: &Unit, b: &Unit) -> bool {
        u.script.anim == anim::MOVE
            && self.allied(u.owner, b.owner)
            && b.script.anim == anim::MOVE
            && b.blocked == 0
            && !b.inside
            && b.path.first().is_some_and(|&t| t != u.tile)
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
        let mask = Self::movement_class(u) & if drop_wall { !HUMAN_WALL } else { !0 };
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
            fn blocked_for_shortening(&self, x: i32, y: i32) -> bool {
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

    /// Pins of the current behaviour on the paths the B-038 movement refactor touches (the walk, the blocked rows, the
    /// search's tile tests and mask, the pull-back and the arrival radius), and of the coverage report's untraced
    /// movement regions, written before it so that it can't change them unseen. Each test states the outcome in a few
    /// fields and pins the whole simulation state with [`World::fingerprint`]. Where a pin differs from the spec's
    /// wording, its comment says so: those are open questions, not rules.
    mod pin {
        use super::*;

        const FLYER: u8 = 2;

        /// A 16 x 8 field with player 1 a computer player, 2 a passive computer player and 3 a human player.
        fn arena() -> World {
            let mut w = field(16, 8);
            w.players[1].controller = controller::COMPUTER;
            w.players[2].controller = controller::PASSIVE;
            w.players[3].controller = controller::HUMAN;
            w
        }

        /// Unit `id` in a move order toward `goal`, its next cached step onto `next`, its counter `blocked`.
        fn walking(w: &mut World, id: UnitId, goal: (i32, i32), next: (i32, i32), blocked: u16) -> usize {
            let i = w.index(id).unwrap();
            w.start_anim(i, anim::MOVE);
            let u = &mut w.units[i];
            u.order = Order::Move { goal, target: None };
            u.path = vec![next];
            u.blocked = blocked;
            i
        }

        /// Moves a unit's tile and its mark to `tile`, whatever placement chose.
        fn put(w: &mut World, id: UnitId, tile: (i32, i32)) {
            let i = w.index(id).unwrap();
            let unit = w.units[i].clone();
            w.mark(&unit, unit.tile, None);
            w.units[i].tile = tile;
            w.units[i].pos = (tile.0 * crate::TILE_PX, tile.1 * crate::TILE_PX);
            let unit = w.units[i].clone();
            w.mark(&unit, tile, Some(id));
        }

        /// The outcome of one blocked decision: order, pending order, counter, animation, wait and route length.
        fn outcome(w: &World, i: usize) -> (Order, Option<Order>, u16, u8, u8, usize) {
            let u = &w.units[i];
            (
                u.order,
                u.pending,
                u.blocked,
                u.script.anim,
                u.script.wait,
                u.path.len(),
            )
        }

        /// Rows A and B (computer players, a wall tile with bit 0x04) against the rows below for a passive computer
        /// player and a human player (V22, V23).
        #[test]
        fn rows_a_and_b_are_for_computer_players_only() {
            let mut prints = vec![];
            for (owner, attacked) in [(1, false), (1, true), (2, false), (3, false)] {
                let mut w = arena();
                w.terrain.set_bits(5, 3, 0x8d);
                let f = w.spawn(0, owner, (4, 3), 0).unwrap();
                let foe = w.spawn(0, 0, (4, 5), 0).unwrap();
                let i = walking(&mut w, f, (9, 3), (5, 3), 0);
                if attacked {
                    w.units[i].attacked_by = Some(foe);
                }
                assert_eq!(w.move_toward(i, Goal::tile((9, 3), 0)), Walk::Blocked);
                let want_pending = (owner == 1 && !attacked).then_some(Order::AttackWall { tile: (5, 3) });
                assert_eq!(w.units[i].pending, want_pending, "owner {owner}, attacked {attacked}");
                prints.push((outcome(&w, i), w.fingerprint()));
            }
            pinned(
                &prints.iter().map(|p| p.1).collect::<Vec<_>>(),
                &[
                    4487319968324443784,
                    8996746854704468010,
                    6321041293130641587,
                    17878908885256948603,
                ],
            );
        }

        /// Rows C to I in order, each reached by one setting: the counter (C at 15, D from 8), a dying or leaving
        /// blocker (E), an ally walking away (F), and an attacker near the goal (G, H) or not (I).
        #[test]
        fn rows_c_to_i() {
            let mut prints = vec![];
            for case in ["C", "D", "E dying", "E removing", "F", "I"] {
                let mut w = arena();
                let f = w.spawn(0, 3, (4, 3), 0).unwrap();
                let b = w.spawn(0, 3, (5, 3), 0).unwrap();
                let counter = match case {
                    "C" => 14,
                    "D" => 7,
                    _ => 0,
                };
                let i = walking(&mut w, f, (9, 3), (5, 3), counter);
                let j = w.index(b).unwrap();
                match case {
                    "E dying" => w.units[j].dying = true,
                    "E removing" => w.units[j].removing = true,
                    "F" => {
                        w.start_anim(j, anim::MOVE);
                        w.units[j].path = vec![(6, 3)];
                    }
                    _ => {}
                }
                w.move_toward(i, Goal::tile((9, 3), 0));
                prints.push(w.fingerprint());
                let (order, pending, blocked, a, wait, route) = outcome(&w, i);
                let _ = (order, a);
                let want = match case {
                    "C" => (Some(Order::Idle), 0, 15, 1),
                    "D" => (None, 8, 15, 0),
                    "E dying" | "E removing" | "F" => (None, 1, 15, 1),
                    _ => (None, 1, 1, 0),
                };
                assert_eq!((pending, blocked, wait, route), want, "{case}");
            }
            pinned(
                &prints,
                &[
                    4332474618142329864,
                    17249441303574175181,
                    16866233877860219639,
                    11374725858968485895,
                    18077612124802644883,
                    18140778682893306582,
                ],
            );
        }

        /// Row F's conditions on the blocker one at a time: a counter above 0, inside something, a next step onto
        /// this unit's tile (head-on), no next step, an enemy, and still with a command pending (the search's
        /// moving-ally rule takes that mark; row F doesn't). Each falls through to row I.
        #[test]
        fn row_f_needs_every_condition() {
            let mut prints = vec![];
            for case in ["counter", "inside", "head-on", "no step", "enemy", "pending command"] {
                let mut w = arena();
                let f = w.spawn(0, 3, (4, 3), 0).unwrap();
                let owner = if case == "enemy" { 1 } else { 3 };
                let b = w.spawn(0, owner, (5, 3), 0).unwrap();
                let i = walking(&mut w, f, (9, 3), (5, 3), 0);
                let j = w.index(b).unwrap();
                w.start_anim(j, anim::MOVE);
                w.units[j].path = vec![(6, 3)];
                match case {
                    "counter" => w.units[j].blocked = 1,
                    "inside" => w.units[j].inside = true,
                    "head-on" => w.units[j].path = vec![(4, 3)],
                    "no step" => w.units[j].path.clear(),
                    "pending command" => {
                        w.start_anim(j, anim::STILL);
                        w.command(
                            j,
                            Order::Move {
                                goal: (12, 3),
                                target: None,
                            },
                        );
                        let u = w.units[i].clone();
                        assert!(w.ignored_by_search(&u, b), "the search ignores it");
                    }
                    _ => {}
                }
                w.move_toward(i, Goal::tile((9, 3), 0));
                let (_, pending, blocked, _, wait, route) = outcome(&w, i);
                assert_eq!((pending, blocked, wait, route), (None, 1, 1, 0), "{case}");
                prints.push(w.fingerprint());
            }
            pinned(
                &prints,
                &[
                    1315267033542475176,
                    6937201428577653383,
                    1039352084613777046,
                    4882441076806024008,
                    11850996428695292654,
                    11030857759968042977,
                ],
            );
        }

        /// Rows G and H: with an attacker, a land unit stops within 1 tile of its goal on both axes; a flyer within
        /// 2; a flyer within 2 on x but farther on y drops its route (H); farther, row I. Row G measures to the goal
        /// it is given, and its stop is a pending idle (static study only; coverage: untraced).
        #[test]
        fn rows_g_and_h() {
            let mut prints = vec![];
            for (kind, goal, stops) in [
                (0, (6, 4), true),
                (0, (8, 3), false),
                (FLYER, (8, 5), true),
                (FLYER, (8, 7), false),
                (FLYER, (10, 3), false),
            ] {
                let mut w = arena();
                let f = w.spawn(kind, 3, (6, 3), 0).unwrap();
                let s = if kind == FLYER { 2 } else { 1 };
                let b = w.spawn(kind, 3, (6 + s, 3), 0).unwrap();
                // Placement moves a flyer to an even row; put both back.
                put(&mut w, f, (6, 3));
                put(&mut w, b, (6 + s, 3));
                let foe = w.spawn(0, 1, (1, 1), 0).unwrap();
                let i = walking(&mut w, f, goal, (6 + s, 3), 0);
                w.units[i].attacked_by = Some(foe);
                w.move_toward(i, Goal::tile(goal, 0));
                let (_, pending, _, _, wait, route) = outcome(&w, i);
                assert_eq!(pending.is_some(), stops, "{kind} {goal:?}");
                assert_eq!((wait, route == 0), (1, !stops), "{kind} {goal:?}");
                prints.push(w.fingerprint());
            }
            pinned(
                &prints,
                &[
                    9749873335569134805,
                    16696698943700492888,
                    9726181436724365457,
                    11287543049927568287,
                    4081969389894885419,
                ],
            );
        }

        /// The search mask's bit-0x04 drop is for every player that isn't human-controlled, with no attacker: a tile
        /// carrying only 0x04 is seen through by a computer, passive computer or neutral unit, not by a human player's
        /// unit or an attacked one (pathfinding spec, "Inputs"; movement spec, "Player kinds").
        #[test]
        fn the_search_drops_bit_4_for_every_player_not_human_controlled() {
            let mut routes = vec![];
            for (owner, attacked) in [(1, false), (2, false), (4, false), (3, false), (1, true)] {
                let mut w = arena();
                w.players[4].controller = controller::NOBODY;
                for y in 0..8 {
                    w.terrain.set_bits(6, y, HUMAN_WALL);
                }
                let f = w.spawn(0, owner, (4, 3), 0).unwrap();
                let foe = w.spawn(0, 0, (1, 1), 0).unwrap();
                let i = w.index(f).unwrap();
                if attacked {
                    w.units[i].attacked_by = Some(foe);
                }
                let u = w.units[i].clone();
                routes.push(w.plan(&u, Goal::tile((9, 3), 0)));
            }
            let through = vec![(5, 3), (6, 3), (7, 3), (8, 3), (9, 3)];
            assert_eq!(routes[..3], [through.clone(), through.clone(), through]);
            // The others find no way through and keep the straight line's prefix (the fallback).
            assert_eq!(routes[3..], [vec![(5, 3)], vec![(5, 3)]], "{routes:?}");
        }

        /// The search's three tile tests on one map: the walk sees an ally with a command pending as free, the
        /// shortening's swap counts it as blocking, and the step is refused by it (row I).
        #[test]
        fn the_search_and_the_step_see_a_pending_ally_differently() {
            let mut w = arena();
            let f = w.spawn(0, 3, (4, 3), 0).unwrap();
            let b = w.spawn(0, 3, (5, 3), 0).unwrap();
            let j = w.index(b).unwrap();
            w.command(
                j,
                Order::Move {
                    goal: (12, 7),
                    target: None,
                },
            );
            let i = w.index(f).unwrap();
            w.units[i].order = Order::Move {
                goal: (9, 3),
                target: None,
            };
            let u = w.units[i].clone();
            assert_eq!(w.plan(&u, Goal::tile((9, 3), 0)).first(), Some(&(5, 3)));
            assert_eq!(w.move_toward(i, Goal::tile((9, 3), 0)), Walk::Blocked);
            assert_eq!(outcome(&w, i).2, 1);
            pinned(&[w.fingerprint()], &[582936150198803950]);
        }

        /// The walk's step: tile, facing, occupancy and the move animation; a flyer's two-tile step and its +1 on the
        /// radius; a ship in the landing class may enter shore water, in the sea class it may not.
        #[test]
        fn whole_walks() {
            let mut prints = vec![];
            for (kind, to) in [(0, (11, 6)), (FLYER, (11, 6)), (FLYER, (12, 2)), (8, (2, 1))] {
                let mut w = arena();
                let f = w.spawn(kind, 3, (3, 2), 0).unwrap();
                w.spawn(0, 3, (7, 4), 0).unwrap();
                w.spawn(5, 1, (5, 2), 0).unwrap();
                mv(&mut w, f, to);
                let mut tiles = vec![];
                for _ in 0..120 {
                    w.step(&[]);
                    let t = w.unit(f).unwrap().tile;
                    if tiles.last() != Some(&t) {
                        tiles.push(t);
                    }
                }
                prints.push(w.fingerprint());
                let _ = tiles;
            }
            pinned(
                &prints,
                &[
                    15645015048395609214,
                    8090279918076836087,
                    14189584049738236248,
                    17901305525487887472,
                ],
            );
        }

        /// The arrival radius: +1 for sea and air units except in orders 30 and 31, never with an attack range.
        #[test]
        fn the_arrival_radius() {
            let mut w = arena();
            let f = w.spawn(FLYER, 3, (4, 4), 0).unwrap();
            let i = w.index(f).unwrap();
            let mut got = vec![];
            for (order, range) in [
                (
                    Order::Move {
                        goal: (6, 6),
                        target: None,
                    },
                    false,
                ),
                (
                    Order::Move {
                        goal: (6, 6),
                        target: None,
                    },
                    true,
                ),
                (Order::Land { goal: (6, 6) }, false),
                (Order::PushOff, false),
            ] {
                w.units[i].order = order;
                let g = Goal {
                    range,
                    ..Goal::tile((6, 6), 1)
                };
                got.push(w.at_goal(i, g));
            }
            assert_eq!(got, [true, false, false, false]);
        }

        /// The command's pull-back for a flyer (no region test) and a two-tile mover (one tile per step), and a goal
        /// next to the unit (k = 1: nothing tested, the goal kept).
        #[test]
        fn the_command_pull_back_settings() {
            let mut w = arena();
            for y in 0..8 {
                w.terrain.set_bits(10, y, bits::OBSTACLE);
            }
            let flyer = w.spawn(FLYER, 3, (4, 2), 0).unwrap();
            let f = w.spawn(0, 3, (4, 4), 0).unwrap();
            let (fi, i) = (w.index(flyer).unwrap(), w.index(f).unwrap());
            assert_eq!(w.pull_back(fi, (10, 2)), (10, 2), "a flyer's mask has no obstacle bit");
            assert_eq!(
                w.pull_back(i, (10, 4)),
                (10, 4),
                "k - 1 tiles, all usable: the first is the goal's neighbour"
            );
            assert_eq!(w.pull_back(i, (5, 4)), (5, 4));
            w.terrain.set_bits(9, 4, bits::OBSTACLE);
            assert_eq!(w.pull_back(i, (10, 4)), (9, 4));
        }

        /// Coverage: a pending order taking over (untraced region): animation, wait 3, counter kept, the command mark
        /// cleared, and a return with no depot looking one up or becoming idle.
        #[test]
        fn a_take_over() {
            let mut w = arena();
            let f = w.spawn(0, 3, (4, 4), 0).unwrap();
            let i = w.index(f).unwrap();
            w.units[i].blocked = 3;
            w.units[i].commanded = true;
            w.take_over(i, Order::ReturnGoods { depot: None });
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.blocked, u.commanded, u.script.wait),
                (Order::Idle, 3, false, 3)
            );
            pinned(&[w.fingerprint()], &[11035032890756001496]);
        }

        /// Compares pinned values, printing the whole list on a mismatch so that a first run can fill it in.
        fn pinned(got: &[u64], want: &[u64]) {
            assert_eq!(got, want, "pinned fingerprints: {got:?}");
        }
    }
}
