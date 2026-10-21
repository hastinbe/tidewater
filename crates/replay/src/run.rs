//! Running a replay without a client: [`verify`] plays it through and checks every checkpoint, [`first_desync`] plays
//! two worlds side by side and finds where they part.

use tidewater_sim::{Difference, World};

use crate::format::Replay;

/// A checkpoint whose hash is not the recorded one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The cycle the hash was taken at (the world's tick counter then).
    pub cycle: u32,
    pub recorded: u64,
    pub got: u64,
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the replay went out of sync at cycle {}: the recording has state hash {:016x}, this run has {:016x}",
            self.cycle, self.recorded, self.got
        )
    }
}

impl std::error::Error for Mismatch {}

/// What a verified run did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub cycles: u32,
    /// How many checkpoints were compared.
    pub checked: usize,
    /// The state hash after the last cycle.
    pub final_hash: u64,
}

/// Plays `replay` on `world`, a world as [`crate::start::new_world`] builds it, and compares each checkpoint as the
/// world reaches it. Stops at the first mismatch.
pub fn verify(world: &mut World, replay: &Replay) -> Result<Report, Mismatch> {
    let mut checked = 0;
    for _ in 0..replay.cycles {
        world.step(replay.commands_at(world.tick));
        if let Some(recorded) = replay.checkpoint_at(world.tick) {
            let got = world.state_hash();
            if got != recorded {
                return Err(Mismatch {
                    cycle: world.tick,
                    recorded,
                    got,
                });
            }
            checked += 1;
        }
    }
    Ok(Report {
        cycles: replay.cycles,
        checked,
        final_hash: world.state_hash(),
    })
}

/// Where two runs first differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Desync {
    /// The cycle (the world's tick counter after the step) at which the state hashes first differ.
    pub cycle: u32,
    pub left: u64,
    pub right: u64,
    /// The first piece and field that differ.
    pub difference: Option<Difference>,
}

/// Plays `a` on `left` and `b` on `right` cycle by cycle and returns where their state hashes first differ, or `None`
/// if they agree for as many cycles as both replays have (the shorter one decides). The start states are compared
/// too, so a difference before the first cycle is reported at the cycle the worlds start at.
pub fn first_desync(left: &mut World, a: &Replay, right: &mut World, b: &Replay) -> Option<Desync> {
    let found = |l: &World, r: &World| {
        (l.state_hash() != r.state_hash()).then(|| Desync {
            cycle: l.tick,
            left: l.state_hash(),
            right: r.state_hash(),
            difference: l.first_difference(r),
        })
    };
    if let Some(d) = found(left, right) {
        return Some(d);
    }
    for _ in 0..a.cycles.min(b.cycles) {
        left.step(a.commands_at(left.tick));
        right.step(b.commands_at(right.tick));
        if let Some(d) = found(left, right) {
            return Some(d);
        }
    }
    None
}
