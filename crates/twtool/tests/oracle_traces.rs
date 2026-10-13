//! Runs `twtool oracle` on real traces of the original game (`docs/specs/oracle/trace-format.md`), when they are on
//! this machine: set `TIDEWATER_ORACLE_TRACES` to the directory holding the trace files and `TIDEWATER_INSTALL` to the
//! game's installation. Without both, the test does nothing. Each trace must match on every field up to its
//! limit.
//!
//! The format-2 traces carry the player's commands in their `turn` records, which the oracle replays. They must match
//! up to the cycle before their first known gap, and every input entry must run in its predicted turn T(i). A known
//! gap gets a limit and a note here.
//!
//! human01-2000, human04-3000, human01-basics, orc02-assault, orc02-victory, orc02-defeat, orc02-dyingcmd,
//! xhuman10-spells, xhuman10-blizzard, xorc12-ogre-mage, orc09-naval, orc09-repair, human12-transport,
//! human12-demolition, human06-tech, human12-7000, orc4-20000, human10-10000, xorc7, human5, human7-10000, xorc3,
//! human10-24000, orc8, xhuman2, xhuman4, human06-31357, orc4-40000, human7-22000, human01-order15-range,
//! human12-row-f-move, human12-row-f-landing, xorc01-death-coil, xorc01-death-knight, xorc10-dragons, orc3, orc6,
//! orc5, xorc2, xorc4, xorc5, xhuman1, xhuman3, xhuman5, xorc1-20000, xorc10-20000, human2, human3, human9, human11,
//! orc1, orc7, orc10, orc11, orc12, orc13, xhuman6, xorc9, human8, human14, xhuman8, xhuman7, xhuman9, xorc6, xorc11,
//! xhuman12, human13, orc14, xhuman11, xorc8, human4-15000, xhuman4-15000, orc9-12000, xhuman10-12000, xorc7-15000,
//! human8-30000, xorc12-4000, xorc09-death-knight-2500, orc10-sappers-2500, xhuman10-slow-3000, human13-subs-2000,
//! human12-25000, human09-fleet-3000, xorc11-air-sea-3000, human01-walls, human06-upgrades, xorc12-spells,
//! xhuman10-exorcism, orc13-25000, orc14-25000, xhuman11-25000, orc11-25000, human2-human02-all-out-4000 (Human02
//! with the invulnerability cheat turned on in its commands), orc5-40000 (the game ends at n 36583) and human9-30000
//! match in full.
//!
//! The multiplayer-launch traces (trace format, "Multiplayer launch") name their map in the header. The comparer
//! derives the setup's generator state and computer slots (round-58 errata) and takes only the lobby's choices (the
//! humans' slots and races, the computer rows) from the header. The header has no Fixed Order field yet (round-59
//! corrections), so the starts are taken as random. All match in full: mp-dark-paths-bne-1000,
//! mp-gold-rush-bne-6000, mp-gold-rush-bne-25000, mp-great-white-north-bne-6000, mp-big-rock-candy-mountain-bne-6000,
//! mp-big-rock-candy-mountain-bne-25000, mp-big-rock-candy-mountain-bne-40000, mp-schwartzwald-bne-6000,
//! mp-schwartzwald-bne-25000, mp-rose-petal-bne-10000, mp-rose-petal-bne-25000, mp-isolation-bne-30000 and
//! mp-frosty-fjords-bne-30000 (Enhanced), and mp-dark-paths-bne-classic-1000, mp-gold-rush-bne-classic-8000 and
//! mp-gold-rush-bne-classic-20000 (Classic mode). xorc12-reach-400 matches in full.

use std::path::PathBuf;
use std::process::Command;

/// Checks one trace in full and again with RNG resync. Skips when the environment isn't set up. `place` is the map
/// and campaign entry; `None` for a multiplayer trace, whose header names its map.
fn check(file: &str, place: Option<(&str, u8)>, fields: &[&str], limit: Option<u32>) {
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
            .args(place.iter().flat_map(|(map, entry)| {
                [
                    "--map".to_string(),
                    map.to_string(),
                    "--entry".to_string(),
                    entry.to_string(),
                ]
            }))
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
/// the last cycle that must match (`None`: the whole trace)). A multiplayer row is `mp(trace file, fields left out,
/// limit)`: its header names the map.
macro_rules! traces {
    ($($name:ident: $row:tt,)*) => {
        $(
            #[test]
            fn $name() {
                traces!(@row $row);
            }
        )*
    };
    (@row (mp, $file:expr, $fields:expr, $limit:expr)) => {
        check($file, None, $fields, $limit)
    };
    (@row ($file:expr, $map:expr, $entry:expr, $fields:expr, $limit:expr)) => {
        check($file, Some(($map, $entry)), $fields, $limit)
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
    human8_10000: ("human8-10000.jsonl.gz", "Campaign\\Human\\Human08.pud", 14, &[], None),
    human9_10000: ("human9-10000.jsonl.gz", "Campaign\\Human\\Human09.pud", 16, &[], None),
    human11_10000: ("human11-10000.jsonl.gz", "Campaign\\Human\\Human11.pud", 20, &[], None),
    human13_10000: ("human13-10000.jsonl.gz", "Campaign\\Human\\Human13.pud", 24, &[], None),
    human14_10000: ("human14-10000.jsonl.gz", "Campaign\\Human\\Human14.pud", 26, &[], None),
    orc1_10000: ("orc1-10000.jsonl.gz", "Campaign\\Orc\\Orc01.pud", 1, &[], None),
    orc7_10000: ("orc7-10000.jsonl.gz", "Campaign\\Orc\\Orc07.pud", 13, &[], None),
    orc10_10000: ("orc10-10000.jsonl.gz", "Campaign\\Orc\\Orc10.pud", 19, &[], None),
    orc11_10000: ("orc11-10000.jsonl.gz", "Campaign\\Orc\\Orc11.pud", 21, &[], None),
    orc12_10000: ("orc12-10000.jsonl.gz", "Campaign\\Orc\\Orc12.pud", 23, &[], None),
    orc13_10000: ("orc13-10000.jsonl.gz", "Campaign\\Orc\\Orc13.pud", 25, &[], None),
    orc14_10000: ("orc14-10000.jsonl.gz", "Campaign\\Orc\\Orc14.pud", 27, &[], None),
    xhuman6_10000: ("xhuman6-10000.jsonl.gz", "Campaign\\XHuman\\2XHum06.pud", 38, &[], None),
    xhuman7_10000: ("xhuman7-10000.jsonl.gz", "Campaign\\XHuman\\2XHum07.pud", 40, &[], None),
    xhuman8_10000: ("xhuman8-10000.jsonl.gz", "Campaign\\XHuman\\2XHum08.pud", 42, &[], None),
    xhuman9_10000: ("xhuman9-10000.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    xhuman11_10000: ("xhuman11-10000.jsonl.gz", "Campaign\\XHuman\\2XHum11.pud", 48, &[], None),
    xhuman12_10000: ("xhuman12-10000.jsonl.gz", "Campaign\\XHuman\\2XHum12.pud", 50, &[], None),
    xorc6_10000: ("xorc6-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc06.pud", 39, &[], None),
    xorc8_10000: ("xorc8-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc08.pud", 43, &[], None),
    xorc9_10000: ("xorc9-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc09.pud", 45, &[], None),
    xorc11_10000: ("xorc11-10000.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], None),
    human4_15000: ("human4-15000.jsonl.gz", "Campaign\\Human\\Human04.pud", 6, &[], None),
    xhuman4_15000: ("xhuman4-15000.jsonl.gz", "Campaign\\XHuman\\2XHum04.pud", 34, &[], None),
    orc9_12000: ("orc9-12000.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    xhuman10_12000: ("xhuman10-12000.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xorc7_15000: ("xorc7-15000.jsonl.gz", "Campaign\\XOrc\\2XOrc07.pud", 41, &[], None),
    human8_30000: ("human8-30000.jsonl.gz", "Campaign\\Human\\Human08.pud", 14, &[], None),
    xorc12_4000: ("xorc12-4000.jsonl.gz", "Campaign\\XOrc\\2XOrc12.pud", 51, &[], None),
    xorc09_death_knight_2500: ("xorc09-death-knight-2500.jsonl.gz", "Campaign\\XOrc\\2XOrc09.pud", 45, &[], None),
    human12_25000: ("human12-25000.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    orc10_sappers_2500: ("orc10-sappers-2500.jsonl.gz", "Campaign\\Orc\\Orc10.pud", 19, &[], None),
    xhuman10_slow_3000: ("xhuman10-slow-3000.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    human13_subs_2000: ("human13-subs-2000.jsonl.gz", "Campaign\\Human\\Human13.pud", 24, &[], None),
    human09_fleet_3000: ("human09-fleet-3000.jsonl.gz", "Campaign\\Human\\Human09.pud", 16, &[], None),
    xorc11_air_sea_3000: ("xorc11-air-sea-3000.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], None),
    human01_walls_4000: ("human01-walls-4000.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human06_upgrades_1500: ("human06-upgrades-1500.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    xorc12_spells_1500: ("xorc12-spells-1500.jsonl.gz", "Campaign\\XOrc\\2XOrc12.pud", 51, &[], None),
    xhuman10_exorcism_3000: ("xhuman10-exorcism-3000.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    orc13_25000: ("orc13-25000.jsonl.gz", "Campaign\\Orc\\Orc13.pud", 25, &[], None),
    orc14_25000: ("orc14-25000.jsonl.gz", "Campaign\\Orc\\Orc14.pud", 27, &[], None),
    xhuman11_25000: ("xhuman11-25000.jsonl.gz", "Campaign\\XHuman\\2XHum11.pud", 48, &[], None),
    orc11_25000: ("orc11-25000.jsonl.gz", "Campaign\\Orc\\Orc11.pud", 21, &[], None),
    mp_dark_paths_bne_1000: (mp, "mp-dark-paths-bne-1000.jsonl.gz", &[], None),
    mp_dark_paths_bne_classic_1000: (mp, "mp-dark-paths-bne-classic-1000.jsonl.gz", &[], None),
    xorc12_reach_400: ("xorc12-reach-400.jsonl.gz", "Campaign\\XOrc\\2XOrc12.pud", 51, &[], None),
    mp_gold_rush_bne_6000: (mp, "mp-gold-rush-bne-6000.jsonl.gz", &[], None),
    mp_great_white_north_bne_6000: (mp, "mp-great-white-north-bne-6000.jsonl.gz", &[], None),
    mp_big_rock_candy_mountain_bne_6000: (mp, "mp-big-rock-candy-mountain-bne-6000.jsonl.gz", &[], None),
    mp_schwartzwald_bne_6000: (mp, "mp-schwartzwald-bne-6000.jsonl.gz", &[], None),
    mp_rose_petal_bne_10000: (mp, "mp-rose-petal-bne-10000.jsonl.gz", &[], None),
    mp_gold_rush_bne_classic_8000: (mp, "mp-gold-rush-bne-classic-8000.jsonl.gz", &[], None),
    mp_gold_rush_bne_classic_20000: (mp, "mp-gold-rush-bne-classic-20000.jsonl.gz", &[], None),
    mp_big_rock_candy_mountain_bne_25000: (mp, "mp-big-rock-candy-mountain-bne-25000.jsonl.gz", &[], None),
    mp_rose_petal_bne_25000: (mp, "mp-rose-petal-bne-25000.jsonl.gz", &[], None),
    mp_gold_rush_bne_25000: (mp, "mp-gold-rush-bne-25000.jsonl.gz", &[], None),
    mp_schwartzwald_bne_25000: (mp, "mp-schwartzwald-bne-25000.jsonl.gz", &[], None),
    mp_isolation_bne_30000: (mp, "mp-isolation-bne-30000.jsonl.gz", &[], None),
    mp_big_rock_candy_mountain_bne_40000: (mp, "mp-big-rock-candy-mountain-bne-40000.jsonl.gz", &[], None),
    mp_frosty_fjords_bne_30000: (mp, "mp-frosty-fjords-bne-30000.jsonl.gz", &[], None),
    human2_human02_all_out_4000: ("human2-human02-all-out-4000.jsonl.gz", "Campaign\\Human\\Human02.pud", 2, &[], None),
    orc5_40000: ("orc5-40000.jsonl.gz", "Campaign\\Orc\\Orc05.pud", 9, &[], None),
    human9_30000: ("human9-30000.jsonl.gz", "Campaign\\Human\\Human09.pud", 16, &[], None),
}
