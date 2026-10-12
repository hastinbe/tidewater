# Movement: path search

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`). The test vectors come from
  a small research model of the rules below, kept in this repo's scratch area. Not yet confirmed against the running
  game.
- **Changes:** 2026-10-12, round-22 errata: for two-tile movers the progress check measures P to the unrounded goal;
  the goal zone plays no part in it.
  2026-10-12, round-31 errata: for transports the kept tile of a goal pull-back is defined in `orders/transports.md`.
  2026-10-12, round-35 errata: attack-type orders for the search are those with the "acquires" mark (plus order 10 in
  Enhanced mode); orders 9 and 11 aren't; the target gets no special treatment. New trace vectors (a stored path kept
  while the target moves; a route into a moving ally refused at the step).

## Behaviour

The game does not search the whole map. A path request produces **at most 20 steps**, built from:

- a straight line toward the goal; and
- if the line is blocked, a **wall-following walk** around the obstacle, tried on the left and then on the right.

The unit follows those steps (see `movement.md`). When they run out, or are dropped, it searches again from wherever
it is. Long journeys are therefore many short searches.

### Inputs

- **S:** the unit's tile.
- **G:** the goal tile:
  - the ordered tile;
  - for a target unit, its tile;
  - for a target **building**, the footprint tile picked by the "building goal" rule below.
- **Step vectors:** the 8 directions of `movement.md`. Two-tile movers use vectors of length 2, and G is rounded down
  to even coordinates on both axes.
- **Blocking mask:** a set of movement-map bits for the unit's movement layer (see `occupancy.md`). A tile is
  **blocked** if its movement-map word shares a bit with the mask, after the unit-ignoring rules below.
- **Edge cases at the start:**
  - In Enhanced mode, a request with S = G returns no path.
  - Units of any player that **isn't human-controlled** drop one map bit (0x04) from their mask, if they are not
    invisible and have no attacker (as defined in `movement.md`). "Not human-controlled" covers computer players,
    passive computer players, rescue players and neutral or empty slots; see "Player kinds" in `movement.md`. Bit 0x04
    marks walls built by a human-controlled player and human-style walls placed in the map (`occupancy.md`). Every
    such wall tile also carries the obstacle bit (0x80), which stays in the mask, so on ordinary maps dropping the bit
    changes no search result. Computer units reach human walls another way: when both walks fail, the straight-line
    prefix leads them up to the wall, and the blocked rules in `movement.md` then make them attack it.

### Units seen by the search

Buildings and terrain always block. A tile that holds a **ground-layer unit** (land unit or ship) is treated as free
when either of these holds:

1. **Attack-type orders:** the searching unit's order carries the "acquires" mark of `orders/orders.md` (orders 2,
   4, 5, 7, 12, 13, 14, 15 and 16, and 32, which never moves), or it is order 10 (attack-move) in Enhanced mode; it
   has no attacker (see `movement.md`); and it is not a worker, tanker, demolition unit or spell-caster type. Orders
   9 (attack a unit) and 11 (attack a wall) are **not** attack-type. Then **units of non-allied players are
   ignored**, the target included: the path goes through enemies, and the unit fights whatever it meets.
2. **Moving allies:** the tile's unit belongs to an allied player (including the searcher's own player). It is
   currently moving or about to move, its blocked counter is 0, and it is not inside something. Then it is ignored.

In every other case the unit blocks the search, the target included: the target gets no special treatment, and the
goal zone (Step 1) only ends the walks, it doesn't make any tile free. Air units always block other air units. These
exceptions apply only to the search. When the unit actually takes a step, any unit on the next tile blocks it (see
the blocked rules in `movement.md`).

### Step 1: mark the goal zone

The **goal zone** is the 3 × 3 tiles centred on G. For a target unit or building, it is the target's footprint
widened by one tile on each side. For two-tile movers it is aligned to even coordinates.

### Step 2: the straight line

Walk from S toward G with the line rule below, one step vector at a time.

- If the next tile is blocked, stop. The **last free tile** is T, and the **blocked direction** is the direction from
  T toward the blocked tile. The remaining line tiles, from the blocked tile up to G, join the goal zone; the search
  calls these "line tiles".
- If 20 steps were taken, or G was reached, the line is the result. Even if G is farther than 20 steps, the path is
  the first 20 steps of the line.

**Line rule** (integer Bresenham):

- Let `dx = Gx − Sx`, `dy = Gy − Sy`, major `M = max(|dx|, |dy|)` and minor `m = min(|dx|, |dy|)`.
- The x axis is major when `|dx| ≥ |dy|`, so ties go to x.
- Start with `e = floor(M / 2)`, or 1 if that is 0.
- Each step:
  - move 1 along the major axis;
  - set `e ← e − m`; if `e < 1`, also move 1 along the minor axis and set `e ← e + M`.
  - Movement on an axis is in the + direction when that axis's delta is ≥ 0, and − otherwise.

### Step 3: walk around the obstacle

This runs only if the line was blocked. A walk takes a **turn sense** p: −1 (counter-clockwise, which keeps the
obstacle on the walker's right) or +1 (clockwise, obstacle on the left). Direction numbers are taken mod 8.

1. Start at C = T. The **first direction tried** is the blocked direction + p.
2. From C, try directions in the order first, first + p, first + 2p, … (up to 8 tries) until the neighbour of C in
   that direction is free:
   - a neighbour outside the map ends the walk as a **failure**;
   - if all 8 directions are blocked, it also fails.
3. Call the free direction d and the neighbour N:
   - If N is T (the walk has come back to its start), it fails.
   - If C was already on the route (the walk crossed itself), the part of the route after C is removed. This also
     applies if C is on the straight-line part before T. The route then continues from C in direction d.
   - If N is in the goal zone or on a line tile, the walk **succeeds at N**.
   - Otherwise, if C's neighbour in direction d + p is in the goal zone or on a line tile, the walk **succeeds at C**.
4. Move to C = N. The first direction tried from the new C is **d − 2p** (two turns back against the turn sense from
   the direction just taken), and step 2 runs again. Give up (failure) after 50 moves.

For example, with p = −1 (counter-clockwise) after a move east (d = 2), the next tries are 4 (S), 3 (SE), 2 (E),
1 (NE), and so on.

The route found runs from S along the line to T, then along the walk to the end point P.

### Step 4: judge and shorten each walk

- **Progress check:** the walk is rejected if P = S. Otherwise compare P with S using Chebyshev distance to G:
  - if P is strictly closer, the progress value is P's distance;
  - if it is farther, the walk is rejected;
  - if the distance is equal, the progress value is P's **minor-axis** distance, provided it is smaller than S's.
    Otherwise the walk is rejected. A minor-axis distance of 0 is also a rejection.

  For a two-tile mover, P's distance is measured to the goal before it was rounded to even coordinates (the ordered
  tile, or for a target building the "Building goal rule" tile picked from S), while S's distance uses the rounded G.
  For other units the two are the same. The goal zone plays no part in this check: a walk that ends inside the goal
  zone is still rejected when P is farther from G than S.
- **Shortening:** improve the route from S to P in **passes**. A pass walks the route from S, looking at pairs of
  consecutive steps (a, b), with a leaving the current tile. It looks each pair up in the table below:
  - A **direction** entry replaces the two steps with one step in that direction. No passability check is made, so
    diagonal corner cutting is allowed. The pass then continues from the tile that step reaches: the next pair is
    the step **after** the merged step and the step after that. The merged step itself isn't paired again in this
    pass.
  - **swap** turns (a, b) into (b, a) through the other corner tile, but only if that tile is free (any unit blocks,
    with no ignoring) and not already on the route. If the swap is made, the next pair is the second new step (a)
    and the step after it. If not, the pass moves on one step: the next pair is (b, the step after it).
  - **two-of v** turns them into two steps in direction v through a new middle tile, with the same check. If made,
    the next pair is the second new step (v) and the step after it. If not, the pass moves on one step.
  - **reverse** (a step straight back) rejects the walk at once.
  - **keep** leaves the pair alone; the pass moves on one step.

  A pass ends when the next step would reach P. Passes repeat until a pass makes no change.

  The route length L is the number of steps from S to P after shortening.
- **Score** = L + the progress value.

### Step 5: choose

1. Try the left walk (p = −1). If it is accepted with a **score below 8**, use it at once.
2. Otherwise try the right walk (p = +1). Use it if it is accepted and either the left walk was rejected or the right
   score is **strictly lower**. Ties keep the left walk.
3. If both are rejected, the result is the straight-line steps from S up to T. This can be no steps at all.

The unit stores the first 20 steps of the chosen route. **An empty result means no path**, and the order logic then
gives up on the move (see `movement.md`).

### Building goal rule

For a target building with footprint from x0 to x1 and y0 to y1 (inclusive), and the searching unit at (ux, uy):

- **Goal x:**
  - x0 if `ux ≤ x0`;
  - x1 if `ux ≥ x1`;
  - otherwise `x0 + floor(width / 2)`.
- **Goal y:** the same rule on y.

For a land unit heading to a shore building that lies in a different land region, the goal is moved to a footprint
corner that touches the unit's region, if one exists. Regions come from the map's region data.

### Goal pulled back

For some orders, and for transports heading to a shore, a goal tile that the unit can't stand on is moved before the
search. "Can't stand on" means blocked terrain or buildings for the unit's layer, ignoring units, or a different
region for non-flying units. The goal is walked back along the line toward the unit until a usable tile is found.
For a transport sent to a land or shore-water tile from open water, `orders/transports.md` ("Land and shore targets")
gives the full rule: the kept tile is the one just before the first usable tile, on the goal's side. For other
orders, which of the two neighbouring tiles is kept is still open.

## Data

| Item | Value |
| --- | --- |
| Most steps stored per search | 20 |
| Most straight-line steps taken | 20 |
| Most wall-walk moves | 50 |
| Accept-left-immediately score | below 8 |
| Goal zone, tile goal | 3 × 3 around G |

**Shortening table.** Rows are the first step a, columns the second step b. Directions: 0 N, 1 NE, 2 E, 3 SE, 4 S,
5 SW, 6 W, 7 NW.

| a \ b | N | NE | E | SE | S | SW | W | NW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| N | keep | swap | NE | E | reverse | W | NW | swap |
| NE | keep | keep | keep | two of E | E | reverse | N | two of N |
| E | NE | swap | keep | swap | SE | S | reverse | N |
| SE | E | two of E | keep | keep | keep | two of S | S | reverse |
| S | reverse | E | SE | swap | keep | swap | SW | W |
| SW | W | reverse | S | two of S | keep | keep | keep | two of W |
| W | NW | N | reverse | S | SW | swap | keep | swap |
| NW | keep | two of N | N | reverse | W | two of W | keep | keep |

The swaps move a diagonal step ahead of a neighbouring straight step, so that later passes can merge it.

## Test vectors

The maps are open ground except for `#`. S and G are given as (x, y), with y growing southward. The results assume no
units on the map.

| Input | Expected steps |
| --- | --- |
| Open map, S (0,0), G (5,2) | SE, E, E, SE, E |
| Open map, S (0,0), G (25,3) | 20 steps: E, E, E, SE, E×8, SE, E×7 (then a new search) |
| Wall at y = 2, x = 1..3 (5 × 5 map); S (2,0), G (2,4) | left walk reaches (3,3), score 4: SE, SE, SW |
| Wall at y = 1, x = 1..5 (8 × 4 map); S (2,0), G (2,3) | W, SW, SE |
| Wall at x = 3, y = 1..4 (10 × 6 map); S (1,2), G (6,2) | NE, NE, SE, S |
| Wall at x = 2, y = 1..5 (6 × 6 map); S (0,3), G (5,3) | NE, N, NE, SE |
| S (2,2) fully enclosed by a ring of `#`, G (0,0) | no path (empty), so the order gives up |
| Enhanced mode, S = G | no path |
| Shortening only (open ground): route N, E, SW, SE from S | Pass 1: (N, E) merges to NE; the pass resumes at (SW, SE), which becomes S, S. Pass 2: (NE, S) merges to E; the pass resumes after it and has no pair left. Pass 3: (E, S) merges to SE. Result: one step, SE. (Re-checking the merged step at once would meet (NE, SW), a reverse, and reject the walk.) |
| Footman with an attack order; the only route passes an enemy footman | the search goes through the enemy's tile; the actual step is then blocked and the unit fights it |
| One-tile mover at S = (35, 4), G = (38, 5) (a 4 × 4 hall's tile); a walk ends at P = (37, 9), inside the goal zone | S's distance 3, P's 4: rejected |
| Two-tile mover at S = (10, 10), ordered tile (13, 11) | G = (12, 10); S's distance 2. A walk ending at P = (12, 12) has distance 1 to (13, 11): accepted with progress value 1. (Measured to the rounded G it would be 2, equal, with minor-axis distance 0: rejected) |
| `orc02-assault` (oracle trace): computer footman 1582, order 12 from n = 275, target 1575. Its search at n = 278 from (54, 27) aims at 1575's tile then, (50, 33) | Order 12 is attack-type. The stored steps run along the line: (53, 28), (53, 29), (52, 30), (51, 31). The footman follows them although 1575 moves to (51, 32) at n = 288 and (51, 31) at n = 304: n = 294 tile (53, 29), facing 4; n = 310 tile (52, 30), facing 5 |
| A footman in order 9 (no attacker); the line to its target passes an idle enemy grunt (static study only) | The grunt's tile blocks the search; the route walks round it |
| Same with order 12 | The search goes through the grunt's tile; the step itself is then blocked (`movement/movement.md`, blocked rules) |
| `xorc01-death-coil` (oracle trace): grunt 1497 on (72, 79), order 10 since n = 353. At n = 356 the line's first tile (71, 79) holds ogre 1496 (order 10 since n = 354, still animation): blocked. Ally death knight 1491 on (71, 80) is in its move animation; (71, 78) and (70, 79) are free | Left walk (p = −1): south-west to (71, 80) (the moving death knight is ignored), north-west to (70, 79), and it succeeds there because (70, 78) is a line tile. The pair (SW, NW) is "two of W" through (71, 79), which is occupied, so no change: 2 steps, P's distance 12, score 14. Right walk (p = +1): north-west to (71, 78), succeeds there: 1 step, distance 13, score 14. The tie keeps the left walk |
| Same, the step at n = 356 | The first step, south-west, is onto the death knight's tile: refused (`movement/movement.md`, row I). Record n = 356: order 10, animation 3, wait 1, tile (72, 79), facing 6 |
| Same, n = 357 | Ogre 1496 has moved on to (70, 79) earlier in the same cycle, so the new line's first tile (71, 79) is free: the grunt steps west, tile (71, 79), facing 6 |

## Open questions

- Which tile is kept when a goal is pulled back, for orders other than the transport's (`orders/transports.md`).
  Default: the same rule.
- Which orders use the "goal pulled back" rule. Transports do.
- Whether two-tile movers' start tiles are always even.
- Classic-mode behaviour when S = G (it isn't short-circuited there).
- The exact meanings of "about to move" and "inside something" in the moving-ally rule.
