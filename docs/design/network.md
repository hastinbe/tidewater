# Network play: lockstep over TCP

**Status: early.** Two to eight Tidewater programs can play one game over TCP, started from the command line. Every
machine runs the whole simulation and exchanges only commands. The behaviour is `docs/specs/net/lockstep.md` and
`docs/specs/net/leave-and-drop.md`; the wire format is Tidewater's own, because the original's packets are out of scope
(`lockstep.md`, "Scope"). The lobby screens, pause counts and chat are the next phase (M4).

## Play over a LAN

The host starts the game and waits; each joiner connects to the host's address.

```sh
# Host (listens on port 6112 unless you name another):
tidewater 'Maps\Cramped BNE.pud' --host --lobby humans=2,races=human/orc,seed=1
# Joiner (the map, rules and options come from the host):
tidewater --join HOST-ADDRESS            # or HOST-ADDRESS:PORT
```

- **Write the map before `--host`.** A bare `--host` followed by a word reads the word as a port. `--host=7000` and
  `--host 7000` name a port.
- **Default port: 6112 (TCP).** Open it on the host's firewall; joiners connect out and need nothing open. Nothing uses
  UDP and there is no discovery, so a joiner needs the host's address.
- **The host sets the lobby**; joiners accept it by joining. `--setup custom:classic,fog=off` picks the rules and the
  game's options; `--lobby` picks the rest (below). The host prints the game it hosts and each joiner prints the game it
  was shown, with Classic rules marked "opt-in".
- **Enhanced rules are the default.** Classic rules are only ever chosen on purpose, with `classic` in `--setup`.
- **Record each side** with `--record-replay` (the file is `p1-NAME-TIME.twrep`, `p2-...` and so on, in the replay
  folder). `twtool desync-diff A B` on two sides' files finds nothing when the game stayed in step.
- **Both programs must be the same build with the same game data.** The handshake refuses a joiner that differs and
  says which of the protocol, the simulation build or the data tables (`unitdata.dat`, `upgrades.dat`, `script.bin`,
  `ai.bin`) differs. A joiner without the host's map (the same file contents) refuses before the game starts.

| `--lobby` choice | Meaning | Default |
|---|---|---|
| `humans=N` | Human players, 2 to 8. The host is slot 1. | 2 |
| `races=human/orc` | One race per human, in slot order, or `map` for the map's own. | the map's |
| `computers=N` | Computer rows after the humans. | 0 |
| `fixed` | Fixed Order: each human takes the start slot of its row. | off |
| `speed=0..8` | Game speed (8 is Fastest). | 8 |
| `row=0..15` | Network delay row, which sets the turn length (below). | 4 |
| `seed=N` | The game's seed. The host's choice, sent to every machine. | the host's clock |

## How it fits together

```
  game screen (game::tick)
        |  submit / ready_commands / checkpoint       crates/client/src/session.rs  (Session trait)
  NetSession (net/lockstep.rs)  -- turn engine: queue, turns, sync check, stalls, drops
        |  Wire trait: send / poll / forward          net/tcp.rs
  HostWire  <---- TCP, one connection per joiner ---->  ClientWire        net/proto.rs  (messages, framing)
```

- **Threads, not async.** Each connection has a reader and a writer thread feeding channels; the game thread never
  blocks on the network once a game runs. The handshake before the window opens does block, on purpose.
- **Star topology.** Joiners connect only to the host. The host stamps each packet with the slot it came from (a joiner
  cannot send as another player) and passes on what it accepts. The host is not a game server: it decides nothing about
  the game, every machine runs everything, and the only thing it alone decides is a drop (below).
- **The session is the only seam.** `NetSession` implements `Session`; `game::tick` gained a few calls (`observe_speed`,
  leave notices, the deadline reset) that other sessions leave at their defaults. A network game's replay is the merged,
  ordered commands the session handed out, so it replays on one machine through `ReplaySession`.

## The protocol

A frame is a `u32` little-endian length and that many bytes of `postcard`. At most 1 MiB. Variants are only added at
the end; `Hello` is first so a program of another protocol version can still read it far enough to refuse.

| Message | Direction | Carries |
|---|---|---|
| `Hello` | joiner to host | Identity (protocol, simulation build, data hash) and the name asked for |
| `Refuse` | either | Why the game cannot go on |
| `Welcome` | host to joiner | The joiner's slot and the whole game: map name and hash, seed, setup, lobby, speed, delay row, names |
| `Ready` | joiner to host | The game is acceptable here (the map is found, with the same hash) |
| `Start` | host to joiners | Every slot filled; play. Carries every slot's name |
| `Turn` | any | A player's packet: slot, sequence number, one optional command |
| `Sync` | any | The sender's state digest for a turn |
| `Drop` | host to joiners | Slot and the sequence number its packets stop at |
| `DropRequest` | joiner to host | Please drop this slot (Alt+F2) |

## The turn engine

- **Packets and turns** (`lockstep.md`, "Turns and the order of commands"). Every human has a queue of packets, one
  optional command each. A turn carries out one packet from every human, in slot order, each acting as the slot it came
  from; a remote command whose player is not its sender's slot is ignored. Turns 0 and 1 both run before cycle 0; after
  that a turn runs every K cycles. Surrender and Quit (draw) are never turn commands: they are local to the machine
  that made them.
- **Latency.** The queue is topped up to 2 at each turn start (limit 26, a full queue drops the command), so a command
  read at counter c runs in the turn before counter `K * ceil(c / K) + K`. The vectors in the spec (MP-L6 and the rest)
  are unit tests.
- **Turn length.** `K = ceil(T * 13 / L)` for Enhanced rules (L the speed's cycle length in ms) and `ceil(T * 60 / S)`
  for Classic. T comes from the delay row: row 4 gives K = 12 at speed 8, the traced Direct Connect game. The spec's
  gap-fill for Direct IP is row 3 (K = 8); `--lobby row=3` picks it. The default is 4 because that is the row the
  traces measured.
- **Game speed.** The world starts at Normal. The lobby's speed reaches every machine as `Command::Speed` commands in
  the first turn, so the recording replays at the same speed. Turn 1's length already uses the lobby's speed.
- **Stalls** (`lockstep.md`, "Stalls"). A turn waits until every human's packet for it has arrived. Over 2 s the cursor
  is the watch; over 7 s a "Waiting for players" notice names who. Ctrl+F1 leaves the game (the others see a drop);
  Alt+F2 drops the players still missing, on every machine at the same turn. A wait over 200 ms resets the cycle
  deadline, so the game does not run a burst of cycles to catch up. A shorter wait is caught up.
- **Sync check, every turn, wider than the original's.** At the start of turn n each machine sends the digest of its game
  state (`World::state_hash`: players, units, map, vision, the random generator). At turn n + 1 the digests of turn n
  are compared. A machine whose digest differs is dropped, in place of its packet, with "Player X was dropped.", on
  the turn the original drops on (the second turn after the divergence). Each machine drops the other; neither keeps a
  game the other cannot be shown to share. Replays still carry the usual checkpoint every 100 cycles.

## Leaving and dropping

- **Left.** Surrender or quit sends a `Leave` command in the leaver's own packet. Every machine carries it out in the
  turn and shows "Player X has left the game." (`network.tbl` entry 64). `World::leave_game` makes the slot nobody;
  Enhanced rules also clear the leaver from shared vision, Classic mode only empties the slot.
- **Dropped.** A drop is a `Leave` made up in the missing player's place, shown as "was dropped" (entry 65). It comes
  from the sync check, a lost connection, Ctrl+F1 or Alt+F2. The host numbers it into the dropped player's packet
  stream (`Drop` with the count of that slot's packets it accepted), so every machine carries it out in the same turn.
  The host only passes on packets it counted, which is what makes the count exact.
- **The dropped machine** is told, ends its game with a surrender and shows that it left.

## Spec gaps and choices

- **The host leaving.** The original's players each hold every connection and have no host to lose. A joiner that loses
  the host drops every other player where it stands and plays on alone. The other joiners may do the same at slightly
  different turns, so they can drop one another through the sync check. Host migration is not built.
- **Default delay row** is 4, not the spec's gap-fill of 3 (see above).
- **The "Waiting for players" dialog** is a line on the notice bar; the dialog is a lobby-phase screen.
- **"Eliminated" (entry 63)** shows from the victory check, not from the leave; the check still reads the local player
  in places (the open questions in `multiplayer-determinism.md`).
- **Speed changes and pause in a network game** go through the turns like any command, but the pause counts (the limit
  per player) and the "Pause" and "Resume" names are not built.
- **Chat, the lobby screens and the player list** are M4.
- **IPv6** works for `--join [::1]:PORT`, and the listener binds IPv4 only.

## Tests

- **Unit tests** (`net/proto.rs`, `net/tcp.rs`, `net/lockstep.rs`, `net/lobby.rs`) cover the message round trips, the
  handshake and its refusals, the turn-length tables, the latency vectors, stalls with an injected clock, drops,
  leaves and the sync check, on a stand-in world over loopback TCP.
- **`crates/client/tests/net.rs`** plays real games between two machines in one process over loopback TCP on the smallest
  installed map, with scripted commands from both sides: hashes at every turn, recordings compared with the same
  checks as `twtool desync-diff`, a forced divergence, a delayed peer, a joiner with a different map, and Classic
  rules. It needs `TIDEWATER_INSTALL` and does nothing without it.
