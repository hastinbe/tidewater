//! Turns a map file into the starting [`World`] plus ready-to-upload RGBA images: the terrain and, per unit type and
//! owner, the frames the client shows. No Bevy types here, so this stays testable and reusable by a headless renderer.

use std::collections::HashMap;

use anyhow::{Context, Result};
use tidewater_formats::grp::Grp;
use tidewater_formats::install::Install;
use tidewater_formats::pud::{Era, Pud};
use tidewater_formats::script::Scripts;
use tidewater_formats::sprites::unit_sheet_candidates;
use tidewater_formats::tileset::{Palette, TILE, Tileset};
use tidewater_formats::udta::UnitTable;
use tidewater_sim::World;

/// An RGBA8 picture.
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Scene {
    pub era: Era,
    pub description: String,
    pub terrain: Picture,
    /// Average colour of each map tile, row by row (for the minimap).
    pub tile_colours: Vec<[u8; 3]>,
    /// The era's palette, which the interface art uses too.
    pub palette: Palette,
    /// For redrawing tiles whose shape changes in play.
    pub tileset: Tileset,
    /// Each unit type's sprite sheet, for the types that have one.
    pub sheets: HashMap<u8, Grp>,
    pub world: World,
    /// The player the local user controls (game setup's local player).
    pub human: u8,
    /// The game's seed, which also starts the sound-variant generator (`client::sound`).
    pub seed: u32,
}

/// The sheet frame and horizontal flip that draw a unit: its animation frame, plus the facing column for units
/// (sheets hold 5 facings per step). Buildings and ruins draw the animation frame as it is. Out of range falls back
/// to the facing column alone.
pub fn sprite_frame(u: &tidewater_sim::Unit, frames: usize) -> (usize, bool) {
    if u.building || (0x6a..=0x6d).contains(&u.kind) || frames < 5 {
        let f = u.script.frame as usize;
        return (if f < frames { f } else { 0 }, false);
    }
    let (col, flip) = facing_frame(u.facing);
    let f = u.script.frame as usize + col;
    (if f < frames { f } else { col }, flip)
}

/// The frame and horizontal flip that show `facing` (0 north, clockwise in eighths).
pub fn facing_frame(facing: u8) -> (usize, bool) {
    match facing & 7 {
        f @ 0..=4 => (f as usize, false),
        f => ((8 - f) as usize, true),
    }
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

pub fn team_colour(owner: u8, shade: u8) -> [u8; 3] {
    let base = PLAYER_COLOURS[(owner as usize) % PLAYER_COLOURS.len()];
    // Shades step down by roughly a fifth, matching the look of the shipped red ramp.
    let k = [1.0, 0.78, 0.58, 0.40][shade as usize];
    base.map(|c| (c as f32 * k) as u8)
}

pub fn to_rgba(idx: &[u8], pal: &Palette, transparent0: bool, owner: Option<u8>) -> Vec<u8> {
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
pub fn frame_picture(grp: &Grp, frame: usize, pal: &Palette, owner: u8) -> Picture {
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

/// One megatile as a `TILE` x `TILE` picture.
pub fn tile_picture(ts: &Tileset, megatile: usize) -> Picture {
    let mut px = vec![0u8; TILE * TILE];
    if megatile < ts.megatiles.len() {
        ts.draw_megatile(megatile, &mut px, TILE);
    }
    Picture {
        width: TILE as u32,
        height: TILE as u32,
        rgba: to_rgba(&px, &ts.palette, false, None),
    }
}

/// Loads `map` and starts a game on it with the options `setup` (`docs/specs/game/setup.md`).
pub fn load(install: &mut Install, map: &str, setup: &tidewater_sim::setup::Setup) -> Result<Scene> {
    let data = if std::path::Path::new(map).is_file() {
        std::fs::read(map).with_context(|| format!("reading {map}"))?
    } else {
        install.read(map)?
    };
    let pud = Pud::parse(&data)?;
    // Single player seeds with the time in whole seconds (timing-and-rng spec, "Seeding"); slot setup draws from it.
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as u32);
    let start = tidewater_sim::setup::Start::choose(&pud, setup, seed)?;
    let era = start.era;
    let ts = install.tileset(era)?;
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
    let scripts = Scripts::parse(install.read("rez\\script.bin")?)?;
    let upgrades = tidewater_formats::ugrd::UpgradeTable::from_table(&install.read("rez\\upgrades.dat")?)?;
    let mut world = World::from_pud(&pud, &ts, stats, upgrades, std::sync::Arc::new(scripts), &start)?;
    let ai = tidewater_formats::aibin::AiFile::parse(install.read("rez\\ai.bin")?)?;
    world.start_ai(std::sync::Arc::new(ai));

    let sheets = (0..tidewater_formats::script::UNIT_TYPES as u8)
        .filter_map(|k| {
            unit_sheet_candidates(k, era).into_iter().find_map(|name| {
                let g = Grp::parse(&install.read(&name).ok()?).ok()?;
                (!g.frames.is_empty()).then_some((k, g))
            })
        })
        .collect();
    let human = start.local;
    Ok(Scene {
        era,
        description: pud.description(),
        tile_colours: crate::hud::tile_colours(&terrain, TILE as u32),
        terrain,
        palette: ts.palette.clone(),
        tileset: ts,
        sheets,
        world,
        human,
        seed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facings_mirror_the_west_side() {
        assert_eq!(facing_frame(0), (0, false));
        assert_eq!(facing_frame(4), (4, false));
        assert_eq!(facing_frame(5), (3, true));
        assert_eq!(facing_frame(7), (1, true));
    }

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
