//! Runs `twtool oracle` on real traces of the original game (`docs/specs/oracle/trace-format.md`), when they are on
//! this machine: set `TIDEWATER_ORACLE_TRACES` to the directory holding the trace files and `TIDEWATER_INSTALL` to the
//! game's installation. Without both, the test does nothing. Each trace must match on every field up to its
//! limit.
//!
//! The format-2 traces carry the player's commands in their `turn` records, which the oracle replays. They must match
//! up to the cycle before their first known gap, and every input entry must run in its predicted turn T(i). Every
//! trace below matches in full; a known gap gets a limit and a note here.
//!
//! human01-2000, human04-3000, human01-basics, orc02-assault, orc02-victory, orc02-defeat, orc02-dyingcmd,
//! xhuman10-spells, xhuman10-blizzard, xorc12-ogre-mage, orc09-naval, orc09-repair, human12-transport,
//! human12-demolition, human06-tech, human12-7000, orc4-20000, human10-10000, xorc7, human5, human7-10000, xorc3,
//! human10-24000, orc8, xhuman2, xhuman4, human06-31357, orc4-40000, human7-22000, human01-order15-range,
//! human12-row-f-move, human12-row-f-landing, xorc01-death-coil, xorc01-death-knight, xorc10-dragons, orc3, orc6,
//! orc5, xorc2, xorc4, xorc5, xhuman1, xhuman3, xhuman5, xorc1-20000, xorc10-20000, human2, human3, human9, human11,
//! orc1, orc7, orc10, orc11, orc12, orc13, xhuman6 and xorc9 match in full.
//!
//! Known gaps (the first cycle that differs; each limit is the cycle before):
//!
//! - human8, n = 2486: peasant 1508's idle facing (ours 5, theirs 0).
//! - human13, n = 196: peon 1550's position (ours (78, 9), theirs (75, 8)).
//! - human14, n = 3740: one draw fewer than the trace in that cycle.
//! - orc14, n = 2421: unit 1433 in order 34 (boarding) steps to another tile.
//! - xhuman7, n = 5501: dragon 1415, idle, keeps target 1425 where the trace shows none.
//! - xhuman8, n = 6831: unit 1535, dying, keeps target 1557 where the trace shows none.
//! - xhuman9, n = 9065: peon 1440's idle facing (ours 6, theirs 7).
//! - xhuman11, n = 4443: unit 1510 in order 12 steps to another tile.
//! - xhuman12, n = 1847: grunt 1476's order-12 wait (ours 3, theirs 23).
//! - xorc6, n = 6106: axe thrower 1475's idle search gives order 12, the trace order 16 (orders spec, open question
//!   "An attacker forgotten between runs").
//! - xorc8, n = 6051: gryphon rider 1550's order-12 target (ours 1386, theirs 1400).
//! - xorc11, n = 628: unit 1539 on patrol takes target 1521 (order 12) where the trace stays in order 5.

use std::path::PathBuf;
use std::process::Command;

/// Checks one trace in full and again with RNG resync. Skips when the environment isn't set up.
fn check(file: &str, map: &str, entry: u8, fields: &[&str], limit: Option<u32>) {
    let (Some(dir), Some(install)) = (
        std::env::var_os("TIDEWATER_ORACLE_TRACES"),
        std::env::var_os("TIDEWATER_INSTALL"),
    ) else {
        eprintln!("skipped: set TIDEWATER_ORACLE_TRACES and TIDEWATER_INSTALL to run");
        return;
    };
    let trace = PathBuf::from(&dir).join(file);
    if !trace.is_file() {
        eprintln!("skipped {file}: not found");
        return;
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
    }
}

/// One `#[test]` per row, so the harness runs the traces in parallel. Each row is: test name = (trace file, map,
/// campaign mission table entry (`game/cheats.md`, mission select; `game/victory.md` numbering), fields left out,
/// the last cycle that must match (`None`: the whole trace)).
macro_rules! traces {
    ($($name:ident: ($file:expr, $map:expr, $entry:expr, $fields:expr, $limit:expr),)*) => {
        $(
            #[test]
            fn $name() {
                check($file, $map, $entry, $fields, $limit);
            }
        )*
    };
}

traces! {
    human01_2000: ("human01-2000.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human04_3000: ("human04-3000.jsonl.gz", "Campaign\\Human\\Human04.pud", 6, &[], None),
    human01_basics_9500: ("human01-basics-9500.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    orc02_assault_3000: ("orc02-assault-3000.jsonl.gz", "Campaign\\Orc\\Orc02.pud", 3, &[], None),
    orc02_victory_7639: ("orc02-victory-7639.jsonl.gz", "Campaign\\Orc\\Orc02.pud", 3, &[], None),
    orc02_defeat_4021: ("orc02-defeat-4021.jsonl.gz", "Campaign\\Orc\\Orc02.pud", 3, &[], None),
    orc02_dyingcmd_3760: ("orc02-dyingcmd-3760.jsonl.gz", "Campaign\\Orc\\Orc02.pud", 3, &[], None),
    xorc01_death_coil_2500: ("xorc01-death-coil-2500.jsonl.gz", "Campaign\\XOrc\\2XOrc01.pud", 29, &[], None),
    xhuman10_spells_700: ("xhuman10-spells-700.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xhuman10_blizzard_600: ("xhuman10-blizzard-600.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xorc01_death_knight_2100: ("xorc01-death-knight-2100.jsonl.gz", "Campaign\\XOrc\\2XOrc01.pud", 29, &[], None),
    xorc12_ogre_mage_180: ("xorc12-ogre-mage-180.jsonl.gz", "Campaign\\XOrc\\2XOrc12.pud", 51, &[], None),
    xorc10_dragons_800: ("xorc10-dragons-800.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
    orc09_naval_1500: ("orc09-naval-1500.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc09_repair_600: ("orc09-repair-600.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    human12_transport_650: ("human12-transport-650.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    human12_demolition_450: ("human12-demolition-450.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    human12_row_f_move_700: ("human12-row-f-move-700.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    human12_row_f_landing_700: ("human12-row-f-landing-700.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    human06_tech_650: ("human06-tech-650.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human12_7000: ("human12-7000.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    orc4_20000: ("orc4-20000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], None),
    human10_10000: ("human10-10000.jsonl.gz", "Campaign\\Human\\Human10.pud", 18, &[], None),
    xorc7_7000: ("xorc7-7000.jsonl.gz", "Campaign\\XOrc\\2XOrc07.pud", 41, &[], None),
    human5_10000: ("human5-10000.jsonl.gz", "Campaign\\Human\\Human05.pud", 8, &[], None),
    human7_10000: ("human7-10000.jsonl.gz", "Campaign\\Human\\Human07.pud", 12, &[], None),
    orc8_10000: ("orc8-10000.jsonl.gz", "Campaign\\Orc\\Orc08.pud", 15, &[], None),
    xorc3_10000: ("xorc3-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc03.pud", 33, &[], None),
    xhuman2_10000: ("xhuman2-10000.jsonl.gz", "Campaign\\XHuman\\2XHum02.pud", 30, &[], None),
    xhuman4_7000: ("xhuman4-7000.jsonl.gz", "Campaign\\XHuman\\2XHum04.pud", 34, &[], None),
    human06_31357: ("human06-31357.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human10_24000: ("human10-24000.jsonl.gz", "Campaign\\Human\\Human10.pud", 18, &[], None),
    orc4_40000: ("orc4-40000.jsonl.gz", "Campaign\\Orc\\Orc04.pud", 7, &[], None),
    human7_22000: ("human7-22000.jsonl.gz", "Campaign\\Human\\Human07.pud", 12, &[], None),
    human01_order15_range_700: ("human01-order15-range-700.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    orc3_10000: ("orc3-10000.jsonl.gz", "Campaign\\Orc\\Orc03.pud", 5, &[], None),
    orc5_10000: ("orc5-10000.jsonl.gz", "Campaign\\Orc\\Orc05.pud", 9, &[], None),
    orc6_10000: ("orc6-10000.jsonl.gz", "Campaign\\Orc\\Orc06.pud", 11, &[], None),
    xorc2_10000: ("xorc2-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc02.pud", 31, &[], None),
    xorc4_10000: ("xorc4-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc04.pud", 35, &[], None),
    xorc5_10000: ("xorc5-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc05.pud", 37, &[], None),
    xhuman1_10000: ("xhuman1-10000.jsonl.gz", "Campaign\\XHuman\\2XHum01.pud", 28, &[], None),
    xhuman3_10000: ("xhuman3-10000.jsonl.gz", "Campaign\\XHuman\\2XHum03.pud", 32, &[], None),
    xhuman5_10000: ("xhuman5-10000.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    xorc1_20000: ("xorc1-20000.jsonl.gz", "Campaign\\XOrc\\2XOrc01.pud", 29, &[], None),
    xorc10_20000: ("xorc10-20000.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
    human2_10000: ("human2-10000.jsonl.gz", "Campaign\\Human\\Human02.pud", 2, &[], None),
    human3_10000: ("human3-10000.jsonl.gz", "Campaign\\Human\\Human03.pud", 4, &[], None),
    human8_10000: ("human8-10000.jsonl.gz", "Campaign\\Human\\Human08.pud", 14, &[], Some(2485)),
    human9_10000: ("human9-10000.jsonl.gz", "Campaign\\Human\\Human09.pud", 16, &[], None),
    human11_10000: ("human11-10000.jsonl.gz", "Campaign\\Human\\Human11.pud", 20, &[], None),
    human13_10000: ("human13-10000.jsonl.gz", "Campaign\\Human\\Human13.pud", 24, &[], Some(195)),
    human14_10000: ("human14-10000.jsonl.gz", "Campaign\\Human\\Human14.pud", 26, &[], Some(3739)),
    orc1_10000: ("orc1-10000.jsonl.gz", "Campaign\\Orc\\Orc01.pud", 1, &[], None),
    orc7_10000: ("orc7-10000.jsonl.gz", "Campaign\\Orc\\Orc07.pud", 13, &[], None),
    orc10_10000: ("orc10-10000.jsonl.gz", "Campaign\\Orc\\Orc10.pud", 19, &[], None),
    orc11_10000: ("orc11-10000.jsonl.gz", "Campaign\\Orc\\Orc11.pud", 21, &[], None),
    orc12_10000: ("orc12-10000.jsonl.gz", "Campaign\\Orc\\Orc12.pud", 23, &[], None),
    orc13_10000: ("orc13-10000.jsonl.gz", "Campaign\\Orc\\Orc13.pud", 25, &[], None),
    orc14_10000: ("orc14-10000.jsonl.gz", "Campaign\\Orc\\Orc14.pud", 27, &[], Some(2420)),
    xhuman6_10000: ("xhuman6-10000.jsonl.gz", "Campaign\\XHuman\\2XHum06.pud", 38, &[], None),
    xhuman7_10000: ("xhuman7-10000.jsonl.gz", "Campaign\\XHuman\\2XHum07.pud", 40, &[], Some(5500)),
    xhuman8_10000: ("xhuman8-10000.jsonl.gz", "Campaign\\XHuman\\2XHum08.pud", 42, &[], Some(6830)),
    xhuman9_10000: ("xhuman9-10000.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], Some(9064)),
    xhuman11_10000: ("xhuman11-10000.jsonl.gz", "Campaign\\XHuman\\2XHum11.pud", 48, &[], Some(4442)),
    xhuman12_10000: ("xhuman12-10000.jsonl.gz", "Campaign\\XHuman\\2XHum12.pud", 50, &[], Some(1846)),
    xorc6_10000: ("xorc6-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc06.pud", 39, &[], Some(6105)),
    xorc8_10000: ("xorc8-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc08.pud", 43, &[], Some(6050)),
    xorc9_10000: ("xorc9-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc09.pud", 45, &[], None),
    xorc11_10000: ("xorc11-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], Some(627)),
}
