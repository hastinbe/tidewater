//! Computer player scripts: the installation's `Rez\ai.bin`.
//!
//! Format source: the reviewed spec `docs/specs/ai/ai-scripts.md` ("Which script a player runs" and "Running a
//! script"). The file starts with 128 little-endian u16 offsets, one per script number, of the script's record. A
//! record is the build list's offset (u16), the second list's offset (u16), then instructions. All offsets are from
//! the start of the file and all values are little-endian.

use thiserror::Error;

/// Script numbers the header has room for.
pub const SCRIPTS: usize = 128;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("AI script file is {0} bytes, too short for its header")]
    TooShort(usize),
}

/// One script's record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Record {
    pub build_list: u16,
    pub second_list: u16,
    /// Offset of the first instruction.
    pub start: u16,
}

/// The whole file, kept as bytes: scripts jump to file offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiFile {
    pub data: Vec<u8>,
}

impl AiFile {
    pub fn parse(data: Vec<u8>) -> Result<Self, AiError> {
        if data.len() < SCRIPTS * 2 {
            return Err(AiError::TooShort(data.len()));
        }
        Ok(Self { data })
    }

    /// Script `n`'s record, if the header names one inside the file.
    pub fn record(&self, n: u8) -> Option<Record> {
        if n as usize >= SCRIPTS {
            return None;
        }
        let at = self.u16(n as usize * 2)? as usize;
        Some(Record {
            build_list: self.u16(at)?,
            second_list: self.u16(at + 2)?,
            start: u16::try_from(at + 4).ok()?,
        })
    }

    pub fn u8(&self, at: usize) -> Option<u8> {
        self.data.get(at).copied()
    }

    pub fn u16(&self, at: usize) -> Option<u16> {
        Some(u16::from_le_bytes(self.data.get(at..at + 2)?.try_into().ok()?))
    }

    pub fn u32(&self, at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(self.data.get(at..at + 4)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_come_from_the_header() {
        let mut d = vec![0u8; 256];
        d[2..4].copy_from_slice(&256u16.to_le_bytes());
        d.extend([0x10, 0x01, 0x20, 0x01, 2, 0, 0, 0, 0]);
        let f = AiFile::parse(d).unwrap();
        assert_eq!(
            f.record(1),
            Some(Record {
                build_list: 0x110,
                second_list: 0x120,
                start: 260
            })
        );
        assert_eq!(f.u8(260), Some(2));
        assert_eq!(f.record(128), None);
        assert!(AiFile::parse(vec![0; 10]).is_err());
    }
}
