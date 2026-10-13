//! Runs `twtool oracle` on real traces of the original game (`docs/specs/oracle/trace-format.md`), when they are on
//! this machine: set `TIDEWATER_ORACLE_TRACES` to the directory holding the trace files and `TIDEWATER_INSTALL` to the
//! game's installation. Without both, the test does nothing. Each trace must match on every field up to its
//! limit.
//!
//! The format-2 traces carry the player's commands in their `turn` records, which the oracle replays. They must match
//! up to the cycle before their first known gap, and every input entry must run in its predicted turn T(i). The
//! gaps below are what the specs don't yet explain; none has been researched:
//!
//! - **orc02-victory**, **orc02-defeat**, n = 2719: at n = 2720 a computer grunt (slot 1576) attacking a wall (order
//!   11) records tile (56, 17), facing 7, animation 4 in the game, and (57, 16), facing 1, animation 3 in ours.
//! - **xhuman10-spells**, n = 327: at n = 328 a computer catapult (slot 1480), idle, has wait 3 in the game and 1 in
//!   ours, and the game made one draw fewer since the previous record (7 against our 8).
//! - **xhuman10-blizzard**, n = 308: at n = 309 a computer catapult (slot 1483), cooling down in order 16, has no
//!   target in the game and still targets 1490 (dying since about n = 277) in ours, just after its own rock lands
//!   near that target.
//! - **human5**, n = 1758: at n = 1759 a grunt (slot 1531) in order 12 targets 1525, facing 5, wait 1 in the game,
//!   and 1518, facing 2, wait 3 in ours; and a critter (slot 1538) is in order 3 in the game and 2 in ours.
//! - **xhuman2**, n = 2486: at n = 2487 player 0 has 0 gold and 825 lumber in the game and 50 and 775 in ours, and a unit
//!   worker (slot 1588) let out of a building faces 0 in the game and 7 in ours.
//! - **orc5**, n = 1640: at n = 1641 player 1 has 300 gold and 75 lumber in the game and 800 and 325 in ours, and
//!   a unit of type 0x02 (slot 1564) stands at tile y 41, facing 5, in the game and y 39, facing 7, in ours.
//! - **xorc4**, n = 8070: at n = 8071 a unit of type 0x02 (slot 1580, owner 2) in order 28 is at tile y 11, facing
//!   7, in the game and y 12, facing 6, in ours.
//! - **xhuman3**, n = 8748: at n = 8749 an idle unit of type 0x1b (slot 1418, owner 3) has wait 3 in the game and 1
//!   in ours.
//!
//! human01-2000, human04-3000, human01-basics, orc02-assault, xorc12-ogre-mage, orc09-naval, orc09-repair,
//! human12-transport, human12-demolition, human06-tech, human12-7000, orc4-20000, human10-10000, xorc7, human7-10000,
//! xorc3, human10-24000, orc8, xhuman4, human06-31357, orc4-40000, human7-22000, human01-order15-range,
//! human12-row-f-move, human12-row-f-landing, xorc01-death-coil, xorc01-death-knight, xorc10-dragons, orc3, orc6,
//! xorc2, xorc5, xhuman1, xhuman5, xorc1-20000 and xorc10-20000 match in full.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 45] = [
    ("human01-2000.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    ("human04-3000.jsonl.gz", "Campaign\\Human\\Human04.pud", 6, &[], None),
    (
        "human01-basics-9500.jsonl.gz",
        "Campaign\\Human\\Human01.pud",
        0,
        &[],
        None,
    ),
    ("orc02-assault-3000.jsonl.gz", "Campaign\\Orc\\Orc02.pud", 3, &[], None),
    (
        "orc02-victory-7639.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(2719),
    ),
    (
        "orc02-defeat-4021.jsonl.gz",
        "Campaign\\Orc\\Orc02.pud",
        3,
        &[],
        Some(2719),
    ),
    (
        "xorc01-death-coil-2500.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        None,
    ),
    (
        "xhuman10-spells-700.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(327),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(308),
    ),
    (
        "xorc01-death-knight-2100.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        None,
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
        None,
    ),
    ("orc09-naval-1500.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
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
        "human12-row-f-move-700.jsonl.gz",
        "Campaign\\Human\\Human12.pud",
        22,
        &[],
        None,
    ),
    (
        "human12-row-f-landing-700.jsonl.gz",
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
    ("human12-7000.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    ("orc4-20000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], None),
    ("human10-10000.jsonl.gz", "Campaign\\Human\\Human10.pud", 18, &[], None),
    ("xorc7-7000.jsonl.gz", "Campaign\\XOrc\\2XOrc07.pud", 41, &[], None),
    (
        "human5-10000.jsonl.gz",
        "Campaign\\Human\\Human05.pud",
        8,
        &[],
        Some(1758),
    ),
    ("human7-10000.jsonl.gz", "Campaign\\Human\\Human07.pud", 12, &[], None),
    ("orc8-10000.jsonl.gz", "Campaign\\Orc\\Orc08.pud", 15, &[], None),
    ("xorc3-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc03.pud", 33, &[], None),
    (
        "xhuman2-10000.jsonl.gz",
        "Campaign\\XHuman\\2XHum02.pud",
        30,
        &[],
        Some(2486),
    ),
    ("xhuman4-7000.jsonl.gz", "Campaign\\XHuman\\2XHum04.pud", 34, &[], None),
    ("human06-31357.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    ("human10-24000.jsonl.gz", "Campaign\\Human\\Human10.pud", 18, &[], None),
    ("orc4-40000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], None),
    ("human7-22000.jsonl.gz", "Campaign\\Human\\Human07.pud", 12, &[], None),
    (
        "human01-order15-range-700.jsonl.gz",
        "Campaign\\Human\\Human01.pud",
        0,
        &[],
        None,
    ),
    ("orc3-10000.jsonl.gz", "Campaign\\Orc\\Orc03.pud", 5, &[], None),
    ("orc5-10000.jsonl.gz", "Campaign\\Orc\\Orc05.pud", 9, &[], Some(1640)),
    ("orc6-10000.jsonl.gz", "Campaign\\Orc\\Orc06.pud", 11, &[], None),
    ("xorc2-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc02.pud", 31, &[], None),
    (
        "xorc4-10000.jsonl.gz",
        "Campaign\\XOrc\\2XOrc04.pud",
        35,
        &[],
        Some(8070),
    ),
    ("xorc5-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc05.pud", 37, &[], None),
    ("xhuman1-10000.jsonl.gz", "Campaign\\XHuman\\2XHum01.pud", 28, &[], None),
    (
        "xhuman3-10000.jsonl.gz",
        "Campaign\\XHuman\\2XHum03.pud",
        32,
        &[],
        Some(8748),
    ),
    ("xhuman5-10000.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    ("xorc1-20000.jsonl.gz", "Campaign\\XOrc\\2XOrc01.pud", 29, &[], None),
    ("xorc10-20000.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
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
