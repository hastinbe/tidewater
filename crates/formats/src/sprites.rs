//! Which `Art\Unit\...\*.grp` sheet draws each unit type.
//!
//! Established by data inspection: frame 0 of every named sheet was rendered (`twtool grp-sheet`) and matched by eye
//! against the unit type list in the public PUD specification. Findings:
//!
//! - Unit sheets hold 5 facings (N, NE, E, SE, S) per animation step; the other three are mirrored.
//! - Building sheets hold 2 frames: 0 complete, 1 under construction. Sheets with a `1`/`2` suffix (`oplat`, `ref`,
//!   `ship`, `found`) are the construction site and the finished building.
//! - Era variants are separate files with a prefix and sometimes a shorter stem, e.g. `invent.grp` / `s_inv.grp` /
//!   `x_inv.grp`. The prefixes are `s_` winter, `l_` wasteland and `x_` swamp (the expansion's tileset). Checked by
//!   rendering each variant with every era's palette next to maps of that era: `s_` sheets show snow only under the
//!   winter palette; `l_` sheets (farm, lumber mill, oil platform, gold mine) swap grass for the wasteland's bare
//!   ground and draw water in the wasteland's dark blue; `x_` sheets add vines, dark rock and the swamp's green
//!   water.
//! - Not every sheet has every variant: wasteland has the fewest (`l_farm`, `l_lmill`, `l_oplat*`, `l_sub`,
//!   `l_mine`, `l_patch`, `l_gate`, `l_dwall`, `l_blddth`). A missing variant falls back to the base sheet.
//! - `Other\rock.grp` (with `s_rock`, `x_rock`) is the runestone; `Other\rune.grp` is the small rune marker of the
//!   Runes spell, not a building.
//! - Heroes, paladins, ogre-mages, rangers and berserkers have no sheet of their own: probing several thousand
//!   candidate names found none, and every unnamed archive block shaped like a sheet turned out to be something
//!   else. They use their base unit's sheet.
//! - Walls (types 0x67, 0x68) are map tiles, not sprites.
//! - Remains: `Other\death.grp` draws dead bodies (type 0x69) with 5 facings per step: human body from frame 0, orc
//!   body from 5, bones 10 to 29, ship wreck from 30. `Other\bldgdth.grp` (era variants `blddth`) draws the ruins
//!   (0x6A to 0x6D): land ruin frames 0 and 1, water ruin frames 2 and 3. Found by probing names and rendering
//!   the sheets; the frames line up with the corpses spec's decay stages.

use crate::pud::Era;

/// (race directory, sheet stem, stem used by era-prefixed variants if different).
type Sheet = (&'static str, &'static str, Option<&'static str>);

const H: &str = "Human";
const O: &str = "Orc";
const X: &str = "Other";

fn sheet(kind: u8) -> Option<Sheet> {
    Some(match kind {
        0x00 | 0x2e => (H, "grunt", None),
        0x01 | 0x19 | 0x2f => (O, "grunt", None),
        0x02 | 0x10 => (H, "peon", None),
        0x03 | 0x11 => (O, "peon", None),
        0x04 => (H, "catapult", None),
        0x05 => (O, "catapult", None),
        0x06 | 0x0c | 0x2c | 0x32 | 0x34 => (H, "knight", None),
        0x07 | 0x0d | 0x17 | 0x31 => (O, "knight", None),
        0x08 | 0x12 | 0x14 => (H, "spear", None),
        0x09 | 0x13 | 0x35 => (O, "spear", None),
        0x0a | 0x18 => (H, "wizard", None),
        0x0b | 0x15 | 0x33 => (O, "dknight", None),
        0x0e => (H, "dwarves", None),
        0x0f => (O, "goblins", None),
        0x1a => (H, "tanker", None),
        0x1b => (O, "tanker", None),
        0x1c => (H, "transp", None),
        0x1d => (O, "transp", None),
        0x1e => (H, "destroy", None),
        0x1f => (O, "destroy", None),
        0x20 => (H, "battlshp", None),
        0x21 => (O, "battlshp", None),
        0x23 | 0x2b => (O, "dragon", None),
        0x26 => (H, "sub", None),
        0x27 => (O, "sub", None),
        0x28 => (H, "orn", None),
        0x29 => (O, "zep", None),
        0x16 | 0x2a => (H, "griffon", None),
        0x2d => (O, "eyeofkil", None),
        0x37 => (O, "skeleton", None),
        0x38 => ("Monster", "demon", None),
        0x3a => (H, "farm", None),
        0x3b => (O, "farm", None),
        0x3c => (H, "barr", None),
        0x3d => (O, "barr", None),
        0x3e => (H, "church", None),
        0x3f => (O, "temple", None),
        0x40 => (H, "tower", None),
        0x41 => (O, "tower", None),
        0x42 => (H, "stable", None),
        0x43 => (O, "ogrecamp", Some("ogre")),
        0x44 => (H, "invent", Some("inv")),
        0x45 => (O, "invent", Some("inv")),
        0x46 => (H, "aviary", None),
        0x47 => (O, "roost", None),
        0x48 => (H, "ship2", None),
        0x49 => (O, "ship2", None),
        0x4a => (H, "thall", None),
        0x4b => (O, "thall", None),
        0x4c => (H, "lmill", None),
        0x4d => (O, "lmill", None),
        0x4e => (H, "found2", None),
        0x4f => (O, "found2", None),
        0x50 => (H, "wtower", None),
        0x51 => (O, "dtower", None),
        0x52 => (H, "black", None),
        0x53 => (O, "black", None),
        0x54 => (H, "ref2", None),
        0x55 => (O, "ref2", None),
        0x56 => (H, "oplat2", None),
        0x57 => (O, "oplat2", None),
        0x58 => (H, "keep", None),
        0x59 => (O, "keep", None),
        0x5a => (H, "castle", None),
        0x5b => (O, "blakrock", Some("blakrk")),
        0x5c => (X, "mine", None),
        0x5d => (X, "patch", None),
        0x5e => (H, "startloc", None),
        0x5f => (O, "startloc", None),
        0x60 => (H, "towera", None),
        0x61 => (O, "towera", None),
        0x62 => (H, "towerc", None),
        0x63 => (O, "towerc", None),
        0x64 => (X, "vcircle", Some("vcir")),
        0x65 => (X, "gate", None),
        0x66 => (X, "rock", None),
        0x69 => (X, "death", None),
        0x6a..=0x6d => (X, "bldgdth", Some("blddth")),
        _ => return None,
    })
}

fn era_prefix(era: Era) -> Option<&'static str> {
    match era {
        Era::Forest => None,
        Era::Winter => Some("s_"),
        Era::Wasteland => Some("l_"),
        Era::Swamp => Some("x_"),
    }
}

/// Candidate archive names for a unit type's sheet in an era, most specific first. The caller uses the first that
/// exists. Empty for types without a sprite.
pub fn unit_sheet_candidates(kind: u8, era: Era) -> Vec<String> {
    if kind == 0x39 {
        // The critter's look follows the terrain.
        let stem = match era {
            Era::Forest => "sheep",
            Era::Winter => "seal",
            Era::Wasteland => "boar",
            Era::Swamp => "hellhog",
        };
        return vec![format!("Art\\Unit\\Monster\\{stem}.grp")];
    }
    let Some((dir, stem, short)) = sheet(kind) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    if let Some(p) = era_prefix(era) {
        v.push(format!("Art\\Unit\\{dir}\\{p}{}.grp", short.unwrap_or(stem)));
        if short.is_some() {
            v.push(format!("Art\\Unit\\{dir}\\{p}{stem}.grp"));
        }
    }
    v.push(format!("Art\\Unit\\{dir}\\{stem}.grp"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn era_variants_come_first() {
        assert_eq!(
            unit_sheet_candidates(0x44, Era::Winter),
            [
                "Art\\Unit\\Human\\s_inv.grp",
                "Art\\Unit\\Human\\s_invent.grp",
                "Art\\Unit\\Human\\invent.grp"
            ]
        );
        assert_eq!(
            unit_sheet_candidates(0x00, Era::Forest),
            ["Art\\Unit\\Human\\grunt.grp"]
        );
        assert!(unit_sheet_candidates(0x67, Era::Forest).is_empty());
    }

    #[test]
    fn wasteland_and_swamp_prefixes() {
        assert_eq!(
            unit_sheet_candidates(0x3a, Era::Wasteland),
            ["Art\\Unit\\Human\\l_farm.grp", "Art\\Unit\\Human\\farm.grp"]
        );
        assert_eq!(
            unit_sheet_candidates(0x66, Era::Swamp),
            ["Art\\Unit\\Other\\x_rock.grp", "Art\\Unit\\Other\\rock.grp"]
        );
    }
}
