//! The touch chrome (`docs/design/touch.md`): the controls a touchscreen has no keyboard for, drawn in the window
//! outside the 4:3 picture and read from the fingers before the gesture machine sees them.
//!
//! | Bar | Control | Does |
//! |---|---|---|
//! | Left | Menu | F10, the game menu |
//! | Left | Pause | Pauses or resumes the game, as the pause key does |
//! | Left | Shift, Ctrl, Alt | A latch: the key is held for the next click, then let go (one frame after that click's release, as the rules that read Alt when the button comes up need) |
//! | Left | Plain click | A latch: the next tap on the map is a plain left click, never a smart order |
//! | Left | Keyboard | Opens or closes the system's soft keyboard (the shell calls `SDL_StartTextInput`) |
//! | Right | 1 to 9 | A control group: a tap is the digit key (twice quickly centres the view, as the key does), a hold of [`HOLD_MS`] is Ctrl and the digit (assign) |
//! | Right | Idle worker | F6, the next idle worker (only with the `idle-worker` option on) |
//!
//! Outside a game only the keyboard button shows. The `touch-swap-bars` option swaps the two bars' sides.
//!
//! **Where.** The controls sit in the black bars the picture's fit leaves, each bar a grid of as many columns as its
//! width holds. When only one bar is wide enough it takes both lists; a window with no wide bar gets the left list
//! over the picture's left edge and the right list over its right edge. Everything is placed inside the safe area, and
//! sized in millimetres (the `touch-buttons` option, [`BUTTON_MM`] by default) between [`MIN_POINTS`] and
//! [`MAX_POINTS`] points, smaller when a bar is too short for its buttons. The right bar has six group buttons, up to
//! nine when they fit at full size.
//!
//! **Pressing.** A finger that goes down on a control belongs to the chrome until it lifts; the control acts when it
//! lifts still inside the same control, so sliding off cancels. A group button held long enough acts at once
//! ([`Chrome::advance`]) and then does nothing at the lift. The chrome's pictures are drawn as one RGBA overlay of the
//! window's size ([`Chrome::overlay`]) that the shell blends over the picture; the picture itself is never touched,
//! so frames and snapshots do not depend on the chrome.

use crate::display::Area;
use crate::input::{Touch, TouchPhase};

/// A button's size in millimetres, by default.
pub const BUTTON_MM: f32 = 9.0;
/// The smallest and largest a button gets, in logical points.
pub const MIN_POINTS: f32 = 30.0;
pub const MAX_POINTS: f32 = 80.0;
/// A bar must be this many button sizes wide to hold the controls.
const BAR_FRACTION: f32 = 0.8;
/// How long a group button is held to assign the group.
pub const HOLD_MS: f32 = 450.0;
/// The group buttons: at least this many, and at most nine.
const MIN_GROUPS: u8 = 6;
const MAX_GROUPS: u8 = 9;

/// One control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    Menu,
    Pause,
    Shift,
    Ctrl,
    Alt,
    /// The next tap is a plain left click.
    Plain,
    Keyboard,
    /// A control group, 1 to 9.
    Group(u8),
    IdleWorker,
}

impl Control {
    /// Whether holding the control does something of its own.
    fn holds(self) -> bool {
        matches!(self, Control::Group(_))
    }
}

/// A rectangle in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, p: (f32, f32)) -> bool {
        p.0 >= self.x && p.0 < self.x + self.w && p.1 >= self.y && p.1 < self.y + self.h
    }
}

/// What the chrome did with a finger's event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fed {
    /// Not the chrome's: the gesture machine gets it.
    Pass,
    /// The chrome's finger, nothing more to do.
    Consumed,
    /// The chrome's finger lifted on its control: the control was pressed.
    Pressed(Control),
}

/// The window's shape and the options for [`Chrome::layout`], in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The picture's place in the window.
    pub picture: Rect,
    /// The window's usable part.
    pub safe: Rect,
    /// Logical points to a millimetre.
    pub per_mm: f32,
    /// A game is on (everything but the keyboard only makes sense then).
    pub in_game: bool,
    /// The buttons' size in millimetres (`touch-buttons`).
    pub button_mm: f32,
    /// The bars change sides (`touch-swap-bars`).
    pub swap: bool,
    /// The idle-worker button shows (`idle-worker`).
    pub idle_worker: bool,
}

/// The chrome's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {
    /// Where each control is now.
    items: Vec<(Control, Rect)>,
    /// The latched modifiers.
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// The next tap is a plain left click.
    pub plain: bool,
    /// The soft keyboard is open.
    pub keyboard: bool,
    /// Fingers the chrome owns: the control each went down on, how long it has been down (ms), and whether its hold
    /// already acted.
    owned: Vec<(u64, Control, f32, bool)>,
}

/// Lays `list` out as a grid in a bar `w` wide at `x0`, from `top` over `height`, buttons `unit` big if they fit.
fn grid(out: &mut Vec<(Control, Rect)>, list: &[Control], x0: f32, w: f32, top: f32, height: f32, unit: f32) {
    if list.is_empty() {
        return;
    }
    let n = list.len();
    let gap = (unit / 6.0).round().max(2.0);
    let mut size = unit.min(w - gap).max(8.0);
    let max_cols = (((w - gap) / (size + gap)).floor() as usize).clamp(1, n);
    let tall = |rows: usize, size: f32| rows as f32 * size + (rows as f32 - 1.0) * gap;
    let mut cols = 1;
    while cols < max_cols && tall(n.div_ceil(cols), size) > height {
        cols += 1;
    }
    let rows = n.div_ceil(cols);
    if tall(rows, size) > height {
        size = ((height - (rows as f32 - 1.0) * gap) / rows as f32).max(8.0);
    }
    let total_w = cols as f32 * size + (cols as f32 - 1.0) * gap;
    let x = x0 + ((w - total_w) / 2.0).max(0.0);
    let y = top + ((height - tall(rows, size)) / 2.0).max(0.0);
    for (i, &c) in list.iter().enumerate() {
        // Down the first column, then the next: the groups read 1 to 9 from the top.
        let (col, row) = (i / rows, i % rows);
        out.push((
            c,
            Rect {
                x: x + col as f32 * (size + gap),
                y: y + row as f32 * (size + gap),
                w: size,
                h: size,
            },
        ));
    }
}

/// How many buttons of `unit` points a bar `w` wide and `height` tall holds.
fn capacity(w: f32, height: f32, unit: f32) -> usize {
    let gap = (unit / 6.0).round().max(2.0);
    let size = unit.min(w - gap).max(8.0);
    let cols = (((w - gap) / (size + gap)).floor() as usize).max(1);
    let rows = (((height + gap) / (size + gap)).floor() as usize).max(1);
    cols * rows
}

impl Chrome {
    /// Places the controls for a window shaped like `f`.
    pub fn layout(&mut self, f: Frame) {
        self.items.clear();
        let unit = (f.button_mm * f.per_mm).clamp(MIN_POINTS, MAX_POINTS);
        let left_bar = (f.picture.x - f.safe.x).max(0.0);
        let right_bar = (f.safe.x + f.safe.w - (f.picture.x + f.picture.w)).max(0.0);
        let fits = |bar: f32| bar >= BAR_FRACTION * unit;
        let (mut tools, mut groups): (Vec<Control>, Vec<Control>) = (Vec::new(), Vec::new());
        if f.in_game {
            tools.extend([
                Control::Menu,
                Control::Pause,
                Control::Shift,
                Control::Ctrl,
                Control::Alt,
                Control::Plain,
            ]);
        }
        tools.push(Control::Keyboard);
        let (top, height) = (f.safe.y, f.safe.h);
        let (lx, rx) = (f.safe.x, f.picture.x + f.picture.w);
        // Where each list goes: (x, width) of its bar.
        let edge = unit + (unit / 6.0).round();
        let (tools_at, groups_at, shared) = match (fits(left_bar), fits(right_bar)) {
            (true, true) => ((lx, left_bar), (rx, right_bar), false),
            (true, false) => ((lx, left_bar), (lx, left_bar), true),
            (false, true) => ((rx, right_bar), (rx, right_bar), true),
            // Over the picture's edges.
            (false, false) => (
                (f.picture.x.max(f.safe.x), edge),
                ((f.picture.x + f.picture.w).min(f.safe.x + f.safe.w) - edge, edge),
                false,
            ),
        };
        let (tools_at, groups_at) = if f.swap && !shared {
            (groups_at, tools_at)
        } else {
            (tools_at, groups_at)
        };
        if f.in_game {
            let helpers = usize::from(f.idle_worker);
            let room = if shared {
                capacity(tools_at.1, height, unit).saturating_sub(tools.len())
            } else {
                capacity(groups_at.1, height, unit)
            };
            let n = (room.saturating_sub(helpers) as u8).clamp(MIN_GROUPS, MAX_GROUPS);
            groups.extend((1..=n).map(Control::Group));
            if f.idle_worker {
                groups.push(Control::IdleWorker);
            }
        }
        if shared {
            let all: Vec<Control> = tools.iter().chain(&groups).copied().collect();
            grid(&mut self.items, &all, tools_at.0, tools_at.1, top, height, unit);
        } else {
            grid(&mut self.items, &tools, tools_at.0, tools_at.1, top, height, unit);
            grid(&mut self.items, &groups, groups_at.0, groups_at.1, top, height, unit);
        }
    }

    /// Where the controls are now.
    pub fn items(&self) -> &[(Control, Rect)] {
        &self.items
    }

    /// Whether a window point is on a control.
    pub fn hit(&self, p: (f32, f32)) -> Option<Control> {
        self.items.iter().find(|(_, r)| r.contains(p)).map(|&(c, _)| c)
    }

    /// Whether a control is on now (a latch, or a button a finger holds).
    pub fn is_on(&self, c: Control) -> bool {
        match c {
            Control::Shift => self.shift,
            Control::Ctrl => self.ctrl,
            Control::Alt => self.alt,
            Control::Plain => self.plain,
            Control::Keyboard => self.keyboard,
            Control::Menu | Control::Pause | Control::Group(_) | Control::IdleWorker => {
                self.owned.iter().any(|&(_, o, _, _)| o == c)
            }
        }
    }

    /// A finger's event: the chrome takes the fingers that go down on a control.
    pub fn feed(&mut self, t: Touch) -> Fed {
        match t.phase {
            TouchPhase::Began => match self.hit(t.pos) {
                Some(c) => {
                    self.owned.push((t.id, c, 0.0, false));
                    Fed::Consumed
                }
                None => Fed::Pass,
            },
            TouchPhase::Moved => {
                if self.owned.iter().any(|&(id, ..)| id == t.id) {
                    Fed::Consumed
                } else {
                    Fed::Pass
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(at) = self.owned.iter().position(|&(id, ..)| id == t.id) else {
                    return Fed::Pass;
                };
                let (_, c, _, held) = self.owned.remove(at);
                if t.phase == TouchPhase::Ended && !held && self.hit(t.pos) == Some(c) {
                    Fed::Pressed(c)
                } else {
                    Fed::Consumed
                }
            }
        }
    }

    /// Time passes by `ms`: the controls held for [`HOLD_MS`] that act on a hold, once each.
    pub fn advance(&mut self, ms: f32) -> Vec<Control> {
        let mut out = Vec::new();
        for (_, c, age, held) in &mut self.owned {
            *age += ms;
            if !*held && c.holds() && *age >= HOLD_MS {
                *held = true;
                out.push(*c);
            }
        }
        out
    }

    /// Fingers are down on a control.
    pub fn busy(&self) -> bool {
        !self.owned.is_empty()
    }

    /// The overlay for a window of `size` physical pixels at `factor` pixels per point: RGBA, premultiplication-free,
    /// transparent where there is no control.
    pub fn overlay(&self, size: (u32, u32), factor: f32) -> Overlay {
        let mut o = Overlay {
            size,
            rgba: vec![0; size.0 as usize * size.1 as usize * 4],
        };
        for &(c, r) in &self.items {
            let on = self.is_on(c);
            let (x0, y0) = ((r.x * factor).round() as i32, (r.y * factor).round() as i32);
            let (w, h) = ((r.w * factor).round() as i32, (r.h * factor).round() as i32);
            let (fill, ink) = if on {
                ([212, 168, 52, 235], [24, 16, 4, 255])
            } else {
                ([24, 28, 40, 170], [225, 225, 225, 255])
            };
            o.fill(x0, y0, w, h, fill);
            o.border(
                x0,
                y0,
                w,
                h,
                (factor.round() as i32).max(1),
                if on { [255, 224, 120, 255] } else { [150, 150, 160, 220] },
            );
            o.glyph(c, x0, y0, w, h, ink);
        }
        o
    }
}

/// An RGBA picture of the window's size to blend over what the shell drew.
#[derive(Clone, Debug, PartialEq)]
pub struct Overlay {
    pub size: (u32, u32),
    pub rgba: Vec<u8>,
}

/// A control's mark: a 3 × 5 cell pattern, `#` drawn.
pub fn glyph_rows(c: Control) -> [&'static str; 5] {
    match c {
        Control::Menu => ["###", "...", "###", "...", "###"],
        Control::Pause => ["#.#", "#.#", "#.#", "#.#", "#.#"],
        Control::Shift => ["###", "#..", "###", "..#", "###"],
        Control::Ctrl => ["###", "#..", "#..", "#..", "###"],
        Control::Alt => [".#.", "#.#", "###", "#.#", "#.#"],
        Control::Plain => ["#..", "#..", "#..", "#..", "###"],
        Control::Keyboard => ["#.#", "#.#", "##.", "#.#", "#.#"],
        Control::IdleWorker => ["###", ".#.", ".#.", ".#.", "###"],
        Control::Group(n) => match n {
            1 => [".#.", "##.", ".#.", ".#.", "###"],
            2 => ["##.", "..#", ".#.", "#..", "###"],
            3 => ["##.", "..#", ".#.", "..#", "##."],
            4 => ["#.#", "#.#", "###", "..#", "..#"],
            5 => ["###", "#..", "##.", "..#", "##."],
            6 => [".##", "#..", "###", "#.#", "###"],
            7 => ["###", "..#", ".#.", ".#.", ".#."],
            8 => ["###", "#.#", "###", "#.#", "###"],
            _ => ["###", "#.#", "###", "..#", "##."],
        },
    }
}

impl Overlay {
    /// Whether anything is drawn.
    pub fn is_empty(&self) -> bool {
        self.rgba.chunks_exact(4).all(|p| p[3] == 0)
    }

    fn put(&mut self, x: i32, y: i32, c: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.size.0 as i32 || y >= self.size.1 as i32 {
            return;
        }
        let i = (y as usize * self.size.0 as usize + x as usize) * 4;
        self.rgba[i..i + 4].copy_from_slice(&c);
    }

    fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: [u8; 4]) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.put(xx, yy, c);
            }
        }
    }

    fn border(&mut self, x: i32, y: i32, w: i32, h: i32, t: i32, c: [u8; 4]) {
        self.fill(x, y, w, t, c);
        self.fill(x, y + h - t, w, t, c);
        self.fill(x, y, t, h, c);
        self.fill(x + w - t, y, t, h, c);
    }

    /// A control's mark, centred in its rectangle.
    fn glyph(&mut self, c: Control, x: i32, y: i32, w: i32, h: i32, ink: [u8; 4]) {
        let rows = glyph_rows(c);
        let cell = (h.min(w) / 8).max(2);
        let (gw, gh) = (3 * cell, 5 * cell);
        let (ox, oy) = (x + (w - gw) / 2, y + (h - gh) / 2);
        for (ry, row) in rows.iter().enumerate() {
            for (rx, ch) in row.bytes().enumerate() {
                if ch == b'#' {
                    self.fill(ox + rx as i32 * cell, oy + ry as i32 * cell, cell, cell, ink);
                }
            }
        }
    }
}

/// The options [`frame_of`] passes on: the button size, swapped bars and the idle-worker button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub button_mm: f32,
    pub swap: bool,
    pub idle_worker: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            button_mm: BUTTON_MM,
            swap: false,
            idle_worker: false,
        }
    }
}

/// A window's [`Frame`] for the chrome from the pieces the core has: the picture's rectangle (physical pixels) and
/// the safe area (physical pixels), in logical points.
pub fn frame_of(
    picture: (u32, u32, u32, u32),
    safe: Area,
    factor: f32,
    per_mm: f32,
    in_game: bool,
    o: Options,
) -> Frame {
    let f = factor.max(0.01);
    let r = |a: (u32, u32, u32, u32)| Rect {
        x: a.0 as f32 / f,
        y: a.1 as f32 / f,
        w: a.2 as f32 / f,
        h: a.3 as f32 / f,
    };
    Frame {
        picture: r(picture),
        safe: r(safe),
        per_mm,
        in_game,
        button_mm: o.button_mm,
        swap: o.swap,
        idle_worker: o.idle_worker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: u64, phase: TouchPhase, x: f32, y: f32) -> Touch {
        Touch { id, phase, pos: (x, y) }
    }

    /// A wide window: a 640 x 480 picture in 1000 x 600 points, 6 points to a millimetre.
    fn wide(in_game: bool) -> Frame {
        Frame {
            picture: Rect {
                x: 180.0,
                y: 60.0,
                w: 640.0,
                h: 480.0,
            },
            safe: Rect {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 600.0,
            },
            per_mm: 6.0,
            in_game,
            button_mm: BUTTON_MM,
            swap: false,
            idle_worker: false,
        }
    }

    fn rect_of(c: &Chrome, k: Control) -> Rect {
        c.items()
            .iter()
            .find(|&&(x, _)| x == k)
            .map(|&(_, r)| r)
            .expect("the control is laid out")
    }

    fn no_overlap(c: &Chrome) {
        let all = c.items();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let (ra, rb) = (a.1, b.1);
                assert!(
                    ra.x + ra.w <= rb.x || rb.x + rb.w <= ra.x || ra.y + ra.h <= rb.y || rb.y + rb.h <= ra.y,
                    "{a:?} and {b:?} overlap"
                );
            }
        }
    }

    fn groups(c: &Chrome) -> usize {
        c.items().iter().filter(|x| matches!(x.0, Control::Group(_))).count()
    }

    #[test]
    fn the_tools_sit_left_and_the_groups_right_clear_of_the_picture() {
        let mut c = Chrome::default();
        let f = wide(true);
        c.layout(f);
        let menu = rect_of(&c, Control::Menu);
        let one = rect_of(&c, Control::Group(1));
        assert!(menu.x + menu.w <= f.picture.x, "the tools in the left bar");
        assert!(one.x >= f.picture.x + f.picture.w, "the groups in the right bar");
        for k in [
            Control::Pause,
            Control::Shift,
            Control::Ctrl,
            Control::Alt,
            Control::Plain,
            Control::Keyboard,
        ] {
            let r = rect_of(&c, k);
            assert!(r.x + r.w <= f.picture.x, "{k:?}");
        }
        for &(_, r) in c.items() {
            assert!(r.w >= 8.0 && r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 1000.0 && r.y + r.h <= 600.0);
            assert!(r.w <= MAX_POINTS);
        }
        // 180 points of bar hold three columns of 54-point buttons: room for all nine groups.
        assert_eq!(groups(&c), 9);
        assert!(rect_of(&c, Control::Group(2)).y > one.y, "the groups go down first");
        no_overlap(&c);
    }

    #[test]
    fn a_narrow_tall_bar_has_six_groups_or_more_shrunk_to_fit() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        // 60-point bars, 600 tall: one column of 54-point buttons holds nine at most.
        f.picture.x = 60.0;
        f.picture.w = 880.0;
        c.layout(f);
        assert!((6..=9).contains(&groups(&c)), "{}", groups(&c));
        // A short window: six still, smaller.
        f.safe.h = 300.0;
        f.picture.y = 0.0;
        f.picture.h = 300.0;
        c.layout(f);
        assert_eq!(groups(&c), 6);
        assert!(c.items().iter().all(|(_, r)| r.y + r.h <= 300.0));
        no_overlap(&c);
    }

    #[test]
    fn the_idle_worker_button_comes_with_its_option() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        c.layout(f);
        assert!(c.items().iter().all(|x| x.0 != Control::IdleWorker));
        f.idle_worker = true;
        c.layout(f);
        let r = rect_of(&c, Control::IdleWorker);
        assert!(r.x >= f.picture.x + f.picture.w);
        no_overlap(&c);
    }

    #[test]
    fn the_bars_can_swap_sides() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        f.swap = true;
        c.layout(f);
        assert!(rect_of(&c, Control::Menu).x >= f.picture.x + f.picture.w);
        assert!(rect_of(&c, Control::Group(1)).x + 10.0 <= f.picture.x);
    }

    #[test]
    fn the_button_size_follows_the_option() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        f.button_mm = 7.0;
        c.layout(f);
        assert_eq!(rect_of(&c, Control::Menu).w, 42.0);
        f.button_mm = 11.0;
        c.layout(f);
        assert_eq!(rect_of(&c, Control::Menu).w, 66.0);
    }

    #[test]
    fn outside_a_game_only_the_keyboard_shows() {
        let mut c = Chrome::default();
        c.layout(wide(false));
        assert_eq!(c.items().iter().map(|x| x.0).collect::<Vec<_>>(), [Control::Keyboard]);
    }

    #[test]
    fn one_wide_bar_takes_every_control() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        // A notch on the right: the safe area ends where the picture does.
        f.safe.w = 820.0;
        c.layout(f);
        assert!(groups(&c) >= 6);
        assert!(c.items().iter().all(|(_, r)| r.x + r.w <= f.picture.x));
        no_overlap(&c);
    }

    #[test]
    fn no_bar_puts_the_lists_over_the_picture_edges() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        f.picture = Rect {
            x: 0.0,
            y: 60.0,
            w: 1000.0,
            h: 480.0,
        };
        c.layout(f);
        assert!(
            c.items()
                .iter()
                .all(|(_, r)| r.x >= 0.0 && r.x + r.w <= 1000.0 && r.y >= 0.0)
        );
        assert!(rect_of(&c, Control::Menu).x < 100.0);
        assert!(rect_of(&c, Control::Group(1)).x > 900.0);
        no_overlap(&c);
    }

    #[test]
    fn a_control_is_pressed_by_lifting_on_it() {
        let mut c = Chrome::default();
        c.layout(wide(true));
        let r = rect_of(&c, Control::Alt);
        let mid = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        assert_eq!(c.feed(t(1, TouchPhase::Began, mid.0, mid.1)), Fed::Consumed);
        assert!(c.busy());
        assert_eq!(c.feed(t(1, TouchPhase::Moved, mid.0 + 1.0, mid.1)), Fed::Consumed);
        assert_eq!(
            c.feed(t(1, TouchPhase::Ended, mid.0, mid.1)),
            Fed::Pressed(Control::Alt)
        );
        assert!(!c.busy());
        // Sliding off cancels; a finger elsewhere is not the chrome's.
        c.feed(t(2, TouchPhase::Began, mid.0, mid.1));
        assert_eq!(c.feed(t(2, TouchPhase::Ended, 500.0, 300.0)), Fed::Consumed);
        assert_eq!(c.feed(t(3, TouchPhase::Began, 500.0, 300.0)), Fed::Pass);
        assert_eq!(
            c.feed(t(3, TouchPhase::Moved, mid.0, mid.1)),
            Fed::Pass,
            "a finger that began elsewhere stays the game's"
        );
        assert_eq!(c.feed(t(3, TouchPhase::Ended, mid.0, mid.1)), Fed::Pass);
        // Cancelled is not a press.
        c.feed(t(4, TouchPhase::Began, mid.0, mid.1));
        assert_eq!(c.feed(t(4, TouchPhase::Cancelled, mid.0, mid.1)), Fed::Consumed);
    }

    #[test]
    fn a_group_button_held_acts_once_and_not_again_at_the_lift() {
        let mut c = Chrome::default();
        c.layout(wide(true));
        let r = rect_of(&c, Control::Group(3));
        let mid = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        c.feed(t(1, TouchPhase::Began, mid.0, mid.1));
        assert!(c.advance(400.0).is_empty(), "not yet");
        assert!(c.is_on(Control::Group(3)), "a held button lights");
        assert_eq!(c.advance(60.0), [Control::Group(3)]);
        assert!(c.advance(500.0).is_empty(), "once");
        assert_eq!(c.feed(t(1, TouchPhase::Ended, mid.0, mid.1)), Fed::Consumed);
        // A quick tap is a press; other buttons never act on a hold.
        c.feed(t(2, TouchPhase::Began, mid.0, mid.1));
        c.advance(100.0);
        assert_eq!(
            c.feed(t(2, TouchPhase::Ended, mid.0, mid.1)),
            Fed::Pressed(Control::Group(3))
        );
        let m = rect_of(&c, Control::Menu);
        c.feed(t(3, TouchPhase::Began, m.x + 2.0, m.y + 2.0));
        assert!(c.advance(1000.0).is_empty());
        assert_eq!(
            c.feed(t(3, TouchPhase::Ended, m.x + 2.0, m.y + 2.0)),
            Fed::Pressed(Control::Menu)
        );
    }

    #[test]
    fn every_control_has_its_own_mark() {
        let mut all = vec![
            Control::Menu,
            Control::Pause,
            Control::Shift,
            Control::Ctrl,
            Control::Alt,
            Control::Plain,
            Control::Keyboard,
            Control::IdleWorker,
        ];
        all.extend((1..=9).map(Control::Group));
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(glyph_rows(*a), glyph_rows(*b), "{a:?} and {b:?}");
            }
        }
    }

    #[test]
    fn the_overlay_draws_the_controls_and_shows_what_is_on() {
        let mut c = Chrome::default();
        c.layout(wide(true));
        let off = c.overlay((2000, 1200), 2.0);
        assert!(!off.is_empty());
        assert_eq!(off.rgba.len(), 2000 * 1200 * 4);
        // The picture's middle is untouched.
        let at = |o: &Overlay, x: usize, y: usize| o.rgba[(y * 2000 + x) * 4 + 3];
        assert_eq!(at(&off, 1000, 600), 0);
        let r = rect_of(&c, Control::Ctrl);
        let (px, py) = (((r.x + 3.0) * 2.0) as usize, ((r.y + 3.0) * 2.0) as usize);
        let before = off.rgba[(py * 2000 + px) * 4..][..4].to_vec();
        c.ctrl = true;
        let on = c.overlay((2000, 1200), 2.0);
        assert_ne!(
            before,
            on.rgba[(py * 2000 + px) * 4..][..4].to_vec(),
            "a latched control looks different"
        );
    }
}
