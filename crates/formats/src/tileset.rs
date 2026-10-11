//! Terrain tilesets (`Art\bgs\<name>\<name>.{ppl,cv4,vx4,vr4}`).
//!
//! Layouts established by inspecting the data files (no public write-up covers these Battle.net Edition files; they
//! resemble StarCraft's `.cv5/.vx4/.vr4/.wpe` set described on the Staredit Network wiki):
//!
//! - `.ppl`: 256 RGB triples, 6 bits per channel (VGA DAC values 0..=63).
//! - `.vr4`: 8x8-pixel minitiles, 64 palette indices each, row-major.
//! - `.vx4`: megatiles (32x32 pixels), each 16 little-endian u16 minitile references in 4x4 row-major order.
//!   A reference is `minitile << 2 | vflip << 1 | hflip` (found by rendering: `>> 1` overruns the minitile table).
//! - `.cv4`: 42-byte tile groups. Group `n` holds map tiles `n << 4 ..= n << 4 | 15`: 16 u16 megatile indices (one
//!   per variant), then 4 u16 corner terrain codes and a u16 flags word.
//!
//! A map tile number `t` (the PUD `MTXM` value) therefore draws megatile `groups[t >> 4].megatiles[t & 15]`.
//!
//! The wall, forest and rock change tables (`n?_wall.bin`, `n?_tree.bin`, `n?_rock.bin` beside them) follow the
//! reviewed occupancy spec (`docs/specs/movement/occupancy.md`, "Wall, forest and rock tables"): a u16 column count, a
//! u16 row count, then the rows of u16 entries. Row 0 numbers the columns. The row counts differ between tilesets
//! (the forest table has 41 rows in Forest, 38 in Swamp and 36 in Iceland and XSwamp), so they are read, not assumed.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TilesetError {
    #[error("{file} has {len} bytes, not a multiple of {unit}")]
    BadSize {
        file: &'static str,
        len: usize,
        unit: usize,
    },
    #[error("palette has {0} bytes, expected 768")]
    BadPalette(usize),
    #[error("change table of {len} bytes doesn't hold its {columns} x {rows} entries")]
    BadTable { len: usize, columns: usize, rows: usize },
}

/// A wall, forest or rock change table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChangeTable {
    pub columns: usize,
    pub rows: usize,
    pub entries: Vec<u16>,
}

impl ChangeTable {
    pub fn parse(data: &[u8]) -> Result<Self, TilesetError> {
        let w: Vec<u16> = u16s(data).collect();
        let (columns, rows) = match w.as_slice() {
            [c, r, ..] => (*c as usize, *r as usize),
            _ => (0, 0),
        };
        if w.len() < 2 || w.len() - 2 != columns * rows {
            return Err(TilesetError::BadTable {
                len: data.len(),
                columns,
                rows,
            });
        }
        Ok(Self {
            columns,
            rows,
            entries: w[2..].to_vec(),
        })
    }

    /// The entry at `row`, `column`, if the table has it.
    pub fn get(&self, row: usize, column: usize) -> Option<u16> {
        (row < self.rows && column < self.columns).then(|| self.entries[row * self.columns + column])
    }
}

/// The three change tables of a tileset.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ChangeTables {
    pub wall: ChangeTable,
    pub forest: ChangeTable,
    pub rock: ChangeTable,
}

/// A 256-colour palette in 8-bit RGB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Palette(pub [[u8; 3]; 256]);

impl Palette {
    /// Reads 6-bit VGA triples, scaling to 8 bits as `v << 2 | v >> 4`.
    pub fn from_vga(data: &[u8]) -> Result<Self, TilesetError> {
        if data.len() != 768 {
            return Err(TilesetError::BadPalette(data.len()));
        }
        let mut p = [[0u8; 3]; 256];
        for (i, c) in data.chunks_exact(3).enumerate() {
            for k in 0..3 {
                let v = c[k] & 0x3F;
                p[i][k] = (v << 2) | (v >> 4);
            }
        }
        Ok(Self(p))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileGroup {
    pub megatiles: [u16; 16],
    pub terrain: [u16; 4],
    pub flags: u16,
}

#[derive(Clone, Debug)]
pub struct Tileset {
    pub palette: Palette,
    pub groups: Vec<TileGroup>,
    pub megatiles: Vec<[u16; 16]>,
    /// 64 bytes per minitile.
    pub minitiles: Vec<u8>,
    /// The wall, forest and rock change tables, when loaded.
    pub changes: Option<ChangeTables>,
}

fn u16s(b: &[u8]) -> impl Iterator<Item = u16> + '_ {
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]))
}

fn check(file: &'static str, len: usize, unit: usize) -> Result<(), TilesetError> {
    if !len.is_multiple_of(unit) {
        return Err(TilesetError::BadSize { file, len, unit });
    }
    Ok(())
}

pub const TILE: usize = 32;

impl Tileset {
    pub fn parse(ppl: &[u8], cv4: &[u8], vx4: &[u8], vr4: &[u8]) -> Result<Self, TilesetError> {
        check("cv4", cv4.len(), 42)?;
        check("vx4", vx4.len(), 32)?;
        check("vr4", vr4.len(), 64)?;
        let groups = cv4
            .chunks_exact(42)
            .map(|g| {
                let w: Vec<u16> = u16s(g).collect();
                TileGroup {
                    megatiles: w[0..16].try_into().expect("16"),
                    terrain: w[16..20].try_into().expect("4"),
                    flags: w[20],
                }
            })
            .collect();
        let megatiles = vx4
            .chunks_exact(32)
            .map(|m| u16s(m).collect::<Vec<_>>().try_into().expect("16"))
            .collect();
        Ok(Self {
            palette: Palette::from_vga(ppl)?,
            groups,
            megatiles,
            minitiles: vr4.to_vec(),
            changes: None,
        })
    }

    /// The megatile drawn for a map tile number, if the tileset defines it.
    pub fn megatile_for(&self, tile: u16) -> Option<usize> {
        let g = self.groups.get((tile >> 4) as usize)?;
        let m = g.megatiles[(tile & 15) as usize] as usize;
        (m < self.megatiles.len()).then_some(m)
    }

    /// The group flags for a map tile number (passability and terrain class).
    pub fn flags_for(&self, tile: u16) -> u16 {
        self.groups.get((tile >> 4) as usize).map_or(0, |g| g.flags)
    }

    /// Draws a `width` x `height` map of tile numbers as palette indices (row stride `width * TILE`). Returns the
    /// pixels and the number of tiles the tileset doesn't define (left as index 0).
    pub fn draw_map(&self, tiles: &[u16], width: usize, height: usize) -> (Vec<u8>, usize) {
        let stride = width * TILE;
        let mut px = vec![0u8; stride * height * TILE];
        let mut missing = 0;
        for ty in 0..height {
            for tx in 0..width {
                match self.megatile_for(tiles[ty * width + tx]) {
                    Some(m) => self.draw_megatile(m, &mut px[ty * TILE * stride + tx * TILE..], stride),
                    None => missing += 1,
                }
            }
        }
        (px, missing)
    }

    /// Draws megatile `m` as 32x32 palette indices into `out` (row stride `stride`).
    pub fn draw_megatile(&self, m: usize, out: &mut [u8], stride: usize) {
        for (k, &r) in self.megatiles[m].iter().enumerate() {
            let (mx, my) = ((k % 4) * 8, (k / 4) * 8);
            let mini = (r >> 2) as usize;
            let (hflip, vflip) = (r & 1 != 0, r & 2 != 0);
            let Some(src) = self.minitiles.get(mini * 64..mini * 64 + 64) else {
                continue;
            };
            for y in 0..8 {
                let sy = if vflip { 7 - y } else { y };
                for x in 0..8 {
                    let sx = if hflip { 7 - x } else { x };
                    out[(my + y) * stride + mx + x] = src[sy * 8 + sx];
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vga_scaling() {
        let mut ppl = vec![0u8; 768];
        ppl[3..6].copy_from_slice(&[63, 32, 1]);
        let p = Palette::from_vga(&ppl).unwrap();
        assert_eq!(p.0[1], [255, 130, 4]);
    }

    #[test]
    fn draws_flipped_minitile() {
        let ppl = vec![0u8; 768];
        let mut cv4 = vec![0u8; 42 * 2];
        cv4[42 + 2..42 + 4].copy_from_slice(&1u16.to_le_bytes()); // tile 0x11 -> megatile 1
        let mut vx4 = vec![0u8; 64];
        vx4[32..34].copy_from_slice(&((1u16 << 2) | 1).to_le_bytes()); // megatile 1, slot 0: minitile 1 flipped
        let mut vr4 = vec![0u8; 128];
        vr4[64] = 9; // minitile 1, pixel (0, 0)
        let t = Tileset::parse(&ppl, &cv4, &vx4, &vr4).unwrap();
        assert_eq!(t.megatile_for(0x11), Some(1));
        let mut out = vec![0u8; 32 * 32];
        t.draw_megatile(1, &mut out, 32);
        assert_eq!(out[7], 9);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn change_tables() {
        let b: Vec<u8> = [2u16, 2, 0, 1, 1, 0x25].iter().flat_map(|v| v.to_le_bytes()).collect();
        let t = ChangeTable::parse(&b).unwrap();
        assert_eq!((t.get(1, 1), t.get(2, 0)), (Some(0x25), None));
        assert!(ChangeTable::parse(&b[..10]).is_err());
    }
}
