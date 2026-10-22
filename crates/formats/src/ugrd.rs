//! Upgrade costs and effects: the PUD `UGRD` section and the installation's `rez\upgrades.dat`.
//!
//! Format source: "PUD file format specification" (cade.datamax.bg/war2x/pudspec.html, archived at
//! <http://web.archive.org/web/2020/http://cade.datamax.bg/war2x/pudspec.html>), section 8 "UGRD" and appendix B
//! "Upgrade types". The table is a structure of arrays over 52 upgrades: u8 time, then u16 gold, lumber, oil, icon and
//! group, then u32 affect flags. `upgrades.dat` was established by data inspection to be the same table without the
//! leading "use default data" word (780 bytes). Unlike the unit table, costs are stored as full amounts: sword 1 reads
//! 800 gold and ship cannon 1 reads 700 gold, 100 lumber, 1000 oil, matching the game's prices.

use thiserror::Error;

pub const UPGRADES: usize = 52;

/// Bytes of the table after the "use default data" word.
pub const TABLE_LEN: usize = UPGRADES * (1 + 5 * 2 + 4);

/// Upgrade names in table order (PUD specification, appendix B).
pub const NAMES: [&str; UPGRADES] = [
    "sword 1",
    "sword 2",
    "axe 1",
    "axe 2",
    "arrow 1",
    "arrow 2",
    "spear 1",
    "spear 2",
    "human shield 1",
    "human shield 2",
    "orc shield 1",
    "orc shield 2",
    "human ship cannon 1",
    "human ship cannon 2",
    "orc ship cannon 1",
    "orc ship cannon 2",
    "human ship armor 1",
    "human ship armor 2",
    "orc ship armor 1",
    "orc ship armor 2",
    "catapult 1",
    "catapult 2",
    "ballista 1",
    "ballista 2",
    "train rangers",
    "longbow",
    "ranger scouting",
    "ranger marksmanship",
    "train berserkers",
    "lighter axes",
    "berserker scouting",
    "berserker regeneration",
    "train ogre-mages",
    "train paladins",
    "holy vision",
    "healing",
    "exorcism",
    "flame shield",
    "fireball",
    "slow",
    "invisibility",
    "polymorph",
    "blizzard",
    "eye of kilrogg",
    "bloodlust",
    "raise dead",
    "death coil",
    "whirlwind",
    "haste",
    "unholy armor",
    "runes",
    "death and decay",
];

#[derive(Debug, Error)]
#[error("upgrade data has {got} bytes, need at least {need}")]
pub struct UgrdError {
    got: usize,
    need: usize,
}

/// Cost and effect of one upgrade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Upgrade {
    /// Research time, in the file's units.
    pub time: u8,
    pub gold: u16,
    pub lumber: u16,
    pub oil: u16,
    /// Index of the button icon.
    pub icon: u16,
    /// The group of units the upgrade applies to.
    pub group: u16,
    /// What the upgrade gives (bit mask).
    pub affect: u32,
}

/// All [`UPGRADES`] upgrades.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeTable(pub Vec<Upgrade>);

impl UpgradeTable {
    /// Parses a PUD `UGRD` section. Returns `None` for the table if the section is empty or says "use default data"
    /// (any word but 0; the rest of the section is then skipped), as for [`crate::udta::UnitTable::from_udta`].
    pub fn from_ugrd(section: &[u8]) -> Result<Option<Self>, UgrdError> {
        if section.is_empty() || (section.len() >= 2 && section[..2] != [0, 0]) {
            return Ok(None);
        }
        if section.len() < 2 + TABLE_LEN {
            return Err(UgrdError {
                got: section.len(),
                need: 2 + TABLE_LEN,
            });
        }
        Self::from_table(&section[2..]).map(Some)
    }

    /// Parses `rez\upgrades.dat` (the table without the leading word).
    pub fn from_table(d: &[u8]) -> Result<Self, UgrdError> {
        if d.len() < TABLE_LEN {
            return Err(UgrdError {
                got: d.len(),
                need: TABLE_LEN,
            });
        }
        const N: usize = UPGRADES;
        let u16_at = |array: usize, i: usize| {
            let o = N + array * N * 2 + i * 2;
            u16::from_le_bytes([d[o], d[o + 1]])
        };
        let affect_at = |i: usize| {
            let o = N + 5 * N * 2 + i * 4;
            u32::from_le_bytes(d[o..o + 4].try_into().expect("4 bytes"))
        };
        Ok(Self(
            (0..N)
                .map(|i| Upgrade {
                    time: d[i],
                    gold: u16_at(0, i),
                    lumber: u16_at(1, i),
                    oil: u16_at(2, i),
                    icon: u16_at(3, i),
                    group: u16_at(4, i),
                    affect: affect_at(i),
                })
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_offsets() {
        assert_eq!(TABLE_LEN, 780);
        let mut d = vec![0u8; TABLE_LEN];
        d[1] = 250; // time of upgrade 1
        d[52 + 2..][..2].copy_from_slice(&2400u16.to_le_bytes()); // gold of upgrade 1
        d[52 + 104 * 2 + 102..][..2].copy_from_slice(&3000u16.to_le_bytes()); // oil of upgrade 51
        d[52 + 104 * 4..][..2].copy_from_slice(&7u16.to_le_bytes()); // group of upgrade 0
        d[TABLE_LEN - 4..].copy_from_slice(&0x8_0000u32.to_le_bytes()); // affect of upgrade 51
        let t = UpgradeTable::from_table(&d).unwrap();
        assert_eq!((t.0[1].time, t.0[1].gold), (250, 2400));
        assert_eq!(t.0[51].oil, 3000);
        assert_eq!(t.0[0].group, 7);
        assert_eq!(t.0[51].affect, 0x8_0000);
        assert_eq!(
            t.0[0],
            Upgrade {
                group: 7,
                ..Default::default()
            }
        );
    }

    #[test]
    fn default_flag_means_no_table() {
        let mut s = vec![0u8; 2 + TABLE_LEN];
        assert!(UpgradeTable::from_ugrd(&s).unwrap().is_some());
        s[0] = 1;
        assert!(UpgradeTable::from_ugrd(&s).unwrap().is_none());
        // The word is not 0: the rest is skipped, whatever its length (setup spec, "`UDTA` and `UGRD` lengths").
        assert!(UpgradeTable::from_ugrd(&s[..100]).unwrap().is_none());
        s[0] = 0;
        assert!(UpgradeTable::from_ugrd(&s[..100]).is_err());
    }
}
