//! `twtool snapshot`: a whole-map behaviour snapshot. Every campaign and custom map of an installation runs headless
//! in each mode for a number of cycles, and a fingerprint of the simulation is printed every so many cycles, one line
//! per map, mode and checkpoint. The committed file (`crates/twtool/tests/snapshots/sim.txt`) pins today's behaviour,
//! so a refactor that should not change behaviour can show that it doesn't (`tests/snapshot.rs`).
//!
//! **Modes.** `move` sends all the local player's units to the map's centre, `fight` each one against the nearest
//! unit it can attack and `harvest` each worker to the nearest mine or tree and each tanker to its side's nearest
//! oil platform (the orders `twtool sim` gives), all on the map's normal start (a campaign map as its mission, any
//! other map as a custom game at the defaults). `ai`, for non-campaign maps only, starts a custom game with as many
//! computer opponents as the map has room for (`custom:opponents=7`) and gives no orders, so the computer players
//! build up and attack an idle local player. Computer players are always allied with each other (setup spec,
//! "Starting alliances and shared vision"), so they never fight each other. Every run uses Enhanced rules and seed 0.
//!
//! **Line.** `map<TAB>mode<TAB>cycle<TAB>behaviour<TAB>world<TAB>units`, after header lines starting with `#`.
//! Cycle 0 is the state after set-up, before any order. A map that fails to start or a run that panics gives one
//! line with `error` and the message instead.
//!
//! - `behaviour` is FNV-1a over what can be observed, chosen not to depend on how the simulation stores it: the
//!   oracle's record (`oracle::snapshot`: the generator, every player's gold, lumber and oil, every unit slot's
//!   compared fields, every effect slot's fields), each player's counters, levels and per-player words, alliances
//!   and outcome, the terrain's tile, region and wall-damage words, the local player's explored and visible maps, and
//!   everything the simulation told the client since the previous checkpoint (messages, sound cues, event places).
//! - `world` is [`tidewater_sim::World::fingerprint`]: the whole stored state, which changes when a stored field is
//!   renamed, added or reordered even if behaviour does not.
//! - `units` is the number of live units, to make a diff easier to read.

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use tidewater_formats::install::Install;
use tidewater_formats::pud::Pud;
use tidewater_sim::World;
use tidewater_sim::setup::{Setup, Start};

use crate::oracle::{self, Cell};

/// The ways a map is run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Move,
    Fight,
    Harvest,
    Ai,
}

pub const MODES: [Mode; 4] = [Mode::Move, Mode::Fight, Mode::Harvest, Mode::Ai];

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Move => "move",
            Mode::Fight => "fight",
            Mode::Harvest => "harvest",
            Mode::Ai => "ai",
        }
    }

    pub fn parse(s: &str) -> Result<Mode> {
        MODES
            .into_iter()
            .find(|m| m.name() == s)
            .ok_or_else(|| anyhow::anyhow!("unknown mode {s:?} (move, fight, harvest or ai)"))
    }
}

/// What to run.
pub struct Options {
    pub cycles: u32,
    pub every: u32,
    pub jobs: usize,
    pub modes: Vec<Mode>,
    /// Only maps whose name contains this (case-insensitive).
    pub filter: Option<String>,
    /// Print each part of the behaviour fingerprint after the line.
    pub parts: bool,
}

/// Campaign maps by folder, file prefix and count, and the attract-mode maps.
const CAMPAIGNS: [(&str, &str, u8); 4] = [
    ("Human", "Human", 14),
    ("Orc", "Orc", 14),
    ("XHuman", "2XHum", 12),
    ("XOrc", "2XOrc", 12),
];

/// The maps of an installation: the campaign and attract-mode maps that its archives (or loose files) hold, then
/// every `.pud` under its `Maps` folder, by name.
pub fn maps(root: &Path, inst: &mut Install) -> Result<Vec<String>> {
    let mut out: Vec<String> = CAMPAIGNS
        .iter()
        .flat_map(|&(dir, prefix, n)| (1..=n).map(move |k| format!("Campaign\\{dir}\\{prefix}{k:02}.pud")))
        .chain((1..=9).map(|k| format!("OrigMaps\\Attract\\ATTRCT{k:02}.PUD")))
        .filter(|name| inst.read(name).is_ok())
        .collect();
    let mut loose = Vec::new();
    let dir = std::fs::read_dir(root)?
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("maps") && e.path().is_dir());
    if let Some(dir) = dir {
        walk(&dir.path(), &dir.file_name().to_string_lossy(), &mut loose)?;
    }
    loose.sort();
    out.extend(loose);
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let name = format!("{prefix}\\{}", e.file_name().to_string_lossy());
        if e.path().is_dir() {
            walk(&e.path(), &name, out)?;
        } else if name.to_ascii_lowercase().ends_with(".pud") {
            out.push(name);
        }
    }
    Ok(())
}

/// FNV-1a, 64 bits.
#[derive(Clone, Copy)]
struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 = (self.0 ^ x as u64).wrapping_mul(0x100_0000_01b3);
        }
    }
    fn int(&mut self, n: i64) {
        self.bytes(&n.to_le_bytes());
    }
    fn ints<T: Copy + Into<i64>>(&mut self, v: &[T]) {
        self.int(v.len() as i64);
        for &x in v {
            self.int(x.into());
        }
    }
    fn cells(&mut self, v: &[Cell]) {
        self.int(v.len() as i64);
        for c in v {
            match c {
                Cell::Int(n) => {
                    self.bytes(b"i");
                    self.int(*n);
                }
                Cell::Null => self.bytes(b"n"),
                Cell::Text(t) => {
                    self.bytes(b"t");
                    self.bytes(t.as_bytes());
                }
                Cell::Skip(_) => self.bytes(b"s"),
            }
        }
    }
}

/// The parts of the behaviour fingerprint, in the order they are combined.
const PARTS: [&str; 5] = ["oracle", "players", "terrain", "vision", "told"];

/// Each part of the behaviour fingerprint. `told` is the running hash of what the client was told.
fn parts(w: &World, told: u64) -> [u64; 5] {
    let mut o = Fnv::new();
    let r = oracle::snapshot(w, 0, "cycle");
    o.int(r.rng as i64);
    for list in &r.resources {
        o.cells(list);
    }
    for map in [&r.units, &r.effects] {
        o.int(map.len() as i64);
        for (&slot, cells) in map {
            o.int(slot as i64);
            o.cells(cells);
        }
    }
    let mut p = Fnv::new();
    p.int(w.tick as i64);
    for q in &w.players {
        p.ints(&[
            q.kills,
            q.razings,
            q.score,
            q.units_lost,
            q.buildings_lost,
            q.gathered[0],
            q.gathered[1],
            q.gathered[2],
            q.units_allowed,
            q.spells_known,
            q.spells_allowed,
            q.spells_researching,
            q.upgrades_allowed,
            q.upgrades_researching,
        ]);
        p.ints(&q.levels);
        p.ints(&[q.allies, q.rescued, q.converted, q.under_construction, q.in_training]);
        p.ints(&[q.controller, q.shared_vision, q.had_hall as u8, q.race]);
        p.bytes(format!("{:?}", q.result).as_bytes());
    }
    let mut t = Fnv::new();
    let ter = &w.terrain;
    t.ints(&ter.words);
    t.ints(&ter.tiles);
    t.ints(&ter.regions);
    t.ints(&ter.wall_damage);
    let mut v = Fnv::new();
    v.ints(w.vision.flags(tidewater_sim::vision::Map::Explored));
    v.ints(w.vision.flags(tidewater_sim::vision::Map::Visible));
    [o.0, p.0, t.0, v.0, told]
}

/// Folds what the simulation told the client this cycle into `told` and empties the queues, as the client does.
fn drain(w: &mut World, told: &mut Fnv) {
    if !w.events.is_empty() {
        told.bytes(format!("{:?}", w.events).as_bytes());
        w.events.clear();
    }
    if !w.sounds.cues.is_empty() {
        told.bytes(format!("{:?}", w.sounds.cues).as_bytes());
        w.sounds.cues.clear();
    }
    if !w.sounds.events.is_empty() {
        told.bytes(format!("{:?}", w.sounds.events).as_bytes());
        w.sounds.events.clear();
    }
}

/// Runs one map in one mode and returns its lines.
fn run(inst: &mut Install, map: &str, mode: Mode, opts: &Options) -> Result<Vec<String>> {
    let pud = Pud::parse(&inst.read(map)?)?;
    let setup = match mode {
        Mode::Ai => Setup::parse("custom:opponents=7").map_err(anyhow::Error::msg)?,
        _ => Setup::for_map(map),
    };
    let start = Start::choose(&pud, &setup, 0)?;
    let mut w = crate::start_world(inst, &pud, &start)?;
    let player = start.local;
    let mut told = Fnv::new();
    let mut lines = Vec::new();
    let mut line = |w: &World, cycle: u32, told: &Fnv| {
        let parts = parts(w, told.0);
        let mut b = Fnv::new();
        for p in parts {
            b.int(p as i64);
        }
        let mut s = format!(
            "{map}\t{}\t{cycle}\t{:016x}\t{:016x}\t{}",
            mode.name(),
            b.0,
            w.fingerprint(),
            w.units.len()
        );
        if opts.parts {
            for (name, p) in PARTS.iter().zip(parts) {
                s.push_str(&format!("\t{name}={p:016x}"));
            }
        }
        lines.push(s);
    };
    drain(&mut w, &mut told);
    line(&w, 0, &told);
    let orders = match mode {
        Mode::Ai => Vec::new(),
        Mode::Move | Mode::Fight | Mode::Harvest => {
            let sim_mode = match mode {
                Mode::Fight => crate::SimMode::Fight,
                Mode::Harvest => crate::SimMode::Harvest,
                _ => crate::SimMode::Move,
            };
            let centre = (w.terrain.width as i32 / 2, w.terrain.height as i32 / 2);
            let units = crate::own_units(&w, player);
            crate::sim_orders(&w, player, &units, sim_mode, centre)
        }
    };
    for cycle in 1..=opts.cycles {
        w.step(if cycle == 1 { &orders } else { &[] });
        drain(&mut w, &mut told);
        if cycle % opts.every == 0 {
            line(&w, cycle, &told);
        }
    }
    Ok(lines)
}

/// Runs every map in every mode that applies, in parallel, and returns the lines in map then mode order.
pub fn snapshot(root: &Path, opts: &Options) -> Result<Vec<String>> {
    anyhow::ensure!(opts.every > 0, "--every must be at least 1");
    let mut inst = Install::open(root)?;
    let filter = opts.filter.as_ref().map(|f| f.to_ascii_lowercase());
    let maps: Vec<String> = maps(root, &mut inst)?
        .into_iter()
        .filter(|m| filter.as_ref().is_none_or(|f| m.to_ascii_lowercase().contains(f)))
        .collect();
    let jobs: Vec<(&str, Mode)> = maps
        .iter()
        .flat_map(|m| {
            let campaign = tidewater_sim::setup::campaign_entry(m).is_some();
            opts.modes
                .iter()
                .filter(move |&&mode| !(campaign && mode == Mode::Ai))
                .map(move |&mode| (m.as_str(), mode))
        })
        .collect();
    let results: Mutex<Vec<Option<Vec<String>>>> = Mutex::new(vec![None; jobs.len()]);
    let next = AtomicUsize::new(0);
    let threads = opts.jobs.max(1).min(jobs.len().max(1));
    std::thread::scope(|s| -> Result<()> {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| -> Result<()> {
                    let mut inst = Install::open(root)?;
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&(map, mode)) = jobs.get(i) else {
                            return Ok(());
                        };
                        let run =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut inst, map, mode, opts)));
                        let lines = match run {
                            Ok(Ok(lines)) => lines,
                            Ok(Err(e)) => vec![format!("{map}\t{}\terror\t{e:#}", mode.name())],
                            Err(p) => {
                                let msg = p
                                    .downcast_ref::<String>()
                                    .cloned()
                                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                                    .unwrap_or_default();
                                // A panic can leave the archives mid-read: start afresh.
                                inst = Install::open(root)?;
                                vec![format!("{map}\t{}\terror\tpanic: {msg}", mode.name())]
                            }
                        };
                        results.lock().expect("no worker panics holding the lock")[i] = Some(lines);
                    }
                })
            })
            .collect();
        for w in workers {
            w.join().expect("worker thread")?;
        }
        Ok(())
    })?;
    let modes: Vec<&str> = opts.modes.iter().map(|m| m.name()).collect();
    let mut out = vec![
        "# twtool snapshot: map, mode, cycle, behaviour fingerprint, world fingerprint, live units".to_string(),
        format!(
            "# cycles {} every {} modes {} seed 0 rules enhanced",
            opts.cycles,
            opts.every,
            modes.join(",")
        ),
    ];
    for lines in results.into_inner().expect("workers done") {
        out.extend(lines.expect("every job ran"));
    }
    Ok(out)
}
