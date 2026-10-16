//! Tech tree: who makes, researches or becomes what, the training and building-upgrade requirements, and the default
//! upgrade data. Spec: `docs/specs/economy/tech-tree.md`. Unit and upgrade numbers are those of the public PUD
//! specification (appendices A and B).
//!
//! The job mechanics (paying, progress, ending) are in `economy.rs`; this module holds the tables and the checks a
//! command runs when it arrives.

use tidewater_formats::ugrd::{UPGRADES, Upgrade, UpgradeTable};

use crate::world::{Player, World};

/// Unit types by their PUD numbers.
pub mod unit {
    pub const KNIGHT: u8 = 6;
    pub const OGRE: u8 = 7;
    pub const ARCHER: u8 = 8;
    pub const AXE_THROWER: u8 = 9;
    pub const PALADIN: u8 = 12;
    pub const OGRE_MAGE: u8 = 13;
    pub const RANGER: u8 = 18;
    pub const BERSERKER: u8 = 19;
    /// Unit types 0 to 57 are the ones a building can train.
    pub const TRAINABLE: u8 = 58;
}

/// Building types by their PUD numbers.
pub mod building {
    pub const HUMAN_BARRACKS: u8 = 60;
    pub const ORC_BARRACKS: u8 = 61;
    pub const CHURCH: u8 = 62;
    pub const ALTAR: u8 = 63;
    pub const HUMAN_SCOUT_TOWER: u8 = 64;
    pub const ORC_SCOUT_TOWER: u8 = 65;
    pub const STABLES: u8 = 66;
    pub const OGRE_MOUND: u8 = 67;
    pub const INVENTOR: u8 = 68;
    pub const ALCHEMIST: u8 = 69;
    pub const AVIARY: u8 = 70;
    pub const ROOST: u8 = 71;
    pub const HUMAN_SHIPYARD: u8 = 72;
    pub const ORC_SHIPYARD: u8 = 73;
    pub const TOWN_HALL: u8 = 74;
    pub const GREAT_HALL: u8 = 75;
    pub const ELVEN_LUMBER_MILL: u8 = 76;
    pub const TROLL_LUMBER_MILL: u8 = 77;
    pub const HUMAN_FOUNDRY: u8 = 78;
    pub const ORC_FOUNDRY: u8 = 79;
    pub const MAGE_TOWER: u8 = 80;
    pub const TEMPLE: u8 = 81;
    pub const HUMAN_BLACKSMITH: u8 = 82;
    pub const ORC_BLACKSMITH: u8 = 83;
    pub const KEEP: u8 = 88;
    pub const STRONGHOLD: u8 = 89;
    pub const CASTLE: u8 = 90;
    pub const FORTRESS: u8 = 91;
    pub const HUMAN_GUARD_TOWER: u8 = 96;
    pub const ORC_GUARD_TOWER: u8 = 97;
    pub const HUMAN_CANNON_TOWER: u8 = 98;
    pub const ORC_CANNON_TOWER: u8 = 99;
}

/// Level lines kept per player: upgrade groups 0 to 10 ("Upgrade lines and their effects").
pub const LINES: usize = 11;
/// The group of ranger and berserker training (the training requirements read its level).
pub const RANGER_TRAINING_LINE: usize = 7;
/// Upgrade numbers with a level check.
const LEVEL_CHECKED: u8 = 24;
/// Spell bits of the holy vision and eye of Kilrogg grants.
pub const HOLY_VISION_BIT: u32 = 1 << 0;
pub const EYE_OF_KILROGG_BIT: u32 = 1 << 10;
/// The "affects" value of the paladin and ogre-mage upgrades (spells word).
pub const PALADIN_BIT: u32 = 1 << 20;
/// The "affects" value of ranger and berserker training (upgrades word).
pub const RANGER_TRAINING_BIT: u32 = 1 << 16;

/// The building type that trains unit type `k`, or `None` for types no building trains ("Who makes what").
fn producer(k: u8) -> Option<&'static [u8]> {
    use building::*;
    Some(match k {
        0 | 4 | 6 | 8 | 12 | 18 => &[HUMAN_BARRACKS],
        1 | 5 | 7 | 9 | 13 | 19 => &[ORC_BARRACKS],
        2 => &[TOWN_HALL],
        3 => &[GREAT_HALL],
        10 => &[MAGE_TOWER],
        11 => &[TEMPLE],
        14 | 40 => &[INVENTOR],
        15 | 41 => &[ALCHEMIST],
        26 | 28 | 30 | 32 | 38 => &[HUMAN_SHIPYARD],
        27 | 29 | 31 | 33 | 39 => &[ORC_SHIPYARD],
        42 => &[AVIARY],
        43 => &[ROOST],
        _ => return None,
    })
}

/// Whether a building of type `b` trains unit type `k`. A keep or castle counts as a town hall, a stronghold or
/// fortress as a great hall.
pub fn trains(b: u8, k: u8) -> bool {
    use building::*;
    let b = match b {
        KEEP | CASTLE => TOWN_HALL,
        STRONGHOLD | FORTRESS => GREAT_HALL,
        b => b,
    };
    k < unit::TRAINABLE && producer(k).is_some_and(|p| p.contains(&b))
}

/// The building type that researches upgrade number `n` ("Who researches what").
pub fn researcher(n: u8) -> Option<u8> {
    use building::*;
    Some(match n {
        0 | 1 | 8 | 9 | 22 | 23 => HUMAN_BLACKSMITH,
        2 | 3 | 10 | 11 | 20 | 21 => ORC_BLACKSMITH,
        4 | 5 | 24..=27 => ELVEN_LUMBER_MILL,
        6 | 7 | 28..=31 => TROLL_LUMBER_MILL,
        12 | 13 | 16 | 17 => HUMAN_FOUNDRY,
        14 | 15 | 18 | 19 => ORC_FOUNDRY,
        33..=36 => CHURCH,
        32 | 43 | 44 | 50 => ALTAR,
        37..=42 => MAGE_TOWER,
        45..=49 | 51 => TEMPLE,
        _ => return None,
    })
}

/// The level line (an upgrade group) that the level check of upgrade `n` (0 to 23) reads. The lines are fixed by
/// upgrade number, not read from the upgrade data: weapons, missiles, shields, ship cannons, ship armour, siege.
pub(crate) fn fixed_line(n: u8) -> usize {
    [1, 0, 2, 3, 4, 6][(n / 4) as usize]
}

/// The building types a building of type `from` becomes, with the requirement of each ("Building upgrades").
fn becomes(from: u8) -> &'static [u8] {
    use building::*;
    match from {
        TOWN_HALL => &[KEEP],
        GREAT_HALL => &[STRONGHOLD],
        KEEP => &[CASTLE],
        STRONGHOLD => &[FORTRESS],
        HUMAN_SCOUT_TOWER => &[HUMAN_GUARD_TOWER, HUMAN_CANNON_TOWER],
        ORC_SCOUT_TOWER => &[ORC_GUARD_TOWER, ORC_CANNON_TOWER],
        _ => &[],
    }
}

/// The default upgrade data ("Default upgrade data"), for worlds built without an installation (`World::new`);
/// `World::from_pud` takes the installation's `upgrades.dat` instead. Costs are stored at full value; "affects" is the single bit given in the spec's table.
pub fn default_upgrades() -> UpgradeTable {
    // (time, gold, lumber, oil, group, bit)
    let row = |n: usize| -> (u8, u16, u16, u16, u16, u32) {
        match n {
            0 => (200, 800, 0, 0, 1, 2),
            1 => (250, 2400, 0, 0, 1, 3),
            2 => (200, 500, 100, 0, 1, 2),
            3 => (250, 1500, 300, 0, 1, 3),
            4 | 6 => (200, 300, 300, 0, 0, 0),
            // Arrow 2 shares arrow 1's bit; "Research twice" depends on it.
            5 => (250, 900, 500, 0, 0, 0),
            7 => (250, 900, 500, 0, 0, 1),
            8 | 10 => (200, 300, 300, 0, 2, 4),
            9 | 11 => (250, 900, 500, 0, 2, 5),
            12 | 14 => (200, 700, 100, 1000, 3, 6),
            13 | 15 => (250, 2000, 250, 3000, 3, 7),
            16 | 18 => (200, 500, 500, 0, 4, 8),
            17 | 19 => (250, 1500, 900, 0, 4, 9),
            20 | 22 => (250, 1500, 0, 0, 6, 12),
            21 | 23 => (250, 4000, 0, 0, 6, 13),
            24 | 28 => (250, 1500, 0, 0, 7, 16),
            25 | 29 => (250, 2000, 0, 0, 8, 17),
            26 | 30 => (250, 1500, 0, 0, 9, 18),
            27 => (250, 2500, 0, 0, 10, 19),
            31 => (250, 3000, 0, 0, 10, 19),
            32 | 33 => (250, 1000, 0, 0, 20, 20),
            // Spells: the group equals the bit and isn't used.
            34 => (0, 0, 0, 0, 0, 0),
            35 => (200, 1000, 0, 0, 1, 1),
            36 => (200, 2000, 0, 0, 3, 3),
            37 => (100, 1000, 0, 0, 4, 4),
            38 => (0, 0, 0, 0, 5, 5),
            39 => (100, 500, 0, 0, 6, 6),
            40 => (200, 2500, 0, 0, 7, 7),
            41 => (200, 2000, 0, 0, 8, 8),
            42 => (200, 2000, 0, 0, 9, 9),
            43 => (0, 0, 0, 0, 10, 10),
            44 => (100, 1000, 0, 0, 11, 11),
            45 => (100, 1500, 0, 0, 13, 13),
            46 => (100, 0, 0, 0, 14, 14),
            47 => (150, 1500, 0, 0, 15, 15),
            48 => (100, 500, 0, 0, 16, 16),
            49 => (200, 2500, 0, 0, 17, 17),
            50 => (150, 1000, 0, 0, 18, 18),
            _ => (200, 2000, 0, 0, 19, 19),
        }
    };
    UpgradeTable(
        (0..UPGRADES)
            .map(|n| {
                let (time, gold, lumber, oil, group, bit) = row(n);
                Upgrade {
                    time,
                    gold,
                    lumber,
                    oil,
                    icon: 0,
                    group,
                    affect: 1 << bit,
                }
            })
            .collect(),
    )
}

impl Player {
    /// Whether training needs the ranger (berserker) training level above 0.
    pub(crate) fn ranger_training_done(&self) -> bool {
        self.levels[RANGER_TRAINING_LINE] > 0
    }

    pub(crate) fn paladin_known(&self) -> bool {
        self.spells_known & PALADIN_BIT != 0
    }
}

impl World {
    /// "Has X": the player has at least one completed building of one of these types.
    fn has_any(&self, owner: u8, types: &[u8]) -> bool {
        self.units
            .iter()
            .any(|u| u.owner == owner && u.building && !u.dying && u.construction.is_none() && types.contains(&u.kind))
    }

    /// The training requirement of unit type `k` ("Training requirements"): the button rules without the ALOW bit
    /// and without "ready".
    pub fn training_requirement_met(&self, owner: u8, k: u8) -> bool {
        use building::*;
        let p = &self.players[owner as usize];
        let has = |t: &[u8]| self.has_any(owner, t);
        let blacksmith = has(&[HUMAN_BLACKSMITH, ORC_BLACKSMITH]);
        let lumber = has(&[ELVEN_LUMBER_MILL, TROLL_LUMBER_MILL]);
        let stables = has(&[STABLES, OGRE_MOUND]);
        let inventor = has(&[INVENTOR, ALCHEMIST]);
        let foundry = has(&[HUMAN_FOUNDRY, ORC_FOUNDRY]);
        match k {
            4 | 5 => blacksmith && lumber,
            6 | 7 => blacksmith && stables && !p.paladin_known(),
            12 | 13 => blacksmith && stables && p.paladin_known(),
            8 | 9 => lumber && !p.ranger_training_done(),
            18 | 19 => lumber && p.ranger_training_done(),
            14 | 15 | 38 | 39 => inventor,
            28 | 29 | 32 | 33 => foundry,
            40 | 41 => lumber && inventor,
            _ => true,
        }
    }

    /// Whether a building of type `from` may become `to`, ignoring requirements ("Building upgrades").
    fn is_building_upgrade(from: u8, to: u8) -> bool {
        becomes(from).contains(&to)
    }

    /// The requirement of becoming `to` ("Building upgrades").
    pub fn upgrade_requirement_met(&self, owner: u8, to: u8) -> bool {
        use building::*;
        let has = |t: &[u8]| self.has_any(owner, t);
        match to {
            KEEP | STRONGHOLD => has(&[HUMAN_BARRACKS, ORC_BARRACKS]),
            CASTLE | FORTRESS => {
                has(&[STABLES, OGRE_MOUND])
                    && has(&[ELVEN_LUMBER_MILL, TROLL_LUMBER_MILL])
                    && has(&[HUMAN_BLACKSMITH, ORC_BLACKSMITH])
            }
            HUMAN_GUARD_TOWER | ORC_GUARD_TOWER => has(&[ELVEN_LUMBER_MILL, TROLL_LUMBER_MILL]),
            HUMAN_CANNON_TOWER | ORC_CANNON_TOWER => has(&[HUMAN_BLACKSMITH, ORC_BLACKSMITH]),
            _ => true,
        }
    }

    /// The per-kind check of a train job at a building of type `b` ("Starting a job", step 3).
    pub(crate) fn may_train(&self, owner: u8, b: u8, k: u8) -> bool {
        trains(b, k) && (!self.rules.enforce_requirements || self.training_requirement_met(owner, k))
    }

    /// The per-kind check of a building upgrade.
    pub(crate) fn may_become(&self, owner: u8, b: u8, to: u8) -> bool {
        Self::is_building_upgrade(b, to)
            && (!self.rules.enforce_requirements || self.upgrade_requirement_met(owner, to))
    }

    /// The per-kind check of a spell or upgrade job: only that this building type researches it.
    pub(crate) fn may_research(b: u8, n: u8) -> bool {
        researcher(n) == Some(b)
    }

    /// The level check of upgrade `n` ("Starting a job", step 4): numbers 0 to 23 need level 0 (even) or 1 (odd) in
    /// their fixed line. Other numbers have none.
    pub(crate) fn level_ok(&self, owner: u8, n: u8) -> bool {
        n >= LEVEL_CHECKED || self.players[owner as usize].levels[fixed_line(n)] == n & 1
    }
}

#[cfg(test)]
mod tests {
    use super::building::*;
    use super::*;
    use crate::economy::{Event, JobKind};
    use crate::world::tests::{field, run};
    use crate::world::{Command, UnitId};
    use tidewater_formats::udta::{UnitStats, flags};

    /// An open map with synthetic building types 58 to 99 and a rich player 0. Enhanced, single player.
    fn lab() -> World {
        let mut w = field(24, 24);
        for k in 58..100 {
            w.stats.0[k] = UnitStats {
                hit_points: 400,
                size: (2, 2),
                flags: flags::BUILDING,
                ..Default::default()
            };
        }
        let p = &mut w.players[0];
        (p.gold, p.lumber, p.oil) = (20_000, 20_000, 20_000);
        p.spells_allowed = !0;
        w
    }

    /// A completed building of type `k` for player 0 on a spot of its own; `again` puts a second one in the last
    /// column.
    fn put(w: &mut World, k: u8, again: bool) -> UnitId {
        let x = if again { 21 } else { (k as i32 % 7) * 3 };
        let y = (k as i32 / 7 - 8) * 3;
        w.spawn(k, 0, (x, y), 0).unwrap()
    }

    fn gold(w: &World) -> u32 {
        w.players[0].gold
    }

    fn research(w: &mut World, b: UnitId, n: u8) {
        w.step(&[Command::Research {
            player: 0,
            building: b,
            upgrade: n,
        }]);
    }

    fn spell(w: &mut World, b: UnitId, n: u8) {
        w.step(&[Command::Spell {
            player: 0,
            building: b,
            spell: n,
        }]);
    }

    /// Cancels building `b`'s job and runs until it ends, at the building's next work run ("Cancelling a job").
    fn cancel(w: &mut World, b: UnitId) {
        w.step(&[Command::CancelJob { player: 0, unit: b }]);
        for _ in 0..60 {
            if job(w, b).is_none() {
                return;
            }
            w.step(&[]);
        }
        panic!("the cancelled job didn't end");
    }

    fn job(w: &World, b: UnitId) -> Option<crate::economy::Job> {
        w.unit(b).unwrap().job
    }

    /// Shortens a running job so it ends soon.
    fn shorten(w: &mut World, b: UnitId, steps: u16) {
        let i = w.index(b).unwrap();
        w.units[i].job.as_mut().unwrap().steps = steps;
    }

    #[test]
    fn default_data_matches_the_table() {
        let t = default_upgrades().0;
        assert_eq!(t.len(), UPGRADES);
        let a = |n: usize| (t[n].time, t[n].gold, t[n].lumber, t[n].oil, t[n].group, t[n].affect);
        assert_eq!(a(1), (250, 2400, 0, 0, 1, 1 << 3));
        assert_eq!(a(4).5, a(5).5, "arrow 2 shares arrow 1's bit");
        assert_eq!(a(5), (250, 900, 500, 0, 0, 1));
        assert_eq!(a(12), (200, 700, 100, 1000, 3, 1 << 6));
        assert_eq!(a(33), (250, 1000, 0, 0, 20, PALADIN_BIT));
        assert_eq!(a(42), (200, 2000, 0, 0, 9, 1 << 9));
        assert_eq!(a(51), (200, 2000, 0, 0, 19, 1 << 19));
    }

    #[test]
    fn train_checks_the_producer_and_counts_keeps_as_halls() {
        let mut w = lab();
        w.spawn(0x3a, 0, (0, 20), 0).unwrap();
        w.stats.0[1].gold = 100;
        w.stats.0[2].gold = 100;
        let barracks = put(&mut w, HUMAN_BARRACKS, false);
        w.step(&[Command::Train {
            player: 0,
            building: barracks,
            kind: 1,
        }]);
        assert_eq!((job(&w, barracks), gold(&w), w.events.len()), (None, 20_000, 0));
        let keep = put(&mut w, KEEP, false);
        w.step(&[Command::Train {
            player: 0,
            building: keep,
            kind: 2,
        }]);
        assert_eq!(job(&w, keep).unwrap().kind, JobKind::Train(2));
        assert_eq!((gold(&w), w.players[0].in_training), (19_900, 1));
        // A castle isn't a town hall for upgrading.
        let castle = put(&mut w, CASTLE, false);
        w.step(&[Command::Upgrade {
            player: 0,
            building: castle,
            to: KEEP,
        }]);
        assert_eq!(job(&w, castle), None);
        // Types no building trains.
        assert!(!trains(HUMAN_BARRACKS, 58) && !trains(TOWN_HALL, 0));
        assert!(trains(FORTRESS, 3) && trains(STRONGHOLD, 3) && !trains(KEEP, 3));
    }

    #[test]
    fn sword_levels() {
        let mut w = lab();
        let smith = put(&mut w, HUMAN_BLACKSMITH, false);
        research(&mut w, smith, 1);
        assert_eq!((job(&w, smith), gold(&w)), (None, 20_000));
        w.players[0].levels[1] = 1;
        research(&mut w, smith, 1);
        let j = job(&w, smith).unwrap();
        assert_eq!((gold(&w), j.steps), (17_600, 500));
        assert_eq!(w.players[0].upgrades_researching, 1 << 3);
        // Level 2: rejected.
        let mut w = lab();
        let smith = put(&mut w, HUMAN_BLACKSMITH, false);
        w.players[0].levels[1] = 2;
        research(&mut w, smith, 1);
        assert_eq!(job(&w, smith), None);
        // The orc blacksmith doesn't research human upgrades.
        let orc = put(&mut w, ORC_BLACKSMITH, false);
        research(&mut w, orc, 0);
        assert_eq!(job(&w, orc), None);
    }

    #[test]
    fn research_without_prerequisites_or_when_known() {
        let mut w = lab();
        let mill = put(&mut w, ELVEN_LUMBER_MILL, false);
        research(&mut w, mill, 25); // longbow, ranger training not done
        assert_eq!(gold(&w), 18_000);
        assert!(job(&w, mill).is_some());
        // Blizzard already known: starts, is paid, nothing changes when it ends.
        let tower = put(&mut w, MAGE_TOWER, false);
        w.players[0].spells_known = 1 << 9;
        spell(&mut w, tower, 42);
        assert_eq!(gold(&w), 16_000);
        shorten(&mut w, tower, 1);
        run(&mut w, 80);
        assert_eq!((job(&w, tower), w.players[0].spells_known), (None, 1 << 9));
        assert_eq!(w.players[0].spells_researching, 0);
    }

    #[test]
    fn requirements_while_enforced() {
        for enforce in [true, false] {
            let mut w = lab();
            w.rules.enforce_requirements = enforce;
            w.spawn(0x3a, 0, (0, 20), 0).unwrap();
            w.stats.0[6].gold = 100;
            let b = put(&mut w, HUMAN_BARRACKS, false);
            put(&mut w, HUMAN_BLACKSMITH, false);
            put(&mut w, STABLES, false);
            w.players[0].spells_known = PALADIN_BIT;
            w.step(&[Command::Train {
                player: 0,
                building: b,
                kind: 6,
            }]);
            assert_eq!(job(&w, b).is_some(), !enforce);
        }
        // Scout tower: guard needs a lumber mill, cannon a blacksmith.
        let mut w = lab();
        put(&mut w, HUMAN_BLACKSMITH, false);
        let t = put(&mut w, HUMAN_SCOUT_TOWER, false);
        let up = |w: &mut World, b, to| {
            w.step(&[Command::Upgrade {
                player: 0,
                building: b,
                to,
            }])
        };
        up(&mut w, t, HUMAN_GUARD_TOWER);
        assert_eq!(job(&w, t), None);
        up(&mut w, t, HUMAN_CANNON_TOWER);
        assert_eq!(job(&w, t).unwrap().kind, JobKind::Upgrade(HUMAN_CANNON_TOWER));
        // The orc tower doesn't become a human one.
        let o = put(&mut w, ORC_SCOUT_TOWER, false);
        up(&mut w, o, HUMAN_CANNON_TOWER);
        assert_eq!(job(&w, o), None);
        // Town hall to keep needs a barracks; keep to castle needs three buildings.
        let h = put(&mut w, TOWN_HALL, false);
        up(&mut w, h, KEEP);
        assert_eq!(job(&w, h), None);
        put(&mut w, ORC_BARRACKS, false);
        up(&mut w, h, KEEP);
        assert!(job(&w, h).is_some());
        let k = put(&mut w, KEEP, false);
        put(&mut w, TROLL_LUMBER_MILL, false);
        up(&mut w, k, CASTLE);
        assert_eq!(job(&w, k), None, "no stables");
        put(&mut w, OGRE_MOUND, false);
        up(&mut w, k, CASTLE);
        assert!(job(&w, k).is_some());
    }

    /// Two elven lumber mills at arrow level 1, arrow 2 started in both.
    fn arrow_two() -> (World, UnitId, UnitId) {
        let mut w = lab();
        w.players[0].levels[0] = 1;
        let a = put(&mut w, ELVEN_LUMBER_MILL, false);
        let b = put(&mut w, ELVEN_LUMBER_MILL, true);
        research(&mut w, a, 5);
        research(&mut w, b, 5);
        // Both started, and the arrow cost was paid twice.
        assert!(job(&w, a).is_some() && job(&w, b).is_some());
        assert_eq!((gold(&w), w.players[0].lumber), (20_000 - 1_800, 20_000 - 1_000));
        (w, a, b)
    }

    fn arrow_level(w: &World) -> u8 {
        w.players[0].levels[0]
    }

    #[test]
    fn arrow_two_finish_then_finish() {
        let (mut w, a, b) = arrow_two();
        shorten(&mut w, a, 1);
        run(&mut w, 60);
        assert_eq!((job(&w, a), arrow_level(&w)), (None, 2));
        assert_eq!(w.players[0].upgrades.arrow, 2);
        shorten(&mut w, b, 1);
        run(&mut w, 60);
        assert_eq!(job(&w, b), None);
        assert_eq!((arrow_level(&w), gold(&w)), (2, 20_000 - 1_800));
    }

    #[test]
    fn arrow_two_finish_then_cancel() {
        let (mut w, a, b) = arrow_two();
        shorten(&mut w, a, 1);
        run(&mut w, 60);
        assert_eq!(job(&w, a), None);
        cancel(&mut w, b);
        assert_eq!((arrow_level(&w), gold(&w), w.players[0].lumber), (2, 18_200, 19_000));
    }

    #[test]
    fn arrow_two_cancel_b_then_a_finishes() {
        let (mut w, a, b) = arrow_two();
        cancel(&mut w, b);
        assert_eq!((gold(&w), w.players[0].lumber), (18_200 + 900, 19_000 + 500));
        shorten(&mut w, a, 1);
        run(&mut w, 60);
        assert_eq!(job(&w, a), None);
        // A finds its bits clear: nothing happens, and A's payment is lost.
        assert_eq!((arrow_level(&w), gold(&w)), (1, 19_100));
    }

    #[test]
    fn arrow_two_cancel_a_then_cancel_b() {
        let (mut w, a, b) = arrow_two();
        cancel(&mut w, a);
        assert_eq!((gold(&w), w.players[0].lumber), (19_100, 19_500));
        cancel(&mut w, b);
        assert_eq!((gold(&w), w.players[0].lumber, arrow_level(&w)), (19_100, 19_500, 1));
    }

    #[test]
    fn arrow_two_cancel_a_then_b_finishes() {
        let (mut w, a, b) = arrow_two();
        cancel(&mut w, a);
        shorten(&mut w, b, 1);
        run(&mut w, 60);
        assert_eq!(job(&w, b), None);
        assert_eq!((arrow_level(&w), gold(&w)), (1, 19_100));
    }

    #[test]
    fn arrow_two_third_mill_sets_the_bit_again() {
        let (mut w, a, b) = arrow_two();
        cancel(&mut w, a);
        let c = w.spawn(ELVEN_LUMBER_MILL, 0, (18, 21), 0).unwrap();
        research(&mut w, c, 5);
        assert!(job(&w, c).is_some());
        shorten(&mut w, c, 1);
        run(&mut w, 60);
        assert_eq!((job(&w, c), arrow_level(&w)), (None, 2));
        // B ends later and finds the bit clear.
        shorten(&mut w, b, 1);
        run(&mut w, 60);
        assert_eq!((job(&w, b), arrow_level(&w)), (None, 2));
    }

    fn conversion_stats(w: &mut World) {
        let st = |hp, caster: bool| UnitStats {
            hit_points: hp,
            size: (1, 1),
            flags: flags::LAND | if caster { flags::CASTER } else { 0 },
            gold: 100,
            ..Default::default()
        };
        w.stats.0[unit::KNIGHT as usize] = st(90, false);
        w.stats.0[unit::PALADIN as usize] = st(110, true);
        w.stats.0[unit::OGRE as usize] = st(100, false);
        w.stats.0[unit::OGRE_MAGE as usize] = st(80, true);
        w.stats.0[unit::ARCHER as usize] = st(40, false);
        w.stats.0[unit::RANGER as usize] = st(50, false);
        w.stats.0[unit::RANGER as usize].gold = 150;
        w.stats.0[unit::AXE_THROWER as usize] = st(40, false);
        w.stats.0[unit::BERSERKER as usize] = st(55, false);
    }

    #[test]
    fn ranger_training_converts_units_and_the_barracks_job() {
        let mut w = lab();
        conversion_stats(&mut w);
        w.spawn(0x3a, 0, (0, 20), 0).unwrap();
        w.stats.0[unit::ARCHER as usize].build_time = 100;
        let mill = put(&mut w, ELVEN_LUMBER_MILL, false);
        let barracks = put(&mut w, HUMAN_BARRACKS, false);
        let a1 = w.spawn(unit::ARCHER, 0, (0, 0), 0).unwrap();
        let a2 = w.spawn(unit::ARCHER, 0, (1, 0), 0).unwrap();
        let other = w.spawn(unit::ARCHER, 1, (2, 0), 0).unwrap();
        let i = w.index(a2).unwrap();
        w.units[i].hp = 30;
        w.step(&[Command::Train {
            player: 0,
            building: barracks,
            kind: unit::ARCHER,
        }]);
        run(&mut w, 40);
        let before = job(&w, barracks).unwrap();
        assert!(before.progress > 0);
        research(&mut w, mill, 24);
        shorten(&mut w, mill, 1);
        run(&mut w, 60);
        assert_eq!(w.players[0].levels[7], 1);
        assert_eq!(w.players[0].upgrades_researching, 0);
        let hp = |id| {
            let u = w.unit(id).unwrap();
            (u.kind, u.hp)
        };
        assert_eq!((hp(a1), hp(a2)), ((unit::RANGER, 50), (unit::RANGER, 40)));
        assert_eq!(hp(other), (unit::ARCHER, 40), "other players keep theirs");
        let after = job(&w, barracks).unwrap();
        assert_eq!(after.kind, JobKind::Train(unit::RANGER));
        assert!(after.progress >= before.progress);
        assert_eq!(after.steps, before.steps);
        assert_eq!(
            w.job_cost(after.kind)[0],
            150,
            "a refund now gives back the ranger's cost"
        );
        assert!(
            w.events
                .iter()
                .any(|e| matches!(e, Event::ResearchDone { player: 0, .. }))
        );
        // The archer is no longer trainable; the ranger is.
        assert!(!w.training_requirement_met(0, unit::ARCHER) && w.training_requirement_met(0, unit::RANGER));
        // Cancelling now refunds the ranger's cost.
        let g = gold(&w);
        cancel(&mut w, barracks);
        assert_eq!(gold(&w), g + 150);
    }

    #[test]
    fn paladin_upgrade_grants_and_converts() {
        let mut w = lab();
        conversion_stats(&mut w);
        let church = put(&mut w, CHURCH, false);
        let knight = w.spawn(unit::KNIGHT, 0, (0, 0), 0).unwrap();
        let ogre = w.spawn(unit::OGRE, 0, (1, 0), 0).unwrap();
        let inside = w.spawn(unit::KNIGHT, 0, (2, 0), 0).unwrap();
        let i = w.index(inside).unwrap();
        w.units[i].inside = true;
        w.players[0].spells_allowed = HOLY_VISION_BIT;
        spell(&mut w, church, 33);
        assert_eq!(w.players[0].spells_researching, PALADIN_BIT);
        shorten(&mut w, church, 1);
        run(&mut w, 60);
        let p = w.players[0];
        assert_eq!(p.spells_known, PALADIN_BIT | HOLY_VISION_BIT);
        assert_eq!(p.spells_researching, 0);
        let k = w.unit(knight).unwrap();
        // It became a caster with 85 mana and a counter of 40; the timers pass has run on it since.
        assert_eq!((k.kind, k.hp), (unit::PALADIN, 110));
        assert!(
            k.mana >= 85 && (1..=40).contains(&k.mana_counter),
            "{} {}",
            k.mana,
            k.mana_counter
        );
        assert_eq!(w.unit(ogre).unwrap().kind, unit::OGRE_MAGE);
        assert_eq!(w.unit(inside).unwrap().kind, unit::KNIGHT);
        // Knights are no longer trainable, paladins are.
        assert!(!w.training_requirement_met(0, unit::KNIGHT));
    }

    #[test]
    fn a_duplicate_paladin_job_does_nothing() {
        let mut w = lab();
        conversion_stats(&mut w);
        let a = put(&mut w, CHURCH, false);
        let b = put(&mut w, CHURCH, true);
        let knight = w.spawn(unit::KNIGHT, 0, (0, 0), 0).unwrap();
        spell(&mut w, a, 33);
        spell(&mut w, b, 33);
        assert_eq!(gold(&w), 18_000);
        cancel(&mut w, b);
        assert_eq!(gold(&w), 19_000);
        shorten(&mut w, a, 1);
        run(&mut w, 60);
        assert_eq!(job(&w, a), None);
        assert_eq!(w.players[0].spells_known, 0);
        assert_eq!(w.unit(knight).unwrap().kind, unit::KNIGHT);
        assert!(!w.events.iter().any(|e| matches!(e, Event::ResearchDone { .. })));
    }

    #[test]
    fn cancelling_a_spell_refunds_it_at_the_current_cost() {
        let mut w = lab();
        let tower = put(&mut w, MAGE_TOWER, false);
        spell(&mut w, tower, 42);
        assert_eq!((gold(&w), w.players[0].spells_researching), (18_000, 1 << 9));
        // The data's cost at the time of the cancel is what comes back.
        w.upgrade_data.0[42].gold = 2_500;
        cancel(&mut w, tower);
        assert_eq!(
            (gold(&w), w.players[0].spells_researching, job(&w, tower)),
            (20_500, 0, None)
        );
        assert_eq!(w.players[0].spells_known, 0);
    }

    #[test]
    fn holy_vision_takes_no_steps() {
        let mut w = lab();
        let church = put(&mut w, CHURCH, false);
        spell(&mut w, church, 34);
        assert_eq!(job(&w, church).unwrap().steps, 0);
        run(&mut w, 60);
        assert_eq!((job(&w, church), w.players[0].spells_known), (None, HOLY_VISION_BIT));
    }

    #[test]
    fn cant_afford_is_told_and_nothing_starts() {
        let mut w = lab();
        let smith = put(&mut w, HUMAN_BLACKSMITH, false);
        w.players[0].gold = 799;
        research(&mut w, smith, 0);
        assert_eq!((job(&w, smith), w.players[0].upgrades_researching), (None, 0));
        assert!(matches!(
            w.events[..],
            [Event::CantAfford {
                player: 0,
                shortage: crate::economy::Shortage::Gold
            }]
        ));
    }

    #[test]
    fn classic_has_no_completion_alert() {
        let mut w = lab();
        w.rules.enhanced = false;
        let tower = put(&mut w, MAGE_TOWER, false);
        spell(&mut w, tower, 38);
        run(&mut w, 60);
        assert_eq!(w.players[0].spells_known, 1 << 5);
        assert!(w.events.is_empty());
    }

    /// Pins of the current research behaviour, written before the B-038 refactor so that it can't change them unseen:
    /// the whole simulation state with [`World::fingerprint`] and the queued events.
    mod pin {
        use super::*;

        fn pinned(w: &World) -> u64 {
            format!("{}|{:?}", w.fingerprint(), w.events)
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325, |h, b| {
                    (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
                })
        }

        /// An upgrade and a spell, each finished and cancelled, in both modes, with player 0 local.
        #[test]
        fn research_and_spells_finish_and_cancel() {
            let mut got = Vec::new();
            for enhanced in [false, true] {
                for spell_job in [false, true] {
                    for cancelled in [false, true] {
                        let mut w = lab();
                        w.rules.enhanced = enhanced;
                        w.vision.local = 0;
                        let b = if spell_job {
                            let t = put(&mut w, MAGE_TOWER, false);
                            spell(&mut w, t, 42);
                            t
                        } else {
                            let s = put(&mut w, HUMAN_BLACKSMITH, false);
                            research(&mut w, s, 0);
                            s
                        };
                        run(&mut w, 20);
                        if cancelled {
                            cancel(&mut w, b);
                        } else {
                            shorten(&mut w, b, 1);
                            run(&mut w, 30);
                        }
                        assert_eq!(job(&w, b), None);
                        got.push(pinned(&w));
                    }
                }
            }
            assert_eq!(
                got,
                [
                    13026258576897419260,
                    9343072542633606597,
                    3490737724711109217,
                    14390555962828912986,
                    17322055568500918574,
                    1542042842312108074,
                    13307126491906572310,
                    8861246373422088459
                ]
            );
        }

        /// A barracks job converted by ranger training, then cancelled: the refund is the new type's cost.
        #[test]
        fn a_converted_job_cancelled() {
            let mut w = lab();
            conversion_stats(&mut w);
            w.spawn(0x3a, 0, (0, 20), 0).unwrap();
            w.stats.0[unit::ARCHER as usize].build_time = 100;
            let mill = put(&mut w, ELVEN_LUMBER_MILL, false);
            let barracks = put(&mut w, HUMAN_BARRACKS, false);
            w.step(&[Command::Train {
                player: 0,
                building: barracks,
                kind: unit::ARCHER,
            }]);
            run(&mut w, 40);
            research(&mut w, mill, 24);
            shorten(&mut w, mill, 1);
            run(&mut w, 60);
            cancel(&mut w, barracks);
            assert_eq!(pinned(&w), 9595299239633207397);
        }
    }
}
