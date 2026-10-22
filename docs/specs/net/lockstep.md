# Network: lockstep turns

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 89, B-057). The review's rulings are in
  `ERRATA-round89-multiplayer.md`, "Review".
- **Changes:** 2026-10-21, round 91 (oracle traces with multiplayer commands, `ERRATA-round91-traces.md`; reviewed by
  a spec-review session, 2026-10-21): command latency traced (MP-L6); the watch and the waiting dialog seen at about
  3 s and 8 s into a stall (MP-L4, first part); the default sync check catches a generator difference at the second
  turn after it and doesn't catch a "seen"-map difference (MP-L5, `allowsync` off). Tidewater's wider digest is
  recorded as a design choice that drops on the original's turn (DECISIONS.md, 2026-10-21).
- **Changes:** 2026-10-22, round 96 (oracle trace, `ERRATA-round96-traces.md`; reviewed by a spec-review session,
  2026-10-22): trace request MP-L5b's typing step answered: in an Enhanced game `allowsync` typed as a chat line is
  plain chat, carried out as a command in a turn, and a generator difference still drops the player (traced). The
  `allowsync` mode stays untraced.
- **Changes:** 2026-10-22, round 99 (`ERRATA-audit2-net.md`; reviewed by a spec-review session, 2026-10-22): the
  diplomacy command is specified in `game/diplomacy.md` (static study), and Tidewater's digest names the shared-vision
  sets and allied-victory settings explicitly (player state; a design choice, not the original's check).
- **Sources:** static study of the 2.02 program for both network layers (research notes under `research/notes/`), the
  dialog files `rez\timeout.bin`, `rez\btimeout.bin` and `NetDlg.bin`, and the tables `Rez\connect.tbl` and
  `Rez\network.tbl`. One oracle run confirms the turn spacing of an Enhanced Direct Connect game at speed 8
  (round 89, Dark Paths BNE, 300 cycles); round-91 runs of the same game with injected commands confirm the command
  latency, the stall dialog and the default sync check (named at each rule). Every section says which of its rules
  are traced, static study or gap-fill.
- **Scope:** what players observe and the order in which commands are carried out. Packet layouts are out of scope:
  Tidewater uses its own wire protocol. Leaving and dropping players is in `net/leave-and-drop.md`.

Table entries are numbered from 1: "network entry n" is entry n of `Rez\network.tbl`. A **cycle** is one game cycle
(`core/timing-and-rng.md`). A **turn** is one exchange of commands between all human players; every player carries out
the same commands in the same turn, in the same order, so all games stay identical.

Rules mode: Enhanced is the default and the target. Classic exists only in the opt-in multiplayer path. Every Classic
finding is labelled "Classic mode", and none changes an Enhanced rule.

## Behaviour

### Network layers by game type

The original has two turn layers. Which one a game uses depends only on the connection chosen.

| Game | Rules | Layer | Source |
| --- | --- | --- | --- |
| Single player (campaign, custom game) | Enhanced or Classic | The game's own layer, local only, row 0 | Static study |
| Direct Connect (serial cable) | Enhanced | The game's own layer | **Traced** (round 89) |
| Direct Connect | Classic mode | The game's own layer | Static study |
| Modem | Enhanced or Classic | The game's own layer | Static study |
| IPX network | Enhanced or Classic | The game's own layer | Static study |
| Battle.net | Enhanced only | The Battle.net turn layer | Static study |
| TCP/IP | none | Not offered: the Windows menu has no TCP entry. A `viewtcp.bin` dialog and an AppleTalk `viewatlk.bin` dialog exist as leftovers | Static study |

This spec calls them the **peer layer** (the game's own layer, used by everything except Battle.net) and the
**Battle.net layer**.

**The connection menu.** The Enhanced list shows, in order: Battle.net, Direct Connect, Modem, IPX network. The
Classic mode list shows Direct Connect, Modem, IPX network, with no Battle.net. The names come from `Rez\connect.tbl`.

**Tidewater slots.** Tidewater keeps the list positions. Battle.net stays first and is shown disabled. Direct IP sits in
the Direct Connect slot and takes over Modem's role too. Local Network sits in the IPX network slot. Both run the peer
layer's rules below: where the original's peer-layer ("chat-room") games and its Battle.net games differ, Direct IP
and Local Network follow the peer layer (DECISIONS.md, 2026-10-21). The Battle.net rules are given for completeness
only. Tidewater's wire protocol is its own; this spec fixes only what players see and the order in which commands are
carried out.

### Turns and the order of commands (peer layer)

**When a turn runs (static study, spacing traced).** The game keeps a turn countdown. Before each cycle, the countdown
drops by 1. When it reaches 0, a turn runs, and then the cycle runs. So a turn always sits just before a cycle, and the
commands of that turn are carried out before anything else in that cycle. After a turn the countdown is reloaded with
the turn length K for the current row and speed (see "Turn length"). Turns therefore come before the cycles whose
counter is 0, K, 2K, 3K and so on.

**Paused cycles.** The countdown and the turns keep running while the game is paused, at the same pace. Commands
carried out during a pause still take effect (resume, chat, speed). The cycle counter also keeps counting
(`core/timing-and-rng.md`, scheduler rule 4). Static study.

**Game start (traced).** Before the two start-up cycles the game queues two empty packets of its own. After the
start-up cycles it waits for a first turn, however long that takes, then runs a second turn right away before the
first normal cycle. From then on turns come every K cycles. In the round-89 run (K = 12) turns fell before records
3, 3, 15, 27, ..., 291: 26 turns in 300 cycles.

**One packet per player per turn (static study).** Each turn carries exactly one packet from each human player. A
packet holds one command or nothing (an empty packet). Each command a player issues goes into its own packet, in
issue order, on a queue:

- The queue holds at most **26** packets. A command issued while 26 are waiting is silently dropped.
- At the start of every turn the game tops the queue up to **2** packets with empty ones. In steady play, one packet
  is already waiting ahead of any new command.
- A packet is sent to the other players as soon as it is queued.

**Local commands wait too (static study).** The local player's own packets go through the same queue and are carried
out in the same turn as everyone else's. Nothing a player orders takes effect before the turn that carries it.

**Order inside a turn (static study).** The turn carries out one packet for each player slot 0 to 7, in ascending
order, and only for slots whose controller is human. Each packet's command acts as the slot it came from.

**A command issued mid-turn.** A command issued between turns waits for a later turn. With q packets already waiting
ahead of it, it is carried out in the (q + 1)-th turn to come. In steady play q = 1, so it runs in the second turn
to come. Input is read once per frame before that frame's cycles, so a command read at cycle counter c (the counter
of the next cycle to run) is carried out in the turn before counter K × ceil(c / K) + K. That is a delay of K to
2K − 1 cycles. A second command issued in the same frame runs one turn later, a third one turn after that, and so on.
Static study; agrees with the speed-8 single-player rule in `oracle/trace-format.md` when K = 1. Traced (round 91,
`mp-dark-paths-bne-mp-latency-game-400`): two frames, each with a select and a move, read at counters 198 and 299,
were carried out in the turns before counters 216 and 228, and 312 and 324, as the rule predicts; the traced player's
own commands waited like everyone else's.

### Turn length

**Peer layer (static study, one value traced).** The turn length depends on a **row** (a network delay setting) and
the speed. Row r has a base length T(r) in cycles at the fastest speed (data table "T"). At game start the game
works out the other speeds so that a turn lasts about the same real time at every speed:

- Enhanced: K(r, s) = ceil(T(r) × 13 / L(s)), where L(s) is the cycle length in ms at speed s (80, 60, 53, 46, 40, 33,
  26, 20, 13 for speeds 0 to 8, `core/timing-and-rng.md`). One turn lasts about T × 13 ms.
- Classic mode: K(r, s) = ceil(T(r) × 60 / S(s)), where S(s) is the sum of the three Classic cycle lengths in ms for
  Classic speed s (180, 160, 140, 120, 100, 80, 60 for speeds 0 to 6). One turn lasts about T × 20 ms. Classic mode
  stores speeds on the 9-step scale and maps them 0, 0, 1, 2, 3, 4, 5, 6, 6.

Speed changes don't change the countdown in progress; the new K applies from the next turn on.

**Which row (static study).** The player who creates the game picks the row when entering the lobby:

| Connection | Row | T | Enhanced K at speed 8 |
| --- | --- | --- | --- |
| Single player | 0 | 1 | 1 |
| IPX network | 1 | 2 | 2 |
| Modem or Direct Connect at 2400 baud | 5 | 16 | 16 |
| Modem or Direct Connect at up to 9600 baud | 4 | 12 | 12 (**traced**, Direct Connect) |
| Modem or Direct Connect above 9600 baud | 3 | 8 | 8 |

Four hidden start-up options, typed on the program's command line and not reachable from any menu, change the peer
layer: one overrides the row (1 to 15), one sends each packet extra times (1 to 5 extra copies), one sets the resend
interval (1000 to 5000 ms), and one turns off the automatic drop described under "Stalls". Tidewater needs none of
them. The creator's row is written into the game's advertisement; whether a joining player adopts it or uses its own
default wasn't found (open question).

Gap-fill: Tidewater's Direct IP uses row 3 (T = 8, Enhanced K = 8 at speed 8, about 104 ms per turn) and Local Network
uses row 1, unless a later trace says otherwise. The creator's row applies to every player.

**Latency settings (static study).** Network entries 106 to 108 and the `NetDlg.bin` "Network Options" dialog (Low,
High, Extra high latency) belong to the Battle.net layer only. The peer layer has no latency setting a player can
change; its delay is the row.

**Battle.net layer (static study).** The turn length is max(1, floor(1000 / (L(s) × X))) cycles, where X is the turn
rate the Battle.net provider reports, capped at 20 per second (a reported 0 counts as 20). The number of turns in
transit is the provider's default (kept within 2 to 8) plus the latency setting: 0 for Low (the start value), 1 for
High, 2 for Extra high. A player changes the setting with the Network button (control 4) of the Battle.net options
menu, which opens `NetDlg.bin`. The change travels as a turn command; when it is carried out, every player's setting
changes and every player sees network entry 106, 107 or 108 with that player's name. A Battle.net turn can carry any
number of commands from each player. Within a turn, players are carried out from slot 7 down to slot 0. The value of
X for Battle.net wasn't found (open question).

### Stalls

**Peer layer (static study).** A turn waits until it has every human player's next packet.

1. When a turn starts waiting, a wait timer starts. The cycle doesn't run. The game retries on the next frame and
   keeps drawing the screen and reading input in the meantime.
2. Each time a further resend interval passes (1000 ms by default), the game asks every missing player to send
   again: each one in turn for the first 3 intervals, then all players at once. Each such request adds 1 to the
   turn's resend count, which starts at 0.
3. Once the resend count is above 1, that is after more than 2 intervals (2 s by default), the cursor becomes the
   watch (`ui/screen-and-cursors.md`, pointer rule 1). Traced (round 91): the watch and no dialog about 3 s into a
   stall.
4. After more than 7 intervals (7 s), the waiting dialog `rez\timeout.bin` opens. It shows "Waiting for players:"
   above one row per missing player, by name, with "Press and hold Ctrl-F1 to abort game" and "Press and hold Alt-F2
   to drop dead players". A row disappears when that player's packet arrives. The dialog closes when the turn
   completes. Traced (round 91, `mp-dark-paths-bne-mp-stall-game-300`): the dialog, naming the stopped player, was up
   about 8 s into a 12 s stall and had closed by itself, with nobody dropped, 50 cycles after turns arrived again.
5. **Ctrl+F1**, checked at every retry while waiting, makes the local player leave: the others are told the player
   dropped, the game waits 2 s, and the game ends for that player.
6. **Alt+F2**, checked once per interval from the 8th interval on, drops every player still missing.
7. Outside a game (no waiting dialog), missing players are dropped automatically at the 7th interval unless the
   hidden no-drop option is set. In a game there is no automatic drop: the dialog of step 4 opens instead.

What happens to a dropped player is in `net/leave-and-drop.md`.

**Deadline after a stall (peer layer, static study).** If a turn completes more than 200 ms after it first started
waiting, the cycle deadline is reset to now at the next frame (`core/timing-and-rng.md`, scheduler rules 3 and 5).
Shorter waits change nothing. While the stall lasts, the 500 ms rule applies as usual. This is pacing only: it
changes when cycles run in real time, never what they do.

**Battle.net layer (static study).**

- The deadline: if the wait lasted under 100 ms, the deadline moves forward by half a cycle length only. If it lasted
  longer, the deadline becomes now plus half a turn (turn length in cycles × cycle length ÷ 2). The 500 ms reset is
  suppressed while the stall lasts.
- After 2.5 s without a turn, `rez\btimeout.bin` opens: "Waiting for players:", the missing players, and a countdown
  that starts at 45 s (shorter if the game already waited within the last 2 minutes) and ticks once a second. At 0 the
  "Drop Players" button becomes active.

### Sync check

**Peer layer.** Each packet carries a sequence number and, when marked, a sync byte. The sync byte is the low 7 bits
of the game's random generator state at the moment the packet was made, combined with a 7-bit state value (static
study). An empty packet made while fewer than 2 packets wait is marked, so in steady play every player's packet is
marked unless the player queued a command (static study).

- **What it catches with `allowsync` off (the default; traced, round 91).** A generator difference is caught: with
  the generator's lowest bit flipped on one machine between two turns, each machine dropped the other player at the
  second turn after the change, both showed network entry 65, and both games went on
  (`mp-dark-paths-bne-mp-desync-rng-game-400`). A difference in the "seen" map alone is not caught: one "seen" bit
  different on one machine for four turns (until a fog refresh set it again) gave no drop and no notice
  (`mp-dark-paths-bne-mp-desync-seen-fogmaps-game-400`). Static study reads the state value as a hash of the game
  state only with `allowsync` mode on; default for other state with `allowsync` off: not caught, unless it changes
  the generator.
- **When.** The byte is taken when the packet is made and compared when the packet is carried out. A difference that
  arises between two turns is caught at the second turn after it (traced); a difference in the generator's higher
  bits is caught only once it reaches the low 7 bits (static study).

At every turn, for each other player, the game compares that player's sequence number with its own, and, when both
packets are marked, the two sync bytes.

- **Default (`allowsync` off).** A player whose packet doesn't match is dropped. Each side drops the other, and players
  see network entry 65 "Player %s was dropped." (`ui/messages.md`). The rest of the drop is in `net/leave-and-drop.md`.
- **`allowsync` mode on** (a local cheat toggle, `game/cheats.md`). A mismatch doesn't drop anyone. The game sends a
  sync-error command. When it is carried out, every player flushes the turns up to a marker, writes a sync dump file
  and shows network entry 83 "syncing error", then the game continues. Ctrl+Shift+Alt+Backspace sends the same command
  by hand in this mode.

Network entry 134 "SYNC ERROR." has no use found.

Enhanced and Classic mode behave the same. Their packets carry a different checksum, so an Enhanced and a Classic
game can't mix. A Classic mode session start seeds the generator with 0.

**Tidewater: a design choice, not the original's behaviour (DECISIONS.md, 2026-10-21).** Each turn's check compares
a digest of the whole lockstep state: the generator, the cycle counter, every unit, every player, the alliance table,
the shared-vision sets and allied-victory settings (`game/diplomacy.md`), the "seen" map and every unit's hidden-from
and detected-by sets (`vision/fog.md`). That is wider than the original's
default check, which catches a generator difference but not a "seen"-map difference (traced, above). In Tidewater
any divergence is a bug, so it is caught as early as the turn structure allows. The digest covers only state that
every machine computes the same way, never per-machine state (the explored and visible maps, the local player, the
selection, local settings). A game that stays in sync shows nothing, as in the original. On a mismatch, players see
what the original's default shows: the mismatched players are dropped and network entry 65 appears
(`net/leave-and-drop.md`), **on the original's turn**: a difference that arises between two turns drops at the
second turn after it. Tidewater carries the drop out in the same turn on every machine (`net/leave-and-drop.md`,
"Rules for Tidewater"). Tidewater has no `allowsync` mode. The digest's form is Tidewater's own.

**Battle.net layer (static study).** Every turn carries one sync record: a hash that cycles through unit state, player
state with the generator and cycle counter, and one map row at a time, checked against a 16-turn history. On a
mismatch the player is marked bad. Then the majority decides: if the local player is outnumbered it leaves the game;
otherwise it drops the mismatched players. The outcome goes into the Battle.net game report.

### Pause and speed

**Pause (static study).** The pause rules are in `ui/game-menu.md`, "Pause". New here: pause and resume are turn
commands. The pause key sends a pause if the local game isn't paused and a resume if it is, judged at the moment the
key is pressed. Any player can resume. Turns keep running during a pause (see "Paused cycles").

**Speed (static study).**

- Enhanced multiplayer: the creator picks the speed in the lobby ("Game Speed:" in `chat_p1.bin`; the Classic mode
  lobby shows "Cheat Codes:" there). The in-game speed slider is disabled. On Battle.net the speed is fixed at game
  start from the game's creation settings.
- Classic mode and single player: the speed dialog and keys send a speed command, from any player. The dialog puts the
  local speed back until the command is carried out in a turn, which then changes the speed for every player. The
  change resets the deadline (`core/timing-and-rng.md`, rule 3).

### Other turn content

Every turn command is carried out at the same point. Besides unit commands, a turn carries:

| Content | What players see |
| --- | --- |
| Chat line, with a recipient list | The line appears for each recipient when carried out. Under Enhanced rules the sender's own line appears at once. Classic mode shows network entry 66 to the sender (`ui/messages.md`) |
| Pause, resume | `ui/game-menu.md` |
| Speed (Classic mode and single player) | See "Speed" |
| Cheat | `game/cheats.md` |
| Diplomacy: a player's alliances, shared vision and allied victory | `game/diplomacy.md` (static study). Tidewater sends it as one more command kind of its own wire protocol |
| Drop and leave notices | Network entries 63 to 65 (`net/leave-and-drop.md`) |
| Sync-error report (`allowsync` mode) | Network entry 83 |
| Latency change (Battle.net only) | Network entries 106 to 108 |

## Data

**T: turn length at the fastest speed, in cycles (static study).**

| Row | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T | 1 | 2 | 4 | 8 | 12 | 16 | 20 | 24 | 32 | 40 | 48 | 64 | 96 | 128 | 192 | 255 |

**Enhanced K per row and speed (computed from the rule).**

| Row | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| 1 | 1 | 1 | 1 | 1 | 1 | 1 | 1 | 2 | 2 |
| 2 | 1 | 1 | 1 | 2 | 2 | 2 | 2 | 3 | 4 |
| 3 | 2 | 2 | 2 | 3 | 3 | 4 | 4 | 6 | 8 |
| 4 | 2 | 3 | 3 | 4 | 4 | 5 | 6 | 8 | 12 |
| 5 | 3 | 4 | 4 | 5 | 6 | 7 | 8 | 11 | 16 |
| 6 | 4 | 5 | 5 | 6 | 7 | 8 | 10 | 13 | 20 |
| 7 | 4 | 6 | 6 | 7 | 8 | 10 | 12 | 16 | 24 |
| 8 | 6 | 7 | 8 | 10 | 11 | 13 | 16 | 21 | 32 |
| 9 | 7 | 9 | 10 | 12 | 13 | 16 | 20 | 26 | 40 |
| 10 | 8 | 11 | 12 | 14 | 16 | 19 | 24 | 32 | 48 |
| 11 | 11 | 14 | 16 | 19 | 21 | 26 | 32 | 42 | 64 |
| 12 | 16 | 21 | 24 | 28 | 32 | 38 | 48 | 63 | 96 |
| 13 | 21 | 28 | 32 | 37 | 42 | 51 | 64 | 84 | 128 |
| 14 | 32 | 42 | 48 | 55 | 63 | 76 | 96 | 125 | 192 |
| 15 | 42 | 56 | 63 | 73 | 83 | 101 | 128 | 166 | 255 |

**Classic mode K per row and Classic speed (computed from the rule).**

| Row | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| 1 | 1 | 1 | 1 | 1 | 2 | 2 | 2 |
| 2 | 2 | 2 | 2 | 2 | 3 | 3 | 4 |
| 3 | 3 | 3 | 4 | 4 | 5 | 6 | 8 |
| 4 | 4 | 5 | 6 | 6 | 8 | 9 | 12 |
| 5 | 6 | 6 | 7 | 8 | 10 | 12 | 16 |
| 6 | 7 | 8 | 9 | 10 | 12 | 15 | 20 |
| 7 | 8 | 9 | 11 | 12 | 15 | 18 | 24 |
| 8 | 11 | 12 | 14 | 16 | 20 | 24 | 32 |
| 9 | 14 | 15 | 18 | 20 | 24 | 30 | 40 |
| 10 | 16 | 18 | 21 | 24 | 29 | 36 | 48 |
| 11 | 22 | 24 | 28 | 32 | 39 | 48 | 64 |
| 12 | 32 | 36 | 42 | 48 | 58 | 72 | 96 |
| 13 | 43 | 48 | 55 | 64 | 77 | 96 | 128 |
| 14 | 64 | 72 | 83 | 96 | 116 | 144 | 192 |
| 15 | 85 | 96 | 110 | 128 | 153 | 192 | 255 |

**Constants (static study).**

| Constant | Value |
| --- | --- |
| Packet queue limit | 26 packets |
| Queue top-up at each turn | 2 packets |
| Resend interval | 1000 ms (hidden option: 1000 to 5000) |
| Watch cursor | after more than 2 intervals |
| `rez\timeout.bin` opens | after more than 7 intervals |
| Deadline reset after a peer-layer stall | wait over 200 ms |
| Ctrl+F1 leave delay | 2 s |
| Battle.net waiting dialog | after 2.5 s |
| Battle.net drop countdown | 45 s |
| Battle.net turns in transit | provider default (2 to 8) + latency setting (0 to 2) |

## Test vectors

| Input | Expected |
| --- | --- |
| Enhanced Direct Connect at up to 9600 baud, speed 8, 300 cycles from game start (round 89, **traced**) | Turns before records 3, 3, 15, 27, ..., 291: 26 turns, then every 12 cycles |
| Same game, speed 8, a command read at counter 1 | Carried out in the turn before counter 24 (23 cycles later) |
| Same game, a command read at counter 12 | Turn before counter 24 |
| Same game, a command read at counter 13 | Turn before counter 36 |
| Same game, two commands read in one frame at counter 13 | Turns before counters 36 and 48 |
| Single player (row 0), any speed, a command read at counter c | Turn before counter c + 1 |
| IPX network, Enhanced, speed 8 (row 1) | K = 2; a command read at counter 3 runs at the turn before counter 6 |
| Enhanced, row 4, speed 4 (40 ms cycles) | K = 4; one turn about 160 ms; command delay 4 to 7 cycles |
| Classic mode, row 4, Classic speed 6 | K = 12 |
| Classic mode, row 4, Classic speed 0 | K = 4 |
| A command issued with 26 packets waiting | Dropped; never carried out |
| A peer stops sending; resend interval 1000 ms | Watch after more than 2 s; `rez\timeout.bin` after more than 7 s naming that player |
| Peer-layer stall of 150 ms | Deadline not reset |
| Peer-layer stall of 300 ms | Deadline reset to now at the next frame |
| Battle.net stall of 50 ms | Deadline moves forward by half a cycle |
| Battle.net, a player sets High latency | Every player sees network entry 107 with that player's name; one more turn in transit |
| Two players' sync bytes differ, `allowsync` off | Each drops the other; network entry 65 |
| Enhanced Direct Connect, speed 8, K = 12 (round 91, **traced**: `mp-dark-paths-bne-mp-latency-game-400`): a select and a move read in one frame at counter 198 | Turns before counters 216 and 228 |
| Same game, a select and a move read in one frame at counter 299 (**traced**) | Turns before counters 312 and 324 |
| Same game, `allowsync` off: the generator's lowest bit flipped on one machine after record 150, between the turns before counters 144 and 156 (round 91, **traced**: `mp-dark-paths-bne-mp-desync-rng-game-400`) | At the turn before counter 168 each machine drops the other (slot 0 becomes "nobody" on the traced machine); both show network entry 65; both games go on |
| Same game, `allowsync` off; first the second human types the chat line `allowsync` (Return, the nine letters and Return sent to its display at records 60 to 82), then the generator flip after record 150 (round 96, **traced**: `mp-dark-paths-bne-mp-r96-allowsync-desync-rng-game-400`) | The line is chat (cheats are always disabled in an Enhanced multiplayer game, `ui/game-setup-lobby.md`; `game/cheats.md`): it is carried out as a command of the second human in the turn at record 98 and changes no game state. The drop is exactly the row above's: every record equals `mp-dark-paths-bne-mp-desync-rng-game-400`'s but the chat turn |
| Same game, one "seen" bit cleared on one machine after record 150, set again by the fog refresh at record 201 (round 91, **traced**: `mp-dark-paths-bne-mp-desync-seen-fogmaps-game-400`) | No drop and no notice. **Tidewater** (design choice above): its digest catches it, and both machines drop each other at the turn before counter 168 |
| A peer stops sending for 12 s, Direct Connect (round 91, **traced**: `mp-dark-paths-bne-mp-stall-game-300`) | The watch at about 3 s, the waiting dialog naming the player at about 8 s; the dialog closes by itself when turns resume; nobody dropped |
| Two players' sync bytes differ, `allowsync` on | Network entry 83 for every player; the game continues |
| Paused game over Direct Connect, row 4, speed 8 | Turns still every 12 cycle slots; a resume command takes effect at a turn |

## Open questions

- **Joining players and the row.** The creator's row is advertised, but nothing that copies it to a joining player was
  found. Default: every player uses the creator's row. Trace request MP-L1: in a two-player Direct Connect game where
  the joiner starts with the hidden row option set to 8 and the creator doesn't, record the turn spacing on both
  sides.
- **Battle.net turn rate.** The value X and the provider's default turns in transit weren't found, so the Battle.net
  turn length per speed is unknown. Out of scope for Tidewater (Battle.net is disabled). Trace request MP-L2 only if
  Battle.net is ever revived.
- **Classic mode K.** The Classic table is computed, not traced. Trace request MP-L3: a Classic mode Direct Connect game
  at Classic speed 6 and speed 3; expected K = 12 and K = 6.
- **The stall dialog.** The watch and the dialog were seen at about 3 s and 8 s into a 12 s stall, and the dialog
  closes by itself when turns arrive again (traced, round 91). The Alt+F2 drop and the 200 ms deadline reset are
  static study. Trace request MP-L4 keeps its second part: stop one side for 150 ms and for 300 ms and log the cycle
  timing after each (needs a timer on the waiting side).
- **What the sync check covers with `allowsync` on.** Closed for the default (`allowsync` off, traced, round 91: the
  generator, not the "seen" map). With `allowsync` on, static study reads a hash of the game state in the state
  value; what it covers isn't traced. Out of scope for Tidewater (no `allowsync` mode). Trace request MP-L5b's typing
  step is answered (traced, round 96): in an Enhanced game cheats are always disabled, so a typed `allowsync` is a
  chat line and can't turn the mode on, and a generator difference still drops the player. The mode can only be
  reached where cheats are allowed (a Classic mode lobby that allows them, or the word as a program argument,
  `game/cheats.md`); MP-L5b stays open for that, low priority.
- **Network entry 134.** "SYNC ERROR." has no known use. Default: not shown.
