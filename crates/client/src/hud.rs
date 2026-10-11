//! The in-game interface, drawn in software into one RGBA picture that the client shows over the map: the stone
//! frame around the map view, the left panel with the minimap and the selected units, and the resource bar.
//!
//! Art comes from the installation (formats in the formats crate's `raw`, `grp` and `fnt` modules): the frame pieces
//! `Art\{h,o}_{top,btm,rgt,lft0,lft1,lft2}.bmp` and the info box `Art\{h,o}frame.grp` per race, the resource icons
//! `Art\icons.grp`, the unit icons `Art\unit\Portrait\*port*.grp` per era, the font `Art\font10x.fnt` and the unit
//! names in `rez\stat_txt.tbl` (string `kind + 1`). Pieces are placed as the 640x480 screen shows them: a 176-pixel
//! panel on the left, 16-pixel bars above, below and to the right of the map view. Larger canvases repeat the bars
//! and extend the panel. Positions inside the panel (icon, name, health bar, resource slots) and the text colours are
//! our own choices that follow the look of the game. No Bevy types here.

use anyhow::{Context, Result, bail};
use tidewater_formats::fnt::Font;
use tidewater_formats::grp::{Frame, Grp};
use tidewater_formats::install::Install;
use tidewater_formats::pud::Era;
use tidewater_formats::raw::RawImage;
use tidewater_formats::tbl::StringTable;
use tidewater_formats::tileset::Palette;

use crate::scene::{Picture, team_colour, to_rgba};

/// A rectangle in canvas pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x as f32
            && py >= self.y as f32
            && px < (self.x + self.w as i32) as f32
            && py < (self.y + self.h as i32) as f32
    }
}

/// Sizes of the frame pieces, read from the art.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pieces {
    pub panel_w: u32,
    /// Heights of the panel's menu strip, minimap box, info box and button area, top to bottom.
    pub panel_h: [u32; 4],
    /// Thickness of the top, bottom and right bars.
    pub bar: u32,
    /// The minimap's place inside the minimap box.
    pub hole: Rect,
}

/// Where everything goes on a canvas of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub canvas: (u32, u32),
    /// The map view.
    pub viewport: Rect,
    pub minimap: Rect,
    pub info: Rect,
    pub buttons: Rect,
    pub top: Rect,
}

impl Layout {
    pub fn new(canvas: (u32, u32), p: &Pieces) -> Self {
        let (cw, ch) = canvas;
        let y_info = (p.panel_h[0] + p.panel_h[1]) as i32;
        let y_buttons = y_info + p.panel_h[2] as i32;
        let vw = cw.saturating_sub(p.panel_w + p.bar).max(1);
        let vh = ch.saturating_sub(2 * p.bar).max(1);
        Self {
            canvas,
            viewport: Rect::new(p.panel_w as i32, p.bar as i32, vw, vh),
            minimap: Rect::new(p.hole.x, p.panel_h[0] as i32 + p.hole.y, p.hole.w, p.hole.h),
            info: Rect::new(0, y_info, p.panel_w, p.panel_h[2]),
            buttons: Rect::new(0, y_buttons, p.panel_w, p.panel_h[3]),
            top: Rect::new(p.panel_w as i32, 0, vw, p.bar),
        }
    }
}

/// Maps between minimap pixels and map tiles. The map is scaled to fit the minimap and centred in it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Minimap {
    pub rect: Rect,
    /// Map size in tiles.
    pub map: (u32, u32),
}

impl Minimap {
    /// Minimap pixels per tile.
    pub fn scale(&self) -> f32 {
        (self.rect.w as f32 / self.map.0.max(1) as f32).min(self.rect.h as f32 / self.map.1.max(1) as f32)
    }

    /// Canvas position of the map's top-left corner.
    fn origin(&self) -> (f32, f32) {
        let s = self.scale();
        (
            self.rect.x as f32 + (self.rect.w as f32 - self.map.0 as f32 * s) / 2.0,
            self.rect.y as f32 + (self.rect.h as f32 - self.map.1 as f32 * s) / 2.0,
        )
    }

    /// The tile position (fractional, clamped to the map) under canvas point `p`.
    pub fn tile_at(&self, p: (f32, f32)) -> (f32, f32) {
        let (ox, oy, s) = (self.origin().0, self.origin().1, self.scale());
        (
            ((p.0 - ox) / s).clamp(0.0, self.map.0 as f32),
            ((p.1 - oy) / s).clamp(0.0, self.map.1 as f32),
        )
    }

    /// The canvas point of tile position `t`.
    pub fn canvas_at(&self, t: (f32, f32)) -> (f32, f32) {
        let (ox, oy, s) = (self.origin().0, self.origin().1, self.scale());
        (ox + t.0 * s, oy + t.1 * s)
    }
}

/// The average colour of each tile of a terrain picture `tiles_w * tiles_h` tiles of `tile` pixels.
pub fn tile_colours(terrain: &Picture, tile: u32) -> Vec<[u8; 3]> {
    let (tw, th) = (terrain.width / tile, terrain.height / tile);
    let mut out = Vec::with_capacity((tw * th) as usize);
    for ty in 0..th {
        for tx in 0..tw {
            let mut sum = [0u32; 3];
            for y in 0..tile {
                let row = ((ty * tile + y) * terrain.width + tx * tile) as usize * 4;
                for x in 0..tile as usize {
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += terrain.rgba[row + x * 4 + c] as u32;
                    }
                }
            }
            let n = tile * tile;
            out.push(sum.map(|s| (s / n) as u8));
        }
    }
    out
}

/// A unit as the minimap shows it.
#[derive(Clone, Copy, Debug)]
pub struct Dot {
    pub tile: (i32, i32),
    pub size: (u8, u8),
    pub owner: u8,
}

/// A selected unit as the info box shows it.
#[derive(Clone, Copy, Debug)]
pub struct Selected {
    pub kind: u8,
    pub owner: u8,
    pub hp: u16,
    pub max_hp: u16,
}

/// The changing state the interface shows.
pub struct View<'a> {
    pub gold: u32,
    pub lumber: u32,
    pub oil: u32,
    pub selected: &'a [Selected],
    pub dots: &'a [Dot],
    /// The map view in tiles: left, top, right, bottom.
    pub view: (f32, f32, f32, f32),
}

// --- drawing primitives -------------------------------------------------------------------------------------------

/// Copies `src` to `(x, y)`, skipping fully transparent pixels and clipping to `dst`.
pub fn blit(dst: &mut Picture, x: i32, y: i32, src: &Picture) {
    for sy in 0..src.height as i32 {
        let dy = y + sy;
        if dy < 0 || dy >= dst.height as i32 {
            continue;
        }
        for sx in 0..src.width as i32 {
            let dx = x + sx;
            if dx < 0 || dx >= dst.width as i32 {
                continue;
            }
            let s = ((sy as u32 * src.width + sx as u32) * 4) as usize;
            if src.rgba[s + 3] == 0 {
                continue;
            }
            let d = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
            dst.rgba[d..d + 4].copy_from_slice(&src.rgba[s..s + 4]);
        }
    }
}

/// Fills a rectangle, clipped to `dst`.
pub fn fill(dst: &mut Picture, r: Rect, c: [u8; 3]) {
    let x0 = r.x.max(0) as u32;
    let y0 = r.y.max(0) as u32;
    let x1 = ((r.x + r.w as i32).max(0) as u32).min(dst.width);
    let y1 = ((r.y + r.h as i32).max(0) as u32).min(dst.height);
    for y in y0..y1 {
        for x in x0..x1 {
            let d = ((y * dst.width + x) * 4) as usize;
            dst.rgba[d..d + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

/// A one-pixel outline.
pub fn outline(dst: &mut Picture, r: Rect, c: [u8; 3]) {
    if r.w == 0 || r.h == 0 {
        return;
    }
    fill(dst, Rect::new(r.x, r.y, r.w, 1), c);
    fill(dst, Rect::new(r.x, r.y + r.h as i32 - 1, r.w, 1), c);
    fill(dst, Rect::new(r.x, r.y, 1, r.h), c);
    fill(dst, Rect::new(r.x + r.w as i32 - 1, r.y, 1, r.h), c);
}

/// Text colours by font shade (see `fnt`): a light face fading to grey, and a black shadow.
const TEXT: [[u8; 3]; 8] = [
    [0, 0, 0],
    [252, 252, 236],
    [216, 212, 192],
    [176, 172, 152],
    [136, 132, 116],
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
];

/// Draws `text` with its top-left at `(x, y)` and returns its width.
pub fn text(dst: &mut Picture, font: &Font, x: i32, y: i32, s: &str) -> u32 {
    let mut pen = x;
    for c in s.bytes() {
        if let Some(g) = font.glyph(c) {
            for gy in 0..g.height as i32 {
                for gx in 0..g.width as i32 {
                    let shade = g.shades[(gy * g.width as i32 + gx) as usize];
                    if shade == 0 {
                        continue;
                    }
                    let (dx, dy) = (pen + g.x as i32 + gx, y + g.y as i32 + gy);
                    if dx >= 0 && dy >= 0 && (dx as u32) < dst.width && (dy as u32) < dst.height {
                        let d = ((dy as u32 * dst.width + dx as u32) * 4) as usize;
                        let col = TEXT[shade as usize];
                        dst.rgba[d..d + 4].copy_from_slice(&[col[0], col[1], col[2], 255]);
                    }
                }
            }
        }
        pen += font.advance(c) as i32;
    }
    (pen - x) as u32
}

/// Splits `s` into at most two lines that each fit `max` pixels where possible, breaking at a space.
pub fn wrap2(font: &Font, s: &str, max: u32) -> Vec<String> {
    if font.width(s.as_bytes()) <= max {
        return vec![s.to_string()];
    }
    // The last space that leaves a fitting first line, else the first space.
    let spaces: Vec<usize> = s.match_indices(' ').map(|(i, _)| i).collect();
    let cut = spaces
        .iter()
        .rev()
        .find(|&&i| font.width(&s.as_bytes()[..i]) <= max)
        .or(spaces.first());
    match cut {
        Some(&i) => vec![s[..i].to_string(), s[i + 1..].to_string()],
        None => vec![s.to_string()],
    }
}

fn indexed(width: u32, height: u32, idx: &[u8], pal: &Palette, transparent0: bool, owner: Option<u8>) -> Picture {
    Picture {
        width,
        height,
        rgba: to_rgba(idx, pal, transparent0, owner),
    }
}

fn raw_picture(r: &RawImage, pal: &Palette) -> Picture {
    indexed(r.width as u32, r.height as u32, &r.pixels, pal, false, None)
}

fn frame_picture(f: &Frame, pal: &Palette, owner: Option<u8>) -> Picture {
    let w = (f.pixels.len() / f.height.max(1) as usize) as u32;
    indexed(w, f.height as u32, &f.pixels, pal, true, owner)
}

/// The unit icon frame (in the era's `portrait` sheet) of each unit type, found by looking at the sheet. Heroes
/// without a recognisable portrait use their base unit's icon. `None` for unused types and start locations.
pub fn unit_icon(kind: u8) -> Option<u16> {
    #[rustfmt::skip]
    const ICONS: [i16; 104] = [
        // 0-19: footman, grunt, peasant, peon, ballista, catapult, knight, ogre, archer, axethrower, mage,
        // death knight, paladin, ogre-mage, dwarves, sappers, attack peasant, attack peon, ranger, berserker
        2, 3, 0, 1, 16, 17, 8, 9, 4, 5, 14, 15, 10, 11, 12, 13, 0, 1, 6, 7,
        // 20-39: Alleria, Teron, Kurdran, Dentarg, Khadgar, Grom, tankers, transports, destroyers, battleship,
        // juggernaught, -, Deathwing, -, -, submarine, turtle
        187, 15, 30, 11, 14, 3, 18, 19, 20, 21, 22, 23, 24, 25, -1, 192, -1, -1, 26, 27,
        // 40-57: flying machine, zeppelin, gryphon rider, dragon, Turalyon, eye of Kilrogg, Danath, Korgath, -,
        // Cho'gall, Lothar, Gul'dan, Uther, Zuljin, -, skeleton, daemon, critter
        28, 29, 30, 31, 8, 111, 2, 3, -1, 194, 8, 15, 10, 5, -1, 114, 37, 115,
        // 58-79: farms, barracks, church, altar, towers, stables, mound, inventor, alchemist, aviary, roost,
        // shipyards, halls, lumber mills, foundries
        38, 39, 42, 43, 62, 63, 60, 61, 56, 57, 58, 59, 72, 73, 48, 49, 40, 41, 44, 45, 50, 51,
        // 80-103: mage tower, temple, blacksmiths, refineries, platforms, keep, stronghold, castle, fortress, mine,
        // oil patch, start locations, guard towers, cannon towers, circle, portal, runestone, wall
        64, 65, 46, 47, 52, 53, 54, 55, 68, 69, 70, 71, 74, 79, -1, -1, 75, 77, 76, 78, 81, 80, 82, 92,
    ];
    ICONS.get(kind as usize).and_then(|&i| u16::try_from(i).ok())
}

// --- the art ------------------------------------------------------------------------------------------------------

pub struct HudArt {
    pal: Palette,
    top: Picture,
    bottom: Picture,
    right: Picture,
    /// Menu strip, minimap box, button area.
    left: [Picture; 3],
    /// Info box: plain, framed.
    info: [Picture; 2],
    menu_button: Option<Picture>,
    /// Gold, lumber, oil.
    resources: Vec<Picture>,
    icons: Grp,
    font: Font,
    names: StringTable,
    pub pieces: Pieces,
}

/// The unit icon sheet of an era.
fn portrait_sheet(era: Era) -> &'static str {
    match era {
        Era::Forest => "Art\\unit\\Portrait\\portrait.grp",
        Era::Winter => "Art\\unit\\Portrait\\s_port.grp",
        Era::Wasteland => "Art\\unit\\Portrait\\l_port.grp",
        Era::Swamp => "Art\\unit\\Portrait\\x_port.grp",
    }
}

impl HudArt {
    /// Loads the interface art for a race (0 human, otherwise orc) with an era's palette.
    pub fn load(install: &mut Install, era: Era, race: u8, pal: &Palette) -> Result<Self> {
        let p = if race == 0 { "h" } else { "o" };
        let mut raw = |name: &str| -> Result<RawImage> {
            let path = format!("Art\\{p}_{name}.bmp");
            RawImage::parse(&install.read(&path)?).with_context(|| path.clone())
        };
        let (top, bottom, right) = (raw("top")?, raw("btm")?, raw("rgt")?);
        let left = [raw("lft0")?, raw("lft1")?, raw("lft2")?];
        let frame_name = format!("Art\\{p}frame.grp");
        let frames = Grp::parse_uncompressed(&install.read(&frame_name)?).context(frame_name)?;
        if frames.frames.len() < 2 {
            bail!("info box sheet has {} frames", frames.frames.len());
        }
        let race_sheet = if race == 0 { "art\\human.grp" } else { "art\\orc.grp" };
        let menu_button = install
            .read(race_sheet)
            .ok()
            .and_then(|d| Grp::parse_uncompressed(&d).ok())
            .and_then(|g| g.frames.get(48).map(|f| frame_picture(f, pal, None)));
        let res = Grp::parse_uncompressed(&install.read("Art\\icons.grp")?).context("icons.grp")?;
        let icons = Grp::parse(&install.read(portrait_sheet(era))?).context("portrait sheet")?;
        let font = Font::parse(&install.read("Art\\font10x.fnt")?).context("font10x.fnt")?;
        let names = StringTable::parse(&install.read("rez\\stat_txt.tbl")?).context("stat_txt.tbl")?;

        // The minimap goes where the minimap box is transparent black (index 0).
        let hole = bounds_of(&left[1], 0).unwrap_or(Rect::new(24, 2, 128, 128));
        let info_h = frames.height as u32;
        let pieces = Pieces {
            panel_w: left[0].width as u32,
            panel_h: [
                left[0].height as u32,
                left[1].height as u32,
                info_h,
                left[2].height as u32,
            ],
            bar: top.height as u32,
            hole,
        };
        Ok(Self {
            top: raw_picture(&top, pal),
            bottom: raw_picture(&bottom, pal),
            right: raw_picture(&right, pal),
            left: left.map(|r| raw_picture(&r, pal)),
            info: [
                frame_picture(&frames.frames[0], pal, None),
                frame_picture(&frames.frames[1], pal, None),
            ],
            menu_button,
            resources: res.frames.iter().take(3).map(|f| frame_picture(f, pal, None)).collect(),
            icons,
            font,
            names,
            pal: pal.clone(),
            pieces,
        })
    }

    pub fn layout(&self, canvas: (u32, u32)) -> Layout {
        Layout::new(canvas, &self.pieces)
    }

    /// The unit type's name from the game's own table.
    pub fn name(&self, kind: u8) -> String {
        self.names.text(kind as usize + 1).unwrap_or_default()
    }

    /// The parts that only change with the canvas size; the map view is left transparent.
    pub fn frame(&self, l: &Layout) -> Picture {
        let (cw, ch) = l.canvas;
        let mut pic = Picture {
            width: cw,
            height: ch,
            rgba: vec![0; (cw * ch * 4) as usize],
        };
        let px = l.viewport.x;
        let right_x = l.viewport.x + l.viewport.w as i32;
        // Bars: repeat each piece along its length.
        let mut x = px;
        while x < right_x {
            blit(&mut pic, x, 0, &self.top);
            blit(&mut pic, x, ch as i32 - self.bottom.height as i32, &self.bottom);
            x += self.top.width.max(1) as i32;
        }
        let mut y = 0;
        while y < ch as i32 {
            blit(&mut pic, right_x, y, &self.right);
            y += self.right.height.max(1) as i32;
        }
        // The panel, then plain info-box stone below it on tall canvases.
        blit(&mut pic, 0, 0, &self.left[0]);
        blit(&mut pic, 0, self.pieces.panel_h[0] as i32, &self.left[1]);
        blit(&mut pic, l.info.x, l.info.y, &self.info[1]);
        blit(&mut pic, l.buttons.x, l.buttons.y, &self.left[2]);
        let mut y = l.buttons.y + l.buttons.h as i32;
        while y < ch as i32 {
            blit(&mut pic, 0, y, &self.info[0]);
            y += self.info[0].height.max(1) as i32;
        }
        if let Some(menu) = &self.menu_button {
            let bx = (self.pieces.panel_w as i32 - menu.width as i32) / 2;
            let by = (self.pieces.panel_h[0] as i32 - menu.height as i32) / 2;
            blit(&mut pic, bx, by, menu);
            let label = "Menu (F10)";
            let tw = self.font.width(label.as_bytes()) as i32;
            text(&mut pic, &self.font, bx + (menu.width as i32 - tw) / 2, by + 1, label);
        }
        pic
    }

    fn icon(&self, kind: u8, owner: u8) -> Option<Picture> {
        let f = self.icons.frames.get(unit_icon(kind)? as usize)?;
        Some(frame_picture(f, &self.pal, Some(owner)))
    }

    /// Draws an icon with a bevelled border and its health bar below.
    fn icon_box(&self, pic: &mut Picture, x: i32, y: i32, s: &Selected) {
        let (w, h) = (46u32, 38u32);
        fill(pic, Rect::new(x - 2, y - 2, w + 4, h + 4), [24, 24, 24]);
        outline(pic, Rect::new(x - 2, y - 2, w + 4, h + 4), [140, 120, 60]);
        if let Some(icon) = self.icon(s.kind, s.owner) {
            blit(pic, x, y, &icon);
        }
        let bar = Rect::new(x, y + h as i32 + 3, w, 3);
        fill(pic, bar, [0, 0, 0]);
        let frac = if s.max_hp == 0 {
            1.0
        } else {
            (s.hp as f32 / s.max_hp as f32).clamp(0.0, 1.0)
        };
        let colour = if frac > 0.5 {
            [0, 200, 0]
        } else if frac > 0.25 {
            [220, 200, 0]
        } else {
            [200, 0, 0]
        };
        fill(
            pic,
            Rect::new(bar.x, bar.y, (w as f32 * frac).round() as u32, bar.h),
            colour,
        );
    }

    /// The whole interface: `frame` plus the minimap, resources and selection.
    pub fn draw(&self, frame: &Picture, l: &Layout, tiles: &[[u8; 3]], map: (u32, u32), v: &View) -> Picture {
        let mut pic = Picture {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba.clone(),
        };
        draw_minimap(&mut pic, &Minimap { rect: l.minimap, map }, tiles, v.dots, v.view);

        // Resources, right-aligned in the top bar.
        let slot = 84;
        let mut x = l.top.x + l.top.w as i32 - 3 * slot - 4;
        for (icon, amount) in self.resources.iter().zip([v.gold, v.lumber, v.oil]) {
            blit(&mut pic, x, l.top.y + 1, icon);
            text(
                &mut pic,
                &self.font,
                x + icon.width as i32 + 4,
                l.top.y + 1,
                &amount.to_string(),
            );
            x += slot;
        }

        // Selection.
        let info = l.info;
        match v.selected {
            [] => {}
            [s] => {
                let (ix, iy) = (info.x + 12, info.y + 12);
                self.icon_box(&mut pic, ix, iy, s);
                let name = self.name(s.kind);
                let tx = ix + 46 + 8;
                for (i, line) in wrap2(&self.font, &name, info.w - (tx - info.x) as u32 - 8)
                    .iter()
                    .enumerate()
                {
                    text(&mut pic, &self.font, tx, iy + 2 + i as i32 * 13, line);
                }
                let hp = format!("{}/{}", s.hp, s.max_hp);
                let tw = self.font.width(hp.as_bytes()) as i32;
                text(&mut pic, &self.font, ix + (46 - tw) / 2, iy + 38 + 8, &hp);
            }
            many => {
                for (i, s) in many.iter().take(9).enumerate() {
                    let (c, r) = ((i % 3) as i32, (i / 3) as i32);
                    self.icon_box(&mut pic, info.x + 10 + c * 54, info.y + 10 + r * 52, s);
                }
            }
        }
        pic
    }
}

/// The bounding box of the pixels of index `v`.
fn bounds_of(r: &RawImage, v: u8) -> Option<Rect> {
    let (w, h) = (r.width as usize, r.height as usize);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if r.pixels[y * w + x] == v {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    (x0 != usize::MAX).then(|| Rect::new(x0 as i32, y0 as i32, (x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32))
}

/// Draws the terrain, unit dots and the map-view rectangle into the minimap area.
pub fn draw_minimap(pic: &mut Picture, m: &Minimap, tiles: &[[u8; 3]], dots: &[Dot], view: (f32, f32, f32, f32)) {
    let s = m.scale();
    let (mw, mh) = m.map;
    fill(pic, m.rect, [0, 0, 0]);
    let (ox, oy) = m.canvas_at((0.0, 0.0));
    let (w, h) = ((mw as f32 * s).round() as i32, (mh as f32 * s).round() as i32);
    for py in 0..h {
        let ty = (((py as f32 + 0.5) / s) as u32).min(mh.saturating_sub(1));
        for px in 0..w {
            let tx = (((px as f32 + 0.5) / s) as u32).min(mw.saturating_sub(1));
            if let Some(&c) = tiles.get((ty * mw + tx) as usize) {
                fill(pic, Rect::new(ox as i32 + px, oy as i32 + py, 1, 1), c);
            }
        }
    }
    for d in dots {
        let (x, y) = m.canvas_at((d.tile.0 as f32, d.tile.1 as f32));
        let size = |n: u8| ((n as f32 * s).round() as u32).max(1);
        fill(
            pic,
            Rect::new(x as i32, y as i32, size(d.size.0), size(d.size.1)),
            team_colour(d.owner, 0),
        );
    }
    let (x0, y0) = m.canvas_at((view.0.max(0.0), view.1.max(0.0)));
    let (x1, y1) = m.canvas_at((view.2.min(mw as f32), view.3.min(mh as f32)));
    let r = Rect::new(
        x0 as i32,
        y0 as i32,
        ((x1 - x0) as u32).max(1),
        ((y1 - y0) as u32).max(1),
    );
    outline(pic, r, [252, 252, 252]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces() -> Pieces {
        Pieces {
            panel_w: 176,
            panel_h: [24, 136, 176, 144],
            bar: 16,
            hole: Rect::new(25, 3, 126, 126),
        }
    }

    #[test]
    fn layout_of_the_original_screen() {
        let l = Layout::new((640, 480), &pieces());
        assert_eq!(l.viewport, Rect::new(176, 16, 448, 448));
        assert_eq!(l.minimap, Rect::new(25, 27, 126, 126));
        assert_eq!(l.info, Rect::new(0, 160, 176, 176));
        assert_eq!(l.buttons, Rect::new(0, 336, 176, 144));
        assert_eq!(l.top, Rect::new(176, 0, 448, 16));
        assert!(l.viewport.contains(176.0, 16.0));
        assert!(!l.viewport.contains(175.9, 100.0));
        assert!(!l.viewport.contains(624.0, 100.0));
    }

    #[test]
    fn minimap_maps_both_ways_and_centres_wide_maps() {
        let m = Minimap {
            rect: Rect::new(10, 20, 128, 128),
            map: (64, 32),
        };
        assert_eq!(m.scale(), 2.0);
        // 64 pixels tall, centred vertically: top at 20 + 32.
        assert_eq!(m.canvas_at((0.0, 0.0)), (10.0, 52.0));
        assert_eq!(m.tile_at((10.0 + 20.0, 52.0 + 8.0)), (10.0, 4.0));
        // Outside the map clamps to its edge.
        assert_eq!(m.tile_at((0.0, 0.0)), (0.0, 0.0));
        assert_eq!(m.tile_at((500.0, 500.0)), (64.0, 32.0));
    }

    #[test]
    fn tile_colours_average_each_tile() {
        // Two 2x2 tiles side by side: left all 10, right alternating 0 and 100.
        let mut rgba = Vec::new();
        for _ in 0..2 {
            for v in [10, 10, 0, 100] {
                rgba.extend([v, v, v, 255]);
            }
        }
        let pic = Picture {
            width: 4,
            height: 2,
            rgba,
        };
        assert_eq!(tile_colours(&pic, 2), vec![[10; 3], [50; 3]]);
    }

    #[test]
    fn minimap_draws_terrain_dots_and_view() {
        let mut pic = Picture {
            width: 8,
            height: 8,
            rgba: vec![0; 256],
        };
        let m = Minimap {
            rect: Rect::new(0, 0, 8, 8),
            map: (4, 4),
        };
        let tiles = vec![[1, 2, 3]; 16];
        let dots = [Dot {
            tile: (3, 3),
            size: (1, 1),
            owner: 1,
        }];
        draw_minimap(&mut pic, &m, &tiles, &dots, (0.0, 0.0, 2.0, 2.0));
        let at = |x: u32, y: u32| &pic.rgba[((y * 8 + x) * 4) as usize..((y * 8 + x) * 4 + 3) as usize];
        assert_eq!(at(5, 1), &[1, 2, 3]);
        assert_eq!(at(7, 7), &team_colour(1, 0));
        assert_eq!(at(6, 6), &team_colour(1, 0));
        // The view outline covers tiles 0..2, i.e. pixels 0..4.
        assert_eq!(at(0, 0), &[252, 252, 252]);
        assert_eq!(at(3, 2), &[252, 252, 252]);
        assert_eq!(at(2, 2), &[1, 2, 3]);
    }

    #[test]
    fn blit_skips_transparent_and_clips() {
        let mut dst = Picture {
            width: 2,
            height: 1,
            rgba: vec![9; 8],
        };
        let src = Picture {
            width: 2,
            height: 1,
            rgba: vec![1, 1, 1, 255, 2, 2, 2, 0],
        };
        blit(&mut dst, 1, 0, &src);
        assert_eq!(dst.rgba, vec![9, 9, 9, 9, 1, 1, 1, 255]);
        blit(&mut dst, -1, 0, &src);
        assert_eq!(dst.rgba, vec![9, 9, 9, 9, 1, 1, 1, 255]);
    }

    #[test]
    fn icons_cover_the_trainable_units() {
        assert_eq!(unit_icon(0), Some(2));
        assert_eq!(unit_icon(2), Some(0));
        assert_eq!(unit_icon(74), Some(40));
        assert_eq!(unit_icon(94), None);
        assert_eq!(unit_icon(200), None);
        for kind in (0..20).chain(26..34).chain(38..44).chain(58..94) {
            assert!(unit_icon(kind).is_some(), "type {kind}");
        }
    }

    #[test]
    fn long_names_wrap_at_a_space() {
        // A font where every character is 4 pixels wide (3 + 1 spacing).
        let mut d = b"FONT".to_vec();
        d.extend([b'a', b'z', 3, 1]);
        let first = 8 + 26 * 4;
        for i in 0..26u32 {
            d.extend((first as u32 + i * 7).to_le_bytes());
        }
        for _ in 0..26 {
            d.extend([3, 1, 0, 0, 1, 1, 1]);
        }
        let font = Font::parse(&d).unwrap();
        assert_eq!(wrap2(&font, "ab cd", 40), vec!["ab cd"]);
        assert_eq!(wrap2(&font, "ab cd ef", 22), vec!["ab cd", "ef"]);
        assert_eq!(wrap2(&font, "abcdefgh", 8), vec!["abcdefgh"]);
    }
}
