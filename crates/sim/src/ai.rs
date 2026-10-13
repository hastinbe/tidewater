//! Computer player scripts (reviewed spec `docs/specs/ai/ai-scripts.md`).
//!
//! Each of players 0 to 7 has a small script that writes numbers into per-player slots (wanted unit counts, the
//! build-list limit, attack settings) and waits on conditions. It runs once for every player at game start and then
//! in phase 2 of each cycle for computer-controlled players only. Nothing here acts on the slots yet: the training,
//! building and attack logic that reads them comes with later specs.

use std::sync::Arc;

use tidewater_formats::aibin::AiFile;
use tidewater_formats::udta::flags;

use crate::economy::kind;
use crate::world::World;

/// Players that have a script.
pub const SCRIPT_PLAYERS: usize = 8;
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
    pub in_progress: [u8; 128],
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
    /// Loads the script file and starts every player's script ("Game start"): each of players 0 to 7, whatever its
    /// controller, runs until its first pause.
    pub fn start_ai(&mut self, file: Arc<AiFile>) {
        self.ai_file = Some(file.clone());
        for p in 0..SCRIPT_PLAYERS {
            let mut s = AiPlayer::default();
            s.slots[BUILD_LIMIT] = 0xff;
            if let Some(r) = file.record(self.players[p].ai_script) {
                s.build_list = r.build_list;
                s.second_list = r.second_list;
                s.pos = Some(r.start as u32);
            }
            self.ai[p] = s;
            self.mark_game_start(p);
            self.run_script(p);
            // Attack-ai spec, "Slots": set after the opening run, so an opening write is overwritten.
            self.ai[p].slots[FERRY_SEARCH] = 1;
        }
    }

    /// Phase 2: one step for each computer-controlled player 0 to 7, in ascending order ("Each cycle").
    pub(crate) fn step_ai(&mut self) {
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
}
