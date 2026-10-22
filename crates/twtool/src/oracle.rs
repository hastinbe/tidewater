//! `twtool oracle`: runs our simulation alongside a per-cycle trace of the original game and reports where the two
//! first differ.
//!
//! The trace format is `docs/specs/oracle/trace-format.md` (format versions 1 and 2): JSON Lines, optionally
//! gzip-compressed, a header line and then one record per simulated cycle (`setup`, `startup`, `cycle`, `turn`),
//! compared by `n` and `kind` in file order. A version 2 `turn` record lists the commands carried out in it; they are
//! applied to our world in that turn before it is compared (see [`crate::replay`]). The fields mean what the specs that format cites say: `core/unit-slots.md` (unit and effect
//! slots, creation numbers, owners, the three permanent effects), `core/timing-and-rng.md` (the generator's state and
//! step), `core/animation-scripts.md` (pixel positions, animation, frame and wait), `orders/orders.md` ("The order
//! list"), `economy/harvest.md` (trips) and `spells/spells.md` (mana and timers).
//!
//! The opt-in header fields (2026-10-14 errata, still version 2): a windowed trace (`from` = F) leaves out the
//! records before F but the setup, the start-up and the command turns; we compare the setup and start-up records, run
//! up to F − 1 carrying out the gap's command turns without comparing them, then compare from F on. A setup-only trace
//! (`setup_only`) is compared on its setup record. The `extras` (`hits`, and the units' `goal`, `pending` and
//! `transport_dest`) are read but not compared, as no spec defines their comparison yet. The round-85 `aigoals`
//! extra (each unit's `ai_goal`) has its kind and tile compared with our units' goals (`ai/attack-ai.md`, "Unit
//! goals"; a unit with no goal tile shows (0, 0)); its `extra` value is read by no rule in the specs and isn't.
//! The round-91 `fogmaps` extra (summaries of the "seen", visible and explored maps and the local player's units on
//! explored tiles) is compared where a record carries it; its `say` and `fog` records (a cheat word typed, the fog
//! option switched during play; vector F45) hold no units and no generator state, so only the option and the maps
//! are compared, and a `fog` record switches the option in our world.
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
use tidewater_sim::worker_ai::AiJob;
use tidewater_sim::{Order, Unit, World};

use crate::replay::{self, Entry};

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
/// Positions in [`UNIT_FIELDS`] the goal comparison reads, and the repair order's number.
const UNIT_TYPE: usize = 1;
const UNIT_ORDER: usize = 8;
const UNIT_TARGET: usize = 14;
const REPAIR_ORDER: i64 = 27;
const TIMERS: [&str; 4] = ["invisibility", "unholy_armour", "bloodlust", "haste"];

/// Effect fields in report order; `dest` is flattened into `dest_x` and `dest_y`.
pub const EFFECT_FIELDS: [&str; 8] = ["kind", "x", "y", "dest_x", "dest_y", "target", "shooter", "damage"];

/// Types that carry `trips` (trace format, "Unit fields"): the oil platforms, the gold mine and the oil patch.
const TRIP_TYPES: [u8; 4] = [0x56, 0x57, 0x5c, 0x5d];
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
    /// A short description of a map, for the `fogmaps` extra's summaries.
    Text(String),
    /// Not compared, for the reason given (listed once in the report). No field is skipped at present.
    #[allow(dead_code)]
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
            (a, b) => a == b,
        })
    }

    #[cfg(test)]
    fn json(&self) -> Option<Value> {
        match self {
            Cell::Int(n) => Some(Value::from(*n)),
            Cell::Null => Some(Value::Null),
            Cell::Text(t) => Some(Value::from(t.clone())),
            Cell::Skip(_) => None,
        }
    }
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Cell::Int(n) => write!(f, "{n}"),
            Cell::Null => f.write_str("null"),
            Cell::Text(t) => f.write_str(t),
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
    /// Format version, 1 or 2.
    pub version: u8,
    /// The scenario's commands in file order (version 2; empty for version 1).
    pub input: Vec<Entry>,
    /// The `rules` row: `"enhanced"` or `"classic"`, matching `enhanced` (absent from older traces).
    pub rules: Option<String>,
    /// The multiplayer launch fields ("Multiplayer launch"); `None` for a campaign start.
    pub multiplayer: Option<Multiplayer>,
    /// `from`: a windowed trace's F, the first `n` from which every `cycle` and `turn` record is written ("Windowed
    /// traces"); `None` for a trace complete from n 0.
    pub from: Option<u64>,
    /// `setup_only`: the trace is the header and the `setup` record ("Setup-only traces").
    pub setup_only: bool,
    /// `extras`: the opt-in extras the records carry (`"goals"`, `"hits"`; "Opt-in extras"). Empty when absent.
    pub extras: Vec<String>,
}

/// The header fields of a multiplayer launch (trace format, "Multiplayer launch").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Multiplayer {
    /// `connection`: `"direct-connect"`.
    pub connection: String,
    /// `map`: the scenario file name from the game's `Maps/` folder.
    pub map: String,
    /// `start_seed`: the lobby's start seed, 0 to 255.
    pub start_seed: u8,
    /// `humans`: human players in the lobby (the traced player and idle ones).
    pub humans: u8,
    /// `opponents`: the lobby rows set to Computer, 0 to 6 (round-60 harness). `None` in traces made before round 60;
    /// for those the comparer counts the computer controllers.
    pub opponents: Option<u8>,
    /// `starts`: `"fixed"` (Fixed Order) is true; `"random"`, and a trace made before round 60 without the field, is
    /// false.
    pub fixed_order: bool,
    /// `fog`: the lobby's Fog of War option; `"off"` is false. A trace without the field has it on.
    pub fog_of_war: bool,
}

impl Multiplayer {
    /// The seed the game runs with ("Start seed", step 2): (`start_seed` + 4) mod 256.
    pub fn game_seed(&self) -> u32 {
        (self.start_seed as u32 + 4) % 256
    }
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
        if !(1..=2).contains(&version) {
            bail!("trace format version {version}; this comparer reads versions 1 and 2");
        }
        let input = v
            .get("input")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, e)| Entry::parse(e).with_context(|| format!("header input entry {}", i + 1)))
            .collect::<Result<Vec<_>>>()?;
        let text = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let enhanced = v.get("enhanced").and_then(Value::as_bool).unwrap_or(true);
        let rules = v.get("rules").and_then(Value::as_str).map(str::to_string);
        let want = if enhanced { "enhanced" } else { "classic" };
        if let Some(r) = rules.as_deref()
            && r != want
        {
            bail!("header `rules` {r:?} doesn't match `enhanced` {enhanced}");
        }
        // A missing `launch_path` means a campaign start ("Multiplayer launch").
        let multiplayer = match v.get("launch_path").and_then(Value::as_str) {
            None => None,
            Some("multiplayer") => Some(Multiplayer {
                connection: text("connection"),
                map: v
                    .get("map")
                    .and_then(Value::as_str)
                    .context("a multiplayer launch without `map`")?
                    .to_string(),
                start_seed: u8::try_from(int(&v, "start_seed")?).context("`start_seed` is 0 to 255")?,
                humans: int(&v, "humans")? as u8,
                opponents: match v.get("opponents") {
                    None => None,
                    Some(_) => Some(
                        u8::try_from(int(&v, "opponents")?)
                            .ok()
                            .filter(|&o| o <= 6)
                            .context("`opponents` is 0 to 6")?,
                    ),
                },
                fixed_order: match v.get("starts").map(Value::as_str) {
                    None | Some(Some("random")) => false,
                    Some(Some("fixed")) => true,
                    Some(s) => bail!("unknown `starts` {s:?}"),
                },
                fog_of_war: match v.get("fog").map(Value::as_str) {
                    None | Some(Some("on")) => true,
                    Some(Some("off")) => false,
                    Some(s) => bail!("unknown `fog` {s:?}"),
                },
            }),
            Some(p) => bail!("unknown `launch_path` {p:?}"),
        };
        // The opt-in fields ("Windowed traces", "Setup-only traces", "Opt-in extras"); none changes the rules.
        let from = match v.get("from") {
            None => None,
            Some(f) => Some(
                f.as_u64()
                    .filter(|&f| f >= 1)
                    .context("`from` is a whole number of at least 1")?,
            ),
        };
        let setup_only = match v.get("setup_only") {
            None => false,
            Some(b) => b.as_bool().context("`setup_only` is true or false")?,
        };
        if setup_only && from.is_some() {
            bail!("a setup-only trace with `from`");
        }
        let extras = match v.get("extras") {
            None => Vec::new(),
            Some(e) => e
                .as_array()
                .and_then(|a| a.iter().map(|s| s.as_str().map(str::to_string)).collect())
                .context("`extras` is a list of names")?,
        };
        Ok(Header {
            scenario: text("scenario"),
            launch: text("launch"),
            clock: v.get("clock").and_then(Value::as_u64).map(|c| c as u32),
            speed: int(&v, "speed")? as u8,
            enhanced,
            era: int(&v, "era")? as u8,
            map_size: int(&v, "map_size")? as u32,
            local_player: int(&v, "local_player")? as u8,
            controllers: strings(&v, "controllers"),
            races: strings(&v, "races"),
            cheats: strings(&v, "cheats"),
            unit_slots: int(&v, "unit_slots")? as usize,
            effect_slots: int(&v, "effect_slots")? as usize,
            version: version as u8,
            input,
            rules,
            multiplayer,
            from,
            setup_only,
            extras,
        })
    }

    /// The header as one JSON line (for synthetic traces).
    #[cfg(test)]
    pub fn to_json(&self) -> String {
        let mut v = serde_json::json!({
            "format": "tidewater-oracle-trace",
            "version": self.version,
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
        });
        if self.version >= 2 {
            v["input"] = Value::Array(self.input.iter().map(Entry::to_json).collect());
        }
        if let Some(r) = &self.rules {
            v["rules"] = r.clone().into();
        }
        if let Some(m) = &self.multiplayer {
            v["launch_path"] = "multiplayer".into();
            v["connection"] = m.connection.clone().into();
            v["map"] = m.map.clone().into();
            v["start_seed"] = m.start_seed.into();
            v["humans"] = m.humans.into();
            if let Some(o) = m.opponents {
                v["opponents"] = o.into();
                v["starts"] = if m.fixed_order { "fixed" } else { "random" }.into();
            }
            if !m.fog_of_war {
                v["fog"] = "off".into();
            }
        }
        if let Some(f) = self.from {
            v["from"] = f.into();
        }
        if self.setup_only {
            v["setup_only"] = true.into();
        }
        if !self.extras.is_empty() {
            v["extras"] = self.extras.clone().into();
        }
        v.to_string()
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
    /// `turn` records of format version 2: the commands carried out in the turn, in order.
    pub commands: Vec<Entry>,
    /// The `hits` extra ("Opt-in extras"): the damage step's calls made by this record's work, in order.
    pub hits: Vec<Hit>,
    /// The `goals` extra per unit slot, for the units that carry it.
    pub goals: BTreeMap<u16, Goals>,
    /// The `aigoals` extra per unit slot, in [`AI_GOAL_FIELDS`] order (kind, tile x, tile y), for the units that carry
    /// it. Ours holds every unit's.
    pub ai_goals: BTreeMap<u16, [Cell; 3]>,
    /// The `vision` extra per unit slot: `hidden_from` and `detected_by` ([`VISION_FIELDS`] order), for the units that
    /// carry it. Ours holds every unit's.
    pub vision: BTreeMap<u16, [Cell; 2]>,
    /// The `game` extra: the game-wide values the record carries (`None` without the extra).
    pub game: Option<GameState>,
    /// The `aijobs` extra's training group counts: per player (0 to 15), the four [`AIJOB_FIELDS`] counts. The
    /// extra's other fields (`wanted`, `have`) are read by no comparison. Ours holds a count only for a computer
    /// player and [`Cell::Skip`] for the rest.
    pub aijobs: Option<Vec<[Cell; 4]>>,
    /// The `aigroups` extra: per player (0 to 15), the 14 [`AIGROUP_FIELDS`] counts. Ours holds a count only for a
    /// computer player and [`Cell::Skip`] for the rest.
    pub aigroups: Option<Vec<[Cell; 14]>>,
    /// The `airesearch` extra: per player 0 to 7, the [`AIRESEARCH_FIELDS`] values of a computer player. A player the
    /// trace doesn't list, or that isn't a computer player of ours, is all [`Cell::Skip`].
    pub airesearch: Option<Vec<[Cell; 15]>>,
    /// The `aiworkers` extra (trace format, "Computer worker jobs"); `None` without it.
    pub aiworkers: Option<AiWorkers>,
    /// The `fogmaps` extra ([`FOGMAP_FIELDS`] order), where the record carries it.
    pub fogmaps: Option<Vec<Cell>>,
    /// A `fog` record (`kind` = "fog"): the fog-of-war option was switched to this value in the cycle just run.
    pub fog_event: Option<bool>,
    /// The top-level `fog_of_war` of a `say` or `fog` record: the option after the event.
    pub fog_option: Option<i64>,
    /// A `save` record's save name (`kind` = "save"): the game wrote a saved game here.
    pub save: Option<String>,
    /// A `setup` record's `load`: the game was started by loading the saved game of this name.
    pub load: Option<String>,
    /// The `game` extra's `controllers` after the record's turn (empty without the extra). A turn's `other` command
    /// is a player leaving only if this shows the player as "nobody" afterwards.
    pub controllers: Vec<String>,
}

/// The `vision` extra's compared unit fields (`vision/fog.md`, "Unit visibility" and "Detection (phase 1)").
pub const VISION_FIELDS: [&str; 2] = ["hidden_from", "detected_by"];

/// The `game` extra's compared values, in report order (`game/save-load.md`; `vision/fog.md`; `game/victory.md`).
/// The extra also lists `results` and `controllers`, which are read and not compared.
pub const GAME_FIELDS: [&str; 4] = ["cheat_used", "fog_of_war", "fog_countdown", "victory_countdown"];

/// A record's `game` extra: the values in [`GAME_FIELDS`] order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameState(pub [Cell; 4]);

impl GameState {
    fn parse(g: &Value) -> GameState {
        GameState(GAME_FIELDS.map(|f| Cell::of(g.get(f))))
    }
}

/// The `aijobs` extra's compared fields: a computer player's footmen, archers, siege and knights counts
/// (`game/save-load.md` SL4 to SL6; `ai/training-research.md`, "Training buildings"), the extra's `count` rows in
/// this order. The extra's other fields (`wanted`, `have`) are read by no rule here and aren't compared.
pub const AIJOB_FIELDS: [&str; 4] = ["footmen", "archers", "siege", "knights"];

/// The `aijobs` extra of a record: per player, the [`AIJOB_FIELDS`] counts from its `count` rows (group, then player).
fn parse_aijobs(a: &Value) -> Vec<[Cell; 4]> {
    let rows = a.get("count").and_then(Value::as_array);
    (0..16)
        .map(|p| {
            std::array::from_fn(|g| {
                Cell::of(
                    rows.and_then(|r| r.get(g))
                        .and_then(Value::as_array)
                        .and_then(|r| r.get(p)),
                )
            })
        })
        .collect()
}

/// The `aigroups` extra's compared fields, as the report names them, and the trace's own names for them in the same
/// order. All 14 counts of `ai/training-research.md`, "Training buildings" (the trace's `flyers` are the flying
/// machines and its `subs` the submarines); `game/save-load.md` SL4 to SL6 and SL13 to SL15.
pub const AIGROUP_FIELDS: [&str; 14] = [
    "group_footmen",
    "group_archers",
    "group_siege",
    "group_knights",
    "group_mages",
    "group_demolition",
    "group_gryphons",
    "group_flying_machines",
    "group_transports",
    "group_destroyers",
    "group_battleships",
    "group_submarines",
    "group_workers",
    "group_tankers",
];
const AIGROUP_KEYS: [&str; 14] = [
    "footmen",
    "archers",
    "siege",
    "knights",
    "mages",
    "demolition",
    "gryphons",
    "flyers",
    "transports",
    "destroyers",
    "battleships",
    "subs",
    "workers",
    "tankers",
];

/// The `aigroups` extra of a record: per player, the 14 counts of its per-group rows.
fn parse_aigroups(a: &Value) -> Vec<[Cell; 14]> {
    (0..16)
        .map(|p| {
            std::array::from_fn(|g| Cell::of(a.get(AIGROUP_KEYS[g]).and_then(Value::as_array).and_then(|r| r.get(p))))
        })
        .collect()
}

/// The `airesearch` extra's compared fields (`ai/training-research.md`, "Research requests and done marks";
/// `ai/worker-ai.md`, "Done marks"; `game/save-load.md` SL9 to SL12): a computer player's research requests (codes,
/// ascending), its done marks (entry numbers, ascending), its build list up to the end mark, the build limit, the ten
/// upgrade levels and the spells-known word.
pub const AIRESEARCH_FIELDS: [&str; 15] = [
    "research_requests",
    "research_done",
    "build_list",
    "build_limit",
    "level_arrow",
    "level_weapon",
    "level_shield",
    "level_siege",
    "level_cannon",
    "level_armour",
    "level_rangers",
    "level_marksmanship",
    "level_longbow",
    "level_scouting",
    "spells_known",
];
const AIRESEARCH_LEVELS: [&str; 10] = [
    "arrow",
    "weapon",
    "shield",
    "siege",
    "cannon",
    "armour",
    "rangers",
    "marksmanship",
    "longbow",
    "scouting",
];
/// Our level line behind each of [`AIRESEARCH_LEVELS`] (`economy/tech-tree.md`, "Upgrade lines and their effects"): the
/// trace's `cannon` and `armour` are the ship lines 3 and 4.
const AIRESEARCH_LINES: [usize; 10] = [0, 1, 2, 6, 3, 4, 7, 10, 8, 9];

fn number_list(v: Option<&Value>) -> Cell {
    let items: Vec<String> = v
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|n| n.as_i64().map_or("?".to_string(), |n| n.to_string()))
        .collect();
    Cell::Text(format!("[{}]", items.join(",")))
}

/// The `airesearch` extra of a record: players 0 to 7, [`Cell::Skip`] for those it doesn't list.
fn parse_airesearch(a: &Value) -> Vec<[Cell; 15]> {
    (0..8)
        .map(|p| {
            let Some(r) = a.get(p.to_string()) else {
                return [(); 15].map(|_| Cell::Skip(""));
            };
            let level = |k: &str| Cell::of(r.get("levels").and_then(|l| l.get(k)));
            let mut cells = vec![
                number_list(r.get("requests")),
                number_list(r.get("done")),
                number_list(r.get("list")),
                Cell::of(r.get("limit")),
            ];
            cells.extend(AIRESEARCH_LEVELS.map(level));
            cells.push(Cell::of(r.get("spells")));
            cells.try_into().expect("fifteen cells")
        })
        .collect()
}

/// The `aiworkers` extra's per-player compared fields (trace format, "Computer worker jobs"): the three job counts as
/// 16-bit numbers, the hall cooldown and the buildings in progress (`"0x3a=2,0x40=1"`: type and count, ascending, a
/// missing type is 0).
pub const AIWORKER_FIELDS: [&str; 5] = [
    "aiw_miners",
    "aiw_lumberjacks",
    "aiw_repairers",
    "aiw_hall_cooldown",
    "aiw_building",
];
/// The `aiworkers` extra's per-unit compared fields: the job bits and the harvest mode.
pub const AIWORKER_UNIT_FIELDS: [&str; 2] = ["aiw_job", "aiw_mode"];

/// A record's `aiworkers` extra: per player (0 to 15) the [`AIWORKER_FIELDS`], and per unit slot that has a job the
/// [`AIWORKER_UNIT_FIELDS`] (the trace's `[job, mode]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiWorkers {
    pub players: Vec<[Cell; 5]>,
    pub jobs: BTreeMap<u16, [Cell; 2]>,
}

impl AiWorkers {
    /// The `building` object as per-player texts: the types with a nonzero count, ascending by type number.
    fn building_texts(counts: impl Iterator<Item = (usize, u8, u64)>) -> Vec<Cell> {
        let mut per: Vec<BTreeMap<u8, u64>> = vec![BTreeMap::new(); 16];
        for (p, t, n) in counts {
            if n != 0 && p < 16 {
                per[p].insert(t, n);
            }
        }
        per.into_iter()
            .map(|m| {
                Cell::Text(
                    m.iter()
                        .map(|(t, n)| format!("{t:#04x}={n}"))
                        .collect::<Vec<_>>()
                        .join(","),
                )
            })
            .collect()
    }

    fn parse(a: &Value) -> AiWorkers {
        let list = |k: &str, p: usize| Cell::of(a.get(k).and_then(Value::as_array).and_then(|r| r.get(p)));
        // Keys are "P:0xTT", the player in decimal and the type in two lower-case hex digits.
        let counts = a
            .get("building")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                let (p, t) = k.split_once(':')?;
                let t = u8::from_str_radix(t.strip_prefix("0x")?, 16).ok()?;
                Some((p.parse::<usize>().ok()?, t, v.as_u64()?))
            });
        let building = AiWorkers::building_texts(counts);
        let players = (0..16)
            .map(|p| {
                [
                    list("miners", p),
                    list("lumberjacks", p),
                    list("repairers", p),
                    list("hall_cooldown", p),
                    building[p].clone(),
                ]
            })
            .collect();
        let jobs = a
            .get("jobs")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| {
                let slot = k.parse::<u16>().ok()?;
                let v = v.as_array()?;
                Some((slot, [Cell::of(v.first()), Cell::of(v.get(1))]))
            })
            .collect();
        AiWorkers { players, jobs }
    }
}

/// The `fogmaps` extra's compared values, in report order (`vision/fog.md`, "The fog maps" and "The seen map"; vector
/// F45): the "seen" bytes of each class of tile (by the explored map: unexplored, explored, edge) as `byte:count`
/// pairs, the counts of clear, fogged and edge tiles of the visible map and of unexplored, explored and edge tiles of
/// the explored map, and the local player's units on explored tiles and in all.
pub const FOGMAP_FIELDS: [&str; 7] = [
    "seen_unexplored",
    "seen_explored",
    "seen_edge",
    "visible_map",
    "explored_map",
    "local_units_on_explored",
    "local_units",
];

/// The `aigoals` extra's compared fields (`ai_goal`: `kind` and `tile`; `extra` is read by no rule in the specs, so it
/// isn't compared).
pub const AI_GOAL_FIELDS: [&str; 3] = ["ai_goal_kind", "ai_goal_x", "ai_goal_y"];

/// A unit's `ai_goal` (kind, tile x, tile y), or `None` when it has none.
fn parse_ai_goal(u: &Value) -> Option<[Cell; 3]> {
    let g = u.get("ai_goal")?;
    let tile = g.get("tile").and_then(Value::as_array);
    Some([
        Cell::of(g.get("kind")),
        Cell::of(tile.and_then(|t| t.first())),
        Cell::of(tile.and_then(|t| t.get(1))),
    ])
}

/// One `hits` entry: a call of the damage step (trace format, "Hit entries"). Read, not compared: no spec defines
/// the comparison yet.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Hit {
    /// `melee`, `missile`, `splash`, `demolish`, `exorcism` or `other`.
    pub by: String,
    pub attacker: Option<u16>,
    pub target: Option<u16>,
    /// The amount given to the damage step, before its own checks.
    pub damage: i64,
    /// The target's hit points before the call (only with a target).
    pub hp: Option<i64>,
    /// `missile` and `splash` entries: the missile's effect slot.
    pub effect: Option<u16>,
}

impl Hit {
    fn parse(v: &Value) -> Result<Hit> {
        let slot = |k: &str| v.get(k).and_then(Value::as_u64).map(|s| s as u16);
        Ok(Hit {
            by: v.get("by").and_then(Value::as_str).context("missing `by`")?.to_string(),
            attacker: slot("attacker"),
            target: slot("target"),
            damage: int(v, "damage")?,
            hp: v.get("hp").and_then(Value::as_i64),
            effect: slot("effect"),
        })
    }

    #[cfg(test)]
    fn to_json(&self) -> Value {
        let mut o = serde_json::json!({
            "by": self.by,
            "attacker": self.attacker,
            "target": self.target,
            "damage": self.damage,
        });
        if let Some(hp) = self.hp {
            o["hp"] = hp.into();
        }
        if matches!(self.by.as_str(), "missile" | "splash") {
            o["effect"] = self.effect.into();
        }
        o
    }
}

/// A unit's `goals` extra (trace format, "Unit fields"). Read; only a building's goal x is compared, because the
/// stored values mean something only where a spec defines them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Goals {
    /// `goal`: the stored goal tile, as stored.
    pub goal: Option<[i64; 2]>,
    /// `pending`: the pending order number, [`Cell::Null`] for none.
    pub pending: Option<Cell>,
    /// `transport_dest`: a transport's destination tile, as stored.
    pub transport_dest: Option<[i64; 2]>,
}

impl Goals {
    /// The unit's `goals` fields, or `None` when it has none.
    fn parse(u: &Value) -> Option<Goals> {
        let pair = |k: &str| {
            let a = u.get(k)?.as_array()?;
            Some([a.first()?.as_i64()?, a.get(1)?.as_i64()?])
        };
        let g = Goals {
            goal: pair("goal"),
            pending: u.get("pending").map(|p| Cell::of(Some(p))),
            transport_dest: pair("transport_dest"),
        };
        (g != Goals::default()).then_some(g)
    }
}

/// The `fogmaps` extra of a record: [`FOGMAP_FIELDS`] order.
fn parse_fogmaps(f: &Value) -> Vec<Cell> {
    let hist = |class: &str| {
        let mut pairs: Vec<(i64, i64)> = f
            .pointer(&format!("/seen/{class}"))
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(k, c)| Some((k.parse().ok()?, c.as_i64()?)))
            .collect();
        pairs.sort();
        Cell::Text(histogram(&pairs))
    };
    let counts = |map: &str, keys: [&str; 3]| {
        let n = |k: &str| f.pointer(&format!("/{map}/{k}")).and_then(Value::as_i64).unwrap_or(0);
        Cell::Text(format!("{}/{}/{}", n(keys[0]), n(keys[1]), n(keys[2])))
    };
    let units = f.get("local_units_on_explored").and_then(Value::as_array);
    vec![
        hist("unexplored"),
        hist("explored"),
        hist("edge"),
        counts("visible", ["clear", "fogged", "edge"]),
        counts("explored", ["unexplored", "explored", "edge"]),
        Cell::of(units.and_then(|a| a.first())),
        Cell::of(units.and_then(|a| a.get(1))),
    ]
}

fn histogram(pairs: &[(i64, i64)]) -> String {
    if pairs.is_empty() {
        return "none".into();
    }
    pairs
        .iter()
        .map(|(k, c)| format!("{k}:{c}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Our world's `fogmaps` extra values ([`FOGMAP_FIELDS`] order).
pub fn fog_maps(w: &World) -> Vec<Cell> {
    use tidewater_sim::vision::{Map, quarter};
    let v = &w.vision;
    let (explored, visible, seen) = (v.flags(Map::Explored), v.flags(Map::Visible), v.seen_flags());
    let mut hists: [BTreeMap<i64, i64>; 3] = Default::default();
    let (mut vis, mut exp) = ([0i64; 3], [0i64; 3]);
    for ((&e, &vi), &s) in explored.iter().zip(visible).zip(seen) {
        let class = match e {
            quarter::ALL => 0,
            0 => 1,
            _ => 2,
        };
        *hists[class].entry(s as i64).or_default() += 1;
        exp[class] += 1;
        vis[match vi {
            0 => 0,
            quarter::ALL => 1,
            _ => 2,
        }] += 1;
    }
    let hist = |h: &BTreeMap<i64, i64>| Cell::Text(histogram(&h.iter().map(|(&k, &c)| (k, c)).collect::<Vec<_>>()));
    let local = v.local;
    let (mut on, mut total) = (0i64, 0i64);
    for u in w.units.iter().filter(|u| u.owner == local && !u.dying && !u.removing) {
        total += 1;
        on += i64::from(!v.unexplored(u.tile.0, u.tile.1));
    }
    vec![
        hist(&hists[0]),
        hist(&hists[1]),
        hist(&hists[2]),
        Cell::Text(format!("{}/{}/{}", vis[0], vis[1], vis[2])),
        // The explored map's classes: unexplored, explored, edge.
        Cell::Text(format!("{}/{}/{}", exp[0], exp[1], exp[2])),
        Cell::Int(on),
        Cell::Int(total),
    ]
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
        let mut goals = BTreeMap::new();
        let mut ai_goals = BTreeMap::new();
        let mut vision = BTreeMap::new();
        for u in v.get("units").and_then(Value::as_array).into_iter().flatten() {
            let slot = int(u, "slot").with_context(|| format!("record n={n}: a unit"))? as u16;
            if let Some(g) = Goals::parse(u) {
                goals.insert(slot, g);
            }
            if let Some(g) = parse_ai_goal(u) {
                ai_goals.insert(slot, g);
            }
            if u.get("hidden_from").is_some() || u.get("detected_by").is_some() {
                vision.insert(slot, VISION_FIELDS.map(|f| Cell::of(u.get(f))));
            }
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
            counter: v.get("counter").and_then(Value::as_u64),
            // A `save`, `say`, `fog` or `peer` record has no generator state.
            rng: if matches!(kind.as_str(), "save" | "say" | "fog" | "peer") {
                0
            } else {
                int(&v, "rng")? as u32
            },
            resources: RESOURCES.map(list),
            units,
            effects,
            commands: v
                .get("commands")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|c| Entry::parse(c).with_context(|| format!("record n={n}: a command")))
                .collect::<Result<_>>()?,
            hits: v
                .get("hits")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|h| Hit::parse(h).with_context(|| format!("record n={n}: a hit entry")))
                .collect::<Result<_>>()?,
            goals,
            ai_goals,
            vision,
            game: v.get("game").map(GameState::parse),
            aijobs: v.get("aijobs").map(parse_aijobs),
            aigroups: v.get("aigroups").map(parse_aigroups),
            airesearch: v.get("airesearch").map(parse_airesearch),
            aiworkers: v.get("aiworkers").map(AiWorkers::parse),
            fogmaps: v.get("fogmaps").map(parse_fogmaps),
            fog_event: (kind == "fog")
                .then(|| v.get("value").and_then(Value::as_str).map(|x| x == "on"))
                .flatten(),
            fog_option: v.get("fog_of_war").and_then(Value::as_i64),
            save: (kind == "save")
                .then(|| v.get("name").and_then(Value::as_str).map(str::to_string))
                .flatten(),
            load: v.get("load").and_then(Value::as_str).map(str::to_string),
            controllers: v.get("game").map(|g| strings(g, "controllers")).unwrap_or_default(),
            kind,
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
                if let Some(g) = self.goals.get(&slot) {
                    if let Some(p) = g.goal {
                        u.insert("goal".into(), p.to_vec().into());
                    }
                    if let Some(p) = &g.pending {
                        u.insert("pending".into(), p.json().unwrap_or(Value::Null));
                    }
                    if let Some(p) = g.transport_dest {
                        u.insert("transport_dest".into(), p.to_vec().into());
                    }
                }
                if let Some([k, x, y]) = self.ai_goals.get(&slot) {
                    let j = |c: &Cell| c.json().unwrap_or(Value::Null);
                    u.insert(
                        "ai_goal".into(),
                        serde_json::json!({"kind": j(k), "tile": [j(x), j(y)], "extra": 0}),
                    );
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
        if !self.commands.is_empty() {
            o.insert(
                "commands".into(),
                Value::Array(self.commands.iter().map(Entry::to_json).collect()),
            );
        }
        if !self.hits.is_empty() {
            o.insert(
                "hits".into(),
                Value::Array(self.hits.iter().map(Hit::to_json).collect()),
            );
        }
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
    /// The `aigoals` extra: the computer player's goal kind and tile.
    AiGoals,
    /// The `vision` extra: each unit's hidden-from and detected-by sets.
    Vision,
    /// The `game` extra: the cheat mark, the fog option and the two countdowns.
    Game,
    /// The `fogmaps` extra: summaries of the "seen", visible and explored maps.
    FogMaps,
    /// The `aijobs` extra: the computer players' training group counts.
    AiJobs,
    /// The `aigroups` extra: all 14 group counts.
    AiGroups,
    /// The `airesearch` extra: research requests, done marks and the state they read.
    AiResearch,
    /// The `aiworkers` extra: the computer workers' job counts and jobs.
    AiWorkers,
}

impl Category {
    pub const ALL: [Category; 18] = [
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
        Category::AiGoals,
        Category::Vision,
        Category::Game,
        Category::FogMaps,
        Category::AiJobs,
        Category::AiGroups,
        Category::AiResearch,
        Category::AiWorkers,
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
            Category::AiGoals => "ai-goals",
            Category::Vision => "vision",
            Category::Game => "game",
            Category::FogMaps => "fogmaps",
            Category::AiJobs => "aijobs",
            Category::AiGroups => "aigroups",
            Category::AiResearch => "airesearch",
            Category::AiWorkers => "aiworkers",
        }
    }
}

fn category(scope: Scope, field: &str) -> Category {
    match scope {
        Scope::Global if GAME_FIELDS.contains(&field) => Category::Game,
        Scope::Global if FOGMAP_FIELDS.contains(&field) => Category::FogMaps,
        Scope::Global => Category::Rng,
        Scope::Player(_) if AIJOB_FIELDS.contains(&field) => Category::AiJobs,
        Scope::Player(_) if AIGROUP_FIELDS.contains(&field) => Category::AiGroups,
        Scope::Player(_) if AIRESEARCH_FIELDS.contains(&field) => Category::AiResearch,
        Scope::Player(_) if AIWORKER_FIELDS.contains(&field) => Category::AiWorkers,
        Scope::Player(_) => Category::Resources,
        Scope::Effect(_) => Category::Effects,
        Scope::Unit(_) => match field {
            "used" | "serial" | "type" | "owner" => Category::Slots,
            "x" | "y" | "tx" | "ty" => Category::Positions,
            "order" | "target" => Category::Orders,
            "facing" => Category::Facing,
            "anim" | "frame" | "wait" => Category::Anim,
            f if AI_GOAL_FIELDS.contains(&f) => Category::AiGoals,
            f if VISION_FIELDS.contains(&f) => Category::Vision,
            f if AIWORKER_UNIT_FIELDS.contains(&f) => Category::AiWorkers,
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
    // The `goals` extra, a building's or a transport's x only (trace format, `goal` row, rounds 97 and 98): a
    // building's construction pass and repair counter, a transport's stored goal x as repair changes it. Every other
    // `goal` value is read, not compared (no spec defines it).
    // A transport's x is defined only for one a worker is repairing in this record (its own movement changes the goal
    // too, and no spec defines that).
    let repaired: Vec<i64> = theirs
        .units
        .values()
        .filter(|c| c.get(UNIT_ORDER) == Some(&Cell::Int(REPAIR_ORDER)))
        .filter_map(|c| match c.get(UNIT_TARGET) {
            Some(Cell::Int(t)) => Some(*t),
            _ => None,
        })
        .collect();
    for (&slot, tg) in &theirs.goals {
        let (Some(t), Some(o)) = (tg.goal, ours.goals.get(&slot).and_then(|g| g.goal)) else {
            continue;
        };
        let Some(unit) = theirs.units.get(&slot) else { continue };
        if !ours.units.contains_key(&slot) {
            continue;
        }
        let transport = matches!(unit.get(UNIT_TYPE), Some(Cell::Int(k)) if is_transport_type(*k as u8));
        if transport && !repaired.contains(&(i64::from(slot))) {
            continue;
        }
        if t[0] != o[0] && filter.allows(Scope::Unit(slot), "goal_x") {
            out.push(Diff {
                scope: Scope::Unit(slot),
                field: "goal_x",
                ours: Cell::Int(o[0]),
                theirs: Cell::Int(t[0]),
            });
        }
    }
    // The `aigoals` extra: compared for the units both sides hold where the trace carries it.
    for (&slot, tg) in &theirs.ai_goals {
        let Some(og) = ours.ai_goals.get(&slot) else { continue };
        if !theirs.units.contains_key(&slot) || !ours.units.contains_key(&slot) {
            continue;
        }
        for ((&f, tc), oc) in AI_GOAL_FIELDS.iter().zip(tg).zip(og) {
            if oc.matches(tc) == Some(false) && filter.allows(Scope::Unit(slot), f) {
                out.push(Diff {
                    scope: Scope::Unit(slot),
                    field: f,
                    ours: oc.clone(),
                    theirs: tc.clone(),
                });
            }
        }
    }
    // The `vision` extra: the units' sets, for the units both sides hold where the trace carries it.
    for (&slot, tv) in &theirs.vision {
        let Some(ov) = ours.vision.get(&slot) else { continue };
        if !theirs.units.contains_key(&slot) || !ours.units.contains_key(&slot) {
            continue;
        }
        for ((&f, tc), oc) in VISION_FIELDS.iter().zip(tv).zip(ov) {
            // spec gap: a map unit's detected-by set in the `setup` record is empty in the trace and everyone's here
            // until the first detection phase (`vision/fog.md`, "A unit created during a cycle" covers only units made
            // in a cycle); the set is compared from the first start-up cycle on.
            if theirs.kind == "setup" && f == "detected_by" {
                continue;
            }
            if oc.matches(tc) == Some(false) && filter.allows(Scope::Unit(slot), f) {
                out.push(Diff {
                    scope: Scope::Unit(slot),
                    field: f,
                    ours: oc.clone(),
                    theirs: tc.clone(),
                });
            }
        }
    }
    // The `game` extra, where the trace carries it.
    if let (Some(tg), Some(og)) = (&theirs.game, &ours.game) {
        for ((&f, tc), oc) in GAME_FIELDS.iter().zip(&tg.0).zip(&og.0) {
            // spec gap: the fog countdown isn't compared in a `setup` record (before the game start sets it, it holds
            // the last game's value, so 99 in a load's setup record) nor in the first start-up record (the trace shows
            // 0 where our refresh in the first start-up cycle has already reloaded it); `game/save-load.md` and
            // `core/timing-and-rng.md` give the refresh cycle, not the stored value.
            if f == "fog_countdown" && (theirs.kind == "setup" || (theirs.kind == "startup" && theirs.n == 1)) {
                continue;
            }
            if oc.matches(tc) == Some(false) && filter.allows(Scope::Global, f) {
                out.push(Diff {
                    scope: Scope::Global,
                    field: f,
                    ours: oc.clone(),
                    theirs: tc.clone(),
                });
            }
        }
    }
    // The `aijobs` extra, where the trace carries it: a computer player's four training counts.
    if let (Some(tj), Some(oj)) = (&theirs.aijobs, &ours.aijobs) {
        for (p, (tp, op)) in tj.iter().zip(oj).enumerate() {
            for ((&f, tc), oc) in AIJOB_FIELDS.iter().zip(tp).zip(op) {
                let scope = Scope::Player(p as u8);
                if oc.matches(tc) == Some(false) && filter.allows(scope, f) {
                    out.push(Diff {
                        scope,
                        field: f,
                        ours: oc.clone(),
                        theirs: tc.clone(),
                    });
                }
            }
        }
    }
    // The `aigroups` extra: all 14 group counts of a computer player.
    if let (Some(tj), Some(oj)) = (&theirs.aigroups, &ours.aigroups) {
        for (p, (tp, op)) in tj.iter().zip(oj).enumerate() {
            for ((&f, tc), oc) in AIGROUP_FIELDS.iter().zip(tp).zip(op) {
                let scope = Scope::Player(p as u8);
                if oc.matches(tc) == Some(false) && filter.allows(scope, f) {
                    out.push(Diff {
                        scope,
                        field: f,
                        ours: oc.clone(),
                        theirs: tc.clone(),
                    });
                }
            }
        }
    }
    // The `airesearch` extra: a computer player's research requests, done marks and what they read.
    if let (Some(tj), Some(oj)) = (&theirs.airesearch, &ours.airesearch) {
        for (p, (tp, op)) in tj.iter().zip(oj).enumerate() {
            for ((&f, tc), oc) in AIRESEARCH_FIELDS.iter().zip(tp).zip(op) {
                let scope = Scope::Player(p as u8);
                if oc.matches(tc) == Some(false) && filter.allows(scope, f) {
                    out.push(Diff {
                        scope,
                        field: f,
                        ours: oc.clone(),
                        theirs: tc.clone(),
                    });
                }
            }
        }
    }
    // The `aiworkers` extra: the counts, the hall cooldown and the buildings in progress exactly, as 16-bit numbers
    // (trace format, "Comparing extras"), and the jobs only for slots that are in the same record's `units`.
    if let (Some(tj), Some(oj)) = (&theirs.aiworkers, &ours.aiworkers) {
        for (p, (tp, op)) in tj.players.iter().zip(&oj.players).enumerate() {
            for ((&f, tc), oc) in AIWORKER_FIELDS.iter().zip(tp).zip(op) {
                let scope = Scope::Player(p as u8);
                if oc.matches(tc) == Some(false) && filter.allows(scope, f) {
                    out.push(Diff {
                        scope,
                        field: f,
                        ours: oc.clone(),
                        theirs: tc.clone(),
                    });
                }
            }
        }
        let slots: BTreeSet<u16> = tj.jobs.keys().chain(oj.jobs.keys()).copied().collect();
        for slot in slots {
            if !theirs.units.contains_key(&slot) {
                continue;
            }
            let none = [Cell::Null, Cell::Null];
            let (tc, oc) = (tj.jobs.get(&slot).unwrap_or(&none), oj.jobs.get(&slot).unwrap_or(&none));
            for ((&f, tc), oc) in AIWORKER_UNIT_FIELDS.iter().zip(tc).zip(oc) {
                let scope = Scope::Unit(slot);
                if oc.matches(tc) == Some(false) && filter.allows(scope, f) {
                    out.push(Diff {
                        scope,
                        field: f,
                        ours: oc.clone(),
                        theirs: tc.clone(),
                    });
                }
            }
        }
    }
    // The `fogmaps` extra, where the trace carries it.
    if let (Some(tf), Some(of)) = (&theirs.fogmaps, &ours.fogmaps) {
        for ((&f, tc), oc) in FOGMAP_FIELDS.iter().zip(tf).zip(of) {
            if oc.matches(tc) == Some(false) && filter.allows(Scope::Global, f) {
                out.push(Diff {
                    scope: Scope::Global,
                    field: f,
                    ours: oc.clone(),
                    theirs: tc.clone(),
                });
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
    // Orders spec, "Orders given when a unit is created": a building reads as order 2 until its first order (14,
    // 33 or 59) takes over at its first step done. We hold 33 and 59 as `Idle`; 37 is a job that has taken over.
    if u.building && u.pending.is_some() && matches!(u.order, Order::Idle) {
        return Cell::Int(2);
    }
    let n = match u.order {
        Order::Idle if u.building => {
            return Cell::Int(if u.job.is_some_and(|j| j.running) {
                37
            } else if u.kind == tidewater_sim::rescue::CIRCLE_OF_POWER {
                59
            } else {
                33
            });
        }
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

/// The slot of the unit the order aims at, as far as our orders name one. A command sets the unit's target at once,
/// so a pending order's target unit is shown while it waits to take over. A dying unit shows the target it had when
/// it died, and a killer whose order is still the one whose target it forgot shows none (damage spec, "Applying
/// damage"). A dead unit (dying, remains or in its remove order) keeps reporting its target's slot after that slot is
/// released; a live unit's target is cleared then (trace-format spec, `target`, round-44 errata).
fn target_cell(w: &World, u: &Unit) -> Cell {
    if u.dying {
        return Cell::opt(u.dying_target);
    }
    // A command's target replaces the old one at once, even with none (`xorc01-death-knight`, n = 82: a move given to
    // a caster that was in order 54 on a unit shows no target).
    Cell::opt(u.recorded_target().and_then(|id| w.unit(id)).map(|t| t.slot))
}

fn unit_cells(w: &World, u: &Unit) -> Vec<Cell> {
    UNIT_FIELDS
        .iter()
        .map(|&f| match f {
            "serial" => Cell::Int(u.serial.0 as i64),
            "type" => Cell::Int(u.kind as i64),
            "owner" => Cell::Int(u.owner as i64),
            "x" => Cell::Int(u.pos.0 as i64),
            "y" => Cell::Int(u.pos.1 as i64),
            "tx" => Cell::Int(u.tile.0 as i64),
            "ty" => Cell::Int(u.tile.1 as i64),
            "hp" => Cell::Int(u.hp as i64),
            "order" => order_cell(u),
            // Unit-slots spec, "Creating a unit", step 3: facing 0, then drawn for mobile units only.
            "facing" => Cell::Int(u.facing as i64),
            "anim" => Cell::Int(u.script.anim as i64),
            "frame" => Cell::Int(u.script.frame as i64),
            "wait" => Cell::Int(u.script.wait as i64),
            "mana" => Cell::Int(u.mana as i64),
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
                // Trace-format spec (round-56 errata): an effect's shooter is never cleared, even once its slot is
                // released.
                Cell::opt(m.shooter),
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
        commands: Vec::new(),
        hits: Vec::new(),
        goals: w
            .units
            .iter()
            .filter(|u| u.building || is_transport_type(u.kind))
            .map(|u| {
                // Trace format, `goal` row: a building's x is its construction pass and repair counter (round 97), a
                // transport's x the stored goal a repair counts in (round 98, RP8).
                let g = Goals {
                    goal: Some([i64::from(u.goal_x as u16), 0]),
                    ..Goals::default()
                };
                (u.slot, g)
            })
            .collect(),
        ai_goals: w.units.iter().map(|u| (u.slot, ai_goal_cells(u))).collect(),
        vision: w.units.iter().map(|u| (u.slot, vision_cells(u))).collect(),
        game: Some(game_state(w)),
        aijobs: Some(aijob_cells(w)),
        aigroups: Some(aigroup_cells(w)),
        airesearch: Some(airesearch_cells(w)),
        aiworkers: Some(aiworker_cells(w)),
        fogmaps: None,
        fog_event: None,
        fog_option: None,
        save: None,
        load: None,
        controllers: Vec::new(),
    }
}

/// The town hall types (0x4A human, 0x4B orc), the hall a worker's no-depot step builds.
fn is_town_hall(kind: u8) -> bool {
    matches!(kind, 0x4a | 0x4b)
}

/// The transport types (0x1C and 0x1D) whose `goal` x is compared.
fn is_transport_type(kind: u8) -> bool {
    matches!(kind, 0x1c | 0x1d)
}

/// A unit's `hidden_from` and `detected_by` sets as the `vision` extra shows them.
fn vision_cells(u: &Unit) -> [Cell; 2] {
    [Cell::Int(u.hidden_from as i64), Cell::Int(u.detected_by as i64)]
}

/// Our world's `aijobs` extra values: a computer player's counts (with the surplus a load left), the rest skipped.
fn aijob_cells(w: &World) -> Vec<[Cell; 4]> {
    (0..16)
        .map(|p| {
            if w.ai_player(p as u8) {
                let g = w.ai_groups_with_surplus(p);
                [g.footmen, g.archers, g.siege, g.knights].map(|n| Cell::Int(n as i64))
            } else {
                [(); 4].map(|_| Cell::Skip(""))
            }
        })
        .collect()
}

/// Our world's `aigroups` extra values: a computer player's 14 counts (the eleven doubled groups with the surplus a load
/// left, the worker and tanker counts as counted), the rest skipped.
fn aigroup_cells(w: &World) -> Vec<[Cell; 14]> {
    (0..16)
        .map(|p| {
            if w.ai_player(p as u8) {
                let g = w.ai_groups_with_surplus(p);
                let n = |n: u32| Cell::Int(n as i64);
                [
                    n(g.footmen),
                    n(g.archers),
                    n(g.siege),
                    n(g.knights),
                    n(g.mages),
                    // spec gap: `ai/training-research.md` has no demolition group count (the trace shows 0 in every
                    // record of the traces so far), so it isn't compared.
                    Cell::Skip("no demolition group in the spec"),
                    n(g.gryphons),
                    n(g.flying_machines),
                    n(g.transports),
                    n(g.destroyers),
                    n(g.battleships),
                    n(g.submarines),
                    n(g.workers),
                    n(g.tankers),
                ]
            } else {
                [(); 14].map(|_| Cell::Skip(""))
            }
        })
        .collect()
}

/// Our world's `airesearch` extra values for players 0 to 7: a computer player's requests, done marks, build list and
/// limit, upgrade levels and spells, the rest skipped.
fn airesearch_cells(w: &World) -> Vec<[Cell; 15]> {
    use std::fmt::Write as _;
    (0..8)
        .map(|p| {
            if !w.ai_player(p as u8) {
                return [(); 15].map(|_| Cell::Skip(""));
            }
            let ai = &w.ai[p];
            let list = |it: Vec<u32>| {
                let mut t = String::from("[");
                for (n, v) in it.iter().enumerate() {
                    let _ = write!(t, "{}{v}", if n > 0 { "," } else { "" });
                }
                t.push(']');
                Cell::Text(t)
            };
            let build: Vec<u32> = w
                .build_list_of(p)
                .into_iter()
                .take_while(|&e| e != 0xff)
                .map(u32::from)
                .collect();
            let done: Vec<u32> = (0..64).filter(|n| ai.done & (1 << n) != 0).collect();
            let requests: Vec<u32> = (0x80..=0x9au32).filter(|&c| ai.requested(c as u8)).collect();
            let mut cells = vec![
                list(requests),
                list(done),
                list(build),
                // The limit is slot 0x22, compared from the first record: a new game's openings run before the map's
                // units are placed (`ai/ai-scripts.md`, "Game start", AS-V1 to AS-V3) and a load keeps the saved state.
                Cell::Int(ai.slots[tidewater_sim::ai::BUILD_LIMIT] as i64),
            ];
            cells.extend(AIRESEARCH_LINES.map(|l| Cell::Int(w.players[p].levels[l] as i64)));
            cells.push(Cell::Int(w.players[p].spells_known as i64));
            cells.try_into().expect("fifteen cells")
        })
        .collect()
}

/// Our world's `aiworkers` extra values (trace format, "Computer worker jobs"). The job bits are 1 gold, 2 lumber,
/// 8 a farm the worker's own steps started, 16 a build-list entry or a no-depot town hall, 32 repair; the mode is the worker's
/// gather mode, 128 gold and 64 lumber. A worker with no job isn't listed.
fn aiworker_cells(w: &World) -> AiWorkers {
    let count = |p: usize, f: fn(&tidewater_sim::ai::AiPlayer) -> i64| Cell::Int(w.ai.get(p).map_or(0, f));
    let building = AiWorkers::building_texts(w.ai.iter().enumerate().flat_map(|(p, a)| {
        a.in_progress
            .iter()
            .enumerate()
            .map(move |(t, &n)| (p, t as u8, u64::from(n)))
    }));
    let players = (0..16)
        .map(|p| {
            [
                count(p, |a| i64::from(a.miners)),
                count(p, |a| i64::from(a.lumberjacks)),
                count(p, |a| i64::from(a.repairers)),
                count(p, |a| i64::from(a.hall_cooldown)),
                building[p].clone(),
            ]
        })
        .collect();
    let jobs = w
        .units
        .iter()
        .filter(|u| u.owner < 8)
        .filter_map(|u| {
            let bits = job_bits(u.ai_job)?;
            let mode = match u.gather {
                tidewater_sim::Gather::None => 0,
                tidewater_sim::Gather::Gold => 128,
                tidewater_sim::Gather::Lumber => 64,
            };
            Some((u.slot, [Cell::Int(bits), Cell::Int(mode)]))
        })
        .collect();
    AiWorkers { players, jobs }
}

/// The trace's job bit for a worker's job (trace format, "Computer worker jobs"), `None` for no job. Bit 8 is a farm
/// the worker's own steps started; a town hall its no-depot step started is a build-list job, bit 16, like a list
/// entry (round 98, W2).
fn job_bits(job: AiJob) -> Option<i64> {
    Some(match job {
        AiJob::None => return None,
        AiJob::Gold => 1,
        AiJob::Lumber => 2,
        AiJob::Build { kind, .. } if is_town_hall(kind) => 16,
        AiJob::Build { .. } => 8,
        AiJob::List { .. } => 16,
        AiJob::Repair => 32,
    })
}

/// Our world's `game` extra values.
fn game_state(w: &World) -> GameState {
    GameState([
        Cell::Int(w.cheats.used as i64),
        Cell::Int(w.rules.fog_of_war as i64),
        // The trace shows 0 in the cycle of a refresh, where ours has just reloaded it.
        Cell::Int(if w.fog_countdown() == w.refresh_period() {
            0
        } else {
            w.fog_countdown() as i64
        }),
        Cell::Int(w.victory.countdown as i64),
    ])
}

/// A unit's goal kind and tile as the `aigoals` extra shows them (`ai/attack-ai.md`, "Unit goals"): a unit with no
/// goal tile shows (0, 0).
fn ai_goal_cells(u: &Unit) -> [Cell; 3] {
    let (x, y) = u.goal.tile.unwrap_or((0, 0));
    [Cell::Int(u.goal.kind as i64), Cell::Int(x as i64), Cell::Int(y as i64)]
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
    /// The header's `input`, to check each entry's turn against T(i) (trace format, "When a command runs").
    pub input: Vec<Entry>,
    /// The turn length in cycles of a multiplayer game whose command latency is known (Direct Connect, 12 at speed 8,
    /// `net/lockstep.md`); `None` for a single-player game.
    pub turn_length: Option<u64>,
    /// A multiplayer launch (the header has the multiplayer fields): the second human's commands are checked
    /// ("The second human"), against the header's `local_player` and `controllers`.
    pub multiplayer: bool,
    pub local_player: u8,
    pub controllers: Vec<String>,
    /// The header's `from`: a windowed trace, compared from this `n` on ("Windowed traces").
    pub from: Option<u64>,
    /// The header's `setup_only`: compare the setup record only ("Setup-only traces").
    pub setup_only: bool,
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
    /// Commands from `turn` records applied to our world.
    pub replayed: usize,
    /// The input checked against the turns that ran it.
    pub timing: replay::Timing,
    /// The last `n` read, compared or not (a windowed trace's gap turns count).
    pub reached: u64,
    /// A windowed trace's F (the header's `from`).
    pub from: Option<u64>,
    /// A windowed trace: command turns before F, replayed without comparing.
    pub gap_turns: usize,
    /// Saved games loaded (`setup` records with `load`).
    pub loads: usize,
    /// A windowed trace: records compared from F on.
    pub window_records: usize,
    /// A setup-only trace.
    pub setup_only: bool,
    /// Extras read and not compared: `hits` entries, and records with units carrying `goals` fields.
    pub hits: usize,
    pub goal_records: usize,
}

/// Context lines for a record's differences: draw counts for an rng difference, and both sides of each unit or
/// effect involved.
fn context(theirs: &Record, ours: &Record, diffs: &[Diff], prev: Option<(u32, u32)>) -> Vec<String> {
    let mut out = Vec::new();
    let rng = diffs.iter().any(|d| d.scope == Scope::Global && d.field == "rng");
    if rng && prev.is_none() {
        // Trace format, "Windowed traces": the gap's draws aren't counted anywhere.
        out.push("draws since the previous record: not known across the window's gap".to_string());
    }
    if let Some(prev) = prev.filter(|_| rng) {
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

/// Steps `w` to the end of cycle `to` without comparing: normal cycles, the first one ending the start-up cycles if
/// they haven't been ended yet (timing-and-rng spec, "Scheduler", rule 1).
fn step_to(w: &mut World, at: &mut u64, start_up: &mut bool, to: u64) {
    while *at < to {
        if std::mem::take(start_up) {
            w.end_start_up();
        }
        w.step(&[]);
        *at += 1;
    }
}

/// Carries out a `turn` record's commands one at a time (a build or job acts on the synced selection as the commands
/// before it left it), and checks them against the input's timing.
fn replay_turn(w: &mut World, theirs: &Record, s: &mut Summary, multiplayer: bool) {
    for c in &theirs.commands {
        // spec gap: the trace format doesn't say what a turn's `other` command is. In a multiplayer-launch trace it is
        // the player leaving when the slot is "nobody" in the record's `game` extra afterwards:
        // `mp-skirmish-bne-mp-skirmish-nobody-fight-game-1300` lists slot 1's `other` in the turn of n = 1022, where the
        // slot becomes "nobody" (`net/leave-and-drop.md` LV19). `mp-battle-2-mp-battle2-nobody-moving-game-goals-
        // vision-1450` lists one at n = 1202 that leaves slot 1 a human until its `other` at n = 1250 (LV24).
        let left = theirs.controllers.get(c.player as usize).is_none_or(|k| k == "nobody");
        if multiplayer && left && matches!(c.kind, replay::Kind::Other) {
            w.leave_game(c.player as usize);
        }
        if let Some(cmd) = replay::sim_command(w, c) {
            w.turn(std::slice::from_ref(&cmd));
        }
        s.replayed += 1;
    }
    s.timing.turn(theirs.n, &theirs.commands);
}

/// Steps `w` through the trace's records and compares each, printing the first divergence (and, with `--keep-going`
/// or `--resync-rng`, carrying on as the options say). Setup and turn records are compared without stepping; a turn
/// record's commands are applied first, in the order it lists them (trace format, "When a command runs": the turn
/// records are the authoritative timing).
///
/// A windowed trace (`from` = F; trace format, "Windowed traces") has its setup and start-up records compared as
/// usual. Then the cycles up to F − 1 run without comparing; each `turn` record before F has its commands carried out
/// after the cycles up to its `n`, and isn't compared or counted. The records from F on are compared as usual. A
/// setup-only trace has its setup record compared and nothing else ("Setup-only traces").
pub fn run(
    w: &mut World,
    records: impl Iterator<Item = Result<Record>>,
    opts: &Options,
    out: &mut dyn Write,
) -> Result<Summary> {
    let mut s = Summary {
        timing: replay::Timing::with_turn_length(&opts.input, opts.turn_length),
        from: opts.from,
        setup_only: opts.setup_only,
        ..Default::default()
    };
    if opts.multiplayer {
        s.timing
            .check_second_human(&opts.input, opts.local_player, &opts.controllers);
    }
    let seed = opts.seed.unwrap_or(w.rng.state);
    // Theirs and ours after the previous record compared; `None` across a windowed trace's gap.
    let mut prev = Some((seed, seed));
    let mut last_n: Option<u64> = None;
    // The cycle our world has run to, and whether a start-up cycle has run that the counter reset hasn't ended.
    let mut at = 0u64;
    let mut start_up = false;
    let mut in_window = opts.from.is_none();
    // The saved games the trace wrote, by name (`save` records): our world's stored bytes at that point.
    let mut saves: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // A `save` record names the cycle it saved after (`n`) but can follow later records in the file, so a trace whose
    // input saves keeps our stored bytes after each of the last few cycles to pick from.
    let saving = opts.input.iter().any(|e| matches!(e.kind, replay::Kind::Save { .. }));
    let mut history: std::collections::VecDeque<(u64, Vec<u8>)> = Default::default();
    // The differences of the previous record, and every place each field has differed.
    let mut before: BTreeSet<(Scope, &'static str)> = BTreeSet::new();
    let mut places: BTreeMap<String, BTreeSet<Scope>> = BTreeMap::new();
    for rec in records {
        let mut theirs = rec?;
        if opts.max_cycles.is_some_and(|m| theirs.n > m) {
            break;
        }
        if opts.setup_only && theirs.kind != "setup" {
            writeln!(
                out,
                "note: a setup-only trace has a record n={} ({}); not compared",
                theirs.n, theirs.kind
            )?;
            break;
        }
        // A `peer` record (multiplayer launch: a key sent to, or a screenshot of, a peer's window) is a note of what
        // the harness did on the other machine; it holds no game state and nothing to compare.
        if theirs.kind == "peer" {
            continue;
        }
        s.reached = s.reached.max(theirs.n);
        s.hits += theirs.hits.len();
        s.goal_records += usize::from(!theirs.goals.is_empty());
        if let Some(from) = opts.from {
            if !in_window && theirs.kind == "turn" && theirs.n < from {
                // A command turn in the gap: run up to it and carry out its commands; it isn't compared.
                step_to(w, &mut at, &mut start_up, theirs.n);
                replay_turn(w, &theirs, &mut s, opts.turn_length.is_some());
                s.gap_turns += 1;
                prev = None;
                continue;
            }
            if !in_window && theirs.n >= from {
                in_window = true;
                let to = if theirs.kind == "turn" { theirs.n } else { theirs.n - 1 };
                if at < to {
                    step_to(w, &mut at, &mut start_up, to);
                    prev = None;
                }
                if prev.is_none() {
                    // The gap is expected: the record follows the cycles just run.
                    last_n = Some(at);
                }
            }
            if in_window {
                s.window_records += 1;
            }
        }
        match theirs.kind.as_str() {
            "setup" => {
                // `game/save-load.md`, "Loading": a `setup` record with `load` starts a new game from the saved game
                // of that name; the counter restarts, so the `n` of the records restarts too.
                if let Some(name) = &theirs.load {
                    let Some(bytes) = saves.get(name) else {
                        bail!(
                            "record n={}: loads `{name}`, which no earlier `save` record wrote",
                            theirs.n
                        );
                    };
                    w.restore(bytes, theirs.rng)
                        .map_err(|e| anyhow::anyhow!("record n={}: loading `{name}`: {e:?}", theirs.n))?;
                    // The turn records and the input entries after the `load` count `n` from the loaded game.
                    s.timing.loaded(s.reached);
                    s.reached = 0;
                    at = 0;
                    start_up = false;
                    prev = Some((theirs.rng, theirs.rng));
                    history.clear();
                    s.loads += 1;
                }
            }
            "save" => {
                // The game wrote a saved game after the cycle before this record; keep ours for a later load.
                if let Some(name) = &theirs.save {
                    let Some((_, bytes)) = history.iter().rev().find(|(n, _)| *n == theirs.n) else {
                        bail!(
                            "record n={}: saves `{name}`, but our world isn't kept at that cycle",
                            theirs.n
                        );
                    };
                    saves.insert(name.clone(), bytes.clone());
                }
                continue;
            }
            "turn" => replay_turn(w, &theirs, &mut s, opts.turn_length.is_some()),
            // A `say` record follows the turn that carried the cheat word (the turn's `cheat` command ran it). A `fog`
            // record is the fog option switched during play in the cycle just run (`vision/fog.md`, "Fog-of-war
            // option and cheat"; F45). Neither carries units or a generator state: only the option and the `fogmaps`
            // extra are compared.
            "say" | "fog" => {
                if let Some(on) = theirs.fog_event {
                    w.set_fog_of_war(on);
                }
            }
            "startup" | "cycle" => {
                if let Some(l) = last_n
                    && theirs.n != l + 1
                {
                    writeln!(out, "note: record n={} follows n={l}; stepping once", theirs.n)?;
                }
                // Scheduler rule 1: the counter is reset after the start-up cycles.
                if theirs.kind == "cycle" && std::mem::take(&mut start_up) {
                    w.end_start_up();
                }
                w.step(&[]);
                at = theirs.n;
                start_up |= theirs.kind == "startup";
            }
            k => {
                writeln!(out, "note: skipping record n={} of unknown kind `{k}`", theirs.n)?;
                continue;
            }
        }
        // A `say` or `fog` record sits between cycles; it isn't a cycle the next one follows.
        if !matches!(theirs.kind.as_str(), "say" | "fog") {
            last_n = Some(theirs.n);
        }
        if saving && theirs.kind == "cycle" {
            history.push_back((theirs.n, w.save_state()));
            if history.len() > 4 {
                history.pop_front();
            }
        }
        let mut ours = snapshot(w, theirs.n, &theirs.kind);
        if theirs.fogmaps.is_some() {
            ours.fogmaps = Some(fog_maps(w));
        }
        if matches!(theirs.kind.as_str(), "say" | "fog") {
            // The record holds the option and the maps alone: take the rest from ours so it compares equal.
            let (n, kind, fogmaps) = (theirs.n, theirs.kind.clone(), theirs.fogmaps.take());
            let option = theirs.fog_option;
            theirs = ours.clone();
            theirs.n = n;
            theirs.kind = kind;
            theirs.fogmaps = fogmaps;
            if let (Some(g), Some(o)) = (&mut theirs.game, option) {
                g.0[1] = Cell::Int(o);
            }
        }
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
        prev = Some((theirs.rng, w.rng.state));
        if opts.setup_only {
            break;
        }
    }
    s.timing.finish(s.reached);
    Ok(s)
}

/// The replay lines of the summary: commands applied, and the input checked against T(i). A line starting
/// "input timing disagrees" marks a disagreement between the input and the turn records.
fn print_replay(out: &mut dyn Write, s: &Summary) -> Result<()> {
    let t = &s.timing;
    if s.replayed == 0 && t.is_empty() {
        return Ok(());
    }
    writeln!(out, "replayed {} commands from turn records", s.replayed)?;
    if !t.is_empty() {
        let last = s.reached;
        writeln!(
            out,
            "input timing: {} entries ran in turn T(i), {} disagree, {} due after n={last}",
            t.on_time,
            t.disagree.len(),
            t.not_reached(last)
        )?;
    }
    for d in &t.disagree {
        writeln!(out, "  input timing disagrees: {d}")?;
    }
    for e in &t.extra {
        writeln!(out, "  not from the input: {e}")?;
    }
    for e in &t.second_human_errors {
        writeln!(out, "  second human's command not allowed: {e}")?;
    }
    Ok(())
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
    if s.setup_only {
        writeln!(out, "setup-only trace: compared the setup record only")?;
    }
    if let Some(from) = s.from {
        writeln!(
            out,
            "windowed trace from n={from}: {} command turns before it replayed without comparing",
            s.gap_turns
        )?;
        if s.window_records == 0 {
            writeln!(
                out,
                "the trace ends before n={from} (last record n={}): no record from the window compared",
                s.reached
            )?;
        }
    }
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
    print_replay(out, s)?;
    if s.hits > 0 || s.goal_records > 0 {
        writeln!(
            out,
            "extras read, not compared: {} hits entries, goals in {} records",
            s.hits, s.goal_records
        )?;
    }
    if !s.skipped.is_empty() {
        writeln!(out, "not compared:")?;
        for k in &s.skipped {
            writeln!(out, "  {k}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tidewater_formats::script::Scripts;
    use tidewater_formats::udta::{UnitStats, UnitTable, flags};

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
            version: 1,
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
            // The `vision` and `game` extras aren't written by the synthetic writer.
            r.vision.clear();
            r.game = None;
            r.aijobs = None;
            r.aigroups = None;
            r.airesearch = None;
            r.aiworkers = None;
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
    fn format_2_turn_records_are_replayed_and_timed() {
        // A select handed over after record 2 runs in turn 3 (T = H + 1); a move by the same player handed over at
        // the same time runs one turn later (trace format, "When a command runs").
        // The synthetic scripts have no move animation; a stride lets the type take a move command.
        let movable = || {
            let mut w = world(7);
            w.stride = vec![1; 110];
            w
        };
        let mut w = movable();
        let unit = w.units.iter().find(|u| u.owner == 0).unwrap().clone();
        let entry = |n: Option<u64>, v: Value| {
            let mut e = Entry::parse(&v).unwrap();
            e.n = n;
            e
        };
        let select = serde_json::json!({"player": 0, "kind": "select", "units": [unit.slot]});
        let mv = serde_json::json!({"player": 0, "kind": "command", "command": 3, "tile": [4, 1]});
        let mut out = vec![snapshot(&w, 0, "setup")];
        for n in 1..=8u64 {
            w.step(&[]);
            out.push(snapshot(&w, n, if n <= 2 { "startup" } else { "cycle" }));
            let ran = match n {
                3 => Some(&select),
                4 => Some(&mv),
                _ => None,
            };
            if let Some(v) = ran {
                let e = entry(None, v.clone());
                w.turn(&[replay::sim_command(&w, &e).unwrap()]);
                let mut r = snapshot(&w, n, "turn");
                r.commands = vec![e];
                out.push(r);
            }
        }
        let moved = w.unit(unit.id).unwrap();
        assert!(
            moved.order != unit.order || moved.pending.is_some(),
            "the move reached the unit"
        );
        let h = Header {
            version: 2,
            input: vec![entry(Some(2), select), entry(Some(2), mv)],
            ..header()
        };
        let mut t = h.to_json() + "\n";
        for r in &out {
            t += &r.to_json();
            t += "\n";
        }
        let (back, records) = read(reader(std::io::Cursor::new(t.into_bytes())).unwrap()).unwrap();
        assert_eq!(back, h);
        let opts = Options {
            input: back.input.clone(),
            ..Options::default()
        };
        let mut w = movable();
        let mut text = Vec::new();
        let s = run(&mut w, records, &opts, &mut text).unwrap();
        print_summary(&mut text, &s).unwrap();
        let text = String::from_utf8(text).unwrap();
        assert_eq!((s.diverged, s.replayed), (0, 2), "{text}");
        assert!(
            text.contains("input timing: 2 entries ran in turn T(i), 0 disagree"),
            "{text}"
        );
        assert_eq!(w.selection(0), vec![unit.id]);
    }

    /// Trace format, "Cheat commands", the Human06 vectors: the cheat's `turn` record already holds the resources for
    /// all 16 players and draws nothing; afterwards build speed and mission select stay on, the one-shot resources flag
    /// is dropped, and the mission counts as cheated for the local player.
    #[test]
    fn cheat_turns_are_replayed() {
        use tidewater_sim::cheats::flag;
        let cheat = serde_json::json!({"player": 1, "kind": "cheat",
            "cheats": ["make it so", "glittering prizes", "tigerlily"]});
        let e = Entry::parse(&cheat).unwrap();
        let start = |w: &mut World| {
            w.vision.local = 1;
            w.start_cheats(tidewater_sim::cheats::Flags(flag::MISSION_SELECT));
            for p in &mut w.players {
                p.gold = 1000;
            }
            w.players[5].gold = 500;
        };
        let mut w = world(7);
        start(&mut w);
        let mut out = vec![snapshot(&w, 0, "setup")];
        for n in 1..=5u64 {
            w.step(&[]);
            out.push(snapshot(&w, n, if n <= 2 { "startup" } else { "cycle" }));
            if n == 3 {
                let mut r = snapshot(&w, n, "turn");
                for p in 0..16 {
                    let gold = if p == 5 { 10_500 } else { 11_000 };
                    r.resources[0][p] = Cell::Int(gold);
                    r.resources[1][p] = Cell::Int(5_000);
                    r.resources[2][p] = Cell::Int(5_000);
                }
                r.commands = vec![e.clone()];
                out.push(r);
                // The turn's state, built by hand above, is what the cheat gives.
                w.turn(&[replay::sim_command(&w, &e).unwrap()]);
            }
        }
        let h = Header {
            version: 2,
            input: vec![Entry {
                n: Some(2),
                ..Entry::parse(
                    &serde_json::json!({"player": 1, "kind": "cheat", "add": ["make it so", "glittering prizes"]}),
                )
                .unwrap()
            }],
            ..header()
        };
        let mut t = h.to_json() + "\n";
        for r in &out {
            t += &r.to_json();
            t += "\n";
        }
        let (back, records) = read(reader(std::io::Cursor::new(t.into_bytes())).unwrap()).unwrap();
        let opts = Options {
            input: back.input.clone(),
            ..Options::default()
        };
        let mut w = world(7);
        start(&mut w);
        let mut text = Vec::new();
        let s = run(&mut w, records, &opts, &mut text).unwrap();
        print_summary(&mut text, &s).unwrap();
        let text = String::from_utf8(text).unwrap();
        assert_eq!((s.diverged, s.replayed), (0, 1), "{text}");
        assert!(
            text.contains("input timing: 1 entries ran in turn T(i), 0 disagree"),
            "{text}"
        );
        assert_eq!(
            w.cheats.flags,
            tidewater_sim::cheats::Flags(flag::BUILD_SPEED | flag::MISSION_SELECT)
        );
        assert!(w.cheats.used && w.rules.build_speed_cheat);
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
    fn skipped_cells_are_not_compared() {
        assert_eq!(Cell::Int(33).matches(&Cell::Int(33)), Some(true));
        assert_eq!(Cell::Int(33).matches(&Cell::Int(37)), Some(false));
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

    /// Trace format, "Multiplayer launch": the header's launch fields are read, and the game seed is the start seed
    /// plus 4, mod 256. A missing `launch_path` is a campaign start.
    #[test]
    fn reads_the_multiplayer_launch_header() {
        let mut h = header();
        h.rules = Some("enhanced".into());
        h.multiplayer = Some(Multiplayer {
            connection: "direct-connect".into(),
            map: "Synthetic.pud".into(),
            start_seed: 253,
            humans: 2,
            opponents: None,
            fixed_order: false,
            fog_of_war: true,
        });
        let back = Header::parse(&h.to_json()).unwrap();
        assert_eq!(back.rules.as_deref(), Some("enhanced"));
        let m = back.multiplayer.unwrap();
        assert_eq!(m, h.multiplayer.clone().unwrap());
        assert_eq!(m.game_seed(), 1);
        assert_eq!(Header::parse(&header().to_json()).unwrap().multiplayer, None);
    }

    /// Trace format, round-60 harness: `opponents` and `starts`; without them (traces made before round 60) the starts
    /// are random and the opponent count is unknown.
    #[test]
    fn reads_the_opponents_and_starts_fields() {
        let mut h = header();
        h.multiplayer = Some(Multiplayer {
            connection: "direct-connect".into(),
            map: "Synthetic.pud".into(),
            start_seed: 1,
            humans: 2,
            opponents: Some(2),
            fixed_order: true,
            fog_of_war: true,
        });
        let m = Header::parse(&h.to_json()).unwrap().multiplayer.unwrap();
        assert_eq!((m.opponents, m.fixed_order), (Some(2), true));
        let mut v: Value = serde_json::from_str(&h.to_json()).unwrap();
        v["starts"] = "random".into();
        let m = Header::parse(&v.to_string()).unwrap().multiplayer.unwrap();
        assert_eq!((m.opponents, m.fixed_order), (Some(2), false));
        v["starts"] = "sideways".into();
        assert!(Header::parse(&v.to_string()).is_err());
        v["starts"] = "random".into();
        v["opponents"] = 7.into();
        assert!(Header::parse(&v.to_string()).is_err());
        let o = v.as_object_mut().unwrap();
        o.remove("opponents");
        o.remove("starts");
        let m = Header::parse(&v.to_string()).unwrap().multiplayer.unwrap();
        assert_eq!((m.opponents, m.fixed_order), (None, false));
    }

    #[test]
    fn rejects_a_rules_mismatch_and_an_unknown_launch_path() {
        let mut h = header();
        h.rules = Some("classic".into());
        assert!(Header::parse(&h.to_json()).is_err(), "classic rules, enhanced true");
        h.enhanced = false;
        assert!(Header::parse(&h.to_json()).is_ok());
        let mut v: Value = serde_json::from_str(&header().to_json()).unwrap();
        v["launch_path"] = "elsewhere".into();
        assert!(Header::parse(&v.to_string()).is_err());
        v["launch_path"] = "multiplayer".into();
        assert!(Header::parse(&v.to_string()).is_err(), "no `map`");
    }

    /// The synthetic world with a stride, so that its unit type takes a move command.
    fn movable() -> World {
        let mut w = world(7);
        w.stride = vec![1; 110];
        w
    }

    /// A complete version 2 trace of [`movable`] over `cycles` normal cycles, in which player 0 selects its unit in
    /// turn `sel` and sends it east in turn `sel + 1`, both handed over after record `sel - 1`.
    fn command_trace(cycles: u64, sel: u64) -> (Header, Vec<Record>) {
        let mut w = movable();
        let unit = w.units.iter().find(|u| u.owner == 0).unwrap().slot;
        let select = serde_json::json!({"player": 0, "kind": "select", "units": [unit]});
        let mv = serde_json::json!({"player": 0, "kind": "command", "command": 3, "tile": [6, 1]});
        let entry = |n: Option<u64>, v: &Value| Entry {
            n,
            ..Entry::parse(v).unwrap()
        };
        let mut out = vec![snapshot(&w, 0, "setup")];
        for n in 1..=2 + cycles {
            if n == 3 {
                w.end_start_up();
            }
            w.step(&[]);
            out.push(snapshot(&w, n, if n <= 2 { "startup" } else { "cycle" }));
            let ran = [(sel, &select), (sel + 1, &mv)].into_iter().find(|(t, _)| *t == n);
            if let Some((_, v)) = ran {
                let e = entry(None, v);
                w.turn(&[replay::sim_command(&w, &e).unwrap()]);
                let mut r = snapshot(&w, n, "turn");
                r.commands = vec![e];
                out.push(r);
            }
        }
        let h = Header {
            version: 2,
            input: vec![entry(Some(sel - 1), &select), entry(Some(sel - 1), &mv)],
            ..header()
        };
        (h, out)
    }

    /// Trace format, "Windowed traces": the header with `from` = F, then the setup and start-up records, the command
    /// turns before F and every record from F on.
    fn windowed(h: &Header, full: &[Record], from: u64) -> (Header, Vec<Record>) {
        let keep = |r: &&Record| {
            matches!(r.kind.as_str(), "setup" | "startup")
                || (r.kind == "turn" && !r.commands.is_empty())
                || r.n >= from
        };
        let h = Header {
            from: Some(from),
            ..h.clone()
        };
        (h, full.iter().filter(keep).cloned().collect())
    }

    /// Runs a trace through the JSON writer and reader against [`movable`].
    fn run_trace(h: &Header, records: &[Record], keep_going: bool) -> (Summary, String) {
        let mut t = h.to_json() + "\n";
        for r in records {
            t += &r.to_json();
            t += "\n";
        }
        let (back, records) = read(reader(std::io::Cursor::new(t.into_bytes())).unwrap()).unwrap();
        assert_eq!(&back, h);
        let opts = Options {
            keep_going,
            input: back.input.clone(),
            from: back.from,
            setup_only: back.setup_only,
            ..Options::default()
        };
        let mut w = movable();
        let mut text = Vec::new();
        let s = run(&mut w, records, &opts, &mut text).unwrap();
        print_summary(&mut text, &s).unwrap();
        (s, String::from_utf8(text).unwrap())
    }

    #[test]
    fn windowed_trace_replays_the_gap_turns_and_compares_from_f() {
        let (h, full) = command_trace(30, 5);
        let (wh, win) = windowed(&h, &full, 12);
        // Setup, two start-up records, the two command turns, then every record from n 12.
        assert_eq!(win.len(), 5 + full.iter().filter(|r| r.n >= 12).count());
        let (s, text) = run_trace(&wh, &win, false);
        assert_eq!((s.diverged, s.gap_turns, s.replayed), (0, 2, 2), "{text}");
        assert_eq!((s.records, s.window_records), (3 + win.len() - 5, win.len() - 5));
        assert!(!text.contains("stepping once"), "{text}");
        assert!(
            text.contains("windowed trace from n=12: 2 command turns before it replayed without comparing"),
            "{text}"
        );
        assert!(
            text.contains("input timing: 2 entries ran in turn T(i), 0 disagree"),
            "{text}"
        );
        // The gap's commands matter: without them the unit isn't sent anywhere and the window diverges at once.
        let mut dropped = win.clone();
        for r in &mut dropped {
            r.commands.clear();
        }
        let (s, text) = run_trace(&wh, &dropped, false);
        assert_eq!(s.stopped, Some((12, "cycle".to_string())), "{text}");
        assert!(s.first.contains_key(&Category::Orders), "{text}");
        assert!(text.contains("not known across the window's gap"), "{text}");
    }

    #[test]
    fn a_windowed_trace_compares_as_its_complete_trace() {
        let (h, mut full) = command_trace(30, 5);
        // A divergence inside the window, after the move has started.
        let i = full.iter().position(|r| r.n == 16 && r.kind == "cycle").unwrap();
        let slot = *full[i].units.keys().next().unwrap();
        set(&mut full[i], slot, "facing", 9);
        let (wh, win) = windowed(&h, &full, 12);
        for keep_going in [false, true] {
            let (a, at) = run_trace(&h, &full, keep_going);
            let (b, bt) = run_trace(&wh, &win, keep_going);
            assert_eq!(a.stopped, b.stopped, "{at}\n{bt}");
            assert_eq!(a.diverged, b.diverged, "{at}\n{bt}");
            assert_eq!(a.fields, b.fields, "{at}\n{bt}");
            assert_eq!(a.onsets, b.onsets, "{at}\n{bt}");
            assert_eq!(a.replayed, b.replayed);
            assert_eq!(a.first[&Category::Facing].diff, b.first[&Category::Facing].diff);
            assert_eq!(a.first[&Category::Facing].n, 16);
        }
        // Unchanged, both match in full.
        let (h, full) = command_trace(30, 5);
        let (wh, win) = windowed(&h, &full, 12);
        let (a, _) = run_trace(&h, &full, false);
        let (b, _) = run_trace(&wh, &win, false);
        assert_eq!((a.diverged, b.diverged), (0, 0));
        assert_eq!(a.last_n, b.last_n);
    }

    #[test]
    fn a_windowed_trace_that_ends_before_f_compares_no_window_record() {
        let (h, full) = command_trace(30, 5);
        let (wh, win) = windowed(&h, &full, 500);
        assert_eq!(win.len(), 5);
        let (s, text) = run_trace(&wh, &win, false);
        assert_eq!(
            (s.records, s.window_records, s.diverged, s.gap_turns),
            (3, 0, 0, 2),
            "{text}"
        );
        assert!(
            text.contains("the trace ends before n=500 (last record n=6): no record from the window compared"),
            "{text}"
        );
        assert!(text.contains("0 disagree"), "{text}");
    }

    #[test]
    fn a_setup_only_trace_compares_the_setup_record_only() {
        let (h, full) = command_trace(10, 5);
        let h = Header { setup_only: true, ..h };
        let (s, text) = run_trace(&h, &full[..1], false);
        assert_eq!((s.records, s.diverged, s.last_n), (1, 0, Some(0)), "{text}");
        assert!(
            text.contains("setup-only trace: compared the setup record only"),
            "{text}"
        );
        assert!(text.contains("no divergence"), "{text}");
        // The input that never ran isn't due in a setup-only trace's single record.
        assert!(text.contains("0 disagree"), "{text}");
        // Its setup record diverging is still found.
        let mut setup = full[..1].to_vec();
        setup[0].rng ^= 1;
        let (s, _) = run_trace(&h, &setup, false);
        assert_eq!(s.stopped, Some((0, "setup".to_string())));
    }

    /// Trace format, "Header": `from`, `setup_only` and `extras` are read, are checked, and change nothing else.
    #[test]
    fn reads_the_window_setup_only_and_extras_header_fields() {
        let h = Header {
            from: Some(6000),
            extras: vec!["goals".into(), "hits".into()],
            rules: Some("enhanced".into()),
            ..header()
        };
        let back = Header::parse(&h.to_json()).unwrap();
        assert_eq!(back, h);
        assert!(back.enhanced);
        let s = Header {
            setup_only: true,
            ..header()
        };
        assert_eq!(Header::parse(&s.to_json()).unwrap(), s);
        let mut v: Value = serde_json::from_str(&header().to_json()).unwrap();
        for bad in [Value::from(0), Value::from(-3), Value::from("12"), Value::from(1.5)] {
            v["from"] = bad;
            assert!(Header::parse(&v.to_string()).is_err(), "{v}");
        }
        v["from"] = 1.into();
        assert_eq!(Header::parse(&v.to_string()).unwrap().from, Some(1));
        v["setup_only"] = true.into();
        assert!(Header::parse(&v.to_string()).is_err(), "setup-only with `from`");
        v.as_object_mut().unwrap().remove("from");
        v["setup_only"] = "yes".into();
        assert!(Header::parse(&v.to_string()).is_err());
        v["setup_only"] = false.into();
        v["extras"] = "hits".into();
        assert!(Header::parse(&v.to_string()).is_err());
    }

    /// Trace format, "Computer worker jobs" (round 98, W2): a town hall the no-depot step started carries bit 16, a farm
    /// bit 8.
    #[test]
    fn a_no_depot_town_hall_job_is_a_build_list_job_bit() {
        let site = (4, 4);
        let job = |kind| AiJob::Build { kind, site };
        assert_eq!(job_bits(job(0x4a)), Some(16));
        assert_eq!(job_bits(job(0x4b)), Some(16));
        assert_eq!(job_bits(job(0x3a)), Some(8));
        assert_eq!(job_bits(job(0x3b)), Some(8));
        assert_eq!(job_bits(AiJob::List { kind: 0x3a, site }), Some(16));
        assert_eq!(job_bits(AiJob::None), None);
    }

    /// Trace format, "Opt-in extras": `hits` and the units' `goal`, `pending` and `transport_dest` are read and not
    /// compared.
    #[test]
    fn reads_the_extras_without_comparing_them() {
        let line = r#"{"n":936,"kind":"cycle","rng":5,"units":[{"slot":1492,"hp":52,"goal":[5,86],"pending":null},{"slot":1428,"type":28,"goal":[0,65535],"pending":3,"transport_dest":[36,114]},{"slot":7}],"effects":[],"hits":[{"by":"melee","attacker":1484,"target":1492,"damage":8,"hp":60},{"by":"missile","attacker":1483,"target":null,"damage":13,"effect":3}]}"#;
        let r = Record::parse(line).unwrap();
        assert_eq!(
            r.hits[0],
            Hit {
                by: "melee".into(),
                attacker: Some(1484),
                target: Some(1492),
                damage: 8,
                hp: Some(60),
                effect: None,
            }
        );
        assert_eq!((r.hits[1].target, r.hits[1].effect), (None, Some(3)));
        assert_eq!(
            r.goals[&1492],
            Goals {
                goal: Some([5, 86]),
                pending: Some(Cell::Null),
                transport_dest: None,
            }
        );
        assert_eq!(r.goals[&1428].pending, Some(Cell::Int(3)));
        assert_eq!(r.goals[&1428].transport_dest, Some([36, 114]));
        assert!(!r.goals.contains_key(&7));
        assert_eq!(Record::parse(&r.to_json()).unwrap(), r);
        let mut ours = r.clone();
        ours.hits.clear();
        ours.goals.clear();
        let mut skipped = BTreeSet::new();
        assert!(compare(&r, &ours, &Filter::default(), &mut skipped).is_empty());
        assert!(skipped.is_empty());
    }

    /// The `aigoals` extra (round 85): each unit's `ai_goal` kind and tile are compared where the trace carries them;
    /// `extra` isn't, and a trace without the extra compares as before.
    #[test]
    fn compares_the_ai_goal_kind_and_tile() {
        let line = r#"{"n":1004,"kind":"cycle","rng":5,"units":[{"slot":1575,"hp":30,"ai_goal":{"kind":2,"tile":[6,19],"extra":59}},{"slot":1576,"hp":30}],"effects":[]}"#;
        let theirs = Record::parse(line).unwrap();
        let goal = |k, x, y| [Cell::Int(k), Cell::Int(x), Cell::Int(y)];
        assert_eq!(theirs.ai_goals[&1575], goal(2, 6, 19));
        assert!(!theirs.ai_goals.contains_key(&1576));
        assert_eq!(Record::parse(&theirs.to_json()).unwrap(), theirs);
        let mut ours = theirs.clone();
        ours.ai_goals.insert(1576, goal(1, 3, 3));
        let mut skipped = BTreeSet::new();
        let f = Filter::default();
        assert!(
            compare(&theirs, &ours, &f, &mut skipped).is_empty(),
            "1576 carries no ai_goal"
        );
        ours.ai_goals.insert(1575, goal(2, 8, 19));
        let d = compare(&theirs, &ours, &f, &mut skipped);
        assert_eq!(d.len(), 1);
        assert_eq!(
            (d[0].scope, d[0].field, d[0].category()),
            (Scope::Unit(1575), "ai_goal_x", Category::AiGoals)
        );
        assert_eq!(d[0].to_string(), "unit slot 1575 ai_goal_x: ours 8, theirs 6");
        assert!(compare(&theirs, &ours, &Filter::parse("-ai-goals"), &mut skipped).is_empty());
        // Without the extra nothing is compared, whatever ours holds.
        let mut plain = theirs.clone();
        plain.ai_goals.clear();
        assert!(compare(&plain, &ours, &f, &mut skipped).is_empty());
    }

    /// The `aiworkers` extra (trace format, "Computer worker jobs", "Comparing extras"): the counts as 16-bit numbers,
    /// `hall_cooldown` and `building` exactly (a missing key is 0), and `jobs` only for slots in the record's `units`.
    #[test]
    fn compares_the_computer_worker_jobs() {
        let ints = |v: &[(usize, u32)]| {
            let mut a = [0u32; 16];
            for &(p, n) in v {
                a[p] = n;
            }
            format!("{a:?}")
        };
        let line = format!(
            r#"{{"n":9,"kind":"cycle","rng":5,"units":[{{"slot":1423,"hp":30}},{{"slot":1448,"hp":30}}],"effects":[],"aiworkers":{{"miners":{},"lumberjacks":{},"repairers":{},"hall_cooldown":{},"building":{{"6:0x40":65535,"0:0x3a":2}},"jobs":{{"1423":[32,128],"1448":[16,0],"1999":[1,128]}}}}}}"#,
            ints(&[(6, 2)]),
            ints(&[]),
            ints(&[(6, 65535)]),
            ints(&[(3, 7)]),
        );
        let theirs = Record::parse(&line).unwrap();
        let aw = theirs.aiworkers.clone().unwrap();
        assert_eq!(aw.players[6][2], Cell::Int(65535));
        assert_eq!(aw.players[6][4], Cell::Text("0x40=65535".into()));
        assert_eq!(aw.players[0][4], Cell::Text("0x3a=2".into()));
        assert_eq!(aw.players[1][4], Cell::Text(String::new()), "no key is 0");
        assert_eq!(aw.jobs[&1423], [Cell::Int(32), Cell::Int(128)]);
        let mut ours = theirs.clone();
        let mut skipped = BTreeSet::new();
        let f = Filter::default();
        // Slot 1999 isn't in `units`: its job isn't compared, whatever ours says.
        ours.aiworkers.as_mut().unwrap().jobs.remove(&1999);
        assert!(compare(&theirs, &ours, &f, &mut skipped).is_empty());
        // A count that wraps differently, a building count and a job bit each differ.
        let o = ours.aiworkers.as_mut().unwrap();
        o.players[6][2] = Cell::Int(65534);
        o.players[6][4] = Cell::Text("0x40=1".into());
        o.jobs.insert(1448, [Cell::Int(8), Cell::Int(0)]);
        o.jobs.remove(&1423);
        let d = compare(&theirs, &ours, &f, &mut skipped);
        let named: Vec<_> = d.iter().map(|d| (d.scope, d.field, d.category())).collect();
        assert_eq!(
            named,
            [
                (Scope::Player(6), "aiw_repairers", Category::AiWorkers),
                (Scope::Player(6), "aiw_building", Category::AiWorkers),
                (Scope::Unit(1423), "aiw_job", Category::AiWorkers),
                (Scope::Unit(1423), "aiw_mode", Category::AiWorkers),
                (Scope::Unit(1448), "aiw_job", Category::AiWorkers),
            ]
        );
        assert!(compare(&theirs, &ours, &Filter::parse("-aiworkers"), &mut skipped).is_empty());
    }
}
