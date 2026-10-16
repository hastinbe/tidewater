//! Rescue: rescuable units joining a human-controlled player, the owner change that goes with it, the circle of
//! power's rescue check, and the flash after an owner change. Spec: `docs/specs/game/rescue.md`. Unit type numbers
//! and flag bits are those of the public PUD specification.
//!
//! The rescuable mark itself is [`Unit::rescuable`](crate::Unit): units the map gives to a rescue player get it when
//! the map loads ([`World::from_pud`]). The two global counts (rescuable units, rescuable heroes) are taken by scanning
//! the marks ([`World::rescuable_counts`]), as the victory check does; a unit that is dying or removed stops counting.
//!
//! Our readings where the spec leaves room:
//! - "Off the map" is a unit inside a mine, platform, building or transport (`inside`); "dying or dead" covers the
//!   death animation, remains and removal.
//! - "Taken out of every player's current selection": the sim keeps no selections, so it reports the change as
//!   [`Event::OwnerChanged`] and the client drops the unit from its selection.
//! - The flash countdown is display state for the local player (it depends on what the local player detects), so it
//!   is kept out of [`World::state_hash`].
//! - Circle timing (open question): the check is order 59's logic, so it runs at each "step done" of the circle's
//!   still animation once that order has taken over, and the 50-cycle pause is the animation's wait. The period is
//!   then 50 cycles plus the rest of the still loop (54 in the Orc02 oracle trace).
//! - Frozen state (open question): implemented as described, but no command sets it yet.
//! - The two redraw requests at the end of an owner change (open question) are the client's business: it redraws
//!   every frame.

use tidewater_formats::udta::flags;

use crate::economy::Event;
use crate::sound::Sound;
use crate::tech;
use crate::world::{Layer, UnitId, World};

/// The circle of power's type.
pub const CIRCLE_OF_POWER: u8 = 0x64;
/// Cycles the circle waits after a check ("Rescuing at a circle of power").
pub const CIRCLE_PAUSE: u8 = 50;
/// Length of the flash after an owner change ("Flash").
pub const FLASH_CYCLES: u8 = 100;
/// The hero bit in rescue counts.
pub const HERO_RESCUE: u16 = 0x1000;
/// Flag bits 0 to 4 (land, air, explode, sea, critter): a type with one of them is a unit, not a building ("Terms").
const UNIT_FLAGS: u32 = flags::LAND | flags::AIR | flags::EXPLODE_WHEN_KILLED | flags::SEA | flags::CRITTER;

/// Selection-box colours of the flash: palette entries for the local player's own units, allied units, and the rest.
pub mod flash_colour {
    pub const OWN: u8 = 250;
    pub const ALLIED: u8 = 251;
    pub const OTHER: u8 = 249;
}

/// The flash countdown of a unit ("Flash"). It depends on the local player, so it doesn't take part in the state
/// hash: its `Hash` writes nothing.
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Flash(pub u8);

impl std::hash::Hash for Flash {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

/// The frozen state of a unit ("Frozen units"). Nothing sets it yet: the commands that freeze and release a worker
/// haven't been identified (open question).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Freeze {
    #[default]
    None,
    /// Skips its next "step done" (no rescue check, no order logic), then is frozen.
    Freezing,
    /// Doesn't advance its animation at all.
    Frozen,
    /// Runs the rescue check and the order logic on its next update without waiting for a "step done".
    Released,
}

impl World {
    /// The global rescuable counts: units with the rescuable mark that aren't dying or removed, and how many of them
    /// are heroes (type flag bit 23).
    pub fn rescuable_counts(&self) -> (u32, u32) {
        let mut n = (0, 0);
        for u in self.units.iter().filter(|u| u.rescuable && !u.dying && !u.removing) {
            n.0 += 1;
            if self.stats_of(u.kind).flags & flags::HERO != 0 {
                n.1 += 1;
            }
        }
        n
    }

    /// The joining check, run at a rescuable unit's "step done" before the pending order and the order logic
    /// ("Joining"). Returns whether the unit changed owner.
    pub(crate) fn joining_check(&mut self, i: usize) -> bool {
        let u = &self.units[i];
        if !u.rescuable || self.human_controlled(u.owner) {
            return false;
        }
        let Some(to) = self.rescuer(i) else { return false };
        let from = self.units[i].owner;
        let gold_store = self.stats_of(self.units[i].kind).flags & flags::GOLD_STORAGE != 0;
        self.rescue_owner_change(i, to, true);
        if gold_store {
            for j in 0..self.units.len() {
                let o = &self.units[j];
                if j != i && o.owner == from && !o.dying && !o.removing && !o.inside {
                    self.rescue_owner_change(j, to, false);
                }
            }
        }
        true
    }

    /// The owner of the first qualifying ground occupant in the area around unit `i`, tested row by row from the
    /// top and left to right ("The area", "The rescuer").
    fn rescuer(&self, i: usize) -> Option<u8> {
        let u = &self.units[i];
        let r = if self.stride_of(u.kind) == 2 { 2 } else { 1 };
        let (x, y) = u.tile;
        let (w, h) = (u.size.0 as i32, u.size.1 as i32);
        let (mw, mh) = (self.terrain.width as i32, self.terrain.height as i32);
        for ty in (y - r).max(0)..=(y + h).min(mh - 1) {
            for tx in (x - r).max(0)..=(x + w).min(mw - 1) {
                let Some(id) = self.occupant(Layer::Land, tx, ty) else {
                    continue;
                };
                if id == u.id {
                    continue;
                }
                let Some(o) = self.unit(id) else { continue };
                if self.stats_of(o.kind).flags & UNIT_FLAGS == 0 || o.dying || o.removing || o.inside {
                    continue;
                }
                if self.human_controlled(o.owner) {
                    return Some(o.owner);
                }
            }
        }
        None
    }

    /// "Owner change", steps 1 to 7. `announce` plays the capture sound (the unit joined itself, not in the
    /// gold-store cascade).
    pub(crate) fn rescue_owner_change(&mut self, i: usize, to: u8, announce: bool) {
        let from = self.units[i].owner;
        // 1. A building's job is cancelled with the refund to the old owner; a site moves between the counts.
        if self.units[i].building {
            // The job ends at once, and order 33 is given as a pending order, taking over at the building's next step.
            if self.cancel_job(i) {
                self.units[i].pending = Some(crate::world::Order::Idle);
            }
            if self.units[i].construction.is_some() {
                self.players[from as usize].under_construction -= 1;
                self.players[to as usize].under_construction += 1;
            }
        }
        // 2. The converted tally.
        let p = &mut self.players[to as usize];
        p.converted = p.converted.wrapping_add(1);
        // 3. The owner; the order stays.
        self.change_owner(i, to);
        // Campaign-flow spec, "Statistics": the old owner's count went down.
        let u = self.units[i].clone();
        self.count_lost(from, &u);
        let unit = self.units[i].id;
        self.events.push(Event::OwnerChanged { unit, from, to });
        // 4. Enhanced mode's conversions.
        if self.rules.enhanced {
            let p = &self.players[to as usize];
            let into = match self.units[i].kind {
                tech::unit::KNIGHT if p.paladin_known() => Some(tech::unit::PALADIN),
                tech::unit::OGRE if p.paladin_known() => Some(tech::unit::OGRE_MAGE),
                tech::unit::ARCHER if p.ranger_training_done() => Some(tech::unit::RANGER),
                tech::unit::AXE_THROWER if p.ranger_training_done() => Some(tech::unit::BERSERKER),
                _ => None,
            };
            if let Some(k) = into {
                self.convert_unit(i, k);
            }
        }
        // 5. Counts follow the owner and the mark. A completed hall counted for the new owner sets its "ever had a
        // hall" mark, as a completion does (B-040 audit, V66).
        let u = &self.units[i];
        if u.building && u.construction.is_none() && crate::economy::kind::is_hall(u.kind) {
            self.players[to as usize].had_hall = true;
        }
        // 6. The announcement.
        if announce {
            self.sound_on(i, Sound::Capture);
        }
        // 7. The flash.
        self.units[i].flash = Flash(FLASH_CYCLES);
    }

    /// The circle of power's rescue check, its resting order's logic (order 59, run at a "step done" like every
    /// building's resting order): every footprint tile's ground occupant with the mark and a human-controlled owner is
    /// rescued, then the circle's animation waits 50 cycles before it goes on ("Rescuing at a circle of power", step 4).
    pub(crate) fn circle_check(&mut self, i: usize) {
        let c = &mut self.units[i];
        c.script.wait = CIRCLE_PAUSE;
        let (x, y) = c.tile;
        let (w, h) = (c.size.0 as i32, c.size.1 as i32);
        let hero_bit = self.required() & HERO_RESCUE as u32 != 0;
        for ty in y..y + h {
            for tx in x..x + w {
                let Some(j) = self.occupant(Layer::Land, tx, ty).and_then(|id| self.index(id)) else {
                    continue;
                };
                let o = &self.units[j];
                if !o.rescuable || !self.human_controlled(o.owner) {
                    continue;
                }
                let owner = o.owner as usize;
                let hero = self.stats_of(o.kind).flags & flags::HERO != 0;
                self.sound_on(i, Sound::Rescue);
                let p = &mut self.players[owner];
                if hero && hero_bit {
                    p.rescued = p.rescued.wrapping_add(HERO_RESCUE);
                }
                p.rescued = p.rescued.wrapping_add(1);
                self.units[j].rescuable = false;
            }
        }
    }

    /// The flash countdowns, once per cycle: down by 1, and cleared at once for a unit the local player can't detect.
    pub(crate) fn flash_pass(&mut self) {
        let local = self.vision.local;
        for u in &mut self.units {
            if u.flash.0 == 0 {
                continue;
            }
            let detected = local < 8 && u.detected_by & (1 << local) != 0;
            u.flash.0 = if detected { u.flash.0 - 1 } else { 0 };
        }
    }

    /// The colour the unit's selection box flashes in this cycle, if it is shown: palette entry 250 for the local
    /// player's unit, 251 for an ally's, 249 otherwise; on while the countdown has bit 3 set ("Flash").
    pub fn flash_colour(&self, id: UnitId) -> Option<u8> {
        let u = self.unit(id)?;
        if u.flash.0 & 8 == 0 {
            return None;
        }
        let local = self.vision.local;
        Some(if u.owner == local {
            flash_colour::OWN
        } else if self.allied(local, u.owner) {
            flash_colour::ALLIED
        } else {
            flash_colour::OTHER
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::economy::kind;
    use crate::sound::Cue;
    use crate::world::tests::field;
    use crate::world::{Command, Unit};
    use tidewater_formats::udta::UnitStats;

    const FOOTMAN: u8 = 0x00;
    const GRUNT: u8 = 0x01;
    const GRYPHON: u8 = 0x02;
    const DESTROYER: u8 = 0x1e;
    const TRANSPORT: u8 = 0x1c;
    const HERO: u8 = 0x2c;
    const BARRACKS: u8 = tech::building::HUMAN_BARRACKS;
    const RESCUE: u8 = 2;

    /// A 64 x 64 field. Player 0 is the local human player, player 1 another human player, player 2 a passive
    /// rescue player, player 3 a computer. Land units of 1 x 1 that can't attack, so nobody fights.
    fn world() -> World {
        let mut w = field(64, 64);
        let land = UnitStats {
            hit_points: 60,
            size: (1, 1),
            flags: flags::LAND,
            ..Default::default()
        };
        for k in [FOOTMAN, GRUNT, HERO, tech::unit::KNIGHT, tech::unit::ARCHER, 0x10] {
            w.stats.0[k as usize] = land;
        }
        w.stats.0[HERO as usize].flags |= flags::HERO;
        w.stats.0[tech::unit::PALADIN as usize] = UnitStats {
            hit_points: 90,
            flags: flags::LAND | flags::CASTER,
            ..land
        };
        w.stats.0[tech::unit::KNIGHT as usize].hit_points = 70;
        w.stats.0[GRYPHON as usize] = UnitStats {
            flags: flags::AIR,
            ..land
        };
        for k in [DESTROYER, TRANSPORT] {
            w.stats.0[k as usize] = UnitStats {
                flags: flags::SEA,
                ..land
            };
            // Ships move two tiles at a time.
            w.stride[k as usize] = 2;
        }
        w.stats.0[kind::TOWN_HALL as usize] = UnitStats {
            hit_points: 1200,
            size: (4, 4),
            flags: flags::BUILDING | flags::GOLD_STORAGE,
            ..Default::default()
        };
        w.stats.0[BARRACKS as usize] = UnitStats {
            hit_points: 800,
            size: (3, 3),
            flags: flags::BUILDING,
            build_time: 200,
            gold: 60,
            ..Default::default()
        };
        w.stats.0[CIRCLE_OF_POWER as usize] = UnitStats {
            hit_points: 1,
            size: (2, 2),
            flags: flags::BUILDING,
            ..Default::default()
        };
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::HUMAN;
        w.players[RESCUE as usize].controller = controller::RESCUE_PASSIVE;
        w.players[3].controller = controller::COMPUTER;
        w.vision.local = 0;
        w.rules.reveal_map = true;
        w
    }

    /// Creates a unit as the map loader does, so a rescue player's unit gets the mark.
    fn put(w: &mut World, k: u8, owner: u8, tile: (i32, i32)) -> UnitId {
        w.loading = true;
        let id = w.spawn(k, owner, tile, 0).unwrap();
        w.loading = false;
        id
    }

    fn u(w: &World, id: UnitId) -> &Unit {
        w.unit(id).unwrap()
    }

    fn i(w: &World, id: UnitId) -> usize {
        w.index(id).unwrap()
    }

    fn sounds(w: &World, s: Sound) -> usize {
        w.sounds.cues.iter().filter(|c: &&Cue| c.sound == s).count()
    }

    /// Runs the joining check for `id` directly, as its "step done" would.
    fn join(w: &mut World, id: UnitId) -> bool {
        let n = i(w, id);
        w.joining_check(n)
    }

    #[test]
    fn the_mark_comes_from_the_map_only() {
        let mut w = world();
        let loaded = put(&mut w, FOOTMAN, RESCUE, (5, 5));
        let later = w.spawn(FOOTMAN, RESCUE, (7, 5), 0).unwrap();
        let mine = put(&mut w, FOOTMAN, 0, (9, 5));
        put(&mut w, BARRACKS, RESCUE, (20, 20));
        assert!(u(&w, loaded).rescuable && !u(&w, later).rescuable && !u(&w, mine).rescuable);
        w.stats.0[FOOTMAN as usize].flags |= flags::HERO;
        assert_eq!(w.rescuable_counts(), (2, 1));
    }

    #[test]
    fn a_footman_joins_at_its_next_step_done() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        let g = put(&mut w, GRUNT, 0, (11, 11));
        let mut steps = 0;
        while u(&w, f).owner == RESCUE && steps < 20 {
            w.step(&[]);
            steps += 1;
        }
        assert_eq!(u(&w, f).owner, 0, "joined within a few cycles");
        assert!(u(&w, f).rescuable, "the mark is kept");
        assert_eq!(sounds(&w, Sound::Capture), 1);
        assert!(u(&w, f).flash.0 > 90);
        assert_eq!(w.players[0].rescued, 0);
        assert_eq!(w.rescuable_counts(), (1, 0));
        assert_eq!(w.players[0].converted, 1);
        // Campaign-flow spec, "Statistics": the old owner lost a unit.
        assert_eq!(w.players[RESCUE as usize].units_lost, 1);
        assert!(w.events.contains(&Event::OwnerChanged {
            unit: f,
            from: RESCUE,
            to: 0
        }));
        assert_eq!(u(&w, g).owner, 0);
        // Once owned by a human-controlled player it never checks again.
        assert!(!join(&mut w, f));
    }

    #[test]
    fn the_area_is_inclusive_on_the_right_and_bottom() {
        // Columns and rows 9 to 11 around a 1 x 1 footman at (10, 10).
        for (at, joins) in [
            ((11, 11), true),
            ((9, 9), true),
            ((12, 10), false),
            ((10, 12), false),
            ((8, 10), false),
        ] {
            let mut w = world();
            let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
            put(&mut w, GRUNT, 0, at);
            assert_eq!(join(&mut w, f), joins, "{at:?}");
        }
    }

    #[test]
    fn the_area_is_cut_at_the_map_edges() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (0, 5));
        put(&mut w, GRUNT, 0, (1, 5));
        assert!(join(&mut w, f));
        // At the right edge only columns 62 and 63 are tested: nothing at 64 is read.
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (63, 5));
        assert!(!join(&mut w, f));
        put(&mut w, GRUNT, 0, (62, 6));
        assert!(join(&mut w, f));
        // The top-left corner.
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (0, 0));
        put(&mut w, GRUNT, 0, (1, 1));
        assert!(join(&mut w, f));
    }

    #[test]
    fn ships_have_a_two_tile_ring_on_the_left_and_top() {
        let mut w = world();
        let d = put(&mut w, DESTROYER, RESCUE, (20, 20));
        // Ships stand on even tiles, so the transport asked for at (18, 19) stands at (18, 18).
        let t = put(&mut w, TRANSPORT, 0, (18, 19));
        assert_eq!(u(&w, t).tile, (18, 18));
        assert!(join(&mut w, d));
        assert_eq!(sounds(&w, Sound::Capture), 1);
        // A land unit exactly at (18, 19) qualifies too; column 17 is out.
        for (at, joins) in [((18, 19), true), ((17, 19), false)] {
            let mut w = world();
            let d = put(&mut w, DESTROYER, RESCUE, (20, 20));
            put(&mut w, GRUNT, 0, at);
            assert_eq!(join(&mut w, d), joins, "{at:?}");
        }
        // On the right the ring is still one tile wide (x + W = 21); 22 is out.
        let mut w = world();
        let d = put(&mut w, DESTROYER, RESCUE, (20, 20));
        put(&mut w, GRUNT, 0, (22, 20));
        assert!(!join(&mut w, d));
    }

    #[test]
    fn air_units_and_buildings_never_rescue() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        put(&mut w, GRYPHON, 0, (10, 11));
        put(&mut w, BARRACKS, 0, (11, 8));
        assert!(!join(&mut w, f));
        assert_eq!(u(&w, f).owner, RESCUE);
    }

    #[test]
    fn only_live_units_of_human_controlled_players_rescue() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        let c = put(&mut w, GRUNT, 3, (11, 10));
        let d = put(&mut w, GRUNT, 0, (9, 10));
        let n = i(&w, d);
        w.units[n].dying = true;
        assert!(!join(&mut w, f));
        // A computer player's marked unit checks too, and a human unit takes it.
        let n = i(&w, c);
        w.units[n].rescuable = true;
        put(&mut w, GRUNT, 1, (12, 10));
        assert!(join(&mut w, c));
        assert_eq!(u(&w, c).owner, 1);
    }

    #[test]
    fn the_first_row_decides_between_two_humans() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        put(&mut w, GRUNT, 1, (9, 11));
        put(&mut w, GRUNT, 0, (11, 9));
        assert!(join(&mut w, f));
        assert_eq!(u(&w, f).owner, 0);
        // Within a row, left before right.
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        put(&mut w, GRUNT, 0, (11, 9));
        put(&mut w, GRUNT, 1, (9, 9));
        assert!(join(&mut w, f));
        assert_eq!(u(&w, f).owner, 1);
    }

    #[test]
    fn a_hall_pulls_its_whole_side_along() {
        let mut w = world();
        w.players[RESCUE as usize].gold = 0;
        let hall = put(&mut w, kind::TOWN_HALL, RESCUE, (30, 30));
        let barracks = put(&mut w, BARRACKS, RESCUE, (40, 30));
        let man = put(&mut w, FOOTMAN, RESCUE, (50, 50));
        let inside = put(&mut w, FOOTMAN, RESCUE, (51, 50));
        let n = i(&w, inside);
        w.units[n].inside = true;
        let other = put(&mut w, FOOTMAN, 3, (52, 50));
        w.stats.0[kind::FARM as usize] = w.stats.0[CIRCLE_OF_POWER as usize];
        let farm = put(&mut w, kind::FARM, RESCUE, (45, 45));
        put(&mut w, GRUNT, 0, (34, 32));
        // The barracks is training a footman, paid by the rescue player.
        w.players[RESCUE as usize].gold = 1000;
        w.players[RESCUE as usize].units_allowed = !0;
        w.stats.0[FOOTMAN as usize].gold = 60;
        w.step(&[Command::Train {
            player: RESCUE,
            building: barracks,
            kind: FOOTMAN,
        }]);
        assert!(u(&w, barracks).job.is_some(), "{:?}", w.events);
        assert_eq!(w.players[RESCUE as usize].in_training, 1);
        let paid = 1000 - w.players[RESCUE as usize].gold;
        assert!(paid > 0);
        w.sounds.cues.clear();
        assert!(join(&mut w, hall));
        for id in [hall, barracks, man, farm] {
            assert_eq!(u(&w, id).owner, 0);
            assert!(u(&w, id).flash.0 == FLASH_CYCLES);
        }
        assert_eq!(u(&w, inside).owner, RESCUE, "off-map units stay");
        assert_eq!(u(&w, other).owner, 3);
        assert_eq!(sounds(&w, Sound::Capture), 1, "only the hall announces");
        assert!(u(&w, barracks).job.is_none());
        assert_eq!(w.players[RESCUE as usize].gold, 1000, "refunded to the old owner");
        assert_eq!(
            (w.players[RESCUE as usize].in_training, w.players[0].in_training),
            (0, 0)
        );
        assert_eq!(w.players[0].converted, 4);
    }

    /// Owner change, step 1: a working building's job ends at once (refund to the old owner) and order 33 is pending,
    /// taking over at the building's next step.
    #[test]
    fn a_rescued_working_building_gets_order_33_pending() {
        let mut w = world();
        let barracks = put(&mut w, BARRACKS, RESCUE, (40, 30));
        put(&mut w, kind::TOWN_HALL, RESCUE, (30, 30));
        w.players[RESCUE as usize].gold = 1000;
        w.players[RESCUE as usize].units_allowed = !0;
        w.stats.0[FOOTMAN as usize].gold = 60;
        w.stats.0[FOOTMAN as usize].build_time = 200;
        w.step(&[Command::Train {
            player: RESCUE,
            building: barracks,
            kind: FOOTMAN,
        }]);
        for _ in 0..20 {
            w.step(&[]);
        }
        assert!(u(&w, barracks).job.is_some_and(|j| j.running));
        let n = i(&w, barracks);
        w.rescue_owner_change(n, 0, false);
        let b = u(&w, barracks);
        assert_eq!(
            (b.job, b.order, b.pending),
            (None, crate::world::Order::Idle, Some(crate::world::Order::Idle))
        );
        assert_eq!(w.players[RESCUE as usize].gold, 1000);
        for _ in 0..10 {
            w.step(&[]);
        }
        assert_eq!(u(&w, barracks).pending, None, "taken over at a step");
    }

    /// V66: a completed hall counted for the new owner sets its "ever had a hall" mark (so its next town hall takes the
    /// full build time); a hall still under construction doesn't.
    #[test]
    fn a_rescued_completed_hall_sets_ever_had_a_hall() {
        for (site, want) in [(false, true), (true, false)] {
            let mut w = world();
            let hall = put(&mut w, kind::TOWN_HALL, RESCUE, (30, 30));
            if site {
                let n = i(&w, hall);
                w.units[n].construction = Some(Default::default());
                w.players[RESCUE as usize].under_construction = 1;
            }
            assert!(!w.players[0].had_hall);
            let n = i(&w, hall);
            w.rescue_owner_change(n, 0, false);
            assert_eq!(w.players[0].had_hall, want, "site {site}");
        }
    }

    #[test]
    fn a_site_moves_between_the_construction_counts() {
        let mut w = world();
        let site = put(&mut w, BARRACKS, RESCUE, (10, 10));
        let n = i(&w, site);
        w.units[n].construction = Some(Default::default());
        w.players[RESCUE as usize].under_construction = 1;
        put(&mut w, GRUNT, 0, (13, 13));
        assert!(join(&mut w, site));
        assert_eq!(
            (
                w.players[RESCUE as usize].under_construction,
                w.players[0].under_construction
            ),
            (0, 1)
        );
        assert!(u(&w, site).construction.is_some());
    }

    #[test]
    fn enhanced_mode_converts_knights_for_a_paladin_owner() {
        for (enhanced, want) in [(true, tech::unit::PALADIN), (false, tech::unit::KNIGHT)] {
            let mut w = world();
            w.rules.enhanced = enhanced;
            w.players[0].spells_known |= tech::PALADIN_BIT;
            let k = put(&mut w, tech::unit::KNIGHT, RESCUE, (10, 10));
            let n = i(&w, k);
            w.units[n].hp = 50;
            put(&mut w, GRUNT, 0, (11, 10));
            assert!(join(&mut w, k));
            assert_eq!(u(&w, k).kind, want);
            if enhanced {
                assert_eq!((u(&w, k).hp, u(&w, k).mana), (70, 85), "+20 hit points, starting mana");
            } else {
                assert_eq!(u(&w, k).hp, 50);
            }
        }
        // Archers need the ranger level.
        let mut w = world();
        let a = put(&mut w, tech::unit::ARCHER, RESCUE, (10, 10));
        put(&mut w, GRUNT, 0, (11, 10));
        assert!(join(&mut w, a));
        assert_eq!(u(&w, a).kind, tech::unit::ARCHER);
        let mut w = world();
        w.players[0].levels[tech::RANGER_TRAINING_LINE] = 1;
        let a = put(&mut w, tech::unit::ARCHER, RESCUE, (10, 10));
        put(&mut w, GRUNT, 0, (11, 10));
        assert!(join(&mut w, a));
        assert_eq!(u(&w, a).kind, tech::unit::RANGER);
    }

    #[test]
    fn the_flash_blinks_and_fades() {
        let mut w = world();
        w.players[0].allies = 1 << 1;
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        let n = i(&w, f);
        w.units[n].flash = Flash(FLASH_CYCLES);
        // 100 has bit 3 clear: off; 99 to 96 too; 95 (0b101_1111) on.
        assert_eq!(w.flash_colour(f), None);
        w.units[n].flash = Flash(95);
        assert_eq!(w.flash_colour(f), Some(flash_colour::OTHER));
        w.units[n].owner = 1;
        assert_eq!(w.flash_colour(f), Some(flash_colour::ALLIED));
        w.units[n].owner = 0;
        assert_eq!(w.flash_colour(f), Some(flash_colour::OWN));
        let mut on = 0;
        w.units[n].flash = Flash(FLASH_CYCLES);
        for _ in 0..120 {
            w.step(&[]);
            on += usize::from(w.flash_colour(f).is_some());
        }
        assert_eq!(u(&w, f).flash.0, 0);
        assert!(on > 40 && on < 60, "about half the cycles: {on}");
        // Not detected by the local player: cleared at once. And the flash isn't game state.
        let h = w.state_hash();
        w.units[n].flash = Flash(50);
        assert_eq!(w.state_hash(), h);
        w.units[n].invisibility = 100;
        w.units[n].owner = 3;
        w.step(&[]);
        assert_eq!(u(&w, f).flash.0, 0);
    }

    fn circle_world() -> (World, UnitId) {
        let mut w = world();
        let c = put(&mut w, CIRCLE_OF_POWER, 15, (20, 20));
        (w, c)
    }

    /// Runs the circle's check now.
    fn check(w: &mut World, c: UnitId) {
        let n = i(w, c);
        w.circle_check(n);
    }

    #[test]
    fn a_marked_unit_is_rescued_on_a_circle() {
        let (mut w, c) = circle_world();
        let f = put(&mut w, FOOTMAN, RESCUE, (21, 21));
        let n = i(&w, f);
        w.change_owner(n, 0);
        check(&mut w, c);
        assert_eq!(sounds(&w, Sound::Rescue), 1);
        assert_eq!(w.players[0].rescued, 1);
        assert!(!u(&w, f).rescuable);
        assert_eq!(w.rescuable_counts(), (0, 0));
        // Never counted twice.
        check(&mut w, c);
        assert_eq!(w.players[0].rescued, 1);
    }

    #[test]
    fn heroes_add_0x1000_only_when_the_mission_asks() {
        for (rule, want) in [(3u8, 0x1001u16), (2, 1)] {
            let (mut w, c) = circle_world();
            w.victory.rule = Some(crate::victory::mission_rule(rule));
            let h = put(&mut w, HERO, RESCUE, (20, 20));
            let n = i(&w, h);
            w.change_owner(n, 0);
            assert_eq!(w.rescuable_counts(), (1, 1));
            check(&mut w, c);
            assert_eq!(w.players[0].rescued, want);
            assert_eq!(w.rescuable_counts(), (0, 0));
        }
    }

    #[test]
    fn two_units_in_one_check() {
        let (mut w, c) = circle_world();
        for at in [(20, 20), (21, 21)] {
            let f = put(&mut w, FOOTMAN, RESCUE, at);
            let n = i(&w, f);
            w.change_owner(n, 0);
        }
        check(&mut w, c);
        assert_eq!(sounds(&w, Sound::Rescue), 2);
        assert_eq!(w.players[0].rescued, 2);
    }

    #[test]
    fn computer_units_are_never_rescued() {
        let (mut w, c) = circle_world();
        let f = put(&mut w, FOOTMAN, RESCUE, (20, 20));
        let n = i(&w, f);
        w.change_owner(n, 3);
        check(&mut w, c);
        // The rescue player's own marked unit isn't either.
        put(&mut w, FOOTMAN, RESCUE, (21, 20));
        check(&mut w, c);
        assert_eq!(sounds(&w, Sound::Rescue), 0);
        assert_eq!(w.rescuable_counts(), (2, 0));
    }

    /// "Rescuing at a circle of power", step 4: after a check the circle's animation waits 50 cycles before it goes
    /// on, so the checks (order 59's logic, at a step done of the still animation) come 50 cycles plus the rest of the
    /// still loop apart. The first runs at the first step done after order 59 has taken over, not at creation.
    #[test]
    fn the_circle_pauses_its_animation_after_each_check() {
        let (mut w, id) = circle_world();
        let mut checks = Vec::new();
        for t in 0..400 {
            w.step(&[]);
            if u(&w, id).script.wait == CIRCLE_PAUSE {
                checks.push(t);
            }
        }
        assert!(checks.len() >= 3 && checks[0] > 0, "{checks:?}");
        let period = checks[1] - checks[0];
        assert!(period > CIRCLE_PAUSE as usize, "{checks:?}");
        assert!(checks.windows(2).all(|p| p[1] - p[0] == period), "{checks:?}");
        // The pause counts down like any wait.
        let (mut w, id) = circle_world();
        for _ in 0..=checks[0] {
            w.step(&[]);
        }
        w.step(&[]);
        assert_eq!(u(&w, id).script.wait, CIRCLE_PAUSE - 1);
    }

    #[test]
    fn a_joined_unit_walks_onto_a_circle() {
        let (mut w, _) = circle_world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        put(&mut w, GRUNT, 0, (11, 10));
        for _ in 0..10 {
            w.step(&[]);
        }
        assert_eq!(u(&w, f).owner, 0);
        w.step(&[Command::Move {
            player: 0,
            units: vec![f],
            to: (21, 21),
        }]);
        for _ in 0..400 {
            w.step(&[]);
        }
        assert_eq!(u(&w, f).tile, (21, 21));
        assert_eq!((w.players[0].rescued, sounds(&w, Sound::Rescue)), (1, 1));
        assert!(!u(&w, f).rescuable);
    }

    #[test]
    fn a_joined_footman_dying_stops_counting() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        put(&mut w, GRUNT, 0, (11, 10));
        assert!(join(&mut w, f));
        assert_eq!(w.rescuable_counts(), (1, 0));
        let n = i(&w, f);
        w.kill(n);
        assert_eq!(w.rescuable_counts(), (0, 0));
        assert_eq!(w.players[0].rescued, 0);
    }

    #[test]
    fn frozen_units_skip_and_resume() {
        let mut w = world();
        let f = put(&mut w, FOOTMAN, RESCUE, (10, 10));
        let n = i(&w, f);
        w.units[n].freeze = Freeze::Freezing;
        put(&mut w, GRUNT, 0, (11, 10));
        // The next "step done" is skipped and the unit freezes; frozen, it never checks.
        for _ in 0..30 {
            w.step(&[]);
        }
        assert_eq!((u(&w, f).owner, u(&w, f).freeze), (RESCUE, Freeze::Frozen));
        let script = u(&w, f).script;
        w.step(&[]);
        assert_eq!(u(&w, f).script, script, "no animation");
        // Released: the check runs on the next update.
        let n = i(&w, f);
        w.units[n].freeze = Freeze::Released;
        w.step(&[]);
        assert_eq!((u(&w, f).owner, u(&w, f).freeze), (0, Freeze::None));
    }
}
