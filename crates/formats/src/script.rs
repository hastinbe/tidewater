//! Unit and missile animation scripts: the installation's `Rez\script.bin`.
//!
//! Format source: the reviewed spec `docs/specs/core/animation-scripts.md` ("File layout of `script.bin`" and
//! "Instruction set"). All values are little-endian. Bytes 0 to 219 hold 110 u16 offsets, one per unit type in PUD
//! numbering, of that type's animation table; the missile kinds' offsets follow from byte 220. The number of animation
//! entries per table (7, animations 0 to 6) and of missile kinds (29, bytes 220 to 277) were established by data
//! inspection: the first unit table starts at byte 284 and its seven entries are followed directly by script code.

use thiserror::Error;

pub const UNIT_TYPES: usize = 110;
pub const MISSILE_KINDS: usize = 29;
pub const ANIMATIONS: usize = 7;
const MISSILE_BASE: usize = UNIT_TYPES * 2;

/// Animation numbers.
pub mod anim {
    pub const NONE: u8 = 0;
    pub const DEATH: u8 = 1;
    pub const STILL: u8 = 2;
    pub const MOVE: u8 = 3;
    pub const ATTACK: u8 = 4;
    pub const WORK_A: u8 = 5;
    pub const WORK_B: u8 = 6;
}

#[derive(Debug, Error)]
pub enum ScriptError {
    #[error("script file is {0} bytes, too short for its offset tables")]
    TooShort(usize),
    #[error("{what} {index} points to offset {offset}, past the end of the file")]
    BadTable {
        what: &'static str,
        index: usize,
        offset: usize,
    },
}

/// One decoded instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    StepDone,
    Wait(u8),
    Halt,
    Jump(u16),
    Frame(u8),
    Move(i8),
    MoveFrame(i8, u8),
    WaitHasteSlow(u8),
    WaitHaste(u8),
    WaitSlow(u8),
    Sound,
    Strike,
    WaitCarry(u8),
    Move2(i8),
    Move2Frame(i8, u8),
    Neutral,
    /// Opcodes 16 to 255: one byte, no effect.
    Nop(u8),
}

impl Op {
    /// Bytes this instruction takes, opcode included.
    pub fn size(self) -> usize {
        match self {
            Op::StepDone | Op::Halt | Op::Sound | Op::Strike | Op::Neutral | Op::Nop(_) => 1,
            Op::Jump(_) | Op::MoveFrame(..) | Op::Move2Frame(..) => 3,
            _ => 2,
        }
    }

    /// Whether running this instruction ends the cycle's run.
    pub fn sets_wait(self) -> bool {
        matches!(
            self,
            Op::StepDone
                | Op::Wait(_)
                | Op::Halt
                | Op::WaitHasteSlow(_)
                | Op::WaitHaste(_)
                | Op::WaitSlow(_)
                | Op::WaitCarry(_)
        )
    }
}

#[derive(Clone, Debug)]
pub struct Scripts {
    bytes: Vec<u8>,
}

impl Scripts {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, ScriptError> {
        let s = Self { bytes };
        if s.bytes.len() < MISSILE_BASE + MISSILE_KINDS * 2 {
            return Err(ScriptError::TooShort(s.bytes.len()));
        }
        for i in 0..UNIT_TYPES {
            let t = s.u16(i * 2) as usize;
            if t + ANIMATIONS * 2 > s.bytes.len() {
                return Err(ScriptError::BadTable {
                    what: "unit type",
                    index: i,
                    offset: t,
                });
            }
        }
        for i in 0..MISSILE_KINDS {
            let t = s.u16(MISSILE_BASE + i * 2) as usize;
            if t >= s.bytes.len() {
                return Err(ScriptError::BadTable {
                    what: "missile kind",
                    index: i,
                    offset: t,
                });
            }
        }
        Ok(s)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    /// Offset of the script for a unit type's animation, or `None` for an unknown type or animation.
    pub fn unit(&self, kind: u8, animation: u8) -> Option<u16> {
        if kind as usize >= UNIT_TYPES || animation as usize >= ANIMATIONS {
            return None;
        }
        let table = self.u16(kind as usize * 2) as usize;
        Some(self.u16(table + animation as usize * 2))
    }

    /// Offset of a missile kind's table.
    pub fn missile_table(&self, kind: u8) -> Option<u16> {
        ((kind as usize) < MISSILE_KINDS).then(|| self.u16(MISSILE_BASE + kind as usize * 2))
    }

    /// Decodes the instruction at `pc`, or `None` if it runs off the end of the file.
    pub fn decode(&self, pc: u16) -> Option<Op> {
        decode(&self.bytes, pc as usize)
    }
}

/// Decodes the instruction at `pc` of `code`.
pub fn decode(code: &[u8], pc: usize) -> Option<Op> {
    let b = |i: usize| code.get(pc + i).copied();
    let s = |i: usize| b(i).map(|v| v as i8);
    Some(match b(0)? {
        0 => Op::StepDone,
        1 => Op::Wait(b(1)?),
        2 => Op::Halt,
        3 => Op::Jump(u16::from_le_bytes([b(1)?, b(2)?])),
        4 => Op::Frame(b(1)?),
        5 => Op::Move(s(1)?),
        6 => Op::MoveFrame(s(1)?, b(2)?),
        7 => Op::WaitHasteSlow(b(1)?),
        8 => Op::WaitHaste(b(1)?),
        9 => Op::WaitSlow(b(1)?),
        10 => Op::Sound,
        11 => Op::Strike,
        12 => Op::WaitCarry(b(1)?),
        13 => Op::Move2(s(1)?),
        14 => Op::Move2Frame(s(1)?, b(2)?),
        15 => Op::Neutral,
        n => Op::Nop(n),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_every_opcode() {
        let code = [0, 1, 5, 2, 3, 0x34, 0x12, 4, 9, 5, 0xfd, 6, 3, 7, 20];
        let mut pc = 0;
        let mut ops = vec![];
        while let Some(op) = decode(&code, pc) {
            pc += op.size();
            ops.push(op);
        }
        assert_eq!(
            ops,
            [
                Op::StepDone,
                Op::Wait(5),
                Op::Halt,
                Op::Jump(0x1234),
                Op::Frame(9),
                Op::Move(-3),
                Op::MoveFrame(3, 7),
                Op::Nop(20)
            ]
        );
        assert_eq!(decode(&[6, 1], 0), None);
    }

    #[test]
    fn looks_up_unit_and_missile_tables() {
        let mut bytes = vec![0u8; 400];
        // Every unit type and missile kind points at one table at 300.
        for i in 0..UNIT_TYPES + MISSILE_KINDS {
            bytes[i * 2..i * 2 + 2].copy_from_slice(&300u16.to_le_bytes());
        }
        bytes[306..308].copy_from_slice(&350u16.to_le_bytes());
        let s = Scripts::parse(bytes).unwrap();
        assert_eq!(s.unit(5, anim::MOVE), Some(350));
        assert_eq!(s.unit(5, 7), None);
        assert_eq!(s.unit(110, 0), None);
        assert_eq!(s.missile_table(28), Some(300));
        assert!(Scripts::parse(vec![0; 100]).is_err());
    }
}
