//! Corpses, wrecks and ruins: what a dead unit becomes at its death animation's "step done", how the remains decay,
//! and how a dead unit's slot is released. Spec: `docs/specs/core/corpses.md`.
//!
//! Remains stay the same unit in the same slot, with a new type, and keep the dying state, so everything that skips
//! dead units skips them too. Their decay is their own animation (the animation number is the remains kind); its
//! "neutral" instruction gives them to the neutral player and its "step done" starts the removal.

use tidewater_formats::script::anim;
use tidewater_formats::udta::flags;

use crate::slots;
use crate::terrain::bits;
use crate::world::{RELEASED, World};

/// The dead body type and the first ruin type (spec, "Data").
pub const DEAD_BODY: u8 = 0x69;
const RUIN: u8 = 0x6a;
const OIL_PATCH: u8 = 0x5d;
const OIL_PLATFORMS: [u8; 2] = [0x56, 0x57];
/// A type that makes the cosmetic explosion without the explode flag, and doesn't kill passengers.
const EXPLODES_ANYWAY: u8 = 0x29;
const OGRE: u8 = 0x07;
const OGRE_MAGE: u8 = 0x0d;
/// "Ogre facing remap".
const OGRE_FACING: [u8; 8] = [5, 5, 5, 7, 7, 1, 3, 3];
/// The "remove" animation (animation 0 of every type): halt, halt, step done.
const REMOVE: u8 = anim::NONE;

/// Remains kinds, which are also their decay animation numbers ("Decay stages").
pub mod kind {
    pub const HUMAN_BODY: u8 = 1;
    pub const ORC_BODY: u8 = 2;
    pub const WRECK: u8 = 3;
    pub const LAND_RUIN: u8 = 4;
    pub const WATER_RUIN: u8 = 5;
}

/// "Corpse kinds" by PUD unit type; `None` leaves no remains.
pub fn corpse_kind(t: u8) -> Option<u8> {
    match t {
        0x00 | 0x02 | 0x06 | 0x08 | 0x0c | 0x10 | 0x12 | 0x14 | 0x2c | 0x2e => Some(kind::HUMAN_BODY),
        0x01 | 0x03 | 0x07 | 0x09 | 0x0d | 0x11 | 0x13 | 0x19 | 0x2f => Some(kind::ORC_BODY),
        0x1a..=0x22 | 0x24..=0x27 => Some(kind::WRECK),
        _ => None,
    }
}

impl World {
    /// The parts of a kill that belong to this spec, run at the end of the kill. Returns whether the passengers die
    /// with it (not when the dead unit has the explode flag or is type 0x29).
    pub(crate) fn kill_remains(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        let s = self.stats_of(u.kind);
        let explodes = s.flags & flags::EXPLODE_WHEN_KILLED != 0 || u.kind == EXPLODES_ANYWAY;
        if explodes {
            // "Units with the explode flag": a cosmetic effect 8 px above the centre, no damage, no draw.
            let c = u.centre_px();
            self.explosion(u.id, (c.0, c.1 - 8));
        }
        if OIL_PLATFORMS.contains(&self.units[i].kind) && self.units[i].resources > 0 {
            self.units[i].used = true;
        }
        !explodes
    }

    /// "Oil platform killed with oil left": a patch with the oil that was left, at once, taking a new slot and
    /// creation number with a building's 2 draws. Run last in the kill, since creating a unit moves unit indices. Our
    /// reading of the open question: the patch is always placed (the dying platform's footprint is already cleared).
    pub(crate) fn leave_oil(&mut self, i: usize) {
        let u = &self.units[i];
        if OIL_PLATFORMS.contains(&u.kind) && u.resources > 0 {
            let (tile, oil) = (u.tile, u.resources);
            let _ = self.spawn(OIL_PATCH, slots::NEUTRAL, tile, oil);
        }
    }

    /// The death animation's "step done" ("What a dead unit becomes").
    pub(crate) fn death_done(&mut self, i: usize) {
        // Movement spec, "Blocked rules" (round-42 errata): units other than the killer keep a dying attacker's record
        // until it leaves play. Our reading (`orc02-assault` grunt 1568 answering a call for help at n = 969, footman
        // 1587 stepping at n = 940): it leaves play when its death animation ends.
        let id = self.units[i].id;
        for o in &mut self.units {
            if o.attacked_by == Some(id) {
                o.attacked_by = None;
            }
        }
        let u = &self.units[i];
        let s = self.stats_of(u.kind);
        // A unit inside something carries the used mark (spec, "The used mark").
        let used = u.used || u.inside;
        if u.building {
            if used {
                return self.start_removal(i);
            }
            let water = u.layer == crate::Layer::Sea || s.flags & flags::SHORE_BUILDING != 0;
            let width = (u.size.0.max(1) - 1).min(3);
            let u = &mut self.units[i];
            u.kind = RUIN + width;
            u.facing = 0;
            u.construction = None;
            u.remains = true;
            let k = if water { kind::WATER_RUIN } else { kind::LAND_RUIN };
            // Ruins start their decay with an ordinary start: the first stage runs on the next cycle.
            return self.start_anim(i, k);
        }
        let on_shore = self.terrain.word(u.tile.0, u.tile.1) & bits::SHORE != 0;
        if s.flags & flags::TRANSPORT != 0 && on_shore {
            self.units[i].used = true;
            return self.start_removal(i);
        }
        let Some(k) = corpse_kind(u.kind) else {
            return self.start_removal(i);
        };
        let u = &mut self.units[i];
        if matches!(u.kind, OGRE | OGRE_MAGE) {
            u.facing = OGRE_FACING[u.facing as usize & 7];
        }
        u.kind = DEAD_BODY;
        u.used = used;
        u.remains = true;
        // Bodies and wrecks: the first stage runs in this same cycle.
        self.switch_anim(i, k);
    }

    /// The remains' last stage, or a dead unit that leaves none: the "remove" order, given to the neutral player.
    /// The slot is released at the remove animation's "step done", 513 cycles on.
    pub(crate) fn start_removal(&mut self, i: usize) {
        // Map-drawing spec, "Remembered buildings": a building leaves play here, and may leave a ghost.
        self.remember_building(i);
        if self.units[i].owner != slots::NEUTRAL {
            self.change_owner(i, slots::NEUTRAL);
        }
        let u = &mut self.units[i];
        u.remains = false;
        u.removing = true;
        self.start_anim(i, REMOVE);
    }

    /// One update of a dead unit after its script ran: the stage it is in decides what a "step done" means.
    pub(crate) fn dead_update(&mut self, i: usize, fx: crate::anim::Effects) {
        let u = &self.units[i];
        if u.removing {
            if fx.step_done {
                // Release order decides slot reuse, so the free list is updated now. The slot keeps its last
                // contents until reused, with owner 14 and only the unused mark (caster-ai spec, "Free slots").
                self.slots.release(u.slot, u.owner);
                let stale = crate::caster_ai::Stale {
                    kind: u.kind,
                    tile: u.tile,
                    marks: u.marks,
                };
                self.stale.insert(u.slot, stale);
                self.units[i].script.anim = RELEASED;
            }
        } else if u.remains {
            if fx.step_done {
                self.start_removal(i);
            }
        } else if fx.step_done || fx.halted {
            // Our reading: a death animation that halts without a "step done" ends there too.
            self.death_done(i);
        }
    }

    /// Unused slots of remains that raise dead may take, in ascending slot order ("Raise dead").
    pub(crate) fn raisable_bodies(&self) -> Vec<crate::UnitId> {
        self.units
            .iter()
            .filter(|u| u.kind == DEAD_BODY && u.remains && !u.used)
            .map(|u| u.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Layer;
    use crate::world::tests::{field, run};

    const FOOTMAN: u8 = 0;
    const MAGE: u8 = 0x0a;
    const DESTROYER: u8 = 0x1e;
    const TRANSPORT: u8 = 0x1c;
    const FARM: u8 = 0x3a;
    const SHIPYARD: u8 = 0x48;

    /// Steps until `id`'s death animation has ended; returns the cycles taken.
    fn until_remains(w: &mut crate::World, id: crate::UnitId) -> u32 {
        for n in 1..1000 {
            w.step(&[]);
            let u = w.unit(id).unwrap();
            if u.remains || u.removing {
                return n;
            }
        }
        panic!("the death animation never ended");
    }

    fn kill(w: &mut crate::World, id: crate::UnitId) {
        let i = w.index(id).unwrap();
        w.kill(i);
    }

    #[test]
    fn corpse_kinds() {
        assert_eq!(
            [0x00, 0x01, 0x07, 0x1a, 0x22, 0x23, 0x24, 0x27, 0x0a, 0x30, 0x37].map(corpse_kind),
            [
                Some(1),
                Some(2),
                Some(2),
                Some(3),
                Some(3),
                None,
                Some(3),
                Some(3),
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn a_footman_leaves_a_body_that_decays_and_is_removed() {
        let mut w = field(16, 16);
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let slot = w.unit(f).unwrap().slot;
        let draws = w.rng.state;
        kill(&mut w, f);
        until_remains(&mut w, f);
        let u = w.unit(f).unwrap();
        assert_eq!(
            (u.kind, u.slot, u.owner, u.remains, u.used),
            (DEAD_BODY, slot, 1, true, false)
        );
        assert_eq!(u.script.anim, kind::HUMAN_BODY);
        assert_eq!(w.rng.state, draws, "remains make no draws");
        // The fixture's decay: neutral after 10 cycles, "step done" after 20.
        run(&mut w, 10);
        assert_eq!(w.unit(f).unwrap().owner, slots::NEUTRAL);
        run(&mut w, 10);
        assert!(w.unit(f).unwrap().removing);
        run(&mut w, 512);
        assert!(w.unit(f).is_some(), "the slot is held for 513 cycles");
        run(&mut w, 1);
        assert!(w.unit(f).is_none());
    }

    #[test]
    fn bodies_keep_the_mana_byte_through_their_decay() {
        // Spec, "What a dead unit becomes": neither change touches the mana byte, and the timers pass skips remains
        // (vector: a unit with 120 mana is killed; its body shows 120 throughout its decay).
        let mut w = field(16, 16);
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let i = w.index(f).unwrap();
        w.units[i].mana = 120;
        w.units[i].decays = true;
        kill(&mut w, f);
        until_remains(&mut w, f);
        for _ in 0..20 {
            assert_eq!(w.unit(f).unwrap().mana, 120);
            run(&mut w, 1);
        }
    }

    #[test]
    fn a_killed_mage_keeps_its_mana_while_dying_and_removed() {
        // Round-36 vector (xhuman10-r36a): mage 1481 with 89 mana is killed; mana 89 throughout, no gain while it
        // dies, and its emptied slot (owner 15, order 0) still shows 89.
        let mut w = field(16, 16);
        let m = w.spawn(MAGE, 1, (5, 5), 0).unwrap();
        let i = w.index(m).unwrap();
        w.units[i].mana = 89;
        kill(&mut w, m);
        for _ in 0..500 {
            let Some(u) = w.unit(m) else { break };
            assert_eq!(u.mana, 89);
            run(&mut w, 1);
        }
        let u = w.unit(m).unwrap();
        assert!(u.removing && u.owner == slots::NEUTRAL);
    }

    #[test]
    fn a_mage_leaves_nothing_and_its_slot_goes_513_cycles_later() {
        let mut w = field(16, 16);
        let m = w.spawn(MAGE, 1, (5, 5), 0).unwrap();
        kill(&mut w, m);
        until_remains(&mut w, m);
        let u = w.unit(m).unwrap();
        assert!(u.removing && !u.remains && u.owner == slots::NEUTRAL && u.kind == MAGE);
        run(&mut w, 512);
        assert!(w.unit(m).is_some());
        run(&mut w, 1);
        assert!(w.unit(m).is_none());
    }

    #[test]
    fn ogres_turn_as_they_fall() {
        let mut w = field(16, 16);
        let o = w.spawn(OGRE, 1, (5, 5), 0).unwrap();
        let i = w.index(o).unwrap();
        w.units[i].facing = 2;
        kill(&mut w, o);
        until_remains(&mut w, o);
        let u = w.unit(o).unwrap();
        assert_eq!((u.kind, u.facing, u.script.anim), (DEAD_BODY, 5, kind::ORC_BODY));
    }

    #[test]
    fn buildings_leave_ruins_by_width_on_the_next_cycle() {
        let mut w = field(16, 16);
        w.stats.0[SHIPYARD as usize].size = (3, 3);
        w.stats.0[SHIPYARD as usize].flags |= flags::SHORE_BUILDING | flags::BUILDING;
        let farm = w.spawn(FARM, 1, (2, 2), 0).unwrap();
        let yard = w.spawn(SHIPYARD, 1, (8, 8), 0).unwrap();
        kill(&mut w, farm);
        kill(&mut w, yard);
        until_remains(&mut w, farm);
        let (f, y) = (w.unit(farm).unwrap(), w.unit(yard).unwrap());
        assert_eq!(
            (f.kind, f.facing, f.script.anim),
            (RUIN + w.unit(farm).unwrap().size.0 - 1, 0, kind::LAND_RUIN)
        );
        assert_eq!((y.kind, y.script.anim), (0x6c, kind::WATER_RUIN));
        // An ordinary start: the first stage hasn't run yet.
        assert_eq!(f.script.wait, 1);
    }

    #[test]
    fn transports_on_shore_water_sink_without_a_wreck() {
        let mut w = field(16, 16);
        let n = w.terrain.at(5, 5).unwrap();
        w.terrain.words[n] |= bits::SHORE;
        let n = w.terrain.at(9, 5).unwrap();
        w.terrain.words[n] |= bits::SHORE;
        w.stats.0[TRANSPORT as usize].flags |= flags::TRANSPORT;
        let t = w.spawn(TRANSPORT, 1, (5, 5), 0).unwrap();
        let d = w.spawn(DESTROYER, 1, (9, 5), 0).unwrap();
        kill(&mut w, t);
        kill(&mut w, d);
        until_remains(&mut w, t);
        let (t, d) = (w.unit(t).unwrap(), w.unit(d).unwrap());
        assert!(t.removing && t.used && t.kind == TRANSPORT);
        assert_eq!((d.kind, d.script.anim), (DEAD_BODY, kind::WRECK));
    }

    #[test]
    fn used_units_keep_the_mark_and_used_buildings_leave_nothing() {
        let mut w = field(16, 16);
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let b = w.spawn(FARM, 1, (9, 9), 0).unwrap();
        for id in [f, b] {
            let i = w.index(id).unwrap();
            w.units[i].used = true;
            w.kill(i);
        }
        until_remains(&mut w, f);
        assert_eq!(w.unit(f).unwrap().kind, DEAD_BODY);
        assert!(w.unit(f).unwrap().used);
        assert!(w.raisable_bodies().is_empty());
        assert!(w.unit(b).unwrap().removing);
    }

    #[test]
    fn an_oil_platform_with_oil_left_leaves_a_patch() {
        let mut w = field(16, 16);
        let p = w.spawn(OIL_PLATFORMS[0], 1, (5, 5), 50).unwrap();
        let before = w.units.iter().map(|u| u.id.0).max().unwrap() + 1;
        let draws = w.rng.state;
        kill(&mut w, p);
        let patch = w.units.iter().find(|u| u.kind == OIL_PATCH).unwrap();
        assert_eq!(
            (patch.owner, patch.tile, patch.resources, patch.id.0),
            (slots::NEUTRAL, (5, 5), 50, before)
        );
        assert_eq!(
            w.rng,
            {
                let mut r = crate::rng::Rng::new(draws);
                r.draw();
                r.draw();
                r
            },
            "a building's creation draws"
        );
        assert!(w.unit(p).unwrap().used);
        until_remains(&mut w, p);
        assert!(w.unit(p).unwrap().removing, "no ruin");
    }

    #[test]
    fn exploding_units_add_a_cosmetic_effect_and_spare_passengers() {
        let mut w = field(16, 16);
        w.stats.0[FOOTMAN as usize].flags |= flags::EXPLODE_WHEN_KILLED;
        let f = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let draws = w.rng.state;
        kill(&mut w, f);
        assert_eq!(w.rng.state, draws);
        let fx: Vec<_> = w.missiles.iter().flatten().filter(|m| m.kind == 0x17).collect();
        assert_eq!(fx.len(), 1);
        let c = w.unit(f).unwrap().centre_px();
        assert_eq!(fx[0].pos, (c.0, c.1 - 8));
        let i = w.index(f).unwrap();
        assert_eq!(w.units[i].layer, Layer::Land);
    }
}
