//! Unit type statistics: the PUD `UDTA` section and the installation's `rez\unitdata.dat` / `rez\unitdato.dat`.
//!
//! Format source: "PUD file format specification" (cade.datamax.bg/war2x/pudspec.html, archived at
//! <http://web.archive.org/web/2020/http://cade.datamax.bg/war2x/pudspec.html>), section 6 "UDTA". The table is a
//! structure of arrays over 110 unit types. `unitdata.dat` was established by data inspection to be the same table
//! without the leading "use default data" word, followed by the 127-word swamp frame table (5948 bytes);
//! `unitdato.dat` stops before that table (5694 bytes).

use thiserror::Error;

pub const UNIT_TYPES: usize = 110;
/// The first 58 unit types have a right-click action.
pub const ACTION_TYPES: usize = 58;

#[derive(Debug, Error)]
#[error("unit data has {got} bytes, need at least {need}")]
pub struct UdtaError {
    got: usize,
    need: usize,
}

/// Unit flags (UDTA "flags" bits).
pub mod flags {
    pub const LAND: u32 = 1 << 0;
    pub const AIR: u32 = 1 << 1;
    pub const EXPLODE_WHEN_KILLED: u32 = 1 << 2;
    pub const SEA: u32 = 1 << 3;
    pub const CRITTER: u32 = 1 << 4;
    pub const BUILDING: u32 = 1 << 5;
    pub const SUBMARINE: u32 = 1 << 6;
    pub const SEES_SUBMARINES: u32 = 1 << 7;
    pub const PEON: u32 = 1 << 8;
    pub const TANKER: u32 = 1 << 9;
    pub const TRANSPORT: u32 = 1 << 10;
    pub const OIL_SOURCE: u32 = 1 << 11;
    pub const GOLD_STORAGE: u32 = 1 << 12;
    pub const GROUND_ATTACK: u32 = 1 << 14;
    pub const UNDEAD: u32 = 1 << 15;
    pub const SHORE_BUILDING: u32 = 1 << 16;
    pub const CASTER: u32 = 1 << 17;
    pub const WOOD_STORAGE: u32 = 1 << 18;
    pub const CAN_ATTACK: u32 = 1 << 19;
    pub const TOWER: u32 = 1 << 20;
    pub const OIL_PATCH: u32 = 1 << 21;
    pub const MINE: u32 = 1 << 22;
    pub const HERO: u32 = 1 << 23;
    pub const OIL_STORAGE: u32 = 1 << 24;
    pub const KILLED_BY_INVISIBILITY: u32 = 1 << 25;
    pub const MAGE_LIKE: u32 = 1 << 26;
    pub const ORGANIC: u32 = 1 << 27;
}

/// Stats of one unit type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitStats {
    pub overlap_frames: u16,
    /// Tiles.
    pub sight: u32,
    pub hit_points: u16,
    pub magic: bool,
    /// In game cycles / 6 (6 = one second at normal speed).
    pub build_time: u8,
    /// Full amounts (the file stores tenths).
    pub gold: u16,
    pub lumber: u16,
    pub oil: u16,
    /// Footprint in tiles.
    pub size: (u16, u16),
    /// Selection box in pixels.
    pub box_size: (u16, u16),
    pub attack_range: u8,
    pub react_range_computer: u8,
    pub react_range_human: u8,
    pub armor: u8,
    pub rect_selectable: bool,
    pub priority: u8,
    pub basic_damage: u8,
    pub piercing_damage: u8,
    pub weapons_upgradable: bool,
    pub armor_upgradable: bool,
    pub missile: u8,
    /// 0 land, 1 air, 2 sea (appearance only).
    pub movement_class: u8,
    /// Dies after `decay_rate * 6` seconds; 0 never.
    pub decay_rate: u8,
    pub annoy_computer: u8,
    /// Right-click action, defined for the first [`ACTION_TYPES`] types.
    pub right_click: u8,
    pub points: u16,
    /// Bit mask: 1 land, 2 sea, 4 air.
    pub can_target: u8,
    pub flags: u32,
}

/// Stats of all [`UNIT_TYPES`] unit types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitTable(pub Vec<UnitStats>);

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> &[u8] {
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        s
    }
    fn u8s(&mut self, n: usize) -> Vec<u8> {
        self.bytes(n).to_vec()
    }
    fn u16s(&mut self, n: usize) -> Vec<u16> {
        self.bytes(n * 2)
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect()
    }
    fn u32s(&mut self, n: usize) -> Vec<u32> {
        self.bytes(n * 4)
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().expect("4 bytes")))
            .collect()
    }
}

/// Bytes of the table after the "use default data" word, up to and including the flags.
pub const TABLE_LEN: usize = 5694;

impl UnitTable {
    /// Parses a PUD `UDTA` section. Returns `None` for the table if the section says "use default data".
    pub fn from_udta(section: &[u8]) -> Result<Option<Self>, UdtaError> {
        if section.len() < 2 + TABLE_LEN {
            return Err(UdtaError {
                got: section.len(),
                need: 2 + TABLE_LEN,
            });
        }
        if u16::from_le_bytes([section[0], section[1]]) != 0 {
            return Ok(None);
        }
        Self::from_table(&section[2..]).map(Some)
    }

    /// Parses `rez\unitdata.dat` or `rez\unitdato.dat` (the table without the leading word).
    pub fn from_table(d: &[u8]) -> Result<Self, UdtaError> {
        if d.len() < TABLE_LEN {
            return Err(UdtaError {
                got: d.len(),
                need: TABLE_LEN,
            });
        }
        const N: usize = UNIT_TYPES;
        let mut r = Reader { d, p: 0 };
        let overlap = r.u16s(N);
        r.bytes(508 * 2); // obsolete per-tileset frames
        let sight = r.u32s(N);
        let hp = r.u16s(N);
        let magic = r.u8s(N);
        let build = r.u8s(N);
        let gold = r.u8s(N);
        let lumber = r.u8s(N);
        let oil = r.u8s(N);
        let size = r.u16s(N * 2);
        let boxs = r.u16s(N * 2);
        let range = r.u8s(N);
        let react_c = r.u8s(N);
        let react_h = r.u8s(N);
        let armor = r.u8s(N);
        let selectable = r.u8s(N);
        let priority = r.u8s(N);
        let basic = r.u8s(N);
        let pierce = r.u8s(N);
        let weap_up = r.u8s(N);
        let armor_up = r.u8s(N);
        let missile = r.u8s(N);
        let class = r.u8s(N);
        let decay = r.u8s(N);
        let annoy = r.u8s(N);
        let action = r.u8s(ACTION_TYPES);
        let points = r.u16s(N);
        let target = r.u8s(N);
        let fl = r.u32s(N);
        debug_assert_eq!(r.p, TABLE_LEN);
        Ok(Self(
            (0..N)
                .map(|i| UnitStats {
                    overlap_frames: overlap[i],
                    sight: sight[i],
                    hit_points: hp[i],
                    magic: magic[i] != 0,
                    build_time: build[i],
                    gold: gold[i] as u16 * 10,
                    lumber: lumber[i] as u16 * 10,
                    oil: oil[i] as u16 * 10,
                    size: (size[i * 2], size[i * 2 + 1]),
                    box_size: (boxs[i * 2], boxs[i * 2 + 1]),
                    attack_range: range[i],
                    react_range_computer: react_c[i],
                    react_range_human: react_h[i],
                    armor: armor[i],
                    rect_selectable: selectable[i] != 0,
                    priority: priority[i],
                    basic_damage: basic[i],
                    piercing_damage: pierce[i],
                    weapons_upgradable: weap_up[i] != 0,
                    armor_upgradable: armor_up[i] != 0,
                    missile: missile[i],
                    movement_class: class[i],
                    decay_rate: decay[i],
                    annoy_computer: annoy[i],
                    right_click: action.get(i).copied().unwrap_or(0),
                    points: points[i],
                    can_target: target[i],
                    flags: fl[i],
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
        // Hit points of type 1 live after: overlap (220) + obsolete (1016) + sight (440) + type 0's hp (2).
        let mut d = vec![0u8; TABLE_LEN];
        d[220 + 1016 + 440 + 2..][..2].copy_from_slice(&60u16.to_le_bytes());
        // Flags are the last 440 bytes.
        d[TABLE_LEN - 440..][..4].copy_from_slice(&(flags::LAND | flags::CAN_ATTACK).to_le_bytes());
        let t = UnitTable::from_table(&d).unwrap();
        assert_eq!(t.0[1].hit_points, 60);
        assert_eq!(t.0[0].flags, flags::LAND | flags::CAN_ATTACK);
    }

    #[test]
    fn default_flag_means_no_table() {
        let mut s = vec![0u8; 2 + TABLE_LEN];
        s[0] = 1;
        assert!(UnitTable::from_udta(&s).unwrap().is_none());
    }
}
