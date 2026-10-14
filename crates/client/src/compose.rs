//! The screen's composition, without an engine: pictures placed on the 640 × 480 screen, one over another, into one
//! frame (`docs/design/client-core.md`). Each picture is sampled at the centre of each screen pixel it covers (nearest
//! texel), a fully transparent texel leaves the pixel as it was, and a tinted picture's colours are multiplied by the
//! tint in linear light, as a GPU sampling an sRGB texture into an sRGB target does.

/// The screen size in pixels.
pub const W: usize = 640;
pub const H: usize = 480;

/// A screen rectangle in pixels: left, top, right and bottom (exclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Clip {
    pub const SCREEN: Clip = Clip {
        x0: 0,
        y0: 0,
        x1: W as i32,
        y1: H as i32,
    };

    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Clip {
            x0: x,
            y0: y,
            x1: x + w,
            y1: y + h,
        }
    }

    /// The part on the screen.
    pub fn on_screen(self) -> Self {
        Clip {
            x0: self.x0.clamp(0, W as i32),
            y0: self.y0.clamp(0, H as i32),
            x1: self.x1.clamp(0, W as i32),
            y1: self.y1.clamp(0, H as i32),
        }
    }
}

/// A picture placed on the screen.
#[derive(Clone, Copy, Debug)]
pub struct Placed<'a> {
    /// RGBA, row by row (sRGB, straight alpha).
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
    /// The screen position of the picture's top-left corner, in pixels (it may fall between pixels).
    pub origin: (f64, f64),
    /// Screen pixels per picture pixel.
    pub scale: (f64, f64),
    /// Only this part of the screen is drawn on.
    pub clip: Clip,
    /// A grey tint (sRGB, 0 to 1): 1 leaves the colours as they are.
    pub tint: f32,
}

/// One whole screen, RGBA (opaque).
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub rgba: Vec<u8>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::black()
    }
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({:016x})", self.hash())
    }
}

/// sRGB to linear light (the sRGB standard's curve).
pub fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to sRGB.
pub fn to_srgb(l: f32) -> f32 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    }
}

/// A tint's multiplication of an 8-bit sRGB channel, in linear light.
fn tinted(table: &[u8; 256], c: u8) -> u8 {
    table[c as usize]
}

fn tint_table(tint: f32) -> [u8; 256] {
    let k = to_linear(tint.clamp(0.0, 1.0));
    std::array::from_fn(|c| {
        (to_srgb(to_linear(c as f32 / 255.0) * k) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    })
}

impl Frame {
    pub fn black() -> Self {
        let mut rgba = vec![0; W * H * 4];
        for px in rgba.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Frame { rgba }
    }

    /// Fills a rectangle with a colour.
    pub fn fill(&mut self, clip: Clip, rgb: [u8; 3]) {
        let c = clip.on_screen();
        for y in c.y0..c.y1 {
            for x in c.x0..c.x1 {
                let i = (y as usize * W + x as usize) * 4;
                self.rgba[i..i + 3].copy_from_slice(&rgb);
                self.rgba[i + 3] = 255;
            }
        }
    }

    /// Draws a placed picture over the frame.
    pub fn draw(&mut self, p: &Placed) {
        if p.width == 0 || p.height == 0 || p.scale.0 <= 0.0 || p.scale.1 <= 0.0 {
            return;
        }
        let table = (p.tint != 1.0).then(|| tint_table(p.tint));
        let c = p.clip.on_screen();
        // The texel under a pixel's centre, if the picture covers it.
        let texel = |px: i32, origin: f64, scale: f64, len: u32| {
            let t = ((px as f64 + 0.5 - origin) / scale).floor();
            (t >= 0.0 && t < len as f64).then_some(t as usize)
        };
        let cols: Vec<Option<usize>> = (c.x0..c.x1).map(|x| texel(x, p.origin.0, p.scale.0, p.width)).collect();
        for y in c.y0..c.y1 {
            let Some(ty) = texel(y, p.origin.1, p.scale.1, p.height) else {
                continue;
            };
            let row = &p.rgba[ty * p.width as usize * 4..(ty + 1) * p.width as usize * 4];
            let out = &mut self.rgba[y as usize * W * 4..(y as usize + 1) * W * 4];
            for (x, tx) in (c.x0..c.x1).zip(&cols) {
                let Some(tx) = *tx else { continue };
                let s = &row[tx * 4..tx * 4 + 4];
                if s[3] == 0 {
                    continue;
                }
                let mut rgb = [s[0], s[1], s[2]];
                if let Some(t) = &table {
                    rgb = rgb.map(|c| tinted(t, c));
                }
                let d = &mut out[x as usize * 4..x as usize * 4 + 4];
                if s[3] == 255 {
                    d[..3].copy_from_slice(&rgb);
                } else {
                    let a = s[3] as f32 / 255.0;
                    for k in 0..3 {
                        let l = to_linear(rgb[k] as f32 / 255.0) * a + to_linear(d[k] as f32 / 255.0) * (1.0 - a);
                        d[k] = (to_srgb(l) * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }

    /// The frame's fingerprint (FNV-1a over the pixels' colours).
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for px in self.rgba.chunks_exact(4) {
            let v = u32::from_le_bytes([px[0], px[1], px[2], 0]);
            h ^= v as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pic(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4] + Copy) -> Vec<u8> {
        (0..h).flat_map(|y| (0..w).flat_map(move |x| f(x, y))).collect()
    }

    fn at(f: &Frame, x: usize, y: usize) -> [u8; 4] {
        let i = (y * W + x) * 4;
        f.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn a_picture_lands_on_its_pixels_and_transparent_texels_leave_the_frame() {
        let rgba = pic(2, 2, |x, y| {
            if x == 1 && y == 1 {
                [0, 0, 0, 0]
            } else {
                [10, 20, 30, 255]
            }
        });
        let mut f = Frame::black();
        f.fill(Clip::SCREEN, [1, 2, 3]);
        f.draw(&Placed {
            rgba: &rgba,
            width: 2,
            height: 2,
            origin: (5.0, 7.0),
            scale: (1.0, 1.0),
            clip: Clip::SCREEN,
            tint: 1.0,
        });
        assert_eq!(at(&f, 5, 7), [10, 20, 30, 255]);
        assert_eq!(at(&f, 6, 8), [1, 2, 3, 255]);
        assert_eq!(at(&f, 4, 7), [1, 2, 3, 255]);
        assert_eq!(at(&f, 7, 7), [1, 2, 3, 255]);
    }

    #[test]
    fn scaling_repeats_texels_and_the_clip_cuts() {
        let rgba = pic(2, 1, |x, _| [x as u8 * 100, 0, 0, 255]);
        let mut f = Frame::black();
        f.draw(&Placed {
            rgba: &rgba,
            width: 2,
            height: 1,
            origin: (0.0, 0.0),
            scale: (2.0, 2.0),
            clip: Clip::new(0, 0, 3, 480),
            tint: 1.0,
        });
        assert_eq!(at(&f, 1, 1)[0], 0);
        assert_eq!(at(&f, 2, 1)[0], 100);
        assert_eq!(at(&f, 3, 1)[0], 0, "clipped");
        assert_eq!(at(&f, 2, 2), [0, 0, 0, 255], "below the picture");
    }

    #[test]
    fn a_tint_multiplies_in_linear_light() {
        let rgba = pic(1, 1, |_, _| [255, 128, 0, 255]);
        let mut f = Frame::black();
        f.draw(&Placed {
            rgba: &rgba,
            width: 1,
            height: 1,
            origin: (0.0, 0.0),
            scale: (1.0, 1.0),
            clip: Clip::SCREEN,
            tint: 0.5,
        });
        // 0.5 sRGB is about 0.214 linear: full white comes out as 0.5 sRGB again, and 128 (0.216 linear) as 0.046
        // linear, 61 in sRGB; a multiplication in sRGB would have given 64.
        assert_eq!(at(&f, 0, 0), [128, 61, 0, 255]);
    }

    #[test]
    fn the_hash_sees_every_pixel() {
        let a = Frame::black();
        let mut b = Frame::black();
        b.fill(Clip::new(639, 479, 1, 1), [0, 0, 1]);
        assert_ne!(a.hash(), b.hash());
    }
}
