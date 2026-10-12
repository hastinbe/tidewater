//! Repair by workers (reviewed spec `docs/specs/economy/repair.md`).
//!
//! Repair is order 27: it plays the attack animation, so its logic runs once per attack loop. Each run on an arrived
//! worker adds 4 hit points to the target; every second run on the same target, by any worker, costs 1 gold and 1
//! lumber. Reacting to an attacker (step 1) waits on the orders spec: until then an attacked repairer keeps repairing.

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::economy::{Event, Shortage};
use crate::movement::{Goal, Walk, building_goal, chebyshev};
use crate::world::{Order, Unit, UnitId, World, facing};

/// Hit points one run adds.
const HP_PER_RUN: u16 = 4;
/// Runs on a target per payment.
const RUNS_PER_PAYMENT: u8 = 2;
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
            let _ = i;
            Order::Move { goal: t.tile }
        })
    }

    /// One run of the repair order's logic ("Each run of the order's logic").
    pub(crate) fn repair_order(&mut self, i: usize, target: UnitId) {
        // 2. No target.
        let Some(ti) = self.index(target).filter(|&ti| self.units[ti].present()) else {
            self.go_idle(i);
            self.switch_anim(i, anim::STILL);
            return;
        };
        // 3. Not arrived: move toward the target.
        let (u, t) = (&self.units[i], &self.units[ti]);
        let goal = if t.building { building_goal(u.tile, t) } else { t.tile };
        if chebyshev(u.tile, goal) > RADIUS {
            let zone = (
                t.tile.0 - 1,
                t.tile.1 - 1,
                t.tile.0 + t.size.0 as i32,
                t.tile.1 + t.size.1 as i32,
            );
            let g = Goal {
                tile: goal,
                radius: RADIUS,
                range: false,
                zone: Some(zone),
            };
            if self.move_toward(i, g) == Walk::NoPath {
                self.go_idle(i);
                self.switch_anim(i, anim::STILL);
            }
            return;
        }
        // 4. Full hit points.
        let max = self.stats_of(t.kind).hit_points;
        if t.hp >= max {
            self.go_idle(i);
            self.switch_anim(i, anim::STILL);
            return;
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
            self.go_idle(i);
            self.switch_anim(i, anim::STILL);
            return;
        }
        // 6. Repair: face the target (a building at its top-left tile plus (1, 1)), add hit points, count the run.
        let aim = if t.building {
            (t.tile.0 + 1, t.tile.1 + 1)
        } else {
            t.tile
        };
        let f = facing(aim.0 - u.tile.0, aim.1 - u.tile.1);
        let t = &mut self.units[ti];
        t.hp = (t.hp + HP_PER_RUN).min(max);
        t.repair_runs += 1;
        if t.repair_runs >= RUNS_PER_PAYMENT {
            t.repair_runs -= RUNS_PER_PAYMENT;
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
        assert_eq!(w.unit(farm).unwrap().repair_runs as u16, runs % 2);
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
        assert_eq!(w.units[fi].repair_runs, 1);
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
        w.repair_order(ia, farm);
        assert_eq!(w.units[ia].order, Order::Idle);
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

    #[test]
    fn only_buildings_and_transports_are_repaired() {
        let (mut w, a, _) = site();
        let footman = w.spawn(8, 0, (7, 7), 0).unwrap();
        repair(&mut w, vec![a], footman);
        for _ in 0..10 {
            w.step(&[]);
        }
        assert!(matches!(w.unit(a).unwrap().order, Order::Move { .. }));
    }
}
