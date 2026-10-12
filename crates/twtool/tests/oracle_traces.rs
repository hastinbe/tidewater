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
//! - **orc02-***, n = 616: at n = 617 a computer footman (slot 1587) in order 12 records wait 23 in the game and
//!   wait 3 in ours.
//! - **xorc01-death-coil**, n = 744: at n = 745 a grunt (slot 1497) in order 12 records tile x 57, facing 7 in the
//!   game, and x 56, facing 6 in ours.
//! - **xhuman10-spells**, n = 62: at n = 63 the game makes one more draw, and effect slot 4 aims at (3149, 1810)
//!   with damage 5 where ours aims at (3153, 1805) with damage 4.
//! - **xhuman10-blizzard**, n = 48: at n = 49 the game makes three more draws and has effect slot 5 in use where
//!   ours has it free.
//! - **xorc01-death-knight**, n = 707: at n = 708 a skeleton (slot 1515) attacks grunt 1497 (order 12) in the game
//!   and is idle in ours: the halving search as written stops before the grunt (orders spec, the n = 708 vector).
//! - **xorc10-dragons**, n = 367: at n = 368 a fleeing footman (slot 1520) records tile x 33, facing 3 in the game,
//!   and x 32, facing 4 in ours.
//! - **orc09-naval**, n = 267: at n = 268 slot 1502 (owner 15) is type 0x5D with order 0 in the game and type 0x6C
//!   with order 1 in ours.
//! - **human12-7000**, n = 5987: at n = 5988 a computer ship (slot 1417, type 0x1E) in order 16 faces 3 in the game
//!   and 2 in ours.
//! - **xorc7**, n = 1955: at n = 1956 a tanker (slot 1587) in order 23 records its animation 4 with wait 3 in the
//!   game, and animation 3 with wait 1 in ours.
//! - **human5**, n = 1685: at n = 1686 an archer (slot 1533) in order 12 records tile (47, 92), animation 4, wait 3
//!   in the game, and (46, 93), animation 3, wait 1 in ours.
//! - **orc8**, n = 948: at n = 949 a tanker (slot 1479, type 0x1A) records tile y 102, facing 4 in the game, and
//!   y 100, facing 3 in ours.
//! - **xorc3**, n = 8894: at n = 8895 a worker (slot 1462) records order 24 with target 1589 in the game and idle in
//!   ours.
//! - **xhuman2**, n = 1895: at n = 1896 a worker (slot 1522) in order 23 records tile x 95 in the game and x 96 in
//!   ours.
//! - **xhuman4**, n = 86: at n = 87 a footman (slot 1484) in order 12 records tile (72, 62), facing 3 in the game,
//!   and (71, 61), facing 0 in ours.
//!
//! xorc12-ogre-mage, orc09-repair, human12-transport, human12-demolition, human06-tech, orc4,
//! human10 and human7 match in full.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 27] = [
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
        Some(616),
    ),
    (
        "orc02-victory-7639.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(616),
    ),
    (
        "orc02-defeat-4021.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(616),
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
        Some(62),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(48),
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
        Some(367),
    ),
    (
        "orc09-naval-1500.jsonl.gz",
        "Campaign\\Orc\\Orc09.pud",
        17,
        &[],
        Some(267),
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
        Some(1955),
    ),
    (
        "human5-10000.jsonl.gz",
        "Campaign\\Human\\Human05.pud",
        8,
        &[],
        Some(1685),
    ),
    ("human7-10000.jsonl.gz", "Campaign\\Human\\Human07.pud", 12, &[], None),
    ("orc8-10000.jsonl.gz", "Campaign\\Orc\\Orc08.pud", 15, &[], Some(948)),
    (
        "xorc3-10000.jsonl.gz",
        "Campaign\\XOrc\\2XOrc03.pud",
        33,
        &[],
        Some(8894),
    ),
    (
        "xhuman2-10000.jsonl.gz",
        "Campaign\\XHuman\\2XHum02.pud",
        30,
        &[],
        Some(1895),
    ),
    (
        "xhuman4-7000.jsonl.gz",
        "Campaign\\XHuman\\2XHum04.pud",
        34,
        &[],
        Some(86),
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
