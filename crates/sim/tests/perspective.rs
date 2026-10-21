//! Lockstep safety: the same game must hash the same every cycle whichever player is local.
//!
//! Two machines in a network game differ in one input only, the local player (`Start::local`). Everything the local
//! player sees (fog display, minimap, sounds, flashes) may differ, but the game state ([`World::state_hash`]) must
//! not. This test starts a two-human multiplayer game on one of the installation's skirmish maps twice, once with
//! each human local, feeds both runs the same scripted commands for both players (gather, train, build, move,
//! attack-move, attack, stop) and compares the hash after every cycle.
//!
//! Set `TIDEWATER_INSTALL` to the game's installation; without it the test does nothing. The script avoids the reads
//! of the local player that `docs/design/multiplayer-determinism.md` lists as open questions: no player is knocked
//! out (the victory check), and every building site is explored (the "Unexplored" build check).

use std::path::PathBuf;
use std::sync::Arc;

use tidewater_formats::install::Install;
use tidewater_formats::pud::Pud;
use tidewater_formats::udta::UnitTable;
use tidewater_sim::economy::kind;
use tidewater_sim::setup::{Lobby, Setup, Start};
use tidewater_sim::site::SiteCheck;
use tidewater_sim::{Command, UnitId, World};

const CYCLES: u32 = 4000;

/// The skirmish map with the fewest tiles that has at least two slots a human can take.
fn pick_map(root: &std::path::Path) -> (Vec<u8>, [u8; 2]) {
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
    let (_, bytes, slots) = best.expect("a map with two player slots");
    (bytes, slots)
}

fn start_world(inst: &mut Install, pud: &Pud, local: u8, slots: [u8; 2]) -> World {
    let lobby = Lobby {
        humans: vec![(slots[0], 0), (slots[1], 1)],
        local,
        computers: vec![],
        fixed_order: true,
    };
    let start = Start::multiplayer(pud, &Setup::default(), 12345, &lobby).expect("a multiplayer start");
    let ts = inst.tileset(start.era).unwrap();
    let stats = match pud
        .section(b"UDTA")
        .map(UnitTable::from_udta)
        .transpose()
        .unwrap()
        .flatten()
    {
        Some(t) => t,
        None => UnitTable::from_table(&inst.read("rez\\unitdata.dat").unwrap()).unwrap(),
    };
    let scripts = tidewater_formats::script::Scripts::parse(inst.read("rez\\script.bin").unwrap()).unwrap();
    let upgrades = tidewater_formats::ugrd::UpgradeTable::from_table(&inst.read("rez\\upgrades.dat").unwrap()).unwrap();
    let mut w = World::from_pud(pud, &ts, stats, upgrades, Arc::new(scripts), &start).unwrap();
    let ai = tidewater_formats::aibin::AiFile::parse(inst.read("rez\\ai.bin").unwrap()).unwrap();
    w.start_ai(Arc::new(ai));
    w
}

/// The commands for cycle `t`, worked out from `w` (which both runs share, as their hashes are equal).
fn script(w: &World, t: u32, players: [u8; 2]) -> Vec<Command> {
    let mut out = Vec::new();
    for (n, &p) in players.iter().enumerate() {
        let foe = players[1 - n];
        let mine: Vec<&tidewater_sim::Unit> = w.units.iter().filter(|u| u.owner == p && !u.dying).collect();
        let workers: Vec<UnitId> = mine.iter().filter(|u| !u.building).map(|u| u.id).collect();
        let Some(hall) = mine.iter().find(|u| kind::is_hall(u.kind)) else {
            continue;
        };
        let foe_hall = w
            .units
            .iter()
            .find(|u| u.owner == foe && kind::is_hall(u.kind))
            .map(|u| (u.id, u.tile));
        match t {
            // Gather: each worker takes the nearest mine or tree it can.
            1 => {
                let dist = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
                let (mw, mh) = (w.terrain.width as i32, w.terrain.height as i32);
                let sites: Vec<(i32, i32)> = w
                    .units
                    .iter()
                    .filter(|u| u.building)
                    .map(|u| u.tile)
                    .chain((0..mh).flat_map(|y| (0..mw).map(move |x| (x, y))).filter(|&(x, y)| {
                        w.terrain
                            .region(x, y)
                            .is_some_and(tidewater_sim::terrain::region::is_forest)
                    }))
                    .collect();
                for &id in &workers {
                    let me = w.unit(id).unwrap();
                    if let Some(tile) = sites
                        .iter()
                        .copied()
                        .filter(|&s| w.can_harvest(me, s))
                        .min_by_key(|&s| dist(me.tile, s))
                    {
                        out.push(Command::Harvest {
                            player: p,
                            units: vec![id],
                            target: None,
                            tile,
                        });
                    }
                }
            }
            // Train a worker now and then.
            2 | 400 | 900 | 1500 => out.push(Command::Train {
                player: p,
                building: hall.id,
                kind: if hall.kind == kind::TOWN_HALL { 0x02 } else { 0x03 },
            }),
            // A building near the hall (explored ground), by the first worker.
            300 | 1200 => {
                let farm = if hall.kind == kind::TOWN_HALL {
                    kind::FARM
                } else {
                    kind::PIG_FARM
                };
                if let Some(&b) = workers.first() {
                    let at = (-6..=6)
                        .flat_map(|dy| (-6..=6).map(move |dx| (hall.tile.0 + dx, hall.tile.1 + dy)))
                        .find(|&s| w.check_site(farm, s, b) == SiteCheck::Allowed);
                    if let Some(tile) = at {
                        out.push(Command::Build {
                            player: p,
                            unit: b,
                            kind: farm,
                            tile,
                        });
                    }
                }
            }
            // A few workers wander off, one stops.
            700 => {
                let to = (hall.tile.0 + 3, hall.tile.1 + 3);
                out.push(Command::Move {
                    player: p,
                    units: workers.iter().copied().take(2).collect(),
                    to,
                });
            }
            760 => out.push(Command::Stop {
                player: p,
                units: workers.iter().copied().take(1).collect(),
            }),
            // Send some workers across the map: attack-move, then an attack on the other hall.
            1800 => {
                if let Some((_, tile)) = foe_hall {
                    out.push(Command::AttackMove {
                        player: p,
                        units: workers.iter().copied().take(3).collect(),
                        to: tile,
                    });
                }
            }
            2600 => {
                if let Some((id, _)) = foe_hall {
                    out.push(Command::Attack {
                        player: p,
                        units: workers.iter().copied().take(3).collect(),
                        target: id,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

#[test]
fn the_hash_does_not_depend_on_the_local_player() {
    let Some(root) = std::env::var_os("TIDEWATER_INSTALL") else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let root = PathBuf::from(root);
    let (bytes, slots) = pick_map(&root);
    let pud = Pud::parse(&bytes).unwrap();
    let mut inst = Install::open(&root).unwrap();
    let mut a = start_world(&mut inst, &pud, slots[0], slots);
    let mut b = start_world(&mut inst, &pud, slots[1], slots);
    assert_eq!(a.vision.local, slots[0]);
    assert_eq!(b.vision.local, slots[1]);
    assert_eq!(a.state_hash(), b.state_hash(), "at the start");
    let mut commands = 0;
    for t in 1..=CYCLES {
        let cmds = script(&a, t, slots);
        commands += cmds.len();
        a.step(&cmds);
        b.step(&cmds);
        assert_eq!(a.state_hash(), b.state_hash(), "cycle {t}");
    }
    assert!(commands > 10, "the script ran {commands} commands");
    for p in slots {
        assert!(a.players[p as usize].result.is_none(), "player {p} was knocked out");
    }
}
