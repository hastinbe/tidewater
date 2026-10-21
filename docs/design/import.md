# Game-file import

Tidewater contains no game data. `crates/import` (no SDL, no `unsafe`) finds the user's *Warcraft II: Battle.net
Edition* files, checks them and copies the ones the client needs into Tidewater's data directory. This is phase I1:
the library and the command line. The first-run screen (I2) and installer reading (I3) come later.

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
- `tidewater` without `--install` uses `<data>/game` when it exists, else prints how to import and exits with 1.
- `--data-dir` moves the whole per-user directory (`game`, `Save`, the settings file); `--save-dir` and `--settings` still override their parts.

Exit codes: 0 success, 10 path not found, 11 DOS edition, 12 installer, 13 no game files, 14 no `War2Dat.mpq`, 15
damaged archive, 16 key file missing or unreadable, 17 not enough space, 18 other i/o error, 19 CD image. (2 is clap's,
for a bad command line.)

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
