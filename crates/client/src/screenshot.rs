//! Print Screen (spec `docs/specs/ui/palette-effects.md`, "Screenshot flash"): the screen is saved to the first unused
//! `screen000.pcx` to `screen999.pcx` (nothing happens when all 1000 exist), and while the file is written the screen
//! shows the palette in negative.
//!
//! The file is an 8-bit PCX, the spec's open-question default ("an 8-bit PCX of the 640 × 480 screen with the normal
//! palette"). The PCX layout follows ZSoft's published "PCX Technical Reference Manual" (version 5 header of 128 bytes,
//! run-length encoded scan lines with runs of up to 63 written as `0xC0 | count, value`, then `0x0C` and a 768-byte
//! palette); `tidewater_formats::pcx` reads it back in the tests.
//!
//! Choices where the spec leaves a gap:
//! - **Where.** The original writes into the game's folder; Tidewater never writes into the install, so the files go
//!   to the storage's (`storage`); the desktop's is the user folder (`storage::user_dir`: the `--save-dir` folder if
//!   given, else Tidewater's user data directory).
//! - **What.** The game screen as the window shows it, without the black border around it (`crate::display`): the
//!   composed 640 × 480 screen of the frame the key was pressed in. Each pixel is given the palette entry of its colour (the
//!   nearest entry for the interface's own colours, which aren't palette entries). The palette is the one the screen
//!   shows (grey while paused), not the negative. Only the game screen has a palette, so the key works there only.
//! - **Key.** Print Screen, and F13 (where Mac keyboards put it).
//! - **Flash length.** Writing takes less than a frame, so the negative shows for [`FLASH_FRAMES`] frames once the
//!   file is written.

use std::collections::HashMap;

use tidewater_formats::tileset::Palette;

use crate::core::Core;
use crate::input::Key;
use crate::palette::{self, Rgb};

/// Frames the negative palette shows after a screenshot is written.
pub const FLASH_FRAMES: u8 = 3;

/// A screenshot asked for this frame: written once the frame is composed.
pub struct PendingShot {
    name: String,
    canvas: (u32, u32),
    pal: Palette,
}

/// The screenshot key, in the game screen.
pub(crate) fn print_screen(c: &mut Core) {
    let Core {
        commands,
        cycling,
        hud,
        store,
        input,
        ..
    } = c;
    let keys = &input.keys;
    if !keys.any_just_pressed([Key::PrintScreen, Key::F(13)]) {
        return;
    }
    let (Some(cycling), Some(canvas)) = (cycling.as_ref(), hud.as_ref().and_then(|h| h.layout).map(|l| l.canvas))
    else {
        return;
    };
    let Some(name) = palette::screenshot_name(|n| store.has_screenshot(n)) else {
        return;
    };
    let shot = PendingShot {
        name,
        canvas,
        pal: cycling.shown.clone(),
    };
    commands.queue(move |c| c.pending_shot = Some(shot));
}

/// Writes the screenshot asked for from the frame just composed (the 640 × 480 screen, without the window's border),
/// and starts the flash.
pub(crate) fn write_shot(c: &mut Core) {
    let Some(shot) = c.pending_shot.take() else {
        return;
    };
    let frame = &c.frame;
    let px = index_pixels(
        &frame.rgba,
        (crate::display::SCREEN.0, crate::display::SCREEN.1),
        shot.canvas,
        &shot.pal,
    );
    let file = encode_pcx(shot.canvas.0 as u16, shot.canvas.1 as u16, &px, &shot.pal);
    match c.store.write_screenshot(&shot.name, &file) {
        Ok(()) => {
            if let Some(cycling) = &mut c.cycling {
                cycling.flash = FLASH_FRAMES;
            }
        }
        Err(e) => eprintln!("can't write {}: {e}", shot.name),
    }
}

/// The palette entry for each pixel of a `to`-sized picture sampled from an RGBA picture of size `from` (nearest
/// pixel; exact colours first, otherwise the nearest entry).
pub fn index_pixels(rgba: &[u8], from: (u32, u32), to: (u32, u32), pal: &Palette) -> Vec<u8> {
    let mut cache: HashMap<Rgb, u8> = HashMap::new();
    for (i, c) in pal.0.iter().enumerate().rev() {
        cache.insert(*c, i as u8);
    }
    let (fw, fh) = (from.0.max(1) as u64, from.1.max(1) as u64);
    let (tw, th) = (to.0.max(1) as u64, to.1.max(1) as u64);
    let mut out = Vec::with_capacity((tw * th) as usize);
    for y in 0..th {
        let sy = ((2 * y + 1) * fh / (2 * th)).min(fh - 1);
        for x in 0..tw {
            let sx = ((2 * x + 1) * fw / (2 * tw)).min(fw - 1);
            let at = ((sy * fw + sx) * 4) as usize;
            let c: Rgb = rgba.get(at..at + 3).map_or([0; 3], |s| [s[0], s[1], s[2]]);
            let i = *cache.entry(c).or_insert_with(|| nearest(pal, c));
            out.push(i);
        }
    }
    out
}

fn nearest(pal: &Palette, c: Rgb) -> u8 {
    let d = |p: &Rgb| (0..3).map(|k| (p[k] as i32 - c[k] as i32).pow(2)).sum::<i32>();
    (0..256).min_by_key(|&i| d(&pal.0[i])).unwrap_or(0) as u8
}

/// An 8-bit, one-plane PCX (version 5, run-length encoded) of `pixels` (`width` × `height` palette indices).
pub fn encode_pcx(width: u16, height: u16, pixels: &[u8], pal: &Palette) -> Vec<u8> {
    let (w, h) = (width.max(1), height.max(1));
    // Bytes per line must be even.
    let line = w as usize + (w as usize & 1);
    let mut out = vec![0u8; 128];
    out[0] = 0x0A; // manufacturer
    out[1] = 5; // version
    out[2] = 1; // run-length encoding
    out[3] = 8; // bits per pixel per plane
    let put16 = |o: &mut [u8], at: usize, v: u16| o[at..at + 2].copy_from_slice(&v.to_le_bytes());
    put16(&mut out, 8, w - 1); // x max (x min, y min are 0)
    put16(&mut out, 10, h - 1); // y max
    put16(&mut out, 12, 72); // dpi
    put16(&mut out, 14, 72);
    out[65] = 1; // planes
    put16(&mut out, 66, line as u16);
    put16(&mut out, 68, 1); // colour palette
    for y in 0..h as usize {
        let row: Vec<u8> = (0..line)
            .map(|x| {
                if x < w as usize {
                    pixels.get(y * w as usize + x).copied().unwrap_or(0)
                } else {
                    0
                }
            })
            .collect();
        let mut x = 0;
        while x < row.len() {
            let v = row[x];
            let mut n = 1;
            while x + n < row.len() && row[x + n] == v && n < 63 {
                n += 1;
            }
            if n > 1 || v >= 0xC0 {
                out.push(0xC0 | n as u8);
            }
            out.push(v);
            x += n;
        }
    }
    out.push(0x0C);
    for c in &pal.0 {
        out.extend_from_slice(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_formats::pcx::Pcx;

    fn pal() -> Palette {
        let mut p = Palette([[0; 3]; 256]);
        for (i, c) in p.0.iter_mut().enumerate() {
            *c = [i as u8, 255 - i as u8, (i * 7) as u8];
        }
        p
    }

    #[test]
    fn a_written_pcx_reads_back() {
        // An odd width (padded line), runs longer than 63 and values that need a run byte.
        let (w, h) = (67u16, 3u16);
        let mut px = vec![0xC5u8; w as usize * h as usize];
        px[w as usize] = 1;
        px[w as usize + 1] = 2;
        for (i, p) in px[2 * w as usize..].iter_mut().enumerate() {
            *p = i as u8;
        }
        let file = encode_pcx(w, h, &px, &pal());
        let back = Pcx::parse(&file).unwrap();
        assert_eq!((back.width, back.height), (w, h));
        assert_eq!(back.palette.as_ref().map(|p| p.0), Some(pal().0));
        // The reader may keep the padding byte of each line; compare the visible pixels.
        let line = back.pixels.len() / h as usize;
        for y in 0..h as usize {
            assert_eq!(
                &back.pixels[y * line..y * line + w as usize],
                &px[y * w as usize..(y + 1) * w as usize]
            );
        }
    }

    #[test]
    fn pixels_take_their_palette_entry_and_scale_down() {
        let p = pal();
        // A 4 × 2 picture at twice the size: entry 10 on the left half, entry 200 on the right, plus a colour not in
        // the palette next to entry 200's.
        let c10 = p.0[10];
        let c200 = p.0[200];
        let mut rgba = Vec::new();
        for _y in 0..2 {
            for x in 0..4 {
                let c = if x < 2 {
                    c10
                } else {
                    [c200[0], c200[1], c200[2].wrapping_add(1)]
                };
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        assert_eq!(index_pixels(&rgba, (4, 2), (2, 1), &p), vec![10, 200]);
    }

    #[test]
    fn the_name_is_the_first_unused() {
        let taken = ["screen000.pcx", "screen001.pcx"];
        assert_eq!(
            palette::screenshot_name(|n| taken.contains(&n)).as_deref(),
            Some("screen002.pcx")
        );
        assert_eq!(palette::screenshot_name(|_| true), None);
    }
}
