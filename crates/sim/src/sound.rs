//! The simulation's share of the unit and event sounds. Spec: `docs/specs/ui/sounds.md`.
//!
//! Sounds never change the game (with one exception, the critter explosion), so nothing here is part of
//! [`World::state_hash`]. The sim says *what happened* as [`Cue`]s, drained by the client each tick, and keeps the
//! state the spec ties to game events: the selection memory and click counts ("Selecting units", "Critters"), the
//! help timer and per-unit quiet counters ("Under attack"), the construction-sound timer ("Construction") and each
//! unit's pending sound ("Attacks"). Which file plays, the variant draws from the setup-shuffle generator, the sound
//! options, the fog and screen tests and the subtitles belong to the client (`crates/client/src/sound.rs`).
//!
//! Much of this state is about the local player (`Vision::local`) and so differs between machines in multiplayer;
//! that is why it stays out of the state hash. The one exception that touches the game, the critter kill, only
//! happens in single player.

use std::collections::BTreeMap;

use tidewater_formats::udta::flags;

use crate::slots;
use crate::vision::bit;
use crate::world::{UnitId, World};

/// The critter's unit type (0x39), the last type with unit voices.
pub const CRITTER: u8 = 0x39;
/// Guard towers and cannon towers ("Attacks", "Towers").
const GUARD_TOWERS: [u8; 2] = [0x60, 0x61];
const CANNON_TOWERS: [u8; 2] = [0x62, 0x63];
/// Wall pieces: no building explosion ("Deaths").
const WALLS: [u8; 2] = [0x67, 0x68];
/// The peasant (type 0x02) has its own work-complete file.
const PEASANT: u8 = 0x02;

/// Timer values, in interface ticks ("Selecting units", "Under attack", "Construction").
pub const SELECT_COOLDOWN: u16 = 10;
pub const LONG_COOLDOWN: u16 = 30;
pub const CONSTRUCTION_TIMER: u16 = 150;
pub const HELP_TIMER: u16 = 60;
pub const QUIET: u8 = 200;
/// The under-attack square is 23 x 23 tiles centred on the victim.
const QUIET_REACH: i32 = 11;
/// Clicks on the same critter that make it explode (single player).
const CRITTER_CLICKS: u8 = 10;

/// Where a placed sound is, with what the client's silencing rules need, captured when the sound is made
/// ("How a sound is played").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// On a unit: its tile, whether it is inside another unit, and whether the local player was in its hidden-from
    /// set as it stood after the most recent phase 5.
    Unit {
        tile: (i32, i32),
        inside: bool,
        hidden: bool,
    },
    /// At a tile: whether the local player's "seen" bit there says "not seen". A tile outside the map counts as seen
    /// (the test is skipped).
    Tile { tile: (i32, i32), unseen: bool },
}

impl Place {
    pub fn tile(&self) -> (i32, i32) {
        match *self {
            Place::Unit { tile, .. } | Place::Tile { tile, .. } => tile,
        }
    }
}

/// Which case of "Selecting units" step 4 picked the selection sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A completed building showing the large fire: `misc\burning`.
    Burning,
    /// A building under construction: `misc\constrct`.
    Constructing,
    /// A building row of "Building selection sounds", or `Button` for any other type above the critter's.
    /// `wood_storage` is the type's wood-storage flag (bit 18), which one row tests.
    Building {
        wood_storage: bool,
    },
    /// One of the type's selected files (a draw if there are several).
    Selected,
    /// The type's n-th annoyed file, counting from 1.
    Annoyed(u8),
    /// The critter's selected or annoyed file for the map's terrain.
    CritterSelected,
    CritterAnnoyed,
}

/// What made a sound, before a file is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    // Plain sounds.
    /// The selection sound of a unit of type `kind`. `building` decides which option it plays under.
    Select {
        kind: u8,
        building: bool,
        pick: Pick,
    },
    Acknowledge {
        kind: u8,
    },
    Ready {
        kind: u8,
    },
    /// Work complete; the peasant has its own file.
    WorkComplete {
        peasant: bool,
    },
    /// Under attack: a unit or a building of the local player.
    Help {
        building: bool,
    },
    /// `misc\explode` after the critter explosion, under the acknowledgements option.
    CritterExplode,
    /// A unit's attack sound made pending ("Attacks"): no sound plays. The client makes the variant draw now, in
    /// the simulation update, and holds the file for `unit` until its `Attack` cue plays it, so a pending sound that
    /// is cleared or never heard has still made its draw.
    AttackPending {
        kind: u8,
        unit: UnitId,
    },
    // Placed sounds.
    /// A unit's pending attack sound, played by its animation: the file drawn at its `AttackPending` cue.
    Attack {
        kind: u8,
        unit: UnitId,
    },
    /// A worker's chop sound made pending (harvest spec, "Chop sound"): no sound plays; the client makes the variant
    /// draw now and holds the file for `unit` until its `Chop` cue plays it.
    ChopPending {
        unit: UnitId,
    },
    /// A worker's pending chop sound (silent off screen): the file drawn at its `ChopPending` cue.
    Chop {
        unit: UnitId,
    },
    TowerFire {
        kind: u8,
    },
    /// A unit (not a building) killed; `flags` are its type's flags, which the death rules read.
    Death {
        kind: u8,
        flags: u32,
    },
    BuildingDeath,
    /// A missile reached its impact; `damaging` when it carried damage.
    Impact {
        kind: u8,
        damaging: bool,
    },
    Explode,
    Construction,
    Dock,
    /// A circle of power rescued a unit (rescue spec): `misc\hrescue` or `misc\orescue`.
    Rescue,
    /// A rescuable unit joined a human-controlled player (rescue spec): `misc\hcapture` or `misc\ocapture`.
    Capture,
    /// A spell took effect; the spell's order number (38 to 56). The eye of Kilrogg (48) is a plain sound.
    Spell(u8),
}

/// One sound event: plain when `place` is `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cue {
    pub sound: Sound,
    pub place: Option<Place>,
}

/// A unit's pending sound, played by the animation's play-sound instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    Attack(u8),
    Chop,
}

/// Sound state. Not game state: excluded from the state hash.
#[derive(Clone, Debug, Default)]
pub struct Sounds {
    /// Cues since the client last drained them, in the order they happened.
    pub cues: Vec<Cue>,
    /// The demo playback mode: no under-attack sounds (the client skips the tile test).
    pub demo: bool,
    pub help_timer: u16,
    pub construction_timer: u16,
    /// The selection memory: the last unit that made a selection sound, its click count, the critter count and the
    /// selection cooldown.
    pub remembered: Option<UnitId>,
    pub clicks: u8,
    pub critters: u8,
    pub cooldown: u16,
    /// The latest event location (a unit ready or under attack), for the interface.
    pub last_event: Option<(i32, i32)>,
    /// Every event location since the client last drained them (a unit ready, work complete or an under-attack
    /// sound), for the event history of `docs/specs/ui/messages.md`.
    pub events: Vec<(i32, i32)>,
    /// Quiet counters above 0, by unit.
    quiet: BTreeMap<UnitId, u8>,
    pending: BTreeMap<UnitId, Pending>,
}

impl Sounds {
    /// A new latest event location.
    fn event_at(&mut self, tile: (i32, i32)) {
        self.last_event = Some(tile);
        self.events.push(tile);
    }

    /// A unit's quiet counter.
    pub fn quiet(&self, id: UnitId) -> u8 {
        self.quiet.get(&id).copied().unwrap_or(0)
    }

    /// One interface tick ("Timers"). Open question "Tick rate": we take one tick per game cycle. Quiet counters go
    /// down on every second tick; we take the even cycle numbers.
    fn tick(&mut self, cycle: u32) {
        self.cooldown = self.cooldown.saturating_sub(1);
        self.construction_timer = self.construction_timer.saturating_sub(1);
        self.help_timer = self.help_timer.saturating_sub(1);
        if cycle.is_multiple_of(2) {
            self.quiet.retain(|_, q| {
                *q -= 1;
                *q > 0
            });
        }
    }

    fn forget(&mut self) {
        self.remembered = None;
        self.clicks = 0;
        self.critters = 0;
    }
}

/// The annoyance limit L of a unit type ("Unit sounds", the number of annoyed files; 0 for "none"). The critter's
/// limit of 1 is applied by its own rule.
pub fn annoyance_limit(kind: u8) -> u8 {
    match kind {
        0x00..=0x03 | 0x10 | 0x11 => 7,
        0x07 | 0x28 => 5,
        0x27 => 4,
        0x29 => 2,
        0x06 | 0x08..=0x0f | 0x12..=0x21 | 0x23 | 0x26 | 0x2c | 0x2e | 0x2f | 0x31..=0x35 => 3,
        _ => 0,
    }
}

impl World {
    fn local(&self) -> u8 {
        self.vision.local
    }

    /// Build-train spec, "The local player sees it": sounds (and messages, [`World::tell`]) about a player's own
    /// units are for that player only if it is the local player. Every other effect happens for every player.
    pub(crate) fn local_owner(&self, owner: u8) -> bool {
        owner == self.local()
    }

    /// The placement of a sound on unit `i` as it stands now.
    pub(crate) fn unit_place(&self, i: usize) -> Place {
        let u = &self.units[i];
        Place::Unit {
            tile: u.tile,
            inside: u.inside,
            hidden: u.hidden_from & bit(self.local()) != 0,
        }
    }

    /// The placement of a sound at a tile.
    pub(crate) fn tile_place(&self, tile: (i32, i32)) -> Place {
        let inside = self.terrain.contains(tile.0, tile.1);
        Place::Tile {
            tile,
            unseen: inside && self.vision.unseen_by(self.local(), tile.0, tile.1),
        }
    }

    fn cue(&mut self, sound: Sound, place: Option<Place>) {
        self.sounds.cues.push(Cue { sound, place });
    }

    pub(crate) fn cue_plain(&mut self, sound: Sound) {
        self.cue(sound, None);
    }

    pub(crate) fn sound_on(&mut self, i: usize, sound: Sound) {
        let p = self.unit_place(i);
        self.cue(sound, Some(p));
    }

    /// A placed sound at the tile holding map pixel `px`.
    pub(crate) fn sound_at_px(&mut self, px: (i32, i32), sound: Sound) {
        let t = (px.0.div_euclid(crate::TILE_PX), px.1.div_euclid(crate::TILE_PX));
        let p = self.tile_place(t);
        self.cue(sound, Some(p));
    }

    pub(crate) fn sound_at_tile(&mut self, tile: (i32, i32), sound: Sound) {
        let p = self.tile_place(tile);
        self.cue(sound, Some(p));
    }

    /// The interface timers, at the end of each cycle.
    pub(crate) fn sound_tick(&mut self) {
        let cycle = self.tick;
        self.sounds.tick(cycle);
        // Pending sounds of units that have left the table are dropped.
        if !self.sounds.pending.is_empty() {
            let units = &self.units;
            let slot_of = |id: &UnitId| units.iter().any(|u| u.id == *id);
            self.sounds.pending.retain(|id, _| slot_of(id));
        }
    }

    /// "Attacks": a unit turning to face its target to strike gets its type's attack sound as its pending sound.
    pub(crate) fn pend_attack(&mut self, i: usize) {
        let u = &self.units[i];
        if !u.building {
            let (kind, unit) = (u.kind, u.id);
            self.sounds.pending.insert(unit, Pending::Attack(kind));
            self.cue_plain(Sound::AttackPending { kind, unit });
        }
    }

    /// "Workers chopping": each chop makes the chop sound the worker's pending sound.
    pub(crate) fn pend_chop(&mut self, i: usize) {
        let unit = self.units[i].id;
        self.sounds.pending.insert(unit, Pending::Chop);
        self.cue_plain(Sound::ChopPending { unit });
    }

    /// The animation's play-sound instruction: plays the pending sound on the unit and clears it.
    pub(crate) fn play_pending(&mut self, i: usize) {
        let Some(p) = self.sounds.pending.remove(&self.units[i].id) else {
            return;
        };
        let unit = self.units[i].id;
        let sound = match p {
            Pending::Attack(kind) => Sound::Attack { kind, unit },
            Pending::Chop => Sound::Chop { unit },
        };
        self.sound_on(i, sound);
    }

    /// "Towers": a guard or cannon tower firing.
    pub(crate) fn tower_fire_sound(&mut self, i: usize) {
        let kind = self.units[i].kind;
        if GUARD_TOWERS.contains(&kind) || CANNON_TOWERS.contains(&kind) {
            self.sound_on(i, Sound::TowerFire { kind });
        }
    }

    /// "Deaths": the sound of unit `i` being killed, placed as the unit stood before the kill. Nothing while the map is
    /// being set up; a building with the used mark or a wall piece makes no sound.
    pub(crate) fn death_sound(&mut self, i: usize) {
        let u = &self.units[i];
        if self.loading {
            return;
        }
        let sound = if u.building {
            if u.used || WALLS.contains(&u.kind) {
                return;
            }
            Sound::BuildingDeath
        } else {
            Sound::Death {
                kind: u.kind,
                flags: self.stats_of(u.kind).flags,
            }
        };
        self.sounds.pending.remove(&u.id);
        self.sound_on(i, sound);
    }

    /// "Under attack": a hit that took hit points from unit `i` without killing it.
    pub(crate) fn under_attack(&mut self, i: usize) {
        // The minimap's attack mark comes before this check's other tests (messages spec, "Attacks on the minimap").
        self.mark_attacked(i);
        let u = &self.units[i];
        let s = &self.sounds;
        if !self.local_owner(u.owner)
            || s.demo
            || self.stats_of(u.kind).flags & flags::CRITTER != 0
            || s.help_timer != 0
            || s.quiet(u.id) != 0
        {
            return;
        }
        let (c, building) = (u.tile, u.building);
        for y in c.1 - QUIET_REACH..=c.1 + QUIET_REACH {
            for x in c.0 - QUIET_REACH..=c.0 + QUIET_REACH {
                for plane in &self.occupancy {
                    if let Some(id) = self.terrain.at(x, y).and_then(|n| plane[n]) {
                        self.sounds.quiet.insert(id, QUIET);
                    }
                }
            }
        }
        self.sounds.help_timer = HELP_TIMER;
        self.cue(Sound::Help { building }, None);
        self.sounds.event_at(c);
    }

    /// "Unit ready": a trained unit was placed.
    pub(crate) fn ready_sound(&mut self, id: UnitId) {
        let Some(u) = self.unit(id) else { return };
        if !self.local_owner(u.owner) {
            return;
        }
        let (kind, tile) = (u.kind, u.tile);
        self.cue(Sound::Ready { kind }, None);
        self.sounds.event_at(tile);
    }

    /// "Work complete" for unit `i` (a worker out of a finished building, or in Enhanced rules a building whose
    /// research or upgrade finished).
    pub(crate) fn work_complete(&mut self, i: usize) {
        let u = &self.units[i];
        if self.local_owner(u.owner) {
            let (peasant, tile) = (u.kind == PEASANT, u.tile);
            self.cue(Sound::WorkComplete { peasant }, None);
            // Messages spec, "Event history": work complete is an event too.
            self.sounds.events.push(tile);
        }
    }

    /// "Construction": the sound asked for on a construction pass of building `i` (every pass whose count is a
    /// multiple of 12, and every pass with the build-speed cheat on).
    pub(crate) fn construction_sound(&mut self, i: usize) {
        if self.sounds.construction_timer != 0 {
            return;
        }
        // Our reading: the timer restarts whenever the sound is due, whether or not the client can hear it.
        self.sounds.construction_timer = CONSTRUCTION_TIMER;
        self.sound_on(i, Sound::Construction);
    }

    /// Whether this is a single-player game: exactly one human player ("Critters").
    fn single_player(&self) -> bool {
        self.players
            .iter()
            .filter(|p| p.controller == crate::ai::controller::HUMAN)
            .count()
            == 1
    }

    /// "Acknowledging orders": the local player gave an order (or cast the eye of Kilrogg) with `first` as the
    /// selection's first unit. Call it from the interface; it makes no draws.
    pub fn acknowledge(&mut self, first: UnitId) {
        self.sounds.remembered = None;
        let Some(u) = self.unit(first) else { return };
        if self.local_owner(u.owner) {
            let kind = u.kind;
            self.cue(Sound::Acknowledge { kind }, None);
        }
    }

    /// "Selecting units": a selecting action with `first` as the selection's first unit. Call it from the interface
    /// between cycles. In single player the 10th click on a critter kills it here, which makes the kill's draws;
    /// open question "When the critter explosion happens": we apply it between two cycles, as the default says.
    pub fn select_sound(&mut self, first: UnitId) {
        let Some(i) = self.index(first) else { return };
        let u = &self.units[i];
        if u.dying {
            return;
        }
        // 1. Owner.
        if !self.local_owner(u.owner) && u.owner != slots::NEUTRAL {
            self.sounds.forget();
            return;
        }
        // 2. Cooldown.
        let s = &mut self.sounds;
        if s.remembered == Some(first) && s.cooldown > 0 {
            return;
        }
        // 3. Start.
        s.cooldown = SELECT_COOLDOWN;
        if s.remembered != Some(first) {
            s.remembered = Some(first);
            s.clicks = 0;
            s.critters = 0;
        }
        // 4. Pick the sound.
        let u = &self.units[i];
        let st = self.stats_of(u.kind);
        let (kind, building) = (u.kind, u.building);
        let wood_storage = st.flags & flags::WOOD_STORAGE != 0;
        let pick = if building && u.construction.is_none() && u.hp < st.hit_points / 2 {
            Pick::Burning
        } else if u.construction.is_some() {
            self.sounds.cooldown = LONG_COOLDOWN;
            self.sounds.construction_timer = CONSTRUCTION_TIMER;
            Pick::Constructing
        } else if wood_storage || kind > CRITTER {
            Pick::Building { wood_storage }
        } else if kind == CRITTER {
            let s = &mut self.sounds;
            s.critters += 1;
            if s.critters == CRITTER_CLICKS {
                s.critters = 0;
                if self.single_player() {
                    return self.critter_explodes(i);
                }
                Pick::CritterSelected
            } else {
                match self.count_click(1) {
                    Pick::Annoyed(_) => Pick::CritterAnnoyed,
                    _ => Pick::CritterSelected,
                }
            }
        } else {
            self.count_click(annoyance_limit(kind))
        };
        // 5. Play.
        self.cue(Sound::Select { kind, building, pick }, None);
    }

    /// The unit case of step 4 with annoyance limit `limit`.
    fn count_click(&mut self, limit: u8) -> Pick {
        let s = &mut self.sounds;
        s.clicks = s.clicks.saturating_add(1);
        let n = s.clicks.saturating_sub(3);
        if n == 0 {
            Pick::Selected
        } else if n <= limit {
            s.cooldown = LONG_COOLDOWN;
            Pick::Annoyed(n)
        } else {
            s.clicks = 0;
            Pick::Selected
        }
    }

    /// "Critters", the 10th click in single player: the ordinary kill (with its death sound), the cosmetic explosion
    /// the explode flag would make, then `misc\explode`.
    fn critter_explodes(&mut self, i: usize) {
        let (id, c) = (self.units[i].id, self.units[i].centre_px());
        self.kill(i);
        self.explosion(id, (c.0, c.1 - 8));
        self.cue(Sound::CritterExplode, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command;
    use crate::ai::controller;
    use crate::world::tests::{field, run, stats};
    use tidewater_formats::udta::UnitStats;

    const FOOTMAN: u8 = 0x00;
    const KNIGHT: u8 = 0x06;
    const ARCHER: u8 = 0x08;
    const FARM: u8 = 0x3a;
    const BARRACKS: u8 = 0x3e;
    const MINE: u8 = 0x5c;

    /// A 40 x 40 field, single player: player 0 human and local, player 1 a computer.
    fn lab() -> World {
        let mut w = field(40, 40);
        let mut st = stats();
        st.0[KNIGHT as usize] = st.0[FOOTMAN as usize];
        st.0[CRITTER as usize] = UnitStats {
            hit_points: 5,
            size: (1, 1),
            flags: flags::LAND | flags::CRITTER,
            ..Default::default()
        };
        st.0[MINE as usize] = UnitStats {
            hit_points: 25500,
            size: (3, 3),
            flags: flags::BUILDING | flags::MINE,
            ..Default::default()
        };
        st.0[BARRACKS as usize].hit_points = 800;
        w.stats = st;
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        w.vision.local = 0;
        w
    }

    fn picks(w: &mut World) -> Vec<Pick> {
        std::mem::take(&mut w.sounds.cues)
            .into_iter()
            .filter_map(|c| match c.sound {
                Sound::Select { pick, .. } => Some(pick),
                _ => None,
            })
            .collect()
    }

    /// Selects `id` `n` times, letting the cooldown run out before each click.
    fn clicks(w: &mut World, id: UnitId, n: usize) -> Vec<Pick> {
        let mut out = Vec::new();
        for _ in 0..n {
            w.sounds.cooldown = 0;
            w.select_sound(id);
            out.extend(picks(w));
        }
        out
    }

    #[test]
    fn footman_eleven_clicks() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        w.select_sound(f);
        assert_eq!(w.sounds.cooldown, SELECT_COOLDOWN);
        w.sounds.cooldown = 0;
        let mut got = picks(&mut w);
        got.extend(clicks(&mut w, f, 10));
        let mut want = vec![Pick::Selected; 3];
        want.extend((1..=7).map(Pick::Annoyed));
        want.push(Pick::Selected);
        assert_eq!(got, want);
        assert_eq!(w.sounds.clicks, 0, "click 11 restarts the count");
        // Then three more selected sounds before annoyed 1 again.
        assert_eq!(
            clicks(&mut w, f, 4),
            [Pick::Selected, Pick::Selected, Pick::Selected, Pick::Annoyed(1)]
        );
        assert_eq!(w.sounds.cooldown, LONG_COOLDOWN);
    }

    #[test]
    fn knight_seven_clicks() {
        let mut w = lab();
        let k = w.spawn(KNIGHT, 0, (5, 5), 0).unwrap();
        let got = clicks(&mut w, k, 7);
        let mut want = vec![Pick::Selected; 3];
        want.extend((1..=3).map(Pick::Annoyed));
        want.push(Pick::Selected);
        assert_eq!(got, want);
        assert_eq!(w.sounds.clicks, 0);
    }

    #[test]
    fn a_click_within_the_cooldown_plays_nothing_and_is_not_counted() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        w.select_sound(f);
        run(&mut w, 9);
        w.select_sound(f);
        assert_eq!(picks(&mut w), [Pick::Selected]);
        assert_eq!(w.sounds.clicks, 1);
        run(&mut w, 1);
        w.select_sound(f);
        assert_eq!(picks(&mut w), [Pick::Selected]);
        assert_eq!(w.sounds.clicks, 2);
    }

    #[test]
    fn an_order_forgets_the_remembered_unit() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        clicks(&mut w, f, 3);
        w.acknowledge(f);
        assert_eq!(
            w.sounds.cues.pop().map(|c| c.sound),
            Some(Sound::Acknowledge { kind: FOOTMAN })
        );
        assert_eq!(clicks(&mut w, f, 1), [Pick::Selected]);
    }

    #[test]
    fn another_unit_resets_the_count() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let a = w.spawn(ARCHER, 0, (7, 5), 0).unwrap();
        clicks(&mut w, f, 1);
        clicks(&mut w, a, 1);
        assert_eq!(clicks(&mut w, f, 3), [Pick::Selected; 3]);
    }

    #[test]
    fn an_enemy_plays_nothing_and_clears_the_memory() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let e = w.spawn(FOOTMAN, 1, (9, 5), 0).unwrap();
        clicks(&mut w, f, 2);
        assert_eq!(clicks(&mut w, e, 1), []);
        assert_eq!((w.sounds.remembered, w.sounds.clicks, w.sounds.critters), (None, 0, 0));
        // An enemy's acknowledgement doesn't play either.
        w.acknowledge(e);
        assert!(w.sounds.cues.is_empty());
    }

    #[test]
    fn buildings() {
        let mut w = lab();
        let mine = w.spawn(MINE, slots::NEUTRAL, (20, 20), 0).unwrap();
        assert_eq!(clicks(&mut w, mine, 1), [Pick::Building { wood_storage: false }]);
        let b = w.spawn(BARRACKS, 0, (10, 10), 0).unwrap();
        let hp = |w: &mut World, hp| {
            let i = w.index(b).unwrap();
            w.units[i].hp = hp;
        };
        hp(&mut w, 300);
        assert_eq!(clicks(&mut w, b, 1), [Pick::Burning]);
        hp(&mut w, 400);
        assert_eq!(clicks(&mut w, b, 1), [Pick::Building { wood_storage: false }]);
    }

    #[test]
    fn a_site_under_construction() {
        let mut w = lab();
        let farm = w.place_building(FARM, 0, (12, 12)).unwrap();
        w.select_sound(farm);
        assert_eq!(picks(&mut w), [Pick::Constructing]);
        assert_eq!(
            (w.sounds.cooldown, w.sounds.construction_timer),
            (LONG_COOLDOWN, CONSTRUCTION_TIMER)
        );
    }

    #[test]
    fn sheep_ten_clicks_in_single_player() {
        let mut w = lab();
        let sheep = w.spawn(CRITTER, slots::NEUTRAL, (8, 8), 0).unwrap();
        let mut got = Vec::new();
        for _ in 0..9 {
            w.sounds.cooldown = 0;
            w.select_sound(sheep);
            got.extend(picks(&mut w));
        }
        use Pick::{CritterAnnoyed as A, CritterSelected as S};
        assert_eq!(got, [S, S, S, A, S, S, S, S, A]);
        w.sounds.cooldown = 0;
        w.select_sound(sheep);
        let cues: Vec<Sound> = w.sounds.cues.iter().map(|c| c.sound).collect();
        assert_eq!(
            cues,
            [
                Sound::Death {
                    kind: CRITTER,
                    flags: flags::LAND | flags::CRITTER
                },
                Sound::CritterExplode
            ]
        );
        assert!(w.unit(sheep).unwrap().dying);
        assert!(
            w.missiles.iter().flatten().any(|m| m.kind == 0x17),
            "the explosion effect"
        );
        assert_eq!(w.sounds.critters, 0);
    }

    #[test]
    fn sheep_tenth_click_in_multiplayer() {
        let mut w = lab();
        w.players[1].controller = controller::HUMAN;
        let sheep = w.spawn(CRITTER, slots::NEUTRAL, (8, 8), 0).unwrap();
        let got = clicks(&mut w, sheep, 10);
        assert_eq!(got[9], Pick::CritterSelected);
        assert!(!w.unit(sheep).unwrap().dying);
    }

    #[test]
    fn annoyance_limits_follow_the_table() {
        assert_eq!(annoyance_limit(0x00), 7);
        assert_eq!(annoyance_limit(0x04), 0);
        assert_eq!(annoyance_limit(0x06), 3);
        assert_eq!(annoyance_limit(0x07), 5);
        assert_eq!(annoyance_limit(0x27), 4);
        assert_eq!(annoyance_limit(0x29), 2);
        assert_eq!(annoyance_limit(0x2a), 0);
        assert_eq!(annoyance_limit(0x31), 3);
        assert_eq!(annoyance_limit(0x37), 0);
    }

    #[test]
    fn under_attack_sets_the_timers_and_quiet_counters() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        let near = w.spawn(FOOTMAN, 0, (31, 9), 0).unwrap();
        let far = w.spawn(FOOTMAN, 0, (32, 20), 0).unwrap();
        let foe = w.spawn(FOOTMAN, 1, (21, 20), 0).unwrap();
        let i = w.index(f).unwrap();
        w.damage(i, 5, foe);
        let help: Vec<_> = w.sounds.cues.drain(..).map(|c| (c.sound, c.place)).collect();
        assert_eq!(help, [(Sound::Help { building: false }, None)]);
        assert_eq!(w.sounds.help_timer, HELP_TIMER);
        assert_eq!(w.sounds.last_event, Some((20, 20)));
        assert_eq!(w.sounds.events, [(20, 20)]);
        for id in [f, near, foe] {
            assert_eq!(w.sounds.quiet(id), QUIET);
        }
        assert_eq!(w.sounds.quiet(far), 0, "outside the 23 x 23 square");
        // 100 ticks later the help timer is 0 but the quiet counters are about 150.
        w.sounds.cues.clear();
        for _ in 0..100 {
            w.sound_tick();
            w.tick += 1;
        }
        assert_eq!(w.sounds.help_timer, 0);
        assert_eq!(w.sounds.quiet(near), 150);
        let j = w.index(near).unwrap();
        w.damage(j, 5, foe);
        assert!(w.sounds.cues.is_empty());
    }

    #[test]
    fn no_under_attack_sound_for_a_kill_or_someone_else() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        let e = w.spawn(FOOTMAN, 1, (21, 20), 0).unwrap();
        let i = w.index(e).unwrap();
        w.damage(i, 5, f);
        assert!(w.sounds.cues.is_empty(), "not the local player's");
        let i = w.index(f).unwrap();
        w.damage(i, 200, e);
        let sounds: Vec<_> = w.sounds.cues.iter().map(|c| c.sound).collect();
        assert_eq!(
            sounds,
            [Sound::Death {
                kind: FOOTMAN,
                flags: w.stats_of(FOOTMAN).flags
            }]
        );
    }

    #[test]
    fn unholy_armour_blocks_the_help_sound() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        let e = w.spawn(FOOTMAN, 1, (21, 20), 0).unwrap();
        let i = w.index(f).unwrap();
        w.units[i].unholy_armor = 10;
        w.damage(i, 5, e);
        assert!(w.sounds.cues.is_empty());
    }

    #[test]
    fn building_deaths_and_walls() {
        let mut w = lab();
        let b = w.spawn(BARRACKS, 1, (10, 10), 0).unwrap();
        let i = w.index(b).unwrap();
        w.kill(i);
        assert_eq!(w.sounds.cues.pop().map(|c| c.sound), Some(Sound::BuildingDeath));
        let mut st = w.stats.clone();
        st.0[0x67] = st.0[FARM as usize];
        w.stats = st;
        let wall = w.spawn(0x67, 1, (20, 10), 0).unwrap();
        let i = w.index(wall).unwrap();
        w.kill(i);
        assert!(w.sounds.cues.is_empty(), "a wall piece makes no sound");
    }

    #[test]
    fn a_passenger_killed_with_its_transport_is_inside() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        let i = w.index(f).unwrap();
        w.units[i].inside = true;
        w.kill(i);
        assert!(matches!(w.sounds.cues[0].place, Some(Place::Unit { inside: true, .. })));
    }

    #[test]
    fn nothing_while_the_map_loads() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        w.loading = true;
        let i = w.index(f).unwrap();
        w.kill(i);
        assert!(w.sounds.cues.is_empty());
    }

    #[test]
    fn construction_sound_repeats_at_most_once_per_150_ticks() {
        let mut w = lab();
        let a = w.place_building(FARM, 0, (2, 2)).unwrap();
        let b = w.place_building(FARM, 0, (6, 2)).unwrap();
        let mut at = Vec::new();
        for t in 0..400u32 {
            if t % 12 == 0 {
                for id in [a, b] {
                    let i = w.index(id).unwrap();
                    w.construction_sound(i);
                }
            }
            if w.sounds.cues.drain(..).any(|c| c.sound == Sound::Construction) {
                at.push(t);
            }
            w.sound_tick();
        }
        assert_eq!(at, [0, 156, 312]);
    }

    #[test]
    fn tile_places_read_the_local_seen_bits() {
        let mut w = lab();
        assert_eq!(
            w.tile_place((3, 3)),
            Place::Tile {
                tile: (3, 3),
                unseen: true
            }
        );
        w.reveal(0, (3, 3), 1);
        assert_eq!(
            w.tile_place((3, 3)),
            Place::Tile {
                tile: (3, 3),
                unseen: false
            }
        );
        assert_eq!(
            w.tile_place((-1, 3)),
            Place::Tile {
                tile: (-1, 3),
                unseen: false
            },
            "off the map the test is skipped"
        );
    }

    #[test]
    fn attack_sounds_wait_for_the_animation() {
        let mut w = lab();
        let f = w.spawn(FOOTMAN, 0, (5, 5), 0).unwrap();
        let i = w.index(f).unwrap();
        w.play_pending(i);
        assert!(w.sounds.cues.is_empty(), "nothing pending");
        w.pend_attack(i);
        w.play_pending(i);
        w.play_pending(i);
        let got: Vec<_> = w.sounds.cues.iter().map(|c| c.sound).collect();
        let (kind, unit) = (FOOTMAN, f);
        assert_eq!(
            got,
            [Sound::AttackPending { kind, unit }, Sound::Attack { kind, unit }],
            "the draw's cue when made pending, then played once and cleared"
        );
    }

    #[test]
    fn a_fight_makes_attack_and_death_sounds() {
        let mut w = lab();
        // Scripts in the test data have no play-sound instruction, so only deaths and help come through.
        let a = w.spawn(FOOTMAN, 1, (5, 5), 0).unwrap();
        let b = w.spawn(FOOTMAN, 0, (6, 5), 0).unwrap();
        w.step(&[Command::Attack {
            player: 1,
            units: vec![a],
            target: b,
        }]);
        run(&mut w, 600);
        let sounds: Vec<_> = w.sounds.cues.iter().map(|c| c.sound).collect();
        assert!(sounds.contains(&Sound::Help { building: false }));
        assert!(sounds.iter().any(|s| matches!(s, Sound::Death { .. })));
    }
}
