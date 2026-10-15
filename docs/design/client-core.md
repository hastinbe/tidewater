# Design: an engine-neutral client core

**Bottom line.** The client is a plain-Rust core that the host calls once per frame:
`advance(real_time_elapsed, platform) -> &Frame`. The shell only shows a 640 × 480 frame, hands over input, pulls
mixed audio, provides the files, and knows the time. All three phases are done (steps 1 to 10). **We recommend SDL 3
as the shell:** in the phase 3 trial it showed the same pixels as Bevy at half the memory, under half the CPU and a
quarter of the sound latency, with nothing missing, and it reaches every port target. Making it the default is about
a day with Claude. Bevy stays the default until you decide ([Trial shells](#trial-shells-phase-3)).

Backlog item B-039. Nothing here changes game behaviour; where a step can't stay pixel-identical, the table in
[What can't stay identical](#what-cant-stay-identical) says why.

## What we need from you

- [ ] **Nick:** the shell to keep. **[Recommended: SDL 3;** drop the miniquad shell and, once the SDL shell is the
      default, the Bevy one.]
- [ ] **Nick:** the SDL wrapper. **[Recommended: keep our own `sdl.rs`** over `sdl3-sys` (520 lines, every `unsafe`
      in one file) rather than the `sdl3` crate's larger safe API, until a port needs more of SDL.]

The four earlier questions are decided (2026-10-14):

- [x] **Nick:** the frame format. **Decided: RGBA for now.** An indexed frame with a palette comes later, after a
      spec read of `docs/specs/ui/*`.
- [x] **Nick:** draw the pointer in the core on every screen, not only in a game? **Decided: yes.** The core draws
      the game's normal arrow on every screen, per `ui/screen-and-cursors.md` "Pointer shapes". The flow's screens
      show the game's arrow instead of the system's, so the frame snapshot is re-blessed once, in its own commit.
- [x] **Nick:** the output sample rate of the mixer. **Decided: 22,050 Hz stereo, 16-bit,** the rate of the music
      files (`ui/music.md`); shells resample if the device needs it.
- [x] **Nick:** the 3DS. **Decided: out of the core's requirements for now;** revisit after the SDL3 trial.

## Short answers

| Question | Answer |
|---|---|
| Does Bevy-drawn text block the move? | **No.** All text is drawn by our code in the game's fonts into pictures. |
| Is the composed frame faithful? | **Yes.** In a window at one pixel per screen pixel, Bevy's screenshot equals the composed frame at all 28 shots. |
| Is the snapshot deterministic? | **Yes,** after one fix: an Escape that closed the game menu could also quit, depending on system order. |
| How fast is the snapshot? | **Fast.** The 1,372-frame scene runs in about 5 s in release, 80 s in a debug build. |
| What still needs Bevy? | **Only the shell.** Since step 9, `shell.rs` (633 lines: window, input events, the mouse lock, the sound device) is the only module that uses Bevy; before phase 2 it was 8,464 lines in 12 modules. Without the default `bevy` feature the core builds without it. |
| Do the trial shells show the same game? | **Yes.** SDL 3 and miniquad give 0 differing pixels at every shot of all six scenes, and the same frame and audio fingerprints. |
| Which shell is lightest? | **miniquad, closely followed by SDL 3.** Both use about 145 MB and 30 to 40% of a core; Bevy uses 280 to 315 MB and 86%. |

## Core API

The host drives the loop. The core never blocks, never reads the clock and never touches a device.

```rust
// As built in step 9 (`core.rs`):
pub struct Core {
    pub input: Input,         // the shell puts the system's events in between frames
    pub audio: Audio,         // the mixer; `audio.output()` is what the device callback pulls (`Output::mix`)
    pub frame: Frame,         // the screen composed by the last `advance`
    pub cursor_visible: bool, // false: the screen draws its own pointer
    pub title: Option<String>,
    pub exit: bool,           // the program asked to end normally
    /* flow, game, interface, storage */
}

impl Core {
    pub fn new(opts: Args, store: Store, front: Front, flow: Flow, stored: Stored, script: Option<script::Run>) -> Core;
    /// Runs one frame, `elapsed` real time after the last; returns the screen to show.
    pub fn advance(&mut self, elapsed: Duration, platform: &mut dyn Platform) -> &Frame;
    /// "Saving": the settings, on a normal exit.
    pub fn store_settings(&self);
}

// The proposal still to come: suspend(), resume(), and an indexed frame.

pub struct Frame {
    pub pixels: Pixels,         // 640 × 480
    pub pointer_drawn: bool,    // false: the shell may show the system pointer
}

pub enum Pixels {
    Indexed { index: Box<[u8; 640 * 480]>, palette: [[u8; 3]; 256] },
    Rgba(Box<[u8; 640 * 480 * 4]>),
}
```

| Decision | Why |
|---|---|
| One call per frame, real time in | The game's cycle runs on a fixed step (`timing-and-rng.md`); the core keeps the accumulator, so every shell gets the same cycle timing. |
| The core keeps Bevy's time rules | Today a frame's real time is capped at 250 ms (Bevy's virtual time) and the fixed steps run before the frame's `Update` systems. Keeping both keeps the snapshot identical. |
| Frame is RGBA first | The interface and the flow's screens use their own colours (button fills, outlines) and grey fades that aren't palette entries. An indexed frame changes those pixels. |
| `mix` takes `&self` | The audio callback runs on its own thread. The core sends sound starts to the mixer through a queue; the mixer owns its play positions. |

## Input model

The shell turns its events into one `Input` per frame, in screen pixels. Touch and gamepad rules live in the core, so
every shell behaves the same.

```rust
pub struct Input {
    pub pointer: Option<(f32, f32)>,     // screen pixels, after the letterbox (display::Letterbox::screen_point)
    pub buttons: [ButtonEdge; 3],        // left, right, middle: pressed/released this frame, held
    pub keys: Vec<KeyEvent>,             // key code, pressed or released, typed text
    pub wheel: f32,                      // lines
    pub touches: Vec<Touch>,             // id, phase, screen position
    pub pads: Vec<PadState>,             // buttons and sticks, as read this frame
    pub focused: bool,
}
```

| Device | Maps to | Notes |
|---|---|---|
| Mouse | pointer, buttons, wheel | As today. The mouse lock (`display`, "Mouse lock") stays in the shell. |
| Keyboard | key events with text | As today (`KeyboardInput` with text feeds the save dialog and chat). |
| Touch (proposed) | one finger tap: left click; hold 0.5 s: right click; drag: drag-select; two fingers: scroll the map | Our own rules (not part of the original); a later spec of ours. |
| Gamepad (proposed) | left stick moves a drawn pointer with acceleration; A: left, B: right; d-pad or right stick: scroll; Start: game menu (F10) | Our own rules; needs the core-drawn pointer. |

## Audio

| Part | Before step 7 | Now (`audio.rs`) |
|---|---|---|
| Sound effects | One Bevy `AudioPlayer` per start, volume `level / 255` | A voice per start in the mixer, gain `level / 255`; overlapping sounds all play, as before |
| Music | Bevy sink; the jukebox read the sink's play position and seeked to a loop point | The music's voice; the jukebox reads the mixer's position, and the mixer loops from the loop point itself |
| Video sound | A decoding thread fed Bevy | The same thread decodes it for the mixer; it starts where the picture is |
| Output | Bevy (rodio) | The mixer, pulled by the shell (`Output::mix`), 22,050 Hz stereo 16-bit |

The snapshot hashes sound *starts* (frame, a hash of the sound's bytes, loudness, mode) and, since step 7, the mixed
samples too (the audio snapshot).

## Storage

Done in step 8 (`crates/client/src/storage.rs`). Everything the core reads or writes outside itself goes through one
trait the shell provides:

```rust
pub trait Storage: Send + Sync {
    /// The installation's files, opened for reading with seeking (the MPQ reader seeks); `None` when there is none.
    fn install(&self, name: &str) -> io::Result<Option<Box<dyn ReadSeek>>>;
    /// A map the user named by its own path (`--map`); `None` when there is no such file.
    fn user_map(&self, path: &str) -> io::Result<Option<Vec<u8>>>;
    fn saves(&self) -> Vec<String>;                                // file names, in the folder's order
    fn read_save(&self, file: &str) -> io::Result<Vec<u8>>;
    fn write_save(&self, file: &str, bytes: &[u8]) -> Result<(), WriteError>; // whole, or no file
    fn delete_save(&self, file: &str) -> io::Result<()>;
    fn read_settings(&self) -> Option<Vec<u8>>;
    fn write_settings(&self, bytes: &[u8]) -> io::Result<()>;
    fn has_screenshot(&self, name: &str) -> bool;
    fn write_screenshot(&self, name: &str, bytes: &[u8]) -> io::Result<()>;
    fn wall_clock_secs(&self) -> u64;                              // for seeds and save times
}
```

| Part | Before | Now |
|---|---|---|
| Installation | `Install::open(path)`, `std::fs` reads | `tidewater_formats::install::Files` (`open(name) -> Option<Box<dyn ReadSeek>>`); `Install::from_files`, `Archive::from_reader`. `Install::open` and `Archive::open` are kept (a `Folder`), so `twtool` and the simulation's tests are unchanged |
| Saved games | `saves::SaveDir` over a folder path | `SaveDir` over `Storage`; an entry names its file, not a path |
| Settings | `prefs::Prefs::load/store(path)` | `load/store(&dyn Storage)` |
| Screenshots | `std::fs` in `screenshot` | `Storage::write_screenshot` |
| Wall clock | `WallClock` resource, `scene::now_secs` | `Storage::wall_clock_secs`; a scripted run's `Desktop` has a fixed clock |
| Folders | `saves::default_dir`, `saves::user_dir` | `storage::default_save_dir`, `storage::user_dir`: the desktop's choice, the same folders as before |

`storage::Desktop` is the desktop's: the installation folder given on the command line, the save folder, the
settings file and the user folder exactly as before, and the system clock. The methods take `&self` so that the core
and the installation reader can share one `Arc<dyn Storage>`; a platform that has to change state to read or write
keeps it inside. `savegame` is Bevy-free: the shell gathers the running game's parts and `savegame::capture` encodes
them (format v10, unchanged). The snapshot hashes every file a scripted run writes (its saved games and the load
scene's screenshot), so step 8 is checked to write them byte for byte as before.

## Lifecycle

| Event | Core does |
|---|---|
| `suspend()` | Pauses the game (as the pause key), stops the mixer, writes the settings. |
| `resume()` | Restarts the mixer. The game stays paused until the player resumes it. |
| Focus lost | `Input::focused = false`: the palette stops cycling (`palette::Gate`), as now. |

Mobile shells call `suspend` when sent to the background. A long frame (a debugger, a suspended laptop) is capped at
250 ms of game time, as today.

## Platform trait

The core sees only `Input`, `Storage`, the time it is given and, during a frame, `core::Platform`:

```rust
pub trait Platform {
    /// The window: its size in physical pixels and its scale, `None` without one.
    fn window(&self) -> Option<WindowGeom>;
    /// The pointer in the window's logical points and whether the window has the focus, once a frame after the keys
    /// are applied. `game_pointer`: the game draws its own pointer, so the platform may lock the system's.
    fn pointer(&mut self, keys: &Buttons<Key>, game_pointer: bool) -> (Option<(f32, f32)>, bool);
}
```

A shell for step 10 does six things around `Core::advance`:

| Operation | Bevy shell (`shell.rs`, step 9) | What a new shell writes |
|---|---|---|
| Input events | `take_system_input`: key, button, wheel and focus-lost events into `Core::input` between frames | the same calls: `Input::key`, `button`, `wheel`, `focus_lost`; `key_of` is the key map |
| Frame | `run_core`: `core.advance(Time<Real>.delta(), &mut platform)` | `advance(elapsed, &mut platform)` once a frame |
| Window and pointer | `Platform` over the primary window; the mouse lock (`display::lock_wanted`) inside `pointer` | `impl Platform` |
| Show | the frame into one texture, one sprite at the whole-number scale (`display::Letterbox`); `cursor_visible`, `title` after the frame | upload `Core::frame` (RGBA, 640 × 480) and scale it |
| Sound | a `Decodable` that never ends pulls `Output::mix` 256 sample frames at a time | the device callback calls `Output::mix` (22,050 Hz stereo) |
| Files, exit | `storage::Desktop`; `Core::exit` becomes `AppExit`; `store_settings` on a normal exit | the same `Desktop`; end the loop on `exit` |

A scripted run needs one more call after each frame: `script::record(run, frame, output, exiting)`, which also says
when the run is done; with a window, `script::compare_shot` takes the window's own picture of each shot.

## Safety net (phase 1, done)

| Part | What it does | Where |
|---|---|---|
| Compositor | Places pictures on the screen: nearest texel at pixel centres, transparent texels skipped, grey tint in linear light. No engine types. | `crates/client/src/compose.rs` |
| Scripted scenes | Feed the scene's input into the core's `Input` (`input.rs`, held in `Core::input`), with a fixed frame time (1/60 s), a fixed wall clock and no sound device. `--script NAME --script-out DIR [--headless]`. | `script.rs` |
| Frame snapshot | Hashes every frame (runs of equal frames on one line), every shot and every sound start, without a window. Skips without `TIDEWATER_INSTALL`; `TIDEWATER_SNAPSHOT_BLESS=1` blesses. | `tests/frames.rs`, `tests/snapshots/campaign.txt` |
| Bevy comparison | Ignored test: runs the scenes in a window at one pixel per screen pixel and compares Bevy's screenshot with the composed frame at each shot. | `tests/frames.rs` (`-- --ignored`) |
| Recorded frame | The frame the screen composed (`Core::frame`, recorded by `script::record`), for the snapshot and the comparison. Until step 3 a world composer recomposed what Bevy's cameras and sprites would show (`frame.rs`, now gone); both gave the same frames. | `shell.rs` |

The `campaign` scene, 1,372 frames: title fade and title, main menu fade and hover, campaign choice, act screen (text
and picture), briefing (two shots), the first mission's fade-in, drag selection, a right-click command, movement, key
and edge scrolling, a minimap click, about 300 cycles of play, the game menu, options and back, the save dialog with a
wheel turn and a typed name, saving.

### Comparison results

| Stage | Shots equal | Differences and cause |
|---|---|---|
| Layered sprites (before) | 26 of 28 | The two fade shots: 1 step in about 20% of channel values (title) and 6% (menu). Bevy tints on the GPU, whose sRGB conversion differs from the exact curve near black. |
| One composed texture (now) | 28 of 28 | None. Fades are now applied on the CPU, exactly. |

The composed frames were also checked equal to the layered composition at all 1,372 frames before the switch, and
the snapshot didn't change.

## Trial shells (phase 3)

**Bottom line.** Pick SDL 3. Both trial shells show exactly the Bevy shell's pixels and sound. SDL 3 halves Bevy's
memory and CPU, cuts the sound latency from about 50 ms to about 12 ms, and needs no workaround anywhere. miniquad
is a little lighter still and builds with no C toolchain, but it can't do six things the game uses, and it has no
Vita or console-style Linux handheld support.

### Layout

| Part | Where | Why |
|---|---|---|
| The core | `crates/client`, now a library (`lib.rs`) | Moving the core's 30-odd files into a new crate would touch every file and collide with work in progress on them; a library target in the same crate moves nothing. |
| Bevy shell | `crates/client`: `shell.rs` and the `tidewater` binary, behind the default `bevy` feature | `cargo build`, `cargo test` and the frame snapshot run as before. |
| Shared start-up | `crates/client/src/host.rs` (no engine types) | Start-up from the command line (`host::launch`, which was `main`), the headless scripted loop, the mouse lock's pointer bookkeeping (`LockedPointer`), frame statistics. Every shell calls it. |
| Trial shells | `crates/shell-sdl3` (`tidewater-sdl3`), `crates/shell-miniquad` (`tidewater-miniquad`) | Each depends on the core with `default-features = false`, so neither builds Bevy. |
| Default build | the workspace's `default-members` leave out `crates/shell-*` | A plain `cargo build` needs neither CMake nor SDL. SDL is built from source and linked in (`sdl3-sys`, `build-from-source`, `link-static`), so even the SDL shell needs no SDL install. |

The shells take the `tidewater` binary's arguments, including `--script` and `--headless`.

### Equivalence

| Check | SDL 3 | miniquad |
|---|---|---|
| Headless, six scenes: frame and audio fingerprints against `tests/snapshots` | equal | equal |
| Windowed, six scenes (campaign with 28 shots, outcome, ending, load, build, flow): the window's own pixels against the composed frame (`script::compare_shot`) | 0 differing pixels | 0 differing pixels |
| Windowed: frame and audio fingerprints | equal | equal |

The window's pixels are read back before each shot is shown: SDL's `SDL_RenderReadPixels`, and OpenGL's
`glReadPixels` on miniquad's back buffer.

### Measurements

On an M2 Pro MacBook Pro, release builds. The idle run is the first human mission, `--ui-scale 1 --no-mouse-lock`,
for 60 s; the display runs at 120 Hz, so every shell draws 120 frames a second. Another agent's builds shared the
machine, so the build times are rough.

| Measure | Bevy | SDL 3 | miniquad |
|---|---|---|---|
| Release binary, unstripped | 138.1 MB | 5.3 MB | 3.8 MB |
| Release binary, stripped | 85.0 MB | 4.4 MB | 3.0 MB |
| Clean release build, one binary | 207 s | 42 s | 15 s |
| Release rebuild after touching `core.rs` | 9 s | 4 s | 4 s |
| `target/` after the clean build | 1.9 GB | 235 MB | 209 MB |
| RSS at 30 s / 60 s | 314 / 279 MB | 146 / 147 MB | 141 / 144 MB |
| CPU (`ps` %cpu at 30 s / 60 s) | 86 / 86% | 40 / 37% | 33 / 30% |
| CPU time over the run's last 50 s | 43.7 s (87%) | 18.9 s (38%) | 16.5 s (33%) |
| Frame interval, p50 / p99 | 8.33 / 8.9 ms | 8.33 / 8.9 ms | 8.33 / 10.3 ms |
| Work per frame, mean / p99 (see note) | 1.6 / 1.9 ms | 2.5 / 3.7 ms | 2.4 / 3.3 ms |
| Sound: device buffer | rodio's default | 512 frames at 44.1 kHz (11.6 ms) | 512 frames at 44.1 kHz (11.6 ms) |
| Sound: callback period | 46 ms (step 7) | 11.6 ms measured (5,170 calls, longest gap 11.8 ms) | 11.6 ms (cpal's reported buffer; not timed) |
| Sound: queued beyond the device | up to 256 mixer frames (11.6 ms) | none: SDL's stream asks for exactly what the device needs | up to 256 mixer frames (11.6 ms) |
| Sound: latency behind the mixer | about 46 to 58 ms | about 12 to 23 ms | about 12 to 23 ms |
| Shell code (lines) | 633 (`shell.rs`) | 866 (`main.rs` 346, `sdl.rs` wrapper 520) | 771 (`main.rs` 570, `sound.rs` 201) |
| Shared code every shell uses | `host.rs`, 294 lines | the same | the same |
| Build needs | Rust only | Rust, CMake, a C compiler | Rust only (on Linux, ALSA and X11 or Wayland headers) |
| Runtime needs on macOS | system frameworks | system frameworks (SDL linked in) | system frameworks |

How each number was taken:

- **Sizes:** `strip` on a copy of each binary; `target/` is `du -sh` of a fresh `CARGO_TARGET_DIR` holding one
  binary's release build.
- **Builds:** `cargo build --release -p <crate>` in an empty target directory, then again after `touch
  crates/client/src/core.rs`.
- **RSS and CPU:** `ps -o rss=,%cpu=,time=`. CPU time is the `time=` difference between 10 s and 60 s.
- **Frame times:** `TIDEWATER_FRAME_STATS=1` prints them every 10 s (`host::FrameStats`); the table gives the
  steady windows after the first. The interval is from one frame's start to the next.
- **Work per frame** is the shell's own loop before the frame is shown. In SDL 3 and miniquad that covers the events,
  `Core::advance`, the texture upload and the draw. In Bevy it covers only `Core::advance` and the copy into Bevy's
  image; Bevy uploads and draws on its render thread, which is why its CPU is higher although this number is lower.
  Compare the CPU rows across shells, not this one.
- **Sound:** the SDL shell counts its callbacks and prints them on exit. The Bevy figure is step 7's measurement.
  The latency is the device buffer plus what the shell holds back.

### Gaps per shell

| Shell | Gap | Effect |
|---|---|---|
| SDL 3 | None found. Focus, title, monitor size, minimum size, always-on-top for scripted runs, pointer warp at the lock's end, and relative mode on macOS all work (the grab used on other systems is untested). | — |
| SDL 3 | Gamepads aren't built in (`no-default-subsystems`). | One `sdl-gamepad` feature when the core's pad input lands. |
| SDL 3 | Window screenshots taken from outside the program came back blank. | Seen with all three shells, so it is the capture, not the shell; the read-back comparison is unaffected. |
| miniquad | No focus events. | The window always counts as focused; a minimised window releases the keys (`focus_lost`), but switching to another app doesn't, and the palette keeps cycling. |
| miniquad | No title change after start. | The window keeps its first title. |
| miniquad | No monitor size. | Without `--ui-scale` the window opens at 640 × 480 points, not sized to the monitor. |
| miniquad | No minimum window size, no always-on-top. | The window can be made smaller than one screen; scripted runs can be covered. |
| miniquad | No pointer warp. | When the lock ends, the system pointer reappears where it was locked, not where the game's pointer is. |
| miniquad | One wheel number for mouse steps and touchpad pixels. | Taken as tenths of a step; touchpad scrolling feels different from the Bevy shell's. |
| miniquad | No sound. `quad-snd` only plays whole loaded sounds, with no callback to stream a mixer, so the shell uses `cpal` and resamples to the device's rate itself (`sound.rs`, linear). | 200 more lines; the resampling is ours. |
| miniquad | No gamepads. | Would need `gilrs` beside it. |
| Bevy | None (the reference). Cost: size, build time, memory, CPU, and 46 ms sound callbacks. | — |

### Port targets

| Target | Bevy | SDL 3 | miniquad |
|---|---|---|---|
| iOS | Supported (wgpu on Metal) | Supported (UIKit, Metal renderer, Core Audio) | Supported (OpenGL ES or Metal); `cpal` has Core Audio |
| Android | Supported, heavy | Supported; needs SDL's Java glue (`SDLActivity`) in a Gradle project | Supported with its own simple packaging; `cpal` has AAudio |
| Web | Supported (wgpu, WebGL2) | Through Emscripten (`wasm32-unknown-emscripten`); the audio callback and the core's threads (video sound, mixer lock) need Emscripten's threads | Its strongest target: `wasm32-unknown-unknown` with its own small loader; sound through `cpal`'s Web Audio backend. The core's threads need work on any shell |
| PS Vita | No | SDL 3 has a Vita backend; Rust has a tier 3 Vita target with `std` | No |
| Linux handhelds (KMSDRM, no desktop; Steam Deck) | Needs Vulkan or GL on a desktop session | The usual choice there: KMSDRM, GLES, gamepad mappings | Needs X11 or Wayland; no KMSDRM |
| 3DS (out of scope, revisit) | No | SDL 3 has a 3DS backend; the screen and memory are the hard part, not the shell | No |

### Recommendation

**SDL 3**, for four reasons:

1. **Nothing is missing.** Every behaviour the Bevy shell has works, with no workaround; miniquad lacks six.
2. **Light enough.** Memory and CPU are close to miniquad's (147 against 144 MB, 38 against 33%) and under half of Bevy's; the binary is 4.4 MB.
3. **Sound.** The callback asks for exactly what the device needs: about 12 ms behind the mixer, against Bevy's 46 to
   58 ms, with no resampler of our own.
4. **Reach.** It is the only one of the three that covers the Vita and console-style Linux handhelds, and it has
   gamepads built in.

The costs: CMake and a C compiler at build time (SDL's C source adds about 25 s to a clean build), and 520
lines of `unsafe` wrapper to review, all in `sdl.rs`. miniquad would win if the web were the main target; the core's
threads have to be dealt with for the web on any shell.

Next steps if you agree, about a day with Claude (two by hand):

1. Make `tidewater-sdl3` the `tidewater` binary and run the frame snapshot's windowed comparison through it.
2. Remove the miniquad shell, then the Bevy shell and the `bevy` feature.
3. Turn on `sdl-gamepad` when the core's pad input lands.

## Migration plan

| Step | Moves | From → to | Snapshot |
|---|---|---|---|
| 1 ✓ | Frame composition | Bevy cameras and sprites → `compose` + `shell` | unchanged |
| 2 ✓ | Map view geometry and the pointer | Map, interface and flow cameras → a `MapView` (centre, zoom, view rectangle) and `display::Letterbox` | unchanged |
| 3 ✓ | The layers | Sprites and image assets → pictures the core owns (`Screen`: map, interface, flow layers) | unchanged |
| 4 ✓ | Input | `ButtonInput`, `KeyboardInput`, `MouseWheel` readers in `main`, `frontend`, `gamemenu`, `chat`, `cheats` → `Input` | unchanged; the script feeds `Input` directly |
| 5 ✓ | Time and the frame loop | `Time<Real/Virtual/Fixed>`, `FixedUpdate` → the core's accumulator; systems → plain functions called in today's order | unchanged |
| 6 ✓ | States | `AppState`, `OnEnter/OnExit`, `DespawnOnExit` → the flow's own state machine | unchanged |
| 7 ✓ | Audio | `AudioPlayer`, sinks, the video sound thread → the mixer | sound starts and mixed sound unchanged; loop points exact (below) |
| 8 ✓ | Storage | `std::fs` and paths → `Storage`; `Install` over `Read + Seek` | unchanged, saved games and screenshots byte for byte |
| 9 ✓ | Bevy shell thin | What's left of `main.rs` → `core.rs` (`Core::advance`), `game.rs`, `args.rs`; Bevy only in `shell.rs` (window, letterbox, input events, mouse lock, sound device) | unchanged (after the pointer re-bless, below) |
| 10 ✓ | Trial shells (phase 3) | The core a library; SDL 3 and miniquad shells beside the Bevy one, measured ([Trial shells](#trial-shells-phase-3)) | unchanged; both shells match it, headless and windowed |

Each step lands with the snapshot unchanged, or blessed with the reason in the commit.

### Time and system order (step 5)

**Time.** The core's clock (`clock.rs`) takes each frame's real time from the shell and keeps Bevy's rules: the frame's
game time is capped at 250 ms, whole steps of it run the game's cycles (in `Core::advance`, before the frame's
`Update` systems, as `FixedUpdate` did), the step is 64 a second until a game sets its own, and the input logs age only
after a frame that ran a step, as Bevy's message buffers did. No system reads Bevy's `Time` any more; the shell feeds
the clock from `Time<Real>` (`shell::run_core`).

**One order.** Since step 9 the order is the code's: `Core::advance` calls the frame's steps and
`core::frame_systems` calls the systems one after another, with the points where their deferred changes
(`core::Commands`) take effect written in it (after `play_sounds` and between the game menu's systems, where Bevy had
put them). Until step 9 every schedule ran on one thread in its graph's order (`shell::one_order`). The order is the
one Bevy's graph gave, with the two changes the snapshot showed the old multithreaded runs had:

| Pair | Shares | Order and why |
|---|---|---|
| `play_music`, the flow's screens | the jukebox, the install | Music first. A screen's music cue starts the track on the next frame, as before; the graph's own order started the title music a frame early (campaign scene, frame 19 instead of 20). |
| `stop_music`, `start_game` (entering a game) | the jukebox | Stop first. The other order stopped the game's first track (load scene, frames 2, 498 and 621). |
| `watch_outcome`, the game's input and drawing | the game, the screen's pictures, the jukebox | Outcome first; its dialog takes effect at the command point after `play_sounds`, so the dialog's first frame is drawn as before. A command point before the game's input (a first attempt) opened the dialog a frame early. |
| `apply_fades`, the flow's screens | the fade clock | Fades first: a fade a screen starts shows from the next frame. |
| The flow's screens, each other | the flow, the install, the screen's pictures | Chained; each runs only in its own state, so at most one runs in a frame. `run_menu_load` before `run_menu` (the main menu with or without its load dialog). |
| `play_disco`, `cheats::run` and the game | the game, the cheat queue, the jukebox, the speaker | Disco before the game's layout, cheats after the view's scroll and before the panel input, as the graph had them. |
| `size_window`, `fit_screen` and the screens that read the window | the window | After the flow's screens and the view's scroll, before the game's input, as the graph had them. |
| `print_screen`, `hud_layout`, `cycle_palette` | the interface, the palette | After the layout, before the palette turns. |
| `demo_save_load`, `service_saving` (the whole world) and everything | everything | In the chain: the first after the flow's screens, the second near the end of the game menu's systems. |
| `mouse_lock`, `chat_keys` | the input | Lock first: it reads the held keys before the chat line takes the typing keys. |
| `drive` (scripted runs), Bevy's mouse motion | mouse motion | The script before Bevy's input systems. |
| `store_settings`, `record` (scripted runs) | the world | Settings first, then the screen is composed and recorded. |
| The clock and input systems, Bevy's message update | the world | After the message update. |

Bevy's ambiguity check (`TIDEWATER_ORDER_CHECK=1`, built with `--features bevy/debug`; gone in step 9, when no
schedule of ours was left to check), over the five scenes: 434 pairs before step 5, 65 after, none of them between two of our systems. The 65 are a system of ours against Bevy's
own, and none of them shares data that matters:

| Schedule | Pairs | Why the order doesn't matter |
|---|---|---|
| `Update` | 18: `demo_save_load`, `service_saving` and our command points against Bevy's render preparation (MSAA, parallax, tilemaps, screenshots, UI dialogs) and its Ctrl-C handler | Those use none of our data; they only see the whole world because ours are exclusive. |
| `Update` | 12: our systems that end the program against the Ctrl-C handler | Both only add an exit request. |
| `Last` | 19: `record` against Bevy's gizmos, hierarchy checks, frame count, task pools | `record` reads only the composed frame and the sounds. |
| `StateTransition` | 9 | Bevy's own state systems among themselves. |
| `RunFixedMainLoop` | 5: `run_cycles` against the gizmo contexts | We draw no gizmos. |
| `PreUpdate` | 2: `mouse_lock` against IME and UI focus | We use neither. |

### The flow's states (step 6)

The flow's state machine is `states.rs` (no engine types): the screen that shows, the change asked for (the last ask in
a frame wins; asking for the screen that shows leaves and enters it again), and the hooks each screen runs when it is
left and entered, as data (`states::on_exit`, `states::on_enter`). A change takes effect where Bevy's did, between the
frame's input and its game cycles, at most one a frame, in Bevy's order:

1. The state becomes the new one; the leaving hooks already see it (`camera_clear` reads it).
2. The leaving screen's hooks. Leaving a game: `camera_clear`, `close_dialog`, the game menu's close, `end_game`.
3. The leaving screen's layers and sounds (`frontend::OwnedBy`, in place of `DespawnOnExit`) go, the last made
   first, as Bevy's did.
4. The entering screen's hooks. Entering a game: `stop_music`, `camera_clear`, `start_game`, `setup`.

Each hook is a plain function (`core::run_hook`), and its deferred changes take effect as it ends. Bevy applied an
`OnExit` or `OnEnter` schedule's commands at its end instead; no hook reads what an earlier hook of the same change
commands, so the two are the same.

### Sound (step 7)

**The mixer** (`audio.rs`, no engine types) decodes PCM WAV files (8 or 16 bits, mono or stereo, any rate, resampled
once to 22,050 Hz by linear interpolation) and mixes its voices in whole numbers: a 16.16 gain each, the sum clipped
to 16 bits. A voice belongs to a group and stops with it: the music's track, a game's effects (stopped by
`end_game`), a screen's own sounds (stopped when the flow leaves the screen) and the act screen's sound (also stopped
by its fade of all sound). The mixer sits behind a lock, so the shell's audio thread can pull it.

| Part | How |
|---|---|
| Effect levels and modes | A sound effect's level is its gain (`level / 255`); the flow's sounds play at full gain, once. |
| Music fades | `music.rs` is unchanged: a fade stop is 28 steps over 540 ms, the first at once; the jukebox gives the voice the new gain on the frame the step falls in, as it gave the sink. |
| Loop points | The mixer loops a looping track from its loop point the sample after the last one, on its own clock; the jukebox's seek is gone. |
| Video sound | The decoding thread is kept; its WAV goes to the mixer, starting at the picture's position. A scripted run waits for it on the video's first frame, as before. |
| Device output (Bevy shell) | A Bevy audio source that never ends (`shell::CoreStream`, a `Decodable`) pulls 256 sample frames (12 ms) at a time; Bevy's rodio output resamples to the device's rate. Measured: rodio's device callbacks come every 46 ms, so the sound is about 46 to 58 ms behind the mixer. No cpal code of our own was needed. |
| Scripted runs | No device: the recorder mixes each frame's share after the frame (367 or 368 sample frames at 60 frames a second, the fractions carried), so a sound started in a frame starts at that frame's first sample. |

**The audio snapshot** (`tests/snapshots/{scene}-audio.txt`) holds a fingerprint per second of the mixed sound; it
was blessed with the mixer fed alongside Bevy, before Bevy's sound was removed, and the switch left it, the sound
lines and the frames unchanged in all six scenes. The sound lines now come from the mixer's starts, ordered within a
frame as Bevy's query listed its sound entities (by kind, in the order each kind first started, then by start).

### What can't stay identical

| What | Why | Plan |
|---|---|---|
| Fade colours | Already changed in step 1: the GPU's tint differed by 1 step in some values; the CPU's is exact. | Accepted. |
| Pointer positions | The pointer's map and screen positions were reached by a 2D camera's float steps (`camera_point`), kept without a camera so the frames stayed as they were. The exact form (`display::Letterbox::screen_of`, then `mapview::MapView::pointer_map`) lands one map pixel off at some points. | Done in step 9: the exact form is the only one, and the snapshot was re-blessed once for it, in its own commit ("the pointer's exact positions"). The only change is a drag rectangle's edge in the campaign scene, one pixel, in frames 642 and 643; the flow's screen pointer gave the same frames both ways. |
| An indexed frame | The interface's own colours and grey fades aren't palette entries. | Keep RGBA until a spec read of `docs/specs/ui/*` (`game-menu.md`, `palette-effects.md`) gives the original's palette colours. |
| The pointer outside a game | The flow's screens, the game menu and the outcome dialog showed the system pointer; the core now draws the normal arrow there (`shell`), a visible change. | Done: the frame snapshot was re-blessed once for it, in its own commit ("the core draws the pointer on every screen"). |
| Keys pressed in the same frame | Bevy's `ButtonInput` lists the keys that went down in a frame in hash order; `input::Buttons` lists them in the order they were pressed. The two differ only when two keys go down in the same frame, and only a system that takes the first of them could tell. | Accepted: press order is the defined one. |
| Music timing | Before step 7 the jukebox followed the sink's play position, which ran ahead of what was heard by the device's buffer, and a looping track played on from the file's start until the next frame saw the wrap and seeked to the loop point. | Done: the mixer's position is the one the jukebox reads, and a loop jumps to its loop point on the exact sample. A loop point (only track 20, HWARROOM, has one past the start) no longer plays up to a frame plus the device latency (about 17 to 60 ms) of the file's start first. No scene reaches a loop, so neither snapshot shows it. |
| Mixed sound | Rodio resampled and mixed in floating point; the mixer resamples once by linear interpolation and mixes in whole numbers. The sound out of the speakers differs by rounding. | Accepted; the audio snapshot hashes the mixer's samples. |
| Music in scripted runs | A scripted run had no sound device, so the jukebox saw no position: an end fade or a chained track never came. Now it sees the mixer's. | Accepted: no scene plays a track near its end, so the sound lines didn't change. |
| Game clock | The game clock (saved in Enhanced saves) read the system's monotonic clock; the core may read only the time the shell gives it. | Done before step 8: it reads the core's clock (`FrameClock::elapsed`), at most a frame behind the system's. A scripted run's saves now come out the same every time, so the snapshot can hash them. |
| Screenshots | Print Screen captured the window through Bevy, and the flash started when the capture came back, a frame or two later; a run without a window wrote nothing. | Done before step 8: the picture is the composed frame of the frame the key was pressed in (the same pixels), and the flash starts in the next frame. |
| `--zoom` above 1 in a big window | The map is now composed at screen resolution, so zooming out at 3× no longer shows the extra detail. | Development aid only; accepted. |
| A hook or system that couldn't run (step 9) | Bevy logged a hook system that failed to run, and skipped a system whose resource was missing. The plain functions return early on a missing part (`let Core { sim: Some(..), .. } = c else { return }`) and log nothing. | Accepted: no scene logged either, and the frames show no difference. |
| Development output (step 9) | The demo save and load (`TIDEWATER_DEMO_SAVE`, `TIDEWATER_DEMO_LOAD`) logged through Bevy's `info!` and `error!`; a scripted run's pictures (`TIDEWATER_SCRIPT_PICTURES`, the windowed comparison's) were PNG files written by Bevy's image crate. Now `eprintln!` and the `png` crate. | Accepted: the same pixels, the PNG files' bytes may differ; the snapshot hashes neither. |
| Window changes within a frame (step 9) | `size_window`, `fit_screen`, the title and the system pointer's visibility were set by systems mid-frame; an exit request was written mid-frame. The shell now applies them right after `Core::advance`. | Accepted: the window system sees window changes at the frame's end either way; the settings and a scripted run's record see an exit in the same frame as before. |

### Risks

| Risk | Mitigation |
|---|---|
| Hidden system-order dependencies. Bevy runs unordered systems in any order; one was a real bug (an Escape could close the game menu and quit). | Done in step 5: one thread, one order, every pair listed ("Time and system order"). Since step 9 the order is plain calls, so there is nothing left to be ambiguous. |
| The snapshot covers one scene | Add scenes before the steps that touch them: score screen, outcome dialogs, load dialog, chat, cheats, endings, credits, videos (needs the core's video sound). |
| A full 640 × 480 RGBA composition per frame on handhelds | Measured in phase 3 on the desktop: the whole frame, upload and draw take 2.4 to 2.6 ms (p99 under 4 ms) at 120 frames a second. A handheld's slower CPU may need changed-layer composition or an indexed frame; measure on the device. |
| Memory: Bevy's client is about 450 MB | Measured in phase 3 in a mission: Bevy 280 to 315 MB, the core in SDL 3 or miniquad about 145 MB. What that is made of is not yet measured; check it against the handhelds' memory. |

### Estimates

Claude does the engineering; Nick reviews and decides.

| Steps | Nick with Claude | By hand |
|---|---|---|
| 2–3 (geometry, layers) | 1 day | 4–5 days |
| 4–6 (input, time, states) | 2–3 days | 2 weeks |
| 7–8 (audio, storage) | 1–2 days | 1 week |
| 9 (thin Bevy shell) | half a day | 2 days |
| 10 (trial shells, measured) | 1 day | 4–5 days |
| SDL 3 as the only shell (proposed) | 1 day | 2 days |

Claude doesn't speed up the reviews, the decisions above, or checking the trial shells on real devices.
