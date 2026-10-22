//! Command replay for `twtool oracle`: the commands of a format version 2 trace (`docs/specs/oracle/trace-format.md`,
//! "Commands" and "When a command runs"), turned into our simulation's player commands, and the check of the header's
//! `input` against the turns that carried it out.
//!
//! Each command acts on the issuing player's synced selection as `ui/selecting-and-commanding.md` says ("Two
//! selections", "Which units take part in a command": the handled commands, the selection filter and the formation
//! offset), through the same [`Command`]s the client sends: `select` is [`Command::Select`], `command` is
//! [`Command::Issue`], `build` is [`Command::Build`] for the first unit of the synced selection
//! (`economy/build-train.md`, "Placing a building"), and `job` is the job command for that unit
//! (`economy/tech-tree.md`, "Starting a job"; `economy/build-train.md`, "Training, research and building upgrades").
//!
//! Units are named by unit-table slot (`core/unit-slots.md`). A slot that holds no live unit when the command runs
//! names nothing: it leaves a selection, and a target in it reads as no target.
//!
//! A `cheat` is [`Command::Cheat`] with the flags its `cheats` names (trace format, "Cheat commands"), which the
//! simulation carries out as `game/cheats.md`, "Carrying out a cheat command", says: the mission counts as cheated
//! for the local player, the flags are set, the effects run in the spec's order and the one-shot flags are dropped.
//! A header `input` cheat entry has `add` instead, the flags it turned on; it matches the turn whose `cheats` holds
//! them all.
//!
//! A `build` needs the first selected unit to be able to build: a worker (peasant or peon, type 0x02 or 0x03) or a
//! tanker, which builds oil platforms (type 0x1A or 0x1B; types from the public PUD specification, appendix A).
//!
//! Our reading where the format is open: an `"unnamed N"` cheat flag has no meaning in the cheats spec, so it sets nothing on our side.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tidewater_sim::cheats::{PHRASES, WORDS, flag};
use tidewater_sim::{Command, World};

/// Unit types of the two workers (public PUD specification, appendix A).
const PEASANT: u8 = 0x02;
const PEON: u8 = 0x03;
/// Unit types of the two tankers (public PUD specification, appendix A).
const TANKERS: [u8; 2] = [0x1a, 0x1b];

/// A `job` entry's kind (trace format, "Commands").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// `number` is a unit type.
    Train,
    /// Research a spell; `number` is an upgrade number.
    Research,
    /// `number` is an upgrade number.
    Upgrade,
    /// `number` is a building type.
    UpgradeBuilding,
    /// A job kind outside the four, written as its number.
    Other(i64),
}

impl Job {
    fn parse(v: &Value) -> Result<Job> {
        Ok(match v {
            Value::String(s) => match s.as_str() {
                "train" => Job::Train,
                "research" => Job::Research,
                "upgrade" => Job::Upgrade,
                "upgrade building" => Job::UpgradeBuilding,
                s => bail!("unknown job kind `{s}`"),
            },
            v => Job::Other(v.as_i64().context("job kind is neither a name nor a number")?),
        })
    }

    fn json(self) -> Value {
        match self {
            Job::Train => "train".into(),
            Job::Research => "research".into(),
            Job::Upgrade => "upgrade".into(),
            Job::UpgradeBuilding => "upgrade building".into(),
            Job::Other(n) => n.into(),
        }
    }
}

/// What one command does (trace format, "Commands").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Replaces the synced selection with these unit slots.
    Select { units: Vec<u16> },
    /// Command number `command` for the synced selection. `tile` may be missing in a header `input` entry only.
    Command {
        command: u8,
        tile: Option<(i32, i32)>,
        target: Option<u16>,
    },
    /// The first selected unit builds `kind` with its top-left tile at `tile`.
    Build { kind: u8, tile: (i32, i32) },
    /// The first selected unit starts a job.
    Job { job: Job, number: u8 },
    /// A cheat command carrying the whole new set of cheat flags, by name.
    Cheat { cheats: Vec<String> },
    /// A header `input` cheat entry: the flags it turns on.
    CheatAdd { add: Vec<String> },
    /// Cancel construction for the first selected unit (round 77).
    Cancel,
    /// Cancel job for the first selected unit (round 77).
    CancelJob,
    /// A header `input` entry that saves the game under `name` (`game/save-load.md`). It is a menu action, not a
    /// command: no `turn` record lists it, and the trace's `save` record shows where it ran.
    Save { name: String },
    /// A header `input` entry that loads the saved game `name`: the trace restarts with a `setup` record that has
    /// `load`.
    Load { name: String },
    /// A header `input` entry that types the developer word `word`: a menu-like action. The turn that carries its
    /// cheat command is listed as a `cheat` command, not as this entry (the trace's `say` record shows where it ran).
    Say { word: String },
    /// A header `input` entry that switches the fog-of-war option during play (the game menu): no turn lists it, and
    /// the trace's `fog` record shows where it ran.
    Fog { on: bool },
    /// A header `input` entry the harness ran on a peer's window (a key sent or a screenshot taken, trace format
    /// "The second human"): not a command, so no turn lists it. `action` is the harness's action (`key`, `click`,
    /// `hold`, `desync`, `shot`, `own-key` or `own-click`).
    Peer { action: String },
    /// A command the format doesn't describe.
    Other,
}

/// One command: an entry of a `turn` record's `commands` (`n` is `None`) or of the header's `input`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub n: Option<u64>,
    pub player: u8,
    pub kind: Kind,
}

fn tile(v: Option<&Value>) -> Result<Option<(i32, i32)>> {
    let Some(a) = v.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let a = a.as_array().context("tile is not a list")?;
    let at = |i: usize| a.get(i).and_then(Value::as_i64).context("tile is not [x, y]");
    Ok(Some((at(0)? as i32, at(1)? as i32)))
}

impl Entry {
    pub fn parse(v: &Value) -> Result<Entry> {
        let player = v.get("player").and_then(Value::as_u64).context("missing `player`")? as u8;
        let num = |k: &str| {
            v.get(k)
                .and_then(Value::as_i64)
                .with_context(|| format!("missing `{k}`"))
        };
        let kind = match v.get("kind").and_then(Value::as_str).context("missing `kind`")? {
            "select" => Kind::Select {
                units: v
                    .get("units")
                    .and_then(Value::as_array)
                    .context("missing `units`")?
                    .iter()
                    .map(|u| u.as_u64().map(|s| s as u16).context("a unit slot"))
                    .collect::<Result<_>>()?,
            },
            "command" => Kind::Command {
                command: num("command")? as u8,
                tile: tile(v.get("tile"))?,
                target: v.get("target").and_then(Value::as_u64).map(|s| s as u16),
            },
            "build" => Kind::Build {
                kind: num("type")? as u8,
                tile: tile(v.get("tile"))?.context("missing `tile`")?,
            },
            "job" => Kind::Job {
                job: Job::parse(v.get("job").context("missing `job`")?)?,
                number: num("number")? as u8,
            },
            "cheat" => {
                let names = |k: &str| -> Option<Result<Vec<String>>> {
                    let a = v.get(k)?;
                    Some(a.as_array().context("cheat flags are not a list").and_then(|a| {
                        a.iter()
                            .map(|n| n.as_str().map(str::to_string).context("a cheat flag name"))
                            .collect()
                    }))
                };
                match (names("cheats"), names("add")) {
                    (Some(cheats), _) => Kind::Cheat { cheats: cheats? },
                    (None, Some(add)) => Kind::CheatAdd { add: add? },
                    (None, None) => bail!("a cheat without `cheats` or `add`"),
                }
            }
            "cancel" => Kind::Cancel,
            "cancel-job" => Kind::CancelJob,
            "save" => Kind::Save {
                name: v.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            },
            "load" => Kind::Load {
                name: v.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
            },
            "say" => Kind::Say {
                word: v.get("word").and_then(Value::as_str).unwrap_or_default().to_string(),
            },
            "fog" => Kind::Fog {
                on: v.get("value").and_then(Value::as_str) == Some("on"),
            },
            "peer" => Kind::Peer {
                action: v.get("action").and_then(Value::as_str).unwrap_or_default().to_string(),
            },
            _ => Kind::Other,
        };
        Ok(Entry {
            n: v.get("n").and_then(Value::as_u64),
            player,
            kind,
        })
    }

    /// The entry as trace JSON (for synthetic traces).
    #[cfg(test)]
    pub fn to_json(&self) -> Value {
        let mut o = serde_json::Map::new();
        if let Some(n) = self.n {
            o.insert("n".into(), n.into());
        }
        o.insert("player".into(), self.player.into());
        let pair = |t: (i32, i32)| Value::from(vec![t.0, t.1]);
        match &self.kind {
            Kind::Select { units } => {
                o.insert("kind".into(), "select".into());
                o.insert("units".into(), units.clone().into());
            }
            Kind::Command { command, tile, target } => {
                o.insert("kind".into(), "command".into());
                o.insert("command".into(), (*command).into());
                if let Some(t) = tile {
                    o.insert("tile".into(), pair(*t));
                }
                o.insert("target".into(), target.map_or(Value::Null, Value::from));
            }
            Kind::Build { kind, tile } => {
                o.insert("kind".into(), "build".into());
                o.insert("type".into(), (*kind).into());
                o.insert("tile".into(), pair(*tile));
            }
            Kind::Job { job, number } => {
                o.insert("kind".into(), "job".into());
                o.insert("job".into(), job.json());
                o.insert("number".into(), (*number).into());
            }
            Kind::Cheat { cheats } => {
                o.insert("kind".into(), "cheat".into());
                o.insert("cheats".into(), cheats.clone().into());
            }
            Kind::CheatAdd { add } => {
                o.insert("kind".into(), "cheat".into());
                o.insert("add".into(), add.clone().into());
            }
            Kind::Cancel => {
                o.insert("kind".into(), "cancel".into());
            }
            Kind::CancelJob => {
                o.insert("kind".into(), "cancel-job".into());
            }
            Kind::Save { name } => {
                o.insert("kind".into(), "save".into());
                o.insert("name".into(), name.clone().into());
            }
            Kind::Load { name } => {
                o.insert("kind".into(), "load".into());
                o.insert("name".into(), name.clone().into());
            }
            Kind::Say { word } => {
                o.insert("kind".into(), "say".into());
                o.insert("word".into(), word.clone().into());
            }
            Kind::Fog { on } => {
                o.insert("kind".into(), "fog".into());
                o.insert("value".into(), if *on { "on" } else { "off" }.into());
            }
            Kind::Peer { action } => {
                o.insert("kind".into(), "peer".into());
                o.insert("action".into(), action.clone().into());
            }
            Kind::Other => {
                o.insert("kind".into(), "other".into());
            }
        }
        Value::Object(o)
    }

    /// Whether a turn's command `ran` is this input entry: the same player and command, where an input `command`
    /// without a tile matches any tile (the target unit's tile is used then).
    pub fn same_command(&self, ran: &Entry) -> bool {
        if self.player != ran.player {
            return false;
        }
        match (&self.kind, &ran.kind) {
            (
                Kind::Command {
                    command: a,
                    tile: None,
                    target: ta,
                },
                Kind::Command {
                    command: b, target: tb, ..
                },
            ) => a == b && ta == tb,
            // The harness adds `add` to the flags it reads at hand-over (trace format, "In the header's `input`").
            (Kind::CheatAdd { add }, Kind::Cheat { cheats }) => add.iter().all(|f| cheats.contains(f)),
            // The flags are a set; their order means nothing.
            (Kind::Cheat { cheats: a }, Kind::Cheat { cheats: b }) => {
                a.len() == b.len() && a.iter().all(|f| b.contains(f))
            }
            (a, b) => a == b,
        }
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "player {} ", self.player)?;
        match &self.kind {
            Kind::Select { units } => write!(f, "select {units:?}"),
            Kind::Command { command, tile, target } => {
                write!(f, "command {command}")?;
                if let Some((x, y)) = tile {
                    write!(f, " at ({x}, {y})")?;
                }
                match target {
                    Some(t) => write!(f, " on {t}"),
                    None => Ok(()),
                }
            }
            Kind::Build { kind, tile } => write!(f, "build {kind:#04x} at ({}, {})", tile.0, tile.1),
            Kind::Job { job, number } => write!(f, "job {} {number}", job.json()),
            Kind::Cheat { cheats } => write!(f, "cheat {cheats:?}"),
            Kind::CheatAdd { add } => write!(f, "cheat adding {add:?}"),
            Kind::Cancel => f.write_str("cancel"),
            Kind::CancelJob => f.write_str("cancel-job"),
            Kind::Save { name } => write!(f, "save {name}"),
            Kind::Load { name } => write!(f, "load {name}"),
            Kind::Say { word } => write!(f, "say {word}"),
            Kind::Fog { on } => write!(f, "fog {}", if *on { "on" } else { "off" }),
            Kind::Peer { action } => write!(f, "peer {action}"),
            Kind::Other => f.write_str("other"),
        }
    }
}

/// The cheat flag a trace names (trace format, "Cheat commands"): a phrase or developer word of `game/cheats.md`, or
/// `"notice A"` or `"notice B"` (the two codes without a phrase). `None` for `"unnamed N"` and any other name.
pub fn cheat_flag(name: &str) -> Option<u32> {
    match name {
        "notice A" => Some(flag::NOTICE_A),
        "notice B" => Some(flag::NOTICE_B),
        // The two codes without a phrase: our phrase table holds a line that matches them, not a name.
        "day" | "183" => None,
        _ => PHRASES
            .iter()
            .find(|p| p.text == name)
            .map(|p| p.flag)
            .or_else(|| WORDS.iter().find(|(w, _)| *w == name).and_then(|(_, w)| w.flag())),
    }
}

/// The cheat flags a list of names stands for; a name with no flag on our side adds nothing.
pub fn cheat_flags(names: &[String]) -> u32 {
    names.iter().filter_map(|n| cheat_flag(n)).fold(0, |a, f| a | f)
}

/// The player command our simulation runs for a trace command, as the world stands when it runs; `None` when the
/// command does nothing there (no first selected unit, a builder that is neither a worker nor a tanker, a kind we can't express).
pub fn sim_command(w: &World, e: &Entry) -> Option<Command> {
    let player = e.player;
    let live = |slot: u16| w.unit_in_slot(slot);
    let first = || w.selection(player).first().copied();
    Some(match &e.kind {
        Kind::Select { units } => Command::Select {
            player,
            units: units.iter().filter_map(|&s| live(s)).map(|u| u.id).collect(),
        },
        &Kind::Command { command, tile, target } => {
            let t = target.and_then(live);
            Command::Issue {
                player,
                command,
                target: t.map(|u| u.id),
                tile: tile.or(t.map(|u| u.tile))?,
            }
        }
        &Kind::Build { kind, tile } => {
            let unit = first()?;
            // Trace format, "Commands" (round-40 errata, `orc09-naval`): a tanker builds oil platforms.
            let builds = |k: u8| k == PEASANT || k == PEON || TANKERS.contains(&k);
            if !w.unit(unit).is_some_and(|u| builds(u.kind)) {
                return None;
            }
            Command::Build {
                player,
                unit,
                kind,
                tile,
            }
        }
        &Kind::Job { job, number } => {
            let building = first()?;
            match job {
                Job::Train => Command::Train {
                    player,
                    building,
                    kind: number,
                },
                Job::Research => Command::Spell {
                    player,
                    building,
                    spell: number,
                },
                Job::Upgrade => Command::Research {
                    player,
                    building,
                    upgrade: number,
                },
                Job::UpgradeBuilding => Command::Upgrade {
                    player,
                    building,
                    to: number,
                },
                Job::Other(_) => return None,
            }
        }
        Kind::Cheat { cheats } => Command::Cheat {
            player,
            flags: cheat_flags(cheats),
        },
        // Trace format, "Commands" (round 77): for the first selected unit; the simulation checks that it is the
        // player's own building in the state the command acts in.
        Kind::Cancel => Command::Cancel { player, unit: first()? },
        Kind::CancelJob => Command::CancelJob { player, unit: first()? },
        Kind::CheatAdd { .. }
        | Kind::Save { .. }
        | Kind::Load { .. }
        | Kind::Say { .. }
        | Kind::Fog { .. }
        | Kind::Peer { .. }
        | Kind::Other => return None,
    })
}

/// The turn each input entry runs in, T(i) of "When a command runs": an entry is handed over after record H(i), the
/// largest `n` among it and the entries before it, and runs in the turn after that, one per player per turn. Entries
/// after a `load` count `n` from the loaded game ("Saves, loads, cheat words and fog"), so a load starts the count
/// again: T(0) = 0.
pub fn predict(input: &[Entry]) -> Vec<u64> {
    let mut handed = 0;
    let mut last: BTreeMap<u8, u64> = BTreeMap::new();
    input
        .iter()
        .map(|e| {
            handed = handed.max(e.n.unwrap_or(0));
            let t = last.get(&e.player).map_or(handed + 1, |&t| (handed + 1).max(t + 1));
            last.insert(e.player, t);
            // The entries after a load count `n` from the loaded game (the load's own `n` is in the game before).
            if matches!(e.kind, Kind::Load { .. }) {
                handed = 0;
                last.clear();
            }
            t
        })
        .collect()
}

/// T(i) in a multiplayer game with turns of `k` cycles (`net/lockstep.md`, "A command issued mid-turn"; traced for
/// Direct Connect, `k` = 12 at speed 8): the entries handed over after record H(i) are read in the frame of the cycle
/// with counter c = H(i) - 2 (the counter is n - 2: the start-up cycles come first; traced, `mp-skirmish-bne-mp-skirmish-
/// nobody-fight-game-1300`, where H = 530 ran in the turn of record 542, not 554) and run in the turn before counter
/// `k` x ceil(c / `k`) + `k`, that is the turn record n = that counter + 2; a player's later commands run one turn
/// after each other.
pub fn predict_turns(input: &[Entry], k: u64) -> Vec<u64> {
    let mut handed = 0;
    let mut last: BTreeMap<u8, u64> = BTreeMap::new();
    input
        .iter()
        .map(|e| {
            handed = handed.max(e.n.unwrap_or(0));
            let c = handed.saturating_sub(2);
            let first = k * c.div_ceil(k) + k + 2;
            let t = last.get(&e.player).map_or(first, |&t| first.max(t + k));
            last.insert(e.player, t);
            // The entries after a load count `n` from the loaded game (the load's own `n` is in the game before).
            if matches!(e.kind, Kind::Load { .. }) {
                handed = 0;
                last.clear();
            }
            t
        })
        .collect()
}

/// Checks the header's `input` against the commands the `turn` records list.
#[derive(Clone, Debug, Default)]
pub struct Timing {
    input: Vec<(u64, Entry)>,
    /// Per entry of `input`, the number of `load` entries before it: the game it belongs to.
    games: Vec<usize>,
    /// The game the turn records are in now: the number of loads seen.
    game: usize,
    /// Per player, the input entries (indices) of the current game not yet seen in a turn.
    waiting: BTreeMap<u8, VecDeque<usize>>,
    /// Input entries seen in their predicted turn.
    pub on_time: usize,
    /// Where the input and the turn records disagree.
    pub disagree: Vec<String>,
    /// Commands the turn records list that aren't in the input (the game queued them itself).
    pub extra: Vec<String>,
    /// The input has typed words or fog switches: the game queues `cheat` and `other` commands for them that no
    /// input entry names, so those aren't listed as extra.
    menu_driven: bool,
    /// A multiplayer launch: the header's `input` lists only the traced player's commands (trace format, "Header").
    /// The commands of the second human are checked instead (trace format, "The second human").
    second_human: Option<SecondHuman>,
    /// Commands of the second human that break the trace format's check, or come under a player number that is neither
    /// the local player nor the second human.
    pub second_human_errors: Vec<String>,
}

/// What a comparer checks of the second human's commands (trace format, "The second human", "What a comparer does").
#[derive(Clone, Debug, Default)]
struct SecondHuman {
    /// The traced player.
    local: u8,
    /// The second human: the `controllers` entry that is `"human"` and isn't `local`; `None` when there is none.
    other: Option<u8>,
    /// The smallest `n` of a trigger (a `peer` input entry with action `key`, `click`, `hold` or `desync`); a command
    /// of the second human must come in a turn with a greater `n`. `None` when there is no trigger.
    first_trigger: Option<u64>,
}

impl Timing {
    #[cfg(test)]
    pub fn new(input: &[Entry]) -> Timing {
        Timing::with_turn_length(input, None)
    }

    /// As [`Timing::new`]; with `turn_length` (a multiplayer game) the turns follow [`predict_turns`], the harness's
    /// `peer` entries have no turn, and the game's own `other` and `cheat` commands are not listed as extra.
    pub fn with_turn_length(input: &[Entry], turn_length: Option<u64>) -> Timing {
        let input: Vec<Entry> = input
            .iter()
            .filter(|e| !matches!(e.kind, Kind::Peer { .. }))
            .cloned()
            .collect();
        let input = &input[..];
        // A save, load, typed word or fog switch is a menu action: no turn lists it, so it has no turn to check.
        let menu = |e: &Entry| {
            matches!(
                e.kind,
                Kind::Save { .. } | Kind::Load { .. } | Kind::Say { .. } | Kind::Fog { .. }
            )
        };
        let mut loads = 0;
        let games: Vec<usize> = input
            .iter()
            .map(|e| {
                loads += usize::from(matches!(e.kind, Kind::Load { .. }));
                loads
            })
            .collect();
        let timed: Vec<((u64, Entry), usize)> = turn_length
            .map_or_else(|| predict(input), |k| predict_turns(input, k))
            .into_iter()
            .zip(input.iter().cloned())
            .zip(games)
            .filter(|((_, e), _)| !menu(e))
            .collect();
        let mut t = Timing {
            games: timed.iter().map(|(_, g)| *g).collect(),
            input: timed.into_iter().map(|(te, _)| te).collect(),
            menu_driven: turn_length.is_some()
                || input
                    .iter()
                    .any(|e| matches!(e.kind, Kind::Say { .. } | Kind::Fog { .. })),
            ..Default::default()
        };
        t.queue_game();
        t
    }

    /// Queues the current game's input entries to be seen.
    fn queue_game(&mut self) {
        self.waiting.clear();
        for (i, (_, e)) in self.input.iter().enumerate() {
            if self.games[i] == self.game {
                self.waiting.entry(e.player).or_default().push_back(i);
            }
        }
    }

    /// A loaded `setup` record: the turn records count `n` from the loaded game now, and the input entries after the
    /// `load` with them. The entries of the game before are checked as at the end of a trace (`last` is the last `n`
    /// of that game).
    pub fn loaded(&mut self, last: u64) {
        self.finish(last);
        self.game += 1;
        self.queue_game();
    }

    /// Turns on the check of the second human's commands for a multiplayer launch: `input` is the header's whole
    /// `input` (the triggers are its `peer` entries), `local` the traced player and `controllers` the header's.
    pub fn check_second_human(&mut self, input: &[Entry], local: u8, controllers: &[String]) {
        let other = controllers
            .iter()
            .enumerate()
            .position(|(p, c)| c == "human" && p != local as usize)
            .map(|p| p as u8);
        let first_trigger = input
            .iter()
            .filter(|e| {
                matches!(&e.kind, Kind::Peer { action } if matches!(action.as_str(), "key" | "click" | "hold" | "desync"))
            })
            .filter_map(|e| e.n)
            .min();
        self.second_human = Some(SecondHuman {
            local,
            other,
            first_trigger,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.input.is_empty()
    }

    /// The commands of the turn record `n`, in the order they ran.
    pub fn turn(&mut self, n: u64, commands: &[Entry]) {
        for c in commands {
            let queue = self.waiting.entry(c.player).or_default();
            match queue.front() {
                Some(&i) if self.input[i].1.same_command(c) => {
                    queue.pop_front();
                    let (t, _) = self.input[i];
                    if t == n {
                        self.on_time += 1;
                    } else {
                        self.disagree
                            .push(format!("input entry {} ({c}) ran in turn {n}, T(i) = {t}", i + 1));
                    }
                }
                _ if self.second_human.as_ref().is_some_and(|b| c.player != b.local) => {
                    self.check_other_player(n, c);
                }
                _ if self.menu_driven && matches!(c.kind, Kind::Cheat { .. } | Kind::Other) => {}
                _ => self.extra.push(format!("turn {n}: {c}")),
            }
        }
    }

    /// A command of a player that isn't the traced one, in a multiplayer launch: only the second human may give
    /// one, and only in a turn after some trigger.
    fn check_other_player(&mut self, n: u64, c: &Entry) {
        let Some(b) = &self.second_human else { return };
        if Some(c.player) != b.other {
            self.second_human_errors.push(format!(
                "turn {n}: {c} (player {} is neither the local player nor the second human)",
                c.player
            ));
        } else if !b.first_trigger.is_some_and(|t| n > t) {
            self.second_human_errors.push(match b.first_trigger {
                Some(t) => format!("turn {n}: {c} (the first trigger is at n={t})"),
                None => format!("turn {n}: {c} (the trace has no trigger)"),
            });
        }
    }

    /// After the last record compared (`n`): input entries due by then that no turn listed.
    pub fn finish(&mut self, last: u64) {
        let mut missing: Vec<usize> = self.waiting.values().flatten().copied().collect();
        missing.sort();
        for i in missing {
            let (t, e) = &self.input[i];
            if *t <= last {
                self.disagree.push(format!(
                    "input entry {} ({e}) due in turn {t} isn't listed by n={last}",
                    i + 1
                ));
            }
        }
    }

    /// Input entries due after `last`.
    pub fn not_reached(&self, last: u64) -> usize {
        self.waiting
            .values()
            .flatten()
            .filter(|&&i| self.input[i].0 > last)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: Option<u64>, player: u8, kind: Kind) -> Entry {
        Entry { n, player, kind }
    }

    fn select(n: u64, units: &[u16]) -> Entry {
        entry(Some(n), 1, Kind::Select { units: units.to_vec() })
    }

    fn attack(n: u64, target: u16) -> Entry {
        entry(
            Some(n),
            1,
            Kind::Command {
                command: 8,
                tile: None,
                target: Some(target),
            },
        )
    }

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    fn cheat_add(n: u64, add: &[&str]) -> Entry {
        entry(Some(n), 1, Kind::CheatAdd { add: names(add) })
    }

    /// Round-91 `say` and `fog` input entries are menu actions: no turn lists them, and the cheat and other commands
    /// the game queues for them are not extra.
    #[test]
    fn typed_words_and_fog_switches_are_menu_actions() {
        let say = Entry::parse(&serde_json::json!({"n": 10, "player": 1, "kind": "say", "word": "showpath"})).unwrap();
        let fog = Entry::parse(&serde_json::json!({"n": 12, "player": 1, "kind": "fog", "value": "on"})).unwrap();
        assert_eq!(
            say.kind,
            Kind::Say {
                word: "showpath".into()
            }
        );
        assert_eq!(fog.kind, Kind::Fog { on: true });
        let mut t = Timing::new(&[say, fog]);
        t.turn(
            12,
            &[
                entry(None, 1, Kind::Other),
                entry(
                    None,
                    1,
                    Kind::Cheat {
                        cheats: names(&["showpath"]),
                    },
                ),
            ],
        );
        t.finish(400);
        assert!(
            t.disagree.is_empty() && t.extra.is_empty(),
            "{:?} {:?}",
            t.disagree,
            t.extra
        );
    }

    /// Trace format, "Cheat commands", and its Human06 vector: the input entry adds build speed and resources to the
    /// header's mission select, and the turn lists the whole new set.
    #[test]
    fn cheat_entries_parse_and_match_their_turn() {
        let input = r#"{"n":10,"player":1,"kind":"cheat","add":["make it so","glittering prizes"]}"#;
        let ran = r#"{"player":1,"kind":"cheat","cheats":["make it so","glittering prizes","tigerlily"]}"#;
        let input = Entry::parse(&serde_json::from_str(input).unwrap()).unwrap();
        let ran = Entry::parse(&serde_json::from_str(ran).unwrap()).unwrap();
        assert_eq!(input, cheat_add(10, &["make it so", "glittering prizes"]));
        for e in [&input, &ran] {
            assert_eq!(&Entry::parse(&e.to_json()).unwrap(), e);
        }
        assert!(input.same_command(&ran));
        // The set's order means nothing; a missing flag doesn't match.
        let reordered = entry(
            None,
            1,
            Kind::Cheat {
                cheats: names(&["tigerlily", "make it so", "glittering prizes"]),
            },
        );
        assert!(ran.same_command(&reordered));
        assert!(!cheat_add(10, &["valdez"]).same_command(&ran));
        let Kind::Cheat { cheats } = &ran.kind else { panic!() };
        assert_eq!(
            cheat_flags(cheats),
            flag::BUILD_SPEED | flag::RESOURCES | flag::MISSION_SELECT
        );
        assert_eq!(
            ran.to_string(),
            r#"player 1 cheat ["make it so", "glittering prizes", "tigerlily"]"#
        );
        assert!(Entry::parse(&serde_json::json!({"player": 1, "kind": "cheat"})).is_err());
    }

    /// Every name of the trace format's "Cheat commands" ("Names"), and the ones with no flag.
    #[test]
    fn cheat_names() {
        for (name, f) in [
            ("every little thing she does", flag::ALL_SPELLS),
            ("glittering prizes", flag::RESOURCES),
            ("make it so", flag::BUILD_SPEED),
            ("tigerlily", flag::MISSION_SELECT),
            ("valdez", flag::OIL),
            ("ucla", flag::NOTICE_C),
            ("notice A", flag::NOTICE_A),
            ("notice B", flag::NOTICE_B),
            ("noglues", flag::NO_GLUES),
            ("title", flag::SKIP_TITLE),
            ("showpath", flag::SHOW_PATH),
            ("disco", flag::DISCO),
        ] {
            assert_eq!(cheat_flag(name), Some(f), "{name}");
        }
        for name in ["unnamed 3", "day", "183", "fastdemo", "Make It So"] {
            assert_eq!(cheat_flag(name), None, "{name}");
        }
        assert_eq!(cheat_flags(&names(&["unnamed 0", "disco"])), flag::DISCO);
    }

    #[test]
    fn parses_and_writes_every_kind() {
        let lines = [
            r#"{"n":10,"player":1,"kind":"select","units":[7,9]}"#,
            r#"{"player":1,"kind":"command","command":8,"tile":[2,3],"target":null}"#,
            r#"{"n":4,"player":0,"kind":"command","command":3,"target":12}"#,
            r#"{"player":1,"kind":"build","type":60,"tile":[5,8]}"#,
            r#"{"player":1,"kind":"job","job":"upgrade building","number":88}"#,
            r#"{"player":1,"kind":"job","job":7,"number":1}"#,
            r#"{"player":2,"kind":"other"}"#,
            r#"{"player":1,"kind":"cancel"}"#,
            r#"{"player":1,"kind":"cancel-job"}"#,
        ];
        for l in lines {
            let e = Entry::parse(&serde_json::from_str(l).unwrap()).unwrap();
            assert_eq!(Entry::parse(&e.to_json()).unwrap(), e, "{l}");
        }
        let e = Entry::parse(&serde_json::from_str(lines[2]).unwrap()).unwrap();
        assert_eq!(
            e.kind,
            Kind::Command {
                command: 3,
                tile: None,
                target: Some(12)
            }
        );
        let e = Entry::parse(&serde_json::from_str(lines[5]).unwrap()).unwrap();
        assert_eq!(
            e.kind,
            Kind::Job {
                job: Job::Other(7),
                number: 1
            }
        );
        assert_eq!(e.to_string(), "player 1 job 7 1");
    }

    /// Trace format, "When a command runs" and its test vectors.
    #[test]
    fn predicts_the_turn_of_each_entry() {
        // Four entries handed over together run one per turn.
        let human01 = [select(10, &[1]), attack(10, 2), select(10, &[3]), select(10, &[4])];
        assert_eq!(predict(&human01), [11, 12, 13, 14]);
        // Human06: a cheat with n = 10, then six entries with n = 20, run in turns 11 and 21 to 26.
        let mut human06 = vec![cheat_add(10, &["make it so", "glittering prizes"])];
        for _ in 0..6 {
            human06.push(select(20, &[1]));
        }
        assert_eq!(predict(&human06), [11, 21, 22, 23, 24, 25, 26]);
        // Entries handed over before the queue empties wait their turn.
        let queued = [select(10, &[1]), attack(10, 2), select(11, &[3]), attack(30, 4)];
        assert_eq!(predict(&queued), [11, 12, 13, 31]);
        // Each player has its own sequence.
        let mut two = vec![select(10, &[1]), select(10, &[2])];
        two[1].player = 0;
        assert_eq!(predict(&two), [11, 11]);
    }

    #[test]
    fn multiplayer_turns_follow_the_turn_length() {
        // Traced (`mp-skirmish-bne-mp-skirmish-nobody-fight-game-600`): a select and a move handed over after record 10
        // ran in the turns of records 26 and 38 with turns of 12 cycles; the harness's peer entries have no turn.
        let input = [select(10, &[1]), attack(10, 2), peer(205, "key")];
        assert_eq!(predict_turns(&input[..2], 12), [26, 38]);
        let mut t = Timing::with_turn_length(&input, Some(12));
        t.check_second_human(&input, 1, &controllers(&["nobody", "human", "human"]));
        let ran = |e: &Entry| Entry { n: None, ..e.clone() };
        t.turn(26, &[ran(&input[0])]);
        t.turn(38, &[ran(&input[1])]);
        // The second human's command after the trigger is fine.
        t.turn(230, &[entry(None, 2, Kind::Other)]);
        t.finish(400);
        assert_eq!(
            (t.on_time, t.disagree.len(), t.extra.len()),
            (2, 0, 0),
            "{:?}",
            t.disagree
        );
        assert!(t.second_human_errors.is_empty(), "{:?}", t.second_human_errors);
    }

    fn peer(n: u64, action: &str) -> Entry {
        entry(
            Some(n),
            1,
            Kind::Peer {
                action: action.to_string(),
            },
        )
    }

    fn controllers(c: &[&str]) -> Vec<String> {
        names(c)
    }

    /// A multiplayer launch with the traced player 1 and the second human 2.
    fn launch(input: &[Entry]) -> Timing {
        let mut t = Timing::with_turn_length(input, Some(12));
        t.check_second_human(input, 1, &controllers(&["nobody", "human", "human", "computer"]));
        t
    }

    /// Trace format, "The second human": a command of the second human must come in a turn after some trigger (a `peer`
    /// entry with action `key`, `click`, `hold` or `desync`).
    #[test]
    fn the_second_humans_commands_need_an_earlier_trigger() {
        let b_select = entry(None, 2, Kind::Select { units: vec![7] });
        // No trigger at all: any command of the second human is a difference.
        let mut t = launch(&[]);
        t.turn(100, std::slice::from_ref(&b_select));
        assert_eq!(t.second_human_errors.len(), 1, "{:?}", t.second_human_errors);
        // A trigger at n = 60 and commands in later turns: fine, whatever the delay.
        let mut t = launch(&[peer(60, "click")]);
        t.turn(61, std::slice::from_ref(&b_select));
        t.turn(500, &[entry(None, 2, Kind::Other)]);
        assert!(t.second_human_errors.is_empty(), "{:?}", t.second_human_errors);
        // A command in the trigger's own turn, or before it, is a difference.
        t.turn(60, std::slice::from_ref(&b_select));
        assert_eq!(t.second_human_errors.len(), 1);
        // Actions that can't make a command aren't triggers.
        let mut t = launch(&[peer(10, "shot"), peer(10, "own-key"), peer(10, "own-click")]);
        t.turn(100, std::slice::from_ref(&b_select));
        assert_eq!(t.second_human_errors.len(), 1);
        // Hold, desync and key are triggers.
        for action in ["hold", "desync", "key"] {
            let mut t = launch(&[peer(10, action)]);
            t.turn(11, std::slice::from_ref(&b_select));
            assert!(t.second_human_errors.is_empty(), "{action}");
        }
    }

    /// Trace format, "Saves, loads, cheat words and fog": input entries after a `load` count `n` from the loaded game,
    /// and the turn records after the loaded `setup` record do too.
    #[test]
    fn input_after_a_load_counts_from_the_loaded_game() {
        let load = entry(Some(30), 1, Kind::Load { name: "s".into() });
        let input = [select(10, &[1]), load, select(5, &[2])];
        assert_eq!(predict(&input), [11, 31, 6]);
        let mut t = Timing::new(&input);
        let ran = |e: &Entry| Entry { n: None, ..e.clone() };
        t.turn(11, &[ran(&input[0])]);
        t.loaded(40);
        t.turn(6, &[ran(&input[2])]);
        t.finish(100);
        assert_eq!(
            (t.on_time, t.disagree.len(), t.extra.len()),
            (2, 0, 0),
            "{:?}",
            t.disagree
        );
        // An entry before the load that no turn listed is reported when the load comes.
        let mut t = Timing::new(&input);
        t.loaded(40);
        assert_eq!(t.disagree.len(), 1);
    }

    #[test]
    fn a_command_under_another_player_number_is_a_difference() {
        let mut t = launch(&[peer(10, "key")]);
        // Player 3 is a computer player, player 5 is nobody: neither gives a command.
        t.turn(50, &[entry(None, 3, Kind::Other), entry(None, 5, Kind::Other)]);
        assert_eq!(t.second_human_errors.len(), 2, "{:?}", t.second_human_errors);
        // The traced player's commands are still only its input entries.
        let mut t = launch(&[peer(10, "key")]);
        t.turn(50, &[entry(None, 1, Kind::Select { units: vec![4] })]);
        assert_eq!(t.extra.len(), 1);
    }

    #[test]
    fn checks_turns_against_the_input() {
        let input = [select(10, &[1]), attack(10, 2), select(10, &[3])];
        let mut t = Timing::new(&input);
        let ran = |e: &Entry| Entry { n: None, ..e.clone() };
        t.turn(11, &[ran(&input[0])]);
        // The turn gives the tile the input left out.
        let mut cmd = ran(&input[1]);
        if let Kind::Command { tile, .. } = &mut cmd.kind {
            *tile = Some((4, 5));
        }
        t.turn(12, &[cmd]);
        // A command the game queued itself, then the last entry one turn late.
        t.turn(13, &[ran(&select(0, &[9]))]);
        t.turn(14, &[ran(&input[2])]);
        t.finish(20);
        assert_eq!(t.on_time, 2);
        assert_eq!(t.extra, ["turn 13: player 1 select [9]"]);
        assert_eq!(
            t.disagree,
            ["input entry 3 (player 1 select [3]) ran in turn 14, T(i) = 13"]
        );

        let mut t = Timing::new(&input);
        t.turn(11, &[ran(&input[0])]);
        t.finish(12);
        assert_eq!(t.disagree.len(), 1, "{:?}", t.disagree);
        assert!(t.disagree[0].contains("due in turn 12"), "{:?}", t.disagree);
        assert_eq!(t.not_reached(12), 1);
    }
}
