//! The core's touch step (`docs/design/touch.md`): the glue between the fingers' events ([`Input::touches`]), the chrome
//! ([`crate::chrome`]), the gesture machine ([`crate::gesture`]) and the mouse-and-keyboard input the rest of the core
//! reads. It runs in the frame's input step, after a scripted run's input is in and before the frame's events are
//! applied, and does nothing at all until a finger appears (or `--touch-emulation` turns the mouse into one), so
//! without touch every frame is exactly what it was.
//!
//! **What it hands the game.**
//! - Buttons and keys, through [`Input::button`] and [`Input::key`], one gesture frame a game frame.
//! - The pointer ([`TouchState::place_pointer`]): where the gesture last put it while a finger drags or a click is in
//!   progress, nowhere otherwise (a lifted finger leaves no cursor to scroll the edge of the map).
//! - A pan and a zoom for the map view (`game::touch_view`), or, outside the game, the pan as mouse-wheel lines for
//!   lists.
//! - The tap tolerance ([`Input::tap_radius`]) for the frame a tap's left button comes up.
//! - A pulse where a long press fired, drawn onto the picture after it is composed ([`draw_pulse`]).
//!
//! [`Input::touches`]: crate::input::Input::touches
//! [`Input::button`]: crate::input::Input::button
//! [`Input::key`]: crate::input::Input::key
//! [`Input::tap_radius`]: crate::input::Input::tap_radius

use std::time::Duration;

use crate::chrome::{self, Chrome, Control, Fed, Overlay};
use crate::core::{Core, game_running};
use crate::gesture::{Gesture, Pt};
use crate::input::{Key, KeyEvent, Touch, Wheel, WindowGeom};
use crate::touchemu::Emulator;

/// How far a tap may miss a unit and still select it, in millimetres.
pub const TAP_MM: f32 = 6.0;
/// How far fingers travel to scroll a list by a line, in millimetres.
pub const LINE_MM: f32 = 7.0;
/// How long the long-press pulse shows, in milliseconds.
pub const PULSE_MS: f32 = 400.0;

/// The core's touch state.
#[derive(Default)]
pub struct TouchState {
    /// The mouse is a finger (`--touch-emulation`).
    pub emulate: bool,
    /// Touch is in use: the chrome shows and the pointer is the fingers'.
    pub active: bool,
    gesture: Gesture,
    pub chrome: Chrome,
    emu: Emulator,
    pointer: Option<Pt>,
    /// The pulse: where, and how old in milliseconds.
    pulse: Option<(Pt, f32)>,
    /// The map view's pan and zoom not yet taken by the game, in window points.
    pan: Pt,
    zoom: Option<(f32, Pt)>,
    /// Map pixels panned short of a tile (`game::touch_view`).
    pub view_acc: Pt,
    wheel_acc: f32,
    /// The latched modifiers come up at the next step.
    release_mods: bool,
    pause: bool,
    /// The platform's pointer the last frame, to see a mouse come back.
    last_platform: Option<Option<Pt>>,
    /// The chrome as the shell last drew it, and a count of its changes.
    drawn: Chrome,
    pub revision: u64,
}

impl TouchState {
    /// The state for a run: `emulate` makes the mouse a finger from the start.
    pub fn new(emulate: bool) -> Self {
        TouchState {
            emulate,
            active: emulate,
            ..Default::default()
        }
    }

    /// The chrome's picture for a window, `None` while touch is not in use. Draw it over the picture; it changes only
    /// when [`TouchState::revision`] does.
    pub fn overlay(&self, size: (u32, u32), factor: f32) -> Option<Overlay> {
        self.active.then(|| self.chrome.overlay(size, factor))
    }

    /// The map view's pan (window points) and zoom (a factor and its centre in window points) since the last call.
    pub fn take_view(&mut self) -> (Pt, Option<(f32, Pt)>) {
        (std::mem::take(&mut self.pan), self.zoom.take())
    }

    /// Whether the pause control was pressed since the last call.
    pub fn take_pause(&mut self) -> bool {
        std::mem::take(&mut self.pause)
    }

    /// The pointer after a platform's or a scene's (called once the frame's pointer is set): while touch is in use it
    /// is the fingers'. A real mouse that moves with no finger down takes the pointer back.
    pub fn place_pointer(&mut self, pointer: &mut Option<Pt>, scripted: bool) {
        if !self.active {
            return;
        }
        if !self.emulate && !scripted {
            let last = self.last_platform.replace(*pointer);
            if last.is_some_and(|l| l != *pointer) && !self.gesture.touching() && self.pointer.is_none() {
                self.active = false;
                self.last_platform = None;
                return;
            }
        }
        *pointer = self.pointer;
    }

    fn press(&mut self, c: Control, keys: &mut Vec<(Key, bool)>) {
        match c {
            Control::Shift => {
                self.chrome.shift = !self.chrome.shift;
                keys.push((Key::ShiftLeft, self.chrome.shift));
            }
            Control::Ctrl => {
                self.chrome.ctrl = !self.chrome.ctrl;
                keys.push((Key::ControlLeft, self.chrome.ctrl));
            }
            Control::Alt => {
                self.chrome.alt = !self.chrome.alt;
                keys.push((Key::AltLeft, self.chrome.alt));
            }
            Control::RightClick => self.chrome.right_click = !self.chrome.right_click,
            Control::Pause => self.pause = true,
            Control::Keyboard => self.chrome.keyboard = !self.chrome.keyboard,
        }
    }
}

/// One frame's touch step; see the module doc.
pub fn step(c: &mut Core, elapsed: Duration, window: Option<WindowGeom>) {
    c.input.tap_radius = None;
    let fresh = !c.input.touches.is_empty() || !c.input.mouse_touches.is_empty();
    if !(c.touch.active || fresh) {
        return;
    }
    let in_game = game_running(c);
    let raw = std::mem::take(&mut c.input.touches);
    let mice = std::mem::take(&mut c.input.mouse_touches);
    let Some(w) = window else { return };
    let lb = w.letterbox();
    let ppmm = w.points_per_mm();
    let Core {
        touch: t,
        input,
        script,
        ..
    } = c;
    t.active = true;
    let ms = elapsed.as_secs_f32() * 1000.0;

    t.chrome
        .layout(chrome::frame_of(lb.screen(), w.safe, w.factor, ppmm, in_game));
    let mut keys: Vec<(Key, bool)> = Vec::new();
    // The latched modifiers come up a frame after the click they were for.
    if std::mem::take(&mut t.release_mods) {
        for (on, k) in [
            (&mut t.chrome.shift, Key::ShiftLeft),
            (&mut t.chrome.ctrl, Key::ControlLeft),
            (&mut t.chrome.alt, Key::AltLeft),
        ] {
            if std::mem::take(on) {
                keys.push((k, false));
            }
        }
    }

    // The fingers: the screen's, then the mouse's if emulated; the chrome takes its own.
    let mut fingers: Vec<Touch> = raw;
    if !input.focused && script.is_none() {
        // The window lost the keyboard: whatever is held lets go.
        fingers.extend(t.emu.cancel());
        t.gesture.reset();
    }
    for m in mice {
        fingers.extend(t.emu.feed(m, lb.centre(w.factor), 5.0 * ppmm));
    }
    let mut passed = Vec::new();
    for f in fingers {
        match t.chrome.feed(f) {
            Fed::Pass => passed.push(f),
            Fed::Consumed => {}
            Fed::Pressed(ctl) => t.press(ctl, &mut keys),
        }
    }

    // The gesture machine, one of its frames a game frame.
    t.gesture.update(elapsed, &passed, ppmm, t.chrome.right_click);
    if let Some((_, age)) = &mut t.pulse {
        *age += ms;
        if *age >= PULSE_MS {
            t.pulse = None;
        }
    }
    let frame = t.gesture.pop();
    let mut released = false;
    match frame {
        Some(f) => {
            keys.extend(f.keys.iter().copied());
            for &(b, down) in &f.buttons {
                input.button(b, down);
                released |= !down;
            }
            if f.pointer.is_some() {
                t.pointer = f.pointer;
            } else if !t.gesture.dragging() {
                t.pointer = None;
            }
            if f.tap {
                input.tap_radius = Some(TAP_MM * ppmm * w.factor / lb.scale);
            }
            if let Some(p) = f.pulse {
                t.pulse = Some((p, 0.0));
            }
            t.pan.0 += f.pan.0;
            t.pan.1 += f.pan.1;
            if let Some((k, at)) = f.zoom {
                t.zoom = Some((t.zoom.map_or(1.0, |z| z.0) * k, at));
            }
        }
        None => {
            if !t.gesture.dragging() {
                t.pointer = None;
            }
        }
    }
    t.release_mods |= released;
    for (k, down) in keys {
        input.key(KeyEvent {
            key: k,
            pressed: down,
            text: None,
        });
    }

    // Outside a game the pan scrolls lists, as the wheel does; a pinch means nothing there.
    if !in_game {
        t.wheel_acc += t.pan.1 / (LINE_MM * ppmm);
        let n = t.wheel_acc.trunc();
        if n != 0.0 {
            t.wheel_acc -= n;
            input.wheel(Wheel { y: n, lines: true });
        }
        t.pan = (0.0, 0.0);
        t.zoom = None;
    }
    if t.chrome != t.drawn {
        t.drawn = t.chrome.clone();
        t.revision += 1;
    }
}

/// Draws the long-press pulse (a ring that grows and fades) onto the composed picture.
pub fn draw_pulse(c: &mut Core) {
    let Some((at, age)) = c.touch.pulse else { return };
    let Some(w) = c.input.window else { return };
    let (cx, cy) = w.letterbox().screen_of(at, w.factor);
    let t = (age / PULSE_MS).clamp(0.0, 1.0);
    let radius = 4.0 + 26.0 * t;
    let strength = (1.0 - t) * 0.85;
    let (width, height) = (crate::compose::W as i32, crate::compose::H as i32);
    let reach = (radius + 2.0).ceil() as i32;
    for y in (cy as i32 - reach).max(0)..=(cy as i32 + reach).min(height - 1) {
        for x in (cx as i32 - reach).max(0)..=(cx as i32 + reach).min(width - 1) {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let edge = 1.0 - (d - radius).abs() / 1.5;
            if edge > 0.0 {
                let a = (edge * strength).clamp(0.0, 1.0);
                let i = (y as usize * width as usize + x as usize) * 4;
                for (ch, ink) in c.frame.rgba[i..i + 3].iter_mut().zip([255.0, 240.0, 160.0]) {
                    *ch = (*ch as f32 + (ink - *ch as f32) * a).round() as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_touch_the_pointer_is_left_alone() {
        let mut t = TouchState::new(false);
        let mut p = Some((10.0, 20.0));
        t.place_pointer(&mut p, false);
        assert_eq!(p, Some((10.0, 20.0)));
        assert!(!t.active);
    }

    #[test]
    fn with_touch_the_pointer_is_the_fingers_and_nowhere_when_none_is_down() {
        let mut t = TouchState::new(true);
        let mut p = Some((10.0, 20.0));
        t.place_pointer(&mut p, false);
        assert_eq!(p, None, "an emulated finger up leaves no cursor behind");
        t.pointer = Some((5.0, 6.0));
        t.place_pointer(&mut p, false);
        assert_eq!(p, Some((5.0, 6.0)));
    }

    #[test]
    fn a_mouse_that_moves_with_no_finger_down_takes_the_pointer_back() {
        let mut t = TouchState::new(false);
        t.active = true;
        let mut p = Some((1.0, 1.0));
        t.place_pointer(&mut p, false);
        assert!(t.active, "the first look only learns where the mouse is");
        let mut p = Some((9.0, 9.0));
        t.place_pointer(&mut p, false);
        assert!(!t.active);
        assert_eq!(p, Some((9.0, 9.0)));
        // A scripted run's pointer never counts as a mouse.
        t.active = true;
        let mut p = Some((3.0, 3.0));
        t.place_pointer(&mut p, true);
        assert!(t.active);
    }

    #[test]
    fn the_latches_toggle_their_keys_and_the_toggles_flip() {
        let mut t = TouchState::new(true);
        let mut keys = Vec::new();
        t.press(Control::Shift, &mut keys);
        t.press(Control::Alt, &mut keys);
        t.press(Control::Shift, &mut keys);
        assert_eq!(
            keys,
            [(Key::ShiftLeft, true), (Key::AltLeft, true), (Key::ShiftLeft, false)]
        );
        assert!(t.chrome.alt && !t.chrome.shift);
        t.press(Control::RightClick, &mut keys);
        t.press(Control::Keyboard, &mut keys);
        t.press(Control::Pause, &mut keys);
        assert!(t.chrome.right_click && t.chrome.keyboard);
        assert!(t.take_pause() && !t.take_pause());
    }
}
