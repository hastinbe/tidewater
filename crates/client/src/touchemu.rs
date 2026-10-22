//! Touch emulation for a desktop (`--touch-emulation`, `docs/design/touch.md`): the mouse acts as a finger, so every
//! touch path can be tried and scripted without a touchscreen.
//!
//! | Mouse | Fingers |
//! |---|---|
//! | Left button down, drag, up | One finger |
//! | Alt held at the press | A second finger, mirrored through the picture's centre (as in the iOS Simulator): drag outward to pinch out, sideways to pan |
//! | Alt and Shift | The second finger moves in parallel, a fixed distance beside the first: a two-finger pan |
//! | Alt and Ctrl | A third finger rests at the picture's centre: a three-finger tap on release |
//!
//! The modifiers are read when the button goes down and hold until it comes up, so letting go of Alt in mid-drag does
//! not strand a finger. The emulator makes [`Touch`] events only; the gesture machine cannot tell them from a screen's.

use crate::input::{MouseTouch, MouseTouchKind, Touch, TouchPhase};

/// Emulated fingers use ids from here up, clear of the ids a real screen gives.
pub const ID_BASE: u64 = 1 << 62;

#[derive(Clone, Copy, Debug, PartialEq)]
enum How {
    One,
    Mirror,
    Parallel,
    Three,
}

/// The mouse as fingers.
#[derive(Clone, Copy, Debug, Default)]
pub struct Emulator {
    held: Option<(How, [(f32, f32); 3])>,
}

impl Emulator {
    /// Whether the emulated button is down.
    #[cfg(test)]
    fn down(&self) -> bool {
        self.held.is_some()
    }

    /// The fingers' events for a mouse event. `centre`: the picture's middle in window points; `gap`: how far the
    /// parallel finger rides beside the first, in points.
    pub fn feed(&mut self, e: MouseTouch, centre: (f32, f32), gap: f32) -> Vec<Touch> {
        let finger = |n: u64, phase, pos| Touch {
            id: ID_BASE + n,
            phase,
            pos,
        };
        let mirror = |p: (f32, f32)| (2.0 * centre.0 - p.0, 2.0 * centre.1 - p.1);
        match (e.kind, self.held) {
            (MouseTouchKind::Down, None) => {
                let how = match (e.alt, e.shift, e.ctrl) {
                    (true, true, _) => How::Parallel,
                    (true, false, true) => How::Three,
                    (true, false, false) => How::Mirror,
                    _ => How::One,
                };
                let pos = Self::place(how, e.pos, centre, gap, mirror);
                self.held = Some((how, pos));
                (0..Self::count(how))
                    .map(|n| finger(n as u64, TouchPhase::Began, pos[n]))
                    .collect()
            }
            (MouseTouchKind::Move, Some((how, _))) => {
                let pos = Self::place(how, e.pos, centre, gap, mirror);
                self.held = Some((how, pos));
                (0..Self::count(how))
                    .map(|n| finger(n as u64, TouchPhase::Moved, pos[n]))
                    .collect()
            }
            (MouseTouchKind::Up, Some((how, last))) => {
                let pos = Self::place(how, e.pos, centre, gap, mirror);
                let _ = last;
                self.held = None;
                (0..Self::count(how))
                    .map(|n| finger(n as u64, TouchPhase::Ended, pos[n]))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// The emulated fingers are cancelled (the window lost the pointer).
    pub fn cancel(&mut self) -> Vec<Touch> {
        let Some((how, pos)) = self.held.take() else {
            return Vec::new();
        };
        (0..Self::count(how))
            .map(|n| Touch {
                id: ID_BASE + n as u64,
                phase: TouchPhase::Cancelled,
                pos: pos[n],
            })
            .collect()
    }

    fn count(how: How) -> usize {
        match how {
            How::One => 1,
            How::Mirror | How::Parallel => 2,
            How::Three => 3,
        }
    }

    fn place(
        how: How,
        p: (f32, f32),
        centre: (f32, f32),
        gap: f32,
        mirror: impl Fn((f32, f32)) -> (f32, f32),
    ) -> [(f32, f32); 3] {
        match how {
            How::One => [p, p, p],
            How::Mirror => [p, mirror(p), p],
            How::Parallel => [p, (p.0 + gap, p.1), p],
            // The first two fingers mirror each other, the third rests at the centre.
            How::Three => [p, mirror(p), centre],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: (f32, f32) = (320.0, 240.0);

    fn m(kind: MouseTouchKind, x: f32, y: f32, alt: bool, shift: bool, ctrl: bool) -> MouseTouch {
        MouseTouch {
            kind,
            pos: (x, y),
            alt,
            shift,
            ctrl,
        }
    }

    #[test]
    fn the_mouse_alone_is_one_finger() {
        let mut e = Emulator::default();
        let down = e.feed(m(MouseTouchKind::Down, 10.0, 20.0, false, false, false), C, 30.0);
        assert_eq!(down.len(), 1);
        assert_eq!(down[0].phase, TouchPhase::Began);
        assert_eq!(down[0].pos, (10.0, 20.0));
        assert_eq!(down[0].id, ID_BASE);
        let mv = e.feed(m(MouseTouchKind::Move, 12.0, 20.0, false, false, false), C, 30.0);
        assert_eq!((mv[0].phase, mv[0].pos), (TouchPhase::Moved, (12.0, 20.0)));
        let up = e.feed(m(MouseTouchKind::Up, 12.0, 20.0, false, false, false), C, 30.0);
        assert_eq!(up[0].phase, TouchPhase::Ended);
        assert!(!e.down());
        // Motion with the button up does nothing.
        assert!(
            e.feed(m(MouseTouchKind::Move, 1.0, 1.0, false, false, false), C, 30.0)
                .is_empty()
        );
    }

    #[test]
    fn alt_adds_a_finger_mirrored_through_the_centre() {
        let mut e = Emulator::default();
        let down = e.feed(m(MouseTouchKind::Down, 400.0, 200.0, true, false, false), C, 30.0);
        assert_eq!(down.len(), 2);
        assert_eq!(down[1].pos, (240.0, 280.0));
        assert_ne!(down[0].id, down[1].id);
        let mv = e.feed(m(MouseTouchKind::Move, 440.0, 200.0, true, false, false), C, 30.0);
        assert_eq!(mv[0].pos, (440.0, 200.0));
        assert_eq!(mv[1].pos, (200.0, 280.0));
        // The modifier is the press's: releasing Alt mid-drag keeps both fingers.
        let up = e.feed(m(MouseTouchKind::Up, 440.0, 200.0, false, false, false), C, 30.0);
        assert_eq!(up.len(), 2);
        assert!(up.iter().all(|t| t.phase == TouchPhase::Ended));
    }

    #[test]
    fn alt_shift_rides_a_parallel_finger_and_alt_ctrl_adds_a_third() {
        let mut e = Emulator::default();
        let down = e.feed(m(MouseTouchKind::Down, 100.0, 100.0, true, true, false), C, 30.0);
        assert_eq!(
            down.iter().map(|t| t.pos).collect::<Vec<_>>(),
            [(100.0, 100.0), (130.0, 100.0)]
        );
        let mv = e.feed(m(MouseTouchKind::Move, 100.0, 150.0, true, true, false), C, 30.0);
        assert_eq!(mv[1].pos, (130.0, 150.0));
        e.feed(m(MouseTouchKind::Up, 100.0, 150.0, true, true, false), C, 30.0);
        let down = e.feed(m(MouseTouchKind::Down, 100.0, 100.0, true, false, true), C, 30.0);
        assert_eq!(down.len(), 3);
        assert_eq!(down[2].pos, C);
    }

    #[test]
    fn cancel_ends_every_emulated_finger() {
        let mut e = Emulator::default();
        e.feed(m(MouseTouchKind::Down, 400.0, 200.0, true, false, false), C, 30.0);
        let c = e.cancel();
        assert_eq!(c.len(), 2);
        assert!(c.iter().all(|t| t.phase == TouchPhase::Cancelled));
        assert!(e.cancel().is_empty());
    }
}
