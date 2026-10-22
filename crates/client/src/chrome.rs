//! The touch chrome (`docs/design/touch.md`): the few controls a touchscreen has no keyboard for, drawn in the window
//! outside the 4:3 picture and read from the fingers before the gesture machine sees them.
//!
//! | Control | Does |
//! |---|---|
//! | Shift, Ctrl, Alt | A latch: the key is held for the next click, then let go (one frame after that click's release, as the rules that read Alt when the button comes up need) |
//! | Right-click mode | A toggle: taps are right clicks until it is switched off |
//! | Pause | Pauses or resumes the game, as the pause key does |
//! | Keyboard | Opens or closes the system's soft keyboard (the shell calls `SDL_StartTextInput`) |
//!
//! **Where.** The controls sit in the black bars the picture's fit leaves, the modifier latches and the right-click
//! toggle at the left, pause and keyboard at the right (all at the one side if only one bar is wide enough). A window
//! with no wide bar gets them in a column over the picture's left edge. Everything is placed inside the safe area, and
//! sized in millimetres ([`BUTTON_MM`]) between [`MIN_POINTS`] and [`MAX_POINTS`] points.
//!
//! **Pressing.** A finger that goes down on a control belongs to the chrome until it lifts; the control acts when it
//! lifts still inside the same control, so sliding off cancels. The chrome's pictures are drawn as one RGBA overlay of
//! the window's size ([`Chrome::overlay`]) that the shell blends over the picture; the picture itself is never touched,
//! so frames and snapshots do not depend on the chrome.

use crate::display::Area;
use crate::input::{Touch, TouchPhase};

/// A button's size in millimetres.
pub const BUTTON_MM: f32 = 9.0;
/// The smallest and largest a button gets, in logical points.
pub const MIN_POINTS: f32 = 30.0;
pub const MAX_POINTS: f32 = 64.0;
/// A bar must be this many button sizes wide to hold the controls.
const BAR_FRACTION: f32 = 0.8;

/// One control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Control {
    Shift,
    Ctrl,
    Alt,
    RightClick,
    Pause,
    Keyboard,
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

/// The window's shape for [`Chrome::layout`], in logical points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// The picture's place in the window.
    pub picture: Rect,
    /// The window's usable part.
    pub safe: Rect,
    /// Logical points to a millimetre.
    pub per_mm: f32,
    /// A game is on (pause, the latches and the right-click toggle only make sense then).
    pub in_game: bool,
}

/// The chrome's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Chrome {
    /// Where each control is now.
    items: Vec<(Control, Rect)>,
    /// The latched modifiers, in [`Control`] order Shift, Ctrl, Alt.
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Taps are right clicks.
    pub right_click: bool,
    /// The soft keyboard is open.
    pub keyboard: bool,
    /// Fingers the chrome owns: the control each went down on.
    owned: Vec<(u64, Control)>,
}

impl Chrome {
    /// Places the controls for a window shaped like `f`.
    pub fn layout(&mut self, f: Frame) {
        self.items.clear();
        let unit = (BUTTON_MM * f.per_mm).clamp(MIN_POINTS, MAX_POINTS);
        let gap = (unit / 6.0).round();
        let left_bar = (f.picture.x - f.safe.x).max(0.0);
        let right_bar = (f.safe.x + f.safe.w - (f.picture.x + f.picture.w)).max(0.0);
        let fits = |bar: f32| bar >= BAR_FRACTION * unit;
        let (mut left, mut right): (Vec<Control>, Vec<Control>) = (Vec::new(), Vec::new());
        if f.in_game {
            left.extend([Control::Shift, Control::Ctrl, Control::Alt, Control::RightClick]);
            right.push(Control::Pause);
        }
        right.push(Control::Keyboard);
        // One column in the middle of the bar's height, buttons as wide as the bar allows.
        let column = |items: &mut Vec<(Control, Rect)>, list: &[Control], x0: f32, bar: f32| {
            let size = unit.min(bar - gap).max(8.0);
            let x = x0 + ((bar - size) / 2.0).max(0.0);
            let total = list.len() as f32 * size + (list.len() as f32 - 1.0).max(0.0) * gap;
            let mut y = f.safe.y + ((f.safe.h - total) / 2.0).max(0.0);
            for &c in list {
                items.push((c, Rect { x, y, w: size, h: size }));
                y += size + gap;
            }
        };
        match (fits(left_bar), fits(right_bar)) {
            (true, true) => {
                column(&mut self.items, &left, f.safe.x, left_bar);
                column(&mut self.items, &right, f.picture.x + f.picture.w, right_bar);
            }
            (true, false) => {
                let all: Vec<Control> = left.iter().chain(&right).copied().collect();
                column(&mut self.items, &all, f.safe.x, left_bar);
            }
            (false, true) => {
                let all: Vec<Control> = left.iter().chain(&right).copied().collect();
                column(&mut self.items, &all, f.picture.x + f.picture.w, right_bar);
            }
            (false, false) => {
                // Over the picture's left edge.
                let all: Vec<Control> = left.iter().chain(&right).copied().collect();
                column(&mut self.items, &all, f.picture.x.max(f.safe.x), unit + gap);
            }
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

    /// Whether a control is on now.
    pub fn is_on(&self, c: Control) -> bool {
        match c {
            Control::Shift => self.shift,
            Control::Ctrl => self.ctrl,
            Control::Alt => self.alt,
            Control::RightClick => self.right_click,
            Control::Keyboard => self.keyboard,
            Control::Pause => false,
        }
    }

    /// A finger's event: the chrome takes the fingers that go down on a control.
    pub fn feed(&mut self, t: Touch) -> Fed {
        match t.phase {
            TouchPhase::Began => match self.hit(t.pos) {
                Some(c) => {
                    self.owned.push((t.id, c));
                    Fed::Consumed
                }
                None => Fed::Pass,
            },
            TouchPhase::Moved => {
                if self.owned.iter().any(|&(id, _)| id == t.id) {
                    Fed::Consumed
                } else {
                    Fed::Pass
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                let Some(at) = self.owned.iter().position(|&(id, _)| id == t.id) else {
                    return Fed::Pass;
                };
                let (_, c) = self.owned.remove(at);
                if t.phase == TouchPhase::Ended && self.hit(t.pos) == Some(c) {
                    Fed::Pressed(c)
                } else {
                    Fed::Consumed
                }
            }
        }
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
        let rows: [&str; 5] = match c {
            Control::Shift => ["###", "#..", "###", "..#", "###"],
            Control::Ctrl => ["###", "#..", "#..", "#..", "###"],
            Control::Alt => [".#.", "#.#", "###", "#.#", "#.#"],
            Control::RightClick => ["##.", "#.#", "##.", "#.#", "#.#"],
            Control::Pause => ["#.#", "#.#", "#.#", "#.#", "#.#"],
            Control::Keyboard => ["#.#", "#.#", "##.", "#.#", "#.#"],
        };
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

/// A window's [`Frame`] for the chrome from the pieces the core has: the picture's rectangle (physical pixels) and
/// the safe area (physical pixels), in logical points.
pub fn frame_of(picture: (u32, u32, u32, u32), safe: Area, factor: f32, per_mm: f32, in_game: bool) -> Frame {
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
        }
    }

    fn rect_of(c: &Chrome, k: Control) -> Rect {
        c.items()
            .iter()
            .find(|&&(x, _)| x == k)
            .map(|&(_, r)| r)
            .expect("the control is laid out")
    }

    #[test]
    fn the_controls_sit_in_the_bars_and_clear_the_picture() {
        let mut c = Chrome::default();
        let f = wide(true);
        c.layout(f);
        assert_eq!(c.items().len(), 6);
        let (sh, pause) = (rect_of(&c, Control::Shift), rect_of(&c, Control::Pause));
        assert!(sh.x + sh.w <= f.picture.x, "left controls in the left bar");
        assert!(pause.x >= f.picture.x + f.picture.w, "right controls in the right bar");
        for &(_, r) in c.items() {
            assert!(r.w >= 8.0 && r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= 1000.0 && r.y + r.h <= 600.0);
            assert!(r.w <= MAX_POINTS && r.w >= MIN_POINTS.min(f.picture.x));
        }
        // No two overlap.
        let all = c.items();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                let (a, b) = (a.1, b.1);
                assert!(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y);
            }
        }
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
        assert_eq!(c.items().len(), 6);
        assert!(c.items().iter().all(|(_, r)| r.x + r.w <= f.picture.x));
    }

    #[test]
    fn no_bar_puts_the_controls_over_the_picture_edge() {
        let mut c = Chrome::default();
        let mut f = wide(true);
        f.picture = Rect {
            x: 0.0,
            y: 60.0,
            w: 1000.0,
            h: 480.0,
        };
        c.layout(f);
        assert_eq!(c.items().len(), 6);
        assert!(c.items().iter().all(|(_, r)| r.x + r.w <= 1000.0 && r.y >= 0.0));
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
