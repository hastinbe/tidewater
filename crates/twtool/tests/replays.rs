//! `twtool replay` and `twtool desync-diff` on a replay recorded here (set `TIDEWATER_INSTALL` to the game's
//! installation; without it the tests do nothing). The recording is a short two-human game on the installation's
//! smallest two-slot map, made the way the client makes one; the map is passed with `--map` from a temporary file.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tidewater_formats::install::Install;
use tidewater_formats::pud::Pud;
use tidewater_replay::format::CHECKPOINT_EVERY;
use tidewater_replay::start::{build_world, choose_start, map_hash};
use tidewater_replay::{Header, Replay};
use tidewater_sim::Command as Cmd;
use tidewater_sim::economy::kind;
use tidewater_sim::setup::{Lobby, Setup};

const CYCLES: u32 = 400;

/// The installation's smallest map with two player slots: its bytes and the two slots.
fn smallest(root: &Path) -> (Vec<u8>, [u8; 2]) {
    let mut best: Option<(usize, Vec<u8>, [u8; 2])> = None;
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("Maps"))
        .expect("the Maps folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pud")))
        .collect();
    files.sort();
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let Ok(pud) = Pud::parse(&bytes) else { continue };
        let slots: Vec<u8> = pud
            .owners()
            .iter()
            .enumerate()
            .take(8)
            .filter(|&(_, &c)| c == 4 || c == 5)
            .map(|(i, _)| i as u8)
            .collect();
        let size = pud.width as usize * pud.height as usize;
        if slots.len() >= 2 && best.as_ref().is_none_or(|b| size < b.0) {
            best = Some((size, bytes, [slots[0], slots[1]]));
        }
    }
    let (_, map, slots) = best.expect("a two-slot map");
    (map, slots)
}

/// Records a game in which both players send two workers off at cycle 10 (only the first, if `late`).
fn record(root: &Path, map: &[u8], slots: [u8; 2], late: bool) -> Replay {
    let mut install = Install::open(root).unwrap();
    let pud = Pud::parse(map).unwrap();
    let lobby = Lobby {
        humans: vec![(slots[0], 0), (slots[1], 1)],
        local: slots[0],
        computers: vec![],
        fixed_order: true,
    };
    let setup = Setup::default();
    let start = choose_start(&pud, &setup, 7, Some(&lobby)).unwrap();
    let (mut world, _) = build_world(&mut install, &pud, &start).unwrap();
    world.start_up();
    let header = Header::for_game(
        "test.pud".into(),
        map_hash(map),
        7,
        setup,
        Some(lobby),
        &world,
        0,
        start.local,
    );
    let mut replay = Replay::new(header);
    for _ in 0..CYCLES {
        let tick = world.tick;
        let mut cmds = Vec::new();
        if tick == 10 {
            for &p in &slots {
                if late && p != slots[0] {
                    continue;
                }
                let workers: Vec<_> = world.units.iter().filter(|u| u.owner == p && !u.building).collect();
                if let Some(hall) = world.units.iter().find(|u| u.owner == p && kind::is_hall(u.kind)) {
                    cmds.push(Cmd::Move {
                        player: p,
                        units: workers.iter().map(|u| u.id).take(2).collect(),
                        to: (hall.tile.0 + 4, hall.tile.1 + 4),
                    });
                }
            }
        }
        replay.record(tick, &cmds);
        world.step(&cmds);
        if world.tick % CHECKPOINT_EVERY == 0 {
            replay.checkpoint(world.tick, world.state_hash());
        }
    }
    replay
}

fn twtool(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_twtool"))
        .args(args)
        .output()
        .expect("twtool runs")
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn replay_and_desync_diff_check_a_recorded_game() {
    let Some(root) = std::env::var_os("TIDEWATER_INSTALL").map(PathBuf::from) else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let (map, slots) = smallest(&root);
    let dir = tempfile::tempdir().unwrap();
    let map_file = dir.path().join("test.pud");
    std::fs::write(&map_file, &map).unwrap();
    let good = record(&root, &map, slots, false);
    let other = record(&root, &map, slots, true);
    let (good_file, other_file) = (dir.path().join("good.twrep"), dir.path().join("other.twrep"));
    std::fs::write(&good_file, good.encode()).unwrap();
    std::fs::write(&other_file, other.encode()).unwrap();
    let os = |p: &Path| p.as_os_str().to_owned();
    let (install, map_arg) = (os(&root), os(&map_file));
    let run = |cmd: &str, files: &[&Path], extra: &[&str]| {
        let mut args: Vec<std::ffi::OsString> = vec![cmd.into(), "--install".into(), install.clone()];
        args.extend(files.iter().map(|f| os(f)));
        args.extend(["--map".into(), map_arg.clone()]);
        args.extend(extra.iter().map(|e| (*e).into()));
        let refs: Vec<&std::ffi::OsStr> = args.iter().map(|a| a.as_os_str()).collect();
        twtool(&refs)
    };

    // A good replay verifies.
    let o = run("replay", &[&good_file], &[]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("all 4 checkpoints match"), "{}", text(&o));

    // A damaged one is refused, with a message and a failing exit.
    let mut bytes = good.encode();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    let bad = dir.path().join("bad.twrep");
    std::fs::write(&bad, bytes).unwrap();
    let o = run("replay", &[&bad], &[]);
    assert!(!o.status.success());
    assert!(!text(&o).contains("panicked"), "{}", text(&o));

    // A replay with a checkpoint that is wrong stops at that cycle.
    let mut wrong = good.clone();
    wrong.checkpoints[1].hash ^= 1;
    let wrong_file = dir.path().join("wrong.twrep");
    std::fs::write(&wrong_file, wrong.encode()).unwrap();
    let o = run("replay", &[&wrong_file], &[]);
    assert!(!o.status.success());
    assert!(text(&o).contains("cycle 200"), "{}", text(&o));

    // The same replay from the other player's seat has no difference.
    let slot = slots[1].to_string();
    let o = run("desync-diff", &[&good_file], &["--local", &slot]);
    assert!(o.status.success(), "{}", text(&o));
    assert!(text(&o).contains("no difference"), "{}", text(&o));

    // Two different games differ, and the tool names the cycle and a field.
    let o = run("desync-diff", &[&good_file, &other_file], &[]);
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    let t = text(&o);
    assert!(t.contains("first difference at cycle"), "{t}");
    assert!(t.contains("first differing field"), "{t}");
}
