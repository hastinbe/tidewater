//! The unit information panel: the 176 x 176 box above the command buttons that shows the selected units (spec
//! `docs/specs/ui/info-panel.md`).
//!
//! This module holds the panel's rules as plain functions over plain data: the element positions, the per-type panel
//! table (class, panel kind, icon, name entry), the grid's two-pass slot assignment, what an icon click does, the
//! name-line wrapping, the level line, the statistic rows with their bonuses, the building cases, the progress and
//! health bar arithmetic, the speeds worked out from the animation scripts, and the per-type change checks. It reads
//! the sim only in [`facts`], [`owner_facts`] and [`watch`], and draws nothing: `hud` draws, `main` owns the input.
//! No SDL types here.
//!
//! Element positions are the spec's table ("Panel elements"), which gives the layout `statport.bin` holds; the client
//! uses the spec's numbers rather than reading that file. Our reading of the spec where it is open is marked
//! "Open question:" at the place it applies.

use tidewater_formats::script::{Op, Scripts, anim};
use tidewater_formats::udta::{UnitStats, flags};
use tidewater_formats::ugrd::UpgradeTable;
use tidewater_sim::economy::{JobKind, kind as k};
use tidewater_sim::{UnitId, World};

use crate::hud::Rect;

// --- positions (spec "Panel elements"), relative to the panel's top-left corner --------------------------------------

// The panel itself sits at (0, 160) on the 640 x 480 screen: `hud::Layout::info`.

/// Grid slot columns and rows.
pub const SLOT_X: [i32; 3] = [5, 61, 117];
pub const SLOT_Y: [i32; 3] = [5, 60, 115];
pub const SLOT_W: u32 = 54;
pub const SLOT_H: u32 = 53;
/// The two name lines and the level line, centred.
pub const NAME: [Rect; 2] = [Rect::new(60, 10, 109, 14), Rect::new(60, 25, 109, 14)];
pub const LEVEL: Rect = Rect::new(60, 40, 109, 14);
/// The hit point text under the portrait, centred.
pub const HP: Rect = Rect::new(9, 60, 54, 10);
/// The six statistic rows' tops.
pub const ROW_Y: [i32; 6] = [72, 87, 102, 117, 132, 147];
pub const PRODUCTION: Rect = Rect::new(107, 78, 54, 46);
/// The first waiting job's quarter-size icon in the production queue, relative to the production icon, and the gap
/// between icons (option `production-queue`).
pub const QUEUE_ICON: Rect = Rect::new(2, 49, 11, 9);
pub const QUEUE_GAP: i32 = 3;
pub const PROGRESS: Rect = Rect::new(12, 153, 152, 14);
/// The mana bar inside row 5's value area.
pub const MANA_BAR: Rect = Rect::new(107, 149, 59, 10);
/// The name line width limit.
pub const NAME_WIDTH: u32 = 109;

/// A grid slot's rectangle.
pub fn slot_rect(n: usize) -> Rect {
    Rect::new(SLOT_X[n % 3], SLOT_Y[n / 3], SLOT_W, SLOT_H)
}

/// Row `n`'s label area.
pub fn label_rect(n: usize) -> Rect {
    Rect::new(9, ROW_Y[n], 94, 14)
}

/// Row `n`'s value area.
pub fn value_rect(n: usize) -> Rect {
    Rect::new(105, ROW_Y[n], 63, 14)
}

/// The slot under a point relative to the panel.
pub fn slot_at(p: (f32, f32)) -> Option<usize> {
    (0..9).find(|&n| slot_rect(n).contains(p.0, p.1))
}

// --- the per-type table (spec "Data") -------------------------------------------------------------------------------

/// The panel class of a type, which picks its rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Melee,
    Siege,
    Archer,
    Sea,
    Transport,
    Bare,
    /// No panel class ("none" in the spec's table).
    Nothing,
    Farm,
    Training,
    Church,
    ScoutTower,
    Plain,
    Shipyard,
    Hall,
    LumberMill,
    Smith,
    MageTower,
    Refinery,
    OilSource,
    GoldMine,
    Tower,
}

/// The class and panel kind of a type; `None` past the spec's table (0x69).
pub fn class(t: u8) -> Option<(Class, u8)> {
    use Class::*;
    Some(match t {
        0x0A..=0x0D | 0x15 | 0x18 => (Melee, 2),
        0x00 | 0x01 | 0x06 | 0x07 | 0x0E | 0x0F | 0x16 | 0x17 | 0x19 | 0x23 | 0x28..=0x2F | 0x31..=0x34 | 0x37 => {
            (Melee, 1)
        }
        0x02 | 0x03 | 0x10 | 0x11 => (Melee, 1),
        0x04 | 0x05 => (Siege, 1),
        0x08 | 0x09 | 0x12 | 0x13 | 0x14 | 0x35 => (Archer, 1),
        0x1A | 0x1B | 0x1E..=0x21 | 0x26 | 0x27 => (Sea, 1),
        0x1C | 0x1D => (Transport, 1),
        0x22 | 0x24 | 0x25 | 0x38 | 0x39 => (Bare, 1),
        0x30 | 0x36 | 0x5E | 0x5F | 0x69 => (Nothing, 0),
        0x3A | 0x3B => (Farm, 1),
        0x3C | 0x3D | 0x44..=0x47 => (Training, 3),
        0x3E | 0x3F => (Church, 3),
        0x40 | 0x41 => (ScoutTower, 3),
        0x42 | 0x43 | 0x65 | 0x67 | 0x68 => (Plain, 3),
        0x64 | 0x66 => (Plain, 1),
        0x48 | 0x49 => (Shipyard, 3),
        0x4A | 0x4B | 0x58..=0x5B => (Hall, 3),
        0x4C | 0x4D => (LumberMill, 3),
        0x4E | 0x4F | 0x52 | 0x53 => (Smith, 3),
        0x50 | 0x51 => (MageTower, 3),
        0x54 | 0x55 => (Refinery, 3),
        0x56 | 0x57 | 0x5D => (OilSource, 3),
        0x5C => (GoldMine, 1),
        0x60..=0x63 => (Tower, 3),
        _ => return None,
    })
}

/// The class, with types past the table counted as having none.
fn class_of(t: u8) -> (Class, u8) {
    class(t).unwrap_or((Class::Nothing, 0))
}

/// A type's icon in the era's icon set ("Icons"); `None` past the table.
pub fn icon(t: u8) -> Option<u16> {
    #[rustfmt::skip]
    const ICONS: [u16; 106] = [
        2, 3, 0, 1, 16, 17, 8, 9, 4, 5, 14, 15, 10, 11, 12, 13,
        0, 1, 6, 7, 187, 189, 191, 194, 193, 190, 18, 19, 20, 21, 22, 23,
        24, 25, 0, 192, 0, 0, 26, 27, 28, 29, 30, 31, 195, 111, 188, 186,
        0, 36, 32, 33, 34, 35, 0, 114, 37, 115, 38, 39, 42, 43, 62, 63,
        60, 61, 56, 57, 58, 59, 72, 73, 48, 49, 40, 41, 44, 45, 52, 53,
        64, 65, 46, 47, 50, 51, 54, 55, 66, 67, 70, 71, 74, 79, 0, 0,
        75, 77, 76, 78, 81, 80, 82, 92, 93, 0,
    ];
    ICONS.get(t as usize).copied()
}

/// A type's name entry in `stat_txt.tbl` ("Name entry exceptions"); `None` for the types without one.
pub fn name_entry(t: u8) -> Option<u16> {
    Some(match t {
        0x30 | 0x36 | 0x5E | 0x5F => return None,
        0x0E => 244,
        0x10 => 4,
        0x11 => 5,
        0x1A => 245,
        0x1B => 246,
        0x1C => 247,
        0x1D => 248,
        0x22 | 0x24 | 0x25 | 0x69 => 5,
        0x2D => 267,
        0x3C => 249,
        0x3D => 250,
        0x48 => 251,
        0x49 => 252,
        0x4E => 253,
        0x4F => 254,
        0x50 => 261,
        0x51 => 262,
        0x52 => 255,
        0x53 => 256,
        0x54 => 257,
        0x55 => 258,
        0x56 => 259,
        0x57 => 260,
        0x60..=0x63 => 263 + (t - 0x60) as u16,
        0x68 => 105,
        _ => t as u16 + 2,
    })
}

// --- the background frame ---------------------------------------------------------------------------------------------

/// The frame of `hframe.grp` / `oframe.grp` to show (spec "Background frame"). `one` is the single selected unit, if
/// exactly one is selected.
pub fn frame(one: Option<&Subject>) -> u8 {
    let Some(s) = one.filter(|s| s.own) else { return 0 };
    if s.busy() {
        return 3;
    }
    match class_of(s.kind).1 {
        2 => 2,
        1 | 3 => 1,
        _ => 0,
    }
}

// --- the grid ---------------------------------------------------------------------------------------------------------

/// The grid's slots: which unit each shows.
pub type Slots = [Option<UnitId>; 9];

/// How many icons the grid has.
pub const SLOTS: usize = 9;

/// The two-pass slot assignment (spec "What the panel shows"): units already in a slot keep it, the rest take the
/// lowest empty slots. One unit always moves to slot 0. Units past nine get no slot.
pub fn assign(prev: &Slots, sel: &[UnitId]) -> Slots {
    let mut out: Slots = [None; 9];
    if let [one] = sel {
        out[0] = Some(*one);
        return out;
    }
    for (n, s) in prev.iter().enumerate() {
        if let Some(id) = s
            && sel.contains(id)
        {
            out[n] = Some(*id);
        }
    }
    for &id in sel {
        if out.contains(&Some(id)) {
            continue;
        }
        if let Some(free) = out.iter_mut().find(|s| s.is_none()) {
            *free = Some(id);
        }
    }
    out
}

/// The selection in slot order; its first unit is the primary unit (lowest filled slot).
pub fn in_slot_order(slots: &Slots) -> Vec<UnitId> {
    slots.iter().flatten().copied().collect()
}

/// Fills slots 3 to 8 with an own transport's passengers (places 1 to 6); an empty place leaves its slot empty.
pub fn with_passengers(slots: &mut Slots, passengers: &[Option<UnitId>; 6]) {
    for (i, p) in passengers.iter().enumerate() {
        slots[3 + i] = *p;
    }
}

/// What a released click on a filled grid slot does (spec "Clicking an icon").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    /// Several units selected: the clicked one becomes the only one.
    SelectOnly(UnitId),
    /// Several units selected, Shift held: the clicked one leaves the selection.
    Remove(UnitId),
    /// One transport selected: unload the passenger in this place (0 to 5).
    Unload {
        transport: UnitId,
        place: usize,
    },
    /// One unit selected: centre the view on it, if it is own or isn't invisible.
    Centre(UnitId),
    Nothing,
}

/// The click on slot `n`. `selected` is the number of selected units; `transport` whether the one selected unit is a
/// transport (unit flag bit 10); `centre_ok` whether it is own or not invisible.
pub fn click(slots: &Slots, n: usize, selected: usize, shift: bool, transport: bool, centre_ok: bool) -> Click {
    let Some(Some(id)) = slots.get(n).copied() else {
        return Click::Nothing;
    };
    if selected > 1 {
        return if shift {
            Click::Remove(id)
        } else {
            Click::SelectOnly(id)
        };
    }
    let Some(first) = slots[0] else { return Click::Nothing };
    if transport && n != 0 {
        // Open question: the request is the one-passenger unload of `orders/transports.md`. Slots 3 to 8 hold
        // places 1 to 6.
        return match n.checked_sub(3) {
            Some(place) => Click::Unload {
                transport: first,
                place,
            },
            None => Click::Nothing,
        };
    }
    if centre_ok {
        Click::Centre(first)
    } else {
        Click::Nothing
    }
}

// --- name lines -------------------------------------------------------------------------------------------------------

/// Splits a name over the two name lines (spec "Name lines"); `width` measures text in the panel font.
pub fn name_lines(name: &str, width: &dyn Fn(&[u8]) -> u32) -> (String, String) {
    let b = name.as_bytes();
    if width(b) <= NAME_WIDTH {
        return (String::new(), name.to_string());
    }
    let (line1, rest): (&[u8], &[u8]) = match b.iter().take(20).position(|&c| c == b' ') {
        Some(sp) if width(&b[..sp]) <= NAME_WIDTH => (&b[..sp], &b[sp + 1..]),
        _ => {
            // Hyphenate: the longest start (at most 18 characters) that with a hyphen is under the limit.
            let mut n = b.len().min(18);
            while n > 0 {
                let mut t = b[..n].to_vec();
                t.push(b'-');
                if width(&t) < NAME_WIDTH {
                    break;
                }
                n -= 1;
            }
            let mut first = b[..n].to_vec();
            first.push(b'-');
            return (lossy(&first), cut_line2(&b[n..], width));
        }
    };
    (lossy(line1), cut_line2(rest, width))
}

fn cut_line2(rest: &[u8], width: &dyn Fn(&[u8]) -> u32) -> String {
    let mut s = &rest[..rest.len().min(19)];
    while !s.is_empty() && width(s) > NAME_WIDTH {
        s = &s[..s.len() - 1];
    }
    lossy(s)
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

// --- what the panel reads -----------------------------------------------------------------------------------------------

/// The owner's state the panel reads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Owner {
    /// Upgrade group levels 0 to 10 (`economy/tech-tree.md`).
    pub levels: [u8; 11],
    /// Spells known, by spell bit.
    pub spells: u32,
    /// Has a completed castle or fortress; keep or stronghold; lumber mill; refinery.
    pub castle: bool,
    pub keep: bool,
    pub lumber_mill: bool,
    pub refinery: bool,
    /// Food supply (already capped at 200 in Enhanced mode) and food in use.
    pub food_supply: u32,
    pub food_used: u32,
}

impl Owner {
    fn knows(&self, bit: u8) -> u32 {
        (bit < 32 && self.spells & (1 << bit) != 0) as u32
    }

    fn level(&self, group: usize) -> u32 {
        self.levels[group] as u32
    }
}

/// The one selected unit as the panel reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Subject {
    pub kind: u8,
    /// Owned by the local player.
    pub own: bool,
    pub building: bool,
    pub hp: u16,
    pub mana: u8,
    /// The shown construction progress (0 to 255) while under construction.
    pub construction: Option<u8>,
    /// The building's job, if it is working on one.
    pub job: Option<JobKind>,
    /// The jobs waiting behind it (option `production-queue`), next first.
    pub waiting: [Option<JobKind>; 4],
    pub passengers: u8,
    /// Trips left in a mine, patch or platform.
    pub trips: u32,
}

impl Subject {
    /// An own building that is working or under construction (the frame 3 condition).
    pub fn busy(&self) -> bool {
        self.own && self.building && (self.construction.is_some() || self.job.is_some())
    }

    /// The shown progress p, 0 to 255 (spec "Progress bar"). A job's is the building's mana byte (round 76, V52): 1
    /// from the job's start until its first run, then the job's share of 255.
    pub fn progress(&self) -> Option<u8> {
        if let Some(p) = self.construction {
            return Some(p);
        }
        self.job.map(|_| self.mana)
    }
}

// --- what the panel shows -----------------------------------------------------------------------------------------------

/// A piece of text in the normal colour (code 1) or the alternate colour (code 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub alt: bool,
}

/// A statistic row's value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Empty,
    Text(Vec<Span>),
    /// The mana bar, with the mana number on it.
    Mana(u8),
}

impl Value {
    /// The value as plain text.
    #[cfg(test)]
    pub fn plain(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Text(s) => s.iter().map(|s| s.text.as_str()).collect(),
            Value::Mana(m) => m.to_string(),
        }
    }
}

/// One statistic row: its label entry and value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: u16,
    pub value: Value,
}

/// The panel's text for one selected unit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Content {
    pub name: Option<u16>,
    /// The level line's number.
    pub level: Option<u32>,
    /// "HP/max".
    pub hp: Option<(u16, u16)>,
    pub rows: [Option<Row>; 6],
    /// Hides the value column.
    pub production: Option<u16>,
    /// The icons of the jobs waiting in the building's queue, next first (option `production-queue`).
    pub waiting: [Option<u16>; 4],
    pub progress: Option<u8>,
}

/// "N" or "N+B" with the bonus in the alternate colour; a bonus of 0 isn't shown.
fn with_bonus(n: u32, bonus: u8) -> Value {
    text_with_bonus(n.to_string(), bonus)
}

fn text_with_bonus(text: String, bonus: u8) -> Value {
    let mut v = vec![Span { text, alt: false }];
    if bonus != 0 {
        v.push(Span {
            text: format!("+{bonus}"),
            alt: true,
        });
    }
    Value::Text(v)
}

/// The damage range of a type (spec "Damage").
pub fn damage(basic: u8, piercing: u8) -> (u32, u32) {
    let (b, p) = (basic as i32, piercing as i32);
    let max = b + p;
    let min = ((b - 30).div_euclid(2)).max(0) + (p + 1) / 2;
    (min as u32, max as u32)
}

fn damage_value(st: &UnitStats, bonus: u8) -> Value {
    let (min, max) = damage(st.basic_damage, st.piercing_damage);
    let text = if min == max {
        min.to_string()
    } else {
        format!("{min}-{max}")
    };
    text_with_bonus(text, bonus)
}

/// The sight shown: a sight of 1 shows as 0. Open question: a sight above 9 isn't looked up in a list here; no default
/// type has one.
fn sight_shown(sight: u32) -> u32 {
    if sight == 1 { 0 } else { sight }
}

/// The level line's number: 1 plus the owner's levels for the class (spec "Level line").
pub fn level(t: u8, st: &UnitStats, o: &Owner) -> u32 {
    let (wu, au) = (st.weapons_upgradable as u32, st.armor_upgradable as u32);
    1 + match class_of(t).0 {
        Class::Melee => {
            wu * o.level(1)
                + au * o.level(2)
                + match t {
                    0x0A => [4, 6, 7, 8, 9].iter().map(|&b| o.knows(b)).sum(),
                    0x0B => [13, 15, 16, 17, 19].iter().map(|&b| o.knows(b)).sum(),
                    0x0C => 1 + o.knows(1) + o.knows(3),
                    0x0D => 1 + o.knows(11) + o.knows(18),
                    _ => 0,
                }
        }
        Class::Siege => o.level(6),
        Class::Archer => o.level(7) + o.level(10) + o.level(9) + o.level(8) + o.level(0) + au * o.level(2),
        Class::Sea => o.level(5) + wu * o.level(3) + au * o.level(4),
        Class::Transport => o.level(5) + o.level(4),
        _ => 0,
    }
}

/// A bonus worked out in 8 bits.
fn bonus(mult: u32, level: u32, on: bool) -> u8 {
    if on { (mult * level) as u8 } else { 0 }
}

/// Inputs to [`content`] that don't change during a game.
pub struct Tables<'a> {
    pub stats: &'a [UnitStats],
    pub upgrades: &'a UpgradeTable,
    /// Speed per type ([`speeds`]).
    pub speeds: &'a [u8],
}

const RANGER: u8 = 0x12;
const BERSERKER: u8 = 0x13;

/// The statistic rows of an own unit (spec "Statistic rows of own units").
fn unit_rows(s: &Subject, st: &UnitStats, o: &Owner, speed: u8) -> [Option<Row>; 6] {
    let (wu, au) = (st.weapons_upgradable, st.armor_upgradable);
    let cls = class_of(s.kind).0;
    // (show armour, armour bonus, damage bonus)
    let (armour_row, armour_b, damage_b) = match cls {
        Class::Melee => (true, bonus(2, o.level(2), au), bonus(2, o.level(1), wu)),
        Class::Siege => (true, bonus(2, o.level(2), au), bonus(15, o.level(6), wu)),
        Class::Archer => {
            let mut d = bonus(1, o.level(0), wu);
            if s.kind == RANGER && o.level(10) > 0 {
                d = d.wrapping_add(3);
            }
            (true, bonus(2, o.level(2), au), d)
        }
        Class::Sea => (au, bonus(5, o.level(4), true), bonus(5, o.level(3), wu)),
        Class::Transport if s.passengers == 0 => (true, bonus(5, o.level(4), au), bonus(5, o.level(3), wu)),
        _ => return Default::default(),
    };
    let ranger_line = cls == Class::Archer && matches!(s.kind, RANGER | BERSERKER);
    let range_b = if ranger_line && o.level(8) > 0 { 1 } else { 0 };
    let sight_b = if ranger_line && o.level(9) > 0 {
        9u8.wrapping_sub(st.sight as u8)
    } else {
        0
    };
    let mut rows: [Option<Row>; 6] = Default::default();
    if armour_row {
        rows[0] = Some(Row {
            label: 412,
            value: with_bonus(st.armor as u32, armour_b),
        });
    }
    rows[1] = Some(Row {
        label: 413,
        value: damage_value(st, damage_b),
    });
    rows[2] = Some(Row {
        label: 414,
        value: with_bonus(st.attack_range as u32, range_b),
    });
    rows[3] = Some(Row {
        label: 415,
        value: with_bonus(sight_shown(st.sight), sight_b),
    });
    rows[4] = Some(Row {
        label: 416,
        value: with_bonus(speed as u32, 0),
    });
    if cls == Class::Melee && st.flags & flags::CASTER != 0 {
        rows[5] = Some(Row {
            label: 417,
            value: Value::Mana(s.mana),
        });
    }
    rows
}

/// The production icon of a job: a type's icon, or an upgrade's icon from the upgrade data with 108 shown as 103.
fn job_icon(job: JobKind, up: &UpgradeTable) -> Option<u16> {
    match job {
        JobKind::Train(t) | JobKind::Upgrade(t) => icon(t),
        JobKind::Research(n) | JobKind::Spell(n) => {
            up.0.get(n as usize).map(|u| if u.icon == 108 { 103 } else { u.icon })
        }
    }
}

/// The rows and production icon of a building (spec "Buildings").
fn building_rows(s: &Subject, st: &UnitStats, o: &Owner, up: &UpgradeTable) -> ([Option<Row>; 6], Option<u16>) {
    let mut rows: [Option<Row>; 6] = Default::default();
    let cls = class_of(s.kind).0;
    let complete = s.own && s.construction.is_none();
    let job = s.job.filter(|_| complete);
    let label = |n: u16| {
        Some(Row {
            label: n,
            value: Value::Empty,
        })
    };
    let row = |n: u16, v: Value| Some(Row { label: n, value: v });
    // The "with icon" cases.
    let icon_case = match (cls, job) {
        (Class::Training | Class::Shipyard | Class::MageTower | Class::Hall, Some(j @ JobKind::Train(_))) => {
            Some((422, j))
        }
        (Class::Church | Class::MageTower, Some(j @ JobKind::Spell(_))) => Some((425, j)),
        (Class::LumberMill | Class::Smith, Some(j @ JobKind::Research(_))) => Some((423, j)),
        (Class::Hall | Class::ScoutTower, Some(j @ JobKind::Upgrade(_))) => Some((423, j)),
        _ => None,
    };
    if let Some((n, j)) = icon_case {
        rows[1] = label(n);
        return (rows, job_icon(j, up));
    }
    match cls {
        Class::Hall if complete => {
            let gold = if o.castle {
                20
            } else if o.keep {
                10
            } else {
                0
            };
            rows[0] = label(427);
            rows[1] = row(428, with_bonus(100, gold));
            rows[2] = row(429, with_bonus(100, if o.lumber_mill { 25 } else { 0 }));
            rows[3] = row(430, with_bonus(100, if o.refinery { 25 } else { 0 }));
        }
        Class::LumberMill if complete => {
            rows[1] = label(427);
            rows[2] = row(429, with_bonus(100, if o.lumber_mill { 25 } else { 0 }));
        }
        Class::Refinery if complete && job.is_none() => {
            rows[1] = label(427);
            rows[2] = row(430, with_bonus(100, if o.refinery { 25 } else { 0 }));
        }
        Class::Farm if complete => {
            rows[0] = label(419);
            rows[1] = row(420, with_bonus(o.food_supply, 0));
            rows[2] = row(
                421,
                Value::Text(vec![Span {
                    text: o.food_used.to_string(),
                    alt: o.food_used > o.food_supply,
                }]),
            );
        }
        Class::GoldMine => rows[1] = row(426, with_bonus(s.trips * 100, 0)),
        Class::OilSource if !s.busy() => rows[1] = row(418, with_bonus(s.trips * 100, 0)),
        Class::Tower if complete => {
            rows[0] = row(
                412,
                with_bonus(st.armor as u32, bonus(2, o.level(2), st.armor_upgradable)),
            );
            rows[1] = row(413, damage_value(st, bonus(2, o.level(1), st.weapons_upgradable)));
            rows[2] = row(414, with_bonus(st.attack_range as u32, 0));
            rows[3] = row(415, with_bonus(sight_shown(st.sight), 0));
        }
        _ => {}
    }
    (rows, None)
}

/// The panel's text for one selected unit (spec "One unit" and the sections after it).
pub fn content(s: &Subject, o: &Owner, t: &Tables) -> Content {
    let st = t.stats.get(s.kind as usize).copied().unwrap_or_default();
    let max = st.hit_points.max(1);
    let cls = class_of(s.kind).0;
    let loaded = cls == Class::Transport && s.passengers > 0;
    let mut c = Content {
        name: name_entry(s.kind),
        hp: (s.own && !loaded && s.hp <= 9999 && max <= 9999).then_some((s.hp, max)),
        progress: if s.busy() { s.progress() } else { None },
        ..Default::default()
    };
    if s.building {
        (c.rows, c.production) = building_rows(s, &st, o, t.upgrades);
        if s.own && s.construction.is_none() && s.job.is_some() {
            c.waiting = s.waiting.map(|j| j.and_then(|j| job_icon(j, t.upgrades)));
        }
    } else {
        c.level = Some(level(s.kind, &st, o));
        if s.own {
            c.rows = unit_rows(s, &st, o, t.speeds.get(s.kind as usize).copied().unwrap_or(0));
        }
    }
    c
}

// --- bars -------------------------------------------------------------------------------------------------------------

/// The health bar: filled pixels of 50 and the palette colour (spec "Icons and health bars").
pub fn health_bar(hp: u16, max: u16) -> (u32, u8) {
    let m = max.max(1) as u32;
    let fill = (50 * hp as u32 / m).min(50);
    let q = 100 * hp as u32 / m;
    let colour = if q >= 75 {
        149
    } else if q >= 50 {
        251
    } else {
        249
    };
    (fill, colour)
}

/// The progress bar's filled pixels of 152.
pub fn progress_fill(p: u8) -> u32 {
    (152 * (p as u32 + 5) / 256).min(152)
}

/// The mana bar's filled pixels of 59.
pub fn mana_fill(mana: u8) -> u32 {
    59 * mana as u32 / 255
}

// --- speeds -----------------------------------------------------------------------------------------------------------

/// The move duration D(t) of a type: its move animation walked to the second step-done (spec "Speed").
pub fn move_duration(scripts: &Scripts, t: u8) -> u32 {
    let Some(mut pc) = scripts.unit(t, anim::MOVE) else {
        return 0;
    };
    let (mut d, mut done) = (0u32, 0);
    // A guard against scripts that loop without a step-done.
    for _ in 0..10_000 {
        let Some(op) = scripts.decode(pc) else { break };
        match op {
            Op::StepDone => {
                d += 1;
                done += 1;
                if done == 2 {
                    break;
                }
            }
            Op::Wait(n) | Op::WaitHasteSlow(n) | Op::WaitHaste(n) | Op::WaitSlow(n) | Op::WaitCarry(n) => d += n as u32,
            Op::Jump(to) => {
                pc = to;
                continue;
            }
            _ => {}
        }
        pc = pc.wrapping_add(op.size() as u16);
    }
    d
}

/// Speed per type: floor(10 x D(0) / D(t)) in 8 bits, with D halved for air and naval movers; 0 when D(t) is 0.
pub fn speeds(scripts: &Scripts, stats: &[UnitStats]) -> Vec<u8> {
    let d = |t: u8| {
        let d = move_duration(scripts, t);
        match stats.get(t as usize).map(|s| s.movement_class) {
            Some(1 | 2) => d / 2,
            _ => d,
        }
    };
    let base = d(0);
    (0..stats.len().min(256))
        .map(|t| {
            let dt = d(t as u8);
            (10 * base).checked_div(dt).unwrap_or(0) as u8
        })
        .collect()
}

// --- change checks (spec "Updating") ----------------------------------------------------------------------------------

/// The check set of a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checks {
    None,
    Common,
    Carrying,
    Passengers,
    Food,
    AmountLeft,
    JobTarget,
}

pub fn checks(t: u8) -> Checks {
    match t {
        0x30 | 0x36 | 0x5E | 0x5F | 0x69 => Checks::None,
        0x02 | 0x03 | 0x10 | 0x11 | 0x1A | 0x1B => Checks::Carrying,
        0x1C | 0x1D => Checks::Passengers,
        0x3A | 0x3B => Checks::Food,
        0x5C | 0x56 | 0x57 | 0x5D => Checks::AmountLeft,
        0x3C..=0x41 | 0x44..=0x53 => Checks::JobTarget,
        _ => Checks::Common,
    }
}

// --- reading the sim --------------------------------------------------------------------------------------------------

/// A unit as the panel reads it, if it is alive.
pub fn facts(w: &World, id: UnitId, local: u8) -> Option<Subject> {
    let u = w.unit(id)?;
    Some(Subject {
        kind: u.kind,
        own: u.owner == local,
        building: u.building,
        hp: u.hp,
        mana: u.mana,
        construction: w.construction_progress(id),
        job: u.job.map(|j| j.kind),
        waiting: {
            let mut list = [None; 4];
            for (slot, q) in list.iter_mut().zip(w.waiting(id)) {
                *slot = Some(q.kind);
            }
            list
        },
        passengers: u.passengers.iter().flatten().count() as u8,
        trips: u.resources,
    })
}

/// The panel's view of a player.
pub fn owner_facts(w: &World, p: u8) -> Owner {
    let pl = &w.players[p as usize];
    let has = |kinds: &[u8]| {
        w.units
            .iter()
            .any(|u| u.owner == p && u.building && !u.dying && u.construction.is_none() && kinds.contains(&u.kind))
    };
    let mut levels = [0; 11];
    for (l, v) in levels.iter_mut().zip(pl.levels.iter()) {
        *l = *v;
    }
    Owner {
        levels,
        spells: pl.spells_known,
        castle: has(&[k::CASTLE, k::FORTRESS]),
        keep: has(&[k::KEEP, k::STRONGHOLD]),
        lumber_mill: has(&[0x4C, 0x4D]),
        refinery: has(&[0x54, 0x55]),
        food_supply: w.food_supply(p),
        food_used: w.food_used(p),
    }
}

/// The values the one selected unit's checks watch; the panel text is redrawn when they change. `None` for the
/// "none" set, whose panel is never redrawn by a check. `slots` are the grid's filled slots (mobile units watch hit
/// points and mana of every one).
pub fn watch(w: &World, slots: &Slots, local: u8) -> Option<Vec<u32>> {
    let id = slots[0]?;
    let u = w.unit(id)?;
    let set = checks(u.kind);
    if set == Checks::None {
        return None;
    }
    let s = facts(w, id, local)?;
    let mut key = vec![s.busy() as u32];
    if u.building {
        key.extend([s.hp as u32, s.progress().map_or(0, |p| p as u32 + 1)]);
    } else {
        for v in slots.iter().flatten().filter_map(|&v| w.unit(v)) {
            key.extend([v.hp as u32, v.mana as u32]);
        }
    }
    // The jobs waiting in a building's queue: a change redraws the icons.
    key.extend(s.waiting.iter().map(|j| match j {
        None => 0,
        Some(JobKind::Train(t)) => 0x100 | *t as u32,
        Some(JobKind::Upgrade(t)) => 0x200 | *t as u32,
        Some(JobKind::Research(n)) => 0x300 | *n as u32,
        Some(JobKind::Spell(n)) => 0x400 | *n as u32,
    }));
    match set {
        Checks::Carrying => key.push(u.carrying as u32),
        Checks::Passengers => key.extend(u.passengers.iter().map(|p| p.map_or(0, |p| p.0 + 1))),
        Checks::Food if s.own => key.extend([w.food_used(local), w.food_supply(local)]),
        Checks::AmountLeft => key.push(u.resources),
        Checks::JobTarget => key.push(match u.job.map(|j| j.kind) {
            None => 0,
            Some(JobKind::Train(t)) => 0x100 | t as u32,
            Some(JobKind::Upgrade(t)) => 0x200 | t as u32,
            Some(JobKind::Research(n)) => 0x300 | n as u32,
            Some(JobKind::Spell(n)) => 0x400 | n as u32,
        }),
        _ => {}
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tidewater_formats::ugrd::Upgrade;

    fn stats(basic: u8, piercing: u8, armor: u8, wu: bool, au: bool) -> UnitStats {
        UnitStats {
            basic_damage: basic,
            piercing_damage: piercing,
            armor,
            weapons_upgradable: wu,
            armor_upgradable: au,
            hit_points: 60,
            sight: 4,
            attack_range: 1,
            ..Default::default()
        }
    }

    fn own(kind: u8) -> Subject {
        Subject {
            kind,
            own: true,
            hp: 60,
            ..Default::default()
        }
    }

    fn table(kind: u8, st: UnitStats) -> Vec<UnitStats> {
        let mut v = vec![UnitStats::default(); 110];
        v[kind as usize] = st;
        v
    }

    fn content_of(s: &Subject, o: &Owner, st: Vec<UnitStats>, up: &UpgradeTable) -> Content {
        let speeds = vec![10; 110];
        content(
            s,
            o,
            &Tables {
                stats: &st,
                upgrades: up,
                speeds: &speeds,
            },
        )
    }

    fn no_upgrades() -> UpgradeTable {
        UpgradeTable(vec![Upgrade::default(); 52])
    }

    fn row(c: &Content, n: usize) -> Option<(u16, String)> {
        c.rows[n].as_ref().map(|r| (r.label, r.value.plain()))
    }

    fn bonus_of(c: &Content, n: usize) -> Option<String> {
        match &c.rows[n].as_ref()?.value {
            Value::Text(s) => s.iter().find(|s| s.alt).map(|s| s.text.clone()),
            _ => None,
        }
    }

    #[test]
    fn every_listed_type_has_a_class() {
        for t in 0..=0x69u8 {
            assert!(class(t).is_some(), "type {t:#x}");
            assert!(icon(t).is_some());
        }
        assert_eq!(class(0x6A), None);
        assert_eq!(icon(0x00), Some(2));
        assert_eq!(icon(0x67), Some(92));
        assert_eq!(name_entry(0x00), Some(2));
        assert_eq!(name_entry(0x61), Some(264));
        assert_eq!(name_entry(0x5E), None);
    }

    #[test]
    fn footman_without_and_with_upgrades() {
        let st = stats(6, 3, 2, true, true);
        let c = content_of(&own(0), &Owner::default(), table(0, st), &no_upgrades());
        assert_eq!(c.level, Some(1));
        assert_eq!(row(&c, 0), Some((412, "2".into())));
        assert_eq!(row(&c, 1), Some((413, "2-9".into())));
        assert_eq!(bonus_of(&c, 1), None);
        assert_eq!(c.hp, Some((60, 60)));
        let mut o = Owner::default();
        o.levels[1] = 2;
        o.levels[2] = 1;
        let c = content_of(&own(0), &o, table(0, st), &no_upgrades());
        assert_eq!(c.level, Some(4));
        assert_eq!(bonus_of(&c, 1).as_deref(), Some("+4"));
        assert_eq!(bonus_of(&c, 0).as_deref(), Some("+2"));
        assert_eq!(row(&c, 1), Some((413, "2-9+4".into())));
    }

    #[test]
    fn damage_ranges() {
        assert_eq!(damage(0, 0), (0, 0));
        assert_eq!(damage(40, 1), (6, 41));
        let st = stats(0, 0, 0, false, false);
        let c = content_of(&own(0), &Owner::default(), table(0, st), &no_upgrades());
        assert_eq!(row(&c, 1), Some((413, "0".into())));
    }

    #[test]
    fn caster_levels() {
        let st = stats(6, 3, 2, true, true);
        let mut o = Owner {
            spells: 1 << 1,
            ..Default::default()
        };
        o.levels[1] = 1;
        assert_eq!(level(0x0C, &st, &o), 4, "paladin");
        let mage = stats(0, 9, 0, false, false);
        let o = Owner {
            spells: 1 << 5,
            ..Default::default()
        };
        assert_eq!(level(0x0A, &mage, &o), 1, "fireball doesn't count");
        let o = Owner {
            spells: 1 << 16 | 1 << 19,
            ..Default::default()
        };
        assert_eq!(level(0x0B, &mage, &o), 3, "death knight");
    }

    #[test]
    fn ranger_and_berserker() {
        let mut st = stats(3, 6, 0, true, false);
        st.sight = 5;
        let mut o = Owner::default();
        o.levels[0] = 2;
        for g in [7, 8, 9, 10] {
            o.levels[g] = 1;
        }
        let c = content_of(&own(0x12), &o, table(0x12, st), &no_upgrades());
        assert_eq!(c.level, Some(7));
        assert_eq!(bonus_of(&c, 1).as_deref(), Some("+5"));
        assert_eq!(bonus_of(&c, 2).as_deref(), Some("+1"));
        assert_eq!(row(&c, 3), Some((415, "5+4".into())));
        let c = content_of(&own(0x13), &o, table(0x13, st), &no_upgrades());
        assert_eq!(bonus_of(&c, 1).as_deref(), Some("+2"));
    }

    #[test]
    fn ballista_battleship_transport() {
        let mut o = Owner::default();
        o.levels[6] = 2;
        let st = stats(80, 0, 0, true, false);
        let c = content_of(&own(0x04), &o, table(0x04, st), &no_upgrades());
        assert_eq!(c.level, Some(3));
        assert_eq!(bonus_of(&c, 1).as_deref(), Some("+30"));
        let st = stats(130, 0, 15, true, false);
        let c = content_of(&own(0x1E), &Owner::default(), table(0x1E, st), &no_upgrades());
        assert!(c.rows[0].is_none(), "no armour row");
        assert!(c.rows[1].is_some());
        let mut o = Owner::default();
        o.levels[4] = 1;
        let st = stats(0, 0, 0, false, true);
        let c = content_of(&own(0x1C), &o, table(0x1C, st), &no_upgrades());
        assert_eq!(c.level, Some(2));
        assert_eq!((0..5).filter(|&n| c.rows[n].is_some()).count(), 5);
        assert!(c.rows[5].is_none());
        let loaded = Subject {
            passengers: 2,
            ..own(0x1C)
        };
        let c = content_of(&loaded, &o, table(0x1C, st), &no_upgrades());
        assert!(c.rows.iter().all(Option::is_none));
        assert_eq!(c.hp, None);
        assert!(c.name.is_some() && c.level.is_some());
    }

    #[test]
    fn other_players_units_and_big_hit_points() {
        let st = stats(6, 3, 2, true, true);
        let grunt = Subject { own: false, ..own(1) };
        let c = content_of(&grunt, &Owner::default(), table(1, st), &no_upgrades());
        assert_eq!(c.hp, None);
        assert!(c.rows.iter().all(Option::is_none));
        assert_eq!(c.level, Some(1));
        assert_eq!(frame(Some(&grunt)), 0);
        let mut big = st;
        big.hit_points = 10_000;
        let c = content_of(&own(0), &Owner::default(), table(0, big), &no_upgrades());
        assert_eq!(c.hp, None);
    }

    #[test]
    fn health_and_bars() {
        assert_eq!(health_bar(45, 60), (37, 149));
        assert_eq!(health_bar(29, 60), (24, 249));
        assert_eq!(health_bar(30, 60).1, 251);
        assert_eq!(progress_fill(0), 2);
        assert_eq!(progress_fill(255), 152);
        assert_eq!(mana_fill(255), 59);
        // Info-panel spec vectors (round 76): a job just started shows p = 1 (bar 3 px) until its first run, and 2
        // (bar 4 px) after it, 1 of 120 steps: the building's mana byte.
        let mut job = Subject {
            building: true,
            mana: 1,
            job: Some(JobKind::Train(0)),
            ..own(0x3C)
        };
        assert_eq!((job.progress(), progress_fill(1)), (Some(1), 3));
        job.mana = (255 / 120) as u8;
        assert_eq!((job.progress(), progress_fill(2)), (Some(2), 4));
    }

    #[test]
    fn the_waiting_jobs_show_as_icons_only_for_a_working_own_building() {
        let mut s = own(0x3C);
        s.building = true;
        s.job = Some(JobKind::Train(0));
        s.waiting = [Some(JobKind::Train(0)), Some(JobKind::Train(8)), None, None];
        let c = content_of(&s, &Owner::default(), table(0x3C, UnitStats::default()), &no_upgrades());
        assert_eq!(c.waiting, [icon(0), icon(8), None, None]);
        assert!(c.waiting[0].is_some());
        s.job = None;
        let c = content_of(&s, &Owner::default(), table(0x3C, UnitStats::default()), &no_upgrades());
        assert_eq!(c.waiting, [None; 4]);
    }

    #[test]
    fn barracks_training() {
        let s = Subject {
            building: true,
            mana: (10 * 255 / 120) as u8,
            job: Some(JobKind::Train(0)),
            ..own(0x3C)
        };
        let c = content_of(&s, &Owner::default(), table(0x3C, UnitStats::default()), &no_upgrades());
        assert_eq!(frame(Some(&s)), 3);
        assert_eq!(row(&c, 1), Some((422, String::new())));
        assert_eq!(c.production, Some(2));
        assert_eq!(c.progress, Some(21));
        assert_eq!(c.level, None);
    }

    #[test]
    fn halls_mills_farms_mines() {
        let hall = Subject {
            building: true,
            ..own(0x4A)
        };
        let o = Owner {
            keep: true,
            lumber_mill: true,
            ..Default::default()
        };
        let c = content_of(&hall, &o, table(0x4A, UnitStats::default()), &no_upgrades());
        assert_eq!(row(&c, 0), Some((427, String::new())));
        assert_eq!(row(&c, 1), Some((428, "100+10".into())));
        assert_eq!(row(&c, 2), Some((429, "100+25".into())));
        assert_eq!(row(&c, 3), Some((430, "100".into())));

        let mut up = no_upgrades();
        up.0[4].icon = 125;
        let mill = Subject {
            building: true,
            job: Some(JobKind::Research(4)),
            ..own(0x4C)
        };
        let c = content_of(&mill, &o, table(0x4C, UnitStats::default()), &up);
        assert_eq!(row(&c, 1), Some((423, String::new())));
        assert_eq!(c.production, Some(125));

        let farm = Subject {
            building: true,
            ..own(0x3A)
        };
        let o = Owner {
            food_supply: 9,
            food_used: 10,
            ..Default::default()
        };
        let c = content_of(&farm, &o, table(0x3A, UnitStats::default()), &no_upgrades());
        assert_eq!(row(&c, 1), Some((420, "9".into())));
        assert_eq!(
            bonus_of(&c, 2).as_deref(),
            Some("10"),
            "in use shown in the alternate colour"
        );

        let mine = Subject {
            building: true,
            own: false,
            trips: 3000,
            ..own(0x5C)
        };
        let c = content_of(
            &mine,
            &Owner::default(),
            table(0x5C, UnitStats::default()),
            &no_upgrades(),
        );
        assert_eq!(row(&c, 1), Some((426, "300000".into())));
        assert_eq!(c.hp, None);
    }

    #[test]
    fn frames() {
        assert_eq!(frame(None), 0);
        assert_eq!(frame(Some(&own(0))), 1);
        assert_eq!(frame(Some(&own(0x0A))), 2);
        let idle = Subject {
            building: true,
            ..own(0x3C)
        };
        assert_eq!(frame(Some(&idle)), 1);
        let site = Subject {
            construction: Some(10),
            ..idle
        };
        assert_eq!(frame(Some(&site)), 3);
        assert_eq!(frame(Some(&own(0x5E))), 0);
    }

    #[test]
    fn slot_assignment() {
        let (a, b, c, d) = (UnitId(1), UnitId(2), UnitId(3), UnitId(4));
        let prev = assign(&[None; 9], &[a, b, c]);
        assert_eq!(&prev[..3], &[Some(a), Some(b), Some(c)]);
        let next = assign(&prev, &[b, c, d]);
        assert_eq!(&next[..3], &[Some(d), Some(b), Some(c)]);
        assert_eq!(in_slot_order(&next)[0], d, "primary unit");
        let two = assign(&[None; 9], &[a, b]);
        let one = assign(&two, &[b]);
        assert_eq!(one[0], Some(b));
        assert!(one[1..].iter().all(Option::is_none));
    }

    #[test]
    fn clicks() {
        let (a, b, p) = (UnitId(1), UnitId(2), UnitId(9));
        let two = assign(&[None; 9], &[a, b]);
        assert_eq!(click(&two, 1, 2, true, false, true), Click::Remove(b));
        assert_eq!(click(&two, 1, 2, false, false, true), Click::SelectOnly(b));
        assert_eq!(click(&two, 5, 2, false, false, true), Click::Nothing);
        let mut ship = assign(&[None; 9], &[a]);
        with_passengers(&mut ship, &[None, Some(p), None, None, None, None]);
        assert_eq!(ship[4], Some(p));
        assert_eq!(
            click(&ship, 4, 1, false, true, true),
            Click::Unload { transport: a, place: 1 }
        );
        assert_eq!(click(&ship, 0, 1, false, true, true), Click::Centre(a));
        assert_eq!(click(&ship, 0, 1, false, true, false), Click::Nothing);
    }

    /// A font where every character is 6 pixels wide.
    fn w6(b: &[u8]) -> u32 {
        6 * b.len() as u32
    }

    #[test]
    fn name_lines_split() {
        // 16 characters: 96 px fits.
        assert_eq!(
            name_lines("abcdefghijklmnop", &w6),
            (String::new(), "abcdefghijklmnop".into())
        );
        // First word 13 characters (78 px), the whole 25 (150 px).
        assert_eq!(
            name_lines("abcdefghijklm nopqrstuvwx", &w6),
            ("abcdefghijklm".into(), "nopqrstuvwx".into())
        );
        // 30 characters without a space: 17 + hyphen is 108 px (< 109), 18 would be 114.
        let long = "abcdefghijklmnopqrstuvwxyzABCD";
        let (l1, l2) = name_lines(long, &w6);
        assert_eq!(l1, "abcdefghijklmnopq-");
        // The rest is 13 characters, 78 px.
        assert_eq!(l2, "rstuvwxyzABCD");
        // Line 2 is cut to 19 characters and 109 px (18 characters at 6 px).
        let (_, l2) = name_lines("abc defghijklmnopqrstuvwxyzABCDEF", &w6);
        assert_eq!(l2, "defghijklmnopqrstu");
    }

    #[test]
    fn check_sets() {
        assert_eq!(checks(0x58), Checks::Common, "keep");
        assert_eq!(checks(0x4A), Checks::JobTarget, "town hall");
        assert_eq!(checks(0x5C), Checks::AmountLeft);
        assert_eq!(checks(0x5E), Checks::None);
        assert_eq!(checks(0x02), Checks::Carrying);
        assert_eq!(checks(0x00), Checks::Common);
    }
}
