# UI: the multiplayer score screen

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 89, B-057). The review's rulings are in
  `ERRATA-round89-multiplayer.md`, "Review".
- **Sources:** static study of the 2.02 program (round 89; research notes for this round stay on the research side),
  the score dialogs `rez\msnstat2.bin`, `rez\msnstat4.bin`, `rez\msnstat6.bin` and `rez\msnstats.bin`, and the tables
  `Rez\network.tbl` and `rez\gluAll.tbl`. One Enhanced Direct Connect game was watched to its score screen and beyond
  (round 91, `mp-dark-paths-bne-mp-surrender-game-600`, screenshots of the creator after a victory); the rest is static
  study.
- **Changes:** 2026-10-21, round 91 (oracle trace, `ERRATA-round91-traces.md`; reviewed by a spec-review session,
  2026-10-21): row text and order traced for two rows (MS1, multiplayer half); no elapsed time (MS2); after the score
  screen the creator goes back to the lobby of the same session (MS3 for the creator after a victory; both static
  readings refuted). The outcome word is shown (traced), so "no result word" is corrected.
- **Changes:** 2026-10-21, round 92 (oracle trace, `ERRATA-round92-traces.md`; reviewed by a spec-review session,
  2026-10-21): a Direct Connect joiner leaves the score screen for the join screen ("Waiting for response") after a
  defeat by surrender (MS-V9; MS3b narrowed).

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

Each row is one text line, **name first** (static study, round 89; traced, round 91: "Anonymous" for L and
"Anonymous - Enemy" for the other human, MS-V8). The single-player score screen keeps its stated "label - name" text
and slot order (`game/campaign-flow.md`) until the single-player half of MS1 settles both: round 89 reads the same
row text and order for single player, which the review held.

| Player | Row reads |
| --- | --- |
| L | L's player name, alone |
| Another human player, allied with L (L's alliance toward that player) when the row is made | "*name* - Ally" (network entry 36) |
| Another human player, not allied with L | "*name* - Enemy" (network entry 35) |
| A computer player | "*nation name* - Enemy", always Enemy, whatever the alliances; the nation name is the `network.tbl` entry by slot and race of `game/campaign-flow.md` |

The "You" label (network entry 34) is not used in multiplayer: L's row is just the name. The separator is " - " (space,
hyphen, space).

### Row order

**L's row first** (the dialog's first row), then the other rows in slot order, 0 to 7, skipping L (static study;
L's row on top traced, round 91, with L in slot 1 and the other human in slot 0, MS-V8). The
dialog is chosen by the number of rows as in single player (2, 4, 6 or 8 rows).

### Statistics, score and rank

- **Statistics** for each row are taken when the screen opens, as in single player: units and buildings alive now plus
  lost, resources gathered, kills and razings (static study). A player who left still has its counts: its units and
  buildings stay on the map as "nobody" (`net/leave-and-drop.md`).
- **Score and rank.** The campaign total is 0 at the start of every multiplayer game, so the shown score and the rank
  come from L's mission score of this game, plus 500 for a victory (static study; `game/campaign-flow.md`, "Campaign
  total"). The cheat rank (entry 39) applies as in single player, but cheats are off in an Enhanced multiplayer game.
- **No elapsed time.** The screen shows no game time (traced, round 91, MS-V8: the outcome, rank and score fields and
  the seven statistic columns only). gluAll entry 17 "Elapsed Time: %s" was not found used (static study).
- **The outcome word.** The dialog's "Outcome" field shows the outcome word as in single player: "Victory!" after the
  traced victory (round 91, MS-V8). gluAll entries 18 to 22 ("Defeat!", "Victory!", "Draw!", "Disconnect",
  "Undecided") were not found used (static study), so where the shown word comes from is open; default: "Victory!"
  or "Defeat!" by L's outcome, as in single player. Never "Draw!", "Disconnect" or "Undecided" (gap-fill).

### What follows the score screen

| Game | After the score screen |
| --- | --- |
| Battle.net game | back to the Battle.net screens (static study) |
| Chat-room game (Direct Connect, Modem, IPX network; Enhanced and Classic mode) | **The game creator goes back to the lobby** ("Multi Player Setup", `ui/game-setup-lobby.md`) of the same session: its own row, the other seven slots open ("Available"), the chat line, Start Game greyed out (traced after a victory, round 91, MS-V8; it stays there). Both static readings (the first multiplayer screen; the main menu) are refuted for this case. **A joining player** (traced after a defeat by surrender, round 92, MS-V9): Direct Connect's join screen, "Waiting for response" with a Cancel Join button, while the creator plays on; whether it enters the session's lobby once the creator is back there isn't traced (default: it does, as a joiner). The joiner after a victory and the creator after a defeat: not traced; default as above |

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

Static study unless marked traced. Enhanced unless labelled.

| Case | Expected rows (in order) |
| --- | --- |
| MS-V1. Chat-room game, L = slot 1 "Bob", slot 0 "Ann" allied with Bob, slot 2 "Cy" not allied; Bob wins after both leave | "Bob", "Ann - Ally", "Cy - Enemy"; victory background; then the lobby of the same session if Bob created the game (traced for the creator, MS-V8; a joiner by default the same) |
| MS-V2. As MS-V1, but Bob broke the alliance with Ann after Ann left | Still "Ann - Ally": the label was fixed when Ann's row was made |
| MS-V3. Chat-room game, L = slot 0 "Ann", one human slot 1 "Bob" (enemy) and a computer orc in slot 3 allied with Ann | "Ann", "Bob - Enemy", and slot 3's orc nation name "- Enemy" |
| MS-V4. Battle.net, L surrenders | Defeat background; then the Battle.net screens |
| MS-V5. Skip setting on | No score screen; straight to what follows |
| MS-V6. Chat-room game ended by Return to the chat room | No score screen |
| MS-V7. Classic mode chat-room game, L defeated | Defeat background, rows as above; then as MS-V1 (default) |
| MS-V8. `mp-dark-paths-bne-mp-surrender-game-600` (oracle trace, Enhanced, Direct Connect, no computer player): L (the creator, slot 1, "Anonymous") wins at the check after the other human (slot 0, "Anonymous") surrenders | Rows "Anonymous" (L, on top, L's colour) then "Anonymous - Enemy"; "Outcome" shows "Victory!"; no elapsed time. After Continue: the "Multi Player Setup" lobby with L's row, seven "Available" slots and Start Game greyed out, still there 15 s later |
| MS-V9. `mp-dark-paths-bne-mp-surrender-joiner-game-650` (oracle trace, Enhanced, Direct Connect, one computer player): the joiner (slot 0, "Anonymous") surrenders between n = 120 and 150; the creator plays on against the computer | The joiner's screens: n = 200 the defeat picture with no text; n = 260 "Outcome" with "Defeat!" drawn over it; n = 320 the score screen with rank "Slave", score 0 and Continue; by n = 380 also the rows "Anonymous", "Anonymous - Enemy", "Nation of Kul Tiras - Enemy" and the first statistic ("Units"). After Continue (n = 390): Direct Connect's join screen, "Waiting for response", Cancel Join, from n = 440 to at least 600. The creator shows "Player Anonymous left the game." |

## Open questions

- **Row text and order, single player.** The multiplayer half is traced (MS-V8). Whether a single-player score screen
  also reads "name - label" with L on top, against `game/campaign-flow.md` ("label - name", slot order), is still
  held. **Trace request MS1 (single-player half):** screenshot a single-player custom game's score screen with one
  computer player.
- **The outcome word's source.** See "The outcome word": shown, but not from gluAll entries 18 to 22 as far as static
  study found. Default as stated. (MS2, elapsed time, closed: absent, traced.)
- **After the score screen, other cases.** The creator after a victory (MS-V8) and the joiner after a defeat by
  surrender (MS-V9) are traced. **Trace request MS3b (narrowed):** the creator after a defeat, the joiner after a
  victory, and whether a waiting joiner enters the lobby once the creator is back there, in a 2-player Direct Connect
  game; and the music in the lobby after the score screen (the oracle records no sound).
- **Computer rows.** Computer players are always labelled Enemy, even when allied with L (static study).
  Covered by MS1 if the custom game's computer is set as an ally; otherwise default as stated.
