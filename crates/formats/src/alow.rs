//! What each player may build, cast and research: the PUD `ALOW` section.
//!
//! Format source: "PUD file format specification" (cade.datamax.bg/war2x/pudspec.html, archived at
//! <http://web.archive.org/web/2020/http://cade.datamax.bg/war2x/pudspec.html>), section 7 "ALOW". Six arrays of 16
//! u32 bit masks, one per player slot: units and buildings allowed, spells known at the start, spells allowed to
//! research, spells being researched, upgrades allowed, upgrades being acquired. The section is optional; without
//! it everything is allowed. The unit and spell bit orders below are the specification's; it gives no bit order for
//! upgrades.

use thiserror::Error;

/// Player slots (8 players, 7 unusable, neutral).
pub const PLAYERS: usize = 16;
/// Section length: 6 arrays of 16 u32.
pub const LEN: usize = 6 * PLAYERS * 4;

#[derive(Debug, Error)]
#[error("ALOW section has {0} bytes, need {LEN}")]
pub struct AlowError(usize);

/// Bits of [`Restrictions::units`]. Each bit covers the human and the orc version.
pub mod unit_bits {
    pub const FOOTMAN_GRUNT: u32 = 1 << 0;
    pub const PEASANT_PEON: u32 = 1 << 1;
    pub const BALLISTA_CATAPULT: u32 = 1 << 2;
    pub const KNIGHT_OGRE: u32 = 1 << 3;
    pub const ARCHER_AXE_THROWER: u32 = 1 << 4;
    pub const MAGE_DEATH_KNIGHT: u32 = 1 << 5;
    pub const TANKER: u32 = 1 << 6;
    pub const DESTROYER: u32 = 1 << 7;
    pub const TRANSPORT: u32 = 1 << 8;
    pub const BATTLESHIP_JUGGERNAUGHT: u32 = 1 << 9;
    pub const SUBMARINE_TURTLE: u32 = 1 << 10;
    pub const FLYING_MACHINE_ZEPPELIN: u32 = 1 << 11;
    pub const GRYPHON_DRAGON: u32 = 1 << 12;
    pub const DEMOLITION_SQUAD_SAPPER: u32 = 1 << 14;
    pub const AVIARY_ROOST: u32 = 1 << 15;
    pub const FARM: u32 = 1 << 16;
    pub const BARRACKS: u32 = 1 << 17;
    pub const LUMBER_MILL: u32 = 1 << 18;
    pub const STABLES_OGRE_MOUND: u32 = 1 << 19;
    pub const MAGE_TOWER_TEMPLE: u32 = 1 << 20;
    pub const FOUNDRY: u32 = 1 << 21;
    pub const REFINERY: u32 = 1 << 22;
    pub const INVENTOR_ALCHEMIST: u32 = 1 << 23;
    pub const CHURCH_ALTAR: u32 = 1 << 24;
    pub const TOWER: u32 = 1 << 25;
    pub const TOWN_HALL: u32 = 1 << 26;
    pub const KEEP_STRONGHOLD: u32 = 1 << 27;
    pub const CASTLE_FORTRESS: u32 = 1 << 28;
    pub const BLACKSMITH: u32 = 1 << 29;
    pub const SHIPYARD: u32 = 1 << 30;
}

/// Bits of the spell masks.
pub mod spell_bits {
    pub const HOLY_VISION: u32 = 1 << 0;
    pub const HEALING: u32 = 1 << 1;
    pub const EXORCISM: u32 = 1 << 3;
    pub const FLAME_SHIELD: u32 = 1 << 4;
    pub const FIREBALL: u32 = 1 << 5;
    pub const SLOW: u32 = 1 << 6;
    pub const INVISIBILITY: u32 = 1 << 7;
    pub const POLYMORPH: u32 = 1 << 8;
    pub const BLIZZARD: u32 = 1 << 9;
    pub const EYE_OF_KILROGG: u32 = 1 << 10;
    pub const BLOODLUST: u32 = 1 << 11;
    pub const RAISE_DEAD: u32 = 1 << 13;
    pub const DEATH_COIL: u32 = 1 << 14;
    pub const WHIRLWIND: u32 = 1 << 15;
    pub const HASTE: u32 = 1 << 16;
    pub const UNHOLY_ARMOR: u32 = 1 << 17;
    pub const RUNES: u32 = 1 << 18;
    pub const DEATH_AND_DECAY: u32 = 1 << 19;
}

/// The decoded `ALOW` section: one bit mask per player slot in each array.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Restrictions {
    pub units: [u32; PLAYERS],
    pub spells_start: [u32; PLAYERS],
    pub spells_allowed: [u32; PLAYERS],
    pub spells_researching: [u32; PLAYERS],
    pub upgrades_allowed: [u32; PLAYERS],
    pub upgrades_researching: [u32; PLAYERS],
}

impl Restrictions {
    pub fn parse(section: &[u8]) -> Result<Self, AlowError> {
        if section.len() < LEN {
            return Err(AlowError(section.len()));
        }
        let array = |n: usize| -> [u32; PLAYERS] {
            std::array::from_fn(|p| {
                let o = (n * PLAYERS + p) * 4;
                u32::from_le_bytes(section[o..o + 4].try_into().expect("4 bytes"))
            })
        };
        Ok(Self {
            units: array(0),
            spells_start: array(1),
            spells_allowed: array(2),
            spells_researching: array(3),
            upgrades_allowed: array(4),
            upgrades_researching: array(5),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrays_in_order() {
        let mut d = vec![0u8; LEN];
        let mut put = |array: usize, player: usize, v: u32| {
            let o = (array * PLAYERS + player) * 4;
            d[o..o + 4].copy_from_slice(&v.to_le_bytes());
        };
        put(0, 1, unit_bits::FARM | unit_bits::PEASANT_PEON);
        put(2, 0, spell_bits::HEALING);
        put(5, 15, 0xdead_beef);
        let r = Restrictions::parse(&d).unwrap();
        assert_eq!(r.units[1], unit_bits::FARM | unit_bits::PEASANT_PEON);
        assert_eq!(r.units[0], 0);
        assert_eq!(r.spells_allowed[0], spell_bits::HEALING);
        assert_eq!(r.upgrades_researching[15], 0xdead_beef);
        assert!(Restrictions::parse(&d[..LEN - 1]).is_err());
    }
}
