//! Build-list done marks made before play (reviewed specs `docs/specs/ai/worker-ai.md`, "Done marks", and
//! `docs/specs/ai/training-research.md`, "Research requests and done marks").
//!
//! Research entries (0x80 or more) are marked done for what the player already has, and each building the map places
//! marks the first open entry of its own type, as any creation does (round 36: there are no tower, keep or castle
//! cases). Nothing else marks an entry: in particular no research entry is marked because the player can never
//! research it.

use crate::economy::JobKind;
use crate::tech::fixed_line;
use crate::training_ai::{ranger_line, research_item};
use crate::worker_ai::for_race;
use crate::world::World;

/// The spells-known bits of codes 0x90 to 0x97 for a human and an orc player ("Spell bits for done marks at game
/// start").
const SPELL_BITS: [(u32, u32); 8] = [(20, 20), (1, 11), (3, 18), (4, 13), (6, 16), (7, 15), (8, 17), (9, 19)];

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
        // Worker-ai spec, "Creation marks" (round 38): the map's buildings get their keep-clear marks by type.
        let mine: Vec<usize> = (0..self.units.len())
            .filter(|&i| self.units[i].owner as usize == p && self.units[i].building && !self.units[i].dying)
            .collect();
        for &i in &mine {
            self.creation_mark(i);
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
        // Round 36: every building the map places marks the first open entry of its own type, as any creation does;
        // there are no tower, keep or castle cases.
        for i in mine {
            let k = self.units[i].kind;
            self.mark_first(p, &list, |e| e < 0x80 && for_race(e, k & 1) == k);
        }
    }
}

#[cfg(test)]
#[path = "ai_start_tests.rs"]
mod tests;
