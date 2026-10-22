//! The LAN beacon: how a Local Network game list finds the games on the same network (`docs/design/network.md`,
//! "LAN discovery").
//!
//! A game's creator broadcasts one small UDP packet a second while its lobby is open and has a free row; a machine
//! looking at the Local Network game list listens for them and lists each game it hears about for a few seconds after
//! the last packet. Nothing is sent by a machine that only looks, and nothing is trusted: the packet only names where
//! to connect, and the TCP handshake (`net::room`) decides whether the join happens.
//!
//! The packet is built by hand, not with postcard, so that it stays readable by any later version:
//!
//! | Bytes | Field |
//! |---|---|
//! | 4 | magic `TWLN` |
//! | 1 | beacon version, [`VERSION`] |
//! | 1 | kind, 1 = announce (the only one) |
//! | 2 | the network protocol ([`PROTOCOL`](super::proto::PROTOCOL)), little endian |
//! | 2 | the TCP port the lobby listens on, little endian |
//! | 1 | players in the lobby |
//! | 1 | rows that can hold a player |
//! | 1 + n | the creator's name, at most [`OWNER_MAX`] bytes of UTF-8 |
//! | 1 + n | the map's name, at most [`MAP_MAX`] bytes of UTF-8 |
//!
//! A packet that doesn't match exactly (another program's, another version, truncated, with trailing bytes, bad
//! UTF-8, a zero port) is ignored without a word. The sender's address is the game's address: the packet doesn't
//! carry one.
//!
//! The listening sockets share their port (`SO_REUSEADDR`, and `SO_REUSEPORT` on Unix), so two programs on one
//! machine can both watch the list; a broadcast reaches all of them.

use std::collections::HashMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use super::proto::PROTOCOL;

/// The UDP port beacons are sent to (the same number as the game's TCP port; the two don't meet).
pub const PORT: u16 = 6112;
/// The beacon packet's version. A packet of another version is ignored.
pub const VERSION: u8 = 1;
const MAGIC: [u8; 4] = *b"TWLN";
const KIND_ANNOUNCE: u8 = 1;
/// The longest creator name and map name sent, in bytes.
pub const OWNER_MAX: usize = 24;
pub const MAP_MAX: usize = 48;
/// How often a creator announces.
pub const INTERVAL: Duration = Duration::from_secs(1);
/// How long a game stays listed after its last packet.
pub const EXPIRE: Duration = Duration::from_secs(4);

/// What a beacon says about a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Announce {
    pub protocol: u16,
    /// The TCP port the lobby listens on.
    pub tcp_port: u16,
    pub players: u8,
    /// How many rows can hold a player.
    pub max: u8,
    pub owner: String,
    pub map: String,
}

impl Announce {
    /// The beacon of a lobby of this program's protocol.
    pub fn ours(tcp_port: u16, players: u8, max: u8, owner: &str, map: &str) -> Announce {
        Announce {
            protocol: PROTOCOL,
            tcp_port,
            players,
            max,
            owner: clip(owner, OWNER_MAX),
            map: clip(map, MAP_MAX),
        }
    }
}

/// `text` cut to at most `max` bytes at a character boundary, with control characters dropped.
fn clip(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}

/// The packet for `a`.
pub fn encode(a: &Announce) -> Vec<u8> {
    let owner = clip(&a.owner, OWNER_MAX);
    let map = clip(&a.map, MAP_MAX);
    let mut v = Vec::with_capacity(16 + owner.len() + map.len());
    v.extend_from_slice(&MAGIC);
    v.push(VERSION);
    v.push(KIND_ANNOUNCE);
    v.extend_from_slice(&a.protocol.to_le_bytes());
    v.extend_from_slice(&a.tcp_port.to_le_bytes());
    v.push(a.players);
    v.push(a.max);
    v.push(owner.len() as u8);
    v.extend_from_slice(owner.as_bytes());
    v.push(map.len() as u8);
    v.extend_from_slice(map.as_bytes());
    v
}

/// The beacon in a packet, or `None` for anything else.
pub fn decode(b: &[u8]) -> Option<Announce> {
    if b.len() < 14 || b[..4] != MAGIC || b[4] != VERSION || b[5] != KIND_ANNOUNCE {
        return None;
    }
    let protocol = u16::from_le_bytes([b[6], b[7]]);
    let tcp_port = u16::from_le_bytes([b[8], b[9]]);
    let (players, max) = (b[10], b[11]);
    let take = |at: usize, limit: usize| -> Option<(String, usize)> {
        let n = *b.get(at)? as usize;
        if n > limit {
            return None;
        }
        let s = std::str::from_utf8(b.get(at + 1..at + 1 + n)?).ok()?;
        if s.chars().any(char::is_control) {
            return None;
        }
        Some((s.to_string(), at + 1 + n))
    };
    let (owner, at) = take(12, OWNER_MAX)?;
    let (map, end) = take(at, MAP_MAX)?;
    if end != b.len() || tcp_port == 0 || max == 0 || max > 8 || players > max {
        return None;
    }
    Some(Announce {
        protocol,
        tcp_port,
        players,
        max,
        owner,
        map,
    })
}

/// The creator's side: sends the beacon about once a second to wherever it is aimed (the broadcast address).
#[derive(Debug)]
pub struct Announcer {
    socket: UdpSocket,
    target: SocketAddr,
    last: Option<(Instant, Vec<u8>)>,
}

impl Announcer {
    /// An announcer for the local network.
    pub fn broadcast() -> io::Result<Announcer> {
        Announcer::to(SocketAddr::from((Ipv4Addr::BROADCAST, PORT)))
    }

    /// An announcer aimed at one address (a test's).
    pub fn to(target: SocketAddr) -> io::Result<Announcer> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_broadcast(true)?;
        socket.set_nonblocking(true)?;
        Ok(Announcer {
            socket,
            target,
            last: None,
        })
    }

    /// Sends the beacon if a second has passed since the last one, or at once if it changed. A failed send (no
    /// network) is not an error: the next try may work.
    pub fn tick(&mut self, now: Instant, a: &Announce) {
        let packet = encode(a);
        let due = match &self.last {
            Some((at, last)) => *last != packet || now.duration_since(*at) >= INTERVAL,
            None => true,
        };
        if due {
            let _ = self.socket.send_to(&packet, self.target);
            self.last = Some((now, packet));
        }
    }
}

/// A game heard on the network.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// Where to join it: `HOST:PORT`.
    pub addr: String,
    pub announce: Announce,
}

/// The game list's side: listens for beacons and remembers the games for a few seconds each.
#[derive(Debug)]
pub struct Finder {
    socket: UdpSocket,
    seen: HashMap<String, (Announce, Instant)>,
}

impl Finder {
    /// Listens on the beacon port.
    pub fn listen() -> io::Result<Finder> {
        Finder::bind(PORT)
    }

    /// Listens on `port` (a test's, so it can't hear the real network's games).
    pub fn bind(port: u16) -> io::Result<Finder> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        #[cfg(all(unix, not(any(target_os = "solaris", target_os = "illumos", target_os = "cygwin"))))]
        socket.set_reuse_port(true)?;
        socket.set_broadcast(true)?;
        socket.bind(&SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)).into())?;
        socket.set_nonblocking(true)?;
        Ok(Finder {
            socket: socket.into(),
            seen: HashMap::new(),
        })
    }

    /// The port listened on.
    pub fn port(&self) -> u16 {
        self.socket.local_addr().map_or(0, |a| a.port())
    }

    /// Reads what has arrived and returns the games heard of within [`EXPIRE`], in a stable order (by address).
    /// Games of another protocol are left out: they would be refused.
    pub fn poll(&mut self, now: Instant) -> Vec<Found> {
        let mut buf = [0u8; 512];
        while let Ok((n, from)) = self.socket.recv_from(&mut buf) {
            if let Some(a) = decode(&buf[..n]) {
                let addr = format!("{}:{}", from.ip(), a.tcp_port);
                self.seen.insert(addr, (a, now));
            }
        }
        self.seen.retain(|_, (_, at)| now.duration_since(*at) < EXPIRE);
        let mut v: Vec<Found> = self
            .seen
            .iter()
            .filter(|(_, (a, _))| a.protocol == PROTOCOL)
            .map(|(addr, (a, _))| Found {
                addr: addr.clone(),
                announce: a.clone(),
            })
            .collect();
        v.sort_by(|a, b| a.addr.cmp(&b.addr));
        v
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

    fn sample() -> Announce {
        Announce::ours(6112, 2, 4, "Ann", "Gold River")
    }

    #[test]
    fn a_packet_round_trips_and_is_small() {
        let b = encode(&sample());
        assert!(b.len() < 100, "{}", b.len());
        assert_eq!(&b[..4], b"TWLN");
        assert_eq!(decode(&b), Some(sample()));
    }

    #[test]
    fn names_are_clipped_at_a_character_boundary_and_stripped_of_controls() {
        let long = "é".repeat(40);
        let a = Announce::ours(1, 1, 2, &long, "a\nb");
        assert_eq!(a.owner.len(), OWNER_MAX);
        assert_eq!(a.map, "ab");
        assert_eq!(decode(&encode(&a)), Some(a));
    }

    #[test]
    fn foreign_and_damaged_packets_are_ignored() {
        let good = encode(&sample());
        assert!(decode(&[]).is_none());
        assert!(decode(b"hello there, this is not a beacon").is_none());
        let mut v = good.clone();
        v[4] = VERSION + 1;
        assert!(decode(&v).is_none(), "another version");
        let mut v = good.clone();
        v[5] = 9;
        assert!(decode(&v).is_none(), "another kind");
        let mut v = good.clone();
        v.push(0);
        assert!(decode(&v).is_none(), "trailing bytes");
        assert!(decode(&good[..good.len() - 1]).is_none(), "truncated");
        let mut v = good.clone();
        v[8] = 0;
        v[9] = 0;
        assert!(decode(&v).is_none(), "no port");
        let mut v = good.clone();
        v[12] = 0xff;
        assert!(decode(&v).is_none(), "a name longer than allowed");
        let mut v = good.clone();
        v[13] = 0xff;
        assert!(decode(&v).is_none(), "bad UTF-8");
        let a = Announce {
            players: 5,
            max: 4,
            ..sample()
        };
        assert!(decode(&encode(&a)).is_none(), "more players than rows");
    }

    /// Announces to the finder's own port over loopback and waits for the game to show.
    fn hear(finder: &mut Finder, announcer: &mut Announcer, a: &Announce) -> Vec<Found> {
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            announcer.tick(Instant::now(), a);
            let found = finder.poll(Instant::now());
            if !found.is_empty() || Instant::now() > end {
                return found;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_finder_lists_an_announced_game_and_forgets_it_after_the_expiry() {
        let mut finder = Finder::bind(0).unwrap();
        let target = SocketAddr::from((Ipv4Addr::LOCALHOST, finder.port()));
        let mut announcer = Announcer::to(target).unwrap();
        let found = hear(&mut finder, &mut announcer, &sample());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].addr, "127.0.0.1:6112");
        assert_eq!(found[0].announce, sample());
        // A change is sent at once, not after the interval.
        let full = Announce { players: 4, ..sample() };
        announcer.tick(Instant::now(), &full);
        let end = Instant::now() + Duration::from_secs(5);
        while finder.poll(Instant::now())[0].announce.players != 4 && Instant::now() < end {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(finder.poll(Instant::now())[0].announce.players, 4);
        // Silence for longer than the expiry.
        assert!(finder.poll(Instant::now() + EXPIRE + Duration::from_secs(1)).is_empty());
    }

    #[test]
    fn a_finder_skips_other_protocols_and_other_programs_packets() {
        let mut finder = Finder::bind(0).unwrap();
        let target = SocketAddr::from((Ipv4Addr::LOCALHOST, finder.port()));
        let noise = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        noise.send_to(b"not ours", target).unwrap();
        let mut announcer = Announcer::to(target).unwrap();
        let other = Announce {
            protocol: PROTOCOL + 1,
            ..sample()
        };
        announcer.tick(Instant::now(), &other);
        thread::sleep(Duration::from_millis(100));
        assert!(finder.poll(Instant::now()).is_empty());
    }

    #[test]
    fn two_finders_can_share_a_port() {
        let a = Finder::bind(0).unwrap();
        let b = Finder::bind(a.port());
        assert!(b.is_ok(), "{b:?}");
    }

    /// The real broadcast: needs a network that passes broadcasts, so it runs only when asked for
    /// (`TIDEWATER_TEST_BROADCAST=1`).
    #[test]
    fn a_broadcast_reaches_a_finder_on_the_same_network() {
        if std::env::var_os("TIDEWATER_TEST_BROADCAST").is_none() {
            return;
        }
        // An unusual port so a real game list on this machine isn't disturbed.
        let port = 46112;
        let mut finder = Finder::bind(port).unwrap();
        let mut announcer = Announcer::to(SocketAddr::from((Ipv4Addr::BROADCAST, port))).unwrap();
        let found = hear(&mut finder, &mut announcer, &sample());
        assert_eq!(found.len(), 1, "no broadcast came back");
        assert_eq!(found[0].announce, sample());
    }
}
