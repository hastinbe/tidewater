//! Shift-queued orders (`docs/design/extras.md`, "Shift-queued orders"): an optional improvement, off unless
//! [`Extras::order_queue`](crate::Extras) is on. Our own design, not an original-engine rule.
//!
//! A player command wrapped in [`Command::Queued`] (the client sends one when Shift is held) adds its order to the
//! order queue of each unit that takes it, instead of replacing the unit's order. Any other player command a unit
//! takes clears that unit's queue first. The queue is [`State::orders`](crate::extras::State), so it is hashed and
//! saved with the other state of the options (only while an option is on).
//!
//! **What is queued.** The order the command would give the unit, after the command's own checks (a unit that does not
//! take the command, a refused building site, a target already lost: none of them queue anything). Only these orders
//! queue: move (3), move-attack (4), patrol (5), follow (6), attack (9), attack-move (10), repair (27), harvest (23)
//! and build (27 with a building type). A Shift-command that would give any other order (stop, stand ground, cast,
//! attack the ground, demolish, unload, return goods) behaves as a plain command: it clears the queue and runs.
//!
//! **When a queued order starts: "completes".** At the start of a cycle, a unit with a queue and no order of its own
//! (idle: its order is idle and none is pending) takes the first queued order, as if the player had just given it.
//! So an order completes when the unit falls idle:
//!
//! | Order | Completes when |
//! | --- | --- |
//! | move, follow | the unit arrives (follow never ends by itself while the target lives and is reachable) |
//! | attack, move-attack | the target dies or is lost; move-attack also on arrival |
//! | attack-move | the unit arrives at the goal (fights on the way, then resumes) |
//! | repair | the target is fully repaired, destroyed, or the player runs out of what it costs |
//! | harvest | the source is used up and the worker finds nothing else (otherwise never) |
//! | build | the building is finished (or the site was refused on arrival) |
//! | patrol | never: it walks between its ends until given another plain order. Queue it last |
//!
//! A queued order whose target is gone by then is skipped (the unit stays idle that cycle and the next order starts the
//! cycle after). A queued build is checked again when it starts (the site, the cost) and is skipped if refused.
//! Units that are dying, or gone, lose their queue.
//!
//! **Cap.** A unit queues at most [`MAX_QUEUE`] orders after its current one; further Shift-orders are dropped.

use crate::extras::State;
use crate::world::{Command, Order, UnitId, World};

/// The most orders a unit keeps waiting behind its current one.
pub const MAX_QUEUE: usize = 8;

/// What kind of player command is being carried out. `Off` whenever the option is off, so nothing here runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    #[default]
    Off,
    /// A plain command: a unit that takes it forgets its queue.
    Plain,
    /// A Shift-command: queue the order unless the unit has nothing to do.
    Shift,
}

/// Whether an order is one of those a Shift-command queues.
fn queueable(o: Order) -> bool {
    matches!(
        o,
        Order::Move { .. }
            | Order::MoveAttack { .. }
            | Order::Patrol { .. }
            | Order::Follow { .. }
            | Order::Attack { .. }
            | Order::AttackMove { .. }
            | Order::Repair { .. }
            | Order::Harvest { .. }
            | Order::Build { .. }
    )
}

impl World {
    /// Carries out one command from a turn: a plain player command when the option is on.
    pub(crate) fn apply_scoped(&mut self, c: &Command) {
        if self.rules.extras.order_queue {
            self.queue_scope = Scope::Plain;
            self.apply(c);
            self.queue_scope = Scope::Off;
        } else {
            self.apply(c);
        }
    }

    /// `Command::Queued`: carries out `inner` with its orders queued.
    pub(crate) fn apply_queued(&mut self, player: u8, inner: &Command) {
        if !self.rules.extras.order_queue || inner.player() != player || matches!(inner, Command::Queued { .. }) {
            return;
        }
        self.queue_scope = Scope::Shift;
        self.apply(inner);
        self.queue_scope = Scope::Plain;
    }

    /// The hook in [`World::command`]: returns whether the order was queued instead of given. A plain command clears
    /// the unit's queue; a Shift-command of a queueable order to a busy unit (or one with a queue) joins it.
    pub(crate) fn intercept_order(&mut self, i: usize, order: Order) -> bool {
        let id = self.units[i].id;
        match self.queue_scope {
            Scope::Off => false,
            Scope::Plain => {
                self.ext.orders.remove(&id);
                false
            }
            Scope::Shift => {
                if !queueable(order) {
                    self.ext.orders.remove(&id);
                    return false;
                }
                let u = &self.units[i];
                let free = u.order == Order::Idle && u.pending.is_none();
                let waiting = self.ext.orders.get(&id).map_or(0, Vec::len);
                if free && waiting == 0 {
                    return false;
                }
                if waiting < MAX_QUEUE {
                    self.ext.orders.entry(id).or_default().push(order);
                }
                self.just_queued = Some(id);
                true
            }
        }
    }

    /// Start of a cycle: each unit with a queue that has fallen idle starts its next queued order.
    pub(crate) fn run_order_queues(&mut self) {
        if self.ext.orders.is_empty() {
            return;
        }
        let ids: Vec<UnitId> = self.ext.orders.keys().copied().collect();
        for id in ids {
            let Some(i) = self.index(id).filter(|&i| !self.units[i].dying) else {
                self.ext.orders.remove(&id);
                continue;
            };
            let u = &self.units[i];
            if u.order != Order::Idle || u.pending.is_some() {
                continue;
            }
            let Some(list) = self.ext.orders.get_mut(&id) else {
                continue;
            };
            let order = list.remove(0);
            if list.is_empty() {
                self.ext.orders.remove(&id);
            }
            if self.rules.enhanced {
                self.units[i].remembered = None;
            }
            // A queued build is checked again, as the player's command would be, and its walk planned from here.
            let order = match order {
                Order::Build { kind, site, .. } => match self.player_build_command(i, kind, site) {
                    Some(o) => o,
                    None => continue,
                },
                o => o,
            };
            self.command(i, order);
            self.forget_attacker_after_command(id);
        }
    }

    /// The tiles of a unit's queued orders, in order, for the interface to mark.
    pub fn waypoints(&self, id: UnitId) -> Vec<(i32, i32)> {
        let at = |t: UnitId| self.unit(t).map(|u| u.tile);
        self.ext
            .orders
            .get(&id)
            .into_iter()
            .flatten()
            .filter_map(|&o| match o {
                Order::Move { goal, .. } | Order::MoveAttack { goal, .. } | Order::AttackMove { goal } => Some(goal),
                Order::Patrol { to, .. } => Some(to),
                Order::Follow { target } | Order::Attack { target } | Order::Repair { target } => at(target),
                Order::Harvest { source, tree, .. } => source.and_then(at).or(Some(tree)),
                Order::Build { site, .. } => Some(site),
                _ => None,
            })
            .collect()
    }

    /// The number of orders waiting behind a unit's current one.
    pub fn queued_orders(&self, id: UnitId) -> usize {
        self.ext.orders.get(&id).map_or(0, Vec::len)
    }
}

impl State {
    /// Whether any unit has orders waiting.
    pub fn has_orders(&self) -> bool {
        !self.orders.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::{field, run};

    /// A field with a footman-like unit for player 0 at (2, 2), the option as given.
    fn one(on: bool) -> (World, UnitId) {
        let mut w = field(32, 32);
        w.rules.extras.order_queue = on;
        let u = w.spawn(0, 0, (2, 2), 0).unwrap();
        (w, u)
    }

    fn mv(u: UnitId, to: (i32, i32)) -> Command {
        Command::Move {
            player: 0,
            units: vec![u],
            to,
        }
    }

    fn shift(c: Command) -> Command {
        Command::Queued {
            player: 0,
            command: Box::new(c),
        }
    }

    fn near(a: (i32, i32), b: (i32, i32)) -> bool {
        (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1
    }

    #[test]
    fn a_shift_move_waits_for_the_first_and_then_walks() {
        let (mut w, u) = one(true);
        w.step(&[mv(u, (12, 2)), shift(mv(u, (12, 12))), shift(mv(u, (2, 12)))]);
        assert_eq!(w.queued_orders(u), 2);
        assert_eq!(w.waypoints(u), [(12, 12), (2, 12)]);
        let mut seen_first = false;
        let mut seen_second = false;
        for _ in 0..1200 {
            w.step(&[]);
            let t = w.unit(u).unwrap().tile;
            seen_first |= near(t, (12, 2));
            // The second leg starts only after the first is reached.
            seen_second |= seen_first && near(t, (12, 12));
        }
        assert!(seen_first && seen_second, "walked the legs in order");
        assert!(near(w.unit(u).unwrap().tile, (2, 12)), "{:?}", w.unit(u).unwrap().tile);
        assert_eq!(w.queued_orders(u), 0);
        assert!(w.ext.orders.is_empty());
    }

    #[test]
    fn a_shift_order_to_an_idle_unit_just_runs() {
        let (mut w, u) = one(true);
        w.step(&[shift(mv(u, (12, 2)))]);
        assert_eq!(w.queued_orders(u), 0);
        let unit = w.unit(u).unwrap();
        assert!(matches!(unit.pending.unwrap_or(unit.order), Order::Move { .. }));
    }

    #[test]
    fn a_plain_order_clears_the_queue() {
        let (mut w, u) = one(true);
        w.step(&[mv(u, (12, 2)), shift(mv(u, (12, 12)))]);
        assert_eq!(w.queued_orders(u), 1);
        w.step(&[mv(u, (2, 12))]);
        assert_eq!(w.queued_orders(u), 0);
        run(&mut w, 1000);
        assert!(near(w.unit(u).unwrap().tile, (2, 12)));
        // A stop clears it too.
        w.step(&[mv(u, (12, 2)), shift(mv(u, (12, 12)))]);
        w.step(&[Command::Stop {
            player: 0,
            units: vec![u],
        }]);
        assert_eq!(w.queued_orders(u), 0);
    }

    #[test]
    fn the_queue_holds_eight() {
        let (mut w, u) = one(true);
        let mut cs = vec![mv(u, (30, 30))];
        cs.extend((0..12).map(|k| shift(mv(u, (3 + k, 3)))));
        w.step(&cs);
        assert_eq!(w.queued_orders(u), MAX_QUEUE);
        // The first eight were kept, in order.
        assert_eq!(w.waypoints(u)[0], (3, 3));
        assert_eq!(w.waypoints(u)[7], (10, 3));
    }

    #[test]
    fn with_the_option_off_a_shift_command_does_nothing() {
        let (mut w, u) = one(false);
        let (mut v, uv) = one(false);
        w.step(&[mv(u, (12, 2)), shift(mv(u, (12, 12)))]);
        v.step(&[mv(uv, (12, 2))]);
        assert!(w.ext.is_empty());
        assert_eq!(w.state_hash(), v.state_hash());
        run(&mut w, 400);
        run(&mut v, 400);
        assert_eq!(w.state_hash(), v.state_hash());
    }

    #[test]
    fn a_unit_that_dies_loses_its_queue() {
        let (mut w, u) = one(true);
        w.step(&[mv(u, (12, 2)), shift(mv(u, (12, 12)))]);
        let i = w.index(u).unwrap();
        w.units[i].hp = 0;
        w.units[i].dying = true;
        run(&mut w, 3);
        assert!(w.ext.orders.is_empty());
    }

    #[test]
    fn a_lost_target_is_skipped() {
        let (mut w, u) = one(true);
        let foe = w.spawn(0, 1, (20, 20), 0).unwrap();
        w.step(&[
            mv(u, (8, 2)),
            shift(Command::Attack {
                player: 0,
                units: vec![u],
                target: foe,
            }),
            shift(mv(u, (2, 8))),
        ]);
        // The enemy is gone before the attack starts.
        let j = w.index(foe).unwrap();
        w.units[j].dying = true;
        run(&mut w, 1500);
        assert!(near(w.unit(u).unwrap().tile, (2, 8)), "{:?}", w.unit(u).unwrap().tile);
    }

    #[test]
    fn the_queue_is_saved_and_hashed_while_the_option_is_on() {
        let (mut w, u) = one(true);
        w.step(&[mv(u, (12, 2)), shift(mv(u, (12, 12)))]);
        let (mut v, _) = one(true);
        v.step(&[mv(u, (12, 2))]);
        assert_ne!(w.state_hash(), v.state_hash(), "the queue is game state");
        let bytes = w.save_state();
        let (mut b, _) = one(true);
        b.restore(&bytes, 1).unwrap();
        assert_eq!(b.ext.orders, w.ext.orders);
        assert!(b.rules.extras.order_queue);
    }

    #[test]
    fn a_recorded_queue_replays_to_the_same_hashes() {
        // What a replay does: the same commands at the same cycles on a fresh world.
        let script: Vec<(u32, Command)> = vec![
            (0, mv(UnitId(1), (12, 2))),
            (1, shift(mv(UnitId(1), (12, 12)))),
            (2, shift(mv(UnitId(1), (2, 12)))),
        ];
        let play = |w: &mut World| -> Vec<u64> {
            let mut hashes = Vec::new();
            for t in 0..600u32 {
                let now: Vec<Command> = script.iter().filter(|(c, _)| *c == t).map(|(_, c)| c.clone()).collect();
                w.step(&now);
                hashes.push(w.state_hash());
            }
            hashes
        };
        let (mut a, ua) = one(true);
        assert_eq!(ua, UnitId(1));
        let (mut b, _) = one(true);
        assert_eq!(play(&mut a), play(&mut b));
    }
}
