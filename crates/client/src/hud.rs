//! The in-game interface, drawn in software into one RGBA picture that the client shows over the map: the stone
//! frame around the map view, the left panel with the minimap and the selected units, and the resource bar.
//!
//! Art comes from the installation (formats in the formats crate's `raw`, `grp` and `fnt` modules): the frame pieces
//! `Art\{h,o}_{top,btm,rgt,lft0,lft1,lft2}.bmp` and the info box `Art\{h,o}frame.grp` per race, the resource icons
//! `Art\icons.grp`, the unit icons `Art\unit\Portrait\*port*.grp` per era, the font `Art\font10x.fnt` and the
//! strings in `rez\stat_txt.tbl`. Pieces are placed as the 640x480 screen shows them: a 176-pixel panel on the left,
//! 16-pixel bars above, below and to the right of the map view. Larger canvases repeat the bars and extend the panel.
//! The information panel follows `docs/specs/ui/info-panel.md` (layout and rules in [`crate::info`]); the resource
//! slots, button frames and text colours are our own choices that follow the look of the game. No Bevy types here.

use anyhow::{Context, Result, bail};
use tidewater_formats::fnt::Font;
use tidewater_formats::grp::{Frame, Grp};
use tidewater_formats::install::Install;
use tidewater_formats::pud::Era;
use tidewater_formats::raw::RawImage;
use tidewater_formats::tbl::StringTable;
use tidewater_formats::tileset::Palette;

use crate::scene::{Picture, to_rgba};

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
    /// The bottom bar, where messages go.
    pub bottom: Rect,
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
            bottom: Rect::new(p.panel_w as i32, (ch.max(p.bar) - p.bar) as i32, vw, p.bar),
        }
    }
}

/// Maps between minimap pixels and map tiles for the current mouse input, which scales the map to fit the minimap and
/// centres it. The spec's mapping is [`crate::minimap::view_for_press`] and [`crate::minimap::tile_for_press`], which
/// the input rewrite takes over.
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
}

/// A unit in the information panel's grid: its type, owner and health.
#[derive(Clone, Copy, Debug)]
pub struct Icon {
    pub kind: u8,
    pub owner: u8,
    pub hp: u16,
    /// The type's maximum hit points.
    pub max_hp: u16,
}

/// What the information panel shows (see [`crate::info`]).
#[derive(Clone, Copy, Debug, Default)]
pub struct InfoView<'a> {
    /// Background frame 0 to 3.
    pub frame: u8,
    pub slots: [Option<Icon>; 9],
    /// The slot whose icon is held down.
    pub pressed: Option<usize>,
    /// The text for one selected unit.
    pub content: Option<&'a crate::info::Content>,
    /// The local player, whose colour the production icon takes.
    pub local: u8,
}

/// The changing state the interface shows.
pub struct View<'a> {
    pub gold: u32,
    pub lumber: u32,
    pub oil: u32,
    pub info: InfoView<'a>,
    /// The latest message for the player.
    pub status: Option<&'a str>,
    /// The last minimap picture drawn: 128 × 128 palette indices (see [`crate::minimap`]).
    pub minimap: &'a [u8],
    /// The command panel's shown buttons: slot and icon frame (see [`crate::panel`]).
    pub buttons: &'a [(u8, u16)],
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

/// The font's alternate colour (colour code 4), used for bonuses: a yellow face. Our own choice; the spec names the
/// code, not its colours.
const TEXT_ALT: [[u8; 3]; 8] = [
    [0, 0, 0],
    [252, 224, 64],
    [220, 188, 48],
    [180, 152, 36],
    [140, 116, 24],
    [0, 0, 0],
    [0, 0, 0],
    [0, 0, 0],
];

/// Draws `text` with its top-left at `(x, y)` and returns its width.
pub fn text(dst: &mut Picture, font: &Font, x: i32, y: i32, s: &str) -> u32 {
    text_in(dst, font, x, y, s, &TEXT)
}

/// Draws `text` in the given shade colours.
fn text_in(dst: &mut Picture, font: &Font, x: i32, y: i32, s: &str, colours: &[[u8; 3]; 8]) -> u32 {
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
                        let col = colours[shade as usize];
                        dst.rgba[d..d + 4].copy_from_slice(&[col[0], col[1], col[2], 255]);
                    }
                }
            }
        }
        pen += font.advance(c) as i32;
    }
    (pen - x) as u32
}

/// Draws `s` centred in `r`, vertically too.
fn text_centred(dst: &mut Picture, font: &Font, r: Rect, s: &str) {
    let w = font.width(s.as_bytes()) as i32;
    text(dst, font, r.x + (r.w as i32 - w) / 2, text_top(font, r), s);
}

/// The text top that centres a line of `font` in `r`.
fn text_top(font: &Font, r: Rect) -> i32 {
    r.y + (r.h as i32 - font.height as i32) / 2
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

// --- the art ------------------------------------------------------------------------------------------------------

pub struct HudArt {
    pal: Palette,
    top: Picture,
    bottom: Picture,
    right: Picture,
    /// Menu strip, minimap box, button area.
    left: [Picture; 3],
    /// The information panel's background frames 0 to 3 (see [`crate::info::frame`]).
    info: [Picture; 4],
    menu_button: Option<Picture>,
    /// Gold, lumber, oil.
    resources: Vec<Picture>,
    icons: Grp,
    font: Font,
    /// A smaller font for the hit point text, if the install has one.
    small: Option<Font>,
    names: StringTable,
    pub pieces: Pieces,
}

/// Candidate names of a smaller font, tried in order.
const SMALL_FONT: [&str; 1] = ["Art\\font6.fnt"];

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

        // The minimap is x 24 to 151, y 26 to 153 on the screen, 2 pixels below the minimap box's top (screen spec).
        let hole = Rect::new(
            crate::minimap::ORIGIN.0,
            2,
            crate::minimap::SIZE as u32,
            crate::minimap::SIZE as u32,
        );
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
            // A sheet with fewer than four frames repeats its last one.
            info: [0, 1, 2, 3].map(|n| frame_picture(&frames.frames[n.min(frames.frames.len() - 1)], pal, None)),
            menu_button,
            resources: res.frames.iter().take(3).map(|f| frame_picture(f, pal, None)).collect(),
            icons,
            font,
            small: SMALL_FONT
                .iter()
                .find_map(|n| install.read(n).ok().and_then(|d| Font::parse(&d).ok())),
            names,
            pal: pal.clone(),
            pieces,
        })
    }

    pub fn layout(&self, canvas: (u32, u32)) -> Layout {
        Layout::new(canvas, &self.pieces)
    }

    /// String `n` of `stat_txt.tbl`, counting from 1 as the specs do.
    pub fn string(&self, n: u16) -> String {
        self.names.text((n as usize).wrapping_sub(1)).unwrap_or_default()
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

    /// Draws icon frame `n` of the era's icon set in `owner`'s colour inside a button frame, with its top-left at
    /// `(x, y)`. The frame is our own drawing (a dark box with a gold edge), as the command buttons use.
    fn button(&self, pic: &mut Picture, x: i32, y: i32, n: u16, owner: u8) {
        let Some(f) = self.icons.frames.get(n as usize) else {
            return;
        };
        let p = frame_picture(f, &self.pal, Some(owner));
        let r = Rect::new(x - 2, y - 2, p.width + 4, p.height + 4);
        fill(pic, r, [24, 24, 24]);
        blit(pic, x, y, &p);
        outline(pic, r, [140, 120, 60]);
    }

    /// A palette colour of the era.
    fn colour(&self, n: u8) -> [u8; 3] {
        self.pal.0[n as usize]
    }

    /// Draws the information panel with its top-left corner at `o` (spec `ui/info-panel.md`).
    fn draw_info(&self, pic: &mut Picture, o: (i32, i32), v: &InfoView) {
        use crate::info;
        let at = |r: Rect| Rect::new(o.0 + r.x, o.1 + r.y, r.w, r.h);
        blit(pic, o.0, o.1, &self.info[(v.frame as usize).min(3)]);
        // Open question: the background colour of the health bar strip and the mana bar's inner outline is assumed
        // black.
        let background = [0, 0, 0];
        for (n, slot) in v.slots.iter().enumerate() {
            let Some(s) = slot else { continue };
            let r = at(info::slot_rect(n));
            let d = if v.pressed == Some(n) { 5 } else { 4 };
            if let Some(i) = info::icon(s.kind) {
                self.button(pic, r.x + d, r.y + d, i, s.owner);
            }
            fill(pic, Rect::new(r.x, r.y + 46, 54, 7), background);
            let (filled, c) = info::health_bar(s.hp, s.max_hp);
            fill(pic, Rect::new(r.x + 2, r.y + 46, 50, 5), self.colour(99));
            fill(pic, Rect::new(r.x + 2, r.y + 46, filled, 5), self.colour(c));
        }
        let Some(c) = v.content else { return };
        let font = &self.font;
        if let Some(n) = c.name {
            let (l1, l2) = info::name_lines(&self.string(n), &|b| font.width(b));
            text_centred(pic, font, at(info::NAME[0]), &l1);
            text_centred(pic, font, at(info::NAME[1]), &l2);
        }
        if let Some(n) = c.level {
            text_centred(pic, font, at(info::LEVEL), &format!("{} {n}", self.string(411)));
        }
        if let Some((hp, max)) = c.hp {
            // Open question: the hit point text's style probably selects a smaller font; the small font is used
            // when the install has it.
            let small = self.small.as_ref().unwrap_or(font);
            // Ours: "1200/1200" is wider than the area, so the centred text is kept inside the panel's frame.
            let (r, s) = (at(info::HP), format!("{hp}/{max}"));
            let w = small.width(s.as_bytes()) as i32;
            let left = (r.x + (r.w as i32 - w) / 2).max(o.0 + 3);
            text(pic, small, left, text_top(small, r), &s);
        }
        for (n, row) in c.rows.iter().enumerate() {
            let Some(row) = row else { continue };
            // Open question: labels are right-aligned in their area; values start at its left edge.
            let lr = at(info::label_rect(n));
            let label = self.string(row.label);
            let w = font.width(label.as_bytes()) as i32;
            text(pic, font, lr.x + lr.w as i32 - w, text_top(font, lr), &label);
            if c.production.is_some() {
                continue;
            }
            let vr = at(info::value_rect(n));
            match &row.value {
                info::Value::Empty => {}
                info::Value::Text(spans) => {
                    let mut x = vr.x;
                    for s in spans {
                        let colours = if s.alt { &TEXT_ALT } else { &TEXT };
                        x += text_in(pic, font, x, text_top(font, vr), &s.text, colours) as i32;
                    }
                }
                info::Value::Mana(m) => {
                    outline(pic, vr, self.colour(246));
                    outline(pic, Rect::new(vr.x + 1, vr.y + 1, vr.w - 2, vr.h - 2), background);
                    let bar = at(info::MANA_BAR);
                    fill(pic, bar, self.colour(99));
                    fill(
                        pic,
                        Rect::new(bar.x, bar.y, info::mana_fill(*m), bar.h),
                        self.colour(186),
                    );
                    text_centred(pic, font, bar, &m.to_string());
                }
            }
        }
        if let Some(i) = c.production {
            let r = at(info::PRODUCTION);
            self.button(pic, r.x + 4, r.y + 4, i, v.local);
        }
        if let Some(p) = c.progress {
            let r = at(info::PROGRESS);
            fill(pic, Rect::new(r.x, r.y, info::progress_fill(p), r.h), self.colour(149));
            text_centred(pic, font, r, &self.string(434));
        }
    }

    /// The whole interface: `frame` plus the minimap, resources and selection.
    pub fn draw(&self, frame: &Picture, l: &Layout, v: &View) -> Picture {
        let mut pic = Picture {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba.clone(),
        };
        let mini = Picture {
            width: crate::minimap::SIZE as u32,
            height: crate::minimap::SIZE as u32,
            rgba: to_rgba(v.minimap, &self.pal, false, None),
        };
        if v.minimap.len() == crate::minimap::SIZE * crate::minimap::SIZE {
            blit(&mut pic, l.minimap.x, l.minimap.y, &mini);
        }

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

        if let Some(msg) = v.status {
            text(&mut pic, &self.font, l.bottom.x + 4, l.bottom.y + 1, msg);
        }

        // The command panel: frames of the icon sheet (the command icons share the unit icon sheet).
        for &(slot, icon) in v.buttons {
            let r = crate::panel::slot_rect((l.buttons.x, l.buttons.y), slot as usize);
            fill(&mut pic, r, [24, 24, 24]);
            if let Some(f) = self.icons.frames.get(icon as usize) {
                let p = frame_picture(f, &self.pal, None);
                blit(
                    &mut pic,
                    r.x + (r.w as i32 - p.width as i32) / 2,
                    r.y + (r.h as i32 - p.height as i32) / 2,
                    &p,
                );
            }
            outline(&mut pic, r, [140, 120, 60]);
        }

        self.draw_info(&mut pic, (l.info.x, l.info.y), &v.info);
        pic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces() -> Pieces {
        Pieces {
            panel_w: 176,
            panel_h: [24, 136, 176, 144],
            bar: 16,
            hole: Rect::new(24, 2, 128, 128),
        }
    }

    #[test]
    fn layout_of_the_original_screen() {
        let l = Layout::new((640, 480), &pieces());
        assert_eq!(l.viewport, Rect::new(176, 16, 448, 448));
        assert_eq!(l.minimap, Rect::new(24, 26, 128, 128));
        assert_eq!(l.info, Rect::new(0, 160, 176, 176));
        assert_eq!(l.buttons, Rect::new(0, 336, 176, 144));
        assert_eq!(l.top, Rect::new(176, 0, 448, 16));
        assert_eq!(l.bottom, Rect::new(176, 464, 448, 16));
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
        assert_eq!(m.origin(), (10.0, 52.0));
        assert_eq!(m.tile_at((10.0 + 20.0, 52.0 + 8.0)), (10.0, 4.0));
        // Outside the map clamps to its edge.
        assert_eq!(m.tile_at((0.0, 0.0)), (0.0, 0.0));
        assert_eq!(m.tile_at((500.0, 500.0)), (64.0, 32.0));
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
    fn panel_names_split_in_a_real_font() {
        // A font where every character is 8 pixels wide (7 + 1 spacing).
        let mut d = b"FONT".to_vec();
        d.extend([b'a', b'z', 7, 1]);
        let first = 8 + 26 * 4;
        for i in 0..26u32 {
            d.extend((first as u32 + i * 11).to_le_bytes());
        }
        for _ in 0..26 {
            d.extend([7, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1]);
        }
        let font = Font::parse(&d).unwrap();
        let width = |b: &[u8]| font.width(b);
        // 13 characters, 104 px: one line.
        let lines = crate::info::name_lines("abcdefghijklm", &width);
        assert_eq!(lines, (String::new(), "abcdefghijklm".to_string()));
        let lines = crate::info::name_lines("abcdef ghijklmnop", &width);
        assert_eq!(lines, ("abcdef".to_string(), "ghijklmnop".to_string()));
    }
}
