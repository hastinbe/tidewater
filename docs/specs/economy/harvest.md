# Economy: harvesting gold, lumber and oil

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`) and the public PUD
  specification text (UNIT section and unit data flags). Not yet confirmed against the running game, except the
  points of the round-33 errata (oracle traces Human04 and orc4).
- **Changes:** 2026-10-11, round-3 errata: the free tile for coming out is found by the ring search of
  `movement/placement.md` (limit 12, aimed at the tile the unit is heading for); a unit that finds no tile dies. The
  open question is closed.
  2026-10-11, round-5 errata: the AI hand-off on coming out (order 26, step 3) is for computer players only.
  2026-10-12, round-33 errata: the "active" look is the building's frame (1 for a gold mine, 2 for an oil platform);
  felling sets the worker's frame to 0 and its return takes over in the same update; entering aims at the
  building's top-left tile + (1, 1); the command given on coming out waits for the 25-cycle wait. The open question
  on the active look is closed.

## Behaviour

Workers (peasants and peons) gather gold from gold mines and lumber from forest tiles. Oil tankers gather oil from
their own side's oil platforms. All three follow the same cycle: go to the source, gather one load, carry it to the
nearest suitable depot, deliver it, and go back. Order numbers refer to `orders/orders.md`; tile and region terms to
`movement/occupancy.md`.

### Gather mode and carried goods

Each worker or tanker has:

- a **gather mode**: gold, lumber or none (tankers always gather oil);
- a **carrying** mark: it holds one load;
- a **return-to** source: the mine, platform or tree tile it will go back to after delivering, if any.

### Starting to harvest

A harvest command (from a smart move on a harvestable target, or the harvest button) does the following:

1. The chop counter is set to 0.
2. **Worker:**
   - The gather mode is cleared.
   - If the target (the unit named by the command, or else the ground unit standing on the target tile) is a **gold
     mine** and the worker is **not carrying** anything, the mode becomes **gold** and the mine is the target.
   - Otherwise the mode becomes **lumber**, and the target tile is kept as the tree to cut, with no target unit.
3. **Tanker:** if the target is an **oil platform of the tanker's own side** and the tanker isn't carrying, the
   platform becomes the target. Otherwise the target unit is cleared.
4. The order becomes 23 (harvest).

Mines aren't owned, so any worker can mine any gold mine. A worker already carrying goods is never put in gold mode.

### Gold and oil: going to the source

While in order 23 with a target mine or platform:

- If the target is gone (dying, dead, or inside something), the unit becomes idle (order 2).
- If the unit hasn't arrived yet (arrival radius 1, `movement/movement.md`), it moves toward the target.
- On arrival, if the mine or platform is **completed**, its order becomes 25 (enter). An uncompleted platform (still
  under construction) makes the unit idle instead.

### Entering a mine, platform or depot (order 25)

Order 25 can't be interrupted by new orders (see `orders/orders.md`).

1. **First time the order's logic runs:**
   - The unit turns to face the tile one in from the building's top-left corner (top-left tile + (1, 1)).
   - It is taken off the occupancy marks.
   - A **land worker** moves one tile in that direction, playing its move animation; the order's logic runs again
     when that move loop ends.
   - A **tanker** moves two tiles and goes in immediately, in the same cycle (step 2 follows at once).
2. **Going in:**
   - The unit becomes **inside** the building: it is hidden, untargetable, its attacker is forgotten, and its tile
     position becomes the building's position.
   - Its order becomes 26 (inside) at once, and its wait is set to **150 cycles**.
   - If the building is a depot that accepts what it carries, the goods are **delivered** (below), and nothing else
     happens.
   - Otherwise (a mine or platform):
     - the building's occupant count goes up by 1. If its frame is 0, the frame becomes 1 for a gold mine (type
       0x5C) and 2 for any other building (an oil platform): its "active" look. Otherwise the frame doesn't change.
       Every building starts with frame 0 and an occupant count of 0, and a depot's frame never changes here;
     - if the building has trips left, the unit is now **carrying**, and the building loses one trip. If that was the
       last trip, the building **collapses** (below). Otherwise the building becomes the unit's return-to source.
3. **If the building is gone** when the logic runs: a unit that already stepped in (step 1 done) **dies**; otherwise it
   becomes idle.

### Coming out (order 26)

When the 150-cycle wait ends and the inside order's logic runs:

1. The unit becomes idle (order 2) at once.
2. If it is carrying, its return-to source exists, and that source is a gold mine or oil platform, the source's
   occupant count goes down by 1: if the count is 0 it counts as 1, and when it reaches 0 the source's frame
   becomes 0 (the "active" look ends). Both frame changes happen at the moment of going in or coming out, in that
   unit's update.
3. A **computer player's** unit (`orders/orders.md`, "Terms") hands control to the AI here (a later spec). If the AI
   gives it an order, steps 4 and 5 are skipped. Every other player's unit goes straight on to steps 4 and 5.
4. If it is **carrying**, it looks for the nearest depot (below) and moves there in order 24 (return goods). If there
   is none, it stays idle.
5. Otherwise, if it has a **return-to source**, it forgets it and heads back there with a harvest command (so a
   worker that just delivered goes straight back to its mine or tree).
6. Its wait is set to **25 cycles**, and it is placed on a free tile next to the building, found by the ring search
   of `movement/placement.md` (limit 12). The search's source is sized as for a town hall (4 × 4) if the goods were
   just delivered to a gold-accepting depot, and as for a gold mine (3 × 3) otherwise, placed at the building's
   top-left tile. It is aimed at the tile the unit is heading for, so the unit comes out on that side. If no tile is
   found, the unit dies.

The order given in steps 3 to 5 (the AI's, or the return or harvest command) is the unit's pending order. It takes
over when the 25-cycle wait ends, with a wait of 3 (`orders/orders.md`, "Taking over"), not in the same update.

### Lumber

While in order 23 in lumber mode, each time the order's logic runs:

1. **Not yet there** (more than 1 tile from the tree tile on either axis): it moves toward the tree tile.
2. **There, and not yet chopping:**
   - If the tree tile is still **unreserved forest**, the worker starts chopping:
     - the chop counter is set to 0;
     - the worker turns to face the tree;
     - the tree tile is marked **reserved** (see `movement/occupancy.md`);
     - the tree tile becomes its return-to source.
   - Otherwise (the tree was cut, or another worker reserved it), it looks for the **nearest tree** (below) from its
     own tile and moves there. If none is found, it becomes idle.
3. **Chopping**, and the tree tile is still reserved:
   - While the counter is below **50**, it goes up by 1. A cheat sets it straight to 50.
   - When the counter is already 50, the tree is **felled** (`movement/occupancy.md` gives the map changes). The
     worker is now carrying lumber (its graphic changes to the carrying one) and its frame is set to 0. It is given
     a return command to the nearest depot (order 24), or becomes idle if there is none, and that order takes over
     in the same update: order 24, its still animation, wait 3 (`orders/orders.md`, "Same-update take-over").
4. **Chopping, but the tile is no longer reserved:** the reservation is dropped, and the worker looks for the nearest
   tree from its own tile, as in step 2.

The order's logic runs once per loop of the chop animation (the attack animation, 25 cycles per loop for workers;
see `core/animation-scripts.md`). The tree falls on the 51st run after chopping started: 1 run to start, 50 to
count up, then the felling run.

Each chop also picks a chop sound at random (one of 4). That draw uses the **setup-shuffle generator**, not the
simulation generator (see `core/timing-and-rng.md` and `ERRATA-round2.md`).

### Finding the nearest tree

The search works outward in square **rings** around the worker's tile, of radius 1, 2, … up to 6.
The computer job choice runs the same search with a larger limit (`ai/worker-ai.md`, "Harvest").

- Within a ring, tiles are visited clockwise, starting at the top-left corner: along the top row to the right, down
  the right column, along the bottom row to the left, then up the left column. Tiles off the map are skipped.
- Let A be the region value of the worker's own tile.
- A visited tile whose region value is A marks the ring as **reachable**. It isn't a tree.
- A visited tile that is **unreserved forest** is a hit if any tile of the 3 × 3 block around it (top-left first,
  row by row) has region value A and none of the land-blocking movement bits (land mask 0x09CE; so no unit or
  building is on it).
- The first hit is the answer: the worker is sent to that tree tile.
- After a ring with no hit, the next ring is searched only if this ring was reachable. Otherwise the search stops
  with no tree.

### Returning goods (order 24)

A return-goods command with no target looks up the nearest depot. The depot becomes the target, and the order
becomes 24. If no depot exists, the order becomes 2.

While in order 24:

- If the target depot is gone, the unit looks for the nearest depot again and moves to it (idle if none).
- On arrival (radius 1): if the depot is completed, the order becomes 25 (enter); otherwise 2.

### Finding the nearest depot

The unit's own side's units are scanned in owner-list order (most recently added first; see `core/unit-slots.md`).

- **Land worker:** a candidate is a **completed** building of the same owner that stands in the **same region** as
  the worker (the region value at the worker's tile equals the one at the building's top-left tile), and either
  stores gold, or stores wood while the worker is in lumber mode.
- **Ship (tanker):** a candidate is a completed building of the same owner that stores oil and touches the water
  region the tanker is in.
- **Nearest:** distance is measured as for target acquisition (`orders/orders.md`: Chebyshev tiles to the nearest
  edge or middle column and row of the building). A candidate replaces the best so far if it is **as close or
  closer**, so on a tie the candidate **latest** in the list wins, which is the one added to the owner's list
  earliest.

A gold-mode worker therefore only returns to a town hall, keep or castle (or their orc versions). A lumber-mode
worker also returns to a lumber mill.

### Delivering goods

Delivery happens when a unit enters a depot.

| Unit and depot | Amount added to the owner's stock |
| --- | --- |
| Ship (tanker) at a building that isn't an oil platform | Oil: **100**, or **125** if the owner has a completed refinery |
| Gold mode, at a gold-storing building | Gold: **100**; **110** if the owner has a completed keep or stronghold; **120** if a completed castle or fortress (castle checked first) |
| Lumber mode, at a gold-storing or wood-storing building | Lumber: **100**, or **125** if the owner has a completed lumber mill |
| Anything else | Nothing |

- Each amount also goes into the owner's "total gathered" statistic for that resource.
- After a delivery the carrying mark is cleared. The gather mode and return-to source stay.
- **Enhanced** rules deliver only if the unit is carrying. **Classic** rules check only the gather mode, so a unit in
  gold or lumber mode delivers a full load even with empty hands, if it gets into a depot.
- "Has a completed building" counts buildings of both races, of the owner, that have finished construction.
- The source always loses exactly one trip per load, whatever bonus the delivery gets.

### Mine and platform amounts

- A gold mine, oil patch or oil platform holds a number of **trips**. Each load taken removes one trip.
- **From the map:** the trips are 25 × the low byte of the unit's map value (the PUD stores the amount as
  2500 × that value, and one trip is 100). A value of 0 means 120, so 3000 trips (300 000 gold).
- An oil platform built on a patch takes the patch's trips. A destroyed platform leaves an oil patch holding the
  platform's remaining trips.
- **Collapse:** when the last trip is taken, the mine or platform is destroyed. Any unit still inside it is let out at
  its next chance (its target is cleared and its wait cut to 1). The unit that took the last trip carries its load as
  usual, but has no return-to source.

## Data

| Constant | Value |
| --- | --- |
| Base load | 100 (gold, lumber, oil) |
| Gold with keep / castle | 110 / 120 |
| Lumber with lumber mill | 125 |
| Oil with refinery | 125 |
| Chop counter limit | 50 |
| Time inside a mine, platform or depot | 150 cycles |
| Wait after coming out | 25 cycles |
| Trips per map amount unit | 25 (map value 0 means 120) |
| Tree search | rings of radius 1 to 6 |
| Arrival radius for harvest and return | 1 tile |

Unit data flags used (PUD unit data section): peon, tanker, sea unit, oil platform ("place to get oil"), storage for
gold, storage for wood, storage for oil, building.

## Test vectors

| Case | Expected |
| --- | --- |
| Worker with no keep, castle or lumber mill delivers gold | +100 gold |
| Same owner has a completed keep | +110 gold |
| Same owner has a completed castle and a completed keep | +120 gold |
| Worker delivers lumber, owner has a lumber mill under construction | +100 lumber |
| Tanker delivers oil, owner has a completed refinery | +125 oil |
| Lumber-mode worker, nearest building a lumber mill 3 tiles away, town hall 8 tiles away | goes to the lumber mill |
| Gold-mode worker, same buildings | goes to the town hall |
| Two town halls exactly 5 tiles away, built at different times | the one created first (it sits later in the owner list, and a later tie wins) |
| Gold mine from a map with value 0 | 3000 trips |
| Gold mine with map value 4 (10 000 gold in the PUD's terms) | 100 trips |
| Mine with 1 trip; a worker enters | the worker carries a load, the mine collapses, other workers inside come out |
| Worker reaches its tree, which another worker has reserved | searches for the nearest tree from its own tile |
| Worker at (10, 10) in region A, open land all around; forest at (12, 10) and (10, 12), both unreserved | radius-1 ring has no forest but is reachable; in the radius-2 ring (12, 10) is visited before (10, 12), so it goes to (12, 10) |
| Worker on an island where every tile of the radius-1 ring is water or forest with no free land next to it | the search stops after radius 1: no tree, worker idle |
| Worker starts chopping at cycle 0 (chop loop 25 cycles, no haste or slow) | the tree falls about 51 × 25 = 1275 cycles later |
| Peasant walks into a gold mine | one move loop to step in (16 cycles, or 22 if carrying), then 150 cycles inside |
| Mine destroyed while a worker is stepping in | the worker dies |

### Round-33 vectors (oracle traces)

| Input | Expected |
| --- | --- |
| Human04, gold mine slot 1559, worked by peons 1548, 1530 and 1529 | Frame 1 on n = 108 to 257, 660 to 809, 1112 to 1361, 1587 to 1913, 2139 to 2288, 2316 to 2465, 2691 to 2840, and from 2868 on; frame 0 at all other records |
| Same mine, n = 1112 to 1914 | In: 1530 at 1112 (count 1, frame 1), 1548 at 1212 (2). Out: 1530 at 1262 (1), 1548 at 1362 (0, frame 0). In: 1529 at 1587 (1, frame 1), 1530 at 1664 (2). Out: 1529 at 1737 (1). In: 1548 at 1764 (2). Out: 1530 at 1814 (1), 1548 at 1914 (0, frame 0) |
| Human04, great hall slot 1550; peons go in at n = 399, 951, 1403, 1503 and later | Frame stays 0 |
| orc4, oil platform slot 1596 (type 0x56) | Frame 2 on n = 11796 to 11945, 14334 to 14483, 16872 to 17021 and 19410 to 19559 (one tanker visit of 150 cycles each); frame 0 otherwise |
| Human04, peon 1531 chopping at (75, 79) | n = 1912: order 23, animation 4, frame 25, wait 1. n = 1913: order 24, animation 2, frame 0, wait 3, target the lumber mill (slot 1560). It starts walking at n = 1916 (animation 3, wait 1) |
| Human04, peon 1531 returning lumber, at (76, 78); lumber mill slot 1560, top-left (77, 77) | Order 25 takes over at n = 1938. First run at n = 1941: facing 2 (east, toward (78, 78); facing the top-left tile would be 1), tile (77, 78). Goes in at n = 1963: order 26, wait 150, tile (77, 77) |
| Human04, peon 1548 at (87, 80), gold mine 1559, top-left (88, 77) | Order 25 takes over at n = 89; the first run (n = 92) faces 1 (toward (89, 78)) and steps to (88, 79); goes in at n = 108, tile (88, 77) |
| Human04, peon 1548 (computer) comes out of mine 1559 carrying gold | n = 258: order 2, wait 25, tile (87, 79), target the great hall (slot 1550). n = 282: wait 1. n = 283: order 24, animation 2, wait 3 |

## Open questions

- How the AI routines for computer-controlled workers and tankers decide at the mine and depot.
- Whether any normal command can get an empty-handed unit into a depot in Classic mode (the Classic delivery rule
  would then pay a full load).
- The return-goods button: when it is offered and what message appears if there is no depot.
