//! Turns a map file into ready-to-upload RGBA images: the terrain and one picture per placed unit. No Bevy types here,
//! so this stays testable and reusable by a headless renderer.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tidewater_formats::grp::Grp;
use tidewater_formats::install::Install;
use tidewater_formats::pud::{Era, Pud};
use tidewater_formats::sprites::unit_sheet_candidates;
use tidewater_formats::tileset::{Palette, TILE};
use tidewater_formats::udta::{UnitTable, flags};

/// An RGBA8 picture.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// A unit to draw, centred on `center` (map pixels, y down).
pub struct UnitSprite {
    pub center: (f32, f32),
    /// Key into [`Scene::pictures`].
    pub picture: (String, usize, u8),
    /// Draw order: buildings under units, air on top.
    pub layer: f32,
}

pub struct Scene {
    pub era: Era,
    pub description: String,
    pub terrain: Picture,
    /// (sheet, frame, owner) -> picture.
    pub pictures: HashMap<(String, usize, u8), Picture>,
    pub units: Vec<UnitSprite>,
}

/// Indices 208..=211 are the team colour ramp (red in the shipped palette). Other players get a design-choice ramp of
/// four shades, brightest first, until the game's own colour tables are decoded.
const TEAM: std::ops::RangeInclusive<u8> = 208..=211;
const PLAYER_COLOURS: [[u8; 3]; 8] = [
    [164, 0, 0],
    [0, 60, 192],
    [44, 180, 148],
    [152, 72, 176],
    [248, 140, 20],
    [40, 40, 60],
    [224, 224, 224],
    [252, 252, 72],
];

fn team_colour(owner: u8, shade: u8) -> [u8; 3] {
    let base = PLAYER_COLOURS[(owner as usize) % PLAYER_COLOURS.len()];
    // Shades step down by roughly a fifth, matching the look of the shipped red ramp.
    let k = [1.0, 0.78, 0.58, 0.40][shade as usize];
    base.map(|c| (c as f32 * k) as u8)
}

fn to_rgba(idx: &[u8], pal: &Palette, transparent0: bool, owner: Option<u8>) -> Vec<u8> {
    let mut out = Vec::with_capacity(idx.len() * 4);
    for &i in idx {
        let c = match owner {
            Some(o) if o != 0 && o < 8 && TEAM.contains(&i) => team_colour(o, i - TEAM.start()),
            _ => pal.0[i as usize],
        };
        out.extend([c[0], c[1], c[2], if transparent0 && i == 0 { 0 } else { 255 }]);
    }
    out
}

/// One frame placed in its sheet's full cell, so every frame of a sheet shares a centre.
fn frame_picture(grp: &Grp, frame: usize, pal: &Palette, owner: u8) -> Picture {
    let (w, h) = (grp.width as usize, grp.height as usize);
    let mut idx = vec![0u8; w * h];
    let f = &grp.frames[frame];
    for y in 0..f.height as usize {
        for x in 0..f.width as usize {
            let (dx, dy) = (f.x as usize + x, f.y as usize + y);
            let v = f.pixels[y * f.width as usize + x];
            if v != 0 && dx < w && dy < h {
                idx[dy * w + dx] = v;
            }
        }
    }
    Picture {
        width: w as u32,
        height: h as u32,
        rgba: to_rgba(&idx, pal, true, Some(owner)),
    }
}

pub fn load(install: &mut Install, map: &str) -> Result<Scene> {
    let data = if std::path::Path::new(map).is_file() {
        std::fs::read(map).with_context(|| format!("reading {map}"))?
    } else {
        install.read(map)?
    };
    let pud = Pud::parse(&data)?;
    let ts = install.tileset(pud.era)?;
    let (w, h) = (pud.width as usize, pud.height as usize);
    let (px, _) = ts.draw_map(&pud.tiles, w, h);
    let terrain = Picture {
        width: (w * TILE) as u32,
        height: (h * TILE) as u32,
        rgba: to_rgba(&px, &ts.palette, false, None),
    };

    let stats = match pud.section(b"UDTA").map(UnitTable::from_udta).transpose()?.flatten() {
        Some(t) => t,
        None => UnitTable::from_table(&install.read("rez\\unitdata.dat")?)?,
    };

    let mut sheets: HashMap<u8, Option<(String, Grp)>> = HashMap::new();
    let mut pictures = HashMap::new();
    let mut units = Vec::new();
    for u in &pud.units {
        let sheet = sheets.entry(u.kind).or_insert_with(|| {
            unit_sheet_candidates(u.kind, pud.era).into_iter().find_map(|name| {
                let g = Grp::parse(&install.read(&name).ok()?).ok()?;
                (!g.frames.is_empty()).then_some((name, g))
            })
        });
        let Some((name, grp)) = sheet else { continue };
        let st = stats.0.get(u.kind as usize).copied().unwrap_or_default();
        let building = st.flags & flags::BUILDING != 0 || st.size.0 > 1;
        // Units face south (frame 4 of the first facing group); buildings show their finished frame.
        let frame = if building || grp.frames.len() < 5 { 0 } else { 4 };
        let key = (name.clone(), frame, u.owner);
        pictures
            .entry(key.clone())
            .or_insert_with(|| frame_picture(grp, frame, &ts.palette, u.owner));
        let (sw, sh) = (st.size.0.max(1) as f32, st.size.1.max(1) as f32);
        let t = TILE as f32;
        let layer = if building {
            1.0
        } else if st.flags & flags::AIR != 0 {
            3.0
        } else {
            2.0
        };
        units.push(UnitSprite {
            center: ((u.x as f32 + sw / 2.0) * t, (u.y as f32 + sh / 2.0) * t),
            picture: key,
            layer,
        });
    }
    Ok(Scene {
        era: pud.era,
        description: pud.description(),
        terrain,
        pictures,
        units,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_colours_only_touch_the_ramp() {
        let mut pal = Palette([[0; 3]; 256]);
        pal.0[208] = [164, 0, 0];
        pal.0[50] = [1, 2, 3];
        let rgba = to_rgba(&[0, 50, 208], &pal, true, Some(1));
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..8], &[1, 2, 3, 255]);
        assert_eq!(&rgba[8..12], &[0, 60, 192, 255]);
        // Player 0 keeps the palette's own red.
        assert_eq!(&to_rgba(&[208], &pal, true, Some(0))[0..3], &[164, 0, 0]);
    }
}
