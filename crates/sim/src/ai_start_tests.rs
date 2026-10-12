use super::*;
use crate::ai::{BUILD_LIMIT, controller};
use crate::tech::{RANGER_TRAINING_LINE, building as b};
use crate::world::tests::field;
use std::sync::Arc;
use tidewater_formats::aibin::AiFile;
use tidewater_formats::udta::{UnitStats, flags};

const P: usize = 1;

/// A computer player 1 whose build list is `list`, with every building type a 2 x 2 building.
fn world(list: &[u8]) -> World {
    let mut w = field(40, 40);
    w.players[P].controller = controller::COMPUTER;
    for k in 0x3a..0x6e {
        w.stats.0[k] = UnitStats {
            hit_points: 400,
            size: (2, 2),
            flags: flags::BUILDING,
            ..Default::default()
        };
    }
    let mut data = vec![0u8; 256];
    data.extend_from_slice(list);
    data.resize(512, 0xff);
    w.ai_file = Some(Arc::new(AiFile { data }));
    w.ai[P].build_list = 256;
    w.ai[P].slots[BUILD_LIMIT] = 0xff;
    w
}

fn put(w: &mut World, k: u8, n: i32) {
    w.spawn(k, P as u8, ((n % 8) * 4, (n / 8) * 4), 0).unwrap();
}

fn done(list: &[u8], setup: impl Fn(&mut World)) -> u64 {
    let mut w = world(list);
    setup(&mut w);
    w.mark_game_start(P);
    w.ai[P].done
}

#[test]
fn upgrade_codes_follow_the_level_asked_for() {
    // 0x80 asks for level 1 and 0x81 for level 2 of the missiles line.
    let line = fixed_line(4);
    let list = [0x80, 0x81, 0x86];
    assert_eq!(done(&list, |_| {}), 0);
    assert_eq!(done(&list, |w| w.players[P].levels[line] = 1), 0b001);
    assert_eq!(done(&list, |w| w.players[P].levels[line] = 2), 0b011);
    let weapons = fixed_line(0);
    assert_eq!(done(&[0x86, 0x87], |w| w.players[P].levels[weapons] = 1), 0b01);
    assert_eq!(done(&[0x86, 0x87], |w| w.players[P].levels[weapons] = 2), 0b11);
    let orc = fixed_line(20);
    assert_eq!(
        done(&[0x8a, 0x8b], |w| {
            w.players[P].race = 1;
            w.players[P].levels[orc] = 1;
        }),
        0b01
    );
}

#[test]
fn ranger_codes_need_the_item_researched() {
    let list = [0x82, 0x83, 0x84, 0x85];
    assert_eq!(done(&list, |_| {}), 0);
    assert_eq!(done(&list, |w| w.players[P].levels[RANGER_TRAINING_LINE] = 1), 0b0001);
    assert_eq!(done(&list, |w| w.players[P].levels[10] = 1), 0b0010);
    assert_eq!(done(&list, |w| w.players[P].levels[8] = 1), 0b0100);
    assert_eq!(done(&list, |w| w.players[P].levels[9] = 1), 0b1000);
}

#[test]
fn spell_codes_follow_the_bit_table() {
    let list: Vec<u8> = (0x90..=0x97).collect();
    for (i, &(h, o)) in SPELL_BITS.iter().enumerate() {
        for bit in [h, o] {
            assert_eq!(
                done(&list, |w| w.players[P].spells_known = 1 << bit),
                1 << i,
                "code {i} bit {bit}"
            );
        }
    }
    // Either bit counts for either race: the orc bit marks a human player's entry too.
    assert_eq!(done(&[0x91], |w| w.players[P].spells_known = 1 << 11), 1);
}

#[test]
fn the_setup_default_spells_mark_nothing() {
    let list: Vec<u8> = (0x90..=0x97).collect();
    assert_eq!(done(&list, |w| w.players[P].spells_known = 0x4020), 0);
}

#[test]
fn halls_and_towers_mark_only_entries_of_their_own_type() {
    // Round-36 vectors: a castle or a keep marks no 0x98, 0x99 or town hall entry; a tower never the scout tower.
    assert_eq!(done(&[0x4a, 0x98, 0x99, 0x4a], |w| put(w, b::CASTLE, 0)), 0);
    assert_eq!(done(&[0x4a, 0x98], |w| put(w, b::KEEP, 0)), 0);
    assert_eq!(done(&[0x4a, b::KEEP], |w| put(w, b::KEEP, 0)), 0b10);
    assert_eq!(done(&[0x40, 0x60], |w| put(w, b::HUMAN_GUARD_TOWER, 0)), 0b10);
    assert_eq!(done(&[0x40, 0x63], |w| put(w, b::ORC_CANNON_TOWER, 0)), 0b10);
}

#[test]
fn the_xorc12_build_list_vector() {
    // xorc12-ogre-mage, player 3: the map's buildings mark entries 0, 1, 2, 3 and 5; the scout tower (4) stays open.
    let list = [
        b::TOWN_HALL,
        b::HUMAN_BARRACKS,
        b::ELVEN_LUMBER_MILL,
        b::HUMAN_BLACKSMITH,
        b::HUMAN_SCOUT_TOWER,
        b::HUMAN_BARRACKS,
        0x98,
    ];
    let mut kinds = vec![
        b::TOWN_HALL,
        b::HUMAN_BARRACKS,
        b::HUMAN_BARRACKS,
        b::ELVEN_LUMBER_MILL,
        b::HUMAN_BLACKSMITH,
        b::MAGE_TOWER,
        b::MAGE_TOWER,
    ];
    kinds.extend([b::HUMAN_GUARD_TOWER; 3]);
    kinds.extend([b::HUMAN_CANNON_TOWER; 3]);
    kinds.extend([0x3a; 11]);
    let got = done(&list, |w| {
        for (n, &k) in kinds.iter().enumerate() {
            put(w, k, n as i32);
        }
    });
    assert_eq!(got, 0b10_1111);
}

#[test]
fn a_construction_site_marks_its_entry_when_construction_starts() {
    let mut w = world(&[b::HUMAN_BARRACKS, b::HUMAN_BARRACKS]);
    w.place_building(b::HUMAN_BARRACKS, P as u8, (4, 4)).unwrap();
    assert_eq!(w.ai[P].done, 0b01);
}

#[test]
fn other_buildings_mark_the_first_entry_of_their_type() {
    // A human barracks matches an entry written as the orc one: the race change.
    let list = [0x3d, 0x3c, 0x42];
    assert_eq!(done(&list, |w| put(w, b::HUMAN_BARRACKS, 0)), 0b001);
    assert_eq!(
        done(&list, |w| (0..2).for_each(|n| put(w, b::HUMAN_BARRACKS, n))),
        0b011
    );
    assert_eq!(done(&list, |w| put(w, b::CHURCH, 0)), 0);
}

#[test]
fn research_entries_are_not_marked_by_buildings() {
    assert_eq!(done(&[0x90, 0x80], |w| put(w, b::TEMPLE, 0)), 0);
}

#[test]
fn players_that_are_not_computers_are_left_alone() {
    let mut w = world(&[0x4a]);
    w.players[P].controller = controller::HUMAN;
    put(&mut w, b::KEEP, 0);
    w.mark_game_start(P);
    assert_eq!(w.ai[P].done, 0);
}

#[test]
fn start_ai_makes_the_marks_before_the_first_scan() {
    let mut w = world(&[0x4a, 0x98]);
    put(&mut w, b::TOWN_HALL, 0);
    let file = w.ai_file.clone().unwrap();
    // A script file whose record 0 points at the same lists.
    let mut d = file.data.clone();
    d[0..2].copy_from_slice(&600u16.to_le_bytes());
    d.resize(600, 0);
    d.extend([0x00, 0x01, 0x00, 0x02, 0, 0, 0, 0, 0, 0, 0, 0]);
    w.players[P].ai_script = 0;
    w.start_ai(Arc::new(AiFile::parse(d).unwrap()));
    assert_eq!(w.ai[P].done, 0b01);
}

#[test]
fn a_never_researchable_entry_stalls_the_requests_after_it() {
    // Mission 13, player 4: build list 0x93, 0x90, 0x91, bit 13 of the spells word set (the orc bit of 0x93),
    // limit 7, no workers. Once 0x93 is done every scan stops at 0x90.
    let mut w = world(&[0x93, 0x90, 0x91]);
    w.ai[P].slots[BUILD_LIMIT] = 7;
    w.players[P].spells_known = 1 << 13;
    w.mark_game_start(P);
    assert_eq!(w.ai[P].done, 0b001);
    for _ in 0..3 {
        w.no_workers_research_pass();
    }
    assert!(w.ai[P].requested(0x90));
    assert!(!w.ai[P].requested(0x91));
    assert!(!w.ai[P].requested(0x93));
}
