//! Runs `twtool oracle` on real traces of the original game (`docs/specs/oracle/trace-format.md`), when they are on
//! this machine: set `TIDEWATER_ORACLE_TRACES` to the directory holding the trace files and `TIDEWATER_INSTALL` to the
//! game's installation. Without both, the test does nothing. Each trace must match on every field up to its
//! limit.
//!
//! The format-2 traces carry the player's commands in their `turn` records, which the oracle replays. They must match
//! up to the cycle before their first known gap, and every input entry must run in its predicted turn T(i). The
//! gaps below are what the specs don't yet explain; none has been researched:
//!
//! - **human01-basics**, n = 575: at n = 576 the game draws twice from the generator where ours draws once.
//! - **orc02-***, n = 585: at n = 586 a grunt (slot 1582) at the end of its route records tile (53, 25), its attack
//!   animation and wait 3 in the game, and tile (54, 24), its move animation and wait 1 in ours (order 12 given
//!   again).
//! - **xorc01-death-coil**, n = 717: an ogre (slot 1496) on an attack-move records order 10 on tile (56, 73) in the
//!   game, and order 12 on (57, 74) in ours, with the same pixel and target.
//! - **xhuman10-***, n = 32: at n = 33 an ogre (slot 1538) records tile x 93, its attack animation and wait 3 in the
//!   game, and x 94, its move animation and wait 1 in ours (order 12 given again at the end of its route).
//! - **xorc01-death-knight**, n = 707: at n = 708 a skeleton (slot 1515) attacks (order 12, target slot 1497) in
//!   the game and is idle in ours.
//! - **xorc10-dragons**, n = 332: at n = 333 a footman (slot 1520) records order 3 with wait 3 in the game, and
//!   idle with wait 1 in ours.
//! - **orc09-naval**, n = 104: at n = 105 a computer unit (slot 1473, type 0x1B) is given a build order (28) in the
//!   game and stays idle in ours.
//! - **human06-tech**, n = 577: at n = 578 a tower (slot 1516, type 0x60) records order 14 in the game and order 33
//!   in ours.
//! - **human12-7000**, n = 5667: at n = 5668 a zeppelin (slot 1503) records order 3 with wait 3 in the game, and
//!   idle with wait 1 in ours.
//! - **orc4**, n = 10594: at n = 10595 a peon (slot 1540) is 4 tiles from where it is in the game.
//! - **human10**, n = 6955, and **xorc7**, n = 1004: a destroyer (slots 1483 and 1423) records order 5 (patrol)
//!   with wait 3 in the game, and idle with wait 1 in ours.
//! - **human5**, n = 1594: at n = 1595 the game has a cosmetic effect (kind 0x17) in slot 3 where ours has none.
//! - **human7**, n = 7443: at n = 7444 an idle computer worker (slot 1582) is on tile (71, 5) facing 3 in the game,
//!   and on (66, 2) facing 1 in ours.
//! - **orc8**, n = 948: at n = 949 a tanker (slot 1479, type 0x1A) records tile y 102, facing 4 in the game, and
//!   y 100, facing 3 in ours.
//! - **xorc3**, n = 2963: at n = 2964 a worker (slot 1471) faces 7 in the game and 1 in ours.
//!
//! xorc12-ogre-mage, orc09-repair, human12-transport and human12-demolition match in full.
//!
//! The other long traces (xhuman2, xhuman4) diverge in their first ten cycles, so they aren't run.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 25] = [
    ("human01-2000.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    ("human04-3000.jsonl.gz", "Campaign\\Human\\Human04.pud", 6, &[], None),
    (
        "human01-basics-9500.jsonl.gz",
        "Campaign\\Human\\Human01.pud",
        0,
        &[],
        Some(575),
    ),
    (
        "orc02-assault-3000.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(585),
    ),
    (
        "orc02-victory-7639.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(585),
    ),
    (
        "orc02-defeat-4021.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(585),
    ),
    (
        "xorc01-death-coil-2500.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(717),
    ),
    (
        "xhuman10-spells-700.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(32),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(32),
    ),
    (
        "xorc01-death-knight-2100.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(707),
    ),
    (
        "xorc12-ogre-mage-180.jsonl.gz",
        "Campaign\\XOrc\\2XOrc12.pud",
        51,
        &[],
        None,
    ),
    (
        "xorc10-dragons-800.jsonl.gz",
        "Campaign\\XOrc\\2XOrc10.pud",
        47,
        &[],
        Some(332),
    ),
    (
        "orc09-naval-1500.jsonl.gz",
        "Campaign\\Orc\\Orc09.pud",
        17,
        &[],
        Some(104),
    ),
    ("orc09-repair-600.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    (
        "human12-transport-650.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        None,
    ),
    (
        "human12-demolition-450.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        None,
    ),
    (
        "human06-tech-650.jsonl.gz",
        "Campaign\\Human\\Human06.pud",
        10,
        &[],
        Some(577),
    ),
    (
        "human12-7000.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        Some(5667),
    ),
    ("orc4-20000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], Some(10594)),
    (
        "human10-10000.jsonl.gz",
        "Campaign\\Human\\Human10.pud",
        18,
        &[],
        Some(6955),
    ),
    (
        "xorc7-7000.jsonl.gz",
        "Campaign\\XOrc\\2XOrc07.pud",
        41,
        &[],
        Some(1004),
    ),
    (
        "human5-10000.jsonl.gz",
        "Campaign\\Human\\Human05.pud",
        8,
        &[],
        Some(1594),
    ),
    (
        "human7-10000.jsonl.gz",
        "Campaign\\Human\\Human07.pud",
        12,
        &[],
        Some(7443),
    ),
    ("orc8-10000.jsonl.gz", "Campaign\\Orc\\Orc08.pud", 15, &[], Some(948)),
    (
        "xorc3-10000.jsonl.gz",
        "Campaign\\XOrc\\2XOrc03.pud",
        33,
        &[],
        Some(2963),
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
