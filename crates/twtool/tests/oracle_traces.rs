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
//! - **xorc01-death-coil**, n = 903: at n = 904 an ogre (slot 1489) in order 10 records tile x 56, facing 1 in the
//!   game, and x 54, facing 7 in ours.
//! - **xhuman10-spells**, n = 285: at n = 286 a computer ogre (slot 1543) in order 12 keeps footman 1529 as its target
//!   (facing 5, wait 1) in the game, and switches to guard tower 1537, its recorded attacker (facing 3, wait 3), in
//!   ours: two peasants that came out of a hall in that cycle still sit among the front entries of the vertical list,
//!   so our target search visits nothing.
//! - **xhuman10-blizzard**, n = 144: at n = 145 the blizzard shards (effect slots 4 to 9) land on other points:
//!   (2480, 2864) in the game and (2359, 2694) in ours for slot 4.
//! - **xorc01-death-knight**, n = 914: at n = 915 an ogre (slot 1488) is in order 12, animation 4, wait 15 in the
//!   game, and in order 10, animation 2, wait 3 in ours.
//! - **xorc10-dragons**, n = 367: at n = 368 a fleeing footman (slot 1520) records tile x 33, facing 3 in the game,
//!   and x 32, facing 4 in ours.
//! - **orc09-naval**, n = 852: at n = 853 a computer destroyer (slot 1460) on sea patrol toward oil platform 1512
//!   steps west to (76, 60), facing 6 in the game, and north-west to (76, 58), facing 7 in ours.
//! - **human5**, n = 1758: at n = 1759 a grunt (slot 1531) in order 12 targets 1525, facing 5, wait 1 in the game,
//!   and 1518, facing 2, wait 3 in ours; and a critter (slot 1538) is in order 3 in the game and 2 in ours.
//! - **xhuman2**, n = 2486: at n = 2487 player 0 has 0 gold and 825 lumber in the game and 50 and 775 in ours, and a
//!   worker (slot 1588) let out of a building faces 0 in the game and 7 in ours.
//! - **human06-31357**, n = 16295: at n = 16296 an axe thrower (slot 1512) that took order 12 on building 1583 from
//!   order 4 steps to (71, 8), facing 1 in the game, and to (71, 9), facing 2 in ours.
//! - **orc4-40000**, n = 23679: at n = 23680 a worker (slot 1584) in order 23 faces 5 in the game and 7 in ours.
//! - **human7-22000**, n = 19786: at n = 19787 an archer (slot 1598) is in order 2 in the game and attacks unit 1575
//!   (order 16) in ours.
//! - **human01-order15-range**, n = 533: at n = 534 a computer grunt (slot 1592) in order 3 records tile x 27,
//!   facing 4 in the game, and x 28, facing 3 in ours.
//!
//! human01-2000, human04-3000, human01-basics, orc02-assault, xorc12-ogre-mage, orc09-repair, human12-transport,
//! human12-demolition, human06-tech, human12-7000, orc4-20000, human10-10000, xorc7, human7-10000, xorc3,
//! human10-24000, orc8, xhuman4, human12-row-f-move and human12-row-f-landing match in full.

use std::path::PathBuf;
use std::process::Command;

/// Trace file, map, campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering),
/// fields left out, and the last cycle that must match (`None`: the whole trace).
type Trace = (&'static str, &'static str, u8, &'static [&'static str], Option<u32>);

const TRACES: [Trace; 34] = [
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
        Some(903),
    ),
    (
        "xhuman10-spells-700.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(285),
    ),
    (
        "xhuman10-blizzard-600.jsonl.gz",
        "Campaign\\XHuman\\2XHum10.pud",
        46,
        &[],
        Some(144),
    ),
    (
        "xorc01-death-knight-2100.jsonl.gz",
        "Campaign\\XOrc\\2XOrc01.pud",
        29,
        &[],
        Some(914),
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
        Some(852),
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
    (
        "human06-31357.jsonl.gz",
        "Campaign\\Human\\Human06.pud",
        10,
        &[],
        Some(16295),
    ),
    ("human10-24000.jsonl.gz", "Campaign\\Human\\Human10.pud", 18, &[], None),
    ("orc4-40000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], Some(23679)),
    (
        "human7-22000.jsonl.gz",
        "Campaign\\Human\\Human07.pud",
        12,
        &[],
        Some(19786),
    ),
    (
        "human01-order15-range-700.jsonl.gz",
        "Campaign\\Human\\Human01.pud",
        0,
        &[],
        Some(533),
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
