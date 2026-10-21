# Game: saving and loading games

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 29 (save and load;
  gap 9)"), the dialog files `Rez\savegame.bin`, `Rez\loadgame.bin` and `Rez\loadgamt.bin`, and `Rez\network.tbl`
  (entries cited by number, from 1). Not yet confirmed against the running game.
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

A saved game keeps everything that decides how the game goes on, including:

- the map, its terrain and its explored and seen state;
- every unit and building, with their orders, and missiles and corpses;
- each player's resources, upgrades, spells and allowances;
- alliances (Enhanced only);
- the game cycle count and the game clock (Enhanced only; for the cycle count see "Open questions", "Cycle count
  after a load");
- each player's pause count (Enhanced only);
- each player's score counters (Enhanced only);
- the campaign total (`game/campaign-flow.md`, "Campaign total"; static study, round 88);
- each player's rescued count (`game/rescue.md`; static study, round 88);
- the victory countdown and an outcome a check has found but not yet shown (`game/victory.md`; default, see "Open
  questions", "Victory countdown after a load");
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
  clears it, as a new game start does, so only cheats used after the load count (default; see `game/cheats.md`,
  "Open questions", "Cheat-used mark after a load").
- **Start-up cycles.** A loaded game starts like any game: the two start-up cycles run (`core/timing-and-rng.md`,
  "Scheduler"; static study, round 88). They are full cycles, so the restored victory countdown counts them. The
  check applies the saved rule, with any switch made before the save (`game/victory.md`, "Outcome timing").
- **Classic saves.** A Classic multiplayer save doesn't keep the cycle count, the clock, the pause counts or the score
  counters. The cycle count starts again at 0.
- **New-game set-up isn't repeated.** A loaded game doesn't reset resources, re-place players or choose a new music
  track. It doesn't repeat the mission start of `game/victory.md` either (default; see "Open questions", "Mission
  start after a load").
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
| **G11.** Load an Enhanced single-player save made with the victory countdown at 50 | Two start-up cycles run (static study); the first check after the load runs in the 51st cycle after it, the start-up cycles included, then every 201 cycles (default; held, Q4: the static study predicts the second start-up cycle) |

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
- **Cycle count after a load (round 88, Q2 of `ERRATA-game-consolidation.md`).** Static study finds that the cycle
  count is saved and restored in Enhanced, but every game start, a loaded one included, sets it to 0 again after the
  two start-up cycles, so a loaded game counts from 0. No rule reads the count except the network sync check and
  saving. Default: kept, as above. Trace request GA2.
- **Mission start after a load (round 88, Q3).** Static study finds that the mission start runs again after a load
  (see `game/victory.md`, "Open questions"). Default: not repeated. Trace request GA3.
- **Victory countdown after a load (round 88 review, Q4).** Static study finds that a loaded game sets the victory
  countdown to 1 and chooses the check's rule afresh, as a new game does, so the first check runs in the second
  start-up cycle and an outcome found but not yet shown is lost (found again only if the game state still gives it;
  an instant-cheat outcome isn't, since the cheat flags aren't saved). That contradicts "everything that decides how
  the game goes on" above. Default: both are saved and restored. Trace request GA4 (needs a save and a load in the
  oracle harness).
- **Running game.** None of this has been watched in the running game.
