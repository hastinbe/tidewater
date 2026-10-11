//! What each map tile allows: walking, sailing, chopping, building.
//!
//! Derived from the tileset group flags and corner terrain codes (see `tidewater_formats::tileset`). The meanings
//! below were inferred from data inspection: across all four tilesets, the solid water groups carry 0x40, coast 0x11,
//! ground 0x01, forest and rock 0x81 (told apart by corner code 7 or 8), walls 0x89/0x8D, and shore tiles mixing coast
//! and water 0x02 or 0x82. Bit 0x80 marks an obstacle to land units.

use tidewater_formats::tileset::Tileset;

/// Tile properties as bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Tile(pub u8);

impl Tile {
    pub const LAND: u8 = 1 << 0;
    pub const SEA: u8 = 1 << 1;
    pub const COAST: u8 = 1 << 2;
    pub const FOREST: u8 = 1 << 3;
    pub const ROCK: u8 = 1 << 4;
    pub const WALL: u8 = 1 << 5;

    pub fn has(self, bits: u8) -> bool {
        self.0 & bits != 0
    }

    /// Classifies a tileset group from its flags word and corner terrain codes.
    pub fn from_group(flags: u16, corners: [u16; 4]) -> Self {
        let mut t = 0;
        if flags & 0x40 != 0 {
            t |= Self::SEA;
        }
        if flags & 0x01 != 0 && flags & 0x80 == 0 {
            t |= Self::LAND;
        }
        if flags & 0x10 != 0 {
            t |= Self::COAST;
        }
        if flags & 0x08 != 0 {
            t |= Self::WALL;
        } else if flags & 0x80 != 0 && flags & 0x01 != 0 {
            // Forest and rock share flags; the corner codes say which (low byte 7 forest, 8 rock).
            let any = |code: u16| corners.iter().any(|&c| c & 0xff == code || c >> 8 == code);
            if any(7) {
                t |= Self::FOREST;
            } else if any(8) {
                t |= Self::ROCK;
            }
        }
        Self(t)
    }
}

/// The map's tile grid.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Terrain {
    pub width: u16,
    pub height: u16,
    pub tiles: Vec<Tile>,
}

impl Terrain {
    pub fn from_map(tileset: &Tileset, tiles: &[u16], width: u16, height: u16) -> Self {
        let tiles = tiles
            .iter()
            .map(|&t| {
                tileset
                    .groups
                    .get((t >> 4) as usize)
                    .map_or(Tile(0), |g| Tile::from_group(g.flags, g.terrain))
            })
            .collect();
        Self { width, height, tiles }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && x < self.width as i32 && y < self.height as i32
    }

    pub fn get(&self, x: i32, y: i32) -> Tile {
        if self.contains(x, y) {
            self.tiles[(y * self.width as i32 + x) as usize]
        } else {
            Tile(0)
        }
    }

    /// Turns a destroyed wall into open ground.
    pub fn destroy_wall(&mut self, x: i32, y: i32) {
        if self.contains(x, y) {
            self.tiles[(y * self.width as i32 + x) as usize] = Tile(Tile::LAND);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_solid_groups() {
        assert_eq!(Tile::from_group(0x40, [0x202; 4]), Tile(Tile::SEA));
        assert_eq!(Tile::from_group(0x11, [0x404; 4]), Tile(Tile::LAND | Tile::COAST));
        assert_eq!(Tile::from_group(0x01, [0x606; 4]), Tile(Tile::LAND));
        assert_eq!(Tile::from_group(0x81, [0x707; 4]), Tile(Tile::FOREST));
        assert_eq!(Tile::from_group(0x81, [0x808; 4]), Tile(Tile::ROCK));
        assert_eq!(Tile::from_group(0x8d, [0x606, 0x606, 0x909, 0x909]), Tile(Tile::WALL));
        // Mixed coast/water shore: neither walkable nor sailable.
        assert_eq!(Tile::from_group(0x82, [0x402, 0x202, 0x402, 0x404]), Tile(0));
    }
}
