# Tidewater

A clean-room reimplementation of the *Warcraft II: Battle.net Edition* engine in Rust and [Bevy](https://bevyengine.org),
including *Beyond the Dark Portal*. It contains no game data: you need your own copy of the game (for example from
GOG), and Tidewater reads its files at run time.

Status: early. The data readers work; the client shows a map and you can select units and move them. Game rules
beyond movement are not implemented yet.

```sh
cargo run --release -p tidewater-client -- --install <game directory> [map.pud]
```

The window opens at the largest whole-number scale of the 640 × 480 screen that fits your monitor (for example
960 × 720 points on a Retina laptop); `--ui-scale N` asks for a 640N × 480N-point window instead. A larger or
resized window keeps a whole-number scale with a black border around the screen.

The arrow keys and the screen edge scroll the map, `+`/`-` change the game speed, Escape quits. The map view shows
the same area as the original; `--free-zoom` (a development aid, not part of the original) lets the mouse wheel zoom. See [CLEANROOM.md](CLEANROOM.md) for how the
project keeps its code its own.

*Warcraft* is a trademark of Blizzard Entertainment. This project is not affiliated with or endorsed by Blizzard.

License: [MPL-2.0](LICENSE). The MPQ decompressors are ported from StormLib ([MIT](licenses/StormLib-MIT.txt)).
