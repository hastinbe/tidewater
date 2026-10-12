//! Build-list done marks made before play (reviewed specs `docs/specs/ai/worker-ai.md`, "Done marks", and
//! `docs/specs/ai/training-research.md`, "Research requests and done marks").
//!
//! Research entries (0x80 or more) are marked done for what the player already has, and the buildings the map places
//! mark their entries as a completion would, with the keep and castle (and the guard and cannon towers) marking the
//! entries of the buildings they stand for. Nothing else marks an entry: in particular no research entry is marked
//! because the player can never research it.

use crate::economy::JobKind;
use crate::tech::{building as b, fixed_line};
use crate::training_ai::{ranger_line, research_item};
use crate::worker_ai::for_race;
use crate::world::World;

/// The spells-known bits of codes 0x90 to 0x97 for a human and an orc player ("Spell bits for done marks at game
/// start").
const SPELL_BITS: [(u32, u32); 8] = [(20, 20), (1, 11), (3, 18), (4, 13), (6, 16), (7, 15), (8, 17), (9, 19)];

/// The scout tower entry (0x40) and the town hall entry (0x4A) of a build list.
const SCOUT_TOWER: u8 = 0x40;
const TOWN_HALL: u8 = 0x4a;
/// The hall upgrade entries.
const KEEP_CODE: u8 = 0x98;
const CASTLE_CODE: u8 = 0x99;

impl World {
    /// Whether research entry `code` of player `p` is already done at the start of a game.
    fn research_known(&self, p: usize, code: u8) -> bool {
        let player = &self.players[p];
        let orc = player.race == 1;
        match research_item(code, orc) {
            Some(JobKind::Research(n)) if n < 24 => player.levels[fixed_line(n)] > n & 1,
            Some(JobKind::Research(_)) => player.levels[ranger_line(code).0] != 0,
            Some(JobKind::Spell(_)) if (0x90..=0x97).contains(&code) => {
                let (h, o) = SPELL_BITS[(code - 0x90) as usize];
                player.spells_known & ((1 << h) | (1 << o)) != 0
            }
            _ => false,
        }
    }

    /// Marks the first entry of player `p`'s build list that is not done and satisfies `want`. Returns nothing: no
    /// match changes nothing.
    fn mark_first(&mut self, p: usize, list: &[u8], want: impl Fn(u8) -> bool) {
        for (n, &e) in list.iter().enumerate() {
            if e == 0xff {
                break;
            }
            if want(e) && self.ai[p].done & (1 << n) == 0 {
                self.ai[p].done |= 1 << n;
                return;
            }
        }
    }

    /// The marks made at the start of a game for computer player `p`, after its build list is set.
    pub(crate) fn mark_game_start(&mut self, p: usize) {
        if !self.computer_player(p as u8) {
            return;
        }
        let list = self.build_list(p);
        for (n, &e) in list.iter().enumerate() {
            if e == 0xff {
                break;
            }
            if e >= 0x80 && self.research_known(p, e) {
                self.ai[p].done |= 1 << n;
            }
        }
        let placed: Vec<u8> = self
            .units
            .iter()
            .filter(|u| u.owner as usize == p && u.building && !u.dying && u.construction.is_none())
            .map(|u| u.kind)
            .collect();
        for k in placed {
            match k {
                b::HUMAN_GUARD_TOWER | b::ORC_GUARD_TOWER | b::HUMAN_CANNON_TOWER | b::ORC_CANNON_TOWER => {
                    self.mark_first(p, &list, |e| e & !1 == SCOUT_TOWER);
                }
                b::KEEP | b::STRONGHOLD | b::CASTLE | b::FORTRESS => {
                    for (n, &e) in list.iter().enumerate() {
                        if e == 0xff {
                            break;
                        }
                        let castle = k & !1 == b::CASTLE;
                        if e == KEEP_CODE || (castle && e == CASTLE_CODE) {
                            self.ai[p].done |= 1 << n;
                        }
                    }
                    self.mark_first(p, &list, |e| e & !1 == TOWN_HALL);
                }
                _ => self.mark_first(p, &list, |e| e < 0x80 && for_race(e, k & 1) == k),
            }
        }
    }
}

#[cfg(test)]
#[path = "ai_start_tests.rs"]
mod tests;
