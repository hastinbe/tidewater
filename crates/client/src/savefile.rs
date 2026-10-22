//! The saved-game file (spec `docs/specs/game/save-load.md`). SDL-free.
//!
//! The spec describes behaviour, not a layout, and Tidewater doesn't read the original game's saved games, so the
//! layout is our own:
//!
//! ```text
//! 8 bytes   magic  "TWSAVE\x1a\x00"
//! 2 bytes   format version, little endian ([`VERSION`])
//! ...       postcard Header   (name, time, rules, kind, map, set-up, start seed)
//! ...       postcard Body     (the world's bytes from `World::save_state`, and the client's state)
//! ```
//!
//! Postcard (<https://docs.rs/postcard>) is a compact serde format with a stable wire specification. The header comes
//! first so the lists can read it without decoding the world. A file with another version is refused with the "other
//! rules" reason (network entry 11, "invalid version number" in the table), a file without the magic with entry 12, a
//! file that ends early with entry 13.
//!
//! Recorded gaps: the header keeps the start seed and set-up so a load can build the same *template* world from the
//! map (the world's stored part leaves out what the map rebuilds, see `tidewater_sim::World::restore`). The map is
//! kept by the name the game was started with, so a custom map given by a file path loads only while it is still there.

use serde::{Deserialize, Serialize};
use tidewater_sim::setup::Setup;

use crate::campaign::{CampaignTotal, GameKind, Memory};

pub const MAGIC: [u8; 8] = *b"TWSAVE\x1a\0";
/// The layout version; raise it whenever the stored world or the structs below change. Version 2: a unit's decay
/// periods moved into its mana byte (the separate counter is gone). Version 3: a unit's pending-command mark.
/// Version 4: a unit's attack cooldown, a dying unit's kept target and a killer's forgotten target. Version 5: the
/// round-40 to round-49 unit state (the build goal, the hall exit mark, the search skip mark, a dying unit's reported
/// target slot and command 4's target unit). Version 6: the unit-to-slot map keeps a released unit's entry until its
/// slot is reused, and attacker records may name a released unit (round-51 errata). Version 7: a unit inside a
/// collapsed mine or platform has no target (round-54 errata). Version 8: an effect keeps its shooter's unit slot
/// after the slot is released (round-56 errata). Version 9: a move (order 3) may keep a target unit (round-62 errata,
/// "Smart move"). Version 10: order 18 keeps its pulled-back goal beside the target tile (round-68 errata).
/// Version 11: a building's job keeps its to-be-cancelled mark (round 76, "Cancelling a job"). Version 12: order 22
/// may have no target, for a builder let out of a cancelled site (round 77, "Cancelling construction"). Version 13: a
/// job no longer keeps the cost it paid; a refund looks the cost up again (round 77, "Paying"). Version 14: a unit keeps
/// its creation number and the world its next one, the world keeps the rule a found outcome replaced, and the pause mark
/// is no longer stored (round 90, "What a load restarts", `game/save-load.md`). Version 15: each computer player keeps the
/// surplus on its footmen, archers, siege and knights counts (round 92, SL4 to SL6). Version 16: the surplus covers eleven groups, footmen to submarines (round 93, SL13 to SL15). Version 17: a computer player's buildings-in-progress counts are 16-bit (round 95, SL25). Version 18: the world's part ends with the optional improvements and what they keep (`tidewater_sim::extras`). Version 19: a unit's repair counter is its goal x, a signed 16-bit value (round 97, RP1).
pub const VERSION: u16 = 19;
const FIXED: usize = MAGIC.len() + 2;

/// The kind of game a file was saved from; it picks the extension (spec "Where saved games live").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Single,
    /// Multiplayer over a network session.
    Session,
    /// Other multiplayer games.
    Net,
}

impl Kind {
    pub fn extension(self) -> &'static str {
        match self {
            Kind::Single => "sav",
            Kind::Session => "mlt",
            Kind::Net => "net",
        }
    }
}

/// Why a file couldn't be saved or loaded: the reasons of the spec's "Error reasons" table. The number is the
/// `network.tbl` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    CantCreate,
    CantOpen,
    OtherRules,
    NotSaved,
    TooShort,
    WriteFailed,
}

impl Reason {
    pub fn entry(self) -> u16 {
        match self {
            Reason::CantCreate => 9,
            Reason::CantOpen => 10,
            Reason::OtherRules => 11,
            Reason::NotSaved => 12,
            Reason::TooShort => 13,
            Reason::WriteFailed => 14,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// The name typed when saving (the lists show the file name instead).
    pub name: String,
    /// When the game was saved: seconds since 1970-01-01 UTC.
    pub saved_at: u64,
    /// The rules it was saved under: Enhanced, or Classic multiplayer.
    pub enhanced: bool,
    pub kind: Kind,
    /// The map as the game was started with it.
    pub map: String,
    pub setup: Setup,
    /// The seed the game's slot set-up drew from, so the template world is the same.
    pub seed: u32,
    /// The local player's race (0 human, 1 orc): the dialog's picture.
    pub race: u8,
}

/// What the client keeps beside the world.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientState {
    /// Which colour cycles are on, bit n for cycle n, the pulse included.
    pub cycles_on: u8,
    /// The music track (after the bump), if any plays.
    pub music: Option<u8>,
    /// The screen's map position: the centre of the map view in map pixels.
    pub screen: (i32, i32),
    /// The flow's record of the game.
    pub game: GameKind,
    pub total: CampaignTotal,
    pub memory: Memory,
    pub start_controllers: [u8; 8],
    /// The game clock in milliseconds and each player's pause count. Enhanced saves only; the simulation doesn't hold
    /// them: `savegame::GameClock` and `savegame::PauseCounts` do.
    pub clock_ms: Option<u32>,
    pub pause_counts: Option<[u8; 16]>,
}

impl ClientState {
    /// A Classic save drops the clock and the pause counts (spec "What a loaded game starts afresh").
    pub fn for_rules(mut self, enhanced: bool) -> Self {
        if !enhanced {
            self.clock_ms = None;
            self.pause_counts = None;
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    /// `World::save_state`.
    pub world: Vec<u8>,
    pub client: ClientState,
}

pub fn encode(header: &Header, body: &Body) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.world.len() + 256);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend(postcard::to_allocvec(header).expect("encoding to memory"));
    out.extend(postcard::to_allocvec(body).expect("encoding to memory"));
    out
}

/// Reads the header and returns it with the rest of the file. `enhanced` is the rules the caller can load.
pub fn read_header(bytes: &[u8], enhanced: bool) -> Result<(Header, &[u8]), Reason> {
    if bytes.len() < FIXED {
        return Err(
            if bytes.is_empty() || !MAGIC.starts_with(&bytes[..bytes.len().min(MAGIC.len())]) {
                Reason::NotSaved
            } else {
                Reason::TooShort
            },
        );
    }
    if bytes[..MAGIC.len()] != MAGIC {
        return Err(Reason::NotSaved);
    }
    if u16::from_le_bytes([bytes[8], bytes[9]]) != VERSION {
        return Err(Reason::OtherRules);
    }
    let (header, rest): (Header, &[u8]) = postcard::take_from_bytes(&bytes[FIXED..]).map_err(|_| Reason::TooShort)?;
    if header.enhanced != enhanced {
        return Err(Reason::OtherRules);
    }
    Ok((header, rest))
}

/// Reads a whole file.
pub fn decode(bytes: &[u8], enhanced: bool) -> Result<(Header, Body), Reason> {
    let (header, rest) = read_header(bytes, enhanced)?;
    let body = postcard::from_bytes(rest).map_err(|_| Reason::TooShort)?;
    Ok((header, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(enhanced: bool) -> (Header, Body) {
        (
            Header {
                name: "Siege".into(),
                saved_at: 1_700_000_000,
                enhanced,
                kind: Kind::Single,
                map: "Campaign\\Human\\Human01.pud".into(),
                setup: Setup::for_map("Campaign\\Human\\Human01.pud"),
                seed: 77,
                race: 0,
            },
            Body {
                world: vec![1, 2, 3, 4],
                client: ClientState {
                    cycles_on: 0b1000_0101,
                    music: Some(7),
                    screen: (640, 480),
                    game: GameKind::Campaign(0),
                    total: CampaignTotal {
                        total: 900,
                        at_start: 400,
                    },
                    memory: Memory {
                        last_act: Some(0),
                        last_video: None,
                    },
                    start_controllers: [5, 4, 3, 3, 3, 3, 3, 3],
                    clock_ms: Some(61_000),
                    pause_counts: None,
                },
            },
        )
    }

    #[test]
    fn round_trips() {
        let (h, b) = sample(true);
        let bytes = encode(&h, &b);
        assert_eq!(decode(&bytes, true), Ok((h, b)));
    }

    #[test]
    fn refusals_have_their_reasons() {
        let (h, b) = sample(true);
        let bytes = encode(&h, &b);
        assert_eq!(
            decode(&bytes, false),
            Err(Reason::OtherRules),
            "saved under the other rules"
        );
        assert_eq!(decode(b"hello, this is not a saved game", true), Err(Reason::NotSaved));
        assert_eq!(decode(&[], true), Err(Reason::NotSaved));
        assert_eq!(decode(&MAGIC[..4], true), Err(Reason::TooShort));
        assert_eq!(decode(&bytes[..bytes.len() - 3], true), Err(Reason::TooShort));
        assert_eq!(decode(&bytes[..14], true), Err(Reason::TooShort));
        let mut other = bytes.clone();
        other[8] ^= 0xff;
        assert_eq!(decode(&other, true), Err(Reason::OtherRules), "another format version");
    }

    #[test]
    fn classic_drops_the_clock_and_pauses() {
        let (_, b) = sample(false);
        let c = ClientState {
            pause_counts: Some([1; 16]),
            ..b.client
        };
        let c = c.for_rules(false);
        assert_eq!((c.clock_ms, c.pause_counts), (None, None));
    }

    #[test]
    fn extensions_by_kind() {
        assert_eq!(Kind::Single.extension(), "sav");
        assert_eq!(Kind::Session.extension(), "mlt");
        assert_eq!(Kind::Net.extension(), "net");
    }
}
