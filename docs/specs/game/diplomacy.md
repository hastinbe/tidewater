# Game: diplomacy (alliances, shared vision and allied victory during play)

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-22, round 99). Drafted from
  `ERRATA-audit2-net.md` errata 1 to 3; the review's rulings are in that file's "Review".
- **Sources:** static study of the 2.02 program: the command's dispatch, its Enhanced and Classic-mode effects, the
  display rebuild after it and a second place that applies the Enhanced form (research notes, round 99). **No trace**:
  changing an alliance needs the in-game diplomacy screen, which the oracle's multiplayer driver doesn't press yet.
  Every rule here is static study unless it says otherwise.
- **Rules mode:** every rule is Enhanced unless it says **Classic mode**. The Classic-mode rules are static study and
  change no Enhanced rule.
- **Changes:** 2026-10-22: first version (round 99, B-040 second audit; fills the "Diplomacy (alliance) changes" gap
  of `net/lockstep.md`).

## Behaviour

### What a player's diplomacy is

Each player 0 to 7 has three settings that change during play only through the diplomacy command (and the leave
rules of `net/leave-and-drop.md` for shared vision):

| Setting | Defined in | Set at game start by |
| --- | --- | --- |
| **Alliance row:** for each slot 0 to 15, whether this player counts that slot as allied. The table is directional: player a's row says how a treats b, not how b treats a | `game/setup.md`, "Starting alliances and shared vision" | `game/setup.md` |
| **Shared-vision set:** the players this player shares its vision with | `vision/fog.md`, "Shared vision" | `game/setup.md` |
| **Allied-victory setting:** on or off | `game/victory.md`, "The multiplayer rule", step 6 | Not studied (open question "Allied victory at the start"; default: off) |

Every rule that tests an alliance reads the alliance row as it stands at that moment, so a change is seen by the next
test made after the command, in the same turn included.

### The diplomacy command

A player's diplomacy screen sends one **diplomacy command** with the player's whole settings each time the player
confirms the screen. It has these fields:

| Field | Values | Meaning |
| --- | --- | --- |
| `player` | 0 to 7 | The player whose settings these are |
| `alliances` | 8 values, one for each player 0 to 7, each 0 to 3 | 0 is "not allied"; 1, 2 and 3 are "allied" |
| `vision` | A set of players 0 to 7 | The new shared-vision set |
| `allied_victory` | on or off | The new allied-victory setting |

**Classic mode** form: `alliances` is one allied or not-allied bit per player 0 to 7 (values 0 and 1 only), and the
`vision` field isn't used. The other fields are as above.

**When it runs.** The command is carried out in the command turn, in its place among the turn's commands, like every
command (`game/lifecycle.md`; `net/lockstep.md`, "Turns and the order of commands"; `vision/fog.md`, phase table:
before phase 1).

**Who may send it.** It is carried out only when `player` is the player who sent the command. A command whose
`player` names anyone else does nothing at all: no setting changes and no message appears.

**What it changes (Enhanced),** all in the one command:

1. **Alliances.** The sender's row entries toward players 0 to 7 are replaced by the command's values: an entry is
   allied when its value isn't 0. The entry toward the sender itself is part of the row and is written like the
   others. The entries toward slots 8 to 15 (neutral included) don't change. Only the sender's row changes: the other
   players' rows toward the sender stay as they were, so an alliance is one-sided until the other player sends its
   own command.
2. **Shared vision.** The sender's shared-vision set becomes the command's `vision` set, all 8 players at once,
   whatever it was before. Nothing adds the sender back: a set that leaves the sender out takes the sender out of its
   own set until a later diplomacy command or a leave changes it (**mirrored quirk**; see "A player outside its own
   set"). What the set then changes is given by the set rules of `vision/fog.md`.
3. **Allied victory.** The sender's allied-victory setting becomes the command's `allied_victory`.
4. **Display.** On every machine the local player's unit colour table is rebuilt from the local player's row: own
   units, allied units and the rest get the three palette colours of `ui/selecting-and-commanding.md` (250 own, 251
   allied, 249 other). This is display only; no game state reads it.

**Classic mode:** steps 1, 3 and 4 as above, with the 0 or 1 values; step 2 doesn't happen: shared vision never
changes during play in Classic mode (`vision/fog.md`, "Shared vision").

**A raw value of 2 or 3** counts as allied wherever an alliance is tested; no rule found distinguishes 1, 2 and 3.
Tidewater may store the entry as allied or not allied.

### A player outside its own set (mirrored quirk)

The original applies the `vision` set as sent, its sender's own entry included, so a player can leave its own
shared-vision set. Whether the original's diplomacy screen can send such a set wasn't checked. Tidewater mirrors the
command's effect exactly (Nick's standing 1:1 goal; review ruling, round 99): it causes no desync, since every machine
applies the same set in the same turn, and no rule found reads the set in a way that breaks without the owner in it.
By the rules of `vision/fog.md`, a player outside its own set:

- doesn't have its explored and visible maps changed by its own units' reveals (they change only for the players in
  the revealing player's set: "Whose display changes"); the "seen" map is unaffected, as it changes for every
  revealing player;
- doesn't detect its own invisible units through the set (an invisible unit is detected by its owner's set, then
  widened: "Detection (phase 1)"); its own submarines are still detected by it as their owner.

**Tidewater's screen** always sends the sender in its own `vision` set and its own alliance entry as allied, so the
quirk arises only from a command another program sends. Code must not assume that a player is in its own set or
allied with itself after a diplomacy command (no assertion, no repair).

### How it meets the other rules

- **Victory.** `game/victory.md`, "The multiplayer rule", step 6, needs every human player who is in to have allied
  victory on and every pair of them allied both ways, each player paired with itself included. So two players reach
  an allied victory only after both have sent a command that allies them with each other and turns allied victory on.
  A player who cleared its own alliance entry can't share an allied victory while another human player is in.
- **Leaving and dropping.** A leave doesn't change the alliance table (`net/leave-and-drop.md`); it removes the leaver
  from every shared-vision set and makes its own set just itself (`vision/fog.md`), which may put back a player that a
  diplomacy command had taken out of its own set. A player who has left sends no more commands.
- **Team games.** Team-mates' starting alliances and shared vision come from setup (`game/setup.md`); a diplomacy
  command can change them later like any other.
- **Sync (Tidewater).** The alliance rows, the shared-vision sets and the allied-victory settings are lockstep state,
  part of Tidewater's turn digest (`net/lockstep.md`, "Sync check"). The colour table of step 4 is local and isn't.
- **Saves.** The alliance table is saved in Enhanced mode (`game/save-load.md`). Whether the shared-vision sets and the
  allied-victory settings are saved wasn't studied (open question "Saves").
- **Network commands.** The original's packet layout is out of scope (`net/lockstep.md`, "Scope"): Tidewater carries
  the command in its own wire protocol as one more command kind with the four fields above, and records it in replays
  like any other command. The lobby and the multiplayer menus need nothing new: the diplomacy screen is opened from the
  in-game menu (`ui/game-setup-lobby.md` names its dialog, "CHOOSE YOUR ALLIES", an 8-line list ending with "Allied
  Victory"; its layout is a later UI spec).
- **Recorded commands.** The original has a second place that applies the Enhanced form of this command, apparently
  when recorded commands are played back (static study; the purpose is inferred, not traced). See the open question
  "Recorded commands".

## Data

| Item | Value |
| --- | --- |
| Players a command can name | 0 to 7 |
| Alliance values (Enhanced) | 0 not allied; 1, 2, 3 allied |
| Alliance values (Classic mode) | 0 not allied; 1 allied |
| Row entries a command writes | players 0 to 7, the sender's own entry included; slots 8 to 15 never |
| Colour table (display) | 250 own, 251 allied, 249 other |

## Test vectors

All static study (no trace has a diplomacy command). Unless stated, the game is Enhanced, players 0 to 3 are humans,
and the command is sent by the player it names.

| ID | Setup | Expected |
| --- | --- | --- |
| DP1 | Player 1 sends: allied with 1 and 2, not with 0 or 3 (values 0, 1, 1, 0 for players 0 to 3); vision {1, 2}; allied victory on | Player 1's row: allied with 1 and 2, not with 0 or 3; entries toward 8 to 15 as before. Its vision set {1, 2}; its allied victory on. Player 2's row unchanged: the alliance is one-sided until player 2 sends its own command |
| DP2 | Player 1 sends a command whose `player` is 2 | Nothing changes for anyone |
| DP3 | **Classic mode:** the command of DP1 (bits 0, 1, 1, 0) | Player 1's row as in DP1; its vision set unchanged; allied victory on |
| DP4 | Player 1 sends the value 3 toward player 2 | Player 1 counts player 2 as allied |
| DP5 | Player 1 sends vision {2} (mirrored quirk) | Player 1's set is {2}. Player 1's own units' reveals no longer change player 1's explored and visible maps (they change player 2's); player 1's invisible units are detected by player 2 (and whoever player 2's set adds), not by player 1 through the set |
| DP6 | Humans 0 and 1 are the only players in; player 0 sends allied with 0 and 1, allied victory on; player 1 hasn't sent anything (not allied with 0, allied victory off) | The victory check gives no result. After player 1 sends allied with 0 and 1 with allied victory on, the next check gives victory to both (`game/victory.md`, "The multiplayer rule", step 6) |
| DP7 | As DP6, but before the check that follows player 1's command, player 0 sends a second command with its own entry 0 (not allied with itself), still allied with 1, allied victory on | That check gives no result (the self pairing fails) |
| DP8 | Player 1 sends allied with 0 while its row toward the neutral slot 15 is allied | The entry toward 15 stays allied: slots 8 to 15 are never written |

## Open questions

- **Allied victory at the start.** The setting's start value wasn't read (game setup is one of its users not studied
  in round 99). Default: off for every player at every game start. Trace request DIP1 below would show it.
- **Saves.** Whether the shared-vision sets and allied-victory settings are saved and restored. Default: they are
  restored as saved, like the alliance table (no multiplayer save can be captured by the oracle yet).
- **An own alliance entry of 0.** The command can write it (above), and the victory test reads it. Whether every other
  reader (target searches, the colour table) tests the table for a unit of the player's own, or checks the owner
  first, wasn't studied. Default: every reader reads the table as written. Only a command from another program can
  send it, since Tidewater's screen never does.
- **Recorded commands.** The second place that applies the Enhanced form would, if it is the recorded-command
  playback, apply it in a Classic-mode recording too. Not mirrored until traced: Tidewater applies the form of the
  game's own rules mode everywhere.
- **The diplomacy screen** (which lines it shows, whether it can send a set without its sender, whether a vision box
  exists for each line) is a later UI spec.
- **Trace request DIP1** (multiplayer driver, M track): open the diplomacy screen on the traced player, ally with the
  second human and tick shared vision and allied victory; then the same on the second human's display; record the
  `turn` records (the command appears as `other` until `oracle/trace-format.md` describes it), the units' target
  choices between the two players before and after, and the victory check with the `game` extra.
