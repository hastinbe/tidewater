# UI: game setup lobby and Custom Scenario setup

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 89, B-057). The review's rulings are in
  `ERRATA-round89-multiplayer.md`, "Review".
- **Sources:** oracle observation of the original game's multiplayer menus (traced (oracle menu driver):
  `oracle/multiplayer.py`, and its round-89 menu screenshots, kept on the research side); inspection of the dialog files and string
  tables (`rez\loadcstm.bin`, `rez\chat_p1.bin`, `rez\chat_p2.bin`, `rez\chat_mc.bin`, `rez\chat_mj.bin`,
  `rez\gamename.bin`, `rez\filefltr.bin`, `rez\allyfltr.bin`, `rez\fileflt2.tbl`, `rez\chatroom.tbl`,
  `rez\network.tbl`, `rez\gluAll.tbl`); study of the program's lobby code. Each section says whether it is
  **traced**, **static study** or a **Gap-fill:**. Rules: Enhanced first; findings that apply only to Classic rules are
  labelled "Classic mode". The screens before the lobby (menus, name, Connect) are in `specs/ui/multiplayer-menus.md`.
  How the setup turns the lobby's choices into players is in `specs/game/setup.md`. The turn length and latency
  rows belong to `specs/net/lockstep.md`.

## Bottom line

One setup dialog serves three jobs: the single-player **Custom Scenario** screen, the multiplayer **creator's** lobby
and the multiplayer **joiner's** lobby. They share the layout (a column of eight player rows on top, six option
boxes in the middle, a scenario box and three buttons at the bottom). Only the creator can pick the map, change rows
and options, and start. Tidewater builds the lobby from its own dialog files; Enhanced rules show no Ready box, and
Classic mode shows one.

## Behaviour

### Which dialog is shown (static study; traced for the Enhanced and Classic pairs)

| Situation | Enhanced | Classic mode |
| --- | --- | --- |
| Single-player Custom Scenario | `rez\loadcstm.bin` (title "Custom Game Setup") | same file |
| Multiplayer, game creator | `rez\chat_p1.bin` | `rez\chat_mc.bin` |
| Multiplayer, joiner | `rez\chat_p2.bin` | `rez\chat_mj.bin` |

All titles in the multiplayer files read "Multi Player Setup". `loadcstm.bin` and the creator files (`chat_mc`,
`chat_p1`) come from the install data; `chat_mj`, `chat_p2`, `gamename.bin`, `filefltr.bin` and `allyfltr.bin` come
from the base data folder (`War2Dat`). The patch folder (`War2Patch`) holds none of these, so the patch never changes
the lobby. Which file a screen uses depends on two flags only: creator or joiner, and Enhanced or Classic rules.
The same dialog serves Direct Connect and Local Network (IPX in the original); there is no separate lobby per
connection type. The Battle.net game-creation screens are a different set of dialogs and are not covered here
(Tidewater shows Battle.net disabled).

Supporting dialogs: `gamename.bin` ("Enter your name:", a one-line edit box with OK and Cancel; used before the lobby,
see `specs/ui/multiplayer-menus.md`), `filefltr.bin` (the "Select scenario" dialog below), `allyfltr.bin` (titled
"CHOOSE YOUR ALLIES", an 8-line list ending with "Allied Victory"; shown from the in-game menu, not from the lobby,
so it is out of scope here).

### Layout (traced: dialog files; screenshots)

The screen is 640 x 480. Coordinates are (left,top)-(right,bottom).

| Part | Position | Notes |
| --- | --- | --- |
| Title | (160,8)-(479,27) | "Multi Player Setup", or "Custom Game Setup" at (160,220)-(479,239) in the single-player file |
| Row name | 156 x 18 at x 36, rows 1 to 8 at y 48, 68, 92, 116, 140, 164, 188, 212 | Row 1 is plain text (the creator's name). Rows 2 to 8 are drop-down boxes |
| Row chat line | 388 x 18 at x 200, y 48, 72, 96 ... 216 | One text line per row; see "Lobby chat" |
| Ready box | 18 x 20 at x 10, rows 2 to 8 (y 72, 96 ... 216) | Rows 2 to 8 only. The creator's row has none |
| Two status icons per row | 18 x 18 at x 596 and x 616, y 48 ... 216 | See Open questions |
| "Ready (Each player must click Ready to start.)" | (4,26)-(483,39) | Shown in Classic mode only (traced: Classic screenshots show it, Enhanced screenshots do not) |
| Your Race | label (40,256), drop-down (44,274)-(167,297) | |
| Fog of War | label (40,306), drop-down (44,324)-(167,347) | Multiplayer only. The single-player file has "Opponents:" in this place |
| Third left box | label (40,356), (44,374) | Classic mode: "Cheat Codes:" drop-down. Enhanced: "Game Speed:" slider with two arrow buttons. Single-player file: none |
| Resources | label (220,256), drop-down (224,274)-(367,297) | |
| Map Tileset | label (220,306), drop-down (224,324)-(367,347) | |
| Location | label (220,356), drop-down (224,374)-(367,397) | Multiplayer only. The single-player file has the control but it is hidden and has no label |
| Units | label (400,256), drop-down (404,274)-(583,297) | |
| Scenario | label (16,402), text (8,424)-(387,475) | Three lines: the type, the map name, then the description |
| Buttons | 224 x 28 at x 400: "Select Scenario" y 368, "Start Game" y 404, "Cancel Game" y 440 | The hot keys are E (Select Scenario), S (Start Game) and C (Cancel Game). The Start button is hidden or grey when Start is not allowed |

### Single-player Custom Scenario screen (traced: dialog file, setup rules; static study for the details below)

The screen opens from the single-player menu (`snglplay.bin`: New Campaign, Load Game, Custom Scenario, Previous
Menu). It is the same dialog family with these differences:

- **Your Race:** Human, Orc or "Map Default" (the map's race stays).
- **Opponents:** "Map Default", then "1 Opponent" to "7 Opponents". "Map Default" means the number of computer
  slots in the map, at least 1 (`specs/game/setup.md`, "Custom game", step 3).
- **Resources:** Map Default, Low, Medium, High. Enhanced adds Random (`specs/game/setup.md`, "Custom choices").
- **Map Tileset:** Map Default, Forest, Winter, Wasteland, Orc Swamp ("Swamp" in the setup spec). Enhanced adds Random.
- **Units:** Map Default or "One Peasant Only" ("one worker only" in the setup spec).
- **No** Fog of War box, **no** Game Speed or Cheat Codes box, **no** player rows and **no** chat. Fog stays on,
  and the Fixed Order start is not offered (the Location control is hidden in the dialog file, and the screen clears the
Fixed Order setting).
- **Select Scenario** opens the same "Select scenario" dialog as the multiplayer lobby. The default map is the map
  that was last picked in this session; with none, the first built-in scenario.
- **Start Game** needs a chosen map that has at least one slot for a human (owner code 5) and, for the
  single-player game, at least one computer slot or an opponents count of 1 or more. See "Start conditions".

### The Select scenario dialog (`filefltr.bin`; traced: screenshots and the oracle driver; static study for the filters)

The dialog is the same in the single-player screen and the creator's lobby. Joiners never open it.

| Control | Contents |
| --- | --- |
| Type | "Built-in scenario", "Custom scenario", "Saved game". Single-player screen: the first two only |
| Players | "2 to 8 players", "2 to 4 players", "5 to 8 players", then "1 player" to "8 players" |
| Map size | "Any size", "32 x 32", "64 x 64", "96 x 96", "128 x 128" |
| Folder box | Shows "Maps" for custom scenarios; hidden for built-in scenarios |
| List | The scenarios that pass the filters, 6 rows visible, with a scroll bar |
| Info lines (under the list) | The map's description, then "128 x 128", then "8 players" (traced: screenshot) |
| Buttons | Cancel (left), OK (right) |

**Built-in scenario.** The list shows the built-in multiplayer maps; the displayed name is the description text (for
example "Gold separates east from west"). The Type list's first line is also what the Scenario box shows
("Built-in scenario", then the name).

**Custom scenario.** The list shows every `.pud` file in the game's `Maps` folder, by file name without the
extension (traced: "All You Need BNE", "Ant Trails BNE", "Big Rock Candy Mountain BNE", "Cramped BNE", "Crosshair BNE",
"Dark Paths BNE"; the Scenario box then shows "Custom scenario" and "Dark Paths BNE.pud"). The list is sorted
alphabetically, ignoring case (static study). A file that fails the header check is left out (static study).

**Saved game.** Saved games from the game's save folder. In the multiplayer lobby, a saved game is accepted only if
its player count matches the lobby: too few humans gives network entry 16 ("Not enough players to load saved game.")
and too many gives network entry 17 ("Too many players to load saved game."). Static study only; saved-game lobbies
are not traced. Gap-fill: Tidewater's lobby offers Saved game only if saved multiplayer games exist
(`specs/ui/multiplayer-menus.md` decides whether a Load route is offered at all).

**Filters.** The Players and Map size choices filter the list live (static study). A map passes the Players filter
when its number of playable slots (owner codes 4 and 5, slots 0 to 7) falls in the chosen range. A map passes the Map
size filter when its square size equals the choice. Changing any of Type, Players or Map size rebuilds the list and
selects its first line. The info lines show the first line's data straight away, and again for each line the player
clicks or moves to with the keys (whether the arrow keys move a list's selection is held: `ui/multiplayer-menus.md`,
trace request MM8).

**Minimap preview.** The Select scenario dialog shows **no** minimap; it has only the description and the two info
lines above (traced: screenshot). Gap-fill: Tidewater shows no preview either. The lobby has none (traced: screenshots).

### Rows: players, computers, closed slots (traced for Enhanced; static study for the rest)

Eight rows, row 1 to row 8. Row 1 is always the creator. Rows 2 to 8 each hold one of:

| State | Row label (chatroom table entry) | Meaning |
| --- | --- | --- |
| Available | entry 6 "Available" | Empty and open: a joiner can take it |
| Computer | entry 7 "Computer" | A computer opponent |
| Closed | entry 8 "Closed" | Nobody can join |
| Human | the player's name | A joined player. The creator's drop-down shows the name as the row's first line |

Rows 2 to 8 are only changed by the **creator**: clicking a row's drop-down offers **Available**, **Computer**,
**Closed**, and, when the row holds a human, **Kick Out** (chatroom entry 10). Choosing Kick Out sends that player
back with network entry 8 ("You were kicked out by the game creator.") and the row becomes Available (static study;
the kick itself is not traced). Joiners' rows are display-only. A joiner cannot change any row, including their own
(except for the race and chat line, below).

Rules for rows (static study):

- A row set to Computer or Closed cannot be taken by a joiner. A joiner who connects when no row is Available is
  refused with network entry 6 ("The requested game is full.").
- Changing a row that holds a human to Computer, Closed or Available also kicks that player, with the same message.
- The number of **humans** that can start is limited by the CD rule: the humans in rows 1 to 8 may not outnumber
  three times the humans whose copy carries a CD owner flag. When the rule fails, the lobby shows chatroom entry 34
  ("To play a multiplayer game you need: One CD for up to 3 players, Two CDs for up to 6 players, Three ...").
  Gap-fill: Tidewater's copy has no CD flag, so every player counts as holding a CD and the rule never blocks.
- The chatroom entry 9 "Slot Use Error" has no visible trigger in the lobby paths traced. Gap-fill: Tidewater shows it
  nowhere unless a trace request below finds a trigger.

### Races and colours (traced for the race list; static study for the draw)

- **Your Race:** Human, Orc or Random in Enhanced; Classic mode lists the same three. The single-player screen also
  has "Map Default" in place of Random (see above). In the multiplayer lobby each player sets their own race.
  Default: Random (Enhanced screenshot: "Random"). Classic mode joiner screenshots showed "Human".
- **Random is resolved at game start**, not when picked: when the start message arrives, a player whose **own** race
  is Random resolves it on their own machine with a coin flip (Human or Orc) from that machine's clock, not from the
  game's generator, and sends the result back in their reply to the creator. The creator's final game information
  carries every player's resolved race, so every machine uses the same races. The resolved race becomes that player's
  race for the game (`specs/game/setup.md`: a lobby human keeps the lobby race unless that race is neutral). Static
  study. Tidewater: each player's Random race is decided once and shared through the start information, never
  resolved separately on each machine.
- **Colours and teams.** The lobby has **no** colour or team control. Each slot's colour comes from the map's slot
  numbers after a shuffle seeded by the game seed (static study). There is no teams row; allies are chosen in game
  (`allyfltr.bin`). Network entry 109 "Illegal number of teams." belongs to Battle.net setup and is never shown here.
- **Game types.** There are no game types (such as Melee or Top vs. Bottom) in this lobby. Gap-fill: Tidewater adds none.

### Options (traced: layout and read-back; static study for the choice lists)

All options are chosen by the **creator** (single-player: by the player). Joiners see the creator's current choices
(the boxes are disabled on a joiner's screen). A change is sent to the joiners at once. Joiners keep only their own
**Your Race** drop-down, which they change on their own screen.

| Option | Choices | Notes |
| --- | --- | --- |
| Resources | Map Default, Low, Medium, High; Enhanced adds Random | Effects in `specs/game/setup.md` |
| Units | Map Default, One Peasant Only | |
| Map Tileset | Map Default, Forest, Winter, Wasteland, Orc Swamp; Enhanced adds Random | Effects in `specs/game/setup.md` |
| Fog of War | On, Off | Default On |
| Location | Random, Fixed Order | Default Random. Fixed Order is the fixed-starts setting of `specs/game/setup.md`. Multiplayer only |
| Game Speed (Enhanced) | A slider with a left and a right arrow; its positions are the Enhanced speeds (Gap-fill: the slider's range wasn't decoded from the dialog file) | Sets the game speed the game starts with. See `specs/core/timing-and-rng.md` for what each position means. Default: the middle (traced: slider read-back after nine presses of the right arrow) |
| Cheat Codes (Classic mode) | Allowed, Disabled | Default Allowed (traced: Classic screenshot) |
| Cheats in Enhanced | Always disabled in multiplayer | The game information is marked "cheat codes disabled" as soon as an Enhanced lobby opens, so cheat codes do nothing (traced: lobby read-back) |
| Opponents (single-player only) | Map Default, 1 to 7 | |

The numbers of opponents 1 to 7 appear only in the single-player screen; in the lobby the number of computer
opponents is the number of rows set to Computer. **Resolved options.** The game receives the choices as set; "Map Default"
stays "Map Default" until `specs/game/setup.md` applies it.

### Host and guest powers (traced for the Ready box and Start; static study for the rest)

| Action | Creator | Joiner |
| --- | --- | --- |
| Pick the map (Select Scenario) | Yes | No (button disabled) |
| Change rows 2 to 8 | Yes | No |
| Change options | Yes | No |
| Change own race | Yes | Yes |
| Type chat | Yes | Yes |
| Click own Ready box (Classic mode only) | n/a | Yes |
| Start Game | Yes | No (the button is not shown) |
| Cancel Game | Closes the game; every joiner sees network entry 7 "Game was terminated by the game creator." | Leaves; the creator's row becomes Available again |

**Ready (Classic mode).** In Classic mode each joiner must click their Ready box (rows 2 to 8, at the left). The creator
needs no Ready click. In Enhanced rules the Ready box and its explanatory line are not shown, and the joiner's Ready
state is set at once when they join (traced (oracle menu driver): the read-back right after a join shows the joiner's
ready mark and the Computer row's ready mark already set). Changing the map or an option clears every Classic mode Ready box
(static study). Gap-fill: Tidewater's Classic mode follows this, without clearing the box of a joiner whose own race
changed.

### Lobby chat (traced: layout; static study for the behaviour)

There is no separate chat box. Each player types in **the edit line of their own row** (the wide 388 x 18 box to the
right of the name), and everyone else sees it appear live in that row. Pressing Enter sends and keeps the text there
until the player types again; the line holds as many characters as fit. The creator's line is on row 1. Chat is
public; there are no private messages in the lobby. Gap-fill: Tidewater clears a row's line when its owner starts typing a new message.

**Notices.** The lobby shows **no** join or leave text (static study): a row simply fills with the new player's name,
or goes back to Available. Network entries 110 "%s has joined the game.", 111 "%s has left the game." and 112 "%s was
dropped from the game." are for the Battle.net layer and in-game notices, not for this lobby. Network entries 106 to
108 ("Player %s set network for low / high / extra high latency.") are printed during a **Battle.net game**, when a
player changes the Network option of the Battle.net in-game Options menu (`specs/net/lockstep.md` owns the setting). The lobby has no latency
control. Gap-fill: the starting latency row comes from the connection's own suggestion or the command line, never from
the lobby.

### Start conditions and errors (static study unless noted)

The **Start Game** button exists on the creator's screen only. It is **enabled** when all of these hold:

1. A map is chosen. Without one the button stays grey.
2. Rows left Available are ignored: the game starts with the rows that are Human or Computer. Gap-fill: Tidewater
   does the same.
3. The map has a slot for a human. The message "This map does not have a slot for a human participant" belongs to
   the Battle.net screens (gluAll table entry 54 by 1-based count); the Select scenario list of this dialog does not
   show such a map in the multiplayer lobby (static study; see the open question at the end).
4. Classic mode only: every joined human (rows 2 to 8) has clicked Ready.
5. The CD rule above holds.

The error strings of the Battle.net glue ("You must have at least one computer opponent", the slot errors) are never
reachable from this lobby or from the single-player screen (static study). Gap-fill: Tidewater's lobby never shows them;
a game with one human and no computer starts as a one-player game only after the check in `specs/game/setup.md`
passes. A map file that fails to load at start shows network entry 23 ("An error occurred while trying to load
file"); a map that has too many players for the lobby shows network entry 21 ("Too many players to load map"). Other
joiner refusals: network entry 6 (game full), network entry 20 ("Player %s has a map which does not match."), entry
19 ("Player %s does not have the requested scenario."), entry 89 ("Duplicate name - pick another", a name already in
the game; `ui/multiplayer-menus.md`) and entry 124 ("Unable to send message.") (static study).

### What happens at Start (static study; traced for the seed and the placement)

1. **Validation.** The creator clicks Start; the checks above run again.
2. **Start message.** The creator sends every joiner the start message with the game's start seed. Each machine
   resolves its Random race when the message arrives.
3. **Replies.** Each joiner answers. The creator waits for all humans' replies.
4. **Game information.** The creator sends the final game information (map, rows, options, speed). The game's random
   seed is (start seed + 4) mod 256 (traced: round 57).
5. **Row to slot.** With Location Random, rows go to map slots by the draws in `specs/game/setup.md` ("Multiplayer
   game"); with **Fixed Order**, row i goes to slot i. The resolved options go to the setup as in
   `specs/game/setup.md`.
6. **Game start.** All machines load the map, then the first game cycle runs.

The game information the creator sends holds (traced: read-back after a join; static study for the rest): a
**ready** mark per row, the **Fixed Order** mark, the "cheat codes disabled" mark, the resources, units and tileset
choices, the map name, and each row's state (Available, Computer, Closed or Human).

## Data

### Control ids (the same in `chat_p1`, `chat_p2`, `chat_mc`, `chat_mj`)

| Controls | Ids |
| --- | --- |
| Buttons: Cancel Game, Start Game, Select Scenario | 1, 2, 3 |
| Drop-downs: Resources, Your Race, Location (Classic: also Cheat Codes), Map Tileset, Fog of War, Units | 4, 10, 5, 8, 7, 6 |
| Scenario text | 11 |
| Row chat lines, rows 1 to 8 | 12 to 19 |
| Row names, rows 1 to 8 | 20 to 27 |
| Ready boxes, rows 2 to 8 | 36 to 42 |
| Status icons, rows 1 to 8 | 28 to 35, and 43 to 50 |
| Explanatory "Ready" line | 51 (Enhanced files), unlabelled in the Classic ones |

(The single-player file `loadcstm.bin` uses the same ids for its race, opponents, resources, tileset, units and
button controls.)

### Strings (one term per concept)

| Source | Entries |
| --- | --- |
| `rez\chatroom.tbl` | 1 "Built-in scenario", 2 "Custom scenario", 3 "Saved game"; 4 "Built-in scenario", 5 "Saved game"; 6 "Available", 7 "Computer", 8 "Closed", 9 "Slot Use Error", 10 "Kick Out"; 11 "Map Default", 12 "Low", 13 "Medium", 14 "High"; 15 "On", 16 "Off"; 17 "Fixed Order", 18 "Random"; 19 "One Peasant Only"; 20 "Human", 21 "Orc"; 22 "Forest", 23 "Winter", 24 "Wasteland"; 25 "Disabled", 26 "Allowed"; 27 to 33 "1 Opponent" to "7 Opponents"; 34 the CD message; 35 "Orc Swamp" |
| `rez\fileflt2.tbl` | 1 to 5 type and header lines; 6 "Any size", 7 to 10 the four map sizes; 11 to 13 the player ranges; 14 to 21 "1 player" to "8 players"; then the built-in scenario descriptions ("Two player map", "Gold separates east from west", ...) |

The choice lists use these entries in this order. Gap-fill: Enhanced's extra "Random" for Resources and Map Tileset
sits as the last line of each list.

### Folders

The game reads the dialog files above from the install and base data folders only; scenarios come from the `Maps`
folder next to the game, saved games from the game's save folder, built-in scenarios from the game data.

## Test vectors

| Input | Expected |
| --- | --- |
| Enhanced creator opens the lobby with nobody joined | Row 1 shows the creator's name; rows 2 to 8 show "Available"; no Ready box or Ready line is shown; Start Game is shown (traced: screenshot) |
| Creator sets row 2 to Computer | Row 2 shows "Computer"; the game information holds Computer for that row and Available for the rest (static study; the combined read-back below is traced) |
| Enhanced joiner joins an open row, creator adds a computer in the next row | Row states Human (creator), Human (joiner), Computer, then Available; ready marks set for the joiner's and the computer's rows at once, cheat codes disabled; races: creator Random, joiner Human (traced (oracle menu driver)) |
| Location set to Fixed Order | The game information gains the Fixed Order mark; nothing else changes (traced (oracle menu driver)) |
| Game Speed slider, right arrow pressed nine times from the middle | Slider at the right end (traced (oracle menu driver)) |
| Classic creator, joiner joined, joiner has not clicked Ready | Start Game is grey (traced: Classic screenshot) |
| Classic joiner ticks Ready | Start Game becomes available |
| Select scenario, Type "Custom scenario", Players "2 to 8 players", size "Any size" | List shows the `.pud` files in `Maps` by name, alphabetically (traced: screenshot) |
| Select a custom scenario "Dark Paths BNE" | Scenario box reads "Custom scenario" and "Dark Paths BNE.pud" (traced: screenshot); the info lines show its description, "128 x 128", "8 players" |
| Single-player: Opponents "3 Opponents" | Setup places three computers (`specs/game/setup.md`, "Custom game") |
| Creator picks Kick Out on a joined row | Joiner sees "You were kicked out by the game creator."; the row becomes Available (static study) |
| Creator clicks Cancel Game | Joiner sees "Game was terminated by the game creator." (static study) |

## Open questions

Each of these needs a trace to be settled.

- **Trace request LB1: Enhanced Start rule.** Does Start work with the creator alone (no Computer, no human)? Static study
  reads "at least two participants" for Enhanced rules and "at least one computer or one other human" in some
  paths; click Start in the Enhanced lobby with all rows Available and record whether the game starts.
- **Trace request LB2: Classic Ready details.** Which events clear the Ready boxes (map pick, option change, row
  change), and does the creator's own state matter? Drive Classic with one joiner and read back the game
  information's ready marks after each change.
- **Trace request LB3: Kick Out and Slot Use Error.** Capture a Kick Out from a joined row and the text the kicked
  joiner sees; find any path that shows chatroom entry 9 "Slot Use Error" (try Closed on a human row, a joiner
  re-joining a Closed row).
- **Trace request LB4: Status icons.** What do the two icons per row at the right edge (ids 28 to 35 and 43 to 50,
  they showed a disc with a small number) mean? Probably the CD owner flag and a connection quality mark; trace by
  joining with a copy that has no CD flag, and with a slow link.
- **Trace request LB5: Lobby chat.** Type a long line on a joiner and on the creator; record where it appears and how
  long it stays, the maximum length, and whether the row keeps the text after the typist leaves.
- **Trace request LB6: Select scenario filters.** The exact Players/Map size filter rule (owner codes counted), and
  whether the list is truly case-insensitive alphabetical; use a Maps folder with mixed-case names and an odd-sized map.
- **Trace request LB7: Saved-game lobby.** Pick Saved game as the creator, with a save of N players and M joined
  humans: confirm network entries 16 and 17 and what a joiner sees.
- **Trace request LB8: Random race.** Confirm that Random resolves at start, and whether the creator's and joiners'
  resolved races agree (both Random in a two-human game, many starts).
- **Trace request LB9: Enhanced vs Classic Fog and Cheat.** Classic has Cheat Codes where Enhanced has Game Speed. Is
  there any fog or cheat setting reachable in Enhanced that the game information carries but the lobby doesn't show?
- **Trace request LB10: Battle.net-only errors.** Entries 53, 54, 97, 98, 99 and 158 of `rez\gluAll.tbl`. Static study found these reachable only from the Battle.net layer. Confirm none can
  appear in a Direct Connect or Local Network lobby (trace: try to start with no human slot map and with no
  computers).
