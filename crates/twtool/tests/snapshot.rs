//! The whole-map behaviour snapshot: runs `twtool snapshot` on a real installation and checks it against
//! `tests/snapshots/sim.txt`, so a refactor of the simulation can show it changes no behaviour. Set
//! `TIDEWATER_INSTALL` to the game's installation; without it the test does nothing. What the snapshot runs and what
//! its two fingerprints cover is in `src/snapshot.rs`.
//!
//! **How much runs.** The file's header gives the cycles, checkpoint interval and modes. A debug build of the tests
//! (plain `cargo test`) runs every map and mode but only up to cycle 2500 (about two minutes), comparing the
//! checkpoints it reaches; a release build (`cargo test --release`) or `TIDEWATER_SNAPSHOT_FULL=1` runs the whole file
//! (about two minutes in release on ten cores; far longer in debug).
//!
//! **Which columns.** By default both fingerprints and the unit count must match. A refactor that changes how state
//! is stored (a field renamed, added, removed or reordered) changes the `world` fingerprint without changing
//! behaviour: run with `TIDEWATER_SNAPSHOT_TIER=behaviour` to compare only the behaviour fingerprint, then bless.
//!
//! **Blessing.** When a behaviour change is intended, regenerate the file with `TIDEWATER_SNAPSHOT_BLESS=1 cargo test
//! --release -p twtool --test snapshot` (it runs in full and rewrites the file with the same header settings), or
//! directly with `twtool snapshot --install DIR --out crates/twtool/tests/snapshots/sim.txt` (the defaults are the
//! committed settings), and commit the new file with the change that explains it.
//!
//! **Finding the cause.** The report names the first differing map, mode and checkpoint. `twtool snapshot --install
//! DIR --map NAME --modes MODE --parts`, run on the old and the new code, shows which part of the behaviour
//! fingerprint (oracle fields, player counters, terrain, fog, messages and sounds) moved first.

use std::collections::BTreeMap;
use std::process::Command;

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots/sim.txt");
/// The last cycle a debug build compares by default.
const SHORT: u32 = 2500;

fn on(var: &str) -> bool {
    std::env::var_os(var).is_some_and(|v| !v.is_empty() && v != "0")
}

/// The header's `cycles`, `every` and `modes`.
fn settings(text: &str) -> (u32, u32, String) {
    let line = text
        .lines()
        .find(|l| l.starts_with("# cycles "))
        .expect("the snapshot has a settings line");
    let words: Vec<&str> = line.split_whitespace().collect();
    let get = |key: &str| {
        let i = words.iter().position(|&w| w == key).expect("setting");
        words[i + 1].to_string()
    };
    (
        get("cycles").parse().expect("cycles"),
        get("every").parse().expect("every"),
        get("modes"),
    )
}

/// Rows keyed by map, mode and cycle (or `error`), in file order, with the rest of the line.
fn rows(text: &str) -> Vec<((String, String, String), Vec<String>)> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            assert!(f.len() >= 4, "short snapshot line {l:?}");
            (
                (f[0].to_string(), f[1].to_string(), f[2].to_string()),
                f[3..].iter().map(|s| s.to_string()).collect(),
            )
        })
        .collect()
}

#[test]
fn every_map_behaves_as_snapshotted() {
    let Some(install) = std::env::var_os("TIDEWATER_INSTALL") else {
        eprintln!("skipped: set TIDEWATER_INSTALL to run");
        return;
    };
    let expected = std::fs::read_to_string(FILE).expect("the snapshot file");
    let (cycles, every, modes) = settings(&expected);
    let bless = on("TIDEWATER_SNAPSHOT_BLESS");
    let full = bless || on("TIDEWATER_SNAPSHOT_FULL") || !cfg!(debug_assertions);
    let upto = if full {
        cycles
    } else {
        ((SHORT / every).max(1) * every).min(cycles)
    };
    let out = Command::new(env!("CARGO_BIN_EXE_twtool"))
        .arg("snapshot")
        .arg("--install")
        .arg(&install)
        .args([
            "--cycles",
            &upto.to_string(),
            "--every",
            &every.to_string(),
            "--modes",
            &modes,
        ])
        .output()
        .expect("twtool runs");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "twtool snapshot failed: {stderr}");
    let actual = String::from_utf8(out.stdout).expect("text");
    if bless {
        std::fs::write(FILE, &actual).expect("writing the snapshot");
        eprintln!("blessed {FILE}");
        return;
    }
    let behaviour_only = std::env::var("TIDEWATER_SNAPSHOT_TIER").is_ok_and(|t| t == "behaviour");
    let compared = |v: &[String]| -> Vec<String> {
        if behaviour_only && v.len() >= 3 {
            v[..1].to_vec()
        } else {
            v.to_vec()
        }
    };
    let ours: BTreeMap<_, _> = rows(&actual).into_iter().collect();
    let theirs = rows(&expected);
    let mut first = None;
    let mut runs: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut world_only = 0;
    let mut checked = 0;
    for (key, want) in &theirs {
        let reached = key.2 == "error" || key.2.parse::<u32>().is_ok_and(|c| c <= upto);
        if !reached {
            continue;
        }
        checked += 1;
        let got = ours.get(key);
        if got.is_some_and(|g| compared(g) == compared(want)) {
            continue;
        }
        if got.is_some_and(|g| g.first() == want.first()) {
            world_only += 1;
        }
        if seen.insert((&key.0, &key.1)) {
            runs.push(format!("{} {} (first at cycle {})", key.0, key.1, key.2));
        }
        first.get_or_insert_with(|| {
            format!(
                "{} {} cycle {}:\n  expected {}\n  got      {}",
                key.0,
                key.1,
                key.2,
                want.join(" "),
                got.map_or("nothing (map or mode missing)".to_string(), |g| g.join(" "))
            )
        });
    }
    let known: std::collections::BTreeSet<_> = theirs.iter().map(|(k, _)| (&k.0, &k.1)).collect();
    let extra: std::collections::BTreeSet<_> = ours.keys().filter(|k| !known.contains(&(&k.0, &k.1))).collect();
    if !extra.is_empty() {
        eprintln!(
            "note: {} rows for maps or modes the snapshot doesn't list (ignored)",
            extra.len()
        );
    }
    eprintln!("compared {checked} rows up to cycle {upto}");
    if let Some(first) = first {
        let hint = if world_only > 0 {
            format!(
                "\n{world_only} differing rows match on the behaviour fingerprint and differ only in the world \
                 fingerprint or unit count; if the stored state's layout changed on purpose, check with \
                 TIDEWATER_SNAPSHOT_TIER=behaviour, then bless"
            )
        } else {
            String::new()
        };
        panic!(
            "the simulation's behaviour differs from {FILE}\nfirst difference: {first}\n{} runs differ:\n  {}{hint}\n\
             If the change is intended, bless the file (see this test's module doc).",
            runs.len(),
            runs.iter().take(30).cloned().collect::<Vec<_>>().join("\n  ")
        );
    }
}
