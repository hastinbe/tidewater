# Game: campaign flow, briefings and end screens

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 20 (campaign flow)"),
  the game's string tables and dialog files in the `Rez` folder, and the file names of the videos, pictures, speech
  and music. Not yet confirmed against the running game.
- **Changes:** 2026-10-12: restarting a mission from the game menu is now described (`ui/game-menu.md`): it replays
  the mission through the act screen check and the briefing, with no score screen.
- **Changes:** 2026-10-12: the skip-setting open question is closed by `game/cheats.md`: the developer words `noglues`
  (act screen, briefing and score screen) and `title` (startup videos) turn the two settings on and off, and both
  survive into later games.
- **Changes:** 2026-10-14: round-67 errata (`ERRATA-round67-flow.md`; static study only): the act screen's and the
  title's tick is the 100 ms real-time menu timer; fades block it; the act sound starts when the picture's fade-in
  ends; the act screen closes with a 400 ms fade to black and a fade of all sound; act screen timeline vector. The
  "Tick lengths" open question is closed.
- **Changes:** 2026-10-15 (round 70, `ERRATA-round70.md`): button names from the dialog data: the briefing's button
  is "Continue" (`rez\hdisptch.bin`, `rez\odisptch.bin`), the victory dialog's are "Victory" and "Save Game"
  (`rez\wmission.bin`), the defeat dialog's is "OK" (`rez\lmission.bin`). No rule change. Reviewed by a
  spec-review session, 2026-10-15.
- **Changes:** 2026-10-15 (round 72, `ERRATA-round72.md`, C1 and C2; observed on the running game, Enhanced): the
  credits screen's layout (pictures, fonts, colours, positions, clip rows, scroll rate, pages) is new under "Credits
  screen"; the ToD human finale was reached and its picture seen, and Escape on it, before its text had scrolled,
  went straight to the main menu. Reviewed by a spec-review session, 2026-10-15.
- **Changes:** 2026-10-15 (round 73, `ERRATA-round73.md`, M1; observed on the running game, Enhanced): "Startup"
  names the main menu's picture, `Art\TitleMenu_BNE.pcx`, distinct from the title picture. Reviewed by a spec-review
  session, 2026-10-15.
- **Changes:** 2026-10-15 (round 74, `ERRATA-round74.md`, Q2 and Q3; observed on the running game, Enhanced): choosing
  a campaign fades all sound, then the screen out over 200 ms; the dialog's last button is "Previous Menu"; the
  briefing has no Escape and no way back to the main menu; closing it fades all sound, then the screen out over
  100 ms. Reviewed by a spec-review session, 2026-10-15: "no way back from the briefing" labelled static study
  (Escape observed).
- **Changes:** 2026-10-16 (B-040 build and train audit, `ERRATA-audit-buildtrain.md`, erratum 12; static study;
  reviewed by a spec-review session, 2026-10-16): "lost" counts only completed units and buildings with a per-type
  count; a building upgrade counts 1 building lost; closes the open question "Units lost and type changes".
- **Changes:** 2026-10-16 (round 79 traces, `ERRATA-round79.md`, reviewed by a spec-review session, 2026-10-16): the
  lost-count rules are traced (`human06-build-stats-1800`, with breaks); wall pieces have no count and aren't built or
  lost; an ally's kill is credited (`human05-ally-kill-hits-1000`).
- **Changes:** 2026-10-21, round 88 (B-038 consolidation of the game area, `ERRATA-game-consolidation.md`, reviewed by a
  spec-review session, 2026-10-21): "Startup" names the second setting of step 1, the startup-skip flag of the `title`
  word (X6); whether that flag also skips the title picture (step 2) is a new open question (Q1). A load takes the saved
  campaign total (X7); the cheat-used mark is cleared by every new game start and isn't saved (X2), and a load clears it
  too (default; Q5 in `game/cheats.md`). The restart rule of `ui/game-menu.md` now agrees with "The act screen": a
  restart doesn't show the act screen again for the same act (X8). The whole program flow, from startup to the next
  mission, is in the new model, `game/lifecycle.md`.

- **Changes:** 2026-10-21, round 89 (B-057 multiplayer, `ERRATA-round89-multiplayer.md`; reviewed by a spec-review
  session, 2026-10-21): "After the score screen": a Battle.net game returns to the Battle.net screens (static study);
  for the other connections the row stays "the multiplayer screens" (held; static study reads the main menu, MS3).
  Held: round 89's row text "name - label" and local-row-first order (new open question, MS1); the stated rule stays.
  The "Multiplayer score screen" open question is answered by `ui/multiplayer-score.md`.

## Behaviour

This spec covers what happens around a single-player game: the startup videos and title, choosing a new campaign,
the act screens and briefings before a mission, the victory and defeat dialogs, the score screen with its statistics
and rank, moving on to the next mission, and the endings. How a map becomes a game is in `game/setup.md`; when
victory or defeat is decided is in `game/victory.md`. The main menu, the game menu, saving and loading, multiplayer
and the music rules are in later specs; this spec names only the music tracks it starts.

Game text is given by string table and entry number, counting from 1.

### Terms

- **Race:** 0 for human, 1 for orc. In a campaign it is the side of the chosen campaign.
- **Mission entry:** as in `game/victory.md`, "Mission rules": 2(n − 1) for human mission n and 2(n − 1) + 1 for orc
  mission n, plus 28 for *Beyond the Dark Portal* (BDP). *Tides of Darkness* (ToD) has 14 missions per race (entries
  0 to 27), BDP has 12 (entries 28 to 51).
- **Campaign game:** a single-player game started from the campaign (not a custom game, not multiplayer, not demo
  playback).
- **Ctrl held:** the Ctrl key is down at the moment of the check. Several steps are skipped while it is held.
- **Skip setting:** a setting that, when on, skips the act screen, the briefing and the score screen. What turns it
  on is the `noglues` developer word, typed or given when the program starts (`game/cheats.md`, "Word codes"). It
  is off in a normal game and, once on, stays on across games.
- **Menu timer:** the real-time clock that drives menus, dialogs, the title and the act screen (static study only).
  While no key or mouse input is waiting, it gives a tick whenever the clock reaches the next tick's due time, and
  each tick makes the next one due 100 ms after it was due. If a tick comes more than 300 ms after its due time, the
  schedule restarts: the next tick is due at once (so two ticks come back to back) and the lost time is not made up.
  Otherwise late ticks come back to back until the timer has caught up. If the next due time is more than 200 ms
  ahead of the clock (the clock went back), the schedule restarts at the current time. The game speed setting, the
  game cycle and the screen refresh have no effect on it.
- **Fade:** a palette fade (in from black, to a new palette, or out to black) of d ms. It blocks: nothing else runs
  and no menu-timer ticks are handled until it ends. It is drawn in frames of at least 16 ms and ends with one frame
  at exactly d, so it takes about d + 16 ms (250 ms takes about 272, 300 about 320, 400 about 416, 600 about 624)
  (static study only).

### Videos

Videos are played full screen from the `Smk` folder. Any key or mouse click skips the rest of a video.

| Video | File |
| --- | --- |
| 0 | `Blizzard` |
| 1 | `Intro_M` |
| 2 | `Smoke_M` |
| 3 | `Burn_M` |
| 4 | `Zepp_M` |
| 5 | `Turtle_M` |
| 6 | `Ships_M` |
| 7 | `Demn_M` |
| 8 | `HVict_M` |
| 9 | `OVict_M` |
| 10 | `IntroX_M` |
| 11 | `HVicX_M` |
| 12 | `OrcX_M` |

### Startup

1. Unless Ctrl is held (or the startup-skip flag is on: the `title` developer word, `game/cheats.md`, "Word codes"),
   video 0 plays. Then, if the movie archive is
   available and the "play the intro" preference bit is set, the bit is cleared and video 1 plays. So the intro
   plays once, on the first start.
2. Unless Ctrl is held (whether the startup-skip flag also skips this step is an open question, "Startup-skip flag
   and the title"), the title picture `Art\Title_Rel_BNE.pcx` fades in over 250 ms, music track 22 starts, and
   the title stays for 100 menu-timer ticks (about 9.8 to 9.9 s; the first tick comes at once) or until a key or
   click (Shift, Ctrl and Alt don't count). It then fades out over 250 ms, or at once (1 ms) if a key or
   click ended it.
3. The main menu opens. It is drawn over its own picture, `Art\TitleMenu_BNE.pcx`, in that picture's palette, not
   over the title picture (`ui/screen-and-cursors.md`, "Menu screens"; observed, round 73).

### Choosing a new campaign

The new-campaign dialog (`rez\newcmpgn.bin`) has four campaign choices:

| Choice | Race | First mission | Next |
| --- | --- | --- | --- |
| ToD orc | orc | entry 1 | the mission starts |
| ToD human | human | entry 0 | the mission starts |
| BDP orc | orc | entry 29 | the BDP intro, then the mission |
| BDP human | human | entry 28 | the BDP intro, then the mission |

"Previous Menu" returns to the single-player menu, with no fade. Any choice also marks the game as a campaign game
(not custom), then all sound fades out (a fade of all sound, `ui/music.md`: whatever the menu was playing, normally
track 22, fades and closes) and the screen fades to black over 200 ms (observed, round 74, for the ToD choices).

**BDP intro.** If the movie archive is available, video 10 plays unless Ctrl is held, then the mission starts. If
the archive isn't available, the game goes back to the main menu instead (see the open questions).

**Campaign maps.** Each mission entry has its own map file, read from the game's archive:
`Campaign\Human\HumanNN.pud` and `Campaign\Orc\OrcNN.pud` for ToD, `Campaign\XHuman\2XHumNN.pud` and
`Campaign\XOrc\2XOrcNN.pud` for BDP, where NN is the two-digit mission number (01 to 14, or 01 to 12).

### Starting a mission

For a campaign game, before the map is read:

1. **Act screen** (skipped while Ctrl is held, or the skip setting is on).
2. **Briefing** (the same, checked again after the act screen).
3. The game starts (`game/setup.md`). If it fails to start, the game goes back to the main menu.

If either screen ends with a quit, the program quits and the mission doesn't start. A game started by loading a
saved game skips both screens. Custom games, multiplayer games and demo playback have neither screen.

### The act screen

The act screen (`rez\interlud.bin`) is shown only for the first mission of an act, and only if that act isn't the
last act shown since the program started (so replaying an act's first mission after a defeat doesn't show it again).

| Mission | Entry | Act lines (`acts.tbl`) | Picture | Video after |
| --- | --- | --- | --- | --- |
| ToD human 1 | 0 | 1, 2 | `umaact1` | none |
| ToD human 5 | 8 | 3, 4 | `umaact2` | 2 |
| ToD human 8 | 14 | 5, 6 | `umaact3` | 4 |
| ToD human 12 | 22 | 7, 8 | `umaact4` | 6 |
| ToD orc 1 | 1 | 9, 10 | `orcact1` | none |
| ToD orc 5 | 9 | 11, 12 | `orcact2` | 3 |
| ToD orc 8 | 15 | 13, 14 | `orcact3` | 5 |
| ToD orc 12 | 23 | 15, 16 | `orcact4` | 7 |
| BDP orc 1 | 29 | 17, 18 | `2xoact1` | none |
| BDP orc 4 | 35 | 19, 20 | `2xoact2` | none |
| BDP orc 7 | 41 | 21, 22 | `2xoact3` | none |
| BDP orc 10 | 47 | 23, 24 | `2xoact4` | none |
| BDP human 1 | 28 | 25, 26 | `2xhact1` | none |
| BDP human 4 | 34 | 27, 28 | `2xhact2` | none |
| BDP human 7 | 40 | 29, 30 | `2xhact3` | none |
| BDP human 10 | 46 | 31, 32 | `2xhact4` | none |

- The first act line is the act number, the second its title. The picture's palette is taken from the act-1 picture
  of the same campaign and side.
- **Timing.** The screen runs a countdown from 60 to 0, one step per menu-timer tick (100 ms of real time; see
  "Terms"), 60 ticks in all. At 60 the palette is black with only the text colours (entries 1 to 3) white, so the
  text shows first, and fades in over 300 ms. At 50 the picture fades in over 600 ms; when the fade ends, the act
  sound plays (`Sfx\hact.wav` for human, `Sfx\oact.wav` for orc). At 0 the screen closes. The screen doesn't wait
  for the act sound. The fades block the screen and its timer, so the countdown pauses during each fade; after it,
  the timer catches up or restarts its schedule as described under "Terms". From the first tick the screen stays
  about 6.4 s before it starts to close (vector under "Test vectors").
- **Closing.** However the screen closes (the countdown at 0, or a key or click), it fades to black over 400 ms,
  then all sound fades out (a fade of all sound, `ui/music.md`): effects still playing stop, and a started music
  track fades out and closes. Then the video after (if any) or the briefing follows. So no music is playing when
  the briefing opens, and the briefing always starts track 20 or 21.
- **Closing early.** Esc, Enter, Space or a mouse click close it, but not on its first tick. A quit request quits
  the program.
- **Video after.** After the screen closes, the act's video plays if it has one, it isn't the video played last, and
  no quit was requested.

### The briefing

The briefing is shown for mission entries 0 to 51. It uses `rez\hdisptch.bin` for a human campaign and
`rez\odisptch.bin` for an orc one.

- **Background picture** by mission number (the same for ToD and BDP):

  | Missions | Human | Orc |
  | --- | --- | --- |
  | 1 to 4 | `Art\hdspatch.pcx` | `Art\slash.pcx` |
  | 5 to 7 | `Art\ogreknit.pcx` | `Art\odspatch.pcx` |
  | 8 to 11 | `Art\dethnite.pcx` | `Art\mage.pcx` |
  | 12 to 14 | `Art\dragon.pcx` | `Art\griffon.pcx` |

- **Title and objectives:** `objctivs.tbl` entry e + 60 is the mission title and entry e + 1 the objectives, where
  e is the mission entry.
- **Story text:** entry 1 of the mission's table: `rez\humanN.tbl`, `rez\orcN.tbl` (ToD) or `rez\2xhumN.tbl`,
  `rez\2xorcN.tbl` (BDP), N the mission number without a leading zero. It scrolls up through the text panel.
- **Narration:** one to three speech files per mission, played in order with a 255 ms gap between them:
  `Speech\hum_N_K.wav` / `Speech\orc_N_K.wav` (ToD) and `Speech\war2x\humN_K.wav` / `Speech\war2x\orcN_K.wav`
  (BDP), K = 1, 2, 3. The parts for each mission are in the Data section.
- **Scroll speed.** With T = the narration's total length in ms (the sum of each part's length plus 255, minus 255),
  the text moves up one step every T / (text height + panel height − half a line) ms; if that is 0, every 100 ms. So
  the text finishes scrolling about when the narration ends.
- **Music:** if no music is playing, track 20 (human) or 21 (orc) starts.
- **Closing.** The briefing closes by itself once the text has scrolled out of the panel and the narration has
  finished. Its **Continue** button (the dialog data's only button) or Enter closes it at once. Closing is a fade of
  all sound (the briefing track fades out and closes), then a 100 ms fade to black, then the game starts (observed,
  round 74). Escape does nothing (observed), so the briefing has no way back to the main menu (static study: its
  only other exit is a quit request, which quits the program).

### The victory and defeat dialogs

When `game/victory.md` decides the outcome:

- **Victory, single player.** Any open menu is closed and the victory dialog (`rez\wmission.bin`) is
  shown over the game, with the race's small picture (`art\h144x64.bmp` or `art\o144x64.bmp`). It has two choices:
  - **Victory** (hotkey V) ends the game with a victory;
  - **Save Game** (hotkey F11) opens the save dialog (a later spec). After a save the game ends with a victory. If the save is
    cancelled, the victory dialog is shown again.
- **Defeat, single player.** The game is marked as lost first, menus are closed, and the defeat dialog
  (`rez\lmission.bin`, same picture) is shown. Its only button, **OK** (hotkey O), ends the game.
- **Multiplayer.** No dialog: the game ends at once with the result.

### The score screen

After a victory or a defeat the score screen is shown, unless the skip setting is on.

1. **The score for the mission is closed.** On a victory the local player's mission score goes up by 500. Then the
   local player's mission score is added to the campaign total.
2. **Layout.** The background is `Art\hwinglu.pcx` (human victory), `Art\owinglu.pcx` (orc victory),
   `Art\hlosglu.pcx` (human defeat) or `Art\olosglu.pcx` (orc defeat). Music track 14 (human) or 15 (orc) plays
   after a victory, 12 or 13 after a defeat. The dialog depends on how many players are shown: `rez\msnstat2.bin`
   for up to 2, `rez\msnstat4.bin` up to 4, `rez\msnstat6.bin` up to 6, `rez\msnstats.bin` otherwise.
3. **Players shown** (single player): for each slot 0 to 7, in order:

   | Slot | Label (`network.tbl`) |
   | --- | --- |
   | The local human player | entry 34 |
   | A computer player not allied with the local player | entry 35 |
   | A computer player allied with the local player | entry 36 |
   | Anyone else | not shown |

   Each row reads "label - name". The name is a `network.tbl` entry chosen by slot and race:

   | Slot | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
   | --- | --- | --- | --- | --- | --- | --- | --- | --- |
   | Human | 50 | 46 | 52 | 56 | 58 | 54 | 48 | 60 |
   | Orc, not BDP | 51 | 47 | 53 | 57 | 59 | 55 | 49 | 61 |
   | Orc, BDP campaign | 94 | 92 | 95 | 97 | 98 | 96 | 93 | 99 |

   Before the rows are built, each slot's controller is put back to what it was when the game started (so a slot
   whose player was defeated or changed during play is still listed by its starting role).
4. **Statistics** for each shown player:

   | Statistic | Value |
   | --- | --- |
   | Units | units alive now + units lost |
   | Buildings | buildings standing now + buildings lost |
   | Gold, lumber, oil | the amounts the player gathered (delivered) during the mission |
   | Kills | units of other players killed (an ally's units included; `combat/damage.md`, V96) |
   | Razings | buildings of other players destroyed (likewise) |

   "Lost" goes up by 1 every time the player's count of units (or of buildings) goes down, whatever the cause
   (death, or a change of owner). So "units" is roughly the number of units the player ever had. The count is kept
   per type, and only for completed units and buildings of a type that has one (traced with breaks:
   `human06-build-stats-1800`, round 79; first read by the B-040 audit, `ERRATA-audit-buildtrain.md` erratum 12):
   - a site killed or cancelled before it is completed was never counted, so it isn't lost;
   - a finished building upgrade (a town hall becoming a keep, a scout tower becoming a guard tower) takes the
     building out of the old type's count, so it counts **1 building lost**;
   - circles of power, runestones, gold mines, oil patches and wall pieces have no count: they are never lost, and a
     completed wall piece isn't counted as built either (`economy/walls.md`, "Completion: the piece becomes
     terrain").

   Each bar grows in steps of max / 40 (at least 1), where max is the largest value of that statistic among the shown
   players.
5. **Score and rank.** The shown score counts up from 0 to the campaign total in steps of total / 10, with the tick
   sound `Sfx\numbers.wav`; `Sfx\rank.wav` goes with the rank. The rank is a `rank_txt.tbl` entry:
   - i = the position (counting from 0) of the first threshold above the total, in the list 2000, 5000, 8000, 18000,
     28000, 40000, 55000, 70000, 85000, 105000, 125000, 145000, 165000, 185000, 205000, 230000, 255000, 280000,
     300000. If no threshold is above the total, i = 18.
   - The entry is 2i + 1 + race.
   - If the local player used a cheat during this mission, the entry is 39 instead. "During this mission" means
     since the last game start: the cheat-used mark is cleared by every new game start, a replay and a restart
     included, and it isn't saved (static study, round 88); a load clears it too (default; `game/cheats.md`, "Flags
     across games" and "Open questions").
6. **Closing.** Esc, Enter, Space or the button close the screen. A quit request quits the program.

**Mission score.** Each player has a mission score, 0 at the start of a game. When a unit of one player kills a unit
of another player (not its own), the killer's player gets the victim type's point value (the "points" field of the
unit data). A victim of a non-building type (below the first building type) counts as a kill, a building as a
razing.

**Campaign total.** When a game starts, the campaign total is set to 0 if it is a multiplayer or custom game, or the
first mission of a campaign (entries 0, 1, 28 and 29); otherwise it carries over. The value at the start of the
mission is remembered. A defeat puts the total back to that value before the mission is replayed, so a lost attempt
adds nothing. A loaded game doesn't reset the total: it takes the saved one (`game/save-load.md`; static study,
round 88).

### After the score screen

| Game | Victory | Defeat |
| --- | --- | --- |
| Campaign, last ToD mission (entry 26 human, 27 orc) | ToD ending | replay the same mission |
| Campaign, last BDP mission (entry 50 human, 51 orc) | BDP ending | replay the same mission |
| Campaign, any other mission | next mission of the same race (entry + 2): act screen, briefing, game | replay the same mission |
| Custom game | main menu | main menu |
| Multiplayer, Battle.net | the Battle.net screens | the same |
| Multiplayer, other connections | back to the multiplayer screens (`ui/multiplayer-score.md`, "What follows the score screen"; held, see "Open questions") | the same |

Replaying a mission goes through the act screen check and the briefing again. The act screen won't show again
because its act was the last shown.

**Restart.** Restarting the scenario from the game menu (`ui/game-menu.md`, "Ending the scenario") replays the same
mission the same way, with no score screen: the flow goes straight back to "Starting a mission".

### Endings

- **ToD ending.** A video, then the finale dialog. Which video plays wasn't traced (see the open questions); by
  its file name it is video 8 (human) or 9 (orc).
- **BDP ending.** The same, with video 11 (human) or 12 (orc).
- **Finale dialog** (`rez\finaldlg.bin`): picture `art\humanv.pcx` or `art\orcv.pcx`; music track 16 (human) or
  17 (orc); the narration below; and `finale.tbl` entry k + 1 scrolled up, with k = race + 2 for BDP and race for ToD.

  | Ending | Narration |
  | --- | --- |
  | ToD human | `Speech\humvict.wav` |
  | ToD orc | `Speech\orcvict.wav` |
  | BDP human | `Speech\war2x\humvict1.wav`, `humvict2.wav` |
  | BDP orc | `Speech\war2x\orcvict1.wav`, `orcvict2.wav`, `orcvict3.wav` |

  A second page then scrolls entry 1 of `rez\crdt_lst.tbl` (ToD) or `rez\crelst2x.tbl` (BDP), the credits, one step
  every 40 ms.
- After either ending the main menu opens. Observed (round 72, ToD human ending reached with the finale cheat from
  Human01): the finale shows `art\humanv.pcx`. Escape, pressed about 4 minutes in while its text hadn't started
  scrolling (see "Open questions"), went straight to the main menu, with no credits page.

### Credits screen

Shown by the main menu's Show Credits button. Observed on the running game (round 72, Enhanced; frames of the
game's own 640 × 480 display compared with the pictures):

- **Pages.** Page 1 is `Art\credits.pcx` with entries 0 (title) and 1 (body) of `rez\creditsBNE.tbl`. When the
  body's last line has scrolled off the top, page 2 replaces it: `Art\2xcredit.pcx` with entries 2 and 3 (the
  Beyond the Dark Portal credits). What follows page 2 wasn't watched (see "Open questions"). The OK button returns
  to the main menu.
- **Text window.** Text is drawn over the picture and shows only in rows **80 to 354** (inclusive); a line moving
  through row 80 or 354 is cut there. No horizontal cut was seen (text spans x 140 to 455).
- **Title** (the table's title entry): font `Art\font32.fnt`, left edge x **140**, wrapped onto two lines
  ("Warcraft II: Battle.net" / "Edition Credits"; "Warcraft II: Beyond the" / "Dark Portal Credits"), line pitch
  **35** (the font's height). On the first full frame after the fade-in the first line's cell top is at y **274**,
  so the title starts inside the window rather than scrolling in from its bottom.
- **Body** (the next entry): font `Art\font12x.fnt`, line pitch **25**. A line starts at x **140**; a line that
  begins with a tab starts at x **174** (a 34-pixel indent). An empty line takes one pitch, so a heading after a
  blank line is 50 pixels below the line before it (in 6 of 17 measured gaps it was 49 or 51; see "Open
  questions"). The body's first line (after the
  entry's two leading newlines) has its cell top 129 pixels below the title's first cell top.
- **Glyphs.** 1 pixel between glyphs; a space leaves 10 pixels (font12x) or 18 pixels (font32) between the glyphs
  either side of it.
- **Colours.** Both fonts' face shades 1 to 4 show as (255, 255, 255), (219, 219, 219), (187, 187, 187) and
  (155, 155, 155), and shade 5 (the shadow) as black, on both pictures.
- **Scroll.** All text moves up 1 pixel about every 41 ms of wall-clock time (measured over 37 steps), which fits
  the one step per 40 ms of the finale's credits page (`Data`, "Credits scroll").
- **Pointer:** the 3D arrow (`ui/screen-and-cursors.md`).

## Data

### Briefing narration parts per mission

| Campaign | Missions with 1 part | 2 parts | 3 parts |
| --- | --- | --- | --- |
| ToD human | 6, 9, 10, 12, 14 | 1, 2, 3, 4, 5, 7, 8, 11, 13 | none |
| ToD orc | 1, 2, 4, 5, 6, 8, 9, 10, 14 | 3, 7, 11, 12, 13 | none |
| BDP human | none | 1, 2, 3, 5, 8, 9, 11, 12 | 4, 6, 7, 10 |
| BDP orc | 8 | 2, 3, 4, 5, 6, 9, 11 | 1, 7, 10, 12 |

### Music tracks started here

| Track | File (in `Music`) | Used for |
| --- | --- | --- |
| 12 | `HDEFEAT.WAV` | score screen, human defeat |
| 13 | `ODEFEAT.WAV` | score screen, orc defeat |
| 14 | `HVICTORY.WAV` | score screen, human victory |
| 15 | `OVICTORY.WAV` | score screen, orc victory |
| 16 | `HUMAN5.WAV` | human finale |
| 17 | `HUMAN6.WAV` | orc finale |
| 20 | `HWARROOM.WAV` | human briefing |
| 21 | `OWARROOM.WAV` | orc briefing |
| 22 | `OWARROOM.WAV` | title |

### Constants

| Constant | Value |
| --- | --- |
| Victory bonus | 500 added to the local player's mission score |
| Rank thresholds | 2000, 5000, 8000, 18000, 28000, 40000, 55000, 70000, 85000, 105000, 125000, 145000, 165000, 185000, 205000, 230000, 255000, 280000, 300000 |
| Rank entry | 2i + 1 + race (`rank_txt.tbl`), cheat 39 |
| Narration gap | 255 ms |
| Menu timer tick | 100 ms of real time; a tick more than 300 ms late restarts the schedule (next tick due at once) |
| Fade frame | at least 16 ms; a fade of d ms ends with a frame at exactly d (about d + 16 ms in all) |
| Act screen countdown | 60 menu-timer ticks; fade 300 ms at 60, 600 ms at 50 (then the act sound); close: fade to black 400 ms, then a fade of all sound |
| Act sounds | `Sfx\oact.wav` 110,852 samples, `Sfx\hact.wav` 110,617 samples, at 22,050 Hz (5.027 s and 5.017 s) |
| Title | 100 menu-timer ticks, fades 250 ms (the fade out 1 ms if a key or click ended it) |
| Credits scroll | one step per 40 ms |

## Test vectors

| Situation | Expected |
| --- | --- |
| New campaign, ToD human, Ctrl not held | Act screen (act lines 1 and 2, `umaact1`), no video after it; briefing for entry 0; mission `Campaign\Human\Human01.pud` |
| Same, Ctrl held throughout | No act screen, no briefing; the mission starts |
| New campaign, BDP orc, archive available | Video 10, act screen (lines 17 and 18, `2xoact1`), briefing for entry 29, `Campaign\XOrc\2XOrc01.pud` |
| ToD human mission 4 won | Score screen; next mission entry 8: act screen (lines 3 and 4) then video 2; briefing with `Art\ogreknit.pcx` |
| ToD orc mission 5 lost | Score screen (`Art\olosglu.pcx`, track 13); mission 5 replayed; no act screen (act already shown); briefing shown |
| ToD human mission 14 (entry 26) won | Score screen, video 8 (by default), finale with `finale.tbl` entry 1, then credits from `crdt_lst.tbl`, then the main menu |
| BDP orc mission 12 (entry 51) won | Score screen, video 12 (by default), finale with `finale.tbl` entry 4 and three narration parts |
| Briefing for BDP human 4 | Three parts `hum4_1`, `hum4_2`, `hum4_3`; text from `rez\2xhum4.tbl`; title `objctivs.tbl` entry 94, objectives entry 35 |
| Narration parts of 10,000 and 5,000 ms | T = 10,255 + 5,255 − 255 = 15,255 ms |
| Campaign total 0 | Rank entry 1 (human) or 2 (orc) |
| Campaign total 2000 (human) | i = 1 (5000 is the first threshold above 2000): entry 3 |
| Campaign total 299,999 (orc) | i = 18: entry 38 |
| Campaign total 300,000 (human) | No threshold above: i = 18, entry 37 |
| Local player typed a cheat this mission | Rank entry 39 |
| Mission 2, total 4000 at start; the player kills units worth 900 points and wins | Mission score 1400; total 5400; the shown score counts up in steps of 540 |
| Same mission lost | No bonus; total back to 4000 for the replay |
| Footman killed by an enemy archer | Archer's player: kills + 1, mission score + the footman's points |
| Own farm destroyed by own catapult | No kill credit (same owner); the owner's buildings lost + 1 |
| A player cancels a farm site and upgrades a town hall to a keep | Buildings lost + 1 (the hall's upgrade only). Traced with a break on the lost count: `human06-build-stats-1800` (oracle trace, Enhanced): farm site 1517, cancelled at n = 662, makes no call; the only call in the run is town hall 1591 becoming a keep at n = 1468 |
| The same player completes a wall piece (`human06-build-stats-1800`, piece 1519, n = 485) | Neither built nor lost: no standing-count call and no lost-count call |
| An archer of player 1 kills a peasant of allied computer player 0 (`human05-ally-kill-hits-1000`, n = 947, break on the score step) | Player 1 is credited (a kill and the peasant's points) |
| Victory dialog, Save, then cancel | The victory dialog is shown again |
| Custom game won | Score screen (no campaign total carried; the total started at 0), then the main menu |
| Single player, slot 1 a computer allied with the local player | Row label `network.tbl` entry 36 |
| BDP orc campaign, slot 0 local, slot 3 orc computer | Names `network.tbl` entries 94 and 97 |
| **G13.** ToD human mission 1 started from a new campaign (act screen shown), then restarted from the game menu | No act screen (act 1 was the last shown); the briefing for entry 0; then the mission (static study) |

### Act screen timeline (static study only)

Times from the first tick, rounded to 10 ms, with 16 ms fade frames. The screen is assumed to open more than 300 ms
after the last menu-timer tick (the menu closing and the loading take longer), so the first tick comes at once and
the next one is due at the same moment.

| Time | Tick at countdown | What happens |
| --- | --- | --- |
| 0.00 s | 60 | Black; the text fades in (about 0.32 s) |
| 0.32 s | 59, 58 | The timer is 320 ms late, so its schedule restarts: two ticks back to back |
| 0.42 to 1.02 s | 57 to 51 | One tick every 100 ms; text only |
| 1.12 s | 50 | The picture fades in (about 0.62 s) |
| 1.74 s | | The act sound starts (orc 5.03 s, human 5.02 s; it ends at about 6.77 s) |
| 1.74 s | 49, 48 | The schedule restarts again: two ticks back to back |
| 1.84 to 6.44 s | 47 to 1 | One tick every 100 ms; the tick at 1 brings the countdown to 0 |
| 6.44 s | 0 | The screen closes: fade to black (about 0.42 s) |
| 6.86 s | | Effects still playing stop (the act sound has ended); a started music track fades out (28 steps, about 0.54 s from full volume) |
| about 6.9 s (no music started) to 7.4 s (music at full volume) | | The briefing opens (or the act's video plays) |

- If the screen opens less than 300 ms after the last menu-timer tick, the late ticks after the text fade are caught
  up instead, and every later row comes 0.12 to 0.2 s earlier (the close at about 6.24 to 6.32 s).
- A key or click (from countdown 59 on) skips to the 6.44 s row at once. Before countdown 50 that means the picture
  never shows and the act sound never plays.

## Open questions

- **BDP intro without the movie archive.** The program checks for the movie archive (on the original CD) before the
  BDP intro and returns to the main menu when it is missing. How the GOG release satisfies this check wasn't
  checked. Default: the archive is always available.
- **"Play the intro" preference.** Where the preference bit is first set (for the one-time intro) wasn't traced.
- **Quit requests.** Several screens treat one dialog event as "quit the program". Which user action causes it
  (closing the window, a system menu) wasn't traced.
- **Gathered resources.** Whether gold, lumber and oil count deliveries only, or also other income (such as
  trading or cheats), wasn't traced.
- **Multiplayer score screen.** Answered by `ui/multiplayer-score.md`.
- **Score rows: text and order (held, round 89 review).** Static study in round 89 reads every row as "name - label"
  and the local player's row first, then the other slots 0 to 7 in order, in single player too. That contradicts the
  stated rule above (slot order, "label - name"), which stays the default. Trace request MS1
  (`ui/multiplayer-score.md`): a screenshot of a single-player custom game's score screen with one computer player.
- **After a multiplayer game (held).** Static study (rounds 74 and 89) reads the main menu after the score screen of
  a game that isn't on Battle.net. Default: the multiplayer screens, as the table says. Trace request MS3
  (`ui/multiplayer-score.md`).
- **The finale's text in the oracle.** On the ToD human finale (round 72) the scrolling text never moved: one row of
  text pixels stayed at y 308 for about 4 minutes, while the program kept running. The oracle container has no
  sound device; whether the scroll waits for the narration to finish wasn't checked. So the finale's scroll, its
  credits page and the finale pointer once text scrolls are still unobserved.
- **Credits title wrap.** Where the credits title wraps (the line width) wasn't measured: "Warcraft II: Battle.net"
  fits (right edge x 455) and "Warcraft II: Battle.net Edition" doesn't, so the width is between about 315 and 450
  pixels from x 140.
- **Credits blank-line gaps.** In 6 of 17 measured gaps after an empty line the distance was 49 or 51 pixels, not
  the 50 that two pitches give. The cause wasn't found; a capture taken while the screen was being redrawn is one
  possibility, not checked. Implement 50 until a longer watch settles it.
- **Credits after page 2.** Entries 4 and 5 of `rez\creditsBNE.tbl` weren't seen on screen: whether they make a
  third page, and on which picture, wasn't watched.
- **Ending videos.** The endings' video calls weren't traced. Default: by file name, `HVict_M` / `OVict_M` for the
  ToD endings and `HVicX_M` / `OrcX_M` for the BDP endings, played before the finale dialog.
- **Campaign total on restart.** Whether a restart from the game menu puts the campaign total back to its value at
  the start of the mission, as a defeat does, wasn't traced. Default: yes.
- **Title music.** Track 22 is the same file as the orc briefing track. This wasn't checked by ear.
- **Startup-skip flag and the title (round 88, Q1 of `ERRATA-game-consolidation.md`).** Static study finds that the
  startup-skip flag (the `title` word) skips everything of "Startup" before the main menu: video 0, the one-time intro
  and the title picture, and that Ctrl is tested once more before the title picture. Step 2 above shows the title
  unless Ctrl is held. Default: step 2 as written. Trace request GA1.
