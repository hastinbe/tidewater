# Economy: construction, training, research and food

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the animation scripts in
  `script.bin`, and the public PUD specification text (unit data and upgrade data sections). Not yet confirmed
  against the running game.
- **Changes:** 2026-10-11, round-3 errata: a trained unit and a builder coming out are placed by the ring search of
  `movement/placement.md` (limit 12, starting west of the building's top-left tile), replacing "within 12 tiles";
  repair rates and costs now point to `economy/repair.md`. Both open questions are closed.

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
- **In use:** the number of the owner's units that aren't buildings, minus its skeletons, daemons and critters.
- Units still being trained **don't count**. So two buildings can each start a unit when only one food is spare, and
  the player ends up over the supply.

### Placing a building

A worker ordered to build goes to the site in order 28 (arrival radius 0).

- **Computer-controlled players** pay when the AI decides to build, before the worker sets out.
- **Human-controlled players** pay on arrival. The affordability check runs then, and a failure shows its message and
  makes the worker idle.
- The site is then checked for placement (`movement/occupancy.md`, placement masks). The worker's own tile doesn't
  block it. A failure shows a placement message. For a computer player the cost is given back, and the worker becomes
  idle.
- On success, the building is created under construction (below), the human player's cost is taken, and the worker
  **goes inside** the building, in order 22, the same way a worker enters a mine (`economy/harvest.md`, step 2 of
  "Entering").

### Construction

A building placed during play starts **under construction**. (Buildings from the map start complete, at full hit
points.)

**At creation**, with M = the type's hit points (at least 1) and T = its build time:

- **Starting hit points:** H₀ = min(floor(M / 10), 40).
- **Hit points still to add:** D = M − H₀.
- **Fast first hall (Enhanced multiplayer only):** if the owner has never had a completed town hall, keep or castle
  (of either race), a gold-storing building's T is lowered to the same race's farm build time, if that is shorter. In
  Enhanced mode T is at least 1.
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
6. **Look:** below 64/255 the building shows the first construction-site frame, and from 64/255 the second. From
   128/255 the building's own picture replaces the construction site.

An undamaged building reaches M at step T, so it completes on pass **12 × (T + 1)**, counting the creation cycle as
pass 1. A building that took damage during construction doesn't reach M. It shows full progress at step T + 1 and
completes 12 passes later, with its hit points still below M.

**Completion:**

- The building becomes completed: it plays its still animation, the under-construction count goes down by 1, and it
  now counts for food, bonuses, depots and requirements (see `economy/harvest.md` for the delivery bonuses).
- Every unit inside it (the builder) gets any goods it carries delivered (if the building is a depot for them).
  Then:
  - a **tanker** (the builder of an oil platform) is set to order 25, so it starts collecting oil from the new
    platform at once;
  - any other unit is placed on a free tile around the building and becomes idle. The tile is found by the ring
    search of `movement/placement.md` (limit 12, starting from the tile west of the building's top-left tile; the
    number of rings depends on the building's size). If no tile is found, it **dies**.
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
| Train a unit | the unit type is made by this building type (a keep or castle trains what a town hall does) and its requirements are met | 2 × the unit's build time | unit costs × 10 |
| Research a spell | not already being researched by this player | 2 × the upgrade time | upgrade costs |
| Weapon, armour or similar upgrade | made by this building type; for two-level upgrades, the player is at the right level | 2 × the upgrade time | upgrade costs |
| Upgrade the building (town hall to keep, keep to castle, scout tower to guard or cannon tower, and the orc versions) | the target type is an upgrade of this building type | 2 × the new type's build time | new type's costs × 10 |

**Starting:** the cost is taken at once. The building's order becomes 37 (working), and its progress counter is set
to 0. Training also raises the owner's "units in training" count.

**Progress:** the working order's logic runs once per loop of the building's work animation, and each run adds 1 to
the counter (with the build-speed cheat, up to 10 at a time). Building animation timing (from the scripts):

- Work animation: a 10-cycle wait, then a loop with one "step done" every 5 cycles.
- Still animation: one "step done" every 5 cycles.

So a job of N steps runs N + 1 times: the first run is 13 cycles after the job takes over (the 3-cycle first wait,
then the 10-cycle wait), then every 5 cycles. The job ends on the run after the counter reaches N, which is
**13 + 5 × N cycles** after it took over. The job takes over at the still animation's next "step done", up to 5
cycles after the command.

**Ending:** the order returns to 33 and the progress counter to 0. Then:

| Job | Finished | Cancelled |
| --- | --- | --- |
| Train | The unit is created next to the building, on the first tile found by the ring search of `movement/placement.md` (limit 12, starting from the tile west of the building's top-left tile). If it can't be placed (no room, or the unit table is full), the player gets a message and a **full refund** | Full refund |
| Research a spell | The spell becomes known | Full refund |
| Building upgrade | The type changes, and the hit points go up by (new type's M − old type's M). Towers get order 14 | Full refund |

In every case the "units in training" count (for training) goes back down.

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

## Open questions

- The exact requirement rules per unit type (which upgrades or buildings each needs), and the flag that turns them
  off.
- Whether eyes of Kilrogg and other summoned units count as food in use.
- What the working order's logic does with a building that is destroyed mid-job (no refund is expected).
- The research jobs' effects in detail (each upgrade's bonus is in `combat/damage.md`; spells are a later spec).
