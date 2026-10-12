# Oracle: per-cycle trace format

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** this is our own format, written by the oracle harness (`oracle/` in this repo, research side), which
  observes the original 2.02 program running under Wine. The field meanings follow the approved specs named in each
  row; nothing here is copied from the program.
- **Changes:** 2026-10-12: first version (format version 1).
  2026-10-12: a cheat flag with no name in `game/cheats.md` is written as `"unnamed N"`, never as a raw value.
  2026-10-12: format version 2 (input scenarios): the header's `input` field, `turn` records for turns that carried
  out commands, their `commands` list, and when a handed-over command runs.
  2026-10-12: the `cheat` command kind (still version 2); input lists are in ascending order of `n`, so the Orc02
  vector with a late hand-over is replaced by a Human06 queue vector.
  2026-10-12, round-35 errata: the Human12 select that no scenario gave is the capture harness's late click, not the
  game's own command.
  2026-10-12, round-40 errata: a `build` command's builder may be a tanker (oil platforms) as well as a worker;
  `orc09-naval` vector.

## Behaviour

A trace records the simulation state of one game of the original program after every simulated cycle, so that a
reimplementation can run the same scenario and compare state cycle by cycle. A comparer reads the trace, sets up the
same scenario from the header, steps its own simulation, and reports the first record whose fields differ.

### File

- **JSON Lines**, UTF-8, one JSON object per line, optionally gzip-compressed (`.jsonl.gz`).
- **Line 1** is the header. Every later line is a record.
- Integers are plain JSON numbers. A missing optional field means "not applicable", not 0.

### Records and the cycles they follow

Records come in the order the work happened. Each has `n` and `kind`.

| `kind` | When it is written | `n` |
| --- | --- | --- |
| `setup` | Once, after the map is loaded, the players are set up and every map unit is created, before any cycle runs. | 0 |
| `startup` | After each of the two cycles that run at once during start-up (`core/timing-and-rng.md`, "Scheduler", rule 1). These cycles don't advance the cycle counter. | 1, 2 |
| `cycle` | After every normal cycle that did simulation work. | 3, 4, … |
| `turn` | After a command turn (commands from the queue were carried out between cycles), only when the turn drew random numbers or carried out at least one command. Any other turn gets no record. | the `n` of the cycle before it |

- `n` counts simulated cycles from the start of the game, start-up cycles included. A `turn` record shares its `n`
  with the `cycle` record before it, so compare records by `n` and `kind` together, in file order.
- **Paused cycles** (and, in single player, cycles while the game window is inactive) do no simulation work and get
  no record. The program's own cycle counter is given in `counter` for reference only. It can drift from `n` (it
  doesn't count the start-up cycles, and may count paused ones), so a comparer should ignore it.
- Each record is the full state at that point: it doesn't depend on earlier records.

### Header

| Field | Meaning |
| --- | --- |
| `format`, `version` | `"tidewater-oracle-trace"`, `2` (see "Format versions") |
| `program` | The program and its file hash |
| `scenario` | Free text: the map and what input was given |
| `launch` | Free text: how the game was started (command line, clicks) |
| `clock` | The fixed wall-clock value, in whole seconds since 1970-01-01 00:00:00, returned for every time-of-day request (`core/timing-and-rng.md`, "Seeding": single player seeds from it), or `null` when the real clock was used |
| `speed` | Speed level 0 to 8 (`core/timing-and-rng.md`, "Enhanced speeds"). Pacing only; it doesn't change the simulation |
| `enhanced` | `true` for Enhanced rules, `false` for Classic |
| `era` | Tileset: 0 forest, 1 winter, 2 wasteland, 3 swamp |
| `map_size` | Map width = height, in tiles |
| `local_player` | The player number the human plays |
| `controllers` | 16 entries, one per player number: `"human"`, `"computer"`, `"passive"` (passive computer) or `"nobody"`, after setup (`game/setup.md`) |
| `races` | 16 entries: `"human"`, `"orc"` or `"neutral"` |
| `cheats` | The cheat flags on at setup, by their phrase or word as in `game/cheats.md` (for example `"tigerlily"`, mission select, which the launch uses; `"title"`, `"noglues"`); the two notices without a phrase are `"notice A"` and `"notice B"`; any other flag (none is known to be set by anything) is `"unnamed N"`, with N a number from 0 to 31 that only tells such flags apart; empty when none |
| `unit_slots`, `effect_slots` | Table sizes (`core/unit-slots.md`, "Table sizes") |
| `input` | The scenario's commands as given to the harness, in file order; an empty list when there is no input. Each entry has `n` (see "When a command runs"), `player` (always the local player) and the fields of its kind as in "Commands", except that a `command` entry may leave out `tile` (the target unit's tile is then used), and a `cheat` entry has `add` instead of `cheats` ("Cheat commands") |

### Record fields

| Field | Meaning |
| --- | --- |
| `n`, `kind` | As above |
| `counter` | `cycle` records only: the program's cycle counter after this cycle (reference only, see above) |
| `rng` | The simulation generator's state `s` after this record's work (`core/timing-and-rng.md`, "Random number generator"). The number of draws made since the previous record is the number of steps from the previous `rng` to this one |
| `gold`, `lumber`, `oil` | 16 entries each, the stock of each player number |
| `units` | Every unit-table slot not on the free list, in ascending slot order (below) |
| `effects` | Every effect-table slot in use, in ascending slot order (below) |
| `commands` | `turn` records only, present when the turn carried out commands: each command, in the order it ran ("Commands") |

### Unit fields

| Field | Meaning |
| --- | --- |
| `slot` | Unit-table slot (`core/unit-slots.md`) |
| `serial` | Creation number (`core/unit-slots.md`, "Creating a unit", step 3) |
| `type` | Unit type number, as in the map's unit section and the unit data |
| `owner` | Owner value 0 to 15 (`core/unit-slots.md`, "Owner lists") |
| `x`, `y` | Pixel position (`core/animation-scripts.md`): 32 × the tile position while the unit stands on a tile, moving pixel by pixel while it walks |
| `tx`, `ty` | Tile position |
| `hp` | Hit points |
| `order` | Current order number (`orders/orders.md`, "The order list") |
| `facing` | 0 to 7, 0 = north, clockwise |
| `anim` | Current animation number (`core/animation-scripts.md`) |
| `frame` | Sprite frame (`core/animation-scripts.md`) |
| `wait` | Animation wait counter (`core/animation-scripts.md`) |
| `mana` | The unit's 8-bit mana value for spellcasters. For other units the same value may hold another counter; compare it only where a spec defines it |
| `target` | Slot of the unit's target unit, or `null` |
| `trips` | Only for types 0x56 and 0x57 (oil platforms), 0x5C (gold mine) and 0x5D (oil patch): trips left (`economy/harvest.md`) |
| `timers` | Only when one is running: any of `invisibility`, `unholy_armour`, `bloodlust` (cycles left) and `haste` (cycles; positive = haste, negative = slow) |

### Effect fields

| Field | Meaning |
| --- | --- |
| `slot` | Effect-table slot (`core/unit-slots.md`) |
| `kind` | Missile or effect kind number (`combat/damage.md`, `spells/spells.md`) |
| `x`, `y` | Position in pixels |
| `dest` | Destination `[x, y]` in pixels |
| `target`, `shooter` | Unit slots, or `null` |
| `damage` | The damage value stored on the missile (`combat/damage.md`) |

### Commands

Each entry of a `turn` record's `commands` list is one command the game carried out in that turn, and has `player`
and `kind`.

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `select` | `units` | Replaces the player's **synced selection** (`ui/selecting-and-commanding.md`, "Two selections") with these unit slots, in this order (at most 9) |
| `command` | `command`, `tile`, `target` | Gives command number `command` (`orders/orders.md`, "Commands"; a spell by its spell order number, 38 to 56) to the units of the player's synced selection, as in `ui/selecting-and-commanding.md`, "Which units take part in a command": the list of handled commands, the selection filter and the formation offset. `tile` is `[x, y]`; `target` is a unit slot or `null` |
| `build` | `type`, `tile` | The first unit of the player's synced selection, if it can build (a worker, or a tanker, which builds oil platforms; `economy/build-train.md`), is told to build the building of type `type` with its top-left tile at `tile` (`economy/build-train.md`, "Placing a building"). The site is checked when the command runs; on a failure a message is shown and nothing else happens |
| `job` | `job`, `number` | The first unit of the player's synced selection starts a job (`economy/tech-tree.md`, "Starting a job", whose checks include that it is a completed building). `job` is `"train"` (`number` is a unit type), `"research"` (research a spell; an upgrade number), `"upgrade"` (an upgrade number) or `"upgrade building"` (a building type). A job kind outside these four is written as its number |
| `cheat` | `cheats` | A cheat command, carried out as in `game/cheats.md`, "Carrying out a cheat command". `cheats` is the **whole new set** of cheat flags it carries, not only the ones it turns on ("Cheat commands") |
| `other` | — | A command of a kind this format doesn't describe. Input scenarios don't give these |

A command whose conditions fail (a unit not owned by the player, a builder that is neither a worker nor a tanker, too little gold) is
still listed: it was carried out and did nothing.

`commands` lists every command carried out in the turn, not only the scenario's. In the Human12 captures the
harness's last click on the briefing's Continue button can land on the map after the game has started; it selects the
building under the pointer and is carried out as an ordinary `select` (turn 70 in `human12-demolition-450`, 74 in
`human12-transport-650`; the turn depends on wall-clock time, so it moves from run to run). It takes its turn's
place in the queue like any command.
A comparer applies these like any other command.

### Cheat commands

**Names.** Each flag in a `cheat` entry's `cheats` is named as in the header's `cheats` field: by its phrase or word
in `game/cheats.md` (for example `"every little thing she does"`, all spells; `"glittering prizes"`, resources;
`"make it so"`, build speed; `"tigerlily"`, mission select), `"notice A"` or `"notice B"`, or `"unnamed N"`. The list
is a set; its order means nothing. An empty list means every flag is off.

**Carrying it out.** The cheat flags are one set for the whole game, not one per player. The flags before the first
cheat command are the header's `cheats`; no record repeats them, so a comparer follows them from the header through
each `cheat` entry. When the entry runs, the comparer carries out `game/cheats.md`, "Carrying out a cheat command",
with `cheats` as the new flags:

1. If `player` is the local player and `cheats` isn't empty, the mission counts as cheated.
2. The flags become `cheats`.
3. The steps of "Effects when the flags change" run in that spec's order, 1 to 8. The one-shot flags in the list
   (`"glittering prizes"`, `"valdez"`, `"notice A"`, `"notice B"`, `"ucla"`) do their work there and don't stay on:
   after the turn the flags are `cheats` without them.

**When it shows.** The `cycle` record with the turn's `n` holds the state before the cheat. The `turn` record that
lists it holds the state after it: for example the resources cheat's gold, lumber and oil are already added, for
each of the 16 players. Lasting effects (`game/cheats.md`, "Lasting effects") act from cycle n + 1 on. In single
player a cheat command draws no random numbers.

All spells also acts at a later command: when a spell command runs while it is on, the caster's mana is 255 in that
command's own `turn` record, before the caster moves (`spells/spells.md`, "Giving the order").

**In the header's `input`.** A `cheat` entry has `add`, the flags to turn on. The harness reads the game's flags when
it hands the entry over and gives the game a cheat command carrying those flags plus `add`. The `turn` record's
`cheats` is what actually ran.

### When a command runs

Handing a command to the game is not running it. The header's `input` entries are in ascending order of `n` (the
harness refuses any other order), and each is handed to the game at the stop after `cycle` record H(i) = `n`.

The game keeps handed-over commands in a queue and carries out at most one per human-controlled player in each
command turn, first in, first out. At speed 8 a command turn follows every cycle. For one player, entry i (counting
from 1, in list order) then runs in the turn with n = T(i), where T(i) is the larger of H(i) + 1 and T(i − 1) + 1,
with T(0) = 0. So a select and a command handed over together run in two consecutive turns.

The `turn` records' `commands` are the authoritative timing: a comparer applies each command in the turn where the
trace lists it, and uses the rule above only to predict that turn or to drive its own simulation without a trace.

### Format versions

| # | Where | Version 1 | Version 2 |
| --- | --- | --- | --- |
| 1 | Header `version` | `1` | `2` |
| 2 | Header | — | Field `input` |
| 2 | `commands` kinds | — | `select`, `command`, `build`, `job`, `other`; `cheat` was added later without a version change, since no trace had written a cheat command as `other` |
| 3 | `turn` record, when written | Only when the turn drew random numbers | When the turn drew random numbers or carried out at least one command |
| 4 | `turn` record fields | The record fields | The record fields and, when commands ran, `commands` |

Every version 1 field keeps its meaning. A version 2 trace without input is identical to a version 1 trace apart
from `version` and an empty `input`.

**Scenario files.** The harness's input lists in `oracle/` ("Input scenarios") are our own command lists and contain
nothing from the program, so they may be used as test input like the traces. Their line format isn't part of this
spec; use the header's `input`, which is.

## Data

### Example (one unit shown, numbers abridged)

```json
{"format":"tidewater-oracle-trace","version":2,"scenario":"campaign mission 'human1', no input","clock":946684800,"speed":8,"enhanced":true,"era":1,"map_size":32,"local_player":1,"controllers":["computer","human","nobody","…"],"races":["orc","human","human","…"],"cheats":["tigerlily"],"unit_slots":1600,"effect_slots":400,"input":[]}
{"n":0,"kind":"setup","rng":4116580520,"gold":[1000,1000,…],"lumber":[…],"oil":[…],"units":[{"slot":1588,"serial":12,"type":1,"owner":0,"x":640,"y":992,"tx":20,"ty":31,"hp":60,"order":2,"facing":7,"anim":2,"frame":0,"wait":7,"mana":0,"target":null}],"effects":[{"slot":0,"kind":19,"x":0,"y":0,"dest":[0,0],"target":null,"shooter":null,"damage":0},…]}
{"n":3,"kind":"cycle","counter":1,"rng":…,"units":[…],"effects":[…]}
```

The header of a run with input, and the first two `turn` records (fields abridged):

```json
{"format":"tidewater-oracle-trace","version":2,"scenario":"campaign mission 'human1', input from human01-basics.txt",…,"input":[{"n":10,"player":1,"kind":"select","units":[1593,1597,1598]},{"n":10,"player":1,"kind":"command","command":8,"target":1592},…]}
{"n":11,"kind":"turn","commands":[{"player":1,"kind":"select","units":[1593,1597,1598]}],"rng":…,"units":[…],"effects":[…]}
{"n":12,"kind":"turn","commands":[{"player":1,"kind":"command","command":8,"tile":[29,14],"target":1592}],"rng":…,"units":[…],"effects":[…]}
```

## Test vectors

These come from the first oracle trace (Human01, no input; the header's `launch` field gives the exact launch).

| Input | Expected |
| --- | --- |
| Clock 946684800, campaign mission Human01, 12 map units | Setup record `rng` = 0xF55E08A8, which is exactly 24 steps from the seed 946684800: 2 creation draws per unit |
| Same, the first two records after setup | `startup` n = 1 (0 draws), `startup` n = 2 (1 draw); then `cycle` n = 3 with `counter` 1 |
| Same, cycles n = 3 to 2000 | Draws per cycle repeat 1, 2, 1, 1, 3, 3196 in all (8 per 5 cycles: each of the 8 idle mobile units draws once per 5 cycles; this idle draw isn't yet in `core/timing-and-rng.md`'s draw table) |

These come from `oracle/traces/human01-basics-9500.jsonl.gz` (Human01, Enhanced, speed 8, clock 946684800, local
player 1) and the other input traces in `oracle/traces/`, named by mission.

| Input | Expected |
| --- | --- |
| `orc09-naval-1500`: a select of tanker 1473 (type 0x1B, player 5) and a build of type 0x57 (oil platform) at (57, 75), both with `n` = 100 | `turn` n = 101 lists the select and 102 the build; 1473 is in order 28, wait 3 at `cycle` n = 105 |
| Human01: a select of 1593, 1597, 1598 and command 8 on unit 1592, then a select of 1596 and a build of type 60 at (5, 8), all with `n` = 10 | `turn` n = 11 lists the select, 12 the command, 13 the second select, 14 the build: one per turn, in file order |
| Human01: the command 8 entry gives only the target unit | `turn` n = 12 lists `tile` [29, 14], unit 1592's tile |
| Human01: the build in turn 14 | Worker 1596 is in order 28 from `cycle` n = 19, and in order 22 with target 1587 (the new barracks) from n = 134 |
| Human01: a select of 1587 and a job `"train"` 0 with `n` = 5300 | `turn` n = 5302 lists the job; barracks 1587's `mana` is 1 in that record (`economy/build-train.md`) |
| Human06 (`human06-tech-650`): a cheat entry with `n` = 10, then six entries (select, build, select, build, select, job) with `n` = 20 | The cheat runs in `turn` n = 11; the six run in `turn` n = 21 to 26, one per turn |
| Every input trace in `oracle/traces/` (15 as of 2026-10-12) | Each `input` entry runs in turn T(i) of "When a command runs"; no `turn` record lists more than one command. The two Human12 traces also list the harness's stray `select` (`human12-demolition-450`: turn 70; `human12-transport-650`: turn 74) |
| Human06: `input` entry `n` 10, `player` 1, `kind` `"cheat"`, `add` `["make it so", "glittering prizes"]`; header `cheats` `["tigerlily"]` | `turn` n = 11 lists `{"player":1,"kind":"cheat","cheats":["make it so","glittering prizes","tigerlily"]}` and nothing else. `rng` is the same as in `cycle` n = 11 |
| Human06, the same cheat | Gold in `cycle` n = 11: 1000 for every player but player 5 (500). In `turn` n = 11: +10000 gold, +5000 lumber and +5000 oil for each of the 16 players (player 5: 10500 gold) |
| Human06, the farm (type 58) whose build ran in turn 22, under build speed | Created at n = 78 with 43 hit points; 400 hit points and shown progress 255 at n = 177; frame 0 and `mana` 0 (complete) at n = 178 |
| Orc09 (`orc09-naval-1500`): the same cheat entry for player 5 | Player 5's gold is 1000 in `cycle` n = 11 and 11000 in `turn` n = 11 |
| XHuman10 (`xhuman10-spells-700`): cheat `add` `["every little thing she does"]` in turn 11; mage 1481 selected in turn 21; command 46 (polymorph) in turn 22 | Gold unchanged by the cheat. Mage `mana` 85 in `turn` n = 21, 255 in `turn` n = 22 while still in order 2; order 46 from `cycle` n = 23; the cast at n = 145 leaves 55 |
| XHuman10 blizzard, XOrc01 death knight, XOrc12 ogre-mage: the same cheat, then command 47, 53 or 49 in turn 22 (mage 1481, death knight 1491, Dentarg 1548) | `mana` 85 before, 255 in `turn` n = 22. XOrc12: back to 255 at each later spell command (turns 62 and 102) |

## Open questions

- **Other speeds.** Input traces so far all run at speed 8, where a command turn follows every cycle. At slower
  speeds the turns may be further apart; the trace's `turn` records stay authoritative.
- **Commands the game queues by itself:** none has been seen; the Human12 select was the harness's own click
  (found in round 35).
- **Commands from other human players** (multiplayer) would appear under their own player number; input scenarios
  are single player for now.
- **Other kinds** (cancel, alliances, speed, transports) aren't described yet; add them when a scenario needs them.
- **Two cheat entries handed over before the first runs:** the harness builds the second from the flags at hand-over,
  so it would drop what the first turned on. No scenario does this; the `turn` record shows what ran either way.
- **Cheat commands the game makes itself** (mission select sends one with the flags unchanged) would appear as a
  `cheat` entry with the same flags. None has been seen in a trace.
- **Multiplayer cheats:** the throw-away draw when a game disables cheats hasn't been traced.
- Fog of war, the map's tiles (trees cut, walls) and the AI state aren't recorded yet.
- **Scenario identity.** `scenario` and `launch` are free text, so a comparer can't yet set up the scenario from the
  header alone. Default: each comparer test names its scenario (campaign entry or map file) itself; a structured
  field can come in a later format version.
