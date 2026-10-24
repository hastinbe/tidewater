//! Framed TCP on standard threads: connections, the handshake that fills the game's slots, and the wire the turn engine
//! talks through (`docs/design/network.md`, "Transport").
//!
//! **Topology.** One host, up to seven joiners, each joiner connected to the host only. The host relays: a packet from
//! one joiner goes to every other joiner, stamped with the slot the host knows that connection by. Relaying keeps the
//! order every machine sees one sender's messages in (TCP's, per connection), which is what lets the host decide a drop
//! for everyone (`Msg::Drop`, `docs/specs/net/leave-and-drop.md`, "Rules for Tidewater"). It is not a game server: no
//! machine but its own decides anything about the game, and every machine runs the whole simulation.
//!
//! **Threads.** Each connection has a reader thread, which turns frames into events on a channel, and a writer thread,
//! which drains a queue onto the socket, so the game thread never waits on the network. The handshake before a game
//! blocks, on purpose: the lobby is on the command line until the lobby screens exist.

use std::io;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::proto::{
    DEFAULT_PORT, GameConfig, HelloRead, Identity, MAX_OBSERVERS, Msg, OBSERVER_BASE, read_frame, read_hello,
    write_frame,
};

/// How long a joiner has, after connecting, to say Hello and to answer a Welcome.
pub(super) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a joiner waits for the connection itself.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

impl Inbound {
    #[cfg(test)]
    fn into_msg(self) -> Msg {
        match self {
            Inbound::Msg { msg, .. } => msg,
            Inbound::Closed { .. } => panic!("not a message"),
        }
    }
}

/// What the turn engine hears from the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inbound {
    /// A message, with the slot of the machine it came from (the host stamps this; a joiner cannot choose it).
    Msg { from: u8, msg: Msg },
    /// The connection to `slot` ended, cleanly or not.
    Closed { slot: u8 },
}

/// The turn engine's end of the network: whatever carries messages between the machines.
pub trait Wire: Send {
    /// Sends `msg` to every other machine.
    fn send(&mut self, msg: Msg);
    /// Everything that has arrived since the last call, in arrival order. Never blocks.
    fn poll(&mut self) -> Vec<Inbound>;
    /// The host passes a message it accepted from the machine in `from` on to every other machine. Only the host's
    /// wire does anything. The engine calls it, rather than the wire relaying by itself, so that what the host relays
    /// is exactly what it counted (`Msg::Drop`'s `at_seq` is that count).
    fn forward(&mut self, _from: u8, _msg: &Msg) {}
}

pub(super) enum Event {
    Msg(usize, Msg),
    Closed(usize),
}

/// One connection's outgoing queue. Dropping it lets the writer thread finish what is queued, then close the sending
/// side of the socket, so a last message (a leave) is still delivered.
pub(super) struct Link {
    tx: Sender<Msg>,
}

impl Link {
    /// Starts the threads for `stream`, whose frames arrive as events tagged `id`.
    pub(super) fn spawn(stream: TcpStream, id: usize, events: Sender<Event>) -> io::Result<Link> {
        stream.set_nodelay(true)?;
        stream.set_read_timeout(None)?;
        let mut reader = stream.try_clone()?;
        thread::Builder::new().name(format!("net-read-{id}")).spawn(move || {
            while let Ok(msg) = read_frame(&mut reader) {
                if events.send(Event::Msg(id, msg)).is_err() {
                    return;
                }
            }
            let _ = events.send(Event::Closed(id));
        })?;
        let (tx, rx) = mpsc::channel::<Msg>();
        let mut writer = stream;
        thread::Builder::new().name(format!("net-write-{id}")).spawn(move || {
            for msg in rx {
                if write_frame(&mut writer, &msg).is_err() {
                    break;
                }
            }
            // Only the sending side: the reader keeps draining what the other machine still says, so closing does not
            // reset the connection over data we have not read and lose the last message.
            let _ = writer.shutdown(Shutdown::Write);
        })?;
        Ok(Link { tx })
    }

    pub(super) fn send(&self, msg: &Msg) {
        // A closed writer is noticed by the reader as a closed connection.
        let _ = self.tx.send(msg.clone());
    }
}

/// The host's wire: every joiner's connection, relaying their packets to one another.
pub struct HostWire {
    links: Vec<HostLink>,
    events: Receiver<Event>,
    /// Every slot's name, indexed by slot, with the joiners' own names in (the host's copy of the Start message).
    pub names: Vec<String>,
}

struct HostLink {
    slot: u8,
    id: usize,
    link: Link,
    open: bool,
}

impl Wire for HostWire {
    fn send(&mut self, msg: Msg) {
        for l in self.links.iter().filter(|l| l.open) {
            l.link.send(&msg);
        }
    }

    fn poll(&mut self) -> Vec<Inbound> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Closed(id) => {
                    if let Some(l) = self.links.iter_mut().find(|l| l.id == id)
                        && std::mem::replace(&mut l.open, false)
                        // An observer leaving is nobody's business but the link's.
                        && l.slot < OBSERVER_BASE
                    {
                        out.push(Inbound::Closed { slot: l.slot });
                    }
                }
                Event::Msg(id, msg) => {
                    let Some(from) = self.links.iter().find(|l| l.id == id && l.open).map(|l| l.slot) else {
                        continue;
                    };
                    // An observer only listens: whatever it says is not heard.
                    if from >= OBSERVER_BASE {
                        continue;
                    }
                    // The slot is the connection's, whatever the message claims.
                    let msg = match msg {
                        Msg::Turn { seq, cmd, .. } => Msg::Turn { slot: from, seq, cmd },
                        Msg::Sync { turn, digest, .. } => Msg::Sync {
                            slot: from,
                            turn,
                            digest,
                        },
                        Msg::Chat { allies, text, .. } => Msg::Chat {
                            slot: from,
                            allies,
                            text,
                        },
                        Msg::Ping { x, y, .. } => Msg::Ping { slot: from, x, y },
                        m @ Msg::DropRequest { .. } => m,
                        // Everything else is the host's to say, or a handshake message out of place.
                        _ => continue,
                    };
                    out.push(Inbound::Msg { from, msg });
                }
            }
        }
        out
    }

    fn forward(&mut self, from: u8, msg: &Msg) {
        self.forward_to_others(from, msg);
    }
}

impl HostWire {
    /// The wire over connections the lobby (`room`) already holds: `joiners` are each connection's slot and event id,
    /// `events` the channel their reader threads send on.
    pub(super) fn from_parts(joiners: Vec<(u8, usize, Link)>, events: Receiver<Event>, names: Vec<String>) -> HostWire {
        HostWire {
            links: joiners
                .into_iter()
                .map(|(slot, id, link)| HostLink {
                    slot,
                    id,
                    link,
                    open: true,
                })
                .collect(),
            events,
            names,
        }
    }

    fn forward_to_others(&mut self, from: u8, msg: &Msg) {
        for l in self.links.iter().filter(|l| l.open && l.slot != from) {
            l.link.send(msg);
        }
    }
}

/// A joiner's wire: the one connection to the host.
pub struct ClientWire {
    link: Link,
    events: Receiver<Event>,
    host: u8,
    closed: bool,
}

impl ClientWire {
    /// The wire over the connection the lobby (`room`) already holds, to the host in slot `host`.
    pub(super) fn from_parts(link: Link, events: Receiver<Event>, host: u8) -> ClientWire {
        ClientWire {
            link,
            events,
            host,
            closed: false,
        }
    }
}

impl Wire for ClientWire {
    fn send(&mut self, msg: Msg) {
        if !self.closed {
            self.link.send(&msg);
        }
    }

    fn poll(&mut self) -> Vec<Inbound> {
        let mut out = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Closed(_) => {
                    if !std::mem::replace(&mut self.closed, true) {
                        out.push(Inbound::Closed { slot: self.host });
                    }
                }
                Event::Msg(_, msg) => match msg {
                    Msg::Turn { slot, .. } | Msg::Sync { slot, .. } => out.push(Inbound::Msg { from: slot, msg }),
                    Msg::Drop { .. } | Msg::Chat { .. } | Msg::Ping { .. } => {
                        out.push(Inbound::Msg { from: self.host, msg })
                    }
                    _ => {}
                },
            }
        }
        out
    }
}

/// What a joiner learns in the handshake.
pub struct Joined {
    pub wire: ClientWire,
    /// The game, with `lobby.local` set to this machine's slot.
    pub config: GameConfig,
    pub slot: u8,
    /// Every slot's name, indexed by slot.
    pub names: Vec<String>,
}

/// Opens the listening socket for a game on `port` (0 asks the system for a free one).
pub fn listen(port: u16) -> Result<TcpListener> {
    TcpListener::bind(("0.0.0.0", port))
        .with_context(|| format!("can't listen on port {port} (is another game already hosted there?)"))
}

/// Fills the game's slots: accepts connections until every slot of `joiner_slots` has a joiner that is compatible and
/// ready, then starts them all. `progress` hears a line for each thing that happens (a joiner arriving, refused or
/// ready), for the host's terminal. Blocks.
///
/// A joiner is refused, with the reason sent to it, when its [`Identity`] differs from `identity` (protocol, build or
/// data), and the slot stays open for the next. A joiner that finds the map missing refuses itself after the Welcome.
pub fn host_game(
    listener: &TcpListener,
    config: GameConfig,
    identity: &Identity,
    joiner_slots: &[u8],
    progress: impl FnMut(&str),
) -> Result<HostWire> {
    host_game_with(listener, config, identity, joiner_slots, 0, progress)
}

/// [`host_game`] for a game with `observers` places for watchers as well (the host's choice, `--lobby observers=N`):
/// the game starts when every player slot and every observer place is filled. An observer gets the slot
/// [`OBSERVER_BASE`] plus its number, the host's slot as the player whose view it starts from, and the same game; a
/// player that joins when the players are all in, or an observer when the places are, is refused with the reason.
pub fn host_game_with(
    listener: &TcpListener,
    mut config: GameConfig,
    identity: &Identity,
    joiner_slots: &[u8],
    observers: usize,
    mut progress: impl FnMut(&str),
) -> Result<HostWire> {
    let observers = observers.min(MAX_OBSERVERS);
    let mut ready: Vec<(u8, TcpStream)> = Vec::new();
    let (mut players, mut watchers) = (0, 0);
    while players < joiner_slots.len() || watchers < observers {
        let (mut stream, peer) = listener.accept().context("accepting a connection")?;
        stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
        stream.set_nodelay(true)?;
        let hello = match read_hello(&mut stream) {
            Ok(HelloRead::Hello {
                identity: theirs,
                name,
                observer,
            }) => (theirs, name, observer),
            Ok(HelloRead::OtherProtocol(p)) => {
                let why = Identity::protocol_reason(identity.protocol, p);
                progress(&format!("{peer}: refused, {why}"));
                let _ = write_frame(&mut stream, &Msg::Refuse { reason: why });
                continue;
            }
            Ok(HelloRead::NotHello) | Err(_) => {
                progress(&format!("{peer}: not a Tidewater joiner, closed"));
                continue;
            }
        };
        if let Some(why) = identity.mismatch(&hello.0) {
            progress(&format!("{peer}: refused, {why}"));
            let _ = write_frame(&mut stream, &Msg::Refuse { reason: why });
            continue;
        }
        let observer = hello.2;
        if observer && watchers >= observers || !observer && players >= joiner_slots.len() {
            let why = if observer {
                "this game has no place for an observer"
            } else {
                "the game is full"
            };
            progress(&format!("{peer}: refused, {why}"));
            let _ = write_frame(&mut stream, &Msg::Refuse { reason: why.into() });
            continue;
        }
        let slot = if observer {
            OBSERVER_BASE + watchers as u8
        } else {
            joiner_slots[players]
        };
        let fallback = if observer {
            format!("Observer {}", watchers + 1)
        } else {
            format!("Player {}", slot + 1)
        };
        let name = clean_name(&hello.1).unwrap_or(fallback);
        if !observer && let Some(n) = config.names.get_mut(slot as usize) {
            *n = name.clone();
        }
        let mut theirs = config.clone();
        // An observer starts from the host's view: the game needs some player's place to look from.
        theirs.lobby.local = if observer {
            config.lobby.humans.first().map_or(0, |h| h.0)
        } else {
            slot
        };
        let welcome = Msg::Welcome {
            slot,
            config: Box::new(theirs),
        };
        if write_frame(&mut stream, &welcome).is_err() {
            progress(&format!("{peer}: gone before the game was shown"));
            continue;
        }
        match read_frame(&mut stream) {
            Ok(Msg::Ready) => {
                if observer {
                    progress(&format!("{name} ({peer}) joined to watch"));
                    watchers += 1;
                } else {
                    progress(&format!("{name} ({peer}) joined as slot {}", slot + 1));
                    players += 1;
                }
                ready.push((slot, stream));
            }
            Ok(Msg::Refuse { reason }) => progress(&format!("{peer}: declined the game: {reason}")),
            _ => progress(&format!("{peer}: gone before saying ready")),
        }
    }
    let start = Msg::Start {
        names: config.names.clone(),
    };
    let (tx, events) = mpsc::channel();
    let mut links = Vec::new();
    for (id, (slot, mut stream)) in ready.into_iter().enumerate() {
        write_frame(&mut stream, &start).context("starting the game")?;
        links.push(HostLink {
            slot,
            id,
            link: Link::spawn(stream, id, tx.clone())?,
            open: true,
        });
    }
    Ok(HostWire {
        links,
        events,
        names: config.names,
    })
}

/// Joins the game at `addr` (`HOST` or `HOST:PORT`, [`DEFAULT_PORT`] when there is no port). `verify` looks at the
/// game the host shows and returns why this machine cannot play it (a missing map), or `Ok`. Blocks until the host
/// starts the game or refuses.
pub fn join_game(
    addr: &str,
    identity: Identity,
    name: &str,
    verify: impl FnOnce(&GameConfig) -> std::result::Result<(), String>,
    progress: impl FnMut(&str),
) -> Result<Joined> {
    join_as(addr, identity, name, false, verify, progress)
}

/// [`join_game`] as an observer when `observer` is set (`--join ADDR --observe`): the host must have a place for one.
/// The slot in the result is then an observer's ([`OBSERVER_BASE`] and up).
pub fn join_as(
    addr: &str,
    identity: Identity,
    name: &str,
    observer: bool,
    verify: impl FnOnce(&GameConfig) -> std::result::Result<(), String>,
    mut progress: impl FnMut(&str),
) -> Result<Joined> {
    let mut stream = connect(addr)?;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    stream.set_nodelay(true)?;
    write_frame(
        &mut stream,
        &Msg::Hello {
            identity,
            name: name.to_string(),
            observer,
        },
    )
    .context("saying hello")?;
    let (slot, config) = match read_frame(&mut stream) {
        Ok(Msg::Welcome { slot, config }) => (slot, *config),
        Ok(Msg::Refuse { reason }) => bail!("the host refused: {reason}"),
        Ok(_) => bail!("the host sent something unexpected: is that a Tidewater game?"),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
            bail!("the host's reply didn't make sense: it may be a different version of the program")
        }
        Err(e) => bail!("the host went away before showing the game: {e}"),
    };
    if let Err(reason) = verify(&config) {
        let _ = write_frame(&mut stream, &Msg::Refuse { reason: reason.clone() });
        bail!("can't play this game: {reason}");
    }
    write_frame(&mut stream, &Msg::Ready).context("saying ready")?;
    if observer {
        progress("joined to watch; waiting for the players");
    } else {
        progress(&format!("joined as slot {}; waiting for the other players", slot + 1));
    }
    // Waiting for the other players has no limit: the host may take its time.
    stream.set_read_timeout(None)?;
    let names = match read_frame(&mut stream) {
        Ok(Msg::Start { names }) => names,
        Ok(Msg::Refuse { reason }) => bail!("the host cancelled the game: {reason}"),
        Ok(_) => bail!("the host sent something unexpected while starting"),
        Err(e) => bail!("the host went away before the game started: {e}"),
    };
    let (tx, events) = mpsc::channel();
    let link = Link::spawn(stream, 0, tx)?;
    let host = config.lobby.humans.first().map_or(0, |h| h.0);
    Ok(Joined {
        wire: ClientWire {
            link,
            events,
            host,
            closed: false,
        },
        config,
        slot,
        names,
    })
}

/// Whether a game's machine answers at `addr`: connects and closes (the menus' connecting box).
pub(crate) fn probe(addr: &str) -> Result<()> {
    connect(addr).map(drop)
}

pub(super) fn connect(addr: &str) -> Result<TcpStream> {
    let candidates: Vec<SocketAddr> = resolve(addr)?;
    let mut last = None;
    for a in candidates {
        match TcpStream::connect_timeout(&a, CONNECT_TIMEOUT) {
            Ok(s) => return Ok(s),
            Err(e) => last = Some((a, e)),
        }
    }
    match last {
        Some((a, e)) => bail!("can't connect to {a}: {e} (is the game hosted, and is the port open on its firewall?)"),
        None => bail!("{addr} has no address"),
    }
}

/// `HOST`, `HOST:PORT`, `1.2.3.4`, `[::1]:PORT` into socket addresses.
fn resolve(addr: &str) -> Result<Vec<SocketAddr>> {
    let with_port = if addr.parse::<SocketAddr>().is_ok() || has_port(addr) {
        addr.to_string()
    } else if addr.contains(':') && !addr.starts_with('[') {
        format!("[{addr}]:{DEFAULT_PORT}")
    } else {
        format!("{addr}:{DEFAULT_PORT}")
    };
    let found: Vec<SocketAddr> = with_port
        .to_socket_addrs()
        .with_context(|| format!("can't find the address {addr}"))?
        .collect();
    Ok(found)
}

fn has_port(addr: &str) -> bool {
    if let Some(rest) = addr.strip_prefix('[') {
        return rest.contains("]:");
    }
    // One colon: HOST:PORT. More are a bare IPv6 address.
    addr.matches(':').count() == 1 && addr.rsplit(':').next().is_some_and(|p| p.parse::<u16>().is_ok())
}

/// A name a joiner asks for, trimmed and limited: printable text, 24 characters at most.
pub(super) fn clean_name(name: &str) -> Option<String> {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(24).collect();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_sim::setup::{Lobby, Setup};

    fn config(humans: usize) -> GameConfig {
        GameConfig {
            map_name: "Maps\\x.pud".into(),
            map_hash: 7,
            seed: 9,
            setup: Setup::default(),
            lobby: Lobby {
                humans: (0..humans as u8).map(|s| (s, s % 2)).collect(),
                local: 0,
                computers: vec![],
                fixed_order: true,
            },
            speed: 8,
            row: 4,
            extras: Default::default(),
            client_extras: Vec::new(),
            names: (0..humans).map(|s| format!("Player {}", s + 1)).collect(),
        }
    }

    fn poll_until(w: &mut dyn Wire, mut done: impl FnMut(&[Inbound]) -> bool) -> Vec<Inbound> {
        let mut seen = Vec::new();
        for _ in 0..2000 {
            seen.extend(w.poll());
            if done(&seen) {
                return seen;
            }
            thread::sleep(Duration::from_millis(2));
        }
        panic!("timed out; saw {seen:?}");
    }

    #[test]
    fn addresses_take_a_port_or_the_default() {
        assert_eq!(resolve("127.0.0.1").unwrap()[0].port(), DEFAULT_PORT);
        assert_eq!(resolve("127.0.0.1:7000").unwrap()[0].port(), 7000);
        assert_eq!(resolve("[::1]:7001").unwrap()[0].port(), 7001);
        assert_eq!(resolve("::1").unwrap()[0].port(), DEFAULT_PORT);
        assert!(resolve("not an address at all :::").is_err());
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  Ann\n"), Some("Ann".into()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"x".repeat(40)).unwrap().len(), 24);
    }

    #[test]
    fn three_machines_handshake_and_the_host_relays_stamped_packets() {
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let cfg = config(3);
        let host = thread::spawn({
            let id = id.clone();
            move || host_game(&listener, cfg, &id, &[1, 2], |_| {}).unwrap()
        });
        let mut joiners = Vec::new();
        for n in 0..2 {
            let id = id.clone();
            // Join one after the other so the slots come out in order.
            let j = thread::spawn(move || {
                join_game(
                    &format!("127.0.0.1:{port}"),
                    id,
                    &format!("Ann{n}"),
                    |c| {
                        assert_eq!(c.map_hash, 7);
                        Ok(())
                    },
                    |_| {},
                )
                .unwrap()
            });
            thread::sleep(Duration::from_millis(100));
            joiners.push(j);
        }
        let mut host = host.join().unwrap();
        let mut a = joiners.remove(0).join().unwrap();
        let mut b = joiners.remove(0).join().unwrap();
        assert_eq!((a.slot, b.slot), (1, 2));
        assert_eq!((a.config.lobby.local, b.config.lobby.local), (1, 2));
        assert_eq!(a.names, ["Player 1", "Ann0", "Ann1"]);

        // A joiner claims to be slot 0; the host stamps the real one, and forwards it to the other joiner when it accepts it.
        a.wire.send(Msg::Turn {
            slot: 0,
            seq: 0,
            cmd: None,
        });
        let at_host = poll_until(&mut host, |s| !s.is_empty());
        assert_eq!(
            at_host[0],
            Inbound::Msg {
                from: 1,
                msg: Msg::Turn {
                    slot: 1,
                    seq: 0,
                    cmd: None
                }
            }
        );
        host.forward(1, &at_host[0].clone().into_msg());
        let at_b = poll_until(&mut b.wire, |s| !s.is_empty());
        assert_eq!(
            at_b[0],
            Inbound::Msg {
                from: 1,
                msg: Msg::Turn {
                    slot: 1,
                    seq: 0,
                    cmd: None
                }
            }
        );
        // The host's own messages reach both; a joiner cannot send a Drop.
        host.send(Msg::Drop { slot: 2, at_seq: 4 });
        let at_a = poll_until(&mut a.wire, |s| !s.is_empty());
        assert_eq!(
            at_a[0],
            Inbound::Msg {
                from: 0,
                msg: Msg::Drop { slot: 2, at_seq: 4 }
            }
        );
        a.wire.send(Msg::Drop { slot: 0, at_seq: 0 });
        a.wire.send(Msg::DropRequest { slot: 2 });
        let at_host = poll_until(&mut host, |s| !s.is_empty());
        assert_eq!(at_host.len(), 1, "the forged Drop was ignored: {at_host:?}");
        assert_eq!(
            at_host[0],
            Inbound::Msg {
                from: 1,
                msg: Msg::DropRequest { slot: 2 }
            }
        );
        // A closed connection is reported with its slot.
        drop(b);
        let closed = poll_until(&mut host, |s| s.iter().any(|i| matches!(i, Inbound::Closed { .. })));
        assert!(closed.contains(&Inbound::Closed { slot: 2 }));
    }

    #[test]
    fn a_different_build_or_data_is_refused_and_the_slot_stays_open() {
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let host = thread::spawn({
            let id = id.clone();
            move || {
                let mut said = Vec::new();
                host_game(&listener, config(2), &id, &[1], |l| said.push(l.to_string())).unwrap();
                said
            }
        });
        let addr = format!("127.0.0.1:{port}");
        let join = |identity| join_game(&addr, identity, "x", |_| Ok(()), |_| {});
        let err = join(Identity::ours(2)).err().expect("different data");
        assert!(err.to_string().contains("game data"), "{err}");
        let mut other_build = Identity::ours(1);
        other_build.sim_build = "tidewater-sim 99.0.0".into();
        let err = join(other_build).err().expect("different build");
        assert!(err.to_string().contains("builds"), "{err}");
        // The slot is still open for a compatible joiner.
        let ok = join(id).unwrap();
        assert_eq!(ok.slot, 1);
        let said = host.join().unwrap();
        assert_eq!(said.iter().filter(|l| l.contains("refused")).count(), 2, "{said:?}");
    }

    #[test]
    fn a_joiner_without_the_map_declines_and_says_why() {
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let host = thread::spawn({
            let id = id.clone();
            move || {
                let mut said = Vec::new();
                host_game(&listener, config(2), &id, &[1], |l| said.push(l.to_string())).unwrap();
                said
            }
        });
        let addr = format!("127.0.0.1:{port}");
        let err = join_game(&addr, id.clone(), "x", |_| Err("no map".to_string()), |_| {})
            .err()
            .unwrap();
        assert!(err.to_string().contains("no map"), "{err}");
        join_game(&addr, id, "x", |_| Ok(()), |_| {}).unwrap();
        let said = host.join().unwrap();
        assert!(
            said.iter().any(|l| l.contains("declined") && l.contains("no map")),
            "{said:?}"
        );
    }

    #[test]
    fn something_that_is_not_a_joiner_does_not_hold_up_the_game() {
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let host = thread::spawn({
            let id = id.clone();
            move || host_game(&listener, config(2), &id, &[1], |_| {}).map(|_| ())
        });
        let addr = format!("127.0.0.1:{port}");
        // Garbage bytes, then a stray Ready.
        {
            let mut s = TcpStream::connect(&addr).unwrap();
            io::Write::write_all(
                &mut s,
                &[9, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
            )
            .unwrap();
            let mut t = TcpStream::connect(&addr).unwrap();
            write_frame(&mut t, &Msg::Ready).unwrap();
        }
        join_game(&addr, id, "x", |_| Ok(()), |_| {}).unwrap();
        host.join().unwrap().unwrap();
    }

    #[test]
    fn an_observer_gets_a_place_of_its_own_and_hears_the_relayed_turns_but_is_not_heard() {
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let host = thread::spawn({
            let id = id.clone();
            move || host_game_with(&listener, config(2), &id, &[1], 1, |_| {}).unwrap()
        });
        let addr = format!("127.0.0.1:{port}");
        let player = {
            let (addr, id) = (addr.clone(), id.clone());
            thread::spawn(move || join_game(&addr, id, "Ann", |_| Ok(()), |_| {}).unwrap())
        };
        thread::sleep(Duration::from_millis(100));
        let mut watcher = join_as(&addr, id, "Eve", true, |_| Ok(()), |_| {}).unwrap();
        let mut host = host.join().unwrap();
        let mut player = player.join().unwrap();
        assert_eq!(watcher.slot, OBSERVER_BASE);
        assert_eq!(watcher.config.lobby.local, 0, "it looks from the host's place");
        assert_eq!(watcher.names, ["Player 1", "Ann"], "an observer is not a player's name");
        // The host's messages reach it, and so do a player's through the host.
        host.send(Msg::Drop { slot: 1, at_seq: 2 });
        poll_until(&mut watcher.wire, |s| !s.is_empty());
        let turn = Msg::Turn {
            slot: 1,
            seq: 0,
            cmd: None,
        };
        player.wire.send(turn.clone());
        let at_host = poll_until(&mut host, |s| !s.is_empty());
        host.forward(1, &at_host[0].clone().into_msg());
        let at_watcher = poll_until(&mut watcher.wire, |s| !s.is_empty());
        assert_eq!(at_watcher[0], Inbound::Msg { from: 1, msg: turn });
        // Whatever it sends is ignored, and its leaving is not a player leaving.
        watcher.wire.send(Msg::Turn {
            slot: 0,
            seq: 5,
            cmd: None,
        });
        drop(watcher);
        player.wire.send(Msg::DropRequest { slot: 0 });
        let at_host = poll_until(&mut host, |s| !s.is_empty());
        assert_eq!(at_host.len(), 1, "only the player was heard: {at_host:?}");
        assert!(matches!(at_host[0], Inbound::Msg { from: 1, .. }));
    }

    #[test]
    fn an_observer_is_refused_without_a_place_and_a_player_when_the_game_is_full() {
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = Identity::ours(1);
        let host = thread::spawn({
            let id = id.clone();
            move || {
                let mut said = Vec::new();
                host_game_with(&listener, config(2), &id, &[1], 1, |l| said.push(l.to_string())).unwrap();
                said
            }
        });
        let addr = format!("127.0.0.1:{port}");
        let watch = || join_as(&addr, id.clone(), "x", true, |_| Ok(()), |_| {});
        // Joining blocks until the game starts, so the first observer waits in a thread of its own.
        let first = thread::spawn({
            let (addr, id) = (addr.clone(), id.clone());
            move || join_as(&addr, id, "x", true, |_| Ok(()), |_| {}).unwrap()
        });
        thread::sleep(Duration::from_millis(100));
        // The one place is taken; the next observer is turned away, and the player slot is still open.
        let err = watch().err().expect("no second place");
        assert!(err.to_string().contains("no place for an observer"), "{err}");
        join_game(&addr, id.clone(), "x", |_| Ok(()), |_| {}).unwrap();
        assert_eq!(first.join().unwrap().slot, OBSERVER_BASE);
        host.join().unwrap();
        // A host without places says so at once.
        let listener = listen(0).unwrap();
        let port = listener.local_addr().unwrap().port();
        let host = thread::spawn({
            let id = id.clone();
            move || host_game(&listener, config(2), &id, &[1], |_| {}).unwrap()
        });
        let addr = format!("127.0.0.1:{port}");
        let err = join_as(&addr, id.clone(), "x", true, |_| Ok(()), |_| {}).err().unwrap();
        assert!(err.to_string().contains("no place for an observer"), "{err}");
        join_game(&addr, id, "x", |_| Ok(()), |_| {}).unwrap();
        host.join().unwrap();
    }

    #[test]
    fn nobody_listening_is_an_error_that_names_the_firewall() {
        // A port that was just free and is closed now.
        let port = {
            let l = listen(0).unwrap();
            l.local_addr().unwrap().port()
        };
        let err = join_game(&format!("127.0.0.1:{port}"), Identity::ours(1), "x", |_| Ok(()), |_| {})
            .err()
            .unwrap();
        assert!(err.to_string().contains("firewall"), "{err}");
    }
}
