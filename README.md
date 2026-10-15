# Tidewater

A clean-room reimplementation of the *Warcraft II: Battle.net Edition* engine in Rust, including *Beyond the Dark
Portal*: an engine-neutral game core shown in an [SDL 3](https://libsdl.org) window. It contains no game data: you need
your own copy of the game (for example from GOG), and Tidewater reads its files at run time.

Status: early. The data readers work; the client shows a map and you can select units and move them. Game rules
beyond movement are not implemented yet.

```sh
cargo run --release -p tidewater-client -- --install <game directory> [map.pud]
```

Building needs Rust (the toolchain in `rust-toolchain.toml`), **CMake** and a **C compiler**: SDL 3 is built from its
source and linked into the program, so no SDL install is needed. On macOS, `xcode-select --install` and
`brew install cmake`; on Debian or Ubuntu, `apt install build-essential cmake` plus the X11, Wayland and ALSA or
PulseAudio development headers SDL's own build lists.

The window opens at the largest whole-number scale of the 640 × 480 screen that fits your monitor (for example
960 × 720 points on a Retina laptop); `--ui-scale N` asks for a 640N × 480N-point window instead. A larger or
resized window keeps a whole-number scale with a black border around the screen.

During a game the pointer is locked to the window, as it would be in full screen, so pushing it against an edge
scrolls the map; the game menus free it. Switching to another app (Cmd-Tab) or holding Cmd frees it, and clicking back in locks it again;
`--no-mouse-lock` turns this off.

The arrow keys and the screen edge scroll the map, `+`/`-` change the game speed, F10 opens the game menu (Escape closes it). The map view shows
the same area as the original; `--free-zoom` (a development aid, not part of the original) lets the mouse wheel zoom. See [CLEANROOM.md](CLEANROOM.md) for how the
project keeps its code its own.

*Warcraft* is a trademark of Blizzard Entertainment. This project is not affiliated with or endorsed by Blizzard.

License: [MPL-2.0](LICENSE). The MPQ decompressors are ported from StormLib ([MIT](licenses/StormLib-MIT.txt)).
