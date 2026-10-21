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
- every unit and building, with their orders, and missiles and corpses;
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
- **Pause.** A loaded game is never paused: the pause isn't saved, and every game start clears it, so a game saved
  while paused loads running (static study, `ERRATA-audit-game.md` erratum 6; a labelled gap-fill, untraced).
- **Creation numbers.** Every unit's creation number (`core/unit-slots.md`, "Creating a unit", step 3) is its slot
  number plus one after a load, whatever it was before; every other unit field is as saved (traced, round 90, in
  both loads checked: 12 units in `human1-human01-saveload-game-300` and 200 in
  `xhuman9-xhuman9-runestone-saveload-game-60`, for example slot 1547's number 53 became 1548). What number the next
  unit created after a load gets isn't traced (see "Open questions").
- **Classic saves.** A Classic multiplayer save doesn't keep the cycle count, the clock, the pause counts or the score
  counters. The cycle count starts again at 0.
- **New-game set-up isn't repeated.** A loaded game doesn't reset resources, re-place players or choose a new music
  track. It doesn't repeat the mission start's effects of `game/victory.md` either (default; see "Open questions",
  "Mission start after a load").
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
- **Mission start after a load (round 88, Q3; still held after round 90).** Static study finds that a load runs the
  mission start's effects again (see `game/victory.md`, "Open questions"). The round-90 trace
  (`xhuman9-xhuman9-runestone-saveload-game-60`, mode AB) doesn't decide it: the damaged runestone had been destroyed
  (it was a ruin) before the save, so neither reading touches it, and the other five runestones were at 400 already.
  Default: not repeated. Trace request GA3b: entry 44, damage a runestone without destroying it, save, load, and read
  its hit points in the loaded game.
- **Next creation number after a load.** No trace creates a unit after a load. Default: the next unit gets the
  largest creation number in use plus one.
- **Running game.** The save and load rules above that say "traced" were watched in the running game (round 90);
  the rest wasn't.
