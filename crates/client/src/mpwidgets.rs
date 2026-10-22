//! Small controls the multiplayer screens share (`mpmenu`, `lobbyscreen`): a text box, a drop-down's open list, a
//! panel, and a pointer that tracks the hot and the held control. SDL-free; drawn into the menu picture in its palette
//! with the same ring colours the buttons use (`menuscreen`).

use crate::customsetup::{Input, Look};
use crate::hud::{self, Rect};
use crate::input::{Key, KeyEvent};
use crate::menuscreen::{self, Hold};
use crate::scene::Picture;

/// What a text box accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Charset {
    /// Printable ASCII; a leading space is ignored (the player's name).
    Name,
    /// Printable ASCII without spaces (an address).
    Address,
    /// Decimal digits (a port).
    Digits,
}

/// A one-line text box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EditBox {
    pub text: String,
    pub max: usize,
    pub set: Charset,
}

impl EditBox {
    pub(crate) fn new(text: &str, max: usize, set: Charset) -> EditBox {
        let mut b = EditBox {
            text: String::new(),
            max,
            set,
        };
        b.type_str(text);
        b
    }

    /// Types `s` at the end: characters the set accepts, up to the limit.
    pub(crate) fn type_str(&mut self, s: &str) {
        for c in s.chars() {
            let ok = match self.set {
                Charset::Name => (' '..='~').contains(&c) && !(c == ' ' && self.text.is_empty()),
                Charset::Address => ('!'..='~').contains(&c),
                Charset::Digits => c.is_ascii_digit(),
            };
            if ok && self.text.chars().count() < self.max {
                self.text.push(c);
            }
        }
    }

    /// Applies a key event: Backspace and text are the box's; Enter, Escape and Tab are handed back.
    pub(crate) fn feed(&mut self, e: &KeyEvent, command_held: bool) -> Option<Key> {
        if !e.pressed {
            return None;
        }
        match e.key {
            Key::Enter | Key::NumpadEnter | Key::Escape | Key::Tab => return Some(e.key),
            Key::Backspace => {
                self.text.pop();
            }
            _ if command_held => {}
            _ => {
                if let Some(t) = &e.text {
                    self.type_str(t);
                }
            }
        }
        None
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Draws a text box: black with a ring (yellow with the focus), the text and, with the focus, a caret.
pub(crate) fn draw_edit(look: &Look, p: &mut Picture, r: Rect, b: &EditBox, focused: bool) {
    let rgb = |e: u8| look.pal.0[e as usize];
    hud::fill(p, r, rgb(menuscreen::BLACK));
    hud::outline(
        p,
        r,
        rgb(if focused {
            menuscreen::FOCUS_RING
        } else {
            *menuscreen::OUTER_RING.start()
        }),
    );
    let mut s = b.text.clone();
    if focused {
        s.push('_');
    }
    let y = r.y + (r.h as i32 - look.font.height as i32) / 2;
    look.text(
        p,
        (r.x + 4, y),
        &crate::customsetup::fit_tail(look.font, &s, r.w.saturating_sub(8)),
        menuscreen::WHITE,
    );
}

/// A panel: black with the two rings of the small dialogs.
pub(crate) fn draw_panel(look: &Look, p: &mut Picture, r: Rect) {
    let rgb = |e: u8| look.pal.0[e as usize];
    hud::fill(p, r, rgb(menuscreen::BLACK));
    hud::outline(p, r, rgb(*menuscreen::OUTER_RING.start()));
    hud::outline(
        p,
        Rect::new(r.x + 1, r.y + 1, r.w.saturating_sub(2), r.h.saturating_sub(2)),
        rgb(menuscreen::SECOND_RING),
    );
}

/// The line height of an open drop-down list.
pub(crate) const POPUP_LINE: i32 = 20;

/// An open drop-down list under (or over) its box.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Popup {
    /// Which control opened it (the owner's own number).
    pub owner: usize,
    pub anchor: Rect,
    pub labels: Vec<String>,
    /// The highlighted line.
    pub at: usize,
}

/// What a key did to an open list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PopupKey {
    Open,
    Closed,
    Picked(usize),
}

impl Popup {
    pub(crate) fn new(owner: usize, anchor: Rect, labels: Vec<String>, at: usize) -> Popup {
        Popup {
            owner,
            anchor,
            labels,
            at,
        }
    }

    pub(crate) fn rect(&self) -> Rect {
        let b = self.anchor;
        let h = self.labels.len() as u32 * POPUP_LINE as u32 + 2;
        let below = b.y + b.h as i32;
        let y = if below + h as i32 > 478 {
            (b.y - h as i32).max(0)
        } else {
            below
        };
        Rect::new(b.x, y, b.w, h)
    }

    pub(crate) fn row_rect(&self, i: usize) -> Rect {
        let r = self.rect();
        Rect::new(r.x + 1, r.y + 1 + i as i32 * POPUP_LINE, r.w - 2, POPUP_LINE as u32)
    }

    /// The line at a screen point.
    #[allow(dead_code)]
    pub(crate) fn row_at(&self, x: i32, y: i32) -> Option<usize> {
        (0..self.labels.len()).find(|&i| self.row_rect(i).contains(x as f32, y as f32))
    }

    pub(crate) fn key(&mut self, k: Key) -> PopupKey {
        let n = self.labels.len();
        match k {
            Key::Escape => return PopupKey::Closed,
            Key::Up => self.at = self.at.saturating_sub(1),
            Key::Down => self.at = (self.at + 1).min(n - 1),
            Key::Home => self.at = 0,
            Key::End => self.at = n - 1,
            Key::Enter | Key::NumpadEnter | Key::Space => return PopupKey::Picked(self.at),
            _ => {}
        }
        PopupKey::Open
    }
}

/// Draws an open list; `hot` is the line under the pointer.
pub(crate) fn draw_popup(look: &Look, p: &mut Picture, pop: &Popup, hot: Option<usize>) {
    let rgb = |e: u8| look.pal.0[e as usize];
    let r = pop.rect();
    hud::fill(p, r, rgb(menuscreen::BLACK));
    hud::outline(p, r, rgb(*menuscreen::OUTER_RING.start()));
    for (i, l) in pop.labels.iter().enumerate() {
        let row = pop.row_rect(i);
        let on = pop.at == i || hot == Some(i);
        if on {
            hud::fill(p, row, rgb(162));
        }
        let map = if on { menuscreen::WHITE } else { menuscreen::GOLD };
        look.text(
            p,
            (row.x + 4, row.y + 2),
            &crate::customsetup::fit(look.font, l, row.w - 8),
            map,
        );
    }
}

/// The pointer on a screen's controls: which is under it and which the left button went down on.
#[derive(Debug)]
pub(crate) struct Ptr<T> {
    pub hot: Option<T>,
    pub held: Hold<T>,
}

impl<T> Default for Ptr<T> {
    fn default() -> Self {
        Ptr {
            hot: None,
            held: Hold::default(),
        }
    }
}

/// What the pointer did in a frame.
pub(crate) struct PtrEvents<T> {
    /// The control the button went down on.
    pub down: Option<T>,
    /// The control the button was released on after going down on it.
    pub clicked: Option<T>,
    /// The hot or pressed control changed (the picture needs drawing again).
    pub changed: bool,
}

impl<T: Copy + PartialEq> Ptr<T> {
    /// One frame over `targets`, topmost first.
    pub(crate) fn update(&mut self, targets: &[(T, Rect)], input: &Input) -> PtrEvents<T> {
        let (before_hot, before_pressed) = (self.hot, self.held.pressed(self.hot));
        self.hot = input.at.and_then(|(x, y)| {
            targets
                .iter()
                .find(|(_, r)| r.contains(x as f32, y as f32))
                .map(|&(t, _)| t)
        });
        let mut down = None;
        if input.down {
            self.held.down(self.hot);
            down = self.hot;
        }
        let clicked = if input.up { self.held.up(self.hot) } else { None };
        PtrEvents {
            down,
            clicked,
            changed: before_hot != self.hot || before_pressed != self.held.pressed(self.hot),
        }
    }

    pub(crate) fn pressed(&self, t: T) -> bool {
        self.held.pressed(self.hot) == Some(t)
    }

    pub(crate) fn is_hot(&self, t: T) -> bool {
        self.hot == Some(t)
    }

    /// Forgets the hold (a control the button went down on went away).
    pub(crate) fn reset(&mut self) {
        self.held = Hold::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: Key, text: Option<&str>) -> KeyEvent {
        KeyEvent {
            key: k,
            pressed: true,
            text: text.map(str::to_string),
        }
    }

    #[test]
    fn a_name_box_ignores_a_leading_space_and_stops_at_its_limit() {
        let mut b = EditBox::new("", 11, Charset::Name);
        b.feed(&key(Key::Space, Some(" ")), false);
        assert!(b.is_empty(), "a leading space is ignored");
        for c in "Ab cdefghijklm".chars() {
            b.feed(&key(Key::Other(0), Some(&c.to_string())), false);
        }
        assert_eq!(b.text, "Ab cdefghij", "11 characters, a space inside is kept");
        b.feed(&key(Key::Backspace, None), false);
        assert_eq!(b.text, "Ab cdefghi");
    }

    #[test]
    fn a_command_key_types_nothing_and_enter_is_handed_back() {
        let mut b = EditBox::new("x", 11, Charset::Name);
        assert_eq!(b.feed(&key(Key::Letter('A'), Some("a")), true), None);
        assert_eq!(b.text, "x");
        assert_eq!(b.feed(&key(Key::Enter, None), false), Some(Key::Enter));
        assert_eq!(b.feed(&key(Key::Tab, None), false), Some(Key::Tab));
    }

    #[test]
    fn address_and_port_boxes_filter_their_characters() {
        let mut a = EditBox::new("", 20, Charset::Address);
        a.type_str("10.0 .0:1");
        assert_eq!(a.text, "10.0.0:1");
        let mut p = EditBox::new("", 5, Charset::Digits);
        p.type_str("61a1234");
        assert_eq!(p.text, "61123");
    }

    #[test]
    fn an_open_list_moves_and_picks() {
        let mut p = Popup::new(
            0,
            Rect::new(10, 10, 100, 24),
            vec!["a".into(), "b".into(), "c".into()],
            0,
        );
        assert_eq!(p.key(Key::Down), PopupKey::Open);
        assert_eq!(p.key(Key::End), PopupKey::Open);
        assert_eq!(p.at, 2);
        assert_eq!(p.key(Key::Enter), PopupKey::Picked(2));
        assert_eq!(p.key(Key::Escape), PopupKey::Closed);
        assert_eq!(p.row_at(20, p.rect().y + 3), Some(0));
        assert_eq!(p.row_at(20, 400), None);
    }

    #[test]
    fn the_pointer_clicks_only_where_the_button_went_down() {
        let targets = [(1, Rect::new(0, 0, 10, 10)), (2, Rect::new(20, 0, 10, 10))];
        let mut p = Ptr::default();
        let at = |x| Input {
            at: Some((x, 5)),
            ..Default::default()
        };
        let e = p.update(&targets, &Input { down: true, ..at(5) });
        assert_eq!(e.down, Some(1));
        let e = p.update(&targets, &Input { up: true, ..at(25) });
        assert_eq!(e.clicked, None, "released elsewhere");
        p.update(&targets, &Input { down: true, ..at(25) });
        let e = p.update(&targets, &Input { up: true, ..at(25) });
        assert_eq!(e.clicked, Some(2));
    }
}
