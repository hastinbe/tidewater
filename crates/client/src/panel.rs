//! The command panel: the 3 x 3 grid of buttons in the lower left of the screen (spec `docs/specs/ui/command-buttons.md`).
//!
//! This module holds the button sets as data (slot, icon frame, string number, "shown when" condition, action), the
//! rules that pick the set to show and fill its slots, the parsing of the string table entries behind every button
//! (hotkey, cost kind, text) and what a click or hotkey does. It reads the local player's state only through the
//! [`PanelState`] trait, so the sim can change underneath it, and it draws nothing: `hud` draws the icons and `main`
//! owns the input. No SDL types here.
//!
//! Our reading of the spec where it is open is marked "Our reading:" at the place it applies.

use tidewater_sim::economy;
use tidewater_sim::{Command, Order, UnitId, World};

use crate::hud::Rect;

// --- the data model -----------------------------------------------------------------------------------------------

/// The level lines of the tech-tree spec that the panel reads (upgrade groups).
pub mod line {
    pub const MISSILE: usize = 0;
    pub const WEAPON: usize = 1;
    pub const SHIELD: usize = 2;
    pub const CANNON: usize = 3;
    pub const SHIP_ARMOUR: usize = 4;
    pub const SIEGE: usize = 6;
    pub const RANGER_TRAINING: usize = 7;
    pub const RANGER_RANGE: usize = 8;
    pub const RANGER_SCOUTING: usize = 9;
    pub const RANGER_LAST: usize = 10;
    /// Marks "no level line": the entry is shown whatever the level.
    pub const NONE: usize = usize::MAX;
}
use line::*;

/// Spell bit of the paladin and ogre-mage upgrade.
const PALADIN_BIT: u8 = 20;

/// What a target-mode button asks for. The number is the command number of `orders/orders.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Command 3, the smart move.
    Move,
    /// Command 8.
    Attack,
    /// Command 5.
    Patrol,
    /// Command 23.
    Harvest,
    /// Command 27.
    Repair,
    /// Command 17.
    AttackGround,
    /// Command 19.
    Demolish,
    /// Command 29.
    Unload,
    /// A targeted spell, by command number.
    Spell(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    Basic,
    Advanced,
}

/// What a button does (spec "What the buttons do").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Target(Target),
    Stop,
    StandGround,
    ReturnGoods,
    Unload,
    /// Cast: command n.
    Cast(u8),
    Build(u8),
    Train(u8),
    /// Research or upgrade number n.
    Research(u8),
    /// Become unit type n (building upgrade).
    Become(u8),
    CancelConstruction,
    CancelJob,
    Menu(Menu),
    /// Go back to the unit's own set (closes a build menu).
    Back,
    /// Leave target or placement mode.
    LeaveMode,
}

/// "Shown when" (spec "Conditions" and its legend).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Never,
    Always,
    One,
    Enhanced,
    /// The player's level on a line equals n.
    LineIs(usize, u8),
    GroundAttack,
    HarvesterFree,
    HarvesterCarrying,
    Passengers,
    /// One unit selected and the spell bit is known.
    Spell(u8),
    /// One unit selected and some build button of the menu would show.
    MenuHas(Menu),
    UnderConstruction,
    Training,
    Working,
    /// "Unit requirement": the job-buttons row for training unit type n.
    Unit(u8),
    /// "Building requirement": the build-buttons row for building type n.
    Building(u8),
    HallToKeep,
    KeepToCastle,
    GuardTower,
    CannonTower,
    /// Two-level upgrade of a line, level n.
    TwoLevel(usize, u8),
    RangerTraining,
    RangerRange,
    RangerScouting,
    RangerLast,
    /// Research spell bit n (mage tower, temple; bit 20 is the paladin or ogre-mage upgrade).
    Research(u8),
    /// Research spell bit n at a church or altar.
    ChurchResearch(u8),
}

/// One button of a set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub slot: u8,
    /// Frame of the icon sprite.
    pub icon: u16,
    /// String number in `stat_txt.tbl`, counting from 1.
    pub string: u16,
    pub cond: Cond,
    pub act: Act,
}

const fn e(slot: u8, icon: u16, string: u16, cond: Cond, act: Act) -> Entry {
    Entry {
        slot,
        icon,
        string,
        cond,
        act,
    }
}

const NOTHING: Entry = e(0, 0, 0, Cond::Never, Act::LeaveMode);

/// A button set: segments of entries in slot order.
pub type Set = &'static [&'static [Entry]];

// --- the button sets ----------------------------------------------------------------------------------------------

const fn lvl(l: usize, n: u8) -> Cond {
    if l == NONE {
        if n == 0 { Cond::Always } else { Cond::Never }
    } else {
        Cond::LineIs(l, n)
    }
}

/// The common mobile-unit layout: move (0), stop (1, an icon per shield level), attack (2, an icon per weapon level),
/// patrol (3), stand ground (4) and attack ground (5). An icon of 0 leaves the button out.
#[allow(clippy::too_many_arguments)]
const fn walker(
    mv: u16,
    stop: u16,
    stop_line: usize,
    atk: u16,
    atk_line: usize,
    patrol: u16,
    patrol_cond: Cond,
    stand: u16,
    ground: u16,
) -> [Entry; 10] {
    let mut out = [NOTHING; 10];
    out[0] = e(0, mv, 392, Cond::Always, Act::Target(Target::Move));
    let mut n = 0;
    while n < 3 {
        out[1 + n] = e(1, stop + n as u16, 393, lvl(stop_line, n as u8), Act::Stop);
        if atk != 0 {
            out[4 + n] = e(
                2,
                atk + n as u16,
                394,
                lvl(atk_line, n as u8),
                Act::Target(Target::Attack),
            );
        }
        n += 1;
    }
    if patrol != 0 {
        out[7] = e(3, patrol, 404, patrol_cond, Act::Target(Target::Patrol));
    }
    if stand != 0 {
        out[8] = e(4, stand, 403, Cond::Always, Act::StandGround);
    }
    if ground != 0 {
        out[9] = e(5, ground, 402, Cond::GroundAttack, Act::Target(Target::AttackGround));
    }
    out
}

use Cond::{Always, Enhanced};

// Human and orc ground fighters.
const H_MELEE_G: [Entry; 10] = walker(83, 164, SHIELD, 116, WEAPON, 178, Always, 180, 182);
const H_MELEE: [Entry; 10] = walker(83, 164, SHIELD, 116, WEAPON, 178, Always, 180, 0);
const O_MELEE_G: [Entry; 10] = walker(84, 167, SHIELD, 119, WEAPON, 179, Always, 181, 183);
const O_MELEE: [Entry; 10] = walker(84, 167, SHIELD, 119, WEAPON, 179, Always, 181, 0);
const H_ARCHER: [Entry; 10] = walker(83, 164, SHIELD, 124, MISSILE, 178, Always, 180, 0);
const O_AXE: [Entry; 10] = walker(84, 167, SHIELD, 127, MISSILE, 179, Always, 181, 0);
const H_SAPPER: [Entry; 10] = walker(83, 164, SHIELD, 116, WEAPON, 184, Enhanced, 0, 0);
const O_SAPPER: [Entry; 10] = walker(84, 167, SHIELD, 119, WEAPON, 185, Enhanced, 0, 0);
const H_FLYER: [Entry; 10] = walker(83, 164, SHIELD, 0, NONE, 184, Enhanced, 0, 0);
const O_FLYER: [Entry; 10] = walker(84, 167, SHIELD, 0, NONE, 185, Enhanced, 0, 0);
const H_ATTACK_WORKER: [Entry; 10] = walker(83, 164, NONE, 0, NONE, 0, Cond::Never, 0, 0);
const O_ATTACK_WORKER: [Entry; 10] = walker(84, 167, NONE, 0, NONE, 0, Cond::Never, 0, 0);
const H_MIXED: [Entry; 10] = walker(83, 164, SHIELD, 116, NONE, 178, Always, 180, 182);
const O_MIXED: [Entry; 10] = walker(84, 167, SHIELD, 119, NONE, 179, Always, 181, 183);
// Ships.
const H_SHIP_G: [Entry; 10] = walker(157, 153, SHIP_ARMOUR, 144, CANNON, 184, Always, 180, 182);
const O_SHIP_G: [Entry; 10] = walker(156, 150, SHIP_ARMOUR, 147, CANNON, 185, Always, 181, 183);
const H_SUB: [Entry; 10] = walker(157, 153, SHIP_ARMOUR, 144, CANNON, 184, Always, 180, 0);
const O_TURTLE: [Entry; 10] = walker(156, 150, SHIP_ARMOUR, 147, CANNON, 185, Always, 181, 0);
const H_HULL: [Entry; 10] = walker(157, 153, SHIP_ARMOUR, 0, NONE, 0, Cond::Never, 0, 0);
const O_HULL: [Entry; 10] = walker(156, 150, SHIP_ARMOUR, 0, NONE, 0, Cond::Never, 0, 0);

const fn spell(slot: u8, icon: u16, string: u16, bit: u8, command: u8) -> Entry {
    e(slot, icon, string, Cond::Spell(bit), Act::Cast(command))
}

const fn spell_always(slot: u8, icon: u16, string: u16, command: u8) -> Entry {
    e(slot, icon, string, Cond::Always, Act::Cast(command))
}

const MAGE_PART: [Entry; 9] = [
    e(0, 83, 392, Always, Act::Target(Target::Move)),
    e(1, 164, 393, Always, Act::Stop),
    e(2, 99, 348, Always, Act::Target(Target::Attack)),
    spell(3, 101, 349, 5, 43),
    spell(4, 94, 351, 6, 44),
    spell(5, 100, 350, 4, 42),
    spell(6, 95, 352, 7, 45),
    spell(7, 115, 353, 8, 46),
    spell(8, 105, 354, 9, 47),
];
const DK_PART: [Entry; 9] = [
    e(0, 84, 392, Always, Act::Target(Target::Move)),
    e(1, 167, 393, Always, Act::Stop),
    e(2, 102, 355, Always, Act::Target(Target::Attack)),
    spell(3, 108, 357, 14, 51),
    spell(4, 96, 358, 16, 53),
    spell(5, 114, 369, 13, 50),
    spell(6, 104, 361, 15, 52),
    spell(7, 98, 359, 17, 54),
    spell(8, 103, 356, 19, 56),
];
const PALADIN_SPELLS: [Entry; 3] = [
    spell(6, 106, 362, 0, 38),
    spell(7, 107, 363, 1, 39),
    spell(8, 110, 365, 3, 41),
];
const OGRE_MAGE_SPELLS: [Entry; 3] = [
    spell(6, 111, 366, 10, 48),
    spell(7, 112, 367, 11, 49),
    spell(8, 97, 360, 18, 55),
];
const CHOGALL_SPELLS: [Entry; 3] = [
    spell_always(6, 111, 366, 48),
    spell_always(7, 112, 367, 49),
    spell_always(8, 97, 360, 55),
];
const H_DEMOLISH: [Entry; 1] = [e(8, 142, 406, Always, Act::Target(Target::Demolish))];
const O_DEMOLISH: [Entry; 1] = [e(8, 143, 406, Always, Act::Target(Target::Demolish))];

const fn worker_part(mv: u16, stop: u16, atk: u16, carry_icon: u16) -> [Entry; 8] {
    let (basic, advanced) = (Menu::Basic, Menu::Advanced);
    [
        e(0, mv, 392, Always, Act::Target(Target::Move)),
        e(1, stop, 393, Always, Act::Stop),
        e(2, atk, 394, Always, Act::Target(Target::Attack)),
        e(3, 85, 395, Always, Act::Target(Target::Repair)),
        e(4, 86, 396, Cond::HarvesterFree, Act::Target(Target::Harvest)),
        e(5, carry_icon, 401, Cond::HarvesterCarrying, Act::ReturnGoods),
        e(6, 87, 398, Cond::MenuHas(basic), Act::Menu(basic)),
        e(7, 88, 399, Cond::MenuHas(advanced), Act::Menu(advanced)),
    ]
}
const PEASANT: [Entry; 8] = worker_part(83, 164, 116, 89);
const PEON: [Entry; 8] = worker_part(84, 167, 119, 90);

const fn tanker_part(mv: u16, armour: u16, platform_icon: u16, harvest: u16, carry: u16, platform: u8) -> [Entry; 8] {
    let mut out = [NOTHING; 8];
    out[0] = e(0, mv, 392, Always, Act::Target(Target::Move));
    let mut n = 0;
    while n < 3 {
        out[1 + n] = e(1, armour + n as u16, 393, Cond::LineIs(SHIP_ARMOUR, n as u8), Act::Stop);
        n += 1;
    }
    out[4] = e(3, platform_icon, 309, Cond::One, Act::Build(platform));
    out[5] = e(4, harvest, 397, Cond::HarvesterFree, Act::Target(Target::Harvest));
    out[6] = e(5, carry, 401, Cond::HarvesterCarrying, Act::ReturnGoods);
    out
}
const H_TANKER: [Entry; 8] = tanker_part(157, 153, 54, 161, 158, 86);
const O_TANKER: [Entry; 8] = tanker_part(156, 150, 55, 160, 159, 87);
const H_UNLOAD: [Entry; 1] = [e(2, 162, 405, Cond::Passengers, Act::Unload)];
const O_UNLOAD: [Entry; 1] = [e(2, 163, 405, Cond::Passengers, Act::Unload)];

const fn train(slot: u8, icon: u16, string: u16, kind: u8) -> Entry {
    e(slot, icon, string, Cond::Unit(kind), Act::Train(kind))
}
const fn research(slot: u8, icon: u16, string: u16, cond: Cond, n: u8) -> Entry {
    e(slot, icon, string, cond, Act::Research(n))
}
const fn two(slot: u8, icon: u16, string: u16, l: usize, level: u8, n: u8) -> Entry {
    research(slot, icon, string, Cond::TwoLevel(l, level), n)
}

const H_BARRACKS: [Entry; 6] = [
    train(0, 2, 283, 0),
    train(1, 4, 272, 8),
    train(1, 6, 273, 18),
    train(2, 16, 284, 4),
    train(3, 8, 275, 6),
    train(3, 10, 277, 12),
];
const O_BARRACKS: [Entry; 6] = [
    train(0, 3, 282, 1),
    train(1, 5, 271, 9),
    train(1, 7, 274, 19),
    train(2, 17, 285, 5),
    train(3, 9, 276, 7),
    train(3, 11, 278, 13),
];
const CHURCH: [Entry; 3] = [
    research(0, 10, 370, Cond::Research(PALADIN_BIT), 33),
    research(0, 107, 385, Cond::ChurchResearch(1), 35),
    research(1, 110, 387, Cond::ChurchResearch(3), 36),
];
const ALTAR: [Entry; 3] = [
    research(0, 11, 371, Cond::Research(PALADIN_BIT), 32),
    research(0, 112, 389, Cond::ChurchResearch(11), 44),
    research(1, 97, 382, Cond::ChurchResearch(18), 50),
];
const fn becomes_into(slot: u8, icon: u16, string: u16, cond: Cond, kind: u8) -> Entry {
    e(slot, icon, string, cond, Act::Become(kind))
}
const H_SCOUT: [Entry; 2] = [
    becomes_into(0, 75, 319, Cond::GuardTower, 96),
    becomes_into(0, 76, 320, Cond::CannonTower, 98),
];
const O_SCOUT: [Entry; 2] = [
    becomes_into(0, 77, 319, Cond::GuardTower, 97),
    becomes_into(0, 78, 320, Cond::CannonTower, 99),
];
const INVENTOR: [Entry; 2] = [train(0, 28, 294, 40), train(1, 12, 297, 14)];
const ALCHEMIST: [Entry; 2] = [train(0, 29, 293, 41), train(1, 13, 298, 15)];
const AVIARY: [Entry; 1] = [train(0, 30, 295, 42)];
const ROOST: [Entry; 1] = [train(0, 31, 296, 43)];
const H_SHIPYARD: [Entry; 5] = [
    train(0, 18, 286, 26),
    train(1, 22, 287, 30),
    train(2, 20, 288, 28),
    train(3, 24, 289, 32),
    train(4, 26, 291, 38),
];
const O_SHIPYARD: [Entry; 5] = [
    train(0, 19, 286, 27),
    train(1, 23, 287, 31),
    train(2, 21, 288, 29),
    train(3, 25, 290, 33),
    train(4, 27, 292, 39),
];
const H_HALL: [Entry; 3] = [
    train(0, 0, 270, 2),
    becomes_into(1, 66, 317, Cond::HallToKeep, 88),
    becomes_into(1, 68, 315, Cond::KeepToCastle, 90),
];
const O_HALL: [Entry; 3] = [
    train(0, 1, 269, 3),
    becomes_into(1, 67, 318, Cond::HallToKeep, 89),
    becomes_into(1, 69, 316, Cond::KeepToCastle, 91),
];
const ELVEN_MILL: [Entry; 6] = [
    two(0, 125, 332, MISSILE, 0, 4),
    two(0, 126, 332, MISSILE, 1, 5),
    research(3, 6, 338, Cond::RangerTraining, 24),
    research(3, 133, 340, Cond::RangerScouting, 26),
    research(4, 132, 339, Cond::RangerRange, 25),
    research(5, 134, 341, Cond::RangerLast, 27),
];
const TROLL_MILL: [Entry; 6] = [
    two(0, 128, 330, MISSILE, 0, 6),
    two(0, 129, 330, MISSILE, 1, 7),
    research(3, 7, 342, Cond::RangerTraining, 28),
    research(3, 136, 344, Cond::RangerScouting, 30),
    research(4, 135, 343, Cond::RangerRange, 29),
    research(5, 137, 345, Cond::RangerLast, 31),
];
const H_FOUNDRY: [Entry; 4] = [
    two(0, 145, 335, CANNON, 0, 12),
    two(0, 146, 335, CANNON, 1, 13),
    two(1, 154, 336, SHIP_ARMOUR, 0, 16),
    two(1, 155, 336, SHIP_ARMOUR, 1, 17),
];
const O_FOUNDRY: [Entry; 4] = [
    two(0, 148, 335, CANNON, 0, 14),
    two(0, 149, 335, CANNON, 1, 15),
    two(1, 151, 336, SHIP_ARMOUR, 0, 18),
    two(1, 152, 336, SHIP_ARMOUR, 1, 19),
];
const MAGE_TOWER: [Entry; 6] = [
    train(0, 14, 281, 10),
    research(1, 94, 374, Cond::Research(6), 39),
    research(2, 100, 373, Cond::Research(4), 37),
    research(3, 95, 375, Cond::Research(7), 40),
    research(4, 115, 376, Cond::Research(8), 41),
    research(5, 105, 377, Cond::Research(9), 42),
];
const TEMPLE: [Entry; 6] = [
    train(0, 15, 279, 11),
    research(1, 96, 380, Cond::Research(16), 48),
    research(2, 114, 391, Cond::Research(13), 45),
    research(3, 104, 383, Cond::Research(15), 47),
    research(4, 98, 381, Cond::Research(17), 49),
    research(5, 103, 378, Cond::Research(19), 51),
];
const H_SMITH: [Entry; 6] = [
    two(0, 117, 329, WEAPON, 0, 0),
    two(0, 118, 329, WEAPON, 1, 1),
    two(1, 165, 333, SHIELD, 0, 8),
    two(1, 166, 333, SHIELD, 1, 9),
    two(2, 140, 347, SIEGE, 0, 22),
    two(2, 141, 347, SIEGE, 1, 23),
];
const O_SMITH: [Entry; 6] = [
    two(0, 120, 331, WEAPON, 0, 2),
    two(0, 121, 331, WEAPON, 1, 3),
    two(1, 168, 334, SHIELD, 0, 10),
    two(1, 169, 334, SHIELD, 1, 11),
    two(2, 138, 346, SIEGE, 0, 20),
    two(2, 139, 346, SIEGE, 1, 21),
];

const fn build(slot: u8, icon: u16, string: u16, kind: u8) -> Entry {
    e(slot, icon, string, Cond::Building(kind), Act::Build(kind))
}
const BACK: Entry = e(8, 91, 407, Always, Act::Back);
const H_BASIC: [Entry; 8] = [
    build(0, 38, 306, 58),
    build(1, 42, 299, 60),
    build(2, 40, 307, 74),
    build(3, 44, 300, 76),
    build(4, 46, 314, 82),
    build(6, 60, 303, 64),
    build(7, 92, 312, 103),
    BACK,
];
const H_ADVANCED: [Entry; 9] = [
    build(0, 48, 313, 72),
    build(1, 52, 311, 78),
    build(2, 50, 310, 84),
    build(3, 58, 321, 68),
    build(4, 56, 301, 66),
    build(5, 62, 304, 62),
    build(6, 64, 325, 80),
    build(7, 72, 323, 70),
    BACK,
];
const O_BASIC: [Entry; 8] = [
    build(0, 39, 306, 59),
    build(1, 43, 299, 61),
    build(2, 41, 308, 75),
    build(3, 45, 300, 77),
    build(4, 47, 314, 83),
    build(6, 61, 303, 65),
    build(7, 93, 312, 103),
    BACK,
];
const O_ADVANCED: [Entry; 9] = [
    build(0, 49, 313, 73),
    build(1, 53, 311, 79),
    build(2, 51, 310, 85),
    build(3, 59, 322, 69),
    build(4, 57, 302, 67),
    build(5, 63, 305, 63),
    build(6, 65, 326, 81),
    build(7, 73, 324, 71),
    BACK,
];
const CANCEL_TARGET: [Entry; 1] = [e(8, 91, 407, Always, Act::LeaveMode)];
const BUSY: [Entry; 3] = [
    e(8, 91, 408, Cond::UnderConstruction, Act::CancelConstruction),
    e(8, 91, 410, Cond::Working, Act::CancelJob),
    e(8, 91, 409, Cond::Training, Act::CancelJob),
];

/// Which set the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetId {
    /// No buttons.
    Empty,
    /// A unit type's own set.
    Kind(u8),
    Menu(Menu, bool),
    Target,
    Place,
    Busy,
    /// The mixed-selection set of a race (true for human).
    Mixed(bool),
}

/// The entries of a set.
pub fn set(id: SetId) -> Set {
    match id {
        SetId::Empty => &[],
        SetId::Kind(k) => kind_set(k).unwrap_or(&[]),
        SetId::Menu(Menu::Basic, true) => &[&H_BASIC],
        SetId::Menu(Menu::Advanced, true) => &[&H_ADVANCED],
        SetId::Menu(Menu::Basic, false) => &[&O_BASIC],
        SetId::Menu(Menu::Advanced, false) => &[&O_ADVANCED],
        SetId::Target | SetId::Place => &[&CANCEL_TARGET],
        SetId::Busy => &[&BUSY],
        SetId::Mixed(true) => &[&H_MIXED],
        SetId::Mixed(false) => &[&O_MIXED],
    }
}

/// The set of unit type `k`, or `None` for types without buttons ("Button sets").
pub fn kind_set(k: u8) -> Option<Set> {
    Some(match k {
        0 | 4 | 22 | 36 | 42 | 46 => &[&H_MELEE_G],
        1 | 5 | 25 | 35 | 37 | 43 | 47 | 55 | 56 => &[&O_MELEE_G],
        2 => &[&PEASANT],
        3 => &[&PEON],
        6 => &[&H_MELEE],
        7 => &[&O_MELEE],
        8 | 18 | 20 => &[&H_ARCHER],
        9 | 19 | 53 => &[&O_AXE],
        10 | 24 => &[&MAGE_PART],
        11 | 21 | 51 => &[&DK_PART],
        12 | 44 | 50 | 52 => &[&H_MELEE, &PALADIN_SPELLS],
        13 | 23 => &[&O_MELEE, &OGRE_MAGE_SPELLS],
        14 => &[&H_SAPPER, &H_DEMOLISH],
        15 => &[&O_SAPPER, &O_DEMOLISH],
        16 => &[&H_ATTACK_WORKER],
        17 => &[&O_ATTACK_WORKER],
        26 => &[&H_TANKER],
        27 => &[&O_TANKER],
        28 => &[&H_HULL, &H_UNLOAD],
        29 => &[&O_HULL, &O_UNLOAD],
        30 | 32 | 34 => &[&H_SHIP_G],
        31 | 33 => &[&O_SHIP_G],
        38 => &[&H_SUB],
        39 => &[&O_TURTLE],
        40 => &[&H_FLYER],
        41 | 45 => &[&O_FLYER],
        49 => &[&O_MELEE, &CHOGALL_SPELLS],
        60 => &[&H_BARRACKS],
        61 => &[&O_BARRACKS],
        62 => &[&CHURCH],
        63 => &[&ALTAR],
        64 | 96 | 98 => &[&H_SCOUT],
        65 | 97 | 99 => &[&O_SCOUT],
        68 => &[&INVENTOR],
        69 => &[&ALCHEMIST],
        70 => &[&AVIARY],
        71 => &[&ROOST],
        72 => &[&H_SHIPYARD],
        73 => &[&O_SHIPYARD],
        74 | 88 | 90 => &[&H_HALL],
        75 | 89 | 91 => &[&O_HALL],
        76 => &[&ELVEN_MILL],
        77 => &[&TROLL_MILL],
        78 => &[&H_FOUNDRY],
        79 => &[&O_FOUNDRY],
        80 => &[&MAGE_TOWER],
        81 => &[&TEMPLE],
        82 => &[&H_SMITH],
        83 => &[&O_SMITH],
        _ => return None,
    })
}

// --- the state the panel reads ------------------------------------------------------------------------------------

/// Gold, lumber and oil.
pub type Cost = [u32; 3];

/// What the job of a working building is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobClass {
    Training,
    /// Research, an upgrade or a building upgrade.
    Other,
}

/// A building's production queue (option `production-queue`): how many jobs wait behind the running one, and whether
/// another can still be queued. `None` in [`UnitInfo::queue`] means the option is off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueInfo {
    pub waiting: u8,
    pub open: bool,
}

/// A selected unit as the panel needs to know it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitInfo {
    pub id: UnitId,
    pub kind: u8,
    pub owner: u8,
    pub tile: (i32, i32),
    pub building: bool,
    /// A building that is not under construction.
    pub completed: bool,
    pub job: Option<JobClass>,
    /// The building's production queue, if the option is on.
    pub queue: Option<QueueInfo>,
    /// A worker or tanker.
    pub harvester: bool,
    pub carrying: bool,
    /// Its type can attack the ground (unit flag bit 14).
    pub ground_attack: bool,
    /// Passenger places filled (of 6).
    pub passengers: u8,
    /// A transport whose order or pending order is 32.
    pub docked: bool,
    pub mana: u8,
}

/// Everything the panel reads about the local player and the game. The only place the panel touches the sim.
pub trait PanelState {
    /// The local player.
    fn local(&self) -> u8;
    /// The local player's race: 0 human, otherwise orc.
    fn race(&self) -> u8;
    fn enhanced(&self) -> bool;
    /// More than one human player.
    fn multiplayer(&self) -> bool;
    /// A unit by id, if it is alive.
    fn unit(&self, id: UnitId) -> Option<UnitInfo>;
    /// The player's level on a [`line`], counting completed upgrades.
    fn level(&self, line: usize) -> u8;
    /// Spells known, by spell bit.
    fn knows(&self, spell_bit: u8) -> bool;
    /// Spells being researched.
    fn researching_spell(&self, spell_bit: u8) -> bool;
    /// Spells allowed to research.
    fn spell_allowed(&self, spell_bit: u8) -> bool;
    /// The map's "units and buildings allowed" word (ALOW), by bit.
    fn unit_allowed(&self, bit: u8) -> bool;
    /// The upgrade-allowed word, by bit.
    fn upgrade_allowed(&self, bit: u8) -> bool;
    /// The upgrade-being-researched word, by bit.
    fn upgrade_researching(&self, bit: u8) -> bool;
    /// The player has at least one completed building of one of these types.
    fn has_complete(&self, kinds: &[u8]) -> bool;
    /// The player has at some time had a completed hall.
    fn had_hall(&self) -> bool;
    /// The cost of making a unit or building type.
    fn kind_cost(&self, kind: u8) -> Cost;
    /// The cost of an upgrade or research.
    fn upgrade_cost(&self, n: u8) -> Cost;
    /// The message number (438 to 441) of the first thing the player lacks, if any.
    fn shortage(&self, cost: Cost, needs_food: bool) -> Option<u16>;
    /// Whether a depot takes what the unit carries.
    fn has_depot(&self, id: UnitId) -> bool;
    /// The footprint of a building type, in tiles.
    fn footprint(&self, kind: u8) -> (u8, u8);
    /// The all-spells cheat is on: the cast buttons skip their mana test (cheats spec, "Lasting effects").
    fn all_spells_cheat(&self) -> bool {
        false
    }
}

// --- conditions ---------------------------------------------------------------------------------------------------

const HALLS: [u8; 2] = [74, 75];
const KEEP_OR_CASTLE: [u8; 4] = [88, 89, 90, 91];
const CASTLE: [u8; 2] = [90, 91];
const BARRACKS: [u8; 2] = [60, 61];
const LUMBER_MILL: [u8; 2] = [76, 77];
const BLACKSMITH: [u8; 2] = [82, 83];
const STABLES: [u8; 2] = [66, 67];
const FOUNDRY: [u8; 2] = [78, 79];
const INVENTOR_B: [u8; 2] = [68, 69];
const AVIARY_B: [u8; 2] = [70, 71];
const SHIPYARD: [u8; 2] = [72, 73];

fn ready(p: &UnitInfo) -> bool {
    p.building && p.completed && (p.job.is_none() || p.queue.is_some_and(|q| q.open))
}

/// Whether the building is busy and cannot take another job: working with no room in its queue (or no queue).
fn busy(p: &UnitInfo) -> bool {
    p.job.is_some() && !p.queue.is_some_and(|q| q.open)
}

/// The upgrade-allowed bit of the lower level of a two-level line.
fn two_level_bit(l: usize) -> u8 {
    match l {
        MISSILE => 0,
        WEAPON => 2,
        SHIELD => 4,
        CANNON => 6,
        SHIP_ARMOUR => 8,
        _ => 12,
    }
}

/// The "Job buttons" row for training unit type `k` at the primary building `p`.
fn train_ok(s: &impl PanelState, p: &UnitInfo, k: u8) -> bool {
    if !ready(p) {
        return false;
    }
    let has = |t: &[u8]| s.has_complete(t);
    let paladin = s.knows(PALADIN_BIT);
    let ranger = s.level(RANGER_TRAINING) > 0;
    match k {
        2 | 3 => s.unit_allowed(1),
        0 | 1 => s.unit_allowed(0),
        8 | 9 => s.unit_allowed(4) && has(&LUMBER_MILL) && !ranger,
        18 | 19 => s.unit_allowed(4) && has(&LUMBER_MILL) && ranger,
        4 | 5 => s.unit_allowed(2) && has(&BLACKSMITH) && has(&LUMBER_MILL),
        6 | 7 => s.unit_allowed(3) && has(&BLACKSMITH) && has(&STABLES) && !paladin,
        12 | 13 => s.unit_allowed(3) && has(&BLACKSMITH) && has(&STABLES) && paladin,
        10 | 11 => s.unit_allowed(5),
        26 | 27 => s.unit_allowed(6),
        30 | 31 => s.unit_allowed(7),
        28 | 29 => s.unit_allowed(8) && has(&FOUNDRY),
        32 | 33 => s.unit_allowed(9) && has(&FOUNDRY),
        38 | 39 => s.unit_allowed(10) && has(&INVENTOR_B),
        40 | 41 => s.unit_allowed(11) && has(&LUMBER_MILL) && has(&INVENTOR_B),
        42 | 43 => s.unit_allowed(12) && has(&AVIARY_B),
        14 | 15 => s.unit_allowed(14) && has(&INVENTOR_B),
        _ => false,
    }
}

/// The "Build buttons" row for building type `k`.
fn build_ok(s: &impl PanelState, k: u8) -> bool {
    let hall = s.had_hall();
    let has = |t: &[u8]| s.has_complete(t);
    match k {
        58 | 59 => s.unit_allowed(16) && hall,
        60 | 61 => s.unit_allowed(17) && hall,
        74 | 75 => s.unit_allowed(26),
        76 | 77 => s.unit_allowed(18) && hall,
        82 | 83 => s.unit_allowed(29) && hall,
        64 | 65 => s.unit_allowed(25) && hall,
        // Our reading: "a multiplayer game (more than one human player)" is the state's `multiplayer`.
        103 => s.unit_allowed(31) && hall && s.multiplayer(),
        72 | 73 => s.unit_allowed(30) && hall && has(&LUMBER_MILL),
        78 | 79 => s.unit_allowed(21) && hall && has(&SHIPYARD),
        84 | 85 => s.unit_allowed(22) && hall && has(&SHIPYARD),
        68 | 69 => s.unit_allowed(23) && has(&KEEP_OR_CASTLE),
        66 | 67 => s.unit_allowed(19) && has(&KEEP_OR_CASTLE),
        62 | 63 => s.unit_allowed(24) && has(&CASTLE),
        80 | 81 => s.unit_allowed(20) && has(&CASTLE),
        70 | 71 => s.unit_allowed(15) && has(&CASTLE),
        _ => false,
    }
}

/// Whether the walk of "a selected harvester is (not) carrying" finds one.
fn harvester_walk(sel: &[UnitInfo], carrying: bool) -> bool {
    for u in sel {
        if !u.harvester {
            return false;
        }
        if u.carrying == carrying {
            return true;
        }
    }
    false
}

/// Tests an entry's condition against the player, the primary unit and the selection.
pub fn cond_ok(c: Cond, s: &impl PanelState, sel: &[UnitInfo]) -> bool {
    let Some(p) = sel.first() else { return false };
    let one = sel.len() == 1;
    let working = busy(p);
    match c {
        Cond::Never => false,
        Cond::Always => true,
        Cond::One => one,
        Cond::Enhanced => s.enhanced(),
        Cond::LineIs(l, n) => s.level(l) == n,
        Cond::GroundAttack => sel.iter().any(|u| u.ground_attack),
        Cond::HarvesterFree => harvester_walk(sel, false),
        Cond::HarvesterCarrying => harvester_walk(sel, true),
        Cond::Passengers => one && p.passengers > 0,
        Cond::Spell(bit) => one && s.knows(bit),
        Cond::MenuHas(m) => {
            one && set(SetId::Menu(m, p.kind == 2))
                .iter()
                .flat_map(|seg| seg.iter())
                .any(|en| matches!(en.act, Act::Build(_)) && cond_ok(en.cond, s, sel))
        }
        Cond::UnderConstruction => p.building && !p.completed,
        Cond::Training => p.building && p.completed && p.job == Some(JobClass::Training),
        Cond::Working => p.building && p.completed && p.job == Some(JobClass::Other),
        Cond::Unit(k) => train_ok(s, p, k),
        Cond::Building(k) => build_ok(s, k),
        Cond::HallToKeep => HALLS.contains(&p.kind) && ready(p) && s.unit_allowed(27) && s.has_complete(&BARRACKS),
        Cond::KeepToCastle => {
            matches!(p.kind, 88 | 89)
                && ready(p)
                && s.unit_allowed(28)
                && s.has_complete(&STABLES)
                && s.has_complete(&LUMBER_MILL)
                && s.has_complete(&BLACKSMITH)
        }
        // Nothing else: no ALOW, completed or working test (spec, "Job buttons").
        Cond::GuardTower => matches!(p.kind, 64 | 65) && s.has_complete(&LUMBER_MILL),
        Cond::CannonTower => matches!(p.kind, 64 | 65) && s.has_complete(&BLACKSMITH),
        Cond::TwoLevel(l, n) => {
            let bit = two_level_bit(l) + n;
            !working && s.upgrade_allowed(bit) && !s.upgrade_researching(bit) && s.level(l) == n
        }
        Cond::RangerTraining => {
            s.has_complete(&KEEP_OR_CASTLE)
                && !working
                && s.upgrade_allowed(16)
                && !s.upgrade_researching(16)
                && s.level(RANGER_TRAINING) == 0
        }
        Cond::RangerRange => ranger_row(s, working, 17, RANGER_RANGE),
        Cond::RangerScouting => ranger_row(s, working, 18, RANGER_SCOUTING),
        Cond::RangerLast => ranger_row(s, working, 19, RANGER_LAST),
        Cond::Research(bit) => !working && !s.knows(bit) && !s.researching_spell(bit) && s.spell_allowed(bit),
        Cond::ChurchResearch(bit) => cond_ok(Cond::Research(bit), s, sel) && s.knows(PALADIN_BIT),
    }
}

fn ranger_row(s: &impl PanelState, working: bool, bit: u8, l: usize) -> bool {
    s.level(RANGER_TRAINING) > 0 && !working && s.upgrade_allowed(bit) && !s.upgrade_researching(bit) && s.level(l) == 0
}

// --- choosing and filling a set -----------------------------------------------------------------------------------

/// The panel's mode, set by buttons and ended by a click or Cancel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    None,
    /// Choosing a target for a command.
    Target(Target),
    /// Placing a building of this type.
    Place(u8),
    /// A build menu is open.
    Menu(Menu),
}

/// The set a unit shows by itself: the busy set while a building is under construction or working.
fn own_set(u: &UnitInfo) -> SetId {
    if u.building && (!u.completed || busy(u)) {
        SetId::Busy
    } else {
        SetId::Kind(u.kind)
    }
}

/// Picks the set to show (spec "Which set of buttons is shown": the first matching rule wins).
pub fn choose_set(mode: Mode, sel: &[UnitInfo], s: &impl PanelState) -> SetId {
    // 1. Nothing selected, or the primary unit isn't the local player's.
    let Some(p) = sel.first() else { return SetId::Empty };
    if p.owner != s.local() {
        return SetId::Empty;
    }
    match mode {
        // 2 and 3. Our reading: target and placement modes win over the build menu and the mixed set.
        Mode::Target(_) => return SetId::Target,
        Mode::Place(_) => return SetId::Place,
        // 4. A build menu.
        Mode::Menu(m) => return SetId::Menu(m, p.kind == 2),
        Mode::None => {}
    }
    // 5. Several units whose sets differ.
    let own = own_set(p);
    if sel.iter().any(|u| own_set(u) != own) {
        return SetId::Mixed(s.race() == 0);
    }
    // 6. The primary unit's own set.
    own
}

/// The entry shown in each slot: the first entry for the slot whose condition passes.
pub fn fill(id: SetId, s: &impl PanelState, sel: &[UnitInfo]) -> [Option<&'static Entry>; 9] {
    let mut slots: [Option<&'static Entry>; 9] = [None; 9];
    for en in set(id).iter().flat_map(|seg| seg.iter()) {
        let n = en.slot as usize;
        if n < 9 && slots[n].is_none() && cond_ok(en.cond, s, sel) {
            slots[n] = Some(en);
        }
    }
    // Production queue (`docs/design/extras.md`): a building with a running job and room in its queue shows its own
    // buttons, and Cancel (which removes the last waiting job, or cancels the running one) takes the last place.
    if matches!(id, SetId::Kind(_))
        && let Some(p) = sel.first()
        && p.building
        && p.completed
        && p.job.is_some()
        && p.queue.is_some()
        && slots[8].is_none()
    {
        slots[8] = BUSY.iter().find(|en| en.slot == 8 && cond_ok(en.cond, s, sel));
    }
    slots
}

/// Where slot `n` is on the canvas, given the top-left corner of the 176 x 144 panel.
pub fn slot_rect(origin: (i32, i32), n: usize) -> Rect {
    const X: [i32; 3] = [5, 61, 117];
    const Y: [i32; 3] = [0, 47, 94];
    Rect::new(origin.0 + X[n % 3], origin.1 + Y[n / 3], 54, 46)
}

/// The slot under a canvas point.
pub fn slot_at(origin: (i32, i32), p: (f32, f32)) -> Option<usize> {
    (0..9).find(|&n| slot_rect(origin, n).contains(p.0, p.1))
}

// --- string table entries -----------------------------------------------------------------------------------------

/// A button's hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hotkey {
    /// An upper-case letter or digit.
    Char(char),
    /// The Cancel entries' control character.
    Escape,
}

/// A parsed string table entry (spec "Tooltips, hotkeys and costs").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub hotkey: Option<Hotkey>,
    /// 0 text only, 1 unit or building cost, 2 upgrade cost, 3 spell mana.
    pub cost_kind: u8,
    pub text: String,
}

/// Parses the raw text of an entry: first character the hotkey (a control character means Escape, our reading of the
/// spec's open question), second byte the cost kind, the rest the text with formatting bytes dropped.
pub fn parse_label(raw: &str) -> Label {
    let mut chars = raw.chars();
    let hotkey = chars.next().map(|c| {
        if (c as u32) < 0x20 {
            Hotkey::Escape
        } else {
            // Our reading: the comparison ignores case.
            Hotkey::Char(c.to_ascii_uppercase())
        }
    });
    let cost_kind = chars.next().map_or(0, |c| c as u32 as u8);
    let text: String = chars.filter(|c| (*c as u32) >= 0x20).collect();
    // A second byte that is printable is text, not a cost kind (the short entries have no cost byte).
    if cost_kind >= 0x20 {
        let mut all = raw.chars();
        all.next();
        let text: String = all.filter(|c| (*c as u32) >= 0x20).collect();
        return Label {
            hotkey,
            cost_kind: 0,
            text,
        };
    }
    Label {
        hotkey,
        cost_kind,
        text,
    }
}

/// The slot whose button has this hotkey, among the buttons currently shown.
pub fn hotkey_slot(slots: &[Option<&Entry>; 9], label: &dyn Fn(u16) -> Label, key: Hotkey) -> Option<usize> {
    slots
        .iter()
        .position(|en| en.is_some_and(|en| label(en.string).hotkey == Some(key)))
}

/// Mana the button compares for spell command `n` (spec "Button mana cost").
pub fn button_mana(command: u8) -> Option<u8> {
    Some(match command {
        38 => 70,
        39 => 6,
        41 => 4,
        42 => 80,
        43 => 100,
        44 => 50,
        45 => 200,
        46 => 200,
        47 => 25,
        48 => 70,
        49 => 50,
        50 => 50,
        51 => 100,
        52 => 100,
        53 => 50,
        54 => 100,
        55 => 200,
        56 => 25,
        _ => return None,
    })
}

/// A name for the commands whose entries carry only a hotkey, used where the table has no tooltip text.
fn act_name(a: Act) -> &'static str {
    match a {
        Act::Target(Target::Move) => "Move",
        Act::Target(Target::Attack) => "Attack",
        Act::Target(Target::Patrol) => "Patrol",
        Act::Target(Target::Harvest) => "Harvest",
        Act::Target(Target::Repair) => "Repair",
        Act::Target(Target::AttackGround) => "Attack ground",
        Act::Target(Target::Demolish) => "Demolish",
        Act::Target(Target::Unload) | Act::Unload => "Unload",
        Act::Target(Target::Spell(_)) | Act::Cast(_) => "Cast",
        Act::Stop => "Stop",
        Act::StandGround => "Stand ground",
        Act::ReturnGoods => "Return goods",
        Act::Menu(Menu::Basic) => "Basic structures",
        Act::Menu(Menu::Advanced) => "Advanced structures",
        Act::CancelConstruction | Act::CancelJob | Act::Back | Act::LeaveMode => "Cancel",
        _ => "",
    }
}

/// The tooltip of a shown button: its text (the costs go to the status bar's cost items, [`costs`]).
pub fn tooltip(en: &Entry, label: &Label) -> String {
    if label.text.is_empty() {
        act_name(en.act).to_string()
    } else {
        label.text.clone()
    }
}

/// A shown button's costs for the status bar (messages spec, "The status bar"): the cost kind, the gold, lumber and
/// oil costs (unit data for kind 1, upgrade data for kind 2) and the spell's button mana cost (kind 3).
pub fn costs(en: &Entry, label: &Label, s: &impl PanelState) -> (u8, Cost, u32) {
    let cost = match (label.cost_kind, en.act) {
        (1, Act::Build(k) | Act::Train(k) | Act::Become(k)) => s.kind_cost(k),
        (2, Act::Research(n)) => s.upgrade_cost(n),
        _ => [0; 3],
    };
    let mana = match (label.cost_kind, en.act) {
        (3, Act::Cast(n)) => button_mana(n).unwrap_or(0) as u32,
        _ => 0,
    };
    (label.cost_kind, cost, mana)
}

// --- what a button does -------------------------------------------------------------------------------------------

/// The result of activating a button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Nothing,
    /// A command for the sim.
    Send(Command),
    /// A message for the player, by string number.
    Message(u16),
    /// Switch the panel's mode.
    SetMode(Mode),
}

/// Message numbers of the spec's "Data" table.
pub mod msg {
    pub const NOT_ENOUGH_MANA: u16 = 435;
    pub const NOWHERE_TO_RETURN: u16 = 436;
}

/// Activates a shown button for the selection `sel` (non-empty, the local player's).
pub fn activate(en: &Entry, s: &impl PanelState, sel: &[UnitInfo]) -> Effect {
    let Some(p) = sel.first() else { return Effect::Nothing };
    let player = s.local();
    // Commands for the whole selection go to the synced selection, aimed at the primary unit's tile.
    let issue = |command| Command::Issue {
        player,
        command,
        target: None,
        tile: p.tile,
    };
    match en.act {
        Act::Target(t) => Effect::SetMode(Mode::Target(t)),
        Act::Stop => Effect::Send(issue(2)),
        Act::StandGround => Effect::Send(issue(13)),
        Act::ReturnGoods => {
            if s.has_depot(p.id) {
                Effect::Send(issue(24))
            } else {
                Effect::Message(msg::NOWHERE_TO_RETURN)
            }
        }
        Act::Unload => {
            if p.docked {
                // A docked transport unloads on its own tile.
                Effect::Send(issue(29))
            } else {
                Effect::SetMode(Mode::Target(Target::Unload))
            }
        }
        Act::Cast(n) => {
            if !s.all_spells_cheat() && button_mana(n).is_some_and(|m| p.mana < m) {
                Effect::Message(msg::NOT_ENOUGH_MANA)
            } else if n == 48 {
                // Cast at once, on the primary unit's tile.
                Effect::Send(issue(48))
            } else {
                Effect::SetMode(Mode::Target(Target::Spell(n)))
            }
        }
        Act::Build(k) => match s.shortage(s.kind_cost(k), false) {
            Some(m) => Effect::Message(m),
            None => Effect::SetMode(Mode::Place(k)),
        },
        Act::Train(k) => match s.shortage(s.kind_cost(k), true) {
            Some(m) => Effect::Message(m),
            None => Effect::Send(Command::Train {
                player,
                building: p.id,
                kind: k,
            }),
        },
        Act::Research(n) => match s.shortage(s.upgrade_cost(n), false) {
            Some(m) => Effect::Message(m),
            // Upgrade numbers 32 and up are the spells and the paladin and ogre-mage upgrades.
            None if n >= 32 => Effect::Send(Command::Spell {
                player,
                building: p.id,
                spell: n,
            }),
            None => Effect::Send(Command::Research {
                player,
                building: p.id,
                upgrade: n,
            }),
        },
        Act::Become(k) => match s.shortage(s.kind_cost(k), false) {
            Some(m) => Effect::Message(m),
            None => Effect::Send(Command::Upgrade {
                player,
                building: p.id,
                to: k,
            }),
        },
        Act::CancelConstruction => Effect::Send(Command::Cancel { player, unit: p.id }),
        Act::CancelJob => Effect::Send(Command::CancelJob { player, unit: p.id }),
        Act::Menu(m) => Effect::SetMode(Mode::Menu(m)),
        Act::Back | Act::LeaveMode => Effect::SetMode(Mode::None),
    }
}

/// The command number a target-mode button sends (orders spec, "Commands").
pub fn target_number(t: Target) -> u8 {
    match t {
        Target::Move => 3,
        Target::Attack => 8,
        Target::Patrol => 5,
        Target::Harvest => 23,
        Target::Repair => 27,
        Target::AttackGround => 17,
        Target::Demolish => 19,
        Target::Unload => 29,
        Target::Spell(n) => n,
    }
}

/// The command a map click gives in target mode, for the clicked unit or tile. It goes to the synced selection, and
/// the sim's command filter decides which units take it (spec `ui/selecting-and-commanding.md`, "Which units take a
/// command").
pub fn target_command(t: Target, player: u8, hit: Option<UnitId>, tile: (i32, i32)) -> Option<Command> {
    let target = match t {
        // Commands 17 and 19 (attack-ground-demolish spec): attack ground ignores any unit under the cursor.
        Target::AttackGround => None,
        // Our reading: repair without a target does nothing, so nothing is sent (and nothing acknowledged).
        Target::Repair => Some(hit?),
        _ => hit,
    };
    Some(Command::Issue {
        player,
        command: target_number(t),
        target,
        tile,
    })
}

/// What a map click in placement mode comes to (spec `docs/specs/ui/placing-buildings.md`, "The click").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceClick {
    /// The site is allowed: send this build command, play `misc\thunk` and end placement mode.
    Build(Command),
    /// The site fails the check: show this `stat_txt.tbl` message, play `misc\error` and stay in placement mode.
    Refused(u16),
}

/// The site check for the primary unit `builder` placing `kind` with the pointer on `pointer`: the site follows the
/// pointer (sea-layer buildings snap to odd coordinates). Our reading of the open question on the pointer tile: the
/// tile under the pointer's hot spot is the site's top-left tile before the sea-layer rule.
pub fn place_click(world: &World, player: u8, builder: UnitId, kind: u8, pointer: (i32, i32)) -> PlaceClick {
    let site = world.site_at(kind, pointer);
    match world.check_site(kind, site, builder).message() {
        Some(n) => PlaceClick::Refused(n),
        None => PlaceClick::Build(build_command(player, builder, kind, site)),
    }
}

/// The command a map click gives in placement mode: the primary unit builds `kind` at `tile`.
pub fn build_command(player: u8, unit: UnitId, kind: u8, tile: (i32, i32)) -> Command {
    Command::Build {
        player,
        unit,
        kind,
        tile,
    }
}

// --- the sim as panel state ---------------------------------------------------------------------------------------

/// The sim's world seen by the panel, for one local player.
pub struct WorldState<'a> {
    pub world: &'a World,
    pub local: u8,
}

impl WorldState<'_> {
    fn player(&self) -> &tidewater_sim::Player {
        &self.world.players[self.local as usize]
    }
}

fn bit(word: u32, n: u8) -> bool {
    n < 32 && word & (1 << n) != 0
}

impl PanelState for WorldState<'_> {
    fn local(&self) -> u8 {
        self.local
    }

    fn race(&self) -> u8 {
        self.player().race
    }

    fn enhanced(&self) -> bool {
        self.world.rules.enhanced
    }

    fn multiplayer(&self) -> bool {
        self.world.rules.multiplayer
    }

    fn all_spells_cheat(&self) -> bool {
        self.world.rules.all_spells_cheat
    }

    fn unit(&self, id: UnitId) -> Option<UnitInfo> {
        use tidewater_formats::udta::flags;
        let u = self.world.unit(id)?;
        let f = self.world.stats.0.get(u.kind as usize).map_or(0, |s| s.flags);
        Some(UnitInfo {
            id,
            kind: u.kind,
            owner: u.owner,
            tile: u.tile,
            building: u.building,
            completed: u.construction.is_none(),
            job: u.job.map(|j| match j.kind {
                economy::JobKind::Train(_) => JobClass::Training,
                _ => JobClass::Other,
            }),
            queue: self.world.rules.extras.production_queue.then(|| {
                let waiting = self.world.waiting(id);
                let upgrade_last = waiting.last().map_or_else(
                    || matches!(u.job.map(|j| j.kind), Some(economy::JobKind::Upgrade(_))),
                    |q| matches!(q.kind, economy::JobKind::Upgrade(_)),
                );
                QueueInfo {
                    waiting: waiting.len() as u8,
                    open: 1 + waiting.len() < tidewater_sim::queue::QUEUE_MAX && !upgrade_last,
                }
            }),
            harvester: f & (flags::PEON | flags::TANKER) != 0,
            carrying: u.carrying,
            ground_attack: f & flags::GROUND_ATTACK != 0,
            passengers: u.passengers.iter().flatten().count() as u8,
            docked: u.order == Order::Docked || u.pending == Some(Order::Docked),
            mana: u.mana,
        })
    }

    fn level(&self, l: usize) -> u8 {
        self.player().levels.get(l).copied().unwrap_or(0)
    }

    fn knows(&self, n: u8) -> bool {
        bit(self.player().spells_known, n)
    }

    fn researching_spell(&self, n: u8) -> bool {
        bit(self.player().spells_researching, n)
    }

    fn spell_allowed(&self, n: u8) -> bool {
        bit(self.player().spells_allowed, n)
    }

    fn unit_allowed(&self, n: u8) -> bool {
        bit(self.player().units_allowed, n)
    }

    fn upgrade_allowed(&self, n: u8) -> bool {
        bit(self.player().upgrades_allowed, n)
    }

    fn upgrade_researching(&self, n: u8) -> bool {
        bit(self.player().upgrades_researching, n)
    }

    fn has_complete(&self, kinds: &[u8]) -> bool {
        self.world.units.iter().any(|u| {
            u.owner == self.local && u.building && !u.dying && u.construction.is_none() && kinds.contains(&u.kind)
        })
    }

    fn had_hall(&self) -> bool {
        self.player().had_hall
    }

    fn kind_cost(&self, kind: u8) -> Cost {
        self.world.stats.0.get(kind as usize).map_or([0; 3], economy::cost)
    }

    fn upgrade_cost(&self, n: u8) -> Cost {
        self.world
            .upgrade_data
            .0
            .get(n as usize)
            .map_or([0; 3], |u| [u.gold as u32, u.lumber as u32, u.oil as u32])
    }

    fn shortage(&self, cost: Cost, needs_food: bool) -> Option<u16> {
        self.world
            .can_afford(self.local, cost, needs_food)
            .err()
            .map(crate::messages::shortage_entry)
    }

    fn has_depot(&self, id: UnitId) -> bool {
        self.world.has_depot_for(id)
    }

    fn footprint(&self, kind: u8) -> (u8, u8) {
        self.world
            .stats
            .0
            .get(kind as usize)
            .map_or((1, 1), |s| (s.size.0 as u8, s.size.1 as u8))
    }
}

// --- tests --------------------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(Default)]
    struct Fake {
        race: u8,
        enhanced: bool,
        multiplayer: bool,
        levels: [u8; 11],
        known: u32,
        spells_allowed: u32,
        spells_researching: u32,
        units_allowed: u32,
        upgrades_allowed: u32,
        upgrades_researching: u32,
        have: HashSet<u8>,
        hall: bool,
        gold: u32,
        depot: bool,
    }

    impl Fake {
        fn all() -> Self {
            Self {
                units_allowed: u32::MAX,
                upgrades_allowed: u32::MAX,
                spells_allowed: u32::MAX,
                gold: 100_000,
                ..Self::default()
            }
        }
    }

    impl PanelState for Fake {
        fn local(&self) -> u8 {
            0
        }
        fn race(&self) -> u8 {
            self.race
        }
        fn enhanced(&self) -> bool {
            self.enhanced
        }
        fn multiplayer(&self) -> bool {
            self.multiplayer
        }
        fn unit(&self, _: UnitId) -> Option<UnitInfo> {
            None
        }
        fn level(&self, l: usize) -> u8 {
            self.levels[l]
        }
        fn knows(&self, n: u8) -> bool {
            bit(self.known, n)
        }
        fn researching_spell(&self, n: u8) -> bool {
            bit(self.spells_researching, n)
        }
        fn spell_allowed(&self, n: u8) -> bool {
            bit(self.spells_allowed, n)
        }
        fn unit_allowed(&self, n: u8) -> bool {
            bit(self.units_allowed, n)
        }
        fn upgrade_allowed(&self, n: u8) -> bool {
            bit(self.upgrades_allowed, n)
        }
        fn upgrade_researching(&self, n: u8) -> bool {
            bit(self.upgrades_researching, n)
        }
        fn has_complete(&self, kinds: &[u8]) -> bool {
            kinds.iter().any(|k| self.have.contains(k))
        }
        fn had_hall(&self) -> bool {
            self.hall
        }
        fn kind_cost(&self, _: u8) -> Cost {
            [400, 0, 0]
        }
        fn upgrade_cost(&self, _: u8) -> Cost {
            [800, 100, 0]
        }
        fn shortage(&self, c: Cost, _: bool) -> Option<u16> {
            (self.gold < c[0]).then_some(439)
        }
        fn has_depot(&self, _: UnitId) -> bool {
            self.depot
        }
        fn footprint(&self, _: u8) -> (u8, u8) {
            (2, 2)
        }
    }

    fn unit(id: u32, kind: u8) -> UnitInfo {
        UnitInfo {
            id: UnitId(id),
            kind,
            owner: 0,
            tile: (3, 4),
            building: kind >= 58,
            completed: true,
            job: None,
            queue: None,
            harvester: matches!(kind, 2 | 3 | 26 | 27),
            carrying: false,
            ground_attack: matches!(kind, 4 | 5 | 32 | 33),
            passengers: 0,
            docked: false,
            mana: 0,
        }
    }

    fn icons(s: &Fake, mode: Mode, sel: &[UnitInfo]) -> Vec<Option<u16>> {
        let id = choose_set(mode, sel, s);
        fill(id, s, sel).iter().map(|e| e.map(|e| e.icon)).collect()
    }

    fn strings(s: &Fake, mode: Mode, sel: &[UnitInfo]) -> Vec<Option<u16>> {
        let id = choose_set(mode, sel, s);
        fill(id, s, sel).iter().map(|e| e.map(|e| e.string)).collect()
    }

    #[test]
    fn footman_with_shield_one() {
        let mut s = Fake::all();
        s.levels[SHIELD] = 1;
        let got = icons(&s, Mode::None, &[unit(1, 0)]);
        assert_eq!(
            got,
            [
                Some(83),
                Some(165),
                Some(116),
                Some(178),
                Some(180),
                None,
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn other_players_unit_has_no_buttons() {
        let s = Fake::all();
        let mut u = unit(1, 0);
        u.owner = 1;
        assert!(icons(&s, Mode::None, &[u]).iter().all(Option::is_none));
    }

    #[test]
    fn mixed_selection_follows_types() {
        let s = Fake::all();
        let both = [unit(1, 0), unit(2, 4)];
        assert_eq!(choose_set(Mode::None, &both, &s), SetId::Mixed(true));
        assert_eq!(icons(&s, Mode::None, &both)[5], Some(182));
        let same = [unit(1, 0), unit(2, 0)];
        assert_eq!(choose_set(Mode::None, &same, &s), SetId::Kind(0));
        let foot_peasant = [unit(1, 0), unit(2, 2)];
        let got = icons(&s, Mode::None, &foot_peasant);
        assert_eq!(got[5], None);
        assert_eq!(got[2], Some(116));
        // A peasant first and a footman second: still mixed, and no Harvest.
        let mut p = unit(1, 2);
        p.carrying = false;
        let got = icons(&s, Mode::None, &[p, unit(2, 0)]);
        assert_eq!(got[4], Some(180));
    }

    #[test]
    fn peasant_carrying_and_harvest_walk() {
        let mut s = Fake::all();
        s.hall = true;
        let mut p = unit(1, 2);
        p.carrying = true;
        let got = icons(&s, Mode::None, &[p]);
        assert_eq!(got[4], None);
        assert_eq!(got[5], Some(89));
        // Three peasants, the first carrying, the second not: both buttons, no build menus.
        let mut a = unit(1, 2);
        a.carrying = true;
        let got = icons(&s, Mode::None, &[a, unit(2, 2), unit(3, 2)]);
        assert_eq!(got[4], Some(86));
        assert_eq!(got[5], Some(89));
        assert_eq!(got[6], None);
        assert_eq!(got[7], None);
    }

    #[test]
    fn build_menus() {
        let mut s = Fake::all();
        let p = [unit(1, 2)];
        // Never had a hall: only the town hall; the advanced button is hidden.
        let set_id = choose_set(Mode::Menu(Menu::Basic), &p, &s);
        let got = fill(set_id, &s, &p);
        let shown: Vec<usize> = (0..9).filter(|&i| got[i].is_some()).collect();
        assert_eq!(shown, [2, 8]);
        assert_eq!(icons(&s, Mode::None, &p)[7], None);
        assert!(icons(&s, Mode::None, &p)[6].is_some());
        // A castle and a lumber mill, single player.
        s.hall = true;
        s.have = HashSet::from([90, 76]);
        let basic = fill(SetId::Menu(Menu::Basic, true), &s, &p);
        let shown: Vec<usize> = (0..9).filter(|&i| basic[i].is_some()).collect();
        assert_eq!(shown, [0, 1, 2, 3, 4, 6, 8]);
        let adv = fill(SetId::Menu(Menu::Advanced, true), &s, &p);
        let shown: Vec<usize> = (0..9).filter(|&i| adv[i].is_some()).collect();
        // Shipyard, inventor, stables, church, mage tower, aviary; no foundry or refinery.
        assert_eq!(shown, [0, 3, 4, 5, 6, 7, 8]);
        s.multiplayer = true;
        let basic = fill(SetId::Menu(Menu::Basic, true), &s, &p);
        assert!(basic[7].is_some());
    }

    #[test]
    fn barracks_buttons() {
        let mut s = Fake::all();
        s.have = HashSet::from([76]);
        let b = [unit(1, 60)];
        let got = icons(&s, Mode::None, &b);
        assert_eq!(&got[..4], &[Some(2), Some(4), None, None]);
        s.levels[RANGER_TRAINING] = 1;
        assert_eq!(icons(&s, Mode::None, &b)[1], Some(6));
        // Working: only Cancel, string 409 for training.
        let mut w = unit(1, 60);
        w.job = Some(JobClass::Training);
        let st = strings(&s, Mode::None, &[w]);
        assert_eq!(st[8], Some(409));
        assert_eq!(st.iter().flatten().count(), 1);
        w.job = Some(JobClass::Other);
        assert_eq!(strings(&s, Mode::None, &[w])[8], Some(410));
        let mut c = unit(1, 76);
        c.completed = false;
        assert_eq!(strings(&s, Mode::None, &[c])[8], Some(408));
    }

    #[test]
    fn a_building_with_room_in_its_queue_keeps_its_buttons_and_gets_cancel() {
        let mut s = Fake::all();
        s.have = HashSet::from([76]);
        let mut w = unit(1, 60);
        w.job = Some(JobClass::Training);
        // The option off: only Cancel, as always.
        assert_eq!(strings(&s, Mode::None, &[w]).iter().flatten().count(), 1);
        // On, with room: the training buttons and Cancel.
        w.queue = Some(QueueInfo { waiting: 1, open: true });
        let got = icons(&s, Mode::None, &[w]);
        assert_eq!(&got[..2], &[Some(2), Some(4)]);
        assert_eq!(strings(&s, Mode::None, &[w])[8], Some(409));
        // On, full: only Cancel again.
        w.queue = Some(QueueInfo {
            waiting: 4,
            open: false,
        });
        let st = strings(&s, Mode::None, &[w]);
        assert_eq!(st.iter().flatten().count(), 1);
        assert_eq!(st[8], Some(409));
        // On and idle: no Cancel.
        let mut idle = unit(1, 60);
        idle.queue = Some(QueueInfo::default());
        assert_eq!(strings(&s, Mode::None, &[idle])[8], None);
    }

    #[test]
    fn blacksmith_levels() {
        let mut s = Fake::all();
        s.levels[WEAPON] = 1;
        let b = [unit(1, 82)];
        assert_eq!(icons(&s, Mode::None, &b)[0], Some(118));
        s.levels[WEAPON] = 2;
        assert_eq!(icons(&s, Mode::None, &b)[0], None);
    }

    #[test]
    fn arrow_two_can_start_twice() {
        // Level 1 reached, upgrade bit 1 still allowed and not marked as being researched.
        let mut s = Fake::all();
        s.levels[MISSILE] = 1;
        s.upgrades_researching = 1;
        let m = [unit(1, 76)];
        assert_eq!(icons(&s, Mode::None, &m)[0], Some(126));
    }

    #[test]
    fn spells_need_one_unit_and_known() {
        let mut s = Fake::all();
        s.known = (1 << 5) | (1 << 6);
        let mage = [unit(1, 10)];
        let got = icons(&s, Mode::None, &mage);
        assert_eq!(&got[3..], &[Some(101), Some(94), None, None, None, None]);
        let two = [unit(1, 10), unit(2, 10)];
        assert_eq!(&icons(&s, Mode::None, &two)[3..], &[None; 6]);
        // Cho'gall's spells are always shown.
        s = Fake::all();
        s.race = 1;
        let got = icons(&s, Mode::None, &[unit(1, 49)]);
        assert_eq!(&got[6..], &[Some(111), Some(112), Some(97)]);
    }

    #[test]
    fn casting_checks_mana() {
        let s = Fake {
            known: u32::MAX,
            ..Fake::all()
        };
        let mut mage = unit(1, 10);
        mage.mana = 99;
        let fireball = Entry {
            act: Act::Cast(43),
            ..MAGE_PART[3]
        };
        assert_eq!(activate(&fireball, &s, &[mage]), Effect::Message(msg::NOT_ENOUGH_MANA));
        mage.mana = 100;
        assert_eq!(
            activate(&fireball, &s, &[mage]),
            Effect::SetMode(Mode::Target(Target::Spell(43)))
        );
        // Eye of Kilrogg is sent at once, on the primary unit's tile.
        mage.mana = 70;
        let eye = Entry {
            act: Act::Cast(48),
            ..fireball
        };
        assert_eq!(
            activate(&eye, &s, &[mage]),
            Effect::Send(Command::Issue {
                player: 0,
                command: 48,
                target: None,
                tile: mage.tile
            })
        );
    }

    #[test]
    fn transports_unload() {
        let s = Fake::all();
        let mut t = unit(1, 28);
        assert_eq!(icons(&s, Mode::None, &[t])[2], None);
        t.passengers = 2;
        assert_eq!(icons(&s, Mode::None, &[t])[2], Some(162));
        let unload = &H_UNLOAD[0];
        assert_eq!(
            activate(unload, &s, &[t]),
            Effect::SetMode(Mode::Target(Target::Unload))
        );
        t.docked = true;
        assert_eq!(
            activate(unload, &s, &[t]),
            Effect::Send(Command::Issue {
                player: 0,
                command: 29,
                target: None,
                tile: (3, 4)
            })
        );
    }

    #[test]
    fn return_goods_needs_a_depot() {
        let mut s = Fake::all();
        let mut p = unit(1, 2);
        p.carrying = true;
        let en = Entry {
            act: Act::ReturnGoods,
            ..PEASANT[5]
        };
        assert_eq!(activate(&en, &s, &[p]), Effect::Message(msg::NOWHERE_TO_RETURN));
        s.depot = true;
        assert!(matches!(
            activate(&en, &s, &[p]),
            Effect::Send(Command::Issue { command: 24, .. })
        ));
    }

    #[test]
    fn hall_and_tower_upgrades() {
        let mut s = Fake::all();
        s.have = HashSet::from([60]);
        let hall = [unit(1, 74)];
        assert_eq!(&icons(&s, Mode::None, &hall)[..2], &[Some(0), Some(66)]);
        let mut keep = unit(1, 88);
        s.have = HashSet::from([66, 76, 82]);
        assert_eq!(&icons(&s, Mode::None, &[keep])[..2], &[Some(0), Some(68)]);
        keep.kind = 90;
        assert_eq!(&icons(&s, Mode::None, &[keep])[..2], &[Some(0), None]);
        // A scout tower with a blacksmith but no lumber mill offers the cannon tower.
        s.have = HashSet::from([82]);
        assert_eq!(icons(&s, Mode::None, &[unit(1, 64)])[0], Some(76));
        // A guard tower has no buttons.
        assert!(icons(&s, Mode::None, &[unit(1, 96)]).iter().all(Option::is_none));
    }

    #[test]
    fn cancel_sets() {
        let s = Fake::all();
        let sel = [unit(1, 0)];
        let t = fill(choose_set(Mode::Target(Target::Move), &sel, &s), &s, &sel);
        assert_eq!(t.iter().flatten().count(), 1);
        assert_eq!(t[8].map(|e| e.string), Some(407));
        let p = [unit(1, 2)];
        let m = fill(choose_set(Mode::Menu(Menu::Basic), &p, &s), &s, &p);
        assert_eq!(m[8].map(|e| e.act), Some(Act::Back));
    }

    #[test]
    fn train_and_research_commands() {
        let mut s = Fake::all();
        let b = [unit(7, 60)];
        let en = Entry {
            act: Act::Train(0),
            ..H_BARRACKS[0]
        };
        assert_eq!(
            activate(&en, &s, &b),
            Effect::Send(Command::Train {
                player: 0,
                building: UnitId(7),
                kind: 0
            })
        );
        s.gold = 100;
        assert_eq!(activate(&en, &s, &b), Effect::Message(439));
        s.gold = 100_000;
        let r = Entry {
            act: Act::Research(2),
            ..H_SMITH[0]
        };
        assert!(matches!(
            activate(&r, &s, &b),
            Effect::Send(Command::Research { upgrade: 2, .. })
        ));
        let sp = Entry {
            act: Act::Research(33),
            ..H_SMITH[0]
        };
        assert!(matches!(
            activate(&sp, &s, &b),
            Effect::Send(Command::Spell { spell: 33, .. })
        ));
    }

    #[test]
    fn labels() {
        let l = parse_label("a\u{2}RESEARCH \u{4}A\u{1}RMS");
        assert_eq!(l.hotkey, Some(Hotkey::Char('A')));
        assert_eq!(l.cost_kind, 2);
        assert_eq!(l.text, "RESEARCH ARMS");
        let l = parse_label("s");
        assert_eq!(
            (l.hotkey, l.cost_kind, l.text.as_str()),
            (Some(Hotkey::Char('S')), 0, "")
        );
        let l = parse_label("\u{10}");
        assert_eq!(l.hotkey, Some(Hotkey::Escape));
    }

    #[test]
    fn hotkeys_match_shown_buttons_only() {
        let s = Fake::all();
        let sel = [unit(1, 0)];
        let slots = fill(choose_set(Mode::None, &sel, &s), &s, &sel);
        let label = |n: u16| match n {
            392 => parse_label("m"),
            393 => parse_label("s"),
            _ => parse_label("z"),
        };
        assert_eq!(hotkey_slot(&slots, &label, Hotkey::Char('S')), Some(1));
        assert_eq!(hotkey_slot(&slots, &label, Hotkey::Char('Q')), None);
    }

    #[test]
    fn slots_sit_where_the_spec_says() {
        assert_eq!(slot_rect((0, 336), 0), Rect::new(5, 336, 54, 46));
        assert_eq!(slot_rect((0, 336), 5), Rect::new(117, 383, 54, 46));
        assert_eq!(slot_rect((0, 336), 8), Rect::new(117, 430, 54, 46));
        assert_eq!(slot_at((0, 336), (62.0, 340.0)), Some(1));
        assert_eq!(slot_at((0, 336), (2.0, 340.0)), None);
    }

    #[test]
    fn tooltips_and_costs() {
        let s = Fake::all();
        let train = &H_BARRACKS[0];
        let l = parse_label("f\u{1}TRAIN FOOTMAN");
        assert_eq!(tooltip(train, &l), "TRAIN FOOTMAN");
        assert_eq!(costs(train, &l, &s), (1, [400, 0, 0], 0));
        let l = parse_label("s");
        assert_eq!(tooltip(&MAGE_PART[1], &l), "Stop");
        assert_eq!(costs(&MAGE_PART[1], &l, &s), (0, [0; 3], 0));
    }

    #[test]
    fn target_clicks() {
        let issue = |command, target, tile| {
            Some(Command::Issue {
                player: 0,
                command,
                target,
                tile,
            })
        };
        let foe = Some(UnitId(9));
        assert_eq!(target_command(Target::Attack, 0, foe, (1, 1)), issue(8, foe, (1, 1)));
        assert_eq!(target_command(Target::Attack, 0, None, (1, 1)), issue(8, None, (1, 1)));
        assert_eq!(target_command(Target::Repair, 0, None, (1, 1)), None);
        assert_eq!(target_command(Target::Repair, 0, foe, (1, 1)), issue(27, foe, (1, 1)));
        assert_eq!(
            target_command(Target::AttackGround, 0, foe, (1, 1)),
            issue(17, None, (1, 1)),
            "attack ground ignores the unit under the cursor"
        );
        assert_eq!(target_command(Target::Demolish, 0, foe, (1, 1)), issue(19, foe, (1, 1)));
        assert_eq!(
            target_command(Target::Spell(43), 0, foe, (2, 3)),
            issue(43, foe, (2, 3))
        );
        assert_eq!(target_command(Target::Move, 0, None, (2, 3)), issue(3, None, (2, 3)));
        assert_eq!(target_command(Target::Patrol, 0, None, (2, 3)), issue(5, None, (2, 3)));
        assert_eq!(
            target_command(Target::Harvest, 0, None, (2, 3)),
            issue(23, None, (2, 3))
        );
        assert_eq!(target_command(Target::Unload, 0, None, (2, 3)), issue(29, None, (2, 3)));
    }
}
