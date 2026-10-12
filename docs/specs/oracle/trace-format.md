# Oracle: per-cycle trace format

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** this is our own format, written by the oracle harness (`oracle/` in this repo, research side), which
  observes the original 2.02 program running under Wine. The field meanings follow the approved specs named in each
  row; nothing here is copied from the program.
- **Changes:** 2026-10-12: first version (format version 1).

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
| `turn` | After a command turn (commands from the queue were carried out between cycles), only when the turn drew random numbers. A turn without draws gets no record. | the `n` of the cycle before it |

- `n` counts simulated cycles from the start of the game, start-up cycles included. A `turn` record shares its `n`
  with the `cycle` record before it, so compare records by `n` and `kind` together, in file order.
- **Paused cycles** (and, in single player, cycles while the game window is inactive) do no simulation work and get
  no record. The program's own cycle counter is given in `counter` for reference only. It can drift from `n` (it
  doesn't count the start-up cycles, and may count paused ones), so a comparer should ignore it.
- Each record is the full state at that point: it doesn't depend on earlier records.

### Header

| Field | Meaning |
| --- | --- |
| `format`, `version` | `"tidewater-oracle-trace"`, `1` |
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
| `cheats` | The cheat flags on at setup, by their phrase or word as in `game/cheats.md` (for example `"tigerlily"`, mission select, which the launch uses; `"title"`, `"noglues"`); the two notices without a phrase are `"notice A"` and `"notice B"`; empty when none |
| `unit_slots`, `effect_slots` | Table sizes (`core/unit-slots.md`, "Table sizes") |

### Record fields

| Field | Meaning |
| --- | --- |
| `n`, `kind` | As above |
| `counter` | `cycle` records only: the program's cycle counter after this cycle (reference only, see above) |
| `rng` | The simulation generator's state `s` after this record's work (`core/timing-and-rng.md`, "Random number generator"). The number of draws made since the previous record is the number of steps from the previous `rng` to this one |
| `gold`, `lumber`, `oil` | 16 entries each, the stock of each player number |
| `units` | Every unit-table slot not on the free list, in ascending slot order (below) |
| `effects` | Every effect-table slot in use, in ascending slot order (below) |

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

## Data

### Example (one unit shown, numbers abridged)

```json
{"format":"tidewater-oracle-trace","version":1,"scenario":"campaign mission 'human1', no input","clock":946684800,"speed":8,"enhanced":true,"era":1,"map_size":32,"local_player":1,"controllers":["computer","human","nobody","…"],"races":["orc","human","human","…"],"cheats":["tigerlily"],"unit_slots":1600,"effect_slots":400}
{"n":0,"kind":"setup","rng":4116580520,"gold":[1000,1000,…],"lumber":[…],"oil":[…],"units":[{"slot":1588,"serial":12,"type":1,"owner":0,"x":640,"y":992,"tx":20,"ty":31,"hp":60,"order":2,"facing":7,"anim":2,"frame":0,"wait":7,"mana":0,"target":null}],"effects":[{"slot":0,"kind":19,"x":0,"y":0,"dest":[0,0],"target":null,"shooter":null,"damage":0},…]}
{"n":3,"kind":"cycle","counter":1,"rng":…,"units":[…],"effects":[…]}
```

## Test vectors

These come from the first oracle trace (Human01, no input; the header's `launch` field gives the exact launch).

| Input | Expected |
| --- | --- |
| Clock 946684800, campaign mission Human01, 12 map units | Setup record `rng` = 0xF55E08A8, which is exactly 24 steps from the seed 946684800: 2 creation draws per unit |
| Same, the first two records after setup | `startup` n = 1 (0 draws), `startup` n = 2 (1 draw); then `cycle` n = 3 with `counter` 1 |
| Same, cycles n = 3 to 2000 | Draws per cycle repeat 1, 2, 1, 1, 3, 3196 in all (8 per 5 cycles: each of the 8 idle mobile units draws once per 5 cycles; this idle draw isn't yet in `core/timing-and-rng.md`'s draw table) |

## Open questions

- Command turns: the format records a turn only when it draws. Input scenarios will need the commands themselves
  (who, what, which units, target) in the turn record; to be added with the first input scenario.
- Fog of war, the map's tiles (trees cut, walls) and the AI state aren't recorded yet.
- **Scenario identity.** `scenario` and `launch` are free text, so a comparer can't yet set up the scenario from the
  header alone. Default: each comparer test names its scenario (campaign entry or map file) itself; a structured
  field can come in a later format version.
