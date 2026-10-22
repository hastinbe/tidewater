# Movement: path search

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11). Consolidated 2026-10-14 into one model with no
  rule change (B-038; the review notes, with a map from every earlier rule and vector to its place here, are kept
  with the research-side specs). The consolidation was checked by a second spec-review session, 2026-10-14.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`) and oracle traces of the
  running game. Each rule says which: "traced" with the trace name and record number n, or "static study only"; a
  rule with neither is from the static study. The vectors without a trace come from a small research model of the
  rules below, kept in this repo's scratch area. The erratum rounds each rule came from are listed under "History" at
  the end.
- **Changes:** 2026-10-15 (round 72, movement audit, `ERRATA-audit-movement.md`): the moving-ally rule's last state
  mark is identified and its open question closed (S1, static study); a static reading of the goal pull-back for air
  units (erratum 5) is filed under "Open questions", and step 1 is unchanged. Reviewed by a spec-review session,
  2026-10-15.
- **Changes:** 2026-10-15 (round 75, `ERRATA-round75.md`, D2; game data): the air-unit pull-back open question
  is closed as having no effect in play; no rule changed.
- **Changes:** 2026-10-22, round 101 (`ERRATA-audit2-orders.md` erratum 5; reviewed by a spec-review session,
  2026-10-22): the moving-ally rule's last condition is "not frozen" (a held worker, set in single player too); a
  frozen walker blocks the search (static study).

## Behaviour

The game does not search the whole map. A path request produces **at most 20 steps**, built from:

- a straight line toward the goal; and
- if the line is blocked, a **wall-following walk** around the obstacle, tried on the left and then on the right.

The unit follows those steps (see `movement.md`). When they run out, or are dropped, it searches again from wherever
it is. Long journeys are therefore many short searches.

### The model

A search is one fixed sequence with two fallbacks:

1. **Goal zone** (Step 1): the tiles that end a walk.
2. **Straight line** (Step 2): if it isn't blocked, it is the result.
3. **Walks** (Step 3), each **judged and shortened** (Step 4), and **chosen** (Step 5): the left walk is used at once
   if its score is below 8; otherwise the better of the two.
4. **Fallbacks:** if both walks are rejected, the straight-line steps up to the obstacle; if those are none, no path.

Four shared rules are each stated once below and used everywhere they apply.

**One walk goal.** The goal the search aims at (G) and the goal the arrival test of `movement.md` ("Arriving")
measures to are the same tile, set the same way for each kind of goal: the ordered or stored tile; a target unit's
tile, read when the walk runs; a building's "Building goal rule" tile, or its "Shore building" corner for a unit that
is neither a sea nor an air unit; and for boarding, the transport's tile or destination ("G for a walk with a target
unit"). The only difference is the rounding: the search rounds G down to even for a two-tile mover ("Inputs"), while
the arrival test, and the progress check's distance for P, use the goal before rounding (Step 4, "Goal pulled
back"). That both tests
read this one goal for every kind of goal is static study only (traced for the cases each rule names).

**Three tile tests, kept apart.** They don't give the same answer for the same tile, and each is used only where
stated:

| Test | Used by | A tile fails when |
| --- | --- | --- |
| **Blocked for the search** | Steps 2 and 3 | its movement-map word shares a bit with the unit's blocking mask, after the unit-ignoring rules of "Units seen by the search" |
| **Free for shortening** | Step 4's swap and two-of entries | it is blocked with no unit ignored (any unit blocks), or it is already on the route |
| **Usable for the pull-back** | "Goal pulled back"; the transport's pull-back (`orders/transports.md`, "Land and shore targets") | it has a terrain or building bit of the mask (units are ignored), or, for a unit that isn't an air unit, its region isn't the unit's |

The step itself uses a fourth test, in `movement.md` ("Paths"): any unit on the next tile blocks it.

**One line rule, four users.** The "Line rule" (Step 2) gives the steps from one tile to another. It is used by the
search's straight line (Step 2), by the goal pull-back ("Goal pulled back", one tile per step even for a two-tile
mover), by the transport's pull-back, and by the transport's land-first test (`orders/transports.md`, "The straight
line"). The last three always take one-tile steps.

**One pull-back, two settings.** The command's pull-back ("Goal pulled back") and the transport's pull-back
(`orders/transports.md`, "Land and shore targets", step 2.1) follow the same steps: test the goal, then the goal
minus the line's steps in their order, stopping before the unit's tile; at the first usable tile, the goal becomes
the tile tested just before it. They differ only in two settings:

| Setting | Command's pull-back | Transport's pull-back |
| --- | --- | --- |
| Mask for "usable" | the terrain and building bits of the unit's own mask | the sea mask without the unit bit (0x0803) |
| No usable tile | the goal is kept | the tile tested last, which is the unit's tile plus the line's last step |

That they follow the same steps is static study only; the traced cases agree (`xorc7`, `orc8`, Human04).

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
   `xhuman11-10000`, V43; ogre mages are expected to behave the same). Orders 9 (attack a unit) and 11 (attack
   a wall) are **not** attack-type. Then **units of non-allied players are ignored**, the target included: the path goes
   through enemies, and the unit fights whatever it meets.
2. **Moving allies:** the tile's unit belongs to an allied player (including the searcher's own player); it is in
   its move animation (3), **or** it has been given a command whose pending order hasn't taken over yet; its blocked
   counter is 0; and it isn't **frozen** (a held worker: `orders/orders.md`, "Holding and releasing workers"; it
   happens in single player whenever the player picks a building to place with workers selected; round 101, static
   study; `movement.md`, row F, V39 and V40).
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
order holds no target unit. Traced for attack-move and order 4 (`orc12-30000` n = 23612 and `orc5-40000` n = 33260);
**static study only** for follow, guard and unit-targeted spells, where it differs from the 3 × 3
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
4 on a transport (`xhuman5-30000` n = 26764, V52: the walk heads for the transport's tile, not the
pulled-back goal); **static study only** for other orders, which can differ only when their target has moved off the
stored goal, and for building targets. An order with no target unit keeps its stored goal.

### Step 2: the straight line

Walk from S toward G with the line rule below, one step vector at a time.

- If the next tile is blocked, stop. The **last free tile** is T, and the **blocked direction** is the direction from
  T toward the blocked tile. The remaining line tiles, from the blocked tile up to **but not including** G, join the
  goal zone; the search calls these "line tiles". G itself counts only if it is in the goal zone. With no target unit
  the zone is the 3 × 3 around G, so G is always in it; this matters only when G is outside the target's zone, as for
  a boarding walk aimed at its transport's destination (traced: `xhuman5-30000` n = 26761, V51).
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
paid (traced: `orc9-orc09-shore-no-corner-600` n = 20, V50). The goal rule is traced for the build case
`mp-frosty-fjords-bne-30000` (V45), the attack case `orc9-orc09-shore-build-3000` n = 343 (V48).

### Goal pulled back

Every command runs this when the order it gives is one of **0 to 16, 18 to 29, 31 to 37 and 57 to 60**, whoever gives it
(a human or a computer player). Orders 17, 30 and 38 to 56 (the spells) don't use it. A build command (order 28) is
included: it pulls back the goal tile worked out for the site, not the site (traced: `mp-frosty-fjords-bne-30000`,
V46). It works on the goal tile the command stores (the command also clears the unit's blocked counter, for
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
only kept by orders with no target unit (traced for order 4 on a transport: `xhuman5-30000` n = 26764, V52;
static study for other orders). A transport sent to a land or shore-water tile from open
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
units on the map unless the row names them.

Vector IDs are stable: they number the vectors in the order the spec held them before the consolidation, so the
review file can map each one. "Source" says whether a vector is from an oracle trace (the trace is named in the
row and in the Source column), from static study, or a worked example of the rules (from the research model). A
row that builds on another row starts "As" that row's ID, and names the trace when that row is from one.

### Line, walks and choice

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V01 | Open map, S (0,0), G (5,2) | SE, E, E, SE, E | Example |
| V02 | Open map, S (0,0), G (25,3) | 20 steps: E, E, E, SE, E×8, SE, E×7 (then a new search) | Example |
| V03 | Wall at y = 2, x = 1..3 (5 × 5 map); S (2,0), G (2,4) | left walk reaches (3,3), score 4: SE, SE, SW | Example |
| V04 | Wall at y = 1, x = 1..5 (8 × 4 map); S (2,0), G (2,3) | W, SW, SE | Example |
| V05 | Wall at x = 3, y = 1..4 (10 × 6 map); S (1,2), G (6,2) | NE, NE, SE, S | Example |
| V06 | Wall at x = 2, y = 1..5 (6 × 6 map); S (0,3), G (5,3) | NE, N, NE, SE | Example |
| V07 | S (2,2) fully enclosed by a ring of `#`, G (0,0) | no path (empty), so the order gives up | Example |
| V08 | Enhanced mode, S = G | no path | Example |
| V20 | `human12` (oracle trace): zeppelin 1559 (type 0x29, two-tile) at (92, 14), roam goal (87, 1) | G = (86, 0); tile deltas (−6, −14): M 14, m 6, e 7. Steps N, NW, N, NW, N, NW: (92, 12) at n = 9, (90, 10) at 29, (90, 8) at 49, (88, 6) at 69, (88, 4) at 89, (86, 2) at 109; 1 tile from (87, 1) on both axes, so arrived (radius 0 + 1, air); idle at n = 129 | Trace (`human12`) |
| V21 | As V20 (`human12`), deltas halved to (−3, −7) (wrong) | NW first | Example |
| V32 | `orc4-20000` (oracle trace): computer peasant 1578 (type 0x02) idle on (6, 6), given lumber at n = 6808 (order 23, animation 4, wait 3, taken over: no pending order); search at n = 6811 toward (4, 3) | The line's first step NW onto (5, 5) (worker 1539) is blocked, so T = S. Left walk: W to (5, 6); from there N, NW, W, SW, S, SE (footman 1585 on (6, 7)) and E (S itself, which blocks) are blocked, NE onto (6, 5) is free, and C's neighbour N, (5, 5), is a line tile: the walk succeeds at (5, 6). Route W; equal distance 3, minor-axis 1 against 2, so progress 1, score 2: used at once. Trace: W to (5, 6) at n = 6811, NE to (6, 5) at 6827, NW to (5, 4) at 6843 | Trace (`orc4-20000`) |
| V35 | `orc09-repair-600` (oracle trace) n = 5: computer peasant 1587 (type 0x02) in order 28, S = (89, 4), G = (82, 6), the "Building goal rule" tile of its farm site (81, 6) (`ai/worker-ai.md`); town hall 1592 covers x 85 to 88, y 2 to 5; peasant 1584 is idle on (89, 5) | Line dx −7, dy 2, e = 3: the first step W onto (88, 4) is the hall, so T = S, blocked direction W; line tiles (88, 4), (87, 5), (86, 5), (85, 5), (84, 6), (83, 6) (G = (82, 6) itself is in the goal zone). Left walk SE, SW, W, W, W, W, succeeding at C = (85, 6) (its W neighbour (84, 6) is a line tile); (SE, SW) isn't made "two of S" because (89, 5) holds 1584; L 6, progress 3, score 9. Right walk N, N, NW, W, W, W, SW, S, S to P = (84, 4): L 9, progress 2, score 11. The left walk is used: first step SE to (90, 5), facing 3 | Trace (`orc09-repair-600`) |
| V36 | `human10-10000` (oracle trace) n = 6804: computer destroyer 1483 (type 0x1F, two-tile) on (24, 12); at n = 6801 its player gives it a patrol to (22, 9), which the pull-back keeps (line steps NW, NW, N; the k − 1 = 2 tiles tested, (23, 10) and (24, 11), are both shipyard 1580's, 3 × 3 at (22, 9)) | G = (22, 8); zone the even tiles x 20 to 24, y 6 to 10. The line's first step NW onto (22, 10) is blocked. The left walk follows the coast south from S (24, 14) and doesn't score below 8. Right walk: N onto (24, 10) is the shipyard, NE onto (26, 10) is free, then NW onto (24, 8), water and in the zone: route NE, NW, L 2, progress 2 (P's distance to (22, 9)), score 4. Trace: (26, 10), facing 1, animation 3 | Trace (`human10-10000`) |
| V41 | `xorc7-7000` (oracle trace): computer tanker 1594 (type 0x1A, two-tile), order 24 to its 3 × 3 shipyard 1567 at (6, 18); search at n = 410 from (14, 14) toward (8, 18), the "Building goal rule" tile | Line SW to (12, 16), W to (10, 16), then SW onto the shipyard: blocked, T = (10, 16). The left walk first tries S onto (10, 18), in the zone, and succeeds: route SW, W, S, shortened to SW, SW; P's distance 2 against S's 6, score 4, below 8, so the right walk isn't tried. Record n = 410: (12, 16), facing 5 | Trace (`xorc7-7000`) |
| V42 | As V41 (`xorc7-7000`), n = 442 and n = 474 | n = 442: (10, 18), facing 5. n = 474: order 25 | Trace (`xorc7-7000`) |

### Shortening and the progress check

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V09 | Shortening only (open ground): route N, E, SW, SE from S | Pass 1: (N, E) merges to NE; the pass resumes at (SW, SE), which becomes S, S. Pass 2: (NE, S) merges to E; the pass resumes after it and has no pair left. Pass 3: (E, S) merges to SE. Result: one step, SE. (Re-checking the merged step at once would meet (NE, SW), a reverse, and reject the walk.) | Example |
| V11 | One-tile mover at S = (35, 4), G = (38, 5) (a 4 × 4 hall's tile); a walk ends at P = (37, 9), inside the goal zone | S's distance 3, P's 4: rejected | Example |
| V12 | Two-tile mover at S = (10, 10), ordered tile (13, 11) | G = (12, 10); S's distance 2. A walk ending at P = (12, 12) has distance 1 to (13, 11): accepted with progress value 1. (Measured to the rounded G it would be 2, equal, with minor-axis distance 0: rejected) | Example |

### Which units the search sees

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V10 | Footman with an attack order; the only route passes an enemy footman | the search goes through the enemy's tile; the actual step is then blocked and the unit fights it | Example |
| V13 | `orc02-assault` (oracle trace): computer footman 1582, order 12 from n = 275, target 1575. Its search at n = 278 from (54, 27) aims at 1575's tile then, (50, 33) | Order 12 is attack-type. The stored steps run along the line: (53, 28), (53, 29), (52, 30), (51, 31). The footman follows them although 1575 moves to (51, 32) at n = 288 and (51, 31) at n = 304: n = 294 tile (53, 29), facing 4; n = 310 tile (52, 30), facing 5 | Trace (`orc02-assault`) |
| V14 | A footman in order 9 (no attacker); the line to its target passes an idle enemy grunt (static study only) | The grunt's tile blocks the search; the route walks round it | Static study |
| V15 | `xorc01-death-coil-2500` (oracle trace): ogre 1489 (type 0x07, owner 5) in order 12; its recorded attacker, skeleton 1515, is dying from n = 882 (killed by ogre 1496). At n = 886 its search picks skeleton 1522 at (54, 70) | Enemy units are ignored by its route search (the dying attacker doesn't count): n = 889, tile (55, 71), facing 7, move animation | Trace (`xorc01-death-coil-2500`) |
| V16 | As V14, but in order 12 | The search goes through the grunt's tile; the step itself is then blocked (`movement/movement.md`, blocked rules) | Static study |
| V17 | `xorc01-death-coil` (oracle trace): grunt 1497 on (72, 79), order 10 since n = 353. At n = 356 the line's first tile (71, 79) holds ogre 1496 (order 10 since n = 354, still animation): blocked. Ally death knight 1491 on (71, 80) is in its move animation; (71, 78) and (70, 79) are free | Left walk (p = −1): south-west to (71, 80) (the moving death knight is ignored), north-west to (70, 79), and it succeeds there because (70, 78) is a line tile. The pair (SW, NW) is "two of W" through (71, 79), which is occupied, so no change: 2 steps, P's distance 12, score 14. Right walk (p = +1): north-west to (71, 78), succeeds there: 1 step, distance 13, score 14. The tie keeps the left walk | Trace (`xorc01-death-coil`) |
| V18 | As V17 (`xorc01-death-coil`), the step at n = 356 | The first step, south-west, is onto the death knight's tile: refused (`movement/movement.md`, row I). Record n = 356: order 10, animation 3, wait 1, tile (72, 79), facing 6 | Trace (`xorc01-death-coil`) |
| V19 | As V17 (`xorc01-death-coil`), n = 357 | Ogre 1496 has moved on to (70, 79) earlier in the same cycle, so the new line's first tile (71, 79) is free: the grunt steps west, tile (71, 79), facing 6 | Trace (`xorc01-death-coil`) |
| V30 | `orc4` (oracle trace): computer footman 1538 on (9, 12), order 3, route north into (9, 11). At n = 3246 peasant 1540 (same owner) comes out of a mine onto (9, 11): order 2, wait 25, with a harvest command pending (it takes over at n = 3271) | Each new search ignores the peasant, so the route stays north and the step is refused: n = 3252 to 3257 tile (9, 12), wait 1 (row I); n = 3258 wait 15 (row D). At n = 3273 the peasant's order has taken over, its tile blocks the search, and the footman steps to (10, 11), facing 1 | Trace (`orc4`) |
| V31 | As V30 (`orc4`), but the peasant has no pending command (static study only) | The search sees it: the route goes round at the first new search | Static study |
| V33 | As V32 (`orc4-20000`): the same unit in its move animation with blocked counter 0 (static study only) | Its own tile is free: the left walk reaches S and fails ("N is T") | Static study |
| V34 | As V32 (`orc4-20000`): the same unit in its move animation with blocked counter 2 (static study only) | Its own tile blocks | Static study |
| V37 | `xorc01-death-coil` (oracle trace), n = 353: the attack-tile command given in turn 352 reaches the six selected units; death knight 1491, in its move animation on (71, 80) since n = 341, blocked once at n = 304 (counter 1) | Its counter is 0 at once, while its order 10 is still pending, so grunt 1497's search at n = 356 ignores it (V17) | Trace (`xorc01-death-coil`) |
| V43 | `xhuman11-10000` (oracle trace), n = 4443: paladin 1510 (type 0x0C, owner 1) at (88, 76) in order 12 on grunt 1570 at (88, 74); dying grunt 1577 on (88, 75) | Steps north to (88, 75): facing 0, animation 3 (the paladin's search ignores enemy units) | Trace (`xhuman11-10000`) |

### Goal zone and the walk goal

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V39 | Two-tile mover, target building 3 × 3 at (6, 18) | Goal zone: even tiles x 4 to 10, y 16 to 22 | Example |
| V40 | Two-tile mover, no target, G = (22, 8) | Goal zone: even tiles x 20 to 24, y 6 to 10 | Example |
| V49 | A follow (order 6) of a 2 × 2 unit (static study) | Goal zone: the followed unit's footprint widened by one tile, not the 3 × 3 around G | Static study |
| V51 | `xhuman5-30000` (oracle trace, Enhanced, no input), n = 26761: computer grunt 1480 (type 0x01, owner 7) at (102, 108) in order 34 on orc transport 1520 (type 0x1D, owner 7), which is landing (order 30) at (96, 98); walk goal (100, 106), the transport's destination, in the grunt's region (a break capture whose log isn't committed; research notes, "Round 65"); axethrower 1587 (type 0x09, owner 7) at (101, 107). The zone is 1520's widened footprint, so (100, 106) is outside it | The line's first step onto (101, 107) is blocked; the only line tile is (101, 107) (not G), which is blocked, so both walks fail: no route. The no-route restart then gives command 4: order 4, still animation, wait 3, target 1520 in that record (`orders/transports.md`). Counting G as a line tile, a walk succeeds and the grunt steps west to (101, 108) | Trace (`xhuman5-30000`) |
| V52 | As V51 (`xhuman5-30000`), n = 26764: 1480 in order 4 on 1520 since n = 26761; 1520 at (96, 98); the command stored the transport's tile pulled back | The walk's G is (96, 98), 1520's tile (the same kind of break capture): the grunt steps north to (102, 107), facing 0. Aimed at the pulled-back goal it would step west | Trace (`xhuman5-30000`) |
| V53 | A unit in order 6 whose followed unit has moved off the goal the follow command stored (static study) | Its walk aims at the followed unit's current tile, with that unit's widened footprint as the zone | Static study |
| V54 | A unit in order 12 on a building (static study) | Each walk's G is the tile the building goal rule picks on the footprint | Static study |

### Pulling the goal back

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V22 | `xorc7` (oracle trace): submarine 1493 (type 0x26, two-tile) at (36, 64), sea patrol to (6, 18), the top-left tile of a shipyard covering (6..8, 18..20) | Line steps NW, N, NW and so on; (6, 18) is a shipyard tile. Tested: (6, 18) − NW = (7, 19) and − N = (7, 20), shipyard tiles; − NW = (8, 21), water in its region, so the goal is (7, 20) and the search aims at (6, 20). The first 20 steps: NW, N, NW, NW, N, NW, NW, N, NW, NW, NW, N, NW, NW, N, NW, NW, N, NW, NW | Trace (`xorc7`) |
| V23 | As V22: the same shipyard, a submarine at (58, 4) | Line steps W, SW and so on. Tested (7, 18), shipyard, then (8, 17), water: goal (7, 18) | Example |
| V24 | Footman ordered onto a tile of its own region with no building | Goal unchanged | Example |
| V25 | Footman ordered onto an island tile (another region) with sea between (static study only) | The goal becomes the tile tested just before the first tested tile of its own region (a water tile) | Static study |
| V26 | `orc8` (oracle trace), n = 591: computer worker 1505 inside its hall, unit tile (123, 96), is sent to harvest the tree at (124, 90). Line steps N, N, NE, N, N, N. Church 1503 (type 0x3E, 3 × 3) at (124, 91); (124, 90) and (125, 90) forest | Tested: (124, 91) building, (124, 92) building, (123, 93) usable. Stored goal **(124, 92)**. The worker comes out facing 0 (N), toward (124, 92) | Trace (`orc8`) |
| V27 | As V26 (`orc8`): the same command, the line's own tiles walked backwards (wrong) | Would test (124, 91), (124, 92), (124, 93), (123, 94): goal (124, 93), facing 1. Differs from the trace | Example |
| V28 | `orc8`, n = 667: worker 1505 at (123, 93), re-aimed at the tree (124, 90). Line steps NE, N, N | First tested: (124, 90) − NE = (123, 91), usable. Goal kept: **(124, 90)**. n = 670 it steps N; from n = 702 it chops from (123, 91), facing 1 | Trace (`orc8`) |
| V29 | Unit at (0, 0), goal (0, 3), line steps S, S, S; (0, 3), (0, 2) and (0, 1) all unusable | Tested (0, 2) and (0, 1) only (k − 1 = 2). None usable: goal kept (0, 3). In the transport's own pull-back (`orders/transports.md`), (0, 0) + S = (0, 1) | Example |
| V38 | A unit given a spell command (order 38 to 56) with blocked counter 5 (static study only) | Counter 0; the goal is not pulled back | Static study |
| V46 | As V45 (`mp-frosty-fjords-bne-30000`): peasant 1560 (type 0x02, owner 0) at (52, 96) given a shipyard with top-left (67, 92) at n = 16187; the raw goal (67, 92) is unusable | Pull-back: the first tile tested, (66, 92), is unusable; the second, (65, 93), is usable; stored goal (66, 92) | Trace (`mp-frosty-fjords-bne-30000`) |

### Building goal rule and shore buildings

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V44 | Shipyard (3 × 3) with top-left (120, 52); land worker in the same land region as (120, 52) | Goal (120, 52), not the bottom-left corner (120, 54) | Example |
| V45 | `mp-frosty-fjords-bne-30000` (oracle trace, Enhanced): computer peasant 1548 (type 0x02, owner 2) walks in order 28 to build the shipyard of V44 | From (113, 54) at n = 16464 it steps north-east: (114, 53) at n = 16480, (115, 52) at n = 16496, (116, 51) at n = 16512, then east along row 51 to (119, 51) at n = 16560; shipyard 1518 (type 0x48) placed at (120, 52) at n = 16576. Aimed at (120, 54) it would step east to (115, 53) at n = 16496 | Trace (`mp-frosty-fjords-bne-30000`) |
| V47 | Land worker going to build a shore building; a type with the sea or air flag going to build one | Land: corner test as above. Sea or air: the top-left tile, no corner test | Example |
| V48 | `orc9-orc09-shore-build-3000` (oracle trace, Enhanced): archer 1476 (type 0x08, owner 0) at (61, 92) attacks shipyard 1469 (top-left (55, 86), 3 × 3); the top-left (55, 86) isn't in its region, and of the other corners only the bottom-left (55, 88) is | G = (55, 88): the archer walks on until it is within 4 of that tile | Trace (`orc9-orc09-shore-build-3000`) |
| V50 | A shore building build command, no footprint corner in the worker's region (`orc9-orc09-shore-no-corner-600` n = 20: a shipyard at (22, 65), on another island's coast) | No order, nothing paid | Trace (`orc9-orc09-shore-no-corner-600`) |

## Open questions

Carried over:

- Whether two-tile movers' start tiles are always even.
- Classic-mode behaviour when S = G (it isn't short-circuited there).
Closed in round 75:

- **Goal pull-back with other air units** (round 72, `ERRATA-audit-movement.md`, erratum 5; T13). The static study
  finds that the usable test of step 1 ignores only ground-layer units, so for an air unit a tile holding another air
  unit isn't usable. That could matter only when air units stand both on the goal and on the first tile tested, which
  differ in the parity of at least one coordinate. In play air units stand on even tiles: `placement.md` places them
  on even tiles, they move as two-tile movers, and all 324 air units placed in the shipped maps (119 in the
  campaigns, 205 in the skirmish maps; every unit type with the air-unit flag, bit 1, in the default unit data,
  daemons included) stand on even tiles on both axes (game data, round 75; count corrected by the spec review). So
  the two readings never give different goals in play.
  Step 1 stands; the open question is closed as having no effect (`ERRATA-round75.md`, D2).

Closed in round 72:

- **The moving-ally rule's last condition** (was: a state mark must be clear; "not inside something" was a guess).
  It is the frozen state of a held worker, which the placement interface sets in single player too (round 101); see
  "Units seen by the search".

Found by the consolidation (static study, 2026-10-14; the rules above are unchanged):

- **Statements made explicit by the consolidation, static study only:** the search and the arrival test of
  `movement.md` read one walk goal for every kind of goal, differing only in the rounding ("The model", "One walk
  goal"; traced for the cases each rule names); and the command's pull-back and the transport's pull-back follow
  the same steps with two settings, the mask and the fallback when no tile is usable ("The model", "One pull-back, two
  settings"; the traced cases `xorc7`, `orc8` and Human04 agree). The static study found no case where either
  reading changes an outcome.

## History

Erratum rounds that shaped this spec and the sections where their content now lives. The erratum notes for each
round are kept with the research-side specs.

| Round | Content | Now in |
| --- | --- | --- |
| 22 | For two-tile movers the progress check measures P to the unrounded goal; the goal zone plays no part in it | Step 4; V12 |
| 31 | For transports the kept tile of a goal pull-back is defined in `orders/transports.md` | "Goal pulled back", step 5 |
| 35 | Attack-type orders for the search are those with the "acquires" mark (plus order 10 in Enhanced mode); orders 9 and 11 aren't; the target gets no special treatment. New trace vectors (a stored path kept while the target moves; a route into a moving ally refused at the step) | "Units seen by the search"; V13, V14, V16 to V19 |
| 36 | The line counts in tiles for two-tile movers too; the goal pull-back's orders, walk and kept tile; a unit with a command whose pending order hasn't taken over counts as about to move. Two open questions closed, trace vectors added | "Line rule"; "Goal pulled back"; "Units seen by the search"; V20 to V25, V30, V31 |
| 37 | A two-tile mover's goal zone rounds its first row and column down to even and its last up to even; the blocked counter is cleared by every command (`movement.md`), not by the pull-back | Step 1; "Goal pulled back"; V37 to V40 |
| 38 | The searching unit's own tile is free only under the moving-ally case and otherwise blocks the walks; trace vectors for `orc4`, `orc09-repair` and `human10` | "Units seen by the search"; V32 to V36 |
| 39 | The goal pull-back subtracts the line's steps from the goal in their order (the run mirrored from the goal end) and tests k − 1 tiles; the transport case is stated here; `orc8` vectors added and the `xorc7` and `human10` vectors restated in the new terms (same results) | "Goal pulled back"; V22, V23, V26 to V29, V36 |
| 40 | The no-usable-tile fallback to the unit's tile plus the last step belongs to the transport's own pull-back in `orders/transports.md` (any command to a transport on open water with a land or shore-water target); the ordinary pull-back keeps the goal for transports too | "Goal pulled back", step 5; "The model", "One pull-back, two settings" |
| 43 | A recorded attacker that is dying doesn't count for the attack-type rule's "no attacker" test; `xorc01-death-coil` vector | "Units seen by the search"; V15 |
| 50 | The search's spell-caster exclusion is the mage-type units only (mage and death knight types); paladins and ogre mages count as fighters; `xhuman11-10000` vector | "Units seen by the search"; V43 |
| 52 | Step 1 names the boarding walk's transport as a target unit, so its goal zone is the transport's widened footprint | Step 1 |
| 59 | A shore building's build goal (top-left in the worker's region, else the first corner in it, bottom-right first); a build command's goal is pulled back; Step 1's target zone also serves a picked target unit and order 4's walk while it keeps a target unit; `mp-frosty-fjords-bne-30000` vectors | "Shore building"; "Goal pulled back"; Step 1; V44 to V47 |
| 60 | The target goal zone serves every walk whose order holds a target unit (static study; traced for attack-move and order 4); the shore corner goal is also the walk goal of a unit that is neither sea nor air targeting a shore building, and the build goal has no sea or air test; vectors | Step 1; "Shore building"; V48, V49 |
| 62 | A smart move with a target unit that becomes the move stores the target's tile without pulling it back (traced for a tanker); the shore build with no corner is traced | "Goal pulled back"; "Shore building"; V50 |
| 65 | When the line is blocked, the line tiles stop **before** G, which counts only if it is in the goal zone (traced: `xhuman5-30000` n = 26761, a boarding walk aimed at its transport's destination); a walk whose order holds a target unit sets G again from the target, where it stands now, every time it runs, so a command's stored goal and its pull-back last only until the first walk (traced for order 4 on a transport; static study only for other orders and for building targets); the open question on the pull-back with a target unit is closed; the `orc09-repair-600` vector no longer lists G as a line tile (same result); vectors | Step 2; "G for a walk with a target unit"; "Goal pulled back"; V35, V51 to V54 |
| B-038 (2026-10-14) | Consolidated into one model: "The model" added (the search sequence, one walk goal, three tile tests, one line rule, one pull-back); vectors given IDs, a Source column and topic groups; the round citations moved here; the test-vector note says trace rows name their units. No rule changed | Whole spec |
| 72 | Movement audit: the moving-ally rule's last state mark identified (open question closed); the air-unit pull-back reading filed as an open question with a trace request, step 1 unchanged | "Units seen by the search"; "Open questions" |
| 101 | The moving-ally rule's mark is the frozen state of a held worker (`orders/orders.md`, "Holding and releasing workers") | "Units seen by the search"; "Open questions" |
| 75 | The air-unit pull-back question closed by game data (air units stand only on even tiles, so the readings never differ); step 1 unchanged | "Open questions" |
