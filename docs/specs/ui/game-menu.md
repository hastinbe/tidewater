# UI: game menu, pause and options

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 25 (game menu,
  pause, options; gap 8)"), the dialog layouts in `Rez\` (`gamemenu.bin`, `bgamemnu.bin`, `options.bin`,
  `OptionsPlat.bin`, `helpmenu.bin`, `snd_dlg.bin`, `spd_dlg.bin`, `intrface.bin`, `objctdlg.bin`, `abrtmenu.bin`,
  `restart.bin`, `cstcnfrm.bin`, `chatrtrn.bin`, `surrendr.bin`, `quitdraw.bin`, `quit2mnu.bin`, `quit.bin`) and the
  tables `Rez\objctivs.tbl` and `Rez\network.tbl`. Not yet confirmed against the running game.

## Behaviour

This spec covers the menus a player opens during a game, plus pause and the player's stored settings:

- the game menu and the menus under it;
- the sound, speed and preferences dialogs;
- the objectives screen;
- the end-scenario choices: restart, surrender and quit;
- pause;
- the settings kept between runs.

The keys that open these are listed in `ui/selecting-and-commanding.md` ("Keys"). The save and load dialogs are a
later spec. Table entries are numbered from 1: "network entry n" is entry n of `Rez\network.tbl`. Dialog controls are
named by their control id in the dialog file.

### Menus and the game clock

- **Single player.** While any menu or dialog of this spec is open, the game doesn't advance: no cycles run, as while
  paused (`core/timing-and-rng.md`). Closing the last one lets the game go on.
- **Multiplayer.** Opening a menu doesn't stop the game. The game goes on underneath, and only pause stops it.
- **Only one dialog is open at a time.** Opening one closes the one before it. Going "back" means opening the
  parent again.
- **On opening**, each menu and settings dialog sets the normal arrow pointer.

**Going back.** The game counts how deep the player is in the menu chain. Each menu opened from another menu adds one
level.

- **Previous or Cancel** in a dialog opened from a menu reopens that menu.
- **Same dialog opened directly by its key** (for example F7 for sound): Previous or Cancel returns straight to the
  game.

### The game menu

Opened by its key or the menu button. The file is `bgamemnu.bin` when the game runs over a network session, and
`gamemenu.bin` otherwise. The only difference is that the network version has no Load button. The backdrop is
`art\h128x144.bmp` for the human interface and `art\o128x144.bmp` for the orc interface.

| Control | Function | Key in the menu | Opens |
| --- | --- | --- | --- |
| −3 | Return to the game | Esc | Closes the menu |
| 1 | Save | F11 | The save dialog (later spec) |
| 2 | Load (single player only) | F12 | The load dialog (later spec) |
| 3 | Options | F5 | The options menu |
| 4 | Help | F1 | The help menu |
| 5 | Objectives | O | The objectives screen |
| 6 | End scenario | E | The end-scenario menu |

**Options menu.** The file is `OptionsPlat.bin` in a Battle.net game and `options.bin` otherwise.

| Control | Function | Opens |
| --- | --- | --- |
| 1 | Sound (F7) | The sound dialog |
| 2 | Speeds (F8) | The speed dialog |
| 3 | Preferences (F9) | The preferences dialog |
| 4 | Network (Battle.net file only) | See the open questions |
| −3 | Previous | The game menu |

**Help menu** (`helpmenu.bin`):

| Control | Function | Opens |
| --- | --- | --- |
| 1 | Keystroke help | `help.bin` |
| 2 | Tips | `tips_dlg.bin` |
| −3 | Previous | The game menu |

### The settings dialogs

The sound, speed and preferences dialogs share these rules:

- **Changes apply while the dialog is open.** Speed and preference changes apply at once. Sound dialog changes apply
  after a short delay (below).
- **OK** keeps the changes and closes all menus. The player is back in the game, even when the dialog was opened from
  the options menu.
- **Cancel** puts back these settings as they were when the dialog opened, then goes back as above:
  - the game speed (sent again as a speed command if it changed);
  - the mouse and key scroll speeds;
  - the music and digital volumes;
  - the next tip number and all option bits;
  - fog of war and the minimap mode.

  Cancel in the preferences dialog also puts the system-pointer bit (0x1000) back into effect.

**Sound dialog** (`snd_dlg.bin`):

| Control | Setting | Range or bit | Notes |
| --- | --- | --- | --- |
| Slider 1 | Music volume | 0 to 100 | Disabled when the music bit (0x8) is clear or the computer has no music device |
| Slider 2 | Digital (sound effects) volume | 0 to 100 | Disabled when there is no sound device |
| 4 | Unit speech | bit 0x1 | `ui/sounds.md`, "Sound options". Disabled with no sound device |
| 5 | Acknowledgements | bit 0x2 | Same |
| 6 | Building sounds | bit 0x4 | Same |
| 8 | Subtitles | bit 0x40000 | `ui/sounds.md`, "Sound options" |
| 7 | Speak player messages | bit 0x100000 | Hidden when the dialog opens, so never shown in 2.02 |
| 9 | 3-D sound | bit 0x20000 | Same |

The labels are those of the dialog file. The game's own names for the stored bits agree with them.

**Delay.** A change to a slider or a checkbox starts a countdown of 5 dialog ticks, unless one is already running.
When it runs out, both volumes and all the option bits are applied together. So a slider dragged quickly is applied
once, about five ticks after the first change. How long a dialog tick is wasn't traced (see the open questions).

**Speed dialog** (`spd_dlg.bin`):

| Control | Setting | Range | Notes |
| --- | --- | --- | --- |
| Slider 1 | Game speed | 0 to 6 under Classic rules, 0 to 8 under Enhanced rules (`core/timing-and-rng.md`) | Disabled in Enhanced multiplayer. A change is sent as a speed command, as the + and − keys are |
| Slider 2 | Mouse scroll speed | 0 to 6 | `ui/screen-and-cursors.md`, "Speeds" |
| Slider 3 | Key scroll speed | 0 to 6 | Same |

**Preferences dialog** (`intrface.bin`):

| Controls | Setting | Notes |
| --- | --- | --- |
| Radio 5 or 4 | Mouse style | 5 is the first style (right-click commands; bit 0x800 set), 4 the second (`ui/selecting-and-commanding.md`) |
| Radio 1 or 2 | Fog of war on or off | Disabled in multiplayer. If fog was on when the dialog opened and is off when it closes, the map's visibility is rebuilt as for fog off at that moment (`vision/fog.md`). Nothing happens while the dialog is still open |
| Radio 7 or 8 | Minimap terrain on or off | `ui/minimap.md`, "Fog and the terrain option" |
| Checkbox 9 | Colour cycling | bit 0x40. Palette cycling is a later spec |

### The objectives screen

`objctdlg.bin`, with the race's backdrop as for the game menu. Its text box shows one entry of `Rez\objctivs.tbl`:

- **A single-player campaign mission** with campaign entry e below 52 (the entry numbering of `game/victory.md`)
  shows entry e + 1;
- **any other game** (a custom scenario, multiplayer) shows entry 59.

Previous reopens the game menu if the screen was opened from it, and otherwise returns to the game.

### Ending the scenario

**End-scenario menu** (`abrtmenu.bin`). It has six buttons; some are hidden by the kind of game:

| Control | Function | Single-player campaign | Custom scenario | Multiplayer | Battle.net |
| --- | --- | --- | --- | --- | --- |
| 2 | Restart the scenario | shown | hidden | hidden | hidden |
| 5 | Return to the chat room | hidden | hidden | shown | hidden |
| 6 | Load another custom scenario | hidden | shown | hidden | hidden |
| 1 | Surrender (on Battle.net: see below) | shown | shown | shown | shown |
| 3 | Quit to the main menu | shown | shown | shown | hidden |
| 4 | Exit the program | shown | shown | shown | shown |
| −3 | Previous | shown | shown | shown | shown |

**Battle.net's quit button.** On Battle.net, button 1 is labelled with the text of hidden control 7 (surrender) once
more than **120 seconds** of game time have gone by. Before that it has the text of hidden control 9, a quit counted as
a draw. The game time is real time in whole seconds since this game's clock started at game start, plus the time already
played before it was saved for a loaded game. The clock stops when the game ends. If the menu is open across the
120-second mark, the label changes while it is open. If the quit (draw) confirmation is open at that mark, it is
replaced by the surrender confirmation.

**Confirmations.** Each choice opens a confirmation dialog. Its OK does the action below; its Cancel goes back as
above.

| Choice | Confirmation | On OK |
| --- | --- | --- |
| Restart | `restart.bin` | Replay the same mission: no score screen; the flow goes back to starting the mission, which shows the act screen and the briefing again unless Ctrl is held (`game/campaign-flow.md`, "Starting a mission") |
| Return to the chat room | `chatrtrn.bin` | Leave the game for the multiplayer chat room |
| Load another custom scenario | `cstcnfrm.bin` | Go to the custom-game screen |
| Surrender (and on Battle.net after 120 s) | `surrendr.bin` | The local player is defeated. All menus close; in single player the defeat dialog (`rez\lmission.bin`) is shown, then the defeat score screen (`game/campaign-flow.md`) |
| Quit (draw), Battle.net before 120 s | `quitdraw.bin` | Locally the same as surrender: a defeat |
| Quit to the main menu | `quit2mnu.bin` | Back to the main menu |
| Exit the program | `quit.bin` | The program ends (settings are saved; see "Stored settings") |

**Who may restart, return or load.** Before its confirmation opens, the restart, return-to-chat-room and
load-custom choices pass a check:

- **Single player:** always allowed.
- **Multiplayer:** allowed only for one player. With a network session that is the player in the lowest session slot
  that is still connected. Without one, it is the first active human player, and the check passes anyway when fewer
  than three players are in the game. If there is no such player, the check passes.
- **Refused:** the game shows network entry 28, filled in with that player's name, in a box with an OK button, and no
  confirmation opens.

The same choices have keys outside the menu (`ui/selecting-and-commanding.md`): Ctrl/Alt+R restart, Ctrl/Alt+Q
surrender or quit to menu, Ctrl/Alt+X exit. These open the confirmation directly, so its Cancel returns to the game.
In a Battle.net game the surrender key opens `surrendr.bin` or `quitdraw.bin` by the 120-second rule.

### Pause

Pause and resume are synced commands (`ui/selecting-and-commanding.md`), in single player too.

**Pausing.** A pause command does nothing if the game is already paused. In Enhanced multiplayer:

- each player starts the game with **3** pauses;
- a pause from a player with none left does nothing;
- otherwise it uses one of that player's pauses, and every player sees network entry 86 filled in with the pausing
  player's name and the pauses that player has left after this one.

A paused game runs no game cycles (`core/timing-and-rng.md`, for what the clock does). The pause button changes to the
resume button (`ui/screen-and-cursors.md`). Saved games keep each player's pause count.

**Greyscale while paused.** When the game pauses, the whole palette is shown in grey. Each colour becomes a grey of
level floor(77 × red / 256) + floor(151 × green / 256) + floor(28 × blue / 256). Each term is rounded down on its
own, and the level is used on all three channels. The change happens all at once. Resuming puts back the game's
current palette.

**Resuming.** A resume command does nothing if the game isn't paused. It puts the palette back and, in Enhanced
multiplayer, shows network entry 87 filled in with the player's name. Resuming uses no pause.

Under Classic rules and in single player there is no limit on pauses, and no pause or resume notice is shown.

### Tips at the start of a game

When the "tips at start-up" bit (0x100) is set, a game begins with the tips dialog (`tips_dlg.bin`) open. This
doesn't happen in multiplayer or the attract-mode demo.

**Tip number.** The stored next-tip number n is first set to 1 if it is 0 or above 60. The dialog shows
`Rez\tips.tbl` entry n, and the stored number becomes n + 1, or 1 if that is above 60.

### Stored settings

The game keeps these settings for the player between runs:

- **Loading.** At program start each setting first takes its default, then the stored value if there is one. A stored
  number outside its range is replaced by the default.
- **Saving.** All settings are saved when the program exits normally. They are not saved during a game, or when the
  program stops on an error.

Where they are stored is the reimplementation's choice.

| Setting | Range | Default |
| --- | --- | --- |
| Game speed | 0 to 8 | 5 |
| Mouse scroll speed | 0 to 6 | 4 |
| Key scroll speed | 0 to 6 | 4 |
| Music volume | 0 to 100 | 75 |
| Digital volume | 0 to 100 | 100 |
| Next tip number | — | 0 |
| Option bits | (below) | (below) |

The game speed is stored as a 9-level value. Under Classic rules it maps onto 7 levels (`core/timing-and-rng.md`).

**Option bits.** Each is kept as a stored on/off value.

| Bit | Option | Default | Set where |
| --- | --- | --- | --- |
| 0x1 | Unit speech | on | Sound dialog |
| 0x2 | Acknowledgements | on | Sound dialog |
| 0x4 | Building sounds | on | Sound dialog |
| 0x8 | Music allowed | on | No dialog. When clear, the music volume slider is disabled. Ctrl+M doesn't change it (below) |
| 0x40 | Colour cycling | on | Preferences dialog |
| 0x100 | Tips at start-up | on | Tips dialog |
| 0x200 | Play the opening film | on | Not in this spec |
| 0x800 | First mouse style | on | Preferences dialog |
| 0x1000 | System pointer instead of the game's | off | No dialog |
| 0x10000 | Stereo | on | No dialog |
| 0x20000 | 3-D sound | on | Hidden in the sound dialog |
| 0x40000 | Subtitles | off | Sound dialog |
| 0x80000 | Speech recognition | off | No dialog |
| 0x100000 | Speak player messages | off | Hidden in the sound dialog |

The rest of the stored settings are player names and modem connection details for the multiplayer screens. They
aren't covered here.

**Ctrl+M and Ctrl+S** don't change any bit. Ctrl+M sets the music volume to 0 and remembers the old value, and stops the
music; pressed again, it puts the remembered volume back and starts the music again. Ctrl+S does the same for the
digital volume. Since the volumes are stored, a program that exits with music off by Ctrl+M stores a music volume of 0.

**Not stored:**

- **Fog of war** is set at the start of each game from that game's options (`vision/fog.md`, `game/setup.md`).
- **The minimap mode** starts as terrain on when the program starts. It keeps its value from game to game until the
  program ends.

## Data

| Item | Value |
| --- | --- |
| Pauses per player, Enhanced multiplayer | 3 |
| Battle.net surrender threshold | more than 120 s of game time |
| Objectives text, default | `objctivs.tbl` entry 59 |
| Objectives text, campaign entry e (e < 52) | `objctivs.tbl` entry e + 1 |
| Refused restart message | network entry 28 |
| Pause, resume notices | network entries 86, 87 |
| Greyscale level | floor(77 R / 256) + floor(151 G / 256) + floor(28 B / 256) |
| Game menu backdrops | `art\h128x144.bmp` (human), `art\o128x144.bmp` (orc) |

## Test vectors

| Situation | Expected |
| --- | --- |
| Single player, Esc → game menu at cycle 1000; wait 10 s; Return to the game | The cycle counter is still 1000 when the menu closes |
| Multiplayer, open the game menu | The game keeps running; units go on moving |
| Network game, game menu | Six buttons; no Load |
| Game menu → Options → Sound → Cancel | Back to the options menu; volumes as before the dialog |
| F7 (sound dialog directly) → Cancel | Back in the game, no menu open |
| Game menu → Options → Speeds, drag game speed to 8, OK | All menus close; the game runs at level 8 (Enhanced) |
| Speeds dialog, game speed 3 → 7, then Cancel | Speed back to 3 |
| Enhanced multiplayer speed dialog | Game speed slider disabled; scroll sliders work |
| Human campaign mission 1 (entry 0), objectives | `objctivs.tbl` entry 1 |
| BDP orc mission 12 (entry 51), objectives | Entry 52 |
| Custom scenario, objectives | Entry 59 |
| Single-player campaign, End scenario | Restart, Surrender, Quit to menu, Exit, Previous |
| Custom scenario, End scenario | Load custom, Surrender, Quit to menu, Exit, Previous |
| LAN multiplayer, End scenario | Return to chat room, Surrender, Quit to menu, Exit, Previous |
| Battle.net, End scenario 60 s into the game | Quit (draw) button, Exit, Previous; the button opens `quitdraw.bin` |
| Same, 121 s in | Surrender button; it opens `surrendr.bin` |
| Four-player LAN game, a player in slot 2 (slot 0 occupied) chooses Return to chat room | Network entry 28 with slot 0's player name; no confirmation |
| Enhanced multiplayer, player A pauses three times (resuming between) | Notices show 2, 1, 0 pauses left; a fourth pause from A does nothing |
| Pause with pure red (255, 0, 0) in the palette | That entry shows grey 76 on all channels |
| First run, no stored settings | Speed 5, scroll 4/4, music 75, digital 100, all option defaults as in the table |
| Stored mouse scroll speed 9 | Loaded as 4 |
| Change music volume, then end the program from the task manager | The change is lost |
| Pause with white (255, 255, 255) | Grey 76 + 150 + 27 = 253 |
| Stored tip number 60; tips dialog opens | Shows tip 60; stored number becomes 1 |
| Stored tip number 0 | Shows tip 1; stored number becomes 2 |
| Ctrl+M with music volume 75, exit, restart the program | Music volume 0 |
| Battle.net, 100 s in, the quit (draw) confirmation is open and left until 121 s | It is replaced by the surrender confirmation |

## Open questions

- **The Battle.net Network button.** What the Network button (control 4) in the Battle.net options menu opens wasn't
  traced. Default: a dialog of connection details, outside the scope of the reimplementation's first version.
- **Volume change delay.** The delay is 5 dialog ticks, but how often a dialog ticks wasn't traced. Default: a
  tick is one frame, so about 0.1 to 0.2 s.
- **Quit (draw).** What the other players see when someone quits before 120 s on Battle.net wasn't traced (it is
  handled by the session layer). Default: the same as a player leaving (`ui/messages.md`).
- **Window focus.** In single player the game also stops while the program's window isn't active. That belongs with
  the scheduler; not covered here.
- **Restart check without a network session.** The "fewer than three players" test in that case comes from the session
  layer and wasn't traced further. Default: as stated.
- **Running game.** Nothing here was watched in the running game.
