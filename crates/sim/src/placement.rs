//! Putting a unit on a free tile next to a building or unit: the ring search of `docs/specs/movement/placement.md`.
//!
//! A unit trained, a builder or worker coming out of a building, a passenger unloaded from a transport and a
//! transport pushing off from the shore all use one walk around a source rectangle, stopping at the first tile that
//! passes the unit's tile test. [`ring_walk`] is the walk itself (pure, so the spec's test counts can be checked
//! directly); [`World::find_place_class`] and [`World::find_place`] add the tile tests over the movement words.

use crate::terrain::{bits, mask};
use crate::world::{Layer, World};

/// The ring search never looks further than this for units coming out of a building (spec, "Data").
pub const LIMIT_OUT: i32 = 12;
/// The limit for unloading one passenger.
pub const LIMIT_UNLOAD: i32 = 6;
/// The limit for a transport pushing off from the shore.
pub const LIMIT_PUSH_OFF: i32 = 10;

/// The tile rectangle the search walks around: top-left tile and size in tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Source {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Source {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }

    /// A single tile, such as a transport's.
    pub fn tile(x: i32, y: i32) -> Self {
        Self::new(x, y, 1, 1)
    }
}

/// The tile test to apply ("Tile test").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceClass {
    /// No bit of the land mask is set.
    Land,
    /// Even coordinates and no other air unit.
    Air,
    /// Even coordinates, water, and no bit of the sea mask.
    Sea,
    /// Even coordinates, water, and no bit of the landing mask.
    Landing,
    /// Even coordinates and no bit of the sea mask; water is not required. Used only by the push-off search.
    PushOff,
}

impl From<Layer> for PlaceClass {
    fn from(l: Layer) -> Self {
        match l {
            Layer::Land => PlaceClass::Land,
            Layer::Air => PlaceClass::Air,
            Layer::Sea => PlaceClass::Sea,
        }
    }
}

/// East, south, west, north. y grows downwards, so +1 is a clockwise turn.
const STEP: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Walks the rings around `src` ("Where the search starts", "Walking the rings"), calling `visit` on each tile that
/// lies inside a square map of side `map_width` (others are skipped untested), and returns the first tile for which
/// it returns true. Tiles on a ring's corner are visited more than once, as in the spec's test counts.
pub fn ring_walk(
    src: Source,
    aim: (i32, i32),
    limit: i32,
    map_width: i32,
    mut visit: impl FnMut(i32, i32) -> bool,
) -> Option<(i32, i32)> {
    let (px, py, w, h) = (src.x, src.y, src.w, src.h);
    let (tx, ty) = aim;
    let (cx, mut dir, mut turn) = if tx <= px {
        (px - 1, 1usize, -1i32)
    } else if tx < px + w {
        (px - 1, 0, 1)
    } else {
        (px + w, 1, 1)
    };
    let cy = if ty >= py + h {
        if dir == 1 {
            dir = 3;
        }
        turn = -turn;
        py + h
    } else {
        py - 1
    };
    let vertical = dir % 2 == 1;
    let (mut a, mut b) = if vertical { (h, w) } else { (w, h) };
    let (mut x, mut y) = (cx, cy);
    let turned = |d: usize| (d as i32 + turn).rem_euclid(4) as usize;
    while a < limit {
        for n in [a + 1, b + 1, a + 1, b + 2] {
            for _ in 0..n {
                x += STEP[dir].0;
                y += STEP[dir].1;
                if x >= 0 && y >= 0 && x < map_width && y < map_width && visit(x, y) {
                    return Some((x, y));
                }
            }
            dir = turned(dir);
        }
        x -= STEP[dir].0;
        y -= STEP[dir].1;
        a += 2;
        b += 2;
    }
    None
}

impl World {
    /// The ring search for a unit of type `kind`: its movement class decides the tile test. `src` is the rectangle to
    /// search around, `aim` decides where the search starts, and `limit` is 12 for units coming out of a building.
    pub fn find_place(&self, kind: u8, src: Source, aim: (i32, i32), limit: i32) -> Option<(i32, i32)> {
        self.find_place_class(self.layer_of(kind).into(), src, aim, limit, None)
    }

    /// The ring search with an explicit tile test and, optionally, the region the tile must lie in (unloading).
    pub fn find_place_class(
        &self,
        class: PlaceClass,
        src: Source,
        aim: (i32, i32),
        limit: i32,
        region: Option<u16>,
    ) -> Option<(i32, i32)> {
        ring_walk(src, aim, limit, self.terrain.width as i32, |x, y| {
            self.place_test(class, x, y, region)
        })
    }

    /// The tile a transport pushing off from the shore should head for (`LIMIT_PUSH_OFF`), or `None` if it docks.
    pub fn find_push_off(&self, at: (i32, i32), target: (i32, i32)) -> Option<(i32, i32)> {
        self.find_place_class(
            PlaceClass::PushOff,
            Source::tile(at.0, at.1),
            target,
            LIMIT_PUSH_OFF,
            None,
        )
    }

    /// The tile test of the placement spec, on the raw movement word (marks included).
    fn place_test(&self, class: PlaceClass, x: i32, y: i32, region: Option<u16>) -> bool {
        let Some(n) = self.terrain.at(x, y) else { return false };
        if region.is_some_and(|r| self.terrain.regions[n] != r) {
            return false;
        }
        let word = self.terrain.words[n];
        let even = x & 1 == 0 && y & 1 == 0;
        match class {
            PlaceClass::Land => word & mask::LAND == 0,
            PlaceClass::Air => even && word & mask::AIR == 0,
            PlaceClass::Sea => even && word & bits::WATER != 0 && word & mask::SEA == 0,
            PlaceClass::Landing => even && word & bits::WATER != 0 && word & mask::LANDING == 0,
            PlaceClass::PushOff => even && word & mask::SEA == 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::tests::world;

    /// Runs the walk with a test that never passes; returns the tests made, in order.
    fn walk_all(src: Source, aim: (i32, i32), limit: i32) -> Vec<(i32, i32)> {
        let mut seen = Vec::new();
        let r = ring_walk(src, aim, limit, 100, |x, y| {
            seen.push((x, y));
            false
        });
        assert_eq!(r, None);
        seen
    }

    /// Walks until `pass` accepts a tile; returns it and the number of tests made.
    fn walk_until(
        src: Source,
        aim: (i32, i32),
        limit: i32,
        pass: impl Fn(i32, i32) -> bool,
    ) -> (Option<(i32, i32)>, usize) {
        let mut n = 0;
        let r = ring_walk(src, aim, limit, 100, |x, y| {
            n += 1;
            pass(x, y)
        });
        (r, n)
    }

    /// A world with a `size` x `size` map of open ground, for tests that set the movement words themselves.
    fn open(size: u16) -> World {
        let mut w = world();
        let n = size as usize * size as usize;
        w.units.clear();
        w.occupancy = [vec![None; n], vec![None; n]];
        w.terrain.width = size;
        w.terrain.height = size;
        w.terrain.words = vec![0; n];
        w.terrain.tiles = vec![0; n];
        w.terrain.regions = vec![0; n];
        w.terrain.wall_damage = vec![0; n];
        w
    }

    const HALL3: Source = Source {
        x: 10,
        y: 10,
        w: 3,
        h: 3,
    };

    #[test]
    fn first_tile_is_west_of_the_top_left() {
        let (r, n) = walk_until(HALL3, (10, 10), 12, |_, _| true);
        assert_eq!((r, n), (Some((9, 10)), 1));
    }

    #[test]
    fn three_by_three_nothing_passes() {
        let seen = walk_all(HALL3, (10, 10), 12);
        assert_eq!(seen.len(), 165);
        assert_eq!(seen.last(), Some(&(4, 5)));
        assert_eq!(
            seen[..17],
            [
                (9, 10),
                (9, 11),
                (9, 12),
                (9, 13),
                (10, 13),
                (11, 13),
                (12, 13),
                (13, 13),
                (13, 12),
                (13, 11),
                (13, 10),
                (13, 9),
                (12, 9),
                (11, 9),
                (10, 9),
                (9, 9),
                (8, 9)
            ]
        );
        // Ring 2 starts at the corner (8, 8): its first test is (8, 9) again.
        assert_eq!(seen[17], (8, 9));
    }

    #[test]
    fn aim_right_and_below_starts_south_east_going_north() {
        let seen = walk_all(HALL3, (20, 20), 12);
        assert_eq!(
            seen[..9],
            [
                (13, 12),
                (13, 11),
                (13, 10),
                (13, 9),
                (12, 9),
                (11, 9),
                (10, 9),
                (9, 9),
                (9, 10)
            ]
        );
    }

    #[test]
    fn aim_inside_the_span_above_starts_north_west_going_east() {
        let seen = walk_all(HALL3, (11, 5), 12);
        assert_eq!(
            seen[..9],
            [
                (10, 9),
                (11, 9),
                (12, 9),
                (13, 9),
                (13, 10),
                (13, 11),
                (13, 12),
                (13, 13),
                (12, 13)
            ]
        );
    }

    #[test]
    fn the_other_starts() {
        // Left, below: south-west corner, west side bottom to top, clockwise (north then east).
        let seen = walk_all(HALL3, (5, 20), 12);
        assert_eq!(seen[..5], [(9, 12), (9, 11), (9, 10), (9, 9), (10, 9)]);
        // Inside the span, below: south-west corner, south side left to right, anticlockwise (east then north).
        let seen = walk_all(HALL3, (11, 13), 12);
        assert_eq!(seen[..5], [(10, 13), (11, 13), (12, 13), (13, 13), (13, 12)]);
        // Right, not below: north-east corner, east side top to bottom, clockwise (south then west).
        let seen = walk_all(HALL3, (20, 10), 12);
        assert_eq!(seen[..5], [(13, 10), (13, 11), (13, 12), (13, 13), (12, 13)]);
    }

    #[test]
    fn unloading_from_a_transport() {
        let src = Source::tile(20, 20);
        let seen = walk_all(src, (20, 20), LIMIT_UNLOAD);
        assert_eq!(seen.len(), 51);
        assert_eq!(seen.last(), Some(&(16, 17)));
        assert_eq!(
            seen[..9],
            [
                (19, 20),
                (19, 21),
                (20, 21),
                (21, 21),
                (21, 20),
                (21, 19),
                (20, 19),
                (19, 19),
                (18, 19)
            ]
        );
    }

    #[test]
    fn four_by_four_nothing_passes() {
        let seen = walk_all(Source::new(10, 10, 4, 4), (10, 10), 12);
        assert_eq!(seen.len(), 132);
        assert_eq!(seen.last(), Some(&(5, 6)));
    }

    #[test]
    fn ring_counts_follow_the_table() {
        // Tests per ring are 4A + 5 for a square source of side A, so the total tells the number of rings.
        let rings = |size: i32, limit: i32| {
            let n = walk_all(Source::new(30, 30, size, size), (30, 30), limit).len() as i32;
            let (mut total, mut k) = (0, 0);
            while total < n {
                total += 4 * (size + 2 * k) + 5;
                k += 1;
            }
            assert_eq!(total, n);
            k
        };
        for (size, l12, l10, l6) in [(1, 6, 5, 3), (2, 5, 4, 2), (3, 5, 4, 2), (4, 4, 3, 1)] {
            assert_eq!(
                [rings(size, 12), rings(size, 10), rings(size, 6)],
                [l12, l10, l6],
                "size {size}"
            );
        }
    }

    #[test]
    fn tiles_off_the_map_are_skipped_untested() {
        let mut seen = Vec::new();
        ring_walk(Source::tile(0, 0), (0, 0), 2, 4, |x, y| {
            seen.push((x, y));
            false
        });
        assert!(seen.iter().all(|&(x, y)| (0..4).contains(&x) && (0..4).contains(&y)));
        assert!(!seen.is_empty());
    }

    #[test]
    fn shipyard_ship_comes_out_on_the_first_even_water_tile() {
        let mut w = open(40);
        for y in 0..40 {
            for x in 0..40 {
                w.terrain.words[y * 40 + x] = if x <= 8 { bits::WATER } else { bits::LAND };
            }
        }
        let mut n = 0;
        let r = ring_walk(HALL3, (10, 10), 12, 40, |x, y| {
            n += 1;
            w.place_test(PlaceClass::Sea, x, y, None)
        });
        assert_eq!((r, n), (Some((8, 10)), 19));
        assert_eq!(
            w.find_place_class(PlaceClass::Sea, HALL3, (10, 10), 12, None),
            Some((8, 10))
        );
    }

    #[test]
    fn shipyard_ship_can_come_out_at_the_south_west_corner() {
        let mut w = open(40);
        for y in 0..40 {
            for x in 0..40 {
                w.terrain.words[y * 40 + x] = if y >= 14 { bits::WATER } else { bits::LAND };
            }
        }
        let mut n = 0;
        let r = ring_walk(HALL3, (10, 10), 12, 40, |x, y| {
            n += 1;
            w.place_test(PlaceClass::Sea, x, y, None)
        });
        assert_eq!((r, n), (Some((8, 14)), 23));
    }

    #[test]
    fn push_off_vectors() {
        let src = Source::tile(20, 20);
        let target = (30, 20);
        // Open water where x >= 22, shore water or land elsewhere.
        let (r, n) = walk_until(src, target, LIMIT_PUSH_OFF, |x, y| x >= 22 && x % 2 == 0 && y % 2 == 0);
        assert_eq!((r, n), (Some((22, 20)), 11));
        let seen = walk_all(src, target, LIMIT_PUSH_OFF);
        assert_eq!(
            seen[..9],
            [
                (21, 20),
                (21, 21),
                (20, 21),
                (19, 21),
                (19, 20),
                (19, 19),
                (20, 19),
                (21, 19),
                (22, 19)
            ]
        );
        // Open water where x <= 18.
        let (r, n) = walk_until(src, target, LIMIT_PUSH_OFF, |x, y| x <= 18 && x % 2 == 0 && y % 2 == 0);
        assert_eq!((r, n), (Some((18, 22)), 17));
        // Nothing passes: 5 rings, 125 tests, last tile (26, 15) for target (25, 20).
        let seen = walk_all(src, (25, 20), LIMIT_PUSH_OFF);
        assert_eq!(seen.len(), 125);
        assert_eq!(seen.last(), Some(&(26, 15)));
    }

    #[test]
    fn push_off_does_not_need_water() {
        let mut w = open(40);
        // Plain ground everywhere: the first even/even tile on the walk passes.
        assert_eq!(w.find_push_off((20, 20), (30, 20)), Some((22, 20)));
        w.terrain.words.fill(mask::SEA);
        assert_eq!(w.find_push_off((20, 20), (30, 20)), None);
    }

    #[test]
    fn tile_tests_per_class() {
        let mut w = open(8);
        let put = |w: &mut World, x: usize, y: usize, v: u16| w.terrain.words[y * 8 + x] = v;
        // Land: the land bit alone is fine; shore, walls, buildings, units, water and obstacles block.
        put(&mut w, 1, 1, bits::LAND);
        assert!(w.place_test(PlaceClass::Land, 1, 1, None));
        for bit in [
            bits::SHORE,
            bits::HUMAN_WALL,
            bits::COMPUTER_WALL,
            bits::WATER,
            bits::OBSTACLE,
            bits::BUILDING,
            bits::UNIT,
        ] {
            put(&mut w, 1, 1, bit);
            assert!(!w.place_test(PlaceClass::Land, 1, 1, None), "{bit:#x}");
        }
        put(&mut w, 1, 1, bits::AIR_UNIT);
        assert!(w.place_test(PlaceClass::Land, 1, 1, None));
        // Air: even coordinates only, and only air units block.
        put(&mut w, 2, 2, bits::LAND | bits::WATER | bits::BUILDING);
        assert!(w.place_test(PlaceClass::Air, 2, 2, None));
        assert!(!w.place_test(PlaceClass::Air, 2, 3, None));
        put(&mut w, 2, 2, bits::AIR_UNIT);
        assert!(!w.place_test(PlaceClass::Air, 2, 2, None));
        // Sea: water needed; shore water, land, buildings and units block. Landing lets shore water through.
        put(&mut w, 4, 4, bits::WATER);
        assert!(w.place_test(PlaceClass::Sea, 4, 4, None));
        assert!(w.place_test(PlaceClass::Landing, 4, 4, None));
        put(&mut w, 4, 4, bits::WATER | bits::SHORE);
        assert!(!w.place_test(PlaceClass::Sea, 4, 4, None));
        assert!(w.place_test(PlaceClass::Landing, 4, 4, None));
        put(&mut w, 4, 4, bits::WATER | bits::UNIT);
        assert!(!w.place_test(PlaceClass::Landing, 4, 4, None));
        put(&mut w, 4, 4, 0);
        assert!(!w.place_test(PlaceClass::Sea, 4, 4, None));
        assert!(!w.place_test(PlaceClass::Landing, 4, 4, None));
        // Region condition.
        w.terrain.regions[3 * 8 + 3] = 7;
        assert!(w.place_test(PlaceClass::Land, 3, 3, Some(7)));
        assert!(!w.place_test(PlaceClass::Land, 3, 3, Some(8)));
    }
}
