# Movement: tile steps, paths, arrival and blocking

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11). Consolidated 2026-10-14 into one model with no
  rule change (B-038; the review notes, with a map from every earlier rule and vector to its place here, are kept
  with the research-side specs). The consolidation was checked by a second spec-review session, 2026-10-14.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), per-type step timings
  computed from `Rez\script.bin` (see `specs/core/animation-scripts.md`), and oracle traces of the running game. Each
  rule says which: "traced" with the trace name and record number n, or "static study only"; a rule with neither is
  from the static study. The path search is in `pathfinding.md` and tile blocking is in `occupancy.md`. The erratum
  rounds each rule came from are listed under "History" at the end.
- **Changes:** 2026-10-15 (round 70, `ERRATA-round70.md`, M8; static study): the open question on row A's order 11
  now records the round-70 restatement and its trace request; row A and V22 are unchanged (spec review).
- **Changes:** 2026-10-15 (round 72, movement audit, `ERRATA-audit-movement.md`): the blocked counter wraps from 15
  to 0 on a block that row C doesn't catch (erratum 2, static study only, V37); row F's further state mark is
  identified and its open question closed (S1, static study); the tanker's arrival in order 28 is traced for an
  oil patch at odd coordinates (S2, V38), the even case stays open. Errata 1 and 3 restate rules this spec already
  gives (no change). Reviewed by a spec-review session, 2026-10-15.
- **Changes:** 2026-10-15 (round 75, `ERRATA-round75.md`, D1; game data): the even-patch tanker open question is
  closed (no shipped map has one); no rule changed.
- **Changes:** 2026-10-22, round 101 (`ERRATA-audit2-orders.md` erratum 5 and `ERRATA-round101-traces.md` section 1;
  reviewed by a spec-review session, 2026-10-22): row F's further state mark is the **frozen** state of a held worker,
  which the placement interface sets in single player too, so row F doesn't wait for a frozen blocker (static study;
  the state traced, V39; example V40).
- **Changes:** 2026-10-22, round 102 (`ERRATA-round102-traces.md` section 3; reviewed by a spec-review session,
  2026-10-22): the open question on two-tile movers and even tiles notes that map-placed ships and flyers can start on
  odd tiles (traced, `core/unit-slots.md` FL1). No rule changed.

## Behaviour

### The model

Every moving unit walks the same way, whatever its order. Its order logic decides where to go
(`orders/orders.md`, "Walking orders"); this spec says how the unit gets there. At each step boundary of a unit that
is walking ("When decisions are made"), in this order:

1. **Arrival.** The unit is tested against its order's arrival radius and its goal ("Arriving"). If it is there, its
   cached path is dropped and the walk ends. The goal is the one the route search aims at (`pathfinding.md`,
   "Inputs"), before the search rounds it to even for a two-tile mover: the arrival test uses the stored goal, not
   the rounded one.
2. **Next direction.** Otherwise it takes the next direction from its cached path; if the cache is used up, a new
   path is searched first ("Paths"). If the search returns no steps, the unit can't move and its order's no-route
   rule applies.
3. **Blocked test.** The next tile is tested against the blocking mask for the unit's layer. If it is blocked, the
   step doesn't happen and the first matching row of the blocked rules applies ("Blocked rules").
4. **The step.** Otherwise the step starts: the tile changes at once and the move animation carries the pixel
   position across ("Positions and steps").

Every walker follows the same steps: land units step one tile, sea and air units two. A transport landing or
pushing off takes its steps and meets blocks the same way; only its goals, step size and arrival test are its own
(`orders/transports.md`, "Moving near the shore").

**One counter, one table.** The blocked rules use one **blocked counter** per unit. Only three things clear it: a
command (any command, at the moment it is given), giving up (row C) and a transport pushing off picking a new water
tile. Every block adds 1 (a 4-bit count, so a block at 15 that row C doesn't catch wraps it to 0), and successful
steps don't reset it. The rows of the table are tested in order, and fall
into groups:

| Rows | What they are about |
| --- | --- |
| A, B | A computer player's unit blocked by a wall tile carrying bit 0x04 |
| C, D | The counter: give up at 15, search again after a 15-cycle wait from 8 |
| E, F | A blocker that is about to leave the tile: wait 15 cycles for it |
| G, H | A unit with an attacker, near its goal: stop, or search again |
| I | Anything else: search again next cycle |

Because the counter rows come before the waiting rows, a unit blocked 8 times or more is handled by row C or D
before rows E and F are tested, so it drops its path even when its blocker is moving off (V12).

### Positions and steps

- Every unit has a **tile position** and a **pixel position**. Map tiles are 32 × 32 px.
- **The pixel position** is the map-pixel point of the top-left corner of a tile cell (x grows east, y grows south;
  pixel (0, 0) is the top-left corner of tile (0, 0)):
  - **At rest**, it is exactly the tile position × 32, for mobile units and buildings alike. For a building (and any
    unit bigger than one tile) the tile position is the top-left tile of its footprint, so the pixel position is the
    top-left corner of the footprint.
  - **When a unit is placed** on the map (trained, unloaded, coming out of a building), both are set together: pixel
    = tile × 32.
  - **During a step**, the tile position already holds the destination tile (see below), while the pixel position
    starts at the old tile × 32 and is moved by the move animation's pixel moves, frame by frame, until it equals the
    new tile × 32 when the step's loop ends.
  - A building never moves, so its pixel position never changes.
  - Rules that need a tile from the pixel position (for example in `vision/fog.md`) use pixel ÷ 32, rounded down.
- Units move **one step at a time** in one of 8 directions (facing 0 = north, then clockwise; see the
  animation-scripts spec). A diagonal step changes both tile coordinates at once.
- **Step size:**
  - Land units step **one tile**.
  - Every sea unit and every air unit steps **two tiles**; the daemon and the Eye of Kilrogg are air units. Their move
    scripts cover 64 px per loop.
  - Transports that come to rest away from the shore settle on even tile coordinates. This suggests that all two-tile
    movers keep to even coordinates; not yet confirmed.
- **The tile changes at the start of a step.** When a step begins:
  1. The unit leaves its old tile in the occupancy map, its tile position becomes the destination tile, and it is
     entered there.
  2. Its facing is set to the step's direction.
  3. If it belongs to a human player, its sight is applied to the fog from the new tile at once.
  4. Its move animation then carries the pixel position across over the following cycles.
  
  So the destination tile counts as occupied for the whole step, and the tile left behind is free at once.
- **Step duration** is the move-script loop length for the unit type (16 cycles for a footman, 12 for a knight, 32
  for a ballista, and so on; full table in the animation-scripts spec). Haste and slow change it as described there.

### When decisions are made

- A unit's **order logic runs only when its animation signals "step done"**, which happens once per script loop. That
  is once per tile step for a moving land unit (every two tiles for two-tile movers), and once per attack loop for an
  attacking unit.
- A **new order** given to a unit doesn't interrupt the current step. It is held as a **pending order** and replaces
  the current one at the next "step done" boundary. When it does:
  - the new order's animation is started, which on its own would set the wait to 1;
  - the wait is then **overwritten with 3**, so the new animation's first instruction runs on the third cycle after
    the boundary. This 3 is a plain value and is not adjusted for haste or slow;
  - nothing else happens for that unit in that cycle. The new order's logic first runs at the animation's next
    "step done" signal, which is normally that first instruction;
  - a two-tile mover that becomes idle on even coordinates away from the shore settles there.
- **Which first wait applies when:**

  | Situation | First wait |
  | --- | --- |
  | Animation started by a pending order taking over | 3 |
  | Animation started for a new unit | 1, plus a random 0 to 7 (see the timing spec) |
  | Any other animation start, such as switching to the move animation to take a step, or back to the order's own animation on arrival | 1 |
  | Blocked unit (see below) | 15 or 1, set by the blocked rules |

  When a unit switches to its move animation to take a step, or back to its order's animation on arrival, the first
  instruction runs **in the same cycle**, so in that case the 1 is not waited.

### Arriving

Each order has an **arrival radius** in tiles. A unit is "there" when both the horizontal and the vertical tile
distance to its goal are within the radius (Chebyshev distance). Some orders count as always there, so the unit never
moves; for attack orders the radius is the attacker's attack range (see the combat spec). The goal is:

- for an order aimed at a mobile unit, that unit's current tile, re-read every time the check is made, so chasing
  works;
- for an order aimed at a **building**, the footprint tile picked by the building goal rule in `pathfinding.md`
  ("Building goal rule"), re-picked from the unit's current tile at every check. So arrival is measured to the near
  edge of the footprint, or to its middle row or column when the unit is level with the building.
  A unit that is neither a sea nor an air unit, heading for a **shore building** (to build it or as its target),
  uses the shore corner goal instead (`pathfinding.md`, "Shore building"), so an attack on a shore building measures
  its reach to that tile (traced: `orc9-orc09-shore-build-3000` n = 343);
- for **boarding** (order 34), the transport's tile if the transport stands in the unit's region, otherwise the
  tile the transport is heading for (`orders/transports.md`);
- otherwise, the ordered tile.

**The +1.** The radius from the order table is increased by 1 in exactly these cases:

- the unit is a **sea unit or an air unit**, **and** its current order is not 28 (build), 30 (transport landing) or 31
  (transport pushing off). The exception tests the unit's **current** order, not the order whose radius is used.
  Where it matters:
  - a **tanker** going to build an oil platform (order 28) gets no +1;
  - a **landing transport** (order 30) gets no +1: its general radius is 3, and the landing logic then checks its
    exact goal (`orders/transports.md`; `orders/orders.md`, "Order 30's radius");
  - order 31 never makes an arrival check, so its exception has no known effect;
- the unit is a land unit, the order being checked is 28, and the building to be built is a shore building.

There is never a +1 when the radius is the attack range, or when the order counts as always there. The +1 exists
because two-tile steps can't always land exactly. When a unit arrives, its cached path is dropped.

### Paths

- A unit keeps a **cached path** of up to **20 directions**.
- At each step boundary, if it hasn't arrived, it takes the next direction from the cache. If the cache is used up,
  a new path is searched first.
- The search itself is specified in `pathfinding.md`.
- If the search returns no steps, the unit can't move. Its order then ends and it becomes idle; some orders have
  their own fallback, such as an attack-type order looking for a new target, or a harvesting worker looking for the
  nearest tree (`economy/harvest.md`, "Lumber", "No route").
- **Before each step,** the next tile is tested against the blocking mask for the unit's layer (`occupancy.md`). Any
  unit on that tile blocks, including allies. If it is blocked, the step doesn't happen and the **blocked rules**
  apply.

### Blocked rules

Each unit has a **blocked counter**, from 0 to 15. It is a 4-bit count: a block that finds it at 15 wraps it to 0.
Only rows A and B below can let that happen, since they come before row C, which clears it at 15 for every other
unit (round 72, static study only; V37).

- It is set to 0 when the unit is given any command (by a player, by a computer player or by its own order logic,
  for every order, including 17, 30 and the spells), at the moment the command is given, even if the unit is in the
  middle of a step; it doesn't wait for the pending order to take over. Every command counts: a player's or a
  computer player's, a target search's resulting order (`orders/orders.md`, "Resulting order"), an order given
  again, a patrol's turnaround and the order given on coming out of a building. It is also set to 0 when the unit
  gives up (row C below), and when a transport pushing off picks a new water tile (`orders/transports.md`).
- Nothing else clears it: not a pending order taking over, an order made pending without a command, or an immediate
  order.
- It is **not** reset by successful steps, so blocks add up over the whole order.

A unit's **attacker** is the enemy unit that most recently damaged it. It is recorded when the unit is notified that it
was attacked (see the combat spec). When that unit dies, only its killer forgets it at once; any other unit keeps the
record while it is dying (`combat/damage.md`, "Applying damage") and forgets it when it next finds it no longer valid,
or when it leaves play. For human players, attacks by buildings are not recorded. While such a dying record is kept, the
"no attacker" tests of the call for help (`ai/attack-ai.md`, "Call for help") and of the route search
(`movement/pathfinding.md`, "Units seen by the search") ignore it, while the resulting-order table counts it
(`orders/orders.md`, "Resulting order"); orders 9, 11 and 17 forget it at the start of each run (`orders/orders.md`).
Any command given to the unit (`orders/orders.md`, "Commands forget a stale attacker") also forgets a recorded attacker
that is dying, dead, removed or not detected by the unit's owner, so a kept dying attacker lasts at most until the
unit's next command (traced: `xorc01-death-knight-2100`).

When the next tile is blocked, the unit restarts its walking animation in place: the move animation, or animation 6 for
an order whose own animation is 6 (orders 30 and 31). Its counter goes up by 1. Then the **first matching row** below
applies.

| # | Condition (after the counter is increased) | Outcome | Path cache | Next decision |
| --- | --- | --- | --- | --- |
| A | The unit's player is a **computer player** (see "Player kinds"), the blocking tile's movement word carries bit 0x04, and the unit is not invisible and has no attacker | Its order becomes **11** (attack a wall tile, `orders/orders.md`) on that tile, as a pending order | replaced by the new order | 1 cycle |
| B | The unit's player is a computer player, the blocking tile's movement word carries bit 0x04, and the unit is invisible or has an attacker | Keeps its order. The rows below aren't checked, so a 16th block here wraps the counter to 0 (V37) | dropped | 1 cycle |
| C | Counter is 15 | Gives up: the counter is cleared and the order becomes "stand still" (idle as a pending order). Enhanced: a unit that remembers a patrol, or an attack-move more than 1 tile away, with its resume countdown 0, instead starts its still animation with wait 1 and gets the remembered order as its pending order (`orders/orders.md`, "Giving up when blocked"; traced: `xhuman8-25000` n = 11284). Otherwise nothing else changes: the unit keeps the walking animation restarted above (traced: `orc10-10000` n = 3505, `xhuman12-10000` n = 4144) | kept (unused) | 15 cycles (1 when it resumes) |
| D | Counter is 8 to 14 | Keeps its order | dropped (it searches again) | 15 cycles |
| E | The blocker is dying or leaving play | Keeps its order | kept | 15 cycles |
| F | This unit is in its move animation (after the restart above; not in orders 30 and 31). The blocker belongs to an allied player (or the same player), is in its move animation, has a counter of 0, is not inside anything and is not **frozen** (a held worker: `orders/orders.md`, "Holding and releasing workers"; it happens in single player whenever the player picks a building to place with workers selected; a frozen walker keeps showing its move animation, so the animation alone would see it walking; static study, round 101, V39, V40). It has a next cached step, and that step is **not** onto this unit's tile | Waits for it | kept | 15 cycles |
| G | The unit has an attacker, and it is within 1 tile of its goal (the stored goal, after any pull-back) on both axes (land units) or within 2 tiles (sea and air units) | Stops: "stand still" becomes its **pending** order, so the resume rules of `orders/orders.md` apply (static study only) | kept (unused) | 1 cycle |
| H | The unit has an attacker, is a sea or air unit, and is within 2 tiles of its goal on x but farther on y | Keeps its order | dropped | 1 cycle |
| I | Anything else | Keeps its order | dropped (it searches again) | 1 cycle |

"Next decision" is how many cycles pass before the unit acts again. With 15, the unit spends 15 cycles playing the
animation in place. With 1, it tries again on the next cycle.

Notes:

- **Head-on meetings:** in row F, two allies walking into each other don't wait for each other, because each one's
  next step is onto the other's tile. They fall through to the rows below.
- **Stationary or enemy blockers:** these reach row I while the counter is 1 to 7, so the unit searches again every
  cycle. The search sees the blocker and routes around it (unless an attack-type order ignores enemies; see
  `pathfinding.md`). From the 8th block it waits 15 cycles per attempt, and at the 15th it gives up.
- **Walls:** when a computer unit's search fails, it follows the straight-line prefix up to the obstacle (see
  `pathfinding.md`). If that obstacle carries bit 0x04, rows A and B make it attack the wall. `occupancy.md` puts
  bit 0x04 on walls built by human-controlled players and on human-style walls placed in the map, **whatever their
  owner**. Walls built during play by computer players don't carry it and never trigger these rows.

### Player kinds

Two rules here and in `pathfinding.md` depend on what kind of player owns the unit, and they don't test the same
thing:

| Rule | Players it applies to |
| --- | --- |
| Blocked rows A and B (attack a wall tile) | Computer players only |
| Search mask drops bit 0x04 (`pathfinding.md`) | Every player that isn't human-controlled: computer players, passive computer players, rescue players and neutral or empty slots |

During game setup a rescuable (active) player becomes a computer player and a rescuable (passive) player becomes a
passive computer player (`game/setup.md` gives how every owner code becomes a player kind), so active rescue
players are covered by both rules and passive ones only by the second.

## Data

| Item | Value |
| --- | --- |
| Tile size | 32 px |
| Path cache length | 20 directions |
| Wait after a blocked step (rows C to F) | 15 cycles |
| Wait after a blocked step (rows A, B, G to I) | 1 cycle |
| Blocks before the path is searched again | 8 |
| Blocks before giving up | 15 |
| First wait after a pending order takes over | 3 cycles |
| Step length, land units | 1 tile |
| Step length, sea and air units | 2 tiles |
| Stop distance with an attacker (row G) | 1 tile land, 2 tiles sea and air |

Step durations per unit type are in `specs/core/animation-scripts.md` ("Measured timings").

## Test vectors

Vector IDs are stable: they number the vectors in the order the spec held them before the consolidation, so the
review file can map each one. "Source" says whether a vector is from an oracle trace (the trace is named in the
row and in the Source column), from static study, or a worked example of the rules. A row that builds on another
row starts "As" that row's ID, and names the trace when that row is from one.

### Steps and timing

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V01 | Idle footman ordered 5 tiles east over open ground | the order takes over at the idle animation's next boundary (up to 5 cycles later); 3 cycles after that the first step starts and the tile changes; 4 further tile changes follow, 16 cycles apart; it arrives at the boundary 16 cycles after the 5th step began | Example |
| V02 | As V01, but a knight | 5 steps of 12 cycles (60 cycles) | Example |
| V03 | Footman moving diagonally 3 tiles north-east | 3 diagonal steps of 16 cycles; each step moves +32 px x and −32 px y | Example |
| V04 | Unit given a new order mid-step | finishes the current step; the new order takes over at the next step boundary | Example |
| V28 | Footman at rest on tile (5, 7) | Pixel position (160, 224) | Example |
| V29 | Footman stepping from (5, 7) to (6, 7) | At the step start: tile (6, 7), pixel (160, 224). The pixel x then grows by the move animation's moves (for a footman, moves of 2 or 3 px at a time, 32 px over the 16-cycle loop; `core/animation-scripts.md`) and reaches (192, 224) when the step's loop ends | Example |
| V30 | Town hall placed with its top-left tile at (10, 10) | Pixel position (320, 320), for as long as it stands | Example |

### Arrival

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V08 | Footman attacking a 4 × 4 town hall whose footprint is (10, 10) to (13, 13), starting at (6, 11), nothing else in the way | Goal (10, 12) (west edge, middle row; a footprint tile). The straight line (`pathfinding.md`) is blocked at the goal itself, with T = (9, 12); the walk from T goes north to (9, 11), inside the goal zone. After shortening the path is (7, 11), (8, 11), (9, 11). At (9, 11) the re-picked goal is still (10, 12), at distance 1 on both axes, so it stops and attacks there after 3 steps | Example |
| V26 | Attack order, target 3 tiles away, attacker range 4 | already in range; no movement; attack starts | Example |
| V27 | Destroyer ordered to a tile it can only get within 1 tile of | stops when within 1 tile on both axes (radius +1) | Example |
| V31 | Transport landing (order 30) whose goal is 1 tile away on both axes | General test met (radius 3); the landing logic's own test (radius 0, no +1) isn't, so it keeps moving | Example |
| V32 | Transport landing (order 30) whose goal is 5 tiles away | Not arrived (radius 3, no +1): an ordinary two-tile step with the move animation | Example |
| V34 | `orc9-orc09-shore-build-3000` (oracle trace, Enhanced): archer 1476 (range 4) attacking shipyard 1469, corner goal (55, 88) | At (60, 91), distance 5: walks on; at (59, 91), distance 4: there | Trace (`orc9-orc09-shore-build-3000`) |
| V38 | `xhuman5-30000` (oracle trace, Enhanced): tanker 1542 sent to build an oil platform on the patch whose top-left tile is (75, 107), odd on both axes | Its goal is (76, 108), even, radius 0, no +1. It stands exactly on (76, 108) at n = 27600 and starts building at n = 27632. Here a +1 changes nothing: at (78, 106), before its last two-tile step, it is 2 tiles away, so the run doesn't separate radius 0 from radius 1 | Trace (`xhuman5-30000`) |

### Blocking and the counter

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V05 | Footman (human player, no attacker) on a move order in a corridor one tile wide; the next tile holds an enemy catapult standing ground (it can't fire at an adjacent unit, so the footman never gets an attacker) | block 1 (row I): path dropped. Next cycle the search sees the catapult: the line is blocked at its first tile and both walks fail or are rejected, so the result is empty and the move order ends (idle) | Example |
| V06 | Footman (human player, no attacker) chasing a found target (order 12) beyond such a catapult, in a corridor one tile wide | The search ignores the catapult (attack-type order, non-allied unit), so it keeps returning a path through it, and each step is blocked. Blocks 1 to 7: row I, searches again next cycle; blocks 8 to 14: row D, waits 15 cycles each; block 15: row C, "stand still", counter 0 | Example |
| V07 | As V06, but with an attack order on the target beyond the catapult (order 9, not attack-type; `movement/pathfinding.md`) | The search sees the catapult, as for the move order: the route is blocked at its first tile and both walks fail or are rejected | Example |
| V09 | As V05, but a route around exists | block 1: searches again next cycle and walks round the blocker; the counter stays at 1 for the rest of the order | Example |
| V10 | Footman blocked by an allied footman walking away (next step not onto it) | waits 15 cycles, keeps its path; counter +1 | Example |
| V11 | Two allied footmen stepping onto each other's tiles | neither waits; each drops its path and searches again next cycle (row I) | Example |
| V12 | Footman with counter 7 blocked by a moving ally | row D applies first: path dropped, waits 15 cycles | Example |
| V13 | `orc8-10000` (oracle trace): tanker 1483 (type 0x1A) blocked once on its way in, its pending order 25 takes over at n = 947 | Its counter isn't 0 after the take-over. n = 949: tanker 1479, blocked by 1483, doesn't wait (row F needs a counter of 0) and steps to (88, 102), facing 4 | Trace (`orc8-10000`) |
| V14 | A unit blocked twice is given a command | Counter 0 at once | Example |
| V15 | A patrolling unit reaches one end | The turnaround is a command: counter 0 | Example |
| V16 | `human12-row-f-move-700` (oracle trace), n = 403: transport 1428 in order 3 at (38, 120), next tile (36, 120) held by battleship 1420 walking west | Row F: move animation, wait 15, tile (38, 120) kept; the same at n = 450 | Trace (`human12-row-f-move-700`) |
| V17 | `human12-row-f-landing-700` (oracle trace), n = 435: transport 1428 in order 30 at (36, 122), next tile (34, 122) held by tanker 1425 (order 3, move animation, heading to (34, 124)) | Row F doesn't apply (animation 6 after the restart): animation 6, wait 1 | Trace (`human12-row-f-landing-700`) |
| V18 | As V17 (`human12-row-f-landing-700`), n = 436 | Tile (34, 120), move animation, facing 7 | Trace (`human12-row-f-landing-700`) |
| V19 | `orc8-10000` (oracle trace), n = 3554: transport 1454 (type 0x1C) at (86, 64) in order 30, landing, blocked by docked transport 1455 at (84, 64) | Animation 6, wait 1; n = 3555: no route, idle (order 2, animation 2, wait 1) | Trace (`orc8-10000`) |
| V20 | `human10-24000` (oracle trace), n = 22388: transport 1480 (type 0x1D) in order 30 | Animation 6 (animation 3 at n = 22387); the research runs stop here without this rule | Trace (`human10-24000`) |
| V21 | `xhuman10-spells-700` (oracle trace), n = 89: grunt 1497 had been blocked twice, then acquired a target (a command); grunt 1490 (owner 2) at (78, 87) is blocked by it | 1497's counter is 0, so row F applies: 1490 steps to (79, 87), animation 3 | Trace (`xhuman10-spells-700`) |
| V22 | Computer grunt on a move order (not invisible, no attacker) blocked by a wall tile carrying bit 0x04 | pending order 11 on that wall tile; acts again next cycle | Example |
| V23 | As V22, but a passive computer player's grunt | row A doesn't apply (not a computer player); falls through to the rows below | Example |
| V24 | Grunt with an attacker, blocked 1 tile from its goal (static study only) | Order unchanged in that update, idle pending, wait 1; it becomes idle (stand still) next cycle | Static study |
| V25 | Gryphon rider with an attacker, blocked 2 tiles from its goal on both axes | becomes idle | Example |
| V33 | `xhuman8-25000` (oracle trace, Enhanced), n = 11284: troll destroyer 1441 (owner 7) on patrol, 15th block, resume countdown 0 | Row C: still animation, wait 1, patrol pending; not wait 15 | Trace (`xhuman8-25000`) |
| V35 | `orc10-10000` (oracle trace), n = 3505: tanker 1533 (owner 1) in order 24 gives up (row C), nothing to resume | Animation 3, wait 15 (the move animation restarted when blocked, kept) | Trace (`orc10-10000`) |
| V36 | `xhuman12-10000` (oracle trace), n = 4144: axe thrower 1521 (owner 2) in order 4 gives up (row C), nothing resumed | Animation 3, wait 15 | Trace (`xhuman12-10000`) |
| V37 | Computer grunt with an attacker, on a move order, blocked again and again by a tile carrying bit 0x04, counter 15 before the block | Row B: counter 0 (wrapped), order kept, path dropped, acts again next cycle; no row C. If it is then blocked, with no attacker any more, by an enemy unit standing still (no bit 0x04, so rows A and B don't apply), the counter goes to 1 and row I applies, not row C | Static study |
| V39 | `human1-human01-r101-hold-workers-goals-700` (oracle trace, Enhanced), n = 67 to 162: peasant 1596 frozen at (352, 288) px (tile (11, 9)) in order 3 with animation 3 (move), frame 0, wait 1 (`orders/orders.md`, HW1) | Its stored animation stays the move animation the whole time: a blocker test that reads the animation alone sees it walking, so row F must also check that it isn't frozen | Trace (the state); static study (the test) |
| V40 | A friendly footman on a move order, blocked by peasant 1596 frozen on (11, 9) as in V39, its next cached step not onto the footman's tile | Row F doesn't apply (frozen): the footman doesn't wait for 1596, and the later rows decide. Its path search counts (11, 9) as occupied (`pathfinding.md`) | Example |

## Open questions

Carried over:

- Whether two-tile movers always keep to even tile coordinates. A ship or flyer placed by a map at an odd column or
  row starts there (traced, round 102: `core/unit-slots.md`, FL1); no shipped map does this for air units (round 75).
  How such a ship moves, lands or pushes off from an odd tile is untraced. Trace request (low): a generated map with a
  destroyer and a transport at odd tiles given moves.
- Row F's condition on the unit's own animation (orders 30 and 31 never wait) is traced for order 30 only
  (`human12-row-f-landing`); order 31 rests on the static study. The blocker's side is its current animation itself,
  read strictly; the pending-command reading is the path search's rule only.
- **When a kept dying attacker is forgotten.** "When it leaves play" is read by the implementation as the end of the
  dying unit's death animation (when it becomes remains or is removed). No trace isolates that moment:
  `orc02-assault-3000` (grunt 1568 answering a call for help at n = 969 while its attacker 1597 is still dying; footman
  1587 stepping at n = 940 after its attacker 1573 became remains at n = 890) fits it, but also fits a later end,
  because the call for help and the route search ignore a dying attacker anyway. One case is settled: the unit's
  next command forgets it ("Blocked rules"); the moment without a command is still open.
- **Row G's pending idle** rests on the static study: no trace separates a pending from an immediate idle there, because
  the traced case that once did no longer reaches row G once commands forget a dying attacker.
Closed in round 75:

- **A tanker on order 28 at an oil patch with an even top-left tile** (T12). Its goal, the patch's top-left plus
  (1, 1), would then be odd, which a two-tile mover can't stand on exactly. Every oil patch in the shipped maps has an
  odd top-left tile on both axes (game data, round 75: all 600, 105 in the campaigns and 495 in the skirmish
  maps), so the case doesn't occur with them; the traced odd case (V38) is the only one in play. Default for a
  custom map with an even patch: the same rule (radius 0, no +1) (`ERRATA-round75.md`, D1).

Closed in round 72:

- **Row F's further state mark** (was: "Row F is also skipped when the blocker has one more state bit set, whose
  meaning hasn't been identified"). The mark is set and cleared only by one multiplayer network command whose purpose
  isn't identified, and only on workers. The simulation itself never sets it, so in single-player play it is always
  clear and row F never fails on it (static study; `ERRATA-audit-movement.md`, S1). The path search's moving-ally rule
  tests the same mark (`pathfinding.md`, "Units seen by the search"). **Round 101:** the mark is the **frozen**
  state of a held worker, set by the Hold workers command that the game's own placement interface sends, in single
  player too (`orders/orders.md`, "Holding and releasing workers"; traced, HW1 to HW5), so row F can fail on it in
  ordinary play (V39, V40).

Found by the consolidation (static study, 2026-10-14; the rules above are unchanged):

- **Row A's order 11 may be given as a command.** Row A says the order becomes 11 "as a pending order", and
  `orders/orders.md` ("Order 11: attacking a wall tile", step 5) says it is set directly, not through a command. The
  static study instead finds the steps of a command there (`orders/orders.md`, "What every command does"): the
  blocked counter is cleared, the target unit is dropped, the command's attacker and target check runs, and the unit
  is marked as having a pending command, which the path search reads (`pathfinding.md`, "Units seen by the search").
  The goal is the same either way: the pull-back on a one-step line tests only the goal tile, which is kept, so the
  goal stays the blocking tile. What differs is the counter (0, not the count that reached row A), the target unit
  and the mark. Round 70 (`ERRATA-round70.md`, M8) restated this static reading with no new trace; under the
  round-68 ruling row A, V22, step 5 and V183 keep governing until a trace decides. No trace has a unit blocked again
  after row A. Trace request: a computer unit that row A sends at a wall and that is blocked again on its way (its
  counter would then start from 0 if the static reading holds).

## History

Erratum rounds that shaped this spec and the sections where their content now lives. The erratum notes for each
round are kept with the research-side specs.

| Round | Content | Now in |
| --- | --- | --- |
| 2026-10-12 (no round) | The open question on how owner codes map to player kinds is closed by `game/setup.md` | "Player kinds" |
| 35 | The corridor vector uses order 12, since order 9 isn't attack-type for the path search; a vector for order 9 is added | V06, V07 |
| 36 | A landing transport's general radius is 3 (no +1); the landing logic checks the exact goal | "The +1"; V31, V32 |
| 37 | Every command clears the blocked counter at once, for any order and even in mid-step | "Blocked rules"; V14 |
| 39 | The "Paths" fallbacks name the harvesting worker's nearest-tree search (`economy/harvest.md`) | "Paths" |
| 41 | Only commands clear the blocked counter; a pending order taking over doesn't (a patrol's turnaround and the order given on coming out are commands). Row F also needs the blocked unit's own animation to be the move animation, so it never applies in orders 30 and 31 | "Blocked rules", row F; V13, V15 |
| 42 | The blocked counter is cleared by every command (a target search's resulting order included), by giving up and by a transport pushing off, and by nothing else; only the killer forgets its victim as its attacker at once; the restart in orders 30 and 31 uses animation 6 (traced: `orc8`, `human10-24000`) | "Blocked rules"; V19, V20, V21 |
| 43 | Row F's own-animation condition is traced for order 30 (the `human12-row-f-move-700` and `human12-row-f-landing-700` vectors replace the static one); the call for help and the route search ignore a kept dying attacker, the resulting-order table counts it, and orders 9, 11 and 17 forget it; open question on when a kept dying attacker is forgotten | "Blocked rules"; V16 to V18; "Open questions" |
| 45 | Every command forgets a recorded attacker that is dying, dead, removed or not detected; row G's stand still is a pending order and measures to the stored goal (static study only) | "Blocked rules", row G; "Open questions" |
| 60 | Arrival for a unit that is neither sea nor air targeting a shore building is measured to the shore corner goal; row C resumes a remembered patrol or attack-move at once (wait 1); `xhuman8-25000` and `orc9-orc09-shore-build-3000` vectors | "Arriving"; row C; V33, V34 |
| 61 | Row C with nothing to resume keeps the restarted walking animation (the round-60 text said the still animation); `orc10-10000` and `xhuman12-10000` vectors | Row C; V35, V36 |
| B-038 (2026-10-14) | Consolidated into one model: "The model" added; vectors given IDs, a Source column and topic groups; the round citations moved here; one open question added. No rule changed | Whole spec |
| 70 | Evidence only: round 70 restated the static reading of row A (order 11 through a command); the rule is unchanged until a trace decides (spec review) | "Open questions" |
| 72 | Movement audit: the blocked counter wraps after 15 in rows A and B (static study only); row F's further state mark identified (open question closed); tanker arrival on order 28 traced for an odd-placed patch, the even case kept open | "The model"; "Blocked rules", row B and row F; "Open questions"; V37, V38 |
| 75 | The even-patch tanker question closed by game data (every shipped patch has an odd top-left tile); the rule is unchanged | "Open questions" |
| 101 | Row F's further state mark is the frozen state of a held worker (`orders/orders.md`, "Holding and releasing workers"), set in single player too | "Blocked rules", row F; "Open questions"; V39, V40 |
| 102 | Map-placed ships and flyers can start on odd tiles (`core/unit-slots.md`, FL1) | "Open questions" |
