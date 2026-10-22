//! Generated test maps: the maps an oracle trace's header can name as `R<round>-<name>.pud`.
//!
//! Source: `docs/specs/oracle/test-maps.md`, which gives every section of a generated map and the tables of the
//! maps themselves; the section layouts are the PUD text's (Daniel Lemberg, "PUD file format", 1997, mirrored in
//! Wargus's `doc/pud-format.html`). A generated map holds no game data: its unit and upgrade data sections say "use
//! the default data". The bytes built here must match the SHA-256 the spec gives for each map (see the tests).

use crate::pud::build::section;

/// One unit of a generated map: (x, y, PUD unit type, owner, data word).
type Unit = (u16, u16, u8, u8, u16);

/// One player's start settings: (race, gold, lumber, oil, spells known).
type Player = (u8, u16, u16, u16, u32);

/// Everything that distinguishes one generated map from another.
struct Spec {
    size: u16,
    id_tag: u32,
    description: &'static str,
    players: &'static [Player],
    units: &'static [Unit],
    /// The rows of sea along the bottom edge (0: none; the spec's "Sea" option, 1 to N - 4).
    sea: u16,
}

/// A start location of a human player (0x5E) or an orc player (0x5F).
const HUMAN_START: u8 = 0x5E;
const ORC_START: u8 = 0x5F;

/// `R99-tests.pud` (spec: "Data", "R99-tests.pud").
const R99_TESTS: Spec = Spec {
    size: 64,
    id_tag: 0x7199,
    description: "tidewater r99 tests",
    players: &[
        (0, 0, 0, 0, 0),
        (1, 0, 0, 0, 0),
        // Invisibility is bit 7 of the spells word (128).
        (0, 0, 0, 0, 128),
        (0, 2000, 1000, 0, 0),
    ],
    units: &[
        (30, 40, HUMAN_START, 0, 0),
        (30, 4, ORC_START, 1, 0),
        (4, 52, HUMAN_START, 2, 0),
        (40, 33, HUMAN_START, 3, 0),
        (8, 8, 0x0A, 0, 0),
        (10, 8, 0x0B, 1, 0),
        (8, 18, 0x0A, 0, 0),
        (10, 18, 0x01, 1, 0),
        (50, 5, 0x2B, 1, 0),
        (44, 8, 0x28, 0, 0),
        (40, 14, 0x00, 0, 0),
        (50, 12, 0x01, 1, 0),
        (2, 56, 0x4A, 2, 0),
        (12, 42, 0x00, 2, 1),
        (12, 44, 0x0A, 2, 1),
        (12, 46, 0x08, 2, 1),
        (34, 58, 0x40, 2, 0),
        (47, 58, 0x04, 0, 0),
        (47, 60, 0x04, 0, 0),
        (40, 30, 0x02, 3, 0),
        (41, 30, 0x02, 3, 0),
        // A gold mine of 50 000 gold (data is the gold / 2500), owned by the neutral slot.
        (46, 26, 0x5C, 15, 20),
    ],
    sea: 0,
};

/// `R100-tests.pud` (spec: "Data", "R100-tests.pud"): a sea of 12 rows along the bottom.
const R100_TESTS: Spec = Spec {
    size: 64,
    id_tag: 0x7200,
    description: "tidewater r100 tests",
    players: &[(0, 10_000, 10_000, 1_000, 0), (1, 0, 0, 0, 0)],
    units: &[
        (10, 10, HUMAN_START, 0, 0),
        (55, 10, ORC_START, 1, 0),
        // The human transport, then six footmen on the shore beside it.
        (40, 54, 0x1C, 0, 0),
        (44, 48, 0x00, 0, 0),
        (45, 48, 0x00, 0, 0),
        (46, 48, 0x00, 0, 0),
        (47, 48, 0x00, 0, 0),
        (48, 48, 0x00, 0, 0),
        (49, 48, 0x00, 0, 0),
        (24, 48, 0x00, 0, 0),
        // The town hall, the barracks, a ballista and three peasants.
        (6, 6, 0x4A, 0, 0),
        (14, 6, 0x3C, 0, 0),
        (6, 17, 0x04, 0, 0),
        (11, 8, 0x02, 0, 0),
        (26, 20, 0x02, 0, 0),
        (26, 23, 0x02, 0, 0),
        // The orc player's grunt and dragon.
        (41, 47, 0x01, 1, 0),
        (32, 46, 0x2B, 1, 0),
    ],
    sea: 12,
};

/// Whether a trace header's map name is a generated test map's: `R<round>-<name>.pud`, a capital R, the round
/// number, a hyphen, a name.
pub fn is_generated_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('R').and_then(|r| r.strip_suffix(".pud")) else {
        return false;
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest[digits..].strip_prefix('-').is_some_and(|n| !n.is_empty())
}

/// The bytes of the generated test map `name`, or `None` when the name isn't a known one.
pub fn generated(name: &str) -> Option<Vec<u8>> {
    match name {
        "R99-tests.pud" => Some(build(&R99_TESTS)),
        "R100-tests.pud" => Some(build(&R100_TESTS)),
        _ => None,
    }
}

fn words(w: impl IntoIterator<Item = u16>) -> Vec<u8> {
    w.into_iter().flat_map(u16::to_le_bytes).collect()
}

fn dwords(w: impl IntoIterator<Item = u32>) -> Vec<u8> {
    w.into_iter().flat_map(u32::to_le_bytes).collect()
}

fn build(s: &Spec) -> Vec<u8> {
    let n = usize::from(s.size);
    let tiles = n * n;
    let p = s.players.len();
    // One value for each of the 16 slots: players 0 to P - 1 from `own`, the slots up to 14 `other`, slot 15 `last`.
    let slots = |own: &dyn Fn(usize) -> u8, other: u8, last: u8| -> Vec<u8> {
        (0..16)
            .map(|i| match i {
                15 => last,
                i if i < p => own(i),
                _ => other,
            })
            .collect()
    };
    // The section words by tile: the spec's sea rows (the coast, the shore, then the water) or the land value.
    let tile_words = |land: u16, coast: u16, shore: u16, water: u16| {
        words((0..tiles).map(|t| {
            let row = (t / n) as u16;
            match (s.sea > 0).then(|| s.size - s.sea) {
                Some(w) if row >= w => water,
                Some(w) if row + 1 == w => shore,
                Some(w) if row + 2 == w => coast,
                _ => land,
            }
        }))
    };
    let resource = |f: &dyn Fn(&Player) -> u16| words((0..16).map(|i| s.players.get(i).map_or(0, f)));
    let mut ty = b"WAR2 MAP\0\0\x0a\xff".to_vec();
    ty.extend(s.id_tag.to_le_bytes());
    let mut desc = [0u8; 32];
    desc[..s.description.len()].copy_from_slice(s.description.as_bytes());
    let mut udta = vec![0u8; 5696];
    udta[..2].copy_from_slice(&1u16.to_le_bytes());
    let mut ugrd = vec![0u8; 782];
    ugrd[..2].copy_from_slice(&1u16.to_le_bytes());
    // The six per-player arrays of 16 words, in the PUD text's order.
    let mut alow = Vec::new();
    alow.extend(dwords([0xFFFF_FFFF; 16]));
    alow.extend(dwords((0..16).map(|i| s.players.get(i).map_or(0, |pl| pl.4))));
    alow.extend(dwords([0x000F_FFFB; 16]));
    alow.extend(dwords([0; 16]));
    alow.extend(dwords([0xFFFF_FFFF; 16]));
    alow.extend(dwords([0; 16]));
    let units: Vec<u8> = s
        .units
        .iter()
        .flat_map(|&(x, y, kind, owner, data)| {
            let mut r = Vec::with_capacity(8);
            r.extend(x.to_le_bytes());
            r.extend(y.to_le_bytes());
            r.extend([kind, owner]);
            r.extend(data.to_le_bytes());
            r
        })
        .collect();
    [
        section(b"TYPE", &ty),
        section(b"VER ", &words([0x11])),
        section(b"DESC", &desc),
        section(b"OWNR", &slots(&|_| 5, 3, 2)),
        section(b"ERA ", &words([0])),
        section(b"DIM ", &words([s.size, s.size])),
        section(b"UDTA", &udta),
        section(b"ALOW", &alow),
        // After `ALOW`, so a reader following the setup spec's reading rule passes it by.
        section(b"UGRD", &ugrd),
        section(b"SIDE", &slots(&|i| s.players[i].0, 0, 2)),
        section(b"SGLD", &resource(&|pl| pl.1)),
        section(b"SLBR", &resource(&|pl| pl.2)),
        section(b"SOIL", &resource(&|pl| pl.3)),
        section(b"AIPL", &[0; 16]),
        section(b"MTXM", &tile_words(0x0050, 0x0030, 0x02B0, 0x0010)),
        section(b"SQM ", &tile_words(0x0001, 0x0011, 0x0082, 0x0040)),
        section(b"OILM", &vec![0; tiles]),
        section(b"REGM", &tile_words(0x4000, 0x4000, 0x4000, 0x0001)),
        section(b"UNIT", &units),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alow::Restrictions;
    use crate::pud::Pud;
    use sha2::{Digest, Sha256};

    fn sha256(b: &[u8]) -> String {
        Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    /// Spec, "R99-tests.pud": 36 062 bytes with this SHA-256.
    #[test]
    fn r99_tests_matches_the_spec_hash() {
        let b = generated("R99-tests.pud").expect("known map");
        assert_eq!(b.len(), 36_062);
        assert_eq!(
            sha256(&b),
            "ae532b4ec6bbd8836de20793c2b5f136146389e5fdbdbecaae891b41dfb86091"
        );
    }

    /// Spec, "R100-tests.pud": 36 030 bytes with this SHA-256.
    #[test]
    fn r100_tests_matches_the_spec_hash() {
        let b = generated("R100-tests.pud").expect("known map");
        assert_eq!(b.len(), 36_030);
        assert_eq!(
            sha256(&b),
            "009f7bed418ffde11d22aa8d2ad1835ad94b3776aee2f7dd47b85a8bd995eda8"
        );
    }

    /// The sea rows: coast at row 50, shore at 51, water from 52 (spec, "Sea").
    #[test]
    fn r100_tests_has_its_sea_rows() {
        let pud = Pud::parse(&generated("R100-tests.pud").unwrap()).unwrap();
        assert_eq!((pud.width, pud.id_tag, pud.units.len()), (64, 0x7200, 18));
        let sqm = pud.section(b"SQM ").unwrap();
        let at = |y: usize| u16::from_le_bytes([sqm[(y * 64 + 5) * 2], sqm[(y * 64 + 5) * 2 + 1]]);
        assert_eq!((at(49), at(50), at(51), at(52), at(63)), (1, 0x11, 0x82, 0x40, 0x40));
    }

    /// The map loads: `ALOW` is read (its spells word is player 2's 128), `UGRD` after it is passed by, and the
    /// unit and upgrade data say "use the default".
    #[test]
    fn r99_tests_loads_with_the_reading_rule() {
        let pud = Pud::parse(&generated("R99-tests.pud").unwrap()).unwrap();
        assert_eq!((pud.width, pud.id_tag), (64, 0x7199));
        assert_eq!(pud.units.len(), 22);
        assert!(pud.section(b"UGRD").is_none(), "UGRD comes after ALOW and is skipped");
        let r = Restrictions::parse(pud.section(b"ALOW").expect("ALOW read")).unwrap();
        assert_eq!(r.spells_start[2], 128);
        assert_eq!(r.spells_start[0], 0);
        assert_eq!(r.spells_start[4], 0);
        assert_eq!(r.spells_allowed[0], 0x000F_FFFB);
        assert_eq!(pud.start_resources(3), (2000, 1000, 0));
        assert_eq!(pud.start_resources(4), (0, 0, 0));
        let udta = pud.section(b"UDTA").unwrap();
        assert_eq!(u16::from_le_bytes([udta[0], udta[1]]), 1);
    }

    #[test]
    fn names_of_generated_maps() {
        assert!(is_generated_name("R99-tests.pud"));
        assert!(!is_generated_name("Rose Petal BNE.pud"));
        assert!(!is_generated_name("R-tests.pud"));
        assert!(!is_generated_name("R99.pud"));
        assert!(generated("R98-none.pud").is_none());
    }
}
