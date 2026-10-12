//! The minimap's state on the simulation side. Specs: `docs/specs/ui/minimap.md` ("Units and buildings", "Attack
//! mark"), `docs/specs/ui/messages.md` ("Attacks on the minimap") and `docs/specs/ui/map-drawing.md` ("Buildings and
//! remembered buildings").
//!
//! - **The attack mark** ([`Mark`], on each [`Unit`](crate::Unit)): set by every hit that takes hit points from one of
//!   the local player's units without killing it, outside demo playback. The client's minimap redraw reads it and
//!   clears it.
//! - **Remembered looks** ([`Look`], on each building): the picture and frame the local player last saw. Set in
//!   phase 5 for every building that isn't hidden from the local player, in the map view or not (the map-drawing
//!   spec's default). A building that carries one counts as seen.
//! - **Remembered buildings** ([`Memory`], the "ghosts"): up to 25 copies of buildings that left play (their "remove"
//!   order started) while hidden from the local player, after being seen, unless the remembered look is an excluded
//!   ruin. A copy goes once its footprint is no longer hidden from the local player (the spec's default for a ghost's
//!   hidden set).
//!
//! All of it depends on who the local player is, so it is display state, not game state: none of it takes part in
//! [`World::state_hash`]. It is still updated deterministically inside [`World::step`].

use crate::vision::bit;
use crate::world::World;

/// How many remembered buildings the local player can have.
pub const COPIES: usize = 25;

/// The attack mark of a unit. It depends on the local player, so its `Hash` writes nothing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mark(pub bool);

impl std::hash::Hash for Mark {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// A picture and frame number: the unit type whose sprite draws it (a construction site or a ruin is its own type
/// here), the animation frame and the facing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Picture {
    pub kind: u8,
    pub frame: u8,
    pub facing: u8,
}

/// A building's remembered look; `None` while never seen. Its `Hash` writes nothing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look(pub Option<Picture>);

impl std::hash::Hash for Look {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// Ruin types whose remembered look leaves no ghost: widths 2 to 4 in Classic mode, 1 to 4 in Enhanced mode.
pub fn excluded_ruin(kind: u8, enhanced: bool) -> bool {
    let first = if enhanced { 0x6a } else { 0x6b };
    (first..=0x6d).contains(&kind)
}

/// A copy of a building where it stood (a ghost): what the minimap and the map view need to draw it.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Remembered {
    pub kind: u8,
    /// The building's colour player (the owner it was created with).
    pub colour: u8,
    /// Top-left tile of the footprint.
    pub tile: (i32, i32),
    pub size: (u8, u8),
    /// The remembered look it is drawn with in the map view.
    pub look: Picture,
}

/// The remembered buildings: [`COPIES`] slots, drawn in slot order.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq, Eq)]
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

    /// Called as unit `i` leaves play (its "remove" order starts): a building that is hidden from the local player,
    /// was seen, and whose remembered look isn't an excluded ruin is copied into the first free ghost record.
    pub(crate) fn remember_building(&mut self, i: usize) {
        let u = &self.units[i];
        let Some(look) = u.look.0 else { return };
        if !u.building || u.hidden_from & bit(self.vision.local) == 0 || excluded_ruin(look.kind, self.rules.enhanced) {
            return;
        }
        let copy = Remembered {
            kind: u.kind,
            colour: u.colour,
            tile: u.tile,
            size: u.size,
            look,
        };
        self.remembered.keep(copy);
    }

    /// Phase 5, after the units' visibility: every building not hidden from the local player remembers its current
    /// picture and frame.
    pub(crate) fn remember_looks(&mut self) {
        let me = bit(self.vision.local);
        for u in self.units.iter_mut().filter(|u| u.building && u.hidden_from & me == 0) {
            u.look = Look(Some(Picture {
                kind: u.kind,
                frame: u.script.frame,
                facing: u.facing,
            }));
        }
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

    /// A farm of player 1 that player 0 has seen (it has a remembered look) but no longer sees, as at the end of a
    /// cycle.
    fn seen_hidden_farm(w: &mut World, tile: (i32, i32)) -> UnitId {
        let f = w.spawn(FARM, 1, tile, 0).unwrap();
        w.reveal(0, (tile.0 + 1, tile.1 + 1), 1);
        w.unit_visibility();
        w.remember_looks();
        w.vision.seen.fill(0xff);
        w.unit_visibility();
        w.remember_looks();
        f
    }

    /// Kills unit `id` and starts its "remove" order, where it leaves play.
    fn remove(w: &mut World, id: UnitId) {
        let i = at(w, id);
        w.kill(i);
        let i = at(w, id);
        w.start_removal(i);
    }

    fn copy(tile: (i32, i32)) -> Remembered {
        Remembered {
            kind: FARM,
            colour: 1,
            tile,
            size: (2, 2),
            look: Picture {
                kind: FARM,
                frame: 0,
                facing: 0,
            },
        }
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
        a.remembered.keep(copy((1, 1)));
        a.units[i].look = Look(Some(copy((1, 1)).look));
        assert_eq!(a.state_hash(), b.state_hash());
    }

    #[test]
    fn a_seen_building_killed_while_hidden_is_remembered() {
        let mut w = world();
        // Three copies already in use.
        for n in 0..3 {
            w.remembered.keep(copy((40 + n * 3, 40)));
        }
        let f = seen_hidden_farm(&mut w, (10, 10));
        let i = at(&w, f);
        assert_ne!(w.units[i].hidden_from & 1, 0);
        w.kill(i);
        assert_eq!(w.remembered.len(), 3, "not when it is killed");
        let i = at(&w, f);
        w.start_removal(i);
        assert_eq!(w.remembered.len(), 4, "but when it leaves play");
        let c = w.remembered.slots[3].unwrap();
        assert_eq!((c.kind, c.tile, c.look.kind), (FARM, (10, 10), FARM));
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
            w.remembered.keep(copy((n * 2, 50)));
        }
        let f = seen_hidden_farm(&mut w, (10, 10));
        remove(&mut w, f);
        assert_eq!(w.remembered.len(), COPIES);
        assert!(w.remembered.copies().all(|c| c.tile != (10, 10)));
    }

    #[test]
    fn no_copy_of_an_unseen_visible_or_non_building_unit() {
        let mut w = world();
        // Never explored.
        let f = w.spawn(FARM, 1, (20, 20), 0).unwrap();
        w.unit_visibility();
        w.remember_looks();
        remove(&mut w, f);
        // Seen now: not hidden.
        let g = w.spawn(FARM, 1, (30, 30), 0).unwrap();
        w.reveal(0, (31, 31), 1);
        w.unit_visibility();
        w.remember_looks();
        remove(&mut w, g);
        // A unit, not a building.
        let u = w.spawn(FOOTMAN, 1, (40, 40), 0).unwrap();
        w.reveal(0, (40, 40), 1);
        w.unit_visibility();
        w.vision.seen.fill(0xff);
        w.unit_visibility();
        remove(&mut w, u);
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
        remove(&mut w, f);
        assert_eq!(w.remembered.len(), 1);
        w.rules.reveal_map = true;
        run(&mut w, 1);
        assert!(w.remembered.is_empty());
    }

    #[test]
    fn a_building_seen_under_construction_keeps_that_look_in_fog() {
        // Map-drawing vector: an enemy farm seen under construction, then fogged, completes out of sight.
        let mut w = world();
        let f = seen_hidden_farm(&mut w, (10, 10));
        let i = at(&w, f);
        let seen = w.units[i].look.0.unwrap();
        assert_eq!(seen.kind, FARM);
        w.units[i].script.frame = seen.frame + 1;
        w.remember_looks();
        assert_eq!(w.units[i].look.0, Some(seen), "hidden: the look stays as it was");
        w.rules.reveal_map = true;
        w.unit_visibility();
        w.remember_looks();
        assert_eq!(
            w.units[i].look.0.unwrap().frame,
            seen.frame + 1,
            "seen again: the look follows"
        );
    }

    #[test]
    fn a_ghost_stays_until_its_site_is_seen_again() {
        // Map-drawing vector: an enemy barracks destroyed out of sight, after being seen.
        let mut w = world();
        let f = seen_hidden_farm(&mut w, (10, 10));
        remove(&mut w, f);
        assert_eq!(w.remembered.len(), 1);
        w.forget_seen_buildings();
        assert_eq!(w.remembered.len(), 1);
        w.reveal(0, (11, 11), 1);
        w.forget_seen_buildings();
        assert!(w.remembered.is_empty());
    }

    #[test]
    fn a_ruin_last_seen_leaves_no_ghost_except_a_small_classic_one() {
        for (enhanced, width, ghost) in [(true, 1, false), (false, 1, true), (false, 2, false), (true, 4, false)] {
            let mut w = world();
            w.rules.enhanced = enhanced;
            let f = seen_hidden_farm(&mut w, (10, 10));
            let i = at(&w, f);
            w.units[i].look.0.as_mut().unwrap().kind = 0x6a + width - 1;
            w.start_removal(i);
            assert_eq!(w.remembered.len() == 1, ghost, "enhanced {enhanced}, width {width}");
        }
        assert!(excluded_ruin(0x6b, false) && !excluded_ruin(0x6a, false) && excluded_ruin(0x6a, true));
    }

    #[test]
    fn twenty_six_hidden_buildings_leave_twenty_five_ghosts() {
        let mut w = world();
        let farms: Vec<UnitId> = (0..26)
            .map(|n| seen_hidden_farm(&mut w, (2 + (n % 13) * 3, 2 + (n / 13) * 20)))
            .collect();
        for f in farms {
            remove(&mut w, f);
        }
        assert_eq!(w.remembered.len(), COPIES);
    }

    #[test]
    fn the_seen_marker_is_the_remembered_look() {
        let mut w = world();
        let f = w.spawn(FARM, 1, (20, 20), 0).unwrap();
        w.reveal(0, (21, 21), 1);
        w.unit_visibility();
        let i = at(&w, f);
        assert!(!w.on_minimap(&w.units[i]), "explored but no look yet");
        w.remember_looks();
        assert!(w.on_minimap(&w.units[i]));
    }
}
