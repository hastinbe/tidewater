//! Touch gestures turned into the mouse model (`docs/design/touch.md`). The game's rules only know a pointer, buttons,
//! keys and a wheel; this state machine reads the fingers' events (`input::Touch`) and says what the mouse and keyboard
//! would have done, so every rule stays the spec's own. Nothing here depends on an engine or a clock: the caller says
//! how much time passed and how many window points a millimetre is.
//!
//! | Fingers | Gesture | Becomes |
//! |---|---|---|
//! | 1 | Tap (lift before [`LONG_PRESS_MS`], moved less than [`SLOP_MM`]) | Left press, then release a frame later (Right in right-click mode) |
//! | 1 | Drag (moves past the slop) | Left button held from where the finger went down: box select, minimap drag |
//! | 1 | Long press ([`LONG_PRESS_MS`] still) | Right click where the finger is, with a pulse for feedback |
//! | 2 | Drag of both together | [`Frame::pan`] |
//! | 2 | Spread or pinch | [`Frame::zoom`] |
//! | 3 | Tap (all up within [`MENU_TAP_MS`], none moved) | F10, the game menu |
//!
//! **Frames.** The mouse model needs a press and its release in different frames (a press and release in one frame
//! drop the release), and a pointer move after a press in a later one. The machine therefore emits [`Frame`]s, and
//! the caller applies one per game frame ([`Gesture::pop`]); events that would break these rules start a new frame.
//!
//! **Fingers joining and leaving.** A second finger while one is undecided or held cancels it silently (no click); one
//! that comes while a drag holds the left button releases it where the finger is, then goes on as two fingers. When one
//! of several fingers lifts, the rest are ignored until all are up, so a pinch never turns into a stray click or drag.
//! A cancelled finger releases what it held and nothing more.

use std::collections::VecDeque;
use std::time::Duration;

use crate::input::{Button, Key, Touch, TouchPhase};

/// How long a finger must stay down, nearly still, to be a long press.
pub const LONG_PRESS_MS: f64 = 450.0;
/// How far a single finger may move and still be a tap or a long press, in millimetres.
pub const SLOP_MM: f32 = 3.0;
/// How far two fingers' centre must move before it pans, in millimetres.
pub const PAN_MM: f32 = 3.0;
/// How much the fingers' distance must change before it zooms, in millimetres.
pub const PINCH_MM: f32 = 3.0;
/// The longest three fingers may stay down for a menu tap.
pub const MENU_TAP_MS: f64 = 450.0;

/// A point in the window's logical points.
pub type Pt = (f32, f32);

/// What one game frame gets from the gesture machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    /// The pointer goes here.
    pub pointer: Option<Pt>,
    /// Button changes, in order.
    pub buttons: Vec<(Button, bool)>,
    /// Key changes, in order.
    pub keys: Vec<(Key, bool)>,
    /// How far the two fingers' centre moved, in points.
    pub pan: Pt,
    /// By what factor the fingers' distance changed, and the centre it changed around.
    pub zoom: Option<(f32, Pt)>,
    /// This frame's left release ends a tap (the tap tolerance applies to it).
    pub tap: bool,
    /// A long press fired at this point: draw the pulse.
    pub pulse: Option<Pt>,
}

impl Frame {
    fn is_empty(&self) -> bool {
        *self == Frame::default()
    }
}

#[derive(Clone, Copy, Debug)]
struct Finger {
    id: u64,
    pos: Pt,
}

#[derive(Clone, Copy, Debug)]
struct Two {
    ids: [u64; 2],
    centre0: Pt,
    dist0: f32,
    last_centre: Pt,
    last_dist: f32,
    pan: bool,
    zoom: bool,
}

#[derive(Clone, Copy, Debug)]
enum Mode {
    Idle,
    /// One finger down, undecided: where, and when the first finger of the touch went down.
    Pending {
        id: u64,
        start: Pt,
        t0: f64,
    },
    /// One finger dragging with the left button held.
    Drag {
        id: u64,
    },
    /// A long press fired; the finger is still down.
    Held,
    /// Two fingers.
    Two {
        t0: f64,
        two: Two,
    },
    /// Three fingers or more: a menu tap if they all lift soon, still.
    Menu {
        t0: f64,
        moved: bool,
    },
    /// Waiting for every finger to lift.
    Ignore,
}

/// The gesture machine.
#[derive(Debug)]
pub struct Gesture {
    mode: Mode,
    fingers: Vec<Finger>,
    /// Where the fingers of a menu tap went down, to see whether they moved.
    starts: Vec<Finger>,
    now: f64,
    cur: Frame,
    queue: VecDeque<Frame>,
    /// Millimetres to points, for the update in progress.
    mm: f32,
    /// A tap is a right click (the chrome's right-click mode).
    right_mode: bool,
}

impl Default for Gesture {
    fn default() -> Self {
        Gesture {
            mode: Mode::Idle,
            fingers: Vec::new(),
            starts: Vec::new(),
            now: 0.0,
            cur: Frame::default(),
            queue: VecDeque::new(),
            mm: 6.0,
            right_mode: false,
        }
    }
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn mid(a: Pt, b: Pt) -> Pt {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

impl Gesture {
    /// Time passes by `dt`, then the fingers' `events` (in order) are read. `mm` is how many logical points a
    /// millimetre is; `right_mode`: a tap is a right click.
    pub fn update(&mut self, dt: Duration, events: &[Touch], mm: f32, right_mode: bool) {
        self.now += dt.as_secs_f64() * 1000.0;
        self.mm = mm.max(0.1);
        self.right_mode = right_mode;
        for &t in events {
            self.feed(t);
        }
        self.eval_two();
        // The timers: a finger that stayed still long enough.
        if let Mode::Pending { id, t0, .. } = self.mode
            && self.now - t0 >= LONG_PRESS_MS
            && let Some(f) = self.finger(id)
        {
            self.pulse(f.pos);
            self.button(Button::Right, true, f.pos);
            self.next_frame();
            self.button(Button::Right, false, f.pos);
            self.mode = Mode::Held;
        }
        self.next_frame();
    }

    /// The next frame to apply, oldest first.
    pub fn pop(&mut self) -> Option<Frame> {
        self.queue.pop_front()
    }

    /// No finger is down and nothing is waiting to be applied.
    #[cfg(test)]
    fn idle(&self) -> bool {
        matches!(self.mode, Mode::Idle) && self.fingers.is_empty() && self.queue.is_empty() && self.cur.is_empty()
    }

    /// A finger drags with the left button held.
    pub fn dragging(&self) -> bool {
        matches!(self.mode, Mode::Drag { .. })
    }

    /// Whether fingers are down.
    pub fn touching(&self) -> bool {
        !self.fingers.is_empty()
    }

    /// Lets go of everything held (a screen change, losing focus): the left button is released where its finger is.
    pub fn reset(&mut self) {
        if let Mode::Drag { id } = self.mode
            && let Some(f) = self.finger(id)
        {
            self.button(Button::Left, false, f.pos);
        }
        self.mode = if self.fingers.is_empty() {
            Mode::Idle
        } else {
            Mode::Ignore
        };
        self.next_frame();
    }

    fn finger(&self, id: u64) -> Option<Finger> {
        self.fingers.iter().copied().find(|f| f.id == id)
    }

    fn slop(&self) -> f32 {
        SLOP_MM * self.mm
    }

    // --- building frames -----------------------------------------------------------------------------------------

    /// Ends the frame being built.
    fn next_frame(&mut self) {
        let f = std::mem::take(&mut self.cur);
        if !f.is_empty() {
            self.queue.push_back(f);
        }
    }

    fn pointer(&mut self, p: Pt) {
        // A pointer that moves after a press belongs to a later frame.
        if !self.cur.buttons.is_empty() && self.cur.pointer != Some(p) {
            self.next_frame();
        }
        self.cur.pointer = Some(p);
    }

    fn button(&mut self, b: Button, down: bool, at: Pt) {
        // The same button twice, or a button after the pointer went elsewhere, is a new frame.
        if self.cur.buttons.iter().any(|&(x, _)| x == b) || self.cur.pointer.is_some_and(|q| q != at) {
            self.next_frame();
        }
        self.cur.pointer = Some(at);
        self.cur.buttons.push((b, down));
    }

    fn key(&mut self, k: Key, down: bool) {
        if self.cur.keys.iter().any(|&(x, _)| x == k) {
            self.next_frame();
        }
        self.cur.keys.push((k, down));
    }

    fn pulse(&mut self, at: Pt) {
        self.cur.pulse = Some(at);
    }

    // --- the fingers -----------------------------------------------------------------------------------------------

    fn feed(&mut self, t: Touch) {
        if t.phase != TouchPhase::Moved {
            self.eval_two();
        }
        match t.phase {
            TouchPhase::Began => self.began(t),
            TouchPhase::Moved => self.moved(t),
            TouchPhase::Ended => self.ended(t, false),
            TouchPhase::Cancelled => self.ended(t, true),
        }
    }

    fn began(&mut self, t: Touch) {
        if self.fingers.iter().any(|f| f.id == t.id) {
            return;
        }
        self.fingers.push(Finger { id: t.id, pos: t.pos });
        match self.mode {
            Mode::Idle => {
                self.starts = vec![Finger { id: t.id, pos: t.pos }];
                self.mode = Mode::Pending {
                    id: t.id,
                    start: t.pos,
                    t0: self.now,
                };
            }
            Mode::Pending { id, t0, .. } => self.two(id, t.id, t0),
            Mode::Drag { id } => {
                if let Some(f) = self.finger(id) {
                    self.button(Button::Left, false, f.pos);
                }
                self.two(id, t.id, self.now);
            }
            Mode::Held => {
                let first = self.fingers.first().map_or(t.id, |f| f.id);
                self.two(first, t.id, self.now);
            }
            Mode::Two { t0, .. } => {
                self.starts.push(Finger { id: t.id, pos: t.pos });
                self.mode = Mode::Menu { t0, moved: false };
            }
            Mode::Menu { .. } => self.starts.push(Finger { id: t.id, pos: t.pos }),
            Mode::Ignore => {}
        }
    }

    /// Two fingers, the older `a` and the new `b`.
    fn two(&mut self, a: u64, b: u64, t0: f64) {
        let (Some(fa), Some(fb)) = (self.finger(a), self.finger(b)) else {
            self.mode = Mode::Ignore;
            return;
        };
        let (centre, d) = (mid(fa.pos, fb.pos), dist(fa.pos, fb.pos));
        if !self.starts.iter().any(|s| s.id == a) {
            self.starts.push(fa);
        }
        self.starts.push(fb);
        self.mode = Mode::Two {
            t0,
            two: Two {
                ids: [a, b],
                centre0: centre,
                dist0: d,
                last_centre: centre,
                last_dist: d,
                pan: false,
                zoom: false,
            },
        };
    }

    fn moved(&mut self, t: Touch) {
        let Some(f) = self.fingers.iter_mut().find(|f| f.id == t.id) else {
            return;
        };
        f.pos = t.pos;
        match self.mode {
            Mode::Pending { id, start, .. } if id == t.id => {
                if dist(start, t.pos) > self.slop() {
                    // A drag: the left button goes down where the finger did, and follows it from the next frame.
                    self.button(Button::Left, true, start);
                    self.mode = Mode::Drag { id };
                    self.pointer(t.pos);
                }
            }
            Mode::Drag { id } if id == t.id => self.pointer(t.pos),
            Mode::Menu { t0, .. } => {
                let slop = self.slop();
                let moved = self
                    .starts
                    .iter()
                    .any(|s| self.finger(s.id).is_some_and(|f| dist(f.pos, s.pos) > slop));
                if moved {
                    self.mode = Mode::Menu { t0, moved: true };
                }
            }
            _ => {}
        }
    }

    /// Reads two fingers' places once for all the moves of a frame: SDL reports one finger at a time, and the first
    /// finger's move alone would look like a pinch.
    fn eval_two(&mut self) {
        let Mode::Two { t0, mut two } = self.mode else {
            return;
        };
        if let (Some(a), Some(b)) = (self.finger(two.ids[0]), self.finger(two.ids[1])) {
            let (centre, d) = (mid(a.pos, b.pos), dist(a.pos, b.pos));
            if !two.pan && dist(centre, two.centre0) > PAN_MM * self.mm {
                two.pan = true;
            }
            if !two.zoom && (d - two.dist0).abs() > PINCH_MM * self.mm {
                two.zoom = true;
            }
            if two.pan {
                self.cur.pan.0 += centre.0 - two.last_centre.0;
                self.cur.pan.1 += centre.1 - two.last_centre.1;
                two.last_centre = centre;
            }
            if two.zoom && two.last_dist > 1.0 && d > 1.0 {
                let (old, _) = self.cur.zoom.unwrap_or((1.0, centre));
                self.cur.zoom = Some((old * d / two.last_dist, centre));
                two.last_dist = d;
            }
        }
        self.mode = Mode::Two { t0, two };
    }

    fn ended(&mut self, t: Touch, cancelled: bool) {
        let Some(at) = self.fingers.iter().position(|f| f.id == t.id) else {
            return;
        };
        let last = self.fingers[at];
        self.fingers.remove(at);
        let pos = if cancelled { last.pos } else { t.pos };
        match self.mode {
            Mode::Pending { id, start, .. } if id == t.id => {
                if !cancelled {
                    self.tap(start);
                }
                self.mode = Mode::Idle;
            }
            Mode::Drag { id } if id == t.id => {
                if !cancelled {
                    self.pointer(pos);
                }
                self.button(Button::Left, false, if cancelled { last.pos } else { pos });
                self.mode = Mode::Idle;
            }
            Mode::Held => self.mode = Mode::Idle,
            Mode::Menu { t0, moved } => {
                if self.fingers.is_empty() {
                    if !cancelled && !moved && self.now - t0 <= MENU_TAP_MS {
                        self.key(Key::F(10), true);
                        self.next_frame();
                        self.key(Key::F(10), false);
                    }
                    self.mode = Mode::Idle;
                } else if cancelled {
                    self.mode = Mode::Ignore;
                }
            }
            Mode::Two { .. } => self.mode = Mode::Ignore,
            Mode::Idle | Mode::Pending { .. } | Mode::Drag { .. } | Mode::Ignore => {}
        }
        if self.fingers.is_empty() {
            self.starts.clear();
            if matches!(self.mode, Mode::Ignore) {
                self.mode = Mode::Idle;
            }
        }
    }

    /// A tap at `at`: the button goes down, and up in the next frame.
    fn tap(&mut self, at: Pt) {
        let b = if self.right_mode { Button::Right } else { Button::Left };
        self.button(b, true, at);
        self.next_frame();
        self.button(b, false, at);
        self.cur.tap = b == Button::Left;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 5 points to a millimetre: the slop is 15 points, a pan or pinch starts at 15.
    const MM: f32 = 5.0;
    const FRAME: Duration = Duration::from_millis(16);

    fn t(id: u64, phase: TouchPhase, x: f32, y: f32) -> Touch {
        Touch { id, phase, pos: (x, y) }
    }
    fn down(id: u64, x: f32, y: f32) -> Touch {
        t(id, TouchPhase::Began, x, y)
    }
    fn mv(id: u64, x: f32, y: f32) -> Touch {
        t(id, TouchPhase::Moved, x, y)
    }
    fn up(id: u64, x: f32, y: f32) -> Touch {
        t(id, TouchPhase::Ended, x, y)
    }
    fn cancel(id: u64, x: f32, y: f32) -> Touch {
        t(id, TouchPhase::Cancelled, x, y)
    }

    /// Runs frames of `FRAME` with `events` in the first and returns every frame the machine produced.
    fn run(g: &mut Gesture, events: &[Touch]) -> Vec<Frame> {
        step(g, events, 1)
    }

    fn step(g: &mut Gesture, events: &[Touch], frames: u32) -> Vec<Frame> {
        let mut out = Vec::new();
        for i in 0..frames {
            g.update(FRAME, if i == 0 { events } else { &[] }, MM, false);
        }
        while let Some(f) = g.pop() {
            out.push(f);
        }
        out
    }

    fn waits(g: &mut Gesture, ms: u64) -> Vec<Frame> {
        let mut out = Vec::new();
        let mut left = ms as i64;
        while left > 0 {
            g.update(FRAME, &[], MM, false);
            left -= FRAME.as_millis() as i64;
            while let Some(f) = g.pop() {
                out.push(f);
            }
        }
        out
    }

    fn buttons(frames: &[Frame]) -> Vec<(usize, Button, bool)> {
        frames
            .iter()
            .enumerate()
            .flat_map(|(i, f)| f.buttons.iter().map(move |&(b, d)| (i, b, d)))
            .collect()
    }

    #[test]
    fn a_tap_is_a_left_press_and_a_release_in_the_next_frame() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 80.0)]);
        assert!(frames.is_empty(), "nothing until the finger decides");
        frames.extend(waits(&mut g, 100));
        frames.extend(run(&mut g, &[up(1, 102.0, 81.0)]));
        // The press is where the finger went down.
        assert_eq!(
            buttons(&frames),
            [(0, Button::Left, true), (1, Button::Left, false)],
            "{frames:?}"
        );
        assert_eq!(frames[0].pointer, Some((100.0, 80.0)));
        assert_eq!(frames[1].pointer, Some((100.0, 80.0)));
        assert!(!frames[0].tap && frames[1].tap, "the release carries the tap");
        assert!(g.idle());
    }

    #[test]
    fn a_tap_that_starts_and_ends_in_one_frame_still_takes_two() {
        let mut g = Gesture::default();
        let frames = run(&mut g, &[down(1, 10.0, 10.0), up(1, 10.0, 10.0)]);
        assert_eq!(buttons(&frames), [(0, Button::Left, true), (1, Button::Left, false)]);
    }

    #[test]
    fn a_tap_in_right_click_mode_is_a_right_click() {
        let mut g = Gesture::default();
        g.update(FRAME, &[down(1, 50.0, 60.0), up(1, 50.0, 60.0)], MM, true);
        let frames: Vec<_> = std::iter::from_fn(|| g.pop()).collect();
        assert_eq!(buttons(&frames), [(0, Button::Right, true), (1, Button::Right, false)]);
        assert!(frames.iter().all(|f| !f.tap), "the tap tolerance is for selecting");
    }

    #[test]
    fn a_finger_that_wanders_a_little_is_still_a_tap() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0)]);
        // 10 points is 2 mm: within the slop of 3.
        frames.extend(run(&mut g, &[mv(1, 106.0, 108.0)]));
        frames.extend(run(&mut g, &[up(1, 106.0, 108.0)]));
        let b = buttons(&frames);
        assert_eq!(b.len(), 2, "{frames:?}");
        assert_eq!(
            frames[0].pointer,
            Some((100.0, 100.0)),
            "it clicks where the finger landed"
        );
    }

    #[test]
    fn a_drag_holds_the_left_button_from_where_the_finger_landed() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0)]);
        frames.extend(run(&mut g, &[mv(1, 130.0, 100.0)]));
        frames.extend(run(&mut g, &[mv(1, 200.0, 150.0)]));
        frames.extend(run(&mut g, &[up(1, 220.0, 160.0)]));
        let b = buttons(&frames);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].1, Button::Left);
        assert!(b[0].2 && !b[1].2);
        // The press is at the start, the pointer then follows.
        assert_eq!(frames[0].pointer, Some((100.0, 100.0)));
        assert_eq!(frames[0].buttons, [(Button::Left, true)]);
        assert_eq!(
            frames[1].pointer,
            Some((130.0, 100.0)),
            "later frame: the move after the press"
        );
        let last = frames.last().unwrap();
        assert_eq!(last.pointer, Some((220.0, 160.0)));
        assert_eq!(last.buttons, [(Button::Left, false)]);
        assert!(!last.tap, "a drag is not a tap");
        assert!(g.idle());
    }

    #[test]
    fn a_still_finger_long_presses_into_a_right_click_with_a_pulse() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 200.0, 120.0)]);
        frames.extend(waits(&mut g, 400));
        assert!(frames.is_empty(), "not yet at 400 ms: {frames:?}");
        frames.extend(waits(&mut g, 100));
        assert_eq!(
            buttons(&frames),
            [(0, Button::Right, true), (1, Button::Right, false)],
            "{frames:?}"
        );
        assert_eq!(frames[0].pulse, Some((200.0, 120.0)));
        assert_eq!(frames[0].pointer, Some((200.0, 120.0)));
        // Lifting afterwards does nothing more, and neither does a late wobble.
        let rest = run(&mut g, &[up(1, 201.0, 120.0)]);
        assert!(rest.is_empty());
        assert!(g.idle());
    }

    #[test]
    fn a_long_press_allows_small_movement_but_not_a_drag() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 200.0, 120.0)]);
        frames.extend(run(&mut g, &[mv(1, 208.0, 126.0)]));
        frames.extend(waits(&mut g, 500));
        assert_eq!(buttons(&frames).len(), 2, "still a long press: {frames:?}");
        assert_eq!(buttons(&frames)[0].1, Button::Right);
        // Past the slop it is a drag instead, however long the wait.
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 200.0, 120.0)]);
        frames.extend(waits(&mut g, 300));
        frames.extend(run(&mut g, &[mv(1, 260.0, 120.0)]));
        frames.extend(waits(&mut g, 600));
        assert_eq!(buttons(&frames), [(0, Button::Left, true)], "{frames:?}");
        assert!(frames.iter().all(|f| f.pulse.is_none()));
    }

    #[test]
    fn a_tap_just_before_the_long_press_time_is_still_a_tap() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 20.0, 20.0)]);
        frames.extend(waits(&mut g, 420));
        frames.extend(run(&mut g, &[up(1, 20.0, 20.0)]));
        let b = buttons(&frames);
        assert_eq!(b.iter().map(|x| x.1).collect::<Vec<_>>(), [Button::Left, Button::Left]);
    }

    #[test]
    fn two_fingers_moving_together_pan() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0), down(2, 200.0, 100.0)]);
        // 6 points: under the pan threshold of 15; nothing yet.
        frames.extend(run(&mut g, &[mv(1, 106.0, 100.0), mv(2, 206.0, 100.0)]));
        assert!(frames.is_empty());
        // Past it: the pan includes the travel so far (centre 150 -> 140 is -10 on top of the 6... measured from the start).
        frames.extend(run(&mut g, &[mv(1, 80.0, 110.0), mv(2, 180.0, 110.0)]));
        assert_eq!(frames.len(), 1);
        let p = frames[0].pan;
        assert!((p.0 - -20.0).abs() < 1e-3 && (p.1 - 10.0).abs() < 1e-3, "{p:?}");
        assert!(frames[0].zoom.is_none(), "the distance did not change");
        assert!(frames[0].buttons.is_empty(), "no click came out of a pan");
        // And on: each move adds its own travel.
        let more = run(&mut g, &[mv(1, 70.0, 110.0), mv(2, 170.0, 110.0)]);
        assert!((more[0].pan.0 - -10.0).abs() < 1e-3);
    }

    #[test]
    fn two_fingers_spreading_zoom_around_their_centre() {
        let mut g = Gesture::default();
        run(&mut g, &[down(1, 100.0, 100.0), down(2, 200.0, 100.0)]);
        // Spread from 100 apart to 160: factor 1.6 around the centre (150, 100); the pan stays quiet.
        let frames = run(&mut g, &[mv(1, 70.0, 100.0), mv(2, 230.0, 100.0)]);
        assert_eq!(frames.len(), 1);
        let (f, c) = frames[0].zoom.expect("a zoom");
        assert!((f - 1.6).abs() < 1e-3, "{f}");
        assert_eq!(c, (150.0, 100.0));
        assert_eq!(frames[0].pan, (0.0, 0.0));
        // Pinching in again is a factor under one.
        let frames = run(&mut g, &[mv(1, 100.0, 100.0), mv(2, 200.0, 100.0)]);
        let (f, _) = frames[0].zoom.unwrap();
        assert!((f - 100.0 / 160.0).abs() < 1e-3, "{f}");
    }

    #[test]
    fn several_moves_in_one_frame_multiply_their_zoom() {
        let mut g = Gesture::default();
        run(&mut g, &[down(1, 100.0, 100.0), down(2, 200.0, 100.0)]);
        let frames = run(
            &mut g,
            &[
                mv(1, 80.0, 100.0),
                mv(2, 200.0, 100.0),
                mv(1, 60.0, 100.0),
                mv(2, 200.0, 100.0),
            ],
        );
        let (f, _) = frames[0].zoom.unwrap();
        assert!((f - 1.4).abs() < 1e-3, "{f}");
    }

    #[test]
    fn a_second_finger_cancels_an_undecided_first_without_a_click() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0)]);
        frames.extend(run(&mut g, &[down(2, 180.0, 100.0)]));
        frames.extend(run(&mut g, &[up(2, 180.0, 100.0), up(1, 100.0, 100.0)]));
        assert!(frames.is_empty(), "{frames:?}");
        assert!(g.idle());
    }

    #[test]
    fn a_second_finger_during_a_drag_lets_go_of_the_button() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0), mv(1, 150.0, 100.0)]);
        frames.extend(run(&mut g, &[down(2, 300.0, 300.0)]));
        let b = buttons(&frames);
        assert_eq!(b.len(), 2, "{frames:?}");
        assert!(b[0].2 && !b[1].2, "pressed, then released where the finger is");
        assert_eq!(frames.last().unwrap().pointer, Some((150.0, 100.0)));
    }

    #[test]
    fn after_a_pinch_the_remaining_finger_is_ignored() {
        let mut g = Gesture::default();
        run(&mut g, &[down(1, 100.0, 100.0), down(2, 200.0, 100.0)]);
        let mut frames = run(&mut g, &[up(2, 200.0, 100.0)]);
        // The first finger stays and drags far: no click, no box.
        frames.extend(run(&mut g, &[mv(1, 300.0, 300.0)]));
        frames.extend(waits(&mut g, 600));
        frames.extend(run(&mut g, &[up(1, 300.0, 300.0)]));
        assert!(frames.is_empty(), "{frames:?}");
        // A new touch afterwards works.
        let frames = run(&mut g, &[down(1, 10.0, 10.0), up(1, 10.0, 10.0)]);
        assert_eq!(buttons(&frames).len(), 2);
    }

    #[test]
    fn three_fingers_tapping_open_the_menu() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0), down(2, 150.0, 100.0)]);
        frames.extend(run(&mut g, &[down(3, 200.0, 100.0)]));
        frames.extend(waits(&mut g, 100));
        assert!(frames.is_empty(), "nothing before they lift");
        frames.extend(run(
            &mut g,
            &[up(1, 100.0, 100.0), up(2, 150.0, 100.0), up(3, 200.0, 100.0)],
        ));
        let keys: Vec<(usize, Key, bool)> = frames
            .iter()
            .enumerate()
            .flat_map(|(i, f)| f.keys.iter().map(move |&(k, d)| (i, k, d)))
            .collect();
        assert_eq!(keys, [(0, Key::F(10), true), (1, Key::F(10), false)], "{frames:?}");
        assert!(buttons(&frames).is_empty());
        assert!(g.idle());
    }

    #[test]
    fn three_fingers_held_too_long_or_moved_do_not_open_the_menu() {
        let mut g = Gesture::default();
        run(
            &mut g,
            &[down(1, 100.0, 100.0), down(2, 150.0, 100.0), down(3, 200.0, 100.0)],
        );
        let mut frames = waits(&mut g, 600);
        frames.extend(run(
            &mut g,
            &[up(1, 100.0, 100.0), up(2, 150.0, 100.0), up(3, 200.0, 100.0)],
        ));
        assert!(frames.is_empty(), "held too long: {frames:?}");
        let mut g = Gesture::default();
        run(
            &mut g,
            &[down(1, 100.0, 100.0), down(2, 150.0, 100.0), down(3, 200.0, 100.0)],
        );
        let mut frames = run(&mut g, &[mv(3, 300.0, 100.0)]);
        frames.extend(run(
            &mut g,
            &[up(1, 100.0, 100.0), up(2, 150.0, 100.0), up(3, 300.0, 100.0)],
        ));
        assert!(frames.is_empty(), "moved: {frames:?}");
        assert!(g.idle());
    }

    #[test]
    fn three_fingers_may_land_a_few_frames_apart() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0)]);
        frames.extend(step(&mut g, &[down(2, 150.0, 100.0)], 2));
        frames.extend(step(&mut g, &[down(3, 200.0, 100.0)], 2));
        frames.extend(step(&mut g, &[up(2, 150.0, 100.0)], 1));
        frames.extend(step(&mut g, &[up(1, 100.0, 100.0), up(3, 200.0, 100.0)], 1));
        assert_eq!(
            frames.iter().filter(|f| f.keys.contains(&(Key::F(10), true))).count(),
            1
        );
    }

    #[test]
    fn a_cancelled_drag_releases_the_button_and_nothing_else() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0), mv(1, 150.0, 100.0)]);
        frames.extend(run(&mut g, &[cancel(1, 150.0, 100.0)]));
        let b = buttons(&frames);
        assert_eq!(b.len(), 2);
        assert!(!b[1].2, "the button is up again");
        assert!(g.idle());
    }

    #[test]
    fn cancelled_taps_and_menu_taps_do_nothing() {
        let mut g = Gesture::default();
        let frames = run(&mut g, &[down(1, 100.0, 100.0), cancel(1, 100.0, 100.0)]);
        assert!(frames.is_empty());
        assert!(g.idle());
        // A cancel in a three-finger tap spoils it.
        let mut frames = run(&mut g, &[down(1, 1.0, 1.0), down(2, 50.0, 1.0), down(3, 100.0, 1.0)]);
        frames.extend(run(&mut g, &[cancel(2, 50.0, 1.0)]));
        frames.extend(run(&mut g, &[up(1, 1.0, 1.0), up(3, 100.0, 1.0)]));
        assert!(frames.is_empty(), "{frames:?}");
        assert!(g.idle());
    }

    #[test]
    fn a_finger_held_after_a_long_press_does_not_drag() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 100.0, 100.0)]);
        frames.extend(waits(&mut g, 500));
        frames.extend(run(&mut g, &[mv(1, 300.0, 300.0)]));
        frames.extend(run(&mut g, &[up(1, 300.0, 300.0)]));
        assert_eq!(buttons(&frames).len(), 2, "only the right click: {frames:?}");
    }

    #[test]
    fn reset_lets_go_of_a_drag() {
        let mut g = Gesture::default();
        run(&mut g, &[down(1, 100.0, 100.0), mv(1, 150.0, 100.0)]);
        g.reset();
        let frames: Vec<_> = std::iter::from_fn(|| g.pop()).collect();
        assert_eq!(buttons(&frames).last().map(|b| b.2), Some(false), "{frames:?}");
        // The finger is still down but ignored; its lift is silent.
        let frames = run(&mut g, &[up(1, 150.0, 100.0)]);
        assert!(frames.is_empty());
        assert!(g.idle());
    }

    #[test]
    fn two_taps_in_a_row_make_two_clicks() {
        let mut g = Gesture::default();
        let mut frames = run(&mut g, &[down(1, 10.0, 10.0), up(1, 10.0, 10.0)]);
        frames.extend(run(&mut g, &[down(1, 12.0, 11.0), up(1, 12.0, 11.0)]));
        let b = buttons(&frames);
        assert_eq!(b.len(), 4, "{frames:?}");
        assert!(b.iter().filter(|x| x.2).count() == 2);
    }
}
