//! The production queue (`docs/design/extras.md`, "Production queue"): an optional improvement, off unless
//! [`Extras::production_queue`](crate::Extras) is on. Our own design, not an original-engine rule.
//!
//! A building keeps its one running job exactly as the original does ([`Job`](crate::economy::Job)); the queue is the
//! list of jobs waiting behind it, in [`State::queue`](crate::extras::State). Up to [`QUEUE_MAX`] jobs in all, the
//! running one included, so four wait.
//!
//! - **Queueing.** A train, upgrade, research or spell command for a building that is busy joins the queue, if it
//!   passes the same checks a job start does (the building trains it, the level, the food and the price) and the
//!   research is not already running or waiting anywhere. The price is paid at once, so what a queued job costs is
//!   gone from the player's resources while it waits. A building that is idle starts the job as always.
//! - **Cancelling.** Cancel on a building with waiting jobs removes the last one and gives back what was paid for
//!   it ([`Queued::paid`](crate::extras::Queued)). With nothing waiting, Cancel cancels the running job as always.
//! - **Promotion.** When the running job ends (finished or cancelled), the next waiting job starts in the same
//!   cycle: its payment is given back and the job is started again by the ordinary start, so every check is made
//!   again with today's state. A job that no longer passes (the food ran out, a farm burned, the building became a
//!   keep) is dropped, with its payment already returned, and the next one is tried.
//! - **Nothing behind a building upgrade**: it changes what the building is, so no job is queued behind one, and an
//!   upgrade is only ever the last job in a queue.
//! - **A building that is killed or changes owner** gives back everything that waits, to the owner it has then.

use crate::economy::{Job, JobKind};
use crate::extras::Queued;
use crate::world::{UnitId, World};

/// The most jobs a building holds, the running one included.
pub const QUEUE_MAX: usize = 5;

impl World {
    /// The jobs waiting behind building `id`'s running one, next first.
    pub fn waiting(&self, id: UnitId) -> &[Queued] {
        self.ext.queue.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Whether a job command for building `i` joins the queue: the option is on and the building is busy. A building
    /// that is idle (or not finished) takes the command as always.
    pub(crate) fn queue_job(&mut self, i: usize, job: JobKind) -> bool {
        let u = &self.units[i];
        if !self.rules.extras.production_queue || u.dying || u.construction.is_some() || u.job.is_none() {
            return false;
        }
        let (id, owner, b) = (u.id, u.owner, u.kind);
        let waiting = self.waiting(id);
        let last_is_upgrade = waiting.last().map_or_else(
            || {
                matches!(
                    u.job,
                    Some(Job {
                        kind: JobKind::Upgrade(_),
                        ..
                    })
                )
            },
            |q| matches!(q.kind, JobKind::Upgrade(_)),
        );
        if 1 + waiting.len() >= QUEUE_MAX || last_is_upgrade {
            return true;
        }
        // The checks of a job start (`economy::try_job`, steps 2 to 4).
        let needs_food = match job {
            JobKind::Train(k) => {
                if !self.may_train(owner, b, k) {
                    return true;
                }
                true
            }
            JobKind::Upgrade(to) => {
                if !self.may_become(owner, b, to) {
                    return true;
                }
                false
            }
            JobKind::Research(n) | JobKind::Spell(n) => {
                if !World::may_research(b, n) || (matches!(job, JobKind::Research(_)) && !self.level_ok(owner, n)) {
                    return true;
                }
                // Already running or waiting somewhere: the button is hidden for it, and a second copy is refused.
                if self.researching(owner, job) {
                    return true;
                }
                false
            }
        };
        let paid = self.job_cost(job);
        if !self.afford_or_tell(owner, paid, needs_food) {
            return true;
        }
        self.pay(owner, paid);
        self.mark_researching(owner, job, true);
        self.ext.queue.entry(id).or_default().push(Queued { kind: job, paid });
        true
    }

    /// Whether the research or spell `job` is already running or waiting for `owner` (its "being researched" bits are
    /// set).
    fn researching(&self, owner: u8, job: JobKind) -> bool {
        let Some((n, spell)) = job.research() else { return false };
        let affect = self.upgrade_data.0.get(n as usize).copied().unwrap_or_default().affect;
        let p = &self.players[owner as usize];
        (if spell {
            p.spells_researching
        } else {
            p.upgrades_researching
        }) & affect
            != 0
    }

    /// Sets or clears the "being researched" bits of a research or spell job (nothing for the other kinds).
    fn mark_researching(&mut self, owner: u8, job: JobKind, on: bool) {
        let Some((n, spell)) = job.research() else { return };
        let affect = self.upgrade_data.0.get(n as usize).copied().unwrap_or_default().affect;
        let p = &mut self.players[owner as usize];
        let word = if spell {
            &mut p.spells_researching
        } else {
            &mut p.upgrades_researching
        };
        if on {
            *word |= affect;
        } else {
            *word &= !affect;
        }
    }

    /// Cancel on a building with waiting jobs: removes the last and gives back what was paid for it. Returns whether
    /// there was one to remove.
    pub(crate) fn unqueue_last(&mut self, i: usize) -> bool {
        let id = self.units[i].id;
        let Some(list) = self.ext.queue.get_mut(&id) else {
            return false;
        };
        let Some(q) = list.pop() else { return false };
        if list.is_empty() {
            self.ext.queue.remove(&id);
        }
        let owner = self.units[i].owner;
        self.refund(owner, q.paid);
        self.mark_researching(owner, q.kind, false);
        true
    }

    /// The running job of building `i` has just ended: starts the next waiting one that still passes its checks.
    pub(crate) fn promote_queue(&mut self, i: usize) {
        if self.ext.queue.is_empty() {
            return;
        }
        let id = self.units[i].id;
        while self.units[i].job.is_none() && !self.units[i].dying {
            let Some(list) = self.ext.queue.get_mut(&id) else {
                return;
            };
            let q = list.remove(0);
            if list.is_empty() {
                self.ext.queue.remove(&id);
            }
            let owner = self.units[i].owner;
            self.refund(owner, q.paid);
            self.mark_researching(owner, q.kind, false);
            self.try_job(i, q.kind, true);
        }
    }

    /// Gives back everything that waits at building `i`, to its owner now (a killed or taken building).
    pub(crate) fn flush_queue(&mut self, i: usize) {
        let Some(list) = self.ext.queue.remove(&self.units[i].id) else {
            return;
        };
        let owner = self.units[i].owner;
        for q in list {
            self.refund(owner, q.paid);
            self.mark_researching(owner, q.kind, false);
        }
    }

    /// Forgets the queues of buildings that are gone (their payments went back when they were killed).
    pub(crate) fn prune_queue(&mut self) {
        if self.ext.queue.is_empty() {
            return;
        }
        let gone: Vec<UnitId> = self
            .ext
            .queue
            .keys()
            .copied()
            .filter(|&b| !self.unit(b).is_some_and(|u| !u.dying && u.building))
            .collect();
        for b in gone {
            self.ext.queue.remove(&b);
        }
    }

    /// A barracks's waiting `from` trainees become `to` when the upgrade that changes them finishes (the running one
    /// is converted by [`World::convert`]). The payment is what was paid.
    pub(crate) fn convert_queued(&mut self, owner: u8, from: u8, to: u8) {
        let ids: Vec<UnitId> = self.ext.queue.keys().copied().collect();
        for id in ids {
            if !self.unit(id).is_some_and(|u| u.owner == owner) {
                continue;
            }
            if let Some(list) = self.ext.queue.get_mut(&id) {
                for q in list.iter_mut().filter(|q| q.kind == JobKind::Train(from)) {
                    q.kind = JobKind::Train(to);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::kind as k;
    use crate::select::action;
    use crate::world::Command;
    use crate::world::tests::{field, run};
    use tidewater_formats::udta::{UnitStats, flags};

    /// Peasant (2) trains at a town hall: 400 gold, 2 steps.
    fn town(on: bool) -> (World, UnitId) {
        let mut w = field(24, 24);
        for b in 58..100 {
            w.stats.0[b] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING,
                ..Default::default()
            };
        }
        w.stats.0[2] = UnitStats {
            hit_points: 30,
            size: (1, 1),
            flags: flags::LAND | flags::PEON,
            right_click: action::MOVE,
            build_time: 2,
            gold: 400,
            ..Default::default()
        };
        w.rules.extras.production_queue = on;
        w.rules.enforce_requirements = false;
        let hall = w.spawn(k::TOWN_HALL, 0, (5, 6), 0).unwrap();
        for n in 0..4 {
            w.spawn(k::FARM, 0, (3 * n, 0), 0).unwrap();
        }
        let p = &mut w.players[0];
        (p.gold, p.lumber, p.oil) = (10_000, 10_000, 10_000);
        (w, hall)
    }

    fn train(w: &mut World, hall: UnitId) {
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
    }

    fn peasants(w: &World) -> usize {
        w.units.iter().filter(|u| u.kind == 2).count()
    }

    fn gold(w: &World) -> u32 {
        w.players[0].gold
    }

    #[test]
    fn with_the_option_off_a_second_command_is_ignored_as_always() {
        let (mut w, hall) = town(false);
        train(&mut w, hall);
        train(&mut w, hall);
        assert_eq!(gold(&w), 9_600);
        assert!(w.ext.is_empty());
    }

    #[test]
    fn jobs_queue_up_to_five_and_each_is_paid_when_queued() {
        let (mut w, hall) = town(true);
        for _ in 0..7 {
            train(&mut w, hall);
        }
        assert_eq!(w.waiting(hall).len(), QUEUE_MAX - 1);
        assert_eq!(gold(&w), 10_000 - 400 * QUEUE_MAX as u32);
    }

    #[test]
    fn the_queue_runs_one_job_after_another() {
        let (mut w, hall) = town(true);
        for _ in 0..3 {
            train(&mut w, hall);
        }
        let before = peasants(&w);
        run(&mut w, 2000);
        assert_eq!(peasants(&w), before + 3);
        assert!(w.waiting(hall).is_empty());
        assert!(w.unit(hall).unwrap().job.is_none());
        assert_eq!(gold(&w), 10_000 - 1200);
    }

    #[test]
    fn cancel_removes_the_last_waiting_job_and_refunds_it_then_the_running_one() {
        let (mut w, hall) = town(true);
        for _ in 0..3 {
            train(&mut w, hall);
        }
        assert_eq!(gold(&w), 10_000 - 1200);
        let cancel = |w: &mut World| w.step(&[Command::CancelJob { player: 0, unit: hall }]);
        cancel(&mut w);
        assert_eq!((w.waiting(hall).len(), gold(&w)), (1, 10_000 - 800));
        cancel(&mut w);
        assert_eq!((w.waiting(hall).len(), gold(&w)), (0, 10_000 - 400));
        cancel(&mut w);
        run(&mut w, 100);
        assert_eq!(gold(&w), 10_000, "the running job is cancelled as always");
        assert!(w.unit(hall).unwrap().job.is_none());
    }

    #[test]
    fn a_job_that_cannot_start_when_its_turn_comes_is_refunded() {
        let (mut w, hall) = town(true);
        train(&mut w, hall);
        train(&mut w, hall);
        // Lose all the food: the second peasant cannot start.
        let farms: Vec<usize> = (0..w.units.len()).filter(|&n| w.units[n].kind == k::FARM).collect();
        for n in farms {
            w.units[n].owner = 1;
        }
        run(&mut w, 2000);
        assert!(w.waiting(hall).is_empty());
        assert_eq!(gold(&w), 10_000 - 400, "only the trained one was kept");
    }

    #[test]
    fn research_queues_pays_blocks_a_copy_and_gives_back_on_cancel() {
        let (mut w, _) = town(true);
        let smith = w
            .spawn(crate::tech::building::HUMAN_BLACKSMITH, 0, (12, 12), 0)
            .unwrap();
        let research = |w: &mut World, n| {
            w.step(&[Command::Research {
                player: 0,
                building: smith,
                upgrade: n,
            }])
        };
        let start = w.players[0];
        research(&mut w, 0);
        let one = w.players[0];
        assert_ne!(one.gold, start.gold, "the first starts and is paid");
        research(&mut w, 8);
        assert_eq!(w.waiting(smith).len(), 1);
        let two = w.players[0];
        assert!(two.gold < one.gold);
        assert_ne!(
            two.upgrades_researching, one.upgrades_researching,
            "the waiting one is marked"
        );
        research(&mut w, 8);
        assert_eq!(w.waiting(smith).len(), 1, "a second copy is refused");
        assert_eq!(w.players[0].gold, two.gold);
        w.step(&[Command::CancelJob { player: 0, unit: smith }]);
        let back = w.players[0];
        assert_eq!(
            (back.gold, back.lumber, back.upgrades_researching),
            (one.gold, one.lumber, one.upgrades_researching)
        );
        assert!(w.waiting(smith).is_empty());
    }

    #[test]
    fn nothing_queues_behind_an_upgrade() {
        let (mut w, hall) = town(true);
        w.stats.0[k::KEEP as usize] = w.stats.0[k::TOWN_HALL as usize];
        w.stats.0[k::KEEP as usize].gold = 700;
        w.stats.0[k::KEEP as usize].build_time = 5;
        w.step(&[Command::Upgrade {
            player: 0,
            building: hall,
            to: k::KEEP,
        }]);
        let g = gold(&w);
        train(&mut w, hall);
        assert!(w.waiting(hall).is_empty());
        assert_eq!(gold(&w), g);
    }

    #[test]
    fn a_killed_building_gives_back_the_queue() {
        let (mut w, hall) = town(true);
        for _ in 0..3 {
            train(&mut w, hall);
        }
        let i = w.index(hall).unwrap();
        w.kill(i);
        run(&mut w, 2);
        assert_eq!(gold(&w), 10_000);
        assert!(w.ext.queue.is_empty());
    }

    #[test]
    fn two_machines_agree_with_the_queue_on() {
        let one = || {
            let (mut w, hall) = town(true);
            let mut hashes = Vec::new();
            for step in 0..400u32 {
                let cmds: Vec<Command> = match step {
                    0 | 1 | 2 | 50 => vec![Command::Train {
                        player: 0,
                        building: hall,
                        kind: 2,
                    }],
                    60 | 61 => vec![Command::CancelJob { player: 0, unit: hall }],
                    _ => vec![],
                };
                w.step(&cmds);
                hashes.push(w.state_hash());
            }
            hashes
        };
        assert_eq!(one(), one());
    }

    #[test]
    fn the_queue_survives_a_save_and_the_loaded_game_runs_the_same_twice() {
        let (mut a, hall) = town(true);
        for _ in 0..3 {
            train(&mut a, hall);
        }
        run(&mut a, 30);
        let bytes = a.save_state();
        let load = || {
            let (mut b, _) = town(false);
            b.restore(&bytes, 1).unwrap();
            b
        };
        let (mut b, mut c) = (load(), load());
        assert_eq!(b.ext.queue, a.ext.queue);
        assert_eq!(b.waiting(hall).len(), 2);
        run(&mut b, 2000);
        run(&mut c, 2000);
        assert_eq!(b.state_hash(), c.state_hash());
        assert!(b.waiting(hall).is_empty());
        assert_eq!(
            peasants(&b),
            peasants(&a) + 3,
            "all three came out, one of them already running at the save"
        );
    }
}
