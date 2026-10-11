//! String tables (`*.tbl`: menu text, campaign titles and briefings, help).
//!
//! Layout established by data inspection, and the same as the StarCraft `.tbl` string tables described on the
//! Staredit Network wiki ("TBL"): a u16 string count, then one u16 offset per string (from the start of the file),
//! then the strings, each ending in a NUL byte. All 68 tables in the installation follow it, with the first string
//! right after the offsets.
//!
//! Strings are kept as bytes. Besides tab and newline they contain bytes 0x01-0x04 and 0x10, which look like inline
//! formatting codes (help text wraps key names in 0x04 ... 0x01, suggesting "highlight" / "normal"); their meaning is
//! not established. No byte above 0x7F occurs in the shipped tables, so [`StringTable::text`] reads bytes as
//! Latin-1.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TblError {
    #[error("string table truncated")]
    Truncated,
    #[error("string {0} starts outside the table or has no terminator")]
    BadString(usize),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StringTable(pub Vec<Vec<u8>>);

impl StringTable {
    pub fn parse(d: &[u8]) -> Result<Self, TblError> {
        let rd16 = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
        let count = rd16(0).ok_or(TblError::Truncated)?;
        let mut strings = Vec::with_capacity(count);
        for i in 0..count {
            let start = rd16(2 + i * 2).ok_or(TblError::Truncated)?;
            let rest = d.get(start..).ok_or(TblError::BadString(i))?;
            let len = rest.iter().position(|&b| b == 0).ok_or(TblError::BadString(i))?;
            strings.push(rest[..len].to_vec());
        }
        Ok(Self(strings))
    }

    /// String `i` as text, reading each byte as Latin-1. Formatting bytes are kept.
    pub fn text(&self, i: usize) -> Option<String> {
        self.0.get(i).map(|s| s.iter().map(|&b| b as char).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_offsets_and_strings() {
        // Two strings; the second offset points at the first one's terminator, so it is empty.
        let mut d = vec![2, 0, 6, 0, 8, 0];
        d.extend(b"Hi\0");
        let t = StringTable::parse(&d).unwrap();
        assert_eq!(t.0, [b"Hi".to_vec(), Vec::new()]);
        assert_eq!(t.text(0).as_deref(), Some("Hi"));
        assert_eq!(t.text(2), None);
    }

    #[test]
    fn rejects_bad_tables() {
        assert_eq!(StringTable::parse(&[]), Err(TblError::Truncated));
        assert_eq!(StringTable::parse(&[1, 0]), Err(TblError::Truncated));
        assert_eq!(StringTable::parse(&[1, 0, 9, 0]), Err(TblError::BadString(0)));
        assert_eq!(StringTable::parse(&[1, 0, 4, 0, b'x']), Err(TblError::BadString(0)));
    }
}
