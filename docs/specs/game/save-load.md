# Game: saving and loading games

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 29 (save and load;
  gap 9)"), the dialog files `Rez\savegame.bin`, `Rez\loadgame.bin` and `Rez\loadgamt.bin`, and `Rez\network.tbl`
  (entries cited by number, from 1), and the round-90 oracle traces with save and load (named at each rule).
- **Changes:** 2026-10-21, round 88 (B-038 consolidation of the game area, `ERRATA-game-consolidation.md`, reviewed by a
  spec-review session, 2026-10-21): a loaded game runs the two start-up cycles (X3, static study); the cheat-used mark
  isn't saved, and the campaign total and rescued counts are (X2, X7, X12, static study). Four static findings that
  contradict this spec are held as open questions with the old text as the default: the cycle count starting again at 0
  (Q2), the mission start running again (Q3), the victory countdown set to 1 with a found outcome lost (Q4, split from
  X3 by the review) and a load keeping the cheat-used mark (Q5, split from X2 by the review, in `game/cheats.md`). The
  comparison of a new and a loaded game start is in `game/lifecycle.md`.

- **Changes:** 2026-10-21, round 89 (B-057 multiplayer, `ERRATA-round89-multiplayer.md`; reviewed by a spec-review
  session, 2026-10-21): "What a saved game keeps" lists the players' results, dropped marks, the Battle.net tie mark
  and the slot controllers (static study).
- **Changes:** 2026-10-21, round 90 (oracle traces with save and load, `ERRATA-round90-traces.md`; reviewed by a
  spec-review session, 2026-10-21, see its "Review"): three held questions are settled by traces. A loaded game counts
  its cycles from 0 again (Q2), sets the victory countdown as a new game does and loses an outcome found but not yet
  shown (Q4), and leaves the cheat-used mark as it was in the running program (Q5). The fog refresh countdown isn't
  saved (`ERRATA-audit-game.md` erratum 3, traced), a loaded game is never paused (erratum 6, static gap-fill), and
  every unit's creation number is renumbered (traced observation). The mission start after a load (Q3) stays held:
  the round-90 trace can't tell the two readings apart (see "Open questions").
- **Changes:** 2026-10-21, round 91 (oracle trace, `ERRATA-round91-traces.md`; reviewed by a spec-review session,
  2026-10-21, see its "Review"): Q3 closed: a load runs the mission start of `game/victory.md` again (traced for mode
  AB, `game/victory.md` G24), with the goal rule for repeated conversions (`ERRATA-audit-game.md` erratum 2).
- **Changes:** 2026-10-21, round 92 (oracle traces, `ERRATA-round92-traces.md`; reviewed by a spec-review session,
  2026-10-21, see its "Review"): the computer players' 50-cycle pass counters aren't saved and start at 1 after a
  load (SL1 to SL3, traced); the four traced training group counts are saved and then counted again, so they double
  (SL4 to SL6, traced; the other groups held); a unit's animation, script and pending order are kept and the frozen
  worker marks cleared (static study); the next creation number after a load is 1626 in Enhanced (SL7, traced; open
  question closed).
- **Changes:** 2026-10-21, round 93 (oracle traces, `ERRATA-round93-traces.md`; reviewed by a spec-review session,
  2026-10-21, see its "Review"): a load clears the computer players' research requests and makes their build-list done
  marks again, with the load's building marks (SL9 to SL12, traced; parts static study); the mages, gryphons and
  destroyers counts double too, and the worker and tanker counts don't (SL13 to SL15, traced); the flying machine,
  transport, battleship and submarine counts double by a labelled gap-fill (static study). The SL8 question is
  narrowed to those four groups.

## Behaviour

This spec covers what a player can save, where it goes, the save and load dialogs, who may save in a multiplayer game,
and what a loaded game keeps or starts afresh. It describes behaviour, not a file layout: Tidewater writes and reads
saved games in its own format and doesn't read the original game's saved games.

The game menu buttons that open these dialogs, and their keys, are in `ui/game-menu.md`.

### Where saved games live

- Saved games are files in a folder named `Save` inside the game's folder.
- The extension depends on the kind of game:

  | Game | Extension |
  | --- | --- |
  | Single player | `.sav` |
  | Multiplayer over a network session (for example Battle.net) | `.mlt` |
  | Other multiplayer games | `.net` |

- Each dialog lists only files with the extension for the current kind of game. A file is listed only if it is a
  valid saved game that this mode can load (see "Which files can be loaded").
- At most **100** saved games of one kind take part. The 100-file limit is checked when saving a new name.

### The saved-game list

Both dialogs show the same list:

- Each entry shows the file name without the folder and without the extension. The name typed when saving is stored
  inside the file too, but the list shows the file name.
- Entries are sorted by the time the game was saved, newest first. Files saved in the same second keep the order in
  which the folder listing returned them.
- The dialog's picture is `Art\h192x128.bmp` for a human player and `Art\o192x128.bmp` for an orc player, when the
  dialog is opened during a game.

### Who may save or load

Before either dialog opens:

- **Single player:** always allowed.
- **Multiplayer:** only one player may save, load or restart. It's the lowest-numbered player still in the game. If
  the local player is someone else, notice entry 28 appears, filled in with that player's name, and the dialog doesn't
  open.
- **Battle.net:** after a save, nobody may open the save dialog again for **120 seconds** of real time. Trying earlier
  shows notice entry 29 instead. The wait starts when the save is sent. There's no wait at the start of a game.

### Save dialog

The save dialog (`savegame.bin`) has the list, a name field and the buttons Save, Delete and Cancel.

- **Typing a name.** The name can hold up to 27 characters. Each character must be a printable character from space
  up to code 127, and not one of the characters in network entry 105 (characters a file name can't contain). A space
  can't be the first character. Characters that break these rules are ignored as they are typed.
- **Choosing a list entry** copies its name into the name field and enables Delete. Typing in the field clears the list
  choice and disables Delete.
- **Save** is enabled only while the name field isn't empty.
- **Delete** asks for confirmation with network entry 25, filled in with the entry's name. OK deletes the file. Either
  answer returns to the save dialog, where the list is read again.
- **Cancel** closes the dialog.

**When Save is pressed:**

1. Spaces at the end of the name are removed.
2. The name is compared with the names in the list. The comparison is exact: letter case matters.
3. If the name matches an entry, the game asks for confirmation with network entry 24, filled in with that name.
   OK saves over that file. What the other answer does isn't traced (see "Open questions").
4. If the name matches no entry and the list already has 100 entries, notice entry 26 appears and nothing is saved.
5. Otherwise the game is saved to a new file.

**The file name** is the typed name plus the extension. One case is different: if the name equals one of the reserved
device names `nul`, `con`, `prn`, `aux`, `com1` to `com9` or `lpt1` to `lpt9` (letter case ignored), the file name is
that device name in lower case followed by `_`. For example, `CON` is saved as `con_.sav`.

**Single player** saves at once. On success the dialog closes and the game goes on. On failure:

- a partly written file is deleted;
- a message shows the reason, network entry 22 and the file name (reasons are in "Data");
- its OK returns to the save dialog.

**Multiplayer** saving is described under "Saving in multiplayer". The dialog closes as soon as the save is sent.

### Load dialog

The load dialog has the list and the buttons Load and Cancel. During a game it's `loadgame.bin`; from the main menu
it's `loadgamt.bin`. It can't be opened in a network session: the game menu there has no Load button.

- **Load** loads the chosen list entry. When the button is enabled isn't traced (see "Open questions").
- **Load in single player:** the chosen file is opened and checked. If it's valid, the current game ends and the
  saved game starts. A loaded campaign game skips the act screen and the briefing (`game/campaign-flow.md`). If it
  isn't valid, a message shows the reason, network entry 23 and the file name, and its OK closes the message.
  If there isn't enough memory, a fixed out-of-memory message appears instead.
- **Load in a multiplayer game without a network session:** the load goes out as a command in the game's command
  stream, like a save. What each player does with it belongs to multiplayer loading (see "Open questions").
- **Cancel** closes the dialog.

### Which files can be loaded

A saved game records which rules it was saved under:

- **Enhanced** rules (all single-player games and Enhanced multiplayer games);
- **Classic** multiplayer rules.

A file can be listed and loaded only under the same rules it was saved under. A file whose contents aren't a
recognisable saved game is not listed.

Tidewater lists and loads every saved game it wrote, including Classic multiplayer games in the swamp era.

**Note: the original's swamp quirk.** In the original, a Classic multiplayer game played in the swamp era is saved
with a marker that no version of the dialog accepts. Such a save is written but never listed, so it can't be loaded,
and its slot looks free, so a later save can overwrite it without asking. (The same marker is used for Classic games
of the expansion campaign, but those are never Classic in practice.) Tidewater doesn't copy this.

### Saving in multiplayer

In a multiplayer game every player saves their own copy, in their own `Save` folder, under the same name.

- **Without a network session**, the save is a command in the game's command stream (`core/timing-and-rng.md`), so
  every player saves at the same game cycle.
- **In a network session**, the save goes out as a session message. Each player saves when it arrives, but only if it
  came from the player allowed to save (see "Who may save or load").

When a player saves from a multiplayer save request, the file is chosen as follows:

1. A saved game of this kind whose stored name equals the requested name (letter case matters) is overwritten.
2. Otherwise the first free slot of the 100 is used.
3. If all 100 are taken, the oldest saved game is replaced, or nothing is saved (see open questions).

If a save fails, the player who failed sees the same error message as in single player.

In an Enhanced game in a network session, saving draws one value from the game's simulation generator on each
machine that saves (`core/timing-and-rng.md`), so a save changes the rest of the game's random sequence. It stays the
same for everyone only if every machine saves at the same game cycle (see "Session save timing" under "Open
questions"). Other saves make no draw.

### What a saved game keeps

A saved game keeps everything that decides how the game goes on, apart from what "What a loaded game starts afresh"
lists, including:

- the map, its terrain and its explored and seen state;
- every unit and building, with their orders, and missiles and corpses. A unit's position, order and pending order,
  target, animation, its place in its animation script with the wait there, and its cached route are kept exactly, so
  a moving unit resumes mid-step (every traced field matched the saved state in the four round-92 loads). The frozen
  and about-to-freeze marks of workers (`game/rescue.md`, "Frozen units"; only a multiplayer network command sets
  them) are cleared, so no unit is frozen after a load (static study, round 92; a labelled gap-fill, untraced);
- each computer player's **training group counts**, the groups from footmen to submarines of `ai/training-research.md`,
  "Training buildings" (not the worker and tanker counts). They are restored as saved, and then the load counts every
  loaded unit again as the map start does: each unit the count includes (complete, not held, not dying) adds 1. So after
  a load each of these counts is its saved value plus the number of such loaded units in its group: twice the saved
  value when the saved count was right. Nothing corrects the surplus later: the count goes on rising and falling with
  the units from there, and a further save and load adds the loaded units again (static study; the trace shows the
  counts unchanged to n = 60, with no unit of the group made or lost). Traced, round 92 (SL4 to SL6): player 6 of
  `xhuman9-xhuman9-r92-aijobs-aijobs-game-60` saved with footmen 3, archers 2, siege 0, knights 1, loaded with 6, 4, 0,
  2, unchanged to n = 60; its second ogre was dying at the save and isn't counted again (knights 1 + 1, not 1 + 2). The
  building tests of the same rules (a complete lumber mill, blacksmith, stables) are worked out afresh and come back
  right. Traced, round 93: the mages (SL13), gryphons (SL15) and destroyers (SL14) counts double the same way; the
  destroyers count leaves out the held map-placed ships, as every group count does, so only the trained ones are counted
  again. The worker and tanker counts are worked out afresh and come back right (SL13 to SL15). The flying machine,
  transport, battleship and submarine counts aren't traced; they double too (static study: they are saved and counted
  again with the traced groups; a labelled gap-fill, see "Open questions");
- each player's resources, upgrades, spells and allowances;
- alliances (Enhanced only);
- the game clock (Enhanced only). The cycle count is written but not kept: see "What a loaded game starts afresh";
- each player's pause count (Enhanced only);
- each player's score counters (Enhanced only);
- the campaign total (`game/campaign-flow.md`, "Campaign total"; static study, round 88);
- each player's rescued count (`game/rescue.md`; static study, round 88);
- which colour cycles are on (`ui/palette-effects.md`);
- the music track (`ui/music.md`);
- the screen's map position;
- each player's result, which players were dropped, the Battle.net tie mark, and each slot's controller (a player who
  left is saved as "nobody") (static study, round 89; `net/leave-and-drop.md`, "A save made after a leave").

### What a loaded game starts afresh

- **Random numbers.** The generator's state isn't saved. A loaded game seeds it the same way as a new game
  (`core/timing-and-rng.md`). Loading the same file twice gives different random sequences (in single player,
  unless both loads start in the same second).
- **Cheats.** Cheat flags aren't saved. A loaded game starts with the flags that carry across games, as for a new
  game (`game/cheats.md`, "Flags across games"). The map-reveal cheat is never kept. The cheat-used mark that gives
  rank 39 (`game/campaign-flow.md`, "The score screen") isn't saved either (static study, round 88), and a load
  doesn't clear it: it stays as it was in the running program, so a cheat used earlier in the same program run still
  counts after loading a save made without one (traced, round 90: `human1-human01-cheat-load-game-50`, the mark is 0
  in the save, 1 from a cheat after it, and still 1 in the loaded game's setup record).
- **Start-up cycles.** A loaded game starts like any game: the two start-up cycles run (`core/timing-and-rng.md`,
  "Scheduler"; static study, round 88; traced, round 90).
- **Cycle count.** A loaded Enhanced game counts its cycles from 0 again: after its two start-up cycles the counter is
  0, as in a new game, whatever it was when the game was saved (traced, round 90: `human1-human01-saveload-game-300`,
  saved at counter 342; the loaded game's first normal cycle ends with the counter at 1). Only the network sync check
  and saving read the count.
- **Victory countdown.** The countdown starts as in a new game (1, so the first check after a load runs in the second
  start-up cycle, then at counter 201, 402, …), whatever it was when the game was saved. An outcome a check had found
  but not yet shown is lost; a later check finds it again only if the game state still gives it, which an
  instant-cheat outcome doesn't, since the cheat flags aren't saved. The check's rule comes from the saved mission
  rule, which keeps any switch made before the save (`game/victory.md`, "Outcome timing"). Traced, round 90:
  `human1-human01-saveload-game-300` (saved with the countdown at 59; 1 in the loaded game's setup record, then as a
  new game) and `human1-human01-unite-saveload-game-650` (no victory after the load; see G07 in `game/lifecycle.md`).
- **Fog refresh countdown.** It isn't saved: a loaded game refreshes the fog in its first start-up cycle, as a new game
  does, then every 100 cycles (`core/timing-and-rng.md`, phase 3; traced, round 90: `human1-human01-saveload-game-300`,
  saved with the countdown at 57, refreshes in the loaded game's first start-up cycle).
- **Computer-player pass counters.** The two counters of the 50-cycle passes (`ai/attack-ai.md`, "Timing") aren't
  saved. A loaded game starts both at 1, as a new game does, so pass A, the no-workers pass and pass B run in the
  loaded game's first start-up cycle and then every 50 cycles, whatever phase the saved game was in. A computer unit
  moving to its goal then gets its move again from pass B; it is already moving, so the move becomes its pending
  order, which takes over at its next decision with the still animation and wait 3 (`orders/orders.md`). Traced,
  round 92: SL1 to SL3.
- **Computer research requests and build-list done marks.** A load doesn't keep them. In the loaded game's setup
  every research request of every player is clear (`ai/training-research.md`, "Research requests and done marks"),
  and each computer player's done marks (`ai/worker-ai.md`, "Done marks") are made again, in this order:
  1. the game-start marks for what the player already has: upgrade levels, researched items and known spells;
  2. then the marks of each complete building of the player, the **load's building marks**, which have special
     cases the creation marks don't have:

     | Building | Marks |
     | --- | --- |
     | Guard tower or cannon tower (0x60 to 0x63) | the first open scout-tower entry (0x40) |
     | Keep or stronghold (0x58, 0x59) | every keep-upgrade entry (0x98), then the first open town-hall entry (0x4A) |
     | Castle or fortress (0x5A, 0x5B) | every 0x98 and castle-upgrade (0x99) entry, then the first open 0x4A entry |
     | Any other building | the first open entry of its type with the race bit cleared (an orc barracks, 0x3D, marks a 0x3C entry) |

  Loaded units aren't created again, so they make no creation marks. A research or upgrade still running at the save
  has no level or flag yet, so its entry isn't marked, although its start had marked it in the saved game. The
  research carries on and finishes as saved, and nothing marks its entry afterwards (only a start or the game-start
  marks mark a research entry). When that item can't be started again (its level or flag is already reached), the
  entry stops both scans for the rest of the game, like any entry the player can't research. Traced, round 93 (SL9 to
  SL12): the cleared requests, the running research left unmarked, the towers' 0x40 marks and a fortress's 0x98 mark.
  Static study (labelled gap-fills, untraced): the 0x99 marks, the 0x4A mark after a keep or castle, that only
  complete buildings mark, and the game-start marks after a load (no traced player had a level, or a spell that marks
  an entry).
- **Pause.** A loaded game is never paused: the pause isn't saved, and every game start clears it, so a game saved
  while paused loads running (static study, `ERRATA-audit-game.md` erratum 6; a labelled gap-fill, untraced).
- **Creation numbers.** Every unit's creation number (`core/unit-slots.md`, "Creating a unit", step 3) is its slot
  number plus one after a load, whatever it was before; every other unit field is as saved (traced, round 90, in
  both loads checked: 12 units in `human1-human01-saveload-game-300` and 200 in
  `xhuman9-xhuman9-runestone-saveload-game-60`, for example slot 1547's number 53 became 1548; the same in every
  round-92 load). The number doesn't depend on which slots hold units: every slot, in use or free, is numbered.
  The next unit created after a load gets **1626** in Enhanced, then one more per created unit (traced, round 92:
  SL7). As a formula it is the unit-slot count plus 26 (1600 slots in Enhanced, `core/unit-slots.md`): after the
  slots, a load uses up 25 further numbers whatever the game holds (static study). The formula is a labelled
  gap-fill for Classic mode (600 slots, so 626), where no load has been traced.
- **Classic saves.** A Classic multiplayer save doesn't keep the cycle count, the clock, the pause counts or the score
  counters. The cycle count starts again at 0.
- **New-game set-up isn't repeated, except the mission start.** A loaded game doesn't reset resources, re-place
  players or choose a new music track. It **does** run the mission start of `game/victory.md` again, from the saved
  mission rule, before the loaded game's first start-up cycle; a rule switched before the save stays switched, so
  modes E and H change nothing again (traced for mode AB, round 91: `xhuman9-xhuman9-runestone-alive-saveload-game-60`,
  `game/victory.md` G24). The other modes' effects follow from the same rule (static study): see `game/victory.md`,
  "Mission start".
- **Colour cycling** steps start counting again from the moment the game starts.
- **Preferences** such as game speed aren't part of a saved game.
- **Multiplayer players.** When a multiplayer save is loaded, the saved players are matched to the players now in the
  game. The lobby rules for this are outside this spec.

### Saving with the map revealed

If the map-reveal cheat is on when a game is saved, the cheat is switched off just long enough to refresh the remembered
buildings, as when the cheat is switched (`game/cheats.md`): buildings that stand wholly on unexplored ground lose their
remembered image. Then the cheat is switched back on and the game is written. This change to the remembered images also
stays in the running game.

## Data

### Text entries (`network.tbl`)

| Entry | Use |
| --- | --- |
| 9 to 14 | The reason in an error message (below) |
| 22 | Saving failed |
| 23 | Loading failed |
| 24 | Confirm saving over a file (name filled in) |
| 25 | Confirm deleting a file (name filled in) |
| 26 | Too many saved games |
| 28 | Only the named player may save, load or restart |
| 29 | Battle.net save wait |
| 105 | Characters not allowed in a name |

### Error reasons

| Entry | When |
| --- | --- |
| 9 | The file can't be created |
| 10 | The file can't be opened |
| 11 | The file was saved under the other rules |
| 12 | The file isn't a saved game |
| 13 | The file is too short |
| 14 | Writing failed |

### Constants

| Constant | Value |
| --- | --- |
| Folder | `Save` |
| Saved games per kind | 100 |
| Name length | 27 characters typed |
| Battle.net wait between saves | 120 s |

## Test vectors

| Situation | Expected |
| --- | --- |
| Single player, save as `Siege` | `Save\Siege.sav` is written; the list shows `Siege` at the top |
| Save as `Siege` again | Confirmation entry 24 with `Siege`; OK writes over the same file |
| Save as `siege` when `Siege` is listed | No match (case matters), so no confirmation; on a file system that ignores case, as the original's does, `Siege.sav` is overwritten |
| Save as `con` | File `Save\con_.sav`; the list shows `con_` |
| Type `a?b` | The `?` is ignored; the field shows `ab` |
| Type a space into an empty field | Ignored |
| Name `Base   ` (trailing spaces) | Saved as `Base` |
| 100 saved games, save as a new name | Notice entry 26; nothing written |
| Choose `Siege` in the list, then type a letter | The choice is cleared and Delete is disabled |
| Delete `Siege`, OK | The file is removed; the save dialog reopens without it |
| Single player, a `.net` file in the folder | Not listed: only `.sav` files are shown |
| Classic multiplayer game, swamp era, saved as `Swamp` | `Swamp.net` (`.mlt` in a session) is written and listed (the original never lists it) |
| Battle.net, save, then open the save dialog 60 s later | Notice entry 29; no dialog |
| Battle.net, the local player isn't the lowest-numbered one present | Notice entry 28 with that player's name |
| Load a campaign save | No act screen, no briefing; the saved music track plays |
| Load the same save twice and do the same things | The random results differ between the two runs |
| Save with the reveal cheat on, then load | The map isn't revealed after the load |
| **G11.** Load an Enhanced single-player save made with the victory countdown at 50 | Two start-up cycles run; the first check after the load runs in the second start-up cycle, then at counter 201, 402, …, as in a new game (traced with a save at countdown 59: `human1-human01-saveload-game-300`) |
| **G17.** `human1-human01-saveload-game-300` (oracle trace, Enhanced): Human01 saved after the cycle that ends with the counter at 342 (fog countdown 57, victory countdown 59), then loaded | The loaded game's setup record has victory countdown 1; first start-up cycle: fog refresh (fog countdown 0); second start-up cycle: fog countdown 99, victory countdown 200; first normal cycle: counter 1, fog countdown 98, victory countdown 199, the same values as the new game's first three records |
| **G18.** Pause a single-player game, save from the game menu, load (static study) | The loaded game runs |
| **G19.** `human1-human01-cheat-load-game-50` (oracle trace): saved with the cheat-used mark clear, then "on screen" twice, then that save loaded | The mark is 1 in the loaded game; the mission would rank 39 |
| **SL1.** `xhuman9-xhuman9-runestone-saveload-game-60` (oracle trace, Enhanced): computer unit 1562 (owner 5) moving to its goal (order 3); the loaded game's setup record has it at tile (56, 20), still animation, wait 1 | n = 1: still animation, wait 3 (pass B's move taking over as its pending order); n = 2, 3: wait 2, 1; n = 4: on tile (55, 19) in its move animation. Pass B runs next at n = 51, and the move takes over again at n = 58 (still animation, wait 3) |
| **SL2.** `xhuman9-xhuman9-r92-pending-game-30` (oracle trace, Enhanced): the same game saved one cycle earlier (1562 saved with wait 2) | Before the save pass B gives 1562 its move in the cycles 1601, 1651, 1701 and 1751; after the load at n = 1 and in no other cycle up to n = 30. 1562: n = 2 still animation, wait 3; n = 5 on tile (55, 19) |
| **SL3.** `xhuman9-xhuman9-r92-alive-pending-game-30` (oracle trace, Enhanced): saved after n = 1653 | After the load pass B moves units 1532 and 1562 at n = 1. Kept phase would have run the passes at n = 48 instead |
| **SL4.** `xhuman9-xhuman9-r92-aijobs-aijobs-game-60` (oracle trace, Enhanced): computer player 6 (3 grunts, 2 axethrowers, 2 ogres, one of them dying) saved with footmen, archers, siege, knights counts 3, 2, 0, 1 | The loaded game's counts are 6, 4, 0, 2 from its setup record to n = 60; the lumber mill, blacksmith and stables tests are unchanged |
| **SL5.** Same trace, loaded game n = 12: player 6's orc barracks (unit 1480) thinks; wanted archers 3 | Archers 4 is not below 3, so rule 3's 1-in-4 draw isn't made; rule 4's 1-in-8 draw is (siege 0 < 1). The cycle makes 7 draws in all |
| **SL6.** `xhuman9-xhuman9-runestone-alive-saveload-game-60` (oracle trace, Enhanced) | The generator state at n = 12 of the loaded game matches the trace once SL4's counts are used (one draw fewer than with the saved counts) |
| **SL7.** `xhuman9-xhuman9-runestone-saveload-game-400` (oracle trace, Enhanced, 1600 slots): after the load all 200 units have number slot + 1 (largest 1600) | n = 252: two units are created, slot 1399 gets 1626 and slot 1398 gets 1627; n = 257: slot 1397 gets 1628 |
| **SL9.** `xhuman9-xhuman9-runestone-saveload-aigroups-airesearch-game-400` (oracle trace, Enhanced; the SL7 scenario, saved after n = 1704): computer player 6, build list 0x4A, 0x3C, 0x4C, 0x52, 0x40, 0x3C, 0x86, 0x80, 0x88, limit 37. At the save: request {0x88}, done entries 0 to 7, weapon and arrow levels 0, its blacksmith researching 0x86 and its lumber mill 0x80 | The loaded game's setup record: no requests; done entries 0 to 5 (6 and 7 open again) |
| **SL10.** Same trace, player 6 after the load | n = 55: request 0x86 set (the scan stops at entry 6). n = 329: the 0x86 research finishes (weapon level 1) and the blacksmith goes idle. To n = 400: requests stay {0x86}, done marks stay 0 to 5, no research or upgrade starts (gold 350, lumber 1450 at n = 343), and 0x88 is never requested |
| **SL11.** Same trace, computer player 5 (build list 0x4A, 0x4C, 0x3C, 0x98, 0x3C, 0x52, 0x42, 0x3E, 0x82, …, limit 30; it owns a map fortress). At the save: request {0x98}, done entries 0, 1, 2, 4, 5, 6, 7, 11 | Setup record: no requests; done 0 to 7 and 11 (entry 3, 0x98, marked by the fortress). n = 15: request 0x82 (entry 8). n = 398: rangers start (gold 1560 to 60) and entry 8 is marked |
| **SL12.** Same trace, computer player 0 (0x40 at entries 7, 8 and 9; two guard towers and two cannon towers). At the save: done 0, 1, 2, 11 | Setup record: done 0, 1, 2, 7, 8, 9, 11: three towers mark the three scout-tower entries, and the fourth finds none open |
| **SL13.** Same trace: player 5's mages count is 1 at the save | Setup record to n = 400: 2. The worker counts of players 0, 1, 2, 5 and 6 are the same in the save and setup records |
| **SL14.** `orc12-orc12-r93-groups-saveload-aigroups-game-60` (oracle trace, Enhanced, saved after n = 8004): computer players 1 and 3 at the save: destroyers 3 and 2 (of 6 and 4 on the map; the others held), tankers 4 and 4, workers 7 and 7 | Setup record to n = 60: destroyers 6 and 4; tankers 4 and 4; workers 7 and 7 |
| **SL15.** `xorc6-xorc6-r93-groups-saveload-aigroups-game-60` (oracle trace, Enhanced, saved after n = 3104): computer player 2 at the save: gryphons 2, workers 4 | Setup record to n = 60: gryphons 4, workers 4 |

## Open questions

- **Overwrite answer.** What the other answer of the overwrite confirmation (entry 24) does isn't traced. Default:
  return to the save dialog, as Delete's confirmation does.
- **Load button.** When the load dialog's Load button is enabled isn't traced. Default: while the list isn't empty.
- **Full slots in multiplayer.** When all 100 slots are taken and a multiplayer save asks for a new name, whether the
  oldest save is replaced or nothing is saved depends on a value that wasn't traced. Default: replace the oldest.
- **More than 100 files.** What happens when the folder holds more than 100 matching files isn't traced. Default: only
  the first 100 found are used.
- **Session save timing.** In a network session the save is a message, not a game command. Whether every player is
  sure to save at the same game cycle isn't traced. Default: save when the message is handled.
- **Saver's own copy.** In a network session, whether the player who sends the save also gets it back as a message
  and saves, or saves in another way, isn't traced. Default: every player, including the sender, saves once.
- **Folder in a network session.** In a network session the `Save` folder is taken from the game's data location,
  which normally is the game's folder. Default: the game's folder.
- **Classic clock.** After loading a Classic save, the clock may start from a stale value left by an earlier save.
  Default: start the clock at 0.
- **Selection and groups.** Whether the selection, control groups and the screen's minimap state survive a load
  hasn't been traced. Default: the selection is empty and groups are kept only if they're part of the units' state.
- **Small games without a session.** In a multiplayer game without a network session, the permission check asks
  the network layer for a count, and a count under 3 lets any player save. What that count means isn't traced.
  Default: apply the lowest-numbered-player rule to every multiplayer game.
- **Multiplayer loading.** The lobby side (entries 16 and 17 about player counts, matching players to saved slots) is
  outside this spec and not yet studied.
- **Untraced training group counts after a load.** Seven groups are traced doubling (SL4, SL13 to SL15), and the
  worker and tanker counts are traced coming back right. The flying machine, transport, battleship and submarine
  counts aren't traced. Default: they double like the traced groups (static study, round 93). **Trace request SL8
  (narrowed):** a save and load with a computer player that owns a trained, not held, flying machine, transport,
  battleship or submarine at the save.
- **Running game.** The save and load rules above that say "traced" were watched in the running game (round 90);
  the rest wasn't.
