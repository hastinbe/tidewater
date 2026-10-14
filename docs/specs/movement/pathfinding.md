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
  2026-10-12, round-36 errata: the line counts in tiles for two-tile movers too; the goal pull-back's orders, walk and
  kept tile; a unit with a command whose pending order hasn't taken over counts as about to move. Two open questions
  closed, trace vectors added.
  2026-10-12, round-37 errata: a two-tile mover's goal zone rounds its first row and column down to even and its last up
  to even; the blocked counter is cleared by every command (`movement.md`), not by the pull-back.
  2026-10-12, round-38 errata: the searching unit's own tile is free only under the moving-ally case and otherwise
  blocks the walks; trace vectors for `orc4`, `orc09-repair` and `human10`.
  2026-10-12, round-39 errata: the goal pull-back subtracts the line's steps from the goal in their order (the run
  mirrored from the goal end) and tests k − 1 tiles; the transport case is stated here; `orc8` vectors added and the
  `xorc7` and `human10` vectors restated in the new terms (same results).
  2026-10-12, round-40 errata: the no-usable-tile fallback to the unit's tile plus the last step belongs to the
  transport's own pull-back in `orders/transports.md` (any command to a transport on open water with a land or shore-
  water target); the ordinary pull-back keeps the goal for transports too.
  2026-10-12, round-43 errata: a recorded attacker that is dying doesn't count for the attack-type rule's "no attacker"
  test; `xorc01-death-coil` vector.
  2026-10-13, round-50 errata: the search's spell-caster exclusion is the mage-type units only (mage and death knight
  types); paladins and ogre mages count as fighters; `xhuman11-10000` vector.
  2026-10-13, round-52 errata: Step 1 names the boarding walk's transport as a target unit, so its goal zone is the
  transport's widened footprint.
  2026-10-13, round-59 errata: a shore building's build goal (top-left in the worker's region, else the first
  corner in it, bottom-right first); a build command's goal is pulled back; Step 1's target zone also serves a
  picked target unit and order 4's walk while it keeps a target unit; `mp-frosty-fjords-bne-30000` vectors.
  2026-10-13, round-60 errata: the target goal zone serves every walk whose order holds a target unit (static
  study; traced for attack-move and order 4); the shore corner goal is also the walk goal of a unit that is neither
  sea nor air targeting a shore building, and the build goal has no sea or air test; round-60 vectors.
  2026-10-13, round-62 errata: a smart move with a target unit that becomes the move stores the target's tile
  without pulling it back (traced for a tanker); the shore build with no corner is traced.
  2026-10-13, round-65 errata: when the line is blocked, the line tiles stop **before** G, which counts only if it is
  in the goal zone (traced: `xhuman5-30000` n = 26761, a boarding walk aimed at its transport's destination); a walk
  whose order holds a target unit sets G again from the target, where it stands now, every time it runs, so a
  command's stored goal and its pull-back last only until the first walk (traced for order 4 on a transport; static
  study only for other orders and for building targets); the open question on the pull-back with a target unit is
  closed; the `orc09-repair-600` vector no longer lists G as a line tile (same result); round-65 vectors.

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

1. **Attack-type orders:** the searching unit's order carries the "acquires" mark of `orders/orders.md` (orders 2, 4, 5,
   7, 12, 13, 14, 15 and 16, and 32, which never moves), or it is order 10 (attack-move) in Enhanced mode; it has no
   attacker (see `movement.md`; a recorded attacker that is **dying** doesn't count); and it is not a worker, tanker,
   demolition unit or **mage-type unit** (mage and death knight types, heroes included, as in `orders/orders.md`,
   "Reacting to an attacker"). Paladins and ogre mages can cast spells but count as fighters here (traced for a paladin:
   `xhuman11-10000`, vectors below; ogre mages are expected to behave the same). Orders 9 (attack a unit) and 11 (attack
   a wall) are **not** attack-type. Then **units of non-allied players are ignored**, the target included: the path goes
   through enemies, and the unit fights whatever it meets.
2. **Moving allies:** the tile's unit belongs to an allied player (including the searcher's own player); it is in
   its move animation (3), **or** it has been given a command whose pending order hasn't taken over yet; its blocked
   counter is 0; and one further state mark is clear (assumed to be "not inside something"; see "Open questions").
   Then it is ignored. The pending-command mark is set when a command stores a pending order and cleared when that
   order takes over, or when an order is set at once. This applies to the search only: row F of the blocked rules
   (`movement.md`) tests the move animation alone.

In every other case the unit blocks the search, the target included: the target gets no special treatment, and the
goal zone (Step 1) only ends the walks, it doesn't make any tile free. Air units always block other air units. These
exceptions apply only to the search. When the unit actually takes a step, any unit on the next tile blocks it (see
the blocked rules in `movement.md`).

The searching unit's own tile is no exception. It is free only when the moving-ally case applies to the unit itself
(in its move animation, or with a command whose pending order hasn't taken over, and with blocked counter 0);
otherwise it blocks the walks. This matters only when a walk comes back next to S: if S blocks, step 3 can't find it
free, so the "N is T" failure doesn't happen there and the walk turns on past S. Shortening never makes a new tile
that is S, so its checks never meet the unit's own tile.

### Step 1: mark the goal zone

The **goal zone** is the 3 × 3 tiles centred on G. For a target unit or building, it is the target's footprint widened
by one tile on each side. This rule holds **whenever the walk's order holds a target unit**, whatever the order: attack
orders, follow and guard, unit-targeted spells, a target just picked by an attack-move (orders 4, 5 and 10) and order
4's walk while it keeps a target unit (`orders/orders.md`, "Attack-move"). The 3 × 3 around G is used only when the
order holds no target unit. Traced for attack-move and order 4 (`orc12-30000` n = 23612 and `orc5-40000` n = 33260,
round-59 vectors); **static study only** for follow, guard and unit-targeted spells, where it differs from the 3 × 3
only for a target larger than 1 × 1 or when G isn't the target's tile. A boarding walk (order 34,
`orders/transports.md`) has its transport as the target unit, so its zone is the transport's widened footprint even when
G isn't the transport's tile. For two-tile movers the zone's first row and column are rounded **down** to even and its
last row and column are rounded **up** to even; the zone is the even tiles in that range, so it can reach one tile
farther out than the plain zone on the right and bottom. For example, for a 3 × 3 building at (6, 18) the widened
footprint is columns 5 to 9 and rows 17 to 21, and a two-tile mover's zone is the even tiles from (4, 16) to (10, 22);
with no target it is the even tiles from G − 2 to G + 2.

**G for a walk with a target unit.** Each time the walk runs, G is set again from the order's target unit, where it
stands now, and the zone comes from the target there:

- a **transport** target: its tile; in order 34 (board), its tile if it stands in the unit's region, otherwise its
  destination tile (`orders/transports.md`, "Order 34 (board)", step 2);
- a **building** target: the tile the building goal rule picks on its footprint ("Building goal rule", below); that
  it is worked out again at each walk is static study only;
- any other target unit: its tile.

So a goal the command stored, and its pull-back ("Goal pulled back"), lasts only until the first walk. Traced for order
4 on a transport (`xhuman5-30000` n = 26764, round-65 vectors: the walk heads for the transport's tile, not the
pulled-back goal); **static study only** for other orders, which can differ only when their target has moved off the
stored goal, and for building targets. An order with no target unit keeps its stored goal.

### Step 2: the straight line

Walk from S toward G with the line rule below, one step vector at a time.

- If the next tile is blocked, stop. The **last free tile** is T, and the **blocked direction** is the direction from
  T toward the blocked tile. The remaining line tiles, from the blocked tile up to **but not including** G, join the
  goal zone; the search calls these "line tiles". G itself counts only if it is in the goal zone. With no target unit
  the zone is the 3 × 3 around G, so G is always in it; this matters only when G is outside the target's zone, as for
  a boarding walk aimed at its transport's destination (traced: `xhuman5-30000` n = 26761, round-65 vectors).
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
- `dx` and `dy` are always differences in **tiles**, for two-tile movers too (G is already rounded down to even);
  `M`, `m` and the starting `e` come from them. Only the step length differs: a two-tile mover moves 2 tiles on each
  axis it steps along, and `e` still changes by `m` per step. Halving the deltas gives different routes.

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

**Shore building.** The rule above is **not** used for a shore building in two cases: a worker going to build one
(`economy/build-train.md`, "Placing a building"), and a unit that is **neither a sea nor an air unit** targeting one (an
attack, or any order with the building as its target). The goal is:

1. the footprint's **top-left tile**, if that tile's land region is the unit's region;
2. otherwise the first footprint corner in the unit's region, tried in this order: **bottom-right, top-right,
   bottom-left, top-left**, each checked against the map bounds first;
3. otherwise the top-left tile.

The unit's region is the region-map value at its recorded tile (for a unit inside a transport or a building, the tile
recorded for it). The build case has no sea or air test: every builder of a shore building uses this rule (in play only
land workers build them). A sea or air unit targeting a shore building uses the building goal rule above. **A build
command whose shore site has no corner in the worker's region fails silently**: the worker gets no order and nothing is
paid (traced: `orc9-orc09-shore-no-corner-600` n = 20, round-62 vectors). The goal rule is traced for the build case
`mp-frosty-fjords-bne-30000` (round-59 vectors), the attack case `orc9-orc09-shore-build-3000` n = 343 (round-60
vectors).

### Goal pulled back

Every command runs this when the order it gives is one of **0 to 16, 18 to 29, 31 to 37 and 57 to 60**, whoever gives it
(a human or a computer player). Orders 17, 30 and 38 to 56 (the spells) don't use it. A build command (order 28) is
included: it pulls back the goal tile worked out for the site, not the site (traced: `mp-frosty-fjords-bne-30000`,
round-59 vectors). It works on the goal tile the command stores (the command also clears the unit's blocked counter, for
every order, `movement.md`). One exception: a smart move with a target unit that becomes the move (order 3) stores
the target's tile **without** pulling it back, and its route goal comes from the building goal rule on the target's
footprint (`orders/orders.md`, "Smart move"; traced for a tanker: `orc9-orc09-tanker-move-2200` n = 1700; static
study only for land units):

1. A tile is **usable** when it has none of the terrain or building bits of the unit's movement mask (units are
   ignored) and, for a unit that isn't an air unit, its region is the unit's own region.
2. If the goal is usable, it is kept as it is.
3. Otherwise, take the k steps of the line rule (Step 2) from the unit's tile to the goal, **one tile per step even
   for a two-tile mover**. Start at the goal and subtract the steps one by one **in their order, first step first**;
   each subtraction gives the next tile to test. This is the line's run mirrored from the goal end, not the line's
   own tiles in reverse: the first tile tested is the goal minus the line's first step. Only the first k − 1
   subtractions are tested; the last would give the unit's own tile, which is never tested.
4. At the first usable tile, the goal becomes the tile tested just before it (the goal itself if the first tile
   tested is usable). So the new goal is still unusable unless it is the goal itself.
5. If none of the k − 1 tiles is usable, the goal is kept as it is, for transports too. The transport's own
   pull-back in `orders/transports.md` ("Land and shore targets") is a separate step with its own fallback (the
   unit's tile plus the line's **last** step). It runs for any command given to a transport on open water whose
   target tile is land or shore water.

The tested tiles are the line's own tiles in reverse when the line has a single direction, and also when its steps
are symmetric. They differ when a diagonal step is not in the middle of the run.

The search then aims at this stored goal, rounded down to even for a two-tile mover; the arrival test uses the stored
goal, not rounded, with the radius rules of `movement.md`. For an order that holds a target unit, the walk sets the
goal again from the target at its first run ("Step 1", "G for a walk with a target unit"), so the pulled-back goal is
only kept by orders with no target unit (traced for order 4 on a transport: `xhuman5-30000` n = 26764, round-65
vectors; static study for other orders). A transport sent to a land or shore-water tile from open
water follows the same rule, with the additions of `orders/transports.md` ("Land and shore targets").

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
| `xorc01-death-coil-2500` (oracle trace): ogre 1489 (type 0x07, owner 5) in order 12; its recorded attacker, skeleton 1515, is dying from n = 882 (killed by ogre 1496). At n = 886 its search picks skeleton 1522 at (54, 70) | Enemy units are ignored by its route search (the dying attacker doesn't count): n = 889, tile (55, 71), facing 7, move animation |
| Same with order 12 | The search goes through the grunt's tile; the step itself is then blocked (`movement/movement.md`, blocked rules) |
| `xorc01-death-coil` (oracle trace): grunt 1497 on (72, 79), order 10 since n = 353. At n = 356 the line's first tile (71, 79) holds ogre 1496 (order 10 since n = 354, still animation): blocked. Ally death knight 1491 on (71, 80) is in its move animation; (71, 78) and (70, 79) are free | Left walk (p = −1): south-west to (71, 80) (the moving death knight is ignored), north-west to (70, 79), and it succeeds there because (70, 78) is a line tile. The pair (SW, NW) is "two of W" through (71, 79), which is occupied, so no change: 2 steps, P's distance 12, score 14. Right walk (p = +1): north-west to (71, 78), succeeds there: 1 step, distance 13, score 14. The tie keeps the left walk |
| Same, the step at n = 356 | The first step, south-west, is onto the death knight's tile: refused (`movement/movement.md`, row I). Record n = 356: order 10, animation 3, wait 1, tile (72, 79), facing 6 |
| Same, n = 357 | Ogre 1496 has moved on to (70, 79) earlier in the same cycle, so the new line's first tile (71, 79) is free: the grunt steps west, tile (71, 79), facing 6 |
| `human12` (oracle trace): zeppelin 1559 (type 0x29, two-tile) at (92, 14), roam goal (87, 1) | G = (86, 0); tile deltas (−6, −14): M 14, m 6, e 7. Steps N, NW, N, NW, N, NW: (92, 12) at n = 9, (90, 10) at 29, (90, 8) at 49, (88, 6) at 69, (88, 4) at 89, (86, 2) at 109; 1 tile from (87, 1) on both axes, so arrived (radius 0 + 1, air); idle at n = 129 |
| Same, deltas halved to (−3, −7) (wrong) | NW first |
| `xorc7` (oracle trace): submarine 1493 (type 0x26, two-tile) at (36, 64), sea patrol to (6, 18), the top-left tile of a shipyard covering (6..8, 18..20) | Line steps NW, N, NW and so on; (6, 18) is a shipyard tile. Tested: (6, 18) − NW = (7, 19) and − N = (7, 20), shipyard tiles; − NW = (8, 21), water in its region, so the goal is (7, 20) and the search aims at (6, 20). The first 20 steps: NW, N, NW, NW, N, NW, NW, N, NW, NW, NW, N, NW, NW, N, NW, NW, N, NW, NW |
| Same shipyard, a submarine at (58, 4) | Line steps W, SW and so on. Tested (7, 18), shipyard, then (8, 17), water: goal (7, 18) |
| Footman ordered onto a tile of its own region with no building | Goal unchanged |
| Footman ordered onto an island tile (another region) with sea between (static study only) | The goal becomes the tile tested just before the first tested tile of its own region (a water tile) |
| `orc8` (oracle trace), n = 591: computer worker 1505 inside its hall, unit tile (123, 96), is sent to harvest the tree at (124, 90). Line steps N, N, NE, N, N, N. Church 1503 (type 0x3E, 3 × 3) at (124, 91); (124, 90) and (125, 90) forest | Tested: (124, 91) building, (124, 92) building, (123, 93) usable. Stored goal **(124, 92)**. The worker comes out facing 0 (N), toward (124, 92) |
| Same command, the line's own tiles walked backwards (wrong) | Would test (124, 91), (124, 92), (124, 93), (123, 94): goal (124, 93), facing 1. Differs from the trace |
| `orc8`, n = 667: worker 1505 at (123, 93), re-aimed at the tree (124, 90). Line steps NE, N, N | First tested: (124, 90) − NE = (123, 91), usable. Goal kept: **(124, 90)**. n = 670 it steps N; from n = 702 it chops from (123, 91), facing 1 |
| Unit at (0, 0), goal (0, 3), line steps S, S, S; (0, 3), (0, 2) and (0, 1) all unusable | Tested (0, 2) and (0, 1) only (k − 1 = 2). None usable: goal kept (0, 3). In the transport's own pull-back (`orders/transports.md`), (0, 0) + S = (0, 1) |
| `orc4` (oracle trace): computer footman 1538 on (9, 12), order 3, route north into (9, 11). At n = 3246 peasant 1540 (same owner) comes out of a mine onto (9, 11): order 2, wait 25, with a harvest command pending (it takes over at n = 3271) | Each new search ignores the peasant, so the route stays north and the step is refused: n = 3252 to 3257 tile (9, 12), wait 1 (row I); n = 3258 wait 15 (row D). At n = 3273 the peasant's order has taken over, its tile blocks the search, and the footman steps to (10, 11), facing 1 |
| Same, but the peasant has no pending command (static study only) | The search sees it: the route goes round at the first new search |
| `orc4-20000` (oracle trace): computer peasant 1578 (type 0x02) idle on (6, 6), given lumber at n = 6808 (order 23, animation 4, wait 3, taken over: no pending order); search at n = 6811 toward (4, 3) | The line's first step NW onto (5, 5) (worker 1539) is blocked, so T = S. Left walk: W to (5, 6); from there N, NW, W, SW, S, SE (footman 1585 on (6, 7)) and E (S itself, which blocks) are blocked, NE onto (6, 5) is free, and C's neighbour N, (5, 5), is a line tile: the walk succeeds at (5, 6). Route W; equal distance 3, minor-axis 1 against 2, so progress 1, score 2: used at once. Trace: W to (5, 6) at n = 6811, NE to (6, 5) at 6827, NW to (5, 4) at 6843 |
| Same unit in its move animation with blocked counter 0 (static study only) | Its own tile is free: the left walk reaches S and fails ("N is T") |
| Same unit in its move animation with blocked counter 2 (static study only) | Its own tile blocks |
| `orc09-repair-600` (oracle trace) n = 5: computer peasant 1587 (type 0x02) in order 28, S = (89, 4), G = (82, 6), the "Building goal rule" tile of its farm site (81, 6) (`ai/worker-ai.md`); town hall 1592 covers x 85 to 88, y 2 to 5; peasant 1584 is idle on (89, 5) | Line dx −7, dy 2, e = 3: the first step W onto (88, 4) is the hall, so T = S, blocked direction W; line tiles (88, 4), (87, 5), (86, 5), (85, 5), (84, 6), (83, 6) (G = (82, 6) itself is in the goal zone). Left walk SE, SW, W, W, W, W, succeeding at C = (85, 6) (its W neighbour (84, 6) is a line tile); (SE, SW) isn't made "two of S" because (89, 5) holds 1584; L 6, progress 3, score 9. Right walk N, N, NW, W, W, W, SW, S, S to P = (84, 4): L 9, progress 2, score 11. The left walk is used: first step SE to (90, 5), facing 3 |
| `human10-10000` (oracle trace) n = 6804: computer destroyer 1483 (type 0x1F, two-tile) on (24, 12); at n = 6801 its player gives it a patrol to (22, 9), which the pull-back keeps (line steps NW, NW, N; the k − 1 = 2 tiles tested, (23, 10) and (24, 11), are both shipyard 1580's, 3 × 3 at (22, 9)) | G = (22, 8); zone the even tiles x 20 to 24, y 6 to 10. The line's first step NW onto (22, 10) is blocked. The left walk follows the coast south from S (24, 14) and doesn't score below 8. Right walk: N onto (24, 10) is the shipyard, NE onto (26, 10) is free, then NW onto (24, 8), water and in the zone: route NE, NW, L 2, progress 2 (P's distance to (22, 9)), score 4. Trace: (26, 10), facing 1, animation 3 |
| `xorc01-death-coil` (oracle trace), n = 353: the attack-tile command given in turn 352 reaches the six selected units; death knight 1491, in its move animation on (71, 80) since n = 341, blocked once at n = 304 (counter 1) | Its counter is 0 at once, while its order 10 is still pending, so grunt 1497's search at n = 356 ignores it (the vector above) |
| A unit given a spell command (order 38 to 56) with blocked counter 5 (static study only) | Counter 0; the goal is not pulled back |
| Two-tile mover, target building 3 × 3 at (6, 18) | Goal zone: even tiles x 4 to 10, y 16 to 22 |
| Two-tile mover, no target, G = (22, 8) | Goal zone: even tiles x 20 to 24, y 6 to 10 |
| `xorc7-7000` (oracle trace): computer tanker 1594 (type 0x1A, two-tile), order 24 to its 3 × 3 shipyard 1567 at (6, 18); search at n = 410 from (14, 14) toward (8, 18), the "Building goal rule" tile | Line SW to (12, 16), W to (10, 16), then SW onto the shipyard: blocked, T = (10, 16). The left walk first tries S onto (10, 18), in the zone, and succeeds: route SW, W, S, shortened to SW, SW; P's distance 2 against S's 6, score 4, below 8, so the right walk isn't tried. Record n = 410: (12, 16), facing 5 |
| Same, n = 442 and n = 474 | n = 442: (10, 18), facing 5. n = 474: order 25 |
| `xhuman11-10000` (oracle trace), n = 4443: paladin 1510 (type 0x0C, owner 1) at (88, 76) in order 12 on grunt 1570 at (88, 74); dying grunt 1577 on (88, 75) | Steps north to (88, 75): facing 0, animation 3 (the paladin's search ignores enemy units) |

### Round-59 vectors

| Input | Expected |
| --- | --- |
| Shipyard (3 × 3) with top-left (120, 52); land worker in the same land region as (120, 52) | Goal (120, 52), not the bottom-left corner (120, 54) |
| `mp-frosty-fjords-bne-30000` (oracle trace, Enhanced): computer peasant 1548 (type 0x02, owner 2) walks in order 28 to build that shipyard | From (113, 54) at n = 16464 it steps north-east: (114, 53) at n = 16480, (115, 52) at n = 16496, (116, 51) at n = 16512, then east along row 51 to (119, 51) at n = 16560; shipyard 1518 (type 0x48) placed at (120, 52) at n = 16576. Aimed at (120, 54) it would step east to (115, 53) at n = 16496 |
| Same trace: peasant 1560 (type 0x02, owner 0) at (52, 96) given a shipyard with top-left (67, 92) at n = 16187; the raw goal (67, 92) is unusable | Pull-back: the first tile tested, (66, 92), is unusable; the second, (65, 93), is usable; stored goal (66, 92) |
| Land worker going to build a shore building; a type with the sea or air flag going to build one | Land: corner test as above. Sea or air: the top-left tile, no corner test |

### Round-60 vectors

| Input | Expected |
| --- | --- |
| `orc9-orc09-shore-build-3000` (oracle trace, Enhanced): archer 1476 (type 0x08, owner 0) at (61, 92) attacks shipyard 1469 (top-left (55, 86), 3 × 3); the top-left (55, 86) isn't in its region, and of the other corners only the bottom-left (55, 88) is | G = (55, 88): the archer walks on until it is within 4 of that tile |
| A follow (order 6) of a 2 × 2 unit (static study) | Goal zone: the followed unit's footprint widened by one tile, not the 3 × 3 around G |
| A shore building build command, no footprint corner in the worker's region (`orc9-orc09-shore-no-corner-600` n = 20: a shipyard at (22, 65), on another island's coast) | No order, nothing paid |

### Round-65 vectors

| Input | Expected |
| --- | --- |
| `xhuman5-30000` (oracle trace, Enhanced, no input), n = 26761: computer grunt 1480 (type 0x01, owner 7) at (102, 108) in order 34 on orc transport 1520 (type 0x1D, owner 7), which is landing (order 30) at (96, 98); walk goal (100, 106), the transport's destination, in the grunt's region (a break capture whose log isn't committed; research notes, "Round 65"); axethrower 1587 (type 0x09, owner 7) at (101, 107). The zone is 1520's widened footprint, so (100, 106) is outside it | The line's first step onto (101, 107) is blocked; the only line tile is (101, 107) (not G), which is blocked, so both walks fail: no route. The no-route restart then gives command 4: order 4, still animation, wait 3, target 1520 in that record (`orders/transports.md`). Counting G as a line tile, a walk succeeds and the grunt steps west to (101, 108) |
| Same trace, n = 26764: 1480 in order 4 on 1520 since n = 26761; 1520 at (96, 98); the command stored the transport's tile pulled back | The walk's G is (96, 98), 1520's tile (the same kind of break capture): the grunt steps north to (102, 107), facing 0. Aimed at the pulled-back goal it would step west |
| A unit in order 6 whose followed unit has moved off the goal the follow command stored (static study) | Its walk aims at the followed unit's current tile, with that unit's widened footprint as the zone |
| A unit in order 12 on a building (static study) | Each walk's G is the tile the building goal rule picks on the footprint |

## Open questions

- Whether two-tile movers' start tiles are always even.
- Classic-mode behaviour when S = G (it isn't short-circuited there).
- **The moving-ally rule's last condition.** A state mark must be clear; what sets it isn't identified, and no
  trace has shown it set. "Not inside something" is a guess.
