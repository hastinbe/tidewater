//! The site check a click in placement mode makes before a build command is sent. Spec:
//! `docs/specs/ui/placing-buildings.md` ("The site", "The site check"), with the masks of
//! `docs/specs/movement/occupancy.md` ("Building placement masks") and the fog rules of `docs/specs/vision/fog.md`
//! ("Placing a building").
//!
//! [`World::check_site`] is a pure query: it changes nothing and makes no generator draws. It filters the player's
//! click; the build command is checked again when it is carried out ([`World::check_site_in_turn`], without the
//! unexplored test), and the worker checks the site again when it arrives (`build.rs`, `economy/build-train.md`); that
//! check plays no error sound.
//!
//! Our choice: the builder's race, which picks the foundry that gets the short oil distance, is the race of its unit
//! type (even types human, odd types orc), as in the worker AI.

use tidewater_formats::udta::flags;

use crate::terrain::{bits, mask};
use crate::world::{UnitId, World};

/// Gold stores keep this many tiles from a gold mine.
pub const MINE_DISTANCE: i32 = 3;
/// Shipyards and refineries keep this many tiles from oil.
pub const OIL_DISTANCE: i32 = 4;
/// The builder's own race's foundry keeps this many tiles from oil, in Enhanced mode only.
pub const FOUNDRY_OIL_DISTANCE: i32 = 1;
/// The human and orc foundries.
const FOUNDRIES: [u8; 2] = [0x4e, 0x4f];
/// The unit data's movement class for sea units.
const SEA_CLASS: u8 = 2;
/// The marks units and buildings leave, which fog hides from a human player's builder.
const MARKS: u16 = bits::UNIT | bits::AIR_UNIT | bits::BUILDING;

/// What the site check says, highest priority first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SiteCheck {
    Allowed,
    OffMap,
    NoShore,
    Unexplored,
    TooCloseToMine,
    TooCloseToOil,
    NoOilPatch,
    Blocked,
}

impl SiteCheck {
    /// The `stat_txt.tbl` entry (counting from 1) shown for a failure; `None` when the site is allowed.
    pub fn message(self) -> Option<u16> {
        Some(match self {
            SiteCheck::Allowed => return None,
            SiteCheck::OffMap => 442,
            SiteCheck::Blocked => 443,
            SiteCheck::NoShore => 444,
            SiteCheck::Unexplored => 445,
            SiteCheck::NoOilPatch => 446,
            SiteCheck::TooCloseToMine => 447,
            SiteCheck::TooCloseToOil => 448,
        })
    }
}

/// One coordinate of a sea-layer building's site: (c with bit 0 set) − 2, at least 1.
pub fn sea_coordinate(c: i32) -> i32 {
    ((c | 1) - 2).max(1)
}

/// Whether something with footprint `x` (top-left, size) grown by `d` tiles on every side overlaps the footprint
/// `site`: on each axis `X.start − d < site.end` and `site.start − d < X.end`.
pub fn too_close(x: ((i32, i32), (i32, i32)), site: ((i32, i32), (i32, i32)), d: i32) -> bool {
    let axis = |xs: i32, xl: i32, ss: i32, sl: i32| xs - d < ss + sl && ss - d < xs + xl;
    axis(x.0.0, x.1.0, site.0.0, site.1.0) && axis(x.0.1, x.1.1, site.0.1, site.1.1)
}

/// The placement mask of a building with unit flags `f` (occupancy spec, "Building placement masks").
pub(crate) fn building_mask(f: u32) -> u16 {
    if f & flags::OIL_SOURCE != 0 {
        mask::OIL_PLATFORM
    } else if f & flags::SHORE_BUILDING != 0 {
        mask::SHORE_BUILDING
    } else {
        mask::LAND_BUILDING
    }
}

/// The building's own priority-4 check: the flags X must have and the distance, or `None` for no check.
enum Spacing {
    Near { what: u32, d: i32, fail: SiteCheck },
    OnPatch,
    None,
}

impl World {
    /// The site's top-left tile for a building of type `kind` with the pointer on `tile` ("The site"): sea-layer
    /// buildings move onto odd coordinates.
    pub fn site_at(&self, kind: u8, tile: (i32, i32)) -> (i32, i32) {
        if self.stats_of(kind).movement_class == SEA_CLASS {
            (sea_coordinate(tile.0), sea_coordinate(tile.1))
        } else {
            tile
        }
    }

    /// The site check for building `kind` with its top-left tile at `site`, built by `builder` ("The site check").
    /// Changes nothing. A builder that doesn't exist counts as nobody's: no fog rules, and nothing of its own ignored.
    pub fn check_site(&self, kind: u8, site: (i32, i32), builder: UnitId) -> SiteCheck {
        self.site_check(kind, site, builder, true)
    }

    /// The site check made when a player's build command is carried out in the command turn (build-train spec,
    /// "Carrying out a player's build command", round-60 errata): the same check **without** the unexplored test,
    /// which is never made while commands are carried out (`human01-basics-9500` n = 19).
    pub fn check_site_in_turn(&self, kind: u8, site: (i32, i32), builder: UnitId) -> SiteCheck {
        self.site_check(kind, site, builder, false)
    }

    fn site_check(&self, kind: u8, site: (i32, i32), builder: UnitId, unexplored_test: bool) -> SiteCheck {
        let s = self.stats_of(kind);
        let size = (s.size.0 as i32, s.size.1 as i32);
        let tiles = || (0..size.1).flat_map(move |dy| (0..size.0).map(move |dx| (site.0 + dx, site.1 + dy)));
        // 1. Off the map: reported at once.
        if site.0 < 0
            || site.1 < 0
            || !self.terrain.contains(site.0 + size.0 - 1, site.1 + size.1 - 1)
            || !self.terrain.contains(site.0, site.1)
        {
            return SiteCheck::OffMap;
        }
        // 2. No shore.
        if s.flags & flags::SHORE_BUILDING != 0 && !tiles().any(|(x, y)| self.terrain.word(x, y) & bits::SHORE != 0) {
            return SiteCheck::NoShore;
        }
        let b = self.unit(builder);
        let owner = b.map(|u| u.owner);
        // 3. Unexplored.
        if unexplored_test
            && owner == Some(self.vision.local)
            && !self.site_explored(site, (size.0 as u8, size.1 as u8))
        {
            return SiteCheck::Unexplored;
        }
        // 4. The building's spacing rule.
        let spacing = if s.flags & flags::GOLD_STORAGE != 0 {
            Spacing::Near {
                what: flags::MINE,
                d: MINE_DISTANCE,
                fail: SiteCheck::TooCloseToMine,
            }
        } else if s.flags & flags::SHORE_BUILDING != 0 {
            let own_foundry = b.is_some_and(|u| FOUNDRIES[(u.kind & 1) as usize] == kind);
            let d = match (own_foundry, self.rules.enhanced) {
                (false, _) => Some(OIL_DISTANCE),
                (true, true) => Some(FOUNDRY_OIL_DISTANCE),
                (true, false) => None,
            };
            match d {
                Some(d) => Spacing::Near {
                    what: flags::OIL_SOURCE | flags::OIL_PATCH,
                    d,
                    fail: SiteCheck::TooCloseToOil,
                },
                None => Spacing::None,
            }
        } else if s.flags & flags::OIL_SOURCE != 0 {
            Spacing::OnPatch
        } else {
            Spacing::None
        };
        match spacing {
            Spacing::Near { what, d, fail } => {
                let near = self.units.iter().any(|x| {
                    x.present()
                        && self.stats_of(x.kind).flags & what != 0
                        && too_close((x.tile, (x.size.0 as i32, x.size.1 as i32)), (site, size), d)
                });
                if near {
                    return fail;
                }
            }
            Spacing::OnPatch => {
                let patch = self
                    .units
                    .iter()
                    .any(|x| x.present() && self.stats_of(x.kind).flags & flags::OIL_PATCH != 0 && x.tile == site);
                if !patch {
                    return SiteCheck::NoOilPatch;
                }
            }
            Spacing::None => {}
        }
        // 5. Blocked.
        if tiles().any(|t| self.tile_blocked(kind, t, builder)) {
            SiteCheck::Blocked
        } else {
            SiteCheck::Allowed
        }
    }

    /// Whether one site tile carries a bit of building `kind`'s placement mask ("Blocked"): the builder's own ground
    /// mark and, for a human player, marks on tiles it doesn't see are ignored.
    fn tile_blocked(&self, kind: u8, (x, y): (i32, i32), builder: UnitId) -> bool {
        let m = building_mask(self.stats_of(kind).flags);
        let b = self.unit(builder);
        let owner = b.map(|u| u.owner);
        let human = owner.is_some_and(|p| self.human_controlled(p));
        let own_tile = b
            .filter(|u| self.stats_of(u.kind).flags & (flags::PEON | flags::TANKER) != 0)
            .map(|u| u.tile);
        let mut w = self.terrain.word(x, y);
        if human && owner.is_some_and(|p| self.vision.unseen_by(p, x, y)) {
            w &= !MARKS;
        }
        if own_tile == Some((x, y)) {
            w &= !bits::UNIT;
        }
        w & m != 0
    }

    /// The placement image's bad tiles, row by row over the footprint (map-drawing spec, "Placement image"): every
    /// tile when the outcome is off the map, no shore, too close to a mine or oil, or no oil patch; otherwise each
    /// tile that is unexplored (as in the "Unexplored" check) or blocked.
    pub fn bad_site_tiles(&self, kind: u8, site: (i32, i32), builder: UnitId) -> Vec<bool> {
        let s = self.stats_of(kind);
        let size = (s.size.0 as i32, s.size.1 as i32);
        let n = (size.0 * size.1).max(0) as usize;
        let all_bad = matches!(
            self.check_site(kind, site, builder),
            SiteCheck::OffMap
                | SiteCheck::NoShore
                | SiteCheck::TooCloseToMine
                | SiteCheck::TooCloseToOil
                | SiteCheck::NoOilPatch
        );
        if all_bad {
            return vec![true; n];
        }
        let fogged = self.unit(builder).is_some_and(|u| u.owner == self.vision.local) && !self.rules.reveal_map;
        (0..size.1)
            .flat_map(|dy| (0..size.0).map(move |dx| (site.0 + dx, site.1 + dy)))
            .map(|t| (fogged && self.vision.unexplored(t.0, t.1)) || self.tile_blocked(kind, t, builder))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::controller;
    use crate::world::tests::field;

    const PEASANT: u8 = 2;
    const FOOTMAN: u8 = 0;
    const TANKER: u8 = 0x1a;
    const FARM: u8 = 0x3a;
    const SHIPYARD: u8 = 0x48;
    const TOWN_HALL: u8 = 0x4a;
    const FOUNDRY: u8 = 0x4e;
    const PLATFORM: u8 = 0x56;
    const MINE: u8 = 0x5c;
    const PATCH: u8 = 0x5d;

    /// A 32 x 32 open land map in Classic mode, player 0 human and local with everything seen (and the reveal-map
    /// cheat on), the default sizes, and a peasant of player 0 out of the way.
    fn town() -> (World, UnitId) {
        let mut w = field(32, 32);
        w.players[0].controller = controller::HUMAN;
        w.players[1].controller = controller::COMPUTER;
        let base = w.stats.0[0];
        let mut set = |k: u8, size: (u16, u16), f: u32| {
            let s = &mut w.stats.0[k as usize];
            *s = base;
            s.size = size;
            s.flags = f;
        };
        set(FOOTMAN, (1, 1), flags::LAND);
        set(PEASANT, (1, 1), flags::LAND | flags::PEON);
        set(TANKER, (1, 1), flags::SEA | flags::TANKER);
        set(FARM, (2, 2), flags::BUILDING);
        set(TOWN_HALL, (4, 4), flags::BUILDING | flags::GOLD_STORAGE);
        set(SHIPYARD, (3, 3), flags::BUILDING | flags::SHORE_BUILDING);
        set(FOUNDRY, (3, 3), flags::BUILDING | flags::SHORE_BUILDING);
        set(FOUNDRY + 1, (3, 3), flags::BUILDING | flags::SHORE_BUILDING);
        set(PLATFORM, (2, 2), flags::BUILDING | flags::OIL_SOURCE);
        set(PATCH, (2, 2), flags::BUILDING | flags::OIL_PATCH);
        set(MINE, (3, 3), flags::BUILDING | flags::MINE);
        w.stats.0[PLATFORM as usize].movement_class = SEA_CLASS;
        w.rules.reveal_map = true;
        w.rules.enhanced = false;
        for y in 0..32 {
            for x in 0..32 {
                w.vision.seen[(y * 32 + x) as usize] = 0;
            }
        }
        let p = w.spawn(PEASANT, 0, (0, 30), 0).unwrap();
        (w, p)
    }

    /// Turns the rectangle into water, with shore water in its first column.
    fn sea(w: &mut World, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..=y1 {
            for x in x0..=x1 {
                w.terrain.clear_bits(x, y, bits::LAND);
                w.terrain
                    .set_bits(x, y, bits::WATER | if x == x0 { bits::SHORE } else { 0 });
            }
        }
    }

    #[test]
    fn placement_image_tiles() {
        // Map-drawing vectors: a farm with a tree on one site tile; a shipyard on dry land far from water.
        let (mut w, p) = town();
        w.terrain.set_bits(11, 10, bits::OBSTACLE);
        assert_eq!(w.bad_site_tiles(FARM, (10, 10), p), [false, true, false, false]);
        assert_eq!(w.bad_site_tiles(SHIPYARD, (10, 10), p), [true; 9]);
        // Off the map: every tile.
        assert_eq!(w.bad_site_tiles(FARM, (31, 10), p), [true; 4]);
        // Unexplored tiles are bad for the local player's builder without the cheat.
        w.rules.reveal_map = false;
        w.reveal(0, (19, 19), 1);
        let bad = w.bad_site_tiles(FARM, (20, 20), p);
        let unexplored: Vec<bool> = [(20, 20), (21, 20), (20, 21), (21, 21)]
            .iter()
            .map(|&(x, y)| w.vision.unexplored(x, y))
            .collect();
        assert_eq!(bad, unexplored);
        assert!(bad.contains(&true) && bad.contains(&false), "{bad:?}");
    }

    #[test]
    fn messages() {
        let all = [
            (SiteCheck::OffMap, 442),
            (SiteCheck::Blocked, 443),
            (SiteCheck::NoShore, 444),
            (SiteCheck::Unexplored, 445),
            (SiteCheck::NoOilPatch, 446),
            (SiteCheck::TooCloseToMine, 447),
            (SiteCheck::TooCloseToOil, 448),
        ];
        for (c, n) in all {
            assert_eq!(c.message(), Some(n));
        }
        assert_eq!(SiteCheck::Allowed.message(), None);
    }

    #[test]
    fn too_close_overlap() {
        // Town hall at (10, 10), 4 x 4; a mine at (15, 10), 3 x 3: 15 − 3 = 12 < 14.
        assert!(too_close(((15, 10), (3, 3)), ((10, 10), (4, 4)), 3));
        // At 17: 17 − 3 = 14, not below 14.
        assert!(!too_close(((17, 10), (3, 3)), ((10, 10), (4, 4)), 3));
        // From the other side: the site starts 3 past the mine's end.
        assert!(!too_close(((4, 10), (3, 3)), ((10, 10), (4, 4)), 3));
        assert!(too_close(((5, 10), (3, 3)), ((10, 10), (4, 4)), 3));
        // Both axes must overlap.
        assert!(!too_close(((15, 17), (3, 3)), ((10, 10), (4, 4)), 3));
    }

    #[test]
    fn sea_layer_sites() {
        let (w, _) = town();
        assert_eq!(w.site_at(PLATFORM, (12, 9)), (11, 7));
        assert_eq!(w.site_at(PLATFORM, (0, 1)), (1, 1));
        assert_eq!(w.site_at(PLATFORM, (2, 3)), (1, 1));
        assert_eq!(w.site_at(PLATFORM, (4, 5)), (3, 3));
        // Every other building's site is the pointer's tile.
        assert_eq!(w.site_at(FARM, (12, 9)), (12, 9));
        assert_eq!(w.site_at(SHIPYARD, (0, 1)), (0, 1));
    }

    #[test]
    fn town_hall_and_gold_mine() {
        let (mut w, p) = town();
        w.spawn(MINE, 15, (15, 10), 1000).unwrap();
        assert_eq!(w.check_site(TOWN_HALL, (10, 10), p), SiteCheck::TooCloseToMine);
        let (mut w, p) = town();
        w.spawn(MINE, 15, (17, 10), 1000).unwrap();
        assert_eq!(w.check_site(TOWN_HALL, (10, 10), p), SiteCheck::Allowed);
        // A farm is no gold store: no spacing rule.
        assert_eq!(w.check_site(FARM, (14, 10), p), SiteCheck::Allowed);
    }

    #[test]
    fn a_mine_inside_or_dying_is_not_counted() {
        let (mut w, p) = town();
        let mine = w.spawn(MINE, 15, (15, 10), 1000).unwrap();
        let i = w.index(mine).unwrap();
        w.units[i].dying = true;
        assert_eq!(w.check_site(TOWN_HALL, (10, 10), p), SiteCheck::Allowed);
    }

    #[test]
    fn shipyard_and_oil() {
        let (mut w, p) = town();
        sea(&mut w, 10, 4, 31, 20);
        // Columns 10 to 12; a patch in the same rows from column 16.
        w.spawn(PATCH, 15, (16, 8), 0).unwrap();
        assert_eq!(w.check_site(SHIPYARD, (10, 8), p), SiteCheck::TooCloseToOil);
        let (mut w, p) = town();
        sea(&mut w, 10, 4, 31, 20);
        w.spawn(PATCH, 15, (17, 8), 0).unwrap();
        assert_eq!(w.check_site(SHIPYARD, (10, 8), p), SiteCheck::Allowed);
        // A platform counts as oil too.
        let (mut w, p) = town();
        sea(&mut w, 10, 4, 31, 20);
        w.spawn(PLATFORM, 1, (16, 8), 0).unwrap();
        assert_eq!(w.check_site(SHIPYARD, (10, 8), p), SiteCheck::TooCloseToOil);
    }

    #[test]
    fn foundry_next_to_oil() {
        let (mut w, p) = town();
        sea(&mut w, 10, 4, 31, 20);
        w.spawn(PATCH, 15, (13, 8), 0).unwrap();
        assert_eq!(
            w.check_site(FOUNDRY, (10, 8), p),
            SiteCheck::Allowed,
            "Classic: no oil check"
        );
        w.rules.enhanced = true;
        assert_eq!(w.check_site(FOUNDRY, (10, 8), p), SiteCheck::TooCloseToOil);
        // Enhanced, one free column between: not too close at d = 1.
        let (mut w, p) = town();
        sea(&mut w, 10, 4, 31, 20);
        w.rules.enhanced = true;
        w.spawn(PATCH, 15, (14, 8), 0).unwrap();
        assert_eq!(w.check_site(FOUNDRY, (10, 8), p), SiteCheck::Allowed);
        // The other race's foundry is an ordinary shore building: d = 4.
        assert_eq!(w.check_site(FOUNDRY + 1, (10, 8), p), SiteCheck::TooCloseToOil);
        w.rules.enhanced = false;
        assert_eq!(w.check_site(FOUNDRY + 1, (10, 8), p), SiteCheck::TooCloseToOil);
    }

    #[test]
    fn oil_platform_needs_a_patch_on_its_site() {
        let (mut w, p) = town();
        sea(&mut w, 8, 0, 31, 31);
        let site = w.site_at(PLATFORM, (12, 9));
        assert_eq!(site, (11, 7));
        assert_eq!(w.check_site(PLATFORM, site, p), SiteCheck::NoOilPatch);
        w.spawn(PATCH, 15, (13, 7), 0).unwrap();
        assert_eq!(
            w.check_site(PLATFORM, site, p),
            SiteCheck::NoOilPatch,
            "overlapping isn't enough"
        );
        w.spawn(PATCH, 15, (11, 7), 0).unwrap();
        assert_eq!(w.check_site(PLATFORM, site, p), SiteCheck::Allowed);
        // Land under the site blocks it.
        w.terrain.clear_bits(12, 8, bits::WATER);
        w.terrain.set_bits(12, 8, bits::LAND);
        assert_eq!(w.check_site(PLATFORM, site, p), SiteCheck::Blocked);
    }

    /// Harvest spec, "Mine and platform amounts": a platform built on a patch takes the patch's trips.
    #[test]
    fn a_platform_built_on_a_patch_takes_its_trips() {
        let (mut w, _) = town();
        sea(&mut w, 8, 0, 31, 31);
        let site = (11, 7);
        let patch = w.spawn(PATCH, 15, site, 37).unwrap();
        let t = w.spawn(TANKER, 0, (14, 7), 0).unwrap();
        let i = w.index(t).unwrap();
        w.raise(i, PLATFORM, site);
        let platform = w.units.iter().find(|u| u.kind == PLATFORM).expect("placed");
        assert_eq!(platform.resources, 37);
        assert!(w.unit(patch).is_none_or(|u| u.dying || !u.present()));
    }

    #[test]
    fn farm_on_a_tree_is_blocked() {
        let (mut w, p) = town();
        w.terrain.set_bits(11, 11, bits::OBSTACLE);
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Blocked);
        assert_eq!(w.check_site(FARM, (12, 10), p), SiteCheck::Allowed);
    }

    #[test]
    fn unexplored_outranks_blocked() {
        let (mut w, p) = town();
        w.rules.reveal_map = false;
        w.reveal(0, (10, 10), 3);
        // A column x on row 10 that is explored, with x + 1 unexplored.
        let x = (10..30)
            .find(|&x| !w.vision.unexplored(x, 10) && w.vision.unexplored(x + 1, 10))
            .unwrap();
        w.terrain.set_bits(x, 10, bits::OBSTACLE);
        // One footprint tile unexplored, another on a tree.
        assert_eq!(w.check_site(FARM, (x, 10), p), SiteCheck::Unexplored);
        // A fully explored site with the tree is just blocked.
        assert!((0..2).all(|d| !w.vision.unexplored(x - 1 + d, 10) && !w.vision.unexplored(x - 1 + d, 11)));
        assert_eq!(w.check_site(FARM, (x - 1, 10), p), SiteCheck::Blocked);
        // The reveal-map cheat skips the test.
        w.rules.reveal_map = true;
        assert_eq!(w.check_site(FARM, (x, 10), p), SiteCheck::Blocked);
        // So does a builder that isn't the local player's.
        w.rules.reveal_map = false;
        let other = w.spawn(PEASANT, 1, (0, 0), 0).unwrap();
        assert_eq!(w.check_site(FARM, (x, 10), other), SiteCheck::Blocked);
    }

    /// Round-60 errata (build-train spec, "Carrying out a player's build command"; `human01-basics-9500` n = 19): the
    /// check made in the command turn has no unexplored test; everything else is the same.
    #[test]
    fn the_command_turn_check_has_no_unexplored_test() {
        let (mut w, p) = town();
        w.rules.reveal_map = false;
        w.reveal(0, (10, 10), 3);
        let x = (10..30)
            .find(|&x| !w.vision.unexplored(x, 10) && w.vision.unexplored(x + 1, 10))
            .unwrap();
        assert_eq!(w.check_site(FARM, (x, 10), p), SiteCheck::Unexplored);
        assert_eq!(w.check_site_in_turn(FARM, (x, 10), p), SiteCheck::Allowed);
        w.terrain.set_bits(x, 10, bits::OBSTACLE);
        assert_eq!(w.check_site_in_turn(FARM, (x, 10), p), SiteCheck::Blocked);
    }

    #[test]
    fn shipyard_on_land_has_no_shore() {
        let (mut w, p) = town();
        // A land footprint with a tree on it: no shore outranks blocked.
        w.terrain.set_bits(11, 11, bits::OBSTACLE);
        assert_eq!(w.check_site(SHIPYARD, (10, 10), p), SiteCheck::NoShore);
        // Unexplored doesn't outrank it either.
        w.rules.reveal_map = false;
        assert_eq!(w.check_site(SHIPYARD, (10, 10), p), SiteCheck::NoShore);
    }

    #[test]
    fn off_the_map_ends_the_check() {
        let (mut w, p) = town();
        // A shipyard running past the right edge on land: no shore isn't reported.
        assert_eq!(w.check_site(SHIPYARD, (30, 10), p), SiteCheck::OffMap);
        // A town hall one column past the edge.
        assert_eq!(w.check_site(TOWN_HALL, (29, 10), p), SiteCheck::OffMap);
        assert_eq!(w.check_site(TOWN_HALL, (28, 28), p), SiteCheck::Allowed);
        assert_eq!(w.check_site(TOWN_HALL, (28, 29), p), SiteCheck::OffMap);
        assert_eq!(w.check_site(FARM, (-1, 4), p), SiteCheck::OffMap);
        assert_eq!(w.check_site(FARM, (4, -1), p), SiteCheck::OffMap);
        // Even with a mine right next to it.
        w.spawn(MINE, 15, (25, 10), 1000).unwrap();
        assert_eq!(w.check_site(TOWN_HALL, (29, 10), p), SiteCheck::OffMap);
    }

    #[test]
    fn units_in_fog_dont_block_a_human_builder() {
        let (mut w, p) = town();
        w.spawn(FOOTMAN, 1, (11, 10), 0).unwrap();
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Blocked, "visible");
        // Explored but not seen now.
        w.vision.seen[10 * 32 + 11] = vision_bit(0);
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Allowed);
        // A building's mark is ignored too, but terrain bits never are.
        let (mut w, p) = town();
        w.spawn(FARM, 1, (11, 10), 0).unwrap();
        w.terrain.set_bits(10, 11, bits::OBSTACLE);
        for n in [10 * 32 + 11, 10 * 32 + 12, 11 * 32 + 11, 11 * 32 + 12, 11 * 32 + 10] {
            w.vision.seen[n] = 0xff;
        }
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Blocked);
        w.terrain.clear_bits(10, 11, bits::OBSTACLE);
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Allowed);
    }

    #[test]
    fn fog_doesnt_help_a_computer_builder() {
        let (mut w, _) = town();
        let b = w.spawn(PEASANT, 1, (0, 0), 0).unwrap();
        w.spawn(FOOTMAN, 0, (11, 10), 0).unwrap();
        w.vision.seen[10 * 32 + 11] = 0xff;
        assert_eq!(w.check_site(FARM, (10, 10), b), SiteCheck::Blocked);
    }

    #[test]
    fn the_builder_doesnt_block_itself() {
        let (mut w, _) = town();
        let p = w.spawn(PEASANT, 0, (11, 11), 0).unwrap();
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Allowed);
        // Another peasant on the site still blocks.
        let (mut w, p) = town();
        w.spawn(PEASANT, 0, (11, 11), 0).unwrap();
        assert_eq!(w.check_site(FARM, (10, 10), p), SiteCheck::Blocked);
        // A builder that isn't a worker or tanker gets no exemption.
        let (mut w, _) = town();
        let f = w.spawn(FOOTMAN, 0, (11, 11), 0).unwrap();
        assert_eq!(w.check_site(FARM, (10, 10), f), SiteCheck::Blocked);
    }

    #[test]
    fn a_tanker_doesnt_block_its_shipyard() {
        let (mut w, _) = town();
        sea(&mut w, 10, 4, 31, 20);
        let t = w.spawn(TANKER, 0, (12, 10), 0).unwrap();
        assert_eq!(w.unit(t).unwrap().tile, (12, 10));
        assert_eq!(w.check_site(SHIPYARD, (10, 8), t), SiteCheck::Allowed);
    }

    #[test]
    fn the_check_changes_nothing() {
        let (mut w, p) = town();
        w.spawn(MINE, 15, (15, 10), 1000).unwrap();
        let before = (w.state_hash(), w.rng, w.terrain.words.clone());
        for site in [(10, 10), (-1, 0), (30, 30), (2, 2)] {
            w.check_site(TOWN_HALL, site, p);
        }
        assert_eq!(before, (w.state_hash(), w.rng, w.terrain.words.clone()));
    }

    fn vision_bit(p: u8) -> u8 {
        crate::vision::bit(p)
    }
}
