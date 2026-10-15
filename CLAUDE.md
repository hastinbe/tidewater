# Tidewater: agent instructions

Rust reimplementation (an engine-neutral core in an SDL 3 shell) of the engine behind *Warcraft II: Battle.net Edition* (with *Beyond the Dark Portal*),
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
  `strings`, `sim`, `oracle`: compare the simulation with a per-cycle trace of the original game, `snapshot`: the
  whole-map behaviour snapshot `tests/snapshot.rs` checks).
- `crates/sim`: the deterministic game simulation (no engine types; placeholders until specs land, see `Rules`).
- `crates/client`: the game client: a library (`lib.rs`, no engine types, no `unsafe`) holding the core (`core.rs`:
  `Core::advance` runs a frame) and `host.rs` (start-up from the command line, the headless scripted loop, helpers
  for the shell), and the `tidewater` binary, the SDL 3 shell (`shell/main.rs`: window, input events, mouse lock,
  sound device; `shell/sdl.rs` is our thin safe wrapper over `sdl3-sys` and the only `unsafe` code). SDL is built
  from source and linked in, so every build needs CMake and a C compiler, not an SDL install.
  `game.rs` runs the game screen, `args.rs` holds the command line. `scene.rs` builds the map and unit images, `hud.rs`
  lays out and draws the interface (frame, minimap, resources, selection, command buttons) into one picture, `panel.rs` holds the command panel's button sets and rules, `info.rs` the unit information panel's rules.
  `campaign.rs` holds the campaign flow's rules, `video.rs` the Smacker playback helpers and `display.rs` the window
  scale, black border, map zoom and mouse-lock rules;
  `frontend.rs` runs the screens around a game (videos, title, menu, act screens, briefings, the outcome dialogs,
  the score screen, endings, credits) as the flow's states (`states.rs`), and the game screen's systems run only in `AppState::Game`.
  `menuscreen.rs` holds the menu screens' rules (the shared menu picture, which switches fade, button and text colours).
  `--goto SCREEN` opens a flow screen directly for development.
  `audio.rs` decodes WAV sounds and mixes them at 22,050 Hz stereo; the shell streams the mixer to the device.
  `storage.rs` is the `Storage` trait for everything read or written outside the core (installation, saves,
  settings, screenshots, wall clock) and the desktop's folders.
  The window shows one 640 × 480 picture composed on the CPU (`compose.rs`; `core.rs` places the layers, the shell
  shows it). `script.rs` runs scripted scenes (`--script NAME --script-out DIR [--headless]`) for the frame
  snapshot `tests/frames.rs`; the design of the core and its shell is `docs/design/client-core.md`.
- `docs/specs/`: reviewed behaviour specifications, the only source for engine behaviour.

## Workflow

- The toolchain is pinned in `rust-toolchain.toml`; dependencies track their newest releases.
- Building needs CMake and a C compiler besides Rust (SDL 3 is built from source). There is no CI config in the
  repository; the full check is run locally.
- `cargo fmt`, `cargo clippy --all-targets` (no warnings) and `cargo test` before every commit.
- Conventional commits (`feat:`, `fix:`, `docs:`, ...).
- Cite the public source for every format detail in the module doc of the code that uses it.
