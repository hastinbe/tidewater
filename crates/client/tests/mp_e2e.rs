//! Network play end to end, through the menus: two or three separate `tidewater` programs on this machine, each with
//! its own data folder, settings, saves and recordings, each driven by a scripted scene (`script.rs`, the `mp-*`
//! scenes) through Multi Player Game, the game list or Direct IP, the lobby and a game: orders, chat both ways, a
//! player leaving. Then the programs' recordings are compared cycle by cycle, as `twtool desync-diff` does.
//!
//! Set `TIDEWATER_INSTALL` to the game files (an imported `game` folder, or an installation) to run; without it every
//! test does nothing, like the other tests that need game files. The Local Network tests also need broadcast to reach
//! this machine (the game list is filled by the UDP beacon): they check it first and skip without it, as in a CI
//! container with no broadcast route. The games use the real ports (TCP and UDP 6112), so the tests take turns, and a
//! Local Network game is announced on the machine's network while its lobby is open.
//!
//! A debug build draws every frame slowly: the five games take about four minutes. `cargo test --release -p
//! tidewater-client --test mp_e2e` takes about a minute. `docs/design/network.md`, "Tests", says how to look at the
//! pictures and logs a run leaves.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tidewater_client::net::beacon::{Announce, Announcer, Finder};
use tidewater_formats::install::Install;
use tidewater_replay::Replay;
use tidewater_replay::run::{first_desync, verify};
use tidewater_replay::start::new_world;

/// The games share the real ports: one at a time.
static TURN: Mutex<()> = Mutex::new(());

/// The game files, if given.
fn install() -> Option<PathBuf> {
    std::env::var_os("TIDEWATER_INSTALL").map(PathBuf::from)
}

/// How long a game may take before its programs are stopped (`TIDEWATER_MP_E2E_TIMEOUT`, seconds).
fn timeout() -> Duration {
    let secs = std::env::var("TIDEWATER_MP_E2E_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(if cfg!(debug_assertions) { 900 } else { 180 });
    Duration::from_secs(secs)
}

/// Whether a broadcast beacon reaches a listener on this machine (the Local Network game list needs it).
fn broadcast_works() -> bool {
    let Ok(mut finder) = Finder::bind(0) else { return false };
    let target = std::net::SocketAddr::from((std::net::Ipv4Addr::BROADCAST, finder.port()));
    let Ok(mut announcer) = Announcer::to(target) else {
        return false;
    };
    let a = Announce::ours(1, 1, 2, "probe", "probe");
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        announcer.tick(Instant::now(), &a);
        if !finder.poll(Instant::now()).is_empty() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// One program of a game.
struct Program {
    scene: &'static str,
    dir: PathBuf,
    child: Option<Child>,
}

impl Program {
    /// Starts `scene` headless in its own folder under `base`: a data folder whose `game` is the game files (or the
    /// installation read in place), and the scene's own output, saves, settings and recordings.
    fn start(base: &Path, install: &Path, scene: &'static str) -> Program {
        let dir = base.join(scene);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the program's folder");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_tidewater"));
        if install.join("manifest.json").is_file() {
            // An imported game folder: each program gets its own data folder that links to it.
            let data = dir.join("data");
            std::fs::create_dir_all(&data).expect("the data folder");
            link(install, &data.join("game"));
            cmd.arg("--data-dir").arg(&data);
        } else {
            cmd.arg("--install").arg(install);
        }
        let log = std::fs::File::create(dir.join("log.txt")).expect("the log");
        cmd.args(["--script", scene, "--script-out"])
            .arg(dir.join("out"))
            .args(["--headless", "--record-replay", "--no-update-check"])
            // The beacon is the point here (the frame snapshot switches it off).
            .env_remove("TIDEWATER_NO_BEACON")
            // The shots are kept as pictures, to look at after a run.
            .env("TIDEWATER_SCRIPT_PICTURES", "shots")
            .stdout(Stdio::null())
            .stderr(log);
        let child = cmd.spawn().expect("the client runs");
        Program {
            scene,
            dir,
            child: Some(child),
        }
    }

    /// The lines of `<scene>-events.txt` (`script::Events`).
    fn events(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("out").join(format!("{}-events.txt", self.scene)))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// The `game ...` line: the slot, the rules and the options in force.
    fn game_line(&self) -> String {
        self.events()
            .into_iter()
            .find(|l| l.starts_with("game "))
            .unwrap_or_else(|| panic!("{} never got into a game:\n{}", self.scene, self.report()))
    }

    fn saw(&self, text: &str) -> bool {
        self.events().iter().any(|l| l.starts_with("msg ") && l.contains(text))
    }

    /// The program's recording.
    fn replay(&self) -> Replay {
        let dir = self.dir.join("out").join("save").join("replays");
        let file = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: no recordings in {}: {e}", self.scene, dir.display()))
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|x| x == "twrep"))
            .unwrap_or_else(|| panic!("{}: no recording", self.scene));
        Replay::decode(&std::fs::read(&file).unwrap()).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
    }

    /// What the program saw and said, for a failure's message.
    fn report(&self) -> String {
        let log = std::fs::read_to_string(self.dir.join("log.txt")).unwrap_or_default();
        format!(
            "--- {} ({})\n{}\n{}",
            self.scene,
            self.dir.display(),
            self.events().join("\n"),
            log.trim_end()
        )
    }
}

impl Drop for Program {
    fn drop(&mut self) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) {
    std::os::unix::fs::symlink(from, to).expect("a link to the game files");
}

#[cfg(windows)]
fn link(from: &Path, to: &Path) {
    std::os::windows::fs::symlink_dir(from, to).expect("a link to the game files");
}

/// Runs the scenes as one game, each a program, started a moment apart in order (the creator first), until every
/// program has ended; panics with what each saw if one fails or the time runs out.
fn play(name: &str, install: &Path, scenes: &[&'static str]) -> Vec<Program> {
    let base = Path::new(env!("CARGO_TARGET_TMPDIR")).join("mp-e2e").join(name);
    let mut programs = Vec::new();
    for &scene in scenes {
        programs.push(Program::start(&base, install, scene));
        std::thread::sleep(Duration::from_millis(1000));
    }
    let end = Instant::now() + timeout();
    let mut failed = Vec::new();
    loop {
        let mut running = false;
        for p in &mut programs {
            let Some(c) = &mut p.child else { continue };
            match c.try_wait().expect("the program's state") {
                Some(status) => {
                    if !status.success() {
                        failed.push(format!("{} ended with {status}", p.scene));
                    }
                    p.child = None;
                }
                None => running = true,
            }
        }
        if !running {
            break;
        }
        if Instant::now() > end {
            failed.push(format!("the time ran out ({:?})", timeout()));
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let report = || programs.iter().map(Program::report).collect::<Vec<_>>().join("\n");
    assert!(failed.is_empty(), "{name}: {}\n{}", failed.join("; "), report());
    programs
}

/// The checks every game makes: each program got into the game in its seat with the creator's options in force, nobody
/// was dropped while all played, and every recording matches the creator's at every cycle they share.
fn check(install: &Path, programs: &[Program], rules: &str) {
    let host = &programs[0];
    let mut seats = std::collections::HashSet::new();
    for (i, p) in programs.iter().enumerate() {
        let line = p.game_line();
        let report = p.report();
        assert!(line.contains(&format!("rules={rules} ")), "{report}");
        assert!(
            line.contains("multiplayer=true") && line.contains("network=true"),
            "{report}"
        );
        // The creator's choices in the lobby: rally points (the simulation's) and smart attack-move (a C+L option),
        // and the creator's own health bars setting (C+L): in force on every machine.
        assert!(line.contains("sim-extras=[rally points]"), "{report}");
        assert!(
            line.contains("health-bars always") && line.contains("smart-attack-move on"),
            "{report}"
        );
        if p.scene == "mp-watch" {
            assert!(line.contains("observing=true"), "{report}");
        } else {
            // The creator is slot 0; joiners take the rows in the order they come in, each its own.
            let slot = line
                .split("slot=")
                .nth(1)
                .and_then(|r| r.split(' ').next())
                .unwrap_or("?")
                .to_string();
            assert!(
                (i == 0) == (slot == "0") && line.contains("observing=false"),
                "{report}"
            );
            assert!(seats.insert(slot), "{report}");
            assert!(!p.saw("was dropped"), "{report}");
        }
    }
    // The recordings: the same game at every cycle, and each replays on its own.
    let mut inst = Install::open(install).expect("the installation");
    let first = host.replay();
    let map = inst.read(&first.header.map_name).expect("the map");
    for p in &programs[1..] {
        let r = p.replay();
        assert_eq!(r.header.seed, first.header.seed);
        let mut a = new_world(&mut inst, &map, &first.header).unwrap();
        let mut b = new_world(&mut inst, &map, &r.header).unwrap();
        let cycles = first.cycles.min(r.cycles);
        assert!(cycles >= 250, "{}: only {cycles} cycles in common", p.scene);
        assert_eq!(
            first_desync(&mut a, &first, &mut b, &r),
            None,
            "{} and {} differ",
            host.scene,
            p.scene
        );
    }
    for p in programs {
        let r = p.replay();
        let mut w = new_world(&mut inst, &map, &r.header).unwrap();
        verify(&mut w, &r).unwrap_or_else(|m| panic!("{}'s recording: {m:?}", p.scene));
    }
    // Commands travelled: every player's orders are in the creator's recording.
    let players: Vec<u8> = first
        .commands
        .iter()
        .flat_map(|c| c.commands.iter().map(tidewater_sim::Command::player))
        .collect();
    let seats = programs.iter().filter(|p| p.scene != "mp-watch").count() as u8;
    for seat in 0..seats {
        assert!(players.contains(&seat), "no command from slot {seat}: {players:?}");
    }
}

/// The shared start: game files, the port's turn, and (for a Local Network game) broadcast.
fn ready(lan: bool) -> Option<(PathBuf, std::sync::MutexGuard<'static, ()>)> {
    let Some(install) = install() else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return None;
    };
    let turn = TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if lan && !broadcast_works() {
        eprintln!("skipped: broadcast does not reach this machine, so the Local Network list stays empty");
        return None;
    }
    Some((install, turn))
}

/// Local Network, Enhanced rules: the joiner finds the game on the list, both play, chat both ways, the joiner opens the
/// game menu and the game goes on; the creator leaves and the joiner is told and wins.
#[test]
fn two_programs_play_a_local_network_game_from_the_menus() {
    let Some((install, _turn)) = ready(true) else { return };
    let ps = play("lan", &install, &["mp-lan-host", "mp-lan-join"]);
    check(&install, &ps, "enhanced");
    let (host, joiner) = (&ps[0], &ps[1]);
    assert!(host.saw("Joiner: hello from the joiner"), "{}", host.report());
    assert!(joiner.saw("Hosty: hello from the host"), "{}", joiner.report());
    assert!(joiner.saw("Hosty left the game"), "{}", joiner.report());
    assert!(joiner.saw("Victory!"), "{}", joiner.report());
    // The joiner held the game menu open for a hundred cycles (its scene waits on them there): the creator never
    // waited for it.
    assert!(!host.saw("Waiting for players"), "{}", host.report());
}

/// Direct IP: the joiner types the creator's address.
#[test]
fn two_programs_play_a_direct_ip_game_from_the_menus() {
    let Some((install, _turn)) = ready(false) else { return };
    let ps = play("ip", &install, &["mp-ip-host", "mp-ip-join"]);
    check(&install, &ps, "enhanced");
    assert!(ps[1].saw("Hosty left the game"), "{}", ps[1].report());
}

/// Classic rules (opt-in): the joiner ticks Ready, chat shows Classic's "sent" notice, and here the joiner leaves.
#[test]
fn two_programs_play_a_classic_game_from_the_menus() {
    let Some((install, _turn)) = ready(true) else { return };
    let ps = play("classic", &install, &["mp-classic-host", "mp-classic-join"]);
    check(&install, &ps, "classic");
    assert!(ps[0].saw("Joiner left the game"), "{}", ps[0].report());
    assert!(ps[0].saw("Victory!"), "{}", ps[0].report());
}

/// An observer joins a two-player Local Network game with Watch Game, runs the same game and sends nothing.
#[test]
fn an_observer_watches_a_local_network_game_from_the_menus() {
    let Some((install, _turn)) = ready(true) else { return };
    let ps = play("watched", &install, &["mp-watched-host", "mp-watched-join", "mp-watch"]);
    check(&install, &ps, "enhanced");
    let watcher = &ps[2];
    assert!(watcher.saw("Joiner: hello from the joiner"), "{}", watcher.report());
    // An observer has no victory of its own.
    assert!(!watcher.saw("Victory!"), "{}", watcher.report());
}

/// Three players: one leaves, the other two play on in step, then the creator leaves and the last one wins.
#[test]
fn three_programs_play_a_local_network_game_from_the_menus() {
    let Some((install, _turn)) = ready(true) else { return };
    let ps = play(
        "three",
        &install,
        &["mp-three-host", "mp-three-join", "mp-three-second"],
    );
    check(&install, &ps, "enhanced");
    let second = &ps[2];
    assert!(second.saw("Joiner left the game"), "{}", second.report());
    assert!(second.saw("Hosty left the game"), "{}", second.report());
    assert!(second.saw("Victory!"), "{}", second.report());
}
