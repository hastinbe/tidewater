//! Game speeds and the cycle scheduler. Spec: `docs/specs/core/timing-and-rng.md`, "Simulation cycles", "Scheduler"
//! and the speed tables.

/// The 9 speed levels, slowest first, as the game names them.
pub const SPEED_NAMES: [&str; 9] = [
    "Slowest",
    "Even Slower",
    "Slower",
    "Slow",
    "Normal",
    "Fast",
    "Faster",
    "Even Faster",
    "Fastest",
];

/// Enhanced pacing: cycle length in ms per speed level.
pub const ENHANCED_MS: [u32; 9] = [80, 60, 53, 46, 40, 33, 26, 20, 13];

/// The cycle length in ms of the speed levels above Fastest (option `speed-levels`, `docs/design/extras.md`), 9 to 11.
pub const EXTRA_MS: [u32; 3] = [10, 8, 6];

/// The highest speed level any game reaches (Fastest plus the three extra levels).
pub const MAX_SPEED: u8 = 8 + EXTRA_MS.len() as u8;

/// Enhanced pacing: cycle length in ms of `speed`, 0 to 8 from the table and above that the extra levels.
pub fn enhanced_ms(speed: u8) -> u32 {
    let s = usize::from(speed);
    if s < ENHANCED_MS.len() {
        ENHANCED_MS[s]
    } else {
        EXTRA_MS[(s - ENHANCED_MS.len()).min(EXTRA_MS.len() - 1)]
    }
}

/// The name of a speed level: the nine game names, then "Fastest +1" and so on.
pub fn speed_name(speed: u8) -> String {
    match SPEED_NAMES.get(usize::from(speed)) {
        Some(n) => (*n).to_string(),
        None => format!("Fastest +{}", speed - 8),
    }
}

/// Classic pacing: repeating three-cycle patterns, indexed by row.
pub const CLASSIC_MS: [[u32; 3]; 7] = [
    [60, 60, 60],
    [50, 50, 60],
    [40, 50, 50],
    [40, 40, 40],
    [30, 30, 40],
    [20, 30, 30],
    [20, 20, 20],
];

/// Which Classic row each 9-level setting uses.
pub const CLASSIC_ROW: [usize; 9] = [0, 0, 1, 2, 3, 4, 5, 6, 6];

pub const NORMAL: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    Enhanced,
    Classic,
}

/// Most cycles run back to back in one frame.
pub const MAX_PER_FRAME: u32 = 3;
/// A deadline further than this from now (either way) is reset.
pub const RESET_MS: i64 = 500;

/// Decides how many cycles to run each frame from a millisecond clock.
#[derive(Clone, Debug)]
pub struct Scheduler {
    pub pacing: Pacing,
    speed: u8,
    deadline: i64,
    /// Cycles run so far, for the Classic pattern phase.
    phase: usize,
}

impl Scheduler {
    pub fn new(pacing: Pacing, speed: u8, now_ms: i64) -> Self {
        Self {
            pacing,
            speed: speed.min(MAX_SPEED),
            deadline: now_ms,
            phase: 0,
        }
    }

    pub fn speed(&self) -> u8 {
        self.speed
    }

    /// Changing speed resets the deadline to now, dropping any backlog.
    pub fn set_speed(&mut self, speed: u8, now_ms: i64) {
        self.speed = speed.min(MAX_SPEED);
        self.deadline = now_ms;
    }

    /// Length of the next cycle in ms.
    pub fn cycle_ms(&self) -> u32 {
        match self.pacing {
            Pacing::Enhanced => enhanced_ms(self.speed),
            Pacing::Classic => CLASSIC_MS[CLASSIC_ROW[self.speed.min(8) as usize]][self.phase % 3],
        }
    }

    /// How many cycles to run in the frame arriving at `now_ms`.
    pub fn frame(&mut self, now_ms: i64) -> u32 {
        // Resolved open question (our choice): a reset replaces that frame's due cycles; the frame then runs from now.
        if (self.deadline - now_ms).abs() > RESET_MS {
            self.deadline = now_ms;
        }
        let mut n = 0;
        while n < MAX_PER_FRAME && self.deadline <= now_ms {
            self.deadline += self.cycle_ms() as i64;
            self.phase += 1;
            n += 1;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extra_levels_are_faster_than_fastest() {
        assert_eq!(
            (enhanced_ms(8), enhanced_ms(9), enhanced_ms(10), enhanced_ms(11)),
            (13, 10, 8, 6)
        );
        assert_eq!(enhanced_ms(200), 6);
        assert_eq!(speed_name(8), "Fastest");
        assert_eq!(speed_name(10), "Fastest +2");
        let mut s = Scheduler::new(Pacing::Enhanced, MAX_SPEED, 0);
        let total: u32 = (0..1000).map(|t| s.frame(t)).sum();
        assert!((165..=168).contains(&total), "{total}");
        // Classic never leaves its table.
        assert_eq!(Scheduler::new(Pacing::Classic, 11, 0).cycle_ms(), 20);
    }

    #[test]
    fn normal_runs_25_cycles_a_second() {
        let mut s = Scheduler::new(Pacing::Enhanced, NORMAL, 0);
        let total: u32 = (0..1000).step_by(5).map(|t| s.frame(t)).sum();
        assert_eq!(total, 25);
    }

    #[test]
    fn classic_row_1_pattern() {
        let mut s = Scheduler::new(Pacing::Classic, 2, 0);
        let ms: u32 = (0..9)
            .map(|_| {
                let m = s.cycle_ms();
                s.phase += 1;
                m
            })
            .sum();
        assert_eq!(ms, 480);
    }

    #[test]
    fn late_frame_runs_three_and_keeps_backlog() {
        let mut s = Scheduler::new(Pacing::Enhanced, NORMAL, 0);
        assert_eq!(s.frame(130), 3);
        assert_eq!(s.deadline, 120);
    }

    #[test]
    fn very_late_frame_resets() {
        let mut s = Scheduler::new(Pacing::Enhanced, NORMAL, 0);
        assert!(s.frame(600) <= 3);
        assert_eq!(s.deadline, 600 + 40);
    }
}
