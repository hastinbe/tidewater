# Tidewater: agent instructions

Rust + Bevy reimplementation of the engine behind *Warcraft II: Battle.net Edition* (with *Beyond the Dark Portal*),
running on the user's own game data. This repository is written to be published: read [CLEANROOM.md](CLEANROOM.md)
before writing any code, and follow it exactly.

## Hard rules

- **This repo is the implementation side.** Never put disassembly, decompiler output, code derived from the original
  program, or any game file or extract into it, and never work from them here. Allowed sources are listed in
  CLEANROOM.md. If a task needs knowledge of the original program's behaviour, request a spec; don't go and look.
- **Nothing personal or private:** no host names, IP addresses, user names, personal paths or secrets in files or
  commit messages. The hooks (`git config core.hooksPath .githooks`) enforce this; never bypass them with
  `--no-verify`. History is never rewritten, so a leak must be stopped before the commit exists.
- **Tests use synthetic data** built by our own code. Checks against a real install go through `twtool` with a path
  given at run time, never a fixture copied into the repo.

## Layout

- `crates/formats`: readers for the installation's data (MPQ archives, PUD maps, tilesets, GRP sprites (compressed
  and uncompressed), PCX images, raw interface pictures and cursors (`raw`), fonts (`fnt`), Smacker video (`smk`), string tables, unit and
  upgrade tables, map restrictions).
- `crates/twtool`: inspection CLI (`mpq-*`, `render-map`, `render-grp`, `render-pcx`, `smk-frame`, `smk-audio`, `grp-sheet`, `units`, `upgrades`,
  `strings`, `sim`).
- `crates/sim`: the deterministic game simulation (no engine types; placeholders until specs land, see `Rules`).
- `crates/client`: the Bevy game client (`tidewater` binary). `scene.rs` builds the map and unit images, `hud.rs`
  lays out and draws the interface (frame, minimap, resources, selection, command buttons) into one picture, `panel.rs` holds the command panel's button sets and rules, `info.rs` the unit information panel's rules; all are Bevy-free.
  `campaign.rs` holds the campaign flow's rules and `video.rs` the Smacker playback helpers (both Bevy-free);
  `frontend.rs` runs the screens around a game (videos, title, menu, act screens, briefings, the outcome dialogs,
  the score screen, endings, credits) as app states, and the game screen's systems run only in `AppState::Game`.
  `--goto SCREEN` opens a flow screen directly for development.
- `docs/specs/`: reviewed behaviour specifications, the only source for engine behaviour.

## Workflow

- The toolchain is pinned in `rust-toolchain.toml`; dependencies track their newest releases.
- `cargo fmt`, `cargo clippy --all-targets` (no warnings) and `cargo test` before every commit.
- Conventional commits (`feat:`, `fix:`, `docs:`, ...).
- Cite the public source for every format detail in the module doc of the code that uses it.
