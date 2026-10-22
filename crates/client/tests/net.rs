//! Network play on a real map: two machines in one process, joined over loopback TCP, play a scripted game, check the
//! state hash at every turn and record it (set `TIDEWATER_INSTALL` to the game's installation; without it the tests do
//! nothing, like the other tests that need game files).
//!
//! The machines are driven by one thread, one cycle at a time each, the way the game screen drives its session
//! (`game::tick`): submit, `ready_commands`, step, `wants_hash` and `checkpoint`. The handshake is the program's own
//! (`net::lobby::host` and `join`), so the lobby, the identity check and the map check are exercised too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use tidewater_client::net::lobby::{self, LobbySpec, NetStart};
use tidewater_client::net::lockstep::{NetSession, Params};
use tidewater_client::net::proto::Msg;
use tidewater_client::net::tcp::{self, Inbound, Wire};
use tidewater_client::session::{LeaveNotice, Recorder, Session};
use tidewater_client::storage::{self, Desktop, Store};
use tidewater_formats::install::Install;
use tidewater_formats::pud::Pud;
use tidewater_replay::run::{first_desync, verify};
use tidewater_replay::start::{build_world, choose_start, map_hash, new_world, recorded_map_name};
use tidewater_replay::{Header, Replay};
use tidewater_sim::economy::kind;
use tidewater_sim::setup::Setup;
use tidewater_sim::site::SiteCheck;
use tidewater_sim::{Command, UnitId, World};

const SEED: u32 = 12345;

struct Rig {
    root: PathBuf,
    /// The map as the installation names it, and its file.
    name: String,
    bytes: Vec<u8>,
}

/// The installation's smallest map with two start slots (the replay round trip's choice).
fn rig() -> Option<Rig> {
    let root = PathBuf::from(std::env::var_os("TIDEWATER_INSTALL")?);
    let mut best: Option<(usize, String, Vec<u8>)> = None;
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("Maps"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pud")))
        .collect();
    files.sort();
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let Ok(pud) = Pud::parse(&bytes) else { continue };
        let slots: Vec<usize> = pud
            .owners()
            .iter()
            .enumerate()
            .take(8)
            .filter(|&(_, &c)| c == 4 || c == 5)
            .map(|(i, _)| i)
            .collect();
        let size = pud.width as usize * pud.height as usize;
        // The lobby seats its humans in rows 0 and 1.
        if slots.starts_with(&[0, 1]) && best.as_ref().is_none_or(|b| size < b.0) {
            let name = format!("Maps\\{}", f.file_name().unwrap().to_string_lossy());
            best = Some((size, name, bytes));
        }
    }
    let (_, name, bytes) = best?;
    Some(Rig { root, name, bytes })
}

fn store(rig: &Rig, dir: &Path, who: &str) -> Store {
    let me = dir.join(who);
    Store(Arc::new(Desktop::new(
        &rig.root,
        me.join("Save"),
        me.join("settings.txt"),
        me,
    )))
}

/// Host and joiner through the program's own handshake. The host's thread says the port it listens on.
fn handshake(rig: &Rig, dir: &Path, lobby_spec: &str, setup: Setup) -> (NetStart, NetStart) {
    let spec = LobbySpec::parse(lobby_spec).unwrap();
    let (tx, rx) = mpsc::channel();
    let host = thread::spawn({
        let (host_store, name) = (store(rig, dir, "host"), rig.name.clone());
        move || {
            let mut install = storage::open_install(&host_store.0).unwrap();
            lobby::host(Some(0), &spec, &name, setup, &mut install, &host_store, "Ann", 1, |l| {
                let _ = tx.send(l.to_string());
            })
            .unwrap()
        }
    });
    let port = loop {
        let line = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the host says its port");
        if let Some(rest) = line.strip_prefix("hosting on port ") {
            break rest.split(':').next().unwrap().parse::<u16>().unwrap();
        }
    };
    let join_store = store(rig, dir, "joiner");
    let mut install = storage::open_install(&join_store.0).unwrap();
    let joiner = lobby::join(&format!("127.0.0.1:{port}"), &mut install, &join_store, "Bob", |_| {}).unwrap();
    (host.join().unwrap(), joiner)
}

/// One machine: the world, its session, and the scripted player.
struct Machine {
    me: u8,
    world: World,
    session: NetSession,
    script_for: Option<u32>,
    /// Hash at every tick the session asked for one.
    hashes: BTreeMap<u32, u64>,
    /// Non-empty command lists by cycle.
    applied: Vec<(u32, Vec<Command>)>,
    notices: Vec<LeaveNotice>,
    /// Do not give cycles (a machine that is late).
    paused: bool,
}

fn machine(rig: &Rig, dir: &Path, who: &str, start: NetStart, record: bool) -> Machine {
    let store = store(rig, dir, who);
    let mut install = storage::open_install(&store.0).unwrap();
    let pud = Pud::parse(&rig.bytes).unwrap();
    let cfg = &start.config;
    assert_eq!(map_hash(&rig.bytes), cfg.map_hash);
    let mut lobby = cfg.lobby.clone();
    lobby.local = start.me;
    let begin = choose_start(&pud, &cfg.setup, cfg.seed, Some(&lobby)).unwrap();
    let (mut world, _) = build_world(&mut install, &pud, &begin).unwrap();
    world.rules.extras = cfg.extras;
    world.start_up();
    let recorder = record.then(|| {
        let header = Header::for_game(
            recorded_map_name(&rig.name, false),
            cfg.map_hash,
            cfg.seed,
            cfg.setup,
            Some(lobby.clone()),
            &world,
            0,
            begin.local,
        );
        Recorder::new(header, store.clone(), "game.twrep".to_string())
    });
    let params = Params {
        me: start.me,
        host: cfg.lobby.humans[0].0,
        humans: cfg.lobby.humans.iter().map(|h| h.0).collect(),
        names: start.names.clone(),
        enhanced: world.rules.enhanced,
        row: cfg.row,
        speed: cfg.speed,
    };
    Machine {
        me: start.me,
        world,
        session: NetSession::new(params, start.wire, recorder),
        script_for: None,
        hashes: BTreeMap::new(),
        applied: Vec::new(),
        notices: Vec::new(),
        paused: false,
    }
}

impl Machine {
    /// One cycle, as `game::tick` runs it. `false`: the session is waiting.
    fn advance(&mut self) -> bool {
        if self.paused {
            return false;
        }
        let t = self.world.tick;
        if self.script_for != Some(t) {
            self.script_for = Some(t);
            for c in script(&self.world, t, self.me) {
                self.session.submit(c);
            }
        }
        self.session.observe_speed(self.world.speed);
        let ready = self.session.ready_commands(t);
        self.notices.extend(self.session.take_leave_notices());
        let Some(cmds) = ready else { return false };
        self.world.step(&cmds);
        if !cmds.is_empty() {
            self.applied.push((t, cmds));
        }
        if self.world.victory.over || self.session.wants_hash(self.world.tick) {
            let (tick, hash) = (self.world.tick, self.world.state_hash());
            self.hashes.insert(tick, hash);
            self.session.checkpoint(tick, hash).unwrap();
        }
        true
    }
}

/// Runs the machines until each reaches cycle `until`.
fn run(ms: &mut [Machine], until: u32) {
    let mut idle = 0;
    while ms.iter().any(|m| m.world.tick < until && !m.paused) {
        let mut moved = false;
        for m in ms.iter_mut().filter(|m| m.world.tick < until) {
            moved |= m.advance();
        }
        if moved {
            idle = 0;
        } else {
            idle += 1;
            assert!(
                idle < 5000,
                "the game stopped at {:?}",
                ms.iter().map(|m| m.world.tick).collect::<Vec<_>>()
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}

/// The commands player `me` gives at cycle `t`, worked out from its own world: gather, train, build, move, stop.
fn script(w: &World, t: u32, me: u8) -> Vec<Command> {
    let mut out = Vec::new();
    let mine: Vec<&tidewater_sim::Unit> = w.units.iter().filter(|u| u.owner == me && !u.dying).collect();
    let workers: Vec<UnitId> = mine.iter().filter(|u| !u.building).map(|u| u.id).collect();
    let Some(hall) = mine.iter().find(|u| kind::is_hall(u.kind)) else {
        return out;
    };
    match t {
        1 => {
            let dist = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
            let sites: Vec<(i32, i32)> = w.units.iter().filter(|u| u.building).map(|u| u.tile).collect();
            for &id in &workers {
                let unit = w.unit(id).unwrap();
                if let Some(tile) = sites
                    .iter()
                    .copied()
                    .filter(|&s| w.can_harvest(unit, s))
                    .min_by_key(|&s| dist(unit.tile, s))
                {
                    out.push(Command::Harvest {
                        player: me,
                        units: vec![id],
                        target: None,
                        tile,
                    });
                }
            }
        }
        // The two sides give their commands at different cycles, so the turns carry one and two at once.
        2 | 41 | 400 => out.push(Command::Train {
            player: me,
            building: hall.id,
            kind: if hall.kind == kind::TOWN_HALL { 0x02 } else { 0x03 },
        }),
        300 | 457 => {
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
                        player: me,
                        unit: b,
                        kind: farm,
                        tile,
                    });
                }
            }
        }
        200 | 501 => out.push(Command::Move {
            player: me,
            units: workers.iter().copied().take(2).collect(),
            to: (hall.tile.0 + 3, hall.tile.1 + 3),
        }),
        260 => out.push(Command::Stop {
            player: me,
            units: workers.iter().copied().take(1).collect(),
        }),
        _ => {}
    }
    out
}

fn replay_of(dir: &Path, who: &str) -> Replay {
    let bytes = std::fs::read(dir.join(who).join("replays").join("game.twrep")).unwrap();
    Replay::decode(&bytes).unwrap()
}

#[test]
fn two_machines_play_in_step_and_record_the_same_game() {
    let Some(rig) = rig() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (host, joiner) = handshake(
        &rig,
        dir.path(),
        "humans=2,races=human/orc,fixed,seed=12345,speed=8,row=4",
        Setup::default(),
    );
    assert_eq!((host.me, joiner.me), (0, 1));
    assert_eq!(joiner.config.seed, SEED, "the host's seed reached the joiner");
    assert_eq!(joiner.names, ["Ann", "Bob"]);
    let mut ms = [
        machine(&rig, dir.path(), "host", host, true),
        machine(&rig, dir.path(), "joiner", joiner, true),
    ];
    // The two worlds start apart only in whose seat the screen shows (the local player).
    assert_eq!(
        ms[0].world.state_hash(),
        ms[1].world.state_hash(),
        "the same game on both machines at the start"
    );
    const CYCLES: u32 = 700;
    run(&mut ms, CYCLES);
    // Hashes checked at every turn (K = 12 at speed 8) and every hundredth cycle, and they agree.
    let turns: Vec<u32> = ms[0].hashes.keys().copied().filter(|t| t % 12 == 0).collect();
    assert!(turns.len() > (CYCLES / 12 - 3) as usize, "{} turn hashes", turns.len());
    assert_eq!(
        ms[0].hashes, ms[1].hashes,
        "the hash at every turn and every checkpoint is the same on both machines"
    );
    assert_eq!(ms[0].applied, ms[1].applied, "the same commands in the same cycles");
    // Both sides' commands ran, each from its own seat, and nobody left.
    let players: Vec<u8> = ms[0]
        .applied
        .iter()
        .flat_map(|(_, c)| c.iter().map(Command::player))
        .collect();
    assert!(players.contains(&0) && players.contains(&1), "{players:?}");
    assert!(ms.iter().all(|m| m.notices.is_empty()));
    assert!(ms[0].world.speed == 8 && ms[1].world.speed == 8, "the lobby's speed");
    let [a, b] = ms;
    let (hash_a, hash_b) = (a.world.state_hash(), b.world.state_hash());
    assert_eq!(hash_a, hash_b);
    drop((a, b));

    // The recordings: identical but for whose seat each is (`twtool desync-diff` compares them the same way).
    let (ra, rb) = (replay_of(dir.path(), "host"), replay_of(dir.path(), "joiner"));
    assert_eq!(ra.cycles, CYCLES);
    assert_eq!(ra.commands, rb.commands, "the merged commands are the same");
    assert_eq!(ra.checkpoints, rb.checkpoints);
    assert_eq!((ra.header.local, rb.header.local), (0, 1));
    assert_eq!(
        ra.header.lobby.as_ref().map(|l| l.humans.clone()),
        rb.header.lobby.as_ref().map(|l| l.humans.clone())
    );
    let mut install = Install::open(&rig.root).unwrap();
    let mut wa = new_world(&mut install, &rig.bytes, &ra.header).unwrap();
    let mut wb = new_world(&mut install, &rig.bytes, &rb.header).unwrap();
    assert_eq!(
        first_desync(&mut wa, &ra, &mut wb, &rb),
        None,
        "desync-diff finds nothing"
    );
    for r in [&ra, &rb] {
        let mut w = new_world(&mut install, &rig.bytes, &r.header).unwrap();
        let report = verify(&mut w, r).expect("a network game replays on one machine");
        assert_eq!(report.final_hash, hash_a);
    }
}

#[test]
fn classic_rules_are_opt_in_and_play_in_step() {
    let Some(rig) = rig() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let setup = Setup::parse("custom:classic").unwrap();
    assert!(!setup.enhanced);
    let (host, joiner) = handshake(&rig, dir.path(), "humans=2,fixed,seed=7,speed=8,row=4", setup);
    let mut ms = [
        machine(&rig, dir.path(), "host", host, false),
        machine(&rig, dir.path(), "joiner", joiner, false),
    ];
    assert!(!ms[0].world.rules.enhanced && !ms[1].world.rules.enhanced);
    run(&mut ms, 300);
    assert_eq!(ms[0].hashes, ms[1].hashes);
    assert!(ms[0].notices.is_empty() && ms[1].notices.is_empty());
}

#[test]
fn a_forced_divergence_drops_both_machines_on_the_specified_turn() {
    let Some(rig) = rig() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (host, joiner) = handshake(
        &rig,
        dir.path(),
        "humans=2,fixed,seed=12345,speed=8,row=4",
        Setup::default(),
    );
    let mut ms = [
        machine(&rig, dir.path(), "host", host, false),
        machine(&rig, dir.path(), "joiner", joiner, false),
    ];
    // Cycles 0 to 300 have run when the joiner's gold changes by one: its state differs from the host's from the turn
    // before cycle 312 on, which is the first digest to show it; the machines compare it at the next turn, before 324.
    run(&mut ms, 301);
    ms[1].world.players[1].gold += 1;
    run(&mut ms, 400);
    let left = |who: u8| {
        vec![Command::Leave {
            player: who,
            dropped: true,
        }]
    };
    let drops = |m: &Machine| -> Vec<(u32, Vec<Command>)> {
        m.applied
            .iter()
            .filter(|(_, c)| c.iter().any(|c| matches!(c, Command::Leave { .. })))
            .cloned()
            .collect()
    };
    assert_eq!(
        drops(&ms[0]),
        [(324, left(1))],
        "the host dropped the joiner at the turn before cycle 324"
    );
    assert_eq!(
        drops(&ms[1]),
        [(324, left(0))],
        "the joiner dropped the host at the turn before cycle 324"
    );
    for (m, other) in [(&ms[0], 1u8), (&ms[1], 0u8)] {
        assert_eq!(m.notices.len(), 1);
        assert!(m.notices[0].dropped && m.notices[0].slot == other, "{:?}", m.notices);
    }
    assert!(ms.iter().all(|m| m.world.tick == 400), "both go on, alone");
}

#[test]
fn a_late_machine_stalls_the_other_without_dropping_anyone() {
    let Some(rig) = rig() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (host, joiner) = handshake(
        &rig,
        dir.path(),
        "humans=2,fixed,seed=12345,speed=8,row=4",
        Setup::default(),
    );
    let mut ms = [
        machine(&rig, dir.path(), "host", host, false),
        machine(&rig, dir.path(), "joiner", joiner, false),
    ];
    run(&mut ms, 100);
    // The joiner goes quiet for a quarter of a second: long enough to reset the cycle deadline, far short of the
    // watch cursor (2 s).
    ms[1].paused = true;
    while ms[0].advance() {}
    let stuck = ms[0].world.tick;
    assert_eq!(stuck % 12, 0, "a wait is at a turn");
    thread::sleep(Duration::from_millis(250));
    let waiting = ms[0].session.waiting().expect("the host is waiting");
    assert!(!waiting.watch && !waiting.dialog, "{waiting:?}");
    assert_eq!(waiting.missing, ["Bob"]);
    ms[1].paused = false;
    run(&mut ms, stuck + 60);
    assert!(ms[0].session.waiting().is_none());
    assert!(
        ms[0].session.take_deadline_reset(),
        "a stall over 200 ms resets the deadline"
    );
    assert_eq!(ms[0].hashes, ms[1].hashes);
    assert!(
        ms[0].notices.is_empty() && ms[1].notices.is_empty(),
        "nobody was dropped"
    );
}

#[test]
fn a_joiner_with_a_different_map_is_refused_cleanly() {
    let Some(rig) = rig() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let host_store = store(&rig, dir.path(), "host");
    let mut install = storage::open_install(&host_store.0).unwrap();
    let identity = lobby::identity(&mut install, &host_store).unwrap();
    let mut config = LobbySpec::default().config(rig.name.clone(), map_hash(&rig.bytes) ^ 1, Setup::default(), 1, "");
    config.lobby.local = 0;
    let listener = tcp::listen(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    let host = thread::spawn({
        let identity = identity.clone();
        move || {
            let mut said = Vec::new();
            // The slot stays open; a second, matching joiner fills it.
            let _ = tcp::host_game(&listener, config, &identity, &[1], |l| said.push(l.to_string()));
            said
        }
    });
    let join_store = store(&rig, dir.path(), "joiner");
    let mut install = storage::open_install(&join_store.0).unwrap();
    let err = lobby::join(&format!("127.0.0.1:{port}"), &mut install, &join_store, "Bob", |_| {})
        .err()
        .expect("refused");
    assert!(err.to_string().contains("different map"), "{err:#}");
    // Let the host's loop finish by joining it properly with a stand-in that accepts the same hash.
    let _ = tcp::join_game(&format!("127.0.0.1:{port}"), identity, "x", |_| Ok(()), |_| {});
    let said = host.join().unwrap();
    assert!(
        said.iter()
            .any(|l| l.contains("declined") && l.contains("different map")),
        "{said:?}"
    );
}

/// A wire that only listens and says nothing, to check a machine reports a silent peer without help from the network.
struct Mute;

impl Wire for Mute {
    fn send(&mut self, _: Msg) {}
    fn poll(&mut self) -> Vec<Inbound> {
        Vec::new()
    }
}

#[test]
fn a_machine_alone_with_a_mute_peer_waits_and_names_who() {
    let Some(rig) = rig() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let (host, _joiner) = handshake(
        &rig,
        dir.path(),
        "humans=2,fixed,seed=1,speed=8,row=4",
        Setup::default(),
    );
    let mut start = host;
    start.wire = Box::new(Mute);
    let mut m = machine(&rig, dir.path(), "host", start, false);
    assert!(!m.advance(), "nothing arrives, so the first turn cannot run");
    assert_eq!(m.world.tick, 0);
    assert_eq!(m.session.waiting().unwrap().missing, ["Bob"]);
}
