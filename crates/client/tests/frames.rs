//! The frame snapshot: runs the client's scripted scenes (`src/script.rs`) without a window on a real installation and
//! checks the fingerprints of every frame and of every sound started against `tests/snapshots/`, so moving the
//! client onto an engine-neutral core (`docs/design/client-core.md`) can show it changes nothing on the screen or in
//! the sound. Set `TIDEWATER_INSTALL` to the game's installation; without it the tests do nothing.
//!
//! **What is hashed.** Each frame is the whole 640 × 480 screen as the window would show it, composed on the CPU from
//! the client's layers (`src/frame.rs`); a line covers a run of frames with the same fingerprint. A sound line gives
//! the frame it starts on, a fingerprint of the sound's bytes, its loudness and its playback mode. The files hold
//! fingerprints only.
//!
//! **The mixed sound.** The core mixes every frame's share of the sound (`src/audio.rs`: 22,050 Hz stereo 16-bit,
//! 367 or 368 sample frames a frame); `tests/snapshots/{scene}-audio.txt` holds a fingerprint per second of it, equal
//! seconds in a row on one line.
//!
//! **Blessing.** When a change to what is shown is intended, regenerate with `TIDEWATER_SNAPSHOT_BLESS=1 cargo test
//! --release -p tidewater-client --test frames` and commit the new files with the change that explains it.
//!
//! **Against Bevy.** `bevy_draws_the_composed_frames` (ignored: it opens a window and needs a GPU) runs the scenes in a
//! window and compares Bevy's own screenshot with the composed frame at each of the scene's shots. Run it with
//! `cargo test --release -p tidewater-client --test frames -- --ignored`; set `TIDEWATER_SCRIPT_OUT` to a folder to
//! keep the pictures (both frames and their difference) for a look.

use std::path::{Path, PathBuf};
use std::process::Command;

const SCENES: &[&str] = &["campaign", "outcome", "ending", "load", "build", "flow"];

fn on(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| !v.is_empty() && v != "0")
}

fn snapshot(scene: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/snapshots/{scene}.txt"))
}

/// The audio snapshot's file for a scene.
fn audio_snapshot(scene: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/snapshots/{scene}-audio.txt"))
}

/// Runs a scene; returns its fingerprint file's text.
fn run(install: &std::ffi::OsStr, scene: &str, out: &Path, headless: bool) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tidewater"));
    cmd.arg("--install")
        .arg(install)
        .args(["--script", scene, "--script-out"])
        .arg(out);
    if headless {
        cmd.arg("--headless");
    }
    let status = cmd.status().expect("the client runs");
    assert!(status.success(), "the scripted run of {scene} failed");
    std::fs::read_to_string(out.join(format!("{scene}.txt"))).expect("the run's fingerprints")
}

/// The first differing line, with its neighbours' context.
fn first_difference(want: &str, got: &str) -> Option<String> {
    let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let i = (0..w.len().max(g.len())).find(|&i| w.get(i) != g.get(i))?;
    Some(format!(
        "line {}:\n  expected {}\n  got      {}",
        i + 1,
        w.get(i).unwrap_or(&"(end)"),
        g.get(i).unwrap_or(&"(end)")
    ))
}

#[test]
fn every_scene_shows_and_sounds_as_snapshotted() {
    let Some(install) = std::env::var_os("TIDEWATER_INSTALL") else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let bless = on("TIDEWATER_SNAPSHOT_BLESS");
    let mut failures = Vec::new();
    for scene in SCENES {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let got = run(&install, scene, dir.path(), true);
        let got_audio =
            std::fs::read_to_string(dir.path().join(format!("{scene}-audio.txt"))).expect("the run's mixed sound");
        for (file, got) in [(snapshot(scene), got), (audio_snapshot(scene), got_audio)] {
            if bless {
                std::fs::write(&file, &got).expect("writing the snapshot");
                eprintln!("blessed {}", file.display());
                continue;
            }
            let want = std::fs::read_to_string(&file).expect("the snapshot file");
            if let Some(d) = first_difference(&want, &got) {
                failures.push(format!(
                    "{}: {d}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the frames, sounds or mixed sound differ from tests/snapshots/:\n{}\nIf the change is intended, bless the files (see this \
         test's module doc).",
        failures.join("\n")
    );
}

#[test]
#[ignore = "opens a window and needs a GPU"]
fn bevy_draws_the_composed_frames() {
    let Some(install) = std::env::var_os("TIDEWATER_INSTALL") else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let mut report = Vec::new();
    for scene in SCENES {
        let tmp = tempfile::tempdir().expect("a temporary folder");
        let out = std::env::var_os("TIDEWATER_SCRIPT_OUT").map_or_else(|| tmp.path().to_path_buf(), PathBuf::from);
        let windowed = run(&install, scene, &out, false);
        let compare = std::fs::read_to_string(out.join(format!("{scene}-compare.txt"))).expect("the comparison");
        for line in compare.lines() {
            let (shot, differing) = line.split_once('\t').expect("shot and count");
            if differing != "0" {
                report.push(format!("{scene} {shot}: {differing} pixels differ"));
            }
        }
        // The window shows what the run without one composes.
        let want = std::fs::read_to_string(snapshot(scene)).expect("the snapshot file");
        if let Some(d) = first_difference(&want, &windowed) {
            report.push(format!(
                "{scene}: the windowed run's fingerprints differ from the snapshot at {d}"
            ));
        }
    }
    assert!(report.is_empty(), "{}", report.join("\n"));
}
