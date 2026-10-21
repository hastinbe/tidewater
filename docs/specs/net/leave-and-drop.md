# Network: leaving, dropping and surrender

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 89, B-057). The review's rulings are in
  `ERRATA-round89-multiplayer.md`, "Review".
- **Changes:** 2026-10-21, round 90 (oracle traces, `ERRATA-round90-traces.md`; reviewed by a spec-review session,
  2026-10-21): the chat-room knock-out is traced at the check (result only, the slot stays human); vector LV15.
- **Sources:** static study of the 2.02 program (round 89; research notes for this round stay on the research side),
  the dialog files `rez\timeout.bin`, `rez\btimeout.bin`, `rez\surrendr.bin`, `rez\quitdraw.bin` and
  `rez\chatrtrn.bin`, and the tables `Rez\network.tbl` and `rez\gluAll.tbl`. Nothing here was watched in the running
  game. Turns, stalls and the sync check are in `net/lockstep.md`; this spec picks up where a player is gone.

"Network entry n" is entry n (counting from 1) of `Rez\network.tbl`; "gluAll entry n" likewise for `rez\gluAll.tbl`.
L is the local player.

## Behaviour

### Two kinds of network game

The original handles a departing player in two different ways, by the turn layer the game uses (`net/lockstep.md`,
"Network layers by game type"):

| Game | Called here | Rules |
| --- | --- | --- |
| Direct Connect, Modem, IPX network | **chat-room game** (the peer layer; the game returns to the multiplayer chat room) | Enhanced, or Classic mode |
| Battle.net | **Battle.net game** | Enhanced only |

So every Classic mode game is a chat-room game, and every Battle.net game is Enhanced. Where the two Enhanced kinds
differ, both are given; the Classic mode differences are labelled.

**Tidewater slots.** Direct IP (in the Direct Connect slot) and Local Network (in the IPX network slot) are chat-room
games and follow the chat-room rules: where the original's chat-room and Battle.net games differ, Tidewater follows
the chat-room game (DECISIONS.md, 2026-10-21). In particular a player who is out is **not** turned into "nobody" by
the victory check, and no player gets the "dropped" or "game ended" result. Battle.net is shown disabled; its rules
are given for completeness only.

**Note on other specs.** `ui/game-menu.md` and `game/save-load.md` say "with a network session" for what this spec
calls a Battle.net game, and "without a network session" for a chat-room game.

### Ways a player leaves

| Way | Who starts it | Chat-room game: what the others see | Battle.net game: what the others see |
| --- | --- | --- | --- |
| Surrender (confirmed in `surrendr.bin`) | the leaver | "left" (network entry 64) | "has left" (network entry 111) |
| Quit (draw), Battle.net before 120 s (`quitdraw.bin`) | the leaver | not offered | "has left" (network entry 111) |
| Defeat by the victory check | the leaver's own check | "eliminated" (network entry 63) if it has nothing left, else "left" (64) | "has left" (111) |
| Victory, Quit to the main menu, Exit the program | the leaver | "left" (64), or 63 | "has left" (111) |
| Ctrl+F1 while the game waits for a turn | the leaver | "dropped" (network entry 65), or 63 | not offered |
| The leaver's connection is lost (the session reports the player gone) | the session | "dropped" (65), or 63 | "was dropped" (network entry 112) |
| Alt+F2 while the game waits, or the automatic drop | one remaining player | "dropped" (65), or 63, **on that player's machine only** | not offered |
| "Drop Players" in `btimeout.bin`, or the majority rule after bad turn data | one remaining player | not offered | "was dropped" (112) |
| A sync mismatch (`net/lockstep.md`, "Sync check") | each side | "dropped" (65), or 63 | "was dropped" (112) |

Static study for every row. "Or 63" is the notice rule below: network entry 63 replaces 64 and 65 when the player has
no units and no buildings left. The automatic drop happens only outside a game, before the lobby's game starts
(`net/lockstep.md`, "Stalls", step 7); in a game the waiting dialog opens instead.

### Chat-room game: carrying out a leave

A chat-room game learns that a player is gone from that player's packet in a turn (`net/lockstep.md`, "Turns and the
order of commands"). There are two kinds:

- **Left.** Every machine that ends its game while still in the session tells the others it left, as the last thing
  it does. This covers surrender, defeat, victory, Quit to the main menu and Exit the program.
- **Dropped.** The leaver's own Ctrl+F1 (the leaver tells the others it dropped), and a drop decided on a remaining
  machine: the session reports the player gone, Alt+F2, the automatic drop, or a sync mismatch. A drop decided on a
  remaining machine is made up on that machine, in place of the missing player's packet.

A leave is carried out in the turn, in slot order with the other players' packets, so every machine that receives the
same packet carries it out at the same point (static study):

1. **Notice.** The notice line shows (`ui/messages.md`, "Showing a notice"; colour 4, 7 seconds), filled in with the
   player's name:
   - network entry 63 "Player %s was eliminated." if the player has no units and no buildings left;
   - otherwise network entry 64 "Player %s left the game." for a player who left;
   - otherwise network entry 65 "Player %s was dropped." for a player who was dropped.
2. **Score row.** The player's row for the score screen is recorded now, with its label as it stands at this moment
   (`ui/multiplayer-score.md`).
3. **Slot.** The player's slot becomes "nobody" (in Enhanced and in Classic mode). The player stops counting as a
   human player of the session.
4. **Shared vision** (Enhanced only). The player is removed from every other player's shared-vision set, and its own
   set becomes just itself (`vision/fog.md`, "Shared vision"). **Classic mode:** shared vision is not touched (it never
   changes during play in Classic mode).
5. **Host.** If the player was the host, a new host is chosen: the first remaining connected player who can host.
   If there is none, the game has no host (see "When the host leaves").

What a chat-room leave does **not** do (static study):

- **The units stay as they are.** The leaver's units and buildings stay on the map and keep their current orders. They
  are not given order 32 and their remembered orders are kept. With the slot "nobody", nothing gives them new orders;
  what idle units of a "nobody" player do on their own (for example, fight back) follows the ordinary unit rules.
- **No result is set** for the leaver by the leave itself. The victory check sets it (next section).
- **Alliances are not changed.** The alliance table keeps the leaver's rows and columns.

### Battle.net game: carrying out a leave

A Battle.net game learns that a player is gone from the session layer, as a leave event with a reason: left or
dropped (static study). It is carried out like this:

1. **Message.** The message list (not the notice line) shows, in colour 4: network entry 112 "%s was dropped from the
   game." for a dropped player, or network entry 111 "%s has left the game." otherwise.
2. **Result.** A dropped player is marked dropped and gets the "dropped" result if it has no result yet. A player who
   left gets the "defeat" result.
3. **Conversion.** The player is turned into "nobody" as in `game/victory.md` (the multiplayer rule, step 2): its slot
   becomes "nobody", it is removed from every shared-vision set and its own set becomes just itself, and every one of
   its living units has its remembered order cleared and is given order 32, so the units stand where they are.

Before the game starts (in the Battle.net game lobby), the same events show the same two texts in the lobby's chat
area, and a player joining shows network entry 110 "%s has joined the game." (static study).

**When a leave event is carried out** relative to the turns wasn't traced (trace request LD4). Default (gap-fill):
treat it as part of the next turn, so every remaining machine carries it out at the same cycle.

### The victory check after a leave

The multiplayer rule of `game/victory.md` runs on every machine at the same cycle. A leaver affects it like this
(static study; `game/victory.md` carries the same rules):

- **A "nobody" slot that still has units or buildings is "in", but counts as neither a human nor a computer player.**
  So it never blocks a victory: the remaining human players can win while the leaver's units still stand.
- **A slot with nothing left** is "out" and, if it has no result yet, gets the "defeat" result. Seen in a Direct
  Connect trace (round 90, LV15): a defeated human's slot stayed human at the check and became "nobody" 16 cycles
  later, outside a check. That is most likely that player's own leave after its defeat (the second machine isn't
  traced).
- **The conversion** (step 2 and step 3 of the multiplayer rule: "nobody", shared vision, order 32) happens **only in a
  Battle.net game**. In a chat-room game the check only sets the result.
- **L out.** When L is out, L's machine ends its game with a defeat at once. Every other machine marks L out at the
  same cycle in its own check, so the remaining machines agree.

### Does the leaver see a defeat?

| How the leaver's game ends | Leaver's outcome | Score screen on the leaver's machine |
| --- | --- | --- |
| Surrender, or Quit (draw) | defeat | yes (defeat background), unless the skip setting is on |
| Defeat by the check | defeat | yes |
| Ctrl+F1 | defeat | yes |
| Return to the chat room | none: the game ends for everyone | no |
| Quit to the main menu, Exit the program | none | no |
| Dropped by the others (still running) | the game goes on locally until this machine itself drops or leaves the others | as for the way it then ends |

Multiplayer has no victory or defeat dialog: the game ends at once (`game/campaign-flow.md`, "The victory and defeat
dialogs"). Static study; the score screen is `ui/multiplayer-score.md`.

### Surrender and quit (draw)

The end-scenario menu and its confirmations are in `ui/game-menu.md`, "Ending the scenario". In multiplayer:

- **Surrender.** OK ends the game on L's machine with a defeat. No command is sent to the others and no result is set
  at that moment; L's machine then leaves the session as it ends its game, so the others see "left" (chat-room game,
  network entry 64) or "has left" (Battle.net, network entry 111), and then mark L out at their next victory check
  (static study).
- **Quit (draw)** (Battle.net, at most 120 seconds of game time). On L's machine it is exactly a surrender. The
  difference is only in the result reported to Battle.net: a game that ends within the first 120 seconds is reported as
  a draw for every player, whoever quit (static study). The other players see "has left" (network entry 111).

**The Battle.net report** (static study; display only, sent to the service and never shown on screen). At the end of
the game each player's result is reported:

| Case | Reported |
| --- | --- |
| The game clock is at most 120 s, or the session holds one player, or the game was loaded from a save | a draw for everyone |
| Result "victory" | win |
| Result "defeat" | loss |
| Result "dropped" | disconnect |
| Result "game ended" | draw |
| No result | nothing; L's own missing result counts as a loss |
| Exactly one other player is still connected, there are no computer players, and that player has no result | that player gets a win (and the "victory" result locally) |

After the majority rule of `btimeout.bin` ran, the report changes: after a tie that dropped the others, the dropped
players are reported as disconnects and L reports nothing; when L was outnumbered and left, the others report nothing
and L reports a disconnect. The report's text uses network entries 137 to 142 and the statistic labels of
`rez\gluAll.tbl`; it is not part of what players see.

### Return to the chat room

Chat-room games only (`ui/game-menu.md` hides the button on Battle.net). Only the host may choose it (the check of
`ui/game-menu.md`, "Who may restart, return or load"). OK sends a command (static study):

1. The command is carried out in the turn on **every** machine.
2. The host goes to the chat room for the creator (`rez\chat_p1.bin`, or `rez\chat_mc.bin` in Classic mode), every
   other player to the chat room for joiners (`rez\chat_p2.bin`, or `rez\chat_mj.bin` in Classic mode).
3. If there is no host at that moment, every machine goes to the main menu instead.
4. The game ends for everyone. No score screen is shown, nobody gets a result, and no leave notice is shown. The
   players are put back in their chat-room order.

Load and save requests follow the same host rule (`game/save-load.md`, "Who may save or load").

### When the host leaves

**Chat-room game** (static study). The host is the player who can host the session (the creator at first). When the
host leaves or is dropped, the first remaining connected player who can host becomes the host, and from then on only
that player may return to the chat room, load or save; the others get network entry 28 (`ui/game-menu.md`, "Who may
restart, return or load", which also lets anyone through while fewer than three players are in the game). If no
remaining player can host, there is no host: the check then lets anyone through, and Return to the chat room sends
everyone to the main menu. The game itself goes on.

**Battle.net game** (static study). The host leaving is an ordinary leave; the game goes on. gluAll entry 104 "The game
host has left." and entry 96 "Player %s has left the game." were not found used in a game (lobby screens only, by
their wording; trace request LD5).

### Waiting for a player

When a turn is late, the waiting dialogs and keys of `net/lockstep.md`, "Stalls", apply:

- **`rez\timeout.bin`** (chat-room game). It lists "Waiting for players:" with the missing players' names, removes a
  name as that player's packet arrives, and closes when the turn completes. Its picture is `art\h128x144.bmp` or
  `art\o128x144.bmp` by L's race. **Ctrl+F1** (held) makes L leave: the others are told L dropped, and L's game ends
  with a defeat. **Alt+F2** (held) drops every player still missing, on L's machine only.
- **`rez\btimeout.bin`** (Battle.net game). "Waiting for players:" with up to 8 missing human players, in an order
  shuffled from the game's seed, a countdown shown as minutes and seconds ("%d:%02d"), and a "Drop Players" button.
  - The countdown starts at 45 seconds. If a waiting dialog closed less than 120 seconds before, it starts at what was
    left then minus 2 seconds (never below 0 or above 45). It goes down by 1 each second. At 0 the countdown text is
    hidden and the button becomes active.
  - The dialog stays until the turn arrives.
  - **Drop Players** applies the majority rule: the players still waited for are counted against the others (L
    included). If the others are more, the waited-for players are dropped. If the numbers are equal, they are dropped
    too, and the tie is remembered for the report. If the others are fewer, L leaves the game itself, as dropped. The
    same rule runs when a player's turn data are bad (`net/lockstep.md`, "Sync check").

### A save made after a leave

A multiplayer save (`game/save-load.md`, "Saving in multiplayer") made after a leave keeps (static study):

- the leaver's slot as "nobody", and its units and buildings with their orders;
- the shared-vision sets as they are (the leaver alone in its own set, Enhanced);
- every player's result, and which players were dropped;
- the Battle.net tie mark and game clock (Battle.net game).

So a loaded game has the leaver as "nobody" from the start. The leaver's units count for victory as described above:
"in", but neither human nor computer. A loaded Battle.net game always reports a draw (see the report table).

### Rules for Tidewater

Tidewater runs only chat-room games (Direct IP, Local Network). Which parts are game state (must match on every
machine) and which are display only:

| Part | Kind | Rule |
| --- | --- | --- |
| Carrying out a leave (slot "nobody", score row, shared vision, host) | game state | in the turn, in slot order, as above |
| The notice text (63/64/65) | display only | as above |
| A drop decided on one machine (Alt+F2, the automatic drop, the session reporting a player gone) | game state | **Gap-fill:** a drop must reach every remaining machine and be carried out in the same turn on all of them, so that all agree. The original carries it out only on the machine that decided it (static study); that can split the game, see "Open questions" |
| The victory check's out-marking and results | game state | as `game/victory.md`, "The multiplayer rule", chat-room game: an out player gets the "defeat" result and nothing else (no conversion to "nobody", no order 32, no "game ended") |
| L's own outcome, the end of L's game, the score screen | display only | as above |
| Return to the chat room | game state | a command carried out in the turn on every machine |

## Data

| Item | Value |
| --- | --- |
| Notice: eliminated / left / dropped (chat-room game) | network entries 63 / 64 / 65, notice line, colour 4, 7 s |
| Message: joined / left / dropped (Battle.net) | network entries 110 / 111 / 112, message list, colour 4 |
| Only one player may load, save, restart | network entry 28 |
| Quit (draw) limit | 120 s of game time (`ui/game-menu.md`) |
| `btimeout.bin` opens after | 2.5 s without a completed turn |
| `btimeout.bin` countdown | 45 s; repeat within 120 s: previous remainder − 2 s, clamped to 0..45 |
| Waiting-dialog texts | "Waiting for players:", "Press and hold Ctrl-F1 to abort game", "Press and hold Alt-F2 to drop dead players" |
| Waiting-dialog pictures | `art\h128x144.bmp` (human), `art\o128x144.bmp` (orc) |
| Battle.net report text | network entries 137 to 142; hours and minutes from network entries 101 to 104 |

Player results (as in `game/victory.md`): dropped, defeat, victory, "game ended", or none. The "dropped" result and
the "game ended" result occur only in a Battle.net game.

## Test vectors

All static study. Enhanced unless labelled.

| Case | Expected |
| --- | --- |
| LV1. Chat-room game, humans 0 and 1 not allied; player 1 surrenders with units left | Player 0 sees "Player <name> left the game." (64). Slot 1 becomes "nobody"; its units keep their orders. At the next check slot 1 is "in" but neither human nor computer: player 0 is the only human in and wins |
| LV2. As LV1, but player 1's last building dies and it has no units | Player 1's machine ends with a defeat at that check; player 0's check marks player 1 defeated in the same cycle and gives player 0 the victory. Player 0 then sees "Player <name> was eliminated." (63) when player 1's leave arrives |
| LV3. Chat-room game, three humans, player 2 pulls the cable; players 0 and 1 both drop it | Both see "Player <name> was dropped." (65); slot 2 becomes "nobody" on both at the same turn |
| LV4. Chat-room game, three humans, only player 0 presses Alt+F2 while player 2 is late | Original: only player 0 carries out the drop (static study). Tidewater (gap-fill): the drop is carried out by players 0 and 1 in the same turn |
| LV5. Chat-room game, Enhanced, players 0 and 1 share vision, player 1 leaves | Player 1 removed from player 0's shared-vision set; player 0 no longer sees through player 1's units |
| LV6. As LV5 in Classic mode | Shared vision isn't changed by the leave |
| LV7. Battle.net, player 1 leaves at 90 s of game time | Others see "<name> has left the game." (111); player 1 gets "defeat" and is converted: its units stand still (order 32). The report is a draw for everyone |
| LV8. Battle.net, player 1 surrenders at 300 s, player 0 then wins alone | Reported: player 1 loss, player 0 win |
| LV9. Battle.net, all players allied with allied victory, nobody lost | Every slot gets "game ended"; reported as a draw |
| LV10. Battle.net, 4 players, 2 late; L presses Drop Players | Responding 2 (L included) equals waiting 2: the late players are dropped, tie remembered; report: dropped players disconnect, L nothing |
| LV11. Battle.net, 4 players, 3 late | L is outnumbered: L leaves as dropped |
| LV12. Battle.net, second wait 30 s after a first one closed with 20 s left | Countdown starts at 18 s |
| LV13. Chat-room game, four humans, host (player 0) leaves; players 1, 2 and 3 remain, all can host | Player 1 becomes the host; player 2 choosing Return to the chat room sees network entry 28 with player 1's name |
| LV14. Chat-room game, host chooses Return to the chat room | Every machine ends the game in the same turn, no score screen; the host is in the creator's chat room, the others in the joiners' |
| LV15. `mp-rose-petal-bne-game-21300` (oracle trace, Enhanced, Direct Connect): the traced human in slot 4, four computers, the idle human B in slot 0 whose only unit, a peon, dies at about n = 21000 (traced) | At the check at counter 21105 (record n = 21107) B gets "defeat" and its controller stays human; at n = 21123, not at a check, B's slot becomes "nobody" (B's leave, inferred). No slot gets "game ended" |

## Open questions

- **Local drops split the game.** In a chat-room game a drop decided on one machine (Alt+F2 in a game, the automatic
  drop before it) is carried out only there; the dropped player and anyone who didn't drop keep it in the game. Whether the session layer
  in practice makes every machine drop together wasn't traced. **Trace request LD1:** a 3-player chat-room game, delay
  player 2's packets, press Alt+F2 on player 0 only; record each machine's notices and whether player 1 keeps waiting.
- **Leave notices in practice.** Which notice each way of leaving shows (the "Ways a player leaves" table) is static.
  **Trace request LD2:** a 2-player Direct Connect game, once with Surrender and once with Ctrl+F1 on player 1;
  record player 0's notice and the cycle of slot 1's change to "nobody".
- **What a "nobody" player's units do.** They keep their orders; whether idle ones still fight back wasn't traced
  (default: the ordinary unit rules). **Trace request LD3:** after LV1, attack one of player 1's idle footmen.
- **Battle.net leave timing.** Whether a leave event is carried out at the same cycle on every machine (default:
  next turn). **Trace request LD4** (needs a Battle.net-style session; probably not reachable).
- **Host-left texts.** gluAll entries 96 and 104 weren't found used in a game. **Trace request LD5:** a Battle.net
  lobby where the host leaves (not reachable without the service; low priority).
- **Quit to the main menu, Exit the program** in multiplayer: these were taken to leave the session as any other game
  end does (static study, not followed in detail). **Trace request LD6:** Quit to the main menu on player 1 of a
  2-player Direct Connect game; record player 0's notice.
