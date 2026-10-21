# Tidewater documentation

**Start with the [project README](../README.md)** to build and run Tidewater. This page indexes the design documents and the behaviour specifications behind the code.

## Design documents

Design documents explain how the code is organised and why. They live in [`design/`](design/).

| Document | What it covers |
|---|---|
| [Game-file import](design/import.md) | How `--import` finds, checks and copies your *Warcraft II* files into Tidewater's data directory, the exit codes, and what later phases (CD images, the GOG installer) add. |
| [An engine-neutral client core](design/client-core.md) | How the client is a plain-Rust core that a shell calls once per frame, the SDL 3 shell, the frame and audio snapshot tests, and the record of the move from Bevy to SDL 3. |

## Specifications

Specifications describe **what** the original game does, never how its code does it. They are the only source for engine behaviour, and each one is reviewed against the [clean-room rules](../CLEANROOM.md) before it is added. They live in [`specs/`](specs/), one folder per area.

| Area | What it specifies |
|---|---|
| [`specs/core`](specs/core/) | Timing and random numbers, unit slots and creation order, unit animation scripts, corpses and wrecks. |
| [`specs/game`](specs/game/) | The life of a game, setup from a map, victory and defeat, rescue, cheat codes, campaign flow (briefings and end screens), saving and loading. |
| [`specs/orders`](specs/orders/) | The order list, commands, pending orders and target acquisition; transports (boarding, landing, unloading). |
| [`specs/movement`](specs/movement/) | Tile steps and arrival, path search, occupancy and passability, placing a unit on a free tile. |
| [`specs/combat`](specs/combat/) | Damage, armour, attack timing and missiles; attack-ground and demolish. |
| [`specs/spells`](specs/spells/) | Casting, effects, durations and summoned units. |
| [`specs/economy`](specs/economy/) | Harvesting, construction, training, research and food, the tech tree, repair, walls. |
| [`specs/vision`](specs/vision/) | Fog of war, sight and detection. |
| [`specs/ai`](specs/ai/) | The computer player: its model, scripts, workers, training and research, army and attacks, spell casting. |
| [`specs/ui`](specs/ui/) | Screen layout and pointers, the map view, minimap, selecting and commanding, command buttons, the information panel, building placement, messages, the game menu, sounds, music, palette effects. |
| [`specs/oracle`](specs/oracle/) | The per-cycle trace format used to compare the simulation with recordings of the original game (`twtool oracle`). |

## Related files

| File | What it is |
|---|---|
| [`CLEANROOM.md`](../CLEANROOM.md) | The rules that keep the code our own work, and the record that shows it. |
| [`CLAUDE.md`](../CLAUDE.md) | Instructions for AI agents working in the repository, including the crate layout. |
| [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) | Licences of the third-party code linked into the programs. |
