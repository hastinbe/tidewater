//! Runs `twtool oracle` on real traces of the original game (`docs/specs/oracle/trace-format.md`), when they are on
//! this machine: set `TIDEWATER_ORACLE_TRACES` to the directory holding the trace files and `TIDEWATER_INSTALL` to the
//! game's installation. Without both, the test does nothing. Each trace must match on every field up to its
//! limit.
//!
//! The format-2 traces carry the player's commands in their `turn` records, which the oracle replays. They must match
//! up to the cycle before their first known gap, and every input entry must run in its predicted turn T(i):
//!
//! - **human01-basics**, n = 70: a worker walking to a build site heads for a footprint tile, where the specs give the
//!   ordered tile as the goal (`movement/pathfinding.md`, "Inputs"; `economy/build-train.md`, "Placing a
//!   building").
//! - **orc02-***, n = 294: a unit chasing in order 12 follows a straight line through its target's tile, as if
//!   order 12 were an attack-type order (`movement/pathfinding.md`, "Units seen by the search", which names only
//!   "attack").
//! - **xorc01-death-coil**, n = 356: a grunt on an attack-move steps to a different tile than ours; not yet
//!   researched.
//! - **xhuman10-***, n = 1 (only the setup record is checked): in the first start-up cycle, idle units of computer
//!   player 2 acquire targets in our run (order 12) but stay idle in the game's, which acquires them only at their
//!   next step done (n = 6). Their idle logic runs on both sides (the draws match); no spec says why the first run
//!   doesn't acquire (`orders/orders.md`, "Automatic target acquisition").
//! - **xorc01-death-knight**, n = 26: a death knight casting haste on a unit in its animation 4 doesn't turn to face
//!   it; facing toward a target from a pixel delta is a known gap (`spells/spells.md` only turns point spells).
//! - **xorc12-ogre-mage**, n = 4: computer player 3's worker builds a scout tower (build list entry 4), which our
//!   game-start done marks have already marked done because of the map's guard and cannon towers, as
//!   `ai/worker-ai.md` ("Build list", done marks) says.
//! - **xorc10-dragons**, n = 7: a computer destroyer on a sea patrol starts at its anchor, its own tile. On arriving
//!   there it swaps ends; the game waits for the next step done (3 cycles) before it moves, ours moves at once.
//!   `orders/orders.md` ("Arriving and losing the target", order 5) says only that it swaps ends and moves again.
//! - **orc09-***, n = 5, and **human06-tech**, n = 46: a worker walking to a build site (order 28) steps to a
//!   different tile; the walk goal of a build order is a known gap (see human01-basics).
//! - **human12-***, n = 11: a computer transport landing (order 30) from an even tile toward an odd goal tile takes
//!   a two-tile step first, then one-tile steps; `orders/transports.md` ("Landing (order 30)", step 2) says it moves
//!   as a two-tile unit only while both tiles are even.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 17] = [
    ("human01-2000.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    ("human04-3000.jsonl.gz", "Campaign\\Human\\Human04.pud", 6, &[], None),
    (
        "human01-basics-9500.jsonl.gz",
        "Campaign\\Human\\Human01.pud",
        0,
        &[],
        Some(69),
    ),
    (
        "orc02-assault-3000.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(293),
    ),
    (
        "orc02-victory-7639.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(293),
    ),
    (
        "orc02-defeat-4021.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(293),
    ),
    (
        "xorc01-death-coil-2500.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(355),
    ),
    (
        "xhuman10-spells-700.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(0),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(0),
    ),
    (
        "xorc01-death-knight-2100.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(25),
    ),
    (
        "xorc12-ogre-mage-180.jsonl.gz",
        "Campaign\\XOrc\\2XOrc12.pud",
        51,
        &[],
        Some(3),
    ),
    (
        "xorc10-dragons-800.jsonl.gz",
        "Campaign\\XOrc\\2XOrc10.pud",
        47,
        &[],
        Some(6),
    ),
    (
        "orc09-naval-1500.jsonl.gz",
        "Campaign\\Orc\\Orc09.pud",
        17,
        &[],
        Some(4),
    ),
    (
        "orc09-repair-600.jsonl.gz",
        "Campaign\\Orc\\Orc09.pud",
        17,
        &[],
        Some(4),
    ),
    (
        "human12-transport-650.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        Some(10),
    ),
    (
        "human12-demolition-450.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        Some(10),
    ),
    (
        "human06-tech-650.jsonl.gz",
        "Campaign\\Human\\Human06.pud",
        10,
        &[],
        Some(45),
    ),
];

#[test]
fn compares_real_traces_when_present() {
    let (Some(dir), Some(install)) = (
        std::env::var_os("TIDEWATER_ORACLE_TRACES"),
        std::env::var_os("TIDEWATER_INSTALL"),
    ) else {
        eprintln!("skipped: set TIDEWATER_ORACLE_TRACES and TIDEWATER_INSTALL to run");
        return;
    };
    let mut ran = 0;
    for (file, map, entry, fields, limit) in TRACES {
        let trace = PathBuf::from(&dir).join(file);
        if !trace.is_file() {
            eprintln!("skipped {file}: not found");
            continue;
        }
        let max = limit.map(|l| l.to_string());
        let short = limit.unwrap_or(300).min(300).to_string();
        let whole: Vec<&str> = max.iter().flat_map(|m| ["--max-cycles", m.as_str()]).collect();
        let resync = vec!["--keep-going", "--resync-rng", "--max-cycles", short.as_str()];
        for extra in [whole, resync] {
            let out = Command::new(env!("CARGO_BIN_EXE_twtool"))
                .arg("oracle")
                .arg("--install")
                .arg(&install)
                .args(["--map", map, "--entry", &entry.to_string()])
                .args(fields)
                .args(&extra)
                .arg(&trace)
                .output()
                .expect("twtool runs");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            // 0: no divergence; 1 with a report: a divergence. An error exits 1 too, but prints no summary.
            assert!(
                matches!(out.status.code(), Some(0 | 1)) && stdout.contains("\ncompared "),
                "{file} {extra:?}: status {:?}\n{stdout}\n{stderr}",
                out.status
            );
            assert!(stdout.contains("divergence"), "{file}: no verdict\n{stdout}");
            assert_eq!(out.status.code(), Some(0), "{file} {extra:?} diverges\n{stdout}");
            assert!(
                !stdout.contains("input timing disagrees") && !stdout.contains("not from the input"),
                "{file}: the turn records and the input disagree\n{stdout}"
            );
            ran += 1;
        }
    }
    eprintln!("ran {ran} comparisons");
}
