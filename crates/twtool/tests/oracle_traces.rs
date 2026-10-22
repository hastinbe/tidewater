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
//! with the invulnerability cheat turned on in its commands), orc5-40000 (the game ends at n 36583), human9-30000,
//! xhuman8-25000, human11-30000, orc7-22000, xhuman7-25000 and xorc11-16000 match in full.
//!
//! The commands scenarios orc9-orc09-shore-build-3000 (a refused shipyard, a shore build and an attack on it),
//! orc9-orc09-smart-move-2000 (build commands that do nothing, and smart moves onto a gold mine and an own oil
//! platform by empty and carrying workers and a tanker), orc9-orc09-attack-move-1500 (complete attack-moves forgotten
//! at countdown 0), orc9-orc09-return-goods-2200 (return goods given a lumber mill and an own oil platform),
//! orc9-orc09-shore-no-corner-600 (a shipyard site with no corner in the worker's region), orc9-orc09-tanker-move-2200
//! (a ship following a ship, and smart moves onto a shipyard and an oil patch that keep the target),
//! orc9-orc09-enter-dying-1800 (a depot dying under a returning and an entering peon), orc9-orc09-enter-nothing-1900
//! (return goods to a blacksmith and a shipyard: no delivery, frame 2 kept, a come-out with no depot),
//! orc9-orc09-smart-target-2100 (land smart moves onto buildings, and one whose farm dies mid-walk),
//! orc9-orc09-flee-walk-1200 and orc9-orc09-flee-chop-walk-900 (a lumber peon hit while walking flees only when its
//! walk ends), orc9-orc09-return-nodepot-2200 (return goods into a peon, then to one inside a mine, and with no depot
//! left), orc9-orc09-patrol-countdown-1500 (patrols resumed at countdown 0), orc9-orc09-return-inside-1200 (return
//! goods to a peon that goes into a mine mid-walk), orc12-orc12-board-attacked-1000 (an attacked ogre boards its
//! transport without turning on its attacker) and orc9-orc09-follow-attacked-1200 (an attacked follower follows the
//! archer its reaction picked) match in full.
//!
//! Round 64: xorc6-30000 (the game ends at n 11257) and xorc5-18500 match in full.
//!
//! Round 65: xhuman5-30000 (its boarding grunt finding no route because the goal isn't a line tile at n 26761, order
//! 4's walk heading for the transport where it stands at n 26764, a tanker coming out with nothing pending at n 26811
//! and an oil patch below its builder's slot at n 27632), orc10-35000, orc8-30000 and xorc4-30000 (dying units'
//! sight at a fog refresh, n 19507, and the new-mine search counting a castle by its footprint, n 27870) match in
//! full, and so do the commands scenarios orc9-orc09-follow-flee-1300 (a hit peon follows, guards, then flees) and
//! orc12-orc12-guard-board-full-900 (a guard with no route, and a unit giving up at a full transport that pushes off).
//!
//! Round 66: orc3-30000, orc6-30000, xhuman1-30000 and xorc2-30000 match in full, and so do the commands scenarios
//! orc9-orc09-repair-water-1300 (repair and an attack on a building with no route, and a tanker told to harvest a
//! gold mine), orc9-orc09-order16-keep-2000 (order 16 still cooling down keeps its order when the search gives order
//! 12, n 937) and human13-sea-fight-3000.
//!
//! Round 68: the commands scenarios orc9-orc09-ground-move-goals-1100 (order 18 walking to its pulled-back goal and
//! switching to order 17), orc9-orc09-hold-resume-hits-goals-1600 (the friendly-fire hold resuming and forgetting an
//! attack-move) and orc9-orc09-lost-resume-goals-500 (a player's command whose target is inside a mine) match in full.
//!
//! Round 70: the commands scenario orc9-orc09-carry-chop-1400 (a carrying peon returning goods to a farm still under
//! construction goes idle as a pending order, then returns its gold) matches in full.
//!
//! Round 71: the commands scenarios human01-order15-lost-700 (order 15 losing its target: order 13 takes over with
//! wait 3, n 469), human12-unload-then-move-900 (a smart move after an unload clears the unload mark: the transport
//! docks with its passengers aboard, n 427) and orc09-sea-patrol-axis-2000 (a sea patrol along the ship's own column
//! isn't remembered, so its fight ends idle, n 1795) match in full.
//!
//! Round 72: the commands scenario orc11-orc11-sapper-attack-move-goals-1500 (Enhanced goblin sappers: one on a
//! remembered patrol demolishes the unit its search finds, n 426 to 443; two in order 4 with nothing remembered take
//! no target while walking, before and after a hit at n 323) matches in full. The spec's open question on the hit
//! sapper not fleeing after it arrives (T14) doesn't cut it short here: order 4's arrival forgets the attacker.
//!
//! Round 75: the commands scenarios human12-unload-then-refused-goals-900 (a command 4 the transport doesn't take
//! keeps its unload mark: it lands and puts its squads ashore, n 533 and 534),
//! orc11-orc11-sapper-attack-move-goals-hits-1500 (as round 72's, with the hit list: the hit sapper forgets its
//! attacker on arriving) and orc9-orc09-carry-chop-goals-1400 (a player's return command finds its depot in the
//! command, n 762) match in full.
//!
//! Round 76: the commands scenarios human06-site-kill-800 (a peasant inside a farm site that is killed dies with it,
//! n 416) and human06-alerts-1400 (work-complete alerts for a keep, an upgrade and a spell, none for a guard tower)
//! match in full.
//!
//! Round 77: the commands scenarios human06-depot-kill-2400 (a peasant delivering gold inside a town hall that is
//! killed dies with it, n 2290), human12-unload-then-patrol-900 (a patrol the transport doesn't take keeps its unload
//! mark), human06-cancel-job-500 (cancelled jobs end at their next work run, n 38, 206 and 304),
//! human06-cancel-site-600 (a cancelled site's builder faces its top-left tile with idle pending, n 122 and 202) and
//! human06-arrive-broke-700 (a peasant arriving without the money goes idle with wait 3, n 340)
//! match in full.
//!
//! Round 79: the commands scenarios orc09-strike-inside-hits-400 (a melee strike at a target gone inside is rolled
//! and does nothing, V93), orc09-missile-inside-hits-600 (a launch at a target gone inside flies and does nothing,
//! V94), human13-splash-air-hits-1700 (an air unit on the impact tile in its scan place, V95),
//! human13-invisible-work-hits-900 (chop and repair strikes end invisibility; a repairer at full hit points idles at
//! once), human05-ally-kill-hits-1000 (an ally's kill is credited, V96), human06-build-stats-1800 (the cheat's
//! every-pass step, V70) and human06-carry-build-1000 (a builder loses a load its new building doesn't store, V68)
//! match in full.
//!
//! Round 80: xhuman10-failcast-520 (a channelled blizzard short of mana still ends the caster's invisibility at its
//! strike point, n 437, S84) and xorc12-spells-hits-1150-1260 (a rune's hit is a direct HP change with no `hits`
//! entry, S85) match in full.
//!
//! Round 81: xhuman10-slow-killed-goals-400 (a slow's target dies during the walk; idle takes over at the next step
//! boundary, S90), xhuman10-heal-ballista-goals-500 (healing a ballista walks into reach before the organic test,
//! S91), xhuman10-invisible-reach-goals-400 (a caster in reach casts at a target it no longer detects, S92),
//! xhuman10-invisible-dwarves-150 (a demolition squad killed in the timers pass still counts its timers, S93),
//! xhuman10-shield-mine-hits-800 (flames go on around a peasant inside a mine and a town hall, splashing the
//! building, S94 to S96) and xhuman10-shield-self-200 (a flame shield command aimed at its own caster does nothing,
//! S97) match in full.
//!
//! Round 82: xorc10-xorc10-losses-4000 (the AI consolidation trace: a patrolling destroyer turns around at its patrol
//! end before the target search, n 1021, V281; the losses rule counts a computer player's buildings at the kill, the
//! destroyed one included, so losing the first of 3 guard towers starts no all-out attack and losing the second
//! does, CP1 and CP2) matches in full.
//!
//! Round 83: orc3-orc03-platform-losses-9500 (an oil platform under construction counts in the losses rule, so the
//! shipyard's and the first farm's losses start no all-out attack and the last farm's does, n 7634, CP7),
//! xhuman5-xhuman05-worker-all-out-goals-1100 (a computer peon given command 4 by the all-out attack's attack goal makes
//! its job choice at its next order-logic run in order 4, return goods pending from n 804, CP5),
//! xhuman2-xhuman02-cheat-scope-1600 (the all-spells cheat tops up a computer death knight's mana but teaches it no
//! spell, CP9 and S101) and human5-human05-rescue-jobs-2000 (a lumberjack that joins another player leaves its job
//! count with active rescue player 0, CP10) match in full.
//!
//! Round 84: xhuman5-xhuman05-worker-order12-goals-3000 (a computer peon in order 12 makes its job choice at each
//! step of its walk; with no job it draws the wait, lost at n 2505 when the step starts the move animation and kept
//! from n 2527, CP12), orc3-orc03-tanker-order4-goals-7500-10500 (a window: a computer tanker with no depot makes
//! the choice at each order-4 step and goes on with no wait, CP13) and human5-30000 (active rescue player 0 starts
//! no building in 30000 cycles) match to their ends.
//!
//! Round 85: xorc10-xorc10-farm-repair-1700 (a computer peasant repairs a farm while dragons hover inside its safety
//! area: repair safety tests a tile's air only over a ground unit, erratum 2), xhuman5-xhuman05-worker-all-out-goals-3000
//! (command 4 for a peon past its leash, from the farm site) and xhuman5-xhuman05-worker-all-out-aigoals-goals-2500
//! (the same with the AI goal compared: kind 2 at (9, 20) from n 754, (6, 19) from the farm job at n 1004, (5, 19) after
//! the pass of n 2401, erratum 7) match in full.
//!
//! Round 86: xhuman12-xhuman12-picker-flip-aigoals-1700 (the target pickers' reference store is measured from the
//! store's top-left tile, so the waves store (36, 5) at n 101 to 201, 301, 501, 551 and 601 and (13, 66) at n 51 and 351,
//! with the AI goal compared, erratum 1) matches in full.
//!
//! Round 90: the game-start and load rules and the vision extras. human1-human01-cheat-load-game-50 (a load keeps the
//! cheat-used mark), human1-human01-saveload-game-300 (a load restarts the cycle counter, sets the victory and fog
//! countdowns to 1 and clears pause), human1-human01-unite-control-game-600 and human1-human01-unite-saveload-game-650
//! (a load renumbers creation numbers to slot + 1), human12-human12-invisible-passenger-game-vision-4550-5600 (an inside
//! unit is detected by everyone), mp-dark-paths-bne-game-vision-300 and mp-dark-paths-bne-fogoff-game-vision-300 (fog off
//! sets every seen bit), mp-rose-petal-bne-game-21300, and the computer-player goal windows orc13-aigoals-goals-17900-18200,
//! xhuman11-aigoals-goals-0, xhuman9-aigoals-goals-8630-8830 and xorc9-aigoals-goals-0 (the conversion goal rule, TC1 to
//! TC6 and AG1 to AG4) match in full. (its runestone trace, xhuman9-xhuman9-runestone-saveload-game-60, matches since round 92.)
//!
//! Round 91: human1-human01-fog-switch-fogmaps-game-400 (F45: the option switched off assigns the local set on
//! unexplored tiles, switched on changes no map; the comparer reads its `say` and `fog` records and the `fogmaps`
//! extra) matches in full.
//!
//! Round 91: a load runs the mission start again (G24): xhuman9-xhuman9-runestone-alive-saveload-game-60 now has the
//! damaged runestone back at 400 hit points in the loaded game's setup record (it was 4 before the change) and matches
//! to n 11; it matches in full since round 92.
//!
//! Round 92: a load restarts the 50-cycle pass counter at 1 and doubles the footmen, archers, siege and knights training
//! counts (SL1 to SL6): xhuman9-xhuman9-runestone-saveload-game-60, xhuman9-xhuman9-runestone-alive-saveload-game-60,
//! xhuman9-xhuman9-r92-pending-game-30, xhuman9-xhuman9-r92-alive-pending-game-30 and
//! xhuman9-xhuman9-r92-aijobs-aijobs-game-60 (the `aijobs` extra: the four counts of each computer player are compared,
//! its `wanted` and `have` fields are not) match in full. The multiplayer-launch traces
//! mp-skirmish-bne-mp-skirmish-nobody-fight-game-600 and -1300 (a joiner leaves; an idle worker of a "nobody" player
//! doesn't react, LV19) match in full: the comparer skips their `peer` records, checks their commands' turns against
//! the Direct Connect turn length (12 cycles) and carries out a turn's `other` command as the player leaving.
//! xhuman9-xhuman9-runestone-saveload-game-400 matched to n 342 in round 92 and matches in full since round 93.
//!
//! Round 93: a load clears every research request and makes the done marks again, with the load's building marks
//! (SL9 to SL12), and every training group from footmen to submarines doubles (SL13 to SL15). The `airesearch` extra
//! (per computer player: requests, done marks, build list, limit, upgrade levels, spells) and the `aigroups` extra (all
//! 14 counts; the demolition count is not compared, the spec has none) are compared by
//! xhuman9-xhuman9-runestone-saveload-aigroups-airesearch-game-400, orc12-orc12-r93-groups-saveload-aigroups-game-60
//! and xorc6-xorc6-r93-groups-saveload-aigroups-game-60, which match in full. The multiplayer-launch traces
//! mp-battle-2-mp-battle2-nobody-combat-game-vision-1300 and -melee-game-vision-1450 (a "nobody" player's idle
//! footman and cannon tower find no target, LV20 and LV21) match in full; the comparer finds the classic map
//! `Battle_2.pud` in the install's `Maps/Classic/Expansion` folder.
//!
//! Round 94: a new game's script openings run before the map's units are placed (AS-V1 to AS-V3) and a load keeps the
//! saved script state, so the build-list limit is compared from the first record. The new rows orc10-aigroups-airesearch-12,
//! xhuman6-xhuman6-r94-keepcastle-saveload-aigroups-airesearch-game-30, xorc11-xorc11-r94-castles-saveload-aigroups-airesearch-game-30,
//! xhuman9-xhuman9-r94-level-resave-aigroups-airesearch-game-60 (two saves and two loads) and
//! mp-battle-2-mp-battle2-nobody-busy-game-vision-1450 match in full; xhuman8-xhuman8-r94-groups-saveload-aigroups-game-60
//! matches to n 43 of the loaded game and its full row is ignored (see its test).
//!
//! Windowed traces (trace format, "Windowed traces") compare from their `from` cycle; their short resync pass covers
//! the 300 cycles after it. xhuman3-15500-17000, xhuman3-20900-22915 (the mission's defeat), xorc5-26000-30000 and
//! xorc10-19400-24000 match to their ends.
//!
//! The multiplayer-launch traces (trace format, "Multiplayer launch") name their map in the header. The comparer
//! derives the setup's generator state and computer slots (round-58 errata) and takes only the lobby's choices (the
//! humans' slots and races, the computer rows) from the header. Headers from round 60 on carry `opponents` and
//! `starts`; older ones have random starts and their computer controllers are counted. All match in full:
//! mp-gold-rush-bne-fixed-6000, mp-dark-paths-bne-fixed-1000 and mp-rose-petal-bne-fixed-3000 (Fixed Order),
//! mp-dark-paths-bne-1000,
//! mp-gold-rush-bne-6000, mp-gold-rush-bne-25000, mp-great-white-north-bne-6000, mp-big-rock-candy-mountain-bne-6000,
//! mp-big-rock-candy-mountain-bne-25000, mp-big-rock-candy-mountain-bne-40000, mp-schwartzwald-bne-6000,
//! mp-schwartzwald-bne-25000, mp-rose-petal-bne-10000, mp-rose-petal-bne-25000, mp-isolation-bne-30000,
//! mp-frosty-fjords-bne-30000, mp-frosty-fjords-bne-50000, mp-kaboom-bne-40000 and mp-kaboom-bne-70000 (Enhanced; its capture ends at n 67739, when
//! the game ended), and mp-dark-paths-bne-classic-1000,
//! mp-gold-rush-bne-classic-8000 and mp-gold-rush-bne-classic-20000 (Classic mode). xorc12-reach-400 matches in full.
//!
//! Round 96: a load clears every dead body's target and no other unit's (SL28 to SL30), puts the owner lists in
//! descending slot order (`core/unit-slots.md`, "Saved games") and builds the vertical list again unit by unit in slot
//! order (`orders/orders.md`, "Vertical order"): xorc11-xorc11-r96-targets-saveload-game-300 matches in full, and
//! xhuman2-xhuman2-r95-upgrading-saveload-aigroups-airesearch-game-1700 no longer needs `unit.target` left out. The
//! `aiworkers` extra (trace format, "Computer worker jobs") is compared where a trace carries it:
//! xorc10-xorc10-r96-repair-saveload-airesearch-aiworkers-game-6000 (SL31, SL32) and the round-95 xhuman8 row match in
//! full. The second human's commands are checked ("The second human"): mp-battle-2-mp-battle2-nobody-moving matches
//! in full, and mp-dark-paths-bne-mp-r96-allowsync-desync-rng-game-400 up to the cycle before the traced player's
//! generator is flipped (n = 150).
//!
//! Round 98: a transport's repair counter is its stored goal x, which the comparer now reads for a transport as for a
//! building (`economy/repair.md`, RP8; trace format, `goal` row): xhuman6-xhuman06-r98-transport-repair-goals-2500
//! (20 repair runs, every one paid) matches in full.
//!
//! Round 99: mp-r99-tests-mp-r99-tests-fixed-aigoals-airesearch-aiworkers-5400 plays the generated map `R99-tests.pud`,
//! which our own code builds (`tidewater_formats::testmap`, from `oracle/test-maps.md`; no map file is read; V285 to
//! V287, the follow row of V282 and the computer worker's no-depot town hall), and
//! xorc11-xorc11-r96-targets-saveload-game-goals-300 compares the six dead bodies' goals in the loaded setup record
//! (SL33). Both match in full.
//!
//! Rounds 100 and 101: mp-r100-tests-mp-r100-tests-fixed-goals-3000 plays the generated map `R100-tests.pud` (with its
//! sea; `oracle/test-maps.md`), mp-r100-tests-mp-r101-site-repair-two-fixed-goals-1000 a repair on a construction
//! site (RP10 to RP12), human1-human01-r101-hold-workers-goals-700 the Hold workers and Release workers commands
//! (HW1 to HW6), and mp-r100-tests-mp-r101-transport-goals-fixed-goals-1100 a transport's goal x and y in every record
//! (V43 to V45) except n = 326 to 329 (TG4, an open question: the turn record 326 and the cycles after it show the
//! target tile (40, 52) until the push-off at n = 330; `-transport-goal@326-329`).

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
    // The short resync pass covers the first 300 compared cycles: from the window's start in a windowed trace.
    let from = window_start(&trace);
    let short = limit.unwrap_or(u32::MAX).min(from + 300).to_string();
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
        assert!(
            !stdout.contains("second human's command not allowed"),
            "{file}: a command of the second human breaks the trace format's check\n{stdout}"
        );
    }
}

/// A windowed trace's first compared cycle (the header's `from`), or 0 for a whole trace.
fn window_start(trace: &std::path::Path) -> u32 {
    use std::io::BufRead;
    let file = std::fs::File::open(trace).expect("trace opens");
    let mut header = String::new();
    std::io::BufReader::new(flate2::read::GzDecoder::new(file))
        .read_line(&mut header)
        .expect("trace header reads");
    let v: serde_json::Value = serde_json::from_str(&header).expect("trace header parses");
    v.get("from").and_then(|f| f.as_u64()).map_or(0, |f| f as u32)
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
    xhuman8_25000: ("xhuman8-25000.jsonl.gz", "Campaign\\XHuman\\2XHum08.pud", 42, &[], None),
    human11_30000: ("human11-30000.jsonl.gz", "Campaign\\Human\\Human11.pud", 20, &[], None),
    orc9_orc09_shore_build_3000: ("orc9-orc09-shore-build-3000.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc7_22000: ("orc7-22000.jsonl.gz", "Campaign\\Orc\\Orc07.pud", 13, &[], None),
    xhuman7_25000: ("xhuman7-25000.jsonl.gz", "Campaign\\XHuman\\2XHum07.pud", 40, &[], None),
    mp_gold_rush_bne_fixed_6000: (mp, "mp-gold-rush-bne-fixed-6000.jsonl.gz", &[], None),
    mp_dark_paths_bne_fixed_1000: (mp, "mp-dark-paths-bne-fixed-1000.jsonl.gz", &[], None),
    mp_rose_petal_bne_fixed_3000: (mp, "mp-rose-petal-bne-fixed-3000.jsonl.gz", &[], None),
    mp_kaboom_bne_40000: (mp, "mp-kaboom-bne-40000.jsonl.gz", &[], None),
    xorc11_16000: ("xorc11-16000.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], None),
    orc9_orc09_smart_move_2000: ("orc9-orc09-smart-move-2000.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_attack_move_1500: ("orc9-orc09-attack-move-1500.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_return_goods_2200: ("orc9-orc09-return-goods-2200.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_shore_no_corner_600: ("orc9-orc09-shore-no-corner-600.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_tanker_move_2200: ("orc9-orc09-tanker-move-2200.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_enter_dying_1800: ("orc9-orc09-enter-dying-1800.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_enter_nothing_1900: ("orc9-orc09-enter-nothing-1900.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_smart_target_2100: ("orc9-orc09-smart-target-2100.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_flee_walk_1200: ("orc9-orc09-flee-walk-1200.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_flee_chop_walk_900: ("orc9-orc09-flee-chop-walk-900.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_return_nodepot_2200: ("orc9-orc09-return-nodepot-2200.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_patrol_countdown_1500: ("orc9-orc09-patrol-countdown-1500.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    mp_kaboom_bne_70000: (mp, "mp-kaboom-bne-70000.jsonl.gz", &[], None),
    orc9_orc09_return_inside_1200: ("orc9-orc09-return-inside-1200.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc12_orc12_board_attacked_1000: ("orc12-orc12-board-attacked-1000.jsonl.gz", "Campaign\\Orc\\Orc12.pud", 23, &[], None),
    orc9_orc09_follow_attacked_1200: ("orc9-orc09-follow-attacked-1200.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    xorc6_30000: ("xorc6-30000.jsonl.gz", "Campaign\\XOrc\\2XOrc06.pud", 39, &[], None),
    xorc5_18500: ("xorc5-18500.jsonl.gz", "Campaign\\XOrc\\2XOrc05.pud", 37, &[], None),
    xhuman5_30000: ("xhuman5-30000.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    mp_frosty_fjords_bne_50000: (mp, "mp-frosty-fjords-bne-50000.jsonl.gz", &[], None),
    orc9_orc09_follow_flee_1300: ("orc9-orc09-follow-flee-1300.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc12_orc12_guard_board_full_900: ("orc12-orc12-guard-board-full-900.jsonl.gz", "Campaign\\Orc\\Orc12.pud", 23, &[], None),
    orc10_35000: ("orc10-35000.jsonl.gz", "Campaign\\Orc\\Orc10.pud", 19, &[], None),
    orc8_30000: ("orc8-30000.jsonl.gz", "Campaign\\Orc\\Orc08.pud", 15, &[], None),
    xorc4_30000: ("xorc4-30000.jsonl.gz", "Campaign\\XOrc\\2XOrc04.pud", 35, &[], None),
    orc9_orc09_repair_water_1300: ("orc9-orc09-repair-water-1300.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_order16_keep_2000: ("orc9-orc09-order16-keep-2000.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    human13_sea_fight_3000: ("human13-sea-fight-3000.jsonl.gz", "Campaign\\Human\\Human13.pud", 24, &[], None),
    orc3_30000: ("orc3-30000.jsonl.gz", "Campaign\\Orc\\Orc03.pud", 5, &[], None),
    orc6_30000: ("orc6-30000.jsonl.gz", "Campaign\\Orc\\Orc06.pud", 11, &[], None),
    xhuman1_30000: ("xhuman1-30000.jsonl.gz", "Campaign\\XHuman\\2XHum01.pud", 28, &[], None),
    xorc2_30000: ("xorc2-30000.jsonl.gz", "Campaign\\XOrc\\2XOrc02.pud", 31, &[], None),
    xhuman3_15500_17000: ("xhuman3-15500-17000.jsonl.gz", "Campaign\\XHuman\\2XHum03.pud", 32, &[], None),
    xhuman3_20900_22915: ("xhuman3-20900-22915.jsonl.gz", "Campaign\\XHuman\\2XHum03.pud", 32, &[], None),
    xorc5_26000_30000: ("xorc5-26000-30000.jsonl.gz", "Campaign\\XOrc\\2XOrc05.pud", 37, &[], None),
    xorc10_19400_24000: ("xorc10-19400-24000.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
    orc9_orc09_ground_move_goals_1100: ("orc9-orc09-ground-move-goals-1100.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_hold_resume_hits_goals_1600: ("orc9-orc09-hold-resume-hits-goals-1600.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_lost_resume_goals_500: ("orc9-orc09-lost-resume-goals-500.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_carry_chop_1400: ("orc9-orc09-carry-chop-1400.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    human01_order15_lost_700: ("human01-order15-lost-700.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human12_unload_then_move_900: ("human12-unload-then-move-900.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    orc09_sea_patrol_axis_2000: ("orc09-sea-patrol-axis-2000.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc11_orc11_sapper_attack_move_goals_1500: ("orc11-orc11-sapper-attack-move-goals-1500.jsonl.gz", "Campaign\\Orc\\Orc11.pud", 21, &[], None),
    human12_unload_then_refused_goals_900: ("human12-unload-then-refused-goals-900.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    orc11_orc11_sapper_attack_move_goals_hits_1500: ("orc11-orc11-sapper-attack-move-goals-hits-1500.jsonl.gz", "Campaign\\Orc\\Orc11.pud", 21, &[], None),
    orc9_orc09_carry_chop_goals_1400: ("orc9-orc09-carry-chop-goals-1400.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc9_orc09_r97_built_farm_repair_goals_2400: ("orc9-orc09-r97-built-farm-repair-goals-2400.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    human06_site_kill_800: ("human06-site-kill-800.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human06_alerts_1400: ("human06-alerts-1400.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human06_depot_kill_2400: ("human06-depot-kill-2400.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human12_unload_then_patrol_900: ("human12-unload-then-patrol-900.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    human06_cancel_job_500: ("human06-cancel-job-500.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human06_cancel_site_600: ("human06-cancel-site-600.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human06_arrive_broke_700: ("human06-arrive-broke-700.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    orc09_strike_inside_hits_400: ("orc09-strike-inside-hits-400.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    orc09_missile_inside_hits_600: ("orc09-missile-inside-hits-600.jsonl.gz", "Campaign\\Orc\\Orc09.pud", 17, &[], None),
    human13_splash_air_hits_1700: ("human13-splash-air-hits-1700.jsonl.gz", "Campaign\\Human\\Human13.pud", 24, &[], None),
    human13_invisible_work_hits_900: ("human13-invisible-work-hits-900.jsonl.gz", "Campaign\\Human\\Human13.pud", 24, &[], None),
    human05_ally_kill_hits_1000: ("human05-ally-kill-hits-1000.jsonl.gz", "Campaign\\Human\\Human05.pud", 8, &[], None),
    human06_build_stats_1800: ("human06-build-stats-1800.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    human06_carry_build_1000: ("human06-carry-build-1000.jsonl.gz", "Campaign\\Human\\Human06.pud", 10, &[], None),
    xhuman10_failcast_520: ("xhuman10-failcast-520.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xorc12_spells_hits_1150_1260: ("xorc12-spells-hits-1150-1260.jsonl.gz", "Campaign\\XOrc\\2XOrc12.pud", 51, &[], None),
    xhuman10_slow_killed_goals_400: ("xhuman10-slow-killed-goals-400.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xhuman10_shield_mine_hits_800: ("xhuman10-shield-mine-hits-800.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xhuman10_shield_self_200: ("xhuman10-shield-self-200.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xhuman10_invisible_reach_goals_400: ("xhuman10-invisible-reach-goals-400.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xhuman10_heal_ballista_goals_500: ("xhuman10-heal-ballista-goals-500.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xhuman10_invisible_dwarves_150: ("xhuman10-invisible-dwarves-150.jsonl.gz", "Campaign\\XHuman\\2XHum10.pud", 46, &[], None),
    xorc10_xorc10_losses_4000: ("xorc10-xorc10-losses-4000.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
    orc3_orc03_platform_losses_9500: ("orc3-orc03-platform-losses-9500.jsonl.gz", "Campaign\\Orc\\Orc03.pud", 5, &[], None),
    xhuman5_xhuman05_worker_all_out_goals_1100: ("xhuman5-xhuman05-worker-all-out-goals-1100.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    xhuman2_xhuman02_cheat_scope_1600: ("xhuman2-xhuman02-cheat-scope-1600.jsonl.gz", "Campaign\\XHuman\\2XHum02.pud", 30, &[], None),
    human5_human05_rescue_jobs_2000: ("human5-human05-rescue-jobs-2000.jsonl.gz", "Campaign\\Human\\Human05.pud", 8, &[], None),
    xhuman5_xhuman05_worker_order12_goals_3000: ("xhuman5-xhuman05-worker-order12-goals-3000.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    orc3_orc03_tanker_order4_goals_7500_10500: ("orc3-orc03-tanker-order4-goals-7500-10500.jsonl.gz", "Campaign\\Orc\\Orc03.pud", 5, &[], None),
    human5_30000: ("human5-30000.jsonl.gz", "Campaign\\Human\\Human05.pud", 8, &[], None),
    xorc10_xorc10_farm_repair_1700: ("xorc10-xorc10-farm-repair-1700.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
    xhuman5_xhuman05_worker_all_out_aigoals_goals_2500: ("xhuman5-xhuman05-worker-all-out-aigoals-goals-2500.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    xhuman5_xhuman05_worker_all_out_goals_3000: ("xhuman5-xhuman05-worker-all-out-goals-3000.jsonl.gz", "Campaign\\XHuman\\2XHum05.pud", 36, &[], None),
    xhuman12_xhuman12_picker_flip_aigoals_1700: ("xhuman12-xhuman12-picker-flip-aigoals-1700.jsonl.gz", "Campaign\\XHuman\\2XHum12.pud", 50, &[], None),
    human1_human01_cheat_load_game_50: ("human1-human01-cheat-load-game-50.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human1_human01_saveload_game_300: ("human1-human01-saveload-game-300.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human1_human01_unite_control_game_600: ("human1-human01-unite-control-game-600.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human1_human01_unite_saveload_game_650: ("human1-human01-unite-saveload-game-650.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    human12_human12_invisible_passenger_game_vision_4550_5600: ("human12-human12-invisible-passenger-game-vision-4550-5600.jsonl.gz", "Campaign\\Human\\Human12.pud", 22, &[], None),
    mp_dark_paths_bne_fogoff_game_vision_300: (mp, "mp-dark-paths-bne-fogoff-game-vision-300.jsonl.gz", &[], None),
    mp_dark_paths_bne_game_vision_300: (mp, "mp-dark-paths-bne-game-vision-300.jsonl.gz", &[], None),
    mp_rose_petal_bne_game_21300: (mp, "mp-rose-petal-bne-game-21300.jsonl.gz", &[], None),
    orc13_aigoals_goals_17900_18200: ("orc13-aigoals-goals-17900-18200.jsonl.gz", "Campaign\\Orc\\Orc13.pud", 25, &[], None),
    xhuman11_aigoals_goals_0: ("xhuman11-aigoals-goals-0.jsonl.gz", "Campaign\\XHuman\\2XHum11.pud", 48, &[], None),
    xhuman9_aigoals_goals_8630_8830: ("xhuman9-aigoals-goals-8630-8830.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    human1_human01_fog_switch_fogmaps_game_400: ("human1-human01-fog-switch-fogmaps-game-400.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
    xorc9_aigoals_goals_0: ("xorc9-aigoals-goals-0.jsonl.gz", "Campaign\\XOrc\\2XOrc09.pud", 45, &[], None),
    xhuman9_xhuman9_r92_pending_game_30: ("xhuman9-xhuman9-r92-pending-game-30.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    xhuman9_xhuman9_r92_alive_pending_game_30: ("xhuman9-xhuman9-r92-alive-pending-game-30.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    xhuman9_xhuman9_r92_aijobs_aijobs_game_60: ("xhuman9-xhuman9-r92-aijobs-aijobs-game-60.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    mp_skirmish_bne_mp_skirmish_nobody_fight_game_600: (mp, "mp-skirmish-bne-mp-skirmish-nobody-fight-game-600.jsonl.gz", &[], None),
    mp_skirmish_bne_mp_skirmish_nobody_fight_game_1300: (mp, "mp-skirmish-bne-mp-skirmish-nobody-fight-game-1300.jsonl.gz", &[], None),
    mp_battle_2_mp_battle2_nobody_combat_game_vision_1300: (mp, "mp-battle-2-mp-battle2-nobody-combat-game-vision-1300.jsonl.gz", &[], None),
    mp_battle_2_mp_battle2_nobody_melee_game_vision_1450: (mp, "mp-battle-2-mp-battle2-nobody-melee-game-vision-1450.jsonl.gz", &[], None),
    xhuman9_xhuman9_runestone_saveload_aigroups_airesearch_game_400: ("xhuman9-xhuman9-runestone-saveload-aigroups-airesearch-game-400.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    orc12_orc12_r93_groups_saveload_aigroups_game_60: ("orc12-orc12-r93-groups-saveload-aigroups-game-60.jsonl.gz", "Campaign\\Orc\\Orc12.pud", 23, &[], None),
    xorc6_xorc6_r93_groups_saveload_aigroups_game_60: ("xorc6-xorc6-r93-groups-saveload-aigroups-game-60.jsonl.gz", "Campaign\\XOrc\\2XOrc06.pud", 39, &[], None),
    orc10_aigroups_airesearch_12: ("orc10-aigroups-airesearch-12.jsonl.gz", "Campaign\\Orc\\Orc10.pud", 19, &[], None),
    xhuman6_xhuman06_r98_transport_repair_goals_2500: ("xhuman6-xhuman06-r98-transport-repair-goals-2500.jsonl.gz", "Campaign\\XHuman\\2XHum06.pud", 38, &[], None),
    xhuman6_xhuman6_r94_keepcastle_saveload_aigroups_airesearch_game_30: ("xhuman6-xhuman6-r94-keepcastle-saveload-aigroups-airesearch-game-30.jsonl.gz", "Campaign\\XHuman\\2XHum06.pud", 38, &[], None),
    xorc11_xorc11_r94_castles_saveload_aigroups_airesearch_game_30: ("xorc11-xorc11-r94-castles-saveload-aigroups-airesearch-game-30.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], None),
    xhuman9_xhuman9_r94_level_resave_aigroups_airesearch_game_60: ("xhuman9-xhuman9-r94-level-resave-aigroups-airesearch-game-60.jsonl.gz", "Campaign\\XHuman\\2XHum09.pud", 44, &[], None),
    mp_battle_2_mp_battle2_nobody_busy_game_vision_1450: (mp, "mp-battle-2-mp-battle2-nobody-busy-game-vision-1450.jsonl.gz", &[], None),
    xhuman8_xhuman8_r95_workers_saveload_aigroups_aiworkers_game_1500: ("xhuman8-xhuman8-r95-workers-saveload-aigroups-aiworkers-game-1500.jsonl.gz", "Campaign\\XHuman\\2XHum08.pud", 42, &[], None),
    human14_human14_r95_spell_saveload_aigroups_airesearch_game_60: ("human14-human14-r95-spell-saveload-aigroups-airesearch-game-60.jsonl.gz", "Campaign\\Human\\Human14.pud", 26, &[], None),
    mp_battle_2_mp_battle2_nobody_moving_game_goals_vision_1450: (mp, "mp-battle-2-mp-battle2-nobody-moving-game-goals-vision-1450.jsonl.gz", &[], None),
    xorc11_xorc11_r96_targets_saveload_game_300: ("xorc11-xorc11-r96-targets-saveload-game-300.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], None),
    xorc10_xorc10_r96_repair_saveload_airesearch_aiworkers_game_6000: ("xorc10-xorc10-r96-repair-saveload-airesearch-aiworkers-game-6000.jsonl.gz", "Campaign\\XOrc\\2XOrc10.pud", 47, &[], None),
    mp_r99_tests_mp_r99_tests_fixed_aigoals_airesearch_aiworkers_5400: (mp, "mp-r99-tests-mp-r99-tests-fixed-aigoals-airesearch-aiworkers-5400.jsonl.gz", &[], None),
    xorc11_xorc11_r96_targets_saveload_game_goals_300: ("xorc11-xorc11-r96-targets-saveload-game-goals-300.jsonl.gz", "Campaign\\XOrc\\2XOrc11.pud", 49, &[], None),
    // The traced player's rng is flipped by the harness at n = 150 (the `peer` record's `old` and `new`), so the generator
    // differs from n = 151 on: the rows are compared up to the cycle before. With the generator resynced the whole trace
    // (n = 0 to 400, the drop at turn 170 included) matches.
    mp_dark_paths_bne_mp_r96_allowsync_desync_rng_game_400: (mp, "mp-dark-paths-bne-mp-r96-allowsync-desync-rng-game-400.jsonl.gz", &[], Some(150)),
    mp_r100_tests_mp_r100_tests_fixed_goals_3000: (mp, "mp-r100-tests-mp-r100-tests-fixed-goals-3000.jsonl.gz", &[], None),
    mp_r100_tests_mp_r101_site_repair_two_fixed_goals_1000: (mp, "mp-r100-tests-mp-r101-site-repair-two-fixed-goals-1000.jsonl.gz", &[], None),
    mp_r100_tests_mp_r101_transport_goals_fixed_goals_1100: (mp, "mp-r100-tests-mp-r101-transport-goals-fixed-goals-1100.jsonl.gz", &["--fields=-transport-goal@326-329"], None),
    human1_human01_r101_hold_workers_goals_700: ("human1-human01-r101-hold-workers-goals-700.jsonl.gz", "Campaign\\Human\\Human01.pud", 0, &[], None),
}

/// The whole SL21 trace (flying machine, transport, battleship and submarine counts double on a load) matches in full
/// since round 95: a load sets the job counts to 0 while the workers keep their jobs (SL22 to SL25), so the peon
/// leaving the town hall at n = 44 goes to the gold mine.
#[test]
fn xhuman8_xhuman8_r94_groups_saveload_aigroups_game_60() {
    check(
        "xhuman8-xhuman8-r94-groups-saveload-aigroups-game-60.jsonl.gz",
        Some(("Campaign\\XHuman\\2XHum08.pud", 42)),
        &[],
        None,
    );
}

/// SL27 (a building upgrading at the save marks as its current type, its upgrade's entry staying open) and SL29 (a load
/// clears every dead body's target: the ogre bodies 1579, 1581 and 1583 had targets at the save). The whole trace
/// matches through n = 1700 (the done marks, the request for 0x98, the stronghold at n = 1547), `unit.target` included,
/// since round 96.
#[test]
fn xhuman2_xhuman2_r95_upgrading_saveload_aigroups_airesearch_game_1700() {
    check(
        "xhuman2-xhuman2-r95-upgrading-saveload-aigroups-airesearch-game-1700.jsonl.gz",
        Some(("Campaign\\XHuman\\2XHum02.pud", 30)),
        &[],
        None,
    );
}

/// The 400-cycle runestone save/load trace (SL7: after the load every unit's number is its slot plus one and the next
/// two units created get 1626 and 1627, at n = 252, then 1628 at n = 257) matches in full since round 93. In round 92
/// it diverged at n 343, where player 6's blacksmith started the research of code 0x88 in ours and not in the original:
/// a load clears the requests and makes the done marks again (SL9, SL10), and the research the blacksmith was running
/// at the save stays unmarked, so the scan stops at it and 0x88 is never requested.
#[test]
fn xhuman9_xhuman9_runestone_saveload_game_400() {
    check(
        "xhuman9-xhuman9-runestone-saveload-game-400.jsonl.gz",
        Some(("Campaign\\XHuman\\2XHum09.pud", 44)),
        &[],
        None,
    );
}

/// The runestone save/load trace and the round-91 runestone trace (a damaged runestone alive at the save) match in
/// full since round 92: a load restarts the 50-cycle pass counter at 1 (SL1 to SL3) and doubles the footmen, archers,
/// siege and knights counts (SL4 to SL6).
#[test]
fn xhuman9_xhuman9_runestone_saveload_game_60() {
    check(
        "xhuman9-xhuman9-runestone-saveload-game-60.jsonl.gz",
        Some(("Campaign\\XHuman\\2XHum09.pud", 44)),
        &[],
        None,
    );
}

#[test]
fn xhuman9_xhuman9_runestone_alive_saveload_game_60() {
    check(
        "xhuman9-xhuman9-runestone-alive-saveload-game-60.jsonl.gz",
        Some(("Campaign\\XHuman\\2XHum09.pud", 44)),
        &[],
        None,
    );
}
