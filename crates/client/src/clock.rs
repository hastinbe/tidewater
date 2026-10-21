//! The core's clock (`docs/design/client-core.md`, migration step 5): real time as the shell reports it, the game time
//! the screens run on, and the fixed step the game's cycles run on. Nothing here depends on an engine; it keeps the
//! rules the engine's clocks had, so every frame sees the same times as before.
//!
//! | Clock | Rule |
//! |---|---|
//! | Real | The sum of the frame times the shell reports. The first frame reports none. |
//! | Game | The frame's real time, capped at [`MAX_DELTA`]: a long frame (a debugger, a suspended laptop) runs at most 250 ms. |
//! | Steps | The frame's real time goes into an accumulator (the gap between now and the scheduler's deadline); each whole [`step`](FrameClock::step) in it is one game cycle (`timing-and-rng.md`, "Scheduler"). At most [`MAX_STEPS`] run in a frame (rule 2, G22), and a backlog over [`RESET_BACKLOG`] is dropped (rule 3). The step starts at [`FIRST_STEP`]; a game sets its own. |
//!
//! The input logs (`input::Log`) age at the start of a frame only if the frame before ran a step, once any step has
//! run: the same rule the engine's message buffers had, so a log read only on some frames sees the same events.

use std::time::Duration;

/// The longest game time one frame can have.
pub const MAX_DELTA: Duration = Duration::from_millis(250);

/// The most cycles one frame runs (`timing-and-rng.md`, "Scheduler" rule 2).
pub const MAX_STEPS: u32 = 3;

/// A backlog (the deadline in the past) longer than this is dropped: the deadline is reset to now (rule 3, "more than
/// 500 ms away").
pub const RESET_BACKLOG: Duration = Duration::from_millis(500);

/// The step before a game sets its own: 64 a second.
pub const FIRST_STEP: Duration = Duration::from_micros(15_625);

pub struct FrameClock {
    real: Duration,
    real_delta: Duration,
    game_delta: Duration,
    step: Duration,
    overstep: Duration,
    /// A step ran in this frame (until the next frame's steps start).
    stepped: bool,
    /// How many steps this frame has taken.
    taken: u32,
    /// A step has ever run.
    ever_stepped: bool,
}

impl Default for FrameClock {
    fn default() -> FrameClock {
        FrameClock {
            real: Duration::ZERO,
            real_delta: Duration::ZERO,
            game_delta: Duration::ZERO,
            step: FIRST_STEP,
            overstep: Duration::ZERO,
            stepped: false,
            taken: 0,
            ever_stepped: false,
        }
    }
}

impl FrameClock {
    /// Starts a frame `real_delta` after the last one.
    pub fn advance(&mut self, real_delta: Duration) {
        self.real += real_delta;
        self.real_delta = real_delta;
        self.game_delta = real_delta.min(MAX_DELTA);
    }

    /// Puts the frame's game time into the step accumulator; then [`expend`](Self::expend) takes the steps.
    pub fn accumulate(&mut self) {
        self.overstep += self.real_delta;
        // Rule 3: a deadline more than 500 ms from now is reset to now, dropping the backlog.
        if self.overstep > RESET_BACKLOG {
            self.overstep = Duration::ZERO;
        }
        self.stepped = false;
        self.taken = 0;
    }

    /// Takes one step from the accumulator, if a whole one is there.
    pub fn expend(&mut self) -> bool {
        if self.taken >= MAX_STEPS {
            return false;
        }
        match self.overstep.checked_sub(self.step) {
            Some(rest) => {
                self.taken += 1;
                self.overstep = rest;
                self.stepped = true;
                self.ever_stepped = true;
                true
            }
            None => false,
        }
    }

    /// Whether the input logs age at the start of this frame (see the module doc). Read before the frame's steps.
    pub fn logs_age(&self) -> bool {
        !self.ever_stepped || self.stepped
    }

    pub fn step(&self) -> Duration {
        self.step
    }

    /// Sets the step for the steps still to be taken (this frame's too).
    pub fn set_step(&mut self, step: Duration) {
        assert_ne!(step, Duration::ZERO, "a step of zero");
        self.step = step;
    }

    /// Starts the steps again at `hz` a second, with an empty accumulator: a new game.
    pub fn restart_steps(&mut self, hz: f64) {
        self.set_step(Duration::from_secs_f64(1.0 / hz));
        self.overstep = Duration::ZERO;
    }

    /// Real time since the first frame.
    pub fn elapsed(&self) -> Duration {
        self.real
    }

    pub fn elapsed_secs_f64(&self) -> f64 {
        self.real.as_secs_f64()
    }

    /// This frame's real time.
    pub fn delta(&self) -> Duration {
        self.real_delta
    }

    pub fn delta_secs_f64(&self) -> f64 {
        self.real_delta.as_secs_f64()
    }

    /// This frame's game time: its real time, capped at [`MAX_DELTA`].
    pub fn game_delta(&self) -> Duration {
        self.game_delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn steps(c: &mut FrameClock) -> u32 {
        c.accumulate();
        let mut n = 0;
        while c.expend() {
            n += 1;
        }
        n
    }

    #[test]
    fn real_time_adds_up_and_game_time_is_capped() {
        let mut c = FrameClock::default();
        c.advance(Duration::ZERO);
        c.advance(10 * MS);
        c.advance(400 * MS);
        assert_eq!(c.elapsed(), 410 * MS);
        assert_eq!(c.delta(), 400 * MS);
        assert_eq!(c.game_delta(), MAX_DELTA);
        assert_eq!(c.elapsed_secs_f64(), 0.41);
    }

    #[test]
    fn whole_steps_run_and_the_rest_carries_over() {
        let mut c = FrameClock::default();
        c.restart_steps(100.0);
        c.advance(25 * MS);
        assert_eq!(steps(&mut c), 2);
        c.advance(5 * MS);
        assert_eq!(steps(&mut c), 1, "5 ms left over plus 5 ms");
        c.advance(400 * MS);
        assert_eq!(steps(&mut c), 3, "at most 3 cycles run in a frame");
        c.advance(Duration::ZERO);
        assert_eq!(steps(&mut c), 3, "the rest of the backlog is still there");
    }

    /// `core/timing-and-rng.md`, G22: a frame 200 ms after the deadline at 40 ms a cycle runs 3 cycles, not 5; the
    /// deadline is still 80 ms in the past.
    #[test]
    fn g22_a_late_frame_runs_three_cycles_and_keeps_the_rest() {
        let mut c = FrameClock::default();
        c.restart_steps(25.0);
        c.advance(200 * MS);
        assert_eq!(steps(&mut c), 3);
        assert_eq!(c.overstep, 80 * MS);
        c.advance(Duration::ZERO);
        assert_eq!(steps(&mut c), 2, "the next frame runs more");
    }

    /// Rule 3: a deadline more than 500 ms away is reset to now, dropping the backlog.
    #[test]
    fn a_backlog_over_500_ms_is_dropped() {
        let mut c = FrameClock::default();
        c.restart_steps(25.0);
        c.advance(600 * MS);
        assert_eq!(steps(&mut c), 0, "reset to now: nothing is owed");
        c.advance(Duration::ZERO);
        assert_eq!(steps(&mut c), 0);
        c.advance(500 * MS);
        assert_eq!(steps(&mut c), 3, "500 ms exactly is kept");
        assert_eq!(c.overstep, 380 * MS);
    }

    #[test]
    fn a_new_step_applies_to_the_steps_still_to_run() {
        let mut c = FrameClock::default();
        c.restart_steps(100.0);
        c.advance(40 * MS);
        c.accumulate();
        assert!(c.expend());
        c.set_step(20 * MS);
        assert!(c.expend());
        assert!(!c.expend(), "10 ms left");
        c.restart_steps(100.0);
        c.advance(5 * MS);
        assert_eq!(steps(&mut c), 0, "a new game empties the accumulator");
    }

    #[test]
    fn logs_age_every_frame_until_a_step_runs_then_after_frames_with_steps() {
        let mut c = FrameClock::default();
        assert!(c.logs_age());
        c.advance(Duration::ZERO);
        assert_eq!(steps(&mut c), 0);
        assert!(c.logs_age(), "no step has run yet");
        c.advance(20 * MS);
        assert_eq!(steps(&mut c), 1);
        assert!(c.logs_age());
        c.restart_steps(20.0);
        c.advance(20 * MS);
        assert_eq!(steps(&mut c), 0);
        assert!(!c.logs_age(), "the last frame ran no step");
        c.advance(30 * MS);
        assert_eq!(steps(&mut c), 1);
        assert!(c.logs_age());
    }
}
