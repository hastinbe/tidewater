//! PCX images (menu backgrounds, briefing art, Battle.net panels).
//!
//! Format source: ZSoft Corporation, "Technical Reference Manual: PCX" (revision 5), as reproduced at
//! <https://www.fileformat.info/format/pcx/egff.htm> and in many mirrors of `PCX.TXT`. A 128-byte header (u8
//! manufacturer 0x0A, u8 version, u8 encoding 1 = RLE, u8 bits per pixel, u16 xmin, ymin, xmax, ymax, ..., u8 colour
//! planes at 65, u16 bytes per plane line at 66), then RLE scan lines: a byte with both top bits set repeats the next
//! byte `b & 0x3F` times, any other byte is one pixel. Each line holds `planes * bytes_per_line` bytes, which may
//! include padding past the image width. A 256-colour image ends with the byte 0x0C and 768 bytes of 8-bit RGB.
//!
//! Every PCX file in the installation was checked to be version 5, RLE, 8 bits per pixel, one plane, with the
//! trailing 256-colour palette; that is the only kind this reader accepts.

use thiserror::Error;

use crate::tileset::Palette;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PcxError {
    #[error("not a PCX image")]
    NotPcx,
    #[error("unsupported PCX layout: {bits} bits per pixel, {planes} planes, encoding {encoding}")]
    Unsupported { bits: u8, planes: u8, encoding: u8 },
    #[error("image data ends early")]
    Truncated,
}

/// A decoded 8-bit paletted image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcx {
    pub width: u16,
    pub height: u16,
    /// `width * height` palette indices, row-major.
    pub pixels: Vec<u8>,
    /// The trailing 256-colour palette, if the file has one.
    pub palette: Option<Palette>,
}

const HEADER: usize = 128;
const PALETTE_TAG: u8 = 0x0C;

impl Pcx {
    pub fn parse(d: &[u8]) -> Result<Self, PcxError> {
        if d.len() < HEADER || d[0] != 0x0A {
            return Err(PcxError::NotPcx);
        }
        let (encoding, bits, planes) = (d[2], d[3], d[65]);
        if encoding != 1 || bits != 8 || planes != 1 {
            return Err(PcxError::Unsupported { bits, planes, encoding });
        }
        let rd16 = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
        let (x0, y0, x1, y1) = (rd16(4), rd16(6), rd16(8), rd16(10));
        if x1 < x0 || y1 < y0 {
            return Err(PcxError::NotPcx);
        }
        let width = x1 - x0 + 1;
        let height = y1 - y0 + 1;
        let stride = rd16(66) as usize;
        if stride < width as usize {
            return Err(PcxError::NotPcx);
        }

        // Runs may cross line boundaries, so decode the whole stream, then drop each line's padding.
        let total = stride * height as usize;
        let mut raw = Vec::with_capacity(total);
        let mut p = HEADER;
        while raw.len() < total {
            let b = *d.get(p).ok_or(PcxError::Truncated)?;
            p += 1;
            if b & 0xC0 == 0xC0 {
                let v = *d.get(p).ok_or(PcxError::Truncated)?;
                p += 1;
                raw.extend(std::iter::repeat_n(v, (b & 0x3F) as usize));
            } else {
                raw.push(b);
            }
        }
        let pixels = raw
            .chunks_exact(stride)
            .take(height as usize)
            .flat_map(|line| &line[..width as usize])
            .copied()
            .collect();

        let palette = (d.len() >= HEADER + 769 && d[d.len() - 769] == PALETTE_TAG).then(|| {
            let mut pal = [[0u8; 3]; 256];
            for (c, rgb) in pal.iter_mut().zip(d[d.len() - 768..].chunks_exact(3)) {
                c.copy_from_slice(rgb);
            }
            Palette(pal)
        });
        Ok(Self {
            width,
            height,
            pixels,
            palette,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: u16, h: u16, stride: u16) -> Vec<u8> {
        let mut d = vec![0u8; HEADER];
        d[..4].copy_from_slice(&[0x0A, 5, 1, 8]);
        d[8..10].copy_from_slice(&(w - 1).to_le_bytes());
        d[10..12].copy_from_slice(&(h - 1).to_le_bytes());
        d[65] = 1;
        d[66..68].copy_from_slice(&stride.to_le_bytes());
        d
    }

    #[test]
    fn decodes_runs_padding_and_palette() {
        // 3x2 image with 4-byte lines. Line 0: a run of three 0xC5 (a value that needs a run), padding 0. Line 1:
        // literal 1, then a run of three 9s whose last byte is padding.
        let mut d = header(3, 2, 4);
        d.extend([0xC3, 0xC5, 0x00, 0x01, 0xC3, 0x09]);
        d.push(PALETTE_TAG);
        let mut pal = vec![0u8; 768];
        pal[9 * 3..9 * 3 + 3].copy_from_slice(&[10, 20, 30]);
        d.extend(pal);
        let img = Pcx::parse(&d).unwrap();
        assert_eq!((img.width, img.height), (3, 2));
        assert_eq!(img.pixels, [0xC5, 0xC5, 0xC5, 1, 9, 9]);
        assert_eq!(img.palette.unwrap().0[9], [10, 20, 30]);
    }

    #[test]
    fn run_crossing_lines() {
        let mut d = header(2, 2, 2);
        d.extend([0xC4, 7]);
        let img = Pcx::parse(&d).unwrap();
        assert_eq!(img.pixels, [7; 4]);
        assert!(img.palette.is_none());
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(Pcx::parse(&[0; 10]), Err(PcxError::NotPcx));
        let mut d = header(2, 1, 2);
        d[65] = 3;
        assert!(matches!(Pcx::parse(&d), Err(PcxError::Unsupported { .. })));
        let mut d = header(4, 1, 4);
        d.push(0xC2);
        assert_eq!(Pcx::parse(&d), Err(PcxError::Truncated));
    }
}
