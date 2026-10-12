//! Construction, training, research, building upgrades and food. Specs: `docs/specs/economy/build-train.md` and
//! `docs/specs/economy/tech-tree.md` (the checks a job command runs, and what finishing or cancelling research does;
//! the tables are in `tech.rs`).
//!
//! Not yet here: workers walking to a site and going inside (orders spec), the completion alert's sound and position
//! (a later spec), spell effects, completed walls becoming terrain (the wall graphic joining isn't specified), and the free tile a trained unit is
//! placed on (a placeholder search until the free-tile rule is approved).

use tidewater_formats::script::anim;
use tidewater_formats::udta::{UnitStats, flags};
use tidewater_formats::ugrd::Upgrade;

use crate::placement::{LIMIT_OUT, Source};
use crate::tech;
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
    pub const HUMAN_WALL: u8 = 0x67;
    pub const ORC_WALL: u8 = 0x68;

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
    /// A repair stopped for lack of lumber (checked first) or gold.
    CantRepair {
        player: u8,
        shortage: Shortage,
    },
    /// A finished unit couldn't be placed; its cost was given back.
    NoRoom {
        player: u8,
        kind: u8,
    },
    /// A worker couldn't place a building on its site.
    CantPlace {
        player: u8,
        kind: u8,
    },
    /// Enhanced mode's completion alert: an upgrade or spell finished at `tile` (tech-tree spec, "Finishing or
    /// cancelling research"). Sound and position use are a later spec.
    ResearchDone {
        player: u8,
        tile: (i32, i32),
    },
    /// A player's game ended (victory spec): the local player's outcome screen, or another player knocked out.
    GameOver {
        player: u8,
        outcome: crate::victory::Outcome,
    },
    /// A unit or building changed owner by rescue (rescue spec, "Owner change"): it leaves every player's current
    /// selection.
    OwnerChanged {
        unit: UnitId,
        from: u8,
        to: u8,
    },
    /// A cheat command changed the flags: the notice to show, with `sender` for the multiplayer name prefix (cheats
    /// spec, "Effects when the flags change", step 8).
    Cheat {
        sender: u8,
        notice: crate::cheats::Notice,
    },
    /// The disco flag changed: the game's music track changes (music spec, "Disco cheat").
    Disco {
        on: bool,
    },
    /// The finale cheat: the game ends and a campaign ending starts (not a victory).
    Finale {
        beyond: bool,
    },
}

/// Gold, lumber and oil.
pub type Cost = [u32; 3];

/// The real cost of a unit or building type ("Costs in the data"). The data reader already multiplies the stored
/// tenths by 10, so the stats hold full amounts.
pub fn cost(s: &UnitStats) -> Cost {
    [s.gold as u32, s.lumber as u32, s.oil as u32]
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
    /// Research an upgrade, by upgrade number.
    Research(u8),
    /// Research a spell (or the paladin and ogre-mage upgrades), by upgrade number.
    Spell(u8),
}

impl JobKind {
    /// The upgrade number and whether it is a spell, for the two research kinds.
    fn research(self) -> Option<(u8, bool)> {
        match self {
            JobKind::Research(n) => Some((n, false)),
            JobKind::Spell(n) => Some((n, true)),
            _ => None,
        }
    }
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

    pub(crate) fn pay(&mut self, owner: u8, c: Cost) {
        let p = &mut self.players[owner as usize];
        p.gold -= c[0];
        p.lumber -= c[1];
        p.oil -= c[2];
    }

    pub(crate) fn refund(&mut self, owner: u8, c: Cost) {
        let p = &mut self.players[owner as usize];
        p.gold += c[0];
        p.lumber += c[1];
        p.oil += c[2];
    }

    pub(crate) fn afford_or_tell(&mut self, owner: u8, c: Cost, needs_food: bool) -> bool {
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
        self.construction_sound(i);
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
        self.release_builders(i, true);
        self.mark_build_list(owner as u8, k, true);
        if matches!(k, kind::HUMAN_WALL | kind::ORC_WALL) {
            self.wall_completed(i);
        }
        // Attack-ai spec, "When a unit is completed": a building's goal is cleared.
        self.units[i].goal = Default::default();
    }

    /// A finished wall piece becomes terrain (walls spec, "Completion: the piece becomes terrain"): it is killed as a
    /// building (wall pieces have no death explosion) and gets the used mark, so it leaves no ruin; then its tile turns
    /// to wall in the owner's race style and joins its neighbours.
    fn wall_completed(&mut self, i: usize) {
        let (owner, tile) = (self.units[i].owner as usize, self.units[i].tile);
        self.kill(i);
        self.units[i].used = true;
        let p = &self.players[owner];
        let (orc, human) = (p.race == 1, p.controller == crate::ai::controller::HUMAN);
        self.terrain.build_wall(tile.0, tile.1, orc, human);
    }

    /// The upgrade data entry of upgrade number `n`.
    fn upgrade_entry(&self, n: u8) -> Upgrade {
        self.upgrade_data.0.get(n as usize).copied().unwrap_or_default()
    }

    /// Starts a job at a building when the command arrives ("Starting a job (when the command arrives)"): every
    /// failed check ends it silently, except affordability, which tells the owner. Pays at once; the work animation
    /// takes over at the still animation's next "step done".
    pub(crate) fn start_job(&mut self, i: usize, job: JobKind) {
        self.try_job(i, job, true);
    }

    /// [`World::start_job`], reporting whether the job started. `tell` false (the computer's own decisions) fails
    /// silently on affordability too (training-research spec, "Starting a job").
    pub(crate) fn try_job(&mut self, i: usize, job: JobKind, tell: bool) -> bool {
        // 1. The building is completed and not working.
        let u = &self.units[i];
        if !u.building || u.dying || u.construction.is_some() || u.job.is_some() {
            return false;
        }
        let (owner, b) = (u.owner, u.kind);
        // 2 and 3. The kind's own check, and what the job costs and takes.
        let (paid, steps, needs_food) = match job {
            JobKind::Train(k) => {
                if !self.may_train(owner, b, k) {
                    return false;
                }
                let s = self.stats_of(k);
                (cost(&s), 2 * s.build_time as u16, true)
            }
            JobKind::Upgrade(to) => {
                if !self.may_become(owner, b, to) {
                    return false;
                }
                let s = self.stats_of(to);
                (cost(&s), 2 * s.build_time as u16, false)
            }
            JobKind::Research(n) | JobKind::Spell(n) => {
                if !World::may_research(b, n) {
                    return false;
                }
                let up = self.upgrade_entry(n);
                // 4. The level check, for upgrade numbers 0 to 23.
                if matches!(job, JobKind::Research(_)) && !self.level_ok(owner, n) {
                    return false;
                }
                (
                    [up.gold as u32, up.lumber as u32, up.oil as u32],
                    2 * up.time as u16,
                    false,
                )
            }
        };
        // 5. Affordability.
        let afford = if tell {
            self.afford_or_tell(owner, paid, needs_food)
        } else {
            self.can_afford(owner, paid, needs_food).is_ok()
        };
        if !afford {
            return false;
        }
        // 6. Pay and start.
        self.pay(owner, paid);
        match job {
            JobKind::Train(_) => self.players[owner as usize].in_training += 1,
            JobKind::Research(n) => self.players[owner as usize].upgrades_researching |= self.upgrade_entry(n).affect,
            JobKind::Spell(n) => self.players[owner as usize].spells_researching |= self.upgrade_entry(n).affect,
            JobKind::Upgrade(_) => {}
        }
        self.units[i].job = Some(Job {
            kind: job,
            steps,
            progress: 0,
            running: false,
            paid,
        });
        true
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
                if let Ok(id) = placed {
                    self.ready_sound(id);
                    // Attack-ai spec, "When a unit is completed".
                    if let Some(n) = self.index(id) {
                        self.first_goal(n);
                    }
                }
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
                if self.rules.enhanced {
                    self.work_complete(i);
                }
            }
            JobKind::Research(_) | JobKind::Spell(_) => self.end_research(i, job.kind, true),
        }
    }

    /// A research job ends, finished or cancelled ("Finishing or cancelling research"). If none of the job's
    /// "affects" bits is set in the matching being-researched word, nothing happens at all, not even a refund.
    /// Otherwise the bits are cleared, and then the job takes effect or is refunded in full at the data's current cost.
    fn end_research(&mut self, i: usize, kind: JobKind, finished: bool) {
        let Some((n, spell)) = kind.research() else { return };
        let up = self.upgrade_entry(n);
        let (owner, tile) = (self.units[i].owner, self.units[i].tile);
        let p = &mut self.players[owner as usize];
        let word = if spell {
            &mut p.spells_researching
        } else {
            &mut p.upgrades_researching
        };
        if *word & up.affect == 0 {
            return;
        }
        *word &= !up.affect;
        if !finished {
            self.refund(owner, [up.gold as u32, up.lumber as u32, up.oil as u32]);
            return;
        }
        if spell {
            self.players[owner as usize].spells_known |= up.affect;
            if up.affect == tech::PALADIN_BIT {
                self.paladin_upgrade_finished(owner);
            }
        } else {
            // A group above 10 has no defined effect (open question); the level is left alone.
            if let Some(level) = self.players[owner as usize].levels.get_mut(up.group as usize) {
                *level += 1;
                self.sync_upgrades(owner);
            }
            if up.affect == tech::RANGER_TRAINING_BIT {
                self.convert(owner, tech::unit::ARCHER, tech::unit::RANGER);
                self.convert(owner, tech::unit::AXE_THROWER, tech::unit::BERSERKER);
            }
        }
        if self.rules.enhanced {
            self.events.push(Event::ResearchDone { player: owner, tile });
            self.work_complete(i);
        }
    }

    /// Paladin or ogre-mage upgrade finished, either race's version: holy vision, knights to paladins, the eye of
    /// Kilrogg, ogres to ogre-mages. The two spells come only if the owner is allowed to research them.
    fn paladin_upgrade_finished(&mut self, owner: u8) {
        let grant = |p: &mut crate::world::Player, bit: u32| {
            if p.spells_allowed & bit != 0 {
                p.spells_known |= bit;
            }
        };
        grant(&mut self.players[owner as usize], tech::HOLY_VISION_BIT);
        self.convert(owner, tech::unit::KNIGHT, tech::unit::PALADIN);
        grant(&mut self.players[owner as usize], tech::EYE_OF_KILROGG_BIT);
        self.convert(owner, tech::unit::OGRE, tech::unit::OGRE_MAGE);
    }

    /// The owner's units of type `from` become `to` in place ("Converting units"): hit points go up by the
    /// difference of the maximums (each at least 1), mana is set, and a barracks working on `from` works on `to`.
    pub(crate) fn convert(&mut self, owner: u8, from: u8, to: u8) {
        let to_cost = cost(&self.stats_of(to));
        for i in 0..self.units.len() {
            let u = &mut self.units[i];
            if u.owner != owner {
                continue;
            }
            if u.building {
                if matches!(u.kind, tech::building::HUMAN_BARRACKS | tech::building::ORC_BARRACKS)
                    && let Some(job) = &mut u.job
                    && job.kind == JobKind::Train(from)
                {
                    // The progress is kept and the cost was paid as `from`; a cancel refunds `to`'s cost.
                    job.kind = JobKind::Train(to);
                    job.paid = to_cost;
                }
                continue;
            }
            if u.kind != from || u.dying || u.removing || u.inside {
                continue;
            }
            self.convert_unit(i, to);
        }
    }

    /// One unit becomes type `to` ("Converting units"): hit points go up by the difference of the maximums (each at
    /// least 1), and mana is set.
    pub(crate) fn convert_unit(&mut self, i: usize, to: u8) {
        let max = |k: u8| (self.stats_of(k).hit_points as i32).max(1);
        let delta = max(to) - max(self.units[i].kind);
        let s = self.stats_of(to);
        let u = &mut self.units[i];
        u.kind = to;
        // Our reading: a unit never drops below 1 hit point from a conversion.
        u.hp = (u.hp as i32 + delta).clamp(1, u16::MAX as i32) as u16;
        // Our reading: the unit data's "magic" value is a flag in our reader, so it gives 0 or 1.
        if s.flags & flags::CASTER != 0 {
            u.mana = 85;
            u.mana_counter = 40;
        } else {
            u.mana = u8::from(s.magic);
        }
    }

    /// Copies the level lines into the combat and vision view of the player's upgrades. Both races' versions of a
    /// shared line (longbow and lighter axes, the two scouting upgrades) read the same level.
    pub(crate) fn sync_upgrades(&mut self, owner: u8) {
        let p = &mut self.players[owner as usize];
        let l = p.levels;
        p.upgrades = crate::combat::Upgrades {
            sword: l[1],
            arrow: l[0],
            shield: l[2],
            ship_cannon: l[3],
            ship_armor: l[4],
            siege: l[6],
            marksmanship: l[10] > 0,
            longbow: l[8] > 0,
            lighter_axes: l[8] > 0,
            ranger_scouting: l[9] > 0,
            berserker_scouting: l[9] > 0,
        };
    }

    /// Cancels a building's job (full refund) or, if it is under construction, the building itself: it is destroyed
    /// and part of its cost comes back ("Cancelling construction").
    pub(crate) fn cancel(&mut self, i: usize) {
        if self.units[i].dying || self.cancel_job(i) {
            return;
        }
        let u = &self.units[i];
        if u.construction.is_none() {
            return;
        }
        let owner = u.owner;
        let s = self.stats_of(u.kind);
        // 3/4 of the real cost, with the stats holding full amounts (10 × the stored value).
        let part = |v: u16| v as u32 * 3 / 4;
        let oil = if self.rules.enhanced { part(s.oil) } else { 0 };
        self.refund(owner, [part(s.gold), part(s.lumber), oil]);
        self.kill(i);
    }

    /// Cancels a building's job with a full refund to its owner, as the cancel column of "Ending" says. Returns
    /// whether it had one.
    pub(crate) fn cancel_job(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        let Some(job) = u.job else { return false };
        let owner = u.owner;
        self.end_job(i, job);
        if job.kind.research().is_some() {
            self.end_research(i, job.kind, false);
        } else {
            self.refund(owner, job.paid);
        }
        true
    }

    /// The tile a unit of type `k` comes out on: the ring search of the placement spec around the building's
    /// footprint, aimed at its own top-left tile, so the first tile tried is the one west of that corner. Builders
    /// leaving a site use the same search (`build.rs`).
    fn free_tile_near(&self, b: &crate::world::Unit, k: u8) -> Option<(i32, i32)> {
        let src = Source::new(b.tile.0, b.tile.1, b.size.0 as i32, b.size.1 as i32);
        self.find_place(k, src, b.tile, LIMIT_OUT)
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
            (s.gold, s.lumber, s.oil) = (500, 250, 100);
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
        w.stats.0[2].build_time = 6;
        w.stats.0[2].gold = 400;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
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
        w.stats.0[2].gold = 400;
        w.stats.0[2].build_time = 60;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
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
            kind: 2,
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
        // This test is about hit points, not the barracks the keep upgrade needs.
        w.rules.enforce_requirements = false;
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

    #[test]
    fn a_trained_unit_comes_out_west_of_the_hall() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[2].build_time = 1;
        w.stats.0[2].gold = 400;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
        run(&mut w, 200);
        let u = w
            .units
            .iter()
            .find(|u| u.kind == 2 && u.slot != 0)
            .expect("trained unit");
        assert_eq!(u.tile, (4, 6));
    }

    #[test]
    fn a_trained_unit_with_no_room_is_refunded() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[2].build_time = 1;
        w.stats.0[2].gold = 400;
        w.stats.0[2].flags = flags::LAND;
        // Everything within reach blocks land units.
        for n in 0..w.terrain.words.len() {
            if w.occupancy[0][n].is_none() {
                w.terrain.words[n] |= crate::terrain::bits::OBSTACLE;
            }
        }
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
        assert_eq!(w.players[0].gold, 9_600);
        let n = w.units.len();
        run(&mut w, 200);
        assert_eq!(w.units.len(), n);
        assert_eq!(w.players[0].gold, 10_000);
        assert_eq!(w.players[0].in_training, 0);
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, Event::NoRoom { player: 0, kind: 2 }))
        );
    }
}
