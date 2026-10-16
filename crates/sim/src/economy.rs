//! Construction, training, research, building upgrades and food. Specs: `docs/specs/economy/build-train.md` and
//! `docs/specs/economy/tech-tree.md` (the checks a job command runs, and what finishing or cancelling research does;
//! the tables are in `tech.rs`).
//!
//! "The model": a building made during play is in one of three states ([`BuildingState`], worked out from its
//! construction and its job), and each event that moves it is one procedure:
//!
//! | Event | Procedure |
//! | --- | --- |
//! | a worker arrives at its site | [`World::raise`] (`build.rs`) and [`World::place_building`] |
//! | the construction pass reaches the end | [`World::construct`], then `complete` |
//! | a cancel-construction command | [`World::cancel`], which takes the building down ([`World::take_down`]) |
//! | a job command | [`World::try_job`] |
//! | the building's work animation | [`World::work`], ending in `end_job` |
//! | a cancel-job command | [`World::cancel`], which marks the job; `end_job` at the next work run |
//! | the building is killed | `kill_as` (`battle.rs`): [`World::cancel_job`], then [`World::kill_inside`] |
//!
//! Its shared rules are [`World::job_cost`] and [`World::unit_cost`] (paying), [`World::take_down`] against the kill
//! (two ways a building goes), [`World::exit_tile`] (letting a unit out) and the local-owner test of `sound.rs`.

use tidewater_formats::script::anim;
use tidewater_formats::udta::{UnitStats, flags};
use tidewater_formats::ugrd::Upgrade;

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
/// The shown construction progress (out of 255) from which the second construction-site frame shows ("Look").
const LOOK_STAGE: u8 = 64;
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
    /// A worker arrived at its site but the unit table was full: the table's own "too many units" message
    /// ("Placement on arrival", step 3; `core/unit-slots.md`).
    TooManyUnits {
        player: u8,
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
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// "The model": the state of a building made during play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingState {
    /// Order 33, with the builder (or a platform's tanker) inside.
    UnderConstruction,
    /// Complete, resting in order 33 (a guard or cannon tower: 14).
    Resting,
    /// Complete, working on one job in order 37.
    Working,
}

impl crate::world::Unit {
    /// The building's state ("The model"), from its construction and its job; `None` for a unit that isn't a
    /// building.
    pub fn building_state(&self) -> Option<BuildingState> {
        if !self.building {
            return None;
        }
        Some(match (self.construction, self.job) {
            (Some(_), _) => BuildingState::UnderConstruction,
            (None, Some(_)) => BuildingState::Working,
            (None, None) => BuildingState::Resting,
        })
    }
}

/// A building's one job ("Training, research and building upgrades").
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Job {
    pub kind: JobKind,
    pub steps: u16,
    pub progress: u16,
    /// Whether the work animation has taken over.
    pub running: bool,
    /// The [`World::job_cost`] of the job's item, looked up when it was paid (a conversion looks it up again for the
    /// new type). A cancelled training or building-upgrade job refunds this; the spec looks the cost up at the
    /// refund, the same amount while the unit data doesn't change during a game.
    pub paid: Cost,
    /// Marked by a cancel-job command: the job ends cancelled at its next work run ("Cancelling a job").
    pub cancelled: bool,
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

    /// Food in use: the owner's units that aren't buildings, less skeletons, daemons and critters. A killed unit stops
    /// counting at the start of its death animation ("Food").
    pub fn food_used(&self, owner: u8) -> u32 {
        self.units
            .iter()
            .filter(|u| u.owner == owner && !u.building && !u.dying)
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

    /// "The local player sees it", for messages: a message about a player's own building or unit is queued with that
    /// player, for every player, and the client shows only the local player's ([`World::local_owner`]).
    pub(crate) fn tell(&mut self, e: Event) {
        self.events.push(e);
    }

    pub(crate) fn afford_or_tell(&mut self, owner: u8, c: Cost, needs_food: bool) -> bool {
        match self.can_afford(owner, c, needs_food) {
            Ok(()) => true,
            Err(shortage) => {
                self.tell(Event::CantAfford {
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
        // Worker-ai spec, "Done marks" (round 36): a computer player's building marks its entry when it is created,
        // so a construction site marks it when construction starts.
        self.mark_build_list(owner, kind, true);
        // "Creation marks" (round 38), made with the done mark.
        self.creation_mark(i);
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
        // Step 1: with the cheat on, the multiple-of-12 test isn't made, so every pass is a step (V65).
        c.passes += if cheat { PASS_STEP } else { 1 };
        if !cheat && !c.passes.is_multiple_of(PASS_STEP) {
            return;
        }
        let total = c.total();
        if c.passes > total {
            self.complete(i);
            return;
        }
        if c.passes == total {
            // Shown as full: the mana byte keeps the shown progress (step 6).
            self.show_construction(i);
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
        self.show_construction(i);
        self.construction_sound(i);
    }

    /// Step 6, "Look": the shown progress goes into the mana byte, and the frame is 0 below 64/255 and 1 from there.
    fn show_construction(&mut self, i: usize) {
        let Some(p) = self.shown_progress(i) else {
            return;
        };
        let u = &mut self.units[i];
        u.mana = p;
        u.script.frame = u8::from(p >= LOOK_STAGE);
    }

    /// The construction progress the progress bar shows, as a fraction of 255: the value kept in the mana byte, which
    /// changes only at the construction steps (step 6; V63), so it is 0 from creation until the first step.
    pub fn construction_progress(&self, id: UnitId) -> Option<u8> {
        let u = self.unit(id)?;
        u.construction.map(|_| u.mana)
    }

    /// Step 5, the shown progress: the larger of passes over the total and hit points over M, as a fraction of 255.
    fn shown_progress(&self, i: usize) -> Option<u8> {
        let u = &self.units[i];
        let c = u.construction?;
        let m = (self.stats_of(u.kind).hit_points as u32).max(1);
        let by_passes = c.passes.min(c.total()) * 255 / c.total();
        Some(by_passes.max(u.hp as u32 * 255 / m).min(255) as u8)
    }

    fn complete(&mut self, i: usize) {
        let u = &mut self.units[i];
        u.construction = None;
        // "Completion": the frame and the mana byte go back to 0.
        u.script.frame = 0;
        u.mana = 0;
        let (owner, k) = (u.owner as usize, u.kind);
        self.start_anim(i, anim::STILL);
        let p = &mut self.players[owner];
        p.under_construction -= 1;
        if kind::is_hall(k) {
            p.had_hall = true;
        }
        self.complete_builders(i);
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

    /// "Paying": the cost lookup for a unit or building type, from the unit data. Building a building, training a
    /// unit and a building upgrade all look up their type's cost here.
    pub fn unit_cost(&self, k: u8) -> Cost {
        cost(&self.stats_of(k))
    }

    /// "Paying": the one cost lookup for a job's item, made when it is paid and again when it is refunded: the unit
    /// data's cost of the type trained or become, or the upgrade data's cost of the upgrade or spell.
    pub fn job_cost(&self, job: JobKind) -> Cost {
        match job {
            JobKind::Train(k) | JobKind::Upgrade(k) => self.unit_cost(k),
            JobKind::Research(n) | JobKind::Spell(n) => {
                let up = self.upgrade_entry(n);
                [up.gold as u32, up.lumber as u32, up.oil as u32]
            }
        }
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
        if u.dying || u.building_state() != Some(BuildingState::Resting) {
            return false;
        }
        let (owner, b) = (u.owner, u.kind);
        // 2 and 3. The kind's own check, and how long the job takes.
        let (steps, needs_food) = match job {
            JobKind::Train(k) => {
                if !self.may_train(owner, b, k) {
                    return false;
                }
                (2 * self.stats_of(k).build_time as u16, true)
            }
            JobKind::Upgrade(to) => {
                if !self.may_become(owner, b, to) {
                    return false;
                }
                (2 * self.stats_of(to).build_time as u16, false)
            }
            JobKind::Research(n) | JobKind::Spell(n) => {
                if !World::may_research(b, n) {
                    return false;
                }
                // 4. The level check, for upgrade numbers 0 to 23.
                if matches!(job, JobKind::Research(_)) && !self.level_ok(owner, n) {
                    return false;
                }
                (2 * self.upgrade_entry(n).time as u16, false)
            }
        };
        let paid = self.job_cost(job);
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
        let u = &mut self.units[i];
        u.job = Some(Job {
            kind: job,
            steps,
            progress: 0,
            running: false,
            paid,
            cancelled: false,
        });
        // "Starting": the mana byte shows 1, and a building upgrade shows frame 1 of the new type's picture
        // ([`crate::world::Unit::picture`]).
        u.mana = 1;
        if matches!(job, JobKind::Upgrade(_)) {
            u.script.frame = 1;
        }
        true
    }

    /// A building's reaction to its script: jobs take over at a "step done" and advance once per work loop.
    pub(crate) fn work(&mut self, i: usize, step_done: bool) {
        if !step_done || self.units[i].building_state() != Some(BuildingState::Working) {
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
        // "Cancelling a job" (static study): a job marked to be cancelled ends, cancelled, at this run, even if its
        // counter has reached the total.
        if !job.cancelled && job.progress < job.steps {
            job.progress = if cheat {
                (job.progress + 10).min(job.steps)
            } else {
                job.progress + 1
            };
            // "Progress": the mana byte holds the shown progress, floor(counter × 255 / total).
            u.mana = (job.progress as u32 * 255 / job.steps as u32) as u8;
            return;
        }
        let finished = !job.cancelled;
        self.end_job(i, finished, true);
    }

    /// "Job end", finished or cancelled: the job goes, the mana byte goes to 0 (and a building upgrade's frame), the
    /// still animation returns, the training count goes back down, and then the job's own ending runs (the "Finished"
    /// or "Cancelled" column). At a work run (`take_over`), order 33 takes over in the same update with wait 3 (Orders
    /// spec, "Taking over"; Human04 vectors), its logic first running at the still animation's next step done.
    /// Returns whether the building had a job.
    fn end_job(&mut self, i: usize, finished: bool, take_over: bool) -> bool {
        let Some(job) = self.units[i].job else { return false };
        let u = &mut self.units[i];
        u.job = None;
        u.mana = 0;
        if matches!(job.kind, JobKind::Upgrade(_)) {
            u.script.frame = 0;
        }
        if job.running {
            self.start_anim(i, anim::STILL);
        }
        if matches!(job.kind, JobKind::Train(_)) {
            self.players[self.units[i].owner as usize].in_training -= 1;
        }
        if take_over {
            self.units[i].script.wait = TAKEOVER_WAIT;
        }
        let u = self.units[i].clone();
        match job.kind {
            JobKind::Research(_) | JobKind::Spell(_) => self.end_research(i, job.kind, finished),
            _ if !finished => self.refund(u.owner, job.paid),
            JobKind::Train(k) => {
                let placed = self
                    .exit_tile(&u, k)
                    .ok_or(SpawnError::CantPlace)
                    .and_then(|t| self.spawn(k, u.owner, t, 0));
                match placed {
                    Ok(id) => self.ready_sound(id),
                    Err(_) => {
                        self.refund(u.owner, job.paid);
                        self.tell(Event::NoRoom {
                            player: u.owner,
                            kind: k,
                        });
                    }
                }
            }
            JobKind::Upgrade(k) => {
                let (old, new) = (self.stats_of(u.kind).hit_points, self.stats_of(k).hit_points);
                let b = &mut self.units[i];
                b.kind = k;
                b.hp = (b.hp as i32 + new as i32 - old as i32).max(1) as u16;
                // Round-40 errata (`human06-tech`, building 1516 at n = 578): a tower gets order 14, which takes over
                // in the same update (attack animation, wait 3).
                if self.stats_of(k).flags & flags::TOWER != 0 {
                    self.take_over(i, crate::world::Order::TowerWatch { target: None });
                }
                // Sounds spec, "Work complete" (round 76, V51: `human06-alerts-1400`): in Enhanced rules, only for a
                // hall becoming a keep or castle (or the orc versions); a tower plays nothing.
                if self.rules.enhanced && kind::is_hall(k) {
                    self.work_complete(i);
                }
            }
        }
        true
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
            self.refund(owner, self.job_cost(kind));
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
            self.tell(Event::ResearchDone { player: owner, tile });
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
        let to_cost = self.unit_cost(to);
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

    /// The two cancel commands, each acting only in its own state (V69): cancel job (`job`) marks a working
    /// building's job to be cancelled, and it ends at its next work run ("Cancelling a job"); cancel construction
    /// takes a building under construction down, part of its cost coming back ("Cancelling construction").
    pub(crate) fn cancel(&mut self, i: usize, job: bool) {
        let u = &mut self.units[i];
        match (u.building_state(), job) {
            _ if u.dying => {}
            (Some(BuildingState::Working), true) => {
                if let Some(job) = &mut u.job {
                    job.cancelled = true;
                }
            }
            (Some(BuildingState::UnderConstruction), false) => self.take_down(i, true),
            _ => {}
        }
    }

    /// Ends a building's job cancelled at once, when the building is killed or changes owner ("A building killed",
    /// "Cancelling a job"): "Job end" with the cancelled column's full refund to the owner it has now. Returns whether
    /// it had one.
    ///
    /// Kept as the code is (an open question): no take-over follows (the spec's "Job end" has order 33 take over
    /// with wait 3); a killed building starts its death animation, a rescued one keeps the still animation's wait.
    pub(crate) fn cancel_job(&mut self, i: usize) -> bool {
        self.end_job(i, false, false)
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

    /// V63: the shown construction progress (the bar's value) is the mana byte, which changes only at the steps, 12
    /// passes apart: 0 from creation until the first step, though the pass counter moves.
    #[test]
    fn the_construction_bar_changes_only_at_steps() {
        let (mut w, _) = town();
        w.stats.0[0x3e].hit_points = 400;
        w.stats.0[0x3e].build_time = 100;
        let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
        let mut shown = vec![w.construction_progress(b)];
        for _ in 0..36 {
            w.step(&[]);
            shown.push(w.construction_progress(b));
        }
        assert_eq!(shown[0], Some(0));
        for pass in 1..=36 {
            let changed = shown[pass] != shown[pass - 1];
            assert_eq!(changed, pass % 12 == 0, "pass {pass}: {:?}", shown);
        }
    }

    /// V65: a farm site at P = 5 when the build-speed cheat goes on steps at P = 17, 29, 41, ... and completes.
    #[test]
    fn the_cheat_makes_every_pass_a_step() {
        let (mut w, _) = town();
        w.stats.0[0x3e].hit_points = 400;
        w.stats.0[0x3e].build_time = 100;
        let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
        run(&mut w, 5);
        let passes = |w: &World| w.unit(b).unwrap().construction.map(|c| c.passes);
        assert_eq!(passes(&w), Some(5));
        w.rules.build_speed_cheat = true;
        let mut hp = w.unit(b).unwrap().hp;
        for want in [17, 29, 41] {
            w.step(&[]);
            assert_eq!(passes(&w), Some(want));
            let now = w.unit(b).unwrap().hp;
            assert!(now > hp, "a step at P = {want}");
            hp = now;
        }
        run(&mut w, 100);
        assert_eq!(w.unit(b).unwrap().construction, None, "complete");
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
        // Round-47 errata: a killed unit stops counting at the start of its death animation.
        let doomed = w.spawn(0, 0, (2, 4), 0).unwrap();
        assert_eq!(w.food_used(0), 10);
        let d = w.index(doomed).unwrap();
        w.kill(d);
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
        w.step(&[Command::CancelJob { player: 0, unit: hall }]);
        // "Cancelling a job": the command only marks the job; it ends at the next work run, within 5 cycles once
        // the runs have begun, with order 33 taking over (still animation, wait 3).
        assert_eq!((w.players[0].gold, w.players[0].in_training), (9_600, 1));
        assert!(w.unit(hall).unwrap().job.is_some_and(|j| j.cancelled));
        let mut ended = 0;
        for t in 1..=5 {
            w.step(&[]);
            if w.unit(hall).unwrap().job.is_none() {
                ended = t;
                break;
            }
        }
        assert!(ended > 0, "ends within 5 cycles");
        assert_eq!((w.players[0].gold, w.players[0].in_training), (10_000, 0));
        let u = w.unit(hall).unwrap();
        assert_eq!((u.script.anim, u.script.wait, u.mana), (anim::STILL, 3, 0));
    }

    /// V69: a cancel-construction command naming a working building, and a cancel-job command naming a site, do
    /// nothing: the job runs on and the site goes on being built.
    #[test]
    fn each_cancel_acts_only_in_its_own_state() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[2].build_time = 60;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
        w.stats.0[kind::FARM as usize].build_time = 100;
        let site = w.place_building(kind::FARM, 0, (5, 0)).unwrap();
        run(&mut w, 20);
        let gold = w.players[0].gold;
        w.step(&[
            Command::Cancel { player: 0, unit: hall },
            Command::CancelJob { player: 0, unit: site },
        ]);
        run(&mut w, 20);
        let (h, s) = (w.unit(hall).unwrap(), w.unit(site).unwrap());
        assert!(h.job.is_some_and(|j| !j.cancelled && j.progress > 0));
        assert!(!s.dying && s.construction.is_some_and(|c| c.passes == 41));
        assert_eq!(w.players[0].gold, gold);
    }

    /// "Cancelling a job": a job marked before its first run ends at that first run, 13 cycles after the work
    /// animation takes over, even when its counter has already reached its total (a job of 0 steps).
    #[test]
    fn a_job_cancelled_before_its_first_run_ends_at_that_run() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[2].gold = 400;
        w.stats.0[2].build_time = 0;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
        w.step(&[Command::CancelJob { player: 0, unit: hall }]);
        let mut took = None;
        let n = w.units.len();
        for t in 1..=40 {
            w.step(&[]);
            let u = w.unit(hall).unwrap();
            if took.is_none() && u.script.anim == anim::WORK_A {
                took = Some(t);
            }
            if u.job.is_none() {
                assert_eq!(took.map(|k| t - k), Some(13));
                break;
            }
        }
        assert_eq!(w.unit(hall).unwrap().job, None);
        assert_eq!((w.units.len(), w.players[0].gold), (n, 10_000));
    }

    /// Round-48 errata, "A building killed during a job" (`human5-10000` great hall 1525, n = 8086): the job is
    /// cancelled at the kill with a full refund, and the mana byte goes to 0.
    #[test]
    fn a_building_killed_during_a_job_refunds_it() {
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[2].gold = 400;
        w.stats.0[2].build_time = 60;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
        run(&mut w, 40);
        assert_eq!(w.players[0].gold, 9_600);
        assert!(w.unit(hall).unwrap().mana > 0);
        let hi = w.index(hall).unwrap();
        w.kill(hi);
        let h = w.unit(hall).unwrap();
        assert_eq!((h.mana, h.job), (0, None));
        assert_eq!((w.players[0].gold, w.players[0].in_training), (10_000, 0));
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
    fn the_mana_byte_shows_a_jobs_progress() {
        // Build-train spec, round-33 vector (Human04 great hall 1550 trains a peon, total 90): 1 from the payment, then
        // floor(counter × 255 / 90) after each run (2, 5, 8, ... 252, 255), and 0 when the unit appears. Frame 0
        // throughout.
        let (mut w, hall) = town();
        w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
        w.stats.0[2].build_time = 45;
        w.step(&[Command::Train {
            player: 0,
            building: hall,
            kind: 2,
        }]);
        let look = |w: &World| {
            let u = w.unit(hall).unwrap();
            (u.mana, u.script.frame)
        };
        assert_eq!(look(&w), (1, 0));
        let n = w.units.len();
        let mut shown = vec![1];
        while w.units.len() == n {
            w.step(&[]);
            let (mana, frame) = look(&w);
            assert_eq!(frame, 0);
            if shown.last() != Some(&mana) {
                shown.push(mana);
            }
        }
        let expected: Vec<u8> = [1]
            .into_iter()
            .chain((1..=90).map(|k| (k * 255 / 90) as u8))
            .chain([0])
            .collect();
        assert_eq!(&shown[..8], [1, 2, 5, 8, 11, 14, 17, 19]);
        assert_eq!(shown, expected);
    }

    #[test]
    fn construction_keeps_its_shown_progress_and_look() {
        // Build-train spec, round-33 vector (orc4 farm 1534): the mana byte holds the shown progress at every step,
        // the frame is 1 from 64/255, and completion sets both back to 0.
        let (mut w, _) = town();
        w.stats.0[0x3e].hit_points = 400;
        w.stats.0[0x3e].build_time = 100;
        let b = w.place_building(0x3e, 0, (5, 0)).unwrap();
        let look = |w: &World| {
            let u = w.unit(b).unwrap();
            (u.mana, u.script.frame)
        };
        assert_eq!(look(&w), (0, 0));
        let mut crossed = false;
        for pass in 1..1212 {
            w.step(&[]);
            let (mana, frame) = look(&w);
            if pass % 12 == 0 {
                assert_eq!(Some(mana), w.construction_progress(b), "pass {pass}");
            }
            assert_eq!(frame, u8::from(mana >= 64), "pass {pass}");
            crossed |= frame == 1;
        }
        assert!(crossed);
        assert_eq!(look(&w), (255, 1));
        w.step(&[]);
        assert_eq!(w.unit(b).unwrap().construction, None);
        assert_eq!(look(&w), (0, 0));
    }

    #[test]
    fn a_building_upgrade_shows_frame_1_of_the_new_picture() {
        // Build-train spec, round-33 vector (xhuman4 tower 1423): frame 1 from the upgrade's start through order 37,
        // frame 0 when the type changes. A cancelled upgrade goes back to frame 0 and the old picture.
        for cancel in [false, true] {
            let (mut w, hall) = town();
            w.rules.enforce_requirements = false;
            w.stats.0[kind::KEEP as usize].build_time = 20;
            w.step(&[Command::Upgrade {
                player: 0,
                building: hall,
                to: kind::KEEP,
            }]);
            let u = w.unit(hall).unwrap();
            assert_eq!((u.kind, u.picture(), u.script.frame, u.mana), (HALL, kind::KEEP, 1, 1));
            run(&mut w, 30);
            let u = w.unit(hall).unwrap();
            assert!(u.job.is_some_and(|j| j.running));
            assert_eq!((u.kind, u.picture(), u.script.frame), (HALL, kind::KEEP, 1));
            if cancel {
                w.step(&[Command::CancelJob { player: 0, unit: hall }]);
                run(&mut w, 5);
            } else {
                run(&mut w, 300);
            }
            let u = w.unit(hall).unwrap();
            let kind = if cancel { HALL } else { kind::KEEP };
            assert_eq!(
                (u.job, u.kind, u.picture(), u.script.frame, u.mana),
                (None, kind, kind, 0, 0),
                "cancel {cancel}"
            );
        }
    }

    /// Sounds spec, "Work complete" (round 76, V51: `human06-alerts-1400`): in Enhanced rules a finished building
    /// upgrade plays it for a hall becoming a keep, not for a scout tower becoming a guard tower.
    #[test]
    fn work_complete_plays_for_a_hall_upgrade_not_a_tower() {
        const SCOUT_TOWER: u8 = 0x40;
        const GUARD_TOWER: u8 = 0x60;
        for (from, to, plays) in [(HALL, kind::KEEP, true), (SCOUT_TOWER, GUARD_TOWER, false)] {
            let (mut w, _) = town();
            w.rules.enhanced = true;
            w.rules.enforce_requirements = false;
            w.vision.local = 0;
            if from == SCOUT_TOWER {
                w.stats.0[from as usize] = w.stats.0[HALL as usize];
                w.stats.0[to as usize] = w.stats.0[HALL as usize];
                w.stats.0[to as usize].flags |= flags::TOWER;
            }
            w.stats.0[to as usize].build_time = 1;
            let b = w.spawn(from, 0, (2, 2), 0).unwrap();
            w.step(&[Command::Upgrade {
                player: 0,
                building: b,
                to,
            }]);
            let mut heard = false;
            for _ in 0..60 {
                w.sounds.cues.clear();
                w.step(&[]);
                heard |= w
                    .sounds
                    .cues
                    .iter()
                    .any(|c| matches!(c.sound, crate::sound::Sound::WorkComplete { .. }));
                if w.unit(b).unwrap().kind == to {
                    break;
                }
            }
            assert_eq!(w.unit(b).unwrap().kind, to);
            assert_eq!(heard, plays, "{from:#x} to {to:#x}");
        }
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

    /// Pins of the current job behaviour, written before the B-038 refactor so that it can't change them unseen. Each
    /// test pins the whole simulation state with [`World::fingerprint`] and the events the client would drain. Where a
    /// pin differs from the spec, its comment says so: those are open questions, not rules.
    mod pin {
        use super::*;

        /// The whole state and the queued events, as one number.
        pub(crate) fn pinned(w: &World) -> u64 {
            format!("{}|{:?}", w.fingerprint(), w.events)
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325, |h, b| {
                    (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
                })
        }

        /// [`town`] with the local player 0, a farm for food and a unit type 2 of build time `time` being trained.
        fn training(time: u8) -> (World, UnitId) {
            let (mut w, hall) = town();
            w.vision.local = 0;
            w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
            w.stats.0[2].gold = 400;
            w.stats.0[2].build_time = time;
            w.step(&[Command::Train {
                player: 0,
                building: hall,
                kind: 2,
            }]);
            (w, hall)
        }

        #[test]
        fn a_trained_unit_finishes() {
            let (mut w, _) = training(2);
            run(&mut w, 80);
            assert_eq!(w.players[0].in_training, 0);
            assert_eq!(pinned(&w), 17728558196850907567);
        }

        #[test]
        fn a_trained_unit_with_no_room() {
            let (mut w, hall) = town();
            w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
            w.stats.0[2].build_time = 1;
            w.stats.0[2].flags = flags::LAND;
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
            run(&mut w, 40);
            assert_eq!(pinned(&w), 666169062581321506);
        }

        #[test]
        fn a_job_cancelled_while_running() {
            let (mut w, hall) = training(60);
            run(&mut w, 30);
            w.step(&[Command::CancelJob { player: 0, unit: hall }]);
            run(&mut w, 10);
            assert_eq!(w.unit(hall).unwrap().job, None);
            assert_eq!(pinned(&w), 4002968534024783746);
        }

        #[test]
        fn a_job_cancelled_before_it_takes_over() {
            let (mut w, hall) = training(60);
            w.step(&[Command::CancelJob { player: 0, unit: hall }]);
            run(&mut w, 30);
            assert_eq!(w.unit(hall).unwrap().job, None);
            assert_eq!(pinned(&w), 12797389103836182310);
        }

        /// A cancel given twice, and a cancel of a resting building, change nothing more.
        #[test]
        fn a_second_cancel_and_a_resting_cancel() {
            let (mut w, hall) = training(60);
            run(&mut w, 30);
            w.step(&[Command::CancelJob { player: 0, unit: hall }]);
            w.step(&[Command::CancelJob { player: 0, unit: hall }]);
            run(&mut w, 10);
            w.step(&[Command::CancelJob { player: 0, unit: hall }]);
            run(&mut w, 10);
            assert_eq!(pinned(&w), 16536490222765927957);
        }

        /// The kill cancels at once: no wait 3 or still animation of "Job end" (the death animation starts).
        #[test]
        fn a_building_killed_while_working() {
            let (mut w, hall) = training(60);
            run(&mut w, 30);
            let hi = w.index(hall).unwrap();
            w.kill(hi);
            run(&mut w, 3);
            assert_eq!(pinned(&w), 282801298655996242);
        }

        /// Code versus spec: rescue cancels at once, leaving the building's animation as it was (the spec's "Job end"
        /// has order 33 take over with wait 3).
        #[test]
        fn a_building_rescued_while_working() {
            let mut got = Vec::new();
            for running in [false, true] {
                let (mut w, hall) = training(60);
                if running {
                    run(&mut w, 30);
                }
                let hi = w.index(hall).unwrap();
                w.rescue_owner_change(hi, 1, false);
                let u = w.unit(hall).unwrap();
                assert_eq!((u.job, u.owner), (None, 1));
                run(&mut w, 3);
                got.push(pinned(&w));
            }
            assert_eq!(got, [2966129253138305198, 10582890044908409729]);
        }

        #[test]
        fn hall_and_tower_upgrades_finish_and_cancel() {
            const SCOUT_TOWER: u8 = 0x40;
            const GUARD_TOWER: u8 = 0x60;
            let mut got = Vec::new();
            for (from, to) in [(HALL, kind::KEEP), (SCOUT_TOWER, GUARD_TOWER)] {
                for cancel in [false, true] {
                    let (mut w, _) = town();
                    w.rules.enhanced = true;
                    w.rules.enforce_requirements = false;
                    w.vision.local = 0;
                    if from == SCOUT_TOWER {
                        w.stats.0[from as usize] = w.stats.0[HALL as usize];
                        w.stats.0[to as usize] = w.stats.0[HALL as usize];
                        w.stats.0[to as usize].flags |= flags::TOWER;
                    }
                    w.stats.0[to as usize].build_time = 2;
                    w.stats.0[to as usize].gold = 700;
                    let b = w.spawn(from, 0, (2, 2), 0).unwrap();
                    let bi = w.index(b).unwrap();
                    w.units[bi].hp = 300;
                    w.step(&[Command::Upgrade {
                        player: 0,
                        building: b,
                        to,
                    }]);
                    run(&mut w, 20);
                    if cancel {
                        w.step(&[Command::CancelJob { player: 0, unit: b }]);
                    }
                    run(&mut w, 40);
                    got.push(pinned(&w));
                }
            }
            assert_eq!(
                got,
                [
                    4199764736764014306,
                    16551267058749962179,
                    10304115314579723997,
                    8704031557821091703
                ]
            );
        }

        /// The computer's own job choice (`tell` false) fails silently on affordability; a player's tells.
        #[test]
        fn a_job_short_of_gold_with_and_without_telling() {
            let mut got = Vec::new();
            for tell in [false, true] {
                let (mut w, hall) = town();
                w.spawn(kind::FARM, 0, (0, 2), 0).unwrap();
                w.stats.0[2].gold = 400;
                w.players[0].gold = 100;
                let hi = w.index(hall).unwrap();
                assert!(!w.try_job(hi, JobKind::Train(2), tell));
                got.push(pinned(&w));
            }
            assert_eq!(got, [13213827380189875725, 14546769762130876319]);
        }
    }
}
