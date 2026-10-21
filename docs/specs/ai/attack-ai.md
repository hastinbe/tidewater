# AI: computer army, attacks and defence

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the AI script file
  `Rez\ai.bin`, the unit data `Rez\unitdata.dat` and the animation data `Rez\script.bin` in the game data, and the
  unit flags, "can target" values and `UNIT` section in the public PUD specification text. Not yet confirmed against
  the running game, except when the first goal is given and when the 50-cycle passes run (oracle traces and
  research-side command logs, round 31).
- **Changes:** 2026-10-12, round-14 errata: the ferry gather vector corrected (units after the 6th counted one are
  not visited), and a mixed-areas vector added. 2026-10-12, round-17 errata: the caster spell step is now
  specified in `ai/caster-ai.md`.
  2026-10-12, round-31 errata: the first goal is given when any unit is created, map units included; the 50-cycle
  passes count the start-up cycles, so they first run in the first start-up cycle.
  2026-10-12, round-40 errata: sea patrol's anchor tanker is the first tanker that isn't inside; with every tanker
  inside the anchor is the unit's own tile; `xorc7` and `human10` vectors.
  2026-10-12, round-42 errata: call for help tests only the answering unit's top-left tile, and the box reaches 3 rows
  above its top; `orc02` vector.
  2026-10-12, round-43 errata: a recorded attacker that is dying doesn't stop a unit from answering a call for help;
  `orc02-assault` vector.
  2026-10-12, round-44 errata: set goal's blocking test sees the unit's own occupancy mark, so a goal given at the
  unit's own tile is moved by the ring search; `human01-order15-range-700` vector.
  2026-10-13, round-46 errata: the own-tile rule is credited to the hold goal set at the unit's own tile, not to the
  leash return (the `human01-order15-range-700` vector corrected); hold step 3's smart move goes to the goal tile as
  given (traced: `xorc3-10000`; static check open).
  2026-10-13, round-47 errata: the call for help tests only whether S can target A's kind, so it answers a dying
  attacker; the hold return's smart move to the goal tile as given is confirmed by static study (open question closed).
  2026-10-13, round-49 errata: a live attacker that V's owner doesn't detect stays recorded, so its next hit makes no
  new call; the submarine call's command 4 records V as the target and ends once V is dying.
  2026-10-13, round-51 errata: the enemy search tests only alliance, the attack order's kind check and dying, gone or
  inside: no fog, detection, air-against-air or minimum-range test, and no score; the nearest candidate wins (strictly
  nearer replaces); `xorc8-10000` and `xhuman7-10000` vectors.
  2026-10-13, round-55 errata: the sea and base-edge patrols set the unit's current patrol ends but not its remembered
  ones (Enhanced), so a patrol resumed during its first leg turns toward the unit's own tile at the command, not the far
  end; `xorc11-air-sea-3000` vector.
  2026-10-13, round-56 errata: a held unit answering a call for help gets goal attack with no tile given, so its goal
  tile is its own tile as it stands (no ring search); `xorc12-spells-1500` vector.
- **Changes:** 2026-10-16, round 82 (AI consolidation, `ERRATA-ai-consolidation.md`, reviewed by a spec-review
  session, 2026-10-16): the model of the whole computer player is in `ai/computer-player.md`. The all-out attack's
  trigger is restated as C1 (counted at the kill, traced); the attack goal is stated to apply to workers and tankers
  too (X9, static study); the held mark also skips oil platforms (X4, as `game/setup.md`).
- **Changes:** 2026-10-16, round 83 (traces, `ERRATA-round83.md`, reviewed by a spec-review session, 2026-10-16):
  the all-out attack's count keeps an oil platform under construction (AI5, traced); a worker given command 4 by the
  attack goal makes its job choice in order 4 (AI1, traced); an active rescue player's map units get no first goal
  (default, from AI2's traced creation marks); open question on a worker's attack goal after its job choice. Review
  fix: the passes after the job choice found the worker within its leash, so they don't settle that question; its
  default is the stated rules (the goal stays), not "the job choice ends it".
- **Changes:** 2026-10-16, round 84 (traces, `ERRATA-round84.md`, reviewed by a spec-review session, 2026-10-16): a
  worker's attack goal survives its job choice (AI6, traced: `xhuman5-xhuman05-worker-all-out-goals-3000`; open question
  closed). New open question on the goal tile the passes use after the worker's farm job; review: the trace fits a
  stored goal tile of (6, 19), the farm site, not (9, 19) or (9, 20), held with a trace request. An active rescue
  player's map units and the first goal (AI7): the default stays, untraced; by the stated rules it changes no outcome in
  any shipped mission.
- **Changes:** 2026-10-16, round 85 (AI audit errata, `ERRATA-audit-ai.md`, with traces, `ERRATA-round85.md`;
  reviewed by a spec-review session, 2026-10-16): a worker's build job writes the site into its goal tile (erratum 7,
  traced for a farm job: `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500`; the AI10 open question closed).
  Static gap-fills: the nearest gold store has no alive test (erratum 1, in part); a post's line walk from a store tile
  shared with the enemy walks east (erratum 4; open question answered); the sea picker never picks a ship at
  distance 0 (erratum 15); the ferry escort and the submarine call don't skip dying units (errata 13 and 14). Held as
  open questions, stated rules kept: the base box clamp on a 128-wide map (erratum 5), a dying anchor tanker
  (erratum 9) and, by review, the nearest gold store's measure (erratum 1: this file states the reference-tile
  distance; the top-left-tile reading is static only and no trace separates them).
- **Changes:** 2026-10-16, round 86 (follow-up traces, `ERRATA-round86.md`; reviewed by a spec-review session,
  2026-10-16): the nearest gold store is measured from the store's top-left tile (erratum 1's measure): traced for the
  target pickers' reference store (`xhuman12-xhuman12-picker-flip-aigoals-1700`), and by review extended to the post
  and the ferry's store fallback, which static study reads as the same search (open question closed). Static gap-fill:
  a reference unit that is a building is measured at its reference tile. Requests AA5b (a dying anchor tanker) and AA7
  (the 128-wide clamp) were tried and skipped or found unreachable, and leave the queue; those questions keep their
  defaults.
- **Changes:** 2026-10-21 (B-040 game audit, `ERRATA-audit-game.md`, erratum 1, traced in round 90,
  `ERRATA-round90-traces.md`; reviewed by a spec-review session, 2026-10-21): new "When a unit is converted": in
  Enhanced a converted unit that isn't held gets its goal again as at creation (a computer player's mobile fighting
  unit gets a fresh first goal, with the post walk's draws); a held unit keeps its goal. The open question "Units
  converted during a mission" is answered for conversions; its owner-change default stays.

## Behaviour

This spec covers what a computer-controlled player does with its fighting units: how it rates strength around the
map, when and where it attacks, how each unit holds its post or attacks, how transports ferry units to other land
areas, and how nearby units answer a call for help. The slots the script writes are in `ai/ai-scripts.md`. Workers
and tankers are in `ai/worker-ai.md`. Training and research are in `ai/training-research.md`. Spell casting by
computer units is in `ai/caster-ai.md`.

"Computer player" means a player whose controller is "computer" (`ai/ai-scripts.md`). "Human player" means a player
whose controller is "human". Unit flags are the PUD flag bits (0 land, 1 air, 3 sea, 5 building, 8 worker/peon,
9 tanker, 10 transport, 11 oil platform, 12 gold store, 17 caster, 19 can attack, 21 oil patch, 25 and 26 as in the
PUD text). A **mobile** unit has at least one of flag bits 0 to 4. A unit is **gone** when it is dying, dead, removed
or inside something; "alive" means not gone.

Distances here are the **reference-tile distance** of `orders/orders.md` ("Distance" under automatic target
acquisition): the larger of the horizontal and vertical tile distances, to a building's nearest edge column or
middle column as described there. "Nearest" always means the smallest such distance, with the **first** candidate
in the order stated winning a tie. The one exception is "the nearest gold store" (below), measured from the store's
top-left tile (round 86).

### Timing

Two passes run every 50 cycles, in phase 2 of the cycle (`core/timing-and-rng.md`), after all computer players have
run their script step. Both use counters that start at 1 when a game starts and count every cycle, the two start-up
cycles included (`core/timing-and-rng.md`, "Scheduler", rule 1). So both run in the **first start-up cycle**, and
then every 50 cycles: in the normal cycles that end with the cycle counter at 49, 99, 149, and so on. In such a
cycle, in this order:

1. **Pass A:** the threat map is rebuilt, then attack waves are launched (below).
2. **Research requests with no workers** (`ai/training-research.md`).
3. **Pass B:** each computer player's base box is rebuilt, then every computer unit with a goal runs its goal
   (below).

Nothing in this spec runs between these passes, except the call for help and the losses rule, which run when they
are triggered.

### Unit goals

Every unit has a **goal**: a kind, a goal tile, a goal land area and two marks. A new unit starts with goal kind
**none**, no tile and no marks.

| Kind | Name | Handled in pass B |
| --- | --- | --- |
| 0 | none | nothing |
| 1 | hold | "Hold" below |
| 2 | attack | "Attack" below |
| 3 | ferry (transports) | "Ferry" below |
| 4 | roam (air units) | "Roam" below |
| 5 | patrol the base edge | "Base-edge patrol" below |
| 6 | sea patrol | "Sea patrol" below |
| 7 | escort | nothing |

The two marks are **needs a ferry** (with an **assigned transport**, possibly none) and **held** (below).

**Setting a goal.** "Set goal (unit, kind, tile)" does this and reports success or failure:

1. It fails, changing nothing, if the unit isn't mobile or is gone.
2. With no tile given: the goal tile is the unit's own tile, the kind is set, success.
3. An **air** unit: the goal tile and kind are set, success.
4. Otherwise the tile is first adjusted:
   - a **sea** unit: if the tile has the land bit (0x01) or the shore-water bit (0x02), a ring search
     (`movement/placement.md`) around the 1 × 1 rectangle at the tile, limit L = 16, looks for a tile with the
     open-water bit (0x40) and no bit of the sea blocking mask (0x0903). If one is found it replaces the tile;
   - any other unit: if the tile has any bit of the land blocking mask (0x09CE), the same ring search looks for a tile
     with the land bit (0x01) and no bit of the land blocking mask. If one is found it replaces the tile.
   The blocking test sees the occupancy marks of every unit on the tile, the unit's **own** mark included
   (`movement/occupancy.md`): the unit isn't left out. So a tile given that is the unit's own tile counts as blocked for
   a land unit standing on it, and the ring search moves the goal to the nearest tile without a blocking bit. This is
   what happens to a hold goal set **with** the unit's own tile given (a post that is the unit's own tile, or a held
   unit's hold goal at map load): the stored goal is the ring search's tile from then on, and the unit stands off it
   until something sends it there (traced: `human01-order15-range-700`, vectors below). A goal set with **no** tile
   given takes step 2 instead, with no ring search: the call for help's goal for a held unit is one ("Call for help").
5. If the region value (`movement/occupancy.md`) of the unit's tile equals that of the (adjusted) tile: the goal tile
   and kind are set, success.
6. Otherwise, for a unit that is not a sea unit: the goal tile is set, the **needs a ferry** mark is set, the goal
   land area becomes the tile's region value, the kind is set, and the result is **failure**. A sea unit fails with
   nothing changed.

**When a unit is created** (`core/unit-slots.md`, "Creating a unit": the map's units as the map loads, trained units
as they appear), after its first order is set and before the extra first-wait draw, its goal is cleared (kind none,
no tile, no marks, no assigned transport). Then, only for a unit of a computer player that is mobile and is neither
a worker (bit 8) nor a tanker (bit 9), it gets a first goal, first match (so buildings get none). An active rescue
player's map units get none (round 83, default, untraced): the map's units are created before that player becomes a
computer player, as the missing creation marks of its buildings show (`ai/worker-ai.md`, "Creation marks"). Round 84:
the rule stays an untraced default, and by the stated rules it changes no outcome in any shipped mission: every map
soldier of a shipped active rescue player is held (its goal is hold at its own tile either way, "Held units") and the
others are workers, which get no first goal anyway (`ERRATA-round84.md`, AI7). The first goal, first match:

| Unit | First goal |
| --- | --- |
| Transport (bit 10) | ferry, own tile |
| Sea unit (bit 3) | sea patrol, own tile |
| Air unit (bit 1) | roam, own tile |
| Any other | hold, at the post chosen below |

**The nearest gold store.** Wherever this file says "the player's gold store nearest a unit" (the post below, the
target pickers' reference store and the ferry's store fallback), "nearest" is measured from the **store's top-left
tile**: the larger of the column and row differences between that tile and the unit's tile, not the reference-tile
distance of "Behaviour". The first in list order wins a tie. There is **no alive test**: a store in its death
animation counts until it leaves the list (round 85; static gap-fill, erratum 1). The measure is traced for the target
pickers (round 86: `xhuman12-xhuman12-picker-flip-aigoals-1700`, vectors below); the post and the ferry's store
fallback use the same search (round 86 review; static study, backed by that trace: no committed trace has a post or
ferry search that the two measures rank differently). If the unit is itself a building (a picker's reference unit can
be), its tile is its reference tile as seen from the store's top-left tile (`orders/orders.md`, "Distance"), not its
own top-left tile (round 86 review; static gap-fill, untraced). The site-search centres of `ai/worker-ai.md` use the
same measure ("Keep-clear marks and site search").

**Choosing a land unit's post.** Let *store* be the player's gold store (bit 12) nearest the unit ("The nearest gold
store", just above), among those in the same region as the unit (any, for air and sea units; the player's units in
list order). Let *enemy* be the result of the enemy search below.

1. If there is no store, or no enemy, the post is the unit's own tile.
2. Otherwise walk from the store's top-left tile toward the enemy's tile, one tile per step, by the line rule of
   `ai/worker-ai.md` ("Keep-clear line"), while the current tile is inside the player's **base box** (below, as last
   rebuilt). The walk stops at the first tile outside the box; if the store's own tile is already outside, that tile
   is the result.
3. If that tile is outside the map (either coordinate below 0 or at least the map width), use instead a random tile
   near the unit: x = unit's x + (r mod 12) − 6, then y = unit's y + (r mod 12) − 6 (two draws, x first), each
   clamped to 0 … width − 1.
4. If the tile has a bit of the land blocking mask (0x09CE), a ring search around it, limit L = 24, looks for a tile
   with the land bit, no bit of that mask, and the same region value as the tile itself. If none is found, the post
   is the unit's own tile.
5. If the result's region value differs from the unit's tile's, the post is the unit's own tile.

Then the unit's goal is set to **hold** at the post.

**Held units.** When a map is loaded, a unit placed by the map's `UNIT` section whose 16-bit data value has a
non-zero low byte (the public PUD text calls this field "0 passive, 1 active"; gold mines and oil patches use it for
their amount instead, and are excluded, as are oil platforms, `game/setup.md`) gets the **held** mark. A held unit:

- is taken out of its player's army and unit-group counts (`ai/ai-scripts.md`);
- if it is mobile, can attack (bit 19) and is not a worker, gets goal **hold** at its own tile;
- is never sent in an attack wave, never picked as an escort, and answers a call for help only when close (below).

The held mark is set after the unit is created, so it replaces the first goal only where it sets hold. A map-placed
unit without the mark keeps the first goal it got when it was created. Finishing a building gives no goal.

**When a unit is converted** (Enhanced; `economy/tech-tree.md`, "Converting units": the ranger, berserker, paladin
and ogre-mage upgrades and the mission-start conversions of `game/victory.md`): after its type, hit points and mana
change, a unit that isn't held is treated as just created for its goal. Its goal is cleared (kind none, no tile, no
needs-a-ferry mark, no assigned transport); then, only for a computer player's unit that is mobile and neither a
worker nor a tanker, it gets a first goal from the table above. A land unit's post is chosen as above, from the
player's base box as last rebuilt (at a mission start no pass has built it yet, so it is still in its empty state, as
for "A land unit's post at map load"), so a line walk that ends off the map makes the 2 draws of step 3. A held unit keeps its goal (and its held mark). The unit's order, stored move goal and target are
untouched, so a unit that was walking on to an attack keeps walking. The units
are taken in the owner's unit-list order, so their draws come in that order, in the cycle and phase where the
conversion runs (an upgrade: when its research job finishes, `economy/build-train.md`; a mission start: at game
start, before the first start-up cycle). **Classic mode:** a conversion leaves the goal alone (static study).

Evidence: static study (`ERRATA-audit-game.md`, erratum 1); traced in round 90 for both branches (vectors AG1 and
AG2 below). The off-map post's 2 draws after a conversion aren't traced.

**Boarding.** When a unit goes inside a transport, its needs-a-ferry mark and assigned transport are cleared.

### The threat map

The map is cut into square **blocks** of 8 × 8 tiles: (width / 8) per row, rows from the top, numbered row by row.
Each block stores a **dominant owner** (a player 0 to 7, or none), a **terrain kind**, and two sums for each of the
16 players: a **land strength** and a **sea strength**.

**Terrain kind** is set when the game starts (after the scripts' opening run; the same step sets every player's slot
0x0C to 1 and its base-edge side counter to 0): count the block's tiles with the open-water bit (0x40) and the tiles
with the land bit (0x01). More land: kind **land**. More water: kind **water**. Equal (including both 0): kind
**mixed**.

**Rebuild** (start of pass A): all sums are cleared. Then for each block, for each of its 64 tiles, if a unit
occupies the tile on the ground layer (`movement/occupancy.md`; land units, ships and every footprint tile of a
building; air units never), that unit's strength in mode "land" is added to its owner's land strength and its
strength in mode "sea" to its owner's sea strength. A building therefore counts once per footprint tile.

**Strength of a unit** in a mode, first match:

| Unit | Strength |
| --- | --- |
| Owned by player 15, or gone | 0 |
| Worker (bit 8) | −2 |
| Gold store (bit 12) | −1 |
| Transport (bit 10) | −5 |
| Tanker (bit 9) | −2 |
| Mobile, its type can target land (can-target value 1; mode land) or sea (value 2; mode sea), and its weight w ≠ 0 | floor(HP × w / H), where HP is the unit's current hit points and H is the **maximum hit points of type 1** (the grunt; 1 if that is 0), for every unit |
| Mobile, any other | 0 |
| Anything else (buildings) | −1 |

**Weight** w of a type, computed once per run of the program from the default unit and animation data:

- D(t) is the **attack duration** of type t: walk its attack animation (animation 4, `core/animation-scripts.md`)
  from the start, following continue-at (opcode 3) jumps; add 1 for each step-done (opcode 0) and the argument of each
  wait (opcodes 1, 7, 8, 9, 12); stop after the second step-done. Other opcodes are skipped with their arguments.
- rate(t) = floor(10 × D(0) / D(t)), kept to 8 bits; 0 when D(t) = 0.
- For types below 0x3A: w(t) = 0 if rate(t) = 0; otherwise w(t) = floor(((B − 1) / 2 rounded toward 0, + P) ×
  rate(t), kept to 16 bits, / rate(1)), where B and P are the type's basic and piercing damage. Other types have no
  weight (their strength is the "any other" row).

**Dominant owner** of a block: for a land block, judged on land strengths; for a water block, on sea strengths; a mixed
block has none. Among players 0 to 7 in order, the one with the highest strength that is non-zero and greater than
−100 (strictly higher replaces the best); none if no player qualifies.

### Attack waves (pass A)

For each computer player p from 0 to 7 that has at least one unit, three waves are checked in order: land, sea, air.

| Wave | Trigger slot | Size slot | Repeat slot | Target picker | Units taken |
| --- | --- | --- | --- | --- | --- |
| Land | 0x09 | 0x0D | 0x0E | land | can attack (bit 19), mobile, neither air nor sea |
| Sea | 0x0A | 0x0F | 0x10 | sea | can attack, mobile, sea |
| Air | 0x0B | 0x11 | 0x12 | air | can attack, mobile, air |

In every wave the units taken also exclude workers (bit 8), tankers (bit 9) and units with flag bit 25 or 26.

If the trigger slot is non-zero, the following is done **repeat** times (slot value; 0 times if it is 0), and then
the trigger slot is set to 0:

1. **Target player:** among players 0 to 7 whose controller is human, the one with the most non-building units
   (the count of `game/victory.md`'s unit counts that excludes buildings; strictly more replaces the best, so the
   lowest number wins a tie). If no human player has more than 0, nothing happens in this round.
2. If the size slot is 0, nothing happens in this round.
3. The wave's **target picker** (below) gives a tile, or fails; on failure nothing happens in this round.
4. Walk p's units in list order. Each unit that passes the wave's filter, is not held, and whose goal kind is not
   already **attack**, is given **set goal (unit, attack, tile)**, whatever the result, and counts towards the size.
   The walk stops when the count reaches the size slot's value or the list ends.

Since units already attacking are skipped, each round takes the **next** units, so a wave sends up to size × repeat
units. Because the target is chosen the same way each round, every round of one wave goes to the same tile.

Units given an attack goal don't move yet: they move when pass B runs their goal, later in the same cycle.

### Target pickers

All three pick a tile in the **target player** T's area. "T's list" means T's units in list order.

**Reference unit** (used by all three): the first unit in T's list, call it F.
- Land picker: T's gold store nearest F ("The nearest gold store", above: from the store's top-left tile; round 86,
  traced) among those in F's region (any if F is air or sea), else the first building in T's list, else F.
- Sea and air pickers: T's gold store nearest F as above, else F. They **fail** if T has no units.

**Land picker** (never fails):

1. Among the blocks whose dominant owner is T and whose kind is land, choose the one **farthest** from the reference
   unit, measuring the distance from the point (x = the block's centre row index × 8 + 4, y = the block's centre
   column index × 8 + 4). Note the swap: the block's row gives the point's x and its column gives its y. Strictly
   farther replaces the best, and a block at distance 0 is never chosen. Blocks are visited in block order.
2. If no block qualifies, use the reference unit's own block.
3. The tile is (block column × 8 + 4, block row × 8 + 4).
4. Then find T's unit nearest that tile, skipping units that are gone, sea units, mobile air units that can attack,
   and oil platforms (bit 11). If one is found, the result is its tile; otherwise the tile from step 3.

**Sea picker:**

1. The first unit in T's list that is an oil platform (bit 11): its tile.
2. Otherwise the **last** shipyard (types 0x48, 0x49) in T's list: its tile.
3. Otherwise T's sea unit farthest from the reference unit (strictly farther replaces): its tile. The best distance
   starts at 0, so a ship at distance 0 from the reference unit is never chosen (round 85; static gap-fill,
   erratum 15); if no ship is farther than 0, step 4 applies.
4. Otherwise it fails.

**Air picker:**

1. T's unit of type 0x5C (gold mine) nearest the reference unit: its tile.
2. Otherwise the last gold store in T's list.
3. Otherwise T's unit farthest from the reference unit.
4. Otherwise the reference unit's tile.

(Type 0x5C units normally belong to the neutral player, so step 1 rarely matches; see Open questions.)

### The all-out attack

Started by the losses rule of `ai/worker-ai.md` ("The losses rule": at the kill, the player's buildings, the destroyed
one included, less its complete oil platforms, are fewer than 3; a platform under construction counts: round 83,
traced). For that player p, if it has units:

1. Target player: as in attack waves (the human player with the most non-building units, more than 0). If none,
   nothing happens.
2. The tile is the **land picker**'s result for that player.
3. For each unit in p's list, in order: if it is mobile and not gone, **set goal (unit, attack, tile)** (held units
   and units already attacking are not skipped). In addition, a **worker or tanker** is given a smart move
   (command 3) to the tile, with no target unit.

Units without workers' or tankers' flags move when pass B runs their new goal.

### The base box (pass B)

For each computer player p, its **base box** (min x, min y, max x, max y) is rebuilt at the start of pass B, kept as
signed 8-bit values:

1. min x = min y = the map width (as an 8-bit value), max x = max y = −1.
2. If p has no units, or p's first unit (list order) has no gold store in its region (as for the post, above), stop
   here: the box keeps these values, with no margins.
3. Otherwise, for each building of p (bit 5) in list order that is alive and is neither an oil platform nor a shore
   building (bit 16), using its top-left tile (x, y):
   - if x < min x, min x becomes x; **otherwise**, if x > max x, max x becomes x;
   - the same for y with min y and max y.

   Because of the "otherwise", a building that lowers a minimum never raises the maximum on that axis. In particular
   the first building only sets the minimums, so with a single building the maximums stay −1.
4. Then min x and min y go down by 5 (negative becomes 0), and max x and max y go up by 8 (a result that is not below
   the width, compared as signed 8-bit values, becomes width − 1).

Quirk: on a 128-wide map, the starting minimum is −128 as a signed value, so no building lowers it and the minimum
stays −128 before the margin (−133 kept to 8 bits is +123, which is not negative, so it stays 123). Likewise
max + 8 can wrap past 127. Test vectors below show both cases. Whether the maximums' clamp in step 4 applies on a
128-wide map is open (Open questions, "The base box clamp on a 128-wide map"; round 85).

### Running goals (pass B)

After the base boxes, every unit slot is visited in slot order (`core/unit-slots.md`). A unit is handled if its owner
is a computer player, it is not gone and its goal kind is not none. Casters (flag bit 17) first run the spell
step of `ai/caster-ai.md`; if that acts, the goal is skipped this pass. Then the goal's rule runs.

"Idle" below means the unit's order is 2 (`orders/orders.md`). "React range" is the type's react range (computer)
from the unit data, and the **leash** is floor(react range / 2) + 4.

**Hold (1).** Workers and tankers do nothing.

1. Look at the block containing the unit's tile. If its dominant owner is the unit's owner: if the unit is idle go
   to step 3, otherwise stop.
2. Otherwise compare strengths in that block, using sea strengths if the unit is a sea unit, land strengths
   otherwise: S = the unit's owner's, E = the dominant owner's. If 10 × S ≥ 4 × E and the unit is not idle, stop.
   (If the block has no dominant owner, E is not a defined value; see Open questions.)
3. If the distance from the goal tile to the unit is at least the leash: set goal (unit, hold, goal tile); on success
   give a smart move (command 3) to the goal tile. In both cases the unit forgets its attacker. The smart move goes to
   the goal tile **as given** to set goal, not to the tile set goal stores when its ring search moves it (traced:
   `xorc3-10000`, vectors below, and confirmed by static study). Set goal adjusts and stores its own tile; the tile the
   smart move gets isn't changed.

So a unit holding its post returns when it has wandered or chased past the leash, and gives up a fight early (even
while busy) when it is badly outmatched where it stands.

**Attack (2).** Unlike hold, the attack goal doesn't skip workers and tankers, so a worker given an attack goal by
the all-out attack gets command 4 here. At its next order-logic run in order 4 it makes its job choice
(`ai/worker-ai.md`, "When a job is chosen"), so it takes up a job instead of marching (round 83; traced:
`xhuman5-xhuman05-worker-all-out-goals-1100`, a peon given command 4 at the pass of n = 801 gets return goods at
n = 804). Nothing here clears its attack goal, so a later pass that finds it past the leash and not in an attacking
order gives it command 4 again, and its next order-logic run in order 4 makes a job choice again (round 84; traced:
`xhuman5-xhuman05-worker-all-out-goals-3000`, five passes from n = 2401 to 2951 give peon 1575 command 4 while it
mines, and a job follows 3 cycles after each takes over; vectors below). A pass gives nothing while the worker is in
order 25 or inside a building, since no pending command is written then (`orders/orders.md`). So such a worker keeps
working, with a short detour toward the goal tile each time it strays past the leash. Each build job the worker
takes writes the **site** into its goal tile and leaves the kind (`ai/worker-ai.md`, "The goal tile of a build job";
round 85, erratum 7, traced for a farm job: `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500`, vectors below). So
after a build job the passes measure the leash from that site, and after a pass's own set goal from the tile it
stored (its ring search can move it: (6, 19) became (5, 19) at the pass of n = 2401 in that trace).

1. If the distance from the goal tile to the unit is at least the leash, and the unit's order is not an attacking
   order (9, 11, 12, 15, 16 or 17): set goal (unit, attack, goal tile); on success give command 4 (move, attacking on
   the way) to the goal tile. Either way the unit forgets its attacker. Done.
2. Otherwise (the unit is within the leash, or its order is an attacking order): if the unit is not idle, nothing
   happens. If it is idle, run the **enemy search**. If it finds a unit, give command 12 against it. If not, **set
   goal (unit, roam, own tile)**. So the roam fallback needs both an idle unit and no enemy found.

So attackers march to the target tile, then fight there, hunting the nearest enemy anywhere each time they become
idle, and turn into roamers when no enemy is left.

**Roam (4).** If idle: pick x = unit's x + (r mod (width / 2)) − width / 4, then y the same way with the unit's y
(two draws, x first), each clamped to 0 … width − 1, and give command 4 to that tile.

**Sea patrol (6).** If idle:

1. The **anchor** is the unit's own tile, or, if the player has a tanker that isn't inside a building or platform
   (the first such tanker in its list), the tile of the depot nearest that tanker that accepts oil
   (`economy/harvest.md`), if there is one. Tankers inside are skipped, not taken and then rejected. If every tanker
   is inside (or there is none), the anchor is the unit's own tile: the patrol toward it then arrives at once, and
   the swap to the other end takes over with wait 3 (`orders/orders.md`, "Arriving and losing the target"). Whether a
   tanker in its sinking animation can be the anchor is open (Open questions, "A dying anchor tanker"; round 85).
2. The **far end** is the player's completed oil platform (bit 11) nearest the anchor; else the completed neutral
   oil patch (bit 21) nearest the anchor; else (anchor x − (r mod 5) − 2, anchor y − (r mod 5) − 2), two draws x first,
   clamped to the map.
3. Give a patrol command (command 5) toward the anchor, then make the unit's patrol ends the anchor and the far end.
   The unit's goal tile becomes the anchor. In Enhanced mode the patrol command remembers the anchor and the tile the
   unit stands on (`orders/orders.md`, "Resuming patrol and attack-move"), and setting the ends doesn't change that. So
   the first leg runs to the anchor and turns toward the far end, but a patrol broken off during its first leg and
   resumed later runs to the anchor and turns toward the tile the unit stood on when the patrol was given (traced:
   `xorc11-air-sea-3000`, vectors below).

So warships patrol between the oil supply and the nearest platform or oil patch.

**Base-edge patrol (5).** If idle: with the player's base box, mark the sides that exist: north if min y < width − 1,
east if max x > 0, south if max y > 0, west if min x < width − 1. The player keeps a side counter c (0 at game
start); read it, then store (c + 1) mod 4. Starting at side c (0 north, 1 east, 2 south, 3 west) and going round,
take the first side that exists. If none exists, set goal (unit, roam, own tile). Otherwise patrol along that side
of the box (each end clamped to the map):

| Side | From | To |
| --- | --- | --- |
| North | (min x, min y) | (max x, min y) |
| East | (max x, min y) | (max x, max y) |
| South | (min x, max y) | (max x, max y) |
| West | (min x, min y) | (min x, max y) |

The unit is given a patrol command toward "From", then its patrol ends are set to "From" and "To". As for the sea
patrol, the remembered patrol (Enhanced) keeps "From" and the unit's own tile at the command, not "To". No path that
gives a unit this goal has been found (see Open questions).

**Escort (7)** and **none (0):** nothing.

### Ferry (3)

A transport with goal **ferry** acts only if its order is 2 (idle) or 32 (docked; `orders/transports.md`). Let T be
the transport.

1. **Passengers:** count T's passengers. The **landing tile** G is the goal tile of the last passenger in slot order.
2. **Waiting units:** collect, in list order, the first 6 of the player's units that are alive and whose assigned
   transport is T. If there is any, the one nearest T is given a smart move (command 3) with T as the target unit
   (so it boards). Stop.
3. **Sail:** if T has at least one passenger:
   1. If the player's slot 0x0C is non-zero (it is set to 1 for every player when a game starts), look for a
      landing spot: draw r and let d0 = r mod 8. For directions d = d0, d0 + 1, … (mod 8), stopping when d comes back
      to d0, step from G one tile at a time in direction d (0 north, then clockwise: north-east, east, south-east,
      south, south-west, west, north-west) until leaving the map. The first tile with the same region value as G and
      the shore-water bit (0x02) becomes the new G, and the search ends.
   2. **Escorts:** for i = 0, 1, …: run this round if i is below **player i's** slot 0x0F (a quirk: the slot is read
      from player i, not from T's owner). Each round, take the player's unit nearest T that can attack, is an air or
      sea unit, is not held and whose goal is not escort; if there is none, stop. Set goal (that unit, escort, G) and
      give it command 4 with T as the target unit. There is no alive test: a dying unit can be chosen; set goal then
      fails for it, it gets no command, and as its goal is still not escort the next round picks it again, so it uses
      up the rounds (round 85; static note, erratum 13).
   3. Give T a smart move (command 3) to G with no target unit, then give T the **unload on arrival** mark
      (`orders/transports.md`), whatever G's terrain. So T unloads its passengers (order 29) when it arrives.
   Stop.
4. **Gather:** otherwise (no passengers, nobody assigned), walk the player's units in list order. A unit is a
   **candidate** if it is mobile, alive and has the needs-a-ferry mark, unless its assigned transport is alive, is a
   transport of the same owner, and that transport's target tile (`orders/transports.md`) has the same region value
   as the unit's tile. For each candidate:
   - its assigned transport becomes T;
   - the first candidate fixes the **trip area** (its goal land area); a candidate with the same goal land area as
     the trip area (the first one included) is given a smart move with T as the target unit, and counted;
   - when 6 have been counted, stop: units later in the list aren't visited and keep their assigned transport.
   Candidates bound for another area that come before that point still get T as their assigned transport, but no
   command.
5. If there was no candidate, T is given a smart move with its owner's gold store nearest T ("The nearest gold
   store") as the target unit (for a sea unit any gold store counts), if there is one.

Passengers keep their **attack** or **hold** goal while carried; after landing, pass B moves them on.

### Enemy search

Used by the post choice and the attack goal. Over players 0 to 7 in order, skipping every player that unit u's owner
counts as allied (the alliance table; a player counts as allied with itself), and over each such player's units in list
order, a unit is a candidate if all of these hold:

- u can attack its kind (the attack order's kind check, `orders/orders.md`, "Checks on an attack command");
- it isn't dying, gone or inside anything.

There is **no** fog test, no detection test, no air-against-air test and no minimum-range test, and no score. The result
is the nearest candidate to u, **anywhere on the map** (there is no search box); a candidate replaces the nearest so far
only if it is strictly nearer, so on a tie the first one visited wins. So the search can pick a unit that can't attack,
such as a zeppelin, over a farther one that can (traced: `xorc8-10000` and `xhuman7-10000`, vectors below).

### Call for help

When a unit V is notified that it was attacked by A (`combat/damage.md`, "Applying damage"):

1. If V already has an attacker that is alive, nothing more happens (the old attacker is kept). An attacker that is gone
   is forgotten first. An attacker that is alive but not detected by V's owner (a submarine, for example) stays recorded
   through V's order logic (`orders/orders.md`, "Terms"), so a second hit by it makes no new call (traced:
   `xhuman7-10000`, `orders/orders.md` vectors).
2. Nothing happens if V's owner counts A's owner as allied, or V is gone.
3. If V's owner's controller is human and A is a building, nothing happens (the attacker is not recorded).
4. A becomes V's attacker.
5. **Submarine call:** if V's owner is a computer player and the **attacker A** is a submarine (flag bit 6), V's
   owner's unit nearest V that is an air unit or a submarine, has no attacker and is not V is given command 4 with V as
   the target unit. Dying units aren't skipped (round 85; static note, erratum 14). That unit records V as its
   target, and its order 4 ends once V is dying (`orders/orders.md`, "Commands"); a computer player's command 4 keeps
   its remembered patrol ("Resuming patrol and attack-move").
6. **Neighbours:** the help radius R is 13 for a computer player's V; for others, 4 if V is a sea unit, else 2. The box
   runs from V's tile − R to V's far edge (x + width − 1 + R, y + height − 1 + R), clamped to the map. A unit S of V's
   owner is in the box when its **top-left tile's** column is within the box's columns and its top-left tile's row is
   from **3 rows above** the box's top row (at least 0) to the box's bottom row. Only the top-left tile is tested: a
   unit or building whose footprint overlaps the box but whose top-left tile is outside isn't in it. (The 3 extra rows
   come from the acquisition search's vertical-order lookup, which starts 3 rows higher; `orders/orders.md`, "Vertical
   order".) Each S in the box answers if all hold:
   - S is not dying or dead, and its order is 2 (idle) or 7 (follow and guard) with no order waiting to follow it;
   - S's type is not a worker or tanker, has neither flag bit 25 nor 26, can attack (bit 19), and is not a tower
     (bit 20);
   - S has no attacker of its own, S is not V, and S's acquisition score for A is non-zero. Only the **can-target** part
     of the score counts here (whether S can attack that kind of unit): it doesn't test whether A is dying or whether
     S's owner sees it. So neighbours answer an attacker that is already dying, for example a thrower whose missile
     lands after its death; their command then fails its target check and leaves them a pending idle
     (`orders/orders.md`, "Commands forget a stale attacker"; traced: `xhuman10-spells-700`). For S's own record, an
     attacker that is **dying** doesn't count as one: S answers while the unit it records is still in its death
     animation (`movement/movement.md` keeps that record when another unit made the kill).
   For a computer player, a **held** S answers only if its distance to V is at most 4, and its goal is first set to
   **attack** with no tile given ("Setting a goal", step 2): the goal tile is S's own tile as it stands, with no ring
   search and no region test, so a held S that later chases and is sent back returns to that very tile (traced:
   `xorc12-spells-1500`, vectors below). Every answering S is given command 12 against A.

### Building think interval

The training and research logic of a computer player's building runs from its resting order (33); the interval comes
from the script's second list. See `ai/training-research.md`.

## Data

### Script slots used here

| Slot | Use |
| --- | --- |
| 0x09, 0x0A, 0x0B | Land, sea, air wave trigger; cleared by pass A after the wave |
| 0x0C | Ferry landing-spot search on/off. Set to 1 for players 0 to 7 when a game starts, after the scripts' opening run (`ai/ai-scripts.md`, "Game start"), so an opening write by the script is overwritten; later script writes stick |
| 0x0D, 0x0E | Land wave size, repeat count |
| 0x0F, 0x10 | Sea wave size, repeat count (0x0F is also read by the ferry escort quirk) |
| 0x11, 0x12 | Air wave size, repeat count |

### Threat map block

| Item | Size |
| --- | --- |
| Block | 8 × 8 tiles; (width / 8)² blocks |
| Per block | dominant owner (player 0 to 7 or none), terrain kind, 16 × (land strength, sea strength), signed 16-bit |

### Weights from the default data

Computed with D(0) = 26 (footman); rate(1) = 10.

| Types | D | rate | B, P | w |
| --- | --- | --- | --- | --- |
| 0x00, 0x01 footman, grunt | 26 | 10 | 6, 3 | 5 |
| 0x02, 0x03 peasant, peon | 26 | 10 | 3, 2 | 3 (workers use −2 anyway) |
| 0x04, 0x05 ballista, catapult | 201 | 1 | 80, 0 | 3 |
| 0x06, 0x07 knight, ogre | 26 | 10 | 8, 4 | 7 |
| 0x08, 0x09 archer, axe thrower | 66 | 3 | 3, 6 | 2 |
| 0x0A, 0x0B mage, death knight | 41 | 6 | 0, 9 | 5 |
| 0x0C, 0x0D paladin, ogre-mage | 26 | 10 | 8, 4 | 7 |
| 0x0E, 0x0F demolition squad, sapper | 26 | 10 | 4, 2 | 3 |
| 0x12, 0x13 ranger, berserker | 66 | 3 | 3, 6 | 2 |
| 0x1E, 0x1F destroyers | 121 | 2 | 35, 0 | 3 |
| 0x20, 0x21 battleship, juggernaught | 231 | 1 | 130, 0 | 6 |
| 0x26, 0x27 submarine, turtle | 101 | 2 | 50, 0 | 4 |
| 0x28, 0x29 flying machine, zeppelin | 2 | 130 | 0, 0 | 0 |
| 0x2A, 0x2B gryphon rider, dragon | 215 | 1 | 0, 16 | 1 |

Grunt maximum HP (H) in the default data: 60.

### Fixed values

| Item | Value |
| --- | --- |
| Pass period | 50 cycles; first run in the first start-up cycle, then at cycle counter 49, 99, … |
| Dominant owner floor | strength must be > −100 and ≠ 0 |
| Hold give-up ratio | stays only while 10 × own ≥ 4 × dominant |
| Leash | floor(react range (computer) / 2) + 4 |
| Base box margins | −5 (min), +8 (max) |
| Post random offset | (r mod 12) − 6 on each axis |
| Roam offset | (r mod (width / 2)) − width / 4 on each axis |
| Sea patrol fallback | anchor − (r mod 5) − 2 on each axis |
| Goal tile ring search | limit 16 (attack, hold), 24 (post) |
| Ferry load | up to 6 units per trip |
| Help radius | 13 (computer), 4 (sea unit), 2 (other) |
| Held unit answer distance | 4 |
| Attacking orders | 9, 11, 12, 15, 16, 17 |

## Test vectors

Unless stated, the map is 64 × 64, players 0 (human) and 1 (computer, land-attack script), default unit data.

| Input | Expected |
| --- | --- |
| New game | Passes A and B run in the first start-up cycle, then in the normal cycles ending with the counter at 49, 99, 149, …; both in the same cycle, A first |
| Human04 (oracle trace and research-side command log), the orc transport (type 0x1D, slot 1543, `UNIT` data value 0) at (68, 88), computer player 0, no passengers, nobody needs a ferry | Goal ferry from creation. Pass B in the first start-up cycle (trace record n = 1) runs "Ferry" step 5: a smart move with the great hall (slot 1550, top-left (81, 84)) as the target unit. Pass B commands it again at n = 51, 101, 151, … (counter 49, 99, 149, …) |
| Human04, the orc destroyers (type 0x1F, `UNIT` data value 1) | Held. They can attack, so their goal is hold at their own tile, and pass B gives them no command |
| Block with a computer grunt (60 HP) and a human footman at 30 HP, land block | Land strengths: player 1 = 5, player 0 = floor(30 × 5 / 60) = 2. Dominant owner: player 1 |
| Land block with only player 0's town hall (4 × 4 footprint inside the block) and 2 peasants | Player 0 land strength = 16 × (−1) + 2 × (−2) = −20; non-zero and > −100, so player 0 is dominant |
| Land block containing only player 2's farm (−4) and player 3's farm (−4) | Equal: player 2 (first) is dominant |
| Water block | Judged on sea strengths: a footman (can target land only) adds 0 to sea strength |
| Mixed block (32 land, 32 water tiles) | No dominant owner |
| Knight at full HP (90) | Strength floor(90 × 7 / 60) = 10 in land mode |
| Gryphon rider over a block | Adds nothing (air units are not on the ground layer) |
| Slot 0x09 = 1, 0x0D = 4, 0x0E = 2; computer has 10 grunts, none attacking | Two rounds: first 4 grunts in list order get goal attack at the land picker's tile, then the next 4; slot 0x09 becomes 0 |
| Same, but 3 of the grunts are held | They are skipped; the 8 others are sent |
| Slot 0x09 = 1 and every human player has only buildings | No target: nothing sent, slot 0x09 still becomes 0 |
| Human players 0 and 2 each have 7 non-building units | Target is player 0 |
| Land picker: target owns blocks (row 1, column 6) and (row 6, column 1), reference unit at (10, 50) | Points tested are (x = 12, y = 52) and (x = 52, y = 12); the first is at distance 2, the second at 42, so block (row 6, column 1) wins: tile (12, 52), then the target's nearest qualifying unit to it |
| Sea picker: target has an oil platform | The first oil platform's tile, even if it has shipyards |
| Hold: grunt idle, 10 tiles from its post, react range (computer) 6 | Leash 7: goal re-set, smart move to the post, attacker forgotten |
| Hold: grunt fighting (order 12) in a block where player 1 has 5 and player 0 (dominant) has 13 | 10 × 5 = 50 < 52: if beyond the leash, it is sent back to its post even while fighting |
| Hold: same with player 1 at 6 | 60 ≥ 52 and not idle: no change |
| Attack: grunt at its goal tile, idle, enemy footman 40 tiles away | Command 12 against the footman |
| Attack: idle, no enemy left | Goal becomes roam at its own tile |
| Attack: in order 12, 20 tiles from goal tile | No change (attacking order, and not idle) |
| Attack: in order 4 (moving), 2 tiles from its goal tile, leash 7 | No change (within the leash and not idle); it doesn't become a roamer |
| Computer destroyer V hit by a human submarine; the computer has an idle gryphon rider 30 tiles away and a busy submarine with an attacker | The gryphon rider gets command 4 with V as the target; then the usual neighbours within 13 tiles answer |
| Computer submarine V hit by a human destroyer | No submarine call (the attacker is not a submarine) |
| Ferry sails, landing spot found (G is shore water) | T gets a smart move to G and the unload-on-arrival mark; when it arrives it takes order 29 and unloads |
| Roam on a 64 map, unit at (10, 10), draws 5 then 40 | x = 10 + 5 − 16 = −1 → 0; y = 10 + 8 − 16 = 2 (40 mod 32 = 8); command 4 to (0, 2) |
| Unit placed by the map with data value 1, a grunt | Held: not in the army count, goal hold at its tile, never sent in waves |
| Computer grunt trained, no gold store in its region | Goal hold at its own tile |
| Base box: buildings at (10, 12) and then (20, 25) in list order | min x 5, min y 7, max x 28, max y 33 |
| Base box: buildings at (20, 25) and then (10, 12) | Each building only lowers the minimums: min x 5, min y 7, max x 7, max y 7 (list order matters) |
| Base box: a single building at (10, 12) | min x 5, min y 7, max x 7, max y 7 (the maximums were never raised from −1) |
| Base box: computer player with no gold store | min x = min y = width, max x = max y = −1, no margins; no side of the base-edge patrol exists, so such a unit's goal becomes roam |
| Base box with buildings near the right edge of a 64 map, max x 60 | 68 is not below 64: max x = 63 |
| 128-wide map, any buildings | min x, min y start at −128 and stay; after the margin they are 123 |
| Transport idle with 2 passengers whose goal tiles are A then B (slot order) | G = B; the landing-spot search runs from B (slot 0x0C is 1 by default) |
| Transport idle, no passengers, 8 grunts needing a ferry to the same area | The first 6 in list order are assigned to it and told to board; the walk stops there, so the other 2 keep their old assignment (none). They are gathered on a later run, once the transport is idle with no passengers and nobody assigned |
| Transport idle, no passengers; in list order: a grunt bound for area B, then 7 grunts bound for area A | The first grunt fixes the trip area B: it is assigned and told to board (count 1). All 7 area-A grunts are assigned with no command; the walk goes on to the end of the list, since only 1 was counted |
| Computer footman V hit by an enemy archer; 3 idle computer grunts within 13 tiles | All 3 get command 12 against the archer |
| Human footman V hit; an idle human footman 3 tiles away | Outside R = 2: it doesn't answer (the acquisition search may still react) |
| Human unit hit by a guard tower | The tower is not recorded as its attacker; no call for help |
| `orc02-assault-3000` (oracle trace), n = 969: grunt 1576 (owner 0) at (59, 19), attacking 1587, is V (radius 2: box rows 17 to 21); idle grunt 1568 (owner 0) at (58, 16) | 1568 is in the box (row 16 ≥ 17 − 3) and answers: target 1587 at n = 969; order 12 takes over at n = 973 (animation 4, wait 3) |
| Same trace: 1568's recorded attacker is footman 1597, dying from n = 879 (killed by grunt 1578) until it becomes a body at n = 985 | 1568 still answers at n = 969: a dying attacker doesn't count as "an attacker of its own" |
| Human player's V at (10, 10), 1 × 1 (radius 2: box columns 8 to 12, rows 8 to 12); idle footman of V's owner at (12, 5) | Answers (row 5 ≥ 8 − 3) |
| Same, footman at (12, 4) | Doesn't answer |
| Draws | Pass A makes none. Pass B draws only in roam (2), sea patrol fallback (2), ferry landing spot (1) and the caster spell step's random search (1 to 10 per random try; `ai/caster-ai.md`). The post choice's off-map case (2) draws during creation, not in pass B |

### Sea patrol (oracle traces)

| Input | Expected |
| --- | --- |
| `xorc7-7000`, n = 953: computer destroyer 1423 (type 0x1E, owner 2) idle at (6, 22); tanker 1587 outside (order 23), tanker 1594 inside platform 1590 | Sea patrol: order 5, wait 3 |
| `xorc7-7000`, n = 1002: 1423 idle again at (6, 22); tankers 1587 and 1594 both inside platform 1590 (order 26) | Patrol command toward (6, 22), its own tile: order 5, wait 3. n = 1005: order 5, wait 3 (the swap to the far end); n = 1008: animation 3, it starts walking |
| `human10-10000`, the first tanker in the list taken and then rejected because it is inside (wrong) | Differs from the trace at n = 6907 |
| `human10-10000`, n = 6956: computer destroyer 1483 (type 0x1F) at (24, 8) | Order 5, wait 3; n = 6959: tile (26, 6), facing 1, animation 3 |
| `xorc11-air-sea-3000` (Enhanced): computer battleship 1539 (type 0x20, owner 6) at (6, 24) gets the sea patrol at n = 3 (anchor (21, 34), far end oil platform 1556 at (5, 19)); order 12 from n = 112; the patrol resumes at n = 1145 toward (21, 34) and turns at (20, 34) at n = 1413 | Goal (6, 24), not the platform: at n = 1643 it steps west from (12, 28) to (10, 28) |
| Enhanced: computer warship at (6, 24) idle; sea patrol with anchor (21, 34) and far end (5, 19); it fights during the first leg, then resumes (static study) | The resumed patrol runs to (21, 34) and turns toward (6, 24); without the fight the first leg turns toward (5, 19) |
| `human01-order15-range-700` (oracle trace): computer grunt 1592 (owner 0) starts at (29, 14), its hold goal set at its own tile; it chases a target to (21, 7), where the target is lost at n = 401 | The ring search moved the stored goal to (28, 14) when the goal was set. At n = 403, past its leash, it is sent back (order 3, wait 3) to (28, 14), not (29, 14): there at n = 550, idle at n = 566 (wait 3, facing 3) |
| `xorc12-spells-1500` (oracle trace, Enhanced): computer footman 1504 (type 0x00, owner 2, held) at (93, 26) answers a call for help at n = 492 (order 12 on Teron Gorefiend 1583, type 0x15, owner 5), chases to (101, 22), goes idle at n = 641 and is sent back at n = 654 (order 4, wait 3) | Goal (93, 26), its own tile when it answered, not a ring-search tile: at n = 673 it steps to (99, 23), facing 6 |
| A computer player's held grunt standing at (10, 10) answers a call for help (static study) | Goal kind attack, tile (10, 10) exactly; no ring search |
| `xorc3-10000` (oracle trace): computer grunt 1455 (type 0x01, owner 3), created at (43, 11) at n = 9443 with its hold goal at (34, 9); at n = 9454 peon 1464 stands on (34, 9) | Past its leash, at n = 9454 it gets order 3 (wait 3) and walks to (34, 9) itself, the tile as given, though set goal's ring search would move the stored goal off the peon's tile; idle there at n = 9601 (wait 3) |
| `xorc8-10000` (oracle trace, Enhanced), n = 6051: computer gryphon rider 1550 (type 0x2A, owner 2) idle at (10, 120); goblin zeppelin 1400 (type 0x29, owner 5) at (8, 120) is the nearest enemy; troll destroyer 1386 (type 0x1F, owner 5) is far away, at (106, 124) | Order 12 on 1400, animation 4, wait 3 (the zeppelin isn't skipped for being an air unit that can't attack) |
| `xhuman7-10000` (oracle trace, Enhanced), n = 5501: computer dragon 1415 (type 0x2B, owner 5), idle | Order 2, wait 4, no pending chase (a search with the acquisition skip rules would give a pending chase on elven destroyer 1425, type 0x1E, owner 1) |
| `xhuman5-xhuman05-worker-all-out-goals-3000` (oracle trace, Enhanced; round 84): computer peon 1575 (owner 2) with goal attack from the all-out attack at n = 754, mining at gold mine 1573 near (5, 30) | Command 4 (pending 4) at the passes of n = 2401 (at (5, 26), order 23), 2451, 2651 (idle), 2701 (order 24) and 2951; order 4 takes over at n = 2407, 2461, 2661, 2711, 2958, and a job (harvest or return goods) 3 cycles later each time. No command at n = 2751 (order 25) or while it is inside. The command's stored goal is (6, 20) at n = 2401 and (5, 19) from n = 2451 on; the passes measure from the farm site (6, 19), then from (5, 19) (round 85, erratum 7: next table) |

### Round-85 vectors

| Input | Expected |
| --- | --- |
| `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500` (oracle trace, Enhanced; round 85, erratum 7): computer peon 1575 (owner 2), attack goal (9, 20) from n = 754, takes a farm job at n = 1004 | AI goal kind 2, tile (6, 19), extra 59 from n = 1004 to the pass of n = 2401, which finds it at (5, 26), 7 tiles away, gives command 4 and stores (5, 19) |
| Sea picker: T has no oil platform, shipyard or gold store, and its only unit is a ship, which is also the reference unit (static, erratum 15) | Distance 0 never qualifies: the picker fails and the round sends nobody |
| A computer archer is created while the nearest enemy, a gryphon rider, hovers over the gold store's top-left tile, and the base box reaches the map's east edge (static, erratum 4) | The walk steps east one tile at a time, leaves the map, and the post is the random tile of step 3 (2 draws) |

### Round-86 vectors

| Input | Expected |
| --- | --- |
| `xhuman12-xhuman12-picker-flip-aigoals-1700` (oracle trace, Enhanced; round 86, erratum 1): computer player 2's land wave targets player 1, whose gold stores are castle 1443 at (42, 61) and town hall 1396 at (82, 81), both 4 × 4, in one region; the reference unit is footman 1353 at (63, 80) | Top-left distances 21 (castle) and 19 (town hall), so the town hall is the reference store (the reference-tile distances, 18 and 19, would pick the castle). The land picker's tile is (37, 5), stored as (36, 5) for units 1500, 1518 and 1521 at the passes of n = 501, 551 and 601 |
| Same trace, n = 301: footman 1353 at (62, 76) | Top-left distances tie at 20; the town hall wins (the first in list order in our simulation; the trace doesn't record list order). Stored goal (36, 5) for unit 1473 (the reference-tile distance, 17 against 20, would give the castle's (13, 66)) |
| Same trace, n = 351 (control): footman 1353 at (60, 77) | Both measures pick the castle (top-left 18 against 22): stored goal (13, 66) for units 1480 and 1489 |
| A computer land unit created at (20, 20) with its player's 4 × 4 town halls at (10, 10) and (28, 20), in that list order, both in its region (static, round 86 review) | Top-left distances 10 and 8: the post walk starts from the hall at (28, 20) (the reference-tile distances, 7 and 8, would give the hall at (10, 10)) |
| The target's newest unit F is a 2 × 2 farm at (30, 10); its gold stores, 4 × 4, have top-left tiles (20, 10) and (40, 10), in that list order, both in F's region (static gap-fill, round 86 review) | Seen from (20, 10) the farm's reference tile is (30, 10): distance 10; from (40, 10) it is (31, 10): distance 9. The store at (40, 10) is the reference store (from the farm's top-left tile both are 10 and (20, 10) would win the tie) |

### Round-90 vectors (conversion, traced)

| Input | Expected |
| --- | --- |
| **AG1.** `xhuman9-aigoals-goals-8630-8830` (oracle trace, Enhanced; `EXTRAS=aigoals,goals`): computer player 5's axe throwers 1374, 1379 and 1382 (type 0x09), trained in play and so not held, become berserkers (type 0x13) at n = 8730 when the upgrade finishes | Goal hold at (60, 9), (62, 9) and (62, 9) at n = 8630; hold at (59, 9) for all three from n = 8730 (the post a new unit would get). Order 2 and the stored move goal ((60, 9), (62, 9), (62, 9)) are unchanged |
| **AG2.** `orc13-aigoals-goals-17900-18200` (oracle trace, Enhanced): computer player 3's 20 knights (type 0x06), all placed by the map and held, become paladins (type 0x0C) at n = 17997 | Every one keeps its goal: slot 1470 hold at (28, 115), slot 1475 hold at (66, 110), slot 1476 hold at (68, 110), before and after |
| **AG3.** `xorc9-aigoals-goals-0` (oracle trace, Enhanced; entry 45, mode Q, setup record): computer player 1's six knights, held, are converted by the mission start | Six paladins, each with the hold goal it got at map load (slot 1511 at (40, 57): hold at (39, 57)) |
| **AG4.** Enhanced; computer player 1's knight, not held, with goal attack at (40, 40) and order 4 toward it, when the paladin upgrade finishes (static study) | Goal hold at its post; its order 4 and target tile are unchanged |

## Open questions

- **No dominant owner in hold.** When the unit's block has no dominant owner, the original reads E from outside the
  block's own record (it looks up "player none"), which is not a meaningful value. Default: treat E as 0 (the unit
  always passes the strength test).
- **Base-edge patrol.** No code path was found that gives a unit goal kind 5. Default: implement it as described, but
  nothing assigns it.
- **Air picker step 1.** The air picker looks for gold mines among the target player's own units, which normally
  never own one. Default: implement literally.
- **Land picker coordinate swap.** The block's row index feeds the point's x and its column its y. Default: keep the
  swap; it only changes which far block is chosen.
- **Strength denominator.** Every unit's strength divides by the grunt's maximum HP, not its own. Default: keep it.
- **A worker's attack goal after its job choice** (round 83): **closed in round 84** (AI6): the goal stays; passes
  that find the worker past its leash give command 4 again (`xhuman5-xhuman05-worker-all-out-goals-3000`).
- **The goal tile after a worker's farm job** (round 84, AI6; reworded by the round-84 review): **closed in round 85**
  (AI10, erratum 7, traced: `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500`). A farm job writes its
  site into the worker's goal tile (traced) and the town hall and build-list jobs do the same (static study); the
  kind is unchanged (`ai/worker-ai.md`, "The goal tile of a build job"). In that
  trace peon 1575's goal tile is (9, 20) from the all-out attack (n = 754), (6, 19) from its farm job at n = 1004
  (still so at n = 1080 and 2291), and (5, 19) after the pass of n = 2401, whose set goal moved it by the ring search,
  as the round-84 review inferred.
- **Units converted during a mission.** Answered for type conversions (B-040 game audit erratum 1, traced in round
  90): "When a unit is converted". Still open for a campaign event that changes units' **owner**: static study reads
  the same goal step there (non-held units, Enhanced). Not traced. Default: units that change owner keep their goal.
- **Escort quirk.** Escort rounds read slot 0x0F of player 0, 1, 2, … in turn rather than the transport owner's.
  Default: implement literally.
- **Sea escorts.** A sea unit whose goal tile can't be set (different water area) keeps its old goal, so the same
  ship can be chosen in every escort round. Default: implement literally.
- **Line walk when the store and the enemy share a tile.** The line rule then has a zero-length major axis. Answered
  by static study (round 85; static gap-fill, erratum 4; untraced): a zero difference is taken as an x-major
  line with non-negative steps, so the walk still steps, one tile east (+1, 0) at a time, until it leaves the base box,
  and may end off the map with the 2 draws of step 3. (The earlier default, the store's tile, is replaced.) Trace
  request: none cheap (an enemy flyer exactly over the store's top-left tile when a land unit that can hit air is
  created).
- **The base box clamp on a 128-wide map** (round 85; held, erratum 5). Static study reads step 4's clamp as
  comparing max + 8, read as a signed 8-bit value, with the width as a plain number: on maps narrower than 128 that is
  the stated rule, but on a 128-wide map the clamp never applies, so max + 8 wraps for a maximum of 120 or more, and
  with the minimum at 123 the box is empty unless the largest building x (or y) is 115 to 119. That contradicts step 4
  as stated (both sides signed 8-bit, so the clamp always applies there and the maximums are 127). Replaying all 192
  committed traces, `orc8` included (a 128-wide map with a computer hall at (123, 97)), both readings give the same
  records. Default: step 4 as stated. The readings differ only for a gold store whose top-left tile is at 123 or more on
  both axes; no committed trace has one (round 86, AA7: none has both coordinates at 115 or more), so the
  question is unreachable in shipped data and has no trace request.
- **A dying anchor tanker** (round 85; held, erratum 9). Static study reads the sea patrol's anchor tanker as the
  first tanker that is **present** (not dying and not inside), so a tanker in its sinking animation is skipped. That
  contradicts step 1 as stated ("isn't inside a building or platform"). Not traced: in the round-85 trials
  (`ERRATA-round85.md`, AA5) a computer tanker was sunk, but no warship with goal 6 was idle at a pass during the
  sinking. Default: step 1 as stated. Request AA5b (a killer the warships don't chase) was tried in round 86 (sim
  replays of `xorc8`) and skipped: every killer that can hit the tanker is chased by the depot's warships, and the
  tanker stands still away from the depot only at a guarded platform (`ERRATA-round86.md`, AA5b). Off the queue;
  parked until the tracer can turn a cheat off (invulnerability off after the kill, so the chasers can win and go idle).
- **The nearest gold store's measure** (round 85, erratum 1): **closed in round 86.** The target pickers are traced
  to the store's top-left tile (`xhuman12-xhuman12-picker-flip-aigoals-1700`, `ERRATA-round86.md`, AA1b); by review,
  the post and the ferry's store fallback use the same measure, since static study reads one shared search for all
  three and no committed trace ranks a post or ferry search differently ("The nearest gold store"). A trace of a post
  or a ferry fallback that follows the reference-tile distance would reopen it.
- **A land unit's post at map load.** The first goal of a computer player's non-held land unit is chosen while the
  map is still loading, before any base box has been built, and where its draws fall relative to the creation's
  facing draw isn't confirmed. Neither traced map has such a unit. Default: the base box is still in its empty state
  (as for a player with no gold store), so a store found then is already outside it and its own tile is the walk's
  result; gold stores the map lists later don't count yet. Any draws come after the facing draw.
