//! Runs `twtool oracle` on real traces of the original game (`docs/specs/oracle/trace-format.md`), when they are on
//! this machine: set `TIDEWATER_ORACLE_TRACES` to the directory holding the trace files and `TIDEWATER_INSTALL` to the
//! game's installation. Without both, the test does nothing. Each trace must match on every field up to its
//! limit.
//!
//! The format-2 traces carry the player's commands in their `turn` records, which the oracle replays. They must match
//! up to the cycle before their first known gap, and every input entry must run in its predicted turn T(i). The
//! gaps below are what the specs don't yet explain; none has been researched:
//!
//! - **orc02-***, n = 678: at n = 679 a footman (slot 1573) attacks a computer unit (order 12, target 1586) with one
//!   more draw in the game, and is in order 2 in ours.
//! - **xorc01-death-coil**, n = 744: at n = 745 a grunt (slot 1497) in order 12 records tile x 57, facing 7 in the
//!   game, and x 56, facing 6 in ours.
//! - **xhuman10-spells**, n = 67: at n = 68 the game makes one more draw (11 against our 10).
//! - **xhuman10-blizzard**, n = 50: at n = 51 a footman (slot 1490) in order 12 records tile (78, 87), facing 1,
//!   animation 3, wait 1 in the game, and (77, 88), facing 2, animation 4, wait 3 in ours.
//! - **xorc01-death-knight**, n = 766: at n = 767 a grunt (slot 1496) in order 12 records tile x 57, facing 7 in the
//!   game, and x 56, facing 6 in ours.
//! - **xorc10-dragons**, n = 367: at n = 368 a fleeing footman (slot 1520) records tile x 33, facing 3 in the game,
//!   and x 32, facing 4 in ours.
//! - **orc09-naval**, n = 269: at n = 270 a computer ship (slot 1497, type 0x1F) in order 9 faces 2 in the game and
//!   1 in ours (no spec gives a ship's facing here).
//! - **human12-7000**, n = 5987: at n = 5988 a computer ship (slot 1417, type 0x1E) in order 16 faces 3 in the game
//!   and 2 in ours.
//! - **xorc7**, n = 5865: at n = 5866 a worker (slot 1416) in order 28 records tile y 123, facing 7 in the game, and
//!   y 125, facing 5 in ours.
//! - **human5**, n = 1685: at n = 1686 an archer (slot 1533) in order 12 records tile (47, 92), animation 4, wait 3
//!   in the game, and (46, 93), animation 3, wait 1 in ours.
//! - **orc8**, n = 8081: at n = 8082 a worker (slot 1497) in order 24 records tile x 123, facing 4 in the game, and
//!   x 122, facing 5 in ours.
//! - **xorc3**, n = 9179: at n = 9180 a worker (slot 1485) let out of a finished building faces 0 in the game and 1
//!   in ours (the round-39 exit facing toward the top-left + (1, 1)).
//! - **xhuman2**, n = 2486: at n = 2487 player 0 has 0 gold and 825 lumber in the game and 50 and 775 in ours, and a
//!   worker (slot 1588) let out of a building faces 0 in the game and 7 in ours.
//! - **xhuman4**, n = 137: at n = 138 a footman (slot 1484) in order 12 records tile x 70, facing 7 in the game, and
//!   x 71, facing 0 in ours.
//! - **human06-31357**, n = 13474: at n = 13475 a grunt (slot 1516) in order 12 records tile x 72, facing 3, wait 15
//!   in the game, and x 73, facing 2, wait 1 in ours.
//! - **human10-24000**, n = 16954: at n = 16955 a footman (slot 1473) boards (order 34, target 1480) in the game and
//!   is in order 4 in ours.
//! - **orc4-40000**, n = 23679: at n = 23680 a worker (slot 1584) in order 23 faces 5 in the game and 7 in ours.
//! - **human7-22000**, n = 19786: at n = 19787 an archer (slot 1598) is in order 2 in the game and attacks unit 1575
//!   (order 16) in ours.
//!
//! human01-basics, xorc12-ogre-mage, orc09-repair, human12-transport, human12-demolition, human06-tech, orc4-20000,
//! human10-10000 and human7-10000 match in full.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 31] = [
    ("human01-2000.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    ("human04-3000.jsonl.gz", "Campaign\\Human\\Human04.pud", 6, &[], None),
    (
        "human01-basics-9500.jsonl.gz",
        "Campaign\\Human\\Human01.pud",
        0,
        &[],
        None,
    ),
    (
        "orc02-assault-3000.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(678),
    ),
    (
        "orc02-victory-7639.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(678),
    ),
    (
        "orc02-defeat-4021.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(678),
    ),
    (
        "xorc01-death-coil-2500.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(744),
    ),
    (
        "xhuman10-spells-700.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(67),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(50),
    ),
    (
        "xorc01-death-knight-2100.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(766),
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
        Some(367),
    ),
    (
        "orc09-naval-1500.jsonl.gz",
        "Campaign\\Orc\\Orc09.pud",
        17,
        &[],
        Some(269),
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
        None,
    ),
    (
        "human12-7000.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        Some(5987),
    ),
    ("orc4-20000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], None),
    ("human10-10000.jsonl.gz", "Campaign\\Human\\Human10.pud", 18, &[], None),
    (
        "xorc7-7000.jsonl.gz",
        "Campaign\\XOrc\\2XOrc07.pud",
        41,
        &[],
        Some(5865),
    ),
    (
        "human5-10000.jsonl.gz",
        "Campaign\\Human\\Human05.pud",
        8,
        &[],
        Some(1685),
    ),
    ("human7-10000.jsonl.gz", "Campaign\\Human\\Human07.pud", 12, &[], None),
    ("orc8-10000.jsonl.gz", "Campaign\\Orc\\Orc08.pud", 15, &[], Some(8081)),
    (
        "xorc3-10000.jsonl.gz",
        "Campaign\\XOrc\\2XOrc03.pud",
        33,
        &[],
        Some(9179),
    ),
    (
        "xhuman2-10000.jsonl.gz",
        "Campaign\\XHuman\\2XHum02.pud",
        30,
        &[],
        Some(2486),
    ),
    (
        "xhuman4-7000.jsonl.gz",
        "Campaign\\XHuman\\2XHum04.pud",
        34,
        &[],
        Some(137),
    ),
    (
        "human06-31357.jsonl.gz",
        "Campaign\\Human\\Human06.pud",
        10,
        &[],
        Some(13474),
    ),
    (
        "human10-24000.jsonl.gz",
        "Campaign\\Human\\Human10.pud",
        18,
        &[],
        Some(16954),
    ),
    ("orc4-40000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], Some(23679)),
    (
        "human7-22000.jsonl.gz",
        "Campaign\\Human\\Human07.pud",
        12,
        &[],
        Some(19786),
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
