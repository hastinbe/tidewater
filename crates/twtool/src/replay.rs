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
//! Our reading where the format is open: a `build` needs the first selected unit to be a worker, peasant or peon
//! (type 0x02 or 0x03, public PUD specification, appendix A), whichever race; "its race's worker" is read as the
//! unit's own race.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use tidewater_sim::{Command, World};

/// Unit types of the two workers (public PUD specification, appendix A).
const PEASANT: u8 = 0x02;
const PEON: u8 = 0x03;

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
            Kind::Other => f.write_str("other"),
        }
    }
}

/// The player command our simulation runs for a trace command, as the world stands when it runs; `None` when the
/// command does nothing there (no first selected unit, a builder that isn't a worker, a kind we can't express).
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
            if !w.unit(unit).is_some_and(|u| u.kind == PEASANT || u.kind == PEON) {
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
        Kind::Other => return None,
    })
}

/// The turn each input entry runs in, T(i) of "When a command runs": an entry is handed over after record H(i), the
/// largest `n` among it and the entries before it, and runs in the turn after that, one per player per turn.
pub fn predict(input: &[Entry]) -> Vec<u64> {
    let mut handed = 0;
    let mut last: BTreeMap<u8, u64> = BTreeMap::new();
    input
        .iter()
        .map(|e| {
            handed = handed.max(e.n.unwrap_or(0));
            let t = last.get(&e.player).map_or(handed + 1, |&t| (handed + 1).max(t + 1));
            last.insert(e.player, t);
            t
        })
        .collect()
}

/// Checks the header's `input` against the commands the `turn` records list.
#[derive(Clone, Debug, Default)]
pub struct Timing {
    input: Vec<(u64, Entry)>,
    /// Per player, the input entries (indices) not yet seen in a turn.
    waiting: BTreeMap<u8, VecDeque<usize>>,
    /// Input entries seen in their predicted turn.
    pub on_time: usize,
    /// Where the input and the turn records disagree.
    pub disagree: Vec<String>,
    /// Commands the turn records list that aren't in the input (the game queued them itself).
    pub extra: Vec<String>,
}

impl Timing {
    pub fn new(input: &[Entry]) -> Timing {
        let mut t = Timing {
            input: predict(input).into_iter().zip(input.iter().cloned()).collect(),
            ..Default::default()
        };
        for (i, (_, e)) in t.input.iter().enumerate() {
            t.waiting.entry(e.player).or_default().push_back(i);
        }
        t
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
                _ => self.extra.push(format!("turn {n}: {c}")),
            }
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
        // An entry whose n is smaller than an earlier one's is handed over late, after the larger n.
        let mut orc02 = vec![select(4700, &[1]), attack(4700, 2), select(5050, &[1]), attack(5050, 2)];
        for n in [4000, 4000, 4350, 4350, 4700, 4700, 5050, 5050] {
            orc02.push(select(n, &[3]));
        }
        assert_eq!(
            predict(&orc02),
            [4701, 4702, 5051, 5052, 5053, 5054, 5055, 5056, 5057, 5058, 5059, 5060]
        );
        // Each player has its own sequence.
        let mut two = vec![select(10, &[1]), select(10, &[2])];
        two[1].player = 0;
        assert_eq!(predict(&two), [11, 11]);
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
