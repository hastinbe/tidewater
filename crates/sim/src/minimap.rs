//! The minimap's state on the simulation side. Specs: `docs/specs/ui/minimap.md` ("Units and buildings", "Attack
//! mark") and `docs/specs/ui/messages.md` ("Attacks on the minimap").
//!
//! - **The attack mark** ([`Mark`], on each [`Unit`](crate::Unit)): set by every hit that takes hit points from one of
//!   the local player's units without killing it, outside demo playback. The client's minimap redraw reads it and
//!   clears it.
//! - **Remembered buildings** ([`Memory`]): up to 25 copies of buildings that the local player had seen and that were
//!   removed while hidden from it. A copy goes once it is no longer hidden from the local player.
//!
//! Both depend on who the local player is, so they are display state, not game state: neither takes part in
//! [`World::state_hash`]. They are still updated deterministically inside [`World::step`].
//!
//! Defaults for the spec's open questions: every building can be copied (the per-building value that excludes some
//! isn't decoded); a building is "removed" when it is killed (its death starts); a copy's "no longer hidden" test is
//! the unit-visibility rule (`vision/fog.md`, "Unit visibility") applied to its footprint, run in phase 5 after the
//! units' own test; every copy is drawn.

use crate::vision::bit;
use crate::world::World;

/// How many remembered buildings the local player can have.
pub const COPIES: usize = 25;

/// The attack mark of a unit. It depends on the local player, so its `Hash` writes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark(pub bool);

impl std::hash::Hash for Mark {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// A copy of a building where it stood: what the minimap needs to draw its dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remembered {
    pub kind: u8,
    /// The building's colour player (the owner it was created with).
    pub colour: u8,
    /// Top-left tile of the footprint.
    pub tile: (i32, i32),
    pub size: (u8, u8),
}

/// The remembered buildings: [`COPIES`] slots, drawn in slot order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub slots: [Option<Remembered>; COPIES],
}

impl Memory {
    /// The copies in slot order.
    pub fn copies(&self) -> impl Iterator<Item = &Remembered> {
        self.slots.iter().flatten()
    }

    /// Copies in use.
    pub fn len(&self) -> usize {
        self.copies().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Keeps a copy in the first free slot; false when all slots are in use.
    pub fn keep(&mut self, c: Remembered) -> bool {
        match self.slots.iter_mut().find(|s| s.is_none()) {
            Some(s) => {
                *s = Some(c);
                true
            }
            None => false,
        }
    }
}

impl World {
    /// The attack mark for a hit on unit `i` that took hit points without killing it ("Attack mark").
    pub(crate) fn mark_attacked(&mut self, i: usize) {
        if self.units[i].owner == self.vision.local && !self.sounds.demo {
            self.units[i].attack_mark = Mark(true);
        }
    }

    /// Called as unit `i` is killed: a building the local player has seen that is hidden from it is copied, while a
    /// slot is free ("Remembered buildings").
    pub(crate) fn remember_building(&mut self, i: usize) {
        let u = &self.units[i];
        if !u.building || u.hidden_from & bit(self.vision.local) == 0 || !self.building_seen(u) {
            return;
        }
        let copy = Remembered {
            kind: u.kind,
            colour: u.colour,
            tile: u.tile,
            size: u.size,
        };
        self.remembered.keep(copy);
    }

    /// The units and buildings the minimap draws, in drawing order ("Units and buildings"): player 15 first and
    /// player 0 last, each player's units in its owner list order, most recently added first (`core/unit-slots.md`).
    pub fn minimap_units(&self) -> impl Iterator<Item = &crate::Unit> + '_ {
        (0..=crate::slots::NEUTRAL).rev().flat_map(move |p| {
            self.slots.list(p).filter_map(move |slot| {
                let i = self.units.partition_point(|u| u.slot < slot);
                self.units
                    .get(i)
                    .filter(|u| u.slot == slot && u.owner == p && self.on_minimap(u))
            })
        })
    }

    /// Phase 5, after the units' visibility: copies that are no longer hidden from the local player go.
    pub(crate) fn forget_seen_buildings(&mut self) {
        let me = self.vision.local;
        for n in 0..COPIES {
            if let Some(c) = self.remembered.slots[n]
                && !self.footprint_hidden_from(me, c.tile, c.size)
            {
                self.remembered.slots[n] = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::world::tests::{run, scripts, stats, tileset};
    use crate::{Terrain, UnitId};

    const FOOTMAN: u8 = 0;
    const FARM: u8 = 0x3a;

    fn world() -> World {
        let mut t = stats();
        t.0[FARM as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 400,
            size: (2, 2),
            flags: tidewater_formats::udta::flags::BUILDING,
            ..Default::default()
        };
        t.0[FOOTMAN as usize].sight = 2;
        let terrain = Terrain::from_map(&tileset(), &[0; 64 * 64], None, None, 64, 64, false);
        let mut w = World::new(terrain, t, scripts(), 1);
        // Player 0 is the local human, player 1 a computer.
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        w.vision.local = 0;
        w
    }

    fn at(w: &World, id: UnitId) -> usize {
        w.index(id).unwrap()
    }

    /// A farm of player 1 that player 0 has explored but no longer sees, as at the end of a cycle.
    fn seen_hidden_farm(w: &mut World, tile: (i32, i32)) -> UnitId {
        let f = w.spawn(FARM, 1, tile, 0).unwrap();
        w.reveal(0, (tile.0 + 1, tile.1 + 1), 1);
        w.vision.seen.fill(0xff);
        w.unit_visibility();
        f
    }

    #[test]
    fn a_non_lethal_hit_on_a_local_unit_marks_it() {
        let mut w = world();
        let mine = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let theirs = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        let (m, t) = (at(&w, mine), at(&w, theirs));
        w.damage(m, 5, theirs);
        assert!(w.units[m].attack_mark.0);
        w.damage(t, 5, mine);
        assert!(!w.units[t].attack_mark.0, "only the local player's units");
        // Every hit marks, even with the under-attack sound held back by the help timer.
        w.units[m].attack_mark = Mark(false);
        assert_ne!(w.sounds.help_timer, 0);
        w.damage(m, 5, theirs);
        assert!(w.units[m].attack_mark.0);
        // Not in demo playback.
        w.units[m].attack_mark = Mark(false);
        w.sounds.demo = true;
        w.damage(m, 5, theirs);
        assert!(!w.units[m].attack_mark.0);
    }

    #[test]
    fn a_lethal_hit_does_not_mark() {
        let mut w = world();
        let mine = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let theirs = w.spawn(FOOTMAN, 1, (6, 5), 0).unwrap();
        let m = at(&w, mine);
        let hp = w.units[m].hp;
        w.damage(m, hp.min(255) as u8, theirs);
        assert!(w.units[m].dying && !w.units[m].attack_mark.0);
    }

    #[test]
    fn the_mark_is_not_game_state() {
        let mut a = world();
        let id = a.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let b = a.clone();
        let i = at(&a, id);
        a.units[i].attack_mark = Mark(true);
        a.remembered.keep(Remembered {
            kind: FARM,
            colour: 1,
            tile: (1, 1),
            size: (2, 2),
        });
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn a_seen_building_killed_while_hidden_is_remembered() {
        let mut w = world();
        // Three copies already in use.
        for n in 0..3 {
            w.remembered.keep(Remembered {
                kind: FARM,
                colour: 1,
                tile: (40 + n * 3, 40),
                size: (2, 2),
            });
        }
        let f = seen_hidden_farm(&mut w, (10, 10));
        let i = at(&w, f);
        assert_ne!(w.units[i].hidden_from & 1, 0);
        w.kill(i);
        assert_eq!(w.remembered.len(), 4);
        assert_eq!(
            w.remembered.slots[3],
            Some(Remembered {
                kind: FARM,
                colour: 1,
                tile: (10, 10),
                size: (2, 2)
            })
        );
        // Kept while still hidden.
        w.forget_seen_buildings();
        assert_eq!(w.remembered.len(), 4);
        // Gone once the local player sees a tile of its footprint.
        w.vision.seen[11 * 64 + 11] &= !1;
        w.forget_seen_buildings();
        assert_eq!(w.remembered.len(), 3);
        assert!(w.remembered.slots[3].is_none());
    }

    #[test]
    fn no_copy_when_every_slot_is_in_use() {
        let mut w = world();
        for n in 0..COPIES as i32 {
            w.remembered.keep(Remembered {
                kind: FARM,
                colour: 1,
                tile: (n * 2, 50),
                size: (2, 2),
            });
        }
        let f = seen_hidden_farm(&mut w, (10, 10));
        let i = at(&w, f);
        w.kill(i);
        assert_eq!(w.remembered.len(), COPIES);
        assert!(w.remembered.copies().all(|c| c.tile != (10, 10)));
    }

    #[test]
    fn no_copy_of_an_unseen_visible_or_non_building_unit() {
        let mut w = world();
        // Never explored.
        let f = w.spawn(FARM, 1, (20, 20), 0).unwrap();
        w.unit_visibility();
        w.kill(at(&w, f));
        // Seen now: not hidden.
        let g = w.spawn(FARM, 1, (30, 30), 0).unwrap();
        w.reveal(0, (31, 31), 1);
        w.unit_visibility();
        w.kill(at(&w, g));
        // A unit, not a building.
        let u = w.spawn(FOOTMAN, 1, (40, 40), 0).unwrap();
        w.reveal(0, (40, 40), 1);
        w.vision.seen.fill(0xff);
        w.unit_visibility();
        w.kill(at(&w, u));
        assert!(w.remembered.is_empty());
    }

    #[test]
    fn dots_are_in_owner_list_order_newest_first() {
        let mut w = world();
        w.rules.reveal_map = true;
        // Two units of player 1: A created first, then B. B's dot is drawn first, then A's.
        let a = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let b = w.spawn(FOOTMAN, 1, (5, 6), 0).unwrap();
        let mine = w.spawn(FOOTMAN, 0, (9, 9), 0).unwrap();
        let neutral = w.spawn(FARM, 15, (20, 20), 0).unwrap();
        w.unit_visibility();
        let order: Vec<UnitId> = w.minimap_units().map(|u| u.id).collect();
        assert_eq!(order, [neutral, b, a, mine]);
        // Dying units are skipped.
        let ai = at(&w, a);
        w.kill(ai);
        let order: Vec<UnitId> = w.minimap_units().map(|u| u.id).collect();
        assert_eq!(order, [neutral, b, mine]);
    }

    #[test]
    fn copies_go_when_the_whole_map_is_revealed() {
        let mut w = world();
        let f = seen_hidden_farm(&mut w, (10, 10));
        w.kill(at(&w, f));
        assert_eq!(w.remembered.len(), 1);
        w.rules.reveal_map = true;
        run(&mut w, 1);
        assert!(w.remembered.is_empty());
    }
}
