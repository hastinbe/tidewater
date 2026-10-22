# Game: the life of a game (model)

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 88, B-038 consolidation of the game area,
  `ERRATA-game-consolidation.md`)
- **Sources:** the six game specs as approved at tidewater-docs `e45ac76` (`game/setup.md`, `game/victory.md`,
  `game/rescue.md`, `game/cheats.md`, `game/campaign-flow.md`, `game/save-load.md`), the game-level rules of
  `core/timing-and-rng.md` and `ui/game-menu.md`, every errata file that touches the game area, the committed oracle
  traces, and static study of the 2.02 program where the specs had gaps (each such fill says "static study").
- **Changes:** 2026-10-21, round 88: new file. It is the model of a game from program start to the next mission and the
  map of where each rule lives. The rules stay in the part specs, each said once; this file adds the program flow, the
  two ways a game starts (new or loaded) side by side, the game-level work of a cycle, and one table of the per-game
  counts with when each is reset, whether it is saved and who reads it. Review (2026-10-21): what a load does to the
  victory countdown, a found outcome and the cheat-used mark is held as open questions (Q4, Q5) with the stated rules
  as the defaults.
- **Changes:** 2026-10-21, round 90 (oracle traces with save and load, `ERRATA-round90-traces.md`; reviewed by a
  spec-review session, 2026-10-21): Q1, Q2, Q4 and Q5 are answered by traces, each confirming the static reading: the
  `title` word skips the title picture, a loaded game's counter starts at 0 again, a load sets the victory countdown
  to 1 and loses a found outcome, and a load keeps the cheat-used mark. The fog refresh countdown isn't saved
  (traced) and a loaded game is never paused (static gap-fill). G06 and G07 now state the traced results.
- **Changes:** 2026-10-21, round 91 (oracle trace, `ERRATA-round91-traces.md`; reviewed by a spec-review session,
  2026-10-21): Q3 answered by a trace, confirming the static reading: a load runs the mission start again
  (`game/victory.md` G24).
- **Changes:** 2026-10-21, round 92 (oracle traces, `ERRATA-round92-traces.md`; reviewed by a spec-review session,
  2026-10-21): the comparison and count tables add the 50-cycle pass counters (not saved), the doubled training group
  counts and the first creation number after a load (all traced; `game/save-load.md` SL1 to SL7).
- **Changes:** 2026-10-21, round 93 (oracle traces, `ERRATA-round93-traces.md`; reviewed by a spec-review session,
  2026-10-21): the comparison and count tables add the computer research requests and done marks (cleared and made
  again by a load, SL9 to SL12) and widen the doubled training group counts to footmen through submarines (SL13 to
  SL15; four groups a labelled gap-fill).
- **Changes:** 2026-10-21, round 94 (oracle traces, `ERRATA-round94-traces.md`; reviewed by a spec-review session,
  2026-10-21): the start table adds the scripts' openings (a new game runs them before the map's units are placed; a
  load runs none; AS-V1, AS-V2). The load's building marks count construction sites, and every training group's
  doubling is traced (SL16 to SL21).
- **Changes:** 2026-10-22, round 95 (oracle traces, `ERRATA-round95-traces.md`; reviewed by a spec-review session,
  2026-10-22): the start and count tables add the computer job choice's counts (0 at every game start, a load
  included, while the loaded workers keep their jobs; 16-bit, wrapping; SL22 to SL25). The load's marks for a known
  spell and an upgrading building are traced (SL26, SL27).

## Behaviour

A **game** is one run of the simulation on one map: from the moment the map is read (or a saved game is restored)
until the outcome screen closes or the player leaves. The program around it is a small set of screens that choose the
next game. This spec says how the pieces fit; each rule is in the spec named next to it.

### The parts

| Part | What it decides | Spec |
| --- | --- | --- |
| Startup and menus | Videos, title, main menu | `game/campaign-flow.md`, "Startup" |
| Campaign flow | Which mission comes next, act screen, briefing, endings | `game/campaign-flow.md` |
| Game start from a map | Sections, players, controllers, starting resources, alliances, units, view | `game/setup.md` |
| Game start from a saved game | What is restored and what starts afresh | `game/save-load.md` |
| The cycle | Scheduler, start-up cycles, phases, pause, random numbers | `core/timing-and-rng.md` |
| Victory and defeat | The check, the rules, mission start | `game/victory.md` |
| Rescue | Joining, circles of power, the rescued counts | `game/rescue.md` |
| Cheats | Codes, flags, mission select, developer words | `game/cheats.md` |
| Game menu | Pause, restart, surrender, quit, save and load buttons | `ui/game-menu.md` |
| Outcome screens | Victory and defeat dialogs, score screen, campaign total | `game/campaign-flow.md` |

### Program flow

1. **Startup** (`game/campaign-flow.md`, "Startup"): command-line words are applied (`game/cheats.md`,
   "Command-line arguments"), then the startup videos and the title picture, unless they are skipped. A mission-select
   word on the command line can start a mission straight away (`game/cheats.md`, "Mission select").
2. **Main menu.** From here a game starts in one of five ways:

   | Way | Kind of game | Before the game |
   | --- | --- | --- |
   | New campaign | campaign, first mission of the chosen campaign | BDP intro (BDP only), act screen, briefing |
   | Custom scenario | custom | none |
   | Multiplayer | multiplayer | the lobby (a later spec) |
   | Load | whatever was saved | none (no act screen, no briefing) |
   | Idle at the main menu | demo playback | none |

3. **Starting a mission** (campaign only, `game/campaign-flow.md`, "Starting a mission"): the act screen check, then
   the briefing.
4. **Game start**: a new game from the map (`game/setup.md`, steps 1 to 9) or a loaded game (`game/save-load.md`).
   "How a game starts" below compares the two.
5. **The two start-up cycles**, run at once, then normal cycles on the scheduler (`core/timing-and-rng.md`,
   "Scheduler"). Both kinds of game start run them (static study; traced for a load, round 90).
6. **Play.** Commands, cheats, pause, saves and the game menu act during play (see "Where players act").
7. **Outcome.** The victory check decides the outcome (`game/victory.md`). In single player it shows at the next
   check, 201 cycles after the check that found it. Surrender and quit from the game menu give a defeat at once
   (`ui/game-menu.md`).
8. **Outcome dialog** (single player), then the **score screen** unless the skip setting is on
   (`game/campaign-flow.md`).
9. **Next**: the next mission (back to 3), the same mission again after a defeat (back to 3), an ending, the main
   menu, or the multiplayer screens (`game/campaign-flow.md`, "After the score screen"). A restart from the game menu
   goes back to 3 with no outcome and no score screen. Mission select ends the game and goes back to 3 with the chosen
   mission.

### How a game starts

Both kinds of game start go through the same steps; the table says what each step does in each. "Static study"
marks rules the old text didn't state.

| Step | New game (from a map) | Loaded game |
| --- | --- | --- |
| Cheat flags | All cleared except mission select, the startup-skip flag and the skip setting (`game/cheats.md`, "Flags across games") | The same |
| Cheat-used mark (rank 39) | Cleared (static study) | Not saved (static study) and not cleared: it stays as the running program left it (traced, round 90) |
| Campaign total | Reset to 0 for multiplayer, custom games and the first mission of a campaign; otherwise carried over (`game/campaign-flow.md`, "Campaign total") | Restored from the save |
| Mission scores and the score screen's statistics | Start at 0 (`game/campaign-flow.md`) | Restored from the save (Enhanced) |
| Rescued counts | Set to 0 for every player (static study) | Restored from the save |
| Mission rule (mode) | Taken from the mission table (campaign) or the standard rule (`game/victory.md`) | Restored from the save, already switched if it was switched |
| Map, players, units, resources, alliances | Read from the map (`game/setup.md`) | Restored from the save |
| Computer player scripts (`ai/ai-scripts.md`, "Game start") | Every player's script starts and runs its opening **before the map's units are placed**, with every count its conditions read at 0; the map's units are counted after it. An opening's final "wait until" is tested again in the second start-up cycle, so the instructions after it run at trace record 4 at the earliest (static study; traced, round 94: AS-V1, AS-V2) | No script runs: each player's script state (wait counter, position, slots, the build-list limit included) is restored from the save (traced for the limit, round 94: AS-V1) |
| Music track | Chosen (`ui/music.md`) | Restored from the save |
| Random numbers | Seeded (`core/timing-and-rng.md`) | Seeded the same way (`game/save-load.md`) |
| Mission start (`game/victory.md`, "Mission start") | Runs before the first check | Runs again from the saved mode, before the first start-up cycle; a mode switched before the save (E, H) doesn't run its row again (traced for mode AB, round 91) |
| Victory check's rule | Chosen from the mode | Chosen from the saved mode, which keeps any switch made before the save |
| Victory countdown and a found outcome | Countdown set to 1 (1,400 for demo playback); no outcome | The same as a new game: countdown 1, and an outcome found but not yet shown is lost (traced, round 90) |
| Fog refresh countdown | Set so that the first start-up cycle refreshes | The same; the saved value isn't used (traced, round 90) |
| Pause | Not paused | Not paused: the pause isn't saved (static study, round 90 review) |
| Start-up cycles | Two | Two (traced, round 90) |
| Cycle counter | 0 when the first normal cycle starts | The same: 0 again, whatever was saved (traced, round 90) |
| Creation numbers | From 1, one per created unit (`core/unit-slots.md`) | Every unit's number becomes its slot number plus one (traced, round 90); the next created unit gets 1626 in Enhanced (traced, round 92) |
| 50-cycle pass counters (`ai/attack-ai.md`, "Timing") | Start at 1, so the passes run in the first start-up cycle | The same: not saved, start at 1 (traced, round 92) |
| Training group counts, footmen to submarines (`ai/training-research.md`) | Counted from the map's units | Restored from the save, then every loaded unit is counted again: doubled (traced for every group, rounds 92 to 94; held map ships are never counted). The worker and tanker counts are worked out afresh (traced, round 93) |
| Computer research requests and build-list done marks (`ai/training-research.md`, `ai/worker-ai.md`) | Requests clear; done marks from levels, researched items and known spells, then a creation mark for each map building | Not kept: requests clear; done marks from levels, researched items and known spells, then the load's building marks for each building that isn't dying or removed, construction sites included; a research running at the save stays unmarked, and a building upgrading at the save marks as its current type with its upgrade's entry left open (traced, rounds 93 to 95) |
| Computer job counts: miners, lumberjacks, repairers, buildings in progress, hall cooldown (`ai/worker-ai.md`) | 0 | 0, while every loaded worker keeps its job, so each loaded job's release wraps its 16-bit count to 65535 (traced, round 95: `game/save-load.md` SL22 to SL25; the repairers and hall cooldown static study) |
| Game clock and pause counts | Start at 0 | Restored (Enhanced) |

### Where players act

- **Commands** (orders, builds, cheats typed as text, diplomacy) are carried out as their specs say; in multiplayer
  every machine carries them out in the same cycle.
- **Cheats** change their flags when the cheat command is carried out (`game/cheats.md`). The instant victory and
  defeat cheats are found by the next victory check and, in single player, shown at the one after
  (`game/victory.md`, "Outcome timing").
- **Pause** (`ui/game-menu.md`): a paused cycle changes no game state but the cycle counter still counts it, so the
  counter values of checks and refreshes hold only for a game that was never paused (`core/timing-and-rng.md`,
  "Scheduler", rule 4).
- **Saving** (`game/save-load.md`) writes the state between cycles; it doesn't change the running game, except for
  the remembered images when the map-reveal cheat is on.

### One cycle at the game level

The phases are in `core/timing-and-rng.md`. The game-level work in each:

| Phase | Game-level work | Spec |
| --- | --- | --- |
| 3 | The fog refresh countdown | `core/timing-and-rng.md`, `vision/fog.md` |
| 4 | Rescue: the joining check in each unit's update, and each circle of power's rescue check | `game/rescue.md` |
| 8 | The victory countdown and, when it reaches its check, the victory check | `game/victory.md` |
| 9 | The cycle counter | `core/timing-and-rng.md` |

The victory countdown counts every cycle that runs, the two start-up cycles included, so the first check of a new
game, and of a loaded one, falls in the second start-up cycle (`game/victory.md`, "When the check runs").

### Per-game counts

| Count | Reset when | Saved | Read by |
| --- | --- | --- | --- |
| Campaign total | New multiplayer or custom game; first mission of a campaign | Yes | Score screen (shown score, rank) |
| Total at the mission's start | Each new game | — | A defeat puts the campaign total back to it |
| Mission score | Each new game | Yes (Enhanced) | Added to the campaign total at the score screen |
| Kills, razings, units and buildings lost, resources gathered | Each new game | Yes (Enhanced) | Score screen statistics |
| Rescued counts | Each new game, for every player (static study) | Yes | Victory rules (`game/victory.md`, `game/rescue.md`) |
| Cheat-used mark | Each new game (static study); not at a load (traced) | No (static study) | Score screen rank |
| Cheat flags | Every game start, except three flags | No | `game/cheats.md` |
| Victory countdown, found outcome | Every game start, a load included (traced) | No (traced) | Victory check timing |
| Fog refresh countdown | Every game start, a load included (traced) | No (traced) | Fog refresh timing |
| 50-cycle pass counters | Every game start, a load included (traced) | No (traced) | Computer players' passes A and B, the no-workers pass |
| Computer research requests, build-list done marks | Every game start, a load included, then made again (traced) | No: a load doesn't keep them (traced) | The worker job scan, the no-workers pass, the research buildings |
| Computer job counts (miners, lumberjacks, repairers, buildings in progress, hall cooldown) | Every game start, a load included, to 0 (traced, round 95); the units' jobs are kept | No (traced) | The worker job choice (`ai/worker-ai.md`, "Counts after a load") |
| Cycle counter | Every game start, a load included (traced) | Written in Enhanced, but a load doesn't use it (traced) | Only the network sync check and saving; no rule depends on its value |
| Game clock, pause counts | New game | Yes (Enhanced) | Game menu (`ui/game-menu.md`) |

### Rules mode

Everything here is Enhanced, the default. **Classic mode** (Classic multiplayer only): a Classic save keeps no cycle
count, clock, pause counts or score counters (`game/save-load.md`); the fog refresh period is 101
(`core/timing-and-rng.md`). No Classic rule above changes an Enhanced one.

## Data

| Item | Value | Spec |
| --- | --- | --- |
| Start-up cycles per game start | 2 | `core/timing-and-rng.md` |
| Victory countdown at game start | 1 (demo playback 1,400) | `game/victory.md` |
| Victory check period | 201 cycles | `game/victory.md` |
| Single-player outcome delay | one check (201 cycles) after the check that finds it | `game/victory.md` |

## Test vectors

| Situation | Expected |
| --- | --- |
| **G01.** Human01, Enhanced, no input, no pause | Victory checks in the second start-up cycle, then at counter 201, 402, 603 (`game/victory.md`) |
| **G02.** Same, but the game is paused for 10 cycles between counter 100 and 110 (static study) | The checks fall at counter 211, 412, …: the counter counts the paused cycles, the countdown doesn't |
| **G03.** Demo playback (static study) | The countdown starts at 1,400; the first check, which ends the demo, runs in the 1,401st cycle, the two start-up cycles included |
| **G04.** Single player, no pause, "unite the clans" carried out at counter 100 (static study) | The check at counter 201 finds the victory; the victory dialog opens at the check at counter 402 |
| **G05.** Cheat typed in mission 3, mission lost, mission 3 replayed without cheats | Rank entry is not 39 in the replay: the cheat-used mark is cleared at every new game (static study) |
| **G06.** A cheat is typed in one game; then, in the same program run, a saved game in which no cheat was used is loaded and won | Rank entry 39: the load keeps the mark (traced for the mark: `human1-human01-cheat-load-game-50`; the rank follows from `game/campaign-flow.md`) |
| **G07.** Single player; victory found at a check, the game saved before the next check, then loaded. Traced: `human1-human01-unite-saveload-game-650` ("unite the clans" carried out in record n = 101, the victory found at the check at counter 201, saved in record n = 253, loaded) against its control `human1-human01-unite-control-game-600` | The found outcome is lost: no victory at the loaded game's checks in the second start-up cycle and at counter 201, 402 and 603 (the instant-victory flag isn't saved, so it isn't found again). The control shows the victory at the check at counter 402 |

## Open questions

The open questions of this area are in the part specs. The round-88 ones and their state after the round-90 and
round-91 traces:

| Q | Question | Answer or default | Where |
| --- | --- | --- | --- |
| Q1 | Does the `title` word skip the title picture? | **Answered (traced): yes** | `game/campaign-flow.md` |
| Q2 | Does a loaded Enhanced game's cycle counter start at 0 again? | **Answered (traced): yes** | `game/save-load.md`, `core/timing-and-rng.md` |
| Q3 | Does a load repeat the mission start's effects? | **Answered (traced, round 91): yes** | `game/save-load.md`, `game/victory.md` |
| Q4 | Does a load set the victory countdown to 1 and drop a found outcome? | **Answered (traced): yes** | `game/save-load.md`, `game/victory.md` |
| Q5 | Does a load clear the cheat-used mark? | **Answered (traced): no** | `game/cheats.md` |
