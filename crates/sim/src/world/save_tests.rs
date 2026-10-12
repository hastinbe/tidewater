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
    let a = battle();
    assert!(a.units.iter().any(|u| u.commanded), "a command is pending at the save");
    let mut b = field(16, 16);
    b.restore(&a.save_state(), 1).unwrap();
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
fn a_classic_save_drops_the_cycle_count_and_the_score() {
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
    assert_eq!(b.units, a.units);
    // Enhanced keeps both.
    let mut c = battle();
    run(&mut c, 30);
    c.players[0].score = 500;
    let mut d = field(16, 16);
    d.restore(&c.save_state(), 1).unwrap();
    assert_eq!((d.tick, d.players[0].score), (c.tick, 500));
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
