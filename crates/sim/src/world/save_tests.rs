use super::*;
use crate::world::tests::{field, run};

/// A small battle: footmen against archers, a catapult, two buildings and a move order under way.
fn battle() -> World {
    let mut w = field(16, 16);
    for (kind, owner, tile) in [
        (0u8, 0u8, (2, 2)),
        (0, 0, (2, 3)),
        (8, 0, (1, 2)),
        (5, 0, (1, 4)),
        (0, 1, (9, 2)),
        (8, 1, (10, 3)),
        (0, 1, (9, 4)),
        (0x3a, 1, (12, 10)),
        (0x4a, 0, (4, 10)),
    ] {
        w.spawn(kind, owner, tile, 0).unwrap();
    }
    w.players[0].gold = 1234;
    w.players[1].lumber = 77;
    w.step(&[
        Command::Move {
            player: 0,
            units: vec![UnitId(1), UnitId(2)],
            to: (8, 3),
        },
        Command::Attack {
            player: 1,
            units: vec![UnitId(5)],
            target: UnitId(3),
        },
    ]);
    w
}

#[test]
fn save_then_load_reproduces_the_state() {
    let mut a = battle();
    run(&mut a, 60);
    assert!(
        a.units.iter().any(|u| u.order != Order::Idle),
        "something is going on at the save"
    );
    a.prepare_save();
    let bytes = a.save_state();
    let mut b = field(16, 16);
    b.restore(&bytes, 99).unwrap();
    a.rng = Rng::new(99);
    // What a load starts afresh (counter, pause, countdowns, creation numbers) is not what was saved.
    a.starts_afresh();
    assert_eq!(a.state_hash(), b.state_hash());
    assert_eq!(a.save_state(), b.save_state(), "everything stored comes back the same");
    for t in 0..300 {
        a.step(&[]);
        b.step(&[]);
        assert_eq!(a.state_hash(), b.state_hash(), "cycle {t} after the load");
    }
    assert_eq!(a.save_state(), b.save_state());
}

#[test]
fn the_pending_command_mark_comes_back() {
    // battle() gives its commands in the first cycle; their pending orders haven't taken over yet.
    let mut a = battle();
    assert!(a.units.iter().any(|u| u.commanded), "a command is pending at the save");
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    a.starts_afresh();
    assert_eq!(b.units, a.units);
}

/// Damage spec: a unit's attack cooldown, a dying unit's kept target and a killer's forgotten target are stored.
#[test]
fn the_cooldown_and_the_kept_and_forgotten_targets_come_back() {
    let mut a = battle();
    let attack = Order::Attack { target: UnitId(5) };
    a.units[0].cooldown = 61;
    a.units[0].forgot = Some(attack);
    a.units[4].dying_target = Some(3);
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    a.starts_afresh();
    assert_eq!(
        (b.units[0].cooldown, b.units[0].forgot, b.units[4].dying_target),
        (61, Some(attack), Some(3))
    );
    assert_eq!(b.units, a.units);
}

/// SL28 to SL30 and SL33: a load clears every dead body's target and stored goal (type 0x69, any decay stage and owner, wrecks included) and
/// no other unit's; a body keeps its target during play.
#[test]
fn a_load_clears_a_dead_bodys_target_and_no_other() {
    let mut a = battle();
    // A body in its remains stage (owner 1), another in its remove order (neutral), a dying unit and a dead unit that
    // leaves no remains (still its own type), and a live unit aiming at a body.
    for (i, owner, removing) in [(0usize, 1u8, false), (1, 15, true)] {
        let u = &mut a.units[i];
        u.kind = crate::corpses::DEAD_BODY;
        u.owner = owner;
        u.dying = true;
        u.remains = !removing;
        u.removing = removing;
        u.dying_target = Some(5);
        u.goal_x = 12;
    }
    a.units[2].dying = true;
    a.units[2].dying_target = Some(6);
    a.units[3].removing = true;
    a.units[3].owner = 15;
    a.units[3].dying = true;
    a.units[3].dying_target = Some(7);
    a.units[2].goal_x = 12;
    a.units[3].goal_x = 12;
    a.units[4].order = Order::Attack { target: UnitId(1) };
    let live_target = a.units[4].recorded_target();
    assert!(live_target.is_some());
    // During play the bodies keep reporting their targets.
    run(&mut a, 1);
    assert_eq!((a.units[0].dying_target, a.units[1].dying_target), (Some(5), Some(5)));
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert_eq!(
        (b.units[0].dying_target, b.units[1].dying_target),
        (None, None),
        "SL28, SL29"
    );
    // SL33: a body's stored goal is zero after a load; every other unit's is kept.
    assert_eq!((b.units[0].goal_x, b.units[1].goal_x), (0, 0), "SL33");
    assert_eq!(
        (b.units[2].goal_x, b.units[3].goal_x),
        (12, 12),
        "other units keep their goal"
    );
    assert_eq!(b.units[2].dying_target, Some(6), "SL30: a dying unit keeps its target");
    assert_eq!(
        b.units[3].dying_target,
        Some(7),
        "SL28: a dead unit with no remains keeps its target"
    );
    assert_eq!(
        b.units[4].recorded_target(),
        live_target,
        "a live unit aiming at a body keeps it"
    );
}

#[test]
fn the_generator_starts_afresh_from_the_seed() {
    let mut a = battle();
    run(&mut a, 20);
    let bytes = a.save_state();
    let mut b = field(16, 16);
    b.restore(&bytes, 5).unwrap();
    let mut c = field(16, 16);
    c.restore(&bytes, 6).unwrap();
    assert_eq!(b.rng, Rng::new(5));
    assert_eq!(c.rng, Rng::new(6));
    assert_ne!(b.state_hash(), c.state_hash());
}

#[test]
fn cheats_start_as_for_a_new_game() {
    use crate::cheats::{Flags, flag};
    let mut a = battle();
    a.start_cheats(Flags(flag::REVEAL_MAP | flag::BUILD_SPEED | flag::MISSION_SELECT));
    let bytes = a.save_state();
    let mut b = field(16, 16);
    b.restore(&bytes, 1).unwrap();
    assert!(
        !b.rules.reveal_map && !b.rules.build_speed_cheat,
        "the map isn't revealed after a load"
    );
    assert_eq!(b.cheats.flags, Flags::default());
    // The flags that carry across games are the template's.
    let mut c = field(16, 16);
    c.start_cheats(Flags(flag::MISSION_SELECT));
    c.restore(&bytes, 1).unwrap();
    assert_eq!(c.cheats.flags, Flags(flag::MISSION_SELECT));
    assert!(!c.rules.reveal_map);
}

/// `game/save-load.md`, "Cheats" (traced, `human1-human01-unite-saveload-game-650`): a world that loads over one with
/// the instant-victory cheat on doesn't keep the effect, so the check after the load doesn't find the cheat's outcome.
#[test]
fn a_load_over_a_world_with_a_cheat_on_clears_its_effect() {
    use crate::cheats::{Flags, flag};
    let bytes = battle().save_state();
    let mut b = field(16, 16);
    b.start_cheats(Flags(flag::INSTANT_VICTORY | flag::INVULNERABLE));
    assert!(b.rules.instant_victory && b.rules.invulnerable_cheat);
    b.restore(&bytes, 1).unwrap();
    assert!(!b.rules.instant_victory && !b.rules.invulnerable_cheat);
}

#[test]
fn a_classic_save_drops_the_score() {
    let mut a = battle();
    a.rules.enhanced = false;
    run(&mut a, 30);
    a.players[0].score = 500;
    a.players[0].kills = 3;
    let bytes = a.save_state();
    let mut b = field(16, 16);
    b.rules.enhanced = false;
    b.restore(&bytes, 1).unwrap();
    assert_eq!(b.tick, 0);
    assert_eq!((b.players[0].score, b.players[0].kills), (0, 0));
    assert_eq!(b.players[0].gold, a.players[0].gold, "resources are kept");
    a.starts_afresh();
    assert_eq!(b.units, a.units);
    // Enhanced keeps the score (the cycle counter restarts at 0 in both modes).
    let mut c = battle();
    run(&mut c, 30);
    c.players[0].score = 500;
    let mut d = field(16, 16);
    d.restore(&c.save_state(), 1).unwrap();
    assert_eq!((d.tick, d.players[0].score), (0, 500));
}

#[test]
fn a_stored_world_for_another_map_or_garbage_is_refused() {
    let bytes = battle().save_state();
    let mut small = field(8, 8);
    assert_eq!(small.restore(&bytes, 1), Err(RestoreError::WrongMap));
    let mut b = field(16, 16);
    assert!(matches!(
        b.restore(&bytes[..bytes.len() / 2], 1),
        Err(RestoreError::Decode(_))
    ));
    assert!(matches!(b.restore(&[], 1), Err(RestoreError::Decode(_))));
}

#[test]
fn saving_with_the_map_revealed_resets_looks_on_unexplored_ground() {
    let mut a = battle();
    let b = a.units.iter().position(|u| u.building).unwrap();
    a.units[b].look = crate::minimap::Look(Some(crate::minimap::Picture {
        kind: a.units[b].kind,
        frame: 0,
        facing: 0,
    }));
    a.rules.reveal_map = true;
    a.prepare_save();
    assert!(a.rules.reveal_map, "switched back on");
    let (tx, ty) = a.units[b].tile;
    let dark = (0..2).all(|dy| (0..2).all(|dx| a.vision.unexplored(tx + dx, ty + dy)));
    assert_eq!(a.units[b].look.0.is_none(), dark);
}

#[test]
fn building_looks_and_progress_bytes_come_back() {
    // The round-33 state lives in fields every save keeps: a mine's "active" frame and occupant count, and a
    // building's progress in its mana byte.
    let mut a = battle();
    let farm = a.units.iter().position(|u| u.kind == 0x3a).unwrap();
    a.units[farm].script.frame = 1;
    a.units[farm].occupants = 2;
    a.units[farm].mana = 37;
    let bytes = a.save_state();
    let mut b = field(16, 16);
    b.restore(&bytes, 1).unwrap();
    let u = &b.units[farm];
    assert_eq!((u.script.frame, u.occupants, u.mana), (1, 2, 37));
}

#[test]
fn the_fingerprint_follows_the_whole_state() {
    let (mut a, mut b) = (battle(), battle());
    assert_eq!(a.fingerprint(), b.fingerprint());
    run(&mut a, 40);
    run(&mut b, 40);
    assert_eq!(
        a.fingerprint(),
        b.fingerprint(),
        "the same steps give the same fingerprint"
    );
    let before = (b.fingerprint(), b.state_hash());
    b.players[3].oil += 1;
    assert_ne!(b.fingerprint(), before.0);
    b.players[3].oil -= 1;
    assert_eq!((b.fingerprint(), b.state_hash()), before);
    // A stored field the sync hash leaves out still counts.
    b.next_id += 1;
    assert_eq!(b.state_hash(), before.1);
    assert_ne!(b.fingerprint(), before.0);
    b.next_id -= 1;
    b.rng.draw();
    assert_ne!(b.fingerprint(), before.0);
}

/// Round 90 (`game/save-load.md`, "What a loaded game starts afresh"; vectors G11 and G17): a load runs the two
/// start-up cycles like a new game, so the cycle counter, the victory countdown and the fog countdown start afresh
/// whatever was saved: the counter is 0 after the start-up cycles, the first victory check runs in the second
/// start-up cycle and then at counter 201, 402, ..., and the fog refreshes in the first start-up cycle.
#[test]
fn a_load_starts_the_counter_and_both_countdowns_afresh() {
    let mut a = battle();
    run(&mut a, 60);
    a.victory.countdown = 50;
    a.vision.countdown = 40;
    let bytes = a.save_state();
    let mut b = field(16, 16);
    b.restore(&bytes, 1).unwrap();
    assert_eq!((b.tick, b.victory.countdown, b.vision.countdown), (0, 1, 1));
    let units = b.units.clone();
    b.start_up();
    assert_eq!(b.tick, 0, "the counter is 0 again after the start-up cycles");
    assert_ne!(b.units, units, "they are full cycles: the units update");
    // G17 (traced, `human1-human01-saveload-game-300`): the victory countdown is 200 after the second start-up cycle
    // (its check ran in it) and the fog countdown 99 (the refresh ran in the first).
    assert_eq!((b.victory.countdown, b.vision.countdown), (RELOAD_VICTORY, 99));
    // The next check runs at counter 201.
    let mut counters = Vec::new();
    for _ in 0..403 {
        let before = b.victory.countdown;
        b.step(&[]);
        if before == 0 {
            counters.push(b.tick);
        }
    }
    assert_eq!(counters, [201, 402]);
}

const RELOAD_VICTORY: u32 = 200;

/// G18 (static study, `game/save-load.md`, "Pause"): a game saved while paused loads running.
#[test]
fn a_game_saved_paused_loads_running() {
    let mut a = battle();
    a.paused = true;
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert!(!b.paused);
    b.start_up();
    assert!(!b.paused);
    assert_eq!(b.victory.countdown, RELOAD_VICTORY);
}

/// Q4 (traced, G07 in `game/lifecycle.md`): an outcome a check found but didn't show yet is lost in a load; the rule
/// that was in force comes back, so the check finds the outcome again only if the game state still gives it.
#[test]
fn a_load_loses_a_found_outcome_and_restores_the_rule() {
    use crate::victory::{Outcome, Rule};
    let mut a = battle();
    let rule = Rule::Standard {
        destroy_all: true,
        rescue: None,
    };
    a.victory.rule = Some(rule);
    a.rules.instant_victory = true;
    a.victory.countdown = 0;
    a.step(&[]);
    assert_eq!(
        a.victory.rule,
        Some(Rule::Outcome(Outcome::Victory)),
        "found at the check"
    );
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert_eq!(b.victory.rule, Some(rule), "the outcome is lost");
    assert!(!b.rules.instant_victory, "the cheat flag isn't saved");
    b.start_up();
    run(&mut b, 600);
    assert!(
        b.victory.rule != Some(Rule::Outcome(Outcome::Victory)),
        "not found again"
    );
    assert!(!b.victory.over);
    // A switch made before the save (to destroy all) is kept by the saved rule.
    assert_eq!(b.victory.rule, Some(rule));
}

/// Round 90 (traced: 12 units in `human1-human01-saveload-game-300`, 200 in the runestone trace): a load renumbers every
/// unit's creation number to its slot plus one. Round 92 (SL7, traced): the next unit gets the unit-slot count plus 26,
/// whatever the units hold.
#[test]
fn a_load_renumbers_the_creation_numbers_to_slot_plus_one() {
    let mut a = battle();
    run(&mut a, 10);
    assert!(
        a.units.iter().any(|u| u.serial.0 != u.slot as u32 + 1),
        "not the case before the load"
    );
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert!(b.units.iter().all(|u| u.serial.0 == u.slot as u32 + 1));
    // Every other field is as saved, and the handles other state refers to are the same.
    assert_eq!(
        b.units
            .iter()
            .map(|u| (u.id, u.slot, u.kind, u.tile))
            .collect::<Vec<_>>(),
        a.units
            .iter()
            .map(|u| (u.id, u.slot, u.kind, u.tile))
            .collect::<Vec<_>>()
    );
    let n = b.spawn(0, 0, (6, 6), 0).unwrap();
    assert_eq!(b.unit(n).unwrap().serial.0, b.slots.count() as u32 + 26);
    let m = b.spawn(0, 0, (6, 8), 0).unwrap();
    assert_eq!(
        b.unit(m).unwrap().serial.0,
        b.slots.count() as u32 + 27,
        "then one more per unit"
    );
}

/// SL7: 1600 unit slots (Enhanced) give 1626, then 1627.
#[test]
fn the_first_creation_number_after_a_load_is_1626_with_1600_slots() {
    let mut a = field(16, 16);
    a.spawn(0, 0, (2, 2), 0).unwrap();
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    if b.slots.count() == 1600 {
        let n = b.spawn(0, 0, (6, 6), 0).unwrap();
        assert_eq!(b.unit(n).unwrap().serial.0, 1626);
    } else {
        assert_eq!(
            b.next_serial,
            b.slots.count() as u32 + 26,
            "the formula holds for any slot count"
        );
    }
}

/// SL1 to SL3 (traced): the 50-cycle pass counter isn't saved; a loaded game starts it at 1, so the passes run in its
/// first start-up cycle whatever phase the save was in.
#[test]
fn a_load_restarts_the_pass_counter_at_1() {
    let mut a = battle();
    run(&mut a, 20);
    assert!(a.army.countdown > 1, "the save is mid-phase");
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert_eq!(b.army.countdown, 1);
}

/// SL4 to SL6, SL13 to SL15 (traced; four groups static study): every training group count from footmen to submarines
/// of a computer player is restored and counted again: each load adds the counts at the save to the surplus. A dying
/// unit and a held unit aren't counted; the worker and tanker counts come back right.
#[test]
fn a_load_doubles_every_training_group_count_from_footmen_to_submarines() {
    let mut a = field(16, 16);
    a.players[1].controller = crate::ai::controller::COMPUTER;
    // 3 footmen (one a grunt), 2 archers, a catapult, a knight and a dying knight; then a mage, a gryphon, a flying
    // machine, a transport, two destroyers (one held, as a map ship is), a battleship and a submarine; two
    // workers and a tanker.
    for (kind, tile) in [
        (0u8, (2, 2)),
        (0, (2, 3)),
        (1, (2, 4)),
        (8, (3, 2)),
        (9, (3, 3)),
        (4, (4, 2)),
        (6, (4, 3)),
        (0x0a, (5, 2)),
        (0x16, (5, 3)),
        (0x28, (5, 4)),
        (0x1c, (6, 2)),
        (0x1e, (6, 3)),
        (0x1e, (6, 4)),
        (0x20, (7, 2)),
        (0x26, (7, 3)),
        (2, (8, 2)),
        (3, (8, 3)),
        (0x1a, (8, 4)),
    ] {
        a.spawn(kind, 1, tile, 0).unwrap();
    }
    let dying = a.spawn(6, 1, (4, 4), 0).unwrap();
    let i = a.units.iter().position(|u| u.id == dying).unwrap();
    a.units[i].dying = true;
    let held = a.units.iter().rposition(|u| u.kind == 0x1e).unwrap();
    a.units[held].held = true;
    let g = a.ai_groups_with_surplus(1);
    assert_eq!(g.doubled(), [3, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
    let (workers, tankers) = (g.workers, g.tankers);
    assert_eq!((workers, tankers), (2, 1));
    let mut b = field(16, 16);
    b.players[1].controller = crate::ai::controller::COMPUTER;
    b.restore(&a.save_state(), 1).unwrap();
    let g = b.ai_groups_with_surplus(1);
    assert_eq!(g.doubled(), [6, 4, 2, 2, 2, 2, 2, 2, 2, 2, 2], "doubled");
    assert_eq!((g.workers, g.tankers), (workers, tankers), "worked out afresh");
    // The count goes on rising and falling with the units from there.
    b.spawn(0, 1, (10, 6), 0).unwrap();
    assert_eq!(b.ai_groups_with_surplus(1).footmen, 7);
    // A further save and load adds the loaded units again.
    let mut c = field(16, 16);
    c.players[1].controller = crate::ai::controller::COMPUTER;
    c.restore(&b.save_state(), 1).unwrap();
    assert_eq!(c.ai_groups_with_surplus(1).footmen, 7 + 4);
    // A human player's counts aren't kept.
    assert_eq!(c.group_surplus[0], [0; crate::training_ai::DOUBLED_GROUPS]);
}

/// `game/save-load.md`, "What a saved game keeps" (static study): the frozen and about-to-freeze marks of workers are
/// cleared by a load.
#[test]
fn a_load_clears_the_frozen_marks() {
    let mut a = battle();
    a.units[0].freeze = crate::rescue::Freeze::Frozen;
    a.units[1].freeze = crate::rescue::Freeze::Freezing;
    a.units[2].freeze = crate::rescue::Freeze::Released;
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert!(b.units.iter().all(|u| u.freeze == crate::rescue::Freeze::None));
}

/// `game/save-load.md` SL22: a load sets every computer player's job counts and hall cooldown to 0 while the units
/// keep their jobs and harvest modes, so the first release wraps (SL23 to SL25).
#[test]
fn a_load_clears_the_computer_job_counts_and_keeps_the_jobs() {
    let mut a = battle();
    a.units[4].ai_job = crate::worker_ai::AiJob::Lumber;
    a.units[4].gather = crate::Gather::Lumber;
    a.ai[1].miners = 4;
    a.ai[1].lumberjacks = 7;
    a.ai[1].repairers = 2;
    a.ai[1].hall_cooldown = 5;
    a.ai[1].in_progress[0x3a] = 300;
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    let p = &b.ai[1];
    assert_eq!((p.miners, p.lumberjacks, p.repairers, p.hall_cooldown), (0, 0, 0, 0));
    assert!(p.in_progress.iter().all(|&n| n == 0));
    assert_eq!(b.units[4].ai_job, crate::worker_ai::AiJob::Lumber);
    assert_eq!(b.units[4].gather, crate::Gather::Lumber);
    b.release_job(4);
    assert_eq!(b.ai[1].lumberjacks, 65535, "SL23: the first release wraps");
}

/// The creation number counts up from 1 in a new game, one per created unit.
#[test]
fn creation_numbers_count_up_from_1_in_a_new_game() {
    let a = battle();
    let mut serials: Vec<u32> = a.units.iter().map(|u| u.serial.0).collect();
    serials.sort_unstable();
    assert_eq!(serials, (1..=a.units.len() as u32).collect::<Vec<_>>());
}

/// Round 90 (traced, `human1-human01-cheat-load-game-50`): a load keeps the cheat-used mark as the template has it.
#[test]
fn a_load_keeps_the_cheat_used_mark() {
    let a = battle();
    let mut b = field(16, 16);
    b.cheats.used = true;
    b.restore(&a.save_state(), 1).unwrap();
    assert!(b.cheats.used);
    let mut c = field(16, 16);
    c.restore(&a.save_state(), 1).unwrap();
    assert!(!c.cheats.used);
}

/// A Classic load starts counting at 0 as well.
#[test]
fn a_classic_load_starts_counting_at_0() {
    let mut a = battle();
    a.rules.enhanced = false;
    run(&mut a, 30);
    let mut b = field(16, 16);
    b.rules.enhanced = false;
    b.restore(&a.save_state(), 1).unwrap();
    b.start_up();
    assert_eq!(b.tick, 0);
}

/// X12 (`game/rescue.md`, `game/save-load.md`): each player's rescued count is saved and comes back.
#[test]
fn the_rescued_counts_come_back() {
    let mut a = battle();
    a.players[0].rescued = 3;
    a.players[1].rescued = 0x1001;
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert_eq!(
        (b.players[0].rescued, b.players[1].rescued, b.players[2].rescued),
        (3, 0x1001, 0)
    );
}

/// Repair spec "Cost and rate" (round 97): the counter is part of a unit's stored state, so a load keeps it, a
/// building's and a mobile unit's alike.
#[test]
fn a_load_keeps_the_repair_counter() {
    let mut a = battle();
    let (unit, farm) = (0, a.units.iter().position(|u| u.kind == 0x3a).unwrap());
    a.units[unit].goal_x = 9;
    a.units[farm].goal_x = 1224;
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
    assert_eq!((b.units[unit].goal_x, b.units[farm].goal_x), (9, 1224));
}

/// SL34 to SL37 (traced for the land and sea armies): the land, sea and air army counts of the script conditions are
/// restored and counted again at a load, so they double and the surplus stays; a held ship and a dying unit are
/// not counted; the worker count comes back right; a further load adds again.
#[test]
fn a_load_doubles_the_army_counts_the_conditions_read() {
    use crate::ai::Counts;
    use tidewater_formats::udta::flags;
    let army = || {
        let mut w = field(16, 16);
        w.players[1].controller = crate::ai::controller::COMPUTER;
        let atk = flags::CAN_ATTACK;
        for k in [0usize, 6] {
            w.stats.0[k].flags = flags::LAND | atk;
        }
        w.stats.0[0x1e].flags = flags::SEA | atk;
        w.stats.0[0x16].flags = flags::AIR | atk;
        w.stats.0[2].flags = flags::LAND | flags::PEON;
        w
    };
    let mut a = army();
    // Land: 2 footmen and a knight; sea: a destroyer and a held destroyer; air: a gryphon; a worker.
    for (kind, tile) in [
        (0u8, (2, 2)),
        (0, (2, 3)),
        (6, (4, 3)),
        (0x1e, (6, 3)),
        (0x1e, (6, 4)),
        (0x16, (5, 3)),
        (2, (8, 2)),
    ] {
        a.spawn(kind, 1, tile, 0).unwrap();
    }
    let held = a.units.iter().rposition(|u| u.kind == 0x1e).unwrap();
    a.units[held].held = true;
    let live = a.ai_counts(1);
    assert_eq!((live.land, live.sea, live.air), (3, 1, 1));
    let mut b = army();
    b.restore(&a.save_state(), 1).unwrap();
    let c = b.ai_counts(1);
    assert_eq!(
        c,
        Counts {
            workers: live.workers,
            land: 6,
            sea: 2,
            air: 2
        }
    );
    // The count goes on rising with the units, and a further load adds the loaded units again.
    b.spawn(0, 1, (10, 6), 0).unwrap();
    assert_eq!(b.ai_counts(1).land, 7);
    let mut d = army();
    d.restore(&b.save_state(), 1).unwrap();
    assert_eq!((d.ai_counts(1).land, d.ai_counts(1).sea), (7 + 4, 2 + 1));
    // A human player has none, and the surplus survives in the fingerprint only once there is one.
    assert_eq!(d.army_surplus[0], [0; 3]);
    assert_ne!(a.fingerprint(), b.fingerprint());
}
