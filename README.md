# Tidewater

A clean-room reimplementation of the *Warcraft II: Battle.net Edition* engine in Rust and [Bevy](https://bevyengine.org),
including *Beyond the Dark Portal*. It contains no game data: you need your own copy of the game (for example from
GOG), and Tidewater reads its files at run time.

Status: early. The data readers work and the client shows a map with its terrain and units.

```sh
cargo run --release -p tidewater-client -- --install <game directory> <map.pud>
```

Arrow keys or WASD pan, mouse wheel or `+`/`-` zoom, Escape quits. See [CLEANROOM.md](CLEANROOM.md) for how the
project keeps its code its own.

*Warcraft* is a trademark of Blizzard Entertainment. This project is not affiliated with or endorsed by Blizzard.

License: [MPL-2.0](LICENSE). The MPQ decompressors are ported from StormLib ([MIT](licenses/StormLib-MIT.txt)).
