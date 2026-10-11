//! Construction, training, building upgrades and food. Spec: `docs/specs/economy/build-train.md`.
//!
//! Not yet here: workers walking to a site and going inside (orders spec), research (the upgrade table and spell
//! effects), which building may make which unit and its requirements (not yet specified, so the caller decides),
//! completed walls becoming terrain (the wall graphic joining isn't specified), and the free tile a trained unit is
//! placed on (a placeholder search until the free-tile rule is approved).

use tidewater_formats::script::anim;
use tidewater_formats::udta::{UnitStats, flags};

use crate::world::{SpawnError, UnitId, World};

/// Unit types by their PUD numbers (public PUD specification).
pub mod kind {
    pub const SKELETON: u8 = 0x37;
    pub const DAEMON: u8 = 0x38;
    pub const CRITTER: u8 = 0x39;
    pub const FARM: u8 = 0x3a;
    pub const PIG_FARM: u8 = 0x3b;
    pub const TOWN_HALL: u8 = 0x4a;
    pub const GREAT_HALL: u8 = 0x4b;
    pub const KEEP: u8 = 0x58;
    pub const STRONGHOLD: u8 = 0x59;
    pub const CASTLE: u8 = 0x5a;
    pub const FORTRESS: u8 = 0x5b;

    /// Town halls and their upgrades, of either race.
    pub fn is_hall(k: u8) -> bool {
        matches!(k, TOWN_HALL | GREAT_HALL | KEEP | STRONGHOLD | CASTLE | FORTRESS)
    }
}

/// Enhanced mode's cap on food supply.
const FOOD_CAP: u32 = 200;
/// Construction passes per step.
const PASS_STEP: u32 = 12;
/// The first wait of a job taking over, not adjusted for haste or slow (animation-scripts spec, "Running a script").
const TAKEOVER_WAIT: u8 = 3;

/// What stopped a unit, building or upgrade from starting ("Can the player afford it?").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Shortage {
    Food,
    Gold,
    Lumber,
    Oil,
}

/// Things the player is told. Not game state: the client drains them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    CantAfford {
        player: u8,
        shortage: Shortage,
    },
    /// A finished unit couldn't be placed; its cost was given back.
    NoRoom {
        player: u8,
        kind: u8,
    },
}

/// Gold, lumber and oil.
pub type Cost = [u32; 3];

/// The real cost of a unit or building type: the stored values times 10 ("Costs in the data").
pub fn cost(s: &UnitStats) -> Cost {
    [s.gold as u32 * 10, s.lumber as u32 * 10, s.oil as u32 * 10]
}

/// A building under construction ("Construction").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Construction {
    /// The pass counter P.
    pub passes: u32,
    /// Build time T.
    pub time: u32,
    /// Hit points still to add at creation, D.
    pub to_add: u32,
    /// Steps taken, and hit points given so far.
    pub steps: u32,
    pub added: u32,
}

impl Construction {
    pub fn total(&self) -> u32 {
        PASS_STEP * (self.time + 1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobKind {
    Train(u8),
    /// The building becomes this type.
    Upgrade(u8),
}

/// A building's one job ("Training, research and building upgrades").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Job {
    pub kind: JobKind,
    pub steps: u16,
    pub progress: u16,
    /// Whether the work animation has taken over.
    pub running: bool,
    pub paid: Cost,
}

impl World {
    /// Food supply, capped at 200 in Enhanced mode: 4 per completed farm, 1 per completed hall ("Food").
    pub fn food_supply(&self, owner: u8) -> u32 {
        let supply: u32 = self
            .units
            .iter()
            .filter(|u| u.owner == owner && u.construction.is_none() && !u.dying)
            .map(|u| match u.kind {
                kind::FARM | kind::PIG_FARM => 4,
                k if kind::is_hall(k) => 1,
                _ => 0,
            })
            .sum();
        if self.rules.enhanced {
            supply.min(FOOD_CAP)
        } else {
            supply
        }
    }

    /// Food in use: the owner's units that aren't buildings, less skeletons, daemons and critters.
    pub fn food_used(&self, owner: u8) -> u32 {
        self.units
            .iter()
            .filter(|u| u.owner == owner && !u.building)
            .filter(|u| !matches!(u.kind, kind::SKELETON | kind::DAEMON | kind::CRITTER))
            .filter(|u| self.stats_of(u.kind).flags & flags::CRITTER == 0)
            .count() as u32
    }

    /// The affordability checks in order: food (for mobile units), then gold, lumber and oil.
    pub fn can_afford(&self, owner: u8, cost: Cost, needs_food: bool) -> Result<(), Shortage> {
        if needs_food && self.food_supply(owner) <= self.food_used(owner) {
            return Err(Shortage::Food);
        }
        let p = &self.players[owner as usize];
        for (have, (need, s)) in [p.gold, p.lumber, p.oil].into_iter().zip(cost.into_iter().zip([
            Shortage::Gold,
            Shortage::Lumber,
            Shortage::Oil,
        ])) {
            if have < need {
                return Err(s);
            }
        }
        Ok(())
    }

    fn pay(&mut self, owner: u8, c: Cost) {
        let p = &mut self.players[owner as usize];
        p.gold -= c[0];
        p.lumber -= c[1];
        p.oil -= c[2];
    }

    fn refund(&mut self, owner: u8, c: Cost) {
        let p = &mut self.players[owner as usize];
        p.gold += c[0];
        p.lumber += c[1];
        p.oil += c[2];
    }

    fn afford_or_tell(&mut self, owner: u8, c: Cost, needs_food: bool) -> bool {
        match self.can_afford(owner, c, needs_food) {
            Ok(()) => true,
            Err(shortage) => {
                self.events.push(Event::CantAfford {
                    player: owner,
                    shortage,
                });
                false
            }
        }
    }

    /// Creates a building under construction at `tile`: H₀ hit points, the remaining D to add, and the fast first
    /// hall in Enhanced multiplayer ("Construction", "At creation"). The cost is the caller's to take.
    pub fn place_building(&mut self, kind: u8, owner: u8, tile: (i32, i32)) -> Result<UnitId, SpawnError> {
        let s = self.stats_of(kind);
        let m = (s.hit_points as u32).max(1);
        let mut time = s.build_time as u32;
        let p = self.players[owner as usize];
        if self.rules.enhanced && self.rules.multiplayer && !p.had_hall && s.flags & flags::GOLD_STORAGE != 0 {
            let farm = if p.race == 1 { kind::PIG_FARM } else { kind::FARM };
            time = time.min(self.stats_of(farm).build_time as u32);
        }
        if self.rules.enhanced {
            time = time.max(1);
        }
        let id = self.spawn(kind, owner, tile, 0)?;
        let h0 = (m / 10).min(40);
        let i = self.index(id).expect("just spawned");
        let u = &mut self.units[i];
        u.hp = h0 as u16;
        u.construction = Some(Construction {
            time,
            to_add: m - h0,
            ..Default::default()
        });
        self.players[owner as usize].under_construction += 1;
        Ok(id)
    }

    /// The construction pass for one building, in slot order after the missiles ("The construction pass").
    pub(crate) fn construct(&mut self, i: usize) {
        let cheat = self.rules.build_speed_cheat;
        let u = &mut self.units[i];
        let Some(c) = &mut u.construction else { return };
        if u.dying {
            return;
        }
        c.passes += if cheat { PASS_STEP } else { 1 };
        if !c.passes.is_multiple_of(PASS_STEP) {
            return;
        }
        let total = c.total();
        if c.passes > total {
            self.complete(i);
            return;
        }
        if c.passes == total {
            return;
        }
        c.steps += 1;
        let given = (c.steps as u64 * c.to_add as u64 / c.time.max(1) as u64) as u32;
        let m = self.stats.0.get(u.kind as usize).map_or(1, |s| s.hit_points.max(1));
        u.hp = (u.hp as u32 + given - c.added).min(m as u32) as u16;
        c.added = given;
        if u.hp >= m {
            u.hp = m;
            c.passes = total;
        }
    }

    /// Construction progress as a fraction of 255: the larger of passes over the total and hit points over M.
    pub fn construction_progress(&self, id: UnitId) -> Option<u8> {
        let u = self.unit(id)?;
        let c = u.construction?;
        let m = (self.stats_of(u.kind).hit_points as u32).max(1);
        let by_passes = c.passes.min(c.total()) * 255 / c.total();
        Some(by_passes.max(u.hp as u32 * 255 / m).min(255) as u8)
    }

    fn complete(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.construction = None;
        let (owner, k) = (u.owner as usize, u.kind);
        self.start_anim(i, anim::STILL);
        let p = &mut self.players[owner];
        p.under_construction -= 1;
        if kind::is_hall(k) {
            p.had_hall = true;
        }
    }

    /// Starts a job at a completed, idle building: pays at once, then the work animation takes over at the still
    /// animation's next "step done". The caller decides whether the building may make it.
    pub(crate) fn start_job(&mut self, i: usize, job: JobKind) {
        let u = &self.units[i];
        if !u.building || u.dying || u.construction.is_some() || u.job.is_some() {
            return;
        }
        let owner = u.owner;
        let (made, needs_food) = match job {
            JobKind::Train(k) => (k, true),
            JobKind::Upgrade(k) => (k, false),
        };
        let s = self.stats_of(made);
        let paid = cost(&s);
        if !self.afford_or_tell(owner, paid, needs_food) {
            return;
        }
        self.pay(owner, paid);
        if matches!(job, JobKind::Train(_)) {
            self.players[owner as usize].in_training += 1;
        }
        self.units[i].job = Some(Job {
            kind: job,
            steps: 2 * s.build_time as u16,
            progress: 0,
            running: false,
            paid,
        });
    }

    /// A building's reaction to its script: jobs take over at a "step done" and advance once per work loop.
    pub(crate) fn work(&mut self, i: usize, step_done: bool) {
        if !step_done || self.units[i].construction.is_some() {
            return;
        }
        let cheat = self.rules.build_speed_cheat;
        let u = &mut self.units[i];
        let Some(job) = &mut u.job else { return };
        if !job.running {
            job.running = true;
            self.start_anim(i, anim::WORK_A);
            self.units[i].script.wait = TAKEOVER_WAIT;
            return;
        }
        if job.progress < job.steps {
            job.progress = if cheat {
                (job.progress + 10).min(job.steps)
            } else {
                job.progress + 1
            };
            return;
        }
        let job = *job;
        self.finish_job(i, job);
    }

    fn end_job(&mut self, i: usize, job: Job) {
        self.units[i].job = None;
        if job.running {
            self.start_anim(i, anim::STILL);
        }
        if matches!(job.kind, JobKind::Train(_)) {
            self.players[self.units[i].owner as usize].in_training -= 1;
        }
    }

    fn finish_job(&mut self, i: usize, job: Job) {
        self.end_job(i, job);
        let u = self.units[i].clone();
        match job.kind {
            JobKind::Train(k) => {
                let placed = self
                    .free_tile_near(&u, k)
                    .ok_or(SpawnError::CantPlace)
                    .and_then(|t| self.spawn(k, u.owner, t, 0));
                if placed.is_err() {
                    self.refund(u.owner, job.paid);
                    self.events.push(Event::NoRoom {
                        player: u.owner,
                        kind: k,
                    });
                }
            }
            JobKind::Upgrade(k) => {
                let (old, new) = (self.stats_of(u.kind).hit_points, self.stats_of(k).hit_points);
                let b = &mut self.units[i];
                b.kind = k;
                b.hp = (b.hp as i32 + new as i32 - old as i32).max(1) as u16;
            }
        }
    }

    /// Cancels a building's job (full refund) or, if it is under construction, the building itself: it is destroyed
    /// and part of its cost comes back ("Cancelling construction").
    pub(crate) fn cancel(&mut self, i: usize) {
        let u = &self.units[i];
        if u.dying {
            return;
        }
        let owner = u.owner;
        if let Some(job) = u.job {
            self.end_job(i, job);
            self.refund(owner, job.paid);
            return;
        }
        if u.construction.is_none() {
            return;
        }
        let s = self.stats_of(u.kind);
        let part = |v: u16| v as u32 * 30 / 4;
        let oil = if self.rules.enhanced { part(s.oil) } else { 0 };
        self.refund(owner, [part(s.gold), part(s.lumber), oil]);
        self.kill(i);
    }

    /// A placeholder until the free-tile rule is approved: the first tile, ring by ring outward from the building's
    /// footprint (up to 12 tiles), each ring scanned row by row from the top-left, where a unit of type `k` fits.
    fn free_tile_near(&self, b: &crate::world::Unit, k: u8) -> Option<(i32, i32)> {
        let layer = self.layer_of(k);
        let (x0, y0) = b.tile;
        let (x1, y1) = (x0 + b.size.0 as i32 - 1, y0 + b.size.1 as i32 - 1);
        for r in 1..=12 {
            for y in y0 - r..=y1 + r {
                for x in x0 - r..=x1 + r {
                    let on_ring = x == x0 - r || x == x1 + r || y == y0 - r || y == y1 + r;
                    if on_ring && self.occupant(layer, x, y).is_none() && self.terrain_allows(layer, x, y) {
                        return Some((x, y));
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Command;
    use crate::world::tests::{run, world};

    const HALL: u8 = kind::TOWN_HALL;

    /// The test world with a completed hall for player 0 at (5, 6) and plenty of resources.
    fn town() -> (World, UnitId) {
        let mut w = world();
        let hall = w.spawn(HALL, 0, (5, 6), 0).unwrap();
        let p = &mut w.players[0];
        (p.gold, p.lumber, p.oil) = (10_000, 10_000, 10_000);
        (w, hall)
    }

    #[test]
    fn construction_vectors() {
        let (mut w, _) = town();
        w.stats.0[0x3e].hit_points = 400;
        w.stats.0[0x3e].build_time = 100;
        let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
        let hp = |w: &World| w.unit(b).unwrap().hp;
        assert_eq!(hp(&w), 40);
        assert_eq!(w.players[0].under_construction, 1);
        let mut at = Vec::new();
        for pass in 1..=1212 {
            w.step(&[]);
            if [12, 600, 1200].contains(&pass) {
                at.push(hp(&w));
            }
            if pass < 1212 {
                assert!(w.unit(b).unwrap().construction.is_some(), "pass {pass}");
            }
        }
        assert_eq!(at, [43, 220, 400]);
        let u = w.unit(b).unwrap();
        assert_eq!((u.construction, u.script.anim), (None, anim::STILL));
        assert_eq!(w.players[0].under_construction, 0);
    }

    #[test]
    fn damaged_construction_completes_later() {
        let (mut w, _) = town();
        w.stats.0[0x3e].hit_points = 400;
        w.stats.0[0x3e].build_time = 100;
        let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
        run(&mut w, 30 * 12);
        let i = w.index(b).unwrap();
        w.units[i].hp -= 10;
        run(&mut w, 1212 - 360);
        let u = w.unit(b).unwrap();
        assert_eq!((u.hp, w.construction_progress(b)), (390, Some(255)));
        assert!(u.construction.is_some());
        run(&mut w, 11);
        assert!(w.unit(b).unwrap().construction.is_some());
        run(&mut w, 1);
        assert_eq!(w.unit(b).unwrap().construction, None);
        assert_eq!(w.unit(b).unwrap().hp, 390);
    }

    #[test]
    fn small_and_large_buildings() {
        let (mut w, _) = town();
        w.stats.0[0x3e].hit_points = 1200;
        w.stats.0[0x3e].build_time = 255;
        let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
        assert_eq!(w.unit(b).unwrap().hp, 40);
        run(&mut w, 3071);
        assert!(w.unit(b).unwrap().construction.is_some());
        run(&mut w, 1);
        assert!(w.unit(b).unwrap().construction.is_none());
        w.stats.0[0x3e].hit_points = 60;
        let c = w.place_building(0x3e, 0, (0, 2)).unwrap();
        assert_eq!(w.unit(c).unwrap().hp, 6);
    }

    #[test]
    fn cancel_refunds_three_quarters() {
        for (enhanced, oil) in [(false, 0), (true, 75)] {
            let (mut w, _) = town();
            w.rules.enhanced = enhanced;
            let s = &mut w.stats.0[0x3e];
            (s.gold, s.lumber, s.oil) = (50, 25, 10);
            let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
            w.step(&[Command::Cancel { player: 0, unit: b }]);
            let p = w.players[0];
            assert_eq!((p.gold, p.lumber, p.oil), (10_375, 10_187, 10_000 + oil));
            assert!(w.unit(b).unwrap().dying);
            assert_eq!(p.under_construction, 0);
        }
    }

    #[test]
    fn food() {
        let (mut w, _) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.spawn(kind::FARM, 0, (2, 2), 0).unwrap();
        assert_eq!(w.food_supply(0), 9);
        for x in 0..7 {
            w.spawn(0, 0, (x, 5), 0).unwrap();
        }
        assert_eq!(w.food_used(0), 8);
        assert_eq!(w.can_afford(0, [0; 3], true), Ok(()));
        w.spawn(0, 0, (7, 5), 0).unwrap();
        assert_eq!(w.can_afford(0, [0; 3], true), Err(Shortage::Food));
        w.spawn(kind::SKELETON, 0, (0, 4), 0).unwrap();
        w.spawn(kind::SKELETON, 0, (1, 4), 0).unwrap();
        assert_eq!(w.food_used(0), 9);
        // Gold comes before lumber.
        w.players[0].gold = 0;
        w.players[0].lumber = 0;
        assert_eq!(w.can_afford(0, [1, 1, 0], false), Err(Shortage::Gold));
        // Farms under construction don't count; the Enhanced cap does.
        w.place_building(kind::FARM, 0, (4, 6)).unwrap();
        assert_eq!(w.food_supply(0), 9);
        for _ in 0..60 {
            w.spawn(kind::FARM, 0, (0, 0), 0).unwrap();
        }
        assert_eq!(w.food_supply(0), 200);
        w.rules.enhanced = false;
        assert_eq!(w.food_supply(0), 249);
    }

    #[test]
    fn training_takes_13_plus_5n_cycles_after_takeover() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[0].build_time = 6;
        w.stats.0[0].gold = 40;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 0,
        }]);
        assert_eq!(w.players[0].gold, 9_600);
        assert_eq!(w.players[0].in_training, 1);
        // Wait for the take-over at the still animation's next "step done".
        let mut took = 0;
        for t in 1..=10 {
            w.step(&[]);
            if w.unit(hall).unwrap().script.anim == anim::WORK_A {
                took = t;
                break;
            }
        }
        assert!(took > 0);
        let n = w.units.len();
        run(&mut w, 13 + 5 * 12 - 1);
        assert_eq!(w.units.len(), n, "not yet");
        run(&mut w, 1);
        assert_eq!(w.units.len(), n + 1);
        assert_eq!(w.players[0].in_training, 0);
        assert_eq!(w.unit(hall).unwrap().job, None);
    }

    #[test]
    fn cancelling_a_job_refunds_it_all() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[0].gold = 40;
        w.stats.0[0].build_time = 60;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 0,
        }]);
        run(&mut w, 20);
        w.step(&[Command::Cancel { player: 0, unit: hall }]);
        assert_eq!((w.players[0].gold, w.players[0].in_training), (10_000, 0));
        assert_eq!(w.unit(hall).unwrap().script.anim, anim::STILL);
    }

    #[test]
    fn short_of_food_is_told() {
        let (mut w, hall) = town();
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 0,
        }]);
        assert_eq!(
            w.events,
            [Event::CantAfford {
                player: 0,
                shortage: Shortage::Food
            }]
        );
        assert_eq!(w.unit(hall).unwrap().job, None);
    }

    #[test]
    fn upgrades_keep_damage() {
        let (mut w, hall) = town();
        w.stats.0[kind::TOWN_HALL as usize].hit_points = 1200;
        w.stats.0[kind::KEEP as usize].hit_points = 1400;
        w.stats.0[kind::KEEP as usize].build_time = 1;
        let i = w.index(hall).unwrap();
        w.units[i].hp = 1000;
        w.step(&[Command::Upgrade {
            player: 0,
            building: hall,
            to: kind::KEEP,
        }]);
        run(&mut w, 40);
        let u = w.unit(hall).unwrap();
        assert_eq!((u.kind, u.hp), (kind::KEEP, 1200));
    }
}
