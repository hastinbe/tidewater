//! The lobby's network side: a room that joiners enter, change their race, ready box and chat line in, and that the
//! host fills with Computer rows, closes, kicks from and starts (`docs/design/network.md`, "The lobby";
//! `docs/specs/ui/game-setup-lobby.md`).
//!
//! Unlike the blocking handshake of `tcp::host_game`, a room never blocks: the screens poll it each frame. The host's
//! [`HostRoom`] owns the listening socket and the joiners' connections and holds the one true [`LobbyView`]; every
//! change goes out to every joiner as a whole new view (it is a few hundred bytes). A joiner's [`GuestRoom`] holds the
//! last view it was sent and can change only its own row's race, ready box and chat line.
//!
//! **Handshake.** A connection is accepted and a short-lived thread reads its [`Msg::Hello`]. The host checks the
//! [`Identity`] (a refusal says what differs), the name (no duplicates) and that a row is free, reserves the row and
//! answers [`Msg::Welcome`]; the joiner checks it has the map and answers [`Msg::Ready`]; the host then fills the row
//! and sends the joiner the lobby. A joiner that vanishes in between frees its row again.
//!
//! **Starting.** [`HostRoom::start`] packs the filled rows into slots (Available and Closed rows are skipped, so a
//! slot never lands on a map slot nobody fills), settles Random races from the game's seed, and sends each joiner
//! [`Msg::Go`] with its slot and the game. The connections then become the turn engine's wires (`tcp::HostWire`,
//! `tcp::ClientWire`), so no message already queued is lost.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use anyhow::{Result, bail};
use tidewater_sim::setup::{Custom, Game, Lobby, Setup, race};

use super::lobby::NetStart;
use super::proto::{GameConfig, Identity, LobbyView, Msg, RowState, RowView, read_frame, write_frame};
use super::tcp::{self, ClientWire, Event, HANDSHAKE_TIMEOUT, HostWire, Link};

/// The lobby's rows.
pub const ROWS: usize = 8;
/// The longest line in a row's chat box.
pub const SAY_MAX: usize = 70;

/// The words of the refusals and notices the host sends (network table entries 6, 7, 8, 21 and 89).
#[derive(Clone, Debug)]
pub struct Texts {
    pub full: String,
    pub terminated: String,
    pub kicked: String,
    pub too_many: String,
    pub duplicate_name: String,
}

impl Texts {
    /// The words from the network table (`entry`), the spec's where the table gives none.
    pub fn from(entry: &dyn Fn(u16) -> String) -> Texts {
        let d = Texts::default();
        let get = |n: u16, fallback: String| {
            let s = entry(n);
            if s.is_empty() { fallback } else { s }
        };
        Texts {
            full: get(6, d.full),
            terminated: get(7, d.terminated),
            kicked: get(8, d.kicked),
            too_many: get(21, d.too_many),
            duplicate_name: get(89, d.duplicate_name),
        }
    }
}

impl Default for Texts {
    fn default() -> Self {
        Texts {
            full: "The requested game is full.".into(),
            terminated: "Game was terminated by the game creator.".into(),
            kicked: "You were kicked out by the game creator.".into(),
            too_many: "Too many players to load map".into(),
            duplicate_name: "Duplicate name - pick another".into(),
        }
    }
}

/// An empty lobby: the host in row 0, every other row Available.
pub fn new_view(enhanced: bool, host_name: &str, host_race: u8) -> LobbyView {
    let mut rows: Vec<RowView> = (0..ROWS).map(|_| RowView::new(RowState::Available)).collect();
    rows[0] = RowView {
        state: RowState::Human,
        name: host_name.to_string(),
        race: host_race,
        ready: true,
        say: String::new(),
    };
    LobbyView {
        enhanced,
        map_name: String::new(),
        map_hash: 0,
        scenario: Vec::new(),
        map_players: 0,
        custom: Custom::default(),
        // The slider's middle (`ui/game-setup-lobby.md`, "Options").
        speed: 4,
        cheats_allowed: true,
        row: super::lockstep::DIRECT_IP_ROW,
        rows,
    }
}

impl LobbyView {
    /// The rows that play, in order: Human and Computer rows. A game's slot `n` is the `n`th of them.
    pub fn participants(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| matches!(self.rows[i].state, RowState::Human | RowState::Computer))
            .collect()
    }

    pub fn humans(&self) -> usize {
        self.rows.iter().filter(|r| r.state == RowState::Human).count()
    }

    /// The game this lobby makes, with the Random races settled from `seed`, and each slot's row. `local` is the
    /// receiver's slot, set by the caller.
    pub fn to_config(&self, seed: u32) -> (GameConfig, Vec<usize>) {
        let rows = self.participants();
        let mut humans = Vec::new();
        let mut computers = Vec::new();
        let mut names = Vec::new();
        let mut randoms = 0u32;
        for (slot, &row) in rows.iter().enumerate() {
            let r = &self.rows[row];
            match r.state {
                RowState::Human => {
                    let race = if r.race == race::NEUTRAL {
                        randoms += 1;
                        random_race(seed, randoms)
                    } else {
                        r.race
                    };
                    humans.push((slot as u8, race));
                    names.push(r.name.clone());
                }
                _ => {
                    computers.push(slot as u8);
                    names.push(String::new());
                }
            }
        }
        let custom = Custom {
            race: None,
            opponents: None,
            ..self.custom
        };
        let config = GameConfig {
            map_name: self.map_name.clone(),
            map_hash: self.map_hash,
            seed,
            setup: Setup {
                game: Game::Custom(custom),
                enhanced: self.enhanced,
            },
            lobby: Lobby {
                humans,
                local: 0,
                computers,
                fixed_order: self.custom.fixed_starts,
            },
            speed: self.speed,
            row: self.row,
            extras: tidewater_sim::Extras::default(),
            names,
        };
        (config, rows)
    }
}

/// A Random race, decided once from the game's seed so every machine plays the same races (`ui/game-setup-lobby.md`,
/// "Races and colours": the original flips a coin per machine and shares the result; ours is the host's one draw).
fn random_race(seed: u32, nth: u32) -> u8 {
    let h = seed.wrapping_mul(0x9E37_79B1).rotate_left(5 * nth) ^ nth.wrapping_mul(0x85EB_CA6B);
    if (h >> 7) & 1 == 0 { race::HUMAN } else { race::ORC }
}

/// What the host's drop-down on a row offers (Kick Out is Available on a row that holds a human).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowChoice {
    Available,
    Computer,
    Closed,
    KickOut,
}

/// Why the host cannot start yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    /// No scenario is chosen.
    NoMap,
    /// Nobody has joined. Gap-fill for `ui/game-setup-lobby.md` trace request LB1: Tidewater starts a network game
    /// only with another human in it.
    Alone,
    /// More rows are filled than the map has slots (network entry 21).
    TooMany,
    /// Classic mode: a joiner has not ticked its Ready box.
    NotReady,
}

// ---------------------------------------------------------------------------------------------------------------
// The host

enum Shake {
    Hello {
        id: usize,
        identity: Box<Identity>,
        name: String,
        stream: TcpStream,
    },
    Ready {
        id: usize,
        stream: TcpStream,
    },
    Gone {
        id: usize,
    },
}

struct Member {
    id: usize,
    row: usize,
    link: Link,
}

struct Reserved {
    id: usize,
    row: usize,
    name: String,
}

/// The game being set up on the host's machine.
pub struct HostRoom {
    listener: TcpListener,
    identity: Identity,
    texts: Texts,
    view: LobbyView,
    shakes: Receiver<Shake>,
    shake_tx: Sender<Shake>,
    events_tx: Sender<Event>,
    events: Receiver<Event>,
    members: Vec<Member>,
    reserved: Vec<Reserved>,
    next_id: usize,
}

impl HostRoom {
    /// Opens a room on `listener`, which this takes over. `identity` is what joiners must match.
    pub fn new(listener: TcpListener, identity: Identity, texts: Texts, view: LobbyView) -> io::Result<HostRoom> {
        listener.set_nonblocking(true)?;
        let (shake_tx, shakes) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        Ok(HostRoom {
            listener,
            identity,
            texts,
            view,
            shakes,
            shake_tx,
            events_tx,
            events,
            members: Vec::new(),
            reserved: Vec::new(),
            next_id: 0,
        })
    }

    pub fn view(&self) -> &LobbyView {
        &self.view
    }

    /// The port the room listens on.
    pub fn port(&self) -> u16 {
        self.listener.local_addr().map_or(0, |a| a.port())
    }

    fn broadcast(&self) {
        let msg = Msg::Lobby(Box::new(self.view.clone()));
        for m in &self.members {
            m.link.send(&msg);
        }
    }

    /// Sends the lobby to every joiner after the host changed it, and clears the Classic Ready boxes when `clears`
    /// (a changed map or option).
    fn changed(&mut self, clears: bool) {
        if clears && !self.view.enhanced {
            for r in self.view.rows.iter_mut().skip(1).filter(|r| r.state == RowState::Human) {
                r.ready = false;
            }
        }
        self.broadcast();
    }

    /// Picks the scenario. The Classic Ready boxes clear.
    pub fn set_map(&mut self, name: &str, hash: u64, scenario: Vec<String>, players: u8) {
        let v = &mut self.view;
        if v.map_name == name && v.map_hash == hash && v.scenario == scenario && v.map_players == players {
            return;
        }
        v.map_name = name.to_string();
        v.map_hash = hash;
        v.scenario = scenario;
        v.map_players = players;
        self.changed(true);
    }

    /// Sets the options. The Classic Ready boxes clear when something changed.
    pub fn set_options(&mut self, custom: Custom, speed: u8, cheats_allowed: bool) {
        let v = &mut self.view;
        if v.custom == custom && v.speed == speed && v.cheats_allowed == cheats_allowed {
            return;
        }
        v.custom = custom;
        v.speed = speed.min(8);
        v.cheats_allowed = cheats_allowed;
        self.changed(true);
    }

    /// The host's own race.
    pub fn set_host_race(&mut self, race: u8) {
        if race <= tidewater_sim::setup::race::NEUTRAL && self.view.rows[0].race != race {
            self.view.rows[0].race = race;
            self.changed(false);
        }
    }

    /// The line in the host's chat box.
    pub fn say(&mut self, text: &str) {
        self.view.rows[0].say = clean_say(text);
        self.changed(false);
    }

    /// The host's drop-down on a row 1 to 7. Anything but Available on a row that holds a human kicks that human.
    pub fn set_row(&mut self, row: usize, choice: RowChoice) {
        if row == 0 || row >= ROWS {
            return;
        }
        if self.view.rows[row].state == RowState::Human {
            self.kick(row);
        }
        let state = match choice {
            RowChoice::Available | RowChoice::KickOut => RowState::Available,
            RowChoice::Computer => RowState::Computer,
            RowChoice::Closed => RowState::Closed,
        };
        // A joiner being welcomed into the row loses it.
        self.reserved.retain(|r| r.row != row);
        self.view.rows[row] = RowView::new(state);
        self.changed(false);
    }

    fn kick(&mut self, row: usize) {
        if let Some(i) = self.members.iter().position(|m| m.row == row) {
            let m = self.members.remove(i);
            m.link.send(&Msg::Refuse {
                reason: self.texts.kicked.clone(),
            });
        }
    }

    /// Whether the game can start, and if not, why.
    pub fn can_start(&self) -> Result<(), StartError> {
        let v = &self.view;
        if v.map_name.is_empty() {
            return Err(StartError::NoMap);
        }
        if v.humans() < 2 {
            return Err(StartError::Alone);
        }
        if v.participants().len() > usize::from(v.map_players) {
            return Err(StartError::TooMany);
        }
        if !v.enhanced && v.rows.iter().skip(1).any(|r| r.state == RowState::Human && !r.ready) {
            return Err(StartError::NotReady);
        }
        Ok(())
    }

    /// Everything that happened since the last call: connections accepted and answered, joiners' changes, joiners
    /// gone. Returns whether the lobby changed (the screen draws it again).
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        // New connections.
        while let Ok((stream, _)) = self.listener.accept() {
            let id = self.next_id;
            self.next_id += 1;
            let tx = self.shake_tx.clone();
            let _ = thread::Builder::new().name(format!("net-hello-{id}")).spawn(move || {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
                let _ = stream.set_nodelay(true);
                let mut stream = stream;
                match read_frame(&mut stream) {
                    Ok(Msg::Hello { identity, name }) => {
                        let _ = tx.send(Shake::Hello {
                            id,
                            identity: Box::new(identity),
                            name,
                            stream,
                        });
                    }
                    _ => {
                        let _ = tx.send(Shake::Gone { id });
                    }
                }
            });
        }
        while let Ok(s) = self.shakes.try_recv() {
            changed |= self.shake(s);
        }
        while let Ok(ev) = self.events.try_recv() {
            changed |= self.event(ev);
        }
        changed
    }

    fn name_taken(&self, name: &str) -> bool {
        let same = |n: &str| n.eq_ignore_ascii_case(name);
        self.view
            .rows
            .iter()
            .any(|r| r.state == RowState::Human && same(&r.name))
            || self.reserved.iter().any(|r| same(&r.name))
    }

    fn shake(&mut self, s: Shake) -> bool {
        match s {
            Shake::Hello {
                id,
                identity,
                name,
                mut stream,
            } => {
                let refuse = |stream: &mut TcpStream, reason: String| {
                    let _ = write_frame(stream, &Msg::Refuse { reason });
                };
                if let Some(why) = self.identity.mismatch(&identity) {
                    refuse(&mut stream, why);
                    return false;
                }
                let free = (1..ROWS).find(|&i| {
                    self.view.rows[i].state == RowState::Available && !self.reserved.iter().any(|r| r.row == i)
                });
                let Some(row) = free else {
                    refuse(&mut stream, self.texts.full.clone());
                    return false;
                };
                let name = tcp::clean_name(&name).unwrap_or_else(|| format!("Player {}", row + 1));
                if self.name_taken(&name) {
                    refuse(&mut stream, self.texts.duplicate_name.clone());
                    return false;
                }
                let (mut config, _) = self.view.to_config(0);
                config.lobby.local = row as u8;
                let welcome = Msg::Welcome {
                    slot: row as u8,
                    config: Box::new(config),
                };
                if write_frame(&mut stream, &welcome).is_err() {
                    return false;
                }
                self.reserved.push(Reserved { id, row, name });
                let tx = self.shake_tx.clone();
                let _ = thread::Builder::new().name(format!("net-ready-{id}")).spawn(move || {
                    let mut stream = stream;
                    match read_frame(&mut stream) {
                        Ok(Msg::Ready) => {
                            let _ = tx.send(Shake::Ready { id, stream });
                        }
                        _ => {
                            let _ = tx.send(Shake::Gone { id });
                        }
                    }
                });
                false
            }
            Shake::Ready { id, stream } => {
                let Some(i) = self.reserved.iter().position(|r| r.id == id) else {
                    return false;
                };
                let r = self.reserved.remove(i);
                let Ok(link) = Link::spawn(stream, id, self.events_tx.clone()) else {
                    return false;
                };
                self.view.rows[r.row] = RowView {
                    state: RowState::Human,
                    name: r.name,
                    race: race::NEUTRAL,
                    // Enhanced rules have no Ready box: a joiner is ready the moment it joins.
                    ready: self.view.enhanced,
                    say: String::new(),
                };
                self.members.push(Member { id, row: r.row, link });
                self.broadcast();
                true
            }
            Shake::Gone { id } => {
                self.reserved.retain(|r| r.id != id);
                false
            }
        }
    }

    fn event(&mut self, ev: Event) -> bool {
        match ev {
            Event::Closed(id) => {
                let Some(i) = self.members.iter().position(|m| m.id == id) else {
                    return false;
                };
                let m = self.members.remove(i);
                self.view.rows[m.row] = RowView::new(RowState::Available);
                self.broadcast();
                true
            }
            Event::Msg(id, msg) => {
                let Some(row) = self.members.iter().find(|m| m.id == id).map(|m| m.row) else {
                    return false;
                };
                let r = &mut self.view.rows[row];
                match msg {
                    Msg::SetRace { race } if race <= tidewater_sim::setup::race::NEUTRAL => r.race = race,
                    Msg::SetReady { ready } if !self.view.enhanced => r.ready = ready,
                    Msg::Say { text } => r.say = clean_say(&text),
                    _ => return false,
                }
                self.broadcast();
                true
            }
        }
    }

    /// Closes the room: every joiner is told the game was terminated.
    pub fn cancel(self) {
        for m in &self.members {
            m.link.send(&Msg::Refuse {
                reason: self.texts.terminated.clone(),
            });
        }
    }

    /// Starts the game: every joiner is sent its slot and the game, and the connections become the turn engine's
    /// wire. `seed` is the game's seed (the host's clock).
    pub fn start(mut self, seed: u32) -> Result<NetStart> {
        if let Err(e) = self.can_start() {
            bail!("the game can't start yet: {e:?}");
        }
        // A joiner that left in this very frame is noticed before the game is built.
        self.poll();
        self.can_start()
            .map_err(|e| anyhow::anyhow!("the game can't start yet: {e:?}"))?;
        let (config, rows) = self.view.to_config(seed);
        let mut wires = Vec::new();
        for m in self.members {
            let Some(slot) = rows.iter().position(|&r| r == m.row) else {
                continue;
            };
            let mut theirs = config.clone();
            theirs.lobby.local = slot as u8;
            m.link.send(&Msg::Go {
                slot: slot as u8,
                config: Box::new(theirs),
            });
            wires.push((slot as u8, m.id, m.link));
        }
        let names = config.names.clone();
        let wire = HostWire::from_parts(wires, self.events, names.clone());
        Ok(NetStart {
            config,
            me: 0,
            wire: Box::new(wire),
            names,
        })
    }
}

/// A chat line as it is kept: printable text, [`SAY_MAX`] characters.
fn clean_say(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(SAY_MAX).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// The joiner

/// A joiner that has been welcomed: it knows the game and its row, and must say whether it can play it.
#[derive(Debug)]
pub struct Handshake {
    stream: TcpStream,
    /// The joiner's row.
    pub slot: u8,
    /// The game as the host showed it: the map to check for.
    pub config: GameConfig,
}

/// Connects to the game at `addr` and says hello. Blocks until the host welcomes or refuses; run it on a thread.
pub fn handshake(addr: &str, identity: Identity, name: &str) -> Result<Handshake> {
    let mut stream = tcp::connect(addr)?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_nodelay(true)?;
    write_frame(
        &mut stream,
        &Msg::Hello {
            identity,
            name: name.to_string(),
        },
    )?;
    match read_frame(&mut stream) {
        Ok(Msg::Welcome { slot, config }) => Ok(Handshake {
            stream,
            slot,
            config: *config,
        }),
        Ok(Msg::Refuse { reason }) => bail!("{reason}"),
        Ok(_) => bail!("the host sent something unexpected: is that a Tidewater game?"),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            bail!("the host's reply didn't make sense: it may be a different version of the program")
        }
        Err(e) => bail!("the host went away before showing the game: {e}"),
    }
}

impl Handshake {
    /// This machine can play the game: say so and enter the room.
    pub fn accept(mut self) -> io::Result<GuestRoom> {
        write_frame(&mut self.stream, &Msg::Ready)?;
        self.stream.set_read_timeout(None)?;
        let (tx, rx) = mpsc::channel();
        let link = Link::spawn(self.stream, 0, tx)?;
        Ok(GuestRoom {
            parts: Some((link, rx)),
            slot: self.slot,
            view: None,
        })
    }

    /// This machine can't play the game: tell the host why and leave.
    pub fn decline(mut self, reason: &str) {
        let _ = write_frame(
            &mut self.stream,
            &Msg::Refuse {
                reason: reason.to_string(),
            },
        );
    }
}

/// What a joiner's room tells its screen.
pub enum GuestEvent {
    /// The lobby changed (the first one is the lobby as the joiner enters).
    View,
    /// The host started the game.
    Go(Box<NetStart>),
    /// The room is over: kicked out, terminated, or the connection ended. The words are for the player.
    Ended(String),
}

/// The joiner's end of a room.
pub struct GuestRoom {
    parts: Option<(Link, Receiver<Event>)>,
    /// The joiner's row.
    pub slot: u8,
    /// The lobby, once the host has sent it.
    pub view: Option<LobbyView>,
}

impl GuestRoom {
    fn send(&self, msg: Msg) {
        if let Some((link, _)) = &self.parts {
            link.send(&msg);
        }
    }

    pub fn set_race(&mut self, race: u8) {
        self.send(Msg::SetRace { race });
    }

    pub fn set_ready(&mut self, ready: bool) {
        self.send(Msg::SetReady { ready });
    }

    pub fn say(&mut self, text: &str) {
        self.send(Msg::Say { text: clean_say(text) });
    }

    /// What arrived since the last call. After [`GuestEvent::Go`] or [`GuestEvent::Ended`] the room does nothing more.
    pub fn poll(&mut self) -> Vec<GuestEvent> {
        let mut out = Vec::new();
        while let Some((_, events)) = &self.parts {
            let Ok(ev) = events.try_recv() else { break };
            match ev {
                Event::Closed(_) => {
                    self.parts = None;
                    out.push(GuestEvent::Ended("Connection terminated.".into()));
                }
                Event::Msg(_, Msg::Lobby(view)) => {
                    self.view = Some(*view);
                    out.push(GuestEvent::View);
                }
                Event::Msg(_, Msg::Refuse { reason }) => {
                    self.parts = None;
                    out.push(GuestEvent::Ended(reason));
                }
                Event::Msg(_, Msg::Go { slot, config }) => {
                    // The messages after the Go are the game's: they stay in the channel for the wire.
                    let Some((link, events)) = self.parts.take() else { break };
                    let config = *config;
                    let host = config.lobby.humans.first().map_or(0, |h| h.0);
                    let names = config.names.clone();
                    out.push(GuestEvent::Go(Box::new(NetStart {
                        config,
                        me: slot,
                        wire: Box::new(ClientWire::from_parts(link, events, host)),
                        names,
                    })));
                }
                Event::Msg(..) => {}
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn ident() -> Identity {
        Identity::ours(1)
    }

    fn host(enhanced: bool) -> HostRoom {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut view = new_view(enhanced, "Host", race::HUMAN);
        view.map_name = "Maps\\x.pud".into();
        view.map_hash = 9;
        view.map_players = 4;
        HostRoom::new(listener, ident(), Texts::default(), view).unwrap()
    }

    fn join(room: &mut HostRoom, name: &str) -> GuestRoom {
        let addr = format!("127.0.0.1:{}", room.port());
        let name = name.to_string();
        let t = thread::spawn(move || handshake(&addr, ident(), &name));
        wait(room, |_| t.is_finished());
        let hs = t.join().unwrap().unwrap();
        let mut g = hs.accept().unwrap();
        wait_guest(room, &mut g, |g| g.view.is_some());
        g
    }

    fn wait(room: &mut HostRoom, mut done: impl FnMut(&mut HostRoom) -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !done(room) && Instant::now() < end {
            room.poll();
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_guest(room: &mut HostRoom, g: &mut GuestRoom, mut done: impl FnMut(&GuestRoom) -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !done(g) && Instant::now() < end {
            room.poll();
            g.poll();
            thread::sleep(Duration::from_millis(5));
        }
        assert!(done(g), "timed out");
    }

    #[test]
    fn a_joiner_takes_the_first_free_row_and_sees_the_lobby() {
        let mut room = host(true);
        let g = join(&mut room, "Ann");
        assert_eq!(g.slot, 1);
        let v = g.view.as_ref().unwrap();
        assert_eq!(v.rows[1].name, "Ann");
        assert!(v.rows[1].ready, "Enhanced rules: ready on joining");
        assert_eq!(v.rows[2].state, RowState::Available);
        assert_eq!(room.view().humans(), 2);
    }

    #[test]
    fn a_joiner_with_another_identity_is_refused_and_says_why() {
        let mut room = host(true);
        let addr = format!("127.0.0.1:{}", room.port());
        let t = thread::spawn(move || handshake(&addr, Identity::ours(2), "Ann"));
        wait(&mut room, |_| t.is_finished());
        let err = t.join().unwrap().err().unwrap();
        assert!(format!("{err:#}").contains("game data"), "{err:#}");
        assert_eq!(room.view().humans(), 1);
    }

    #[test]
    fn a_full_room_and_a_duplicate_name_are_refused() {
        let mut room = host(true);
        for i in 1..ROWS {
            room.set_row(i, RowChoice::Closed);
        }
        let addr = format!("127.0.0.1:{}", room.port());
        let a = addr.clone();
        let t = thread::spawn(move || handshake(&a, ident(), "Ann"));
        wait(&mut room, |_| t.is_finished());
        assert!(format!("{:#}", t.join().unwrap().err().unwrap()).contains("full"));
        room.set_row(1, RowChoice::Available);
        let t = thread::spawn(move || handshake(&addr, ident(), "host"));
        wait(&mut room, |_| t.is_finished());
        assert!(format!("{:#}", t.join().unwrap().err().unwrap()).contains("Duplicate"));
    }

    #[test]
    fn rows_race_chat_and_kick_out_reach_the_other_side() {
        let mut room = host(true);
        let mut g = join(&mut room, "Ann");
        g.set_race(race::ORC);
        g.say("hello");
        wait_guest(&mut room, &mut g, |g| {
            g.view
                .as_ref()
                .is_some_and(|v| v.rows[1].race == race::ORC && v.rows[1].say == "hello")
        });
        room.set_row(2, RowChoice::Computer);
        room.say("hi back");
        wait_guest(&mut room, &mut g, |g| {
            g.view
                .as_ref()
                .is_some_and(|v| v.rows[2].state == RowState::Computer && v.rows[0].say == "hi back")
        });
        room.set_row(1, RowChoice::KickOut);
        let end = Instant::now() + Duration::from_secs(10);
        let mut ended = None;
        while ended.is_none() && Instant::now() < end {
            room.poll();
            for e in g.poll() {
                if let GuestEvent::Ended(r) = e {
                    ended = Some(r);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(ended.unwrap(), Texts::default().kicked);
        assert_eq!(room.view().rows[1].state, RowState::Available);
    }

    #[test]
    fn a_joiner_who_leaves_frees_its_row_and_cancel_terminates_the_game() {
        let mut room = host(true);
        let g = join(&mut room, "Ann");
        drop(g);
        wait(&mut room, |r| r.view().humans() == 1);
        assert_eq!(room.view().rows[1].state, RowState::Available);
        let mut g = join(&mut room, "Bob");
        room.cancel();
        let end = Instant::now() + Duration::from_secs(10);
        let mut ended = None;
        while ended.is_none() && Instant::now() < end {
            for e in g.poll() {
                if let GuestEvent::Ended(r) = e {
                    ended = Some(r);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(ended.unwrap(), Texts::default().terminated);
    }

    #[test]
    fn start_needs_a_map_a_joiner_room_and_in_classic_mode_ready_boxes() {
        let mut room = host(false);
        assert_eq!(room.can_start(), Err(StartError::Alone));
        let mut g = join(&mut room, "Ann");
        assert_eq!(
            room.can_start(),
            Err(StartError::NotReady),
            "Classic: the joiner must tick Ready"
        );
        g.set_ready(true);
        wait_guest(&mut room, &mut g, |g| g.view.as_ref().is_some_and(|v| v.rows[1].ready));
        assert_eq!(room.can_start(), Ok(()));
        // A changed option clears the Ready boxes again.
        let custom = Custom {
            one_worker: true,
            ..room.view().custom
        };
        room.set_options(custom, 4, true);
        assert_eq!(room.can_start(), Err(StartError::NotReady));
        // More rows than the map has slots.
        let mut room = host(true);
        let _g = join(&mut room, "Ann");
        room.set_map("Maps\\x.pud", 9, vec![], 2);
        room.set_row(2, RowChoice::Computer);
        assert_eq!(room.can_start(), Err(StartError::TooMany));
        room.set_map("", 0, vec![], 0);
        assert_eq!(room.can_start(), Err(StartError::NoMap));
    }

    #[test]
    fn start_packs_the_rows_into_slots_and_both_sides_get_the_same_game() {
        let mut room = host(true);
        room.set_host_race(race::ORC);
        room.set_row(1, RowChoice::Closed);
        room.set_row(2, RowChoice::Computer);
        room.set_row(1, RowChoice::Available);
        let mut g = join(&mut room, "Ann");
        assert_eq!(g.slot, 1);
        room.set_row(3, RowChoice::Computer);
        room.poll();
        let mut got = None;
        let start = room.start(1234).unwrap();
        let end = Instant::now() + Duration::from_secs(10);
        while got.is_none() && Instant::now() < end {
            for e in g.poll() {
                if let GuestEvent::Go(n) = e {
                    got = Some(n);
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        let theirs = got.expect("the joiner is sent the game");
        assert_eq!(start.me, 0);
        assert_eq!(theirs.me, 1);
        let mine = start.config.clone();
        assert_eq!(mine.seed, 1234);
        assert_eq!(mine.lobby.humans.len(), 2);
        assert_eq!(mine.lobby.humans[0], (0, race::ORC));
        assert!(
            matches!(mine.lobby.humans[1].1, race::HUMAN | race::ORC),
            "Random is settled"
        );
        assert_eq!(
            mine.lobby.computers,
            [2, 3],
            "rows 2 and 3 pack into slots 2 and 3 after humans in 0 and 1"
        );
        assert_eq!(theirs.config.lobby.humans, mine.lobby.humans);
        assert_eq!(theirs.config.lobby.local, 1);
        assert_eq!(theirs.names, ["Host", "Ann", "", ""]);
        // The wires carry a turn both ways after the start.
        let (mut a, mut b) = (start.wire, theirs.wire);
        a.send(Msg::Turn {
            slot: 0,
            seq: 1,
            cmd: None,
        });
        let end = Instant::now() + Duration::from_secs(10);
        let mut heard = false;
        while !heard && Instant::now() < end {
            heard = b
                .poll()
                .iter()
                .any(|i| matches!(i, crate::net::tcp::Inbound::Msg { from: 0, .. }));
            thread::sleep(Duration::from_millis(5));
        }
        assert!(heard);
    }

    #[test]
    fn random_races_are_settled_from_the_seed_and_stay_the_same_for_a_seed() {
        let mut v = new_view(true, "Host", race::NEUTRAL);
        v.rows[1] = RowView {
            state: RowState::Human,
            name: "A".into(),
            race: race::NEUTRAL,
            ready: true,
            say: String::new(),
        };
        let (a, _) = v.to_config(7);
        let (b, _) = v.to_config(7);
        assert_eq!(a.lobby.humans, b.lobby.humans);
        let mut seen = std::collections::HashSet::new();
        for seed in 0..64 {
            let (c, _) = v.to_config(seed);
            for h in &c.lobby.humans {
                assert!(matches!(h.1, race::HUMAN | race::ORC));
                seen.insert(h.1);
            }
        }
        assert_eq!(seen.len(), 2, "both races come up over many seeds");
    }
}
