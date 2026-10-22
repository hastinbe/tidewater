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
    ///
    /// A new game's marks read what the player had **before the mission start** ([`World::before_mission`]), so what
    /// the mission start grants marks nothing (round 104, SW7a); a load's read the current state.
    fn research_known(&self, p: usize, code: u8, before_mission: bool) -> bool {
        let player = &self.players[p];
        let orc = player.race == 1;
        let (spells_known, levels) = match &self.before_mission {
            Some(v) if before_mission => v[p],
            _ => (player.spells_known, player.levels),
        };
        match research_item(code, orc) {
            Some(JobKind::Research(n)) if n < 24 => levels[fixed_line(n)] > n & 1,
            Some(JobKind::Research(_)) => levels[ranger_line(code).0] != 0,
            Some(JobKind::Spell(_)) if (0x90..=0x97).contains(&code) => {
                let (h, o) = SPELL_BITS[(code - 0x90) as usize];
                spells_known & ((1 << h) | (1 << o)) != 0
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

    /// The game-start marks for what player `p` already has: upgrade levels, researched items and known spells.
    fn mark_research_known(&mut self, p: usize, list: &[u8], before_mission: bool) {
        for (n, &e) in list.iter().enumerate() {
            if e == 0xff {
                break;
            }
            if e >= 0x80 && self.research_known(p, e, before_mission) {
                self.ai[p].done |= 1 << n;
            }
        }
    }

    /// The marks a load makes for computer player `p`, whose marks and requests are clear (`ai/worker-ai.md`, "Done
    /// marks", last bullet; `game/save-load.md` SL9 to SL12): the game-start marks, then the **load's building marks**
    /// of each building. Loaded units make no creation marks, and a research running at the save is not marked: it
    /// has no level or flag yet.
    ///
    /// `game/save-load.md` SL20 (traced): the load's building marks count every building that isn't dying or removed,
    /// construction sites included (SL9's player 2 had four sites at the save and their entries are marked in the
    /// loaded game's setup record). Keep, castle and level marks are SL16 to SL19.
    pub(crate) fn mark_load(&mut self, p: usize) {
        if !self.computer_player(p as u8) {
            return;
        }
        let list = self.build_list(p);
        self.mark_research_known(p, &list, false);
        let mine: Vec<u8> = self
            .units
            .iter()
            .filter(|u| u.owner as usize == p && u.building && u.hp > 0 && !u.dying)
            .map(|u| u.kind)
            .collect();
        for k in mine {
            match k {
                // Guard or cannon tower: the first open scout-tower entry.
                0x60..=0x63 => self.mark_first(p, &list, |e| e == 0x40),
                // Keep or stronghold: every 0x98 entry, then the first open town hall entry.
                0x58 | 0x59 => {
                    self.mark_all(p, &list, |e| e == 0x98);
                    self.mark_first(p, &list, |e| e == 0x4a);
                }
                // Castle or fortress: every 0x98 and 0x99 entry, then the first open town hall entry.
                0x5a | 0x5b => {
                    self.mark_all(p, &list, |e| e == 0x98 || e == 0x99);
                    self.mark_first(p, &list, |e| e == 0x4a);
                }
                // Any other building: the first open entry of its type with the race bit cleared.
                _ => self.mark_first(p, &list, |e| e < 0x80 && for_race(e, k & 1) == k),
            }
        }
    }

    /// Marks every entry of player `p`'s build list that satisfies `want`, open or not.
    fn mark_all(&mut self, p: usize, list: &[u8], want: impl Fn(u8) -> bool) {
        for (n, &e) in list.iter().enumerate() {
            if e == 0xff {
                break;
            }
            if want(e) {
                self.ai[p].done |= 1 << n;
            }
        }
    }

    /// The marks made at the start of a game for computer player `p`, after its build list is set.
    ///
    /// An active rescue player's map buildings were created before it became a computer player (`game/setup.md`,
    /// "After the units"), so they get no creation marks (round 83, traced: the `human5` and `human8` setup dumps,
    /// CP8) and mark no build-list entry done (round 83 default, untraced). They are the buildings with the rescuable
    /// mark, which only the map's units of a rescue player get. Its research entries are marked as any computer
    /// player's.
    pub(crate) fn mark_game_start(&mut self, p: usize) {
        if !self.computer_player(p as u8) {
            return;
        }
        // Worker-ai spec, "Creation marks" (round 38): the map's buildings get their keep-clear marks by type.
        let mine: Vec<usize> = (0..self.units.len())
            .filter(|&i| {
                let u = &self.units[i];
                u.owner as usize == p && u.building && !u.dying && !u.rescuable
            })
            .collect();
        for &i in &mine {
            self.creation_mark(i);
        }
        let list = self.build_list(p);
        self.mark_research_known(p, &list, true);
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
