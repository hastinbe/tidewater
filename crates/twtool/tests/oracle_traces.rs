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
//! - **orc02-***, n = 545: at n = 546 a grunt attacking on the move (slot 1578, order 9) records tile (58, 20),
//!   facing 1 and wait 15 in the game (a blocked wait), and tile (59, 20), facing 2 and wait 1 in ours.
//! - **xorc01-death-coil**, n = 717: an ogre (slot 1496) on an attack-move records order 10 on tile (56, 73) in the
//!   game, and order 12 on (57, 74) in ours, with the same pixel and target.
//! - **xhuman10-***, n = 15: at n = 16 a cannon tower's impact effect (kind 0x1A) has destination (0, 0) and no
//!   shooter in the game; no spec gives an impact effect's destination or shooter.
//! - **xorc01-death-knight**, n = 437: an ogre on an attack-move (slot 1489, order 10) records tile (67, 77) in the
//!   game and (68, 78) in ours, at the same pixel.
//! - **xorc12-ogre-mage**, n = 76: at n = 77 a runes marker (kind 0x0B) has destination (0, 0) in the game; no spec
//!   gives it.
//! - **xorc10-dragons**, n = 7: a worker walking to a build site (order 28) steps to a different tile; the walk goal
//!   of a build order is a known gap.
//! - **orc09-***, n = 4, and **human06-tech**, n = 69: the same build-site walk (order 28).
//! - **human12-*** (transport, demolition and the 7000-cycle run), n = 134: a peasant harvesting (order 23) steps
//!   east where the game steps north-east.
//! - **orc4**, n = 6810: a peon harvesting (order 23) steps to (6, 5) where the game steps to (5, 6).
//! - **human10**, n = 6906: at n = 6907 the game draws once more from the generator, and two idle units (slots 1534
//!   and 1552) face one step round from ours.
//! - **xorc7**, n = 805: at n = 806 the game draws 10 times from the generator, ours 9.
//! - **human5**, n = 38: a peasant harvesting (order 23) reaches its tree one step later than in the game.
//!
//! The other long traces (human7, orc8, xhuman2, xhuman4, xorc3) diverge in their first ten cycles, mostly in the
//! same build-site walks and first acquisitions, so they aren't run.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 22] = [
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
        Some(545),
    ),
    (
        "orc02-victory-7639.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(545),
    ),
    (
        "orc02-defeat-4021.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(545),
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
        Some(15),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(15),
    ),
    (
        "xorc01-death-knight-2100.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(437),
    ),
    (
        "xorc12-ogre-mage-180.jsonl.gz",
        "Campaign\\XOrc\\2XOrc12.pud",
        51,
        &[],
        Some(76),
    ),
    (
        "xorc10-dragons-800.jsonl.gz",
        "Campaign\\XOrc\\2XOrc10.pud",
        47,
        &[],
        Some(7),
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
        Some(134),
    ),
    (
        "human12-demolition-450.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        Some(134),
    ),
    (
        "human06-tech-650.jsonl.gz",
        "Campaign\\Human\\Human06.pud",
        10,
        &[],
        Some(69),
    ),
    (
        "human12-7000.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        Some(134),
    ),
    ("orc4-20000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], Some(6810)),
    (
        "human10-10000.jsonl.gz",
        "Campaign\\Human\\Human10.pud",
        18,
        &[],
        Some(6906),
    ),
    ("xorc7-7000.jsonl.gz", "Campaign\\XOrc\\2XOrc07.pud", 41, &[], Some(805)),
    (
        "human5-10000.jsonl.gz",
        "Campaign\\Human\\Human05.pud",
        8,
        &[],
        Some(38),
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
