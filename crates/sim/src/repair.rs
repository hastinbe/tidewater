//! Repair by workers (reviewed spec `docs/specs/economy/repair.md`).
//!
//! Repair is order 27: it plays the attack animation, so its logic runs once per attack loop. Each run on an arrived
//! worker adds 4 hit points to the target and adds 1 to the target's counter (a building's construction pass
//! counter, a mobile unit's stored goal x); a run that leaves it at 2 or more takes 2 off and costs 1 gold and 1
//! lumber (round 97). Step 1, the reaction to an attacker, runs only once the walk has ended (orders spec, round-63 errata).

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::economy::{Event, Shortage};
use crate::movement::chebyshev;
use crate::world::{Order, Unit, UnitId, World};

/// Hit points one run adds.
const HP_PER_RUN: u16 = 4;
/// The counter value (after the +1) from which a run pays, and what a payment takes off.
const PAYMENT_THRESHOLD: i16 = 2;
const RADIUS: i32 = 1;

impl World {
    /// Whether worker `u` has something to repair in `target`: a damaged building or transport. The client uses this
    /// to turn a right click into a repair.
    pub fn can_repair(&self, u: &Unit, target: UnitId) -> bool {
        let Some(t) = self.unit(target).filter(|t| t.present()) else {
            return false;
        };
        let s = self.stats_of(t.kind);
        self.stats_of(u.kind).flags & flags::PEON != 0
            && (t.building || s.flags & flags::TRANSPORT != 0)
            && t.hp < s.hit_points
    }

    /// Command 27: repair `target`, or just walk toward it if it can't be repaired ("The command").
    pub(crate) fn repair_command(&mut self, i: usize, target: UnitId) -> Option<Order> {
        let t = self.unit(target)?;
        let repairable = t.building || self.stats_of(t.kind).flags & flags::TRANSPORT != 0;
        Some(if repairable {
            Order::Repair { target }
        } else {
            // Round 97 (RP7): the unit stays the move's target, as for a smart move that falls through to the move.
            let _ = i;
            Order::Move {
                goal: t.tile,
                target: Some(target),
            }
        })
    }

    /// One run of the repair order's logic ("Each run of the order's logic").
    pub(crate) fn repair_order(&mut self, i: usize, target: UnitId) {
        // 2. No target.
        let Some(ti) = self.index(target).filter(|&ti| self.units[ti].present()) else {
            if self.react_at_walk_end(i) {
                return;
            }
            // Step 2 (round 97, RP6): the pending idle of step 4, with the Enhanced resume.
            return self.idle_now(i);
        };
        // 3. Not arrived: move toward the target.
        let (u, t) = (&self.units[i], &self.units[ti]);
        let goal = self.target_goal(u, t); // Round-60 errata: a shore building by its shore corner goal.
        if chebyshev(u.tile, goal) > RADIUS {
            let g = self.target_walk(u, t, RADIUS);
            // Orders spec, round-63 errata ("Walking orders react only when the walk has ended"; static study only
            // for order 27): while walking only the walk runs. Round-64 errata (static study only): no route is not
            // a walk end; the worker becomes idle as an immediate order and its idle logic runs, not the reaction.
            self.walk(i, g);
            return;
        }
        // 1. Arrived: the reaction to an attacker.
        if self.react_at_walk_end(i) {
            return;
        }
        let (u, t) = (&self.units[i], &self.units[ti]);
        // 4. Full hit points. Traced (`human13-invisible-work-hits-900` peasant 1401, n = 511): idle is the pending
        // order taking over in the same update (still animation, frame kept, wait 3), by the common path with the
        // resume step.
        let max = self.stats_of(t.kind).hit_points;
        if t.hp >= max {
            return self.idle_now(i);
        }
        // 5. Out of gold or lumber; lumber is checked first.
        let owner = u.owner;
        let p = &self.players[owner as usize];
        let short = if p.lumber == 0 {
            Some(Shortage::Lumber)
        } else if p.gold == 0 {
            Some(Shortage::Gold)
        } else {
            None
        };
        if let Some(shortage) = short {
            self.events.push(Event::CantRepair {
                player: owner,
                shortage,
            });
            // Step 5 (round 97, RP6): the same pending idle as at full hit points, not the immediate idle.
            return self.idle_now(i);
        }
        // 6. Repair: face the target (a building at its top-left tile plus (1, 1)), add hit points, count the run.
        let aim = if t.building {
            (t.tile.0 + 1, t.tile.1 + 1)
        } else {
            t.tile
        };
        let f = crate::world::face_tile(u.tile, aim);
        let t = &mut self.units[ti];
        t.hp = (t.hp + HP_PER_RUN).min(max);
        t.goal_x = t.goal_x.wrapping_add(1);
        if t.goal_x >= PAYMENT_THRESHOLD {
            t.goal_x = t.goal_x.wrapping_sub(PAYMENT_THRESHOLD);
            let p = &mut self.players[owner as usize];
            p.gold -= 1;
            p.lumber -= 1;
        }
        let u = &mut self.units[i];
        u.path.clear();
        if u.script.anim != anim::ATTACK {
            u.facing = f;
            self.switch_anim(i, anim::ATTACK);
        } else {
            u.facing = f;
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Command;
    use crate::economy::{Event, Shortage};
    use crate::world::tests::field;
    use crate::{Order, UnitId, World};
    use tidewater_formats::script::anim;
    use tidewater_formats::udta::flags;

    fn repair(w: &mut World, units: Vec<UnitId>, target: UnitId) {
        w.step(&[Command::Repair {
            player: 0,
            units,
            target,
        }]);
    }

    /// A field with a damaged 2 x 2 farm at (4, 4) and a worker at (1, 4).
    fn site() -> (World, UnitId, UnitId) {
        let mut w = field(10, 10);
        w.stats.0[0].flags |= flags::PEON;
        let peasant = w.spawn(0, 0, (1, 4), 0).unwrap();
        let farm = w.spawn(0x3a, 0, (4, 4), 0).unwrap();
        let i = w.index(farm).unwrap();
        w.units[i].hp = 300;
        w.players[0].gold = 100;
        w.players[0].lumber = 100;
        (w, peasant, farm)
    }

    #[test]
    fn walks_up_and_repairs_4_hp_per_run_paying_every_second_run() {
        let (mut w, peasant, farm) = site();
        repair(&mut w, vec![peasant], farm);
        for _ in 0..200 {
            w.step(&[]);
        }
        let p = w.unit(peasant).unwrap();
        assert_eq!(p.order, Order::Repair { target: farm });
        assert_eq!(p.script.anim, anim::ATTACK);
        // Next to the farm's 2 x 2 footprint at (4, 4).
        assert!((3..=6).contains(&p.tile.0) && (3..=6).contains(&p.tile.1));
        assert!(p.tile.0 == 3 || p.tile.1 == 3);
        let gained = w.unit(farm).unwrap().hp - 300;
        assert!(gained > 0 && gained % 4 == 0);
        let runs = gained / 4;
        let paid = 100 - w.players[0].gold;
        assert_eq!((paid, 100 - w.players[0].lumber), (paid, paid));
        assert_eq!(paid as u16, runs / 2);
        assert_eq!(w.unit(farm).unwrap().goal_x as u16, runs % 2);
    }

    #[test]
    fn two_workers_share_the_targets_counter() {
        // Two peasants on one farm, 3 runs in all: 12 HP, one payment, counter 1.
        let (mut w, a, farm) = site();
        let b = w.spawn(0, 0, (4, 3), 0).unwrap();
        let (ia, ib, fi) = (w.index(a).unwrap(), w.index(b).unwrap(), w.index(farm).unwrap());
        for i in [ia, ib] {
            w.units[i].order = Order::Repair { target: farm };
        }
        w.units[ia].tile = (3, 4);
        w.repair_order(ia, farm);
        w.repair_order(ib, farm);
        w.repair_order(ia, farm);
        assert_eq!(w.units[fi].hp, 312);
        assert_eq!(w.units[fi].goal_x, 1);
        assert_eq!((w.players[0].gold, w.players[0].lumber), (99, 99));
    }

    #[test]
    fn stops_at_full_health_or_out_of_stock() {
        let (mut w, a, farm) = site();
        let (ia, fi) = (w.index(a).unwrap(), w.index(farm).unwrap());
        w.units[ia].tile = (3, 4);
        w.units[ia].order = Order::Repair { target: farm };
        w.units[fi].hp = 398;
        w.repair_order(ia, farm);
        assert_eq!(w.units[fi].hp, 400);
        w.units[ia].script.frame = 25;
        w.repair_order(ia, farm);
        assert_eq!(w.units[ia].order, Order::Idle);
        // Traced (`human13-invisible-work-hits-900`, n = 511): the idle takes over at once, frame kept, wait 3.
        let s = w.units[ia].script;
        assert_eq!((s.anim, s.frame, s.wait), (anim::STILL, 25, 3));
        // Lumber is checked first.
        w.units[fi].hp = 300;
        w.units[ia].order = Order::Repair { target: farm };
        w.players[0].lumber = 0;
        w.players[0].gold = 0;
        w.repair_order(ia, farm);
        assert_eq!(w.units[ia].order, Order::Idle);
        assert_eq!(
            w.events.last(),
            Some(&Event::CantRepair {
                player: 0,
                shortage: Shortage::Lumber
            })
        );
    }

    /// Repair spec "Cost and rate" and build-train V72 (round 97): a farm completed by construction (T = 100) holds
    /// 12 x (T + 2) = 1,224 as its counter, so every repair run pays; the counter goes 1,225, 1,223, 1,224, ...
    #[test]
    fn a_built_building_pays_on_every_run() {
        let (mut w, peasant, _) = site();
        w.stats.0[0x3a].hit_points = 400;
        w.stats.0[0x3a].build_time = 100;
        let farm = w.place_building(0x3a, 0, (7, 7)).unwrap();
        let fi = w.index(farm).unwrap();
        assert_eq!(w.units[fi].goal_x, 0);
        // It counts passes from 1 (the pass that shows the site full moves it up to 12 x (T + 1)).
        w.step(&[]);
        assert_eq!(w.units[fi].goal_x, 1);
        for _ in 0..1230 {
            w.step(&[]);
        }
        assert!(w.units[fi].construction.is_none());
        assert_eq!(w.units[fi].goal_x, 1224);
        for _ in 0..5 {
            w.step(&[]);
        }
        assert_eq!(w.units[fi].goal_x, 1224, "nothing resets it");
        w.units[fi].hp = 300;
        let pi = w.index(peasant).unwrap();
        w.units[pi].tile = (6, 7);
        w.units[pi].order = Order::Repair { target: farm };
        for run in 1..=10 {
            w.units[pi].script.frame = 25;
            w.repair_order(pi, farm);
            assert_eq!(100 - w.players[0].gold, run, "run {run} pays");
            assert_eq!(w.units[fi].goal_x, 1224 - run as i16);
        }
    }

    /// Round 97: a mobile unit's counter is the x of its stored goal (the tile its last command sent it to).
    #[test]
    fn a_transports_counter_is_its_goal_x() {
        let (mut w, peasant, _) = site();
        w.stats.0[0x1c].flags |= flags::TRANSPORT;
        w.stats.0[0x1c].hit_points = 150;
        let ship = w.spawn(0x1c, 0, (0, 0), 0).unwrap();
        let si = w.index(ship).unwrap();
        w.units[si].hp = 100;
        w.step(&[Command::Move {
            player: 0,
            units: vec![ship],
            to: (3, 0),
        }]);
        assert_eq!(w.units[si].goal_x, 3);
        let pi = w.index(peasant).unwrap();
        w.units[si].goal_x = 7;
        w.units[pi].tile = (1, 0);
        w.units[pi].order = Order::Repair { target: ship };
        w.repair_order(pi, ship);
        assert_eq!((w.units[si].goal_x, 100 - w.players[0].gold), (6, 1));
        // A command that sends it somewhere stores the new goal's x.
        w.step(&[Command::Move {
            player: 0,
            units: vec![ship],
            to: (5, 0),
        }]);
        assert_eq!(w.units[si].goal_x, 5);
    }

    /// Round 98 (RP8, traced): a repaired transport's counter is the x of its stored goal and a run changes only that
    /// x: every run adds 1 and, at 2 or more, pays 1 gold and 1 lumber and takes 2 off, so while the x is 1 or more the
    /// x falls by 1 and the run pays; at 0 a run leaves 1 and doesn't pay. The transport's destination, tile and order
    /// stay as they were, and the next command sets a new goal.
    #[test]
    fn a_repaired_transports_goal_x_falls_by_one_per_paid_run_and_nothing_else_changes() {
        let (mut w, peasant, _) = site();
        w.stats.0[0x1c].flags |= flags::TRANSPORT;
        w.stats.0[0x1c].hit_points = 150;
        let ship = w.spawn(0x1c, 0, (0, 0), 0).unwrap();
        let si = w.index(ship).unwrap();
        w.units[si].hp = 20;
        w.step(&[Command::Move {
            player: 0,
            units: vec![ship],
            to: (8, 2),
        }]);
        assert_eq!(w.units[si].goal_x, 8);
        w.units[si].order = Order::Docked;
        w.units[si].tile = (5, 5);
        let (dest, tile) = (w.units[si].ship.dest, w.units[si].tile);
        let pi = w.index(peasant).unwrap();
        w.units[pi].tile = (6, 5);
        w.units[pi].order = Order::Repair { target: ship };
        for run in 1..=8i16 {
            w.units[pi].script.frame = 25;
            w.repair_order(pi, ship);
            assert_eq!(w.units[si].goal_x, 8 - run, "run {run}");
            assert_eq!(100 - w.players[0].gold, run as u32, "run {run} pays");
            assert_eq!(w.units[si].hp, 20 + 4 * run as u16);
            assert_eq!((w.units[si].ship.dest, w.units[si].tile, w.units[si].order), (dest, tile, Order::Docked));
        }
        // At 0 the next run leaves 1 and doesn't pay; the one after pays and is back at 0.
        w.repair_order(pi, ship);
        assert_eq!((w.units[si].goal_x, 100 - w.players[0].gold), (1, 8));
        w.repair_order(pi, ship);
        assert_eq!((w.units[si].goal_x, 100 - w.players[0].gold), (0, 9));
        // A command that sends it somewhere sets a new goal.
        w.step(&[Command::Move {
            player: 0,
            units: vec![ship],
            to: (4, 2),
        }]);
        assert_eq!(w.units[si].goal_x, 4);
    }

    /// Repair spec step 2 and 5 (round 97, RP6): a repair that stops for lack of resources or a lost target ends with
    /// the pending idle, as at full hit points: the still animation keeps its frame, and in Enhanced mode a remembered
    /// attack-move resumes.
    #[test]
    fn a_repair_that_stops_ends_with_the_pending_idle_and_the_resume() {
        for lost in [false, true] {
            for enhanced in [true, false] {
                let (mut w, a, farm) = site();
                w.rules.enhanced = enhanced;
                let (ia, fi) = (w.index(a).unwrap(), w.index(farm).unwrap());
                w.units[ia].tile = (3, 4);
                w.units[ia].order = Order::Repair { target: farm };
                w.units[ia].script.frame = 25;
                w.units[ia].remembered = Some(crate::world::Remembered::AttackMove { goal: (9, 9) });
                w.units[ia].resume = 0;
                if lost {
                    w.units[fi].dying = true;
                } else {
                    w.players[0].lumber = 0;
                }
                w.repair_order(ia, farm);
                let u = &w.units[ia];
                if enhanced {
                    assert!(matches!(u.order, Order::AttackMove { .. }), "lost {lost}");
                } else {
                    assert_eq!(u.order, Order::Idle, "lost {lost}");
                    assert_eq!((u.script.anim, u.script.frame, u.script.wait), (anim::STILL, 25, 3));
                }
            }
        }
    }

    #[test]
    fn only_buildings_and_transports_are_repaired() {
        let (mut w, a, _) = site();
        let footman = w.spawn(8, 0, (7, 7), 0).unwrap();
        repair(&mut w, vec![a], footman);
        for _ in 0..10 {
            w.step(&[]);
        }
        assert_eq!(
            w.unit(a).unwrap().order,
            Order::Move {
                goal: w.unit(footman).unwrap().tile,
                target: Some(footman)
            }
        );
    }

    /// Repair spec vectors (round 97, RP7): command 27 on an enemy footman gives order 3 with the footman as target.
    #[test]
    fn repair_on_an_enemy_unit_is_a_move_keeping_it_as_target() {
        let (mut w, a, _) = site();
        let enemy = w.spawn(1, 1, (8, 8), 0).unwrap();
        let ai = w.index(a).unwrap();
        let c = Command::Repair {
            player: 0,
            units: vec![a],
            target: enemy,
        };
        assert_eq!(
            w.command_order(ai, &c),
            Some(Order::Move {
                goal: (8, 8),
                target: Some(enemy)
            })
        );
    }
}
