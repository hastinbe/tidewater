//! A frame's input from the player, as the core reads it (`docs/design/client-core.md`, "Input model", migration
//! step 4): the keys and mouse buttons held and changed this frame, the typed key events and the wheel. The shell
//! (or a scripted run) puts the system's events in with [`Input::key`], [`Input::button`] and [`Input::wheel`]; the
//! core reads them back. Nothing here depends on an engine.
//!
//! The rules are the ones the client had from Bevy's input, so that it behaves as before:
//! - [`Buttons`]: a key or button pressed this frame is "just pressed" for the frame's systems, then only "pressed";
//!   [`Input::begin_frame`] applies the frame's events in the order they came. A system may take a key away from the
//!   others ([`Buttons::reset`]), as the chat line takes the typing keys.
//! - [`Log`]: the typed key events and wheel steps stay readable for two updates of the log
//!   ([`Input::update_logs`]), and each reader keeps its own place ([`LogCursor`]), so a reader that starts late still
//!   sees what came in just before.
//! - The pointer isn't here yet: it still goes through the mouse lock and the letterbox (`frontend::pointer`).

use std::collections::HashSet;

/// A key on the keyboard, by its place (the US layout's name for it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Letter(char),
    Digit(u8),
    Numpad(u8),
    NumpadAdd,
    NumpadSubtract,
    NumpadEnter,
    F(u8),
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Escape,
    Backspace,
    Space,
    Minus,
    Equal,
    PrintScreen,
    CapsLock,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
    /// Any other key, by a number the shell gives it.
    Other(u32),
}

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Left,
    Right,
    Middle,
    Other(u16),
}

/// Keys or buttons: held, and pressed or released this frame.
#[derive(Clone, Debug)]
pub struct Buttons<T> {
    pressed: HashSet<T>,
    just_pressed: Vec<T>,
    just_released: Vec<T>,
}

impl<T> Default for Buttons<T> {
    fn default() -> Self {
        Buttons {
            pressed: HashSet::new(),
            just_pressed: Vec::new(),
            just_released: Vec::new(),
        }
    }
}

impl<T: Copy + Eq + std::hash::Hash> Buttons<T> {
    /// A press: just pressed unless it was held already.
    pub fn press(&mut self, b: T) {
        if self.pressed.insert(b) && !self.just_pressed.contains(&b) {
            self.just_pressed.push(b);
        }
    }

    /// A release: just released if it was held.
    pub fn release(&mut self, b: T) {
        if self.pressed.remove(&b) && !self.just_released.contains(&b) {
            self.just_released.push(b);
        }
    }

    /// Releases everything held (the window lost the keyboard).
    pub fn release_all(&mut self) {
        let held: Vec<T> = self.pressed.iter().copied().collect();
        for b in held {
            self.release(b);
        }
    }

    pub fn pressed(&self, b: T) -> bool {
        self.pressed.contains(&b)
    }

    pub fn just_pressed(&self, b: T) -> bool {
        self.just_pressed.contains(&b)
    }

    pub fn just_released(&self, b: T) -> bool {
        self.just_released.contains(&b)
    }

    pub fn any_pressed(&self, bs: impl IntoIterator<Item = T>) -> bool {
        bs.into_iter().any(|b| self.pressed(b))
    }

    pub fn any_just_pressed(&self, bs: impl IntoIterator<Item = T>) -> bool {
        bs.into_iter().any(|b| self.just_pressed(b))
    }

    /// The keys held, in no particular order.
    pub fn get_pressed(&self) -> impl Iterator<Item = &T> {
        self.pressed.iter()
    }

    /// The keys pressed this frame, in the order they were pressed.
    pub fn get_just_pressed(&self) -> impl Iterator<Item = &T> {
        self.just_pressed.iter()
    }

    /// Takes a key away: no longer held, pressed or released for the systems after this one.
    pub fn reset(&mut self, b: T) {
        self.pressed.remove(&b);
        self.just_pressed.retain(|&x| x != b);
        self.just_released.retain(|&x| x != b);
    }

    /// The start of a frame: nothing is just pressed or released yet.
    pub fn clear(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }
}

/// A key going down or up, with the text it types.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyEvent {
    pub key: Key,
    pub pressed: bool,
    pub text: Option<String>,
}

/// A step of the mouse wheel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    /// Positive: away from the user.
    pub y: f32,
    /// In lines (a mouse's notches) rather than pixels (a touchpad's).
    pub lines: bool,
}

/// Events kept for two updates, each with its number.
#[derive(Clone, Debug)]
pub struct Log<T> {
    older: Vec<(usize, T)>,
    newer: Vec<(usize, T)>,
    count: usize,
}

impl<T> Default for Log<T> {
    fn default() -> Self {
        Log {
            older: Vec::new(),
            newer: Vec::new(),
            count: 0,
        }
    }
}

/// A reader's place in a [`Log`]: the number of the first event it hasn't read.
#[derive(Clone, Copy, Debug, Default)]
pub struct LogCursor(usize);

impl<T> Log<T> {
    pub fn push(&mut self, e: T) {
        self.newer.push((self.count, e));
        self.count += 1;
    }

    /// The oldest events go; the newer ones become the older.
    pub fn update(&mut self) {
        std::mem::swap(&mut self.older, &mut self.newer);
        self.newer.clear();
    }

    /// Every event goes (their numbers aren't used again).
    pub fn clear(&mut self) {
        self.older.clear();
        self.newer.clear();
    }

    /// The events `cursor` hasn't read yet, oldest first; it has read them all afterwards.
    pub fn read<'a>(&'a self, cursor: &mut LogCursor) -> impl Iterator<Item = &'a T> + 'a {
        let from = cursor.0;
        cursor.0 = self.count;
        self.older
            .iter()
            .chain(&self.newer)
            .filter(move |(n, _)| *n >= from)
            .map(|(_, e)| e)
    }
}

/// The player's input.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub keys: Buttons<Key>,
    pub mouse: Buttons<Button>,
    pub typed: Log<KeyEvent>,
    pub wheel: Log<Wheel>,
    /// The wheel this frame, summed.
    pub scroll: f32,
    /// Whether the last of this frame's wheel steps was in pixels.
    pub scroll_pixels: bool,
    /// This frame's events, applied to the keys and buttons by [`Input::begin_frame`].
    pending: Vec<Event>,
}

#[derive(Clone, Debug)]
enum Event {
    Key(Key, bool),
    Button(Button, bool),
    Wheel(Wheel),
    FocusLost,
}

impl Input {
    /// A key went down or up.
    pub fn key(&mut self, e: KeyEvent) {
        self.pending.push(Event::Key(e.key, e.pressed));
        self.typed.push(e);
    }

    /// A mouse button went down or up.
    pub fn button(&mut self, b: Button, pressed: bool) {
        self.pending.push(Event::Button(b, pressed));
    }

    /// The wheel turned.
    pub fn wheel(&mut self, w: Wheel) {
        self.pending.push(Event::Wheel(w));
        self.wheel.push(w);
    }

    /// The window lost the keyboard: every key held is released.
    pub fn focus_lost(&mut self) {
        self.pending.push(Event::FocusLost);
    }

    /// Drops every event not yet applied or read: a scripted run's input replaces the system's.
    pub fn clear_events(&mut self) {
        self.pending.clear();
        self.typed.clear();
        self.wheel.clear();
    }

    /// The typed events and wheel steps age by one update (see the module doc).
    pub fn update_logs(&mut self) {
        self.typed.update();
        self.wheel.update();
    }

    /// The start of a frame's systems: the events since the last frame are applied in order.
    pub fn begin_frame(&mut self) {
        self.keys.clear();
        self.mouse.clear();
        self.scroll = 0.0;
        self.scroll_pixels = false;
        let mut lost = false;
        for e in std::mem::take(&mut self.pending) {
            match e {
                Event::Key(k, true) => self.keys.press(k),
                Event::Key(k, false) => self.keys.release(k),
                Event::Button(b, true) => self.mouse.press(b),
                Event::Button(b, false) => self.mouse.release(b),
                Event::Wheel(w) => {
                    self.scroll += w.y;
                    self.scroll_pixels = !w.lines;
                }
                Event::FocusLost => lost = true,
            }
        }
        // After the frame's key events, as they may have come in before the window lost the keyboard.
        if lost {
            self.keys.release_all();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(k: Key) -> KeyEvent {
        KeyEvent {
            key: k,
            pressed: true,
            text: None,
        }
    }

    fn up(k: Key) -> KeyEvent {
        KeyEvent {
            key: k,
            pressed: false,
            text: None,
        }
    }

    #[test]
    fn a_key_is_just_pressed_for_one_frame() {
        let mut i = Input::default();
        i.key(down(Key::Enter));
        assert!(!i.keys.pressed(Key::Enter), "not before the frame starts");
        i.begin_frame();
        assert!(i.keys.just_pressed(Key::Enter) && i.keys.pressed(Key::Enter));
        i.begin_frame();
        assert!(!i.keys.just_pressed(Key::Enter) && i.keys.pressed(Key::Enter));
        i.key(up(Key::Enter));
        i.begin_frame();
        assert!(i.keys.just_released(Key::Enter) && !i.keys.pressed(Key::Enter));
    }

    #[test]
    fn a_tap_within_one_frame_is_pressed_and_released() {
        let mut i = Input::default();
        i.key(down(Key::Letter('A')));
        i.key(up(Key::Letter('A')));
        i.begin_frame();
        assert!(i.keys.just_pressed(Key::Letter('A')));
        assert!(i.keys.just_released(Key::Letter('A')));
        assert!(!i.keys.pressed(Key::Letter('A')));
    }

    #[test]
    fn keys_pressed_together_come_in_order() {
        let mut i = Input::default();
        for c in ['C', 'A', 'B'] {
            i.key(down(Key::Letter(c)));
        }
        i.begin_frame();
        let got: Vec<Key> = i.keys.get_just_pressed().copied().collect();
        assert_eq!(got, [Key::Letter('C'), Key::Letter('A'), Key::Letter('B')]);
    }

    #[test]
    fn a_reset_key_is_gone_for_the_rest_of_the_frame() {
        let mut i = Input::default();
        i.key(down(Key::Enter));
        i.begin_frame();
        i.keys.reset(Key::Enter);
        assert!(!i.keys.pressed(Key::Enter) && !i.keys.just_pressed(Key::Enter));
        i.key(up(Key::Enter));
        i.begin_frame();
        assert!(!i.keys.just_released(Key::Enter), "a key no longer held isn't released");
    }

    #[test]
    fn losing_focus_releases_every_key() {
        let mut i = Input::default();
        i.key(down(Key::ShiftLeft));
        i.begin_frame();
        i.focus_lost();
        i.key(down(Key::Space));
        i.begin_frame();
        assert!(!i.keys.pressed(Key::ShiftLeft) && i.keys.just_released(Key::ShiftLeft));
        assert!(!i.keys.pressed(Key::Space), "a key pressed in the same frame goes too");
    }

    #[test]
    fn the_logs_keep_events_for_two_updates() {
        let mut log = Log::default();
        let (mut early, mut late) = (LogCursor::default(), LogCursor::default());
        log.push(1);
        assert_eq!(log.read(&mut early).copied().collect::<Vec<_>>(), [1]);
        log.update();
        log.push(2);
        assert_eq!(log.read(&mut early).copied().collect::<Vec<_>>(), [2]);
        // A reader that starts now still sees both.
        assert_eq!(log.read(&mut late).copied().collect::<Vec<_>>(), [1, 2]);
        log.update();
        log.update();
        log.push(3);
        assert_eq!(log.read(&mut LogCursor::default()).copied().collect::<Vec<_>>(), [3]);
    }

    #[test]
    fn the_wheel_is_summed_per_frame() {
        let mut i = Input::default();
        i.wheel(Wheel { y: 1.0, lines: true });
        i.wheel(Wheel { y: 2.0, lines: true });
        i.begin_frame();
        assert_eq!((i.scroll, i.scroll_pixels), (3.0, false));
        i.wheel(Wheel { y: 5.0, lines: false });
        i.begin_frame();
        assert_eq!((i.scroll, i.scroll_pixels), (5.0, true));
        i.begin_frame();
        assert_eq!((i.scroll, i.scroll_pixels), (0.0, false));
    }
}
