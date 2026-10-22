//! The computer player (reviewed spec `docs/specs/ai/computer-player.md`, the model of the whole), and its script
//! interpreter (`docs/specs/ai/ai-scripts.md`).
//!
//! A computer player is a set of small deciders that never plan ahead. Each runs at a fixed moment, reads the game
//! state and a few per-player numbers, and gives ordinary commands. This module holds the timeline ("One cycle, in
//! order") and the hooks; each part's rules live in the module of its spec ("The parts"):
//!
//! | Part | Phase | Code |
//! | --- | --- | --- |
//! | Script interpreter | 2 | `World::script_steps`, here (`ai/ai-scripts.md`) |
//! | Pass A | 2 | `World::pass_a`: the threat map and the attack waves (`attack_ai.rs`, `ai/attack-ai.md`) |
//! | No-workers pass | 2 | `World::no_workers_research_pass` (`training_ai.rs`, `ai/training-research.md`) |
//! | Pass B | 2 | `World::pass_b`: the base boxes, then each unit's caster step and goal (`attack_ai.rs`, `caster_ai.rs`) |
//! | Building think | 4 | `World::building_think_tick`, from order 33 in `World::advance` (`training_ai.rs`) |
//! | Job choice | 4 | `World::job_choice`, from idle (order 2) and at a come-out (`worker_ai.rs`, `ai/worker-ai.md`) |
//! | Creation hook | where created | `World::creation_hook`, `World::site_hook` |
//! | Destroyed hook | 4, in the kill | `World::destroyed_hook` |
//! | Damage hook | 4, where hit | `World::damage_hook`, from the apply-damage step |
//! | Unit-level rules | 4 | the ordinary order logic of every unit (below) |
//!
//! The unit-level rules that differ for a computer player's units ("Unit-level AI") sit in the order logic they
//! change: the acquisition box (`World::search`), fleeing only from idle (`World::flee`), the smart-move wait cap
//! (`World::cap_smart_wait`), friendly fire (`World::holds_fire`), the attacker record and remembered patrol kept by
//! the computer's own commands (`World::command_4`, `World::patrol`), boarding (`transport.rs`), walls
//! (`movement.rs`), the missile's minimum flight (`battle.rs`), fog (`vision.rs`), payment at the decision
//! (`build.rs`) and the come-out (`harvest.rs`).
//!
//! "Computer player" is the movement spec's computer player (computer slots and active rescue players,
//! `World::computer_player`); only players 0 to 7 have script slots, so only they run the deciders
//! (`World::ai_player`).

use std::sync::Arc;

use tidewater_formats::aibin::AiFile;
use tidewater_formats::udta::flags;

use crate::economy::kind;
use crate::world::{UnitId, World};

/// Players that have a script.
pub const SCRIPT_PLAYERS: usize = 8;
/// Cycles between two runs of the 50-cycle passes; the countdown starts at 1, so the first run is in the first cycle.
pub const PASS_PERIOD: u32 = 50;
/// Slot holding the build-list limit.
pub const BUILD_LIMIT: usize = 0x22;
/// Slot switching the ferry's landing-spot search; 1 for every player at game start (ai-scripts spec, "Slots").
pub const FERRY_SEARCH: usize = 0x0c;
/// Instructions one run may carry out before we park the script. Not from the spec: a guard against a script that
/// loops without ever waiting, which would otherwise hang the cycle.
const MAX_INSTRUCTIONS: u32 = 10_000;

/// Map controller values (`OWNR`, PUD specification).
pub mod controller {
    /// Passive computer: game setup gives it to slot 15 and to passive rescue players once the map's units exist.
    pub const PASSIVE: u8 = 2;
    pub const NOBODY: u8 = 3;
    pub const COMPUTER: u8 = 4;
    pub const HUMAN: u8 = 5;
    pub const RESCUE_PASSIVE: u8 = 6;
    pub const RESCUE_ACTIVE: u8 = 7;

    pub fn is_rescue(c: u8) -> bool {
        matches!(c, RESCUE_PASSIVE | RESCUE_ACTIVE)
    }
}

/// Unit types counted as workers: peasant, peon and both attack versions.
const WORKERS: [u8; 4] = [0x02, 0x03, 0x10, 0x11];
const SHIPYARDS: [u8; 2] = [0x48, 0x49];

/// One player's script state ("Per-player state").
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct AiPlayer {
    /// Cycles left before the script runs again. `u32::MAX` with no script also never runs.
    pub wait: u32,
    /// File offset of the next instruction; `None` when the player has no script or its script went wrong.
    pub pos: Option<u32>,
    /// Slots by number. Kept apart from the wait counter and position, so a script writing slots 0 to 8 or above
    /// 0x22 (an open question in the spec; no shipped script does) only changes slots.
    #[serde(with = "crate::bigarray")]
    pub slots: [u8; 256],
    pub build_list: u16,
    pub second_list: u16,
    /// Worker AI bookkeeping (worker-ai spec): the hall cooldown, workers per job, buildings in progress by type,
    /// the build list's done marks (bit n for entry n) and research requests (bit e - 0x80).
    pub hall_cooldown: u8,
    pub repairers: u16,
    pub miners: u16,
    pub lumberjacks: u16,
    #[serde(with = "crate::bigarray")]
    pub in_progress: [u16; 128],
    pub done: u64,
    pub research_requests: u128,
}

impl Default for AiPlayer {
    fn default() -> Self {
        Self {
            wait: 0,
            pos: None,
            slots: [0; 256],
            build_list: 0,
            second_list: 0,
            hall_cooldown: 0,
            repairers: 0,
            miners: 0,
            lumberjacks: 0,
            in_progress: [0; 128],
            done: 0,
            research_requests: 0,
        }
    }
}

/// Live unit counts used by the conditions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub workers: u32,
    pub land: u32,
    pub sea: u32,
    pub air: u32,
}

impl World {
    // ---- One cycle, in order --------------------------------------------------------------------------------------

    /// Phase 2 of a cycle ("One cycle, in order", steps 1 and 2): every computer player's script step, then, in a
    /// 50-cycle pass cycle, pass A, the no-workers pass and pass B. Commands given here take effect through the units'
    /// own updates in phase 4 of the same cycle.
    pub(crate) fn computer_players(&mut self) {
        self.script_steps();
        if self.pass_cycle() {
            self.pass_a();
            self.no_workers_research_pass();
            self.pass_b();
        }
    }

    /// Whether this cycle runs the 50-cycle passes: the first start-up cycle, then every 50 cycles
    /// (`ai/attack-ai.md`, "Timing").
    fn pass_cycle(&mut self) -> bool {
        self.army.countdown = self.army.countdown.saturating_sub(1);
        if self.army.countdown > 0 {
            return false;
        }
        self.army.countdown = PASS_PERIOD;
        true
    }

    /// A computer player with script slots, players 0 to 7: the players whose deciders run. The controller must be
    /// computer (code 4): an active rescue player (code 7) becomes one only once the map's units are created
    /// (`game/setup.md`, "After the units"), so while they are created it isn't one, and its map units get no first
    /// goal (`ai/attack-ai.md`, "When a unit is created"; `ai/computer-player.md`, round 83). Code 7 exists only then.
    pub fn ai_player(&self, owner: u8) -> bool {
        (owner as usize) < SCRIPT_PLAYERS
            && self
                .players
                .get(owner as usize)
                .is_some_and(|p| p.controller == controller::COMPUTER)
    }

    // ---- Hooks ----------------------------------------------------------------------------------------------------

    /// The creation hook for every unit and building of any player (`core/unit-slots.md`, "Creating a unit", step 6):
    /// the goal is cleared and a computer player's mobile fighting unit gets its first goal.
    pub(crate) fn creation_hook(&mut self, i: usize) {
        self.first_goal(i);
    }

    /// The creation hook's marks for a building whose construction starts in play: the build list's done mark
    /// (`ai/worker-ai.md`, "Done marks": a site marks its entry when construction starts) and the keep-clear creation
    /// mark ("Creation marks"). The map's buildings get theirs at game start ([`World::start_ai`]).
    pub(crate) fn site_hook(&mut self, i: usize) {
        let (owner, kind) = (self.units[i].owner, self.units[i].kind);
        self.mark_build_list(owner, kind, true);
        self.creation_mark(i);
    }

    /// The destroyed hook, inside the kill of building `ti` of any player, once its death has begun: a computer
    /// player's entry for it loses its done mark (`ai/worker-ai.md`, "Done marks"), and the losses rule runs
    /// ([`World::building_lost`]). `site` is whether it was under construction when the kill began.
    pub(crate) fn destroyed_hook(&mut self, ti: usize, site: bool) {
        let (owner, kind) = (self.units[ti].owner, self.units[ti].kind);
        self.mark_build_list(owner, kind, false);
        self.building_lost(owner, ti, site);
    }

    /// The damage hook, when unit `ti` of any player is notified of a hit by `attacker` (`combat/damage.md`,
    /// "Applying damage"): the attacker record, the submarine call and the call for help (`ai/attack-ai.md`, "Call
    /// for help").
    pub(crate) fn damage_hook(&mut self, ti: usize, attacker: UnitId) {
        self.call_for_help(ti, attacker);
    }

    // ---- The script interpreter -----------------------------------------------------------------------------------

    /// Loads the script file and starts every player's script ("Game start"): each of players 0 to 7, whatever its
    /// controller, runs until its first pause. The openings run **before the map's units are placed** (round 94,
    /// `ai/ai-scripts.md`, "Game start"; traced: AS-V1 to AS-V3), so every count a condition reads is 0: the units
    /// are set aside while they run, which makes conditions 0, 1, 2 and 7 false, workers and armies 0, and
    /// conditions 3 to 6 true only when their product is 0. The map's units then count from the first start-up cycle.
    pub fn start_ai(&mut self, file: Arc<AiFile>) {
        self.ai_file = Some(file.clone());
        let units = std::mem::take(&mut self.units);
        for p in 0..SCRIPT_PLAYERS {
            let mut s = AiPlayer::default();
            s.slots[BUILD_LIMIT] = 0xff;
            if let Some(r) = file.record(self.players[p].ai_script) {
                s.build_list = r.build_list;
                s.second_list = r.second_list;
                s.pos = Some(r.start as u32);
            }
            self.ai[p] = s;
            self.run_script(p);
            // Attack-ai spec, "Slots": set after the opening run, so an opening write is overwritten.
            self.ai[p].slots[FERRY_SEARCH] = 1;
        }
        self.units = units;
        for p in 0..SCRIPT_PLAYERS {
            self.mark_game_start(p);
        }
    }

    /// Step 1 of the cycle: one script step for each computer-controlled player 0 to 7, in ascending order ("Each
    /// cycle"): its wait counter goes down by 1, or, at 0, its script runs until it pauses. The test is the computer
    /// controller (code 4). An active rescue player (code 7) has become one once the map's units were created
    /// (`game/setup.md`, "After the units"; `ai/computer-player.md`: "a computer player" from then on), so it steps
    /// its script like any other computer player.
    pub(crate) fn script_steps(&mut self) {
        if self.ai_file.is_none() {
            return;
        }
        for p in 0..SCRIPT_PLAYERS {
            if self.players[p].controller != controller::COMPUTER {
                continue;
            }
            if self.ai[p].wait > 0 {
                self.ai[p].wait -= 1;
            } else {
                self.run_script(p);
            }
        }
    }

    /// Carries out instructions until the wait counter is not 0 ("Running a script").
    fn run_script(&mut self, p: usize) {
        let Some(file) = self.ai_file.clone() else { return };
        let mut budget = MAX_INSTRUCTIONS;
        while self.ai[p].wait == 0 {
            let Some(pos) = self.ai[p].pos else { return };
            let at = pos as usize;
            budget -= 1;
            let next = match file.u8(at) {
                _ if budget == 0 => None,
                Some(0) => match (file.u8(at + 1), file.u8(at + 2)) {
                    (Some(slot), Some(value)) => {
                        self.ai[p].slots[slot as usize] = value;
                        Some(pos + 3)
                    }
                    _ => None,
                },
                Some(1) => file.u16(at + 1).map(u32::from),
                Some(2) => file.u32(at + 1).map(|n| {
                    self.ai[p].wait = n;
                    pos + 5
                }),
                Some(3) => file.u8(at + 1).map(|c| {
                    self.ai[p].wait = 1;
                    if self.condition(p, c) { pos + 2 } else { pos }
                }),
                _ => None,
            };
            // A script that runs off the file or meets an unknown opcode stops for good.
            self.ai[p].pos = next;
        }
    }

    /// The player's live counts ("Conditions"): workers include held units; armies count complete, unheld units
    /// with a land, air, explode, sea or critter bit and the can-attack bit.
    pub fn ai_counts(&self, p: usize) -> Counts {
        let mut c = Counts::default();
        for u in self
            .units
            .iter()
            .filter(|u| u.owner as usize == p && u.hp > 0 && !u.dying)
        {
            if WORKERS.contains(&u.kind) {
                c.workers += 1;
            }
            let f = self.stats_of(u.kind).flags;
            let mobile = flags::LAND | flags::AIR | flags::EXPLODE_WHEN_KILLED | flags::SEA | flags::CRITTER;
            if u.held || u.construction.is_some() || f & mobile == 0 || f & flags::CAN_ATTACK == 0 {
                continue;
            }
            if f & (flags::AIR | flags::SEA | flags::PEON) == 0 {
                c.land += 1;
            } else if f & flags::SEA != 0 {
                c.sea += 1;
            } else if f & flags::AIR != 0 {
                c.air += 1;
            }
        }
        c
    }

    /// Whether player `p` has a completed building of one of `kinds`.
    pub(crate) fn has_complete(&self, p: usize, kinds: &[u8]) -> bool {
        self.units.iter().any(|u| {
            u.owner as usize == p && u.hp > 0 && !u.dying && u.construction.is_none() && kinds.contains(&u.kind)
        })
    }

    fn condition(&self, p: usize, c: u8) -> bool {
        let slot = |n: usize| self.ai[p].slots[n] as u32;
        match c {
            0 => self.has_complete(p, &SHIPYARDS),
            1 => self.has_complete(p, &[kind::KEEP, kind::STRONGHOLD]),
            2 => self.has_complete(p, &[kind::CASTLE, kind::FORTRESS]),
            3 => self.ai_counts(p).workers >= slot(0x13),
            4 => self.ai_counts(p).land >= slot(0x0d) * slot(0x0e),
            5 => self.ai_counts(p).sea >= slot(0x0f) * slot(0x10),
            6 => self.ai_counts(p).air >= slot(0x11) * slot(0x12),
            // Round-60 errata: by the alliance table, in which a player counts as allied with itself, so its own
            // workers never count (`human11-30000` n = 22113: allied player 3's peasants don't count either).
            7 => (0..SCRIPT_PLAYERS).any(|q| !self.allied(p as u8, q as u8) && self.ai_counts(q).workers > 0),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::world;

    /// A file whose script 0 is `code`, placed at offset 256 + 4.
    fn file(code: &[u8]) -> Arc<AiFile> {
        let mut d = vec![0u8; 256];
        d[0..2].copy_from_slice(&256u16.to_le_bytes());
        d.extend([0x40, 0x01, 0x50, 0x01]);
        d.extend(code);
        Arc::new(AiFile::parse(d).unwrap())
    }

    const START: u32 = 260;

    fn set(slot: u8, v: u8) -> [u8; 3] {
        [0, slot, v]
    }
    fn wait(n: u32) -> Vec<u8> {
        let mut v = vec![2];
        v.extend(n.to_le_bytes());
        v
    }

    /// The test world's player 0 as a computer with script 0, and nobody else with a script.
    fn computer(code: &[u8]) -> World {
        let mut w = world();
        for (i, p) in w.players.iter_mut().enumerate() {
            p.controller = if i == 0 {
                controller::COMPUTER
            } else {
                controller::NOBODY
            };
            p.ai_script = if i == 0 { 0 } else { 0xff };
        }
        w.start_ai(file(code));
        w
    }

    /// Round 94, AS-V1 to AS-V3: the opening runs before the map's units exist, so a "wait until" on a unit count
    /// fails at game start even when the map places the units, and the units come back afterwards.
    #[test]
    fn the_opening_runs_as_if_no_units_exist() {
        let mut w = world();
        let units = w.units.len();
        let mut code = vec![3, 1];
        code.extend(set(0x22, 7));
        w.players[0].controller = controller::COMPUTER;
        w.players[0].ai_script = 0;
        w.start_ai(file(&code));
        assert_eq!(w.units.len(), units);
        // Condition 1 (a keep or stronghold) is false with no units, so the script waits on it and the write is unreached.
        assert_eq!(w.ai[0].wait, 1);
        assert_eq!(w.ai[0].pos, Some(START));
        assert_eq!(w.ai[0].slots[BUILD_LIMIT], 0xff);
    }

    #[test]
    fn runs_until_the_first_pause_at_game_start() {
        let mut code = vec![];
        code.extend(set(0x13, 2));
        code.extend(set(0x18, 1));
        code.extend(wait(65535));
        code.extend([1, (START & 0xff) as u8 + 6, 1]);
        let w = computer(&code);
        let s = &w.ai[0];
        assert_eq!((s.slots[0x13], s.slots[0x18], s.slots[BUILD_LIMIT]), (2, 1, 0xff));
        assert_eq!(s.slots[FERRY_SEARCH], 1);
        assert_eq!((s.build_list, s.second_list), (0x140, 0x150));
        assert_eq!(s.wait, 65535);
        assert_eq!(s.pos, Some(START + 11));
    }

    #[test]
    fn a_wait_of_n_resumes_n_plus_1_cycles_later() {
        // Wait 0 doesn't pause; wait 500 run at start resumes on the 501st step.
        let mut code = wait(0);
        code.extend(wait(500));
        code.extend(set(0x14, 7));
        code.extend(wait(1000));
        let mut w = computer(&code);
        for _ in 0..500 {
            w.step(&[]);
        }
        assert_eq!(w.ai[0].slots[0x14], 0);
        w.step(&[]);
        assert_eq!(w.ai[0].slots[0x14], 7);
    }

    #[test]
    fn wait_until_costs_two_cycles_and_retests_every_two() {
        // Condition 3: workers >= slot 0x13. The test world has no workers, so ask for 1, then lower the bar.
        let mut code = vec![];
        code.extend(set(0x13, 1));
        code.extend([3, 3]);
        code.extend(set(0x14, 9));
        code.extend(wait(1000));
        let mut w = computer(&code);
        let at = w.ai[0].pos;
        assert_eq!((w.ai[0].wait, at), (1, Some(START + 3)));
        w.step(&[]); // counter -> 0
        assert_eq!(w.ai[0].slots[0x14], 0);
        w.ai[0].slots[0x13] = 0;
        w.step(&[]); // tested again: true, pause 1
        assert_eq!(w.ai[0].slots[0x14], 0);
        w.step(&[]); // counter -> 0
        w.step(&[]); // goes on
        assert_eq!(w.ai[0].slots[0x14], 9);
    }

    #[test]
    fn non_computer_players_run_only_at_start() {
        let mut code = vec![];
        code.extend(set(0x14, 1));
        code.extend(wait(1));
        code.extend(set(0x14, 2));
        code.extend(wait(1000));
        let mut w = world();
        for p in w.players.iter_mut() {
            p.controller = controller::HUMAN;
            p.ai_script = 0;
        }
        w.start_ai(file(&code));
        for _ in 0..10 {
            w.step(&[]);
        }
        assert!(w.ai.iter().all(|s| s.slots[0x14] == 1));
    }

    #[test]
    fn army_counts_follow_the_flags() {
        let mut w = world(); // one footman-like unit (type 0) for player 0
        let peasant = flags::LAND | flags::PEON;
        w.stats.0[0x02].flags = peasant;
        // The attack peasant has no peon flag, so it joins the land army.
        w.stats.0[0x10].flags = flags::LAND | flags::CAN_ATTACK;
        w.stats.0[0x1e].flags = flags::SEA | flags::CAN_ATTACK;
        w.stats.0[0x2b].flags = flags::AIR | flags::CAN_ATTACK;
        w.stats.0[0x2a].flags = flags::AIR;
        for k in [0x02, 0x10, 0x1e, 0x2b, 0x2a] {
            w.stats.0[k].size = (1, 1);
            w.stats.0[k].hit_points = 10;
        }
        w.spawn(0, 0, (1, 0), 0).unwrap();
        w.spawn(0x02, 0, (2, 0), 0).unwrap();
        w.spawn(0x10, 0, (3, 0), 0).unwrap();
        w.spawn(0x2a, 0, (0, 2), 0).unwrap();
        w.spawn(0x2b, 0, (2, 2), 0).unwrap();
        assert_eq!(
            w.ai_counts(0),
            Counts {
                workers: 2,
                land: 3,
                sea: 0,
                air: 1
            }
        );
        // Held units count as workers but in no army; the rescue mark changes nothing.
        w.players[0].controller = controller::RESCUE_PASSIVE;
        assert_eq!(w.ai_counts(0).land, 3);
        for u in &mut w.units {
            u.held = true;
        }
        assert_eq!(
            w.ai_counts(0),
            Counts {
                workers: 2,
                ..Default::default()
            }
        );
    }

    #[test]
    fn conditions_read_the_slots() {
        let mut w = computer(&wait(1000));
        // Condition 4 with slot 0x0D = 0: product 0, always true.
        assert!(w.condition(0, 4));
        w.ai[0].slots[0x0d] = 2;
        w.ai[0].slots[0x0e] = 1;
        assert!(!w.condition(0, 4)); // one land unit
        w.spawn(0, 0, (1, 0), 0).unwrap();
        assert!(w.condition(0, 4));
        // Condition 7: another player has a worker.
        assert!(!w.condition(0, 7));
        w.stats.0[0x03].size = (1, 1);
        w.stats.0[0x03].hit_points = 10;
        w.spawn(0x03, 3, (6, 6), 0).unwrap();
        assert!(w.condition(0, 7));
        assert!(!w.condition(0, 0));
    }

    /// Round-60 errata, condition 7: a player counts as allied with itself, so its own workers never count, and an
    /// allied player's workers don't either (`human11-30000`: player 4's allied passive player 3 has peasants, and
    /// the condition stays false).
    #[test]
    fn condition_7_skips_the_player_itself_and_its_allies() {
        let mut w = computer(&wait(1000));
        w.stats.0[0x03].size = (1, 1);
        w.stats.0[0x03].hit_points = 10;
        w.players[0].allies = 1 << 2;
        w.spawn(0x03, 0, (1, 1), 0).unwrap();
        assert!(!w.condition(0, 7), "own workers only");
        w.spawn(0x03, 2, (2, 2), 0).unwrap();
        assert!(!w.condition(0, 7), "own and an ally's workers");
        w.spawn(0x03, 3, (3, 3), 0).unwrap();
        assert!(w.condition(0, 7), "player 3 isn't allied");
    }

    #[test]
    fn players_above_7_and_bad_scripts_never_run() {
        let mut w = computer(&[9]);
        assert_eq!(w.ai[0].pos, None);
        w.step(&[]);
        assert_eq!(w.ai[0].pos, None);
        // A jump to itself is cut off by the guard instead of hanging.
        let w = computer(&[1, (START & 0xff) as u8, (START >> 8) as u8]);
        assert_eq!(w.ai[0].pos, None);
    }

    /// Pins of the whole computer player, written before the B-038 AI refactor onto `docs/specs/ai/computer-player.md`
    /// so that it can't change them unseen: the whole simulation state with the queued events and sound cues, folded
    /// over every cycle of a run ([`crate::world::tests::trail`]).
    mod pin {
        use super::*;
        use crate::world::tests::{field, trail};
        use tidewater_formats::udta::UnitStats;

        const FOOTMAN: u8 = 0x00;
        const GRUNT: u8 = 0x01;
        const PEASANT: u8 = 0x02;
        const FARM: u8 = 0x3a;
        const BARRACKS: u8 = 0x3c;
        const HALL: u8 = 0x4a;
        const MINE: u8 = 0x5c;

        /// The script file: script 0 sets wanted counts and the land wave's size, then loops: wait until the land
        /// army is big enough, trigger the land wave, wait 150. Its build list is farm, barracks, farm; its second
        /// list gives every building type interval 1.
        fn script() -> Arc<AiFile> {
            let mut d = vec![0u8; 256];
            d[0..2].copy_from_slice(&256u16.to_le_bytes());
            d.extend(384u16.to_le_bytes());
            d.extend(416u16.to_le_bytes());
            for (slot, v) in [(0x13, 5), (0x14, 3), (0x15, 2), (0x0d, 2), (0x0e, 1)] {
                d.extend([0, slot, v]);
            }
            let again = d.len() as u16;
            d.extend([3, 4]);
            d.extend([0, 0x09, 1]);
            d.extend([2, 150, 0, 0, 0]);
            d.push(1);
            d.extend(again.to_le_bytes());
            d.resize(384, 0);
            d.extend([FARM, BARRACKS, FARM, 0xff]);
            d.resize(416, 0xff);
            for _ in 0x3a..0x6a {
                d.extend(1u16.to_le_bytes());
            }
            Arc::new(AiFile::parse(d).unwrap())
        }

        /// A 64 x 64 field: computer player 1 with a town hall, a barracks, 3 peasants and a gold mine nearby;
        /// human player 0 with footmen and a town hall far away.
        fn town() -> World {
            let mut w = field(64, 64);
            for (i, p) in w.players.iter_mut().enumerate() {
                p.controller = match i {
                    0 => controller::HUMAN,
                    1 => controller::COMPUTER,
                    _ => controller::NOBODY,
                };
                p.ai_script = if i == 1 { 0 } else { 0xff };
            }
            let unit = |hp, f| UnitStats {
                hit_points: hp,
                size: (1, 1),
                flags: f,
                gold: 100,
                build_time: 20,
                basic_damage: 6,
                piercing_damage: 3,
                attack_range: 1,
                missile: crate::combat::missile::NONE,
                can_target: 1,
                react_range_computer: 6,
                react_range_human: 4,
                sight: 5,
                ..Default::default()
            };
            let building = |hp, size, f| UnitStats {
                hit_points: hp,
                size: (size, size),
                flags: flags::BUILDING | f,
                gold: 300,
                lumber: 100,
                build_time: 30,
                sight: 3,
                ..Default::default()
            };
            let s = &mut w.stats.0;
            s[FOOTMAN as usize] = unit(60, flags::LAND | flags::CAN_ATTACK);
            s[GRUNT as usize] = unit(60, flags::LAND | flags::CAN_ATTACK);
            s[PEASANT as usize] = unit(30, flags::LAND | flags::PEON);
            s[PEASANT as usize + 1] = unit(30, flags::LAND | flags::PEON);
            s[FARM as usize] = building(400, 2, 0);
            s[FARM as usize + 1] = building(400, 2, 0);
            s[BARRACKS as usize] = building(800, 3, 0);
            s[HALL as usize] = building(1200, 4, flags::GOLD_STORAGE | flags::WOOD_STORAGE);
            s[MINE as usize] = building(25500, 3, flags::MINE);
            let p = &mut w.players[1];
            (p.gold, p.lumber) = (5000, 5000);
            p.units_allowed = !0;
            p.had_hall = true;
            w.spawn(HALL, 1, (10, 10), 0).unwrap();
            w.spawn(BARRACKS, 1, (4, 18), 0).unwrap();
            w.spawn(MINE, crate::slots::NEUTRAL, (20, 10), 20_000).unwrap();
            for k in 0..3 {
                w.spawn(PEASANT, 1, (9 + k, 15), 0).unwrap();
            }
            w.spawn(HALL, 0, (54, 54), 0).unwrap();
            for k in 0..3 {
                w.spawn(FOOTMAN, 0, (48 + k, 50), 0).unwrap();
            }
            w.start_ai(script());
            w
        }

        /// Script steps, the 50-cycle passes, the job choice, the building thinks and a land wave together.
        #[test]
        fn the_whole_computer_player() {
            let mut w = town();
            // Round 85 (CP14): a build job writes its site into the worker's goal tile.
            assert_eq!(trail(&mut w, 2500), 17108286320118587105);
            // The run did what it is for: the build list was built, workers and footmen were trained and mine, and
            // land waves went out.
            let kinds: Vec<u8> = w.units.iter().filter(|u| u.owner == 1).map(|u| u.kind).collect();
            assert_eq!(kinds, [0, 58, 0, 2, 2, 58, 58, 2, 2, 2, 60, 74]);
            assert_eq!(
                (w.ai[1].done, w.ai[1].miners, w.army.launched[1]),
                (0b111, 5, [7, 0, 0])
            );
        }

        /// The same town under attack: a footman goes for the peasants at the mine, and then the player's buildings
        /// are destroyed one by one, every 150 cycles (done marks cleared, rebuilding, the losses rule).
        #[test]
        fn the_computer_player_under_attack() {
            let mut w = town();
            w.spawn(FOOTMAN, 0, (17, 12), 0).unwrap();
            let mut got = vec![trail(&mut w, 600)];
            while let Some(i) = w.units.iter().position(|u| u.owner == 1 && u.building && !u.dying) {
                w.kill(i);
                got.push(trail(&mut w, 150));
            }
            // Round 85 (CP14): every build job writes its site into the worker's goal tile, from the first farm on.
            assert_eq!(
                got,
                [
                    3213507426331524001,
                    1571374425642285923,
                    1846234581298391719,
                    9171750915480468239,
                    10450163163031200530,
                    14025304314889512792,
                    11107217635340028515,
                    14010802749727734365,
                    // Round 83 (CP5): the losses rule's all-out attack sends the peasants off in order 4, where
                    // they now make their job choice. Round 84: the walk after that choice heads for the job.
                    2491769101848385577,
                    7973506156342654525
                ]
            );
        }
    }
}
