//! PUD map/scenario files.
//!
//! Format source: Daniel Lemberg, "PUD file format" (1997), mirrored in Wargus's documentation
//! (<https://github.com/Wargus/wargus>, `doc/pud-format.html`). A PUD is a list of sections, each a 4-byte tag, a
//! little-endian u32 length and the payload. Section layouts, lengths and unit type ids are that text's.
//!
//! Which sections are read, in what order, and the checks on them follow `docs/specs/game/setup.md` ("Reading
//! sections", "Section effects" and the header and body tables). Only the sections those tables accept are kept;
//! [`Pud::section`] gives their payloads.
//!
//! Our readings where the spec leaves room:
//! - The body table is read from the start of the file again, with the header's sections skipped as unknown (the
//!   spec's default for "Where the body reading starts").
//! - A `UNIT` length that isn't a multiple of 8 fails the load (the spec's default for "Short `UNIT` section").
//! - The `UDTA` and `UGRD` length rules are the table in the setup spec ("`UDTA` and `UGRD` lengths", round 101): a
//!   section of length 0, or of at least 2 bytes with a "use default" word that isn't 0, is accepted (the default
//!   data is used); with the word 0 it must hold the word and the whole table (5696 bytes for `UDTA`, 782 for
//!   `UGRD`), and a longer one is accepted with its extra bytes skipped (shipped maps hold `UDTA` at 5696 or 5950
//!   bytes). A section of length 1, or one with the word 0 and fewer bytes than the table, fails the load.
//! - The tile sections are copied as they are into a size × size map; tiles past the end of a short section are 0.
//!   A `DIM ` size of 0 or above 128 fails the load, since the tile maps hold at most 128 × 128 words.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PudError {
    #[error("not a PUD (TYPE section missing or wrong)")]
    NotPud,
    #[error("section {tag} has {got} bytes, which its rule doesn't allow")]
    BadLength { tag: String, got: usize },
    #[error("unsupported PUD version {0:#x}")]
    BadVersion(u16),
    #[error("map size {0} not supported")]
    BadSize(u16),
    #[error("missing section {0}")]
    Missing(String),
}

/// The four terrains (the `ERA`/`ERAX` section).
#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Era {
    Forest,
    Winter,
    Wasteland,
    Swamp,
}

impl Era {
    /// The tileset is the value's low two bits (setup spec, "Section effects").
    pub fn from_u16(v: u16) -> Self {
        match v & 3 {
            0 => Era::Forest,
            1 => Era::Winter,
            2 => Era::Wasteland,
            _ => Era::Swamp,
        }
    }
}

/// A unit placed on the map (`UNIT` section, 8 bytes each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlacedUnit {
    pub x: u16,
    pub y: u16,
    pub kind: u8,
    pub owner: u8,
    /// Resource amount / 2500 for gold mines and oil patches, otherwise 0 = passive, 1 = active (AI).
    pub data: u16,
}

/// What a table entry accepts as the section length.
#[derive(Clone, Copy, Debug)]
enum Len {
    Any,
    Exactly(usize),
    AtMost(usize),
    ZeroOr(usize),
    MultipleOf(usize),
    /// A unit or upgrade data section of this full length (the "use default" word and the table): the rules of the
    /// setup spec's length table, which look at the word.
    Data(usize),
}

impl Len {
    fn allows(self, payload: &[u8]) -> bool {
        let n = payload.len();
        match self {
            Len::Any => true,
            Len::Exactly(m) => n == m,
            Len::AtMost(m) => n <= m,
            Len::ZeroOr(m) => n == 0 || n == m,
            Len::MultipleOf(m) => n.is_multiple_of(m),
            Len::Data(m) => n == 0 || (n >= 2 && (payload[0] != 0 || payload[1] != 0 || n >= m)),
        }
    }
}

struct Entry {
    tag: &'static [u8; 4],
    required: bool,
    len: Len,
}

const fn entry(tag: &'static [u8; 4], required: bool, len: Len) -> Entry {
    Entry { tag, required, len }
}

/// The header table, read first (setup spec, Data). `TYPE`'s length isn't checked.
const HEADER: [Entry; 16] = [
    entry(b"TYPE", true, Len::Any),
    entry(b"VER ", true, Len::Exactly(2)),
    entry(b"DESC", true, Len::Exactly(32)),
    entry(b"OWNR", true, Len::Exactly(16)),
    entry(b"ERA ", true, Len::Exactly(2)),
    entry(b"ERAX", false, Len::Exactly(2)),
    entry(b"SIGN", false, Len::Exactly(4)),
    entry(b"DIM ", true, Len::Exactly(4)),
    entry(b"UDTA", false, Len::Data(2 + crate::udta::TABLE_LEN)),
    entry(b"UGRD", false, Len::Data(2 + crate::ugrd::TABLE_LEN)),
    entry(b"ALOW", false, Len::ZeroOr(crate::alow::LEN)),
    entry(b"SIDE", true, Len::Exactly(16)),
    entry(b"SGLD", true, Len::Exactly(32)),
    entry(b"SLBR", true, Len::Exactly(32)),
    entry(b"SOIL", true, Len::Exactly(32)),
    entry(b"AIPL", false, Len::Exactly(16)),
];

/// The body table, read after the header.
const BODY: [Entry; 5] = [
    entry(b"MTXM", true, Len::AtMost(32768)),
    entry(b"SQM ", true, Len::AtMost(32768)),
    entry(b"OILM", true, Len::Any),
    entry(b"REGM", true, Len::AtMost(32768)),
    entry(b"UNIT", true, Len::MultipleOf(8)),
];

/// The largest map side the tile maps hold.
pub const MAX_SIZE: u16 = 128;

/// A section a table entry read: its tag and payload.
type Read<'a> = ([u8; 4], &'a [u8]);

/// Reads one table over the whole file ("Reading sections"): returns the sections its entries read, in table order.
fn read_table<'a>(data: &'a [u8], table: &[Entry]) -> Result<Vec<Read<'a>>, PudError> {
    let mut out = Vec::new();
    let (mut pos, mut i) = (0, 0);
    while pos < table.len() && i + 8 <= data.len() {
        let tag: [u8; 4] = data[i..i + 4].try_into().expect("4 bytes");
        let len = u32::from_le_bytes(data[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
        if len > data.len() - (i + 8) {
            // A section longer than the bytes left ends the reading, as if the file ended there.
            break;
        }
        let payload = &data[i + 8..i + 8 + len];
        i += 8 + len;
        let Some(k) = table[pos..].iter().position(|e| *e.tag == tag).map(|k| pos + k) else {
            continue;
        };
        if let Some(e) = table[pos..k].iter().find(|e| e.required) {
            return Err(PudError::Missing(tag_name(e.tag)));
        }
        if !table[k].len.allows(payload) {
            return Err(PudError::BadLength {
                tag: tag_name(&tag),
                got: len,
            });
        }
        out.push((tag, payload));
        pos = k + 1;
    }
    // The file ended first: only one optional entry may be left.
    let left = &table[pos..];
    match left {
        [] => Ok(out),
        [e] if !e.required => Ok(out),
        _ => Err(PudError::Missing(tag_name(
            left.iter().find(|e| e.required).unwrap_or(&left[0]).tag,
        ))),
    }
}

fn tag_name(t: &[u8; 4]) -> String {
    String::from_utf8_lossy(t).into_owned()
}

#[derive(Clone, Debug)]
pub struct Pud {
    /// The sections the two tables read, header ones first.
    sections: Vec<([u8; 4], Vec<u8>)>,
    pub version: u16,
    /// The map's id tag, bytes 12 to 15 of `TYPE` (0 when the section is shorter).
    pub id_tag: u32,
    /// The map is always square: both are the second word of `DIM `.
    pub width: u16,
    pub height: u16,
    pub era: Era,
    /// Tile numbers, row-major, size × size.
    pub tiles: Vec<u16>,
    pub units: Vec<PlacedUnit>,
}

fn u16s(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

impl Pud {
    pub fn parse(data: &[u8]) -> Result<Self, PudError> {
        let header = read_table(data, &HEADER)?;
        let find = |t: &[u8; 4]| header.iter().find(|(tag, _)| tag == t).map(|(_, d)| *d);
        let ty = find(b"TYPE").unwrap_or_default();
        if !ty.starts_with(b"WAR2 MAP\0") {
            return Err(PudError::NotPud);
        }
        let id_tag = ty
            .get(12..16)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().expect("4 bytes")));
        let version = u16s(find(b"VER ").unwrap_or_default()).first().copied().unwrap_or(0);
        if !matches!(version, 0x11 | 0x13) {
            return Err(PudError::BadVersion(version));
        }
        // ERAX comes later in the table, so it wins when both are present.
        let era = find(b"ERAX")
            .or_else(|| find(b"ERA "))
            .map_or(Era::Forest, |e| Era::from_u16(u16s(e)[0]));
        // Only the second word (the height) is kept; the map is square.
        let size = u16s(find(b"DIM ").unwrap_or_default()).get(1).copied().unwrap_or(0);
        if size == 0 || size > MAX_SIZE {
            return Err(PudError::BadSize(size));
        }
        let body = read_table(data, &BODY)?;
        let mut sections: Vec<([u8; 4], Vec<u8>)> = header.iter().map(|(t, d)| (*t, d.to_vec())).collect();
        sections.extend(body.iter().map(|(t, d)| (*t, d.to_vec())));
        let mut pud = Self {
            sections,
            version,
            id_tag,
            width: size,
            height: size,
            era,
            tiles: Vec::new(),
            units: Vec::new(),
        };
        pud.tiles = pud.tile_words(b"MTXM");
        pud.units = pud
            .section(b"UNIT")
            .unwrap_or_default()
            .chunks_exact(8)
            .map(|u| PlacedUnit {
                x: u16::from_le_bytes([u[0], u[1]]),
                y: u16::from_le_bytes([u[2], u[3]]),
                kind: u[4],
                owner: u[5],
                data: u16::from_le_bytes([u[6], u[7]]),
            })
            .collect();
        Ok(pud)
    }

    /// The payload of a section the tables read, e.g. `b"UDTA"`. Sections the tables skipped (unknown tags, second
    /// copies, optional entries passed over) aren't kept.
    pub fn section(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.sections.iter().find(|(t, _)| t == tag).map(|(_, d)| d.as_slice())
    }

    pub fn section_tags(&self) -> impl Iterator<Item = &[u8; 4]> {
        self.sections.iter().map(|(t, _)| t)
    }

    /// Player races (`SIDE`) as the map gives them: 0 human, 1 orc, anything else neutral.
    pub fn sides(&self) -> &[u8] {
        self.section(b"SIDE").unwrap_or(&[])
    }

    /// Computer script numbers (`AIPL`), one per player; empty without the section.
    pub fn ai_scripts(&self) -> &[u8] {
        self.section(b"AIPL").unwrap_or(&[])
    }

    /// Owner codes (`OWNR`) as the map gives them: e.g. 2 passive computer, 3 nobody, 4 computer, 5 human,
    /// 6 rescue (passive), 7 rescue (active).
    pub fn owners(&self) -> &[u8] {
        self.section(b"OWNR").unwrap_or(&[])
    }

    pub fn description(&self) -> String {
        let d = self.section(b"DESC").unwrap_or(&[]);
        let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
        String::from_utf8_lossy(&d[..end]).into_owned()
    }

    /// The movement words (`SQM `), one per tile.
    pub fn movement(&self) -> Option<Vec<u16>> {
        self.section(b"SQM ").is_some().then(|| self.tile_words(b"SQM "))
    }

    /// The region map (`REGM`), one value per tile.
    pub fn regions(&self) -> Option<Vec<u16>> {
        self.section(b"REGM").is_some().then(|| self.tile_words(b"REGM"))
    }

    /// A tile section copied as it is into a size × size map, as rows of the map's size; tiles past its end are 0.
    fn tile_words(&self, tag: &[u8; 4]) -> Vec<u16> {
        let n = self.width as usize * self.height as usize;
        let mut w = u16s(self.section(tag).unwrap_or_default());
        w.resize(n, 0);
        w
    }

    /// Starting resources for player `p` (`SGLD`, `SLBR`, `SOIL`), as (gold, lumber, oil).
    pub fn start_resources(&self, p: usize) -> (u16, u16, u16) {
        let get = |t: &[u8; 4]| self.section(t).map(u16s).and_then(|v| v.get(p).copied()).unwrap_or(0);
        (get(b"SGLD"), get(b"SLBR"), get(b"SOIL"))
    }
}

/// Builds PUD files from parts, for tests here and in other crates. The data is synthetic.
pub mod build {
    /// One section: the tag, the u32 length and the payload.
    pub fn section(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }

    fn words(w: &[u16]) -> Vec<u8> {
        w.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// A small map, built up section by section.
    #[derive(Clone, Debug)]
    pub struct Map {
        pub size: u16,
        pub era: u16,
        pub owners: [u8; 16],
        pub sides: [u8; 16],
        pub gold: [u16; 16],
        pub lumber: [u16; 16],
        pub oil: [u16; 16],
        pub ai: Option<[u8; 16]>,
        /// Movement words, one per tile; all 0 when empty.
        pub movement: Vec<u16>,
        /// Region values, one per tile; all 0 when empty.
        pub regions: Vec<u16>,
        /// (x, y, type, owner, data).
        pub units: Vec<(u16, u16, u8, u8, u16)>,
    }

    impl Default for Map {
        fn default() -> Self {
            Map {
                size: 32,
                era: 0,
                owners: [3; 16],
                sides: [0; 16],
                gold: [2000; 16],
                lumber: [1000; 16],
                oil: [1000; 16],
                ai: None,
                movement: Vec::new(),
                regions: Vec::new(),
                units: Vec::new(),
            }
        }
    }

    impl Map {
        /// The header sections in table order.
        pub fn header(&self) -> Vec<Vec<u8>> {
            let mut desc = [0u8; 32];
            desc[..4].copy_from_slice(b"test");
            let mut v = vec![
                section(b"TYPE", b"WAR2 MAP\0\0\x0a\xff\x01\x02\x03\x04"),
                section(b"VER ", &words(&[0x13])),
                section(b"DESC", &desc),
                section(b"OWNR", &self.owners),
                section(b"ERA ", &words(&[self.era])),
                section(b"DIM ", &words(&[self.size, self.size])),
                section(b"SIDE", &self.sides),
                section(b"SGLD", &words(&self.gold)),
                section(b"SLBR", &words(&self.lumber)),
                section(b"SOIL", &words(&self.oil)),
            ];
            if let Some(ai) = self.ai {
                v.push(section(b"AIPL", &ai));
            }
            v
        }

        /// The body sections in table order.
        pub fn body(&self) -> Vec<Vec<u8>> {
            let n = self.size as usize * self.size as usize;
            let tiles = |w: &[u16]| if w.is_empty() { vec![0; n] } else { w.to_vec() };
            let units: Vec<u8> = self
                .units
                .iter()
                .flat_map(|&(x, y, k, o, d)| {
                    let mut r = Vec::with_capacity(8);
                    r.extend(x.to_le_bytes());
                    r.extend(y.to_le_bytes());
                    r.extend([k, o]);
                    r.extend(d.to_le_bytes());
                    r
                })
                .collect();
            vec![
                section(b"MTXM", &words(&vec![0x50; n])),
                section(b"SQM ", &words(&tiles(&self.movement))),
                section(b"OILM", &vec![0; n]),
                section(b"REGM", &words(&tiles(&self.regions))),
                section(b"UNIT", &units),
            ]
        }

        pub fn bytes(&self) -> Vec<u8> {
            self.header().into_iter().chain(self.body()).flatten().collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build::{Map, section};
    use super::*;

    fn join(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.concat()
    }

    /// The header sections of a default map with `f` applied, then its body.
    fn with_header(f: impl FnOnce(&mut Vec<Vec<u8>>)) -> Result<Pud, PudError> {
        let m = Map::default();
        let mut h = m.header();
        f(&mut h);
        Pud::parse(&join(&[join(&h), join(&m.body())]))
    }

    fn pos(h: &[Vec<u8>], tag: &[u8; 4]) -> usize {
        h.iter().position(|s| &s[..4] == tag).unwrap()
    }

    #[test]
    fn parses_core_sections() {
        let mut m = Map {
            size: 2,
            era: 2,
            ..Default::default()
        };
        m.units.push((1, 0, 0x5c, 15, 4));
        let p = Pud::parse(&m.bytes()).unwrap();
        assert_eq!((p.width, p.height, p.version, p.id_tag), (2, 2, 0x13, 0x0403_0201));
        assert_eq!(p.era, Era::Wasteland);
        assert_eq!(p.tiles, vec![0x50; 4]);
        assert_eq!(
            p.units,
            vec![PlacedUnit {
                x: 1,
                y: 0,
                kind: 0x5c,
                owner: 15,
                data: 4
            }]
        );
    }

    #[test]
    fn minimal_header_loads_with_defaults() {
        // TYPE VER DESC OWNR ERA DIM SIDE SGLD SLBR SOIL, no optional sections.
        let p = with_header(|_| {}).unwrap();
        assert_eq!(p.era, Era::Forest);
        assert!(p.section(b"UDTA").is_none() && p.section(b"UGRD").is_none());
        assert!(p.ai_scripts().is_empty());
    }

    #[test]
    fn erax_wins_and_era_uses_the_low_two_bits() {
        let p = with_header(|h| {
            let at = pos(h, b"ERA ") + 1;
            h.insert(at, section(b"ERAX", &2u16.to_le_bytes()));
            h[at - 1] = section(b"ERA ", &1u16.to_le_bytes());
        })
        .unwrap();
        assert_eq!(p.era, Era::Wasteland);
        let p = with_header(|h| {
            let at = pos(h, b"ERA ");
            h[at] = section(b"ERA ", &6u16.to_le_bytes());
        })
        .unwrap();
        assert_eq!(p.era, Era::Wasteland);
        assert_eq!(Era::from_u16(7), Era::Swamp);
    }

    #[test]
    fn dim_keeps_the_second_word() {
        let m = Map::default();
        let mut h = m.header();
        let at = pos(&h, b"DIM ");
        h[at] = section(b"DIM ", &[64, 0, 32, 0]);
        let p = Pud::parse(&join(&[join(&h), join(&m.body())])).unwrap();
        assert_eq!((p.width, p.height, p.tiles.len()), (32, 32, 32 * 32));
    }

    #[test]
    fn only_versions_0x11_and_0x13_load() {
        for (v, ok) in [(0x11u16, true), (0x13, true), (0x12, false)] {
            let r = with_header(|h| {
                let at = pos(h, b"VER ");
                h[at] = section(b"VER ", &v.to_le_bytes());
            });
            assert_eq!(r.is_ok(), ok, "version {v:#x}");
        }
    }

    /// The setup spec's length table for `UDTA` and `UGRD` ("`UDTA` and `UGRD` lengths", round 101).
    #[test]
    fn udta_and_ugrd_lengths_follow_the_spec_table() {
        use crate::{udta::UnitTable, ugrd::UpgradeTable};
        let full = |tag: &[u8; 4], table: usize| (tag.to_vec(), 2 + table);
        for (tag, full_len) in [
            full(b"UDTA", crate::udta::TABLE_LEN),
            full(b"UGRD", crate::ugrd::TABLE_LEN),
        ] {
            let tag: [u8; 4] = tag.try_into().unwrap();
            let load = |payload: Vec<u8>| {
                with_header(|h| {
                    let at = pos(h, b"SIDE");
                    h.insert(at, section(&tag, &payload));
                })
            };
            let word = |w: u16, len: usize| {
                let mut v = vec![0u8; len];
                v[..2].copy_from_slice(&w.to_le_bytes());
                v
            };
            // Length 0: accepted, default data.
            let p = load(vec![]).unwrap();
            assert_eq!(p.section(&tag).map(<[u8]>::len), Some(0));
            // Length 2 or more, the word not 0: accepted, default data, whatever the length.
            for len in [2, 100, full_len - 1, full_len + 40] {
                assert!(load(word(1, len)).is_ok(), "{tag:?} word 1 length {len}");
            }
            // The word 0: the full length and longer are accepted; shorter is refused.
            assert!(load(word(0, full_len)).is_ok());
            assert!(load(word(0, full_len + 254)).is_ok());
            assert!(matches!(load(word(0, full_len - 1)), Err(PudError::BadLength { .. })));
            assert!(matches!(load(word(0, 2)), Err(PudError::BadLength { .. })));
            // Length 1: refused.
            assert!(matches!(load(vec![0]), Err(PudError::BadLength { got: 1, .. })));
            assert!(matches!(load(vec![1]), Err(PudError::BadLength { got: 1, .. })));
        }
        // The readers agree: empty and non-zero-word sections mean the default data; a longer table is read.
        assert_eq!(UnitTable::from_udta(&[]).unwrap(), None);
        assert_eq!(UnitTable::from_udta(&[1, 0, 9]).unwrap(), None);
        assert!(UnitTable::from_udta(&[0]).is_err());
        assert!(UnitTable::from_udta(&vec![0; 2 + crate::udta::TABLE_LEN - 1]).is_err());
        assert!(
            UnitTable::from_udta(&vec![0; 2 + crate::udta::TABLE_LEN + 254])
                .unwrap()
                .is_some()
        );
        assert_eq!(UpgradeTable::from_ugrd(&[]).unwrap(), None);
        assert_eq!(UpgradeTable::from_ugrd(&[1, 0, 9]).unwrap(), None);
        assert!(
            UpgradeTable::from_ugrd(&vec![0; 2 + crate::ugrd::TABLE_LEN + 5])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn a_passed_optional_entry_is_never_read() {
        // SIDE read first skips UDTA, UGRD and ALOW; the later UDTA is unknown by then.
        let mut udta = vec![0u8; 2 + crate::udta::TABLE_LEN];
        udta[0] = 0;
        let p = with_header(|h| {
            let at = pos(h, b"SIDE") + 1;
            h.insert(at, section(b"UDTA", &udta));
        })
        .unwrap();
        assert!(p.section(b"UDTA").is_none());
        assert_eq!(p.sides().len(), 16);
    }

    #[test]
    fn skipping_a_required_entry_fails() {
        let r = with_header(|h| {
            h.remove(pos(h, b"SGLD"));
        });
        assert!(matches!(r, Err(PudError::Missing(t)) if t == "SGLD"));
    }

    #[test]
    fn file_may_end_with_only_aipl_left() {
        let m = Map::default();
        // The header alone, ending right after SOIL: the header reads, but the body is missing.
        let h = join(&m.header());
        assert!(read_table(&h, &HEADER).is_ok());
        assert!(matches!(Pud::parse(&h), Err(PudError::Missing(t)) if t == "MTXM"));
        // Ending before SOIL leaves two entries.
        let short = join(&m.header()[..9]);
        assert!(read_table(&short, &HEADER).is_err());
    }

    #[test]
    fn unknown_sections_are_skipped_anywhere() {
        let m = Map::default();
        let mut parts = m.header();
        parts.insert(0, section(b"ABCD", &[1, 2, 3]));
        parts.insert(5, section(b"ABCD", &[]));
        parts.extend(m.body());
        parts.insert(13, section(b"ABCD", &[9; 7]));
        parts.push(section(b"ABCD", &[0; 4]));
        let p = Pud::parse(&join(&parts)).unwrap();
        assert!(p.section(b"ABCD").is_none());
        assert_eq!(p.width, 32);
    }

    /// X9 (`game/setup.md`, "Where the body reading starts"; vector G14): the body table reads from the start of the
    /// file again, skipping the header's sections as unknown, so body sections that come before the header ones are
    /// still read.
    #[test]
    fn body_sections_before_the_header_sections_are_read() {
        let mut m = Map::default();
        m.units.push((3, 4, 0x5c, 15, 4));
        let mut parts = m.body();
        parts.extend(m.header());
        let p = Pud::parse(&join(&parts)).unwrap();
        let normal = Pud::parse(&m.bytes()).unwrap();
        assert_eq!(p.units, normal.units);
        assert_eq!(p.units.len(), 1);
        assert_eq!(
            (p.width, p.height, p.tiles),
            (normal.width, normal.height, normal.tiles)
        );
    }

    #[test]
    fn wrong_lengths_fail() {
        assert!(matches!(
            with_header(|h| {
                let at = pos(h, b"OWNR");
                h[at] = section(b"OWNR", &[3; 15]);
            }),
            Err(PudError::BadLength { .. })
        ));
        // ALOW may be empty or 384 bytes.
        for (n, ok) in [(0, true), (384, true), (100, false)] {
            let r = with_header(|h| {
                let at = pos(h, b"SIDE");
                h.insert(at, section(b"ALOW", &vec![0; n]));
            });
            assert_eq!(r.is_ok(), ok, "ALOW length {n}");
        }
        // A UNIT section that isn't a multiple of 8.
        let m = Map::default();
        let mut b = m.body();
        b[4] = section(b"UNIT", &[0; 12]);
        assert!(Pud::parse(&join(&[join(&m.header()), join(&b)])).is_err());
    }

    #[test]
    fn an_overlong_section_ends_the_file() {
        let m = Map::default();
        let mut bytes = m.bytes();
        bytes.extend(b"ABCD");
        bytes.extend(1000u32.to_le_bytes());
        assert!(Pud::parse(&bytes).is_ok());
        // Cut inside UNIT's payload: UNIT is required, so the load fails.
        let mut m = Map::default();
        m.units.push((0, 0, 0, 0, 0));
        let bytes = m.bytes();
        assert!(Pud::parse(&bytes[..bytes.len() - 2]).is_err());
    }

    #[test]
    fn type_must_name_a_war2_map() {
        let r = with_header(|h| h[0] = section(b"TYPE", b"WAR2 MAX\0\0\0\0\0\0\0\0"));
        assert!(matches!(r, Err(PudError::NotPud)));
    }

    #[test]
    fn short_tile_sections_are_padded() {
        let m = Map::default();
        let mut b = m.body();
        b[0] = section(b"MTXM", &[0x23, 0x01]);
        let p = Pud::parse(&join(&[join(&m.header()), join(&b)])).unwrap();
        assert_eq!((p.tiles[0], p.tiles[1], p.tiles.len()), (0x123, 0, 32 * 32));
    }
}
