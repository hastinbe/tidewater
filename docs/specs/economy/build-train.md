# Economy: construction, training, research and food

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the animation scripts in
  `script.bin`, and the public PUD specification text (unit data and upgrade data sections). Not yet confirmed
  against the running game, except the looks and the progress byte of the round-33 errata (oracle traces Human04,
  orc4 and xhuman4).
- **Changes:** 2026-10-11, round-3 errata: a trained unit and a builder coming out are placed by the ring search of
  `movement/placement.md` (limit 12, starting west of the building's top-left tile), replacing "within 12 tiles";
  repair rates and costs now point to `economy/repair.md`. Both open questions are closed.
  2026-10-11, round-4 errata: building payment splits computer players (pay when the AI decides) from every other
  player (pay on arrival), replacing "computer-controlled" and "human-controlled".
  2026-10-11, round-6 errata: research jobs aren't checked against "known" or "being researched" at the command;
  finishing or cancelling research depends on the "being researched" bits (new upgrade row); training requirements
  apply only while enforced; two open questions now point to `economy/tech-tree.md`.
  2026-10-12, round-33 errata: the shown progress of construction and of jobs is kept in the building's mana byte;
  the construction look and a building upgrade are frames (0 and 1).
  2026-10-12, round-35 errata: the build walk's goal is the building goal rule on the planned footprint (oil platform:
  top-left plus (1, 1); shore rule for shore buildings), recomputed each time order 28's logic runs (corrected in
  round 41: it stays fixed while the worker walks).
  2026-10-12, round-39 errata: a builder let out of a finished building faces the building (top-left + (1, 1)) before it
  becomes idle, with trace vectors for SE, NE, E and N exits; new open question on exit facing at cancel or destroy.
  2026-10-12, round-40 errata: a failed placement on arrival makes idle take over in the same update (wait 3) and leaves
  the worker's tile marked; an upgraded tower's order 14 takes over in the same update; `xhuman2` and `human06-tech`
  vectors.
  2026-10-12, round-41 errata: the build walk's goal stays fixed while the worker walks and is worked out again only
  when a walk ends without arriving (the round-35 line is corrected); the oil patch under a new platform is marked used
  and killed, leaves no ruin and its slot is released 513 cycles later; `xorc7` and `orc09-naval` vectors.
  2026-10-12, round-42 errata: order 28's logic runs only on arrival; a build walk ends without arriving only with no
  route (idle at once) or at the blocked counter's limit (idle as a pending order), so the round-41 sentence on working
  the goal out again after such a walk is replaced; the oil patch's wait of 1 comes from its own update; open question
  on the patch's update order.
  2026-10-12, round-43 errata: a finished oil platform gives its tanker order 25 as an immediate order (move animation,
  wait 1), so order 26 follows in the next cycle; `orc09-naval` vectors.
  2026-10-13, round-47 errata: a killed unit leaves the food in use at the start of its death animation; the "ever had a
  hall" mark is set at the start of every new game except Enhanced multiplayer; a build walk's goal is worked out from
  the worker's tile when the command is given; a builder inside a completed hall gets the hall-sized exit mark;
  `xorc4-10000` vector; open question on a Classic builder's delivery.
  2026-10-13, round-48 errata: a building killed while a job runs cancels the job at the kill, with the cancel refund
  and the mana byte set to 0; `human5-10000` vector; the open question on jobs at a building's death is closed.

## Behaviour

Order numbers refer to `orders/orders.md`. "Build time", "cost" and "hit points" are the unit data (UDTA) fields of
the type being made; upgrade time and costs come from the upgrade data (UGRD). Repair is specified in
`economy/repair.md`.

### Costs in the data

- **Unit and building costs** are stored divided by 10: the real gold, lumber and oil costs are 10 × the stored
  values.
- **Upgrade and spell research costs** are stored at full value.

### Can the player afford it?

When a unit, building or building upgrade is started, these checks run in order, and the first one that fails stops
it with its message:

1. **Food** (mobile units only): the owner's food supply must be **greater** than the food in use. In Enhanced mode
   the supply counts as at most **200**.
2. **Gold:** the owner's gold must be at least the gold cost.
3. **Lumber**, then 4. **Oil**, the same way.

Research and upgrades check gold, lumber and oil only.

### Food

- **Supply:** +4 for each completed farm (pig farm), +1 for each completed town hall, great hall, keep, stronghold,
  castle or fortress.
- **In use:** the number of the owner's units that aren't buildings, minus its skeletons, daemons and critters. A unit
  stops counting when it is **killed**, at the start of its death animation, not when it leaves play; the training food
  test and the computer players' farm rule both see the lower count while it dies (traced: `xhuman2-10000`,
  `ai/worker-ai.md` vectors).
- Units still being trained **don't count**. So two buildings can each start a unit when only one food is spare, and
  the player ends up over the supply.

### Placing a building

A worker ordered to build goes to the site in order 28 (arrival radius 0), with no target unit. Its goal is a tile
of the planned footprint, worked out from the planned building's type and top-left tile and the worker's current tile
as if the building already stood there:

- a building placed on an oil patch (an oil platform): the top-left tile plus (1, 1);
- a shore building: the shore rule of `movement/pathfinding.md` ("Building goal rule");
- any other building: the building goal rule of `movement/pathfinding.md` on the planned footprint.

The goal is worked out when the order is given, from the worker's tile **at that moment**; while the worker walks, it
stays as it was. A computer player's worker given its build job as it comes out of a building (`economy/harvest.md`,
"Coming out", step 3) is still at the building's top-left tile then, before it is placed outside, so its goal is worked
out from that tile (traced: `xorc4-10000`, vectors below). The walk runs instead of the order's logic, so the goal isn't
worked out again as the worker's own tile changes, even when the building goal rule would now pick another footprint
tile. Order 28's logic runs only on arrival. A build walk ends without arriving in two ways, both leaving the worker
idle: with **no route**, idle as an immediate order (`orders/orders.md`, "No route"), and at the **blocked counter's
limit**, idle as a pending order (`movement/movement.md`, "Blocked rules", row C). The worker arrives when it **stands
on** the goal tile; the placement check ignores the worker's own tile (below), so that doesn't stop the building.

- **Computer players** (`movement/movement.md`, "Player kinds") pay when the AI decides to build, before the worker
  sets out.
- **Every other player** (human, passive computer, neutral) pays on arrival. The affordability check runs then, and a
  failure shows its message and makes the worker idle. In ordinary games only human players reach this, since
  passive and neutral players don't build.
- The site is then checked for placement (`movement/occupancy.md`, placement masks). The worker's own tile doesn't
  block it. A failure shows a placement message. For a computer player the cost is given back, and the worker becomes
  idle: idle **takes over in the same update** (order 2, still animation, wait 3; `orders/orders.md`, "Same-update
  take-over"), not as an immediate order, and the worker's tile stays marked as occupied by it.
- On success, the building is created under construction (below), the cost of a player that pays on arrival is
  taken, and the worker **goes inside** the building, in order 22, the same way a worker enters a mine
  (`economy/harvest.md`, step 2 of "Entering").
- **Oil platform:** the oil patch under the new platform is marked used and killed. It leaves no ruin: in the same
  cycle its order becomes "remove" (order 0, animation 0, wait 1; its type and other fields kept), and its slot is
  released 513 cycles later, as for any removed unit (`core/corpses.md`). The patch's wait of 1 comes from its own
  update, which follows the builder's in that cycle; after that the wait keeps counting down (0 at the next cycle,
  then 255).

### Construction

A building placed during play starts **under construction**. (Buildings from the map start complete, at full hit
points.)

**At creation**, with M = the type's hit points (at least 1) and T = its build time:

- **Starting hit points:** H₀ = min(floor(M / 10), 40).
- **Hit points still to add:** D = M − H₀.
- **Fast first hall (Enhanced multiplayer only):** if the owner has never had a completed town hall, keep or castle (of
  either race), a gold-storing building's T is lowered to the same race's farm build time, if that is shorter. In
  Enhanced mode T is at least 1. The owner's **ever had a hall** mark records this: it is set for every player at the
  start of a new game except in Enhanced multiplayer (`game/setup.md`), so this rule applies only there, until the
  player's first hall is completed.
- The owner's "buildings under construction" count goes up by 1.
- The building's order is 33 (its resting order). The construction itself isn't an order: it is advanced by a
  separate pass.

**The construction pass** runs once per cycle in phase 4 of `core/timing-and-rng.md`, after the missiles and before
the timers pass. It visits units in slot order. For each building under construction (not dying or dead):

1. A pass counter P (0 at creation) goes up by 1. Nothing else happens unless P is a multiple of 12. With the
   build-speed cheat on, P goes up by 12 instead, so every pass is a step.
2. Let the total be 12 × (T + 1).
3. **If P has reached the total:**
   - if P equals the total exactly, the progress shows as full and nothing else happens;
   - if P is past it, the building is **completed** (below).
4. **Otherwise, a step:** the building gains hit points so that after k steps it has received floor(k × D / T) of
   them (the remainder carries over between steps). Then:
   - if its hit points have reached M, they are set to M, and P jumps to the total.
5. The shown progress is the larger of P / total and hit points / M, as a fraction of 255.
6. **Look:** below 64/255 the building shows the first construction-site frame (frame 0), and from 64/255 the second
   (frame 1). From 128/255 the building's own picture replaces the construction site; the frame stays 1. The shown
   progress (0 to 255) is kept in the building's mana byte at every construction step. On completion the frame and
   the mana byte go back to 0.

An undamaged building reaches M at step T, so it completes on pass **12 × (T + 1)**, counting the creation cycle as
pass 1. A building that took damage during construction doesn't reach M. It shows full progress at step T + 1 and
completes 12 passes later, with its hit points still below M.

**Completion:**

- The building becomes completed: it plays its still animation, the under-construction count goes down by 1, and it
  now counts for food, bonuses, depots and requirements (see `economy/harvest.md` for the delivery bonuses).
- Every unit inside it (the builder) goes through delivery (`economy/harvest.md`, "Delivering goods"): it gets any goods
  it carries delivered if the building is a depot for them, and a builder of a town hall, keep or castle gets the
  **hall-sized exit mark**, carrying or not (`economy/harvest.md`, "Coming out", step 6). Being let out here neither
  uses nor clears the mark. Then:
  - a **tanker** (the builder of an oil platform) gets order 25 as an **immediate order** (`orders/orders.md`, "Pending
    orders and immediate orders"): order 25's animation (3, move) starts with a wait of 1, and the order's logic runs in
    the next cycle, which takes the tanker into the new platform (order 26) to collect oil;
  - any other unit is placed on a free tile around the building, faces the building ("Facing a tile",
    `core/animation-scripts.md`, with B the building's top-left tile + (1, 1)) and becomes idle: order 2, still
    animation, wait 1, target cleared. The tile is found by the ring search of `movement/placement.md` (limit 12,
    starting from the tile west of the building's top-left tile; the number of rings depends on the building's size).
    If no tile is found, it **dies**. From the tile west of the top-left tile the builder faces SE (3); other exit
    tiles give other facings by the same rule. A tanker set to order 25 doesn't turn. Only completion is confirmed
    by traces; whether a builder let out of a cancelled or destroyed site also turns is an open question.
- A completed **wall** piece (human or orc) is removed as a unit and becomes wall terrain (`movement/occupancy.md`,
  "Walls").

### Cancelling construction

Cancelling a building still under construction destroys it and refunds part of its cost. The refund per resource is
floor(stored cost × 30 / 4), which is 75 % of the real cost, rounded down.

| Mode | Refunded |
| --- | --- |
| Classic | Gold and lumber (not oil) |
| Enhanced | Gold, lumber and oil |

Any builder inside is let out by the same ring search (or dies if there is no room), as on completion.

### Training, research and building upgrades

A completed building that isn't already working can start **one** job at a time. There is no queue.

| Job | Valid when | Time (progress steps) | Paid |
| --- | --- | --- | --- |
| Train a unit | the unit type is made by this building type (a keep or castle trains what a town hall does) and, while requirements are enforced, its requirements are met (`economy/tech-tree.md`) | 2 × the unit's build time | unit costs × 10 |
| Research a spell | this building type researches it (`economy/tech-tree.md`); the command doesn't check whether it is known or being researched | 2 × the upgrade time | upgrade costs |
| Weapon, armour or similar upgrade | made by this building type; for two-level upgrades, the player is at the right level | 2 × the upgrade time | upgrade costs |
| Upgrade the building (town hall to keep, keep to castle, scout tower to guard or cannon tower, and the orc versions) | the target type is an upgrade of this building type | 2 × the new type's build time | new type's costs × 10 |

**Starting:** the cost is taken at once. The building's order becomes 37 (working), its progress counter is set
to 0 and its mana byte to 1. Starting a **building upgrade** also sets the frame to 1 and shows the new type's
picture, while the type stays the old one until the upgrade ends. Training also raises the owner's "units in
training" count.

**Progress:** the working order's logic runs once per loop of the building's work animation, and each run adds 1 to
the counter (with the build-speed cheat, up to 10 at a time). Building animation timing (from the scripts):

- Work animation: a 10-cycle wait, then a loop with one "step done" every 5 cycles.
- Still animation: one "step done" every 5 cycles.

So a job of N steps runs N + 1 times: the first run is 13 cycles after the job takes over (the 3-cycle first wait,
then the 10-cycle wait), then every 5 cycles. The job ends on the run after the counter reaches N, which is
**13 + 5 × N cycles** after it took over. The job takes over at the still animation's next "step done", up to 5
cycles after the command.

After each run that adds to the counter, the building's mana byte holds floor(counter × 255 / total), its shown
progress (for a total above 255 that can be 0 again after the start's 1).

**Ending:** the order returns to 33 and the progress counter and mana byte to 0. A building upgrade, finished or
cancelled, sets the frame back to 0 (cancelled: the old picture returns). Then:

| Job | Finished | Cancelled |
| --- | --- | --- |
| Train | The unit is created next to the building, on the first tile found by the ring search of `movement/placement.md` (limit 12, starting from the tile west of the building's top-left tile). If it can't be placed (no room, or the unit table is full), the player gets a message and a **full refund** | Full refund |
| Research a spell | The spell becomes known, if its "being researched" bits are still set (`economy/tech-tree.md`) | Full refund, on the same condition |
| Upgrade | The level in its line goes up by 1, on the same condition | Full refund, on the same condition |
| Building upgrade | The type changes, and the hit points go up by (new type's M − old type's M). Towers get order 14, which takes over in the same update (animation 4, wait 3) | Full refund |

In every case the "units in training" count (for training) goes back down.

**A building killed during a job.** When a building is killed while a job runs, the job is cancelled at the kill, as in
the "Cancelled" column above: the cost is refunded in full (for research and upgrades, on the same condition), and the
order's progress counter and mana byte go to 0 in the same cycle. Traced for training (`human5-10000`, vectors below);
the other job kinds follow the same rule but no trace shows them.

## Data

| Constant | Value |
| --- | --- |
| Cost scale (units and buildings) | stored value × 10 |
| Starting hit points under construction | min(floor(M / 10), 40) |
| Construction pass step | every 12 passes |
| Construction length | 12 × (T + 1) passes when undamaged |
| Cancel refund | floor(stored cost × 30 / 4) = 75 % rounded down |
| Training / research steps | 2 × time |
| Building work loop | first run 13 cycles after take-over, then every 5 cycles |
| Food per farm / per hall-type building | 4 / 1 |
| Enhanced food cap | 200 |

Units not counted as using food: skeleton, daemon, critter.

## Test vectors

The values of M and T in these vectors are examples, not quoted from the game data.

| Case | Expected |
| --- | --- |
| Building with M = 400, T = 100 placed | starts at 40 HP; D = 360 |
| Same, after 1, 50 and 100 steps | 43, 220, 400 HP |
| Same, undamaged | completes on pass 1212 (12 × 101) |
| Same, takes 10 damage at step 30 | at step 100 it has 390 HP; full progress at step 101 (pass 1212); completes on pass 1224 with 390 HP |
| Building with M = 1200, T = 255 | starts at 40 HP (120 capped to 40); D = 1160; completes on pass 3072 |
| Building with M = 60 | starts at 6 HP |
| Cancel a building whose stored costs are gold 50, lumber 25, oil 10 (real 500 / 250 / 100), Classic | +375 gold, +187 lumber, +0 oil |
| Same, Enhanced | +375 gold, +187 lumber, +75 oil |
| Unit with build time 60 | 120 steps; done 13 + 600 = 613 cycles after the job takes over |
| Player with 2 farms and a town hall (supply 9) and 8 non-building units | can start a unit (9 > 8) |
| Same player with 9 units, 2 of them skeletons | can start a unit (9 > 7) |
| Same player with 8 units, two barracks each start a footman | both start; after training, 10 units on a supply of 9 |
| Enhanced, 60 farms and a hall (supply 241) | counts as 200 |
| A trained unit can't be placed | message; costs fully refunded |
| Keep (old M) upgraded to castle (new M), keep damaged to h HP | castle with h + (new M − old M) HP |
| Enhanced multiplayer, first town hall of a player who never had one | built in the farm's build time if shorter |
| Human04 (oracle trace), great hall slot 1550 trains a peon (total 90) | Mana 1 from n = 21 (the payment). After the runs at n = 39, 44, 49, …: 2, 5, 8, 11, 14, 17, 19, … 252, 255 (n = 484); 0 at n = 489, when the peon appears. Frame 0 throughout |
| orc4 (oracle trace), farm slot 1534 under construction | Mana 27 at n = 10714, rising with hit points (41 at 10786); 64 and frame 1 at n = 10906; frame 1 to completion; frame 0 and mana 0 at n = 11914 (hit points 400) |
| xhuman4 (oracle trace), tower slot 1423 (type 0x41) | Frame 0 at completion (n = 821), frame 1 from the building-upgrade start (n = 832) through order 37, frame 0 when it becomes type 0x61 (n = 2250) |
| `human01-basics` (oracle trace): peasant 1596 on (14, 9) is told to build a barracks (type 60, 3 × 3) at top-left (5, 8) | Goal (7, 9): x = 7 because 14 ≥ 7; y = 8 + 1 because 9 lies inside 8 to 10. Order 28 takes over at n = 19 (wait 3); the peasant walks west along row 9, one tile every 16 cycles: (13, 9) at n = 22, (12, 9) at 38, (10, 9) at 70, (7, 9) at 118. At n = 134 the barracks is placed at (5, 8) and the peasant is inside it (order 22, wait 150) |
| A worker at (2, 2) told to build a 2 × 2 farm at (5, 5) | Goal (5, 5) |
| A worker at (6, 9) told to build a 3 × 3 building at (5, 5) | Goal (6, 7): x = 5 + 1 (6 lies between 5 and 7), y = 7 (9 ≥ 7) |
| `xorc7-7000` (oracle trace), n = 2028: peasant 1421 (type 0x02, owner 1) in order 28, at (95, 118), building a farm with top-left (96, 118); goal (96, 119), worked out when the order was given | It steps south-east to (96, 119), facing 3, toward the stored goal (working the goal out again from (95, 118) would give another tile) |
| `orc09-naval-1500` (oracle trace), n = 268: a platform (slot 1468) placed on the oil patch in slot 1502 at (57, 75) | Same cycle: patch order 0, animation 0, wait 1, type 0x5D; its slot is released at n = 781 (268 + 513) |
| `human06-tech` (oracle trace), n = 178: farm 1521 (type 0x3A, top-left (76, 5)) completes; builder 1586 | 1586 at (75, 5), facing 3 (SE, toward (77, 6)), order 2, wait 1, target null |
| `human7` (oracle trace), n = 1251: farm 1465 (type 0x3B, top-left (63, 2)) completes; builder 1582 | 1582 at (62, 2), facing 3, order 2, wait 1, target null |
| `xorc3` (oracle trace), n = 1295: farm 1475 (type 0x3B, top-left (27, 113)) completes; builder 1482 | 1482 at (26, 113), facing 3, order 2, wait 1, target null |
| `human06-tech` (oracle trace), n = 202: builder 1528 let out at (16, 82) from a farm with top-left (17, 80) | dx 2, dy −1 to (18, 81): facing 1 (NE) |
| `orc09-naval` (oracle trace), n = 249: builder 1587 let out at (80, 7) from a farm with top-left (81, 6) | dx 2, dy 0 to (82, 7): facing 2 (E) |
| `xhuman2` (oracle trace), n = 3438: builder 1588 let out at (21, 4) from a tower (type 0x41) with top-left (20, 2) | dx 0, dy −1 to (21, 3): facing 0 (N) |
| `orc09-naval` (oracle trace), n = 468: oil platform 1468 completed with tanker 1473 inside | Tanker: order 25, animation 3, wait 1, target 1468, facing kept (2) |
| Same, n = 469 | Tanker: order 26, still animation, wait 150 |
| `xhuman2` (oracle trace), n = 85: computer peon 1562 (type 0x03, owner 5) at (65, 59) in order 28 arrives and the site check fails | Order 2, animation 2, wait 3; its tile stays marked (ogre 1549 is blocked by it at n = 113). n = 88: order 28 again (the computer gives the job again) |
| `human06-tech` (oracle trace), n = 578: building 1516 at (72, 4) finishes its upgrade from type 0x40 to 0x60 | The same record: type 0x60, order 14, animation 4, wait 3 |
| `xorc4-10000` (oracle trace): computer peasant 1580 (type 0x02, owner 2) comes out of 1565 (top-left (26, 11)) at n = 7963, at (25, 12), with a build job for a 3 × 3 site at (14, 11) | Its walk goal, worked out from (26, 11), is (16, 11), not (16, 12): it walks row 12 to (20, 12), steps north-west to (19, 11) at n = 8071, reaches (16, 11) at n = 8119 and is in order 22 at n = 8135 |
| `human5-10000` (oracle trace), n = 8086: great hall 1525 (type 0x4A, owner 0) is killed while training (order 37, mana byte 70) | Same cycle: order 1, player 0's gold 300 to 700 (the 400 paid for the unit refunded), mana byte 0 |

## Open questions

The requirement rules, the setting that turns them off and the research jobs' effects are in `economy/tech-tree.md`.

- Whether eyes of Kilrogg and other summoned units count as food in use.
- **Upgrade picture.** Which frame of the new type's graphics a building upgrade shows with frame 1 (a construction
  stage of the new type, or something else) wasn't checked. Default: frame 1 of the new type's picture.
- **The oil patch's update order.** A patch updated before its builder in a cycle would show order 1 for that cycle
  and be removed in the next. No trace has that order.
- **Exit facing on cancel or destroy.** A builder let out at completion faces the building. Whether one let out of a
  cancelled or destroyed site also turns wasn't checked. Default: it turns the same way.
- **Classic-mode builder's delivery.** Whether a builder in gold or lumber mode with empty hands gets a load delivered
  when its building is completed in Classic mode (the static reading says yes, as for a Classic delivery on entering)
  isn't separated by any trace.
