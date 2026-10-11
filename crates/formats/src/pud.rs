//! PUD map/scenario files.
//!
//! Format source: Daniel Lemberg, "PUD file format" (1997), mirrored in Wargus's documentation
//! (<https://github.com/Wargus/wargus>, `doc/pud-format.html`). A PUD is a list of sections, each a 4-byte tag, a
//! little-endian u32 length and the payload. Only the sections an engine needs are decoded; every section stays
//! available raw through [`Pud::section`].

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PudError {
    #[error("truncated section header at {0}")]
    Truncated(usize),
    #[error("not a PUD (TYPE section missing or wrong)")]
    NotPud,
    #[error("section {tag} has {got} bytes, expected {want}")]
    BadLength { tag: String, got: usize, want: usize },
    #[error("missing section {0}")]
    Missing(&'static str),
}

/// The four terrains (the `ERA`/`ERAX` section).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Era {
    Forest,
    Winter,
    Wasteland,
    Swamp,
}

impl Era {
    fn from_u16(v: u16) -> Self {
        // Values above 3 appear in some maps; the public spec says they read as forest.
        match v {
            1 => Era::Winter,
            2 => Era::Wasteland,
            3 => Era::Swamp,
            _ => Era::Forest,
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

#[derive(Clone, Debug)]
pub struct Pud {
    sections: Vec<([u8; 4], Vec<u8>)>,
    pub version: u16,
    pub width: u16,
    pub height: u16,
    pub era: Era,
    /// Tile numbers, row-major.
    pub tiles: Vec<u16>,
    pub units: Vec<PlacedUnit>,
}

fn u16s(b: &[u8]) -> Vec<u16> {
    b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

impl Pud {
    pub fn parse(data: &[u8]) -> Result<Self, PudError> {
        let mut sections = Vec::new();
        let mut i = 0;
        while i < data.len() {
            if i + 8 > data.len() {
                return Err(PudError::Truncated(i));
            }
            let tag: [u8; 4] = data[i..i + 4].try_into().expect("4 bytes");
            let len = u32::from_le_bytes(data[i + 4..i + 8].try_into().expect("4 bytes")) as usize;
            let end = (i + 8 + len).min(data.len());
            sections.push((tag, data[i + 8..end].to_vec()));
            i = i + 8 + len;
        }
        let find = |t: &[u8; 4]| sections.iter().find(|(tag, _)| tag == t).map(|(_, d)| d.as_slice());
        match find(b"TYPE") {
            Some(t) if t.starts_with(b"WAR2 MAP") => {}
            _ => return Err(PudError::NotPud),
        }
        let version = find(b"VER ").map(|v| u16s(v)[0]).unwrap_or(0);
        let dim = find(b"DIM ").ok_or(PudError::Missing("DIM "))?;
        let d = u16s(dim);
        let (width, height) = (d[0], d[1]);
        // ERAX (expansion) overrides ERA when present.
        let era = find(b"ERAX")
            .or_else(|| find(b"ERA "))
            .map(|e| Era::from_u16(u16s(e)[0]))
            .unwrap_or(Era::Forest);
        let mtxm = find(b"MTXM").ok_or(PudError::Missing("MTXM"))?;
        let want = width as usize * height as usize * 2;
        if mtxm.len() != want {
            return Err(PudError::BadLength {
                tag: "MTXM".into(),
                got: mtxm.len(),
                want,
            });
        }
        let tiles = u16s(mtxm);
        let units = find(b"UNIT")
            .unwrap_or(&[])
            .chunks_exact(8)
            .map(|u| PlacedUnit {
                x: u16::from_le_bytes([u[0], u[1]]),
                y: u16::from_le_bytes([u[2], u[3]]),
                kind: u[4],
                owner: u[5],
                data: u16::from_le_bytes([u[6], u[7]]),
            })
            .collect();
        Ok(Self {
            sections,
            version,
            width,
            height,
            era,
            tiles,
            units,
        })
    }

    /// The raw payload of a section, e.g. `b"UDTA"`.
    pub fn section(&self, tag: &[u8; 4]) -> Option<&[u8]> {
        self.sections.iter().find(|(t, _)| t == tag).map(|(_, d)| d.as_slice())
    }

    pub fn section_tags(&self) -> impl Iterator<Item = &[u8; 4]> {
        self.sections.iter().map(|(t, _)| t)
    }

    /// Player races (`SIDE`): 0 human, 1 orc, 2 neutral.
    pub fn sides(&self) -> &[u8] {
        self.section(b"SIDE").unwrap_or(&[])
    }

    /// Player controllers (`OWNR`): e.g. 3 nobody, 4 computer, 5 human, 6 rescue (passive), 7 rescue (active).
    pub fn owners(&self) -> &[u8] {
        self.section(b"OWNR").unwrap_or(&[])
    }

    pub fn description(&self) -> String {
        let d = self.section(b"DESC").unwrap_or(&[]);
        let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
        String::from_utf8_lossy(&d[..end]).into_owned()
    }

    /// The movement words (`SQM `), one per tile, if the section is present and complete.
    pub fn movement(&self) -> Option<Vec<u16>> {
        self.tile_words(b"SQM ")
    }

    /// The region map (`REGM`), one value per tile, if the section is present and complete.
    pub fn regions(&self) -> Option<Vec<u16>> {
        self.tile_words(b"REGM")
    }

    fn tile_words(&self, tag: &[u8; 4]) -> Option<Vec<u16>> {
        let w = u16s(self.section(tag)?);
        (w.len() >= self.width as usize * self.height as usize).then_some(w)
    }

    /// Starting resources for player `p` (`SGLD`, `SLBR`, `SOIL`), as (gold, lumber, oil).
    pub fn start_resources(&self, p: usize) -> (u16, u16, u16) {
        let get = |t: &[u8; 4]| self.section(t).map(u16s).and_then(|v| v.get(p).copied()).unwrap_or(0);
        (get(b"SGLD"), get(b"SLBR"), get(b"SOIL"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut v = tag.to_vec();
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }

    fn synthetic() -> Vec<u8> {
        let mut d = section(b"TYPE", b"WAR2 MAP\0\0\x0a\xff\0\0\0\0");
        d.extend(section(b"VER ", &0x13u16.to_le_bytes()));
        d.extend(section(b"ERA ", &2u16.to_le_bytes()));
        d.extend(section(b"DIM ", &[2, 0, 1, 0]));
        d.extend(section(b"MTXM", &[0x50, 0x00, 0x23, 0x01]));
        d.extend(section(b"UNIT", &[1, 0, 0, 0, 0x5c, 15, 4, 0]));
        d
    }

    #[test]
    fn parses_core_sections() {
        let p = Pud::parse(&synthetic()).unwrap();
        assert_eq!((p.width, p.height, p.version), (2, 1, 0x13));
        assert_eq!(p.era, Era::Wasteland);
        assert_eq!(p.tiles, vec![0x0050, 0x0123]);
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
    fn rejects_wrong_tile_count() {
        let mut d = synthetic();
        d.extend(section(b"MTXM", &[0; 2]));
        // The first MTXM wins, so build one where it's short instead.
        let mut bad = section(b"TYPE", b"WAR2 MAP\0\0");
        bad.extend(section(b"DIM ", &[2, 0, 2, 0]));
        bad.extend(section(b"MTXM", &[0; 4]));
        assert!(matches!(Pud::parse(&bad), Err(PudError::BadLength { .. })));
    }
}
