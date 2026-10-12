//! `twtool oracle`: runs our simulation alongside a per-cycle trace of the original game and reports where the two
//! first differ.
//!
//! The trace format is `docs/specs/oracle/trace-format.md` (format version 1): JSON Lines, optionally gzip-compressed,
//! a header line and then one record per simulated cycle (`setup`, `startup`, `cycle`, `turn`), compared by `n` and
//! `kind` in file order. The fields mean what the specs that format cites say: `core/unit-slots.md` (unit and effect
//! slots, creation numbers, owners, the three permanent effects), `core/timing-and-rng.md` (the generator's state and
//! step), `core/animation-scripts.md` (pixel positions, animation, frame and wait), `orders/orders.md` ("The order
//! list"), `economy/harvest.md` (trips) and `spells/spells.md` (mana and timers).
//!
//! Scenario identity is still free text in the header (the format's open question "Scenario identity"), so the caller
//! names the map and the campaign entry; the seed, the rules and the table sizes come from the header.
//!
//! Where our simulation doesn't hold a field, or holds it in a form that can't tell two trace values apart, the field
//! is listed as not compared (or compared against every value it may stand for) and the rest is still compared.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tidewater_formats::udta::flags;
use tidewater_sim::{Order, Unit, World};

/// Unit fields in report order; `slot` is the key and isn't listed. The four timers are flattened.
pub const UNIT_FIELDS: [&str; 20] = [
    "serial",
    "type",
    "owner",
    "x",
    "y",
    "tx",
    "ty",
    "hp",
    "order",
    "facing",
    "anim",
    "frame",
    "wait",
    "mana",
    "target",
    "trips",
    "invisibility",
    "unholy_armour",
    "bloodlust",
    "haste",
];
const TIMERS: [&str; 4] = ["invisibility", "unholy_armour", "bloodlust", "haste"];

/// Effect fields in report order; `dest` is flattened into `dest_x` and `dest_y`.
pub const EFFECT_FIELDS: [&str; 8] = ["kind", "x", "y", "dest_x", "dest_y", "target", "shooter", "damage"];

/// Types that carry `trips` (trace format, "Unit fields"): the oil platforms, the gold mine and the oil patch.
const TRIP_TYPES: [u8; 4] = [0x56, 0x57, 0x5c, 0x5d];
/// The berserker, which counts regeneration cycles in its mana byte once its player has the upgrade (spells spec).
const BERSERKER: u8 = 0x13;
/// Effect slots 0 to 2 hold the permanent effects (unit-slots spec, "Starting a map", step 3).
const PERMANENT_EFFECTS: u16 = 3;
/// Onsets listed in the summary.
const ONSETS_SHOWN: usize = 40;
/// The most generator steps searched when counting the draws between two states.
const MAX_STEPS: u32 = 1 << 20;

/// One field's value. The trace only gives `Int` and `Null`; our side may also give the other two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Int(i64),
    Null,
    /// Any of these values matches: our simulation doesn't tell them apart.
    AnyOf(Vec<i64>),
    /// Not compared, for the reason given (listed once in the report).
    Skip(&'static str),
}

impl Cell {
    fn of(v: Option<&Value>) -> Cell {
        match v.and_then(Value::as_i64) {
            Some(n) => Cell::Int(n),
            None => Cell::Null,
        }
    }

    fn opt(v: Option<impl Into<i64>>) -> Cell {
        v.map_or(Cell::Null, |n| Cell::Int(n.into()))
    }

    /// Whether `self` (ours) matches `theirs`, or `None` when it isn't compared.
    fn matches(&self, theirs: &Cell) -> Option<bool> {
        Some(match (self, theirs) {
            (Cell::Skip(_), _) | (_, Cell::Skip(_)) => return None,
            (Cell::AnyOf(set), Cell::Int(n)) => set.contains(n),
            (a, b) => a == b,
        })
    }

    #[cfg(test)]
    fn json(&self) -> Option<Value> {
        match self {
            Cell::Int(n) => Some(Value::from(*n)),
            Cell::Null => Some(Value::Null),
            Cell::AnyOf(set) => set.first().map(|&n| Value::from(n)),
            Cell::Skip(_) => None,
        }
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Cell::Int(n) => write!(f, "{n}"),
            Cell::Null => f.write_str("null"),
            Cell::AnyOf(set) => {
                let s: Vec<String> = set.iter().map(i64::to_string).collect();
                f.write_str(&s.join("|"))
            }
            Cell::Skip(_) => f.write_str("-"),
        }
    }
}

/// The trace header (trace format, "Header").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub scenario: String,
    pub launch: String,
    pub clock: Option<u32>,
    pub speed: u8,
    pub enhanced: bool,
    pub era: u8,
    pub map_size: u32,
    pub local_player: u8,
    pub controllers: Vec<String>,
    pub races: Vec<String>,
    pub cheats: Vec<String>,
    pub unit_slots: usize,
    pub effect_slots: usize,
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|s| s.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default()
}

fn int(v: &Value, key: &str) -> Result<i64> {
    v.get(key)
        .and_then(Value::as_i64)
        .with_context(|| format!("missing or non-integer field `{key}`"))
}

impl Header {
    pub fn parse(line: &str) -> Result<Header> {
        let v: Value = serde_json::from_str(line).context("header is not JSON")?;
        if v.get("format").and_then(Value::as_str) != Some("tidewater-oracle-trace") {
            bail!("not a tidewater oracle trace (header `format`)");
        }
        let version = int(&v, "version")?;
        if version != 1 {
            bail!("trace format version {version}; this comparer reads version 1");
        }
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        Ok(Header {
            scenario: text("scenario"),
            launch: text("launch"),
            clock: v.get("clock").and_then(Value::as_u64).map(|c| c as u32),
            speed: int(&v, "speed")? as u8,
            enhanced: v.get("enhanced").and_then(Value::as_bool).unwrap_or(true),
            era: int(&v, "era")? as u8,
            map_size: int(&v, "map_size")? as u32,
            local_player: int(&v, "local_player")? as u8,
            controllers: strings(&v, "controllers"),
            races: strings(&v, "races"),
            cheats: strings(&v, "cheats"),
            unit_slots: int(&v, "unit_slots")? as usize,
            effect_slots: int(&v, "effect_slots")? as usize,
        })
    }

    /// The header as one JSON line (for synthetic traces).
    #[cfg(test)]
    pub fn to_json(&self) -> String {
        serde_json::json!({
            "format": "tidewater-oracle-trace",
            "version": 1,
            "scenario": self.scenario,
            "launch": self.launch,
            "clock": self.clock,
            "speed": self.speed,
            "enhanced": self.enhanced,
            "era": self.era,
            "map_size": self.map_size,
            "local_player": self.local_player,
            "controllers": self.controllers,
            "races": self.races,
            "cheats": self.cheats,
            "unit_slots": self.unit_slots,
            "effect_slots": self.effect_slots,
        })
        .to_string()
    }
}

/// One record: the state after a setup, start-up cycle, cycle or command turn (trace format, "Record fields").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub n: u64,
    pub kind: String,
    pub counter: Option<u64>,
    pub rng: u32,
    /// Gold, lumber and oil of players 0 to 15.
    pub resources: [Vec<Cell>; 3],
    /// Unit fields by slot, in [`UNIT_FIELDS`] order.
    pub units: BTreeMap<u16, Vec<Cell>>,
    /// Effect fields by slot, in [`EFFECT_FIELDS`] order.
    pub effects: BTreeMap<u16, Vec<Cell>>,
}

const RESOURCES: [&str; 3] = ["gold", "lumber", "oil"];

impl Record {
    pub fn parse(line: &str) -> Result<Record> {
        let v: Value = serde_json::from_str(line).context("record is not JSON")?;
        let n = int(&v, "n")? as u64;
        let kind = v
            .get("kind")
            .and_then(Value::as_str)
            .context("missing `kind`")?
            .to_string();
        let list = |key: &str| -> Vec<Cell> {
            let a = v.get(key).and_then(Value::as_array);
            (0..16).map(|i| Cell::of(a.and_then(|a| a.get(i)))).collect()
        };
        let mut units = BTreeMap::new();
        for u in v.get("units").and_then(Value::as_array).into_iter().flatten() {
            let slot = int(u, "slot").with_context(|| format!("record n={n}: a unit"))? as u16;
            let timers = u.get("timers");
            let fields = UNIT_FIELDS
                .iter()
                .map(|&f| {
                    if TIMERS.contains(&f) {
                        // Timers appear only while running; a missing one is 0.
                        Cell::Int(timers.and_then(|t| t.get(f)).and_then(Value::as_i64).unwrap_or(0))
                    } else {
                        Cell::of(u.get(f))
                    }
                })
                .collect();
            units.insert(slot, fields);
        }
        let mut effects = BTreeMap::new();
        for e in v.get("effects").and_then(Value::as_array).into_iter().flatten() {
            let slot = int(e, "slot").with_context(|| format!("record n={n}: an effect"))? as u16;
            let dest = e.get("dest").and_then(Value::as_array);
            let fields = EFFECT_FIELDS
                .iter()
                .map(|&f| match f {
                    "dest_x" => Cell::of(dest.and_then(|d| d.first())),
                    "dest_y" => Cell::of(dest.and_then(|d| d.get(1))),
                    _ => Cell::of(e.get(f)),
                })
                .collect();
            effects.insert(slot, fields);
        }
        Ok(Record {
            n,
            kind,
            counter: v.get("counter").and_then(Value::as_u64),
            rng: int(&v, "rng")? as u32,
            resources: RESOURCES.map(list),
            units,
            effects,
        })
    }

    /// The record as one JSON line in the trace format (for synthetic traces). Fields we don't compare are left out.
    #[cfg(test)]
    pub fn to_json(&self) -> String {
        let cells = |c: &[Cell]| Value::Array(c.iter().map(|c| c.json().unwrap_or(Value::Null)).collect());
        let mut o = serde_json::Map::new();
        o.insert("n".into(), self.n.into());
        o.insert("kind".into(), self.kind.clone().into());
        if let Some(c) = self.counter {
            o.insert("counter".into(), c.into());
        }
        o.insert("rng".into(), self.rng.into());
        for (name, list) in RESOURCES.iter().zip(&self.resources) {
            o.insert((*name).into(), cells(list));
        }
        let units = self
            .units
            .iter()
            .map(|(&slot, fields)| {
                let mut u = serde_json::Map::new();
                u.insert("slot".into(), slot.into());
                let mut timers = serde_json::Map::new();
                for (&f, c) in UNIT_FIELDS.iter().zip(fields) {
                    match c.json() {
                        Some(v) if TIMERS.contains(&f) => {
                            if v.as_i64().unwrap_or(0) != 0 {
                                timers.insert(f.into(), v);
                            }
                        }
                        Some(Value::Null) if f == "trips" => {}
                        Some(v) => {
                            u.insert(f.into(), v);
                        }
                        None => {}
                    }
                }
                if !timers.is_empty() {
                    u.insert("timers".into(), Value::Object(timers));
                }
                Value::Object(u)
            })
            .collect();
        o.insert("units".into(), Value::Array(units));
        let effects = self
            .effects
            .iter()
            .map(|(&slot, fields)| {
                let mut e = serde_json::Map::new();
                e.insert("slot".into(), slot.into());
                let get = |name: &str| {
                    let i = EFFECT_FIELDS.iter().position(|&f| f == name).expect("effect field");
                    fields[i].json().unwrap_or(Value::Null)
                };
                for f in ["kind", "x", "y"] {
                    e.insert(f.into(), get(f));
                }
                e.insert("dest".into(), Value::Array(vec![get("dest_x"), get("dest_y")]));
                for f in ["target", "shooter", "damage"] {
                    e.insert(f.into(), get(f));
                }
                Value::Object(e)
            })
            .collect();
        o.insert("effects".into(), Value::Array(effects));
        Value::Object(o).to_string()
    }
}

/// Opens a trace, gzip-compressed or not (found by its first two bytes).
pub fn open(path: &std::path::Path) -> Result<Box<dyn BufRead>> {
    let f = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    reader(f)
}

/// Wraps a trace stream, decompressing it if it starts with the gzip magic.
pub fn reader(r: impl Read + 'static) -> Result<Box<dyn BufRead>> {
    let mut b = BufReader::new(r);
    let gz = b.fill_buf()?.starts_with(&[0x1f, 0x8b]);
    Ok(if gz {
        Box::new(BufReader::new(flate2::bufread::MultiGzDecoder::new(b)))
    } else {
        Box::new(b)
    })
}

/// Reads the header line and returns it with an iterator over the records.
pub fn read(r: Box<dyn BufRead>) -> Result<(Header, impl Iterator<Item = Result<Record>>)> {
    let mut lines = r.lines();
    let first = lines.next().context("empty trace")??;
    let header = Header::parse(&first)?;
    let records = lines.enumerate().filter_map(|(i, l)| match l {
        Ok(l) if l.trim().is_empty() => None,
        Ok(l) => Some(Record::parse(&l).with_context(|| format!("trace line {}", i + 2))),
        Err(e) => Some(Err(e.into())),
    });
    Ok((header, records))
}

/// Where a difference is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Global,
    Player(u8),
    Unit(u16),
    Effect(u16),
}

/// One field that differs. Field `used` marks a slot in use on one side only (1 in use, 0 free).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    pub scope: Scope,
    pub field: &'static str,
    pub ours: Cell,
    pub theirs: Cell,
}

impl Diff {
    /// A name for the field across records: `rng`, `gold`, `unit.x`, `effect.kind`.
    pub fn key(&self) -> String {
        match self.scope {
            Scope::Global | Scope::Player(_) => self.field.to_string(),
            Scope::Unit(_) => format!("unit.{}", self.field),
            Scope::Effect(_) => format!("effect.{}", self.field),
        }
    }

    pub fn category(&self) -> Category {
        category(self.scope, self.field)
    }
}

impl fmt::Display for Diff {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let used = |c: &Cell| if *c == Cell::Int(1) { "in use" } else { "free" };
        match self.scope {
            Scope::Global if self.field == "rng" => {
                let hex = |c: &Cell| match c {
                    Cell::Int(n) => format!("{:#010x}", n),
                    c => c.to_string(),
                };
                return write!(f, "rng: ours {}, theirs {}", hex(&self.ours), hex(&self.theirs));
            }
            Scope::Global => write!(f, "{}", self.field)?,
            Scope::Player(p) => write!(f, "player {p} {}", self.field)?,
            Scope::Unit(s) if self.field == "used" => {
                return write!(
                    f,
                    "unit slot {s}: ours {}, theirs {}",
                    used(&self.ours),
                    used(&self.theirs)
                );
            }
            Scope::Effect(s) if self.field == "used" => {
                return write!(
                    f,
                    "effect slot {s}: ours {}, theirs {}",
                    used(&self.ours),
                    used(&self.theirs)
                );
            }
            Scope::Unit(s) => write!(f, "unit slot {s} {}", self.field)?,
            Scope::Effect(s) => write!(f, "effect slot {s} {}", self.field)?,
        }
        write!(f, ": ours {}, theirs {}", self.ours, self.theirs)
    }
}

/// Kinds of divergence, for the summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// A record where the generator state differs and no other field starts differing.
    RngOnly,
    Rng,
    Resources,
    /// Which slots are in use, and each unit's serial, type and owner.
    Slots,
    Positions,
    /// Order and target.
    Orders,
    Facing,
    /// Animation, frame and wait.
    Anim,
    /// Hit points, mana, trips and timers.
    Stats,
    Effects,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::RngOnly,
        Category::Rng,
        Category::Resources,
        Category::Slots,
        Category::Positions,
        Category::Orders,
        Category::Facing,
        Category::Anim,
        Category::Stats,
        Category::Effects,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::RngOnly => "rng-only",
            Category::Rng => "rng",
            Category::Resources => "resources",
            Category::Slots => "slots",
            Category::Positions => "positions",
            Category::Orders => "orders",
            Category::Facing => "facing",
            Category::Anim => "anim/wait",
            Category::Stats => "stats",
            Category::Effects => "effects",
        }
    }
}

fn category(scope: Scope, field: &str) -> Category {
    match scope {
        Scope::Global => Category::Rng,
        Scope::Player(_) => Category::Resources,
        Scope::Effect(_) => Category::Effects,
        Scope::Unit(_) => match field {
            "used" | "serial" | "type" | "owner" => Category::Slots,
            "x" | "y" | "tx" | "ty" => Category::Positions,
            "order" | "target" => Category::Orders,
            "facing" => Category::Facing,
            "anim" | "frame" | "wait" => Category::Anim,
            _ => Category::Stats,
        },
    }
}

/// Which fields to compare: all, or those named by field (`x`, `unit.x`, `effect.kind`, `rng`, `gold`) or category
/// (`positions`, `anim`, `anim/wait`, `effects`, ...). A name starting with `-` leaves that field or category out.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    include: Option<Vec<String>>,
    exclude: Vec<String>,
}

impl Filter {
    pub fn parse(s: &str) -> Filter {
        let mut f = Filter::default();
        for t in s
            .split(',')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
        {
            match t.strip_prefix('-') {
                Some(t) => f.exclude.push(t.to_string()),
                None => f.include.get_or_insert_with(Vec::new).push(t),
            }
        }
        f
    }

    fn names(scope: Scope, field: &str, t: &str) -> bool {
        let cat = category(scope, field).name();
        t == cat
            || (cat == "anim/wait" && t == "anim")
            || match scope {
                Scope::Global | Scope::Player(_) => t == field,
                Scope::Unit(_) => t == field || t.strip_prefix("unit.") == Some(field),
                Scope::Effect(_) => t.strip_prefix("effect.") == Some(field),
            }
    }

    fn allows(&self, scope: Scope, field: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|i| i.iter().any(|t| Self::names(scope, field, t)))
            && !self.exclude.iter().any(|t| Self::names(scope, field, t))
    }
}

/// Compares one record. Fields that ours doesn't model are added to `skipped`.
pub fn compare(theirs: &Record, ours: &Record, filter: &Filter, skipped: &mut BTreeSet<String>) -> Vec<Diff> {
    let mut out = Vec::new();
    if theirs.rng != ours.rng && filter.allows(Scope::Global, "rng") {
        out.push(Diff {
            scope: Scope::Global,
            field: "rng",
            ours: Cell::Int(ours.rng as i64),
            theirs: Cell::Int(theirs.rng as i64),
        });
    }
    for (r, name) in RESOURCES.iter().enumerate() {
        for p in 0..16 {
            let (t, o) = (&theirs.resources[r][p], &ours.resources[r][p]);
            if o.matches(t) == Some(false) && filter.allows(Scope::Player(p as u8), name) {
                out.push(Diff {
                    scope: Scope::Player(p as u8),
                    field: name,
                    ours: o.clone(),
                    theirs: t.clone(),
                });
            }
        }
    }
    let tables = [
        (&theirs.units, &ours.units, &UNIT_FIELDS[..], 0, "unit"),
        (
            &theirs.effects,
            &ours.effects,
            &EFFECT_FIELDS[..],
            PERMANENT_EFFECTS,
            "effect",
        ),
    ];
    for (i, (t, o, fields, first, what)) in tables.into_iter().enumerate() {
        let scope = |slot| if i == 0 { Scope::Unit(slot) } else { Scope::Effect(slot) };
        if first > 0 && (t.keys().any(|&s| s < first) || o.keys().any(|&s| s < first)) {
            skipped.insert(format!(
                "effect slots 0 to {}: the permanent effects (not modelled)",
                first - 1
            ));
        }
        let slots: BTreeSet<u16> = t.keys().chain(o.keys()).copied().filter(|&s| s >= first).collect();
        for slot in slots {
            match (t.get(&slot), o.get(&slot)) {
                (Some(tf), Some(of)) => {
                    for ((&f, tc), oc) in fields.iter().zip(tf).zip(of) {
                        match oc.matches(tc) {
                            None => {
                                if let Cell::Skip(why) = oc {
                                    skipped.insert(format!("{what} {f}: {why}"));
                                }
                            }
                            Some(false) if filter.allows(scope(slot), f) => out.push(Diff {
                                scope: scope(slot),
                                field: f,
                                ours: oc.clone(),
                                theirs: tc.clone(),
                            }),
                            Some(_) => {}
                        }
                    }
                }
                (tf, of) => {
                    if filter.allows(scope(slot), "used") {
                        out.push(Diff {
                            scope: scope(slot),
                            field: "used",
                            ours: Cell::Int(of.is_some() as i64),
                            theirs: Cell::Int(tf.is_some() as i64),
                        });
                    }
                }
            }
        }
    }
    out
}

/// One unit's fields as `name value` pairs, for context lines.
fn fields_line(names: &[&str], cells: &[Cell]) -> String {
    names
        .iter()
        .zip(cells)
        .filter(|(f, c)| !(**c == Cell::Null && **f == "trips") && !(TIMERS.contains(f) && **c == Cell::Int(0)))
        .map(|(f, c)| match (f, c) {
            (&"type", Cell::Int(n)) => format!("type {n:#04x}"),
            _ => format!("{f} {c}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The number of generator steps from state `from` to state `to` (timing-and-rng spec, "Random number generator"),
/// if it is at most [`MAX_STEPS`].
pub fn steps(from: u32, to: u32) -> Option<u32> {
    let mut s = from;
    for i in 0..=MAX_STEPS {
        if s == to {
            return Some(i);
        }
        s = s.wrapping_mul(tidewater_sim::Rng::MULTIPLIER).wrapping_add(1);
    }
    None
}

/// The draws made stepping from `from`, `count` times (at most 12 shown).
fn draws_from(from: u32, count: u32) -> Vec<u32> {
    let mut r = tidewater_sim::Rng::new(from);
    (0..count.min(12)).map(|_| r.draw()).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// Our side.

/// The order number of `u` (orders spec, "The order list").
fn order_cell(u: &Unit) -> Cell {
    if u.removing {
        return Cell::Int(0);
    }
    if u.dying {
        return Cell::Int(1);
    }
    let n = match u.order {
        Order::Idle if u.building => {
            // We hold no order number for a building at rest: it reads as order 2 until its resting logic first runs,
            // then 33 (or 37 while working, 59 for the circle of power).
            return if u.job.is_some() {
                Cell::Int(37)
            } else if u.kind == tidewater_sim::rescue::CIRCLE_OF_POWER {
                Cell::AnyOf(vec![2, 59])
            } else {
                Cell::AnyOf(vec![2, 33])
            };
        }
        Order::TowerWatch { target: None } if u.building => return Cell::AnyOf(vec![2, 14]),
        Order::Idle => 2,
        Order::Move { .. } => 3,
        Order::MoveAttack { .. } => 4,
        Order::Patrol { .. } => 5,
        Order::Follow { .. } => 6,
        Order::Guard { .. } => 7,
        Order::Attack { .. } => 9,
        Order::AttackMove { .. } => 10,
        Order::AttackWall { .. } => 11,
        Order::Chase { .. } => 12,
        Order::StandGround => 13,
        Order::TowerWatch { .. } => 14,
        Order::AttackInPlace { .. } => 15,
        Order::IdleReact { .. } => 16,
        Order::AttackGround { .. } => 17,
        Order::AttackGroundMove { .. } => 18,
        Order::Demolish { .. } => 20,
        Order::DemolishAt { .. } => 21,
        Order::InsideSite { .. } => 22,
        Order::Harvest { .. } => 23,
        Order::ReturnGoods { .. } => 24,
        Order::Enter { .. } => 25,
        Order::Inside { .. } => 26,
        Order::Repair { .. } => 27,
        Order::Build { .. } => 28,
        Order::Unload => 29,
        Order::Land { .. } => 30,
        Order::PushOff => 31,
        Order::Docked => 32,
        Order::Board { .. } => 34,
        Order::Aboard { .. } => 36,
        Order::Cast { spell, .. } => spell as i64,
    };
    Cell::Int(n)
}

/// The slot of the unit the order aims at, as far as our orders name one.
fn target_cell(w: &World, u: &Unit) -> Cell {
    let id = match u.order {
        Order::Follow { target }
        | Order::Guard { target }
        | Order::Attack { target }
        | Order::Chase { target }
        | Order::AttackInPlace { target }
        | Order::IdleReact { target }
        | Order::Repair { target } => Some(target),
        Order::TowerWatch { target } | Order::Cast { target, .. } => target,
        Order::Harvest { source, .. } => source,
        Order::ReturnGoods { depot } => depot,
        Order::Enter { building, .. } | Order::Inside { building, .. } | Order::InsideSite { building } => {
            Some(building)
        }
        Order::Demolish { at, .. } => at,
        Order::Board { transport } | Order::Aboard { transport } => Some(transport),
        _ => None,
    };
    Cell::opt(id.and_then(|id| w.unit(id)).map(|t| t.slot))
}

fn unit_cells(w: &World, u: &Unit) -> Vec<Cell> {
    let stats = w.stats.0.get(u.kind as usize).copied().unwrap_or_default();
    let caster = stats.flags & flags::CASTER != 0;
    let berserker = u.kind == BERSERKER && w.players.get(u.owner as usize).is_some_and(|p| p.upgrades.marksmanship);
    UNIT_FIELDS
        .iter()
        .map(|&f| match f {
            "serial" => Cell::Int(u.id.0 as i64),
            "type" => Cell::Int(u.kind as i64),
            "owner" => Cell::Int(u.owner as i64),
            "x" => Cell::Int(u.pos.0 as i64),
            "y" => Cell::Int(u.pos.1 as i64),
            "tx" => Cell::Int(u.tile.0 as i64),
            "ty" => Cell::Int(u.tile.1 as i64),
            "hp" => Cell::Int(u.hp as i64),
            "order" => order_cell(u),
            // Timing-and-rng spec, "What draws random numbers": only a mobile unit's facing is defined at creation.
            "facing" if u.building => {
                Cell::Skip("buildings (no spec gives a building's facing; ours is a placeholder)")
            }
            "facing" => Cell::Int(u.facing as i64),
            "anim" => Cell::Int(u.script.anim as i64),
            "frame" => Cell::Int(u.script.frame as i64),
            "wait" => Cell::Int(u.script.wait as i64),
            "mana" if caster || berserker => Cell::Int(u.mana as i64),
            "mana" => Cell::Skip("only spellcasters and regenerating berserkers (no spec defines it for other types)"),
            "target" => target_cell(w, u),
            "trips" if TRIP_TYPES.contains(&u.kind) => Cell::Int(u.resources as i64),
            "trips" => Cell::Null,
            "invisibility" => Cell::Int(u.invisibility as i64),
            "unholy_armour" => Cell::Int(u.unholy_armor as i64),
            "bloodlust" => Cell::Int(u.bloodlust as i64),
            "haste" => Cell::Int(u.haste as i64),
            _ => unreachable!("unit field {f}"),
        })
        .collect()
}

/// Our world's state as a record.
pub fn snapshot(w: &World, n: u64, kind: &str) -> Record {
    let res = |f: fn(&tidewater_sim::Player) -> u32| w.players.iter().map(|p| Cell::Int(f(p) as i64)).collect();
    let units = w
        .units
        .iter()
        .map(|u| (u.slot, unit_cells(w, u)))
        .collect::<BTreeMap<_, _>>();
    let slot_of = |id| w.unit(id).map(|t| t.slot);
    let effects = w
        .missiles
        .iter()
        .enumerate()
        .filter_map(|(s, m)| Some((s as u16, m.as_ref()?)))
        .map(|(s, m)| {
            let cells = vec![
                Cell::Int(m.kind as i64),
                Cell::Int(m.pos.0 as i64),
                Cell::Int(m.pos.1 as i64),
                Cell::Int(m.to.0 as i64),
                Cell::Int(m.to.1 as i64),
                Cell::opt(m.target.and_then(slot_of)),
                Cell::opt(slot_of(m.attacker)),
                Cell::Int(m.damage as i64),
            ];
            (s, cells)
        })
        .collect();
    Record {
        n,
        kind: kind.to_string(),
        counter: None,
        rng: w.rng.state,
        resources: [res(|p| p.gold), res(|p| p.lumber), res(|p| p.oil)],
        units,
        effects,
    }
}

fn controller_name(c: u8) -> String {
    use tidewater_sim::ai::controller as k;
    match c {
        k::HUMAN => "human".into(),
        k::COMPUTER => "computer".into(),
        k::PASSIVE => "passive".into(),
        k::NOBODY => "nobody".into(),
        c => format!("code {c}"),
    }
}

fn race_name(r: u8) -> &'static str {
    match r {
        0 => "human",
        1 => "orc",
        _ => "neutral",
    }
}

/// Where our set-up game differs from the header's (era, map size, local player, rules, table sizes, controllers,
/// races).
pub fn setup_mismatches(h: &Header, w: &World, era: u8, local: u8) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |what: &str, ours: String, theirs: String| {
        if ours != theirs {
            out.push(format!("{what}: ours {ours}, theirs {theirs}"));
        }
    };
    check("era", era.to_string(), h.era.to_string());
    check("map size", w.terrain.width.to_string(), h.map_size.to_string());
    check("local player", local.to_string(), h.local_player.to_string());
    check("enhanced", w.rules.enhanced.to_string(), h.enhanced.to_string());
    check("unit slots", w.rules.unit_slots.to_string(), h.unit_slots.to_string());
    check("effect slots", w.missiles.len().to_string(), h.effect_slots.to_string());
    for (p, pl) in w.players.iter().enumerate() {
        if let Some(c) = h.controllers.get(p) {
            check(
                &format!("player {p} controller"),
                controller_name(pl.controller),
                c.clone(),
            );
        }
        if let Some(r) = h.races.get(p) {
            check(&format!("player {p} race"), race_name(pl.race).to_string(), r.clone());
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------
// The run.

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Report every differing record instead of stopping at the first.
    pub keep_going: bool,
    /// After a record whose rng differs, copy theirs into ours; a record where only rng differs doesn't stop the run.
    pub resync_rng: bool,
    /// Stop after the record with this `n`.
    pub max_cycles: Option<u64>,
    pub filter: Filter,
    /// The seed, to count the draws before the setup record.
    pub seed: Option<u32>,
}

/// Per field: records that differed, first and last `n`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldStat {
    pub records: usize,
    pub first: u64,
    pub last: u64,
    /// Distinct units, effects or players where it differed.
    pub places: usize,
}

/// The first divergence of one category, with context.
#[derive(Clone, Debug)]
pub struct First {
    pub n: u64,
    pub kind: String,
    pub diff: Diff,
    pub context: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Summary {
    pub records: usize,
    pub first_n: Option<u64>,
    pub last_n: Option<u64>,
    /// Records with at least one difference.
    pub diverged: usize,
    pub resyncs: usize,
    /// The record that stopped the run, as (`n`, `kind`).
    pub stopped: Option<(u64, String)>,
    pub first: BTreeMap<Category, First>,
    pub fields: BTreeMap<String, FieldStat>,
    /// Each place and field when it starts differing (it didn't differ in the previous record), in order.
    pub onsets: Vec<(u64, String, Diff)>,
    pub skipped: BTreeSet<String>,
}

/// Context lines for a record's differences: draw counts for an rng difference, and both sides of each unit or
/// effect involved.
fn context(theirs: &Record, ours: &Record, diffs: &[Diff], prev: (u32, u32)) -> Vec<String> {
    let mut out = Vec::new();
    if diffs.iter().any(|d| d.scope == Scope::Global && d.field == "rng") {
        let count = |from, to| steps(from, to).map_or_else(|| "?".to_string(), |n| n.to_string());
        let mut line = format!(
            "draws since the previous record: ours {}, theirs {}",
            count(prev.1, ours.rng),
            count(prev.0, theirs.rng)
        );
        if let Some(n) = steps(prev.0, theirs.rng).filter(|&n| n > 0) {
            line += &format!(
                "; theirs drew {:?} (from {:#010x}){}",
                draws_from(prev.0, n),
                prev.0,
                if n > 12 { " ..." } else { "" }
            );
        }
        out.push(line);
    }
    let mut seen = BTreeSet::new();
    for d in diffs {
        if !seen.insert(d.scope) {
            continue;
        }
        let (names, t, o, what) = match d.scope {
            Scope::Unit(s) => (
                &UNIT_FIELDS[..],
                theirs.units.get(&s),
                ours.units.get(&s),
                format!("unit {s}"),
            ),
            Scope::Effect(s) => (
                &EFFECT_FIELDS[..],
                theirs.effects.get(&s),
                ours.effects.get(&s),
                format!("effect {s}"),
            ),
            _ => continue,
        };
        for (side, cells) in [("ours  ", o), ("theirs", t)] {
            out.push(match cells {
                Some(c) => format!("{what} {side}: {}", fields_line(names, c)),
                None => format!("{what} {side}: (free)"),
            });
        }
    }
    out
}

fn print_record(out: &mut dyn Write, title: &str, theirs: &Record, diffs: &[Diff], context: &[String]) -> Result<()> {
    let counter = theirs.counter.map_or(String::new(), |c| format!(", counter {c}"));
    writeln!(out, "{title} at n={} ({}{counter}):", theirs.n, theirs.kind)?;
    for d in diffs.iter().take(40) {
        writeln!(out, "  {d}")?;
    }
    if diffs.len() > 40 {
        writeln!(out, "  ... and {} more", diffs.len() - 40)?;
    }
    for c in context.iter().take(40) {
        writeln!(out, "    {c}")?;
    }
    Ok(())
}

/// Steps `w` through the trace's records and compares each, printing the first divergence (and, with `--keep-going`
/// or `--resync-rng`, carrying on as the options say). Setup and turn records are compared without stepping: no
/// commands are replayed yet (the format's open question "Command turns").
pub fn run(
    w: &mut World,
    records: impl Iterator<Item = Result<Record>>,
    opts: &Options,
    out: &mut dyn Write,
) -> Result<Summary> {
    let mut s = Summary::default();
    let seed = opts.seed.unwrap_or(w.rng.state);
    let mut prev = (seed, seed);
    let mut last_n: Option<u64> = None;
    let mut last_kind: Option<String> = None;
    // The differences of the previous record, and every place each field has differed.
    let mut before: BTreeSet<(Scope, &'static str)> = BTreeSet::new();
    let mut places: BTreeMap<String, BTreeSet<Scope>> = BTreeMap::new();
    for rec in records {
        let theirs = rec?;
        if opts.max_cycles.is_some_and(|m| theirs.n > m) {
            break;
        }
        match theirs.kind.as_str() {
            "setup" | "turn" => {}
            "startup" | "cycle" => {
                if let Some(l) = last_n
                    && theirs.n != l + 1
                {
                    writeln!(out, "note: record n={} follows n={l}; stepping once", theirs.n)?;
                }
                // Scheduler rule 1: the counter is reset after the start-up cycles.
                if theirs.kind == "cycle" && last_kind.as_deref() == Some("startup") {
                    w.end_start_up();
                }
                w.step(&[]);
            }
            k => {
                writeln!(out, "note: skipping record n={} of unknown kind `{k}`", theirs.n)?;
                continue;
            }
        }
        last_n = Some(theirs.n);
        last_kind = Some(theirs.kind.clone());
        let ours = snapshot(w, theirs.n, &theirs.kind);
        let diffs = compare(&theirs, &ours, &opts.filter, &mut s.skipped);
        s.records += 1;
        s.first_n.get_or_insert(theirs.n);
        s.last_n = Some(theirs.n);
        let now: BTreeSet<(Scope, &'static str)> = diffs.iter().map(|d| (d.scope, d.field)).collect();
        let new: Vec<&Diff> = diffs.iter().filter(|d| !before.contains(&(d.scope, d.field))).collect();
        for d in &new {
            s.onsets.push((theirs.n, theirs.kind.clone(), (*d).clone()));
        }
        before = now;
        if !diffs.is_empty() {
            s.diverged += 1;
            let is_rng = |d: &Diff| d.scope == Scope::Global && d.field == "rng";
            let rng_only = diffs.iter().all(is_rng);
            // For the summary: the generator differs and nothing else started differing here.
            let rng_alone = diffs.iter().any(is_rng) && new.iter().all(|d| is_rng(d));
            let ctx = context(&theirs, &ours, &diffs, prev);
            let mut cats: Vec<(Category, &Diff)> = diffs.iter().map(|d| (d.category(), d)).collect();
            if rng_alone {
                cats.push((
                    Category::RngOnly,
                    diffs.iter().find(|d| is_rng(d)).expect("an rng difference"),
                ));
            }
            for (cat, d) in cats {
                s.first.entry(cat).or_insert_with(|| First {
                    n: theirs.n,
                    kind: theirs.kind.clone(),
                    diff: d.clone(),
                    context: ctx.clone(),
                });
            }
            let mut keys = BTreeSet::new();
            for d in &diffs {
                keys.insert(d.key());
                places.entry(d.key()).or_default().insert(d.scope);
            }
            for k in keys {
                let f = s.fields.entry(k.clone()).or_insert(FieldStat {
                    records: 0,
                    first: theirs.n,
                    last: theirs.n,
                    places: 0,
                });
                f.records += 1;
                f.last = theirs.n;
                f.places = places[&k].len();
            }
            let stop = !opts.keep_going && !(opts.resync_rng && rng_only);
            if s.diverged == 1 {
                print_record(out, "first divergence", &theirs, &diffs, &ctx)?;
            } else if stop {
                print_record(out, "stopping: divergence", &theirs, &diffs, &ctx)?;
            }
            if stop {
                s.stopped = Some((theirs.n, theirs.kind.clone()));
                break;
            }
            if opts.resync_rng && theirs.rng != w.rng.state {
                w.rng.state = theirs.rng;
                s.resyncs += 1;
            }
        }
        prev = (theirs.rng, w.rng.state);
    }
    Ok(s)
}

/// Prints the summary: counts, the first divergence of each category, the fields that differed and what wasn't
/// compared.
pub fn print_summary(out: &mut dyn Write, s: &Summary) -> Result<()> {
    writeln!(
        out,
        "compared {} records (n {}..{}): {} diverged, {} rng resyncs{}",
        s.records,
        s.first_n.unwrap_or(0),
        s.last_n.unwrap_or(0),
        s.diverged,
        s.resyncs,
        s.stopped
            .as_ref()
            .map_or(String::new(), |(n, k)| format!("; stopped at n={n} ({k})"))
    )?;
    if s.diverged == 0 {
        writeln!(out, "no divergence")?;
    }
    if !s.first.is_empty() {
        writeln!(out, "first divergence by category:")?;
        for c in Category::ALL {
            let Some(f) = s.first.get(&c) else { continue };
            writeln!(out, "  {:<10} n={} ({}): {}", c.name(), f.n, f.kind, f.diff)?;
            let unit = match f.diff.scope {
                Scope::Unit(u) => Some(format!("unit {u} ")),
                Scope::Effect(e) => Some(format!("effect {e} ")),
                _ => None,
            };
            for line in &f.context {
                let relevant = match &unit {
                    Some(prefix) => line.starts_with(prefix.as_str()),
                    None => line.starts_with("draws"),
                };
                if relevant {
                    writeln!(out, "      {line}")?;
                }
            }
        }
    }
    if !s.fields.is_empty() {
        writeln!(out, "fields that differ (places: distinct units, effects or players):")?;
        writeln!(
            out,
            "  {:<20} {:>8} {:>8} {:>8} {:>8}",
            "field", "records", "places", "first n", "last n"
        )?;
        for (k, f) in &s.fields {
            writeln!(
                out,
                "  {:<20} {:>8} {:>8} {:>8} {:>8}",
                k, f.records, f.places, f.first, f.last
            )?;
        }
    }
    let state: Vec<_> = s.onsets.iter().filter(|(_, _, d)| d.scope != Scope::Global).collect();
    if !state.is_empty() {
        writeln!(
            out,
            "onsets (a field of a unit, effect or player starts differing; rng left out), first {} of {}:",
            state.len().min(ONSETS_SHOWN),
            state.len()
        )?;
        for (n, kind, d) in state.iter().take(ONSETS_SHOWN) {
            writeln!(out, "  n={n:<5} {kind:<8} {d}")?;
        }
    }
    if !s.skipped.is_empty() {
        writeln!(out, "not compared:")?;
        for k in &s.skipped {
            writeln!(out, "  {k}")?;
        }
    }
    writeln!(
        out,
        "note: a building's resting order is compared as 2 or 33 (14 for towers, 59 for the circle of power): our \
         simulation doesn't record the switch from 2 to the resting order"
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tidewater_formats::script::Scripts;
    use tidewater_formats::udta::{UnitStats, UnitTable};

    /// A synthetic world: an open 8 × 8 map, a land unit type 0 whose every animation waits 5 cycles and then signals
    /// "step done", and three units of type 0.
    fn world(seed: u32) -> World {
        let terrain = tidewater_sim::Terrain::new(
            8,
            8,
            vec![0; 64],
            vec![0; 64],
            vec![0; 64],
            Arc::new(Default::default()),
        );
        let stats = UnitTable(vec![
            UnitStats {
                hit_points: 60,
                size: (1, 1),
                flags: flags::LAND,
                ..Default::default()
            };
            110
        ]);
        let mut b = vec![0u8; 300];
        for i in 0..110 + 29 {
            b[i * 2..i * 2 + 2].copy_from_slice(&280u16.to_le_bytes());
        }
        for a in 0..7 {
            b[280 + a * 2..282 + a * 2].copy_from_slice(&294u16.to_le_bytes());
        }
        b[294..300].copy_from_slice(&[1, 5, 0, 3, 38, 1]);
        let scripts = Scripts::parse(b).unwrap();
        let mut w = World::new(terrain, stats, Arc::new(scripts), seed);
        for (owner, tile) in [(0, (1, 1)), (1, (5, 5)), (1, (3, 6))] {
            w.spawn(0, owner, tile, 0).unwrap();
        }
        w
    }

    fn header() -> Header {
        Header {
            scenario: "synthetic".into(),
            clock: Some(7),
            speed: 8,
            enhanced: true,
            map_size: 8,
            unit_slots: 1600,
            effect_slots: 400,
            ..Default::default()
        }
    }

    /// A trace of our own synthetic world: setup, two start-up cycles and `cycles` cycles.
    fn trace(cycles: u64) -> Vec<Record> {
        let mut w = world(7);
        let mut out = vec![snapshot(&w, 0, "setup")];
        for n in 1..=2 + cycles {
            w.step(&[]);
            let mut r = snapshot(&w, n, if n <= 2 { "startup" } else { "cycle" });
            r.counter = n.checked_sub(2).filter(|&c| c > 0);
            out.push(r);
        }
        out
    }

    /// The trace as text, through the JSON writer and reader.
    fn text(records: &[Record]) -> String {
        let mut s = header().to_json() + "\n";
        for r in records {
            s += &r.to_json();
            s += "\n";
        }
        s
    }

    fn run_text(t: String, opts: &Options) -> (Summary, String) {
        let (h, records) = read(reader(std::io::Cursor::new(t.into_bytes())).unwrap()).unwrap();
        assert_eq!(h, header());
        let mut w = world(7);
        let mut out = Vec::new();
        let s = run(&mut w, records, opts, &mut out).unwrap();
        print_summary(&mut out, &s).unwrap();
        (s, String::from_utf8(out).unwrap())
    }

    fn set(r: &mut Record, slot: u16, field: &str, v: i64) {
        let i = UNIT_FIELDS.iter().position(|&f| f == field).unwrap();
        r.units.get_mut(&slot).unwrap()[i] = Cell::Int(v);
    }

    #[test]
    fn records_round_trip_through_json() {
        for mut r in trace(3) {
            let back = Record::parse(&r.to_json()).unwrap();
            // Fields we don't compare are written as absent, and read back as null.
            for c in r.units.values_mut().flatten() {
                if matches!(c, Cell::Skip(_)) {
                    *c = Cell::Null;
                }
            }
            assert_eq!(back, r);
        }
        assert_eq!(Header::parse(&header().to_json()).unwrap(), header());
    }

    #[test]
    fn identical_trace_has_no_divergence() {
        let (s, out) = run_text(text(&trace(20)), &Options::default());
        assert_eq!((s.records, s.diverged, s.stopped), (23, 0, None));
        assert!(out.contains("no divergence"), "{out}");
    }

    #[test]
    fn stops_at_the_first_field_divergence_with_context() {
        let mut t = trace(20);
        // The first unit created takes slot 1599.
        set(&mut t[7], 1599, "facing", 3);
        set(&mut t[9], 1599, "x", 99);
        let (s, out) = run_text(text(&t), &Options::default());
        assert_eq!(s.stopped, Some((7, "cycle".to_string())));
        assert_eq!(s.diverged, 1);
        assert!(out.contains("first divergence at n=7 (cycle, counter 5)"), "{out}");
        assert!(out.contains("unit slot 1599 facing: ours"), "{out}");
        assert!(out.contains("unit 1599 theirs:") && out.contains("facing 3"), "{out}");
        assert_eq!(s.first[&Category::Facing].n, 7);
    }

    #[test]
    fn resync_carries_on_past_an_rng_only_divergence() {
        let mut t = trace(20);
        // From n = 5 on, theirs made one more draw than ours.
        for r in &mut t[5..] {
            r.rng = r.rng.wrapping_mul(tidewater_sim::Rng::MULTIPLIER).wrapping_add(1);
        }
        let (s, out) = run_text(text(&t), &Options::default());
        assert_eq!(s.stopped, Some((5, "cycle".to_string())));
        assert!(out.contains("rng: ours"), "{out}");
        assert!(
            out.contains("draws since the previous record: ours 0, theirs 1"),
            "{out}"
        );
        assert!(s.first.contains_key(&Category::RngOnly));

        let opts = Options {
            resync_rng: true,
            ..Default::default()
        };
        let (s, _) = run_text(text(&t), &opts);
        assert_eq!((s.stopped, s.diverged, s.resyncs), (None, 1, 1));
    }

    #[test]
    fn resync_still_stops_at_a_state_divergence() {
        let mut t = trace(20);
        t[4].rng ^= 1;
        set(&mut t[8], 1598, "hp", 1);
        let opts = Options {
            resync_rng: true,
            ..Default::default()
        };
        let (s, out) = run_text(text(&t), &opts);
        assert_eq!(s.stopped, Some((8, "cycle".to_string())));
        assert!(out.contains("stopping: divergence at n=8"), "{out}");
        assert!(out.contains("unit slot 1598 hp: ours 60, theirs 1"), "{out}");
    }

    #[test]
    fn keep_going_summarises_fields() {
        let mut t = trace(20);
        for r in &mut t[5..=10] {
            set(r, 1597, "x", 1);
        }
        t[12].units.remove(&1597);
        let opts = Options {
            keep_going: true,
            ..Default::default()
        };
        let (s, out) = run_text(text(&t), &opts);
        assert_eq!(s.stopped, None);
        assert_eq!(s.diverged, 7);
        assert_eq!(
            s.fields["unit.x"],
            FieldStat {
                records: 6,
                first: 5,
                last: 10,
                places: 1
            }
        );
        assert_eq!(s.fields["unit.used"].first, 12);
        assert!(out.contains("unit slot 1597: ours in use, theirs free"), "{out}");
        assert!(out.contains("positions  n=5"), "{out}");
        let onsets: Vec<(u64, &str)> = s.onsets.iter().map(|(n, _, d)| (*n, d.field)).collect();
        assert_eq!(onsets, [(5, "x"), (12, "used")]);
        assert!(
            out.contains("n=12    cycle    unit slot 1597: ours in use, theirs free"),
            "{out}"
        );
    }

    #[test]
    fn field_filter_and_max_cycles() {
        let mut t = trace(20);
        set(&mut t[6], 1599, "facing", 3);
        set(&mut t[15], 1599, "x", 5);
        let opts = Options {
            filter: Filter::parse("positions"),
            ..Default::default()
        };
        let (s, _) = run_text(text(&t), &opts);
        assert_eq!(s.stopped, Some((15, "cycle".to_string())));
        let opts = Options {
            filter: Filter::parse("positions"),
            max_cycles: Some(14),
            ..Default::default()
        };
        let (s, _) = run_text(text(&t), &opts);
        assert_eq!((s.records, s.diverged), (15, 0));
        let f = Filter::parse("unit.facing,effect.kind,rng");
        assert!(f.allows(Scope::Unit(1), "facing") && f.allows(Scope::Global, "rng"));
        assert!(!f.allows(Scope::Unit(1), "x") && !f.allows(Scope::Effect(4), "x"));
        assert!(f.allows(Scope::Effect(4), "kind") && !f.allows(Scope::Unit(1), "kind"));
        let f = Filter::parse("-facing,-effects");
        assert!(f.allows(Scope::Unit(1), "x") && f.allows(Scope::Global, "rng"));
        assert!(!f.allows(Scope::Unit(1), "facing") && !f.allows(Scope::Effect(4), "kind"));
    }

    #[test]
    fn reads_gzip() {
        use std::io::Write as _;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(text(&trace(4)).as_bytes()).unwrap();
        let bytes = gz.finish().unwrap();
        let (_, records) = read(reader(std::io::Cursor::new(bytes)).unwrap()).unwrap();
        let kinds: Vec<String> = records.map(|r| r.unwrap().kind).collect();
        assert_eq!(
            kinds,
            ["setup", "startup", "startup", "cycle", "cycle", "cycle", "cycle"]
        );
    }

    #[test]
    fn permanent_effects_and_missing_fields() {
        let line = r#"{"n":0,"kind":"setup","rng":5,"gold":[1],"units":[{"slot":9,"serial":1,"type":92,"trips":4,"timers":{"haste":-3}}],"effects":[{"slot":0,"kind":19,"x":0,"y":0,"dest":[0,0],"target":null,"shooter":null,"damage":0},{"slot":3,"kind":1,"x":1,"y":2,"dest":[3,4],"target":9,"shooter":null,"damage":5}]}"#;
        let r = Record::parse(line).unwrap();
        assert_eq!(r.resources[0][0], Cell::Int(1));
        assert_eq!(r.resources[0][1], Cell::Null);
        let u = &r.units[&9];
        assert_eq!(u[15], Cell::Int(4));
        assert_eq!(u[19], Cell::Int(-3));
        assert_eq!(u[16], Cell::Int(0));
        assert_eq!(r.effects[&3][3..5], [Cell::Int(3), Cell::Int(4)]);
        let mut ours = r.clone();
        ours.effects.remove(&0);
        let mut skipped = BTreeSet::new();
        let d = compare(&r, &ours, &Filter::default(), &mut skipped);
        assert!(d.is_empty(), "{d:?}");
        assert!(skipped.iter().any(|s| s.contains("permanent effects")));
    }

    #[test]
    fn any_of_matches_each_value() {
        let ours = Cell::AnyOf(vec![2, 33]);
        assert_eq!(ours.matches(&Cell::Int(2)), Some(true));
        assert_eq!(ours.matches(&Cell::Int(33)), Some(true));
        assert_eq!(ours.matches(&Cell::Int(37)), Some(false));
        assert_eq!(Cell::Skip("x").matches(&Cell::Int(1)), None);
    }

    #[test]
    fn counts_generator_steps() {
        // Timing-and-rng spec: seed 0, state after 6 draws.
        assert_eq!(steps(0, 0xAD88_9112), Some(6));
        assert_eq!(steps(5, 5), Some(0));
        assert_eq!(draws_from(0, 3), [0, 346, 130]);
    }

    #[test]
    fn setup_checks_compare_with_the_header() {
        let w = world(7);
        let mut h = header();
        h.controllers = vec!["nobody".into(); 16];
        h.races = vec!["human".into(); 16];
        let m = setup_mismatches(&h, &w, 0, 0);
        // The synthetic world's players have controller 0, which has no name.
        assert!(
            m.iter().any(|s| s.starts_with("player 0 controller: ours code 0")),
            "{m:?}"
        );
        assert!(!m.iter().any(|s| s.contains("era") || s.contains("map size")), "{m:?}");
    }
}
