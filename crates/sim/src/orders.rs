//! Commands, immediate orders, target acquisition, reactions and fleeing. Spec: `docs/specs/orders/orders.md`.
//!
//! Transports and boarding are in `transport.rs`, attacking the ground (17, 18) and demolition (19 to 21) in
//! `ground.rs`. Not yet here: spells, the computer players' worker and tanker routines (they skip the search, as
//! the spec says, and do nothing instead), alliances outside target acquisition (there only the owner's entry of the
//! alliance table is read), and the Enhanced
//! demolition unit that turns a found target into a demolish ("Special cases").
//!
//! Our own choice: a player command other than patrol and attack-move clears the remembered order, so a unit sent
//! elsewhere doesn't go back to its old patrol.

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::acquire::{self, Threat, facing_step, flee_offset};
use crate::ai::controller;
use crate::anim::Effects;
use crate::combat;
use crate::movement::{Goal, Walk, chebyshev};
use crate::slots;
use crate::world::{Command, Layer, Order, Remembered, Unit, UnitId, World};

/// Smart-move wait caps ("Commands").
const SMART_WAIT_COMPUTER: u8 = 40;
const SMART_WAIT_OTHER: u8 = 10;
/// Resume countdowns ("Resuming patrol and attack-move").
const RESUME_PATROL: u8 = 40;
const RESUME_ATTACK_MOVE: u8 = 20;
/// Rows above the search box still visited, for tall buildings.
const SEARCH_ABOVE: i32 = 3;
/// Own units in the 3 x 3 around a target that stop a ground attacker.
const FRIENDLY_FIRE_HUMAN: u32 = 1;
const FRIENDLY_FIRE_OTHER: u32 = 3;

/// Dwarven demolition squad and goblin sappers (public PUD specification).
pub(crate) const DEMOLITION: [u8; 2] = [0x0e, 0x0f];

/// How an order acquires targets ("The order list", column "Acquires").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Acquires {
    /// 2, 7, 16.
    Idle,
    /// 4, 5, 10, 12.
    Moving,
    /// 13, 14, 15.
    Other,
}

/// The animation an order plays (order table): 2 still, 3 move, 4 attack.
pub(crate) fn order_anim(o: Order) -> u8 {
    match o {
        Order::Attack { .. }
        | Order::AttackWall { .. }
        | Order::Chase { .. }
        | Order::TowerWatch { .. }
        | Order::AttackInPlace { .. }
        | Order::IdleReact { .. }
        | Order::Harvest { .. }
        | Order::Repair { .. }
        | Order::Cast { .. }
        | Order::AttackGround { .. }
        | Order::Demolish { .. }
        | Order::DemolishAt { .. } => anim::ATTACK,
        Order::Enter { .. } | Order::AttackGroundMove { .. } => anim::MOVE,
        Order::Land { .. } | Order::PushOff => anim::WORK_B,
        _ => anim::STILL,
    }
}

/// The unit an order is aimed at.
pub(crate) fn target_of(o: Order) -> Option<UnitId> {
    match o {
        Order::Follow { target }
        | Order::Guard { target }
        | Order::Attack { target }
        | Order::Chase { target }
        | Order::AttackInPlace { target }
        | Order::IdleReact { target } => Some(target),
        Order::TowerWatch { target } | Order::Demolish { at: target, .. } => target,
        _ => None,
    }
}

/// Orders a follower keeps following ("Follow").
fn followable(o: Order) -> bool {
    matches!(
        o,
        Order::Idle
            | Order::Move { .. }
            | Order::MoveAttack { .. }
            | Order::Patrol { .. }
            | Order::Follow { .. }
            | Order::Guard { .. }
            | Order::AttackMove { .. }
    )
}

impl World {
    fn acquires(&self, o: Order) -> Option<Acquires> {
        match o {
            // Order 7 acquires in the idle logic it runs after its follow steps (`run_order`).
            Order::Idle | Order::IdleReact { .. } => Some(Acquires::Idle),
            Order::MoveAttack { .. } | Order::Patrol { .. } | Order::Chase { .. } => Some(Acquires::Moving),
            Order::AttackMove { .. } if self.rules.enhanced => Some(Acquires::Moving),
            Order::StandGround | Order::TowerWatch { .. } | Order::AttackInPlace { .. } => Some(Acquires::Other),
            _ => None,
        }
    }

    fn flags_of_unit(&self, u: &Unit) -> u32 {
        self.stats_of(u.kind).flags
    }

    /// Workers, tankers, transports and demolition units.
    pub(crate) fn non_combat(&self, u: &Unit) -> bool {
        self.flags_of_unit(u) & (flags::PEON | flags::TANKER | flags::TRANSPORT) != 0 || DEMOLITION.contains(&u.kind)
    }

    fn is_worker(&self, u: &Unit) -> bool {
        self.flags_of_unit(u) & flags::PEON != 0
    }

    /// The player's command for one unit, turned into an order ("Commands"); `None` leaves the unit alone.
    pub(crate) fn command_order(&mut self, i: usize, c: &Command) -> Option<Order> {
        let u = &self.units[i];
        let enhanced = self.rules.enhanced;
        // Transports spec, "Land and shore targets" (round-40 errata): every command given to a transport runs them.
        let shore = |to: (i32, i32)| self.transport_goal(i, to);
        let remember = match *c {
            Command::Patrol { to, .. } => Some(Remembered::Patrol {
                to: shore(to),
                back: u.tile,
            }),
            Command::AttackMove { to, .. } => Some(Remembered::AttackMove { goal: shore(to) }),
            _ => None,
        };
        // Round-45 errata, "Commands forget a stale attacker": a target unit given with the command gets the same check
        // as the attacker, so one that is dying, dead, removed or not detected is dropped (static study only).
        let owner = u.owner;
        let fresh = |t: Option<UnitId>| t.filter(|&t| !self.stale(owner, t));
        let order = match *c {
            Command::Move { to, .. } => Order::Move { goal: shore(to) },
            Command::Stop { .. } => Order::Idle,
            // Round-48 errata: a target unit of any owner (own, allied and neutral units included) gives order 9. A
            // dying target is still given; `command` then finds it stale and leaves a pending idle
            // (`orc02-dyingcmd-3760` grunts 1572 and 1575, n = 3723).
            Command::Attack { target, .. } => {
                if !self.can_strike(u, target) {
                    return None;
                }
                Order::Attack { target }
            }
            Command::AttackMove { to, .. } => Order::AttackMove { goal: shore(to) },
            Command::Patrol { to, .. } => Order::Patrol {
                to: shore(to),
                back: u.tile,
            },
            Command::StandGround { .. } => Order::StandGround,
            Command::AttackGround { tile, .. } => self.attack_ground_command(i, tile),
            Command::Demolish { target, tile, .. } => self.demolish_command(i, fresh(target), tile),
            Command::Smart { target, tile, .. } => self.smart(i, fresh(target), tile)?,
            Command::Repair { target, .. } => {
                if !self.is_worker(u) {
                    return None;
                }
                self.repair_command(i, target)?
            }
            Command::Harvest { target, tile, .. } => self.harvest_command(i, fresh(target), tile)?,
            Command::ReturnGoods { .. } => self.return_command(i)?,
            Command::Unload { tile, .. } => {
                if !self.is_transport(u) {
                    return None;
                }
                self.transport_command(i, None, tile, true)
            }
            Command::Train { .. }
            | Command::Upgrade { .. }
            | Command::Research { .. }
            | Command::Spell { .. }
            | Command::Cancel { .. }
            | Command::Cast { .. }
            | Command::Build { .. }
            | Command::Select { .. }
            | Command::RightClick { .. }
            | Command::Issue { .. }
            | Command::SetGroup { .. }
            | Command::RecallGroup { .. }
            | Command::Speed { .. }
            | Command::Pause { .. }
            | Command::Cheat { .. } => return None,
        };
        self.units[i].remembered = if enhanced { remember } else { None };
        Some(order)
    }

    /// Command 3 ("Smart move"). Transports and boarding come with the transports spec, so for now they move.
    fn smart(&mut self, i: usize, target: Option<UnitId>, tile: (i32, i32)) -> Option<Order> {
        let cap = if self.computer_player(self.units[i].owner) {
            SMART_WAIT_COMPUTER
        } else {
            SMART_WAIT_OTHER
        };
        let u = &mut self.units[i];
        u.script.wait = u.script.wait.min(cap);
        let u = &self.units[i];
        // The command carries its target unit, if any ("Commands"): a unit standing on the target tile isn't looked
        // up, so a critter wandering onto another unit's tile gets order 3 (Human04 trace, n = 82).
        let target = target.filter(|&t| t != u.id);
        if self.is_transport(u) {
            return Some(self.transport_command(i, target, tile, false));
        }
        if let Some(t) = target
            && self.boards(u, t)
        {
            return Some(self.board_command(i, t));
        }
        let u = &self.units[i];
        if let Some(t) = target
            && self.can_return_to(u, t)
        {
            return Some(Order::ReturnGoods { depot: Some(t) });
        }
        if self.can_harvest(u, tile) {
            return self.harvest_command(i, target, tile);
        }
        let own_unit = target
            .and_then(|t| self.unit(t))
            .filter(|t| t.owner == u.owner && !t.building && t.present());
        Some(match own_unit {
            Some(t) => Order::Follow { target: t.id },
            None => Order::Move { goal: tile },
        })
    }

    /// An immediate order 2 ("Pending orders and immediate orders"): set at once, the pending order cleared, the
    /// still animation started with a wait of 1, a reserved tree released. It never resumes a remembered patrol or
    /// attack-move, and keeps it.
    pub(crate) fn stop(&mut self, i: usize) {
        self.release_tree(i);
        let u = &mut self.units[i];
        u.order = Order::Idle;
        u.pending = None;
        u.commanded = false;
        u.path.clear();
        self.start_anim(i, anim::STILL);
        self.units[i].script.wait = 1;
        // A transport settles on an even tile that isn't shore water, and otherwise waits docked.
        let u = &self.units[i];
        if self.is_transport(u) {
            let t = u.tile;
            let settles =
                t.0 & 1 == 0 && t.1 & 1 == 0 && self.terrain.word(t.0, t.1) & crate::terrain::bits::SHORE == 0;
            let u = &mut self.units[i];
            if settles {
                u.ship.landing = false;
                u.ship.one_tile = false;
            } else {
                u.order = Order::Docked;
            }
        }
    }

    /// "Resuming patrol and attack-move" (Enhanced only): when idle is about to become the unit's pending order, the
    /// remembered order to give instead, with the countdown set. `None` keeps the idle. An immediate idle never comes
    /// here.
    ///
    /// Round-45 errata: the resumed order is a full command. Its goal is pulled back (pathfinding spec, "Goal pulled
    /// back"), the cached route dropped, the blocked counter cleared and the recorded attacker checked ("Commands
    /// forget a stale attacker"; `xorc01-death-coil` ogre 1489, n = 901 to 904).
    pub(crate) fn resumed(&mut self, i: usize) -> Option<Order> {
        let u = &self.units[i];
        if !self.rules.enhanced || u.resume > 0 {
            return None;
        }
        let o = match u.remembered {
            Some(Remembered::Patrol { to, back }) => {
                self.units[i].resume = RESUME_PATROL;
                Order::Patrol {
                    to: self.pull_back(i, to),
                    back,
                }
            }
            Some(Remembered::AttackMove { goal }) if chebyshev(u.tile, goal) > 1 => {
                self.units[i].resume = RESUME_ATTACK_MOVE;
                Order::AttackMove {
                    goal: self.pull_back(i, goal),
                }
            }
            _ => return None,
        };
        let u = &mut self.units[i];
        u.path.clear();
        u.blocked = 0;
        self.forget_stale_attacker(i);
        Some(o)
    }

    /// A pending idle that takes over in the same update (arriving, losing the target), or the resumed order.
    pub(crate) fn idle_now(&mut self, i: usize) {
        let o = self.resumed(i).unwrap_or(Order::Idle);
        self.take_over(i, o);
    }

    /// "No route": idle as an immediate order, and the idle logic runs in the same update, straight after.
    pub(crate) fn idle_at_once(&mut self, i: usize) {
        self.stop(i);
        if self.units[i].order == Order::Idle {
            self.run_order(i);
        }
    }

    /// An immediate order other than idle.
    pub(crate) fn immediately(&mut self, i: usize, o: Order) {
        self.release_tree(i);
        let u = &mut self.units[i];
        u.order = o;
        u.pending = None;
        u.commanded = false;
        u.path.clear();
        self.start_anim(i, order_anim(o));
        self.units[i].script.wait = 1;
    }

    /// Gives back the tree a worker reserved, when it leaves the order that holds it.
    pub(crate) fn release_tree(&mut self, i: usize) {
        if let Order::Harvest {
            source: None,
            tree,
            chopping: true,
        } = self.units[i].order
        {
            self.terrain.unreserve(tree.0, tree.1);
        }
    }

    /// Whether a target is still good for an order aimed at it: valid and detected by the owner. Fog doesn't matter.
    pub(crate) fn holds(&self, owner: u8, t: UnitId) -> bool {
        self.unit(t).is_some_and(|t| t.present() && Self::detected(t, owner))
    }

    /// "Arriving and losing the target": a pending order takes over now; otherwise 15 becomes 13 and the rest 2. The
    /// new order takes over in the same update (wait 3): `human01-basics`, n = 291, footman 1598 losing its dead
    /// target gets order 2 with wait 3.
    pub(crate) fn lose_target(&mut self, i: usize) {
        if let Some(o) = self.units[i].pending.take() {
            self.immediately(i, o);
        } else if matches!(self.units[i].order, Order::AttackInPlace { .. }) {
            self.immediately(i, Order::StandGround);
        } else {
            // "Resuming patrol and attack-move": losing the target is one of the places idle becomes pending.
            self.idle_now(i);
        }
    }

    /// The unit's attacker if still good; a lost one is forgotten. Round-42 errata (movement spec, "Blocked rules";
    /// "Resulting order"): a dying attacker isn't good, but its record is kept while it is dying, where it still
    /// counts as an attacker for the resulting order. Round-43 errata: it is kept until it leaves play (its slot is
    /// released); the end of its death animation isn't a rule (movement spec, open questions).
    fn attacker(&mut self, i: usize) -> Option<UnitId> {
        let u = &self.units[i];
        let a = u.attacked_by?;
        if self.holds(u.owner, a) {
            return Some(a);
        }
        if !self.unit(a).is_some_and(|a| a.dying) {
            self.units[i].attacked_by = None;
        }
        None
    }

    /// Orders spec, "Commands forget a stale attacker" (round-45 errata): every command given to a unit that isn't a
    /// building forgets a recorded attacker that is dying, dead or removed, or isn't detected by the unit's owner
    /// (`xorc01-death-knight` ogre 1489, n = 1191).
    pub(crate) fn forget_stale_attacker(&mut self, i: usize) {
        let u = &self.units[i];
        if u.building {
            return;
        }
        if let Some(a) = u.attacked_by
            && self.stale(u.owner, a)
        {
            self.units[i].attacked_by = None;
        }
    }

    /// Whether a unit named by a command is dying, dead, removed or not detected by `owner` ("Commands forget a stale
    /// attacker"; the same check applies to a command's target unit).
    pub(crate) fn stale(&self, owner: u8, t: UnitId) -> bool {
        self.unit(t).is_none_or(|t| t.dying || !Self::detected(t, owner))
    }

    /// The per-step order logic for the orders this module owns. Returns false for orders it doesn't handle.
    pub(crate) fn run_order(&mut self, i: usize) -> bool {
        let o = self.units[i].order;
        // "Idle look-around": the first thing each run of the idle logic (orders 2 and 13) does. A critter that
        // wanders is in its move now, so the rest of the idle logic doesn't run on the old order.
        if matches!(o, Order::Idle | Order::StandGround) && self.look_around(i) {
            return true;
        }
        // Damage spec, "Applying damage": a killer whose order is still the one whose target it forgot has no target,
        // and its next logic run loses it as any other attacker does. That spec leaves this run open; in
        // `human01-basics` footman 1593 kills at n = 575 and its next run, at n = 591, gives order 2 with wait 3.
        let forgot = self.units[i].forgot.take() == Some(o);
        let lost = target_of(o).is_some_and(|t| forgot || !self.holds(self.units[i].owner, t));
        // Round-40 errata: order 12 runs its target search first, and only if it finds nothing new is the target lost.
        // Round-42 errata: so do orders 15 and 16, so a dying target stays the target until then.
        let search_first = matches!(
            o,
            Order::Chase { .. } | Order::AttackInPlace { .. } | Order::IdleReact { .. }
        );
        if lost && !search_first {
            self.lose_target(i);
            return true;
        }
        // "Order 12 with no route" (round-47 errata): a unit marked by that fallback skips the target search (and with
        // it the acquisition) in its next order-logic run. Round-48 errata: if that run's order is idle or an attack
        // order (2, 9, 11, 12, 13, 15, 16 or 17); `World::order` clears the mark at the run's end.
        let skip = self.units[i].skip_search
            && matches!(
                o,
                Order::Idle
                    | Order::Attack { .. }
                    | Order::AttackWall { .. }
                    | Order::Chase { .. }
                    | Order::StandGround
                    | Order::AttackInPlace { .. }
                    | Order::IdleReact { .. }
                    | Order::AttackGround { .. }
            );
        if !skip
            && let Some(kind) = self.acquires(o)
            && self.acquire(i, kind, forgot)
        {
            return true;
        }
        if lost {
            // Round-45 errata, "Losing the target while cooling down": in orders 12, 15 and 16 the cooldown test comes
            // before the target check, so a unit still cooling down restarts its attack animation with the cooldown
            // left as its wait, its pending order cleared, and keeps its order until the cooldown runs out
            // (`xorc01-death-knight` ogre 1488, n = 907 to 930).
            if self.cooling_down(i) {
                let u = &mut self.units[i];
                u.pending = None;
                // A killer keeps having forgotten its victim (no target) until the run that loses it.
                if forgot {
                    u.forgot = Some(o);
                }
                return true;
            }
            self.lose_target(i);
            return true;
        }
        match o {
            Order::Idle | Order::StandGround => {
                if self.units[i].script.anim != anim::STILL {
                    self.switch_anim(i, anim::STILL);
                }
                if o == Order::Idle && self.chooses_jobs(i) {
                    self.idle_choice(i);
                }
            }
            Order::Move { goal } | Order::MoveAttack { goal } | Order::AttackMove { goal } => {
                if matches!(o, Order::AttackMove { .. }) && !self.rules.enhanced && self.react(i) {
                    return true;
                }
                let walk = self.move_toward(i, Goal::tile(goal, 0));
                match walk {
                    // "Arriving and losing the target": the unit forgets its attacker, and idle becomes its pending
                    // order and takes over in the same update (still animation, wait 3); the idle logic first runs
                    // at the next step done.
                    Walk::Arrived => {
                        self.units[i].attacked_by = None;
                        self.idle_now(i);
                    }
                    // No route: idle as an immediate order, so the idle logic runs in the same update, straight after.
                    // Round-45 errata: only arriving forgets the attacker; a unit with no route keeps it, and its
                    // idle logic may react to it (`xorc10-dragons` footman 1520 flees again at n = 464).
                    Walk::NoPath => self.idle_at_once(i),
                    Walk::Stepping | Walk::Blocked => {}
                }
            }
            Order::Patrol { to, back } => match self.move_toward(i, Goal::tile(to, 0)) {
                Walk::Arrived => {
                    // "Arriving and losing the target": a patrol to the other end, with the tile it stands on as the new
                    // other end, takes over in the same update (still animation, wait 3); it steps at its next run.
                    let here = self.units[i].tile;
                    // Round-44 errata: the turnaround is a command, so for every patrol its new goal is pulled back
                    // (pathfinding spec, "Goal pulled back"); the remembered ends stay as they were.
                    let swapped = Order::Patrol {
                        to: self.pull_back(i, back),
                        back: here,
                    };
                    if let Some(Remembered::Patrol { .. }) = self.units[i].remembered {
                        self.units[i].remembered = Some(Remembered::Patrol { to: back, back: here });
                    }
                    // Movement spec, "Blocked rules" (round-41 errata): the turnaround is a command.
                    self.units[i].blocked = 0;
                    self.forget_stale_attacker(i);
                    self.take_over(i, swapped);
                }
                // "No route" (round-39 errata, `human10` and `xorc7`): an immediate idle whose idle logic runs in the
                // same update, straight after.
                Walk::NoPath => self.idle_at_once(i),
                Walk::Stepping | Walk::Blocked => {}
            },
            Order::Follow { target } | Order::Guard { target } => {
                if matches!(o, Order::Follow { .. }) && self.react(i) {
                    return true;
                }
                // "Follow": order 7 then runs the idle logic (look-around, then acquisition as for order 2), unless
                // its follow steps gave it a new order.
                if self.follow(i, target) && !self.look_around(i) {
                    self.acquire(i, Acquires::Idle, false);
                }
            }
            Order::Attack { target } | Order::Chase { target } => self.attack_order(i, target),
            Order::AttackInPlace { target } | Order::IdleReact { target } => self.attack_in_place(i, target),
            Order::AttackGround { tile } => self.attack_ground(i, tile),
            Order::AttackGroundMove { tile } => self.attack_ground_move(i, tile),
            Order::Demolish { .. } | Order::DemolishAt { .. } => self.demolish(i, o),
            _ => return false,
        }
        true
    }

    /// Orders 6 and 7 ("Follow"). Returns whether a guarding unit (order 7) goes on to its idle logic: it is within
    /// the radius and its follow steps gave it no new order.
    fn follow(&mut self, i: usize, target: UnitId) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let (t_order, t_tile) = (t.order, t.tile);
        let u = &self.units[i];
        let following = matches!(u.order, Order::Guard { .. }) || matches!(u.pending, Some(Order::Follow { .. }));
        if following && !followable(t_order) {
            let here = u.tile;
            self.give(i, Order::Move { goal: here });
            return false;
        }
        if chebyshev(u.tile, t_tile) > 1 {
            if matches!(self.move_toward(i, Goal::tile(t_tile, 1)), Walk::NoPath) {
                self.stop(i);
            }
            return false;
        }
        if matches!(u.order, Order::Follow { .. }) {
            if followable(t_order) {
                self.units[i].order = Order::Guard { target };
                self.units[i].path.clear();
                if self.units[i].script.anim != anim::STILL {
                    self.switch_anim(i, anim::STILL);
                }
            } else {
                self.stop(i);
            }
            return false;
        }
        if self.units[i].script.anim != anim::STILL {
            self.switch_anim(i, anim::STILL);
        }
        true
    }

    /// Orders 15 and 16: strike in range, never move ("Attacking without moving").
    fn attack_in_place(&mut self, i: usize, target: UnitId) {
        let Some(t) = self.unit(target) else { return };
        // "Friendly-fire check" (round-41 errata): before the range test.
        let tile = t.tile;
        if self.holds_fire(i, tile) {
            return;
        }
        let t = self.unit(target).expect("checked above");
        let u = &self.units[i];
        if self.in_range(u, t) {
            let b = crate::world::facing_tile_of(t);
            self.units[i].path.clear();
            self.face_and_strike(i, b);
            return;
        }
        match u.order {
            // Round-41 errata (static study only): order 15 forgets the target and order 13 takes over in the same
            // update (still animation, wait 3), by the same path as order 16.
            Order::AttackInPlace { .. } => self.take_over(i, Order::StandGround),
            // Round-40 errata (`xhuman4` axe thrower 1437, n = 6): order 16 forgets the target and idle takes over in
            // the same update (still animation, wait 3); the attacker is kept.
            _ => self.idle_now(i),
        }
    }

    pub(crate) fn reach(&self, u: &Unit) -> i32 {
        let up = self.players[u.owner as usize].upgrades;
        combat::range(u.kind, &self.stats_of(u.kind), &up).max(1) as i32
    }

    /// Whether `t` is within `u`'s attack range, measured to the reference tile ("Distance").
    pub(crate) fn in_range(&self, u: &Unit, t: &Unit) -> bool {
        distance(u.tile, t) <= self.reach(u)
    }

    /// Whether `u`'s type can attack `t`'s kind: its can-target mask against land, sea or air, with submarines'
    /// special list.
    pub(crate) fn can_target_kind(&self, u: &Unit, t: &Unit) -> bool {
        let s = self.stats_of(u.kind);
        let tf = self.flags_of_unit(t);
        if s.flags & flags::SUBMARINE != 0 {
            return t.layer == Layer::Sea || tf & (flags::SHORE_BUILDING | flags::OIL_SOURCE) != 0;
        }
        let bit = match t.layer {
            Layer::Air => 4,
            Layer::Sea => 2,
            Layer::Land => 1,
        };
        s.can_target & bit != 0
    }

    fn threat(&self, u: &Unit, t: &Unit) -> Threat {
        let tf = self.flags_of_unit(t);
        Threat {
            can_attack: tf & flags::CAN_ATTACK != 0,
            in_range: self.in_range(u, t),
            air_or_sub: t.layer == Layer::Air || tf & flags::SUBMARINE != 0,
        }
    }

    fn score(&self, u: &Unit, t: &Unit) -> i32 {
        acquire::score(self.threat(u, t), self.stats_of(t.kind).priority, distance(u.tile, t))
    }

    /// Whether `t` is closer than the searcher's minimum attack range (the minimum range of its missile).
    fn inside_min_range(&self, u: &Unit, t: &Unit) -> bool {
        combat::splash(self.stats_of(u.kind).missile).is_some_and(|min| distance(u.tile, t) < min as i32)
    }

    /// Whether `t` passes the candidate skip list for searcher `u` ("A candidate is skipped if").
    pub(crate) fn candidate(&self, u: &Unit, t: &Unit, order: Order) -> bool {
        let tf = self.flags_of_unit(t);
        // "Its owner is allied to the searcher's owner, or is the searcher's owner", by the searcher's owner's entry
        // of the alliance table (setup spec, "Starting alliances and shared vision").
        if t.owner == u.owner || t.owner == slots::NEUTRAL || self.allied(u.owner, t.owner) || !t.present() {
            return false;
        }
        // A new unit is hidden from everyone until the next visibility pass (unit-slots spec, "Creating a unit"), but
        // the posts that the attack AI's first goals choose while the map loads still find the map's enemies (the
        // xhuman4 and xhuman2 oracle traces), so the fog part of the check is left out while loading.
        if !Self::detected(t, u.owner) || (!self.loading && !self.visible_to(u.owner, t.id)) {
            return false;
        }
        if !self.can_target_kind(u, t) {
            return false;
        }
        if u.layer == Layer::Air && t.layer == Layer::Air && tf & flags::CAN_ATTACK == 0 {
            return false;
        }
        if self.inside_min_range(u, t) {
            return false;
        }
        let roaming = matches!(order, Order::AttackMove { .. } | Order::Patrol { .. });
        if self.rules.enhanced
            && roaming
            && u.layer == Layer::Sea
            && t.layer != Layer::Sea
            && tf & (flags::SHORE_BUILDING | flags::OIL_SOURCE) == 0
        {
            return false;
        }
        true
    }

    /// "Special cases" (round-40 errata): mages score every candidate like any other unit, and the rule is applied to
    /// the chosen target. A target that can attack and isn't a non-combat unit makes them acquire nothing.
    fn mage_refuses(&self, u: &Unit, t: &Unit) -> bool {
        self.mage_like(u) && self.flags_of_unit(t) & flags::CAN_ATTACK != 0 && !self.non_combat(t)
    }

    /// Mage-type units: mage and death knight types, heroes included.
    fn mage_like(&self, u: &Unit) -> bool {
        self.flags_of_unit(u) & flags::MAGE_LIKE != 0
    }

    /// The target search ("Automatic target acquisition"). `seed` starts as the best candidate; `tower` uses the
    /// attack range box reaching one tile further right and down.
    fn search(&self, i: usize, seed: Option<UnitId>, tower: bool) -> Option<UnitId> {
        let u = &self.units[i];
        let s = self.stats_of(u.kind);
        let r = if tower {
            self.reach(u)
        } else if self.computer_player(u.owner) {
            s.react_range_computer as i32
        } else if self.acquires(u.order) == Some(Acquires::Moving) {
            s.react_range_human as i32
        } else {
            self.reach(u)
        };
        self.search_box(i, seed, r, tower as i32)
    }

    /// "Order 12 with no route" (round-47 errata): the target search with the box of the unit's attack range (one
    /// tile further right and down only for tower types) and no attacker seed.
    pub(crate) fn search_in_range(&self, i: usize) -> Option<UnitId> {
        let u = &self.units[i];
        let tower = self.flags_of_unit(u) & flags::TOWER != 0;
        self.search_box(i, None, self.reach(u), tower as i32)
    }

    /// The target search over the box of radius `r` around the unit, reaching `extra` tiles further right and down.
    fn search_box(&self, i: usize, seed: Option<UnitId>, r: i32, extra: i32) -> Option<UnitId> {
        let u = &self.units[i];
        let (x0, y0, x1, y1) = (u.tile.0 - r, u.tile.1 - r, u.tile.0 + r + extra, u.tile.1 + r + extra);
        // "Choice" (round-44 errata, `human06-31357` ballista 1582 at n = 23683): the attacker seeds the search only
        // with a score above 0; one the unit can't attack (score 0: a kind it can't attack, or closer than its minimum
        // range) leaves the search as if there were no attacker.
        let mut best: Option<(i32, UnitId)> = seed
            .and_then(|a| self.unit(a))
            .filter(|a| self.can_target_kind(u, a) && !self.inside_min_range(u, a))
            .map(|a| (self.score(u, a), a.id))
            .filter(|&(sc, _)| sc > 0);
        // "Vertical order" (round-44 errata): only dying, dead and removed entries are skipped at the front. Inside
        // units keep their list place and are visited by their recorded tile row, so they take part in the halving
        // search; the candidate checks then reject them (`human7-22000` archer 1598, n = 19787).
        let gone = |id: &UnitId| self.unit(*id).is_none_or(|t| t.dying);
        let front = self.ylist.iter().take_while(|id| gone(id)).count();
        let list = &self.ylist[front..];
        let rows: Vec<i32> = list
            .iter()
            .map(|id| self.unit(*id).map_or(i32::MIN, |t| t.tile.1))
            .collect();
        let range = acquire::row_range(&rows, (y0 - SEARCH_ABOVE).max(0), y1);
        for id in range.map_or(&[][..], |(lo, hi)| &list[lo..=hi]) {
            let Some(t) = self.unit(*id) else { continue };
            let inside =
                t.tile.0 <= x1 && t.tile.0 + t.size.0 as i32 > x0 && t.tile.1 <= y1 && t.tile.1 + t.size.1 as i32 > y0;
            if !inside || !self.candidate(u, t, u.order) {
                continue;
            }
            let sc = self.score(u, t);
            if best.is_none_or(|(b, _)| sc > b) {
                best = Some((sc, t.id));
            }
        }
        let best = best.map(|(_, id)| id)?;
        if self.unit(best).is_some_and(|t| self.mage_refuses(u, t)) {
            return None;
        }
        Some(best)
    }

    /// The order a chosen target gives ("Resulting order").
    fn resulting(&self, i: usize, kind: Acquires, t: UnitId) -> Order {
        let u = &self.units[i];
        match kind {
            Acquires::Moving => Order::Chase { target: t },
            Acquires::Other => Order::AttackInPlace { target: t },
            Acquires::Idle => {
                let chase = u.attacked_by.is_some()
                    || (self.computer_player(u.owner)
                        && self
                            .unit(t)
                            .is_some_and(|t| t.building || self.flags_of_unit(t) & flags::CAN_ATTACK != 0));
                if chase {
                    Order::Chase { target: t }
                } else {
                    Order::IdleReact { target: t }
                }
            }
        }
    }

    /// Acquisition in an acquiring order, with the attacker handled first ("Reacting to an attacker"). Returns true
    /// if the unit's order changed or it fled. `forgot`: the unit killed its target (and so forgot it) in this update.
    fn acquire(&mut self, i: usize, kind: Acquires, forgot: bool) -> bool {
        let u = &self.units[i];
        let worker_like = self.flags_of_unit(u) & (flags::PEON | flags::TANKER) != 0;
        if worker_like && self.computer_player(u.owner) {
            return false;
        }
        let attacker = self.attacker(i);
        // Round-42 errata, "Special cases": workers and tankers of every player never run the search; the others'
        // only react to an attacker (`human06-31357` peasant 1587, n = 13557).
        if worker_like && attacker.is_none() {
            return false;
        }
        let u = &self.units[i];
        let mage = self.mage_like(u);
        if let Some(a) = attacker {
            let at = self.unit(a).expect("held");
            if !self.can_target_kind(u, at) {
                return self.flee(i);
            }
            if self.non_combat(u) {
                return self.react(i);
            }
            // Round-45 errata, "Reacting to an attacker", step 3: an invisible mage-type unit does nothing more in
            // that run (static study only).
            if mage && u.invisibility > 0 {
                return false;
            }
        }
        if self.flags_of_unit(u) & flags::CAN_ATTACK == 0 {
            return false;
        }
        // Round-45 errata, step 3: a mage-type unit with an attacker that finds no target it may take (or only its
        // current one) runs the reaction instead, which treats it as a non-combat unit, so it flees
        // (`xorc01-death-coil` death knight 1491, n = 1431).
        let found = self
            .search(i, attacker, false)
            .filter(|&b| Some(b) != target_of(u.order));
        let Some(best) = found else {
            if mage && attacker.is_some() {
                return self.react(i);
            }
            return false;
        };
        let u = &self.units[i];
        let o = self.resulting(i, kind, best);
        let t = self.unit(best).expect("found by the search");
        // "Attacking without moving" (round-40 errata, `xhuman2` footman 1548 at n = 34): order 16 doesn't switch to a
        // new target out of range; the target is forgotten and idle takes over in the same update, attacker kept.
        if matches!(u.order, Order::IdleReact { .. }) && !self.in_range(u, t) {
            self.idle_now(i);
            return true;
        }
        let reach = self.within_strike_reach(u, t);
        // Round-40 errata, "Automatic target acquisition": in order 12 a new target out of reach, with the old target
        // still held, is given as a command. The target changes at once, the route is dropped and the blocked counter
        // cleared; order 12 takes over at the next step done (`orc02` footman 1582, n = 570 to 586; `xhuman4`
        // footman 1497, n = 66). Our reading: the order's logic carries on in this update with the new target, as
        // the round-39 traces show (the unit keeps heading for it).
        // Round-42 errata: the same holds for an old target that is dying, unless this unit killed it (and so forgot
        // it); but a unit within strike reach of the old target takes the new order over at once, without a step
        // (`xorc01-death-knight` ogre 1490, n = 845; `orc02-assault` footman 1582, n = 800). Round-48 errata
        // (`human5-10000` axe thrower 1533, n = 2754): whenever the old target is still recorded, whatever its state
        // (dying, inside a building or otherwise not detected).
        if let (Order::Chase { target: old }, Order::Chase { target }) = (u.order, o)
            && !reach
            && !forgot
            && let Some(ot) = self.unit(old)
            && !self.within_strike_reach(u, ot)
        {
            self.units[i].order = o;
            self.command(i, o);
            self.attack_order(i, target);
            return true;
        }
        // Attack-move: orders 4, 5 and Enhanced 10 (`xorc01-death-knight` ogre 1488, n = 718; round-41 errata,
        // `xhuman2` ogre 1581 at n = 2048 in order 4): the same command, but the order stays until the next step done
        // and its logic carries on heading for the new target, even one already within strike reach
        // (`orc02-victory` axe thrower 1591, n = 4789).
        if let (Order::AttackMove { .. } | Order::MoveAttack { .. } | Order::Patrol { .. }, Order::Chase { target }) =
            (u.order, o)
        {
            self.command(i, o);
            self.head_for(i, target);
            return true;
        }
        // Orders spec, "Friendly-fire check" (round-43 errata, `xhuman4` ballista 1488 at n = 210, `xhuman10-spells`
        // ballista 1483 at n = 227): after a target switch in orders 12, 15 and 16 the run goes on with the new
        // target, and the check is made on it (not while still cooling down).
        let tile = t.tile;
        if matches!(
            u.order,
            Order::Chase { .. } | Order::AttackInPlace { .. } | Order::IdleReact { .. }
        ) && self.holds_fire(i, tile)
        {
            return true;
        }
        let u = &self.units[i];
        let switching_chase = matches!((u.order, o), (Order::Chase { .. }, Order::Chase { .. }));
        if matches!(
            u.order,
            Order::Chase { .. } | Order::AttackInPlace { .. } | Order::IdleReact { .. }
        ) && self.acquiring_strike(i, o, best)
            && switching_chase
        {
            // Round-41 errata, "Resulting order" (`orc02-assault` footman 1587, n = 617): an order-12 switch to a
            // target in reach while still cooling down takes effect at once, and the cooldown test's restart (the
            // cooldown left as its wait) stands; there is no wait of 3.
            let u = &mut self.units[i];
            u.order = o;
            u.path.clear();
            u.blocked = 0;
            self.forget_stale_attacker(i);
            return true;
        }
        // Orders spec, "Same-update take-over": a command the unit's own logic gives it takes over at once ("Taking
        // over": the animation starts with a wait of 3). Being a command (round-42 errata), it clears the blocked
        // counter and drops the cached route.
        self.units[i].blocked = 0;
        self.units[i].path.clear();
        self.forget_stale_attacker(i);
        self.take_over(i, o);
        true
    }

    /// Damage spec, "Attack timing (cooldown)": the run that acquires a new target reaches the strike decision with
    /// it, starting the cooldown, when it is within reach, before the order given again restarts the animation. A
    /// unit still cooling down only gets its restart, which the take-over replaces. Our reading of the traces: only a
    /// target switch (a unit already in order 12, 15 or 16) reaches it, and that strike decision turns the unit as any
    /// other does; a first acquisition doesn't (`human5`, slot 1531 at n = 6, and `xhuman10`, slot 1487 at n = 2, keep
    /// their facing, while `orc02`, slot 1582 at n = 402, turns).
    ///
    /// Returns whether the target was in reach and the cooldown test restarted the animation.
    fn acquiring_strike(&mut self, i: usize, o: Order, target: UnitId) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let u = &self.units[i];
        let reach = match o {
            Order::Chase { .. } => self.within_strike_reach(u, t),
            _ => self.in_range(u, t),
        };
        if !reach {
            return false;
        }
        let b = crate::world::facing_tile_of(t);
        if self.cooling_down(i) {
            return true;
        }
        self.decide_strike(i, b, false);
        false
    }

    /// The plain reaction to an attacker ("The reaction"). Returns true if the unit's order changed or it fled.
    pub(crate) fn react(&mut self, i: usize) -> bool {
        let Some(a) = self.attacker(i) else { return false };
        let u = &self.units[i];
        let playing = u.owner != slots::NEUTRAL
            && self
                .players
                .get(u.owner as usize)
                .is_some_and(|p| p.controller != controller::NOBODY);
        if !playing || u.building || self.flags_of_unit(u) & flags::CAN_ATTACK == 0 {
            self.units[i].attacked_by = None;
            return false;
        }
        // Round-45 errata: the reaction treats mage-type units as non-combat units, so they flee.
        if self.non_combat(u) || self.mage_like(u) {
            return self.flee(i);
        }
        let Some(best) = self.search(i, Some(a), false) else {
            return false;
        };
        if Some(best) == target_of(u.order) {
            return false;
        }
        self.immediately(i, Order::Chase { target: best });
        true
    }

    /// "Fleeing". Returns true if the unit fled or turned on its attacker.
    fn flee(&mut self, i: usize) -> bool {
        let Some(a) = self.attacker(i) else { return false };
        let u = &self.units[i];
        let at = self.unit(a).expect("held");
        if self.is_worker(u) && self.is_worker(at) {
            self.give(i, Order::Attack { target: a });
            return true;
        }
        let may = if self.computer_player(u.owner) {
            u.order == Order::Idle
        } else {
            matches!(
                u.order,
                Order::Idle | Order::Guard { .. } | Order::Repair { .. } | Order::Harvest { source: None, .. }
            )
        };
        if !may {
            return false;
        }
        let (kx, ky) = facing_step(at.facing);
        let two = matches!(u.layer, Layer::Sea | Layer::Air);
        let tile = u.tile;
        let dx = flee_offset(self.rng.draw(), kx, two);
        let dy = flee_offset(self.rng.draw(), ky, two);
        let (w, h) = (self.terrain.width as i32, self.terrain.height as i32);
        let mut to = ((tile.0 + dx).clamp(0, w - 1), (tile.1 + dy).clamp(0, h - 1));
        if two {
            to = (to.0 & !1, to.1 & !1);
        }
        // Round-40 errata (`xorc10` footman 1520 at n = 333, `human12` zeppelin 1503 at n = 5668): a unit that isn't in
        // its move animation takes the move in the same update (wait 3); one in its move animation keeps it pending.
        // Our reading: the take-over starts the move order's own animation, the still one, as any take-over does.
        // Round-44 errata: the move is a command, so its goal is pulled back, its blocked counter cleared and its
        // cached route dropped.
        self.command(i, Order::Move { goal: to });
        if self.units[i].script.anim != anim::MOVE
            && let Some(o) = self.units[i].pending.take()
        {
            self.take_over(i, o);
        }
        self.units[i].attacked_by = None;
        true
    }

    /// The friendly-fire check of a ground attacker in orders 12, 15 and 16, made in the order's logic before the
    /// range test (round-41 errata; `xhuman2` ballista 1503, n = 2226, and `human06-31357` ballista 1582, n = 9483).
    /// Returns true if the strike is called off: the target is forgotten and idle takes over in the same update
    /// (still animation, wait 3, facing kept). Our reading of "after the cooldown test": a unit still cooling down
    /// (whose strike decision would only restart its animation) isn't checked in that run.
    pub(crate) fn holds_fire(&mut self, i: usize, at: (i32, i32)) -> bool {
        let u = &self.units[i];
        let in_order = matches!(
            u.order,
            Order::Chase { .. } | Order::AttackInPlace { .. } | Order::IdleReact { .. }
        );
        if !in_order || self.flags_of_unit(u) & flags::GROUND_ATTACK == 0 {
            return false;
        }
        if !u.building && u.layer != Layer::Air && (u.script.wait as u16) < u.cooldown {
            return false;
        }
        let limit = if self.human_controlled(u.owner) {
            FRIENDLY_FIRE_HUMAN
        } else {
            FRIENDLY_FIRE_OTHER
        };
        let mut own = 0;
        for y in at.1 - 1..=at.1 + 1 {
            for x in at.0 - 1..=at.0 + 1 {
                let mine = self
                    .occupant(Layer::Land, x, y)
                    .and_then(|o| self.unit(o))
                    .is_some_and(|o| o.owner == u.owner && o.present());
                own += mine as u32;
            }
        }
        if own >= limit {
            self.take_over(i, Order::Idle);
            return true;
        }
        false
    }

    /// Whether a building is a complete tower, whose logic is order 14.
    pub(crate) fn is_watching_tower(&self, i: usize) -> bool {
        let b = &self.units[i];
        b.construction.is_none() && self.flags_of_unit(b) & flags::TOWER != 0
    }

    /// Order 14 ("Order 14 (towers)"): each run forgets the attacker and searches its attack range afresh. A target
    /// found is fired at at once, in the run (tower scripts have no strike point); the tower stores no target, doesn't
    /// turn and has no cooldown.
    pub(crate) fn tower(&mut self, i: usize, fx: Effects) {
        if !fx.step_done {
            return;
        }
        self.units[i].attacked_by = None;
        self.units[i].order = Order::TowerWatch { target: None };
        let found = if self.flags_of_unit(&self.units[i]) & flags::CAN_ATTACK != 0 {
            self.search(i, None, true)
        } else {
            None
        };
        // With no target the tower "does nothing until its next run": it keeps whatever animation it plays.
        if let Some(ti) = found.and_then(|t| self.index(t)) {
            let to = self.units[ti].centre_px();
            self.hit_or_launch(i, Some(ti), to);
        }
    }

    /// Inserts a new unit into the vertical list before any unit with the same pixel y.
    pub(crate) fn insert_vertical(&mut self, id: UnitId, y: i32) {
        let at = self
            .ylist
            .iter()
            .position(|o| self.unit(*o).is_some_and(|o| self.sort_y(o) >= y))
            .unwrap_or(self.ylist.len());
        self.ylist.insert(at, id);
    }

    /// The pixel y a unit sorts by ("Vertical order", round-44 errata): dying, dead and removed units (remains and
    /// the remove order keep the dying state) sort at −32768. A unit inside something keeps its place: its pixel y
    /// doesn't change while it is inside, and coming out doesn't re-insert it, so it moves only by the sort.
    fn sort_y(&self, u: &Unit) -> i32 {
        if u.dying { acquire::OUT_OF_LIST_Y } else { u.pos.1 }
    }

    /// The vertical list, which is also the map view's draw list (map-drawing spec, "The draw list"): pixel y
    /// ascending, re-sorted once per cycle, with dying and dead units at the front.
    pub fn draw_list(&self) -> &[UnitId] {
        &self.ylist
    }

    /// The once-per-cycle sort at the end of phase 4.
    pub(crate) fn sort_vertical(&mut self) {
        let mut list = std::mem::take(&mut self.ylist);
        acquire::sort_pass(&mut list, |id| {
            self.unit(*id).map_or(acquire::OUT_OF_LIST_Y, |u| self.sort_y(u))
        });
        self.ylist = list;
    }
}

/// Chebyshev distance from `from` to `t`'s reference tile ("Distance").
pub(crate) fn distance(from: (i32, i32), t: &Unit) -> i32 {
    let rx = acquire::reference(from.0, t.tile.0, t.size.0 as i32);
    let ry = acquire::reference(from.1, t.tile.1, t.size.1 as i32);
    chebyshev(from, (rx, ry))
}

#[cfg(test)]
mod tests {
    use tidewater_formats::udta::flags;

    use super::*;
    use crate::world::tests::{field, run};

    const FOOTMAN: u8 = 0;
    const CATAPULT: u8 = 5;
    const ARCHER: u8 = 8;
    const PEASANT: u8 = 3;
    const FARM: u8 = 0x3a;
    const TOWER: u8 = 0x6e - 1;

    /// A 24 x 24 field: player 0 human, player 1 computer, player 2 a passive rescue player. Everyone sees 8 tiles.
    fn map() -> World {
        let mut w = field(24, 24);
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        w.players[2].controller = controller::RESCUE_PASSIVE;
        w.stats.0[PEASANT as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[PEASANT as usize].flags = flags::LAND | flags::PEON | flags::CAN_ATTACK;
        w.stats.0[TOWER as usize] = w.stats.0[FARM as usize];
        w.stats.0[TOWER as usize].flags = flags::BUILDING | flags::TOWER | flags::CAN_ATTACK;
        w.stats.0[TOWER as usize].attack_range = 6;
        w.stats.0[TOWER as usize].missile = 0x0f;
        w.stats.0[TOWER as usize].can_target = 7;
        for s in w.stats.0.iter_mut() {
            s.sight = 8;
            s.react_range_computer = 6;
            s.react_range_human = 4;
        }
        w
    }

    fn at(w: &World, id: UnitId) -> usize {
        w.index(id).unwrap()
    }

    fn order_of(w: &World, id: UnitId) -> Order {
        w.unit(id).unwrap().order
    }

    #[test]
    fn idle_reaction_by_player_kind() {
        let mut w = map();
        let human = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let computer = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        run(&mut w, 30);
        // Human: order 16 on the grunt. Computer: order 12, as the target can attack.
        assert_eq!(order_of(&w, human), Order::IdleReact { target: computer });
        assert!(matches!(order_of(&w, computer), Order::Chase { target } if target == human));
    }

    #[test]
    fn computer_footman_chases_buildings_but_not_peasants() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let farm = w.spawn(FARM, 0, (6, 4), 0).unwrap();
        run(&mut w, 30);
        assert_eq!(order_of(&w, f), Order::Chase { target: farm });

        let mut w = map();
        w.stats.0[PEASANT as usize].flags &= !flags::CAN_ATTACK;
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let p = w.spawn(PEASANT, 0, (6, 5), 0).unwrap();
        run(&mut w, 30);
        assert_eq!(order_of(&w, f), Order::IdleReact { target: p });
    }

    /// Orders spec, "A candidate is skipped if": its owner is allied to the searcher's owner. Setup spec: a passive
    /// rescue player is allied with every slot, so two passive players' units side by side don't react.
    #[test]
    fn allied_units_are_not_candidates() {
        let mut w = map();
        w.players[3].controller = controller::PASSIVE;
        w.players[2].allies = u16::MAX;
        w.players[3].allies = 1 << 2 | 1 << 3;
        let a = w.spawn(FOOTMAN, 2, (5, 5), 0).unwrap();
        let b = w.spawn(FOOTMAN, 3, (6, 5), 0).unwrap();
        run(&mut w, 30);
        assert_eq!((order_of(&w, a), order_of(&w, b)), (Order::Idle, Order::Idle));
        // The human and computer pair of `idle_reaction_by_player_kind`, allied both ways, don't react either.
        let mut w = map();
        w.players[0].allies = 0b11;
        w.players[1].allies = 0b11;
        let human = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let computer = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        run(&mut w, 30);
        assert_eq!(
            (order_of(&w, human), order_of(&w, computer)),
            (Order::Idle, Order::Idle)
        );
    }

    /// Unit-slots spec, "Creating a unit"; fog spec, "Unit visibility"; vector `xhuman4` slots 1435 and others: a new
    /// unit is hidden from everyone, computers included, until the next visibility pass, so units with an enemy in
    /// reach from the start acquire nothing in the first cycle.
    #[test]
    fn a_new_unit_is_hidden_from_everyone_until_the_next_visibility_pass() {
        let mut w = map();
        let human = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let computer = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        for id in [human, computer] {
            assert_eq!(w.unit(id).unwrap().hidden_from, 0xff);
            assert_eq!(w.search(at(&w, id), None, false), None);
            let i = at(&w, id);
            w.units[i].script.wait = 1;
        }
        w.step(&[]);
        for id in [human, computer] {
            let u = w.unit(id).unwrap();
            assert_eq!(
                (u.order, u.pending),
                (Order::Idle, None),
                "no acquisition in the first cycle"
            );
        }
        // After the pass both are candidates, and the next searches find them.
        assert_eq!(w.search(at(&w, computer), None, false), Some(human));
        run(&mut w, 30);
        assert!(matches!(order_of(&w, computer), Order::Chase { target } if target == human));
    }

    #[test]
    fn hidden_enemies_are_not_candidates() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        w.stats.0[FOOTMAN as usize].sight = 0;
        let g = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let gi = at(&w, g);
        w.units[gi].hidden_from |= 1;
        let i = at(&w, f);
        assert_eq!(w.search(i, None, false), None);
    }

    #[test]
    fn losing_the_target() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (9, 9), 0).unwrap();
        let (i, gi) = (at(&w, f), at(&w, g));
        w.units[gi].dying = true;
        w.units[i].order = Order::Attack { target: g };
        w.lose_target(i);
        // `human01-basics` n = 291: order 2 taking over with wait 3.
        assert_eq!((w.units[i].order, w.units[i].script.wait), (Order::Idle, 3));
        w.units[i].order = Order::AttackInPlace { target: g };
        w.lose_target(i);
        assert_eq!(w.units[i].order, Order::StandGround);
        // A pending order takes over instead.
        w.units[i].order = Order::Attack { target: g };
        w.units[i].pending = Some(Order::Move { goal: (1, 1) });
        w.lose_target(i);
        assert_eq!(
            (w.units[i].order, w.units[i].pending),
            (Order::Move { goal: (1, 1) }, None)
        );
    }

    #[test]
    fn smart_move_caps_the_wait() {
        for (owner, expected) in [(0, 10), (1, 37), (2, 10)] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, owner, (5, 5), 0).unwrap();
            let i = at(&w, f);
            w.units[i].script.wait = 37;
            let c = Command::Smart {
                player: owner,
                units: vec![f],
                target: None,
                tile: (10, 10),
            };
            assert_eq!(w.command_order(i, &c), Some(Order::Move { goal: (10, 10) }));
            assert_eq!(w.units[i].script.wait, expected, "owner {owner}");
        }
    }

    #[test]
    fn an_arriving_move_goes_idle_as_a_same_update_take_over() {
        // Human04 critters (n = 58): when order 3 arrives, order 2 takes over in the same update with wait 3 and no
        // idle logic run (orders spec, open question "Same-update take-over", default).
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        w.step(&[Command::Smart {
            player: 0,
            units: vec![f],
            target: None,
            tile: (7, 5),
        }]);
        let (mut moving, mut arrived) = (false, false);
        for _ in 0..200 {
            w.step(&[]);
            let u = w.unit(f).unwrap();
            moving |= matches!(u.order, Order::Move { .. });
            if moving && u.order == Order::Idle {
                assert_eq!(
                    (u.tile, u.pending, u.script.anim, u.script.wait),
                    ((7, 5), None, anim::STILL, 3)
                );
                arrived = true;
                break;
            }
        }
        assert!(arrived);
    }

    #[test]
    fn take_overs_and_immediate_orders_keep_the_frame() {
        // Animation-scripts spec, round-33 errata: no way of starting an animation changes the frame (Human04 peon
        // 1548 returning gold: frame 0 from the arrival switch kept through order 25's take-over at n = 374).
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].script.frame = 15;
        w.take_over(i, Order::Move { goal: (9, 9) });
        assert_eq!((w.units[i].script.frame, w.units[i].script.wait), (15, 3));
        w.stop(i);
        assert_eq!((w.units[i].script.frame, w.units[i].script.wait), (15, 1));
    }

    #[test]
    fn orders_4_and_10_also_go_idle_through_a_same_update_take_over() {
        // Orders spec, "Arriving and losing the target": orders 3, 4 and 10 forget the attacker on arrival (the
        // Human04 critters' order 3 test above), and idle takes over in the same update (still animation, wait 3).
        for o in [
            Order::Move { goal: (7, 5) },
            Order::MoveAttack { goal: (7, 5) },
            Order::AttackMove { goal: (7, 5) },
        ] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let i = at(&w, f);
            w.give(i, o);
            let (mut moving, mut arrived) = (false, false);
            for _ in 0..200 {
                w.step(&[]);
                let u = w.unit(f).unwrap();
                moving |= u.order == o;
                if moving && u.order == Order::Idle {
                    assert_eq!(
                        (u.tile, u.pending, u.script.anim, u.script.wait, u.attacked_by),
                        ((7, 5), None, anim::STILL, 3, None),
                        "{o:?}"
                    );
                    arrived = true;
                    break;
                }
            }
            assert!(arrived, "{o:?}");
        }
    }

    /// Walls all round (5, 5): no route leads out.
    fn walled_in() -> World {
        let mut w = map();
        for y in 4..=6 {
            for x in 4..=6 {
                if (x, y) != (5, 5) {
                    let n = w.terrain.at(x, y).unwrap();
                    w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
                }
            }
        }
        w
    }

    #[test]
    fn a_move_with_no_route_goes_idle_at_once_and_runs_the_idle_logic() {
        // Orders spec, "Arriving and losing the target", no route (static study vector): order 2 as an immediate order
        // and the idle logic in the same update. Here the idle logic of a computer player's unit (reaction range 6)
        // acquires the enemy two tiles away at once.
        let mut w = walled_in();
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 0, (5, 3), 0).unwrap();
        // New units are hidden from everyone until a visibility pass.
        w.unit_visibility();
        let i = at(&w, f);
        w.units[i].order = Order::Move { goal: (15, 15) };
        w.units[i].script.wait = 1;
        // The move's logic runs at its next step done; it never reaches idle with the take-over's wait of 3.
        let mut after = None;
        for _ in 0..20 {
            w.step(&[]);
            let u = w.unit(f).unwrap();
            if !matches!(u.order, Order::Move { .. }) {
                after = Some(u.order);
                break;
            }
        }
        let o = after.expect("the move ends");
        assert!(matches!(target_of(o), Some(t) if t == g), "{o:?}");
    }

    /// Orders spec, no route, the round-39 patrol vectors (`human10-10000` n = 6868 and 6907, `xorc7-7000` n = 806):
    /// a patrol that finds no route becomes an immediate idle (order 2, wait 1), and the idle logic's look-around runs
    /// in the same update (here a turn draw).
    #[test]
    fn a_patrol_with_no_route_goes_idle_at_once_and_runs_the_idle_logic() {
        let mut w = walled_in();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].order = Order::Patrol {
            to: (15, 15),
            back: (5, 5),
        };
        let before = w.rng.state;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!((u.order, u.pending, u.script.wait), (Order::Idle, None, 1));
        assert_ne!(w.rng.state, before, "the look-around drew in the same update");
    }

    #[test]
    fn smart_move_on_an_own_unit_follows_it() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 0, (9, 9), 0).unwrap();
        let i = at(&w, f);
        let c = Command::Smart {
            player: 0,
            units: vec![f],
            target: Some(g),
            tile: (9, 9),
        };
        assert_eq!(w.command_order(i, &c), Some(Order::Follow { target: g }));
    }

    #[test]
    fn attack_move_is_remembered_and_resumed_in_enhanced() {
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        let c = Command::AttackMove {
            player: 0,
            units: vec![f],
            to: (15, 5),
        };
        assert_eq!(w.command_order(i, &c), Some(Order::AttackMove { goal: (15, 5) }));
        assert_eq!(w.units[i].remembered, Some(Remembered::AttackMove { goal: (15, 5) }));
        // An immediate idle never resumes, and keeps the remembered order.
        w.stop(i);
        assert_eq!((w.units[i].order, w.units[i].resume), (Order::Idle, 0));
        assert_eq!(w.units[i].remembered, Some(Remembered::AttackMove { goal: (15, 5) }));
        // A pending idle (here from losing a target in order 16) becomes the attack-move.
        w.give(i, Order::Idle);
        assert_eq!(w.units[i].resume, RESUME_ATTACK_MOVE);
        assert_eq!(w.units[i].pending, Some(Order::AttackMove { goal: (15, 5) }));
        // The countdown is running: the next pending idle stays idle.
        w.give(i, Order::Idle);
        assert_eq!(w.units[i].pending, Some(Order::Idle));
        // A plain move forgets it.
        let c = Command::Move {
            player: 0,
            units: vec![f],
            to: (1, 1),
        };
        w.command_order(i, &c);
        assert_eq!(w.units[i].remembered, None);
    }

    #[test]
    fn a_move_command_ends_a_remembered_attack_move() {
        // Round-36 vector (the xhuman10 hero): command 8, then a move; the move arrives at idle (still animation,
        // wait 3) and stays idle, because the move command cleared the remembered attack-move.
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.step(&[Command::AttackMove {
            player: 0,
            units: vec![f],
            to: (15, 9),
        }]);
        for _ in 0..10 {
            w.step(&[]);
        }
        assert!(w.units[i].remembered.is_some());
        w.step(&[Command::Move {
            player: 0,
            units: vec![f],
            to: (8, 2),
        }]);
        assert_eq!(w.units[i].remembered, None);
        let mut arrived = false;
        for _ in 0..200 {
            w.step(&[]);
            let u = &w.units[i];
            if u.order == Order::Idle && u.pending.is_none() {
                assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
                arrived = true;
                break;
            }
        }
        assert!(arrived);
        for _ in 0..60 {
            w.step(&[]);
        }
        assert_eq!((w.units[i].order, w.units[i].resume), (Order::Idle, 0));
    }

    #[test]
    fn arriving_resumes_a_remembered_attack_move_but_no_route_does_not() {
        // The resume rule for arrivals (no command in between, so the remembered order is still set): a footman on a
        // remembered attack-move to a tile 10 tiles away is moved 2 tiles aside and arrives: the attack-move is given
        // again (countdown 20), taking over in the same update. If the move finds no route: immediate idle, nothing
        // resumed.
        for walled in [false, true] {
            let mut w = map();
            w.rules.enhanced = true;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let i = at(&w, f);
            w.units[i].remembered = Some(Remembered::AttackMove { goal: (15, 5) });
            if walled {
                for (x, y) in [(4, 6), (5, 6), (6, 6), (4, 8), (5, 8), (6, 8), (4, 7), (6, 7)] {
                    w.terrain.set_bits(x, y, 0x80);
                }
            }
            w.take_over(i, Order::Move { goal: (5, 7) });
            let mut seen = None;
            for _ in 0..100 {
                w.step(&[]);
                let u = &w.units[i];
                if u.order != (Order::Move { goal: (5, 7) }) {
                    seen = Some((u.order, u.pending, u.resume, u.script.wait));
                    break;
                }
            }
            let got = seen.expect("the move ends");
            if walled {
                assert_eq!(got, (Order::Idle, None, 0, 1));
                assert_eq!(w.units[i].remembered, Some(Remembered::AttackMove { goal: (15, 5) }));
            } else {
                assert_eq!(got.0, Order::AttackMove { goal: (15, 5) });
                assert_eq!(got.2, RESUME_ATTACK_MOVE);
            }
        }
    }

    #[test]
    fn a_patrol_turnaround_pulls_its_new_goal_back() {
        // Round-44 errata, "Order 5 (patrol)": the turnaround is a command, so its new goal is pulled back. The far end
        // (0, 2) and (1, 2) are blocked: the new goal is (1, 2), the last blocked tile before a usable one.
        let mut w = map();
        for x in 0..2 {
            let n = w.terrain.at(x, 2).unwrap();
            w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
        }
        let f = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
        let i = at(&w, f);
        let out = Order::Patrol {
            to: (5, 2),
            back: (0, 2),
        };
        w.take_over(i, out);
        let mut n = 0;
        while order_of(&w, f) == out {
            w.step(&[]);
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(
            order_of(&w, f),
            Order::Patrol {
                to: (1, 2),
                back: (5, 2)
            }
        );
    }

    #[test]
    fn a_patrol_arrival_takes_over_and_steps_three_cycles_later() {
        // The xorc10 vector: on arrival the unit is given a patrol to its other end, with its own tile as the new
        // other end; it takes over in the same update (still animation, wait 3) and steps at its next run.
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
        let i = at(&w, f);
        w.take_over(
            i,
            Order::Patrol {
                to: (5, 2),
                back: (2, 2),
            },
        );
        // Round-41 errata: the turnaround is a command, so it clears the blocked counter.
        w.units[i].blocked = 3;
        let mut n = 0;
        while order_of(&w, f)
            == (Order::Patrol {
                to: (5, 2),
                back: (2, 2),
            })
        {
            w.step(&[]);
            n += 1;
            assert!(n < 100);
        }
        let u = w.unit(f).unwrap();
        assert_eq!(
            u.order,
            Order::Patrol {
                to: (2, 2),
                back: u.tile
            }
        );
        assert_eq!((u.script.anim, u.script.wait, u.blocked), (anim::STILL, 3, 0));
        let here = u.tile;
        w.step(&[]);
        w.step(&[]);
        let u = w.unit(f).unwrap();
        assert_eq!((u.tile, u.script.anim), (here, anim::STILL));
        // The step starts at the next run and takes the next tile at once.
        w.step(&[]);
        let u = w.unit(f).unwrap();
        assert_eq!((u.tile, u.script.anim), ((here.0 - 1, 2), anim::MOVE));
    }

    #[test]
    fn moves_that_attack_on_the_way_use_the_still_animation() {
        // Orders spec, the order list: orders 3, 4, 5 and 10 use animation 2, so the attack-type fallbacks (only
        // for animation-4 orders) never apply to them.
        let goal = (1, 1);
        for o in [
            Order::Move { goal },
            Order::MoveAttack { goal },
            Order::Patrol { to: goal, back: goal },
            Order::AttackMove { goal },
        ] {
            assert_eq!(order_anim(o), anim::STILL, "{o:?}");
        }
    }

    #[test]
    fn classic_mode_remembers_nothing() {
        let mut w = map();
        w.rules.enhanced = false;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        let c = Command::Patrol {
            player: 0,
            units: vec![f],
            to: (15, 5),
        };
        assert_eq!(
            w.command_order(i, &c),
            Some(Order::Patrol {
                to: (15, 5),
                back: (5, 5)
            })
        );
        assert_eq!(w.units[i].remembered, None);
    }

    /// Movement spec test vector (round-42 errata): an idle unit with blocked counter 2 acquires a target. The resulting
    /// order is a command: counter 0, route dropped, and it takes over at once.
    #[test]
    fn an_acquisition_clears_the_blocked_counter_and_the_route() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].attacked_by = Some(g);
        w.units[i].blocked = 2;
        w.units[i].path = vec![(5, 6), (5, 7)];
        assert!(w.acquire(i, Acquires::Idle, false));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.blocked, u.path.len(), u.script.wait),
            (Order::Chase { target: g }, 0, 0, 3)
        );
    }

    #[test]
    fn attacked_idle_footman_chases_its_attacker() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].attacked_by = Some(g);
        // Order 12 (chase) rather than 16, as the footman has an attacker.
        assert!(w.acquire(i, Acquires::Idle, false));
        assert_eq!(order_of(&w, f), Order::Chase { target: g });
    }

    /// Orders spec, "Same-update take-over" (round-39 errata), the `orc02` n = 570 vector and the static-study one: a
    /// unit in order 12 that acquires another target takes it at once. Out of reach its logic carries on in the same
    /// update and it keeps walking (a step, the move animation, no wait of 3); in reach it turns to strike, and (our
    /// reading of `orc02` n = 343) the attack animation restarts with wait 3.
    #[test]
    fn a_target_switch_in_order_12_carries_on_in_the_same_update() {
        for (foe_tile, reach) in [((5, 8), false), ((5, 6), true)] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let old = w.spawn(FOOTMAN, 1, (20, 20), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, foe_tile, 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let i = at(&w, f);
            w.units[i].pending = None;
            w.units[i].order = Order::Chase { target: old };
            w.units[i].facing = 0;
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!(u.order, Order::Chase { target: g }, "{foe_tile:?}");
            if reach {
                assert_eq!(
                    (u.tile, u.facing, u.script.anim, u.script.wait),
                    ((5, 5), 4, anim::ATTACK, 3)
                );
            } else {
                assert_eq!((u.tile, u.facing, u.script.anim), ((5, 6), 4, anim::MOVE));
                assert_ne!(u.script.wait, 3, "no take-over wait");
            }
        }
    }

    /// Round-41 errata, "Resulting order", and the `orc02-assault` n = 617 vector: an order-12 switch to a target in
    /// reach while still cooling down takes effect at once, and the animation restarts with the cooldown left as its
    /// wait (no wait of 3, no turn).
    #[test]
    fn a_switch_in_reach_while_cooling_down_keeps_the_cooldown_wait() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let old = w.spawn(FOOTMAN, 1, (20, 20), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, f);
        let u = &mut w.units[i];
        u.pending = None;
        u.order = Order::Chase { target: old };
        u.facing = 0;
        u.cooldown = 23;
        u.script.wait = 1;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.facing, u.script.anim, u.script.frame, u.script.wait),
            (Order::Chase { target: g }, 0, anim::ATTACK, 0, 23)
        );
    }

    /// Round-41 errata, "Attack-move: orders 4, 5 and (Enhanced) 10": a unit in order 4 that picks a target already
    /// within strike reach doesn't strike. It gets order 12 as a command (pending, target at once), keeps order 4 and
    /// steps toward the target in the same update (the `orc02-victory` n = 4789 and `xhuman2` n = 2048 vectors).
    #[test]
    fn order_4_heads_for_a_new_target_even_in_reach() {
        let mut w = map();
        let a = w.spawn(ARCHER, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (5, 8), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, a);
        let goal = (5, 20);
        w.units[i].pending = None;
        w.units[i].order = Order::MoveAttack { goal };
        assert!(w.within_strike_reach(&w.units[i], w.unit(g).unwrap()));
        let before = w.rng;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.tile, u.script.anim),
            (
                Order::MoveAttack { goal },
                Some(Order::Chase { target: g }),
                (5, 6),
                anim::MOVE
            )
        );
        assert!(w.missiles.iter().all(Option::is_none) && w.rng == before, "no strike");
    }

    /// Round-41 errata, "Friendly-fire check": made in the order's logic before the range test, so a ground attacker
    /// whose target stands among its own units goes idle in the same update (still animation, wait 3, facing kept),
    /// in range or not (the `xhuman2` n = 2226 vector: the ballista 8 tiles away).
    #[test]
    fn the_friendly_fire_check_idles_in_the_same_update() {
        for foe_tile in [(18, 20), (20, 5)] {
            let mut w = map();
            w.stats.0[CATAPULT as usize].flags |= flags::GROUND_ATTACK;
            let c = w.spawn(CATAPULT, 0, (14, 20), 0).unwrap();
            let g = w.spawn(FOOTMAN, 3, foe_tile, 0).unwrap();
            w.spawn(FOOTMAN, 0, (foe_tile.0 + 1, foe_tile.1), 0).unwrap();
            let i = at(&w, c);
            w.units[i].order = Order::IdleReact { target: g };
            w.units[i].facing = 6;
            w.attack_in_place(i, g);
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait, u.facing),
                (Order::Idle, None, anim::STILL, 3, 6),
                "{foe_tile:?}"
            );
        }
    }

    /// Round-43 errata (`xhuman4` ballista 1488, n = 210): after order 16's search switches to a new target, the run
    /// goes on and the friendly-fire check is made on the new target; a unit still cooling down isn't checked and keeps
    /// the new target.
    #[test]
    fn the_friendly_fire_check_is_made_on_a_switched_target() {
        for cooling in [false, true] {
            let mut w = map();
            w.stats.0[CATAPULT as usize].flags |= flags::GROUND_ATTACK;
            let c = w.spawn(CATAPULT, 0, (14, 20), 0).unwrap();
            let old = w.spawn(FOOTMAN, 3, (16, 20), 0).unwrap();
            let new = w.spawn(FOOTMAN, 3, (18, 20), 0).unwrap();
            w.spawn(FOOTMAN, 0, (19, 20), 0).unwrap();
            w.step(&[]);
            let oi = at(&w, old);
            w.units[oi].dying = true;
            let i = at(&w, c);
            w.units[i].order = Order::IdleReact { target: old };
            w.units[i].attacked_by = None;
            w.units[i].pending = None;
            w.units[i].script.wait = 1;
            w.units[i].cooldown = if cooling { 10 } else { 0 };
            assert!(w.acquire(i, Acquires::Idle, false));
            let u = &w.units[i];
            let want = if cooling {
                Order::IdleReact { target: new }
            } else {
                Order::Idle
            };
            assert_eq!(u.order, want, "cooling {cooling}");
            if !cooling {
                assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
            }
        }
    }

    /// Round-43 errata, "A dying attacker in orders 9, 11 and 17": each run first forgets a dying attacker, before the
    /// target check, so losing a dying target that is also the attacker leaves no attacker (`orc09-naval` destroyer
    /// 1497, n = 510); a living attacker is kept.
    #[test]
    fn order_9_forgets_a_dying_attacker_first() {
        let step_done = Effects {
            step_done: true,
            ..Default::default()
        };
        for dying in [true, false] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (12, 12), 0).unwrap();
            let a = w.spawn(ARCHER, 1, (5, 9), 0).unwrap();
            w.step(&[]);
            let (target, attacker) = if dying { (g, g) } else { (g, a) };
            let gi = at(&w, g);
            w.units[gi].dying = dying;
            let i = at(&w, f);
            w.units[i].order = Order::Attack { target };
            w.units[i].pending = None;
            w.units[i].attacked_by = Some(attacker);
            w.order(i, step_done);
            let u = &w.units[i];
            if dying {
                assert_eq!((u.order, u.attacked_by, u.script.wait), (Order::Idle, None, 3));
            } else {
                assert_eq!(u.attacked_by, Some(a));
            }
        }
    }

    #[test]
    fn standing_ground_takes_its_out_of_range_attacker_then_forgets_it() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (10, 5), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, f);
        w.units[i].order = Order::StandGround;
        w.units[i].attacked_by = Some(a);
        assert_eq!(w.search(i, Some(a), false), Some(a));
        assert!(w.acquire(i, Acquires::Other, false));
        assert_eq!(w.units[i].order, Order::AttackInPlace { target: a });
        // Round-41 errata: order 13 takes over in the same update (still animation, wait 3, target null).
        w.attack_in_place(i, a);
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.script.anim, u.script.wait),
            (Order::StandGround, None, anim::STILL, 3)
        );
    }

    #[test]
    fn towers_ignore_their_attacker() {
        let mut w = map();
        let t = w.spawn(TOWER, 0, (5, 5), 0).unwrap();
        let cat = w.spawn(CATAPULT, 1, (13, 5), 0).unwrap();
        let f = w.spawn(FOOTMAN, 1, (9, 5), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, t);
        w.units[i].attacked_by = Some(cat);
        assert!(w.is_watching_tower(i));
        assert_eq!(w.search(i, None, true), Some(f));
        // At 7 tiles (beyond range 6, past the extra column) there is nothing.
        let fi = at(&w, f);
        w.units[fi].tile = (13, 13);
        assert_eq!(w.search(i, None, true), None);
    }

    /// "Attack-move" (round-44 errata, the `human06-31357` axe thrower 1512 vector at n = 16277): a building target is
    /// headed for at the footprint tile of the building goal rule with the building's goal zone, as an attack on it
    /// walks, not at the building's own tile. A unit target is headed for at its tile.
    #[test]
    fn attack_move_heads_for_a_building_as_an_attack_would() {
        for building in [true, false] {
            let mut w = map();
            let a = w.spawn(ARCHER, 0, (5, 2), 0).unwrap();
            let t = if building {
                w.spawn(FARM, 1, (4, 6), 0).unwrap()
            } else {
                w.spawn(FOOTMAN, 1, (4, 6), 0).unwrap()
            };
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let i = at(&w, a);
            w.units[i].pending = None;
            w.units[i].order = Order::MoveAttack { goal: (5, 20) };
            let (mut tile_walk, mut attack_walk) = (w.clone(), w.clone());
            w.head_for(i, t);
            tile_walk.move_toward(i, Goal::tile((4, 6), 0));
            let tu = tile_walk.units[i].clone();
            let goal = Goal {
                zone: Some(crate::battle::goal_zone(w.unit(t).unwrap())),
                ..Goal::tile(crate::movement::building_goal((5, 2), w.unit(t).unwrap()), 0)
            };
            attack_walk.move_toward(i, if building { goal } else { Goal::tile((4, 6), 0) });
            let step = w.units[i].tile;
            assert_eq!(step, attack_walk.units[i].tile, "building {building}");
            assert_eq!(step == tu.tile, !building, "building {building}: {step:?}");
        }
    }

    /// "Choice" (round-44 errata, the `human06-31357` ballista 1582 vector at n = 23683): an attacker inside the
    /// unit's minimum range scores 0 and doesn't seed the search, which then picks a box candidate; with none, nothing
    /// is found. An attacker that can be attacked still wins a tie.
    #[test]
    fn an_attacker_inside_the_minimum_range_does_not_seed_the_search() {
        let mut w = map();
        w.stats.0[CATAPULT as usize].missile = crate::combat::missile::CATAPULT;
        let c = w.spawn(CATAPULT, 0, (5, 5), 0).unwrap();
        let near = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, c);
        assert_eq!(w.search(i, Some(near), false), None);
        let far = w.spawn(FOOTMAN, 1, (8, 5), 0).unwrap();
        w.step(&[]);
        let i = at(&w, c);
        assert_eq!(w.search(i, Some(near), false), Some(far));
        // A seed with a score above 0 is kept on a tie: a second footman at the same distance.
        let twin = w.spawn(FOOTMAN, 1, (8, 6), 0).unwrap();
        w.step(&[]);
        let i = at(&w, c);
        assert_eq!(w.search(i, Some(twin), false), Some(twin));
    }

    /// "Order 14 (towers)" and the `xhuman10-spells` vector (n = 5): a tower fires at once in its logic run, stores
    /// no target, doesn't turn and has no cooldown; between step dones it does nothing.
    #[test]
    fn a_firing_tower_stores_no_target_and_does_not_turn() {
        let mut w = map();
        let t = w.spawn(TOWER, 0, (5, 5), 0).unwrap();
        let f = w.spawn(FOOTMAN, 1, (9, 5), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, t);
        w.units[i].facing = 0;
        w.missiles.iter_mut().for_each(|m| *m = None);
        w.tower(i, Effects::default());
        assert!(
            w.missiles.iter().flatten().next().is_none(),
            "no shot between step dones"
        );
        w.tower(
            i,
            Effects {
                step_done: true,
                ..Default::default()
            },
        );
        let shots: Vec<_> = w.missiles.iter().flatten().map(|m| (m.attacker, m.target)).collect();
        assert_eq!(shots, [(t, Some(f))]);
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.facing, u.cooldown),
            (Order::TowerWatch { target: None }, 0, 0)
        );
    }

    #[test]
    fn a_tower_finding_nothing_stays_in_its_attack_animation() {
        // "Order 14 (towers)" and the human12 / xorc3 vectors: with no enemy in range each run finds nothing and
        // changes nothing; the tower keeps order 14 and animation 4.
        let mut w = map();
        let t = w.spawn(TOWER, 0, (5, 5), 0).unwrap();
        w.spawn(FOOTMAN, 1, (20, 20), 0).unwrap();
        // Order 14 is pending until the tower's first step done.
        while order_of(&w, t) == Order::Idle {
            w.step(&[]);
        }
        for _ in 0..60 {
            w.step(&[]);
            let u = w.unit(t).unwrap();
            assert_eq!(u.order, Order::TowerWatch { target: None });
            assert_eq!(u.script.anim, anim::ATTACK);
        }
    }

    #[test]
    fn friendly_fire_thresholds() {
        for (owner, own, strikes) in [(0, 1, false), (1, 2, true), (1, 3, false), (2, 1, true)] {
            let mut w = map();
            w.stats.0[CATAPULT as usize].flags |= flags::GROUND_ATTACK;
            let c = w.spawn(CATAPULT, owner, (14, 20), 0).unwrap();
            let g = w.spawn(FOOTMAN, 3, (20, 20), 0).unwrap();
            for t in [(21, 19), (19, 21), (21, 21)].iter().take(own) {
                w.spawn(FOOTMAN, owner, *t, 0).unwrap();
            }
            let i = at(&w, c);
            w.units[i].order = Order::IdleReact { target: g };
            assert_eq!(!w.holds_fire(i, (20, 20)), strikes, "owner {owner}, {own} own units");
        }
    }

    #[test]
    fn fleeing_peasant_moves_away() {
        let mut w = map();
        let p = w.spawn(PEASANT, 0, (10, 10), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (6, 10), 0).unwrap();
        let (i, ai) = (at(&w, p), at(&w, a));
        w.units[ai].facing = 2;
        w.units[i].attacked_by = Some(a);
        assert!(w.flee(i));
        // The move takes over in the same update (round-40 errata).
        let Order::Move { goal } = w.units[i].order else {
            panic!("expected a move, got {:?}", w.units[i].order)
        };
        assert_eq!(w.units[i].pending, None);
        // (r mod 8) - 2 + 4 east, (r mod 8) - 2 vertically.
        assert!((12..=19).contains(&goal.0) && (8..=15).contains(&goal.1), "{goal:?}");
        assert_eq!(w.units[i].attacked_by, None);
    }

    #[test]
    fn a_flee_move_is_a_command_whose_goal_is_pulled_back() {
        // Round-44 errata, "Fleeing" step 6: the flee tile is pulled back off ground the unit can't stand on, and the
        // blocked counter is cleared. Every tile from column 12 on is blocked, so the flee tile east of (10, 10) is
        // pulled back to the last blocked tile on the line, in column 12.
        let mut w = map();
        for y in 0..w.terrain.height as i32 {
            for x in 12..w.terrain.width as i32 {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
            }
        }
        let p = w.spawn(PEASANT, 0, (10, 10), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (6, 10), 0).unwrap();
        let (i, ai) = (at(&w, p), at(&w, a));
        w.units[ai].facing = 2;
        w.units[i].attacked_by = Some(a);
        w.units[i].blocked = 5;
        assert!(w.flee(i));
        let Order::Move { goal } = w.units[i].order else {
            panic!("expected a move, got {:?}", w.units[i].order)
        };
        assert_eq!((goal.0, w.units[i].blocked), (12, 0), "{goal:?}");
    }

    #[test]
    fn peasant_hit_by_a_peon_hits_back() {
        let mut w = map();
        let p = w.spawn(PEASANT, 0, (10, 10), 0).unwrap();
        let q = w.spawn(PEASANT, 1, (11, 10), 0).unwrap();
        let i = at(&w, p);
        w.units[i].attacked_by = Some(q);
        assert!(w.flee(i));
        assert_eq!(w.units[i].pending, Some(Order::Attack { target: q }));
    }

    #[test]
    fn following_turns_into_guarding() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 0, (6, 6), 0).unwrap();
        let i = at(&w, f);
        w.units[i].order = Order::Follow { target: g };
        w.follow(i, g);
        assert_eq!(w.units[i].order, Order::Guard { target: g });
        // The guarded unit starts harvesting: a move to the guard's own tile.
        let gi = at(&w, g);
        w.units[gi].order = Order::ReturnGoods { depot: None };
        w.follow(i, g);
        assert_eq!(w.units[i].pending, Some(Order::Move { goal: (5, 5) }));
    }

    #[test]
    fn vertical_list_sorts_once_per_cycle() {
        let mut w = map();
        let ids: Vec<UnitId> = [3, 6, 1]
            .iter()
            .map(|&y| w.spawn(FOOTMAN, 3, (2, y), 0).unwrap())
            .collect();
        // Spawning inserts in order, so the list is sorted.
        let ys = |w: &World| w.ylist.iter().map(|id| w.unit(*id).unwrap().pos.1).collect::<Vec<_>>();
        assert_eq!(ys(&w), [32, 96, 192]);
        assert_eq!(w.ylist.len(), ids.len());
    }

    #[test]
    fn draw_list_vectors() {
        // Map-drawing spec, "Test vectors": a unit at y 320 is drawn before (under) one at y 352.
        let mut w = map();
        let b = w.spawn(FOOTMAN, 1, (4, 11), 0).unwrap();
        let a = w.spawn(FOOTMAN, 0, (4, 10), 0).unwrap();
        assert_eq!(w.draw_list(), [a, b]);
        // Two units at the same y, A before B; a new unit C at that y goes in front of both.
        let mut w = map();
        let a = w.spawn(FOOTMAN, 0, (2, 5), 0).unwrap();
        let b = w.spawn(FOOTMAN, 0, (4, 5), 0).unwrap();
        let ab = w.draw_list().to_vec();
        let c = w.spawn(FOOTMAN, 0, (6, 5), 0).unwrap();
        let mut want = vec![c];
        want.extend(ab.iter().copied());
        assert!(ab.contains(&a) && ab.contains(&b));
        assert_eq!(w.draw_list(), want);
        // A dying unit sorts first, under every other unit.
        let i = at(&w, b);
        w.units[i].dying = true;
        w.sort_vertical();
        assert_eq!(w.draw_list()[0], b);
    }

    #[test]
    fn inside_units_keep_their_list_place_and_coming_out_does_not_reinsert() {
        // Orders spec, "Vertical order" (round-44 errata): a unit that goes in at pixel y 160 keeps its place while
        // inside, and coming out at y 320 starts from that place; only the sort moves it.
        let mut w = map();
        let a = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
        let b = w.spawn(FOOTMAN, 0, (2, 5), 0).unwrap();
        let c = w.spawn(FOOTMAN, 0, (2, 8), 0).unwrap();
        assert_eq!(w.draw_list(), [a, b, c]);
        let i = at(&w, b);
        w.units[i].inside = true;
        w.sort_vertical();
        assert_eq!(w.draw_list(), [a, b, c], "inside: not moved to the front");
        let i = at(&w, b);
        w.units[i].inside = false;
        w.units[i].tile = (2, 10);
        w.units[i].pos = (64, 320);
        assert_eq!(w.draw_list(), [a, b, c], "coming out doesn't re-insert");
        w.sort_vertical();
        assert_eq!(w.draw_list(), [a, c, b]);
    }

    #[test]
    fn lumber_workers_flee_gold_workers_do_not() {
        let mut w = map();
        w.stats.0[PEASANT as usize].flags &= !flags::CAN_ATTACK;
        let p = w.spawn(PEASANT, 0, (10, 10), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (6, 10), 0).unwrap();
        let mine = w.spawn(FARM, 3, (14, 14), 0).unwrap();
        let i = at(&w, p);
        let walking_to_mine = Order::Harvest {
            source: Some(mine),
            tree: (0, 0),
            chopping: false,
        };
        let chopping = Order::Harvest {
            source: None,
            tree: (11, 10),
            chopping: false,
        };
        for (order, flees) in [(walking_to_mine, false), (chopping, true)] {
            w.units[i].order = order;
            w.units[i].pending = None;
            w.units[i].attacked_by = Some(a);
            assert_eq!(w.flee(i), flees, "{order:?}");
        }
        // A computer player's worker flees only when idle.
        w.players[0].controller = controller::COMPUTER;
        w.units[i].attacked_by = Some(a);
        assert!(!w.flee(i));
    }

    /// Round-45 errata, "Losing the target while cooling down" (the `xorc01-death-knight` ogre 1488 vector): in orders
    /// 12, 15 and 16 a unit whose target died while it is still cooling down keeps its order, its attack animation
    /// restarting with the cooldown left as its wait and its pending order cleared. The first run after the cooldown
    /// loses the target.
    #[test]
    fn losing_the_target_while_cooling_down_keeps_the_order() {
        type Make = fn(UnitId) -> Order;
        let cases: [(Make, Order); 3] = [
            (|t| Order::Chase { target: t }, Order::Idle),
            (|t| Order::AttackInPlace { target: t }, Order::StandGround),
            (|t| Order::IdleReact { target: t }, Order::Idle),
        ];
        for (make, lost) in cases {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.step(&[]);
            let gi = at(&w, g);
            w.kill(gi);
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.order = make(g);
            u.pending = Some(Order::Idle);
            u.cooldown = 15;
            u.script.wait = 1;
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.frame, u.script.wait),
                (make(g), None, anim::ATTACK, 0, 15)
            );
            let u = &mut w.units[i];
            u.cooldown = 0;
            u.script.wait = 1;
            assert!(w.run_order(i));
            assert_eq!(w.units[i].order, lost);
        }
    }

    /// Round-45 errata, "Resuming patrol and attack-move": the resumed order is a full command. Its goal is pulled
    /// back, the cached route dropped, the blocked counter cleared and a stale attacker forgotten.
    #[test]
    fn a_resumed_attack_move_is_a_full_command() {
        let mut w = map();
        w.rules.enhanced = true;
        // Every tile from column 12 on is blocked, so the goal is pulled back to column 12 (as for the flee move).
        for y in 0..w.terrain.height as i32 {
            for x in 12..w.terrain.width as i32 {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
            }
        }
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let a = w.spawn(FOOTMAN, 1, (9, 9), 0).unwrap();
        let ai = at(&w, a);
        w.kill(ai);
        let i = at(&w, f);
        let u = &mut w.units[i];
        u.remembered = Some(Remembered::AttackMove { goal: (15, 5) });
        u.path = vec![(6, 5)];
        u.blocked = 3;
        u.attacked_by = Some(a);
        w.give(i, Order::Idle);
        let u = &w.units[i];
        assert_eq!(u.pending, Some(Order::AttackMove { goal: (12, 5) }));
        assert_eq!(
            (u.path.len(), u.blocked, u.attacked_by, u.commanded),
            (0, 0, None, true)
        );
        // The remembered goal itself is kept.
        assert_eq!(u.remembered, Some(Remembered::AttackMove { goal: (15, 5) }));
    }

    /// Round-45 errata, "Commands forget a stale attacker": a command forgets a dying attacker, and keeps a live one. A
    /// target unit given with the command gets the same check (static study only): a smart move on a dying unit is a
    /// move to the tile.
    #[test]
    fn a_command_forgets_a_dying_attacker() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let a = w.spawn(FOOTMAN, 1, (9, 9), 0).unwrap();
        let own = w.spawn(FOOTMAN, 0, (3, 3), 0).unwrap();
        let i = at(&w, f);
        w.units[i].attacked_by = Some(a);
        w.command(i, Order::Move { goal: (1, 1) });
        assert_eq!(w.units[i].attacked_by, Some(a));
        let ai = at(&w, a);
        w.kill(ai);
        w.command(i, Order::Move { goal: (2, 2) });
        assert_eq!(w.units[i].attacked_by, None);
        let oi = at(&w, own);
        w.kill(oi);
        let c = Command::Smart {
            player: 0,
            units: vec![f],
            target: Some(own),
            tile: (3, 3),
        };
        assert_eq!(w.command_order(i, &c), Some(Order::Move { goal: (3, 3) }));
    }

    /// Round-45 errata, "No route": a move with no route keeps its attacker, so the idle logic that runs in the same
    /// update may react to it. Here a worker flees (the `xorc10-dragons` footman 1520 vector, n = 464).
    #[test]
    fn a_move_with_no_route_keeps_its_attacker_and_flees() {
        let mut w = walled_in();
        let p = w.spawn(PEASANT, 0, (5, 5), 0).unwrap();
        let a = w.spawn(FOOTMAN, 1, (9, 5), 0).unwrap();
        w.unit_visibility();
        let i = at(&w, p);
        w.units[i].order = Order::Move { goal: (15, 15) };
        w.units[i].attacked_by = Some(a);
        let before = w.rng.state;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert!(
            matches!(u.order, Order::Move { goal } if goal != (15, 15)),
            "{:?}",
            u.order
        );
        assert_eq!((u.pending, u.attacked_by, u.script.wait), (None, None, 3));
        assert_ne!(w.rng.state, before, "the flee drew");
    }

    /// Round-45 errata, "Reacting to an attacker" step 3 and "Special cases": an idle mage-type unit hit by a combat
    /// unit, with nothing it may take in its box, runs the reaction, which treats it as a non-combat unit, so it flees
    /// (the `xorc01-death-coil` death knight 1491 vector, n = 1431). With an enemy worker in its box that outscores
    /// the attacker it attacks the worker.
    #[test]
    fn an_idle_mage_flees_its_attacker_unless_it_may_take_a_better_target() {
        const MAGE: u8 = 0x0a;
        for worker in [false, true] {
            let mut w = map();
            w.stats.0[MAGE as usize] = w.stats.0[ARCHER as usize];
            w.stats.0[MAGE as usize].flags |= flags::MAGE_LIKE;
            w.stats.0[PEASANT as usize].priority = w.stats.0[FOOTMAN as usize].priority + 10;
            let m = w.spawn(MAGE, 0, (10, 10), 0).unwrap();
            let a = w.spawn(FOOTMAN, 1, (11, 10), 0).unwrap();
            let p = worker.then(|| w.spawn(PEASANT, 1, (9, 10), 0).unwrap());
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let i = at(&w, m);
            let u = &mut w.units[i];
            (u.order, u.pending, u.attacked_by) = (Order::Idle, None, Some(a));
            u.script.wait = 1;
            assert!(w.run_order(i));
            let o = w.units[i].order;
            match p {
                Some(p) => assert_eq!(o, Order::Chase { target: p }),
                None => assert!(matches!(o, Order::Move { .. }), "{o:?}"),
            }
        }
    }
}
