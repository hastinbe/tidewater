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
in the order stated winning a tie.

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
5. If the region value (`movement/occupancy.md`) of the unit's tile equals that of the (adjusted) tile: the goal tile
   and kind are set, success.
6. Otherwise, for a unit that is not a sea unit: the goal tile is set, the **needs a ferry** mark is set, the goal
   land area becomes the tile's region value, the kind is set, and the result is **failure**. A sea unit fails with
   nothing changed.

**When a unit is created** (`core/unit-slots.md`, "Creating a unit": the map's units as the map loads, trained units
as they appear), after its first order is set and before the extra first-wait draw, its goal is cleared (kind none,
no tile, no marks, no assigned transport). Then, only for a unit of a computer player that is mobile and is neither
a worker (bit 8) nor a tanker (bit 9), it gets a first goal, first match (so buildings get none):

| Unit | First goal |
| --- | --- |
| Transport (bit 10) | ferry, own tile |
| Sea unit (bit 3) | sea patrol, own tile |
| Air unit (bit 1) | roam, own tile |
| Any other | hold, at the post chosen below |

**Choosing a land unit's post.** Let *store* be the player's gold store (bit 12) nearest the unit, among those in the
same region as the unit (any, for air and sea units; the player's units in list order). Let *enemy* be the result of
the enemy search below.

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
their amount instead, and are excluded) gets the **held** mark. A held unit:

- is taken out of its player's army and unit-group counts (`ai/ai-scripts.md`);
- if it is mobile, can attack (bit 19) and is not a worker, gets goal **hold** at its own tile;
- is never sent in an attack wave, never picked as an escort, and answers a call for help only when close (below).

The held mark is set after the unit is created, so it replaces the first goal only where it sets hold. A map-placed
unit without the mark keeps the first goal it got when it was created. Finishing a building gives no goal.

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
- Land picker: T's gold store nearest F among those in F's region (any if F is air or sea), else the first building
  in T's list, else F.
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
3. Otherwise T's sea unit farthest from the reference unit (strictly farther replaces): its tile.
4. Otherwise it fails.

**Air picker:**

1. T's unit of type 0x5C (gold mine) nearest the reference unit: its tile.
2. Otherwise the last gold store in T's list.
3. Otherwise T's unit farthest from the reference unit.
4. Otherwise the reference unit's tile.

(Type 0x5C units normally belong to the neutral player, so step 1 rarely matches; see Open questions.)

### The all-out attack

Started by the losses rule of `ai/worker-ai.md` (a computer player that drops below 3 buildings other than oil
platforms). For that player p, if it has units:

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
max + 8 can wrap past 127. Test vectors below show both cases.

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
   give a smart move (command 3) to the goal tile. In both cases the unit forgets its attacker.

So a unit holding its post returns when it has wandered or chased past the leash, and gives up a fight early (even
while busy) when it is badly outmatched where it stands.

**Attack (2).**

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
   the swap to the other end takes over with wait 3 (`orders/orders.md`, "Arriving and losing the target").
2. The **far end** is the player's completed oil platform (bit 11) nearest the anchor; else the completed neutral
   oil patch (bit 21) nearest the anchor; else (anchor x − (r mod 5) − 2, anchor y − (r mod 5) − 2), two draws x first,
   clamped to the map.
3. Give a patrol command (command 5) toward the anchor, then make the unit's patrol ends the anchor and the far end.
   The unit's goal tile becomes the anchor.

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

The unit is given a patrol command toward "From", then its patrol ends are set to "From" and "To". No path that gives
a unit this goal has been found (see Open questions).

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
      give it command 4 with T as the target unit.
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
5. If there was no candidate, T is given a smart move with its owner's gold store nearest T as the target unit (for
   a sea unit any gold store counts), if there is one.

Passengers keep their **attack** or **hold** goal while carried; after landing, pass B moves them on.

### Enemy search

Used by the post choice and the attack goal. Over players 0 to 7 in order, skipping every player that unit u's owner
counts as allied (the alliance table; a player counts as allied with itself), and over each such player's units in
list order: a unit is a candidate if it is alive and u's acquisition score for it would be non-zero (the "skipped
if" rules of `orders/orders.md`, "Automatic target acquisition": detection, fog, what u can attack, minimum range).
The result is the nearest candidate to u, **anywhere on the map** (there is no search box).

### Call for help

When a unit V is notified that it was attacked by A (`combat/damage.md`, "Applying damage"):

1. If V already has an attacker that is alive, nothing more happens (the old attacker is kept). An attacker that is
   gone is forgotten first.
2. Nothing happens if V's owner counts A's owner as allied, or V is gone.
3. If V's owner's controller is human and A is a building, nothing happens (the attacker is not recorded).
4. A becomes V's attacker.
5. **Submarine call:** if V's owner is a computer player and the **attacker A** is a submarine (flag bit 6), V's
   owner's unit nearest V that is an air unit or a submarine, has no attacker and is not V is given command 4 with V as
   the target
   unit.
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
   - S has no attacker of its own, S is not V, and S's acquisition score for A is non-zero. A recorded attacker that is
     **dying** doesn't count here: S answers while the unit it records is still in its death animation
     (`movement/movement.md` keeps that record when another unit made the kill).
   For a computer player, a **held** S answers only if its distance to V is at most 4, and its goal is first set to
   **attack** at its own tile. Every answering S is given command 12 against A.

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
- **Units converted during a mission.** A campaign event that changes units' owner gives non-held units a first goal
  under a mission flag. Not traced in detail. Default: units that change owner keep their goal.
- **Escort quirk.** Escort rounds read slot 0x0F of player 0, 1, 2, … in turn rather than the transport owner's.
  Default: implement literally.
- **Sea escorts.** A sea unit whose goal tile can't be set (different water area) keeps its old goal, so the same
  ship can be chosen in every escort round. Default: implement literally.
- **Line walk when the store and the enemy share a tile.** The line rule then has a zero-length major axis. Default:
  the walk stays on the store's tile, which is inside the box, so treat it as reaching the box edge without moving:
  use the store's tile.
- **A land unit's post at map load.** The first goal of a computer player's non-held land unit is chosen while the
  map is still loading, before any base box has been built, and where its draws fall relative to the creation's
  facing draw isn't confirmed. Neither traced map has such a unit. Default: the base box is still in its empty state
  (as for a player with no gold store), so a store found then is already outside it and its own tile is the walk's
  result; gold stores the map lists later don't count yet. Any draws come after the facing draw.
