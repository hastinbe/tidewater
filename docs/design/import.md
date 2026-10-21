# Game-file import

Tidewater contains no game data. `crates/import` (no SDL, no `unsafe`) finds the user's *Warcraft II: Battle.net
Edition* files, checks them and copies the ones the client needs into Tidewater's data directory. This is phase I1:
the library and the command line, and phase I2, the first-run screen (below). Installer reading (I3) comes later.

## Decision: always copy

An import **copies** the files into `<data>/game/`; it never keeps a pointer to the user's install. A copy survives
the install being moved, uninstalled or on a CD that isn't in the drive, and gives one place the client reads from on
every platform. `--install DIR` stays as a developer override that reads in place.

## Pieces

| Module | Job |
|---|---|
| `source` | Classifies a path: game folder (searched 6 levels deep, any letter case, breadth first; covers a GOG folder, a Wine prefix's `drive_c`, a `.app` bundle and a mounted CD, which is just a folder), single `.mpq`, folder of loose `.mpq` files. Refuses a GOG `setup_*.exe`/`.bin` (`InstallerUnsupported`), a CD image (`.iso` with `CD001` at sector 16, `.bin`+`.cue` raw 2352-byte sectors, `.img`, `.mdf`, `.nrg`; `DiscImageUnsupported`) and a DOS-edition copy (`DATA/*.WAR`; `DosEdition`). |
| `validate` | Opens every archive; requires `War2Dat.mpq`; reads key files the client loads at start (`rez\unitdata.dat`, `Art\font10x.fnt`, `rez\network.tbl`, the Forest tileset). Missing `Install.mpq` or `Maps/` are warnings ("no videos or music", "no multiplayer map list"); `War2Patch.mpq` is optional and silent. Returns a `Report` with player-facing messages. |
| `copy` | Copies the allowlist (`*.mpq` at the top, `Maps/**`) into `<data>/.import-<pid>`, hashing as it goes, then swaps it in as `<data>/game`. The old import is moved aside first and restored if the swap fails; the temporary folder is always removed. |
| `manifest` | `game/manifest.json`: source kind, edition guess, each file's path, size and SHA-256, the import time (UTC). The known-hash table (`KNOWN_WAR2DAT`) is empty: no hashes collected yet. |

Only the allowlist is copied because the installation reader gives loose files priority over the archives, so stray
files must not come along.

## Command line

- `tidewater --import <path> [--data-dir <dir>]`: validate, copy, print the report and a summary, exit.
- `twtool import-check <path>`: the same checks and report, copies nothing.
- `tidewater` without `--install` uses `<data>/game` when it exists, else opens the first-run screen (a run without a
  window prints how to import and exits with 1).
- `--data-dir` moves the whole per-user directory (`game`, `Save`, the settings file); `--save-dir` and `--settings` still override their parts.

Exit codes: 0 success, 10 path not found, 11 DOS edition, 12 installer, 13 no game files, 14 no `War2Dat.mpq`, 15
damaged archive, 16 key file missing or unreadable, 17 not enough space, 18 other i/o error, 19 CD image. (2 is clap's,
for a bad command line.)

## First-run screen (I2)

With no `--install` and no imported copy, a run with a window opens the first-run screen (`client/src/firstrun.rs`,
core state `AppState::FirstRun`) instead of printing the import message. A scripted or headless run still prints the
message and exits. The screen is not part of the original; it exists only because the game data isn't there yet, so
every screen of the original stays as it was.

- **Look.** It draws into the usual 640 x 480 frame with its own 8 x 8 bitmap font (`firstrun/font.rs`, public domain,
  credited in `THIRD_PARTY_NOTICES.md`; no game font exists before the import) and plain rectangles.
- **Automatic search** (`firstrun/search.rs`, on a worker thread). It looks at the children of the usual folders whose
  names look like the game (`Warcraft`, `War2`) and runs each through `classify` and `validate`; the ones that pass are
  listed. Windows: `Program Files`, the GOG Galaxy and GOG.com folders, `GOG Games` on `C:` to `G:` (no registry:
  reading it would need `unsafe`). macOS: `/Applications`, `/Volumes`, `~/Applications`, `~/GOG Games`, `~/Games`, and
  `drive_c` of Wine, CrossOver and Whisky prefixes. Linux: `~/GOG Games`, `~/Games` and Wine, Lutris and Bottles
  prefixes.
- **Choosing.** "Choose game folder...", "Choose GOG installer..." and "Choose CD image..." open the system's folder or
  file dialog (`SDL_ShowOpenFolderDialog`, `SDL_ShowOpenFileDialog`; the `sdl-dialog` feature of `sdl3-sys`), and a
  file or folder dropped on the window (`SDL_EVENT_DROP_FILE`) does the same. Both come into the core as input events
  (`Input::picked`, `Input::dropped`) and go through the same import, so an installer or a CD image gets the import
  crate's "not supported yet" message until I3. The core asks for a dialog with `Core::take_pick_requests`; the shell
  shows it. Nothing else of SDL is known to the core.
- **Importing.** The copy runs on a worker thread through `Backend::import` (`DesktopBackend` calls `classify`,
  `validate` and `import`); the screen shows the callback's progress as a bar, then the result line ("Imported
  Warcraft II: Battle.net Edition (GOG). Videos and music: yes. Maps: 153.") for about a second and a half (Enter
  skips it), then loads the imported files and goes on into the normal start-up (or the map or `--goto` screen the
  command line asked for).
- **Errors.** The import crate's message is shown wrapped, with a Back button (Enter or Esc) to the list.
- **Input.** Mouse and keyboard: Up, Down and Tab (Shift+Tab backwards) move, Enter chooses, Esc goes back or quits;
  hovering focuses and a click chooses.
- **Tests.** The rules are tested through the core's input with a fake `Backend`; the scripted scene `firstrun` (a
  demo backend and a fake list of installs, so it needs no game data) has a frame snapshot in
  `tests/snapshots/firstrun.txt`.
- **Not done here.** Reading the GOG installer and CD images (I3, I3b); the sandboxed platforms' pickers (I4).

## Free space

There is no portable free-space query without `unsafe` or a new dependency, so there is no check before copying. A
full disk surfaces as a write error, which is reported as "Not enough free space" and cleans up the temporary
folder; an earlier import is untouched.

## Later phases

The copy step reads through the `SourceFiles` trait (`list()` relative paths and sizes, `open(rel)`), with a plain
folder (`FolderFiles`) as its first implementation. A later phase adds implementations for a CD image (ISO 9660 reader)
and a GOG installer (Inno Setup reader), which list and stream only the allowlisted files, and `copy::import_from`
takes them unchanged. Validation currently opens archives by path, so those sources will first extract the archives to
the temporary folder and validate them there before the swap; only `source::classify` and the error variants are in
place for them today.
