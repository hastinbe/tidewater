# Design: an engine-neutral client core

**Bottom line.** The client becomes a plain-Rust core that the host calls once per frame:
`advance(real_time_elapsed, input) -> Frame`. The shell (Bevy today; SDL3 or miniquad after the phase 3 trial) only
shows a 640 × 480 frame, polls input, pulls mixed audio, reads and writes files, and knows the time. Phase 1 is done:
a frame snapshot guards every step, and Bevy already shows one composed 640 × 480 picture instead of stacking
sprites. Phase 2 moves the loop, input, audio and storage behind the platform trait: about 5 to 7 days of Claude's
work, of which Nick's part is a few hours of playtests and reviews (4 to 5 weeks by hand).

Backlog item B-039. Nothing here changes game behaviour; where a step can't stay pixel-identical, the table in
[What can't stay identical](#what-cant-stay-identical) says why.

## What we need from you

All four questions are decided (2026-10-14):

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
| What still needs Bevy? | **Most glue.** About 6,900 lines in 8 modules (state, input, audio, time, files); see [Migration plan](#migration-plan). |

## Core API

The host drives the loop. The core never blocks, never reads the clock and never touches a device.

```rust
pub struct Core { /* flow, game, interface, mixer, storage */ }

impl Core {
    pub fn new(storage: Box<dyn Storage>, settings: Settings) -> Result<Core>;
    /// Runs the game for `elapsed` real time with this frame's input; returns the screen to show.
    pub fn advance(&mut self, elapsed: Duration, input: &Input) -> &Frame;
    /// Fills `out` with mixed sound (interleaved stereo at `Core::SAMPLE_RATE`). Called by the audio callback.
    pub fn mix(&self, out: &mut [i16]);
    pub fn suspend(&mut self);
    pub fn resume(&mut self);
    /// True once the player has quit.
    pub fn finished(&self) -> bool;
}

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

| Part | Today | In the core |
|---|---|---|
| Sound effects | One Bevy `AudioPlayer` per start, volume `level / 255` | The mixer: per start a voice with its level; overlapping sounds all play, as now |
| Music | Bevy sink; the jukebox reads the sink's play position | The mixer's own position, so the jukebox runs on the core's clock |
| Video sound | A decoding thread feeds Bevy | Decoded in the core with the video frames |
| Output | Bevy (rodio) | `Core::mix`, pulled by the shell's audio callback, 22,050 Hz stereo |

The snapshot hashes sound *starts* today (frame, a hash of the sound's bytes, level, mode). Once the mixer is in the
core it can also hash the mixed samples.

## Storage

```rust
pub trait Storage {
    /// The installation's files, opened for reading with seeking (the MPQ reader seeks).
    fn install(&mut self, name: &str) -> io::Result<Box<dyn ReadSeek>>;
    fn saves(&mut self) -> io::Result<Vec<SaveEntry>>;            // name, size, time
    fn read_save(&mut self, name: &str) -> io::Result<Vec<u8>>;
    fn write_save(&mut self, name: &str, bytes: &[u8]) -> io::Result<()>;
    fn delete_save(&mut self, name: &str) -> io::Result<()>;
    fn read_settings(&mut self) -> io::Result<Option<Vec<u8>>>;
    fn write_settings(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn write_screenshot(&mut self, name: &str, bytes: &[u8]) -> io::Result<()>;
    fn wall_clock_secs(&self) -> u64;                             // for seeds and save times
}
```

Today these are `std::fs` calls in `saves.rs`, `prefs.rs`, `screenshot.rs` and `tidewater_formats::install`. The
installation needs one change in `crates/formats`: `Install` and the MPQ reader take a `Read + Seek` source instead of
a path. The web shell can then read a folder the user picks; iOS and Android read the app's documents.

## Lifecycle

| Event | Core does |
|---|---|
| `suspend()` | Pauses the game (as the pause key), stops the mixer, writes the settings. |
| `resume()` | Restarts the mixer. The game stays paused until the player resumes it. |
| Focus lost | `Input::focused = false`: the palette stops cycling (`palette::Gate`), as now. |

Mobile shells call `suspend` when sent to the background. A long frame (a debugger, a suspended laptop) is capped at
250 ms of game time, as today.

## Platform trait

The five operations, as the shell implements them. The core sees only `Input`, `Storage` and the time it is given.

| Operation | Bevy shell (now) | Shape |
|---|---|---|
| Show a frame | `shell.rs`: the frame into one texture, one sprite at the whole-number scale | `fn present(&mut self, frame: &Frame)` |
| Poll input | Bevy input messages, the mouse lock | `fn poll(&mut self) -> Input` |
| Pull audio | Bevy audio | the audio callback calls `Core::mix` |
| Files | `std::fs` | `impl Storage` |
| Time | `Time<Real>` | `fn now(&self) -> Duration` |

## Safety net (phase 1, done)

| Part | What it does | Where |
|---|---|---|
| Compositor | Places pictures on the screen: nearest texel at pixel centres, transparent texels skipped, grey tint in linear light. No engine types. | `crates/client/src/compose.rs` |
| Scripted scenes | Feed the scene's input into the core's `Input` (`input.rs`, held in `shell::PlayerInput`), with a fixed frame time (1/60 s), a fixed wall clock and no sound device. `--script NAME --script-out DIR [--headless]`. | `script.rs` |
| Frame snapshot | Hashes every frame (runs of equal frames on one line), every shot and every sound start, without a window. Skips without `TIDEWATER_INSTALL`; `TIDEWATER_SNAPSHOT_BLESS=1` blesses. | `tests/frames.rs`, `tests/snapshots/campaign.txt` |
| Bevy comparison | Ignored test: runs the scenes in a window at one pixel per screen pixel and compares Bevy's screenshot with the composed frame at each shot. | `tests/frames.rs` (`-- --ignored`) |
| Recorded frame | The frame the screen composed (`shell::shown_frame`), for the snapshot and the comparison. Until step 3 a world composer recomposed what Bevy's cameras and sprites would show (`frame.rs`, now gone); both gave the same frames. | `shell.rs` |

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

## Migration plan

| Step | Moves | From → to | Snapshot |
|---|---|---|---|
| 1 ✓ | Frame composition | Bevy cameras and sprites → `compose` + `shell` | unchanged |
| 2 ✓ | Map view geometry and the pointer | Map, interface and flow cameras → a `MapView` (centre, zoom, view rectangle) and `display::Letterbox` | unchanged |
| 3 ✓ | The layers | Sprites and image assets → pictures the core owns (`Screen`: map, interface, flow layers) | unchanged |
| 4 ✓ | Input | `ButtonInput`, `KeyboardInput`, `MouseWheel` readers in `main`, `frontend`, `gamemenu`, `chat`, `cheats` → `Input` | unchanged; the script feeds `Input` directly |
| 5 ✓ | Time and the frame loop | `Time<Real/Virtual/Fixed>`, `FixedUpdate` → the core's accumulator; systems → plain functions called in today's order | unchanged |
| 6 ✓ | States | `AppState`, `OnEnter/OnExit`, `DespawnOnExit` → the flow's own state machine | unchanged |
| 7 | Audio | `AudioPlayer`, sinks, the video sound thread → the mixer | sound starts unchanged; music timing changes (below) |
| 8 | Storage | `std::fs` and paths → `Storage`; `Install` over `Read + Seek` | unchanged |
| 9 | Bevy shell thin | What's left of `main.rs` → `bevy_shell` (window, letterbox, input, audio, files) | unchanged |
| 10 | Trial shells (phase 3) | SDL3 and miniquad shells, measured | must pass the snapshot |

Each step lands with the snapshot unchanged, or blessed with the reason in the commit.

### Time and system order (step 5)

**Time.** The core's clock (`clock.rs`) takes each frame's real time from the shell and keeps Bevy's rules: the frame's
game time is capped at 250 ms, whole steps of it run the game's cycles (`run_cycles` in `main.rs`, before the frame's
`Update` systems, as `FixedUpdate` did), the step is 64 a second until a game sets its own, and the input logs age only
after a frame that ran a step, as Bevy's message buffers did. No system reads Bevy's `Time` any more; the shell feeds
the clock from `Time<Real>` (`shell::advance_clock`).

**One order.** Every schedule runs on one thread, in the order its graph gives (`shell::one_order`). The frame's
`Update` systems are one chain (`frame_systems` in `main.rs`), and the points where their commands take effect are
written in it (after `play_sounds` and between the game menu's systems, where Bevy had put them). The order is the one
Bevy's graph gave, with the two changes the snapshot showed the old multithreaded runs had:

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

Bevy's ambiguity check (`TIDEWATER_ORDER_CHECK=1`, built with `--features bevy/debug`), over the five scenes: 434
pairs before this step, 65 after, none of them between two of our systems. The 65 are a system of ours against Bevy's
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

The Bevy shell runs each hook as a system registered once at startup (`main.rs`, `register_hooks`), so a hook's
commands take effect as it ends. Bevy applied an `OnExit` or `OnEnter` schedule's commands at its end instead; no hook
reads what an earlier hook of the same change commands, so the two are the same. The hooks are registered before
the first frame, so the screens' entities keep their order.

### What can't stay identical

| What | Why | Plan |
|---|---|---|
| Fade colours | Already changed in step 1: the GPU's tint differed by 1 step in some values; the CPU's is exact. | Accepted. |
| Pointer positions | The pointer's map and screen positions are still reached by a 2D camera's float steps (`main.rs`, `camera_point`), now without a camera. The exact form (`mapview::MapView::pointer_map`) lands one map pixel off at some points: a drag rectangle's edge in the campaign scene moved by a pixel in frames 642 and 643. | Switch to the exact form when the shell is thinned (step 9), and bless the snapshot then, saying why. |
| An indexed frame | The interface's own colours and grey fades aren't palette entries. | Keep RGBA until a spec read of `docs/specs/ui/*` (`game-menu.md`, `palette-effects.md`) gives the original's palette colours. |
| The pointer outside a game | The flow's screens, the game menu and the outcome dialog showed the system pointer; the core now draws the normal arrow there (`shell`), a visible change. | Done: the frame snapshot was re-blessed once for it, in its own commit ("the core draws the pointer on every screen"). |
| Keys pressed in the same frame | Bevy's `ButtonInput` lists the keys that went down in a frame in hash order; `input::Buttons` lists them in the order they were pressed. The two differ only when two keys go down in the same frame, and only a system that takes the first of them could tell. | Accepted: press order is the defined one. |
| Music timing | The jukebox follows the device's play position today, which depends on the sound device's latency. | The mixer's position is exact; loop points move by the old latency. |
| Mixed sound | Rodio resamples and mixes its own way. | Compare sound starts, not samples. |
| `--zoom` above 1 in a big window | The map is now composed at screen resolution, so zooming out at 3× no longer shows the extra detail. | Development aid only; accepted. |

### Risks

| Risk | Mitigation |
|---|---|
| Hidden system-order dependencies. Bevy runs unordered systems in any order; one was a real bug (an Escape could close the game menu and quit). | Done in step 5: one thread, one order, every pair listed ("Time and system order"). Run the ambiguity check again after each step. |
| The snapshot covers one scene | Add scenes before the steps that touch them: score screen, outcome dialogs, load dialog, chat, cheats, endings, credits, videos (needs the core's video sound). |
| A full 640 × 480 RGBA composition per frame on handhelds | Measure in phase 3; compose only changed layers, or go indexed. |
| Memory: Bevy's client is about 450 MB | The core alone should be far smaller; measure in phase 3. |

### Estimates

Claude does the engineering; Nick reviews and decides.

| Steps | Nick with Claude | By hand |
|---|---|---|
| 2–3 (geometry, layers) | 1 day | 4–5 days |
| 4–6 (input, time, states) | 2–3 days | 2 weeks |
| 7–8 (audio, storage) | 1–2 days | 1 week |
| 9 (thin Bevy shell) | half a day | 2 days |

Claude doesn't speed up the reviews, the decisions above, or checking the trial shells on real devices.
