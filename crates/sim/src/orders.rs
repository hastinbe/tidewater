//! Commands, immediate orders, target acquisition, reactions and fleeing. Spec: `docs/specs/orders/orders.md`.
//!
//! Transports and boarding are in `transport.rs`, attacking the ground (17, 18) and demolition (19 to 21) in
//! `ground.rs`. Not yet here: spells, the computer players' worker and tanker routines (they skip the search, as
//! the spec says, and do nothing instead), and alliances outside target acquisition (there only the owner's entry of
//! the alliance table is read).
//!
//! The code follows the spec's model ("The model"): each shared rule is stated once and the orders call it.
//!
//! | Spec | Here |
//! | --- | --- |
//! | "How an update runs" | [`World::order`] (steps 1 and 5), `World::order_logic` (steps 2 to 4) |
//! | "What every command does" | [`World::command`], [`World::command_steps`], [`World::command_at_once`] |
//! | "Pending orders and immediate orders" | [`World::idle_now`], [`World::idle_at_once`], [`World::immediately`] |
//! | "Resuming patrol and attack-move" | [`World::resumed`] |
//! | "Walking orders", "No route" | [`World::walk`] (the plain rule), [`World::move_toward`] (the orders with their own) |
//! | "Arriving and losing the target" | [`World::target_lost`], [`World::lose_target`], `World::move_order`, `World::arrive` |
//! | "Attack orders" | [`World::run_order`], [`World::attack_order`], `World::attack_in_place` |
//! | "Order 12 with no route" | `World::chase_no_route`, [`World::search_in_range`] |
//! | "Follow (orders 6 and 7)" | `World::follow` |
//! | "Automatic target acquisition" | `World::acquire`, `World::search`, `World::resulting` |
//! | "Reacting to an attacker", "Fleeing" | [`World::react`], [`World::flee`] |
//!
//! Where the code keeps an outcome that the spec words differently, the comment there says so and a `pin` test holds
//! it; those are questions for the spec, not rules.
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

/// Orders 12, 15 and 16: the attack orders on a target the unit's own search found. Their run makes the target search
/// before the lost-target check ("Arriving and losing the target"), and the friendly-fire
/// check applies to them ("Friendly-fire check (orders 12, 15 and 16)").
fn found_target(o: Order) -> bool {
    matches!(
        o,
        Order::Chase { .. } | Order::AttackInPlace { .. } | Order::IdleReact { .. }
    )
}

/// "Attack orders", step 2 for orders 9, 11 and 17: instead of a target search, a dying attacker is forgotten.
fn forgets_dying_attacker(o: Order) -> bool {
    matches!(
        o,
        Order::Attack { .. } | Order::AttackWall { .. } | Order::AttackGround { .. }
    )
}

/// "Order 12 with no route", step 3: the orders whose run the skip mark gates, idle and the attack
/// orders 2, 9, 11, 12, 13, 15, 16 and 17.
fn skip_mark_gates(o: Order) -> bool {
    matches!(o, Order::Idle | Order::StandGround) || forgets_dying_attacker(o) || found_target(o)
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

/// The movement-class flags: the five lowest type flag bits (orders spec, "Forgetting a dying attacker first"
/// and "Terms"). A type with any of them is "a unit that can move".
pub(crate) const MOVEMENT_CLASS: u32 = 0x1F;

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

    pub(crate) fn flags_of_unit(&self, u: &Unit) -> u32 {
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
        // Transports spec, "What the transport stores" and "Land and shore targets": every command given to a
        // transport records its target, tile and destination and runs them ([`World::command_goal`]). The smart move
        // and unload do so in their own rules ([`World::transport_command`]).
        let goal = match *c {
            Command::Move { to, .. } | Command::AttackMove { to, .. } | Command::Patrol { to, .. } => {
                self.command_goal(i, None, to).0
            }
            _ => (0, 0),
        };
        let u = &self.units[i];
        let enhanced = self.rules.enhanced;
        let remember = match *c {
            Command::Patrol { .. } => Some(Remembered::Patrol { to: goal, back: u.tile }),
            Command::AttackMove { .. } => Some(Remembered::AttackMove { goal }),
            _ => None,
        };
        // "Commands forget a stale attacker": a target unit given with the command gets the same check
        // as the attacker, so one that is dying, dead, removed or not detected is dropped (static study only).
        let owner = u.owner;
        let fresh = |t: Option<UnitId>| t.filter(|&t| !self.target_lost(owner, t));
        let order = match *c {
            Command::Move { .. } => Order::Move { goal, target: None },
            Command::Stop { .. } => Order::Idle,
            // A target unit of any owner (own, allied and neutral units included) gives order 9. A
            // dying target is still given; `command` then finds it stale and leaves a pending idle
            // (`orc02-dyingcmd-3760` grunts 1572 and 1575, n = 3723).
            // "Checks on an attack command": the command checks only the commanded unit (not the
            // target itself, a type that can attack); what kind it can hit is the order's own check.
            Command::Attack { target, .. } => {
                if target == u.id || self.flags_of_unit(u) & flags::CAN_ATTACK == 0 {
                    return None;
                }
                Order::Attack { target }
            }
            Command::AttackMove { .. } => Order::AttackMove { goal },
            Command::Patrol { .. } => Order::Patrol { to: goal, back: u.tile },
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
            | Command::CancelJob { .. }
            | Command::Cast { .. }
            | Command::Build { .. }
            | Command::Select { .. }
            | Command::RightClick { .. }
            | Command::Issue { .. }
            | Command::SetGroup { .. }
            | Command::RecallGroup { .. }
            | Command::Speed { .. }
            | Command::Pause { .. }
            | Command::Cheat { .. }
            | Command::Surrender { .. }
            | Command::QuitDraw { .. }
            | Command::Leave { .. }
            | Command::UnloadOne { .. } => return None,
        };
        // Our own choice (module doc): every other command clears the remembered order.
        match remember {
            Some(r) if enhanced => self.remember(i, r),
            _ => self.units[i].remembered = None,
        }
        Some(order)
    }

    /// "Resuming patrol and attack-move" (Enhanced only): a patrol or attack-move is remembered and the resume countdown
    /// set to 0 (static study only for the countdown). "An unremembered patrol" leaves the old remembered order.
    fn remember(&mut self, i: usize, r: Remembered) {
        if !self.rules.enhanced {
            return;
        }
        if let Remembered::Patrol { to, .. } = r
            && self.unremembered_patrol(i, to)
        {
            return;
        }
        self.units[i].remembered = Some(r);
        self.units[i].resume = 0;
    }

    /// Orders spec, "An unremembered patrol" (Enhanced; round 71, `orc09-sea-patrol-axis-2000`, V274): a patrol
    /// command given to an air or sea unit whose goal (as given) is within 1 tile of the unit on **either** axis (its x
    /// distance or its y distance at most 1) doesn't write the remembered order.
    pub(crate) fn unremembered_patrol(&self, i: usize, to: (i32, i32)) -> bool {
        let u = &self.units[i];
        let near = (u.tile.0 - to.0).abs() <= 1 || (u.tile.1 - to.1).abs() <= 1;
        self.flags_of_unit(u) & (flags::AIR | flags::SEA) != 0 && near
    }

    /// Command 3 ("Smart move"). Transports and boarding come with the transports spec, so for now they move.
    fn smart(&mut self, i: usize, target: Option<UnitId>, tile: (i32, i32)) -> Option<Order> {
        self.cap_smart_wait(i);
        let u = &self.units[i];
        // The command carries its target unit, if any ("Commands"): a unit standing on the target tile isn't looked
        // up, so a critter wandering onto another unit's tile gets order 3 (Human04 trace, n = 82).
        let target = target.filter(|&t| t != u.id);
        // Step 2: with no target unit, the building on the goal tile becomes the target when it
        // passes the harvest test; otherwise there is none.
        let target = target.or_else(|| self.smart_building(u, tile));
        if self.is_transport(u) {
            return Some(self.transport_command(i, target, tile, false));
        }
        if let Some(t) = target
            && self.boards(u, t)
        {
            return Some(self.board_command(i, t));
        }
        let u = &self.units[i];
        // Return goods by smart move is for workers only; a tanker carrying oil sent onto a refinery
        // or shipyard goes on down the table (traced for its own shipyard, `orc9-orc09-tanker-move-2200`).
        if let Some(t) = target
            && self.is_worker(u)
            && self.can_return_to(u, t)
        {
            return Some(Order::ReturnGoods { depot: Some(t) });
        }
        // The harvest row is the harvest test of step 2, never a tree or an oil patch.
        if let Some(t) = target
            && self.unit(t).is_some_and(|t| self.harvest_test(u, t))
        {
            return self.harvest_command(i, Some(t), tile);
        }
        let own_unit = target
            .and_then(|t| self.unit(t))
            .filter(|t| t.owner == u.owner && !t.building && t.present());
        // Orders spec ("Smart move"; `orc9-orc09-tanker-move-2200` tanker 1473, n = 1265 and 1700): a target unit
        // that falls through to the move stays its target, and the stored goal is the target's tile (not pulled back,
        // `World::command`). Traced for a tanker; static study for land units.
        let moving_to = target.and_then(|t| self.unit(t)).map(|t| (t.id, t.tile));
        Some(match (own_unit, moving_to) {
            (Some(t), _) => Order::Follow { target: t.id },
            (None, Some((t, at))) => Order::Move {
                goal: at,
                target: Some(t),
            },
            (None, None) => Order::Move {
                goal: tile,
                target: None,
            },
        })
    }

    /// "Smart move" step 2: with no target unit, the building on the goal tile becomes the target
    /// when it passes the harvest test ([`World::harvest_test`]). Anything else gives no target.
    pub(crate) fn smart_building(&self, u: &Unit, tile: (i32, i32)) -> Option<UnitId> {
        let at = self.occupant(Layer::Land, tile.0, tile.1).and_then(|t| self.unit(t))?;
        (at.building && self.harvest_test(u, at)).then_some(at.id)
    }

    /// The smart move's harvest test ("Smart move" step 2; `orc9-orc09-smart-move-2000` vectors): a
    /// worker carrying nothing on a gold mine, or a tanker not carrying oil on an oil platform of its own player. It is
    /// on the unit type and what it carries only: what the worker last harvested (its gather mode) plays no part.
    pub(crate) fn harvest_test(&self, u: &Unit, t: &Unit) -> bool {
        if u.carrying {
            return false;
        }
        let (f, tf) = (self.flags_of_unit(u), self.flags_of_unit(t));
        if f & flags::PEON != 0 {
            tf & flags::MINE != 0
        } else if f & flags::TANKER != 0 {
            tf & flags::OIL_SOURCE != 0 && t.owner == u.owner
        } else {
            false
        }
    }

    /// The smart move's wait cap ("Commands"): a waiting unit's wait is cut to 40 cycles for a computer player's
    /// unit and 10 for every other player's.
    pub(crate) fn cap_smart_wait(&mut self, i: usize) {
        let cap = if self.computer_player(self.units[i].owner) {
            SMART_WAIT_COMPUTER
        } else {
            SMART_WAIT_OTHER
        };
        let u = &mut self.units[i];
        u.script.wait = u.script.wait.min(cap);
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
        self.units[i].order = self.idle_or_docked(i);
    }

    /// Idle for a unit about to go idle (an immediate idle, a pending idle taking over, an idle transport's step
    /// done; orders spec, "Taking over" and "Pending orders and immediate orders"): a transport settles on an even
    /// tile that isn't shore water, back in the sea class as a two-tile mover, and otherwise docks (order 32 instead
    /// of 2). Every other unit stays idle.
    pub(crate) fn idle_or_docked(&mut self, i: usize) -> Order {
        let u = &self.units[i];
        if !self.is_transport(u) {
            return Order::Idle;
        }
        let t = u.tile;
        let settles = t.0 & 1 == 0 && t.1 & 1 == 0 && self.terrain.word(t.0, t.1) & crate::terrain::bits::SHORE == 0;
        if !settles {
            return Order::Docked;
        }
        let u = &mut self.units[i];
        u.ship.landing = false;
        u.ship.one_tile = false;
        Order::Idle
    }

    /// "Resuming patrol and attack-move" (Enhanced only): when idle is about to become the unit's pending order, the
    /// remembered order to give instead, with the countdown set. `None` keeps the idle. An immediate idle never comes
    /// here.
    ///
    /// With the countdown at 0, a remembered attack-move that is complete (within 1 tile on both
    /// axes) is forgotten here, wherever idle becomes pending (traced: `orc9-orc09-attack-move-1500` ogre 1493). With
    /// the countdown running nothing is resumed or forgotten (static study only).
    ///
    /// The resumed order is a full command. Its goal is pulled back (pathfinding spec, "Goal pulled
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
            Some(Remembered::AttackMove { .. }) => {
                self.units[i].remembered = None;
                return None;
            }
            _ => return None,
        };
        self.command_steps(i);
        Some(o)
    }

    /// The common pending idle (orders spec, "Giving up when blocked" and "Resuming patrol and attack-move"): idle
    /// becomes the pending order, or, when a remembered order resumes, the unit starts its still animation with wait 1
    /// and the remembered order is the pending order, given as a command. Returns whether it resumed. The caller makes
    /// sure the unit can take a pending order.
    pub(crate) fn pending_idle(&mut self, i: usize) -> bool {
        match self.resumed(i) {
            Some(o) => {
                self.start_anim(i, anim::STILL);
                let u = &mut self.units[i];
                u.script.wait = 1;
                u.pending = Some(o);
                u.commanded = true;
                true
            }
            None => {
                let u = &mut self.units[i];
                u.pending = Some(Order::Idle);
                u.commanded = false;
                false
            }
        }
    }

    /// A pending idle that takes over in the same update (arriving, losing the target), or the resumed order. Like any
    /// pending order it replaces one already pending, so nothing is left pending after the take-over.
    pub(crate) fn idle_now(&mut self, i: usize) {
        self.pending_idle(i);
        if let Some(o) = self.units[i].pending.take() {
            self.take_over(i, o);
        }
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

    /// Leaving the chopping state any way but felling (occupancy spec, "Reservation"): the reserved tree is given
    /// back and the return-to source forgotten (audit round 70, erratum 7). Felling leaves the chopping state first.
    pub(crate) fn release_tree(&mut self, i: usize) {
        if let Order::Harvest {
            source: None,
            tree,
            chopping: true,
        } = self.units[i].order
        {
            self.terrain.unreserve(tree.0, tree.1);
            self.units[i].return_to = None;
        }
    }

    /// The lost-target test ("Arriving and losing the target"; "Terms"): a target unit is lost when it is dying, gone
    /// (dead, removed, or a released slot), inside something, or no longer detected by `owner`. Fog doesn't matter.
    /// One test for the walk's step-boundary drop (V240), the command check of "Commands forget a stale attacker"
    /// (V185, V245) and the follow and attack orders aimed at a unit; the same check on a recorded attacker. The
    /// harvest orders' own target checks differ (harvest spec).
    pub(crate) fn target_lost(&self, owner: u8, t: UnitId) -> bool {
        self.unit(t).is_none_or(|t| !t.present() || !self.detected(t, owner))
    }

    /// "Arriving and losing the target": a pending order takes over now; otherwise a new pending order is set, 13 for
    /// order 15 and 2 for the rest (with the Enhanced resume), and takes over in the same update. Every case is a
    /// take-over as at a step done (still animation for 2 and 13, wait 3, the target forgotten with the old order):
    /// `human01-basics`, n = 291, footman 1598 losing its dead target gets order 2 with wait 3;
    /// `human01-order15-lost-700`, n = 469, footman 1593 in order 15 gets order 13 with wait 3 (V273).
    pub(crate) fn lose_target(&mut self, i: usize) {
        if self.units[i].pending.is_none() {
            if matches!(self.units[i].order, Order::AttackInPlace { .. }) {
                self.give(i, Order::StandGround);
            } else {
                // "Resuming patrol and attack-move": losing the target is one of the places idle becomes pending.
                self.pending_idle(i);
            }
        }
        if let Some(o) = self.units[i].pending.take() {
            self.take_over(i, o);
        }
    }

    /// The unit's attacker if still good; a lost one is forgotten. Movement spec ("Blocked rules";
    /// "Resulting order"): a dying attacker isn't good, but its record is kept while it is dying, where it still
    /// counts as an attacker for the resulting order. It is kept until it leaves play (its slot is
    /// released); the end of its death animation isn't a rule (movement spec, open questions). Orders spec ("Terms",
    /// `xhuman7-10000` troll destroyer 1420): an attacker in play that the owner doesn't detect is ignored in this run
    /// and stays recorded; only a command forgets it. Orders spec ("A recorded attacker outlives its slot"): a
    /// released slot reads as a dying attacker, so its record is kept here too.
    fn attacker(&mut self, i: usize) -> Option<UnitId> {
        let u = &self.units[i];
        let a = u.attacked_by?;
        if !self.target_lost(u.owner, a) {
            return Some(a);
        }
        if self.unit(a).is_some_and(|a| !a.dying && a.inside) {
            self.units[i].attacked_by = None;
        }
        None
    }

    /// Whether a recorded attacker is dying or no longer in play; a released slot reads as both ("A recorded attacker
    /// outlives its slot").
    pub(crate) fn dying_or_gone(&self, a: UnitId) -> bool {
        self.unit(a).is_none_or(|a| a.dying)
    }

    /// "Forgetting a dying attacker first": every target search of a unit that can move starts by
    /// forgetting a recorded attacker that is dying or no longer in play (`xorc6-10000` axe thrower 1475, n = 6106).
    /// A unit can move if its type has a movement-class flag (the five lowest type flag bits).
    pub(crate) fn forget_dying_attacker(&mut self, i: usize) {
        let u = &self.units[i];
        if self.flags_of_unit(u) & MOVEMENT_CLASS != 0
            && let Some(a) = u.attacked_by
            && self.dying_or_gone(a)
        {
            self.units[i].attacked_by = None;
        }
    }

    /// Whether `u` can target a recorded attacker's kind ("Reacting to an attacker", step 1). A released slot is
    /// judged by the last type it held ("A recorded attacker outlives its slot").
    fn can_target_recorded(&self, u: &Unit, a: UnitId) -> bool {
        if let Some(t) = self.unit(a) {
            return self.can_target_kind(u, t);
        }
        let Some(kind) = self.last_slot(a).and_then(|s| self.stale.get(&s)).map(|s| s.kind) else {
            return true;
        };
        self.can_target_type(u, self.stats_of(kind).flags, self.layer_of(kind))
    }

    /// Orders spec, "Commands forget a stale attacker": every command given to a unit that isn't a
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

    /// A recorded attacker that fails the lost-target test ([`World::target_lost`]: dying, gone, inside something or
    /// not detected) is forgotten: pushing off's step 3 (transports spec) and order 25's target check (harvest spec,
    /// "Entering", step 3).
    pub(crate) fn forget_lost_attacker(&mut self, i: usize) {
        let owner = self.units[i].owner;
        if let Some(a) = self.units[i].attacked_by
            && self.target_lost(owner, a)
        {
            self.units[i].attacked_by = None;
        }
    }

    /// Whether a recorded attacker is stale ("Commands forget a stale attacker"): dying, dead, removed or not detected
    /// by `owner`. Unlike [`World::target_lost`], an attacker inside something isn't stale.
    pub(crate) fn stale(&self, owner: u8, t: UnitId) -> bool {
        self.unit(t).is_none_or(|t| t.dying || !self.detected(t, owner))
    }

    /// Orders spec, "Order 12 with no route" step 3 (`xorc8-10000` elven destroyer 1468, n = 7372):
    /// a unit in order 12 out of reach of a target it still holds takes a move step instead of running its order
    /// logic, and the move step's target search runs whatever the skip mark.
    pub(crate) fn out_of_reach(&self, i: usize) -> bool {
        let u = &self.units[i];
        let Order::Chase { target } = u.order else { return false };
        if self.target_lost(u.owner, target) {
            return false;
        }
        let t = self.unit(target).expect("held");
        !self.within_strike_reach(u, t)
    }

    /// The per-step order logic for the orders this module owns. Returns false for orders it doesn't handle.
    pub(crate) fn run_order(&mut self, i: usize) -> bool {
        let o = self.units[i].order;
        // Orders spec, "A dying attacker in orders 9, 11 and 17" (V136, V137): their logic first forgets a recorded
        // attacker that is dying or no longer in play, before the target check.
        if forgets_dying_attacker(o)
            && let Some(a) = self.units[i].attacked_by
            && self.dying_or_gone(a)
        {
            self.units[i].attacked_by = None;
        }
        // "Taking over" (round 71, static study): every step done of a transport idle with nothing pending makes the
        // settle-or-dock check before its idle logic. Our reading for docking there (no known play reaches it): order
        // 32 is set and the still animation runs on with its wait; the run ends, as order 32 has no logic.
        if o == Order::Idle && self.idle_or_docked(i) == Order::Docked {
            self.units[i].order = Order::Docked;
            return true;
        }
        // "Idle look-around": the first thing each run of the idle logic (orders 2 and 13) does. A critter that
        // wanders is in its move now, so the rest of the idle logic doesn't run on the old order.
        if matches!(o, Order::Idle | Order::StandGround) && self.look_around(i) {
            return true;
        }
        // Damage spec, "Applying damage": a killer whose order is still the one whose target it forgot has no target,
        // and its next logic run loses it as any other attacker does. That spec leaves this run open; in
        // `human01-basics` footman 1593 kills at n = 575 and its next run, at n = 591, gives order 2 with wait 3.
        let forgot = self.units[i].forgot.take() == Some(o);
        let lost = target_of(o).is_some_and(|t| forgot || self.target_lost(self.units[i].owner, t));
        // Order 12 runs its target search first, and only if it finds nothing new is the target lost.
        // So do orders 15 and 16, so a dying target stays the target until then.
        let search_first = found_target(o);
        // Orders spec ("Commands", command 4): order 4 with a target unit ends at its run once that target is dying
        // or lost, as on arrival: idle (or the resumed order) takes over in the same update (`xhuman7-10000` giant
        // turtle 1511, n = 271).
        // Orders spec ("Smart move"; `orc9-orc09-smart-target-2100` peon 1478, n = 1877): a smart move (order 3)
        // that keeps a target unit ends the same way, at its first step boundary where the target is dying, gone,
        // inside something or undetected; it doesn't walk on to the stored tile.
        let move_lost = matches!(
            o,
            Order::MoveAttack { target: Some(t), .. } | Order::Move { target: Some(t), .. }
                if self.target_lost(self.units[i].owner, t)
        );
        // Orders spec ("Follow (orders 6 and 7)"; static study only): order 6 whose target is found lost at a step
        // boundary isn't walking, so its reaction runs first; when the reaction's search picks a unit, the follow
        // steps run on it, and only otherwise is the target lost.
        if lost && let Order::Follow { target } = o {
            let picked = self.follow_reaction(i, target);
            if picked != target {
                self.follow_steps(i, picked);
                return true;
            }
        }
        if (lost && !search_first) || move_lost {
            // Static study only: command 4's end forgets the attacker, as arrival does (and so does order
            // 3's).
            if move_lost {
                self.units[i].attacked_by = None;
            }
            self.lose_target(i);
            return true;
        }
        // "Order 12 with no route": a unit marked by that fallback skips the target search (and with
        // it the acquisition) in its next order-logic run. If that run's order is idle or an attack
        // order (2, 9, 11, 12, 13, 15, 16 or 17); `World::order` clears the mark at the run's end.
        let skip = self.units[i].skip_search && skip_mark_gates(o) && !self.out_of_reach(i);
        // "Arriving and losing the target" (`xorc11-10000` gryphon rider 1400, n = 6046): in orders 4
        // and 10 the arrival test comes before the target search, so a unit standing at its move goal arrives below
        // with no acquisition in that run. Patrol (order 5) does the same at a patrol end: the turnaround comes first
        // and the search runs at the next logic run (V281, `xorc10-xorc10-losses-4000` destroyer 1484, n = 1021).
        let arriving = match o {
            Order::MoveAttack { goal, .. } | Order::AttackMove { goal } => self.at_goal(i, Goal::tile(goal, 0)),
            Order::Patrol { to, .. } => self.at_goal(i, Goal::tile(to, 0)),
            _ => false,
        };
        if !skip
            && !arriving
            && let Some(kind) = self.acquires(o)
            && self.acquire(i, kind, forgot)
        {
            return true;
        }
        // `ai/worker-ai.md`, "When a job is chosen": a computer worker's or tanker's job choice runs where a combat unit
        // would run its target search, under the same gates (not at an order-4, 5 or 10 goal; the skip mark). Traced
        // in order 4 (round 83, CP5; tankers round 84, CP13) and order 12 (round 84, CP12); a static gap-fill in the
        // other acquiring orders (5, Enhanced 10, 13 to 16; order 7 in its arm below). In a walking order that point
        // is the walk's step 1, before its arrival test and step ("Walking orders"); in order 12 within reach, the
        // start of its logic (static study). Its command is pending and takes over at the next step done. Order 2
        // makes the choice in its idle arm, where the command takes over at once. With no job a worker has its wait
        // and keeps its order; a step that starts the move animation then overwrites that wait (n = 2505 of
        // `xhuman5-xhuman05-worker-order12-goals-3000`), while one already in it leaves it (n = 2527: 59).
        let job = !skip && !arriving && o != Order::Idle && self.acquires(o).is_some() && self.moving_choice(i);
        // Our reading (static study): a target found lost after the choice is lost as usual, and the job's pending
        // command is what takes over then ("Arriving and losing the target": a pending order takes over now).
        if job && lost {
            self.lose_target(i);
            return true;
        }
        // In an order that doesn't walk, or order 12 within reach, the run ends with the job pending (our reading,
        // static study: the order's own part isn't run on the old target).
        if job && !self.job_walks(i, o) {
            return true;
        }
        if lost {
            // "Losing the target while cooling down": in orders 12, 15 and 16 the cooldown test comes
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
                // `ai/computer-player.md`, "Where the job choice runs": a computer worker's or tanker's job choice runs
                // from its idle logic where a combat unit would search for a target.
                if o == Order::Idle {
                    self.idle_choice(i);
                }
            }
            // The walk after a job choice heads for the job ([`World::job_walk`]).
            _ if job => self.job_walk(i),
            Order::Move { .. } | Order::MoveAttack { .. } | Order::AttackMove { .. } => self.move_order(i, o, arriving),
            Order::Patrol { to, back } => match self.walk(i, Goal::tile(to, 0)) {
                Walk::Arrived => {
                    // "Arriving and losing the target", order 5: a patrol command to the other end, with the tile it
                    // stands on as the new other end, which remembers like every patrol command ("What a patrol
                    // remembers") and takes over in the same update (still animation, wait 3).
                    let here = self.units[i].tile;
                    self.remember(i, Remembered::Patrol { to: back, back: here });
                    self.command_at_once(i, Order::Patrol { to: back, back: here });
                }
                // "No route" (V101 to V103): the plain rule.
                Walk::NoPath | Walk::Stepping | Walk::Blocked => {}
            },
            Order::Follow { target } | Order::Guard { target } => {
                // Order 6 reacts to an attacker only when its walk has ended (`follow`).
                // "Follow": order 7 then runs the idle logic (look-around, then acquisition as for order 2), unless
                // its follow steps gave it a new order.
                // `ai/worker-ai.md`, "When a job is chosen" (round 84, static gap-fill): a computer worker or tanker
                // guarding makes its job choice at that point instead of the search; its command is pending.
                if self.follow(i, target) && !self.look_around(i) && !self.acquire(i, Acquires::Idle, false) {
                    self.moving_choice(i);
                }
            }
            Order::Attack { target } | Order::Chase { target } => self.attack_order(i, target),
            Order::AttackWall { tile } => self.attack_wall_order(i, tile),
            Order::AttackInPlace { target } | Order::IdleReact { target } => self.attack_in_place(i, target),
            Order::AttackGround { tile } => self.attack_ground(i, tile),
            Order::AttackGroundMove { tile, goal } => self.attack_ground_move(i, tile, goal),
            Order::Demolish { .. } | Order::DemolishAt { .. } => self.demolish(i, o),
            _ => return false,
        }
        true
    }

    /// The moves, orders 3, 4 and 10 ("Walking orders"; "Arriving and losing the target").
    ///
    /// Static study only: in Classic mode order 10 reacts in its walk ("Walking orders", step 1), and at
    /// the move goal the whole move step is skipped, so the reaction doesn't run there either.
    ///
    /// A walk whose order holds a target unit heads for the target where it stands now ("Walking orders";
    /// pathfinding spec, "Step 1" and "G for a walk with a target unit"; V256): the route goal of the building goal rule
    /// on the target's footprint, from the unit's tile, with the target's goal zone. Order 4 also stores that goal
    /// (`orc5-40000` n = 33260); order 3 keeps the target's tile its command stored. A lost target
    /// ended the order before the walk (`run_order`).
    fn move_order(&mut self, i: usize, o: Order, arriving: bool) {
        if matches!(o, Order::AttackMove { .. }) && !self.rules.enhanced && !arriving && self.react(i) {
            return;
        }
        let (goal, target) = match o {
            Order::Move { goal, target } | Order::MoveAttack { goal, target } => (goal, target),
            Order::AttackMove { goal } => (goal, None),
            _ => unreachable!("not a move"),
        };
        let g = match target {
            Some(t) => {
                let t = self.unit(t).expect("held");
                let g = self.target_walk(&self.units[i], t, 0);
                if let Order::MoveAttack { goal, .. } = &mut self.units[i].order {
                    *goal = g.tile;
                }
                g
            }
            None => Goal::tile(goal, 0),
        };
        // No route: the plain rule (V170: the unit keeps its attacker, and its idle logic may react to it).
        if self.walk(i, g) == Walk::Arrived {
            self.arrive(i);
        }
    }

    /// Whether a unit in order `o` that has just made its job choice is walking, so its walk goes on after the choice:
    /// orders 4, 5 and 10, and order 12 out of strike reach of its target ("How an update runs", step 2).
    fn job_walks(&self, i: usize, o: Order) -> bool {
        match o {
            Order::MoveAttack { .. } | Order::AttackMove { .. } | Order::Patrol { .. } => true,
            Order::Chase { target } => self
                .unit(target)
                .is_some_and(|t| !self.within_strike_reach(&self.units[i], t)),
            _ => false,
        }
    }

    /// The walk after a job choice in a walking order (`ai/worker-ai.md`, "When a job is chosen"; `orders/orders.md`,
    /// "Walking orders", steps 3 and 4; round 84). The choice is the walk's step 1, and its command has stored the
    /// job's target and goal ("What every command does", step 1) while the order stays until the next step done; so
    /// the walk's arrival test and step use the job's: the walk heads for the job's target where it stands, by the
    /// order's own radius (0 for orders 4, 5 and 10; the attack range for order 12). Traced in order 4:
    /// `xhuman5-xhuman05-worker-all-out-goals-3000` peon 1575 steps toward gold mine 1573 at n = 2410, the harvest
    /// pending. On arrival the walk ends and the pending job takes over in that same update in place of the order's
    /// idle (AI11, traced at n = 2464: order 23, animation 4, wait 3); as on order 4's arrival the attacker is
    /// forgotten (orders 5 and 12 too: our reading, untraced).
    fn job_walk(&mut self, i: usize) {
        let Some(job) = self.units[i].pending else { return };
        let u = &self.units[i];
        let chase = matches!(u.order, Order::Chase { .. });
        let target = job.target().and_then(|t| self.unit(t));
        let g = match target {
            Some(t) if chase => self.strike_goal(u, t),
            Some(t) => self.target_walk(u, t, 0),
            None => {
                let tile = match job {
                    Order::Build { kind, site, goal } => goal.unwrap_or_else(|| self.build_goal(i, kind, site)),
                    Order::Harvest { tree, .. } => tree,
                    // A command that failed its target check left idle pending: the order's own walk goes on.
                    _ => return self.walk_on(i),
                };
                let reach = if chase { self.reach(u) } else { 0 };
                Goal {
                    range: chase,
                    ..Goal::tile(tile, reach)
                }
            }
        };
        if self.walk(i, g) == Walk::Arrived {
            self.units[i].attacked_by = None;
            if let Some(o) = self.units[i].pending.take() {
                self.take_over(i, o);
            }
        }
    }

    /// The order's own walk, for a job choice whose command left idle pending (its target failed the check).
    fn walk_on(&mut self, i: usize) {
        match self.units[i].order {
            o @ (Order::MoveAttack { .. } | Order::AttackMove { .. }) => self.move_order(i, o, false),
            Order::Patrol { to, .. } => {
                self.walk(i, Goal::tile(to, 0));
            }
            Order::Chase { target } => self.attack_order(i, target),
            _ => {}
        }
    }

    /// "Arriving and losing the target", orders 3, 4 and 10 on arrival: the unit forgets its attacker, and idle becomes
    /// its pending order (with the Enhanced resume) and takes over in the same update (still animation, wait 3); the idle
    /// logic first runs at the next step done.
    fn arrive(&mut self, i: usize) {
        self.units[i].attacked_by = None;
        self.idle_now(i);
    }

    /// Orders 6 and 7 ("Follow"). Returns whether a guarding unit (order 7) goes on to its idle logic: it is within
    /// the radius and its follow steps gave it no new order.
    ///
    /// Orders spec ("Follow (orders 6 and 7)", "Walking orders react only when the walk has ended";
    /// `orc9-orc09-follow-attacked-1200` grunt 1481, n = 314 to 560): while order 6 walks only the walk runs, and a
    /// route search that finds no route makes idle an immediate order whose idle logic runs ("No route"), with no
    /// reaction (static study only). Once it is in reach, the reaction runs first and the follow steps then run on
    /// the target the reaction left ([`World::follow_reaction`]).
    fn follow(&mut self, i: usize, target: UnitId) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let step_1 = matches!(self.units[i].pending, Some(Order::Follow { .. })) && !followable(t.order);
        if matches!(self.units[i].order, Order::Follow { .. }) && !step_1 {
            let g = self.follow_goal(t);
            if !self.at_goal(i, g) {
                self.walk(i, g);
                return false;
            }
            let target = self.follow_reaction(i, target);
            return self.follow_steps(i, target);
        }
        self.follow_steps(i, target)
    }

    /// The follow's walk toward `t`. Pathfinding spec, "Step 1" (static study): the walk's order holds
    /// a target unit, so its goal zone is the target's widened footprint. Traced (`orc9-orc09-tanker-move-2200`
    /// juggernaut 1501, n = 614): the radius is 1 plus the +1 for a sea or air follower, as in the general arrival
    /// test. Kept apart from the one walk goal ([`World::target_walk`]): the goal is the target's own tile, even
    /// for a building, not [`World::target_goal`] (our reading of "toward the followed unit"; untraced for a
    /// building).
    fn follow_goal(&self, t: &Unit) -> Goal {
        Goal {
            zone: Some(crate::battle::goal_zone(t)),
            ..Goal::tile(t.tile, 1)
        }
    }

    /// Order 6's reaction to an attacker, in a run where the follower isn't walking ("Follow (orders
    /// 6 and 7)"; `orc9-orc09-follow-attacked-1200` grunt 1481, n = 493): the arrival switch and the reaction run, but
    /// any order the reaction gives is replaced by what the follow steps give. The target it set stays: an attack
    /// command from the reaction's search (or a worker's fight-back) makes the picked unit the one followed. Returns
    /// the target the follow steps run on.
    ///
    /// Our reading (static study): a flee sets no target unit, so a fleeing follower keeps following its own target.
    fn follow_reaction(&mut self, i: usize, target: UnitId) -> UnitId {
        if !self.react_at_walk_end(i) {
            return target;
        }
        let u = &self.units[i];
        let picked = [Some(u.order), u.pending]
            .into_iter()
            .flatten()
            .find_map(|o| match o {
                Order::Chase { target } | Order::Attack { target } => Some(target),
                _ => None,
            })
            .unwrap_or(target);
        let u = &mut self.units[i];
        u.order = Order::Follow { target: picked };
        u.pending = None;
        u.commanded = false;
        picked
    }

    /// The follow steps ("Follow (orders 6 and 7)", steps 1 to 3) on `target`.
    fn follow_steps(&mut self, i: usize, target: UnitId) -> bool {
        let Some(t) = self.unit(target) else { return false };
        let (t_order, g) = (t.order, self.follow_goal(t));
        let u = &self.units[i];
        let follow = matches!(u.order, Order::Follow { .. });
        let following = matches!(u.order, Order::Guard { .. }) || matches!(u.pending, Some(Order::Follow { .. }));
        if following && !followable(t_order) {
            // Step 1 (round 71, static study; V276): the whole smart move to its own tile with no target unit, as for
            // the flee: its wait cap applies, a transport takes the transport outcome, and it is a command (blocked
            // counter cleared, cached route dropped, a stale attacker forgotten), left pending.
            let here = u.tile;
            let o = self.smart_move_to(i, here);
            self.command(i, o);
            return false;
        }
        if !self.at_goal(i, g) {
            if follow {
                // Step 2 after the reaction picked a unit out of reach: the follow is given again toward it and
                // takes over in the same update (order 6, still animation, wait 3; `orc9-orc09-follow-attacked-1200`
                // grunt 1481, n = 493). It is a command with the target check ("What every command does"; round 68,
                // code Q6), so a picked unit that fails the check (a worker's undetected attacker, say) leaves the
                // pending idle instead, taken over the same way.
                self.command_at_once(i, Order::Follow { target });
            } else {
                // "No route" (V262): guard has no special case.
                self.walk(i, g);
            }
            return false;
        }
        if follow {
            if followable(t_order) {
                // Orders spec (step 3; `orc9-orc09-follow-flee-1300` peon 1478, n = 605, and
                // `orc12-orc12-guard-board-full-900` ogre 1450, n = 220): the guard is made pending and takes over in
                // the same update (still animation, wait 3, target kept).
                self.units[i].path.clear();
                self.take_over(i, Order::Guard { target });
            } else {
                // Orders spec (same vector): idle is made the pending order, with the Enhanced resume, and takes
                // over in the same update (still animation, wait 3); not a stop command.
                self.idle_now(i);
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
        // "Friendly-fire check": before the range test.
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
            // Static study only: order 15 forgets the target and order 13 takes over in the same
            // update (still animation, wait 3), by the same path as order 16.
            Order::AttackInPlace { .. } => self.take_over(i, Order::StandGround),
            // Traced (`xhuman4` axe thrower 1437, n = 6): order 16 forgets the target and idle takes over in
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
        self.can_target_type(u, self.flags_of_unit(t), t.layer)
    }

    /// [`World::can_target_kind`] for a target type's flags `tf` in `layer`.
    fn can_target_type(&self, u: &Unit, tf: u32, layer: Layer) -> bool {
        let s = self.stats_of(u.kind);
        if s.flags & flags::SUBMARINE != 0 {
            return layer == Layer::Sea || tf & (flags::SHORE_BUILDING | flags::OIL_SOURCE) != 0;
        }
        let bit = match layer {
            Layer::Air => 4,
            Layer::Sea => 2,
            Layer::Land => 1,
        };
        s.can_target & bit != 0
    }

    /// "Checks on an attack command": the attack order's kind check, by the attacker type's
    /// can-target value, in this order. An air target needs air; a submarine hits only types flagged as sea units,
    /// oil platforms or shore buildings; a transport on a shore tile can be hit with land or sea; any other target in
    /// the sea layer needs sea; any other target needs land.
    pub(crate) fn can_hit_kind(&self, u: &Unit, t: &Unit) -> bool {
        let s = self.stats_of(u.kind);
        let tf = self.flags_of_unit(t);
        if t.layer == Layer::Air {
            return s.can_target & 4 != 0;
        }
        if s.flags & flags::SUBMARINE != 0 {
            return tf & (flags::SEA | flags::OIL_SOURCE | flags::SHORE_BUILDING) != 0;
        }
        if tf & flags::TRANSPORT != 0 && self.terrain.word(t.tile.0, t.tile.1) & crate::terrain::bits::SHORE != 0 {
            return s.can_target & 3 != 0;
        }
        if t.layer == Layer::Sea {
            return s.can_target & 2 != 0;
        }
        s.can_target & 1 != 0
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

    /// Whether `t` is inside the searcher's minimum range: its distance is less than the searcher's missile's minimum
    /// flight value minus 1 (`orc5-40000` n = 21489). Catapults and ballistas (3) skip d <= 1 only;
    /// cannons (2) and missiles with no value skip nothing, since d is at least 1.
    fn inside_min_range(&self, u: &Unit, t: &Unit) -> bool {
        combat::splash(self.stats_of(u.kind).missile).is_some_and(|min| distance(u.tile, t) < min as i32 - 1)
    }

    /// The acquisition kind check of the candidate skip list and the attacker seed: the attack command's kind check
    /// ([`World::can_hit_kind`]) in full, first match wins ("Checks on an attack command"; the
    /// transport-on-shore case is traced in acquisition, `orc12-30000` n = 26982).
    fn acquire_kind(&self, u: &Unit, t: &Unit) -> bool {
        self.can_hit_kind(u, t)
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
        if !self.detected(t, u.owner) || (!self.loading && !self.visible_to(u.owner, t.id)) {
            return false;
        }
        if !self.acquire_kind(u, t) {
            return false;
        }
        if u.layer == Layer::Air && t.layer == Layer::Air && tf & flags::CAN_ATTACK == 0 {
            return false;
        }
        if self.inside_min_range(u, t) {
            return false;
        }
        // Traced (`xorc11-10000` destroyer 1542, n = 38): attack-move (order 10) only; a sea unit on patrol
        // uses the ordinary skip list.
        if self.rules.enhanced
            && matches!(order, Order::AttackMove { .. })
            && u.layer == Layer::Sea
            && t.layer != Layer::Sea
            && tf & (flags::SHORE_BUILDING | flags::OIL_SOURCE) == 0
        {
            return false;
        }
        true
    }

    /// "Special cases": mages score every candidate like any other unit, and the rule is applied to
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

    /// "Order 12 with no route": the target search with the box of the unit's attack range (one
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
        // "Choice" (`human06-31357` ballista 1582 at n = 23683): the attacker seeds the search only
        // with a score above 0; one the unit can't attack (score 0: a kind it can't attack, or closer than its minimum
        // range) leaves the search as if there were no attacker. Traced (`human06-31357` n = 9642,
        // `xhuman11-10000` n = 9065): so does one the unit's owner doesn't detect or that the fog hides from it, the
        // test of the candidate skip list (left out while loading, as there).
        let seen = |a: &Unit| self.detected(a, u.owner) && (self.loading || self.visible_to(u.owner, a.id));
        let mut best: Option<(i32, UnitId)> = seed
            .and_then(|a| self.unit(a))
            .filter(|a| seen(a) && self.acquire_kind(u, a) && !self.inside_min_range(u, a))
            .map(|a| (self.score(u, a), a.id))
            .filter(|&(sc, _)| sc > 0);
        // "Vertical order": only dying, dead and removed entries are skipped at the front. Inside
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
    /// `attacked`: the unit recorded an attacker before the search ("Forgetting a dying attacker first").
    fn resulting(&self, i: usize, kind: Acquires, t: UnitId, attacked: bool) -> Order {
        let u = &self.units[i];
        match kind {
            Acquires::Moving => Order::Chase { target: t },
            Acquires::Other => Order::AttackInPlace { target: t },
            Acquires::Idle => {
                let chase = attacked
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
        let computer_worker = worker_like && self.computer_player(u.owner);
        // "Reacting to an attacker", step 1 comes first for every unit and every player, workers and
        // tankers included. A computer player's worker or tanker with no attacker acquires nothing here; it makes its
        // job choice instead (`ai/worker-ai.md`: in order 2 from the idle arm of `World::run_order`, in the other
        // acquiring orders right after this).
        if computer_worker && u.attacked_by.is_none() {
            return false;
        }
        let attacker = self.attacker(i);
        // "Reacting to an attacker", step 1 (`xorc11-10000` battleship 1539, n = 628): a recorded
        // attacker that is dying or no longer in play, and that the unit couldn't target at all, is forgotten. There is
        // no flee and no target search in that run, and the rest of the order's logic goes on. A
        // released slot by the last type it held.
        let u = &self.units[i];
        if attacker.is_none()
            && let Some(a) = u.attacked_by
            && self.dying_or_gone(a)
            && !self.can_target_recorded(u, a)
        {
            self.units[i].attacked_by = None;
            return false;
        }
        let u = &self.units[i];
        let mage = self.mage_like(u);
        // Step 1: the kind check alone, as in "Checks on an attack command", on the recorded attacker
        // while it is valid; no fog or detection test, so an attacker hidden from the seed by those alone goes on to
        // step 2 unseeded, and one whose kind the unit can't hit is fled from. Traced (`human12-25000`
        // transport 1428, n = 16061, and tanker 1425, n = 16612): an undetected attacker is fled from too, and a
        // computer player's worker or tanker that fails step 1 runs the flee rule and not its job choice.
        if let Some(at) = u.attacked_by.and_then(|a| self.unit(a)).filter(|a| a.present())
            && !self.can_hit_kind(u, at)
        {
            let fled = self.flee(i);
            return fled || computer_worker;
        }
        // "Special cases" (round 71, S1; static study): in Enhanced mode a demolition unit with a remembered order runs
        // the target search after step 1, and a unit found that isn't its current target is given command 19 on it
        // (order 20, demolish at the unit). Nothing else of the acquisition runs, the reaction and its flee included,
        // whatever the search finds. Our reading of "nothing else happens in that run": as for step 1's flee, the run
        // goes on with the rest of the order's logic, whether or not the search gave a command, since "nothing found
        // included" puts both cases alike (a patrol still walks; traced: V278, `orc11-orc11-sapper-attack-move-goals-1500`
        // sapper 1573 steps on at n = 426 with order 20 pending, which takes over at its next step done, n = 440).
        if self.rules.enhanced && DEMOLITION.contains(&u.kind) && u.remembered.is_some() {
            self.forget_dying_attacker(i);
            let current = target_of(self.units[i].order);
            let Some(best) = self.search(i, attacker, false).filter(|&b| Some(b) != current) else {
                return false;
            };
            let tile = self.unit(best).expect("found by the search").tile;
            let o = self.demolish_command(i, Some(best), tile);
            self.command(i, o);
            return false;
        }
        // "Special cases": workers and tankers of every player never run the search; the others'
        // only react to an attacker (`human06-31357` peasant 1587, n = 13557). Computer players' make their job choice
        // instead (`ai/worker-ai.md`, "When a job is chosen": orders 2, 4 and 12 traced, the others a static gap-fill).
        // Nor does an Enhanced demolition unit with nothing remembered (round 72, traced: V279,
        // `orc11-orc11-sapper-attack-move-goals-1500` sappers 1580 and 1575 in order 4): with an attacker it reacts
        // as the other non-combat units do, and with none it acquires nothing.
        // The reaction's non-combat flee has no detection test, so a non-combat unit
        // whose valid attacker its owner doesn't detect runs the reaction too (the "Terms" rule covers only the
        // target search and its seed).
        let valid = u.attacked_by.and_then(|a| self.unit(a)).is_some_and(|a| a.present());
        let non_combat = self.non_combat(u);
        let never_searches = worker_like || (self.rules.enhanced && DEMOLITION.contains(&u.kind));
        if computer_worker || (never_searches && attacker.is_none() && !valid) {
            return false;
        }
        if attacker.is_some() || (non_combat && valid) {
            if non_combat {
                return self.react(i);
            }
            // "Reacting to an attacker", step 3: an invisible mage-type unit does nothing more in
            // that run (static study only).
            if mage && u.invisibility > 0 {
                return false;
            }
        }
        if self.flags_of_unit(u) & flags::CAN_ATTACK == 0 {
            return false;
        }
        // Step 3: a mage-type unit with an attacker that finds no target it may take (or only its
        // current one) runs the reaction instead, which treats it as a non-combat unit, so it flees
        // (`xorc01-death-coil` death knight 1491, n = 1431).
        // "Forgetting a dying attacker first": the resulting order is picked from the record as it was
        // before the search.
        let attacked = u.attacked_by.is_some();
        self.forget_dying_attacker(i);
        let u = &self.units[i];
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
        let o = self.resulting(i, kind, best, attacked);
        let t = self.unit(best).expect("found by the search");
        // "Attacking without moving" (`xhuman2` footman 1548 at n = 34): order 16 doesn't switch to a
        // new target out of range; the target is forgotten and idle takes over in the same update, attacker kept.
        // Static study only: while the unit is still cooling down, the cooling-down rule wins and the new
        // target is taken with no range test.
        if matches!(u.order, Order::IdleReact { .. }) && !self.in_range(u, t) && !self.still_cooling(u) {
            self.idle_now(i);
            return true;
        }
        let reach = self.within_strike_reach(u, t);
        // "Automatic target acquisition": in order 12 a new target out of reach, with the old target
        // still held, is given as a command. The target changes at once, the route is dropped and the blocked counter
        // cleared; order 12 takes over at the next step done (`orc02` footman 1582, n = 570 to 586; `xhuman4`
        // footman 1497, n = 66). Our reading: the order's logic carries on in this update with the new target, as
        // the traces show (the unit keeps heading for it).
        // The same holds for an old target that is dying, unless this unit killed it (and so forgot
        // it); but a unit within strike reach of the old target takes the new order over at once, without a step
        // (`xorc01-death-knight` ogre 1490, n = 845; `orc02-assault` footman 1582, n = 800). "Resulting order"
        // (`human5-10000` axe thrower 1533, n = 2754): whenever the old target is still recorded, whatever its state
        // (dying, inside a building or otherwise not detected).
        // Traced (`xhuman12-10000` grunt 1476, n = 1847): a unit still cooling down is handled as in reach,
        // below, whichever out-of-reach case would otherwise apply.
        if let (Order::Chase { target: old }, Order::Chase { target }) = (u.order, o)
            && !reach
            && !forgot
            && !self.still_cooling(u)
            && let Some(ot) = self.unit(old)
            && !self.within_strike_reach(u, ot)
        {
            self.units[i].order = o;
            self.command(i, o);
            self.attack_order(i, target);
            return true;
        }
        // Attack-move: orders 4, 5 and Enhanced 10 (`xorc01-death-knight` ogre 1488, n = 718,
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
        // Orders spec, "Friendly-fire check" (`xhuman4` ballista 1488 at n = 210, `xhuman10-spells`
        // ballista 1483 at n = 227): after a target switch in orders 12, 15 and 16 the run goes on with the new
        // target, and the check is made on it (not while still cooling down).
        let tile = t.tile;
        if found_target(u.order) && self.holds_fire(i, tile) {
            return true;
        }
        let u = &self.units[i];
        let same_order = matches!(
            (u.order, o),
            (Order::Chase { .. }, Order::Chase { .. })
                | (Order::AttackInPlace { .. }, Order::AttackInPlace { .. })
                | (Order::IdleReact { .. }, Order::IdleReact { .. })
        );
        let attack_order = found_target(u.order);
        // "Automatic target acquisition", the cooling-down case (V264,
        // `orc9-orc09-order16-keep-2000` grunt 1492, n = 937): a unit in order 16 still cooling down whose search gives
        // order 12 keeps order 16 on the new target, waiting for the cooldown left; there is no take-over with wait 3.
        let keeps_16 = matches!((u.order, o), (Order::IdleReact { .. }, Order::Chase { .. })) && self.still_cooling(u);
        let o = if keeps_16 { Order::IdleReact { target: best } } else { o };
        let same_order = same_order || keeps_16;
        let at_once = if same_order && self.still_cooling(u) {
            // Whatever the reach, the cooldown test restarts the animation with the cooldown left;
            // a strike already decided on the old target still lands.
            self.cooling_down(i)
        } else {
            attack_order && self.acquiring_strike(i, o, best) && same_order
        };
        if at_once {
            // "Resulting order" (`orc02-assault` footman 1587, n = 617): an order-12 switch to a
            // target in reach while still cooling down takes effect at once, and the cooldown test's restart (the
            // cooldown left as its wait) stands; there is no wait of 3. So does an out-of-reach
            // order-12 switch, and a same-order switch in orders 15 and 16 (`xhuman12-10000` archer 1372, n = 5142).
            // The command steps, with the order set at once and no take-over.
            self.units[i].order = o;
            self.command_steps(i);
            return true;
        }
        // "Same-update take-over": the order the search gives is a command that takes over at once.
        self.command_at_once(i, o);
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

    /// The reaction of a walking order (23, 24, 27, 6 or 34) in a logic run where its walk has ended
    /// ("Walking orders react only when the walk has ended"; `orc9-orc09-flee-walk-1200` peon 1478, n = 590): while
    /// the unit walks only the walk runs and the attacker stays recorded; these orders call this only once the walk has
    /// ended. That is when the unit has reached its goal or its target was found lost at a step
    /// boundary (and order 23's no-route re-aim); no route is not such a run for orders 24, 27 and 6, and order 34
    /// calls this only when it has reached its transport, can't board and keeps waiting.
    ///
    /// The arrival switch comes first: a unit still in its move animation is switched to its order's own animation
    /// (`core/animation-scripts.md`), so a flee given here takes over at once (still animation, wait 3). The
    /// switch has no attacker condition (`human4-15000` peon 1531, n = 638).
    pub(crate) fn react_at_walk_end(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        if u.script.anim == anim::MOVE {
            self.switch_anim(i, order_anim(u.order));
        }
        self.react(i)
    }

    /// The plain reaction to an attacker ("The reaction"). Returns true if the unit's order changed or it fled.
    pub(crate) fn react(&mut self, i: usize) -> bool {
        // "The reaction": an attacker that is no longer valid is forgotten. A dying or released one
        // too ("A recorded attacker outlives its slot"); an undetected one stays recorded.
        // The non-combat flee has no detection test, so a non-combat or mage-type unit
        // goes on with a valid attacker its owner doesn't detect, and flees from it as in step 1.
        let seed = self.attacker(i);
        let valid = self.units[i]
            .attacked_by
            .filter(|&a| self.unit(a).is_some_and(|a| a.present()));
        let undetected = valid.filter(|_| self.non_combat(&self.units[i]) || self.mage_like(&self.units[i]));
        // Round 85 (static gap-fill, erratum 12): a valid attacker the owner doesn't detect isn't the seed, but a
        // combat unit still runs the search without it.
        if seed.or(undetected).or(valid).is_none() {
            if self.units[i].attacked_by.is_some_and(|a| self.dying_or_gone(a)) {
                self.units[i].attacked_by = None;
            }
            return false;
        }
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
        // The reaction treats mage-type units as non-combat units, so they flee.
        if self.non_combat(u) || self.mage_like(u) {
            return self.flee(i);
        }
        let Some(best) = self.search(i, seed, false) else {
            return false;
        };
        if Some(best) == target_of(u.order) {
            return false;
        }
        self.immediately(i, Order::Chase { target: best });
        true
    }

    /// "Fleeing". Returns true if the unit fled or turned on its attacker.
    pub(crate) fn flee(&mut self, i: usize) -> bool {
        // "The attacker must still be valid": not dying, dead or inside something.
        // An invalid attacker is forgotten, with no flee; there is no detection test (`human12-25000`).
        let Some(a) = self.units[i].attacked_by else {
            return false;
        };
        if !self.unit(a).is_some_and(|a| a.present()) {
            self.units[i].attacked_by = None;
            return false;
        }
        let u = &self.units[i];
        let at = self.unit(a).expect("valid");
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
        // Traced (`xorc10` footman 1520 at n = 333, `human12` zeppelin 1503 at n = 5668): a unit that isn't in
        // its move animation takes the move in the same update (wait 3); one in its move animation keeps it pending.
        // Our reading: the take-over starts the move order's own animation, the still one, as any take-over does.
        // The move is a command, so its goal is pulled back, its blocked counter cleared and its
        // cached route dropped.
        // "Fleeing" step 6: the move is a smart move with no target unit. Its wait cap applies, and a
        // transport takes the transport outcome (landing, order 30, on a shore tile: `xorc8-10000` transport 1423,
        // n = 6177); with no target unit and no mine or platform the harvest outcomes don't, so every other unit gets
        // a move (order 3), even next to a forest.
        // The smart move's step 2 applies with its harvest test, so a worker carrying
        // nothing whose flee tile is a gold mine's harvests it, whatever it last harvested
        // (`orc9-orc09-shore-build-3000` peon 1478, n = 599, a builder that never harvested), as would a tanker not
        // carrying oil on its own player's oil platform.
        let o = self.smart_move_to(i, to);
        self.command(i, o);
        if self.units[i].script.anim != anim::MOVE
            && let Some(o) = self.units[i].pending.take()
        {
            self.take_over(i, o);
        }
        self.units[i].attacked_by = None;
        true
    }

    /// A smart move (command 3) with no target unit to `to`, whole ("Smart move"): the wait cap applies, a transport
    /// takes the transport outcome (`orders/transports.md`, "Commands given to a transport"), a worker carrying nothing
    /// on a gold mine (or a tanker on its own platform) harvests it by step 2's harvest test, and every other unit gets
    /// a move (order 3). The order is for [`World::command`]. Used by the flee ("Fleeing", step 6) and follow step 1
    /// ("Follow (orders 6 and 7)").
    fn smart_move_to(&mut self, i: usize, to: (i32, i32)) -> Order {
        self.cap_smart_wait(i);
        let mine = self.smart_building(&self.units[i], to);
        if self.is_transport(&self.units[i]) {
            self.transport_command(i, None, to, false)
        } else if let Some(o) = mine.and_then(|m| self.harvest_command(i, Some(m), to)) {
            o
        } else {
            Order::Move { goal: to, target: None }
        }
    }

    /// The friendly-fire check of a ground attacker in orders 12, 15 and 16, made in the order's logic before the
    /// range test (`xhuman2` ballista 1503, n = 2226, and `human06-31357` ballista 1582, n = 9483).
    /// Returns true if the strike is called off: the target is forgotten and idle takes over in the same update
    /// (still animation, wait 3, facing kept). Our reading of "after the cooldown test": a unit still cooling down
    /// (whose strike decision would only restart its animation) isn't checked in that run.
    pub(crate) fn holds_fire(&mut self, i: usize, at: (i32, i32)) -> bool {
        let u = &self.units[i];
        if !found_target(u.order) || self.flags_of_unit(u) & flags::GROUND_ATTACK == 0 {
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
            // Round 68 (V270, V271): idle becomes pending by the common path, with the resume step, and takes over in
            // the same update.
            self.idle_now(i);
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
            self.tower_shot(i, ti);
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

    /// The pixel y a unit sorts by ("Vertical order"): dying, dead and removed units (remains and
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

    /// Fog spec, "A unit created during a cycle" (V91): a computer player's attack command on an enemy unit created
    /// earlier in the same cycle drops the target, which nobody detects yet, and idle becomes the pending order. In the
    /// next cycle, after the detection phase, the same command works. A unit created outside a cycle (here, by the
    /// test before the first step) is detected by everyone at once.
    #[test]
    fn a_unit_created_during_a_cycle_is_undetected_until_the_next_detection_phase() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        assert_eq!(w.unit(f).unwrap().detected_by, 0xff);
        w.in_cycle = true;
        let foe = w.spawn(FOOTMAN, 0, (9, 5), 0).unwrap();
        assert_eq!(w.unit(foe).unwrap().detected_by, 0, "not even its owner detects it");
        let i = at(&w, f);
        w.command(i, Order::Attack { target: foe });
        let u = w.unit(f).unwrap();
        assert_eq!(
            (u.order, u.pending, u.commanded),
            (Order::Idle, Some(Order::Idle), true)
        );
        w.in_cycle = false;
        w.detection();
        assert_eq!(w.unit(foe).unwrap().detected_by, 0xff);
        let i = at(&w, f);
        w.command(i, Order::Attack { target: foe });
        assert_eq!(w.unit(f).unwrap().pending, Some(Order::Attack { target: foe }));
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
        w.units[i].pending = Some(Order::Move {
            goal: (1, 1),
            target: None,
        });
        w.lose_target(i);
        assert_eq!(
            (w.units[i].order, w.units[i].pending),
            (
                Order::Move {
                    goal: (1, 1),
                    target: None
                },
                None
            )
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
            assert_eq!(
                w.command_order(i, &c),
                Some(Order::Move {
                    goal: (10, 10),
                    target: None
                })
            );
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
        // Animation-scripts spec: no way of starting an animation changes the frame (Human04 peon
        // 1548 returning gold: frame 0 from the arrival switch kept through order 25's take-over at n = 374).
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].script.frame = 15;
        w.take_over(
            i,
            Order::Move {
                goal: (9, 9),
                target: None,
            },
        );
        assert_eq!((w.units[i].script.frame, w.units[i].script.wait), (15, 3));
        w.stop(i);
        assert_eq!((w.units[i].script.frame, w.units[i].script.wait), (15, 1));
    }

    #[test]
    fn orders_4_and_10_also_go_idle_through_a_same_update_take_over() {
        // Orders spec, "Arriving and losing the target": orders 3, 4 and 10 forget the attacker on arrival (the
        // Human04 critters' order 3 test above), and idle takes over in the same update (still animation, wait 3).
        for o in [
            Order::Move {
                goal: (7, 5),
                target: None,
            },
            Order::MoveAttack {
                goal: (7, 5),
                target: None,
            },
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
        w.units[i].order = Order::Move {
            goal: (15, 15),
            target: None,
        };
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

    /// Orders spec, no route, the patrol vectors (`human10-10000` n = 6868 and 6907, `xorc7-7000` n = 806):
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
        // Vector (the xhuman10 hero): command 8, then a move; the move arrives at idle (still animation,
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
            w.take_over(
                i,
                Order::Move {
                    goal: (5, 7),
                    target: None,
                },
            );
            let mut seen = None;
            for _ in 0..100 {
                w.step(&[]);
                let u = &w.units[i];
                if u.order
                    != (Order::Move {
                        goal: (5, 7),
                        target: None,
                    })
                {
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
        // "Order 5 (patrol)": the turnaround is a command, so its new goal is pulled back. The far end
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
    fn a_patrol_end_turns_around_before_the_target_search() {
        // V281 (`xorc10-xorc10-losses-4000` destroyer 1484, n = 1021): a computer unit whose patrol logic runs at its
        // patrol end turns around with no acquisition in that run, an enemy in reach notwithstanding (order 5, still
        // animation, wait 3, no target); the search runs at the next logic run, 3 cycles later, and attacks.
        let mut w = map();
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let foe = w.spawn(FOOTMAN, 0, (5, 6), 0).unwrap();
        let i = at(&w, f);
        let out = Order::Patrol {
            to: (5, 5),
            back: (12, 5),
        };
        w.take_over(i, out);
        let mut n = 0;
        while order_of(&w, f) == out {
            w.step(&[]);
            n += 1;
            assert!(n < 100);
        }
        let u = w.unit(f).unwrap();
        assert_eq!(
            (u.order, u.script.anim, u.script.wait),
            (
                Order::Patrol {
                    to: (12, 5),
                    back: (5, 5)
                },
                anim::STILL,
                3
            )
        );
        // No search in the 2 cycles the still animation waits out; at the third the search picks the enemy (order 12).
        for _ in 0..2 {
            w.step(&[]);
            assert_eq!(w.unit(f).unwrap().pending, None);
        }
        w.step(&[]);
        assert_eq!(w.unit(f).unwrap().pending, Some(Order::Chase { target: foe }));
        w.step(&[]);
        assert_eq!(order_of(&w, f), Order::Chase { target: foe });
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
        // The turnaround is a command, so it clears the blocked counter.
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
            Order::Move { goal, target: None },
            Order::MoveAttack { goal, target: None },
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

    /// Orders spec, "An unremembered patrol" (Enhanced; V274): an air unit's patrol to a goal within 1 tile on either
    /// axis leaves the remembered order as it was; a goal more than 1 tile away on both axes, or a land unit's, is
    /// remembered as given.
    #[test]
    fn an_air_patrol_within_a_tile_is_not_remembered() {
        const FLYER: u8 = 0x2b;
        let mut w = map();
        w.rules.enhanced = true;
        w.stats.0[FLYER as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[FLYER as usize].flags = flags::AIR | flags::CAN_ATTACK;
        let d = w.spawn(FLYER, 0, (10, 10), 0).unwrap();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let old = Some(Remembered::AttackMove { goal: (3, 3) });
        let patrol = |w: &mut World, id: UnitId, to: (i32, i32)| {
            let i = at(w, id);
            w.units[i].remembered = old;
            let c = Command::Patrol {
                player: 0,
                units: vec![id],
                to,
            };
            w.command_order(i, &c);
            w.units[i].remembered
        };
        assert_eq!(patrol(&mut w, d, (11, 9)), old, "within 1 tile on both axes");
        assert_eq!(patrol(&mut w, d, (10, 10)), old, "its own tile");
        assert_eq!(patrol(&mut w, d, (12, 10)), old, "along its own row");
        assert_eq!(patrol(&mut w, d, (10, 24)), old, "along its own column (V274)");
        assert_eq!(patrol(&mut w, d, (19, 11)), old, "1 tile away on y");
        assert_eq!(
            patrol(&mut w, d, (12, 12)),
            Some(Remembered::Patrol {
                to: (12, 12),
                back: (10, 10)
            })
        );
        assert_eq!(
            patrol(&mut w, f, (6, 6)),
            Some(Remembered::Patrol {
                to: (6, 6),
                back: (5, 5)
            }),
            "a land unit"
        );
    }

    /// Movement spec test vector: an idle unit with blocked counter 2 acquires a target. The resulting
    /// order is a command: counter 0, route dropped, and it takes over at once.
    #[test]
    fn an_acquisition_clears_the_blocked_counter_and_the_route() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        let i = at(&w, f);
        // Seen by the footman's owner (a hidden attacker doesn't seed the search).
        let gi = at(&w, g);
        w.units[gi].hidden_from = 0;
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
        let gi = at(&w, g);
        w.units[gi].hidden_from = 0;
        w.units[i].attacked_by = Some(g);
        // Order 12 (chase) rather than 16, as the footman has an attacker.
        assert!(w.acquire(i, Acquires::Idle, false));
        assert_eq!(order_of(&w, f), Order::Chase { target: g });
    }

    /// Orders spec, "Same-update take-over", the `orc02` n = 570 vector and the static-study one: a
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

    /// "Resulting order", and the `orc02-assault` n = 617 vector: an order-12 switch to a target in
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

    /// "Resulting order" (`xhuman12-10000` grunt 1476, n = 1847): an out-of-reach order-12 switch
    /// while still cooling down is handled as in reach: no command, no step, no wait of 3, the cooldown left as the
    /// wait. Not cooling down, the switch is the command case (a step toward the new target).
    #[test]
    fn an_out_of_reach_switch_while_cooling_down_takes_the_target_at_once() {
        for cooling in [true, false] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let old = w.spawn(FOOTMAN, 1, (5, 4), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 8), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            // The old target is out of the search's picture (dying), so the search picks g, out of reach.
            let oi = at(&w, old);
            w.units[oi].dying = true;
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.pending = None;
            u.order = Order::Chase { target: old };
            u.facing = 0;
            u.cooldown = if cooling { 23 } else { 0 };
            u.script.wait = 1;
            assert!(w.run_order(i));
            let u = &w.units[i];
            if cooling {
                assert_eq!(
                    (u.order, u.pending, u.tile, u.script.anim, u.script.frame, u.script.wait),
                    (Order::Chase { target: g }, None, (5, 5), anim::ATTACK, 0, 23)
                );
            } else {
                assert_eq!((u.order, u.pending), (Order::Chase { target: g }, None));
                assert_ne!(u.script.wait, 23);
            }
        }
    }

    /// Traced (`xhuman12-10000` archer 1372, n = 5142): a unit in order 16 whose search gives order 16 on a
    /// different target while it is still cooling down takes it at once with the cooldown left as its wait.
    #[test]
    fn an_order_16_switch_while_cooling_down_keeps_the_cooldown_wait() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let old = w.spawn(FOOTMAN, 1, (20, 20), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, f);
        let u = &mut w.units[i];
        u.pending = None;
        u.order = Order::IdleReact { target: old };
        u.cooldown = 40;
        u.script.wait = 1;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.script.anim, u.script.frame, u.script.wait),
            (Order::IdleReact { target: g }, anim::ATTACK, 0, 40)
        );
    }

    /// "Reacting to an attacker", step 1 (`xorc11-10000` battleship 1539, n = 628): a dying attacker
    /// the unit can't target is forgotten, with no flee and no search, and the order's logic goes on. One it can
    /// target is kept while dying, and the search runs without it.
    #[test]
    fn a_dying_attacker_the_unit_cannot_target_is_forgotten_without_a_search() {
        const FLYER: u8 = 0x2b;
        for can_hit_air in [false, true] {
            let mut w = map();
            w.stats.0[FLYER as usize] = w.stats.0[FOOTMAN as usize];
            w.stats.0[FLYER as usize].flags = flags::AIR | flags::CAN_ATTACK;
            w.stats.0[FOOTMAN as usize].can_target = if can_hit_air { 5 } else { 1 };
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let d = w.spawn(FLYER, 1, (8, 8), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            assert_eq!(w.unit(d).unwrap().layer, Layer::Air);
            let di = at(&w, d);
            w.units[di].dying = true;
            let i = at(&w, f);
            w.units[i].pending = None;
            w.units[i].order = Order::Idle;
            w.units[i].attacked_by = Some(d);
            let changed = w.acquire(i, Acquires::Idle, false);
            let u = &w.units[i];
            if can_hit_air {
                // The dying attacker still counts for the resulting order (12); the command then forgets it.
                assert!(changed, "the search runs");
                assert_eq!((u.order, u.attacked_by), (Order::Chase { target: g }, None));
            } else {
                assert!(!changed, "no search");
                assert_eq!((u.order, u.attacked_by), (Order::Idle, None));
            }
        }
    }

    /// "Commands", command 4 (static study): order 4's end on a dying target forgets the attacker,
    /// as arrival does.
    #[test]
    fn order_4_ending_on_its_target_forgets_the_attacker() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let t = w.spawn(FOOTMAN, 1, (15, 15), 0).unwrap();
        let a = w.spawn(FOOTMAN, 1, (20, 5), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let ti = at(&w, t);
        w.units[ti].dying = true;
        let i = at(&w, f);
        w.units[i].pending = None;
        w.units[i].order = Order::MoveAttack {
            goal: (15, 15),
            target: Some(t),
        };
        w.units[i].attacked_by = Some(a);
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!((u.order, u.attacked_by, u.script.wait), (Order::Idle, None, 3));
    }

    /// "Checks on an attack command": the kind checks in order, with a submarine judged by the target
    /// type's flags and a transport on a shore tile hittable with land or sea.
    #[test]
    fn the_attack_kind_checks_go_in_order() {
        const SUB: u8 = 0x26;
        const BOAT: u8 = 0x1c;
        let mut w = map();
        w.stats.0[SUB as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[SUB as usize].flags = flags::SEA | flags::SUBMARINE | flags::CAN_ATTACK;
        w.stats.0[SUB as usize].can_target = 2;
        w.stats.0[BOAT as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[BOAT as usize].flags = flags::SEA | flags::TRANSPORT;
        w.stats.0[FOOTMAN as usize].can_target = 1;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let base = w.unit(f).unwrap().clone();
        let with = |kind: u8, layer: Layer, tile: (i32, i32)| {
            let mut u = base.clone();
            (u.kind, u.layer, u.tile) = (kind, layer, tile);
            u
        };
        let footman = with(FOOTMAN, Layer::Land, (5, 5));
        let sub = with(SUB, Layer::Sea, (5, 5));
        let boat = with(BOAT, Layer::Sea, (10, 10));
        let shore_boat = with(BOAT, Layer::Sea, (12, 12));
        w.terrain.set_bits(12, 12, crate::terrain::bits::SHORE);
        // A land attacker hits a transport only on a shore tile; a sea attacker hits it anywhere.
        assert!(!w.can_hit_kind(&footman, &boat));
        assert!(w.can_hit_kind(&footman, &shore_boat));
        assert!(w.can_hit_kind(&sub, &boat));
        // A submarine goes by the target type's flags, not its layer.
        assert!(!w.can_hit_kind(&sub, &footman));
        assert!(w.can_hit_kind(&sub, &with(BOAT, Layer::Land, (5, 5))));
        // An air target needs air, whatever else.
        assert!(!w.can_hit_kind(&footman, &with(FOOTMAN, Layer::Air, (5, 5))));
        assert!(!w.can_hit_kind(&sub, &with(BOAT, Layer::Air, (5, 5))));
    }

    /// "Attack-move: orders 4, 5 and (Enhanced) 10": a unit in order 4 that picks a target already
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
        w.units[i].order = Order::MoveAttack { goal, target: None };
        assert!(w.within_strike_reach(&w.units[i], w.unit(g).unwrap()));
        let before = w.rng;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.tile, u.script.anim),
            (
                Order::MoveAttack { goal, target: None },
                Some(Order::Chase { target: g }),
                (5, 6),
                anim::MOVE
            )
        );
        assert!(w.missiles.iter().all(Option::is_none) && w.rng == before, "no strike");
    }

    /// "Friendly-fire check": made in the order's logic before the range test, so a ground attacker
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

    /// Traced (`xhuman4` ballista 1488, n = 210): after order 16's search switches to a new target, the run
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

    /// "A dying attacker in orders 9, 11 and 17": each run first forgets a dying attacker, before the
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
        // Order 13 takes over in the same update (still animation, wait 3, target null).
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

    /// "Attack-move" (the `human06-31357` axe thrower 1512 vector at n = 16277): a building target is
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
            w.units[i].order = Order::MoveAttack {
                goal: (5, 20),
                target: None,
            };
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

    /// "Choice" (the `human06-31357` ballista 1582 vector at n = 23683): an attacker inside the
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
        // The move takes over in the same update.
        let Order::Move { goal, .. } = w.units[i].order else {
            panic!("expected a move, got {:?}", w.units[i].order)
        };
        assert_eq!(w.units[i].pending, None);
        // (r mod 8) - 2 + 4 east, (r mod 8) - 2 vertically.
        assert!((12..=19).contains(&goal.0) && (8..=15).contains(&goal.1), "{goal:?}");
        assert_eq!(w.units[i].attacked_by, None);
    }

    /// "The reaction": the non-combat flee has no detection test, so an idle peasant flees
    /// from a valid attacker its owner doesn't detect.
    #[test]
    fn a_peasant_flees_an_undetected_attacker() {
        let mut w = map();
        let p = w.spawn(PEASANT, 0, (10, 10), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (6, 10), 0).unwrap();
        w.unit_visibility();
        let (i, ai) = (at(&w, p), at(&w, a));
        w.units[ai].detected_by &= !crate::vision::bit(0);
        w.units[i].attacked_by = Some(a);
        assert_eq!(w.attacker(i), None, "not detected");
        w.react(i);
        assert!(matches!(w.units[i].order, Order::Move { .. }), "{:?}", w.units[i].order);
    }

    #[test]
    fn a_flee_move_is_a_command_whose_goal_is_pulled_back() {
        // "Fleeing" step 6: the flee tile is pulled back off ground the unit can't stand on, and the
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
        let Order::Move { goal, .. } = w.units[i].order else {
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
        // Traced (`orc9-orc09-follow-flee-1300` peon 1478, n = 605): the guard takes over in the same update,
        // with the still animation and a wait of 3.
        assert_eq!((w.units[i].script.anim, w.units[i].script.wait), (anim::STILL, 3));
        // The guarded unit starts harvesting: a move to the guard's own tile.
        let gi = at(&w, g);
        w.units[gi].order = Order::ReturnGoods { depot: None };
        w.follow(i, g);
        assert_eq!(
            w.units[i].pending,
            Some(Order::Move {
                goal: (5, 5),
                target: None
            })
        );
    }

    /// "Follow (orders 6 and 7)", step 1 (round 71, V276): a human player's ogre guarding a grunt, in its still
    /// animation with 14 cycles left; the grunt is given stand ground. The move to its own tile is a smart move:
    /// pending as a command, and the wait drops to the smart move's cap of 10.
    #[test]
    fn a_guard_stops_by_a_smart_move() {
        const OGRE: u8 = 0x07;
        let mut w = map();
        w.stats.0[OGRE as usize] = w.stats.0[FOOTMAN as usize];
        let o = w.spawn(OGRE, 0, (5, 5), 0).unwrap();
        let g = w.spawn(FOOTMAN, 0, (6, 6), 0).unwrap();
        let i = at(&w, o);
        let u = &mut w.units[i];
        u.order = Order::Guard { target: g };
        u.blocked = 2;
        u.script.wait = 14;
        let gi = at(&w, g);
        w.units[gi].order = Order::StandGround;
        assert!(!w.follow(i, g));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.commanded, u.script.anim, u.script.wait, u.blocked),
            (
                Order::Guard { target: g },
                Some(Order::Move {
                    goal: (5, 5),
                    target: None
                }),
                true,
                anim::STILL,
                10,
                0
            )
        );
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
        // Orders spec, "Vertical order": a unit that goes in at pixel y 160 keeps its place while
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

    /// "Losing the target while cooling down" (the `xorc01-death-knight` ogre 1488 vector): in orders
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

    /// "Arriving and losing the target" (V021, V022, V273): with nothing pending, order 15 losing its target gets a
    /// pending 13 and the others a pending 2, which takes over in the same update: still animation, wait 3, target
    /// null. A pending order takes over the same way (wait 3), not as an immediate order.
    #[test]
    fn a_lost_target_takes_over_with_wait_3() {
        type Make = fn(UnitId) -> Order;
        let cases: [(Make, Option<Order>, Order); 4] = [
            (|t| Order::Attack { target: t }, None, Order::Idle),
            (|t| Order::AttackInPlace { target: t }, None, Order::StandGround),
            (|t| Order::IdleReact { target: t }, None, Order::Idle),
            (
                |t| Order::AttackInPlace { target: t },
                Some(Order::Patrol {
                    to: (9, 9),
                    back: (5, 5),
                }),
                Order::Patrol {
                    to: (9, 9),
                    back: (5, 5),
                },
            ),
        ];
        for (make, pending, lost) in cases {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.step(&[]);
            let gi = at(&w, g);
            w.kill(gi);
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.order = make(g);
            u.pending = pending;
            u.script.wait = 5;
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait, u.recorded_target()),
                (lost, None, anim::STILL, 3, None),
                "{:?}",
                make(g)
            );
        }
    }

    /// "Resuming patrol and attack-move": the resumed order is a full command. Its goal is pulled
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

    /// "Commands forget a stale attacker": a command forgets a dying attacker, and keeps a live one. A
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
        w.command(
            i,
            Order::Move {
                goal: (1, 1),
                target: None,
            },
        );
        assert_eq!(w.units[i].attacked_by, Some(a));
        let ai = at(&w, a);
        w.kill(ai);
        w.command(
            i,
            Order::Move {
                goal: (2, 2),
                target: None,
            },
        );
        assert_eq!(w.units[i].attacked_by, None);
        let oi = at(&w, own);
        w.kill(oi);
        let c = Command::Smart {
            player: 0,
            units: vec![f],
            target: Some(own),
            tile: (3, 3),
        };
        assert_eq!(
            w.command_order(i, &c),
            Some(Order::Move {
                goal: (3, 3),
                target: None
            })
        );
    }

    /// "No route": a move with no route keeps its attacker, so the idle logic that runs in the same
    /// update may react to it. Here a worker flees (the `xorc10-dragons` footman 1520 vector, n = 464).
    #[test]
    fn a_move_with_no_route_keeps_its_attacker_and_flees() {
        let mut w = walled_in();
        let p = w.spawn(PEASANT, 0, (5, 5), 0).unwrap();
        let a = w.spawn(FOOTMAN, 1, (9, 5), 0).unwrap();
        w.unit_visibility();
        let i = at(&w, p);
        w.units[i].order = Order::Move {
            goal: (15, 15),
            target: None,
        };
        w.units[i].attacked_by = Some(a);
        let before = w.rng.state;
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert!(
            matches!(u.order, Order::Move { goal, .. } if goal != (15, 15)),
            "{:?}",
            u.order
        );
        assert_eq!((u.pending, u.attacked_by, u.script.wait), (None, None, 3));
        assert_ne!(w.rng.state, before, "the flee drew");
    }

    /// "Reacting to an attacker" step 3 and "Special cases": an idle mage-type unit hit by a combat
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

    /// "Arriving and losing the target" (`xorc11-10000` gryphon rider 1400, n = 6046): a unit in order
    /// 4 or (Enhanced) 10 standing at its move goal arrives, with no target acquisition, even with an enemy in range.
    #[test]
    fn orders_4_and_10_at_their_goal_arrive_without_acquiring() {
        for o in [
            Order::MoveAttack {
                goal: (5, 5),
                target: None,
            },
            Order::AttackMove { goal: (5, 5) },
        ] {
            let mut w = map();
            w.rules.enhanced = true;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let i = at(&w, f);
            assert!(w.in_range(&w.units[i], w.unit(g).unwrap()));
            w.units[i].pending = None;
            w.units[i].order = o;
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait, u.recorded_target()),
                (Order::Idle, None, anim::STILL, 3, None),
                "{o:?}"
            );
        }
    }

    /// "Forgetting a dying attacker first" (`xorc6-10000` axe thrower 1475, n = 6106): the search
    /// forgets a dying attacker, and the resulting order is picked from the record as it was before: the first search
    /// gives order 12, a later one (no attacker) order 16 for a human player's idle unit.
    #[test]
    fn a_search_forgets_a_dying_attacker_but_counts_it_once() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let d = w.spawn(FOOTMAN, 1, (9, 9), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let di = at(&w, d);
        w.units[di].dying = true;
        let i = at(&w, f);
        w.units[i].pending = None;
        w.units[i].order = Order::Idle;
        w.units[i].attacked_by = Some(d);
        assert!(w.acquire(i, Acquires::Idle, false));
        let u = &w.units[i];
        assert_eq!((u.order, u.attacked_by), (Order::Chase { target: g }, None));
        // The next search sees no attacker.
        w.units[i].order = Order::Idle;
        assert!(w.acquire(i, Acquires::Idle, false));
        assert_eq!(w.units[i].order, Order::IdleReact { target: g });
    }

    /// "A recorded attacker outlives its slot" (static study): a released slot reads as a dying
    /// attacker. The first search still counts it (order 12) and forgets it; a released slot whose last type the unit
    /// can't target is forgotten by step 1 of the reaction, with no search.
    #[test]
    fn a_released_attacker_reads_as_dying() {
        const FLYER: u8 = 0x2b;
        for flyer in [false, true] {
            let mut w = map();
            w.stats.0[FLYER as usize] = w.stats.0[FOOTMAN as usize];
            w.stats.0[FLYER as usize].flags = flags::AIR | flags::CAN_ATTACK;
            w.stats.0[FOOTMAN as usize].can_target = 1;
            w.rules.reveal_map = true;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let d = w.spawn(FOOTMAN, 1, (9, 9), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            // Release d's slot, leaving the last type it held.
            let di = at(&w, d);
            let (slot, owner) = (w.units[di].slot, w.units[di].owner);
            w.slots.release(slot, owner);
            w.units.remove(di);
            w.ylist.retain(|&x| x != d);
            let kind = if flyer { FLYER } else { FOOTMAN };
            w.stale.insert(
                slot,
                crate::caster_ai::Stale {
                    kind,
                    tile: (9, 9),
                    marks: 0,
                },
            );
            assert!(w.unit(d).is_none() && w.last_slot(d) == Some(slot));
            let i = at(&w, f);
            w.units[i].pending = None;
            w.units[i].order = Order::Idle;
            w.units[i].attacked_by = Some(d);
            let changed = w.acquire(i, Acquires::Idle, false);
            let u = &w.units[i];
            if flyer {
                assert!(!changed, "no search");
                assert_eq!((u.order, u.attacked_by), (Order::Idle, None));
            } else {
                assert!(changed);
                assert_eq!((u.order, u.attacked_by), (Order::Chase { target: g }, None));
            }
        }
    }

    /// "A recorded attacker outlives its slot": the record names the slot's new unit once the slot is
    /// reused.
    #[test]
    fn a_reused_slot_gives_the_record_its_new_unit() {
        let mut w = map();
        let v = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let a = w.spawn(FOOTMAN, 1, (9, 9), 0).unwrap();
        let vi = at(&w, v);
        w.units[vi].attacked_by = Some(a);
        let ai = at(&w, a);
        let (slot, owner) = (w.units[ai].slot, w.units[ai].owner);
        w.slots.release(slot, owner);
        w.units.remove(ai);
        w.ylist.retain(|&x| x != a);
        let n = w.spawn(FOOTMAN, 1, (12, 12), 0).unwrap();
        assert_eq!(w.unit(n).unwrap().slot, slot);
        assert_eq!(w.unit(v).unwrap().attacked_by, Some(n));
        assert_eq!((w.unit(a).is_none(), w.last_slot(a)), (true, None));
    }

    /// "Fleeing" step 6: the flee is a smart move with its wait cap, and a peasant fleeing toward a
    /// forest gets a move (order 3), never a tree harvest.
    #[test]
    fn a_flee_toward_a_forest_is_a_move_with_the_smart_wait_cap() {
        let mut w = map();
        for y in 0..w.terrain.height as i32 {
            for x in 12..w.terrain.width as i32 {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.regions[n] = crate::terrain::region::FOREST;
                w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
            }
        }
        let p = w.spawn(PEASANT, 0, (10, 10), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (6, 10), 0).unwrap();
        let (i, ai) = (at(&w, p), at(&w, a));
        w.units[ai].facing = 2;
        w.units[i].attacked_by = Some(a);
        // In its move animation the order stays pending, so the capped wait shows.
        w.start_anim(i, anim::MOVE);
        w.units[i].script.wait = 30;
        assert!(w.flee(i));
        let u = &w.units[i];
        assert!(matches!(u.pending, Some(Order::Move { .. })), "{:?}", u.pending);
        assert_eq!(u.script.wait, SMART_WAIT_OTHER);
    }

    /// "Order 12 with no route" step 3 (the `xorc8-10000` elven destroyer 1468 vector at n = 7372,
    /// in small): a marked unit in order 12 out of reach of its target takes a move step, whose target search runs
    /// whatever the mark. It takes an enemy it has no route to and goes idle at once (still animation, wait 1, no
    /// target); the idle logic that runs straight after skips its search and clears the mark. With no such enemy the
    /// unit steps toward its target and the mark stays set.
    #[test]
    fn the_skip_mark_does_not_stop_the_move_steps_search() {
        for walled_foe in [true, false] {
            // A computer player's footman (reaction range 6) chasing a footman 8 tiles away.
            let mut w = map();
            let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
            let old = w.spawn(FOOTMAN, 0, (5, 13), 0).unwrap();
            if walled_foe {
                // Walled in two tiles out, with the first tile of the straight line toward it blocked too, so there is
                // no route to it; the way south to the old target stays open.
                w.spawn(FOOTMAN, 0, (9, 5), 0).unwrap();
                for y in 3..=7 {
                    for x in 6..=11 {
                        if x == 6 && y != 5 {
                            continue;
                        }
                        if x <= 7 || x == 11 || y == 3 || y == 7 {
                            let n = w.terrain.at(x, y).unwrap();
                            w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
                        }
                    }
                }
            }
            w.unit_visibility();
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.order = Order::Chase { target: old };
            u.pending = None;
            u.skip_search = true;
            u.facing = 7;
            let fx = crate::anim::Effects {
                step_done: true,
                ..Default::default()
            };
            w.order(i, fx);
            let u = &w.units[i];
            if walled_foe {
                assert_eq!(
                    (u.order, u.tile, u.facing, u.script.anim, u.script.wait),
                    (Order::Idle, (5, 5), 7, anim::STILL, 1)
                );
                assert!(!u.skip_search, "the idle logic's run clears it");
            } else {
                assert_eq!((u.order, u.script.anim), (Order::Chase { target: old }, anim::MOVE));
                assert!(u.skip_search, "a move step leaves it set");
            }
        }
    }

    /// "Order 12 with no route" step 3 (static study; `xorc8-10000` n = 7373): a marked unit out of
    /// reach takes a move step whose search takes an enemy it has no route to; order 12's fallback then runs its
    /// attack-range search whatever the mark, finds a unit next to it and restarts order 12 on it, setting the mark
    /// again. The update began marked, so the mark is cleared at its end. A mark first set in an update (the same
    /// restart from an unmarked start) lasts until the next one.
    #[test]
    fn a_restart_in_an_update_that_began_marked_clears_the_mark() {
        for marked in [true, false] {
            let mut w = map();
            // A unit that can't attack scores below any that can, so only the attack-range search picks it.
            w.stats.0[PEASANT as usize].flags = flags::LAND | flags::PEON;
            let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
            let old = w.spawn(FOOTMAN, 0, (5, 13), 0).unwrap();
            w.spawn(FOOTMAN, 0, (9, 5), 0).unwrap();
            let p = w.spawn(PEASANT, 0, (4, 5), 0).unwrap();
            for y in 3..=7 {
                for x in 6..=11 {
                    if x == 6 && y != 5 {
                        continue;
                    }
                    if x <= 7 || x == 11 || y == 3 || y == 7 {
                        let n = w.terrain.at(x, y).unwrap();
                        w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
                    }
                }
            }
            w.unit_visibility();
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.order = Order::Chase { target: old };
            u.pending = None;
            u.skip_search = marked;
            let fx = crate::anim::Effects {
                step_done: true,
                ..Default::default()
            };
            w.order(i, fx);
            let u = &w.units[i];
            assert_eq!(
                (u.order, u.tile, u.script.anim),
                (Order::Chase { target: p }, (5, 5), anim::ATTACK)
            );
            assert_eq!(u.skip_search, !marked, "began marked: {marked}");
        }
    }

    /// "Smart move" step 2 and its harvest test (`orc9-orc09-smart-move-2000` vectors): with no
    /// target unit, a gold mine on the goal tile is the target of a worker carrying nothing, whatever it last
    /// harvested (never harvested, or a lumber worker between loads), and an oil platform of the unit's own player
    /// that of a tanker not carrying oil; a carrying unit, another player's platform or anything else gives none.
    #[test]
    fn a_smart_move_onto_a_mine_or_own_platform_takes_it_as_the_target() {
        const MINE: u8 = 0x5c;
        const TANKER: u8 = 0x1a;
        const PLATFORM: u8 = 0x5f;
        let mut w = map();
        for (k, f) in [
            (MINE, flags::BUILDING | flags::MINE),
            (PLATFORM, flags::BUILDING | flags::OIL_SOURCE),
        ] {
            w.stats.0[k as usize] = w.stats.0[FARM as usize];
            w.stats.0[k as usize].flags = f;
        }
        w.stats.0[TANKER as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[TANKER as usize].flags = flags::LAND | flags::TANKER;
        let mine = w.spawn(MINE, slots::NEUTRAL, (10, 10), 0).unwrap();
        let own = w.spawn(PLATFORM, 0, (2, 10), 0).unwrap();
        let theirs = w.spawn(PLATFORM, 1, (2, 2), 0).unwrap();
        let p = w.spawn(PEASANT, 0, (5, 5), 0).unwrap();
        let t = w.spawn(TANKER, 0, (6, 5), 0).unwrap();
        let peon = w.unit(p).unwrap().clone();
        for gather in [crate::world::Gather::Gold, crate::world::Gather::Lumber, peon.gather] {
            let worker = Unit { gather, ..peon.clone() };
            assert_eq!(w.smart_building(&worker, (10, 10)), Some(mine), "{gather:?}");
        }
        assert_eq!(w.smart_building(&peon, (12, 12)), None, "off the footprint");
        let carrying = Unit {
            carrying: true,
            ..peon.clone()
        };
        assert_eq!(w.smart_building(&carrying, (10, 10)), None);
        let tanker = w.unit(t).unwrap().clone();
        assert_eq!(w.smart_building(&tanker, (2, 10)), Some(own));
        assert_eq!(w.smart_building(&tanker, (2, 2)), None, "another player's platform");
        assert_eq!(w.smart_building(&tanker, (10, 10)), None);
        let full = Unit {
            carrying: true,
            ..tanker.clone()
        };
        assert_eq!(w.smart_building(&full, (2, 10)), None, "a tanker carrying oil");
        assert_eq!(w.smart_building(&peon, (2, 10)), None, "a worker on a platform");
        let _ = theirs;
    }

    /// "Smart move" table: a smart move by a lumber worker between loads onto a gold mine harvests
    /// it (order 23 on the mine, `orc9-orc09-smart-move-2000` n = 201); one carrying gold gets the move (n = 481); a
    /// smart move onto a forest tile is the move, never a tree harvest.
    #[test]
    fn a_smart_move_harvests_only_by_the_harvest_test() {
        const MINE: u8 = 0x5c;
        let mut w = map();
        w.stats.0[MINE as usize] = w.stats.0[FARM as usize];
        w.stats.0[MINE as usize].flags = flags::BUILDING | flags::MINE;
        for y in 0..w.terrain.height as i32 {
            for x in 16..w.terrain.width as i32 {
                let n = w.terrain.at(x, y).unwrap();
                w.terrain.regions[n] = crate::terrain::region::FOREST;
                w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
            }
        }
        let mine = w.spawn(MINE, slots::NEUTRAL, (10, 10), 0).unwrap();
        let p = w.spawn(PEASANT, 0, (5, 5), 0).unwrap();
        let i = at(&w, p);
        w.units[i].gather = crate::world::Gather::Lumber;
        let smart = |tile| Command::Smart {
            player: 0,
            units: vec![p],
            target: None,
            tile,
        };
        let o = w.command_order(i, &smart((10, 10)));
        assert!(
            matches!(o, Some(Order::Harvest { source: Some(s), .. }) if s == mine),
            "{o:?}"
        );
        w.units[i].carrying = true;
        w.units[i].gather = crate::world::Gather::Gold;
        let o = w.command_order(i, &smart((10, 10)));
        assert!(matches!(o, Some(Order::Move { .. })), "{o:?}");
        w.units[i].carrying = false;
        let o = w.command_order(i, &smart((17, 5)));
        assert!(matches!(o, Some(Order::Move { .. })), "a forest tile: {o:?}");
    }

    /// "Order 12 with no route" step 3 (the `orc12-30000` juggernaut 1439 vector, n = 29156): a
    /// marked unit whose order 12 loses its target in its own logic run, its remembered patrol taking over in that
    /// run, has the mark cleared at the end of the update. A pending order taking over before the logic runs leaves it.
    #[test]
    fn a_take_over_caused_by_the_logic_run_clears_the_mark() {
        for pending in [false, true] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
            let gone = w.spawn(FOOTMAN, 0, (6, 5), 0).unwrap();
            let gi = at(&w, gone);
            w.units[gi].dying = true;
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.order = Order::Chase { target: gone };
            u.pending = pending.then_some(Order::Idle);
            u.remembered = Some(Remembered::Patrol {
                to: (5, 12),
                back: (5, 5),
            });
            u.skip_search = true;
            let fx = crate::anim::Effects {
                step_done: true,
                ..Default::default()
            };
            w.order(i, fx);
            let u = &w.units[i];
            if pending {
                assert_eq!(u.order, Order::Idle);
                assert!(u.skip_search, "a pending take-over leaves the mark");
            } else {
                assert!(matches!(u.order, Order::Patrol { .. }), "{:?}", u.order);
                assert!(!u.skip_search, "the logic run's own take-over clears the mark");
            }
        }
    }

    /// "Order 12 with no route": the take-over makes no turn and no strike decision (shown by
    /// running it with the follow-up logic run held back); order 12's logic run in the same update then skips the
    /// search because of the mark, turns toward the target and starts the cooldown.
    #[test]
    fn a_no_route_take_over_turns_only_in_the_logic_run_that_follows() {
        for held_back in [true, false] {
            let mut w = map();
            w.stats.0[PEASANT as usize].flags = flags::LAND | flags::PEON;
            let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
            let old = w.spawn(FOOTMAN, 0, (5, 13), 0).unwrap();
            w.spawn(FOOTMAN, 0, (9, 5), 0).unwrap();
            let p = w.spawn(PEASANT, 0, (4, 5), 0).unwrap();
            for y in 3..=7 {
                for x in 6..=11 {
                    if x == 6 && y != 5 {
                        continue;
                    }
                    if x <= 7 || x == 11 || y == 3 || y == 7 {
                        let n = w.terrain.at(x, y).unwrap();
                        w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
                    }
                }
            }
            w.unit_visibility();
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.order = Order::Chase { target: old };
            u.pending = None;
            u.facing = 0;
            u.cooldown = 0;
            w.fallback_run = held_back;
            let fx = crate::anim::Effects {
                step_done: true,
                ..Default::default()
            };
            w.order(i, fx);
            let u = &w.units[i];
            assert_eq!((u.order, u.script.anim), (Order::Chase { target: p }, anim::ATTACK));
            if held_back {
                assert_eq!((u.facing, u.cooldown), (0, 0), "the take-over alone");
            } else {
                assert_eq!(u.facing, crate::world::face_tile((5, 5), (4, 5)));
                assert!(u.cooldown > 0);
            }
        }
    }

    /// "Choice" (`human06-31357` n = 9642, `xhuman11-10000` n = 9065, and the static-study row): the
    /// attacker seeds the search only when the unit's owner detects it and the fog doesn't hide it. A hidden or
    /// undetected attacker leaves the search as if there were none; it isn't fled from either ("Reacting to an
    /// attacker", step 1, is the kind check only: the open question's reading).
    #[test]
    fn a_hidden_or_undetected_attacker_does_not_seed_the_search() {
        for (hidden, detected, seeds) in [(false, true, true), (true, true, false), (false, false, false)] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (12, 5), 0).unwrap();
            let (i, gi) = (at(&w, f), at(&w, g));
            w.units[gi].hidden_from = if hidden { crate::vision::bit(0) } else { 0 };
            if !detected {
                w.units[gi].detected_by &= !crate::vision::bit(0);
            }
            w.units[i].attacked_by = Some(g);
            let changed = w.acquire(i, Acquires::Idle, false);
            let u = &w.units[i];
            if seeds {
                assert!(changed);
                assert_eq!(u.order, Order::Chase { target: g });
            } else {
                assert!(!changed, "hidden {hidden}, detected {detected}");
                assert_eq!((u.order, u.pending), (Order::Idle, None), "no chase and no flee");
                assert_eq!(u.attacked_by, Some(g), "the attacker stays recorded");
            }
        }
    }

    /// "The reaction" (round 85, static gap-fill, erratum 12; vector: a destroyer hit by a submarine its owner
    /// doesn't detect, with an enemy destroyer within its react range, attacks that destroyer): a valid attacker the
    /// owner doesn't detect isn't the seed, but the search still runs, and a result other than the current target
    /// gives order 12. With nothing else in reach there is no change, and the attacker stays recorded.
    #[test]
    fn the_reaction_searches_without_an_undetected_attacker() {
        for other in [false, true] {
            let mut w = map();
            w.stats.0[FOOTMAN as usize].can_target = 1;
            w.rules.reveal_map = true;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (12, 5), 0).unwrap();
            let h = other.then(|| w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap());
            w.unit_visibility();
            let (i, gi) = (at(&w, f), at(&w, g));
            w.units[gi].detected_by &= !crate::vision::bit(0);
            w.units[i].attacked_by = Some(g);
            assert_eq!(w.react(i), other);
            let u = &w.units[i];
            match h {
                Some(h) => assert_eq!(u.order, Order::Chase { target: h }),
                None => {
                    assert_eq!(u.order, Order::Idle);
                    assert_eq!(u.attacked_by, Some(g), "the attacker stays recorded");
                }
            }
        }
    }

    /// A sea attacker and a tanker for the tests of "Reacting to an attacker", step 1: kinds the footman (land only)
    /// can't hit.
    fn with_ships(w: &mut World) -> (u8, u8) {
        const DESTROYER: u8 = 0x1e;
        const TANKER: u8 = 0x1a;
        w.stats.0[DESTROYER as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[DESTROYER as usize].flags = flags::SEA | flags::CAN_ATTACK;
        w.stats.0[DESTROYER as usize].can_target = 3;
        w.stats.0[TANKER as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[TANKER as usize].flags = flags::LAND | flags::TANKER;
        w.stats.0[TANKER as usize].can_target = 0;
        w.stats.0[FOOTMAN as usize].can_target = 1;
        (DESTROYER, TANKER)
    }

    /// "Reacting to an attacker" step 1 and "Fleeing" (`human12-25000` transport 1428 at n = 16061,
    /// and the static-study row): step 1 looks at the record as it is, so an idle footman flees from a ship it can't
    /// hit even when its owner doesn't detect the ship. Two flee draws, a move, the attacker forgotten.
    #[test]
    fn an_undetected_attacker_the_unit_cannot_hit_is_fled_from() {
        for owner in [0, 1] {
            let mut w = map();
            let (destroyer, _) = with_ships(&mut w);
            let f = w.spawn(FOOTMAN, owner, (10, 10), 0).unwrap();
            let d = w.spawn(destroyer, 1 - owner, (14, 10), 0).unwrap();
            let (i, di) = (at(&w, f), at(&w, d));
            w.units[di].layer = Layer::Sea;
            w.units[di].detected_by &= !crate::vision::bit(owner);
            assert!(w.target_lost(owner, d), "not detected");
            w.units[i].attacked_by = Some(d);
            let mut r = w.rng;
            r.draw();
            r.draw();
            assert!(w.acquire(i, Acquires::Idle, false), "owner {owner}");
            assert_eq!(w.rng, r, "two flee draws");
            assert!(matches!(w.units[i].order, Order::Move { .. }), "{:?}", w.units[i].order);
            assert_eq!(w.units[i].attacked_by, None);
        }
    }

    /// Step 1 for workers and tankers (`human12-25000` tanker 1425 at n = 16612, and the static-study
    /// rows): a tanker can hit no kind, so with a valid attacker it always runs the flee rule, detected or not. A
    /// computer player's tanker that fails step 1 flees only from idle, and its job choice doesn't run either way; a
    /// computer peon that can hit its attacker passes step 1 and its job choice runs (`ai/worker-ai.md`).
    #[test]
    fn workers_and_tankers_run_step_1_first() {
        let mut w = map();
        let (destroyer, tanker) = with_ships(&mut w);
        let d = w.spawn(destroyer, 1, (14, 10), 0).unwrap();
        let di = at(&w, d);
        w.units[di].layer = Layer::Sea;
        w.units[di].detected_by &= !crate::vision::bit(0);
        let t = w.spawn(tanker, 0, (10, 10), 0).unwrap();
        let ti = at(&w, t);
        w.units[ti].attacked_by = Some(d);
        assert!(
            w.acquire(ti, Acquires::Idle, false),
            "a human tanker flees from an undetected ship"
        );
        assert!(matches!(w.units[ti].order, Order::Move { .. }));
        assert_eq!(w.units[ti].attacked_by, None);
        // A computer player's tanker, not idle: no flee, and the job choice doesn't run (the run is over).
        let g = w.spawn(FOOTMAN, 0, (3, 3), 0).unwrap();
        let c = w.spawn(tanker, 1, (5, 15), 0).unwrap();
        let ci = at(&w, c);
        let busy = Order::Move {
            goal: (20, 20),
            target: None,
        };
        w.units[ci].order = busy;
        w.units[ci].attacked_by = Some(g);
        let rng = w.rng;
        assert!(w.acquire(ci, Acquires::Idle, false));
        assert_eq!((w.units[ci].order, w.rng), (busy, rng), "no flee");
        // Idle, it flees.
        w.units[ci].order = Order::Idle;
        assert!(w.acquire(ci, Acquires::Idle, false));
        assert!(matches!(w.units[ci].order, Order::Move { .. }));
        // A computer peon hit by a footman: it can hit land, so step 1 passes and its job choice runs.
        let p = w.spawn(PEASANT, 1, (8, 18), 0).unwrap();
        let pi = at(&w, p);
        w.units[pi].attacked_by = Some(g);
        assert!(!w.acquire(pi, Acquires::Idle, false));
        assert_eq!((w.units[pi].order, w.units[pi].attacked_by), (Order::Idle, Some(g)));
    }

    /// "What a patrol remembers": in Enhanced mode each turnaround remembers its own goal and the tile
    /// the unit stands on, even when nothing was remembered before.
    #[test]
    fn a_patrol_turnaround_remembers_its_goal_and_tile() {
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 1, (2, 2), 0).unwrap();
        let i = at(&w, f);
        let out = Order::Patrol {
            to: (5, 2),
            back: (9, 9),
        };
        w.units[i].remembered = None;
        w.take_over(i, out);
        let mut n = 0;
        while order_of(&w, f) == out {
            w.step(&[]);
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(
            w.units[at(&w, f)].remembered,
            Some(Remembered::Patrol {
                to: (9, 9),
                back: (5, 2)
            })
        );
    }

    /// Fog spec, "When sight is applied", item 4 (the `human06-31357` vector in small): each strike
    /// of an attack-type order on another player's unit reveals a sight-2 area at the attacker's tile for the
    /// target's owner, before the strike. Not for a target of the attacker's own player, nor in order 14.
    #[test]
    fn a_strike_reveals_the_attacker_to_its_targets_owner() {
        for (case, reveals) in [("enemy", true), ("own", false), ("tower", false)] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (2, 2), 0).unwrap();
            let owner = if case == "own" { 0 } else { 1 };
            let g = w.spawn(FOOTMAN, owner, (20, 20), 0).unwrap();
            let gi = at(&w, g);
            assert!(w.vision.unseen_by(0, 20, 20));
            w.units[gi].order = if case == "tower" {
                Order::TowerWatch { target: Some(f) }
            } else {
                Order::Attack { target: f }
            };
            w.strike(gi);
            for (t, inside) in [((20, 20), true), ((18, 20), true), ((21, 21), true), ((23, 20), false)] {
                assert_eq!(!w.vision.unseen_by(0, t.0, t.1), reveals && inside, "{case} at {t:?}");
            }
            w.unit_visibility();
            assert_eq!(w.units[gi].hidden_from & crate::vision::bit(0) == 0, reveals, "{case}");
        }
    }

    /// "Attacking without moving" (static study): in order 16 a new target out of range is taken
    /// while the unit is still cooling down (order 16 on it, the cooldown left as the wait); with no cooldown left the
    /// out-of-range rule applies (idle takes over, wait 3).
    #[test]
    fn in_order_16_the_cooldown_wins_over_the_out_of_range_rule() {
        for cooling in [true, false] {
            // A computer player's footman searches its reaction range; a peasant that can't attack gives order 16.
            let mut w = map();
            w.stats.0[PEASANT as usize].flags &= !flags::CAN_ATTACK;
            let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
            let old = w.spawn(PEASANT, 0, (20, 20), 0).unwrap();
            let g = w.spawn(PEASANT, 0, (5, 8), 0).unwrap();
            w.unit_visibility();
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.pending = None;
            u.order = Order::IdleReact { target: old };
            u.cooldown = if cooling { 40 } else { 0 };
            u.script.wait = 1;
            assert!(w.run_order(i));
            let u = &w.units[i];
            if cooling {
                assert_eq!(
                    (u.order, u.tile, u.script.anim, u.script.frame, u.script.wait),
                    (Order::IdleReact { target: g }, (5, 5), anim::ATTACK, 0, 40)
                );
            } else {
                assert_eq!((u.order, u.script.wait), (Order::Idle, 3));
            }
        }
    }

    /// "Arriving and losing the target" (static study): in Classic mode a unit in order 10 at its
    /// move goal skips the whole move step, its reaction included, and arrives. Away from the goal it reacts.
    #[test]
    fn classic_attack_move_at_its_goal_arrives_without_reacting() {
        for at_goal in [true, false] {
            let mut w = map();
            w.rules.enhanced = false;
            w.players[1].controller = controller::NOBODY;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.unit_visibility();
            let gi = at(&w, g);
            w.units[gi].hidden_from = 0;
            let i = at(&w, f);
            let goal = if at_goal { (5, 5) } else { (15, 5) };
            let u = &mut w.units[i];
            u.order = Order::AttackMove { goal };
            u.pending = None;
            u.attacked_by = Some(g);
            assert!(w.run_order(i));
            let u = &w.units[i];
            if at_goal {
                assert_eq!((u.order, u.attacked_by, u.script.wait), (Order::Idle, None, 3));
            } else {
                assert!(matches!(target_of(u.order), Some(t) if t == g), "{:?}", u.order);
            }
        }
    }

    /// "Forgetting a dying attacker first": a unit can move if its type has a movement-class flag
    /// (the five lowest type flag bits). A type with none, building or not, keeps a dying attacker's record.
    #[test]
    fn only_a_type_with_a_movement_class_flag_forgets_a_dying_attacker() {
        for (class, forgets) in [
            (flags::LAND, true),
            (flags::SEA, true),
            (flags::CRITTER, true),
            (0, false),
        ] {
            let mut w = map();
            w.stats.0[FOOTMAN as usize].flags = class | flags::CAN_ATTACK;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let a = w.spawn(ARCHER, 1, (5, 7), 0).unwrap();
            let (i, ai) = (at(&w, f), at(&w, a));
            w.units[ai].dying = true;
            w.units[i].attacked_by = Some(a);
            w.forget_dying_attacker(i);
            assert_eq!(w.units[i].attacked_by.is_none(), forgets, "flags {class:#x}");
        }
    }

    /// "A candidate is skipped if" and damage spec, "Missile weapon kinds": a candidate is inside the
    /// minimum range when d < minimum flight − 1. Catapults and ballistas (3) skip d = 1 but not d = 2 (`orc5-40000`
    /// catapult 1580 at n = 21489, d = 2: picked); cannons (2) and missiles with no value skip nothing.
    #[test]
    fn the_minimum_range_is_the_minimum_flight_minus_1() {
        use crate::combat::missile;
        let mut w = map();
        let s = w.spawn(CATAPULT, 0, (5, 5), 0).unwrap();
        let base = w.unit(s).unwrap().clone();
        let at = |tile: (i32, i32)| {
            let mut t = base.clone();
            t.tile = tile;
            t
        };
        for (m, d, skipped) in [
            (missile::CATAPULT, 1, true),
            (missile::CATAPULT, 2, false),
            (missile::BALLISTA, 1, true),
            (missile::BALLISTA, 2, false),
            (missile::SMALL_CANNON, 1, false),
            (missile::BIG_CANNON, 1, false),
            (0x0f, 1, false),
        ] {
            w.stats.0[CATAPULT as usize].missile = m;
            assert_eq!(
                w.inside_min_range(&base, &at((5 + d, 5))),
                skipped,
                "missile {m:#x}, d = {d}"
            );
        }
    }

    /// "A candidate is skipped if" (`orc12-30000` n = 26982 and the static
    /// row): a transport standing on shore water isn't skipped for its kind by a searcher that can target land or sea;
    /// off the shore a land-only searcher still skips it. The check is the attack command's, in full.
    #[test]
    fn a_transport_on_shore_water_is_a_candidate_for_a_land_searcher() {
        const BOAT: u8 = 0x1c;
        let mut w = map();
        w.stats.0[BOAT as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[BOAT as usize].flags = flags::SEA | flags::TRANSPORT;
        w.stats.0[FOOTMAN as usize].can_target = 1;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let footman = w.unit(f).unwrap().clone();
        let boat = |tile: (i32, i32)| {
            let mut t = footman.clone();
            (t.kind, t.layer, t.tile) = (BOAT, Layer::Sea, tile);
            t
        };
        w.terrain.set_bits(12, 12, crate::terrain::bits::SHORE);
        assert!(!w.can_target_kind(&footman, &boat((12, 12))));
        assert!(w.acquire_kind(&footman, &boat((12, 12))));
        assert!(!w.acquire_kind(&footman, &boat((10, 10))));
        // A searcher that can target neither land nor sea (air only) still skips it.
        w.stats.0[FOOTMAN as usize].can_target = 4;
        assert!(!w.acquire_kind(&footman, &boat((12, 12))));
    }

    /// "Attack-move" (the `orc5-40000` footman 1485 vector at n = 33260): while order 4 keeps a target
    /// unit, its walk toward its stored goal uses the target's widened footprint (where it stands now) as the goal
    /// zone, G staying the stored goal. Without a target the zone is the 3 x 3 around the goal; here (a walled-in goal)
    /// the two routes differ.
    #[test]
    fn order_4_with_a_target_walks_with_the_targets_goal_zone() {
        let mut steps = vec![];
        for with_target in [true, false] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (3, 10), 0).unwrap();
            let t = w.spawn(FOOTMAN, 0, (8, 10), 0).unwrap();
            // A ring of obstacles around the 3 x 3 tiles of goal (12, 10).
            for x in 10..=14 {
                for y in 8..=12 {
                    if x == 10 || x == 14 || y == 8 || y == 12 {
                        w.terrain.set_bits(x, y, crate::terrain::bits::OBSTACLE);
                    }
                }
            }
            w.step(&[]);
            let i = at(&w, f);
            w.units[i].pending = None;
            w.units[i].order = Order::MoveAttack {
                goal: (12, 10),
                target: with_target.then_some(t),
            };
            w.units[i].script.wait = 0;
            let zone = with_target.then(|| crate::battle::goal_zone(w.unit(t).unwrap()));
            let mut walk = w.clone();
            walk.move_toward(
                i,
                Goal {
                    zone,
                    ..Goal::tile((12, 10), 0)
                },
            );
            w.run_order(i);
            let u = &w.units[i];
            assert!(matches!(u.order, Order::MoveAttack { .. }), "{:?}", u.order);
            assert_eq!((u.tile, &u.path), (walk.units[i].tile, &walk.units[i].path));
            steps.push((u.tile, u.path.clone()));
        }
        assert_ne!(steps[0], steps[1], "the zone makes a different route");
    }

    /// "Attack-move": a unit target picked by an attack-move is headed for at its tile with the
    /// target's goal zone (its footprint widened by one tile), not the 3 x 3 around its tile.
    #[test]
    fn a_picked_unit_target_is_headed_for_with_its_goal_zone() {
        let mut w = map();
        let a = w.spawn(FOOTMAN, 0, (3, 3), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (9, 3), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        w.step(&[]);
        let i = at(&w, a);
        w.units[i].order = Order::AttackMove { goal: (3, 20) };
        let mut zone_walk = w.clone();
        let goal = Goal {
            zone: Some(crate::battle::goal_zone(w.unit(g).unwrap())),
            ..Goal::tile((9, 3), 0)
        };
        assert_eq!(goal.zone, Some((8, 2, 10, 4)));
        w.head_for(i, g);
        zone_walk.move_toward(i, goal);
        assert_eq!(w.units[i].path, zone_walk.units[i].path);
        assert_eq!(w.units[i].tile, zone_walk.units[i].tile);
    }

    /// Pathfinding spec "Step 1" (static study vector): a follow (order 6) of a 2 x 2 unit walks
    /// with the followed unit's footprint widened by one tile as its goal zone, not the 3 x 3 around its tile.
    #[test]
    fn a_follow_walks_with_the_targets_goal_zone() {
        const BIG: u8 = 0x10;
        let mut w = map();
        w.stats.0[BIG as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[BIG as usize].size = (2, 2);
        let a = w.spawn(FOOTMAN, 0, (3, 3), 0).unwrap();
        let b = w.spawn(BIG, 0, (12, 3), 0).unwrap();
        let i = at(&w, a);
        w.units[i].order = Order::Follow { target: b };
        let mut zone_walk = w.clone();
        let goal = Goal {
            zone: Some(crate::battle::goal_zone(w.unit(b).unwrap())),
            ..Goal::tile((12, 3), 1)
        };
        assert_eq!(goal.zone, Some((11, 2, 14, 5)));
        w.follow(i, b);
        zone_walk.move_toward(i, goal);
        assert_eq!(w.units[i].path, zone_walk.units[i].path);
        assert_eq!(w.units[i].tile, zone_walk.units[i].tile);
    }

    /// Vectors (`orc9-orc09-attack-move-1500` ogre 1493): an attack-move that arrives with the countdown at 0
    /// is complete and forgotten, so a later pending idle 3 tiles from its target tile (losing a target) doesn't
    /// resume it.
    #[test]
    fn a_complete_attack_move_is_forgotten_at_countdown_0() {
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        let c = Command::AttackMove {
            player: 0,
            units: vec![f],
            to: (5, 6),
        };
        let o = w.command_order(i, &c).unwrap();
        w.take_over(i, o);
        assert_eq!(w.units[i].remembered, Some(Remembered::AttackMove { goal: (5, 6) }));
        // Arriving makes idle pending with the countdown at 0: the attack-move is complete and forgotten.
        w.idle_now(i);
        assert_eq!((w.units[i].order, w.units[i].remembered), (Order::Idle, None));
        // 3 tiles away, a lost target makes idle pending again: nothing resumes.
        w.units[i].tile = (5, 9);
        w.lose_target(i);
        let u = &w.units[i];
        assert_eq!((u.order, u.script.anim, u.script.wait), (Order::Idle, anim::STILL, 3));
    }

    /// Static study vector: a complete attack-move with the countdown running is kept, and a later
    /// pending idle at countdown 0 resumes it once the unit is more than 1 tile away.
    #[test]
    fn a_complete_attack_move_is_kept_while_the_countdown_runs() {
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].remembered = Some(Remembered::AttackMove { goal: (5, 6) });
        w.units[i].resume = 5;
        w.give(i, Order::Idle);
        assert_eq!(w.units[i].pending, Some(Order::Idle));
        assert_eq!(w.units[i].remembered, Some(Remembered::AttackMove { goal: (5, 6) }));
        w.units[i].resume = 0;
        w.units[i].tile = (5, 9);
        w.give(i, Order::Idle);
        assert_eq!(w.units[i].pending, Some(Order::AttackMove { goal: (5, 6) }));
        assert_eq!(w.units[i].resume, RESUME_ATTACK_MOVE);
    }

    /// Static study vector: a unit that resumed (countdown 20) and is given a new attack-move can
    /// resume it at its next pending idle, because remembering sets the countdown to 0. The countdown runs only while
    /// something is remembered.
    #[test]
    fn remembering_sets_the_countdown_to_0_and_it_runs_only_while_remembered() {
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].resume = RESUME_ATTACK_MOVE;
        let c = Command::AttackMove {
            player: 0,
            units: vec![f],
            to: (15, 5),
        };
        w.command_order(i, &c).unwrap();
        assert_eq!(w.units[i].resume, 0);
        w.give(i, Order::Idle);
        assert_eq!(w.units[i].pending, Some(Order::AttackMove { goal: (15, 5) }));
        // With nothing remembered the countdown stays where it is.
        let mut w = map();
        w.rules.enhanced = true;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = at(&w, f);
        w.units[i].resume = 7;
        for _ in 0..20 {
            w.step(&[]);
        }
        assert_eq!(w.units[i].resume, 7);
        w.units[i].remembered = Some(Remembered::AttackMove { goal: (15, 5) });
        for _ in 0..20 {
            w.step(&[]);
        }
        assert_eq!(w.units[i].resume, 0);
    }

    /// Vector (`orc9-orc09-tanker-move-2200` juggernaut 1501, n = 614): a sea or air follower's radius is 1
    /// plus the +1, so it is inside at 2 tiles; one whose target can't be guarded gets a pending idle that takes over
    /// in the same update (still animation, wait 3), not a stop command (wait 1).
    #[test]
    fn a_flying_follower_is_inside_at_2_tiles_and_gets_a_pending_idle() {
        const FLYER: u8 = 0x10;
        let mut w = map();
        w.stats.0[FLYER as usize] = w.stats.0[FOOTMAN as usize];
        w.stats.0[FLYER as usize].flags = flags::AIR | flags::CAN_ATTACK;
        let f = w.spawn(FLYER, 0, (4, 4), 0).unwrap();
        let g = w.spawn(FLYER, 0, (6, 4), 0).unwrap();
        let (i, gi) = (at(&w, f), at(&w, g));
        w.units[i].order = Order::Follow { target: g };
        w.units[gi].order = Order::IdleReact { target: g };
        w.units[i].script.wait = 9;
        assert!(!w.follow(i, g));
        let u = &w.units[i];
        assert_eq!(u.tile, (4, 4), "it doesn't move");
        assert_eq!((u.order, u.pending), (Order::Idle, None));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
        // A followable target gives the guard at the same distance.
        w.units[i].order = Order::Follow { target: g };
        w.units[gi].order = Order::Idle;
        w.follow(i, g);
        assert_eq!(w.units[i].order, Order::Guard { target: g });
    }

    /// Orders spec ("Smart move"; pathfinding spec, "Goal pulled back"): a smart move with a target unit that falls
    /// through to the move keeps the target, and its stored goal is the target's tile, not pulled back; the walk ends
    /// in the target's goal zone and drops the target with the order.
    #[test]
    fn a_smart_move_onto_a_unit_keeps_it_as_the_moves_target() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (3, 10), 0).unwrap();
        // Its own farm: not a follow (a building), so the move.
        let b = w.spawn(FARM, 0, (10, 9), 0).unwrap();
        let i = at(&w, f);
        let c = Command::Smart {
            player: 0,
            units: vec![f],
            target: Some(b),
            tile: (11, 10),
        };
        let o = w.command_order(i, &c).unwrap();
        assert_eq!(
            o,
            Order::Move {
                goal: (10, 9),
                target: Some(b)
            }
        );
        w.command(i, o);
        assert_eq!(w.units[i].pending, Some(o), "the farm's tile, not pulled back");
        assert_eq!(w.units[i].recorded_target(), Some(b));
        let mut arrived = false;
        for _ in 0..400 {
            w.step(&[]);
            let u = &w.units[i];
            if u.order == Order::Idle && u.pending.is_none() {
                // Next to the farm, in its goal zone.
                assert_eq!(u.tile.0, 9, "{:?}", u.tile);
                assert_eq!(u.recorded_target(), None);
                arrived = true;
                break;
            }
        }
        assert!(arrived);
    }

    /// Vectors (`orc9-orc09-smart-target-2100` peon 1478, n = 1877; the inside and undetected cases static
    /// study): a smart move whose target unit is dying, inside something or undetected at a step boundary ends as on
    /// arrival: order 2, target dropped, attacker forgotten, still animation, wait 3. It doesn't walk on.
    #[test]
    fn a_smart_move_losing_its_target_ends_as_on_arrival() {
        type Lose = fn(&mut World, usize);
        let cases: [(&str, Lose); 3] = [
            ("dying", |w, bi| w.kill(bi)),
            ("inside", |w, bi| w.units[bi].inside = true),
            ("undetected", |w, bi| w.units[bi].detected_by = 0),
        ];
        for (what, lose) in cases {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (3, 10), 0).unwrap();
            let b = w.spawn(FARM, 0, (14, 9), 0).unwrap();
            let a = w.spawn(ARCHER, 1, (3, 14), 0).unwrap();
            let i = at(&w, f);
            w.units[i].order = Order::Move {
                goal: (14, 9),
                target: Some(b),
            };
            w.units[i].attacked_by = Some(a);
            w.start_anim(i, anim::MOVE);
            let bi = at(&w, b);
            lose(&mut w, bi);
            let i = at(&w, f);
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!((u.order, u.pending, u.tile), (Order::Idle, None, (3, 10)), "{what}");
            assert_eq!(
                (u.script.anim, u.script.wait, u.attacked_by),
                (anim::STILL, 3, None),
                "{what}"
            );
            assert_eq!(u.recorded_target(), None, "{what}");
        }
    }

    /// Vectors (`orc9-orc09-flee-chop-walk-900` and `orc9-orc09-flee-walk-1200` peon 1478): a lumber worker
    /// hit while walking to its tree keeps walking with its attacker recorded; when it reaches the tree, still in its
    /// move animation, the arrival switch and the reaction make it flee at once: order 3, still animation, wait 3.
    #[test]
    fn a_lumber_worker_flees_only_when_its_walk_has_ended() {
        let mut w = map();
        let p = w.spawn(PEASANT, 0, (5, 10), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (5, 4), 0).unwrap();
        let i = at(&w, p);
        w.units[i].gather = crate::world::Gather::Lumber;
        w.units[i].order = Order::Harvest {
            source: None,
            tree: (12, 10),
            chopping: false,
        };
        w.units[i].attacked_by = Some(a);
        let rng = w.rng;
        w.harvest_order(i, None, (12, 10), false);
        let u = &w.units[i];
        assert_eq!((u.tile, u.script.anim), ((6, 10), anim::MOVE), "a step");
        assert_eq!((u.attacked_by, w.rng), (Some(a), rng), "no reaction while walking");
        // It reaches the tree's side: the walk ends.
        let u = &mut w.units[i];
        u.tile = (11, 10);
        u.path.clear();
        w.harvest_order(i, None, (12, 10), false);
        let u = &w.units[i];
        assert!(matches!(u.order, Order::Move { target: None, .. }), "{:?}", u.order);
        assert_eq!((u.pending, u.script.anim, u.script.wait), (None, anim::STILL, 3));
        let mut two = rng;
        two.draw();
        two.draw();
        assert_eq!(w.rng, two, "the flee's two draws");
    }

    /// Vector (static study): a human player's worker repairing (order 27) hit while walking to the building
    /// doesn't react; it flees at its first logic run after it arrives.
    #[test]
    fn a_repairer_reacts_only_after_it_arrives() {
        let mut w = map();
        let p = w.spawn(PEASANT, 0, (3, 10), 0).unwrap();
        let farm = w.spawn(FARM, 0, (14, 9), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (3, 4), 0).unwrap();
        let fi = at(&w, farm);
        w.units[fi].hp = 1;
        let i = at(&w, p);
        w.units[i].order = Order::Repair { target: farm };
        w.units[i].attacked_by = Some(a);
        w.repair_order(i, farm);
        let u = &w.units[i];
        assert_eq!((u.order, u.attacked_by), (Order::Repair { target: farm }, Some(a)));
        let u = &mut w.units[i];
        u.tile = (13, 10);
        u.path.clear();
        w.repair_order(i, farm);
        assert!(
            matches!(w.units[i].order, Order::Move { target: None, .. }),
            "{:?}",
            w.units[i].order
        );
    }

    /// Orders spec ("Commands forget a stale attacker"; `orc9-orc09-return-nodepot-2200` peon 1482, n = 1374): a
    /// command's target unit that is inside something fails the command check: the target is dropped and idle becomes
    /// the pending order; the unit keeps its order.
    #[test]
    fn a_target_inside_something_fails_the_command_check() {
        let mut w = map();
        let f = w.spawn(FOOTMAN, 0, (3, 10), 0).unwrap();
        let g = w.spawn(FOOTMAN, 1, (6, 10), 0).unwrap();
        let gi = at(&w, g);
        w.units[gi].inside = true;
        let i = at(&w, f);
        w.command(i, Order::Attack { target: g });
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.pending, u.recorded_target()),
            (Order::Idle, Some(Order::Idle), None)
        );
        assert!(w.target_lost(0, g) && !w.stale(0, g));
    }

    /// A footman following its own peasant, an enemy archer that hit it 4 tiles away, everyone seen.
    fn attacked_follower() -> (World, UnitId, UnitId, UnitId) {
        let mut w = map();
        w.players[1].controller = controller::NOBODY;
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let p = w.spawn(PEASANT, 0, (6, 5), 0).unwrap();
        let a = w.spawn(ARCHER, 1, (9, 5), 0).unwrap();
        for u in &mut w.units {
            u.hidden_from = 0;
        }
        let i = at(&w, f);
        let u = &mut w.units[i];
        u.order = Order::Follow { target: p };
        u.pending = None;
        u.attacked_by = Some(a);
        w.start_anim(i, anim::MOVE);
        (w, f, p, a)
    }

    /// Vectors (`orc9-orc09-follow-attacked-1200` grunt 1481, n = 314 to 493): a follower hit while it walks
    /// doesn't react. Once next to the followed unit, the reaction runs but its order 12 is replaced: the follow is
    /// given again toward the unit the search picked and takes over at once (order 6, still animation, wait 3).
    #[test]
    fn an_attacked_follower_follows_the_unit_its_reaction_picked() {
        let (mut w, f, p, a) = attacked_follower();
        // Walking: two tiles from the peasant, only the walk runs.
        let pi = at(&w, p);
        let unit = w.units[pi].clone();
        w.mark(&unit, unit.tile, None);
        w.units[pi].tile = (8, 8);
        let unit = w.units[pi].clone();
        w.mark(&unit, unit.tile, Some(p));
        let i = at(&w, f);
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!((u.order, u.attacked_by), (Order::Follow { target: p }, Some(a)));
        assert_eq!(u.script.anim, anim::MOVE);
        // In reach.
        let (mut w, f, _, a) = attacked_follower();
        let i = at(&w, f);
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!((u.order, u.pending), (Order::Follow { target: a }, None));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
        assert_eq!(u.recorded_target(), Some(a));
    }

    /// Vectors (`orc9-orc09-follow-attacked-1200` grunt 1481, n = 560): a follower that reaches the unit its
    /// reaction picked, whose order is 12 (not followable), gets a pending idle taken over at once (still animation,
    /// wait 3). The search picks its current target, so the reaction changes nothing.
    #[test]
    fn a_follower_next_to_the_attacker_it_picked_goes_idle() {
        let (mut w, f, _, a) = attacked_follower();
        let i = at(&w, f);
        w.units[i].order = Order::Follow { target: a };
        let ai = at(&w, a);
        let unit = w.units[ai].clone();
        w.mark(&unit, unit.tile, None);
        w.units[ai].tile = (6, 6);
        w.units[ai].order = Order::Chase { target: f };
        let unit = w.units[ai].clone();
        w.mark(&unit, unit.tile, Some(a));
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!((u.order, u.pending), (Order::Idle, None));
        assert_eq!((u.script.anim, u.script.wait), (anim::STILL, 3));
    }

    /// Vectors (static study): a follower whose reaction's search picks an idle unit already within the
    /// radius guards it (order 7).
    #[test]
    fn a_follower_whose_reaction_picks_an_idle_unit_in_reach_guards_it() {
        let (mut w, f, _, a) = attacked_follower();
        let ai = at(&w, a);
        let unit = w.units[ai].clone();
        w.mark(&unit, unit.tile, None);
        w.units[ai].tile = (4, 4);
        w.units[ai].order = Order::Idle;
        let unit = w.units[ai].clone();
        w.mark(&unit, unit.tile, Some(a));
        let i = at(&w, f);
        assert!(w.run_order(i));
        assert_eq!(w.units[i].order, Order::Guard { target: a });
    }

    /// Vectors (static study): a follower with an attacker whose route search finds no route becomes idle as
    /// an immediate order and its idle logic runs, not order 6's reaction: the idle logic's take-over of order 12
    /// (wait 3), not the reaction's immediate order 12 (wait 1).
    #[test]
    fn a_follower_with_no_route_goes_idle_at_once() {
        let (mut w, f, p, a) = attacked_follower();
        let pi = at(&w, p);
        let unit = w.units[pi].clone();
        w.mark(&unit, unit.tile, None);
        w.units[pi].tile = (12, 5);
        let unit = w.units[pi].clone();
        w.mark(&unit, unit.tile, Some(p));
        // The follower is walled in.
        for y in 4..=6 {
            for x in 4..=6 {
                if (x, y) != (5, 5) {
                    let n = w.terrain.at(x, y).unwrap();
                    w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
                }
            }
        }
        let i = at(&w, f);
        assert!(w.run_order(i));
        let u = &w.units[i];
        assert_eq!(
            (u.order, u.script.anim, u.script.wait),
            (Order::Chase { target: a }, anim::ATTACK, 3)
        );
    }

    /// Vector (`human4-15000` peon 1531, n = 638): the arrival switch has no attacker condition. A worker
    /// with no attacker reaching its tree in its move animation is switched to the chop animation once.
    #[test]
    fn a_worker_reaching_its_tree_starts_chopping_once() {
        let mut w = map();
        let p = w.spawn(PEASANT, 0, (5, 10), 0).unwrap();
        let n = w.terrain.at(6, 10).unwrap();
        w.terrain.regions[n] = crate::terrain::region::FOREST;
        w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
        let i = at(&w, p);
        w.units[i].gather = crate::world::Gather::Lumber;
        w.units[i].order = Order::Harvest {
            source: None,
            tree: (6, 10),
            chopping: false,
        };
        w.start_anim(i, anim::MOVE);
        let mut once = w.clone();
        once.switch_anim(i, anim::ATTACK);
        w.harvest_order(i, None, (6, 10), false);
        let u = &w.units[i];
        assert!(
            matches!(u.order, Order::Harvest { chopping: true, .. }),
            "{:?}",
            u.order
        );
        assert_eq!(u.script, once.units[i].script);
    }

    /// Vector, traced (`orc9-orc09-order16-keep-2000` grunt 1492, n = 937): a unit in order 16
    /// still cooling down whose search, with an attacker, gives order 12 on another target keeps order 16 on it,
    /// waiting for the cooldown left; with no cooldown left order 12 takes over with wait 3.
    #[test]
    fn order_16_cooling_down_keeps_its_order_when_the_search_gives_order_12() {
        for cooling in [true, false] {
            let mut w = map();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let old = w.spawn(FOOTMAN, 1, (5, 4), 0).unwrap();
            let g = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let i = at(&w, f);
            let u = &mut w.units[i];
            u.pending = None;
            u.order = Order::IdleReact { target: old };
            u.attacked_by = Some(g);
            u.cooldown = if cooling { 23 } else { 0 };
            u.script.wait = 1;
            assert!(w.run_order(i));
            let u = &w.units[i];
            if cooling {
                assert_eq!(
                    (u.order, u.pending, u.script.anim, u.script.frame, u.script.wait),
                    (Order::IdleReact { target: g }, None, anim::ATTACK, 0, 23)
                );
            } else {
                assert_eq!((u.order, u.script.wait), (Order::Chase { target: g }, 3));
            }
        }
    }

    /// `net/leave-and-drop.md` LV20 and LV21 (traced, round 93): a "nobody" player's idle combat unit and tower take no
    /// target and don't strike back. Automatic target acquisition skips every candidate hidden from the searcher's
    /// owner, and every unit is always hidden from a "nobody" player (`vision/fog.md`, "Unit visibility (phase 5)").
    #[test]
    fn a_nobody_players_idle_units_and_towers_find_no_target() {
        let mut w = map();
        // LV21: an enemy footman next to the nobody player's idle footman, which it strikes.
        let foe = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let grunt = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
        // LV20: an enemy footman walking (a move, not an attack) through the nobody player's tower's reach.
        let walker = w.spawn(FOOTMAN, 0, (18, 7), 0).unwrap();
        // A guard tower (0x60): the test tower type's owner is forced to the neutral player.
        w.stats.0[0x60] = w.stats.0[TOWER as usize];
        let tower = w.spawn(0x60, 1, (18, 12), 0).unwrap();
        w.players[1].controller = controller::NOBODY;
        let hp = w.unit(walker).unwrap().hp;
        let mut struck = false;
        for k in 0..120 {
            let go = Command::Move {
                player: 0,
                units: vec![walker],
                to: (18, 17),
            };
            w.step(if k == 0 { std::slice::from_ref(&go) } else { &[] });
            assert_eq!(
                order_of(&w, grunt),
                Order::Idle,
                "the grunt takes no target and no reaction"
            );
            assert!(
                matches!(order_of(&w, tower), Order::Idle | Order::TowerWatch { target: None }),
                "the tower takes no target"
            );
            assert!(
                w.missiles.iter().all(Option::is_none),
                "the tower launches nothing at {k}: {:?}",
                w.missiles
                    .iter()
                    .flatten()
                    .map(|m| (m.kind, m.attacker, m.target))
                    .collect::<Vec<_>>()
            );
            assert_eq!(w.unit(walker).unwrap().hp, hp, "the walker is not hurt");
            if k == 60 {
                let y = w.unit(walker).unwrap().tile.1;
                assert!((8..=16).contains(&y), "the walker is inside the tower's reach at {y}");
            }
            struck |= w.unit(grunt).unwrap().hp < w.stats_of(FOOTMAN).hit_points;
        }
        assert!(
            struck,
            "the enemy footman takes the idle grunt as a target and strikes it"
        );
        assert_ne!(order_of(&w, foe), Order::Idle);
    }

    /// Pins of the current behaviour where no oracle trace reaches (the coverage report's untraced order regions) and
    /// on the paths the B-038 refactor merges, written before it so that it can't change them unseen. Each test states
    /// the outcome in a few fields and pins the whole simulation state with [`World::fingerprint`]. Where a pin
    /// differs from the spec's wording, its comment says so: those are open questions, not rules.
    mod pin {
        use super::*;

        const FLYER: u8 = 2;
        const MAGE: u8 = 0x0a;

        /// [`map`] with player 1 passive (no AI) and one update run, so that every unit is detected and seen.
        fn arena(units: &[(u8, u8, (i32, i32))]) -> (World, Vec<UnitId>) {
            let mut w = map();
            let ids = units.iter().map(|&(k, o, t)| w.spawn(k, o, t, 0).unwrap()).collect();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            (w, ids)
        }

        /// Puts unit `id` in order `o` with nothing pending, ready for a logic run.
        fn ready(w: &mut World, id: UnitId, o: Order) -> usize {
            let i = at(w, id);
            let u = &mut w.units[i];
            (u.order, u.pending, u.commanded) = (o, None, false);
            u.script.wait = 1;
            i
        }

        /// The whole state after `n` updates.
        fn after(w: &mut World, n: u32) -> u64 {
            run(w, n);
            w.fingerprint()
        }

        #[test]
        fn an_attack_command_on_itself_or_by_a_unit_that_cant_attack_gives_nothing() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FLYER, 0, (7, 7))]);
            let (f, b) = (ids[0], ids[1]);
            let i = at(&w, f);
            let c = |target| Command::Attack {
                player: 0,
                units: vec![f],
                target,
            };
            assert_eq!(w.command_order(i, &c(f)), None);
            let bi = at(&w, b);
            assert_eq!(w.command_order(bi, &c(f)), None);
            assert_eq!(w.command_order(i, &c(b)), Some(Order::Attack { target: b }));
            assert_eq!(w.fingerprint(), 6214535284775197206);
        }

        /// "Commands forget a stale attacker" is for units that aren't buildings: a building keeps a dead attacker.
        #[test]
        fn a_building_keeps_a_stale_attacker_through_a_command() {
            let (mut w, ids) = arena(&[(FARM, 0, (5, 5)), (FOOTMAN, 1, (9, 9))]);
            let (b, a) = (ids[0], ids[1]);
            let ai = at(&w, a);
            w.kill(ai);
            let bi = at(&w, b);
            w.units[bi].attacked_by = Some(a);
            w.command(bi, Order::Idle);
            assert_eq!(w.units[bi].attacked_by, Some(a));
            assert_eq!(w.fingerprint(), 17583034015400902841);
        }

        /// A marked unit in order 12 whose target isn't held isn't out of reach: it runs its logic (losing the target)
        /// and the mark is cleared.
        #[test]
        fn a_marked_chase_whose_target_is_lost_isnt_out_of_reach() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (15, 15))]);
            let (f, g) = (ids[0], ids[1]);
            let gi = at(&w, g);
            w.units[gi].detected_by = 0;
            let i = ready(&mut w, f, Order::Chase { target: g });
            w.units[i].skip_search = true;
            assert!(!w.out_of_reach(i));
            w.order(
                i,
                Effects {
                    step_done: true,
                    ..Default::default()
                },
            );
            let u = w.unit(f).unwrap();
            assert_eq!((u.order, u.skip_search, u.script.wait), (Order::Idle, false, 3));
            assert_eq!(w.fingerprint(), 2263607773961175139);
        }

        /// "Follow (orders 6 and 7)": a follower whose target is lost at a step boundary runs its reaction first; the
        /// unit its reaction picks becomes the followed one. Cases: a combat attacker (the reaction's search gives
        /// order 12), a worker hit by a worker (the fight-back gives order 9 pending), and that worker undetected: the
        /// follow re-give is a command with the target check ("What every command does"), which drops it, so the
        /// pending idle takes over in the same update.
        #[test]
        fn a_follower_losing_its_target_follows_what_its_reaction_picks() {
            let mut prints = vec![];
            for case in ["combat", "worker", "undetected worker"] {
                let worker = case != "combat";
                let me = if worker { PEASANT } else { FOOTMAN };
                let (mut w, ids) = arena(&[(me, 0, (5, 5)), (FOOTMAN, 0, (9, 5)), (me, 1, (5, 8))]);
                let (f, friend, a) = (ids[0], ids[1], ids[2]);
                let ti = at(&w, friend);
                w.units[ti].detected_by = 0;
                if case == "undetected worker" {
                    let ai = at(&w, a);
                    w.units[ai].detected_by = 0;
                }
                let i = ready(&mut w, f, Order::Follow { target: friend });
                w.units[i].attacked_by = Some(a);
                w.units[i].blocked = 4;
                w.units[i].path = vec![(6, 5)];
                assert!(w.run_order(i));
                let u = &w.units[i];
                let want = if case == "undetected worker" {
                    Order::Idle
                } else {
                    Order::Follow { target: a }
                };
                assert_eq!((u.order, u.script.wait), (want, 3), "{case}");
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [5828634838243349751, 14060437216325028717, 2277586021291709791]);
        }

        /// Classic attack-move (order 10) runs the reaction in its walk: a recorded attacker in reach gives order 12
        /// at once.
        #[test]
        fn classic_attack_move_reacts_to_its_attacker() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (5, 6))]);
            w.rules.enhanced = false;
            let (f, a) = (ids[0], ids[1]);
            let i = ready(&mut w, f, Order::AttackMove { goal: (20, 20) });
            w.units[i].attacked_by = Some(a);
            assert!(w.run_order(i));
            let u = &w.units[i];
            assert_eq!((u.order, u.script.wait), (Order::Chase { target: a }, 1));
            assert_eq!(after(&mut w, 40), 5888873096768349066);
        }

        /// Order 4 with a target unit sets its goal from the target where it stands now, at every walk.
        #[test]
        fn order_4_with_a_target_rewrites_its_goal() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (2, 2)), (FARM, 1, (14, 14))]);
            let (f, b) = (ids[0], ids[1]);
            let i = ready(
                &mut w,
                f,
                Order::MoveAttack {
                    goal: (20, 20),
                    target: Some(b),
                },
            );
            assert!(w.run_order(i));
            let Order::MoveAttack { goal, target } = w.units[i].order else {
                panic!()
            };
            assert_eq!(target, Some(b));
            assert_ne!(goal, (20, 20));
            assert_eq!(after(&mut w, 60), 12594211297855405489);
        }

        /// "Special cases" (round 71, S1): an Enhanced demolition unit with a remembered order, idle, gives itself
        /// command 19 (order 20) on what its search finds; with nothing found it does nothing more, not even flee from
        /// an attacker it doesn't detect. Without a remembered order, or in Classic mode, it flees as before.
        #[test]
        fn an_enhanced_demolition_unit_demolishes_what_it_finds() {
            const SQUAD: u8 = DEMOLITION[0];
            for (case, enhanced, remembered, enemy_near) in [
                ("finds", true, true, true),
                ("nothing found", true, true, false),
                ("not remembered", true, false, false),
                ("classic", false, true, false),
            ] {
                let near = if enemy_near { (6, 6) } else { (20, 20) };
                let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, near), (FOOTMAN, 1, (14, 10))]);
                w.rules.enhanced = enhanced;
                let si = at(&w, ids[0]);
                w.units[si].kind = SQUAD;
                w.stats.0[SQUAD as usize] = w.stats.0[FOOTMAN as usize];
                let (s, e, a) = (ids[0], ids[1], ids[2]);
                let ai = at(&w, a);
                w.units[ai].detected_by = 0;
                let i = ready(&mut w, s, Order::Idle);
                w.units[i].remembered = remembered.then_some(Remembered::Patrol {
                    to: (2, 2),
                    back: (5, 5),
                });
                w.units[i].attacked_by = Some(a);
                let ran = w.run_order(i);
                let u = &w.units[i];
                match case {
                    "finds" => {
                        assert!(ran, "{case}");
                        let tile = w.unit(e).unwrap().tile;
                        assert_eq!(
                            (u.order, u.pending, u.commanded),
                            (Order::Idle, Some(Order::Demolish { at: Some(e), tile }), true),
                            "{case}"
                        );
                    }
                    "nothing found" => {
                        assert_eq!((u.order, u.pending), (Order::Idle, None), "{case}");
                        assert_eq!(u.attacked_by, Some(a), "no flee: {case}");
                    }
                    _ => {
                        assert!(ran, "{case}");
                        assert!(
                            matches!(u.pending.or(Some(u.order)), Some(Order::Move { .. })),
                            "flees: {case}"
                        );
                    }
                }
            }
        }

        /// "Special cases" (round 72, traced: V279): an Enhanced demolition unit with nothing remembered and no
        /// attacker, walking a move attacking on the way (order 4), takes no target although an enemy stands next to
        /// it; it walks on. In Classic mode the rule doesn't apply and it takes the enemy as before.
        #[test]
        fn an_enhanced_demolition_unit_with_nothing_remembered_takes_no_target() {
            const SQUAD: u8 = DEMOLITION[0];
            for enhanced in [true, false] {
                let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (6, 6))]);
                w.rules.enhanced = enhanced;
                let (s, e) = (ids[0], ids[1]);
                let si = at(&w, s);
                w.units[si].kind = SQUAD;
                w.stats.0[SQUAD as usize] = w.stats.0[FOOTMAN as usize];
                let walk = Order::MoveAttack {
                    goal: (14, 5),
                    target: None,
                };
                let i = ready(&mut w, s, walk);
                w.units[i].remembered = None;
                w.run_order(i);
                let u = &w.units[i];
                if enhanced {
                    assert_eq!((u.order, u.pending, u.commanded), (walk, None, false), "walks on");
                } else {
                    assert_eq!(
                        target_of(u.pending.unwrap_or(u.order)),
                        Some(e),
                        "classic takes the enemy"
                    );
                }
            }
        }

        /// V278 (round 72, traced): an Enhanced demolition unit walking a remembered patrol whose search finds a unit
        /// gets command 19 on it (order 20 pending, order 5 kept), and its patrol walk goes on in the same run: it
        /// steps toward the patrol's end.
        #[test]
        fn a_patrolling_demolition_unit_walks_on_after_its_command() {
            const SQUAD: u8 = DEMOLITION[0];
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (7, 7))]);
            w.rules.enhanced = true;
            let (s, e) = (ids[0], ids[1]);
            let si = at(&w, s);
            w.units[si].kind = SQUAD;
            w.stats.0[SQUAD as usize] = w.stats.0[FOOTMAN as usize];
            let patrol = Order::Patrol {
                to: (5, 12),
                back: (5, 5),
            };
            let i = ready(&mut w, s, patrol);
            w.units[i].remembered = Some(Remembered::Patrol {
                to: (5, 12),
                back: (5, 5),
            });
            w.run_order(i);
            let u = &w.units[i];
            let tile = w.unit(e).unwrap().tile;
            assert_eq!(
                (u.order, u.pending, u.commanded),
                (patrol, Some(Order::Demolish { at: Some(e), tile }), true)
            );
            assert_eq!(u.tile, (5, 6), "steps on toward the patrol's end");
        }

        /// The follow with no target unit returns at once; with the follow pending on a target in an order it can't
        /// follow (step 1) the follower stops where it stands (order 3 pending); with no route it goes idle at once.
        #[test]
        fn follow_edges() {
            let mut prints = vec![];
            // Gone.
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 0, (9, 9))]);
            let gi = at(&w, ids[1]);
            let gone = w.units[gi].id;
            let (slot, owner) = (w.units[gi].slot, w.units[gi].owner);
            w.slots.release(slot, owner);
            w.units.remove(gi);
            w.ylist.retain(|&x| x != gone);
            let i = ready(&mut w, ids[0], Order::Follow { target: gone });
            assert!(!w.follow(i, gone));
            prints.push(w.fingerprint());
            // Step 1.
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 0, (9, 9)), (FOOTMAN, 1, (20, 20))]);
            let ti = ready(&mut w, ids[1], Order::Attack { target: ids[2] });
            let _ = ti;
            let i = ready(&mut w, ids[0], Order::Follow { target: ids[1] });
            w.units[i].pending = Some(Order::Follow { target: ids[1] });
            assert!(w.run_order(i));
            assert_eq!(
                w.units[i].pending,
                Some(Order::Move {
                    goal: (5, 5),
                    target: None
                })
            );
            prints.push(w.fingerprint());
            // No route.
            let mut w = walled_in();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let t = w.spawn(FOOTMAN, 0, (15, 15), 0).unwrap();
            w.step(&[]);
            let i = ready(&mut w, f, Order::Follow { target: t });
            assert!(w.run_order(i));
            assert_eq!((w.units[i].order, w.units[i].script.wait), (Order::Idle, 1));
            prints.push(w.fingerprint());
            // Guard with no route.
            let mut w = walled_in();
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let t = w.spawn(FOOTMAN, 0, (15, 15), 0).unwrap();
            w.step(&[]);
            let i = ready(&mut w, f, Order::Guard { target: t });
            assert!(w.run_order(i));
            assert_eq!((w.units[i].order, w.units[i].script.wait), (Order::Idle, 1));
            prints.push(w.fingerprint());
            assert_eq!(
                prints,
                [
                    8054746818935266047,
                    4153516397773490181,
                    14462624116563959142,
                    14462624116563959142
                ]
            );
        }

        /// "Reacting to an attacker" step 3: an invisible mage-type unit with an attacker does nothing more in the run.
        #[test]
        fn an_invisible_mage_with_an_attacker_does_nothing() {
            let mut w = map();
            w.stats.0[MAGE as usize] = w.stats.0[ARCHER as usize];
            w.stats.0[MAGE as usize].flags |= flags::MAGE_LIKE;
            let m = w.spawn(MAGE, 0, (10, 10), 0).unwrap();
            let a = w.spawn(FOOTMAN, 1, (11, 10), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let i = ready(&mut w, m, Order::Idle);
            w.units[i].attacked_by = Some(a);
            w.units[i].invisibility = 100;
            w.run_order(i);
            assert_eq!((w.units[i].order, w.units[i].attacked_by), (Order::Idle, Some(a)));
            assert_eq!(w.fingerprint(), 11171410153538327769);
        }

        /// The acquiring strike with no target unit decides nothing; one still cooling down restarts its animation.
        #[test]
        fn acquiring_strike_edges() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (5, 6)), (FOOTMAN, 1, (9, 9))]);
            let (f, g, d) = (ids[0], ids[1], ids[2]);
            let di = at(&w, d);
            w.units[di].hp = 0;
            w.kill(di);
            run(&mut w, 30);
            let i = ready(&mut w, f, Order::Chase { target: g });
            let gone = w.unit(d).is_none();
            assert!(!w.acquiring_strike(i, Order::Chase { target: d }, d) || !gone);
            w.units[i].cooldown = 20;
            assert!(w.acquiring_strike(i, Order::Chase { target: g }, g));
            assert_eq!(w.units[i].script.wait, 20);
            assert_eq!(w.fingerprint(), 13272625246071631990);
        }

        /// Fleeing with no attacker recorded does nothing; the reaction forgets a dying attacker.
        #[test]
        fn flee_and_react_edges() {
            let (mut w, ids) = arena(&[(PEASANT, 0, (5, 5)), (FOOTMAN, 1, (5, 7))]);
            let (p, a) = (ids[0], ids[1]);
            let i = ready(&mut w, p, Order::Idle);
            assert!(!w.flee(i));
            let ai = at(&w, a);
            w.kill(ai);
            let i = at(&w, p);
            w.units[i].attacked_by = Some(a);
            assert!(!w.react(i));
            assert_eq!(w.units[i].attacked_by, None);
            assert_eq!(w.fingerprint(), 5947070209261013885);
        }

        /// A released attacker slot with no last type recorded counts as a kind the unit can target.
        #[test]
        fn a_released_attacker_with_no_last_type_can_be_targeted() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (9, 9)), (FOOTMAN, 1, (5, 6))]);
            let (f, d, g) = (ids[0], ids[1], ids[2]);
            let di = at(&w, d);
            let (slot, owner) = (w.units[di].slot, w.units[di].owner);
            w.slots.release(slot, owner);
            w.units.remove(di);
            w.ylist.retain(|&x| x != d);
            let i = ready(&mut w, f, Order::Idle);
            w.units[i].attacked_by = Some(d);
            assert!(w.acquire(i, Acquires::Idle, false));
            assert_eq!(w.units[i].order, Order::Chase { target: g });
            assert_eq!(w.fingerprint(), 15228679685829945375);
        }

        /// The attack order's own checks: a target that fails the kind check gives a pending idle, with the resume step,
        /// taken over in the same update (orders spec, "Attack orders", step 5); with no target unit left `can_strike` is
        /// false.
        #[test]
        fn attack_order_edges() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FLYER, 1, (5, 6))]);
            w.rules.enhanced = true;
            let (f, b) = (ids[0], ids[1]);
            let i = ready(&mut w, f, Order::Chase { target: b });
            w.units[i].remembered = Some(Remembered::Patrol {
                to: (15, 5),
                back: (5, 5),
            });
            assert!(!w.can_strike(&w.units[i], f));
            assert!(!w.can_strike(&w.units[i], b), "an air unit needs a missile");
            w.attack_order(i, b);
            let u = &w.units[i];
            let patrol = Order::Patrol {
                to: (15, 5),
                back: (5, 5),
            };
            assert_eq!(
                (u.order, u.pending, u.script.anim, u.script.wait),
                (patrol, None, anim::STILL, 3)
            );
            assert_eq!(after(&mut w, 5), 15456150756034458482);
        }

        /// Order 12's no-route fallback in the run it guards: no second logic run.
        #[test]
        fn the_fallback_run_guard() {
            let mut w = walled_in();
            w.stats.0[FOOTMAN as usize].attack_range = 4;
            let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
            let far = w.spawn(FOOTMAN, 1, (20, 20), 0).unwrap();
            let near = w.spawn(FOOTMAN, 1, (5, 8), 0).unwrap();
            w.players[1].controller = controller::NOBODY;
            w.step(&[]);
            let mut prints = vec![];
            for guarded in [false, true] {
                let mut w = w.clone();
                let i = ready(&mut w, f, Order::Chase { target: far });
                w.fallback_run = guarded;
                w.attack_order(i, far);
                let u = &w.units[i];
                assert_eq!((u.order, u.skip_search), (Order::Chase { target: near }, true));
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [9683250159747692365, 4209216579335515525]);
        }

        /// Heading for a target no longer in play does nothing.
        #[test]
        fn head_for_a_gone_target() {
            let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (9, 9))]);
            let (f, d) = (ids[0], ids[1]);
            let di = at(&w, d);
            w.units.remove(di);
            w.ylist.retain(|&x| x != d);
            let i = ready(&mut w, f, Order::AttackMove { goal: (20, 20) });
            w.head_for(i, d);
            assert_eq!(w.units[i].order, Order::AttackMove { goal: (20, 20) });
            assert_eq!(w.fingerprint(), 5289077262836675590);
        }

        /// Order 11 at its wall still cooling down restarts its attack animation and waits.
        #[test]
        fn order_11_cooling_down_waits() {
            let mut w = map();
            for y in 3..=5 {
                w.terrain.build_wall(8, y, false, true);
            }
            let f = w.spawn(FOOTMAN, 0, (8, 6), 0).unwrap();
            w.step(&[]);
            let i = ready(&mut w, f, Order::AttackWall { tile: (8, 5) });
            w.units[i].cooldown = 9;
            w.attack_wall_order(i, (8, 5));
            let u = &w.units[i];
            assert_eq!((u.script.anim, u.script.wait), (anim::ATTACK, 9));
            assert_eq!(w.fingerprint(), 1661229256139934324);
        }

        /// The command check: a command naming a lost target leaves a pending idle by the common path. With nothing to
        /// resume the animation and wait are left as they are; a game-logic command to a unit that remembers an order
        /// resumes it with the resume step (still animation, wait 1; orders spec, "Commands forget a stale attacker"
        /// and "Giving up when blocked", static study only).
        #[test]
        fn a_command_naming_a_lost_target_leaves_a_pending_idle() {
            let mut prints = vec![];
            for remembered in [false, true] {
                let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 1, (9, 9))]);
                w.rules.enhanced = true;
                let (f, g) = (ids[0], ids[1]);
                let gi = at(&w, g);
                w.units[gi].detected_by = 0;
                let i = ready(
                    &mut w,
                    f,
                    Order::Move {
                        goal: (20, 5),
                        target: None,
                    },
                );
                w.start_anim(i, anim::MOVE);
                w.units[i].script.wait = 5;
                if remembered {
                    w.units[i].remembered = Some(Remembered::Patrol {
                        to: (15, 5),
                        back: (5, 5),
                    });
                }
                w.command(i, Order::Attack { target: g });
                let u = &w.units[i];
                let expect = if remembered {
                    Order::Patrol {
                        to: (15, 5),
                        back: (5, 5),
                    }
                } else {
                    Order::Idle
                };
                let (a, wait) = if remembered { (anim::STILL, 1) } else { (anim::MOVE, 5) };
                assert_eq!(
                    (u.pending, u.commanded, u.script.anim, u.script.wait),
                    (Some(expect), true, a, wait)
                );
                prints.push(w.fingerprint());
            }
            assert_eq!(prints, [3486153184834880162, 3837238766937421335]);
        }

        /// The patrol turnaround: Enhanced (remembered), Classic (not), and an air unit whose other end is next to it
        /// (unremembered).
        #[test]
        fn the_patrol_turnaround() {
            let mut prints = vec![];
            for (case, enhanced, kind) in [
                ("enhanced", true, FOOTMAN),
                ("classic", false, FOOTMAN),
                ("air", true, FLYER),
            ] {
                let (mut w, ids) = arena(&[(kind, 0, (5, 5)), (FOOTMAN, 1, (22, 22))]);
                w.rules.enhanced = enhanced;
                let (f, a) = (ids[0], ids[1]);
                let back = if kind == FLYER { (6, 6) } else { (2, 2) };
                let ai = at(&w, a);
                w.units[ai].detected_by = 0;
                let i = ready(&mut w, f, Order::Patrol { to: (5, 5), back });
                w.units[i].attacked_by = Some(a);
                w.units[i].blocked = 3;
                w.units[i].resume = 7;
                assert!(w.run_order(i));
                let u = &w.units[i];
                assert!(matches!(u.order, Order::Patrol { .. }), "{case}");
                assert_eq!((u.blocked, u.attacked_by, u.script.wait), (0, None, 3), "{case}");
                prints.push(after(&mut w, 30));
            }
            assert_eq!(prints, [3339823245000380870, 4435197359076703491, 11976303091887342509]);
        }

        /// Acquisition's take-over and the at-once switch: both are commands (blocked counter cleared, route dropped,
        /// a stale attacker forgotten).
        #[test]
        fn acquisition_switches_are_commands() {
            let mut prints = vec![];
            for cooling in [false, true] {
                let (mut w, ids) = arena(&[(ARCHER, 0, (5, 5)), (FOOTMAN, 1, (5, 8)), (FOOTMAN, 1, (20, 20))]);
                let (f, g, old) = (ids[0], ids[1], ids[2]);
                let oi = at(&w, old);
                w.units[oi].detected_by = 0;
                let o = if cooling {
                    Order::AttackInPlace { target: old }
                } else {
                    Order::Idle
                };
                let i = ready(&mut w, f, o);
                w.units[i].attacked_by = Some(old);
                w.units[i].blocked = 3;
                w.units[i].path = vec![(6, 5)];
                w.units[i].cooldown = if cooling { 12 } else { 0 };
                let acquired = w.acquire(i, if cooling { Acquires::Other } else { Acquires::Idle }, false);
                assert!(acquired);
                let u = &w.units[i];
                assert_eq!(target_of(u.order), Some(g));
                assert_eq!((u.blocked, u.path.is_empty(), u.attacked_by), (0, true, None));
                prints.push(after(&mut w, 30));
            }
            assert_eq!(prints, [1443065363198671568, 12195102699214640206]);
        }

        /// The friendly-fire hold makes idle pending by the common path and takes it over in the same update (still
        /// animation, wait 3): a remembered attack-move more than 1 tile away is given again (V270), a complete one is
        /// forgotten (V271), and a remembered patrol is resumed by the same rule.
        #[test]
        fn holding_fire_resumes_or_forgets() {
            let mut prints = vec![];
            let cases = [
                (
                    Remembered::AttackMove { goal: (15, 5) },
                    Order::AttackMove { goal: (15, 5) },
                    true,
                ),
                (Remembered::AttackMove { goal: (6, 5) }, Order::Idle, false),
                (
                    Remembered::Patrol {
                        to: (15, 5),
                        back: (5, 5),
                    },
                    Order::Patrol {
                        to: (15, 5),
                        back: (5, 5),
                    },
                    true,
                ),
            ];
            for (remembered, expected, kept) in cases {
                let (mut w, ids) = arena(&[
                    (CATAPULT, 0, (5, 5)),
                    (FOOTMAN, 1, (5, 10)),
                    (FOOTMAN, 0, (4, 10)),
                    (FOOTMAN, 0, (6, 10)),
                ]);
                w.rules.enhanced = true;
                w.stats.0[CATAPULT as usize].flags |= flags::GROUND_ATTACK;
                let (c, g) = (ids[0], ids[1]);
                let i = ready(&mut w, c, Order::Chase { target: g });
                w.units[i].remembered = Some(remembered);
                assert!(w.run_order(i));
                let u = &w.units[i];
                assert_eq!(
                    (u.order, u.script.anim, u.script.wait, u.pending),
                    (expected, anim::STILL, 3, None)
                );
                assert_eq!(u.remembered.is_some(), kept);
                prints.push(after(&mut w, 30));
            }
            assert_eq!(prints, [8047419496851755040, 2596487782720778595, 10347294597896994698]);
        }

        /// Orders 17 and 18 with no route take the general no-route rule (orders spec, "No route": an immediate idle,
        /// no resume, and the idle logic in the same update, here picking the footman in range); order 18 at its
        /// goal gives order 17 as a command that takes over in the same update (attack animation, wait 3; V269).
        #[test]
        fn attack_ground_edges() {
            let mut prints = vec![];
            for o in [
                Order::AttackGround { tile: (20, 20) },
                Order::AttackGroundMove {
                    tile: (20, 20),
                    goal: (20, 20),
                },
            ] {
                let mut w = walled_in();
                w.rules.enhanced = true;
                let f = w.spawn(CATAPULT, 0, (5, 5), 0).unwrap();
                let near = w.spawn(FOOTMAN, 1, (5, 8), 0).unwrap();
                w.players[1].controller = controller::NOBODY;
                w.step(&[]);
                let i = ready(&mut w, f, o);
                w.units[i].remembered = Some(Remembered::AttackMove { goal: (15, 5) });
                assert!(w.run_order(i));
                let u = &w.units[i];
                assert_eq!((u.order, u.script.wait), (Order::IdleReact { target: near }, 3));
                assert!(u.remembered.is_some());
                prints.push(w.fingerprint());
            }
            let (mut w, ids) = arena(&[(CATAPULT, 0, (5, 5))]);
            let i = ready(
                &mut w,
                ids[0],
                Order::AttackGroundMove {
                    tile: (5, 12),
                    goal: (5, 12),
                },
            );
            assert!(w.run_order(i));
            assert_eq!(w.units[i].order, Order::AttackGround { tile: (5, 12) });
            prints.push(after(&mut w, 30));
            assert_eq!(prints, [420654937339136154, 420654937339136154, 2505695252309601192]);
        }

        /// Blocked row G: an attacked unit blocked next to its goal makes idle pending (with the resume rules).
        #[test]
        fn blocked_next_to_the_goal_while_attacked() {
            let mut prints = vec![];
            for remembered in [false, true] {
                let (mut w, ids) = arena(&[(FOOTMAN, 0, (5, 5)), (FOOTMAN, 0, (5, 6)), (FOOTMAN, 1, (20, 20))]);
                w.rules.enhanced = true;
                let (f, a) = (ids[0], ids[2]);
                let i = ready(
                    &mut w,
                    f,
                    Order::Move {
                        goal: (5, 6),
                        target: None,
                    },
                );
                w.units[i].attacked_by = Some(a);
                w.units[i].path = vec![(5, 6)];
                if remembered {
                    w.units[i].remembered = Some(Remembered::AttackMove { goal: (15, 15) });
                }
                assert!(w.run_order(i));
                let u = &w.units[i];
                assert!(u.pending.is_some(), "{:?}", u.pending);
                prints.push(after(&mut w, 30));
            }
            assert_eq!(prints, [1560584021165354794, 17313268070119048422]);
        }

        /// Whole runs: two small armies of each player kind meeting under move, attack-move, patrol, follow, guard and
        /// stand-ground commands, in both rules modes.
        #[test]
        fn skirmishes() {
            let mut prints = vec![];
            for enhanced in [true, false] {
                for enemy in [controller::COMPUTER, controller::HUMAN] {
                    let mut w = map();
                    w.rules.enhanced = enhanced;
                    let mine: Vec<UnitId> = [
                        (FOOTMAN, (2, 2)),
                        (ARCHER, (3, 2)),
                        (CATAPULT, (2, 3)),
                        (FOOTMAN, (3, 3)),
                        (PEASANT, (4, 2)),
                    ]
                    .iter()
                    .map(|&(k, t)| w.spawn(k, 0, t, 0).unwrap())
                    .collect();
                    let theirs: Vec<UnitId> = [
                        (FOOTMAN, (20, 20)),
                        (ARCHER, (21, 20)),
                        (FOOTMAN, (20, 21)),
                        (ARCHER, (21, 21)),
                        (FOOTMAN, (19, 19)),
                    ]
                    .iter()
                    .map(|&(k, t)| w.spawn(k, 1, t, 0).unwrap())
                    .collect();
                    w.players[1].controller = enemy;
                    w.step(&[
                        Command::AttackMove {
                            player: 0,
                            units: vec![mine[0], mine[1]],
                            to: (20, 20),
                        },
                        Command::Patrol {
                            player: 0,
                            units: vec![mine[2]],
                            to: (12, 12),
                        },
                        Command::Smart {
                            player: 0,
                            units: vec![mine[3]],
                            target: Some(mine[0]),
                            tile: (2, 2),
                        },
                        Command::Smart {
                            player: 0,
                            units: vec![mine[4]],
                            target: Some(mine[2]),
                            tile: (2, 3),
                        },
                        Command::Move {
                            player: 1,
                            units: vec![theirs[0], theirs[1]],
                            to: (8, 8),
                        },
                        Command::Patrol {
                            player: 1,
                            units: vec![theirs[2]],
                            to: (10, 14),
                        },
                        Command::StandGround {
                            player: 1,
                            units: vec![theirs[3]],
                        },
                        Command::Attack {
                            player: 1,
                            units: vec![theirs[4]],
                            target: mine[4],
                        },
                    ]);
                    let mut state = vec![];
                    for _ in 0..4 {
                        state.push(after(&mut w, 150));
                    }
                    prints.push(state);
                }
            }
            assert_eq!(
                prints,
                [
                    [
                        11525183968156132881,
                        6302505224056380727,
                        1466018077931009931,
                        16190976508667506687
                    ],
                    [
                        9211862982836609003,
                        6252542899032683274,
                        6828059071405653345,
                        6283069314672679846
                    ],
                    [
                        1571194208392218786,
                        887300750072471240,
                        15715531239895793397,
                        3705930393885858901
                    ],
                    [
                        10883631827346098924,
                        2335255546591348674,
                        13175772183412894570,
                        17950562395473484039
                    ],
                ]
            );
        }
    }
}
