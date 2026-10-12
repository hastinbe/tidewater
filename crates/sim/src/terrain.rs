//! The map's tiles: movement words, the region map, tile shapes and wall damage, and the ways play changes them
//! (walls broken, trees felled, rock blasted). Spec: `docs/specs/movement/occupancy.md`.
//!
//! A tile's shape is its internal tile index, which is the tileset megatile it draws: the map's tile numbers become
//! internal indices through the tileset's conversion table, which is the megatile list of `.cv4` (see
//! `tidewater_formats::tileset`).

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use tidewater_formats::tileset::{ChangeTable, ChangeTables, Tileset};

/// Movement word bits ("Terrain bits", "Run-time marks").
pub mod bits {
    pub const LAND: u16 = 0x01;
    pub const SHORE: u16 = 0x02;
    pub const HUMAN_WALL: u16 = 0x04;
    pub const COMPUTER_WALL: u16 = 0x08;
    pub const NO_BUILD: u16 = 0x10;
    pub const WATER: u16 = 0x40;
    pub const OBSTACLE: u16 = 0x80;
    pub const UNIT: u16 = 0x0100;
    pub const AIR_UNIT: u16 = 0x0200;
    pub const BUILDING: u16 = 0x0800;
}

/// Blocking and placement masks ("Movement layers and blocking masks", "Building placement masks").
pub mod mask {
    pub const LAND: u16 = 0x09CE;
    pub const AIR: u16 = 0x0200;
    pub const SEA: u16 = 0x0903;
    pub const LANDING: u16 = 0x0901;
    pub const LAND_BUILDING: u16 = 0x09DE;
    pub const SHORE_BUILDING: u16 = 0x0911;
    pub const OIL_PLATFORM: u16 = 0x0913;
}

/// Region map values ("How passability changes"). Below [`LAND_AREA`] is a water area.
pub mod region {
    pub const LAND_AREA: u16 = 0x4000;
    pub const UNASSIGNED: u16 = 0xFFFA;
    pub const WALL: u16 = 0xFFFB;
    pub const RESERVED_FOREST: u16 = 0xFFFC;
    pub const ROCK: u16 = 0xFFFD;
    pub const FOREST: u16 = 0xFFFE;

    pub fn is_land_area(r: u16) -> bool {
        r & LAND_AREA != 0 && r < UNASSIGNED
    }

    pub fn is_forest(r: u16) -> bool {
        r == FOREST || r == RESERVED_FOREST
    }
}

/// Internal index of the first wall shape; intact walls run up to [`DAMAGED_WALL`], damaged ones up to
/// [`RUBBLE`], rubble after.
pub const WALL_BASE: u16 = 16;
/// Internal index of the first intact orc-style wall (walls spec, "Internal tile index ranges of walls").
pub const ORC_WALL: u16 = 34;
pub const DAMAGED_WALL: u16 = 52;
pub const RUBBLE: u16 = 88;
/// The highest wall damage counter that doesn't advance the wall.
const WALL_COUNTER_MAX: u16 = 19;
/// Entry bits meaning "cleared" in the forest and rock tables.
const CLEARED: u16 = 0xF000;

/// "Variant by neighbour mask": masks from 6 on skip variant 6, and from 11 on also variant 12.
fn wall_variant(m: u16) -> u16 {
    let v = if m >= 6 { m + 1 } else { m };
    if v >= 12 { v + 1 } else { v }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallStage {
    Intact,
    Damaged,
    Rubble,
}

/// The positions a demolition blast at (x, y) clears rock and forest around, in order ("Rock").
const BLAST: [(i32, i32); 12] = [
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
    (0, 1),
    (0, -1),
    (1, 0),
    (-1, 0),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terrain {
    pub width: u16,
    pub height: u16,
    /// Per tile: terrain bits in the low byte, run-time marks in the high byte.
    pub words: Vec<u16>,
    /// Per tile: the internal tile index (megatile drawn).
    pub tiles: Vec<u16>,
    pub regions: Vec<u16>,
    /// Per tile: the wall damage counter, 0 to 19.
    pub wall_damage: Vec<u8>,
    changes: Arc<ChangeTables>,
    /// Tiles whose shape changed, for the client to redraw and clear. Not game state.
    pub redraw: Vec<(i32, i32)>,
}

impl Hash for Terrain {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.words.hash(h);
        self.tiles.hash(h);
        self.regions.hash(h);
        self.wall_damage.hash(h);
    }
}

/// A region value for a tileset tile group when the map has no region map: walls, forest and rock by flags and
/// corner terrain codes (7 forest, 8 rock, as the tilesets' data shows), water, and one land area for the rest.
fn derived_region(flags: u16, corners: [u16; 4]) -> u16 {
    let any = |code: u16| corners.iter().any(|&c| c & 0xff == code || c >> 8 == code);
    if flags & bits::WATER != 0 {
        0
    } else if flags & bits::COMPUTER_WALL != 0 {
        region::WALL
    } else if flags & bits::OBSTACLE != 0 && flags & bits::LAND != 0 {
        if any(7) {
            region::FOREST
        } else if any(8) {
            region::ROCK
        } else {
            region::UNASSIGNED
        }
    } else if flags & bits::LAND != 0 {
        region::LAND_AREA
    } else {
        region::UNASSIGNED
    }
}

impl Terrain {
    pub fn new(
        width: u16,
        height: u16,
        tiles: Vec<u16>,
        words: Vec<u16>,
        regions: Vec<u16>,
        changes: Arc<ChangeTables>,
    ) -> Self {
        let n = width as usize * height as usize;
        Self {
            width,
            height,
            words,
            tiles,
            regions,
            wall_damage: vec![0; n],
            changes,
            redraw: Vec::new(),
        }
    }

    /// The terrain of a map: its tile numbers, and its movement words and region map when it has them. Without
    /// them, the movement words are the tileset's group flags (which the map editor fills them from) and the regions
    /// are derived from the same flags. `old_oil` (a map with an `OILM` section) turns bit 0x04 into 0x08.
    pub fn from_map(
        tileset: &Tileset,
        map_tiles: &[u16],
        words: Option<&[u16]>,
        regions: Option<&[u16]>,
        width: u16,
        height: u16,
        old_oil: bool,
    ) -> Self {
        let group = |t: u16| tileset.groups.get((t >> 4) as usize);
        let tiles = map_tiles
            .iter()
            .map(|&t| tileset.megatile_for(t).unwrap_or(0) as u16)
            .collect();
        let mut words: Vec<u16> = match words {
            Some(w) => w[..map_tiles.len()].to_vec(),
            None => map_tiles.iter().map(|&t| tileset.flags_for(t)).collect(),
        };
        if old_oil {
            for w in words.iter_mut().filter(|w| **w & bits::HUMAN_WALL != 0) {
                *w = (*w & !bits::HUMAN_WALL) | bits::COMPUTER_WALL;
            }
        }
        let regions = match regions {
            Some(r) => r[..map_tiles.len()].to_vec(),
            None => map_tiles
                .iter()
                .map(|&t| group(t).map_or(region::UNASSIGNED, |g| derived_region(g.flags, g.terrain)))
                .collect(),
        };
        let changes = Arc::new(tileset.changes.clone().unwrap_or_default());
        Self::new(width, height, tiles, words, regions, changes)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    pub(crate) fn at(&self, x: i32, y: i32) -> Option<usize> {
        self.contains(x, y).then(|| (y * self.width as i32 + x) as usize)
    }

    fn xy(&self, n: usize) -> (i32, i32) {
        ((n % self.width as usize) as i32, (n / self.width as usize) as i32)
    }

    /// The movement word; off the map every bit is set.
    pub fn word(&self, x: i32, y: i32) -> u16 {
        self.at(x, y).map_or(u16::MAX, |n| self.words[n])
    }

    /// Whether the tile's movement word shares no bit with `mask`.
    pub fn allows(&self, mask: u16, x: i32, y: i32) -> bool {
        self.word(x, y) & mask == 0
    }

    pub fn set_bits(&mut self, x: i32, y: i32, b: u16) {
        if let Some(n) = self.at(x, y) {
            self.words[n] |= b;
        }
    }

    pub fn clear_bits(&mut self, x: i32, y: i32, b: u16) {
        if let Some(n) = self.at(x, y) {
            self.words[n] &= !b;
        }
    }

    pub fn region(&self, x: i32, y: i32) -> Option<u16> {
        self.at(x, y).map(|n| self.regions[n])
    }

    pub fn tile(&self, x: i32, y: i32) -> Option<u16> {
        self.at(x, y).map(|n| self.tiles[n])
    }

    fn set_tile(&mut self, n: usize, i: u16) {
        if self.tiles[n] != i {
            self.tiles[n] = i;
            let xy = self.xy(n);
            self.redraw.push(xy);
        }
    }

    /// Internal index of the first forest shape, then of the first rock shape ("Wall, forest and rock tables").
    fn bases(&self) -> (u16, u16) {
        let forest = WALL_BASE + (self.changes.wall.rows as u16).saturating_sub(1);
        (forest, forest + (self.changes.forest.rows as u16).saturating_sub(1))
    }

    /// Whether a tile is a wall a hit can damage: a map wall (region "wall"), or a wall built in play, which keeps its
    /// land-area value but carries a wall-owner bit (walls spec, "Completion"). Our reading: the specs don't say how
    /// a hit finds a built wall.
    fn is_wall(&self, n: usize) -> bool {
        self.regions[n] == region::WALL || self.has_wall_bit(n)
    }

    fn has_wall_bit(&self, n: usize) -> bool {
        self.words[n] & (bits::HUMAN_WALL | bits::COMPUTER_WALL) != 0
    }

    pub fn wall_stage(&self, x: i32, y: i32) -> Option<WallStage> {
        let n = self.at(x, y)?;
        self.is_wall(n).then(|| match self.tiles[n] {
            i if i < DAMAGED_WALL => WallStage::Intact,
            i if i < RUBBLE => WallStage::Damaged,
            _ => WallStage::Rubble,
        })
    }

    /// A hit on a wall tile ("Damaging a wall"). Damage reaches the wall as one byte. Returns whether the wall advanced
    /// a stage.
    pub fn hit_wall(&mut self, x: i32, y: i32, damage: u32) -> bool {
        let Some(n) = self.at(x, y) else { return false };
        if !self.is_wall(n) {
            return false;
        }
        let c = self.wall_damage[n] as u16 + ((damage % 256) / 4).max(1) as u16;
        if c > WALL_COUNTER_MAX {
            self.advance_wall(x, y);
            true
        } else {
            self.wall_damage[n] = c as u8;
            false
        }
    }

    /// Moves a wall tile one stage on through the wall table, resetting its counter.
    pub fn advance_wall(&mut self, x: i32, y: i32) {
        let Some(n) = self.at(x, y) else { return };
        if !self.is_wall(n) {
            return;
        }
        let i = self.tiles[n];
        let next = i
            .checked_sub(WALL_BASE)
            .and_then(|r| self.changes.wall.get(r as usize + 1, 1))
            .map_or(RUBBLE, |e| WALL_BASE + e.saturating_sub(1));
        self.set_tile(n, next);
        self.wall_damage[n] = 0;
        if next >= RUBBLE {
            self.rubble(n);
        }
    }

    /// A finished wall piece becomes wall terrain (walls spec, "Completion", steps 3 and 4): the first intact graphic
    /// of the style (`orc` from the owner's race), counter 0, movement bits 0x91 plus the owner bit, then the joining
    /// of north, west, the tile itself, east and south. The region value is left alone.
    pub fn build_wall(&mut self, x: i32, y: i32, orc: bool, human_controlled: bool) {
        let Some(n) = self.at(x, y) else { return };
        self.set_tile(n, if orc { ORC_WALL } else { WALL_BASE });
        self.wall_damage[n] = 0;
        let owner = if human_controlled {
            bits::HUMAN_WALL
        } else {
            bits::COMPUTER_WALL
        };
        self.words[n] |= bits::OBSTACLE | bits::NO_BUILD | bits::LAND | owner;
        for (dx, dy) in [(0, -1), (-1, 0), (0, 0), (1, 0), (0, 1)] {
            self.join(x + dx, y + dy);
        }
    }

    /// "Joining": a tile with a wall bit takes the variant its wall neighbours call for, in its own style, intact.
    fn join(&mut self, x: i32, y: i32) {
        let Some(n) = self.at(x, y) else { return };
        if !self.has_wall_bit(n) {
            return;
        }
        let wall = |dx: i32, dy: i32| self.at(x + dx, y + dy).is_some_and(|m| self.has_wall_bit(m));
        let m = [(0, 1), (1, 0), (0, -1), (-1, 0)]
            .iter()
            .enumerate()
            .filter(|&(_, &(dx, dy))| wall(dx, dy))
            .fold(0u16, |m, (b, _)| m | 1 << b);
        let base = match self.tiles[n] {
            16..=33 | 52..=69 => WALL_BASE,
            34..=51 | 70..=87 => ORC_WALL,
            _ => return,
        };
        self.set_tile(n, base + wall_variant(m));
        self.wall_damage[n] = 0;
    }

    /// A wall tile has become rubble ("Rubble").
    fn rubble(&mut self, n: usize) {
        let (x, y) = self.xy(n);
        self.words[n] &= !(bits::OBSTACLE | bits::NO_BUILD | bits::COMPUTER_WALL | bits::HUMAN_WALL);
        for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
            if self.wall_stage(x + dx, y + dy) == Some(WallStage::Intact) {
                self.advance_wall(x + dx, y + dy);
            }
        }
        let area = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
            .filter_map(|(x, y)| self.region(x, y))
            .find(|&r| region::is_land_area(r))
            .unwrap_or(region::LAND_AREA);
        self.regions[n] = area;
        self.merge(x, y);
    }

    /// A woodcutter commits to a free tree. Returns whether the tile was a free tree.
    pub fn reserve(&mut self, x: i32, y: i32) -> bool {
        match self.at(x, y) {
            Some(n) if self.regions[n] == region::FOREST => {
                self.regions[n] = region::RESERVED_FOREST;
                true
            }
            _ => false,
        }
    }

    /// Gives a reserved tree back.
    pub fn unreserve(&mut self, x: i32, y: i32) {
        if let Some(n) = self.at(x, y)
            && self.regions[n] == region::RESERVED_FOREST
        {
            self.regions[n] = region::FOREST;
        }
    }

    /// Fells the tree at (x, y) ("Felling"). `area` is the region value of the woodcutter's own tile.
    pub fn fell(&mut self, x: i32, y: i32, area: u16) {
        self.reshape(x, y, false, area);
    }

    /// A demolition blast at (x, y) ("Rock", steps 1 to 3). `area` is the region value of the demolition unit's tile.
    /// Damage to units is the caller's.
    pub fn blast(&mut self, x: i32, y: i32, area: u16) {
        for (dx, dy) in BLAST {
            if self.region(x + dx, y + dy) == Some(region::ROCK) {
                self.reshape(x + dx, y + dy, true, area);
            }
        }
        for (dx, dy) in BLAST {
            if self.region(x + dx, y + dy).is_some_and(region::is_forest) {
                self.reshape(x + dx, y + dy, false, area);
            }
        }
        for (dx, dy) in (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (dx, dy))) {
            if (dx, dy) != (0, 0) {
                self.advance_wall(x + dx, y + dy);
                self.advance_wall(x + dx, y + dy);
            }
        }
    }

    /// The 3 x 3 update centred on (cx, cy) through the forest or rock table.
    fn reshape(&mut self, cx: i32, cy: i32, rock: bool, area: u16) {
        let changes = Arc::clone(&self.changes);
        let (forest_base, rock_base) = self.bases();
        let (table, base): (&ChangeTable, u16) = if rock {
            (&changes.rock, rock_base)
        } else {
            (&changes.forest, forest_base)
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (cx + dx, cy + dy);
                let Some(n) = self.at(x, y) else { continue };
                let r = self.regions[n];
                if !(if rock { r == region::ROCK } else { region::is_forest(r) }) {
                    continue;
                }
                let Some(row) = self.tiles[n].checked_sub(base).map(|r| r as usize + 1) else {
                    continue;
                };
                if table.get(row, 0).is_none_or(|c| c & CLEARED != 0) {
                    continue;
                }
                let k = (1 + 3 * (dy + 1) + (dx + 1)) as usize;
                let Some(e) = table.get(row, k) else { continue };
                self.set_tile(n, base + (e & !CLEARED).saturating_sub(1));
                if e & CLEARED != 0 {
                    self.words[n] &= !bits::OBSTACLE;
                    self.regions[n] = area;
                    self.merge(x, y);
                }
            }
        }
    }

    /// The tile at (x, y) has just been given a land area: unassigned land and other land areas next to it join it
    /// ("Area merge").
    fn merge(&mut self, x: i32, y: i32) {
        let Some(a) = self.region(x, y) else { return };
        for by in y - 1..=y + 1 {
            for bx in x - 1..=x + 1 {
                match self.region(bx, by) {
                    Some(region::UNASSIGNED) => {
                        for (ux, uy) in (by - 1..=by + 1).flat_map(|uy| (bx - 1..=bx + 1).map(move |ux| (ux, uy))) {
                            if let Some(n) = self.at(ux, uy)
                                && self.regions[n] == region::UNASSIGNED
                            {
                                self.regions[n] = a;
                            }
                        }
                    }
                    Some(b) if b != a && region::is_land_area(b) => {
                        for r in self.regions.iter_mut().filter(|r| **r == b) {
                            *r = a;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn table(columns: usize, rows: usize, f: impl Fn(usize, usize) -> u16) -> ChangeTable {
        ChangeTable {
            columns,
            rows,
            entries: (0..rows)
                .flat_map(|r| (0..columns).map(move |c| (r, c)))
                .map(|(r, c)| f(r, c))
                .collect(),
        }
    }

    /// Synthetic tables shaped like the shipped ones. Walls: intact row r steps to damaged row r + 36, damaged rows
    /// step to the first rubble row. Forest: every row keeps its shape except at k = 5 (cleared), with the entries the
    /// occupancy spec's test vectors list for rows 24, 20, 10, 11, 8 and 2.
    pub(crate) fn changes() -> ChangeTables {
        let wall = table(2, 87, |r, c| match (r, c) {
            (_, 0) => r as u16,
            (1..=36, _) => r as u16 + 36,
            (37..=72, _) => 73,
            _ => r as u16,
        });
        let forest = table(10, 41, |r, c| match (r, c) {
            (0, _) => c as u16,
            (_, 0) if r == 25 => 0x1019,
            (25, _) | (_, 5) => 0x1019,
            (24, _) => [24, 10, 23, 11, 8, 0, 2, 7, 5, 4][c],
            (20, 2) => 0x1019,
            (10, 4) | (8, 7) => 8,
            (11, 6) | (2, 9) => 2,
            _ => r as u16,
        });
        ChangeTables {
            wall,
            rock: forest.clone(),
            forest,
        }
    }

    pub(crate) const F: u16 = 16 + 87 - 1;

    /// A 10 x 10 map of one land area, with tiles from `f(x, y)` as (internal index, word, region).
    fn terrain(f: impl Fn(i32, i32) -> (u16, u16, u16)) -> Terrain {
        let cells: Vec<_> = (0..100).map(|n| f(n % 10, n / 10)).collect();
        Terrain::new(
            10,
            10,
            cells.iter().map(|c| c.0).collect(),
            cells.iter().map(|c| c.1).collect(),
            cells.iter().map(|c| c.2).collect(),
            Arc::new(changes()),
        )
    }

    fn ground(_: i32, _: i32) -> (u16, u16, u16) {
        (0, 0x01, 0x4000)
    }

    #[test]
    fn blocking_masks() {
        let open = |m: u16, w: u16| w & m == 0;
        for w in [0x0001, 0x0011, 0x0000, 0x0201, 0x0200] {
            assert!(open(mask::LAND, w), "{w:#x}");
        }
        for w in [0x0040, 0x0081, 0x0002, 0x0082, 0x0101, 0x0801] {
            assert!(!open(mask::LAND, w), "{w:#x}");
        }
        assert!(open(mask::SEA, 0x0040) && open(mask::SEA, 0x0000) && open(mask::SEA, 0x0200));
        assert!([0x0001, 0x0011, 0x0002].iter().all(|&w| !open(mask::SEA, w)));
        assert!([0x0081, 0x0101, 0x0801].iter().all(|&w| open(mask::AIR, w)));
        assert!([0x0201, 0x0200, 0x0F00].iter().all(|&w| !open(mask::AIR, w)));
    }

    #[test]
    fn built_walls_take_their_style_and_owner_bit() {
        let mut t = terrain(ground);
        t.build_wall(5, 5, false, true);
        assert_eq!(
            (t.tile(5, 5), t.word(5, 5), t.region(5, 5)),
            (Some(16), 0x95, Some(0x4000))
        );
        t.build_wall(2, 2, true, false);
        assert_eq!((t.tile(2, 2), t.word(2, 2)), (Some(34), 0x99));
    }

    #[test]
    fn walls_join_their_neighbours() {
        let mut t = terrain(ground);
        t.build_wall(4, 4, false, true);
        t.build_wall(5, 4, true, false);
        assert_eq!((t.tile(4, 4), t.tile(5, 4)), (Some(18), Some(43)));
        let mut t = terrain(ground);
        for (x, y) in [(5, 4), (4, 5), (6, 5), (5, 6)] {
            t.build_wall(x, y, false, true);
        }
        t.build_wall(5, 5, false, true);
        assert_eq!(t.tile(5, 5), Some(33));
        assert_eq!(t.tile(5, 4), Some(17), "the north wall gains its south bit");
        assert_eq!(t.tile(4, 5), Some(18), "the west wall gains its east bit");
        assert_eq!(t.tile(6, 5), Some(16 + 9), "the east wall gains its west bit");
        assert_eq!(t.tile(5, 6), Some(16 + 4), "the south wall gains its north bit");
        assert_eq!([6, 11].map(wall_variant), [7, 13]);
        assert_eq!(
            (0..16).map(wall_variant).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17]
        );
    }

    #[test]
    fn joining_repairs_and_skips_the_edge() {
        let mut t = terrain(|x, y| {
            if (x, y) == (5, 4) {
                (52, 0x8d, region::WALL)
            } else {
                ground(x, y)
            }
        });
        t.wall_damage[45] = 12;
        t.build_wall(5, 5, false, true);
        assert_eq!((t.tile(5, 4), t.wall_damage[45]), (Some(17), 0));
        assert_eq!(t.tile(5, 5), Some(16 + 4));
        let mut t = terrain(ground);
        t.build_wall(0, 0, false, true);
        assert_eq!(t.tile(0, 0), Some(16));
    }

    #[test]
    fn built_walls_break_like_map_walls() {
        let mut t = terrain(ground);
        t.build_wall(5, 5, false, true);
        assert!(t.hit_wall(5, 5, 110));
        assert_eq!(t.wall_stage(5, 5), Some(WallStage::Damaged));
        assert!(t.hit_wall(5, 5, 110));
        assert_eq!((t.word(5, 5), t.wall_stage(5, 5)), (0x01, None));
    }

    #[test]
    fn wall_damage_and_stages() {
        let wall = |x: i32, _: i32| {
            if x < 3 {
                (16 + x as u16, 0x8d, region::WALL)
            } else {
                ground(x, 0)
            }
        };
        let mut t = terrain(wall);
        for _ in 0..19 {
            assert!(!t.hit_wall(1, 0, 4));
        }
        assert_eq!((t.wall_damage[1], t.wall_stage(1, 0)), (19, Some(WallStage::Intact)));
        assert!(t.hit_wall(1, 0, 4));
        assert_eq!((t.wall_damage[1], t.wall_stage(1, 0)), (0, Some(WallStage::Damaged)));
        // 300 damage counts as 44: +11.
        t.hit_wall(0, 0, 300);
        assert_eq!(t.wall_damage[0], 11);
        // An intact wall takes two overflows; the excess is lost.
        assert!(t.hit_wall(2, 0, 110));
        assert_eq!((t.wall_damage[2], t.wall_stage(2, 0)), (0, Some(WallStage::Damaged)));
        assert!(t.hit_wall(2, 0, 110));
        assert_eq!(t.wall_stage(2, 0), None);
        assert_eq!(
            (t.word(2, 0), t.tile(2, 0), t.region(2, 0)),
            (0x01, Some(RUBBLE), Some(0x4000))
        );
        assert_eq!(t.wall_stage(1, 0), Some(WallStage::Damaged));
    }

    #[test]
    fn rubble_damages_intact_neighbours_once() {
        let wall = |x: i32, y: i32| {
            if y == 4 && (4..=6).contains(&x) {
                (20, 0x99, region::WALL)
            } else {
                ground(x, y)
            }
        };
        let mut t = terrain(wall);
        t.hit_wall(4, 4, 7);
        t.advance_wall(5, 4);
        t.advance_wall(5, 4);
        assert_eq!(t.word(5, 4), 0x01);
        assert_eq!(t.wall_stage(4, 4), Some(WallStage::Damaged));
        assert_eq!(t.wall_stage(6, 4), Some(WallStage::Damaged));
        assert_eq!((t.wall_damage[44], t.wall_damage[46]), (0, 0));
    }

    #[test]
    fn felling_follows_the_forest_table() {
        let solid = |_: i32, _: i32| (F + 23, 0x81, region::FOREST);
        let mut t = terrain(solid);
        let row = |t: &Terrain, x, y| t.tile(x, y).unwrap() - F + 1;
        t.fell(5, 5, 0x4001);
        assert_eq!((row(&t, 5, 5), t.word(5, 5), t.region(5, 5)), (25, 0x01, Some(0x4001)));
        let around = [
            (4, 4, 10),
            (5, 4, 23),
            (6, 4, 11),
            (4, 5, 8),
            (6, 5, 2),
            (4, 6, 7),
            (5, 6, 5),
            (6, 6, 4),
        ];
        for (x, y, r) in around {
            assert_eq!((row(&t, x, y), t.word(x, y)), (r, 0x81), "({x}, {y})");
        }
        t.fell(5, 4, 0x4001);
        let around = [
            (4, 3, 10),
            (5, 3, 23),
            (6, 3, 11),
            (4, 4, 8),
            (6, 4, 2),
            (4, 5, 8),
            (6, 5, 2),
            (5, 5, 25),
        ];
        for (x, y, r) in around {
            assert_eq!(row(&t, x, y), r, "({x}, {y})");
        }
        assert_eq!(t.word(5, 4), 0x01);
    }

    #[test]
    fn a_thin_edge_falls_with_its_tree() {
        let f = |x: i32, y: i32| match (x, y) {
            (5, 4) => (F + 19, 0x81, region::FOREST),
            (5, 5) => (F + 23, 0x81, region::RESERVED_FOREST),
            _ => ground(x, y),
        };
        let mut t = terrain(f);
        t.fell(5, 5, 0x4000);
        assert_eq!((t.word(5, 4), t.region(5, 4)), (0x01, Some(0x4000)));
    }

    #[test]
    fn reservations() {
        let mut t = terrain(|_, _| (F + 23, 0x81, region::FOREST));
        assert!(t.reserve(1, 1));
        assert!(!t.reserve(1, 1));
        t.unreserve(1, 1);
        assert_eq!(t.region(1, 1), Some(region::FOREST));
    }

    #[test]
    fn felling_joins_areas() {
        // Area A left of a tree column, area B right of it; an unassigned shore tile beside the tree.
        let f = |x: i32, y: i32| match x {
            5 => (F + 23, 0x81, region::FOREST),
            0..5 => (0, 0x01, 0x4001),
            _ if (x, y) == (6, 9) => (0, 0x02, region::UNASSIGNED),
            _ => (0, 0x01, 0x4002),
        };
        let mut t = terrain(f);
        t.fell(5, 9, 0x4001);
        assert!(t.regions.iter().all(|&r| r != 0x4002));
        assert_eq!((t.region(9, 0), t.region(6, 9)), (Some(0x4001), Some(0x4001)));
    }

    #[test]
    fn blasts_break_walls_and_clear_rock() {
        let rock_base = F + 41 - 1;
        let f = |x: i32, y: i32| match (x, y) {
            (4, 4) => (16, 0x8d, region::WALL),
            (6, 5) => (rock_base + 23, 0x81, region::ROCK),
            _ => ground(x, y),
        };
        let mut t = terrain(f);
        t.blast(5, 5, 0x4000);
        assert_eq!((t.word(4, 4), t.region(4, 4)), (0x01, Some(0x4000)));
        assert_eq!((t.word(6, 5), t.tile(6, 5)), (0x01, Some(rock_base + 24)));
        assert!(t.redraw.contains(&(4, 4)) && t.redraw.contains(&(6, 5)));
    }
}
