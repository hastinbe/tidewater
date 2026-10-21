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
   "Scheduler"). Both kinds of game start run them (static study).
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
| Cheat-used mark (rank 39) | Cleared (static study) | Not saved (static study). Cleared, so only cheats used after the load count (default; the static study finds a load doesn't clear it: `game/cheats.md`, "Open questions", Q5) |
| Campaign total | Reset to 0 for multiplayer, custom games and the first mission of a campaign; otherwise carried over (`game/campaign-flow.md`, "Campaign total") | Restored from the save |
| Mission scores and the score screen's statistics | Start at 0 (`game/campaign-flow.md`) | Restored from the save (Enhanced) |
| Rescued counts | Set to 0 for every player (static study) | Restored from the save |
| Mission rule (mode) | Taken from the mission table (campaign) or the standard rule (`game/victory.md`) | Restored from the save, already switched if it was switched |
| Map, players, units, resources, alliances | Read from the map (`game/setup.md`) | Restored from the save |
| Music track | Chosen (`ui/music.md`) | Restored from the save |
| Random numbers | Seeded (`core/timing-and-rng.md`) | Seeded the same way (`game/save-load.md`) |
| Mission start (`game/victory.md`, "Mission start") | Runs once, before the first check | Not repeated (default; the static study finds it runs again: `game/save-load.md`, "Open questions", Q3) |
| Victory check's rule | Chosen from the mode | The saved rule, with any switch made before the save |
| Victory countdown and a found outcome | Countdown set to 1 (1,400 for demo playback); no outcome | Restored: the countdown goes on from its saved value and an outcome found but not yet shown is kept (default; the static study finds the countdown set to 1 and the outcome lost: `game/save-load.md`, "Open questions", Q4) |
| Start-up cycles | Two | Two (static study) |
| Cycle counter | 0 when the first normal cycle starts | Kept in Enhanced (default; the static study finds it starts at 0 again: `game/save-load.md`, "Open questions", Q2) |
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
game falls in the second start-up cycle (`game/victory.md`, "When the check runs"; for a loaded game see Q4).

### Per-game counts

| Count | Reset when | Saved | Read by |
| --- | --- | --- | --- |
| Campaign total | New multiplayer or custom game; first mission of a campaign | Yes | Score screen (shown score, rank) |
| Total at the mission's start | Each new game | — | A defeat puts the campaign total back to it |
| Mission score | Each new game | Yes (Enhanced) | Added to the campaign total at the score screen |
| Kills, razings, units and buildings lost, resources gathered | Each new game | Yes (Enhanced) | Score screen statistics |
| Rescued counts | Each new game, for every player (static study) | Yes | Victory rules (`game/victory.md`, `game/rescue.md`) |
| Cheat-used mark | Each new game (static study); a load too (default, Q5) | No (static study) | Score screen rank |
| Cheat flags | Every game start, except three flags | No | `game/cheats.md` |
| Victory countdown, found outcome | Each new game (a load restores them: default, Q4) | Yes (default, Q4) | Victory check timing |
| Cycle counter | Each new game (a load keeps it in Enhanced: default, Q2) | Yes (Enhanced) | Only the network sync check and saving; no rule depends on its value |
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
| **G06.** A cheat is typed in one game; then, in the same program run, a saved game in which no cheat was used is loaded and won (default; held, Q5) | Rank isn't entry 39: the load clears the mark (the static study predicts entry 39) |
| **G07.** Single player; victory found at a check, the game saved before the next check, then loaded (default; held, Q4) | The found outcome is kept: the victory dialog opens at the first check after the load, when the saved countdown runs out (the static study predicts the outcome is lost and found again only if the game state still gives it) |

## Open questions

The open questions of this area are in the part specs. The round-88 ones, each with the stated rule as the default
and a trace request:

| Q | Question | Default | Where |
| --- | --- | --- | --- |
| Q1 | Does the `title` word skip the title picture? | No: only Ctrl skips it | `game/campaign-flow.md` |
| Q2 | Does a loaded Enhanced game's cycle counter start at 0 again? | No: kept | `game/save-load.md`, `core/timing-and-rng.md` |
| Q3 | Does a load run the mission start again? | No | `game/save-load.md`, `game/victory.md` |
| Q4 | Does a load set the victory countdown to 1 and drop a found outcome? | No: both restored | `game/save-load.md`, `game/victory.md` |
| Q5 | Does a load clear the cheat-used mark? | Yes | `game/cheats.md` |
