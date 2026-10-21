//! GRP sprite sheets (units, buildings, missiles, icons).
//!
//! Format source: the StarCraft GRP format as documented on the Staredit Network wiki ("GRP"). Header: u16 frame count, u16 width, u16 height. Then per frame: u8 x offset, u8 y offset, u8 width,
//! u8 height, u32 offset of the frame's row table. The row table holds one u16 per row, each an offset (relative to
//! the table) of that row's run-length data. A run byte `b`: `b & 0x80` skips `b & 0x7F` transparent pixels;
//! `b & 0x40` repeats the next byte `b & 0x3F` times; otherwise the next `b` bytes are literal pixels.
//!
//! The interface sheets `Art\hframe.grp`, `Art\oframe.grp`, `Art\icons.grp`, `Art\human.grp` and `Art\orc.grp`
//! use the same header but store each frame uncompressed: the offset points at `width * height` palette indices
//! (established by data inspection: consecutive offsets differ by exactly that). In `human.grp`/`orc.grp` two frames
//! have the offset's top bit set; their data is 256 pixels wider than the stored width byte allows (300, the sheet
//! width, for a stored 44), so the bit is read as "add 256 to the width". See [`Grp::parse_uncompressed`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GrpError {
    #[error("truncated header")]
    Truncated,
    #[error("frame {0} data out of bounds")]
    BadFrame(usize),
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub x: u8,
    pub y: u8,
    pub width: u8,
    pub height: u8,
    /// `width * height` palette indices; 0 is transparent.
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Grp {
    pub width: u16,
    pub height: u16,
    pub frames: Vec<Frame>,
}

impl Grp {
    pub fn parse(d: &[u8]) -> Result<Self, GrpError> {
        let rd16 = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let n = rd16(0).ok_or(GrpError::Truncated)? as usize;
        let width = rd16(2).ok_or(GrpError::Truncated)?;
        let height = rd16(4).ok_or(GrpError::Truncated)?;
        let mut frames = Vec::with_capacity(n);
        for f in 0..n {
            let h = d.get(6 + f * 8..14 + f * 8).ok_or(GrpError::Truncated)?;
            let (x, y, w, fh) = (h[0], h[1], h[2], h[3]);
            let table = u32::from_le_bytes(h[4..8].try_into().expect("4 bytes")) as usize;
            let mut pixels = vec![0u8; w as usize * fh as usize];
            for row in 0..fh as usize {
                let mut p = table + rd16(table + row * 2).ok_or(GrpError::BadFrame(f))? as usize;
                let mut col = 0usize;
                let line = &mut pixels[row * w as usize..(row + 1) * w as usize];
                while col < w as usize {
                    let b = *d.get(p).ok_or(GrpError::BadFrame(f))?;
                    p += 1;
                    if b & 0x80 != 0 {
                        col += (b & 0x7F) as usize;
                    } else if b & 0x40 != 0 {
                        let v = *d.get(p).ok_or(GrpError::BadFrame(f))?;
                        p += 1;
                        let n = ((b & 0x3F) as usize).min(w as usize - col);
                        line[col..col + n].fill(v);
                        col += n;
                    } else {
                        let n = (b as usize).min(w as usize - col);
                        line[col..col + n].copy_from_slice(d.get(p..p + n).ok_or(GrpError::BadFrame(f))?);
                        p += b as usize;
                        col += n;
                    }
                }
            }
            frames.push(Frame {
                x,
                y,
                width: w,
                height: fh,
                pixels,
            });
        }
        Ok(Self { width, height, frames })
    }
}

impl Grp {
    /// Reads a sheet whose frames are stored uncompressed (see the module docs). Frames may be wider than 255 pixels,
    /// so [`Frame::width`] holds only the low byte for them; use `pixels.len() / height` for the real width.
    pub fn parse_uncompressed(d: &[u8]) -> Result<Self, GrpError> {
        let rd16 = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let n = rd16(0).ok_or(GrpError::Truncated)? as usize;
        let width = rd16(2).ok_or(GrpError::Truncated)?;
        let height = rd16(4).ok_or(GrpError::Truncated)?;
        let mut frames = Vec::with_capacity(n);
        for f in 0..n {
            let h = d.get(6 + f * 8..14 + f * 8).ok_or(GrpError::Truncated)?;
            let (x, y, w, fh) = (h[0], h[1], h[2], h[3]);
            let raw = u32::from_le_bytes(h[4..8].try_into().expect("4 bytes"));
            let wide = raw & 0x8000_0000 != 0;
            let off = (raw & 0x7FFF_FFFF) as usize;
            let real_w = w as usize + if wide { 256 } else { 0 };
            let pixels = d
                .get(off..off + real_w * fh as usize)
                .ok_or(GrpError::BadFrame(f))?
                .to_vec();
            frames.push(Frame {
                x,
                y,
                width: w,
                height: fh,
                pixels,
            });
        }
        Ok(Self { width, height, frames })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_uncompressed_frames() {
        // Two frames: 2x1 at offset 22, and a "wide" one whose width byte 1 means 257.
        let mut d = vec![2, 0, 0, 1, 1, 0];
        d.extend([0, 0, 2, 1]);
        d.extend(22u32.to_le_bytes());
        d.extend([0, 0, 1, 1]);
        d.extend((24u32 | 0x8000_0000).to_le_bytes());
        d.extend([7, 8]);
        d.extend(vec![9u8; 257]);
        let g = Grp::parse_uncompressed(&d).unwrap();
        assert_eq!((g.width, g.height), (256, 1));
        assert_eq!(g.frames[0].pixels, vec![7, 8]);
        assert_eq!(g.frames[1].pixels.len(), 257);
        d.pop();
        assert!(Grp::parse_uncompressed(&d).is_err());
    }

    #[test]
    fn decodes_runs() {
        // One frame, 4x1 at (1, 2): skip 1, repeat 7 twice, literal [5].
        let mut d = vec![1, 0, 6, 0, 3, 0, 1, 2, 4, 1];
        d.extend(14u32.to_le_bytes());
        d.extend(2u16.to_le_bytes()); // row 0 data right after the table
        d.extend([0x81, 0x42, 7, 0x01, 5]);
        let g = Grp::parse(&d).unwrap();
        assert_eq!((g.width, g.height), (6, 3));
        let f = &g.frames[0];
        assert_eq!((f.x, f.y, f.width, f.height), (1, 2, 4, 1));
        assert_eq!(f.pixels, vec![0, 7, 7, 5]);
    }
}
