//! Uncompressed interface pictures: the `Art\*.bmp` frame pieces (`h_top.bmp`, `o_lft1.bmp`, ...) and the
//! `Art\*.cur` mouse cursors.
//!
//! Layouts established by data inspection (every file's size matches exactly). Despite the extension, the `.bmp`
//! files are not Windows bitmaps: u16 width, u16 height, then `width * height` palette indices, rows top to bottom.
//! A `.cur` file is u16 hot-spot x, u16 hot-spot y, then the same picture layout (u16 width, u16 height, then
//! `width * height` palette indices); index 0 is transparent. This is also the layout the reviewed spec
//! `docs/specs/ui/screen-and-cursors.md` ("Pointer shapes") gives: four 16-bit numbers (hot spot x, hot spot y, width,
//! height) followed by the indices, with the hot spot used as it is. No public write-up of the `.cur` layout is known
//! to us; every pointer file in the installation matches it exactly in size.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("picture has {got} bytes, its header needs {need}")]
pub struct RawError {
    got: usize,
    need: usize,
}

/// An 8-bit indexed picture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RawImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

impl RawImage {
    pub fn parse(d: &[u8]) -> Result<Self, RawError> {
        let rd16 = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let (Some(width), Some(height)) = (rd16(0), rd16(2)) else {
            return Err(RawError { got: d.len(), need: 4 });
        };
        let need = 4 + width as usize * height as usize;
        let pixels = d.get(4..need).ok_or(RawError { got: d.len(), need })?.to_vec();
        Ok(Self { width, height, pixels })
    }
}

/// A mouse cursor: a picture and the pixel that points.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub hotspot: (u16, u16),
    pub image: RawImage,
}

impl Cursor {
    pub fn parse(d: &[u8]) -> Result<Self, RawError> {
        let hx = d.get(0..2).ok_or(RawError { got: d.len(), need: 4 })?;
        let hy = d.get(2..4).ok_or(RawError { got: d.len(), need: 4 })?;
        let image = RawImage::parse(&d[4..]).map_err(|e| RawError {
            got: e.got + 4,
            need: e.need + 4,
        })?;
        Ok(Self {
            hotspot: (u16::from_le_bytes([hx[0], hx[1]]), u16::from_le_bytes([hy[0], hy[1]])),
            image,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_picture_and_cursor() {
        let img = [3, 0, 2, 0, 1, 2, 3, 4, 5, 6];
        let r = RawImage::parse(&img).unwrap();
        assert_eq!(
            (r.width, r.height, r.pixels.as_slice()),
            (3, 2, &[1, 2, 3, 4, 5, 6][..])
        );
        let mut cur = vec![1, 0, 2, 0];
        cur.extend(img);
        let c = Cursor::parse(&cur).unwrap();
        assert_eq!(c.hotspot, (1, 2));
        assert_eq!(c.image, r);
    }

    #[test]
    fn rejects_short_files() {
        assert!(RawImage::parse(&[3, 0, 2, 0, 1]).is_err());
        assert!(Cursor::parse(&[0, 0]).is_err());
    }
}
