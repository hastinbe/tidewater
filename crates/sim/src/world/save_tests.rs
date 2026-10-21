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
/// unit's creation number to its slot plus one, and the next unit gets one more than the largest in use.
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
    let top = b.units.iter().map(|u| u.serial.0).max().unwrap();
    let n = b.spawn(0, 0, (6, 6), 0).unwrap();
    assert_eq!(b.unit(n).unwrap().serial.0, top + 1);
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
