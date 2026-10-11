//! Bitmap fonts (`Art\font*.fnt`).
//!
//! Layout established by data inspection; it is the same as the StarCraft `.fnt` fonts described on the Staredit
//! Network wiki ("FNT"). Header: the bytes `FONT`, u8 first character, u8 last character, u8 maximum glyph width,
//! u8 line height, then one u32 file offset per character from first to last (0 = no glyph, as for the space).
//! A glyph starts with u8 width, height, x offset and y offset, followed by run bytes until `width * height` pixels
//! are filled: each byte `b` skips `b >> 3` transparent pixels and then sets one pixel to shade `b & 7`.
//!
//! In the shipped fonts shades 1-4 run from the bright face to darker edges and 5 is the dark drop shadow. How the
//! game maps shades to palette colours is not established here; callers choose their own colours.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FntError {
    #[error("not a FONT file")]
    BadMagic,
    #[error("font truncated")]
    Truncated,
    #[error("glyph for character {0} out of bounds")]
    BadGlyph(u32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Glyph {
    pub width: u8,
    pub height: u8,
    pub x: u8,
    pub y: u8,
    /// `width * height` shades; 0 is transparent.
    pub shades: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Font {
    pub first: u8,
    pub max_width: u8,
    pub height: u8,
    /// Glyphs from `first` on; `None` for characters the font leaves out.
    pub glyphs: Vec<Option<Glyph>>,
}

impl Font {
    pub fn parse(d: &[u8]) -> Result<Self, FntError> {
        if d.get(..4) != Some(b"FONT") {
            return Err(FntError::BadMagic);
        }
        let h = d.get(4..8).ok_or(FntError::Truncated)?;
        let (first, last, max_width, height) = (h[0], h[1], h[2], h[3]);
        let count = (last as usize + 1).saturating_sub(first as usize);
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            let c = first as u32 + i as u32;
            let o = d.get(8 + i * 4..12 + i * 4).ok_or(FntError::Truncated)?;
            let off = u32::from_le_bytes(o.try_into().expect("4 bytes")) as usize;
            if off == 0 {
                glyphs.push(None);
                continue;
            }
            let g = d.get(off..off + 4).ok_or(FntError::BadGlyph(c))?;
            let (w, gh) = (g[0], g[1]);
            let n = w as usize * gh as usize;
            let mut shades = vec![0u8; n];
            let (mut p, mut at) = (off + 4, 0usize);
            while at < n {
                let b = *d.get(p).ok_or(FntError::BadGlyph(c))?;
                p += 1;
                at += (b >> 3) as usize;
                if at < n {
                    shades[at] = b & 7;
                }
                at += 1;
            }
            glyphs.push(Some(Glyph {
                width: w,
                height: gh,
                x: g[2],
                y: g[3],
                shades,
            }));
        }
        Ok(Self {
            first,
            max_width,
            height,
            glyphs,
        })
    }

    pub fn glyph(&self, c: u8) -> Option<&Glyph> {
        self.glyphs.get(c.checked_sub(self.first)? as usize)?.as_ref()
    }

    /// Horizontal advance of `c`: its glyph's offset plus width plus one pixel of spacing; for a missing glyph
    /// (the space) a third of the line height.
    pub fn advance(&self, c: u8) -> u32 {
        match self.glyph(c) {
            Some(g) => g.x as u32 + g.width as u32 + 1,
            None => (self.height as u32 / 3).max(1),
        }
    }

    /// Width in pixels of `text` set on one line.
    pub fn width(&self, text: &[u8]) -> u32 {
        text.iter().map(|&c| self.advance(c)).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Vec<u8> {
        // Characters 'A'..='B'; 'A' missing, 'B' 2x2 at (1, 0).
        let mut d = b"FONT".to_vec();
        d.extend([b'A', b'B', 2, 2]);
        d.extend(0u32.to_le_bytes());
        d.extend(16u32.to_le_bytes());
        d.extend([2, 2, 1, 0]);
        // shade 1, skip 2 then shade 5.
        d.extend([0x01, (2 << 3) | 5]);
        d
    }

    #[test]
    fn decodes_glyph_runs() {
        let f = Font::parse(&font()).unwrap();
        assert_eq!((f.first, f.max_width, f.height), (b'A', 2, 2));
        assert!(f.glyph(b'A').is_none());
        let g = f.glyph(b'B').unwrap();
        assert_eq!((g.width, g.height, g.x, g.y), (2, 2, 1, 0));
        assert_eq!(g.shades, vec![1, 0, 0, 5]);
        assert!(f.glyph(b'C').is_none());
        assert!(f.glyph(b' ').is_none());
        assert_eq!(f.width(b"BA"), 4 + 1);
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(Font::parse(b"FOO"), Err(FntError::BadMagic));
        assert_eq!(Font::parse(b"FONT"), Err(FntError::Truncated));
    }
}
