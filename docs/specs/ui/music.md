# UI: music

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 26 (music; gap 10)" and
  its addendum) and the music files from `Install.mpq` (`Music\*.WAV`, measured for format and length). Not yet
  confirmed against the running game.
- **Changes:** 2026-10-12: the disco cheat's typing now cites `game/cheats.md`.
- **Changes:** 2026-10-14: round-67 errata (`ERRATA-round67-flow.md`; static study only): an act screen closing is a
  fade of all sound; the fade's first step comes at once, so 28 steps take 540 ms, not 560 ms (24 steps 460 ms).

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
| Main menu opens | 22 | only if not playing |
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

The fade starts from the current attenuation and, every 20 ms, lowers it by 0.10 dB, then 0.20 dB, then 0.30 dB, and
so on (each step 0.10 dB bigger). It ends at the first step that takes it below −40.00 dB; music is then silent
and the track closed. The first step comes at once and the next ones 20 ms apart, so from full volume (−0.20 dB) it
takes 28 steps over 540 ms.

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
- **Running game.** None of this has been watched or heard in the running game; the fades in particular should be
  checked by ear.
