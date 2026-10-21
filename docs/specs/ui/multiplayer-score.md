# UI: the multiplayer score screen

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 89, B-057). The review's rulings are in
  `ERRATA-round89-multiplayer.md`, "Review".
- **Sources:** static study of the 2.02 program (round 89; research notes for this round stay on the research side),
  the score dialogs `rez\msnstat2.bin`, `rez\msnstat4.bin`, `rez\msnstat6.bin` and `rez\msnstats.bin`, and the tables
  `Rez\network.tbl` and `rez\gluAll.tbl`. Nothing here was watched in the running game.

This spec gives only what differs from the single-player score screen of `game/campaign-flow.md` ("The victory and
defeat dialogs", "The score screen", "After the score screen"). "Network entry n" is entry n (counting from 1) of
`Rez\network.tbl`. L is the local player. "Chat-room game" and "Battle.net game" are as in `net/leave-and-drop.md`.

## Behaviour

### When it is shown

- **No victory or defeat dialog.** When L's game ends with a victory or a defeat, the game ends at once (static study;
  as `game/campaign-flow.md` already says).
- **Then the score screen**, unless the skip setting ("skip the act screen, briefing and score screens") is on, the
  same as in single player (static study).
- **No score screen** when the game ends by Return to the chat room, Quit to the main menu or Exit the program
  (`net/leave-and-drop.md`, "Does the leaver see a defeat?").

**Background and music** follow L's outcome as in single player: the victory art for a victory, the defeat art
otherwise (static study).

### Which players get a row, and when the row is made

A row is made for (static study):

1. **Each human player, at the moment it leaves the session** (`net/leave-and-drop.md`, "Carrying out a leave", step
   2), so the row of a player who left early keeps its label from that moment.
2. **Every human player still connected when L's game ends**, L included, at that moment.
3. **Every computer player** (by its role at the start of the game), when the score screen opens.

Slots that were empty ("nobody") at the start get no row.

### Row text

Each row is one text line, **name first** (static study, round 89; trace request MS1). The single-player score
screen keeps its stated "label - name" text and slot order (`game/campaign-flow.md`) until MS1 settles both: round 89
reads the same row text and order for single player, which the review held.

| Player | Row reads |
| --- | --- |
| L | L's player name, alone |
| Another human player, allied with L (L's alliance toward that player) when the row is made | "*name* - Ally" (network entry 36) |
| Another human player, not allied with L | "*name* - Enemy" (network entry 35) |
| A computer player | "*nation name* - Enemy", always Enemy, whatever the alliances; the nation name is the `network.tbl` entry by slot and race of `game/campaign-flow.md` |

The "You" label (network entry 34) is not used in multiplayer: L's row is just the name. The separator is " - " (space,
hyphen, space).

### Row order

**L's row first** (the dialog's first row), then the other rows in slot order, 0 to 7, skipping L (static study). The
dialog is chosen by the number of rows as in single player (2, 4, 6 or 8 rows).

### Statistics, score and rank

- **Statistics** for each row are taken when the screen opens, as in single player: units and buildings alive now plus
  lost, resources gathered, kills and razings (static study). A player who left still has its counts: its units and
  buildings stay on the map as "nobody" (`net/leave-and-drop.md`).
- **Score and rank.** The campaign total is 0 at the start of every multiplayer game, so the shown score and the rank
  come from L's mission score of this game, plus 500 for a victory (static study; `game/campaign-flow.md`, "Campaign
  total"). The cheat rank (entry 39) applies as in single player, but cheats are off in an Enhanced multiplayer game.
- **No elapsed time and no result word.** The screen shows no game time and no "Victory!", "Defeat!" or "Draw!" text.
  gluAll entries 17 "Elapsed Time: %s" and 18 to 22 ("Defeat!", "Victory!", "Draw!", "Disconnect", "Undecided") were
  not found used anywhere in the program (static study; trace request MS2).

### What follows the score screen

| Game | After the score screen |
| --- | --- |
| Battle.net game | back to the Battle.net screens (static study) |
| Chat-room game (Direct Connect, Modem, IPX network; Enhanced and Classic mode) | **Held** (trace request MS3): "the multiplayer screens", as `game/campaign-flow.md` states. Gap-fill: that is the first multiplayer screen, the Enhanced or Classic choice (`ui/multiplayer-menus.md`, section 1), with the session closed. Static study (rounds 74 and 89) reads the **main menu** instead, after a victory and after a defeat. Either way not the chat room: only Return to the chat room goes there |

**Tidewater slots.** Direct IP and Local Network games are chat-room games and follow the chat-room row (DECISIONS.md,
2026-10-21).

### The "game ended" result

The "game ended" result is used only in a Battle.net game (static study). When the victory check finds a victory and
no player has the "dropped" or "defeat" result, every slot that wasn't empty at the start gets "game ended", and L's
result stays "game ended" (L's "victory" is set only if it has no result). This happens when every remaining player
wins together (allied victory) and nobody lost. "Game ended" is reported to Battle.net as a draw
(`net/leave-and-drop.md`, "The Battle.net report"). It changes nothing on the score screen: the background is still the
victory art.

### Text sent to Battle.net

At the end of a Battle.net game a text summary is sent to the service: network entry 137 "%s was %s and played for
%s", entry 138 'On map "%s":', entries 139 to 142 (overall, units, structures and resources scores), with the
statistic labels of `rez\gluAll.tbl` and the game time in hours and minutes (network entries 101 to 104). It is never
shown on screen (static study). Tidewater has no Battle.net and needs none of it.

## Data

| Item | Value |
| --- | --- |
| Row text, L | name |
| Row text, others | "*name* - *label*" |
| Labels | network entry 35 "Enemy", 36 "Ally" (34 "You" unused in multiplayer) |
| Rows | L first, then slots 0 to 7 in order |
| Unused texts | gluAll entries 17 to 22 |
| Report text (not shown) | network entries 137 to 142, 101 to 104 |

## Test vectors

All static study. Enhanced unless labelled.

| Case | Expected rows (in order) |
| --- | --- |
| MS-V1. Chat-room game, L = slot 1 "Bob", slot 0 "Ann" allied with Bob, slot 2 "Cy" not allied; Bob wins after both leave | "Bob", "Ann - Ally", "Cy - Enemy"; victory background; then the multiplayer screens (held, MS3; static study: the main menu) |
| MS-V2. As MS-V1, but Bob broke the alliance with Ann after Ann left | Still "Ann - Ally": the label was fixed when Ann's row was made |
| MS-V3. Chat-room game, L = slot 0 "Ann", one human slot 1 "Bob" (enemy) and a computer orc in slot 3 allied with Ann | "Ann", "Bob - Enemy", and slot 3's orc nation name "- Enemy" |
| MS-V4. Battle.net, L surrenders | Defeat background; then the Battle.net screens |
| MS-V5. Skip setting on | No score screen; straight to what follows |
| MS-V6. Chat-room game ended by Return to the chat room | No score screen |
| MS-V7. Classic mode chat-room game, L defeated | Defeat background, rows as above; then as MS-V1 |

## Open questions

- **Row text and order.** Name first and L's row on top come from static study and differ from the single-player text
  of `game/campaign-flow.md` (which says "label - name", slot order). **Trace request MS1:** screenshot the score
  screen of a 2-player Direct Connect game and of a single-player custom game with one computer player.
- **Unused result texts.** gluAll entries 17 to 22 may be used through a path the static study missed. **Trace request
  MS2:** search a screenshot of a Battle.net-less multiplayer end for "Elapsed Time" (expected: absent).
- **After the score screen.** Held: `game/campaign-flow.md` and `ui/music.md` say "the multiplayer screens", and the
  2026-10-15 review ruled that static study alone doesn't change that; rounds 74 and 89 both read the main menu for a
  chat-room game. **Trace request MS3:** in a 2-player Direct Connect game, finish with a victory and record the
  screen (and the music) after the score screen. If MS3 shows the main menu, `game/campaign-flow.md`, `ui/music.md`
  and the table above change together.
- **Computer rows.** Computer players are always labelled Enemy, even when allied with L (static study).
  Covered by MS1 if the custom game's computer is set as an ally; otherwise default as stated.
