//! Fog of war, sight and detection. Spec: `docs/specs/vision/fog.md`; phase numbers are those of
//! `docs/specs/core/timing-and-rng.md`.
//!
//! [`Vision`] holds the local player's explored and visible maps (quarter-tile flags, display only) and the per-player
//! "seen" bits (game state, so they are part of [`World::state_hash`]). The `World` methods below are the phases:
//! detection (phase 1), the fog refresh (phase 3), a unit applying its sight (phase 4, and on each movement step),
//! and unit visibility (phase 5).
//!
//! Placeholders (the specs they wait for are not approved yet): shared vision sets
//! ([`Player::shared_vision`](crate::Player)), the invisibility timer on [`Unit`], and the minimap building marker
//! (see [`World::on_minimap`]). The minimap's attack marks and remembered buildings are in [`crate::minimap`]. Player kinds come from the player's `OWNR` controller.

use std::sync::OnceLock;

use crate::ai::controller;
use crate::world::{Order, RELEASED, Unit, UnitId, World};
use tidewater_formats::udta::flags;

/// Quarter flags of one tile. A set flag means that quarter is fogged.
pub mod quarter {
    pub const TL: u8 = 1;
    pub const TR: u8 = 2;
    pub const BL: u8 = 4;
    pub const BR: u8 = 8;
    pub const ALL: u8 = 15;
}
use quarter::{ALL, BL, BR, TL, TR};

/// Players the "seen" bits and the detection sets cover.
pub const SEEN_PLAYERS: u8 = 8;

/// The bit of player `p` in a player set (0 for players outside 0 to 7).
pub fn bit(p: u8) -> u8 {
    if p < SEEN_PLAYERS { 1 << p } else { 0 }
}

/// The fog overlay code (0 to 16) of a set of fogged quarters ("The fog maps", table of 17 codes).
pub fn code(fogged: u8) -> u8 {
    match fogged & ALL {
        0 => 0,
        f if f == TL | TR | BL => 1,
        f if f == TL | TR => 2,
        f if f == TL | TR | BR => 3,
        f if f == TL | BL => 4,
        f if f == TR | BR => 6,
        f if f == TL | BL | BR => 7,
        f if f == BL | BR => 8,
        f if f == TR | BL | BR => 9,
        TL => 10,
        TR => 11,
        BL => 12,
        BR => 13,
        f if f == TL | BR => 14,
        f if f == TR | BL => 15,
        _ => 16,
    }
}

/// The two display maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Map {
    Explored,
    Visible,
}

/// Fog state. Display maps are for the local player only; `seen` is per player.
#[derive(Clone, Debug)]
pub struct Vision {
    pub width: usize,
    pub height: usize,
    /// The player whose display the maps drive.
    pub local: u8,
    explored: Vec<u8>,
    visible: Vec<u8>,
    /// The visible map before the latest refresh, kept for redrawing (phase 3).
    pub previous_visible: Vec<u8>,
    /// Per tile, bit p set: player p has not seen the tile in the current refresh period.
    pub(crate) seen: Vec<u8>,
    /// Bumped whenever the explored or visible map changes, so a renderer can skip unchanged ticks.
    pub generation: u32,
    started: bool,
    /// Cycles until the next fog refresh (timing spec, phase 3): it starts at 1, so the first refresh is in the first
    /// cycle, and holy vision resets it.
    pub(crate) countdown: u32,
}

impl Vision {
    pub fn new(width: usize, height: usize) -> Self {
        let n = width * height;
        Vision {
            width,
            height,
            local: 0,
            explored: vec![ALL; n],
            visible: vec![ALL; n],
            previous_visible: vec![ALL; n],
            seen: vec![0xff; n],
            generation: 0,
            started: false,
            countdown: 1,
        }
    }

    fn at(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| y as usize * self.width + x as usize)
    }

    /// The fogged quarters of a tile (all fogged outside the map).
    pub fn fogged(&self, map: Map, x: i32, y: i32) -> u8 {
        let Some(n) = self.at(x, y) else { return ALL };
        match map {
            Map::Explored => self.explored[n],
            Map::Visible => self.visible[n],
        }
    }

    /// The overlay code (0 to 16) of a tile.
    pub fn code(&self, map: Map, x: i32, y: i32) -> u8 {
        code(self.fogged(map, x, y))
    }

    /// The whole map as fogged-quarter flags, row by row.
    pub fn flags(&self, map: Map) -> &[u8] {
        match map {
            Map::Explored => &self.explored,
            Map::Visible => &self.visible,
        }
    }

    /// Whether no quarter of the tile has ever been revealed (drawn black).
    pub fn unexplored(&self, x: i32, y: i32) -> bool {
        self.fogged(Map::Explored, x, y) == ALL
    }

    /// Tiles with at least one explored quarter.
    pub fn explored_tiles(&self) -> usize {
        self.explored.iter().filter(|&&f| f != ALL).count()
    }

    /// Whether player `p` has not seen the tile this refresh period.
    pub fn unseen_by(&self, p: u8, x: i32, y: i32) -> bool {
        self.at(x, y).is_none_or(|n| self.seen[n] & bit(p) != 0)
    }
}

/// The quarters inside a sight area, per tile offset: `(dx, dy, clear mask)`, for sight values 0 to 9.
fn shape(r: u8) -> &'static [(i8, i8, u8)] {
    static SHAPES: OnceLock<Vec<Vec<(i8, i8, u8)>>> = OnceLock::new();
    let all = SHAPES.get_or_init(|| (0..=9).map(build_shape).collect());
    &all[r.min(9) as usize]
}

fn build_shape(r: u8) -> Vec<(i8, i8, u8)> {
    // Work in half tiles from the centre of the centre tile. Tile offset d covers [2d - 1, 2d + 1]; its near quarter
    // (toward the centre) and far quarter each span one half tile. K is the far edge's distance rounded up to tiles.
    let k = |lo: i32, hi: i32| (lo.abs().max(hi.abs()) + 1) / 2;
    let r = r as i32;
    let inside = |kx: i32, ky: i32| match r {
        0 | 1 => kx <= 1 && ky <= 1,
        2..=4 => kx <= r && ky <= r && kx + ky < 2 * r,
        _ => kx <= r && ky <= r && kx + ky <= 2 * r - 2,
    };
    let mut out = Vec::new();
    for dy in -10..=10i32 {
        for dx in -10..=10i32 {
            let mut clear = 0;
            for (flag, qx, qy) in [(TL, 0, 0), (TR, 1, 0), (BL, 0, 1), (BR, 1, 1)] {
                let (x0, y0) = (2 * dx - 1 + qx, 2 * dy - 1 + qy);
                if inside(k(x0, x0 + 1), k(y0, y0 + 1)) {
                    clear |= flag;
                }
            }
            if clear != 0 {
                out.push((dx as i8, dy as i8, clear));
            }
        }
    }
    out
}

/// The player kinds the vision rules tell apart ("Terms"), derived from the `OWNR` controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Nobody,
    Computer,
    Human,
    Passive,
}

/// Eight facing directions, 0 north, clockwise (same as the path module's).
const STEP: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];

/// Range of submarine detection on each axis, in tiles.
const DETECT_RANGE: i32 = 6;

impl World {
    /// Vision spec, "Terms": the players that see what `p` sees. In Classic mode only `p`; in Enhanced mode `p` and the
    /// players it shares vision with.
    pub fn shared_set(&self, p: u8) -> u8 {
        if p >= SEEN_PLAYERS {
            return 0;
        }
        if self.rules.enhanced {
            self.players[p as usize].shared_vision | bit(p)
        } else {
            bit(p)
        }
    }

    fn kind_of(&self, p: u8) -> Kind {
        if self.human_controlled(p) {
            Kind::Human
        } else if self.computer_player(p) {
            Kind::Computer
        } else if self
            .players
            .get(p as usize)
            .is_some_and(|q| matches!(q.controller, controller::PASSIVE | controller::RESCUE_PASSIVE))
        {
            Kind::Passive
        } else {
            Kind::Nobody
        }
    }

    fn local_in(&self, set: u8) -> bool {
        set & bit(self.vision.local) != 0
    }

    /// The fog refresh period in cycles: 100 in Enhanced mode, 101 in Classic.
    pub fn refresh_period(&self) -> u32 {
        if self.rules.enhanced { 100 } else { 101 }
    }

    /// Reveals a sight area for player `p`: clears the "seen" bits of its shared-vision set on every touched tile, and
    /// the same quarters of the explored and visible maps when the local player is in that set. Parts outside the map
    /// are ignored.
    pub fn reveal(&mut self, p: u8, centre: (i32, i32), sight: u8) {
        let set = self.shared_set(p);
        let display = self.local_in(set);
        let v = &mut self.vision;
        for &(dx, dy, clear) in shape(sight) {
            let Some(n) = v.at(centre.0 + dx as i32, centre.1 + dy as i32) else {
                continue;
            };
            v.seen[n] &= !set;
            if display {
                let (e, vis) = (v.explored[n], v.visible[n]);
                v.explored[n] &= !clear;
                v.visible[n] &= !clear;
                if (e, vis) != (v.explored[n], v.visible[n]) {
                    v.generation = v.generation.wrapping_add(1);
                }
            }
        }
    }

    fn is_dead(u: &Unit) -> bool {
        u.dying || u.removing || u.script.anim == RELEASED
    }

    /// The centre tile and sight value of unit `i`, if it reveals ("Sight of a unit", "Who reveals, and when").
    fn sight_of(&self, i: usize) -> Option<((i32, i32), u8)> {
        let u = &self.units[i];
        if Self::is_dead(u) || u.owner >= SEEN_PLAYERS || self.kind_of(u.owner) != Kind::Human {
            return None;
        }
        if u.inside {
            // Only a worker inside a gold mine sees, with the mine's sight from the mine's centre.
            let Order::Inside { building, .. } = u.order else {
                return None;
            };
            let mine = self.unit(building)?;
            let s = self.stats_of(u.kind);
            let ms = self.stats_of(mine.kind);
            return (s.flags & flags::PEON != 0 && ms.flags & flags::MINE != 0)
                .then(|| (centre_tile(mine), ms.sight.min(9) as u8));
        }
        let s = self.stats_of(u.kind);
        let up = &self.players[u.owner as usize].upgrades;
        let sight = if u.building && u.construction.is_some() {
            3
        } else if (u.kind == crate::combat::kind::RANGER && up.ranger_scouting)
            || (u.kind == crate::combat::kind::BERSERKER && up.berserker_scouting)
        {
            9
        } else {
            s.sight.min(9) as u8
        };
        Some((centre_tile(u), sight))
    }

    /// A unit applies its sight (phase 4 of a refresh cycle, and each movement step).
    pub(crate) fn apply_unit_sight(&mut self, i: usize) {
        if let Some((c, r)) = self.sight_of(i) {
            let owner = self.units[i].owner;
            self.reveal(owner, c, r);
        }
    }

    /// A worker goes inside a mine, depot or construction site: it reveals a sight-3 area at the building's top-left
    /// tile moved one step the way the worker faces ("When sight is applied", rule 3). Harvest's `go_in` calls it.
    pub fn worker_entry_sight(&mut self, worker: UnitId, building: UnitId) {
        let (Some(w), Some(b)) = (self.unit(worker), self.unit(building)) else {
            return;
        };
        if w.owner >= SEEN_PLAYERS || self.kind_of(w.owner) != Kind::Human {
            return;
        }
        let step = STEP[(w.facing & 7) as usize];
        let c = (b.tile.0 + step.0, b.tile.1 + step.1);
        let owner = w.owner;
        self.reveal(owner, c, 3);
    }

    /// Phase 3. Returns whether this is a fog refresh cycle (the first cycle, then every 100, or 101 in Classic). On a
    /// refresh with the option on, the visible map and every "seen" bit are fogged again.
    pub(crate) fn fog_refresh(&mut self) -> bool {
        self.vision.countdown = self.vision.countdown.saturating_sub(1);
        if self.vision.countdown > 0 {
            return false;
        }
        self.vision.countdown = self.refresh_period();
        let fog = self.rules.fog_of_war;
        let v = &mut self.vision;
        v.previous_visible.clone_from(&v.visible);
        if fog {
            v.visible.fill(ALL);
            v.seen.fill(0xff);
        } else if !v.started {
            // Fog off from the start (the demo): every tile counts as seen.
            v.seen.fill(0);
        }
        v.started = true;
        v.generation = v.generation.wrapping_add(1);
        true
    }

    /// Switches the fog-of-war option during play ("Fog-of-war option and cheat"). Turning it off makes the visible map
    /// equal to the explored map and clears the "seen" bits, except the local player's set on unexplored tiles.
    pub fn set_fog_of_war(&mut self, on: bool) {
        let was = self.rules.fog_of_war;
        self.rules.fog_of_war = on;
        if on || !was {
            return;
        }
        let keep = self.shared_set(self.vision.local);
        let v = &mut self.vision;
        v.visible.clone_from(&v.explored);
        for (s, &e) in v.seen.iter_mut().zip(&v.explored) {
            *s = if e == ALL { *s & keep } else { 0 };
        }
        v.generation = v.generation.wrapping_add(1);
    }

    /// The single-player cheat that turns fog off: the option is cleared, the visible map is fully clear and no tile is
    /// unseen.
    pub fn cheat_clear_fog(&mut self) {
        self.rules.fog_of_war = false;
        let v = &mut self.vision;
        v.visible.fill(0);
        v.seen.fill(0);
        v.generation = v.generation.wrapping_add(1);
    }

    /// Phase 1: each unit's detected-by set ("Detection").
    pub(crate) fn detection(&mut self) {
        let enhanced = self.rules.enhanced;
        // Detectors: living units of players 0 to 7 that can see submarines (Enhanced: only complete ones).
        let detectors: Vec<(u8, (i32, i32))> = self
            .units
            .iter()
            .filter(|u| {
                !Self::is_dead(u)
                    && u.owner < SEEN_PLAYERS
                    && self.stats_of(u.kind).flags & flags::SEES_SUBMARINES != 0
                    && !(enhanced && u.construction.is_some())
            })
            .map(|u| (u.owner, u.tile))
            .collect();
        let sets: Vec<u8> = (0..SEEN_PLAYERS).map(|p| self.shared_set(p)).collect();
        for i in 0..self.units.len() {
            let u = &self.units[i];
            let s = self.stats_of(u.kind);
            let set = if Self::is_dead(u) {
                0xff
            } else if u.invisibility > 0 {
                // Classic: the owner only. Enhanced: the owner's shared-vision set (empty for non-player owners).
                if enhanced {
                    self.shared_set(u.owner)
                } else {
                    bit(u.owner)
                }
            } else if s.flags & flags::SUBMARINE != 0 {
                let mut set = bit(u.owner);
                if enhanced {
                    set |= self.shared_set(u.owner);
                }
                for &(p, t) in &detectors {
                    if (t.0 - u.tile.0).abs() <= DETECT_RANGE && (t.1 - u.tile.1).abs() <= DETECT_RANGE {
                        set |= bit(p);
                    }
                }
                if enhanced {
                    for p in 0..SEEN_PLAYERS {
                        if set & bit(p) != 0 {
                            set |= sets[p as usize];
                        }
                    }
                }
                set
            } else {
                0xff
            };
            self.units[i].detected_by = set;
        }
    }

    /// Phase 5: each unit's hidden-from set ("Unit visibility"). A unit that is dead, removed or inside something is
    /// hidden from everyone; a dying one (its death animation, a body or a ruin) is not, so the map view can draw it.
    pub(crate) fn unit_visibility(&mut self) {
        for i in 0..self.units.len() {
            let u = &self.units[i];
            let hidden = if u.removing || u.script.anim == RELEASED || u.inside {
                0xff
            } else if self.rules.reveal_map {
                0
            } else {
                // The footprint starts at the tile holding the top-left pixel, so a moving unit counts where it is drawn.
                let tl = (u.pos.0.div_euclid(crate::TILE_PX), u.pos.1.div_euclid(crate::TILE_PX));
                let mut hidden = 0;
                for p in 0..SEEN_PLAYERS {
                    if self.footprint_hidden_from(p, tl, u.size) {
                        hidden |= bit(p);
                    }
                }
                hidden
            };
            self.units[i].hidden_from = hidden;
        }
    }

    /// Whether a footprint at `tl` of `size` tiles is hidden from player `p` by the fog ("Unit visibility" for a
    /// unit that is alive and not inside anything). Parts outside the map are ignored.
    pub(crate) fn footprint_hidden_from(&self, p: u8, tl: (i32, i32), size: (u8, u8)) -> bool {
        if self.rules.reveal_map {
            return false;
        }
        match self.kind_of(p) {
            Kind::Computer => false,
            Kind::Nobody => true,
            Kind::Human | Kind::Passive => (0..size.1 as i32)
                .flat_map(|dy| (0..size.0 as i32).map(move |dx| (dx, dy)))
                .all(|(dx, dy)| self.vision.unseen_by(p, tl.0 + dx, tl.1 + dy)),
        }
    }

    /// Whether player `p` can see the unit at all (not invisible or submerged to it).
    pub(crate) fn detected(u: &Unit, p: u8) -> bool {
        p >= SEEN_PLAYERS || u.detected_by & bit(p) != 0
    }

    /// Whether player `p` detects the unit (orders spec: an order aimed at a unit is lost when this is false; fog does
    /// not matter). False for a unit that does not exist.
    pub fn detected_by(&self, p: u8, id: UnitId) -> bool {
        self.unit(id).is_some_and(|u| Self::detected(u, p))
    }

    /// Whether a unit is a legal pick for player `p`'s target search: not hidden from `p` and detected by it.
    pub fn visible_to(&self, p: u8, id: UnitId) -> bool {
        self.unit(id)
            .is_some_and(|u| p >= SEEN_PLAYERS || (u.hidden_from & bit(p) == 0 && Self::detected(u, p)))
    }

    /// Whether the local player can place a building on a site: every tile must be explored (some quarter cleared),
    /// unless the reveal-map cheat is on. Units on tiles the owner does not see are the placement spec's business.
    pub fn site_explored(&self, tile: (i32, i32), size: (u8, u8)) -> bool {
        self.rules.reveal_map
            || (0..size.1 as i32)
                .all(|dy| (0..size.0 as i32).all(|dx| !self.vision.unexplored(tile.0 + dx, tile.1 + dy)))
    }

    /// Whether the minimap draws a unit for the local player: a unit that is not hidden from it and is detected by
    /// it; a building once seen (it has a remembered look).
    /// Minimap spec, "Units and buildings": dying, removed and inside units are skipped.
    pub fn on_minimap(&self, u: &Unit) -> bool {
        let p = self.vision.local;
        if u.dying || u.removing || u.inside || !Self::detected(u, p) {
            return false;
        }
        if !u.building {
            return u.hidden_from & bit(p) == 0;
        }
        self.rules.reveal_map || self.building_seen(u)
    }

    /// The marker that the local player has seen a building: its remembered look is set (map-drawing spec,
    /// "Buildings and remembered buildings").
    pub(crate) fn building_seen(&self, u: &Unit) -> bool {
        u.look.0.is_some()
    }
}

/// Centre tile of a unit: its tile plus half its size, rounded down.
fn centre_tile(u: &Unit) -> (i32, i32) {
    (u.tile.0 + u.size.0 as i32 / 2, u.tile.1 + u.size.1 as i32 / 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Terrain;
    use crate::world::tests::{run, scripts, stats, tileset};
    use tidewater_formats::udta::UnitTable;

    const FOOTMAN: u8 = 0;
    const SUB: u8 = 20;
    const DESTROYER: u8 = 21;
    const TOWER: u8 = 22;
    const HALL: u8 = 0x4a;
    const GUARD_TOWER: u8 = 0x3e;

    fn table() -> UnitTable {
        let mut t = stats();
        t.0[FOOTMAN as usize].sight = 4;
        t.0[SUB as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 10,
            size: (1, 1),
            flags: flags::SEA | flags::SUBMARINE,
            ..Default::default()
        };
        t.0[DESTROYER as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 10,
            size: (1, 1),
            flags: flags::SEA | flags::SEES_SUBMARINES,
            ..Default::default()
        };
        t.0[TOWER as usize] = tidewater_formats::udta::UnitStats {
            hit_points: 100,
            size: (2, 2),
            flags: flags::BUILDING | flags::SEES_SUBMARINES,
            ..Default::default()
        };
        t.0[HALL as usize].size = (4, 4);
        t.0[HALL as usize].sight = 4;
        t.0[GUARD_TOWER as usize].sight = 6;
        t
    }

    /// A 64 x 64 ground map, no units. Player 0 and 2 are human, 1 is a computer.
    fn map() -> World {
        let tiles = vec![0u16; 64 * 64];
        let mut w = World::new(
            Terrain::from_map(&tileset(), &tiles, None, None, 64, 64, false),
            table(),
            scripts(),
            0,
        );
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        w.players[2].controller = controller::HUMAN;
        w
    }

    fn sub(w: &World) -> &Unit {
        w.units.iter().find(|u| u.kind == SUB).unwrap()
    }

    fn farm(w: &World) -> &Unit {
        w.units.iter().find(|u| u.kind == 0x3a).unwrap()
    }

    fn touched(w: &World) -> usize {
        w.vision.seen.iter().filter(|&&b| b & 1 == 0).count()
    }

    fn codes(w: &World, pts: &[(i32, i32)]) -> Vec<u8> {
        pts.iter().map(|&(x, y)| w.vision.code(Map::Visible, x, y)).collect()
    }

    #[test]
    fn code_table() {
        let c = |f: &[u8]| code(f.iter().fold(0, |a, b| a | b));
        assert_eq!(c(&[]), 0);
        assert_eq!(c(&[TL, TR, BL]), 1);
        assert_eq!(c(&[TL, TR]), 2);
        assert_eq!(c(&[TL, TR, BR]), 3);
        assert_eq!(c(&[TL, BL]), 4);
        assert_eq!(c(&[TR, BR]), 6);
        assert_eq!(c(&[TL, BL, BR]), 7);
        assert_eq!(c(&[BL, BR]), 8);
        assert_eq!(c(&[TR, BL, BR]), 9);
        assert_eq!([c(&[TL]), c(&[TR]), c(&[BL]), c(&[BR])], [10, 11, 12, 13]);
        assert_eq!([c(&[TL, BR]), c(&[TR, BL]), c(&[TL, TR, BL, BR])], [14, 15, 16]);
    }

    #[test]
    fn tile_counts_match_the_table() {
        for (r, want) in [
            (0, 9),
            (1, 9),
            (2, 21),
            (3, 45),
            (4, 77),
            (5, 109),
            (6, 157),
            (7, 213),
            (8, 277),
            (9, 349),
        ] {
            assert_eq!(shape(r).len(), want, "sight {r}");
            // The formula for touched tiles agrees with the quarter test.
            let rr = r.max(1) as i32;
            let mut n = 0;
            for dy in -rr..=rr {
                for dx in -rr..=rr {
                    let (a, b) = (dx.abs().max(1), dy.abs().max(1));
                    let cut = if rr <= 4 { 2 * rr - 1 } else { 2 * rr - 2 };
                    n += (a + b <= cut || rr == 1) as usize;
                }
            }
            assert_eq!(n, want, "formula for sight {r}");
        }
    }

    #[test]
    fn footman_sight_4_on_a_fresh_map() {
        let mut w = map();
        w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        w.step(&[]);
        assert_eq!(touched(&w), 77);
        assert_eq!(w.vision.explored_tiles(), 77);
        assert_eq!(codes(&w, &[(20, 20), (24, 20), (24, 24), (23, 23)]), [0, 6, 16, 13]);
        assert_eq!(w.vision.code(Map::Explored, 24, 20), 6);
    }

    #[test]
    fn sight_2_shape() {
        let mut w = map();
        w.reveal(0, (10, 10), 2);
        assert_eq!(touched(&w), 21);
        assert_eq!(
            codes(&w, &[(8, 8), (9, 9), (8, 9), (8, 10), (10, 8)]),
            [16, 10, 1, 4, 2]
        );
    }

    #[test]
    fn sight_0_equals_sight_1() {
        let pts = [(10, 10), (9, 10), (11, 10), (9, 9), (11, 11)];
        let mut a = map();
        a.reveal(0, (10, 10), 0);
        let mut b = map();
        b.reveal(0, (10, 10), 1);
        assert_eq!(codes(&a, &pts), [0, 4, 6, 1, 9]);
        assert_eq!(codes(&a, &pts), codes(&b, &pts));
        assert_eq!(touched(&a), 9);
        assert_eq!(a.vision.visible, b.vision.visible);
    }

    #[test]
    fn sight_9_shape() {
        let mut w = map();
        w.reveal(0, (50, 50), 9);
        assert_eq!(touched(&w), 349);
        assert_eq!(
            codes(&w, &[(59, 50), (59, 56), (59, 57), (58, 58), (59, 58), (59, 59)]),
            [6, 6, 9, 9, 16, 16]
        );
    }

    #[test]
    fn revealing_intersects_fogged_quarters() {
        let mut w = map();
        // A tile at code 4 (left half fogged) meets an area that leaves it at code 2 (top half fogged).
        let n = 8 * 64 + 10;
        w.vision.visible[n] = TL | BL;
        w.vision.explored[n] = TL | BL;
        assert_eq!(w.vision.code(Map::Visible, 10, 8), 4);
        w.reveal(0, (10, 10), 2);
        assert_eq!(w.vision.code(Map::Visible, 10, 8), 10);
        assert_eq!(w.vision.code(Map::Explored, 10, 8), 10);
    }

    #[test]
    fn scout_walks_out_and_the_area_stays_visible_until_the_refresh() {
        for fog in [true, false] {
            let mut w = map();
            w.rules.fog_of_war = fog;
            let id = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
            run(&mut w, 50);
            let i = w.index(id).unwrap();
            w.units[i].tile = (40, 40);
            run(&mut w, 50);
            assert_eq!(w.tick, 100);
            assert_eq!(w.vision.code(Map::Visible, 20, 20), 0, "still visible at cycle 100");
            run(&mut w, 1);
            let want = if fog { 16 } else { 0 };
            assert_eq!(w.vision.code(Map::Visible, 20, 20), want);
            assert_eq!(w.vision.code(Map::Explored, 20, 20), 0);
            run(&mut w, 300);
            assert_eq!(w.vision.code(Map::Visible, 20, 20), want);
        }
    }

    #[test]
    fn classic_refreshes_every_101_cycles() {
        let mut w = map();
        w.rules.enhanced = false;
        let id = w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        run(&mut w, 1);
        let i = w.index(id).unwrap();
        w.units[i].tile = (40, 40);
        run(&mut w, 100);
        assert_eq!(w.vision.code(Map::Visible, 20, 20), 0);
        run(&mut w, 1);
        assert_eq!(w.vision.code(Map::Visible, 20, 20), 16);
    }

    #[test]
    fn building_sight_centres() {
        let mut w = map();
        w.spawn(HALL, 0, (30, 30), 0).unwrap();
        assert_eq!(centre_tile(w.units.iter().find(|u| u.kind == HALL).unwrap()), (32, 32));
        // A guard tower under construction sees 3 at (31, 31), whatever its own sight.
        let t = w.place_building(GUARD_TOWER, 0, (30, 30)).unwrap();
        let i = w.index(t).unwrap();
        assert_eq!(w.sight_of(i), Some(((31, 31), 3)));
        w.units[i].construction = None;
        assert_eq!(w.sight_of(i), Some(((31, 31), 6)));
    }

    #[test]
    fn scouting_gives_sight_9() {
        let mut w = map();
        w.stats.0[crate::combat::kind::RANGER as usize] = w.stats.0[FOOTMAN as usize];
        w.players[0].upgrades.ranger_scouting = true;
        let id = w.spawn(crate::combat::kind::RANGER, 0, (30, 30), 0).unwrap();
        let i = w.index(id).unwrap();
        assert_eq!(w.sight_of(i), Some(((30, 30), 9)));
        w.step(&[]);
        assert_eq!(touched(&w), 349);
        // The other race's scouting does not help.
        w.players[0].upgrades.ranger_scouting = false;
        assert_eq!(w.sight_of(i), Some(((30, 30), 4)));
    }

    #[test]
    fn computer_players_apply_no_sight() {
        let mut w = map();
        w.spawn(FOOTMAN, 1, (20, 20), 0).unwrap();
        let before = w.vision.seen.clone();
        w.step(&[]);
        assert_eq!(w.vision.seen, before);
        assert_eq!(w.vision.explored_tiles(), 0);
    }

    #[test]
    fn edges_are_clipped() {
        let mut w = map();
        w.reveal(0, (1, 1), 4);
        assert!(touched(&w) > 0 && touched(&w) < 77);
        assert_eq!(w.vision.code(Map::Visible, 0, 0), 0);
        w.reveal(0, (63, 63), 9);
    }

    #[test]
    fn moving_reveals_at_once() {
        let mut w = map();
        let id = w.spawn(FOOTMAN, 0, (10, 10), 0).unwrap();
        w.step(&[crate::Command::Move {
            player: 0,
            units: vec![id],
            to: (30, 10),
        }]);
        let before = touched(&w);
        run(&mut w, 40);
        assert!(w.unit(id).unwrap().tile.0 > 11);
        assert!(touched(&w) > before);
        assert!(w.vision.explored_tiles() > before);
    }

    #[test]
    fn submarine_detection_range() {
        for (at, seen) in [((46, 34), true), ((47, 40), false), ((46, 46), true), ((34, 33), false)] {
            let mut w = map();
            w.rules.enhanced = false;
            w.spawn(SUB, 1, (40, 40), 0).unwrap();
            w.spawn(DESTROYER, 0, at, 0).unwrap();
            w.step(&[]);
            assert_eq!(sub(&w).detected_by & 1 != 0, seen, "destroyer at {at:?}");
            assert_ne!(sub(&w).detected_by & 2, 0, "owner always detects");
            assert!(w.visible_to(1, sub(&w).id));
        }
    }

    #[test]
    fn unfinished_towers_do_not_detect_in_enhanced_mode() {
        let mut w = map();
        w.spawn(SUB, 1, (40, 40), 0).unwrap();
        let t = w.place_building(TOWER, 0, (42, 42)).unwrap();
        w.step(&[]);
        assert_eq!(sub(&w).detected_by & 1, 0);
        let i = w.index(t).unwrap();
        w.units[i].construction = None;
        w.step(&[]);
        assert_eq!(sub(&w).detected_by & 1, 1);
        // Classic mode ignores completeness.
        let mut w = map();
        w.rules.enhanced = false;
        w.spawn(SUB, 1, (40, 40), 0).unwrap();
        w.place_building(TOWER, 0, (42, 42)).unwrap();
        w.step(&[]);
        assert_eq!(sub(&w).detected_by & 1, 1);
    }

    #[test]
    fn shared_vision_widens_detection_in_enhanced_mode() {
        let mut w = map();
        w.players[1].shared_vision = 1 << 3;
        w.spawn(SUB, 1, (40, 40), 0).unwrap();
        w.step(&[]);
        assert_eq!(sub(&w).detected_by & (1 << 3), 1 << 3);
        w.rules.enhanced = false;
        w.step(&[]);
        assert_eq!(sub(&w).detected_by & (1 << 3), 0);
    }

    #[test]
    fn invisible_units() {
        let mut w = map();
        w.rules.enhanced = false;
        let a = w.spawn(FOOTMAN, 2, (5, 5), 0).unwrap();
        let i = w.index(a).unwrap();
        w.units[i].invisibility = 50;
        w.step(&[]);
        assert_eq!(w.units[i].detected_by, 1 << 2);
        assert!(!w.visible_to(1, a) && !w.visible_to(0, a) && w.visible_to(2, a));
        // Enhanced: the owner's shared set; nobody when the owner is not a player.
        w.rules.enhanced = true;
        w.players[2].shared_vision = 1;
        w.step(&[]);
        assert_eq!(w.units[i].detected_by, 0b101);
        w.change_owner(i, 15);
        w.step(&[]);
        assert_eq!(w.units[i].detected_by, 0);
    }

    #[test]
    fn fog_hides_units_from_humans_but_not_computers() {
        let mut w = map();
        // Player 2's grunt stands where player 0 has seen nothing.
        let g = w.spawn(FOOTMAN, 2, (50, 50), 0).unwrap();
        w.step(&[]);
        let h = w.unit(g).unwrap().hidden_from;
        assert_ne!(h & 1, 0, "hidden from the human player");
        assert_eq!(h & 2, 0, "not hidden from the computer player");
        assert_ne!(h & (1 << 4), 0, "hidden from an empty slot");
        assert_eq!(h & (1 << 2), 0, "its owner sees it");
        w.rules.reveal_map = true;
        w.step(&[]);
        assert_eq!(w.unit(g).unwrap().hidden_from, 0);
    }

    #[test]
    fn dying_units_follow_the_fog_but_removed_ones_are_hidden_from_everyone() {
        let mut w = map();
        let g = w.spawn(FOOTMAN, 2, (10, 10), 0).unwrap();
        w.reveal(0, (10, 10), 1);
        let i = w.index(g).unwrap();
        w.units[i].dying = true;
        w.unit_visibility();
        assert_eq!(w.units[i].hidden_from & 1, 0, "a dying unit in sight is not hidden");
        assert_ne!(
            w.units[i].hidden_from & (1 << 4),
            0,
            "but still hidden from an empty slot"
        );
        w.units[i].removing = true;
        w.unit_visibility();
        assert_eq!(w.units[i].hidden_from, 0xff, "removed: hidden from everyone");
    }

    #[test]
    fn a_building_with_one_seen_tile_is_not_hidden() {
        let mut w = map();
        w.spawn(0x3a, 2, (30, 30), 0).unwrap();
        w.step(&[]);
        assert_ne!(farm(&w).hidden_from & 1, 0);
        w.reveal(0, (29, 29), 0);
        w.unit_visibility();
        assert_eq!(farm(&w).hidden_from & 1, 0);
        // Only the footprint's one tile is seen, and it is the top-left one.
        assert!(!w.vision.unseen_by(0, 30, 30) && w.vision.unseen_by(0, 31, 31));
    }

    #[test]
    fn units_hidden_from_their_owner_cannot_be_targets() {
        let mut w = map();
        let foe = w.spawn(FOOTMAN, 1, (30, 30), 0).unwrap();
        w.step(&[]);
        // Player 1 is a computer: it sees everything. Player 0 does not.
        assert!(w.visible_to(1, foe));
        assert!(!w.visible_to(0, foe));
        w.reveal(0, (30, 30), 1);
        w.unit_visibility();
        assert!(w.visible_to(0, foe));
    }

    #[test]
    fn a_hidden_target_is_lost_when_it_is_no_longer_detected() {
        let mut w = map();
        w.rules.enhanced = false;
        let me = w.spawn(FOOTMAN, 0, (30, 30), 0).unwrap();
        let foe = w.spawn(SUB, 1, (31, 30), 0).unwrap();
        let i = w.index(me).unwrap();
        w.step(&[]);
        w.units[i].order = crate::Order::Attack { target: foe };
        assert!(!w.detected_by(0, foe));
        run(&mut w, 30);
        assert_eq!(w.units[i].order, crate::Order::Idle);
    }

    #[test]
    fn placing_needs_explored_tiles() {
        let mut w = map();
        w.reveal(0, (20, 20), 2);
        assert!(w.site_explored((20, 20), (2, 2)));
        assert!(!w.site_explored((10, 10), (2, 2)));
        assert!(
            !w.site_explored((17, 20), (2, 2)),
            "one unexplored tile refuses the site"
        );
        w.rules.reveal_map = true;
        assert!(w.site_explored((10, 10), (2, 2)));
    }

    #[test]
    fn minimap_units() {
        let mut w = map();
        let u = w.spawn(FOOTMAN, 2, (50, 50), 0).unwrap();
        let b = w.spawn(0x3a, 2, (40, 40), 0).unwrap();
        w.step(&[]);
        assert!(!w.on_minimap(w.unit(u).unwrap()) && !w.on_minimap(w.unit(b).unwrap()));
        w.reveal(0, (50, 50), 1);
        w.reveal(0, (41, 41), 1);
        w.unit_visibility();
        w.remember_looks();
        assert!(w.on_minimap(w.unit(u).unwrap()) && w.on_minimap(w.unit(b).unwrap()));
        // The building stays once seen; the unit goes with the next refresh.
        run(&mut w, 99);
        w.step(&[]);
        assert!(!w.on_minimap(w.unit(u).unwrap()) && w.on_minimap(w.unit(b).unwrap()));
    }

    #[test]
    fn fog_option_and_cheat() {
        let mut w = map();
        w.spawn(FOOTMAN, 0, (20, 20), 0).unwrap();
        w.step(&[]);
        w.set_fog_of_war(false);
        assert_eq!(w.vision.visible, w.vision.explored);
        // Seen bits: cleared everywhere except the local player's bit on unexplored tiles.
        assert_eq!(w.vision.seen[0], 1);
        assert_eq!(w.vision.seen[20 * 64 + 20], 0);
        let mut c = map();
        c.cheat_clear_fog();
        assert!(c.vision.visible.iter().all(|&f| f == 0) && c.vision.seen.iter().all(|&s| s == 0));
        assert!(!c.rules.fog_of_war);
    }

    #[test]
    fn worker_entry_reveals_sight_3_one_step_ahead() {
        let mut w = map();
        let b = w.spawn(HALL, 0, (30, 30), 0).unwrap();
        let p = w.spawn(FOOTMAN, 0, (25, 30), 0).unwrap();
        let i = w.index(p).unwrap();
        w.units[i].facing = 2; // east
        w.worker_entry_sight(p, b);
        assert_eq!(touched(&w), 45);
        assert_eq!(w.vision.code(Map::Visible, 31, 30), 0);
        assert_eq!(w.vision.code(Map::Visible, 30, 30), 0);
    }

    #[test]
    fn workers_in_a_mine_see_from_the_mine() {
        let mut w = map();
        w.stats.0[0x5c] = tidewater_formats::udta::UnitStats {
            hit_points: 100,
            size: (3, 3),
            sight: 2,
            flags: flags::BUILDING | flags::MINE,
            ..Default::default()
        };
        w.stats.0[FOOTMAN as usize].flags |= flags::PEON;
        let m = w.spawn(0x5c, 15, (20, 20), 0).unwrap();
        let p = w.spawn(FOOTMAN, 0, (20, 19), 0).unwrap();
        let i = w.index(p).unwrap();
        w.units[i].inside = true;
        w.units[i].order = Order::Inside {
            building: m,
            hall_sized: false,
        };
        assert_eq!(w.sight_of(i), Some(((21, 21), 2)));
        w.step(&[]);
        assert_eq!(touched(&w), 21);
        assert_ne!(w.unit(p).unwrap().hidden_from, 0, "inside, so hidden from everyone");
        // A non-worker inside something does not see.
        w.stats.0[FOOTMAN as usize].flags &= !flags::PEON;
        assert_eq!(w.sight_of(i), None);
    }

    #[test]
    fn seen_bits_are_part_of_the_hash_but_the_display_maps_are_not() {
        let mut a = map();
        let mut b = map();
        assert_eq!(a.state_hash(), b.state_hash());
        a.vision.explored[5] = 0;
        a.vision.visible[5] = 0;
        assert_eq!(a.state_hash(), b.state_hash());
        b.vision.seen[5] = 0;
        assert_ne!(a.state_hash(), b.state_hash());
    }
}
