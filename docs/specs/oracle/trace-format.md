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
  2026-10-12, round-44 errata: a dead unit keeps reporting a released target's slot in `target`; a blizzard shard's
  `dest` is its landing point from creation; `human06-31357` and `xhuman10-blizzard-600` vectors.
  2026-10-13, round-45 errata: the `dest` of death coil missiles, death-and-decay clouds and whirlwinds, and a death
  coil caster's `target`, as in `spells/spells.md`.
  2026-10-13, round-47 errata: the `dest` of a fireball and of a flame shield bolt, as in `spells/spells.md`.
  2026-10-13, round-48 errata: the `dest`, `target` and `shooter` rows give the fields of flames and of healing and
  exorcism effects.
  2026-10-13, round-56 errata: an effect's `shooter` keeps its slot number after that slot is released;
  `xorc12-spells-1500` vector.
  2026-10-13: multiplayer-launch traces (still version 2): the header's `rules` row, the multiplayer fields
  `launch_path`, `connection`, `map`, `start_seed` and `humans`, and "Multiplayer launch";
  `mp-dark-paths-bne-1000` and `mp-dark-paths-bne-classic-1000` (Classic mode) vectors.
  2026-10-13, round-57 answers: the start seed's + 4 is exact, not only observed.
  2026-10-13, round-58 errata: a comparer can derive a multiplayer setup's generator state (`game/setup.md`).
  2026-10-13, round-59 corrections: the placement draw count needs at least one opponent (none: no draw); Fixed
  Order starts make no placement draws.
  2026-10-13, round-60 harness: the multiplayer header fields `opponents` and `starts`.
  2026-10-14, oracle-extras errata (still version 2): opt-in windowed traces (header `from`), setup-only traces
  (`setup_only`), and the optional `extras`: the record field `hits` and the unit fields `goal`, `pending` and
  `transport_dest`. Default traces don't change.
- **Changes:** 2026-10-16 (round 76, B-038 build/train consolidation, `ERRATA-buildtrain-consolidation.md`): the `job`
  row points at "Starting a job" in `economy/build-train.md`, where the job start procedure now lives (no rule change).
- **Changes:** 2026-10-16 (round 77, `ERRATA-round77.md`, reviewed by a spec-review session, 2026-10-16): the
  `cancel` and `cancel-job` command kinds (still version 2), which the input scenarios can now give; traces
  `human06-cancel-site-600` and `human06-cancel-job-500`.

- **Changes:** 2026-10-21, round 89 (B-057 multiplayer, `ERRATA-round89-multiplayer.md`; reviewed by a spec-review
  session, 2026-10-21): the "Other speeds" open question is answered from `net/lockstep.md` (single player: a turn
  before every cycle, static study; Direct Connect launch: K = 12 at speed 8, traced).
- **Changes:** 2026-10-22, round 96 (oracle traces, `ERRATA-round96-traces.md`; reviewed by a spec-review session,
  2026-10-22), still version 2: the second human's commands and the harness's `peer` actions replace "Idle human",
  with a check a comparer makes on them; the scenario verbs `save`, `load`, `say` and `fog` and their records; a
  loaded game's `setup` record; the `aiworkers` extra; after a load a dead body's `target` is `null`
  (`game/save-load.md`, SL28). The other research extras are listed and held.
- **Changes:** 2026-10-22, round 97 (`ERRATA-audit2-economy.md`; reviewed by a spec-review session, 2026-10-22),
  still version 2: the `goal` row defines a building's x (its construction pass and repair counter), so a comparer may
  check it.
- **Changes:** 2026-10-22, round 98 (`ERRATA-round98-traces.md`, `ERRATA-audit2-ai.md`; reviewed by a spec-review
  session, 2026-10-22), still version 2: the research extras' fields are described ("Research extras"), the `game`
  extra's result codes included; a comparer still ignores them. The `goal` row adds a repaired transport's x (traced,
  `economy/repair.md` RP8). A town hall started by a computer worker's no-depot step carries job bit 16, not 8.
- **Changes:** 2026-10-22, round 99 (`ERRATA-round99-traces.md`; reviewed by a spec-review session, 2026-10-22), still
  version 2: the no-depot town hall's job bit 16 is traced; the header's `map` may name a generated test map
  (`oracle/test-maps.md`).
- **Changes:** 2026-10-22, round 101 (`ERRATA-round101-traces.md` section 3; reviewed by a spec-review session,
  2026-10-22), still version 2: the `goal` row defines a transport's goal as its target tile in every record (traced),
  with one turn-record case held (TG4). Two input kinds, `hold-workers` and `release-workers` (no data), may appear in
  the header's `input` and in turn records' `commands` (`orders/orders.md`, "Holding and releasing workers").
- **Changes:** 2026-10-22, round 102 (`ERRATA-round102-traces.md`; reviewed by a spec-review session, 2026-10-22),
  still version 2: the held TG4 case is explained (the command's pull-back, `orders/transports.md` V46 to V48), so a
  transport's `goal` is compared in every record with no exception. One more input kind, `right-click` (`tile`,
  `target`), may appear in the header's `input` and in turn records' `commands`.
- **Changes:** 2026-10-22, round 103 (`ERRATA-round103-traces.md` section 2; reviewed by a spec-review session,
  2026-10-22), still version 2: the opt-in extra `groups` (every player's control groups and synced selection,
  compared) and two input kinds, `set-group` and `recall-group` (`group`), which a turn record names only when the
  trace has the `groups` extra (otherwise `other`).
- **Changes:** 2026-10-22, round 104 (`ERRATA-round104-sweep.md` H1; reviewed by a spec-review session, 2026-10-22),
  still version 2: `fog_countdown` says what the records show around a refresh and after a holy vision, and that a
  comparer shows 0 only in a cycle where the refresh ran (a comparer fix; no game rule and no format change).
- **Changes:** 2026-10-22, round 105 (`ERRATA-round105-sweep.md` H2; reviewed by a spec-review session, 2026-10-22),
  still version 2: `fog_countdown` covers a holy vision cast in a refresh cycle (the record shows 150; the comparer
  shows 0 only when nothing set the counter after the refresh in that cycle); one more vector (a comparer fix; no game
  rule and no format change).
- **Changes:** 2026-10-23, round 107 (`ERRATA-round107-sweep.md` PI1; reviewed by a spec-review session, 2026-10-23),
  still version 2: a `select`'s `units`, and the `groups` extra's `selection`, may name slots that hold no unit (the
  synced selection keeps them, `ui/selecting-and-commanding.md`); a reader takes a `command` with no `tile` as tile
  `[0, 0]`. No format change.

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
| `setup` with `load` | After a `load` input entry: the loaded game's state, before any of its cycles run ("Saves, loads, cheat words and fog"). The loaded game's `startup` and `cycle` records follow as in a new game | 0: `n` starts again |
| `save` | After a `save` input entry, once the game has written the file. It holds no unit state ("Saves, loads, cheat words and fog") | the `n` of the cycle whose state was saved |
| `say` | After a `say` input entry, once the typed word has been carried out. No unit state | the `n` of the turn that carried out the word's cheat command |
| `fog` | After a `fog` input entry, right after the `cycle` record with the same `n`. No unit state | the `n` of that cycle; the change applies from the next cycle |
| `peer` | Multiplayer launch only: after a `peer` input entry, right after the `cycle` record with the same `n` and before any `turn` record with that `n`. No game state ("The second human") | the `n` of that cycle |

- `n` counts simulated cycles from the start of the game, start-up cycles included. A `turn` record shares its `n`
  with the `cycle` record before it, so compare records by `n` and `kind` together, in file order.
- **Paused cycles** (and, in single player, cycles while the game window is inactive) do no simulation work and get
  no record. The program's own cycle counter is given in `counter` for reference only. It can drift from `n` (it
  doesn't count the start-up cycles, and may count paused ones), so a comparer should ignore it.
- Each record is the full state at that point: it doesn't depend on earlier records. The `save`, `say`, `fog` and
  `peer` records are markers: they hold no `rng`, resources, units or effects, and a comparer doesn't compare them
  as state (it uses them as described in their sections).

**Windowed traces.** When the header has `from` = F, the game still ran from the start, but `cycle` and `turn`
records with n < F are left out, except that the `setup` record, the two `startup` records and every `turn` record
that carried out commands are always written. So the file holds, in order: the header, `setup` (n 0), `startup`
(n 1, 2), the `turn` records with commands whose n is below F, then every record with n ≥ F as in a complete trace.
A comparer:

1. Sets up from the header and compares the `setup` and `startup` records as usual.
2. Runs the cycles up to n = F − 1 without comparing them, as normal cycles after the start-up ones
   (`core/timing-and-rng.md`, "Scheduler", rule 1). At each `turn` record in the gap it first runs the cycles up to
   that record's `n`, then carries out its `commands` exactly as in a complete trace. It doesn't compare these gap records (each is still full state, so a comparer may report it for information).
3. From the record with n = F on, compares every record as usual.

A gap is expected and isn't a missing record. Across the gap, the `rng` of the record before it doesn't give the
number of draws made before the first record at F (the gap's draws aren't counted anywhere). F is at least 1; the
harness refuses an F past the requested cycle count, but a trace that stopped early (victory, defeat, a dialog) can
end before F, and then has no record from F on.

**Setup-only traces.** When the header has `"setup_only": true`, the file is the header and the `setup` record,
nothing else: no `startup`, `cycle` or `turn` record, and never `from`. A comparer compares the `setup` record only.
The setup record is the same as a complete capture's of the same scenario.

### Header

| Field | Meaning |
| --- | --- |
| `format`, `version` | `"tidewater-oracle-trace"`, `2` (see "Format versions") |
| `program` | The program and its file hash |
| `scenario` | Free text: the map and what input was given |
| `launch` | Free text: how the game was started (command line, clicks) |
| `clock` | The fixed wall-clock value, in whole seconds since 1970-01-01 00:00:00, returned for every time-of-day request (`core/timing-and-rng.md`, "Seeding": single player seeds from it), or `null` when the real clock was used. A multiplayer start isn't seeded from it ("Multiplayer launch") |
| `speed` | Speed level 0 to 8 (`core/timing-and-rng.md`, "Enhanced speeds"). Pacing only; it doesn't change the simulation |
| `enhanced` | `true` for Enhanced rules, `false` for Classic |
| `rules` | `"enhanced"` or `"classic"`, matching `enhanced`; every version 2 trace has it. Enhanced is the default and the target. A `"classic"` trace (also `-classic` in its file name) is evidence for Classic-mode rules only and never changes an Enhanced rule |
| `era` | Tileset: 0 forest, 1 winter, 2 wasteland, 3 swamp |
| `map_size` | Map width = height, in tiles |
| `local_player` | The player number the human plays |
| `controllers` | 16 entries, one per player number: `"human"`, `"computer"`, `"passive"` (passive computer) or `"nobody"`, after setup (`game/setup.md`) |
| `races` | 16 entries: `"human"`, `"orc"` or `"neutral"` |
| `cheats` | The cheat flags on at setup, by their phrase or word as in `game/cheats.md` (for example `"tigerlily"`, mission select, which the launch uses; `"title"`, `"noglues"`); the two notices without a phrase are `"notice A"` and `"notice B"`; any other flag (none is known to be set by anything) is `"unnamed N"`, with N a number from 0 to 31 that only tells such flags apart; empty when none |
| `unit_slots`, `effect_slots` | Table sizes (`core/unit-slots.md`, "Table sizes") |
| `input` | The scenario's entries as given to the harness, in file order; an empty list when there is no input (a few early traces have no `input` field: read it as empty). Each entry has `n` (see "When a command runs"), `player` (always the local player) and `kind`. A command entry has the fields of its kind as in "Commands", except that a `command` entry may leave out `tile` (the target unit's tile is then used), and a `cheat` entry has `add` instead of `cheats` ("Cheat commands"). The other kinds, `save`, `load`, `say`, `fog` ("Saves, loads, cheat words and fog") and `peer` ("The second human"), aren't commands: they are things the harness does. Entries after a `load` count `n` from the loaded game, so the list ascends up to each `load` and starts again after it |
| `launch_path` | Multiplayer launch only: `"multiplayer"`. Absent from campaign traces: a missing field means a campaign start ("Multiplayer launch") |
| `connection` | Multiplayer launch only: `"direct-connect"`, the game's two-player serial connection |
| `map` | Multiplayer launch only: the scenario file name from the game's `Maps/` folder, for example `"Dark Paths BNE.pud"`. A name of the form `R<round>-<name>.pud` (for example `"R99-tests.pud"`) is a generated test map, not one of the game's maps: its whole content is in `oracle/test-maps.md`, and a comparer builds it from there |
| `start_seed` | Multiplayer launch only: the lobby's start seed, 0 to 255, fixed by the harness ("Start seed") |
| `humans` | Multiplayer launch only: human players in the lobby, 2: the traced player (`local_player`) and the second human, which acts only when the harness acts on its display ("The second human") |
| `opponents` | Multiplayer launch only: the number of lobby rows set to Computer, 0 to 6. It is the count the opponent placement works from (`game/setup.md`, "Multiplayer game"), so with random starts the setup makes 1 + `opponents` placement draws when it is at least 1 (none when 0), even if a placement search fails and fewer computers are placed. Traces made before round 60 (2026-10-13) don't have it; for those, count the computer controllers after setup |
| `starts` | Multiplayer launch only: the lobby's "Location" option, `"random"` (the default) or `"fixed"` (Fixed Order: each Computer row takes its same-numbered slot, with **no** placement draws; `game/setup.md`, "Fixed Order"). A `"fixed"` trace also has `-fixed` in its file name. Traces made before round 60 don't have it; all of them are `"random"` |
| `from` | Windowed trace only: F, a whole number of at least 1, the first `n` from which every `cycle` and `turn` record is written ("Windowed traces"). Absent means the trace is complete from n 0 |
| `extras` | Only when extras are on: the optional fields this trace adds, as a sorted list of extra names. `"aiworkers"`, `"goals"`, `"groups"` and `"hits"` are described in "Opt-in extras"; any other name (`"aigoals"`, `"aigroups"`, `"aijobs"`, `"airesearch"`, `"fogmaps"`, `"game"`, `"vision"`) adds a research field of the same name that a comparer ignores. Absent means none: no record has `aiworkers`, `groups` or `hits` and no unit has `goal`, `pending` or `transport_dest` |
| `setup_only` | Only when `true`: the trace is the header and the `setup` record and nothing else ("Setup-only traces") |

None of these three changes `rules`, `enhanced` or any other field: a windowed, setup-only or extras trace is Enhanced
unless Classic was explicitly requested on the multiplayer path, exactly as a default trace.

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
| `hits` | Only when `extras` has `"hits"`: the calls of the damage step made by this record's work, one entry per call in the order they happened ("Hit entries"). Absent when there was no call. The `setup` record never has it |
| `load` | A loaded game's `setup` record only: the save's name, as in the `load` input entry ("Saves, loads, cheat words and fog") |
| `aiworkers` | Only when `extras` has `"aiworkers"`, on every `setup`, `startup`, `cycle`, `turn` and `save` record: the computer workers' job counts ("Computer worker jobs") |
| `groups` | Only when `extras` has `"groups"`, on every record with game state: each player's control groups and synced selection ("Control groups") |
| Research fields | Only when `extras` names them: `aigoals`, `aigroups`, `aijobs`, `airesearch`, `fogmaps`, `game`, `vision`, on any record. Described in "Research extras"; a comparer ignores them |

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
| `target` | Slot of the unit's target unit, or `null`. When the target's slot is released, a live unit's target is cleared; a dead unit (dying, remains or in its remove order, `core/corpses.md`) keeps reporting the released slot number. This is reporting only: the slot means nothing for that unit. After a load every dead body (type 0x69) reports `null`, whatever it had at the save; every other unit, dying ones included, keeps its target (`game/save-load.md`, SL28 to SL30). A death knight casting death coil has each victim in turn as its target, as each missile is launched, and keeps the last until the idle takes over (`spells/spells.md`, "Death coil victims") |
| `trips` | Only for types 0x56 and 0x57 (oil platforms), 0x5C (gold mine) and 0x5D (oil patch): trips left (`economy/harvest.md`) |
| `timers` | Only when one is running: any of `invisibility`, `unholy_armour`, `bloodlust` (cycles left) and `haste` (cycles; positive = haste, negative = slow) |
| `goal` | Only when `extras` has `"goals"`, then on every unit: `[x, y]`, the unit's **stored goal**, in tiles like `tx`, `ty`: the tile its movement aims for (`movement/movement.md`, the stored goal of the blocked rules), set when it is sent somewhere and kept afterwards. The values are as stored, each 0 to 65535: a unit that isn't moving, and every building, holds whatever was left there (for example `[0, 65535]`, off the map). Compare it only where a spec defines it. One spec does: a **building's x** is its construction pass counter and repair counter (`economy/build-train.md`, "The construction pass"; `economy/repair.md`, "Cost and rate"): counting up from 1 while it is built, 12 × (T + 2) after completion, 0 for one placed by the map; its y has no defined value (round 97). A **transport's** goal is changed by repair: each run adds 1 to its x and each paid run takes 2 off, the y unchanged (`economy/repair.md`, "Cost and rate"; traced, RP8), so for a transport sent somewhere and then repaired a comparer may check it (round 98). More generally a **transport's** goal (x and y) is its **target tile** (`orders/transports.md`, "When the target tile and the destination change"; traced, round 101), so a comparer may check it in every record, with no exception: the target tile a command stores is already pulled back (`orders/transports.md`, "Pulled back from the transport's own area"; traced, round 102, V46 to V48) |
| `pending` | Only when `extras` has `"goals"`, then on every unit: the unit's pending order number (`orders/orders.md`, "Pending orders and immediate orders"; numbers as in `order`), or `null` when it has none |
| `transport_dest` | Only when `extras` has `"goals"`, and only on transports (types 0x1C and 0x1D), always on those: `[x, y]`, the transport's **destination** tile (`orders/transports.md`, "What the transport stores"). As stored, like `goal`: a transport that has had no command holds leftovers |

### Effect fields

| Field | Meaning |
| --- | --- |
| `slot` | Effect-table slot (`core/unit-slots.md`) |
| `kind` | Missile or effect kind number (`combat/damage.md`, `spells/spells.md`) |
| `x`, `y` | Position in pixels |
| `dest` | Destination `[x, y]` in pixels. A blizzard shard has its chain's landing point from the moment it is created, through its initial wait (`spells/spells.md`, "Blizzard shards"); a death coil missile has its aim point from creation; a death-and-decay cloud has the target tile's centre; a whirlwind has its current goal; a fireball has its target tile's centre and a flame shield bolt (kind 3) its target unit's centre, from creation (`spells/spells.md`). A flame (kind 4), a healing (kind 9) or an exorcism (kind 8) effect has (0, 0) |
| `target`, `shooter` | Unit slots, or `null`. A flame (kind 4) has the shielded unit as its shooter and no target; a healing or exorcism effect has neither (`spells/spells.md`). An effect's `shooter` is never cleared: when the shooter's slot is released (the shooter died and its body is gone), the effect still reports that slot number, whatever the slot holds later (`spells/spells.md`, "Spell effects in flight"). Compare a dead unit's `target` above |
| `damage` | The damage value stored on the missile (`combat/damage.md`) |

### Commands

Each entry of a `turn` record's `commands` list is one command the game carried out in that turn, and has `player`
and `kind`.

| `kind` | Fields | Meaning |
| --- | --- | --- |
| `select` | `units` | Replaces the player's **synced selection** (`ui/selecting-and-commanding.md`, "Two selections") with these unit slots, in this order (at most 9). A slot that holds no unit when the select runs is kept, not dropped (traced, round 107: PI1a) |
| `command` | `command`, `tile`, `target` | Gives command number `command` (`orders/orders.md`, "Commands"; a spell by its spell order number, 38 to 56) to the units of the player's synced selection, as in `ui/selecting-and-commanding.md`, "Which units take part in a command": the list of handled commands, the selection filter and the formation offset. `tile` is `[x, y]`; `target` is a unit slot or `null`. A target slot that holds no unit is still the command's target (the command fails its target check: `orders/orders.md`, "A target slot with no unit"). The tracer writes a tile for every command; a reader takes a missing `tile` as `[0, 0]`, the tile the game sends for a command with no tile and no target (stop, stand ground) |
| `build` | `type`, `tile` | The first unit of the player's synced selection, if it can build (a worker, or a tanker, which builds oil platforms; `economy/build-train.md`), is told to build the building of type `type` with its top-left tile at `tile` (`economy/build-train.md`, "Placing a building"). The site is checked when the command runs; on a failure a message is shown and nothing else happens |
| `job` | `job`, `number` | The first unit of the player's synced selection starts a job (`economy/build-train.md`, "Starting a job", whose checks include that it is a completed building). `job` is `"train"` (`number` is a unit type), `"research"` (research a spell; an upgrade number), `"upgrade"` (an upgrade number) or `"upgrade building"` (a building type). A job kind outside these four is written as its number |
| `cheat` | `cheats` | A cheat command, carried out as in `game/cheats.md`, "Carrying out a cheat command". `cheats` is the **whole new set** of cheat flags it carries, not only the ones it turns on ("Cheat commands") |
| `cancel` | — | Cancel construction: the first unit of the player's synced selection, if it is the player's own building under construction, is taken down (`economy/build-train.md`, "Cancelling construction"). Otherwise it does nothing |
| `cancel-job` | — | Cancel job: the first unit of the player's synced selection, if it is the player's own building, has its job marked to be cancelled at its next work run (`economy/build-train.md`, "Cancelling a job"). Otherwise, or if it isn't working, it does nothing |
| `hold-workers` | — | Hold workers: the player's synced selection's workers are held (`orders/orders.md`, "Holding and releasing workers"). The game sends it when its player picks a building to place (round 101) |
| `release-workers` | — | Release workers: the same units are released (the same section). The game sends it when its player leaves placement mode without placing (round 101) |
| `right-click` | `tile`, `target` | A right-click command for the player's synced selection (`ui/selecting-and-commanding.md`, "What a right-click command does"), as the game sends it for a right click on the map: `tile` is `[x, y]`, `target` a unit slot or `null`. It doesn't release held workers (`orders/orders.md`, HW7b; round 102) |
| `set-group` | `group` | Set control group `group` (0 to 17) from the player's synced selection (`ui/selecting-and-commanding.md`, "Control groups"), as the game sends it for its group keys. A turn record names it only when the trace has the `groups` extra; otherwise the same command is listed as `other` (round 103) |
| `recall-group` | `group` | Recall control group `group` into the player's synced selection (the same section), likewise named only with the `groups` extra (round 103) |
| `other` | — | A command of a kind this format doesn't describe. No input command entry gives one, but other inputs make them: a `say` entry's typing (the pause and the resume around it), and on the multiplayer launch the second human's chat lines, its leave or drop notice, and the message a click on a command button (such as Move) sends, which changes no unit. A comparer lists them and carries out nothing for them |

A command whose conditions fail (a unit not owned by the player, a builder that is neither a worker nor a tanker, too
little gold) is still listed: it was carried out and did nothing.

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

### Saves, loads, cheat words and fog

Four input kinds drive the game's menus or keyboard instead of giving a command. Their entries have `n` and `player`
like the others; the harness acts at the stop after `cycle` record `n` and needs a few cycles to finish, so each
has a marker record a little later.

| Entry `kind` | Entry fields | Record | Record fields |
| --- | --- | --- | --- |
| `save` | `name`; optional `"clear": true` (the harness empties the name box first) | `save` | `name`; `file`, the file name written (`name` + `.sav`); `size` in bytes; `counter`, the program's cycle counter (reference only); the extras' fields |
| `load` | `name`; optional `row`: K, the row of the load list to pick (0 = top) | `setup` with `load` = `name` | A full `setup` record of the loaded game |
| `say` | `word`, a cheat word or phrase (`game/cheats.md`) | `say` | `word`; `cheats`, the cheat flags after it; `fog_of_war`, 1 when the fog-of-war option is on, else 0; the extras' fields |
| `fog` | `value`: `"on"` or `"off"` | `fog` | `value`; `before` and `fog_of_war`, the option before and after (0 or 1); `cheats`; the extras' fields |

**Save.** The `save` record's `n` is the cycle whose state was saved: a load of that save gives the state of the
`cycle` record with that `n`, as changed by `game/save-load.md`. The record itself is written 0 to 3 `cycle` records
later (traced: entry 100, saved state 103, `human1-human01-cheat-load-game-50`; entry 1560, saved state 1564,
`xorc10-xorc10-r96-repair-saveload-airesearch-aiworkers-game-6000`). The game goes on after a save.

**Load.** After a `load` entry the harness loads the save and the trace goes on with the loaded game: a `setup`
record with `"load": name` and `n` 0, then `startup` records 1 and 2 and `cycle` records from 3, as after a new
game's setup. In every traced load the loaded `setup` record's `rng` equals the header's `clock` (946684800). Input
entries after the `load` count `n` from the loaded game.

**Say.** The harness types the word on the traced player's keyboard. This makes three commands of the traced player,
each in a `turn` record of its own: an `other` (the game pauses while the line is typed), the `cheat` command the
word makes (`game/cheats.md`; a phrase with cheats disabled would be chat instead) and an `other` (the game
resumes). Several `turn` records can share one `n` here; the `say` record's `n` is the `n` of the turn with the
`cheat` command, though it is written a few `cycle` records later.

**Fog.** The harness switches the fog-of-war option. This makes no command. The `fog` record follows the `cycle`
record with its `n`, and the option is switched between that cycle and the next. Two `fog` entries with the same `n`
are carried out one after the other, each with a record of its own (entry n 12, records n 18 and 21,
`human1-human01-fog-switch-fogmaps-game-400`).

**What a comparer does.** At a `save` record it keeps its own state of the `cycle` record with that `n` under the
save's name. At a loaded `setup` record it replaces its game with that kept state, carried through
`game/save-load.md` (what a saved game keeps and what a load resets, including that a dead body's target becomes
`null`), and compares the `setup` record; from there it runs and compares as for a new game, counting `n` from 0 and
taking the input entries that follow the `load`. It applies `say` as the `turn` records list the commands, and the
fog option at each `fog` record. It doesn't compare the marker records as state.

### Multiplayer launch

The default launch starts a campaign mission. The harness can also start a skirmish map through the multiplayer menus: a
Direct Connect game between the traced player and a second human, plus computer opponents. The second human gives
commands only when the harness acts on its display ("The second human"). Such a trace has the multiplayer header
fields (`launch_path` to `starts`; traces made before round 60 stop at `humans`) and may have `peer` records; its
other records are as in a campaign trace, and campaign traces don't have these fields at all. The format version
stays 2.

**Enhanced is still the default and the target.** A multiplayer trace is Enhanced unless Classic was explicitly
requested. A Classic trace has `"rules": "classic"`, `"enhanced": false` and `-classic` in its file name, and is
evidence for Classic-mode rules only.

The other header fields keep their meaning. On this path, `scenario` and `launch` describe the map, the opponents and
the menu choices. `cheats` is normally empty, since no mission-select cheat is needed. `controllers` shows both humans
as `"human"`. `speed` is whatever the lobby set (pacing only).

**Start seed.** A multiplayer game isn't seeded from the wall clock. The lobby takes a seed byte from its own message
counter, so the real value varies from run to run; the harness fixes it so that runs repeat:

1. When the game starts, the generator is seeded with `start_seed`, and the players' start locations are drawn when
   the lobby's start locations are random.
2. At the end of the lobby, the generator is seeded with (`start_seed` + 4) mod 256. The game runs with this seed;
   the setup record's `rng` is the state after setup. The same byte orders the player colours.

The + 4 is exact: both bytes are the host's message sequence number, and exactly four host messages separate them,
whatever the timing (static study; every observed start agrees). Any start seed from 0 to 255 occurs in real games,
so a pinned start is one the game can produce. The fields say how to repeat the capture.

A comparer can derive the setup's generator state: seed with (`start_seed` + 4) mod 256, then make the opponent
placement draws of `game/setup.md` ("Choosing the players", multiplayer) before the unit-creation draws. With `"starts":
"random"` the opponent placement makes 1 + `opponents` draws when `opponents` is at least 1, and none when it is 0; with
`"starts": "fixed"` it makes none. A trace without these fields (before round 60) has random starts; take the opponent
count from its computer controllers after setup, which undercounts the draws only if a placement search failed (not seen
on any traced map). The setup record's `rng` checks the result. The humans' slots and start locations come from the
lobby (a later spec): take them from the header and the setup record.

**The second human.** The second human is the player number whose `controllers` entry is `"human"` and isn't
`local_player`. It is a human player, never a computer player, and gives no commands of its own. The harness can act
on its display, or on the traced player's game; each action is an `input` entry with `"kind": "peer"` and a `peer`
record. Both have `n`, `player` (the **traced** player's number, not the second human's) and `action`, with that
action's fields:

| `action` | Fields | What the harness does | Can make a command of the second human |
| --- | --- | --- | --- |
| `key` | `key`: the key, as `"Return"`, `"a"` or `"ctrl+q"` | Presses a key on the second human's display | Yes |
| `click` | `at`: `[x, y]` on the 640 × 480 screen | Clicks on the second human's display | Yes |
| `hold` | `secs`: 1 to 30 | Holds the traced player's game that long; the second human runs on to the next turn and waits, and may leave or drop the traced player (`net/leave-and-drop.md`) | Yes |
| `desync` | `what`: `"rng"` (the traced player's generator state, low bit flipped) or `"seen"` (the traced player's seen value on one tile, with `tile`: `[x, y]`); in the record only, `old` and `new`: the value before and after | Changes the traced player's game so that the sync check fails (`net/lockstep.md`) | Yes (the drop notice) |
| `shot` | In the record only, `files`: the two screenshot names | Takes screenshots of both displays | No |
| `own-key`, `own-click` | `key` or `at` as above | Presses a key or clicks on the traced player's own display | No |

A `peer` record follows the `cycle` record with the same `n` and comes before any `turn` record with that `n`.
Several entries with one `n` give consecutive `peer` records, in `input` order. An entry whose `n` is past the
trace's last record has no record.

The second human acts in its own time, so a command it causes has no fixed cycle: in the traced games it came 8 to
44 cycles after the action that caused it (round 96: a select 8 cycles after a click on a unit, a leave 33 and a
drop or surrender notice up to 44 after the action). An action may cause no command at all (a `"seen"` desync, a
short hold). The second human's commands are in the `turn` records under its own player number, have the same
fields as the traced player's, and are carried out like anyone's. Most are `other`: chat lines, leave and drop
notices, and the message a click on a command button sends (it changes no unit; the command itself follows a later
click on the map). The traced player's commands are still exactly its `input` command entries ("When a command
runs").

**What a comparer does:**

- **The second human:** apply its commands as the `turn` records list them, in their turns; they aren't derived from
  `input`, and T(i) of "When a command runs" doesn't apply to them. Check them: a **trigger** is a `peer` entry with
  action `key`, `click`, `hold` or `desync`. Each command of the second human must come in a turn whose `n` is
  greater than some trigger's `n`; a trace with no trigger has no command from the second human. Don't check the
  delay, which depends on the second human's pace. A command of the second human with no earlier trigger, or under a
  player number that is neither the local player nor the second human, is a difference.
- **Peer records:** skip them when comparing state; they only mark where the harness acted.
- **Turns:** command turns in a multiplayer game come at the network's pace, not every cycle. As before, a `turn`
  record is written only when the turn drew random numbers or carried out commands.

### Opt-in extras

The harness can add optional fields that a default trace never has. Each is opt-in and named in the header's
`extras`: `"hits"` adds the record field `hits`, `"goals"` adds the unit fields `goal`, `pending` and
`transport_dest`, `"aiworkers"` adds the record field `aiworkers`, `"groups"` adds the record field `groups`. They change nothing else: with these fields removed, a trace with extras has the same records as the
default trace of the same scenario, but for the permanent drawing effects in effect slots 0 to 2
(`core/unit-slots.md`), which can move by a record because watching the hits slows the game's pacing. No comparer
checks those slots.

**Hit entries.** A `hits` entry is one call of the common damage step (`combat/damage.md`, "Applying damage"), read
before anything is applied:

| Field | Meaning |
| --- | --- |
| `by` | What made the call: `melee`, `missile`, `splash`, `demolish`, `exorcism` or `other` (below) |
| `attacker` | Unit slot of the attacker given to the damage step (below), or `null` when none is a unit |
| `target` | Unit slot of the unit hit, or `null` when it isn't a unit |
| `damage` | The amount given to the damage step, 0 to 255, before its own checks |
| `hp` | Only when `target` isn't `null`: the target's hit points before the call |
| `effect` | `missile` and `splash` entries only, always on those: the effect slot of the missile, or `null` |

| `by` | When | `attacker` | Seen in a trace |
| --- | --- | --- | --- |
| `melee` | A melee unit's blow (`combat/damage.md`, "How an attack delivers damage"), one entry per blow | The striking unit | Yes |
| `missile` | A missile reaching its target unit | The unit that fired it, as in the missile's `shooter`; it may have died since | Yes |
| `splash` | An area missile's blast (`combat/damage.md`, "Splash on impact"): one entry per unit caught, all with the same `attacker` and `effect` | As `missile` | Yes |
| `demolish` | A demolition squad's or sapper's blast (`combat/attack-ground-demolish.md`); up to two calls per tile | The demolisher | **No: static study only** |
| `exorcism` | The exorcism spell (`spells/spells.md`) | The caster | **No: static study only** |
| `other` | Any other source | Whatever unit was given, or `null` | No |

The `demolish` and `exorcism` kinds, and their `attacker` meanings, come from static study only: no capture has
written one yet (the demolition scenario's blasts hit only walls). Treat them as expected, not observed.

An entry is a call, not damage done: the damage step may then do nothing (a dying or removed target, a passenger, the
Dark Portal case, unholy armour, an amount of 0; `combat/damage.md`, "Applying damage"), and a killing call doesn't
lower the target's hit points. Compare `hp` with the target in the same record to see what was applied; several
entries on one target in a record each show the hit points left by the ones before. Damage that doesn't go through
the common step is not listed: walls, runes and other direct changes.

**Computer worker jobs.** `"aiworkers"` adds the record field `aiworkers` to every record that has game state
(`setup`, `startup`, `cycle`, `turn`) and to every `save` record: the counts the computer workers' job choice reads
(`ai/worker-ai.md`), as stored. A count is a 16-bit number that wraps, so a count lowered past 0 shows as 65535 or
near it (`game/save-load.md`, SL22 to SL25, SL31, SL32).

| Field | What |
| --- | --- |
| `miners`, `lumberjacks`, `repairers` | 16 numbers, one per player number, each 0 to 65535: the player's count of workers with that job |
| `hall_cooldown` | 16 numbers, one per player number: each player's hall cooldown (`ai/worker-ai.md`) |
| `building` | An object with one entry for each building type a player has in progress with a nonzero count: key `"P:0xTT"`, the player number in decimal and the type as two lower-case hex digits (for example `"6:0x40"`), value 1 to 65535. A missing key means 0 |
| `jobs` | An object with one entry per unit of players 0 to 7 that has a job: key the unit's slot as a decimal string, value `[job, mode]` |

`job` is the sum of the unit's job bits: 1 mining gold, 2 cutting lumber, 8 building a farm (`ai/worker-ai.md`,
"Farm"), 16 building a build-list entry ("Build list") or a town hall started by the no-depot step ("Worker: the
steps, in order", step 5; traced, round 99: `mp-r99-tests-mp-r99-tests-fixed-aigoals-airesearch-aiworkers-5400`
n = 2, `[16, 0]`), 32 repairing.
`mode` is the worker's harvest mode: 128 gold, 64 lumber, 0 neither yet. Values the game keeps next to these that no
spec defines are left out.

**Control groups.** `"groups"` (round 103) adds the record field `groups` to every record that has game state: an
object keyed by player number (a decimal string, players 0 to 7), with an entry only for a player that has a nonempty
group or a nonempty synced selection. Each entry is `{"groups": {...}, "selection": [...]}`: `groups` maps each
nonempty group's number (a decimal string, 0 to 17) to its units' slots in group order, and `selection` lists the
slots of the player's synced selection (`ui/selecting-and-commanding.md`, "Two selections") in order, slots that hold
no unit included (round 107). Example
(`mp-r103-tests-mp-r103-groups-pushoff-fixed-goals-groups-1000`, n = 221): `{"0": {"groups": {"1": [1599, 1596,
1597]}, "selection": [1595, 1594, 1593]}}`.

**Which record lists a call.** The record whose work made the call: a `startup`, `cycle` or `turn` record. When a turn
made calls but gets no record (it drew no random numbers and carried out no command), its entries go with the next
record written. In a windowed trace calls before F aren't listed, except that a `turn` record in the gap lists its
own turn's calls.

**Comparing extras.** A comparer may ignore `extras` entirely: unknown fields don't change the result. When it checks
them, it compares `hits` only where a spec defines the calls (`combat/damage.md`), `pending` as an order number, and
`goal` and `transport_dest` only where a spec defines the stored value (leftovers on idle units and buildings mean
nothing). It compares `aiworkers` as 16-bit numbers, wrapped values included: the counts, `hall_cooldown` and
`building` exactly, and `jobs` only for slots that are in the same record's `units` (a released slot's leftover
bits, if one ever showed, would mean nothing; none has been seen). It compares `groups` exactly: each player's groups
with their entries in order, and the selection in order (V-G1 in `ui/selecting-and-commanding.md`). The research fields (`aigoals`, `aigroups`,
`aijobs`, `airesearch`, `fogmaps`, `game`, `vision`) are described in "Research extras" but aren't compared.

### Research extras

These extras add fields for research. A comparer ignores them; they are described here so a reader can interpret a
trace. A field becomes comparable only when a spec defines a rule for it and this spec says so, as the `goal` row did
for buildings. Every value is raw, as the game holds it at the record: counts are 16-bit unsigned (a count lowered
below 0 reads 65535), tiles are in tile units (a unit that was never given one holds 0 or a leftover value), and
per-player lists have 16 entries, players 0 to 15, unless a row says otherwise. The values were read by the harness
and checked against the committed traces named below; where a meaning comes from static study only, the row says so.

**`aigoals`** adds the unit field `ai_goal` to every unit, whatever its owner: `{"kind", "tile", "extra"}`, the
unit's computer goal (`ai/attack-ai.md`, "Unit goals"). Only the kind and the tile are the goal of that section; its
land area and marks aren't recorded.

| Field | Meaning |
| --- | --- |
| `kind` | The goal kind: 0 none, 1 hold, 2 attack, 3 ferry, 4 roam, 5 base-edge patrol, 6 sea patrol, 7 escort |
| `tile` | `[x, y]`, the goal tile. A worker's build job writes its site here (`ai/worker-ai.md`, "The goal tile of a build job") |
| `extra` | A value kept next to the goal. A computer worker's build job writes a building type number here, and setting a goal doesn't change it: the farm job writes the farm's own type (59, the pig farm, for an orc), a build-list job the list entry as the list holds it, which is the **human** building's number for either race (an orc barracks job writes 60, and the site becomes an orc barracks, 61). Traced in `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500`, `xhuman9-aigoals-goals-8630-8830` and `xhuman12-xhuman12-picker-flip-aigoals-1700`. The job's release reads it (`ai/worker-ai.md`, step 1). A transport's ferry steps also change it, but no rule reads that (static study). No other meaning is defined |

**`aigroups`** adds the record field `aigroups`: one 16-entry list per count, keyed `footmen`, `archers`, `siege`,
`knights`, `mages`, `demolition`, `gryphons`, `flyers` (flying machines and zeppelins), `transports`, `destroyers`,
`battleships`, `subs` (submarines), `workers` and `tankers`: each computer player's training group counts and its
worker and tanker counts (`ai/training-research.md`, "Training buildings"). The group counts are the ones a load adds
the loaded units to again (`game/save-load.md`, "What a saved game keeps"). `demolition` (demolition squads and
sappers) is a group count that `ai/training-research.md` doesn't list and no rule reads.

**`aijobs`** adds the record field `aijobs` (the name is historical: these are training counts, not worker jobs):

| Field | Meaning |
| --- | --- |
| `count` | Four 16-entry lists, in this order: the footmen, archers, siege and knights group counts (the same values as `aigroups`). Indexed `count[group][player]` |
| `wanted` | 16 lists of four values, one list per player: that player's wanted counts of the same four groups, script slots 0x14 to 0x17 (`ai/ai-scripts.md`, "Slots"), each 0 to 255. Indexed `wanted[player][group]` |
| `have` | Three 16-entry lists, in this order: each player's counts of lumber mills, blacksmiths and stables, which the barracks rules test (`ai/training-research.md`) |

**`airesearch`** adds the record field `airesearch`: an object keyed by player number as a decimal string, with an
entry only for the players whose controller is computer at that record. Each value:

| Field | Meaning |
| --- | --- |
| `requests` | The research codes (0x80 to 0x9A) whose request flag is set, ascending (`ai/training-research.md`, "Research requests and done marks") |
| `done` | The build-list entry numbers (0 to 63, from 0) marked done, ascending (`ai/worker-ai.md`, "Done marks") |
| `list` | The build list's entries in order, up to (not including) the first 0xFF, at most 64 (`ai/ai-scripts.md`, "Build list and second list"); empty when the player has no list |
| `limit` | Script slot 0x22, the build-list limit, 0 to 255 (0xFF when the script starts; `ai/ai-scripts.md`) |
| `levels` | The player's upgrade levels by line of `economy/tech-tree.md` ("Upgrade lines and their effects"): `arrow` (line 0, missiles), `weapon` (1), `shield` (2), `cannon` (3, ship cannons), `armour` (4, ship armour), `siege` (6), `rangers` (7), `longbow` (8), `scouting` (9) and `marksmanship` (10). Each is a count, 0 to 255; lines 7 to 10 reach only 1 with the default data |
| `spells` | The player's spells-known word (`economy/tech-tree.md`): bits 0 to 19 the spells, numbered as in `ai/caster-ai.md`; bit 20 the paladin (ogre-mage) upgrade |

**`fogmaps`** adds the record field `fogmaps`: a summary of the three per-tile fog maps of `vision/fog.md`, not the
maps themselves. Each map has one code or byte per tile of the whole map.

| Field | Meaning |
| --- | --- |
| `visible` | Tile counts of the local player's visible map: `clear` (code 0), `fogged` (code 16), `edge` (any other code) |
| `explored` | Tile counts of the local player's explored map: `explored` (code 0), `unexplored` (code 16), `edge` (any other code) |
| `seen` | An object keyed by the explored classes above (`explored`, `unexplored`, `edge`); each value counts the tiles of that class per value of the tile's "seen" byte (`vision/fog.md`, "The 'seen' map": bit p set when player p doesn't see the tile), keyed by the byte value as a decimal string, values with no tile left out |
| `local_units_on_explored` | `[a, b]`: b is the number of the record's `units` owned by the local player whose tile is on the map, a the number of those whose tile's explored code isn't 16 |

**`game`** adds the record field `game`:

| Field | Meaning |
| --- | --- |
| `cheat_used` | The cheat-used mark (`game/cheats.md`): 1 once a cheat command from the local player turned a cheat on, else 0 |
| `fog_countdown` | The fog refresh counter (`core/timing-and-rng.md`, phase 3): a refresh runs in the cycle that finds it at 0; that cycle's record shows **0** and the next record 99 (Enhanced; 100 in Classic mode, static study). An Enhanced holy vision sets it as that phase says: the records then show 150, 149, … 101, 100, 99. So in Enhanced mode a record showing 100 is never a refresh cycle, and a comparer whose own counter holds the reload value after a refresh must show 0 **only in a cycle where the refresh ran**, not whenever its counter is at the reload value (round 104, H1; vector below). A holy vision cast in a refresh cycle comes after the refresh (phase 4 follows phase 3), so that cycle's record shows **150**, not 0: the comparer shows 0 only when the refresh ran **and no holy vision set the counter later in the same cycle**; otherwise it shows the counter as it is (round 105, H2; vector below) |
| `victory_countdown` | The victory countdown (`game/victory.md`, "When the check runs"): the check runs in the cycle that finds it at 0, and that cycle ends with it at 200 |
| `fog_of_war` | The game's fog-of-war option: 1 on, 0 off |
| `results` | Eight values, players 0 to 7: each player's result (`game/victory.md`; `net/leave-and-drop.md`, "Data"): **0** none, **1** dropped, **2** defeat, **3** victory, **5** game ended. Codes 0, 2 and 3 are traced (below); codes 1 and 5, and that no other value is ever written, are static study |
| `controllers` | The 16 slots' controllers, written as in the header, read at each record: a player who leaves becomes `"nobody"` (traced: 16 changes, every one from `"human"` to `"nobody"`) |

**Result codes, the evidence.** In the 23 committed multiplayer traces with the `game` extra, every empty or beaten
slot gets 2 at a victory check and the winner gets 3 at the check that finds it: for example
`mp-skirmish-bne-mp-skirmish-nobody-fight-game-1300`, player 0 at n = 1208 (`net/leave-and-drop.md`, LV19), and
`mp-dark-paths-bne-mp-joiner-waits-game-600`, player 1 at n = 203. No committed trace reaches "dropped" or "game
ended" (Battle.net only, `net/leave-and-drop.md`); those codes match the Battle.net report table there (1 a
disconnect, 2 a loss, 3 a win, 5 a draw; "Surrender and quit (draw)"). No single-player trace has a nonzero result. A player who leaves gets 2
(defeat) from the victory check, not a "left" code.

**`vision`** adds two unit fields to every unit, each a set of players 0 to 7 (bit p = player p):

| Field | Meaning |
| --- | --- |
| `hidden_from` | The players the unit is hidden from, as phase 5 last left it (`vision/fog.md`, "Unit visibility (phase 5)") |
| `detected_by` | The players that detect it, as phase 1 last left it (`vision/fog.md`, "Detection (phase 1)"): 255 (everyone) for a unit no rule restricts; the owner's shared-vision set for an invisible unit; the detecting players for a submarine; 0 for a unit created during the cycle, until the next phase 1 |

At the setup record every unit has `hidden_from` 255 and `detected_by` 0 (traced): the sets are first worked out in
the first start-up cycle.

### Format versions

| # | Where | Version 1 | Version 2 |
| --- | --- | --- | --- |
| 1 | Header `version` | `1` | `2` |
| 2 | Header | — | Field `input` |
| 2 | `commands` kinds | — | `select`, `command`, `build`, `job`, `other`; `cheat` was added later without a version change, since no trace had written a cheat command as `other`, and so were `cancel` and `cancel-job` (round 77), since no input scenario had given either, and `hold-workers` and `release-workers` (round 101), `right-click` (round 102), and `set-group` and `recall-group` (round 103, named only in a trace with the `groups` extra). A trace written before round 101 would list the first two as `other`, and one written before round 102 a right-click as `other` |
| 3 | `turn` record, when written | Only when the turn drew random numbers | When the turn drew random numbers or carried out at least one command |
| 4 | `turn` record fields | The record fields | The record fields and, when commands ran, `commands` |

Every version 1 field keeps its meaning. A version 2 trace without input is identical to a version 1 trace apart
from `version` and an empty `input`.

The opt-in header fields `from`, `extras` and `setup_only` and the extras' fields came later without a version change:
a trace without them is exactly as before. A comparer must honour `from` (one that ignores it sees n jump from 2 to F
and diverges there) and `setup_only` (it isn't a truncated trace); the extras' fields may be ignored.

The input kinds `save`, `load`, `say`, `fog` and `peer`, the record kinds `save`, `say`, `fog` and `peer`, and a
loaded game's `setup` record (round 96 in this spec; the traces are older) also came without a version change: a
trace without them is exactly as before. A comparer must honour a loaded `setup` record (n starts again at 0) and
skip the marker records as state.

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

The multiplayer header fields of `mp-dark-paths-bne-1000` (Enhanced; fields abridged):

```json
{"format":"tidewater-oracle-trace","version":2,"speed":8,"enhanced":true,"rules":"enhanced","local_player":1,"controllers":["human","human","computer","nobody","…"],"cheats":[],"unit_slots":1600,"effect_slots":400,"launch_path":"multiplayer","connection":"direct-connect","map":"Dark Paths BNE.pud","start_seed":1,"humans":2,…}
```

A record with both extras, from `orc9-orc09-order16-keep-goals-hits-2000` (Enhanced; campaign, no `from`; fields
abridged):

```json
{"format":"tidewater-oracle-trace","version":2,…,"enhanced":true,"rules":"enhanced",…,"extras":["goals","hits"]}
{"n":936,"kind":"cycle",…,"units":[…,{"slot":1492,…,"hp":52,"order":16,…,"target":1485,"goal":[5,86],"pending":null},…],"effects":[…],"hits":[{"by":"melee","attacker":1484,"target":1492,"damage":8,"hp":60}]}
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
| `human06-31357`, n = 18146: body 1594 (type 105, order 0, the remove order; it died at n = 16653) still has target 1511, whose slot is released in this cycle (1511 died at n = 16726) | Its `target` stays 1511, not `null` |
| `xorc12-spells-1500` (Enhanced), n = 1166: whirlwind effect 3 (kind 12), cast by Teron Gorefiend 1583 (type 0x15), who died at n = 633; 1583's slot is released in this cycle | Its `shooter` stays 1583, not `null`; the whirlwind goes on |
| `xhuman10-blizzard-600`, n = 145: new shards in effect slots 4, 5 and 7, in their initial wait | `dest` is each chain's landing point: (2480, 2864), (2512, 2928) and (2576, 2800), unchanged when they start flying at n = 157 |

These come from the multiplayer-launch traces in `oracle/traces/` (Dark Paths BNE, one computer opponent, an idle
second human, no input, 1000 cycles).

| Input | Expected |
| --- | --- |
| `mp-dark-paths-bne-1000` (Enhanced): `start_seed` 1 | Header `rules` `"enhanced"`, `enhanced` true, speed 8, `local_player` 1, `controllers` begin `"human"`, `"human"`, `"computer"`; 1600 / 400 slots; setup `rng` 0x2E4642F9; 13 units at setup |
| `mp-dark-paths-bne-classic-1000` (**Classic mode**): `start_seed` 1 | Header `rules` `"classic"`, `enhanced` false, speed 6, 600 / 200 slots; setup `rng` 0x2E4642F9 and the same units and positions as the Enhanced trace; only unit slot numbers differ |
| The same map with `start_seed` 2, Enhanced (a harness check; no trace committed) | `local_player` 2; setup `rng` 0x9FE5034A: the seed decides start locations and player numbers |
| Either committed trace, captured twice with the same fields | Identical records |
| `mp-gold-rush-bne-fixed-6000` (Enhanced): `opponents` 2, `starts` `"fixed"` | Setup `rng` 0x0452EB91, 164 steps from 5: the 2 × 82 unit-creation draws and no placement draw |
| `mp-dark-paths-bne-fixed-1000` and `mp-rose-petal-bne-fixed-3000` (Enhanced): `opponents` 1 and 4, `"fixed"` | 26 and 86 setup draws: 2 fewer and 5 fewer than their random-start twins (28, 91), that is 1 + `opponents` |

These come from the 2026-10-14 harness checks of the opt-in extras (all Enhanced; harness checks, not committed).
Four golden traces re-captured without extras (three campaign, one multiplayer) have every record identical to the
committed files.

| Input | Expected |
| --- | --- |
| `orc6-6000-8000`: Orc06, no input, `from` 6000, 8000 cycles | Records n 0, 1, 2, then 2001 records from n 6000, identical to the same records of a complete `orc6-8000` |
| `orc9-orc09-order16-keep-goals-hits-2000`, n 936 (`cycle`) | `hits` `[{"by":"melee","attacker":1484,"target":1492,"damage":8,"hp":60}]`; unit 1492 has `hp` 52, `goal` [5, 86] and `pending` null in the same record; 59 entries in the trace, all `melee`; with the extras removed, the records equal the default capture's |
| `xhuman10-xhuman10-spells-goals-hits-700`, n 86 | A ballista bolt (`effect` 3, `attacker` 1483) gives six `splash` entries, then a `missile` entry hits unit 1497 at `hp` 37 (50 − 13 from its splash entry just before) |
| `human12-human12-demolition-goals-hits-450` | Transports 1428 (type 0x1C) and 1522 (type 0x1D) have `transport_dest` [36, 114] and [68, 34]; no record has `hits` (the blasts hit only walls); buildings hold leftover goals such as [0, 65535] |
| `orc10-0`: Orc10, 0 cycles | 2 lines; header `"setup_only": true` and otherwise as `orc10-35000`'s; the setup record equals `orc10-35000`'s |
| `mp-dark-paths-bne-goals-hits-500-1000` (multiplayer, `from` 500, both extras) | Header `"from": 500`, `"extras": ["goals","hits"]`; records n 0, 1, 2 then the 501 from n 500, equal to `mp-dark-paths-bne-1000`'s with the extras removed; no `hits` by n 1000 |

These come from the round-96 traces and checks (all Enhanced; `ERRATA-round96-traces.md`).

| Input | Expected |
| --- | --- |
| `xorc11-xorc11-r96-targets-saveload-game-300`: saved state n = 881, then loaded | Loaded `setup` record: the six dead bodies (type 0x69) have `target` `null`; dead dragon 1526 keeps 1542 and juggernaught 1512 keeps 1519; `rng` 946684800 (`game/save-load.md`, SL28) |
| `human1-human01-cheat-load-game-50`: `save` entry n = 100, `load` entry n = 250 | `save` record n = 103 (`file` `"ga5.sav"`), written after `cycle` 103; after `cycle` 252 a `setup` record with `"load": "ga5"` and n 0, then `startup` 1 and 2 and `cycle` 3 on |
| `human1-human01-fog-switch-fogmaps-game-400`: `say` `showpath` at n = 10 | Three `turn` records with n = 12: the traced player's `other`, `cheat` with `["tigerlily", "showpath"]`, `other`; then a `say` record with n = 12 after `cycle` 14 |
| Same trace: `fog` `"on"` and `"off"`, both at n = 12; `"on"` at n = 150 | `fog` records n = 18 (`before` 0, `fog_of_war` 1) and n = 21 (1, 0), and n = 152 (0, 1); no `turn` record for them |
| `mp-battle-2-mp-battle2-nobody-moving-game-goals-vision-1450` (traced player 0, second human 1): `peer` clicks at n = 1170 (a unit), 1180 (the Move button), 1190 (the ground), `key` `ctrl+q` at 1212 and `Return` at 1217 | Second human's commands: `select` turn 1178, `other` 1202 (no unit changes), `command` 3 turn 1214, `other` 1250 (the leave); each after a trigger with a smaller `n`. The traced player gives none |
| `mp-dark-paths-bne-mp-r96-allowsync-desync-rng-game-400`: `peer` keys at n = 60 to 82 (Return, `allowsync`, Return), then `desync` `"rng"` at 150 | `other` of the second human (player 0) in turn 98 (the chat line) and turn 170 (the drop); the `peer` record at 150 has `old` 2394588402 and `new` 2394588403 (the low bit flipped). Every record equals `mp-dark-paths-bne-mp-desync-rng-game-400`'s but the chat turn |
| Each of the 45 multiplayer traces in `oracle/traces/` as of round 96 | No command of the second human before a trigger; none at all in the traces without one; the traced player's commands are exactly its `input` command entries; no other player number gives a command |
| `xorc10-xorc10-r96-repair-saveload-airesearch-aiworkers-game-6000`, `aiworkers` | On every `setup`, `startup`, `cycle`, `turn` and `save` record; `repairers` of player 6 go 0 → 65535 at n = 634 of the loaded game and 65534 at 653; `building` `"6:0x40"` is 1 in the `save` record, absent from the loaded `setup` record, and 65535 from n = 678 (`game/save-load.md`, SL31, SL32) |
| `sweep-xorc8` (oracle sweep trace, Enhanced, kept outside the repository; reproducible with `oracle/sweep.py capture`), `fog_countdown`: a refresh at n = 40301 (cycle counter 40299); computer paladin 1582 casts holy vision at n = 40414 and n = 40416, then again at n = 40468 | n = 40301: 0; n = 40302: 99. n = 40414: 150; n = 40416: 150, then 149 … n = 40465: 101, n = 40466: **100**, n = 40467: 99 (no refresh between 40416 and 40467); n = 40468: 150. A comparer shows its own counter as 0 at n = 40301 and as 100 at n = 40466 |
| `win-xorc8-game-44400-44480` (committed windowed trace, Enhanced), `fog_countdown`: 1 at n = 44463, so n = 44464 is a refresh cycle; in it computer paladin 1532 (type 0x0C, order 38) casts holy vision (mana 255 to 185) | n = 44463: 1; n = 44464: **150**; n = 44465: 149, then one less per cycle. The refresh did run at n = 44464 (every other field, the `vision` extra included, matches `sweep-xorc8` to n = 60000 with `fog_countdown` left out). A comparer shows 150 at n = 44464, not 0. Same pattern: `sweep-xorc12`, n = 30615, paladin 1387 |

## Open questions

- **Other speeds.** In single player a command turn comes before every cycle at every speed (`net/lockstep.md`,
  static study). In a network game turns come every K cycles, where K depends on the delay row and the speed; the
  Direct Connect launch runs with K = 12 at speed 8 (traced, round 89). The trace's `turn` records stay
  authoritative.
- **Commands the game queues by itself:** none has been seen; the Human12 select was the harness's own click
  (found in round 35).
- **The second human's commands** are described ("The second human"). The delay from a `peer` action to the command
  depends on the second human's own pace (8 to 44 cycles in the traced games), so a comparer checks the order, not
  the delay. Commands of a kind other than `select`, `command` and `other` from the second human haven't been traced.
- **Research extras.** Described in "Research extras" (round 98). They stay research fields: a comparer ignores them.
  A field becomes comparable only when a spec defines a rule for it, as `goal` did for buildings. The `game` extra's
  results 1 (dropped) and 5 (game ended) are static study: no committed trace reaches them.
- **Setting up a multiplayer start:** the generator state and the computer slots can now be derived ("Multiplayer
  launch"); the lobby's start-location draw isn't specified yet, so the humans' slots and start locations still
  come from the trace.
- **Other kinds** (cancel, alliances, speed, transports) aren't described yet; add them when a scenario needs them.
- **Two cheat entries handed over before the first runs:** the harness builds the second from the flags at hand-over,
  so it would drop what the first turned on. No scenario does this; the `turn` record shows what ran either way.
- **Cheat commands the game makes itself** (mission select sends one with the flags unchanged) would appear as a
  `cheat` entry with the same flags. None has been seen in a trace.
- **Multiplayer cheats:** the throw-away draw when a game disables cheats hasn't been traced.
- **Extras not yet observed:** no capture has a `demolish` or `exorcism` hit entry (static study only), and the
  stored goal of a unit that isn't moving has no defined value.
- Fog of war, the map's tiles (trees cut, walls) and most of the AI state aren't recorded in a described field yet
  (only the option's on/off value in the `say` and `fog` records, and the computer workers' counts in `aiworkers`).
- **Scenario identity.** `scenario` and `launch` are free text, so a comparer can't yet set up the scenario from the
  header alone. Default: each comparer test names its scenario (campaign entry or map file) itself; a structured
  field can come in a later format version.
