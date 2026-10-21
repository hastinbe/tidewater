//! Where a game's commands come from (`docs/design/multiplayer-determinism.md`, "The session layer").
//!
//! The simulation changes only when it is given [`Command`]s (`World::step`). A [`Session`] is the one place the
//! client gets them: the player's own input goes in with [`Session::submit`], and each cycle `game::tick` asks
//! [`Session::ready_commands`] for the commands that run at the start of that cycle. `None` means the cycle cannot run
//! yet and the clock waits.
//!
//! | Session | Commands come from | Input |
//! |---|---|---|
//! | [`LocalSession`] | The player, in the cycle after they are given (single player; the default) | All of it |
//! | [`ReplaySession`] | The recording, at the cycle each was recorded | Ignored, except pause and speed, which become the viewer's own controls |
//!
//! # How a network session plugs in (later phases; nothing of it is here yet)
//!
//! A `NetSession` follows `docs/specs/net/lockstep.md` and implements this same trait:
//!
//! - **Turns.** A turn is K cycles. [`Session::submit`] queues the local player's commands for the turn after the
//!   next; at a turn boundary the session sends them to the other players. [`Session::ready_commands`] returns `None`
//!   for the first cycle of a turn until every human's commands for it have arrived (the stall rule), and then returns
//!   that turn's commands for this cycle: every player's, applied in slot order so all machines run the same list.
//! - **Start.** The two-packet start (`lockstep.md`, "Starting the game") happens before the first
//!   `ready_commands` call: the session reports not ready until both packets are in, then hands out cycle 0.
//! - **Hash checks.** [`Session::wants_hash`] and [`Session::checkpoint`] are where peers compare `state_hash`; a
//!   mismatch is a desync, handled as a replay's is (stop with a message, keep the recording).
//! - **Recording.** A network game records the same way: the merged, ordered list a session hands out is what the
//!   [`Recorder`] stores, so a recording of a network game plays back through [`ReplaySession`] on one machine.
//!
//! The commands a session hands out must be identical, in content and order, on every machine for a given cycle.
//! That is why the viewer's controls of a replay never become commands, and why the client never writes to the world
//! between cycles (rule 4 of the determinism design); [`Session::note_unsynced`] is for the few places that still do.

use tidewater_replay::format::CHECKPOINT_EVERY;
use tidewater_replay::run::Mismatch;
use tidewater_replay::{Header, Replay};
use tidewater_sim::Command;

use crate::storage::Store;

/// Where a game's commands come from. See the module doc.
pub trait Session: Send + Sync {
    /// The local player's input: a command the interface made.
    fn submit(&mut self, cmd: Command);

    /// A command that takes effect now, between cycles, as a turn without a cycle (`World::turn`): the game menu's
    /// Surrender, chosen while the game is stopped, ends the game in the cycle it was chosen in rather than one
    /// later. Returns the command to apply now, or `None` if this session does not take it (a replay's viewer). A
    /// recording keeps it at cycle `tick`, ahead of that cycle's other commands, so a replay applies it at the same
    /// point of the same cycle.
    ///
    /// A network session (later) does not use this: `net/leave-and-drop.md` makes a surrender local to the machine.
    fn immediate(&mut self, _tick: u32, cmd: Command) -> Option<Command> {
        Some(cmd)
    }

    /// The commands that run at the start of cycle `tick` (the world's tick counter before the cycle), in the order
    /// they run, or `None` if the cycle cannot run yet.
    fn ready_commands(&mut self, tick: u32) -> Option<Vec<Command>>;

    /// Whether the state hash after the cycle that brought the counter to `tick` should be handed to
    /// [`checkpoint`](Self::checkpoint). The client also hands one over when the game ends.
    fn wants_hash(&self, tick: u32) -> bool {
        tick > 0 && tick.is_multiple_of(CHECKPOINT_EVERY)
    }

    /// The state hash after the counter reached `tick`. A recording keeps it; a replay compares it with the recorded
    /// one and fails on a difference.
    fn checkpoint(&mut self, _tick: u32, _hash: u64) -> Result<(), Mismatch> {
        Ok(())
    }

    /// The client changed the world outside the command stream (`Writes to the world outside the command stream`):
    /// a replay of this game may not reproduce it. A recording keeps the note.
    fn note_unsynced(&mut self, _what: &str) {}

    /// Texts for the notice line since the last call (a replay's pause, end and desync).
    fn take_notices(&mut self) -> Vec<String> {
        Vec::new()
    }

    /// How many times faster than the game's speed the cycles run (a replay viewer's own control; 1 otherwise).
    fn rate(&self) -> u32 {
        1
    }

    /// Whether this session plays a recording, so the player's input is not the game's.
    fn is_replay(&self) -> bool {
        false
    }

    /// The cheat flags the world must start with, when the session knows them (a replay's); `None` lets the client
    /// start the flags it keeps across games.
    fn start_cheats(&self) -> Option<u32> {
        None
    }

    /// The values of the client-only options that give an edge (**C+L**, `extras`) when this session sets them itself,
    /// instead of the player's own settings: `None` for a local game, which uses the player's. A replay hands over the
    /// defaults (off) until a recording carries its own; the lobby of a network game will hand over its choices (M3).
    fn lobby_client_extras(&self) -> Option<crate::extras::ClientExtras> {
        None
    }

    /// The options that change the simulation, when this session decides them (a recording's, a lobby's): `None` lets
    /// the player's own settings choose, which is only ever so in a local game. They are never read from the local
    /// settings in any other session.
    fn sim_extras(&self) -> Option<tidewater_sim::Extras> {
        None
    }
}

/// Single player: the commands the player gives run in the next cycle, in the order given. Optionally records the
/// game.
#[derive(Default)]
pub struct LocalSession {
    queue: Vec<Command>,
    recorder: Option<Recorder>,
}

impl LocalSession {
    pub fn new() -> LocalSession {
        LocalSession::default()
    }

    /// A single-player session that also records the game.
    pub fn recording(recorder: Recorder) -> LocalSession {
        LocalSession {
            queue: Vec::new(),
            recorder: Some(recorder),
        }
    }
}

impl Session for LocalSession {
    fn submit(&mut self, cmd: Command) {
        self.queue.push(cmd);
    }

    fn immediate(&mut self, tick: u32, cmd: Command) -> Option<Command> {
        if let Some(r) = &mut self.recorder {
            r.replay.record(tick, std::slice::from_ref(&cmd));
        }
        Some(cmd)
    }

    fn ready_commands(&mut self, tick: u32) -> Option<Vec<Command>> {
        let cmds = std::mem::take(&mut self.queue);
        if let Some(r) = &mut self.recorder {
            r.replay.record(tick, &cmds);
        }
        Some(cmds)
    }

    fn checkpoint(&mut self, tick: u32, hash: u64) -> Result<(), Mismatch> {
        if let Some(r) = &mut self.recorder {
            r.replay.checkpoint(tick, hash);
            r.dirty = true;
            // A game that is quit or crashed keeps what was played up to the last flush.
            if tick.is_multiple_of(FLUSH_EVERY) {
                r.flush();
            }
        }
        Ok(())
    }

    fn note_unsynced(&mut self, what: &str) {
        if let Some(r) = &mut self.recorder {
            r.replay.note(what);
            r.dirty = true;
        }
    }
}

/// A recording is written to disk this often while the game runs, and again when the session ends.
const FLUSH_EVERY: u32 = CHECKPOINT_EVERY * 10;

/// Records a game into a `.twrep` file in the replay folder (`Storage::write_replay`). The file is written whole
/// each time, so a crash leaves the last complete write.
pub struct Recorder {
    pub replay: Replay,
    store: Store,
    name: String,
    dirty: bool,
}

impl Recorder {
    /// A recording of the game `header` describes, to be written as `name` in the replay folder.
    pub fn new(header: Header, store: Store, name: String) -> Recorder {
        Recorder {
            replay: Replay::new(header),
            store,
            name,
            dirty: true,
        }
    }

    /// The file name for a game on `map` that started at `wall` seconds: the map's name without folder or extension,
    /// and the start time.
    pub fn file_name(map: &str, wall: u64) -> String {
        let stem = map
            .rsplit(['\\', '/'])
            .next()
            .unwrap_or(map)
            .trim_end_matches(".pud")
            .trim_end_matches(".PUD");
        let stem: String = stem
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
            .collect();
        format!("{stem}-{wall}.{}", tidewater_replay::format::EXTENSION)
    }

    /// Writes the file if anything changed since the last write.
    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        match self.store.write_replay(&self.name, &self.replay.encode()) {
            Ok(()) => self.dirty = false,
            Err(e) => eprintln!("can't write the replay {}: {e}", self.name),
        }
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.flush();
    }
}

/// The speeds a replay can be watched at, as multiples of the game's own.
const RATES: [u32; 4] = [1, 2, 4, 8];

/// Plays a recording: the commands of each cycle come from the file, and the viewer's own input is ignored except
/// pause (holds the playback) and speed (plays faster or slower than the recorded game's speed). Neither of those
/// changes the world, so the replay stays the recording.
pub struct ReplaySession {
    replay: Replay,
    held: bool,
    rate: usize,
    failed: Option<Mismatch>,
    ended: bool,
    notices: Vec<String>,
}

impl ReplaySession {
    pub fn new(replay: Replay) -> ReplaySession {
        let mut notices = Vec::new();
        if !replay.notes.is_empty() {
            notices.push(format!(
                "This recording may not replay exactly: {}.",
                replay.notes.join("; ")
            ));
        }
        ReplaySession {
            replay,
            held: false,
            rate: 0,
            failed: None,
            ended: false,
            notices,
        }
    }

    /// The first checkpoint that did not match, if playback has stopped on one.
    pub fn mismatch(&self) -> Option<Mismatch> {
        self.failed
    }
}

impl Session for ReplaySession {
    fn submit(&mut self, cmd: Command) {
        match cmd {
            Command::Pause { .. } => {
                self.held = !self.held;
                self.notices
                    .push(if self.held { "Replay paused." } else { "Replay resumed." }.to_string());
            }
            Command::Speed { faster, .. } => {
                self.rate = if faster {
                    (self.rate + 1).min(RATES.len() - 1)
                } else {
                    self.rate.saturating_sub(1)
                };
                self.notices.push(format!("Replay speed x{}.", RATES[self.rate]));
            }
            // The viewer's clicks and keys are not the game's.
            _ => {}
        }
    }

    fn immediate(&mut self, _tick: u32, _cmd: Command) -> Option<Command> {
        // The viewer's menu choices are not the game's.
        None
    }

    fn lobby_client_extras(&self) -> Option<crate::extras::ClientExtras> {
        Some(crate::extras::ClientExtras::default())
    }

    fn sim_extras(&self) -> Option<tidewater_sim::Extras> {
        Some(self.replay.header.rules.extras)
    }

    fn ready_commands(&mut self, tick: u32) -> Option<Vec<Command>> {
        if self.failed.is_some() || self.held {
            return None;
        }
        if tick >= self.replay.cycles {
            if !self.ended {
                self.ended = true;
                self.notices.push("End of replay.".to_string());
            }
            return None;
        }
        Some(self.replay.commands_at(tick).to_vec())
    }

    fn checkpoint(&mut self, tick: u32, hash: u64) -> Result<(), Mismatch> {
        match self.replay.checkpoint_at(tick) {
            Some(recorded) if recorded != hash => {
                let m = Mismatch {
                    cycle: tick,
                    recorded,
                    got: hash,
                };
                self.failed = Some(m);
                self.notices.push(format!("Replay stopped: {m}."));
                Err(m)
            }
            _ => Ok(()),
        }
    }

    fn wants_hash(&self, tick: u32) -> bool {
        self.replay.checkpoint_at(tick).is_some()
    }

    fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    fn rate(&self) -> u32 {
        RATES[self.rate]
    }

    fn is_replay(&self) -> bool {
        true
    }

    fn start_cheats(&self) -> Option<u32> {
        Some(self.replay.header.start_cheats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tidewater_replay::{Header, RulesTag};
    use tidewater_sim::setup::Setup;

    fn header() -> Header {
        Header {
            sim_build: "test".into(),
            map_name: "m.pud".into(),
            map_hash: 1,
            seed: 2,
            setup: Setup::default(),
            lobby: None,
            rules: RulesTag {
                enhanced: true,
                multiplayer: false,
                cheats_disabled: false,
                extras: Default::default(),
            },
            start_cheats: 0,
            local: 0,
        }
    }

    fn pause() -> Command {
        Command::Pause { player: 0 }
    }

    fn stop() -> Command {
        Command::Stop {
            player: 0,
            units: vec![],
        }
    }

    #[test]
    fn a_local_session_hands_back_what_was_submitted_in_order() {
        let mut s = LocalSession::new();
        s.submit(stop());
        s.submit(pause());
        assert_eq!(s.ready_commands(0), Some(vec![stop(), pause()]));
        assert_eq!(s.ready_commands(1), Some(vec![]));
        assert!(!s.is_replay());
        assert_eq!(s.rate(), 1);
    }

    #[test]
    fn an_immediate_command_is_recorded_ahead_of_its_cycle_s_others() {
        let mem = std::sync::Arc::new(Memory::default());
        {
            let mut s = LocalSession::recording(Recorder::new(header(), store(&mem), "b.twrep".into()));
            s.submit(stop());
            assert_eq!(s.immediate(4, pause()), Some(pause()));
            assert_eq!(s.ready_commands(4), Some(vec![stop()]));
        }
        let written = mem.0.lock().unwrap().clone();
        let r = Replay::decode(&written.last().unwrap().1).unwrap();
        assert_eq!(r.commands_at(4), &[pause(), stop()]);
        let mut v = ReplaySession::new(r);
        assert_eq!(v.immediate(4, pause()), None, "a viewer's menu is not the game's");
    }

    #[test]
    fn hashes_are_wanted_every_hundred_cycles() {
        let s = LocalSession::new();
        assert!(!s.wants_hash(0) && !s.wants_hash(99) && s.wants_hash(100) && s.wants_hash(200));
    }

    /// A storage that keeps what was written to the replay folder.
    #[derive(Default)]
    struct Memory(Mutex<Vec<(String, Vec<u8>)>>);

    impl crate::storage::Storage for Memory {
        fn install(&self, _: &str) -> std::io::Result<Option<Box<dyn crate::storage::ReadSeek>>> {
            Ok(None)
        }
        fn user_map(&self, _: &str) -> std::io::Result<Option<Vec<u8>>> {
            Ok(None)
        }
        fn saves(&self) -> Vec<String> {
            Vec::new()
        }
        fn read_save(&self, _: &str) -> std::io::Result<Vec<u8>> {
            Err(std::io::ErrorKind::NotFound.into())
        }
        fn write_save(&self, _: &str, _: &[u8]) -> Result<(), crate::storage::WriteError> {
            Ok(())
        }
        fn delete_save(&self, _: &str) -> std::io::Result<()> {
            Ok(())
        }
        fn read_settings(&self) -> Option<Vec<u8>> {
            None
        }
        fn write_settings(&self, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }
        fn has_screenshot(&self, _: &str) -> bool {
            false
        }
        fn write_screenshot(&self, _: &str, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }
        fn write_replay(&self, name: &str, bytes: &[u8]) -> std::io::Result<()> {
            self.0.lock().unwrap().push((name.to_string(), bytes.to_vec()));
            Ok(())
        }
        fn wall_clock_secs(&self) -> u64 {
            0
        }
    }

    fn store(m: &std::sync::Arc<Memory>) -> Store {
        Store(m.clone())
    }

    #[test]
    fn a_recording_session_writes_a_replay_that_plays_back_the_same_commands() {
        let mem = std::sync::Arc::new(Memory::default());
        {
            let mut s = LocalSession::recording(Recorder::new(header(), store(&mem), "a.twrep".into()));
            s.submit(stop());
            s.ready_commands(0);
            s.ready_commands(1);
            s.submit(pause());
            s.ready_commands(2);
            s.checkpoint(100, 77).unwrap();
            s.note_unsynced("fog of war changed");
        } // dropping the session writes the file
        let written = mem.0.lock().unwrap().clone();
        let (name, bytes) = written.last().unwrap();
        assert_eq!(name, "a.twrep");
        let r = Replay::decode(bytes).unwrap();
        assert_eq!(r.cycles, 3);
        assert_eq!(r.commands_at(0), &[stop()]);
        assert_eq!(r.commands_at(2), &[pause()]);
        assert_eq!(r.checkpoint_at(100), Some(77));
        assert_eq!(r.notes, ["fog of war changed"]);
    }

    #[test]
    fn a_replay_session_feeds_each_cycle_its_commands_and_ignores_the_viewer() {
        let mut r = Replay::new(header());
        r.record(1, &[stop()]);
        r.record(2, &[]);
        let mut s = ReplaySession::new(r);
        assert!(s.is_replay());
        s.submit(stop()); // ignored
        assert_eq!(s.ready_commands(0), Some(vec![]));
        assert_eq!(s.ready_commands(1), Some(vec![stop()]));
        assert_eq!(s.ready_commands(2), Some(vec![]));
        assert_eq!(s.ready_commands(3), None, "the recording is over");
        assert_eq!(s.take_notices(), ["End of replay."]);
        assert!(s.take_notices().is_empty());
    }

    #[test]
    fn pause_and_speed_are_the_viewer_s_own() {
        let mut r = Replay::new(header());
        r.record(5, &[]);
        let mut s = ReplaySession::new(r);
        s.submit(pause());
        assert_eq!(s.ready_commands(0), None, "held");
        s.submit(pause());
        assert_eq!(s.ready_commands(0), Some(vec![]));
        assert_eq!(s.rate(), 1);
        s.submit(Command::Speed {
            player: 0,
            faster: true,
        });
        s.submit(Command::Speed {
            player: 0,
            faster: true,
        });
        assert_eq!(s.rate(), 4);
        for _ in 0..9 {
            s.submit(Command::Speed {
                player: 0,
                faster: false,
            });
        }
        assert_eq!(s.rate(), 1);
    }

    #[test]
    fn a_replay_stops_at_the_first_checkpoint_that_differs() {
        let mut r = Replay::new(header());
        r.record(500, &[]);
        r.checkpoint(100, 5);
        let mut s = ReplaySession::new(r);
        assert!(s.wants_hash(100) && !s.wants_hash(101));
        assert_eq!(s.checkpoint(100, 5), Ok(()));
        assert_eq!(s.checkpoint(101, 9), Ok(()), "no checkpoint there");
        let m = s.checkpoint(100, 6).unwrap_err();
        assert_eq!((m.cycle, m.recorded, m.got), (100, 5, 6));
        assert_eq!(s.ready_commands(101), None, "stopped");
        assert!(s.take_notices().iter().any(|n| n.contains("cycle 100")));
    }

    #[test]
    fn recording_names_use_the_map_and_the_start_time() {
        assert_eq!(
            Recorder::file_name("Campaign\\Human01.pud", 1_700_000_000),
            "Human01-1700000000.twrep"
        );
        assert_eq!(Recorder::file_name("my map!.pud", 5), "my_map_-5.twrep");
    }
}
