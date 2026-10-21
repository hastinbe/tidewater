# UI: music

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 26 (music; gap 10)" and
  its addendum) and the music files from `Install.mpq` (`Music\*.WAV`, measured for format and length). The
  outcome dialogs, quitting to the main menu and starting a campaign were watched on the running game (rounds 69,
  70 and 74, Enhanced); the rest is static study.
- **Changes:** 2026-10-12: the disco cheat's typing now cites `game/cheats.md`.
- **Changes:** 2026-10-14: round-67 errata (`ERRATA-round67-flow.md`; static study only): an act screen closing is a
  fade of all sound; the fade's first step comes at once, so 28 steps take 540 ms, not 560 ms (24 steps 460 ms).
- **Changes:** 2026-10-15: round-69 errata (`ERRATA-round69.md`, finding 4; victory observed on the running game,
  Enhanced campaign launch): the music while the outcome dialog is up, and the order of the fades after it closes.
- **Changes:** 2026-10-15: round-70 errata (`ERRATA-round70.md`, findings I6, I7 and R1): the defeat path watched
  (Orc01, twice); the first fade blocks the whole program; the second fade has nothing left to fade and returns at
  once (static study). Reviewed by a spec-review session, 2026-10-15; the button name follows `game/campaign-flow.md`.
- **Changes:** 2026-10-15: round-74 errata (`ERRATA-round74.md`, findings Q1 to Q4; observed on the running game,
  Enhanced, with a working sound device; the paths not reached are static study and say so): new section "Leaving a
  game for the main menu"; new rows in "Where music fades out" (choosing a campaign, the briefing closing, quitting
  to the main menu, the score screen closing, loading a game, leaving the main menu); the fade length measured at
  music volume 75; vectors; the "Running game" open question narrowed. Reviewed by a spec-review session,
  2026-10-15: the multiplayer score screen's path back is held as an open question with a trace request (static
  study contradicted `game/campaign-flow.md`); the "Main menu opens" row now also covers a return without a fade.
- **Changes:** 2026-10-21, round 91 (oracle trace, `ERRATA-round91-traces.md`; reviewed by a spec-review session,
  2026-10-21): after a multiplayer game's score screen the creator goes back to the lobby (traced; the music there
  isn't traced); Ctrl+Q in a chat-room game opens the quit-to-menu confirmation (traced).
- **Changes:** 2026-10-15: round-75 errata (`ERRATA-round75.md`, U1 and U2; observed on the running game, Enhanced,
  backed by static study): what a fade of all sound does to sound effects and narration (cut at once, with no
  exception); the act screen and the briefing stop every sound effect, their narration included, before their fade;
  a new row for Restart Scenario.

## Behaviour

Music is one streamed track at a time, played from a file in the `Music` folder of the install archive. This spec
says which track plays when, how a track ends, loops or leads to the next, how music fades and how the music volume
maps to loudness. Sound effects are in `ui/sounds.md`. The game-menu sound dialog and the Ctrl+M volume swap are in
`ui/game-menu.md`; this spec covers only their effect on the music.

### Tracks

There are 26 numbered tracks (0 to 25), listed under "Data". Each names a file and has one of three end modes:

- **Once:** the file plays to its end, then music is silent.
- **Loop:** the file loops without a gap. When it reaches its end it continues from its loop point, which is the
  start of the file except for track 20 (below).
- **Chain:** when the file ends, music is silent for 2 seconds, then the track's next track starts. The chains form
  rings, so chained music never stops by itself.

Starting a track:

1. **Same track.** If the requested track is the one already loaded, it restarts from its start (or simply carries on
   when the start is a "start if silent" request, below).
2. **Tides of Darkness bump.** If the requested track is 0 or 6 and the game is a single-player campaign game of
   *Tides of Darkness* (campaign entry below 28; not a custom scenario, not multiplayer), track 1 or 7 plays instead.
   This applies to every start, including a chain moving on from track 5 or 11, so in the first campaign the files
   `HUMAN1.WAV` and `ORC1.WAV` never play.
3. **Load.** The old track stops at once, without a fade, and the new file is loaded.
4. **Volume.** If the music volume is 0, the track is chosen but not played (see "Volume").

Track −1 means no music.

### Start if silent

Several screens start their track only if no music is going. Two different tests are used:

- **Playing:** a track is playing if its position is before its end. Main menu and briefing use this test.
- **Started:** a track counts as started from the moment it begins until one of the stops below closes it: the fade
  stops, Ctrl+M muting, or a volume of 0. A once-track that has reached its end still counts as started. The credits
  and the in-game start use this test.

So a credits or in-game start right after a score-screen track (a once-track) has finished does not begin a new
track unless something stopped the music in between. In the normal flows something does: the game end and the
credits' close fade all sound out (see the next table). See also "Open questions".

### Where tracks start

| Moment | Track | Condition |
| --- | --- | --- |
| Program start, title screen | 22 | always |
| Main menu opens | 22 | only if not playing; tested once the menu is drawn, after its 200 ms fade-in when it fades in (see "Leaving a game for the main menu"); coming back from another menu on its picture there is no fade-in and 22 is still playing |
| Mission briefing opens | 20 (human) or 21 (orc) | only if not playing |
| A game starts or a saved game is loaded | the game's track (next section) | only if not started |
| Score screen | victory: 14 (human) or 15 (orc); defeat: 12 (human) or 13 (orc) | always |
| Credits | 23 | only if not started |
| Campaign finale | 16 (human) or 17 (orc) | always, after a fade of all sound |
| Disco cheat toggled | 25 or the game's normal track | only if playing (see "Disco cheat") |

Human or orc means the local player's race. Track 22 and track 21 are the same file; the title and the main menu use
track 22.

### Where music fades out

A **fade stop** lowers the music, step by step, until it is silent, then closes the track. It blocks: nothing else
happens until it ends. A **fade of all sound** does the same while all sound effects are cut.

**Effects and narration.** A fade of all sound first stops every sound effect that is playing, at once (a cut, not a
fade), then fades the music (static study, round 75; the event log shows no cut made by a fade itself: the act screen
and the briefing had stopped every effect just before theirs, and none was logged at the in-game Restart or quit).
Narration is a sound effect: the act screen's and the briefing's speech play through the same channels as the game's
sounds (observed, round 75), so the fade cuts it too. No sound is spared: no sound ever carries a mark that would keep
it through the fade (static study, round 75). The act screen and the briefing, when they close, also stop every sound
effect just before their fade of all sound (observed: the briefing's narration stopped by Continue, round 75), so
their narration ends at the click, not after the fade.

| Moment | Kind |
| --- | --- |
| Entering Battle.net from the multiplayer menu | fade stop |
| Sound shutdown when the program exits | fade stop |
| A game ends (before the score screen) | fade of all sound |
| Credits close | fade of all sound |
| The attract demo starts | fade of all sound |
| Campaign finale starts | fade of all sound |
| A custom game starts from the custom-scenario screen | fade of all sound |
| Disco cheat toggled while music is playing | fade of all sound |
| An act screen closes, after its fade to black (`game/campaign-flow.md`) | fade of all sound |
| A campaign is chosen on the new-campaign menu (any of the four), before its 200 ms fade to black | fade of all sound (observed for both *Tides of Darkness* choices, round 74; the *Beyond the Dark Portal* choices static study) |
| The briefing closes (Continue or Enter), before its 100 ms fade to black | fade of all sound (observed, round 74; closing by itself static study) |
| The player confirms "Quit to Menu" in a game (from the game menu or Ctrl+Q), as the game stops | fade of all sound, then the game end's own fade of all sound (observed, round 74) |
| The player confirms "Restart Scenario" (game menu, End Scenario), as the game stops | fade of all sound of the game's track (536 ms at volume 75), the 1 ms fade to black, then the game end's own fade of all sound (nothing left to fade); the briefing then opens with its track from the start (observed, round 75) |
| The score screen closes, before its 150 ms fade to black | fade of all sound (static study) |
| A saved game is loaded from the load-game screen of the menus | fade of all sound (static study) |
| The main menu's Replay Introduction, Show Credits or Exit Program | fade of all sound (static study) |
| A request to close the program while the single-player menu is up | fade of all sound, then the program exits (static study) |

Going back from one menu to another on the main menu's picture (Previous Menu on the single-player or campaign menu,
Escape on the multiplayer menu) fades nothing, so track 22 plays on (observed, round 74).

The fade starts from the current attenuation and, every 20 ms, lowers it by 0.10 dB, then 0.20 dB, then 0.30 dB, and
so on (each step 0.10 dB bigger). It ends at the first step that takes it below −40.00 dB; music is then silent
and the track closed. The first step comes at once and the next ones 20 ms apart, so from full volume (−0.20 dB) it
takes 28 steps over 540 ms. From music volume 75 (−2.48 dB) it takes 27 steps over 520 ms; the running game
measured 519 to 531 ms each time (round 74).

### The outcome dialog

When a single-player game is won or lost, the game shows its victory or defeat dialog over the map
(`game/campaign-flow.md`, "The victory and defeat dialogs"). Nothing about the music changes while it is up: no track
starts, nothing fades, and the game's track keeps playing. Since the music keeps running, a track that reaches its end
under the dialog fades and moves on to the next one as in "End of a track" (static study; not seen).

When the player closes the dialog with the button that ends the game (the victory dialog's "Victory" or the defeat
dialog's "OK", `game/campaign-flow.md`):

1. A fade of all sound from the current volume (28 steps over 540 ms from full volume), which blocks: the program
   does nothing else until it ends, so the picture stays as it is and no input is read. The wait is there only while
   music is playing; with no music it returns at once (static study, round 70).
2. The game ends: the screen fades to black in 1 ms (`ui/palette-effects.md`, "Game end"), and a second fade of all
   sound runs. The music is already closed by then, so it only stops any sound effects still playing (none, since
   the first fade cut them) and returns at once, with no wait (static study). Leaving it out changes nothing that can
   be heard or seen.
3. The score screen opens and starts its track at once: 14 or 15 for a victory, 12 or 13 for a defeat (the local
   player's race). With the skip setting on there is no score screen (`game/campaign-flow.md`), so no score track.

So the in-game track is heard until the click and is silent before the score screen's track starts; the two never
overlap. The victory path through "Victory" was seen on the running game (a Human01 campaign mission, Enhanced; the
events came in this order within about 45 ms, since the container had no sound device and the fades returned at
once). The defeat path through "OK" was seen in round 70 (Orc01, Enhanced, twice): the same events in the same order,
with track 13.

### Leaving a game for the main menu

The player leaves a single-player game for the main menu through the confirmation "Quit to Menu"
(`rez\quit2mnu.bin`). It is reached two ways, both observed (round 74, Enhanced):

- the game menu's End Scenario, then Quit to Menu (`ui/game-menu.md`);
- Ctrl+Q in a single-player game, which opens the same confirmation at once (`ui/game-menu.md`; a chat-room
  multiplayer game too, traced in round 91; on Battle.net the key surrenders instead, static study).

Enter confirms it as clicking its Quit to Menu button does (observed). When it is confirmed:

1. **Fade of all sound.** The game stops and a fade of all sound runs from the current volume. It blocks: the game
   picture stays on screen, unchanged, until it ends (about 520 ms at music volume 75). The game's track fades out
   and is closed.
2. **Game end.** The screen fades to black in 1 ms and the game end's second fade of all sound runs. Nothing is open
   by then, so it returns at once. No score screen follows.
3. **Main menu.** The main menu is drawn and fades in over 200 ms, with no music.
4. **Track 22.** After the fade-in, the main menu's test finds no track playing (the game's track was closed in step
   1), so track 22 starts from its start, about 230 ms after the menu began to open and before its buttons take input.

So the game's track never carries into the main menu, and nothing overlaps: silence from the end of the fade, about
230 ms, then track 22. Seen three times (human twice, orc once); an attract demo ended by a key went through the
same steps.

**The paths from the other screens.** The briefing has no way back to the main menu: Escape does nothing (observed),
and its Continue button and Enter go on to the game. A request to close the program on the briefing or the act screen
quits the program (`game/campaign-flow.md`). The act screen's keys only close it early, on to the briefing. After a
custom game, closing the score screen leads to the main menu (`game/campaign-flow.md`, "After the score screen"):
its fade of all sound closes the score track, so track 22 starts as above (static study). After a multiplayer game
the game creator goes back to the lobby of the same session (traced, round 91; `game/campaign-flow.md`), not the
main menu. The score screen's fade of all sound closes the score track there too; what plays in the lobby then isn't
traced (default, gap-fill: nothing, since no row of "Where tracks start" covers the lobby).

**Starting a campaign afterwards.** Whatever was playing on the menu (normally track 22), choosing a campaign fades
all sound before the screen goes black, so no music is playing at the act screen and the briefing:

| Next campaign | Act screen | Briefing | Game start |
| --- | --- | --- | --- |
| The same race and act as the game just left (for example human after a human mission 1) | not shown (the same act as the last one shown, `game/campaign-flow.md`) | nothing is playing: track 20 (human) or 21 (orc) starts from its start | the briefing's close fades it out, so the game's track starts |
| The other race | shown; its close is a fade of all sound with nothing open | track 21 or 20 starts | as above |

Both rows were seen on the running game (round 74: human after human, orc after human). The briefing's "only if
not playing" and the game start's "only if not started" therefore always pass on these paths; track 22 never reaches
the briefing.

### The game's track

Each game has a current track, set when a new game starts and saved with the game (a loaded game uses the saved
one):

- **Campaign mission:** 6 × race + ((campaign entry ÷ 2, rounded down) mod 6), where race is 0 for human and 1 for
  orc.
- **Any other game** (custom scenario, multiplayer): 6 × race + (d mod 6), where d (0 to 32767) is one draw from
  the **setup-shuffle generator** (`core/timing-and-rng.md`), taken after the map has loaded. This is not the
  simulation generator, so the pick never affects the game. The same generator picks sound variants
  (`ui/sounds.md`), so in a multiplayer game each machine may pick a different track.
- **Disco:** if the disco cheat is on when the game starts, the track is 25 instead and the cheat is turned off.

All tracks 0 to 11 are chained (human 0→1→…→5→0, orc 6→…→11→6), so a game's music runs through the race's six
files in order, starting from the chosen one, with 2 s of silence between files.

### Disco cheat

The disco cheat toggles track 25 (`DISCOWC.WAV`, looped) as the game's track:

- Turning it on sets the game's track to 25.
- Turning it off sets the game's track back by the rule above: the campaign formula for a campaign mission, otherwise a
  new draw from the setup-shuffle generator.
- Either way, if music is playing, all sound fades and the new track starts (if not started). If music isn't
  playing, only the game's track changes and is heard the next time it starts.

How the cheat is typed is in `game/cheats.md`.

### End of a track

While a track plays, its position is checked every frame:

- **End fade.** For once and chain tracks longer than 616,000 bytes of audio (about 6.98 s; every shipped file is
  longer), when fewer than 616,000 bytes remain the music fades: every 20 ms the
  attenuation drops by 0.10 dB until it is below −33.96 dB, and then it is silent. Loop tracks never fade. From full
  volume the fade takes 338 steps (6.76 s), so it ends just before the file does.
- **End.** When the position reaches the end, the music goes silent and the track ends.
- **Chain.** For a chain track, after the end the game waits 2 seconds (measured from the first check after the end),
  restores the music volume and starts the next track.

### Volume

The music volume is a setting from 0 to 100 (`ui/game-menu.md`). It maps to an attenuation through the volume table
under "Data": volume v plays at table entry v, in hundredths of a decibel. The table isn't a formula (entry 2
jumps to −33.96 dB, and entries 98 to 100 are all −0.20 dB), so it has to be copied.

Every time the music volume is applied (at program start, when the sound dialog's countdown ends, when the sound
dialog is cancelled, when the game menu closes, and on Ctrl+M) the same rules hold:

- **Volume 0:** music goes silent, the track is stopped and closed, and the game forgets that a restart was done.
- **Nonzero volume:** the new attenuation applies to the open track at once. Then, if no restart has been done since
  the last volume 0 (or since program start), one is done now: if no track is open, or the open track isn't started,
  the current track starts from its start. Later nonzero changes only change the loudness.

So raising the volume from 0, by Ctrl+M or the sound dialog, restarts the current track from its start, once. At
program start the current track is "none", so the first apply plays nothing; the title screen starts track 22.

Sound effects use the same table (`ui/sounds.md`): effects at volume V use entry 1 + floor(V × 99 / 100), quiet
effects entry 1 + floor(V × 24 / 100).

**Ctrl+M** (see `ui/game-menu.md`) sets the music volume to 0, which stops the music, or restores the remembered
volume (25 if none), which restarts the current track from its start (the rules above).

## Data

### Tracks

| Track | File (`Music\`) | Mode | Next | Loop point |
| --- | --- | --- | --- | --- |
| 0 | `HUMAN1.WAV` | chain | 1 | — |
| 1 | `HUMAN2.WAV` | chain | 2 | — |
| 2 | `HUMAN3.WAV` | chain | 3 | — |
| 3 | `HUMAN4.WAV` | chain | 4 | — |
| 4 | `HUMAN5.WAV` | chain | 5 | — |
| 5 | `HUMAN6.WAV` | chain | 0 | — |
| 6 | `ORC1.WAV` | chain | 7 | — |
| 7 | `ORC2.WAV` | chain | 8 | — |
| 8 | `ORC3.WAV` | chain | 9 | — |
| 9 | `ORC4.WAV` | chain | 10 | — |
| 10 | `ORC5.WAV` | chain | 11 | — |
| 11 | `ORC6.WAV` | chain | 6 | — |
| 12 | `HDEFEAT.WAV` | once | — | — |
| 13 | `ODEFEAT.WAV` | once | — | — |
| 14 | `HVICTORY.WAV` | once | — | — |
| 15 | `OVICTORY.WAV` | once | — | — |
| 16 | `HUMAN5.WAV` | chain | 17 | — |
| 17 | `HUMAN6.WAV` | chain | 16 | — |
| 18 | `ORC5.WAV` | chain | 19 | — |
| 19 | `ORC6.WAV` | chain | 18 | — |
| 20 | `HWARROOM.WAV` | loop | — | sample frame 66,596 (3.020 s) |
| 21 | `OWARROOM.WAV` | loop | — | start |
| 22 | `OWARROOM.WAV` | loop | — | start |
| 23 | `HUMAN4.WAV` | chain | 24 | — |
| 24 | `HUMAN5.WAV` | chain | 23 | — |
| 25 | `DISCOWC.WAV` | loop | — | start |

Tracks 18 and 19 are never started.

### Files

All music files are PCM WAV, 22,050 Hz, 16-bit stereo (88,200 bytes per second; 4 bytes per sample frame).

| File | Length (s) | File | Length (s) | File | Length (s) |
| --- | --- | --- | --- | --- | --- |
| `HUMAN1.WAV` | 166.43 | `HUMAN2.WAV` | 147.00 | `HUMAN3.WAV` | 123.97 |
| `HUMAN4.WAV` | 126.64 | `HUMAN5.WAV` | 177.23 | `HUMAN6.WAV` | 207.53 |
| `ORC1.WAV` | 139.23 | `ORC2.WAV` | 120.89 | `ORC3.WAV` | 137.14 |
| `ORC4.WAV` | 140.99 | `ORC5.WAV` | 137.15 | `ORC6.WAV` | 204.92 |
| `HWARROOM.WAV` | 51.02 | `OWARROOM.WAV` | 38.24 | `HVICTORY.WAV` | 16.61 |
| `OVICTORY.WAV` | 32.01 | `HDEFEAT.WAV` | 26.66 | `ODEFEAT.WAV` | 32.01 |
| `DISCOWC.WAV` | 132.29 | | | | |

### Constants

| Constant | Value |
| --- | --- |
| Gap between chained tracks | 2,000 ms |
| End-fade window | last 616,000 bytes of audio (6.984 s) |
| End-fade step | −0.10 dB every 20 ms, until below −33.96 dB |
| Fade-stop steps | the first at once, then every 20 ms, −0.10 dB × n on step n, until below −40.00 dB |
| Silent | −100.00 dB |
| Tides of Darkness bump | tracks 0 and 6 become 1 and 7 when campaign entry < 28 |
| Remembered music volume for Ctrl+M at start-up | 25 |

### Volume table

Attenuation in hundredths of a decibel for volume v = 1 to 100 (ten per row; row r holds v = 10r + 1 to 10r + 10):

| v | +1 | +2 | +3 | +4 | +5 | +6 | +7 | +8 | +9 | +10 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | −4000 | −3396 | −3044 | −2794 | −2602 | −2442 | −2308 | −2192 | −2090 | −2000 |
| 10 | −1917 | −1840 | −1772 | −1706 | −1646 | −1590 | −1539 | −1489 | −1442 | −1396 |
| 20 | −1354 | −1315 | −1275 | −1238 | −1204 | −1170 | −1137 | −1104 | −1075 | −1044 |
| 30 | −1017 | −988 | −961 | −937 | −910 | −887 | −862 | −840 | −816 | −794 |
| 40 | −774 | −752 | −733 | −713 | −692 | −674 | −654 | −636 | −618 | −602 |
| 50 | −583 | −566 | −551 | −535 | −519 | −502 | −488 | −473 | −458 | −442 |
| 60 | −429 | −415 | −401 | −386 | −374 | −359 | −346 | −333 | −322 | −308 |
| 70 | −297 | −285 | −273 | −260 | −248 | −238 | −227 | −214 | −203 | −192 |
| 80 | −183 | −172 | −160 | −151 | −141 | −131 | −119 | −111 | −101 | −90 |
| 90 | −80 | −72 | −63 | −52 | −43 | −35 | −26 | −20 | −20 | −20 |

## Test vectors

| Situation | Expected |
| --- | --- |
| *Tides of Darkness* human mission 1 (entry 0) starts | Game track 0, bumped: `HUMAN2.WAV` (track 1) plays |
| *Tides of Darkness* orc mission 5 (entry 9) | Game track 6 + (4 mod 6) = 10: `ORC5.WAV` |
| *Tides of Darkness* human mission 7 (entry 12) | 0 + (6 mod 6) = 0, bumped to 1: `HUMAN2.WAV` |
| *Beyond the Dark Portal* human mission 3 (entry 32) | 0 + (16 mod 6) = 4: `HUMAN5.WAV` |
| *Beyond the Dark Portal* orc mission 1 (entry 29) | 6 + (14 mod 6) = 8: `ORC3.WAV` |
| *Beyond the Dark Portal* human mission 2 (entry 30), track 5 ends | 2 s silence, then track 0, `HUMAN1.WAV` (no bump: entry ≥ 28) |
| *Tides of Darkness* campaign, track 5 ends | 2 s silence, then `HUMAN2.WAV` (0 bumped to 1) |
| Custom scenario as orc, setup-shuffle draw d = 15 | Track 6 + 3 = 9, `ORC4.WAV` |
| Custom scenario as human with the disco cheat on | Track 25 looped; the cheat is off afterwards |
| Music volume 100, track 3 near its end | Fade starts 616,000 bytes before the end; silent after 338 × 20 ms; then 2 s gap; track 4 |
| Briefing as human | Track 20: plays the file from the start, then loops from 3.020 s with no gap |
| Main menu opens while the title music still plays | Title music carries on, not restarted |
| Fade stop at volume 100 (−20) | 28 steps, the last at 540 ms; −20 − 10 × (1 + … + 28) = −4080 |
| Fade stop at volume 25 (−1204) | 24 steps, the last at 460 ms; −1204 − 3000 = −4204 |
| Music volume 50 | Attenuation −6.02 dB |
| Ctrl+M with music volume 75, then Ctrl+M again | Music stops; then volume 75 again and the current track restarts from its start |
| Sound dialog: music volume 0 applied (countdown ends), then 40 applied | The current track starts from its start |
| Sound dialog: music volume 40 → 60 while a track plays | Only louder; the track carries on |
| First Ctrl+M of a run with music volume 0 | Volume 25, the current track starts |
| Score screen, human victory | Track 14 plays once (16.61 s), fading over its last 6.98 s, then silence |
| Human campaign mission won; the victory dialog stays up 30 s, then "Victory" clicked | The game's track plays on for the whole 30 s with no change; at the click a fade of all sound (540 ms at full volume), a 1 ms fade to black, then track 14 starts as the score screen opens |
| Human campaign mission lost; "OK" clicked on the defeat dialog | As above, then track 12 (static study only) |
| Orc campaign mission (Orc01) lost through the defeat cheat; the defeat dialog stays up 8 s, then "OK" clicked | Nothing starts or fades while the dialog is up; at the click a fade of all sound, the game end, a second fade of all sound, the score screen, then track 13 starts (seen on the running game, round 70, twice; about 40 ms in all with no sound device) |
| Fade of all sound at music volume 75 (−248), a track playing | 27 steps, the last at 520 ms; −248 − 10 × (1 + … + 27) = −4028 (round 74: 519 to 531 ms measured) |
| ToD human mission 1 in progress (track 1 playing), game menu, End Scenario, Quit to Menu, Quit to Menu (round 74) | Fade of all sound (about 520 ms at volume 75; the game picture stays), 1 ms fade to black, a second fade that returns at once, main menu 200 ms fade-in in silence, then track 22 from its start |
| ToD orc mission 1 in progress, Ctrl+Q, Enter (round 74) | The quit-to-menu confirmation opens at once; Enter confirms; then as above |
| Main menu after quitting a game; Single Player Game, then Previous Menu | Track 22 plays on, not restarted |
| Main menu with track 22 playing; New Campaign; ToD human, after a ToD human mission 1 was quit (round 74) | Fade of all sound (track 22 out), 200 ms fade to black, no act screen, briefing: track 20 from its start; Continue: fade of all sound, 100 ms fade to black, game start: track 1 |
| The same, but ToD orc chosen (round 74) | Fade of all sound, 200 ms fade to black, act screen (its close fades nothing audible), briefing: track 21; Continue: fade, game start: track 7 |
| Briefing open, Escape pressed (round 74) | Nothing happens; the briefing and its track carry on |
| ToD orc chosen (round 75) | Act screen: its narration starts as a sound effect about 1.8 s after the act screen opens; at its close: every effect stopped, then the fade of all sound. Briefing: track 21 from its start, then its narration starts as a sound effect about 130 ms later |
| Briefing open with its narration started; Continue clicked (round 75) | Every sound effect stopped at the click, then the fade of all sound (music, about 520 ms), the 100 ms fade to black, the game starts and its track starts (as in round 74) |
| Orc mission 1 in play, track 8 playing; game menu, End Scenario, Restart Scenario, Restart Scenario (round 75) | Fade of all sound (536 ms), 1 ms fade to black, game end (second fade, nothing to fade), every effect stopped, no act screen, briefing: track 21 from its start and its narration again |

## Open questions

- **Started versus playing.** The in-game start and the credits start their track only if no track counts as started,
  and a finished once-track still counts. Whether a path exists where this leaves the game silent (for example, a
  saved game loaded from the score screen without a fade in between) hasn't been traced. Default: start the screen's
  track when no music is audible.
- **Order of the music draw.** The draw for a non-campaign game's track comes after the map has loaded. Its position
  among the other setup-shuffle draws (start-position and colour shuffles, sound variants) hasn't been pinned. It can't
  affect sync, only which track is picked, and multiplayer machines may already differ. Default: draw it after every
  setup draw for the map.
- **Unused tracks.** Tracks 18 and 19 are never started; kept for completeness.
- **Network drive.** The game refuses to stream music from a network drive and shows a fixed message (not from a
  string table) on the message line. Default: play from wherever the data is.
- **Running game.** Watched so far: the outcome dialogs (rounds 69 and 70), quitting to the main menu and starting a
  campaign afterwards (round 74). Round 74 had a working sound device but no speaker, so the music was followed
  through the program's own playing tests and fade timings, not heard. In the oracle the sound device runs about 3.5
  times faster than real time, so how long a track plays before its end fade, and the 2 s chain gap, can't be
  measured there; the fade-stop timing can (it is timed by the clock, not the sound device). Default: the static
  study under "End of a track".
- **Score screen to main menu.** Closing the score screen after a custom or multiplayer game, loading a game from
  the menus and the main menu's Replay Introduction, Show Credits and Exit Program weren't run. Default: the fade of
  all sound the table lists.
- **Multiplayer score screen: the lobby's music.** Where it goes is traced for the creator after a victory (the
  lobby of the same session, round 91); the oracle records no sound. Default: the score screen's fade of all sound
  closes the score track and nothing starts in the lobby ("Where tracks start" has no lobby row). Trace request MS3b
  (`ui/multiplayer-score.md`) covers the joiner and a defeat; the music needs a run with a sound device.
- **Outcome dialog: fade length.** The fade after the outcome dialog wasn't timed with a sound device. The same fade
  of all sound, timed on the quit-to-menu path in round 74, matched the formula (520 ms at volume 75), so the
  default is 540 ms at full volume. The second fade returns at once (static study, round 70; seen on the quit path,
  round 74).
- **Outcome dialog: save path.** The victory dialog's save path (a save, then the game ends with a victory) wasn't
  watched. Default: the same sequence as "Victory".
