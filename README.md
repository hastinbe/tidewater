# Tidewater

**Tidewater is a from-scratch Rust reimplementation of the *Warcraft II: Battle.net Edition* engine, including *Beyond the Dark Portal*, that runs on your own copy of the game.** It is playable in single player today: both campaigns, with menus, saved games, sound, music and video. Early LAN multiplayer works from the main menu and from the command line (see [Multiplayer (early)](#multiplayer-early)); a skirmish screen and touch controls are planned but not built yet (see the [roadmap](#roadmap)).

The game rules live in an engine-neutral core that an [SDL 3](https://libsdl.org) window shows. Tidewater contains no game data: it reads the files of your own copy at run time.

**Status: early but playable.** You can start a campaign from the main menu, play missions against the computer, win or lose, see briefings, score screens, endings and credits, and save and load. The main menu is small: New Campaign, Load Game, Show Credits and Quit. A custom game on any map works from the command line only. Multiplayer is early: Local Network and Direct IP games over TCP.

## What you need

- **Your own copy of the game.** Tidewater needs *Warcraft II: Battle.net Edition* (for example the GOG release). No game data is included. The 1995 DOS edition is not supported.
- **A desktop computer.** Release builds are checked in CI on macOS, Linux and Windows. Day-to-day play testing has been on macOS.
- **A release download, or a build toolchain.** Prebuilt downloads are on the [releases page](../../releases/latest) (see [Installing](#installing)); otherwise build from source (next section).

## Quick start

1. **Install the build tools.**
   - The Rust toolchain pinned in [`rust-toolchain.toml`](rust-toolchain.toml) (`rustup` picks it up by itself).
   - CMake and a C compiler. SDL 3 is built from source and linked into the program, so you do not install SDL itself.
   - macOS: `xcode-select --install` and `brew install cmake`.
   - Debian or Ubuntu (the packages CI installs): `sudo apt-get install build-essential cmake pkg-config libx11-dev libxext-dev libxrandr-dev libxcursor-dev libxi-dev libxfixes-dev libxss-dev libxtst-dev libxkbcommon-dev libwayland-dev wayland-protocols libdecor-0-dev libasound2-dev libpulse-dev libudev-dev libdbus-1-dev libegl1-mesa-dev libgl1-mesa-dev`
   - Windows: Visual Studio's C++ build tools and CMake.
2. **Run it.** With no game files yet, Tidewater opens an import screen:

   ```sh
   cargo run --release -p tidewater-client
   ```

   The screen lists the installs it found in the usual places (a GOG folder, `/Applications`, Wine and CrossOver prefixes). Pick one, or choose a game folder with the buttons, or drop a folder on the window. Tidewater checks the files, copies the archives (`War2Dat.mpq`, `Install.mpq`, `War2Patch.mpq`) and the `Maps` folder into its own per-user data directory once, shows what it imported and goes on into the game. If something is wrong, the screen says what and what to do next. Esc quits.

   **Without a window.** To import from the command line instead, point `--import` at where the game is installed; it prints a summary and exits. A second import replaces the first.

   ```sh
   cargo run --release -p tidewater-client -- --import <path-to-game>
   ```
3. **Play.** Once the files are imported, the same command starts the game:

   ```sh
   cargo run --release -p tidewater-client
   ```

   Add a map to start a game on it at once: `cargo run --release -p tidewater-client -- <map.pud>`.

**What counts as `<path-to-game>`.** A game folder (a GOG folder, a Wine prefix, a `.app` bundle or a mounted CD all work; Tidewater looks a few levels down), a folder of `.mpq` files, or one `.mpq` file. The **GOG installer** (`setup_warcraft_ii_*.exe`, with its `.bin` file in the same folder) works directly, without installing the game: choose the `.exe` (or the folder holding it). This needs the free [`innoextract`](https://constexpr.org/innoextract/) program (`brew install innoextract` on macOS, your package manager on Linux, a download on Windows); Tidewater tells you if it can't find it. **CD images** (`.iso`, BIN/CUE (`.bin` with its `.cue`, choose either), `.img`, `.mdf`, `.nrg`) are read directly, with no mounting. This includes the Battle.net Edition CD, whose game is inside its `INSTALL.EXE`: Tidewater reads it from there. A disc image gives the game as first sold, without the later patches. If a disc only holds some other installer, Tidewater says so: install the game from the disc and choose the installed folder. `twtool disc-list <image>` shows what is on an image.

**Where the copy goes.** The per-user data directory: `~/Library/Application Support/Tidewater` on macOS, under `~/.local/share` on Linux, under `%LOCALAPPDATA%\Tidewater` on Windows. Imported files go in `game`, saved games in `Save`, settings in `settings.txt`. `--data-dir <dir>` moves all of it. Tidewater never writes into your game installation.

**Developer override.** `--install <path-to-game>` (or the `TIDEWATER_INSTALL` environment variable) reads the files in place and skips the import. How the import works: [docs/design/import.md](docs/design/import.md).

**Check a path without copying.** `cargo run --release -p twtool -- import-check <path-to-game>` runs the same checks and copies nothing.

## Installing

Download the file for your system from the [latest release](../../releases/latest). None of them contains game data; the first start opens the import screen (see [Quick start](#quick-start), step 2).

| System | File | How |
|---|---|---|
| **Windows** (x64) | `tidewater-<version>-windows-x64-setup.exe` | Run it. It installs for your user only, so it needs no administrator, and adds a Start menu entry and an uninstaller. |
| **macOS** (Apple silicon and Intel) | `tidewater-<version>-macos.dmg` | Drag Tidewater to Applications. The app is not signed, so the first time **right-click it, choose Open and confirm**; after that it opens normally. |
| **Linux** (x64) | `tidewater-<version>-linux-x64.tar.gz` | Unpack it and run `tidewater`. |

## Updating

On the desktop, Tidewater looks for a newer release in the background when it starts (one request to GitHub, nothing sent about you). If there is one, the main menu shows **Update now** and **Later**. Update now installs it on Windows (runs the installer silently and restarts the game) and macOS (replaces the app and restarts it); on Linux, or when the app's folder is not writable, it opens the release page. A failed check is ignored without a message. Development builds never check.

Turn the check off with `update_check=0` in `settings.txt` or `--no-update-check` for one run. Cutting a release: [docs/release.md](docs/release.md).

## Features today

| Area | What works |
|---|---|
| **Campaigns** | All 52 campaign missions: *Tides of Darkness* and *Beyond the Dark Portal*, orc and human. Briefings, act screens, victory and defeat dialogs, score screens, endings and credits. |
| **Game screen** | Map view with fog of war, minimap, resource bar, selection and unit information panel, command buttons and hotkeys, building placement, messages and the status bar. |
| **Game rules** | Movement and pathfinding, combat, spells, harvesting, construction, training and research, repair, transports, walls, rescue, victory and defeat, and computer players that build, train, attack and cast. |
| **Custom games** | Any `.pud` map from the command line, with the original custom-game choices through `--setup` (race, opponents, resources, terrain, starting units, fog), or the Custom Scenario screen (see below). |
| **Menus** | Main menu (Multi Player Game opens the multiplayer menus and lobby; Replay Introduction plays the intro video), single-player menu, new-campaign choice, and the in-game menu (F10) with save, load, options, help, sound, speed and preferences dialogs, objectives, restart and surrender. |
| **Saved games** | Save and load from the in-game menu and the victory dialog, into the `Save` folder of the data directory. |
| **Sound, music, video** | Unit and event sounds, the music jukebox, the intro and story videos (Smacker), and palette effects such as colour cycling and fades. |
| **Cheats** | The original cheat codes, typed into the chat line. |
| **Display** | A 640 x 480 picture drawn in software and shown at the largest whole-number scale that fits your monitor, with a black border. `--ui-scale N` picks a scale. |

### Playing a custom game

Choose **Single Player Game** on the main menu, then **Custom Scenario** (or start with `--goto custom`), to open the Custom Scenario screen. It offers the original's
choices: Your Race, Opponents, Resources, Map Tileset and Units, each a drop-down that you open with the pointer or step
with Tab, Enter and the arrow keys. **Select Scenario** (E) opens a dialog that lists the maps and narrows them by type,
number of players and map size; **Start Game** (S) plays the map you picked with Enhanced rules, and **Cancel Game** (C or
Esc) returns to the main menu. The single-player menu also holds New Campaign and Load Game. The list holds every `.pud` in the `Maps` folder of `<data>/game`, subfolders included, so
copy your own maps there. When the game ends, the score screen leads back to the main menu.

### Controls

| Input | Action |
|---|---|
| **Left click or drag** | Select. Shift adds or removes; Ctrl or a double-click takes every unit of that type on screen. |
| **Right click** | The right-click command (move, attack, harvest). With `--second-mouse-style` it centres the view instead. |
| **Command buttons** | Click one or press its hotkey. A command that needs a target takes the next left click on the map or minimap; a right click cancels. |
| **Control groups** | Ctrl+digit sets a group; the digit recalls it (twice quickly centres the view). |
| **Scrolling** | Screen edge, arrow keys, Home, End, Page Up, Page Down, keypad. Left click or drag on the minimap moves the view. |
| **Placing a building** | While you choose a site, the selected workers stand still; they move again when you place the building, or when you leave placement (right click, Escape, another command). |
| **Speed and pause** | `+` and `-` change the game speed; Ctrl+P pauses. |
| **Menu** | F10 or Alt+M opens the game menu; Escape closes it. Tab moves between controls in dialogs and Enter presses a button. |

The pointer is locked to the window during a game, as in full screen, so pushing it against an edge scrolls the map. The game menus free it, and so does holding Cmd (the Windows key elsewhere) or switching to another app. `--no-mouse-lock` turns this off. The full key list is in the specs under [`docs/specs/ui`](docs/specs/ui/).

### Touch

Tidewater has an input layer for touchscreens, our own addition (the original has no touch): a tap is a left click, a
long press a right click, a drag a box select, two fingers pan the map, a pinch zooms it (up to four times) and a
three-finger tap opens the game menu. The controls a touchscreen lacks (Shift, Ctrl, Alt, right-click mode, pause and
the keyboard) are buttons in the bars beside the picture. You can try all of it on a desktop with `--touch-emulation`:
the mouse is one finger, and holding Alt as you press adds a second finger (Alt+Shift: beside the first, Alt+Ctrl: a
third). The iOS and Android apps (next sections) use it on a real touchscreen. How it works, and the measurements it uses, is
in [`docs/design/touch.md`](docs/design/touch.md).

### iPhone and iPad (early)

Tidewater builds as an iOS app that runs in the Simulator (a real device needs your own signing). It is the same program: touch is on from the start, the picture fits inside the safe area, and the game pauses when you leave the app. Details, signing and limits: [`docs/design/ios.md`](docs/design/ios.md).

**Build it.** You need Xcode, [xcodegen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`), CMake and rustup:

```sh
platform/ios/build.sh sim --app     # library, Xcode project and Tidewater.app for the Simulator
xcrun simctl install booted platform/ios/DerivedData/Build/Products/Debug-iphonesimulator/Tidewater.app
xcrun simctl launch booted dev.tidewater.client
```

For a device, run `platform/ios/build.sh device`, copy `platform/ios/Config/Signing.example.xcconfig` to `Signing.xcconfig`, fill in your team ID and open `platform/ios/Tidewater.xcodeproj`.

**Get the game files onto it.** Copy a game folder (a GOG install, or the folder holding `War2Dat.mpq`) or a CD image (`.iso`, `.bin`/`.cue`, `.mdf`, `.nrg`) into Tidewater's folder: in Finder (device, Files tab), in the Files app (On My iPhone, Tidewater), or for the Simulator into the app's `Documents` folder. Open the app and the first-run screen lists what it finds; tap one, or tap Scan again after copying. "Choose game folder..." opens the system picker instead. **A GOG installer (`.exe`) does not work on iOS**, because unpacking it needs `innoextract`, which cannot run there: unpack it on a computer, or install the game and copy the folder.

### Android (early)

Tidewater builds as an Android app that runs in the emulator (it is not yet tried on a phone). It is the same program as the desktop and iOS builds: touch is on from the start, the screen stays in landscape, and the game pauses when you leave the app. Details and limits: [`docs/design/android.md`](docs/design/android.md).

**Build it.** You need rustup, [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) (`cargo install cargo-ndk`), CMake, a JDK (17 to 24) and the Android SDK with an NDK and `platforms;android-35`:

```sh
platform/android/build.sh          # libmain.so for arm64-v8a and x86_64, then a debug APK
adb install -r platform/android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n dev.tidewater.client/.TidewaterActivity
```

`--abi arm64-v8a` builds one ABI; `--libs-only` stops before Gradle. The APK is signed with Gradle's debug key.

**Get the game files onto it.** Either copy a game folder (a GOG install, or the folder holding `War2Dat.mpq`) or a CD image (`.iso`, `.bin`/`.cue`, `.mdf`, `.nrg`) into the app's folder with a computer, for example `adb push Warcraft2 /sdcard/Android/data/dev.tidewater.client/files/` (open the app once first so the folder exists), then tap Scan again on the first-run screen; or tap "Choose game folder..." or "Choose CD image..." to pick from the phone's storage, which Tidewater copies into its cache first. **A GOG installer (`.exe`) does not work on Android**, because unpacking it needs `innoextract`: unpack it on a computer, or install the game and copy the folder.

### How faithful it is

Tidewater follows the behaviour of the original game, not only its looks. The **Enhanced** rules are the default and are what Tidewater mirrors. The **Classic** rules exist in the original only as an opt-in multiplayer choice; Tidewater has them only where the specs describe them.

Fidelity is measured, not assumed. The simulation is deterministic and integer-only. `twtool oracle` compares it cycle by cycle with recorded traces of the original game and reports the first record that differs. Many traces (campaign missions, spells, naval and transport scenarios, computer players) match up to their stated limits. The traces are not in this repository, so this comparison runs on the maintainers' machines, not in CI.

Some screens are our own layout, because the original's dialog definition files are not specified yet: the menus, dialogs, briefing and score screen look different from the original.

## Optional improvements

Tidewater adds a few optional improvements that the original game does not have. **They are all off by default**: with
none switched on, the game looks, sounds and plays as the original does. Turn one on in the `[extras]` section of the
settings file (`settings.txt`), or for one run with its flag; `--no-extra-NAME` puts an option back to its default.
The idle-worker key is F6. The keys can be rebound in a `[keys]` section of the same file (for example
`pause=ctrl+u`, `button-move=j`, `slot-1=q`); with no section they are the original's. How they work, and how to add one, is in [`docs/design/extras.md`](docs/design/extras.md).

| Option | Flag | Values | Default | What it does |
|---|---|---|---|---|
| `health-bars` | `--extra-health-bars VALUE` | off, hovered, selected, always | off | Hit point bars over units: only those under the pointer, only selected ones, or all (C+L) |
| `idle-worker` | `--extra-idle-worker` | on, off | off | An idle-worker button at the top of the screen and the F6 key: select your next idle worker |
| `game-clock` | `--extra-game-clock VALUE` | off, clock, clock-apm | off | The game clock over the map view, with or without your actions per minute |
| `group-bar` | `--extra-group-bar` | on, off | off | A strip over the map view's top left listing your control groups with their sizes: click one to recall it |
| `smart-attack-move` | `--extra-smart-attack-move` | on, off | off | A right-click on open ground with only fighting units selected attack-moves there instead of moving (C+L) |
| `volume-voices` | `--extra-volume-voices VALUE` | 0 to 100 | 100 | Unit voices, in percent of the sound volume (100 leaves them as the game sets them) |
| `volume-effects` | `--extra-volume-effects VALUE` | 0 to 100 | 100 | Sound effects, in percent of the sound volume |
| `volume-music` | `--extra-volume-music VALUE` | 0 to 100 | 100 | Music, in percent of the music volume |
| `scaling` | `--extra-scaling VALUE` | integer, sharp | integer | How the picture fits the window: only whole-number scales, or any size drawn sharp bilinear (touch platforms always use sharp) |
| `rally-points` | `--extra-rally-points` | on, off | off | Right-click with a training building selected to set a rally point: the units it trains go there (S) |
| `production-queue` | `--extra-production-queue` | on, off | off | Buildings queue up to five jobs, paid when queued; Cancel removes the last (S) |
| `selection-limit` | `--extra-selection-limit VALUE` | 9, 12, 18, unlimited | 9 | How many units you can select and command at once: nine as in the original, 12, 18 or no limit (S) |
| `speed-levels` | `--extra-speed-levels VALUE` | off, 1, 2, 3 | off | Game speeds above Fastest, 1 to 3 more levels (Enhanced rules only) (S) |
| `minimap-ping` | `--extra-minimap-ping` | on, off | off | Alt-click on the minimap pings that place on your allies' minimaps, and shows their pings on yours |
| `shift-queue` | `--extra-shift-queue` | on, off | off | Hold Shift while giving a move, attack, attack-move, patrol, repair, harvest or build order to add it to the unit's order queue (up to 8) instead of replacing its order (S) |
| `auto-replay` | `--extra-auto-replay VALUE` | off, menu, always | off | Record every game: `menu` keeps the recording so the game menu can save it, `always` also writes each game's replay to the replays folder of the data directory as it is played |
| `widescreen` | `--extra-widescreen VALUE` | off, 16:10, 16:9, auto | off | A wider game screen, the extra width being map: 16:10, 16:9, or auto to match the window (ignored on touch devices; menus and videos stay 4:3) (C+L) |

**C+L** marks an option that gives you an edge, so in a game with other players the lobby will set it for everyone
instead of your own settings. **S** marks an option that changes the simulation (rally points, the production queue, shift-queued orders,
the selection limit and the extra speed levels): the flag or the setting starts a single-player game with it, and in a
network game the host turns it on in the lobby's "Extras" box and everyone plays with the host's choice. The game is
saved and recorded with it. Every option, S included, is **off by default**.

## Command-line reference

### `tidewater` (the game)

Run `tidewater --help` for the same list. The optional positional argument is a `.pud` file path or a map name inside the archives.

**Everyday options**

| Flag | What it does |
|---|---|
| `--import <PATH>` | Import the game files from a game folder, a GOG installer, a folder of `.mpq` files or one archive, then exit. The exit code says what went wrong: 10 path not found, 11 DOS edition, 12 installer (not readable, or `innoextract` missing), 13 no game files, 14 no `War2Dat.mpq`, 15 damaged archive, 16 incomplete files, 17 no space, 18 i/o error, 19 CD image. |
| `--data-dir <DIR>` | Tidewater's data directory (imported files, `Save`, settings). Default: the per-user data directory. |
| `--install <PATH>` | Developer override: read the game files in place instead of the imported copy. Also `TIDEWATER_INSTALL`. |
| `<MAP>` | Start this map at once (a `.pud` path or an archive name). |
| `--setup <SETUP>` | Game setup: `campaign:ENTRY`, or `custom` with choices such as `custom:race=orc,opponents=2,resources=high,terrain=swamp,units=one-worker,fixed-starts,fog=off`. |
| `--no-intro` | Skip the intro video. |
| `--ui-scale <1-8>` | Open a 640N x 480N-point window instead of the largest whole-number scale that fits. |
| `--save-dir <DIR>` | Where saved games go (default: `Save` in the data directory). |
| `--settings <FILE>` | The settings file (default: `settings.txt` beside the saved games). |
| `--no-mouse-lock` | Do not lock the pointer to the window during a game. |
| `--second-mouse-style` | Right click centres the view instead of giving a command. |
| `--mouse-scroll-speed <0-6>` | Mouse scroll speed; 6 is fastest. Default: the stored setting. |
| `--key-scroll-speed <0-6>` | Keyboard scroll speed; 6 is fastest. Default: the stored setting. |

**Development aids** (not part of the original game)

| Flag | What it does |
|---|---|
| `--touch-emulation` | The mouse is one finger (Alt, Alt+Shift, Alt+Ctrl at the press add more), and the touch controls show beside the picture. See [Touch](#touch). |
| `--touch-dpi <PPI>` | The screen's density in physical pixels per inch, for touch sizes (taps, slop, buttons). Default: 140 points to the inch. |
| `--free-zoom` | The mouse wheel zooms the map view (1/4 to 8 map pixels per screen pixel). |
| `--zoom <N>` | Starting zoom; implies `--free-zoom`. |
| `--goto <SCREEN>` | Open a flow screen directly: `title`, `menu`, `campaign`, `single` (the single-player menu), `custom`, `mission:E`, `act:E`, `briefing:E`, `score-victory:E`, `score-defeat:E`, `ending:E`, `credits:tod`, `credits:bdp`, `show-credits` or `video:N`, with E a mission entry (0 to 51). |
| `--demo-outcome <victory\|defeat>` | End the game at once, to see the dialog and score screen. |
| `--demo-move <X,Y>` | Order all your units to a tile at the start. |
| `--demo-fight` | Send every player's units against the nearest enemy within 16 tiles. |
| `--demo-select <N>`, `--demo-type <N>`, `--demo-place <N>`, `--demo-site <X,Y>` | Preselect your first N units, narrow that to one unit type, start in building placement, and click a site. |
| `--record-replay` | Record the game to a `.twrep` replay in the `replays` folder of your user data (see [Replays](#replays)). Off unless given. |
| `--replay <FILE>` | Play a recorded game back, as the recording player saw it. Cannot be combined with `--map`, `--record-replay` or the setup options. |
| `--script <NAME>`, `--script-out <DIR>`, `--headless` | Run a scripted scene for the frame snapshot test. `--script` and `--script-out` go together; `--headless` uses no window or GPU. |

### `twtool` (inspection and verification)

`twtool` reads game data from a path or `--install` given at run time and stores none. Run `twtool <command> --help` for each command's arguments.

| Subcommand | What it does |
|---|---|
| `import-check` | Dry run of the import: classify a path, check the files, print the report, copy nothing. |
| `disc-list` | List the files on a CD image (size and path), to see how a disc is laid out. |
| `mpq-info` | Summarise an archive's block table. |
| `mpq-probe` | Print which of the names on stdin an archive contains. |
| `mpq-extract` | Extract the named files into a directory. |
| `mpq-unnamed` | Dump in-use blocks whose names are not listed. |
| `render-map` | Render a map's terrain to a PNG. |
| `render-grp` | Render every frame of a GRP sprite sheet into one PNG. |
| `grp-sheet` | Draw frame 0 of each GRP named on stdin into one grid. |
| `render-pcx` | Render a PCX image to a PNG. |
| `smk-frame` | Decode a Smacker video up to a frame and write it to a PNG. |
| `smk-audio` | Decode one audio track of a Smacker video to a WAV file. |
| `units` | Print the unit stats table as TSV. |
| `upgrades` | Print the upgrade table as TSV (a map's own, or the default). |
| `strings` | Print a string table (`*.tbl`) as TSV. |
| `scripts` | Measure each unit type's move and attack timing from the animation scripts. |
| `script-ops` | List one unit type's animation script instruction by instruction. |
| `fell` | Fell trees in a field of one forest shape and print the resulting forest table rows. |
| `sim` | Run a map headless: give orders, step some ticks, print where units ended up and the state hash. |
| `snapshot` | Print fingerprints of the simulation over every campaign and custom map; the behaviour snapshot `tests/snapshot.rs` checks. |
| `replay` | Play a `.twrep` replay headlessly, check every state-hash checkpoint and exit non-zero at the first mismatch. |
| `desync-diff` | Play two replays (or one twice with `--local SLOT`) and print the first cycle whose state hash differs and the first differing field. |
| `oracle` | Compare the simulation cycle by cycle with a trace of the original game and report the first record that differs. |

## Multiplayer (early)

Two to eight machines can play one game over TCP. Every machine runs the whole simulation and they exchange only commands, so all copies must be the same build with the same game data; the handshake refuses a mismatch and says which part differs (protocol, build, data tables, game edition or game files).

**From the menus:** Multi Player Game, choose Enhanced or Classic, enter a name, then **Local Network** (games on your network are listed; Create Game opens one) or **Direct IP** (type the host's address, or Create Game). Battle.net is shown greyed out. The game setup lobby has the scenario, up to eight rows with races and chat boxes, and Start Game once two or more people are in. A joiner without the host's map is sent it automatically. In the game, Enter opens the chat line; start a line with `/a ` to send it to allies only. `/diplomacy allies=0,2 vision=0,2 victory=on` sets your alliances, shared vision and allied victory (a stand-in until the diplomacy screen). In Enhanced games each player has three pauses and everyone sees who paused and how many are left.

**From the command line** (no menus needed):

```sh
tidewater 'Maps\Cramped BNE.pud' --host --lobby humans=2,races=human/orc,seed=1   # host, waits for joiners
tidewater --join HOST-ADDRESS                                                       # each joiner
```

The default port is 6112: TCP for the game, which the host opens on its firewall (joiners need nothing open), and UDP for the Local Network list, where a game's creator broadcasts a small packet once a second (`TIDEWATER_NO_BEACON=1` turns that off; Direct IP works where broadcasts do not). Write the map before `--host`; `--host PORT` picks another port and `--join ADDR:PORT` follows it. Enhanced rules are the default; Classic is opt-in (`--setup custom:classic`). Leaving the game with Surrender or Quit shows "has left the game"; Ctrl+F1 leaves, and Alt+F2 drops players the game is waiting for. Add `--record-replay` on each side, then `twtool desync-diff` on the two files to confirm both saw the same game. A host can leave places for watchers with `--lobby observers=N` (off by default); a watcher joins with `--join HOST-ADDRESS --observe`, sees the whole map, sends no commands and does not count for victory or defeat. Details: [docs/design/network.md](docs/design/network.md).

## Replays

Run with `--record-replay` and the game writes a `.twrep` file to the `replays` folder of your user data (never into the game folder). It holds the map's name and hash, the seed, the setup and the rules, then the commands given each cycle and a state-hash check every 100 cycles. Recording is a flag rather than always on, so nothing is written unless you ask. The `auto-replay` option does the asking for you: `--extra-auto-replay always` (or the `[extras]` setting) writes every game's replay, single-player or network, and `menu` holds the recording in memory and adds a Save Replay button to the in-game menu.

`tidewater --replay FILE` plays it back as the recording player saw it. Your input is ignored except the game menu, pause (Ctrl+P) and speed (`+` and `-`). If the game ever stops matching the recording, playback stops at the first check that fails and says which cycle. `twtool replay FILE` does the same check without a window, and `twtool desync-diff` finds the first cycle and field where two runs differ. A replay is refused on a map that is not the one it was recorded on. Games that were loaded from a save, or that used the `--demo-*` options, are not recorded. A network game records on each side with the same flag.

## Roadmap

Everything in this table is **planned**; none of it is built yet.

| Track | What it adds | Status |
|---|---|---|
| **Import from CD images** | Read `.iso`, BIN/CUE, `.mdf` and `.nrg` images directly, including the Battle.net Edition CD's installer archive. | Done |
| **Lockstep multiplayer** | Local Network and Direct IP games over TCP. The simulation is deterministic so that this is possible. | Early: menus, lobby, LAN game list, map transfer, chat and pause counts; no Battle.net, modem or skirmish screen; observers by command line only |
| **Skirmish against the computer** | A custom-game setup screen in the menus. The rules and computer players exist; the screen does not. | Planned |
| **Touch controls** | iOS (Simulator-tested; no device or signing yet) and Android (emulator-tested). PlayStation Vita support is under evaluation. | iOS and Android early (emulator and Simulator only) |
| **Optional modern improvements** | Quality-of-life additions, all off by default so the default game matches the original. | Planned |

## How it is built

Tidewater is a **clean-room** project. Behaviour comes only from reviewed written specifications in [`docs/specs`](docs/specs/), public format documentation, open-source code under compatible licences, and measurements of the original game's inputs and outputs. No code from the original program and no game data is in this repository. Read [CLEANROOM.md](CLEANROOM.md) for the rules, and [docs/README.md](docs/README.md) for the design documents and the spec index.

The code is a Cargo workspace:

| Crate | Job |
|---|---|
| `crates/formats` | Readers for the game's data: MPQ archives, maps, tilesets, sprites, images, fonts, Smacker video, string and unit tables. |
| `crates/import` | Game-file import (`--import`). |
| `crates/sim` | The deterministic game simulation, with no engine types. |
| `crates/client` | The game client: an engine-neutral core that composes each frame, and the `tidewater` binary, the SDL 3 shell. |
| `crates/twtool` | The inspection and verification tool. |

## Development

Run this before every commit:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
```

**Game-data tests.** Some tests need a real installation or recorded oracle traces. They are gated on `TIDEWATER_INSTALL` and `TIDEWATER_ORACLE_TRACES` and skip when those are unset. All other tests use synthetic data built by our own code; no game file is ever committed.

**CI.** [GitHub Actions](.github/workflows/ci.yml) runs format, clippy and the tests on Linux (the game-data tests skip there), then builds release binaries of `tidewater` and `twtool` on Linux, macOS and Windows.

**Leak scan.** Run `git config core.hooksPath .githooks` once per clone. The hooks run `tools/leak_scan.py`, which blocks private names, secrets, game-data file types and large files from being committed. Never bypass them.

Commits use conventional commit messages (`feat:`, `fix:`, `docs:`). Contributors and agents should read [CLAUDE.md](CLAUDE.md) and [CLEANROOM.md](CLEANROOM.md) first.

## Licence and notices

*Warcraft*, *Warcraft II*, *Beyond the Dark Portal*, *Battle.net* and *Blizzard Entertainment* are trademarks or registered trademarks of Blizzard Entertainment, Inc. Tidewater is an independent project, not affiliated with, endorsed or sponsored by Blizzard. It contains no Blizzard code or game data; to play, you need your own copy of the game.

License: [MPL-2.0](LICENSE). The MPQ decompressors are ported from StormLib ([MIT](licenses/StormLib-MIT.txt)). Licences of the statically included third-party code are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
