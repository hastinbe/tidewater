//! The turn engine: lockstep over a [`Wire`] (`docs/specs/net/lockstep.md`, "Turns and the order of commands", "Turn
//! length", "Stalls" and "Sync check"; `docs/specs/net/leave-and-drop.md`).
//!
//! # The model
//!
//! Every player has a queue of **packets**, one command or none each. A turn carries out one packet from every human
//! player, in ascending slot order, each acting as the slot it came from. A turn runs just before the cycle whose
//! counter is `next_turn_tick`, then the countdown is reloaded with the turn length K ([`turn_length`]). The first two
//! turns both run before cycle 0 (the spec's game start). Packets are numbered; turn `n` carries packet `n` of each
//! player.
//!
//! - **Queue.** The queue holds at most [`QUEUE_LIMIT`] packets and is topped up to [`QUEUE_TARGET`] with empty ones at
//!   every turn start, so one packet is already waiting ahead of any new command: a command read at counter `c` runs
//!   in the turn before counter `K × ceil(c / K) + K`. Packets are sent the moment they are queued.
//! - **Stall.** A turn waits until every human's packet for it has arrived. [`Waiting`] says for how long, from which
//!   the interface shows the watch cursor (more than 2 s) and the waiting dialog (more than 7 s). Over TCP there is
//!   nothing to resend, so the resend interval of the original is only the unit the thresholds are given in.
//! - **Sync check.** At the start of turn `n` each machine sends [`Msg::Sync`] with the digest of its game state (the
//!   world's `state_hash`, which covers exactly what every machine computes the same way). At turn `n + 1` the digests
//!   of turn `n` are compared, which makes a difference that arises between two turns show at the second turn after it,
//!   the original's turn (`net/lockstep.md`, "Sync check", Tidewater's design choice). A machine whose digest differs
//!   from this one's is dropped, in place of its packet, and "Player %s was dropped." shows.
//! - **Leaving.** A leave is a [`Command::Leave`] in the leaver's own packet, carried out in the turn like any command.
//!   A drop one machine decides on (Alt+F2, a lost connection) goes through the host, which numbers it into the
//!   dropped player's packet stream ([`Msg::Drop`]), so every machine carries it out in the same turn.
//!
//! # What the engine is not
//!
//! It never reads or writes the world: the client hands it the state digest ([`Session::checkpoint`]) and the game
//! speed ([`Session::observe_speed`]), and it hands back the commands of each cycle. Time comes from an injected
//! clock, so tests run stalls without waiting.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tidewater_replay::format::CHECKPOINT_EVERY;
use tidewater_replay::run::Mismatch;
use tidewater_sim::Command;
use tidewater_sim::timing::{CLASSIC_MS, CLASSIC_ROW, ENHANCED_MS};

use super::proto::Msg;
use super::tcp::{Inbound, Wire};
use crate::session::{ChatLine, LeaveNotice, Recorder, Session, Waiting};

/// The most packets a player's queue holds; a command issued with this many waiting is dropped (`net/lockstep.md`).
pub const QUEUE_LIMIT: usize = 26;
/// The queue is topped up to this many packets at every turn start.
pub const QUEUE_TARGET: usize = 2;
/// The wait after which the cursor becomes the watch: more than two resend intervals of 1000 ms.
pub const WATCH_AFTER: Duration = Duration::from_millis(2000);
/// The wait after which the "Waiting for players" dialog opens: more than seven intervals.
pub const DIALOG_AFTER: Duration = Duration::from_millis(7000);
/// A turn that completes after waiting longer than this resets the cycle deadline.
pub const DEADLINE_RESET_AFTER: Duration = Duration::from_millis(200);

/// T of `net/lockstep.md`, "Data": the turn length in cycles at the fastest speed, for each network delay row.
pub const ROW_CYCLES: [u32; 16] = [1, 2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 96, 128, 192, 255];

/// The row Direct IP uses (`net/lockstep.md`, "Which row", gap-fill: T = 8, K = 8 at speed 8), and the row the traced
/// Direct Connect game ran (T = 12, K = 12 at speed 8).
pub const DIRECT_IP_ROW: u8 = 3;
pub const TRACED_DIRECT_CONNECT_ROW: u8 = 4;

/// K, the cycles in one turn, for network delay `row` at game `speed` (0 to 8) under Enhanced or Classic rules
/// (`net/lockstep.md`, "Turn length"). Enhanced: `ceil(T × 13 / L)` with L the speed's cycle length in ms. Classic:
/// `ceil(T × 60 / S)` with S the sum of the speed's three cycle lengths.
pub fn turn_length(row: u8, enhanced: bool, speed: u8) -> u32 {
    let t = ROW_CYCLES[usize::from(row).min(ROW_CYCLES.len() - 1)];
    let speed = usize::from(speed).min(ENHANCED_MS.len() - 1);
    let k = if enhanced {
        (t * 13).div_ceil(ENHANCED_MS[speed])
    } else {
        let sum: u32 = CLASSIC_MS[CLASSIC_ROW[speed]].iter().sum();
        (t * 60).div_ceil(sum)
    };
    k.max(1)
}

/// How long a wait has gone on, in the spec's terms.
pub fn stall_stage(waited: Duration) -> (bool, bool) {
    (waited > WATCH_AFTER, waited > DIALOG_AFTER)
}

/// What a machine needs to know about the game to run its turns.
#[derive(Clone, Debug)]
pub struct Params {
    /// This machine's slot.
    pub me: u8,
    /// The host's slot: the machine that decides drops for everyone.
    pub host: u8,
    /// Every human slot, this one and the host's included.
    pub humans: Vec<u8>,
    /// Slot names, indexed by slot (shorter lists give "Player N").
    pub names: Vec<String>,
    pub enhanced: bool,
    /// The network delay row ([`ROW_CYCLES`]).
    pub row: u8,
    /// The game speed the world starts at.
    pub speed: u8,
}

type Clock = Box<dyn Fn() -> Duration + Send + Sync>;

/// A chat line as it is accepted from the network: printable ASCII only, at most the message line's length. What
/// the typing side lets through anyway; a program that is not ours may send anything.
fn clean_chat(text: &str) -> String {
    text.chars()
        .filter(|c| (' '..='~').contains(c))
        .take(crate::messages::MAX_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Clone, Debug)]
struct Packet {
    seq: u32,
    cmd: Option<Command>,
}

/// A network game's [`Session`]. See the module doc.
pub struct NetSession {
    p: Params,
    wire: Mutex<Box<dyn Wire>>,
    clock: Clock,
    recorder: Option<Recorder>,

    /// Humans still in the game as this machine sees it.
    active: BTreeSet<u8>,
    /// The index of the next turn to carry out, and the cycle counter it runs before.
    turn: u32,
    next_turn_tick: u32,
    /// The turn-start work (digest sent, queue topped up) is done for `turn`.
    started: bool,
    /// The game speed as last seen, for the next turn's length.
    speed: u8,

    /// This player's packets, sent and not yet carried out; the first is packet `turn`.
    queue: VecDeque<Packet>,
    next_seq: u32,
    /// Other players' packets that arrived, by slot and sequence number.
    inbox: BTreeMap<u8, BTreeMap<u32, Option<Command>>>,
    /// How many packets have arrived in order from each slot (the next expected sequence number).
    received: BTreeMap<u8, u32>,
    /// A slot dropped at its packet number: the packets before it count, that one is a drop.
    dropped_at: BTreeMap<u8, u32>,
    /// Digests other players sent, by (slot, turn), and this machine's own by turn.
    syncs: BTreeMap<(u8, u32), u64>,
    digests: BTreeMap<u32, u64>,

    /// Commands of the cycle being handed out (more than one turn at the start).
    carry: Vec<Command>,
    notices: Vec<LeaveNotice>,
    /// Chat lines to hand to the screen.
    chat: Vec<ChatLine>,
    /// The turn is waiting since; for whom.
    stalled_since: Option<Duration>,
    missing: Vec<u8>,
    reset_deadline: bool,
    /// This machine's game is over for it: it aborted, or the others dropped it.
    exit: bool,
    /// The leave packet has been sent.
    left: bool,
    /// The host's connection is gone (a joiner's view).
    host_gone: bool,
}

impl NetSession {
    /// A session over `wire` for the game `params` describes, optionally recording it. The two empty packets the
    /// spec's game start queues go out now.
    pub fn new(params: Params, wire: Box<dyn Wire>, recorder: Option<Recorder>) -> NetSession {
        let start = Instant::now();
        NetSession::with_clock(params, wire, recorder, Box::new(move || start.elapsed()))
    }

    /// [`NetSession::new`] over a clock of the caller's (tests step time by hand).
    pub fn with_clock(params: Params, wire: Box<dyn Wire>, recorder: Option<Recorder>, clock: Clock) -> NetSession {
        let mut s = NetSession {
            active: params.humans.iter().copied().collect(),
            speed: params.speed,
            p: params,
            wire: Mutex::new(wire),
            clock,
            recorder,
            turn: 0,
            next_turn_tick: 0,
            started: false,
            queue: VecDeque::new(),
            next_seq: 0,
            inbox: BTreeMap::new(),
            received: BTreeMap::new(),
            dropped_at: BTreeMap::new(),
            syncs: BTreeMap::new(),
            digests: BTreeMap::new(),
            carry: Vec::new(),
            notices: Vec::new(),
            chat: Vec::new(),
            stalled_since: None,
            missing: Vec::new(),
            reset_deadline: false,
            exit: false,
            left: false,
            host_gone: false,
        };
        for _ in 0..QUEUE_TARGET {
            s.queue_packet(None);
        }
        s
    }

    /// The index of the next turn to carry out (turns 0 and 1 are the game start's pair).
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The cycle counter the next turn runs before.
    pub fn next_turn_tick(&self) -> u32 {
        self.next_turn_tick
    }

    /// The humans still in the game, as this machine sees it.
    pub fn players(&self) -> Vec<u8> {
        self.active.iter().copied().collect()
    }

    /// Whether this machine decides drops for everyone.
    fn is_host(&self) -> bool {
        self.p.me == self.p.host
    }

    fn now(&self) -> Duration {
        (self.clock)()
    }

    fn wire(&mut self) -> &mut dyn Wire {
        self.wire
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_mut()
    }

    fn name(&self, slot: u8) -> String {
        match self.p.names.get(slot as usize) {
            Some(n) if !n.is_empty() => n.clone(),
            _ => format!("Player {}", u32::from(slot) + 1),
        }
    }

    /// Puts a packet at the back of the queue and sends it.
    fn queue_packet(&mut self, cmd: Option<Command>) {
        let packet = Packet {
            seq: self.next_seq,
            cmd,
        };
        self.next_seq += 1;
        let msg = Msg::Turn {
            slot: self.p.me,
            seq: packet.seq,
            cmd: packet.cmd.clone(),
        };
        self.queue.push_back(packet);
        self.wire().send(msg);
    }

    /// Reads what has arrived and acts on it.
    fn pump(&mut self) {
        let inbound = self.wire().poll();
        for i in inbound {
            match i {
                Inbound::Msg { from, msg } => self.hear(from, msg),
                Inbound::Closed { slot } => self.closed(slot),
            }
        }
    }

    fn hear(&mut self, from: u8, msg: Msg) {
        match msg {
            Msg::Turn { slot, seq, cmd } => {
                if slot == self.p.me || !self.active.contains(&slot) {
                    return;
                }
                let expected = self.received.entry(slot).or_insert(0);
                // Packets arrive in order over one connection; anything else is a program that is not ours.
                if seq != *expected || self.dropped_at.get(&slot).is_some_and(|&a| seq >= a) {
                    return;
                }
                *expected += 1;
                self.inbox.entry(slot).or_default().insert(seq, cmd.clone());
                if self.is_host() {
                    self.wire().forward(slot, &Msg::Turn { slot, seq, cmd });
                }
            }
            Msg::Sync { slot, turn, digest } => {
                if slot != self.p.me && self.active.contains(&slot) {
                    self.syncs.insert((slot, turn), digest);
                    if self.is_host() {
                        self.wire().forward(slot, &Msg::Sync { slot, turn, digest });
                    }
                }
            }
            // A drop is the host's to decide.
            Msg::Drop { slot, at_seq } if from == self.p.host && !self.is_host() => {
                if slot == self.p.me {
                    // The others have carried on without this machine.
                    self.exit = true;
                } else if self.active.contains(&slot) {
                    self.dropped_at.entry(slot).or_insert(at_seq);
                }
            }
            Msg::DropRequest { slot } if self.is_host() => self.issue_drop(slot),
            Msg::Chat { slot, allies, text } => {
                // The host knows who sent it; a joiner takes the slot from the host, which stamped it.
                let slot = if self.is_host() { from } else { slot };
                if slot == self.p.me || !self.p.humans.contains(&slot) {
                    return;
                }
                let text = clean_chat(&text);
                if text.is_empty() {
                    return;
                }
                if self.is_host() {
                    self.wire().forward(
                        slot,
                        &Msg::Chat {
                            slot,
                            allies,
                            text: text.clone(),
                        },
                    );
                }
                let name = self.name(slot);
                self.chat.push(ChatLine {
                    slot,
                    name,
                    text,
                    allies,
                    mine: false,
                });
            }
            _ => {}
        }
    }

    fn closed(&mut self, slot: u8) {
        if self.is_host() {
            self.issue_drop(slot);
        } else if slot == self.p.host && !self.host_gone {
            // spec gap: the original has no host to lose (its players each hold every connection). Tidewater's star
            // topology does: without the host this machine cannot hear the others, so it drops them and plays on alone.
            self.host_gone = true;
            for s in self.active.clone() {
                if s != self.p.me {
                    let at = self.received.get(&s).copied().unwrap_or(0);
                    self.dropped_at.entry(s).or_insert(at);
                }
            }
        }
    }

    /// The host drops `slot`: its packets from the next one it has not yet relayed do not count, and its packet at
    /// that number is a drop on every machine.
    fn issue_drop(&mut self, slot: u8) {
        if slot == self.p.me || !self.active.contains(&slot) || self.dropped_at.contains_key(&slot) {
            return;
        }
        let at_seq = self.received.get(&slot).copied().unwrap_or(0);
        self.dropped_at.insert(slot, at_seq);
        self.wire().send(Msg::Drop { slot, at_seq });
    }

    /// Whether `slot`'s packet for turn `n` is in (or is a drop).
    fn packet_ready(&self, slot: u8, n: u32) -> bool {
        self.dropped_at.get(&slot) == Some(&n) || self.inbox.get(&slot).is_some_and(|m| m.contains_key(&n))
    }

    /// Whether the check of turn `n` has what it needs from `slot`: its digest for turn `n - 1`, unless its packet is
    /// a leave or a drop (a leaver may be gone before it reached that turn).
    fn sync_ready(&self, slot: u8, n: u32) -> bool {
        if n < 3 || self.dropped_at.get(&slot) == Some(&n) {
            return true;
        }
        let leaving = matches!(
            self.inbox.get(&slot).and_then(|m| m.get(&n)),
            Some(Some(Command::Leave { .. }))
        );
        leaving || self.syncs.contains_key(&(slot, n - 1))
    }

    fn missing_players(&self) -> Vec<u8> {
        let n = self.turn;
        self.active
            .iter()
            .copied()
            .filter(|&s| s != self.p.me && !(self.packet_ready(s, n) && self.sync_ready(s, n)))
            .collect()
    }

    /// Works on the turn that is due: the turn-start work once, then the turn itself if everything for it is in.
    /// Returns whether the turn was carried out.
    fn try_turn(&mut self, tick: u32) -> bool {
        let n = self.turn;
        if !self.started {
            self.started = true;
            if n >= 2
                && let Some(&digest) = self.digests.get(&n)
            {
                let msg = Msg::Sync {
                    slot: self.p.me,
                    turn: n,
                    digest,
                };
                self.wire().send(msg);
            }
            while self.queue.len() < QUEUE_TARGET {
                self.queue_packet(None);
            }
        }
        let missing = self.missing_players();
        if !missing.is_empty() {
            self.missing = missing;
            return false;
        }
        self.missing.clear();
        let mine = self.queue.pop_front().expect("the queue was topped up");
        debug_assert_eq!(mine.seq, n, "turn {n} carries packet {n}");
        let mut leavers = Vec::new();
        if n == 0 {
            // The lobby's game speed, as the first commands of the game: the world starts at Normal, and commands are
            // what a recording replays (`docs/design/network.md`, "Game speed").
            let from = i32::from(tidewater_sim::timing::NORMAL);
            let to = i32::from(self.p.speed.min(8));
            for _ in 0..(to - from).unsigned_abs() {
                self.carry.push(Command::Speed {
                    player: self.p.host,
                    faster: to > from,
                });
            }
        }
        for s in self.active.clone() {
            let mut cmd = if s == self.p.me {
                mine.cmd.clone()
            } else if self.dropped_at.get(&s) == Some(&n) {
                // Made up in place of the missing player's packet (`net/leave-and-drop.md`, "Dropped").
                Some(Command::Leave {
                    player: s,
                    dropped: true,
                })
            } else {
                self.inbox.get_mut(&s).and_then(|m| m.remove(&n)).flatten()
            };
            // The sync check: this player's digest of the last turn against ours.
            if s != self.p.me && n >= 3 && !matches!(cmd, Some(Command::Leave { .. })) {
                let theirs = self.syncs.get(&(s, n - 1));
                if let (Some(&mine), Some(&theirs)) = (self.digests.get(&(n - 1)), theirs)
                    && mine != theirs
                {
                    cmd = Some(Command::Leave {
                        player: s,
                        dropped: true,
                    });
                }
            }
            // A packet acts as the slot it came from, whatever its command says; and a surrender is local to the
            // machine that made it (`net/leave-and-drop.md`, "Surrender and quit (draw)"), never a turn command.
            cmd = cmd.filter(|c| c.player() == s && !matches!(c, Command::Surrender { .. } | Command::QuitDraw { .. }));
            if let Some(c) = cmd {
                if let Command::Leave { dropped, .. } = c {
                    leavers.push((s, dropped));
                }
                self.carry.push(c);
            }
        }
        for (slot, dropped) in leavers {
            self.active.remove(&slot);
            let name = self.name(slot);
            self.notices.push(LeaveNotice { slot, name, dropped });
        }
        // Forget what the check can no longer use.
        let keep = n.saturating_sub(2);
        self.syncs.retain(|&(_, t), _| t >= keep);
        self.digests.retain(|&t, _| t >= keep);
        self.turn += 1;
        self.started = false;
        // The first turn is followed by the second at once; after that, the countdown is K.
        self.next_turn_tick = if n == 0 {
            tick
        } else {
            let speed = if n < 2 { self.p.speed } else { self.speed };
            tick + turn_length(self.p.row, self.p.enhanced, speed)
        };
        true
    }

    /// Queues this player's leave and tells the others. Once.
    fn send_leave(&mut self, dropped: bool) {
        if !std::mem::replace(&mut self.left, true) {
            self.queue_packet(Some(Command::Leave {
                player: self.p.me,
                dropped,
            }));
        }
    }
}

impl Session for NetSession {
    fn submit(&mut self, cmd: Command) {
        if self.exit || self.left || self.queue.len() >= QUEUE_LIMIT {
            return;
        }
        self.queue_packet(Some(cmd));
    }

    /// A surrender is not a turn command: it ends this machine's game at once, and the machine leaves the session as
    /// its game ends, so the others see "left" (`net/leave-and-drop.md`, "Surrender and quit (draw)").
    fn immediate(&mut self, tick: u32, cmd: Command) -> Option<Command> {
        match cmd {
            Command::Surrender { .. } | Command::QuitDraw { .. } => {
                self.send_leave(false);
                // This machine's recording keeps it, so the recording replays this machine's game.
                if let Some(r) = &mut self.recorder {
                    r.record(tick, std::slice::from_ref(&cmd));
                }
                Some(cmd)
            }
            _ => None,
        }
    }

    fn ready_commands(&mut self, tick: u32) -> Option<Vec<Command>> {
        self.pump();
        while tick >= self.next_turn_tick {
            if !self.try_turn(tick) {
                let now = self.now();
                self.stalled_since.get_or_insert(now);
                return None;
            }
        }
        if let Some(since) = self.stalled_since.take()
            && self.now().saturating_sub(since) > DEADLINE_RESET_AFTER
        {
            self.reset_deadline = true;
        }
        let out = std::mem::take(&mut self.carry);
        if let Some(r) = &mut self.recorder {
            r.record(tick, &out);
        }
        Some(out)
    }

    fn wants_hash(&self, tick: u32) -> bool {
        tick == self.next_turn_tick || (tick > 0 && tick.is_multiple_of(CHECKPOINT_EVERY))
    }

    fn checkpoint(&mut self, tick: u32, hash: u64) -> Result<(), Mismatch> {
        // The state at the start of the turn that is due, before its commands: the digest the next turn checks.
        if tick == self.next_turn_tick && !self.started {
            self.digests.insert(self.turn, hash);
        }
        // A turn's digest alone is not a recording checkpoint; the recording keeps the usual every hundredth cycle
        // and the end of the game.
        let only_a_turn = tick == self.next_turn_tick && !tick.is_multiple_of(CHECKPOINT_EVERY);
        if !only_a_turn && let Some(r) = &mut self.recorder {
            r.checkpoint(tick, hash);
        }
        Ok(())
    }

    fn note_unsynced(&mut self, what: &str) {
        if let Some(r) = &mut self.recorder {
            r.replay.note(what);
        }
    }

    fn observe_speed(&mut self, speed: u8) {
        self.speed = speed;
    }

    fn waiting(&self) -> Option<Waiting> {
        let since = self.stalled_since?;
        let (watch, dialog) = stall_stage(self.now().saturating_sub(since));
        Some(Waiting {
            watch,
            dialog,
            missing: self.missing.iter().map(|&s| self.name(s)).collect(),
        })
    }

    fn take_leave_notices(&mut self) -> Vec<LeaveNotice> {
        std::mem::take(&mut self.notices)
    }

    fn take_deadline_reset(&mut self) -> bool {
        std::mem::take(&mut self.reset_deadline)
    }

    fn take_exit(&mut self) -> Option<Command> {
        std::mem::take(&mut self.exit).then_some(Command::Surrender { player: self.p.me })
    }

    fn abort_game(&mut self) {
        // Ctrl+F1 is checked while the game waits for a turn (`net/lockstep.md`, "Stalls", step 5).
        if self.stalled_since.is_some() && !self.exit {
            self.send_leave(true);
            self.exit = true;
        }
    }

    fn drop_missing(&mut self) {
        // Alt+F2 counts from the eighth interval on (step 6): the dialog is open.
        let Some(w) = self.waiting() else { return };
        if !w.dialog {
            return;
        }
        self.pump();
        for slot in self.missing_players() {
            if self.is_host() {
                self.issue_drop(slot);
            } else {
                self.wire().send(Msg::DropRequest { slot });
            }
        }
    }

    fn is_network(&self) -> bool {
        true
    }

    fn send_chat(&mut self, allies: bool, text: &str) {
        let text = clean_chat(text);
        if text.is_empty() || self.exit || self.left {
            return;
        }
        let slot = self.p.me;
        self.wire().send(Msg::Chat {
            slot,
            allies,
            text: text.clone(),
        });
        let name = self.name(self.p.me);
        self.chat.push(ChatLine {
            slot: self.p.me,
            name,
            text,
            allies,
            mine: true,
        });
    }

    fn take_chat(&mut self) -> Vec<ChatLine> {
        std::mem::take(&mut self.chat)
    }
}

impl Drop for NetSession {
    /// Ending the game while still in the session tells the others this player left, as the last thing it does.
    fn drop(&mut self) {
        self.send_leave(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::proto::{GameConfig, Identity};
    use crate::net::tcp::{self, Joined};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;
    use tidewater_sim::setup::{Lobby, Setup};

    // --- the turn length table ---

    #[test]
    fn turn_lengths_match_the_specs_tables() {
        // Enhanced, `net/lockstep.md` "Data": rows 0 to 15 at speeds 0, 4 and 8.
        let enhanced: [[u32; 3]; 16] = [
            [1, 1, 1],
            [1, 1, 2],
            [1, 2, 4],
            [2, 3, 8],
            [2, 4, 12],
            [3, 6, 16],
            [4, 7, 20],
            [4, 8, 24],
            [6, 11, 32],
            [7, 13, 40],
            [8, 16, 48],
            [11, 21, 64],
            [16, 32, 96],
            [21, 42, 128],
            [32, 63, 192],
            [42, 83, 255],
        ];
        for (row, want) in enhanced.iter().enumerate() {
            let got = [0u8, 4, 8].map(|s| turn_length(row as u8, true, s));
            assert_eq!(&got, want, "Enhanced row {row}");
        }
        // Classic mode: row 4 at Classic speed 6 (the 9-step setting 7 or 8) is 12, at Classic speed 0 it is 4.
        assert_eq!(turn_length(4, false, 8), 12);
        assert_eq!(turn_length(4, false, 7), 12);
        assert_eq!(turn_length(4, false, 0), 4);
        assert_eq!(turn_length(15, false, 0), 85);
        // The vectors: IPX row 1 at speed 8 is 2; row 4 at speed 4 is 4; the traced Direct Connect row 4 is 12.
        assert_eq!(turn_length(1, true, 8), 2);
        assert_eq!(turn_length(4, true, 4), 4);
        assert_eq!(turn_length(TRACED_DIRECT_CONNECT_ROW, true, 8), 12);
        // Direct IP (gap-fill, row 3): 8 cycles at speed 8, about 104 ms.
        assert_eq!(turn_length(DIRECT_IP_ROW, true, 8), 8);
        assert_eq!(turn_length(0, true, 8), 1);
    }

    #[test]
    fn stall_thresholds_are_the_specs() {
        let ms = Duration::from_millis;
        assert_eq!(stall_stage(ms(0)), (false, false));
        assert_eq!(stall_stage(ms(2000)), (false, false));
        assert_eq!(stall_stage(ms(2001)), (true, false));
        assert_eq!(stall_stage(ms(7000)), (true, false));
        assert_eq!(stall_stage(ms(7001)), (true, true));
    }

    // --- peers in one process over loopback TCP, on a stand-in world ---

    /// A stand-in for the world: a counter and a hash of everything it was handed, so peers that are handed the same
    /// commands in the same cycles have the same hash and any difference shows.
    struct Peer {
        s: NetSession,
        tick: u32,
        state: u64,
        /// The commands each cycle was handed, by the cycle's counter (cycles without commands left out).
        applied: Vec<(u32, Vec<Command>)>,
        /// Commands to submit when the counter reaches a cycle.
        script: BTreeMap<u32, Vec<Command>>,
        /// A peer that is told to stand still gives no cycles.
        frozen: bool,
    }

    impl Peer {
        fn new(s: NetSession) -> Peer {
            Peer {
                s,
                tick: 0,
                state: 0xcbf2_9ce4_8422_2325,
                applied: Vec::new(),
                script: BTreeMap::new(),
                frozen: false,
            }
        }

        /// One cycle if the session allows it, as `game::tick` does it.
        fn advance(&mut self) -> bool {
            if self.frozen {
                return false;
            }
            if let Some(cmds) = self.script.remove(&self.tick) {
                for c in cmds {
                    self.s.submit(c);
                }
            }
            let Some(cmds) = self.s.ready_commands(self.tick) else {
                return false;
            };
            self.state = crate::net::proto::fnv_extend(self.state, format!("{}:{cmds:?}", self.tick).as_bytes());
            // The lobby's speed arrives as commands at the very start; the tests look at the players' own.
            let cmds: Vec<Command> = cmds
                .into_iter()
                .filter(|c| !matches!(c, Command::Speed { .. }))
                .collect();
            if !cmds.is_empty() {
                self.applied.push((self.tick, cmds));
            }
            self.tick += 1;
            if self.s.wants_hash(self.tick) {
                self.s.checkpoint(self.tick, self.state).unwrap();
            }
            true
        }
    }

    fn stop(player: u8) -> Command {
        Command::Stop { player, units: vec![] }
    }

    fn config(humans: usize, row: u8, speed: u8) -> GameConfig {
        GameConfig {
            map_name: "m.pud".into(),
            map_hash: 1,
            seed: 1,
            setup: Setup::default(),
            lobby: Lobby {
                humans: (0..humans as u8).map(|s| (s, s % 2)).collect(),
                local: 0,
                computers: vec![],
                fixed_order: true,
            },
            speed,
            row,
            extras: Default::default(),
            names: (0..humans).map(|s| format!("Player {}", s + 1)).collect(),
        }
    }

    fn params(me: u8, cfg: &GameConfig, names: Vec<String>) -> Params {
        Params {
            me,
            host: cfg.lobby.humans[0].0,
            humans: cfg.lobby.humans.iter().map(|h| h.0).collect(),
            names,
            enhanced: cfg.setup.enhanced,
            row: cfg.row,
            speed: cfg.speed,
        }
    }

    /// A manual clock several sessions can read.
    #[derive(Clone, Default)]
    struct Time(Arc<AtomicU64>);

    impl Time {
        fn advance(&self, d: Duration) {
            self.0.fetch_add(d.as_millis() as u64, Ordering::SeqCst);
        }
        fn clock(&self) -> Clock {
            let t = self.0.clone();
            Box::new(move || Duration::from_millis(t.load(Ordering::SeqCst)))
        }
    }

    /// `humans` peers on loopback: the host is peer 0 and the joiners follow in slot order.
    fn game(humans: usize, row: u8, speed: u8, time: &Time) -> Vec<Peer> {
        let listener = tcp::listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let cfg = config(humans, row, speed);
        let slots: Vec<u8> = (1..humans as u8).collect();
        let host = thread::spawn({
            let (id, cfg) = (id.clone(), cfg.clone());
            move || tcp::host_game(&listener, cfg, &id, &slots, |_| {}).unwrap()
        });
        let mut joiners: Vec<thread::JoinHandle<Joined>> = Vec::new();
        for _ in 1..humans {
            let id = id.clone();
            joiners.push(thread::spawn(move || {
                tcp::join_game(&format!("127.0.0.1:{port}"), id, "", |_| Ok(()), |_| {}).unwrap()
            }));
            // One at a time, so the slots come out in order.
            thread::sleep(Duration::from_millis(60));
        }
        let host_wire = host.join().unwrap();
        let mut peers = Vec::new();
        let names = |n: &[String]| n.to_vec();
        peers.push(Peer::new(NetSession::with_clock(
            params(0, &cfg, names(&cfg.names)),
            Box::new(host_wire),
            None,
            time.clock(),
        )));
        for j in joiners {
            let j = j.join().unwrap();
            peers.push(Peer::new(NetSession::with_clock(
                params(j.slot, &j.config, j.names.clone()),
                Box::new(j.wire),
                None,
                time.clock(),
            )));
        }
        peers
    }

    /// Runs every peer until each has reached cycle `until` or `stuck` consecutive rounds gave no progress.
    fn run(peers: &mut [Peer], until: u32) {
        let mut idle = 0;
        while peers.iter().any(|p| p.tick < until && !p.frozen) {
            let mut moved = false;
            for p in peers.iter_mut().filter(|p| p.tick < until) {
                moved |= p.advance();
            }
            if moved {
                idle = 0;
            } else {
                idle += 1;
                assert!(
                    idle < 3000,
                    "the game stopped: {:?}",
                    peers.iter().map(|p| p.tick).collect::<Vec<_>>()
                );
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    /// The cycle counters at which `cmd` was handed out.
    fn when(p: &Peer, cmd: &Command) -> Vec<u32> {
        p.applied
            .iter()
            .filter(|(_, c)| c.contains(cmd))
            .map(|&(t, _)| t)
            .collect()
    }

    #[test]
    fn game_start_runs_two_turns_before_cycle_zero_then_every_k_cycles() {
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        assert_eq!(peers[0].s.next_turn_tick(), 0);
        run(&mut peers, 1);
        assert_eq!(peers[0].s.turn(), 2, "two turns before the first cycle");
        assert_eq!(peers[0].s.next_turn_tick(), 12);
        run(&mut peers, 12);
        assert_eq!(peers[0].s.turn(), 2, "no turn between counters 1 and 11");
        run(&mut peers, 13);
        assert_eq!(peers[0].s.turn(), 3, "a turn before counter 12");
        run(&mut peers, 300);
        // 26 turns in 300 cycles: the pair at the start, then one at 12, 24, ..., 288.
        assert_eq!(peers[1].s.turn(), 26);
    }

    #[test]
    fn a_command_read_at_counter_c_runs_in_the_turn_before_k_times_ceil_c_over_k_plus_k() {
        // `net/lockstep.md`, "Latency": the command waits behind the packet already queued. Each case in a game of its own.
        let time = Time::default();
        for (read, runs) in [(1, 24), (11, 24), (12, 24), (13, 36), (24, 36), (25, 48), (100, 120)] {
            let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
            peers[0].script.insert(read, vec![stop(0)]);
            run(&mut peers, runs + 13);
            for p in &peers {
                assert_eq!(when(p, &stop(0)), [runs], "read at {read}");
            }
        }
    }

    #[test]
    fn a_command_waits_for_the_second_turn_to_come_mp_l6() {
        // `net/lockstep.md` test vectors, Enhanced Direct Connect at speed 8 (K = 12).
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        let c = |n: i32| Command::Move {
            player: 0,
            units: vec![],
            to: (n, 0),
        };
        // Two commands read in one frame at counter 25: turns before 48 and 60.
        peers[0].script.insert(25, vec![c(4), c(5)]);
        // MP-L6: a select and a move in one frame at counter 198 run before 216 and 228; at 299, 312 and 324.
        let sel = Command::Select {
            player: 0,
            units: vec![],
        };
        let mv = |n: i32| Command::Move {
            player: 0,
            units: vec![],
            to: (n, 9),
        };
        peers[0].script.insert(198, vec![sel.clone(), mv(1)]);
        peers[0].script.insert(299, vec![sel.clone(), mv(2)]);
        run(&mut peers, 340);
        for (i, p) in peers.iter().enumerate() {
            assert_eq!(when(p, &c(4)), [48], "peer {i}");
            assert_eq!(when(p, &c(5)), [60], "peer {i}");
            assert_eq!(when(p, &sel), [216, 312], "peer {i}");
            assert_eq!(when(p, &mv(1)), [228], "peer {i}");
            assert_eq!(when(p, &mv(2)), [324], "peer {i}");
        }
        assert_eq!(peers[0].state, peers[1].state);
        assert_eq!(
            peers[0].applied, peers[1].applied,
            "both ran the same commands in the same cycles"
        );
    }

    #[test]
    fn single_player_pace_and_ipx_pace_follow_the_same_rule() {
        // Row 0: every cycle is a turn; a command read at counter c runs in the turn before counter c + 1.
        let time = Time::default();
        let mut peers = game(2, 0, 8, &time);
        peers[0].script.insert(7, vec![stop(0)]);
        run(&mut peers, 20);
        assert_eq!(when(&peers[1], &stop(0)), [8]);
        // IPX network, Enhanced speed 8 (row 1, K = 2): a command read at counter 3 runs before counter 6.
        let mut peers = game(2, 1, 8, &time);
        peers[0].script.insert(3, vec![stop(0)]);
        run(&mut peers, 20);
        assert_eq!(when(&peers[1], &stop(0)), [6]);
        // Row 4 at speed 4: K = 4, a delay of 4 to 7 cycles.
        let mut peers = game(2, 4, 4, &time);
        peers[0].script.insert(9, vec![stop(0)]);
        run(&mut peers, 40);
        assert_eq!(when(&peers[1], &stop(0)), [16]);
    }

    #[test]
    fn players_are_carried_out_in_slot_order_and_a_packet_acts_as_its_own_slot() {
        let time = Time::default();
        let mut peers = game(3, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        // All three give a command in the same cycle; the third also forges one for slot 0 and another surrender.
        peers[2].script.insert(5, vec![stop(2)]);
        peers[1].script.insert(5, vec![stop(1)]);
        peers[0].script.insert(5, vec![stop(0)]);
        peers[1].script.insert(30, vec![stop(0)]);
        peers[2].script.insert(30, vec![Command::Surrender { player: 2 }]);
        run(&mut peers, 70);
        for p in &peers {
            assert_eq!(p.applied[0], (24, vec![stop(0), stop(1), stop(2)]));
            assert_eq!(
                p.applied.len(),
                1,
                "the forged command and the surrender never ran: {:?}",
                p.applied
            );
        }
        assert_eq!(peers[0].state, peers[2].state);
    }

    #[test]
    fn chat_reaches_everyone_by_way_of_the_host_and_stays_out_of_the_turns() {
        let time = Time::default();
        let mut peers = game(3, 1, 8, &time);
        run(&mut peers, 4);
        // A joiner talks: the other joiner hears it through the host, the host hears it, and the sender gets its own
        // line back marked as its own.
        peers[1].s.send_chat(true, "hold the ford \u{7f}\u{e9}");
        peers[1].s.send_chat(false, "   ");
        let mut heard: Vec<Vec<ChatLine>> = vec![Vec::new(); 3];
        for _ in 0..400 {
            let to = peers[0].tick + 1;
            run(&mut peers, to);
            for (h, p) in heard.iter_mut().zip(peers.iter_mut()) {
                h.extend(p.s.take_chat());
            }
            if heard.iter().all(|h| !h.is_empty()) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let want = |slot: u8, mine: bool| ChatLine {
            slot,
            name: format!("Player {}", slot + 1),
            text: "hold the ford".into(),
            allies: true,
            mine,
        };
        assert_eq!(heard[0], [want(1, false)]);
        assert_eq!(heard[2], [want(1, false)]);
        assert_eq!(heard[1], [want(1, true)]);
        assert!(peers[0].s.take_chat().is_empty(), "taken once");
        // Nothing of it is a command.
        assert!(peers.iter().all(|p| p.applied.is_empty()));
        let states: BTreeSet<u64> = peers.iter().map(|p| p.state).collect();
        assert_eq!(states.len(), 1);
    }

    #[test]
    fn a_full_queue_drops_commands_silently() {
        let time = Time::default();
        let mut peers = game(2, 1, 8, &time);
        let many: Vec<Command> = (0..40)
            .map(|n| Command::Move {
                player: 0,
                units: vec![],
                to: (n, 0),
            })
            .collect();
        peers[0].script.insert(3, many);
        run(&mut peers, 200);
        let ran: usize = peers[1].applied.iter().map(|(_, c)| c.len()).sum();
        // One empty packet is always waiting, so a queue of 26 takes 25 commands.
        assert_eq!(ran, QUEUE_LIMIT - 1);
        assert_eq!(peers[0].applied, peers[1].applied);
    }

    #[test]
    fn a_game_in_step_checks_every_turn_and_drops_nobody() {
        let time = Time::default();
        let mut peers = game(2, DIRECT_IP_ROW, 8, &time);
        for t in (10..600).step_by(37) {
            peers[0].script.entry(t).or_default().push(stop(0));
            peers[1].script.entry(t + 3).or_default().push(stop(1));
        }
        run(&mut peers, 700);
        assert_eq!(peers[0].state, peers[1].state);
        assert_eq!(peers[0].s.players(), [0, 1]);
        assert!(peers[0].s.take_leave_notices().is_empty() && peers[1].s.take_leave_notices().is_empty());
        assert!(
            peers[0]
                .applied
                .iter()
                .all(|(_, c)| c.iter().all(|c| !matches!(c, Command::Leave { .. })))
        );
    }

    #[test]
    fn a_difference_between_two_turns_drops_both_at_the_second_turn_after_it() {
        // `net/lockstep.md` vector: a flip after cycle 150, between the turns before counters 144 and 156, drops at
        // the turn before 168, on each machine, each dropping the other.
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        run(&mut peers, 151);
        peers[1].state ^= 1;
        run(&mut peers, 200);
        assert_eq!(
            peers[0].applied,
            [(
                168,
                vec![Command::Leave {
                    player: 1,
                    dropped: true
                }]
            )],
            "A dropped B at the turn before 168"
        );
        assert_eq!(
            peers[1].applied,
            [(
                168,
                vec![Command::Leave {
                    player: 0,
                    dropped: true
                }]
            )],
            "B dropped A at the turn before 168"
        );
        let n0 = peers[0].s.take_leave_notices();
        let n1 = peers[1].s.take_leave_notices();
        assert_eq!(
            n0,
            [LeaveNotice {
                slot: 1,
                name: "Player 2".into(),
                dropped: true
            }]
        );
        assert_eq!(n1[0].slot, 0);
        assert!(n1[0].dropped);
        // Both games go on, alone.
        assert_eq!(peers[0].s.players(), [0]);
        assert_eq!(peers[1].s.players(), [1]);
        run(&mut peers, 400);
    }

    #[test]
    fn a_turn_waits_for_a_late_peer_with_the_watch_at_two_seconds_and_the_dialog_at_seven() {
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        run(&mut peers, 30);
        // B stops: it is not stepped while A goes on, as far as A's turns allow.
        peers[1].frozen = true;
        let mut guard = 0;
        while peers[0].advance() {
            guard += 1;
            assert!(guard < 100, "A should stall at its next turn");
        }
        let stuck_at = peers[0].tick;
        assert_eq!(stuck_at % 12, 0, "a stall is at a turn: {stuck_at}");
        assert_eq!(peers[0].s.waiting().map(|w| (w.watch, w.dialog)), Some((false, false)));
        time.advance(Duration::from_millis(1900));
        assert!(!peers[0].advance());
        assert!(!peers[0].s.waiting().unwrap().watch, "not yet at 1.9 s");
        time.advance(Duration::from_millis(200));
        let w = peers[0].s.waiting().unwrap();
        assert!(w.watch && !w.dialog, "the watch at about 2 s, no dialog yet: {w:?}");
        time.advance(Duration::from_millis(5000));
        let w = peers[0].s.waiting().unwrap();
        assert!(w.dialog, "the dialog after 7 s: {w:?}");
        assert_eq!(w.missing, ["Player 2"], "it names the missing player");
        // Nobody is dropped when the turns come back; the dialog closes by itself.
        peers[1].frozen = false;
        run(&mut peers, stuck_at + 50);
        assert!(peers[0].s.waiting().is_none());
        assert_eq!(peers[0].s.players(), [0, 1]);
        assert_eq!(peers[0].state, peers[1].state);
        assert!(
            peers[0].s.take_deadline_reset(),
            "a stall over 200 ms resets the deadline"
        );
        assert!(!peers[0].s.take_deadline_reset(), "once");
    }

    #[test]
    fn a_short_stall_does_not_reset_the_deadline() {
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        run(&mut peers, 30);
        peers[1].frozen = true;
        while peers[0].advance() {}
        time.advance(Duration::from_millis(150));
        peers[1].frozen = false;
        let to = peers[0].tick + 30;
        run(&mut peers, to);
        assert!(!peers[0].s.take_deadline_reset(), "150 ms leaves the deadline alone");
    }

    #[test]
    fn leaving_is_a_command_in_the_turn_and_the_slot_goes() {
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        run(&mut peers, 40);
        let mut b = peers.pop().unwrap();
        // B surrenders: its game ends at once, and the session tells A it left as it goes.
        assert_eq!(
            b.s.immediate(b.tick, Command::Surrender { player: 1 }),
            Some(Command::Surrender { player: 1 })
        );
        drop(b);
        let mut guard = 0;
        while peers[0].tick < 80 {
            if !peers[0].advance() {
                thread::sleep(Duration::from_millis(1));
                guard += 1;
                assert!(guard < 3000);
            }
        }
        let left = [Command::Leave {
            player: 1,
            dropped: false,
        }];
        assert!(
            peers[0].applied.iter().any(|(_, c)| c == &left),
            "{:?}",
            peers[0].applied
        );
        assert_eq!(
            peers[0].s.take_leave_notices(),
            [LeaveNotice {
                slot: 1,
                name: "Player 2".into(),
                dropped: false
            }]
        );
        assert_eq!(peers[0].s.players(), [0]);
        // A plays on alone.
        run(&mut peers, 200);
    }

    #[test]
    fn alt_f2_drops_a_silent_player_on_every_machine_in_one_turn() {
        let time = Time::default();
        let mut peers = game(3, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        run(&mut peers, 40);
        // Slot 2 goes quiet (frozen), and the other two reach the turn it is needed for.
        peers[2].frozen = true;
        let mut idle = 0;
        while idle < 300 {
            let a = peers[0].advance();
            let b = peers[1].advance();
            if a || b {
                idle = 0;
            } else {
                idle += 1;
                thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(peers[0].s.waiting().is_some() && peers[1].s.waiting().is_some());
        // Alt+F2 before the dialog is up does nothing.
        peers[0].s.drop_missing();
        thread::sleep(Duration::from_millis(30));
        assert!(!peers[0].advance());
        time.advance(Duration::from_millis(7100));
        // The joiner asks the host; the host decides for everyone.
        peers[1].s.drop_missing();
        let mut guard = 0;
        while peers[0].tick < 90 || peers[1].tick < 90 {
            let a = peers[0].advance();
            let b = peers[1].advance();
            if !(a || b) {
                thread::sleep(Duration::from_millis(1));
                guard += 1;
                assert!(guard < 3000, "stuck at {} and {}", peers[0].tick, peers[1].tick);
            }
        }
        let drop2 = Command::Leave {
            player: 2,
            dropped: true,
        };
        let at0 = when(&peers[0], &drop2);
        let at1 = when(&peers[1], &drop2);
        assert_eq!(at0.len(), 1, "{:?}", peers[0].applied);
        assert_eq!(at0, at1, "dropped in the same turn on both machines");
        assert_eq!(peers[0].s.players(), [0, 1]);
        assert_eq!(peers[1].s.players(), [0, 1]);
        assert_eq!(
            peers[0].applied, peers[1].applied,
            "the same commands in the same cycles"
        );
        assert!(peers[0].s.take_leave_notices().iter().all(|n| n.dropped && n.slot == 2));
        // The dropped machine is told, and its game ends.
        peers[2].frozen = false;
        let mut guard = 0;
        while peers[2].s.take_exit().is_none() {
            peers[2].advance();
            thread::sleep(Duration::from_millis(1));
            guard += 1;
            assert!(guard < 3000, "the dropped machine was never told");
        }
    }

    #[test]
    fn ctrl_f1_while_waiting_leaves_and_the_others_see_a_drop() {
        let time = Time::default();
        let mut peers = game(2, TRACED_DIRECT_CONNECT_ROW, 8, &time);
        run(&mut peers, 30);
        // B waits for A, which has stopped.
        peers[0].frozen = true;
        while peers[1].advance() {}
        assert!(peers[1].s.waiting().is_some());
        peers[1].s.abort_game();
        assert_eq!(
            peers[1].s.take_exit(),
            Some(Command::Surrender { player: 1 }),
            "B's game ends"
        );
        peers[0].frozen = false;
        // Which turn carries the leave depends on where each machine stopped, which thread timing can move by a turn,
        // so run until it is carried out rather than to a fixed cycle.
        let left = [Command::Leave {
            player: 1,
            dropped: true,
        }];
        let mut guard = 0;
        while !peers[0].applied.iter().any(|(_, c)| c == &left) {
            assert!(peers[0].tick < 200, "no leave by cycle 200: {:?}", peers[0].applied);
            if !peers[0].advance() {
                thread::sleep(Duration::from_millis(1));
                guard += 1;
                assert!(guard < 3000, "A stalled at cycle {}", peers[0].tick);
            }
        }
        assert_eq!(
            peers[0].s.take_leave_notices(),
            [LeaveNotice {
                slot: 1,
                name: "Player 2".into(),
                dropped: true
            }]
        );
    }

    #[test]
    fn a_recording_of_a_network_game_is_the_same_on_both_sides() {
        let time = Time::default();
        let mut peers = game(2, DIRECT_IP_ROW, 8, &time);
        for t in (10..400).step_by(41) {
            peers[0].script.entry(t).or_default().push(stop(0));
            peers[1].script.entry(t + 2).or_default().push(stop(1));
        }
        run(&mut peers, 450);
        assert_eq!(peers[0].applied, peers[1].applied);
        assert!(peers[0].applied.len() > 8);
    }
}
