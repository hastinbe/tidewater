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
  2026-10-12, round-38 errata: the tree tile is the pulled-back goal of the harvest command, and every new aim of a
  lumber worker is a new harvest command that takes over in the same update.
  2026-10-12, round-39 errata: a unit with the peon flag in order 23 (either gather mode) whose route search finds no
  route runs the nearest-tree search and is given a new harvest command to the tree found; `human5` and `orc8` vectors;
  open question on its untraced details.
  2026-10-12, round-40 errata: a unit coming out with a build order aims the ring search at the site's top-left tile;
  only the peon flag runs the no-route tree search, so tankers go idle; round-40 vectors; open question on the exit aim
  for other orders.
  2026-10-12, round-41 errata: a return (order 24) with no route goes idle at once and its idle logic (for computer
  players, the job choice) runs in the same update; coming out, the ring search aims at the goal the new command set (a
  building's top-left tile for a build, return or mine harvest; the tree tile; with no command, the unit's own stored
  tile; static study only, untraced except for build); round-41 vectors.
  2026-10-12, round-42 errata: a worker coming out with a build job faces its build goal tile (building goal rule from
  its exit tile); the exit aim at the commanded goal is supported by the trace set and no longer marked static study
  only (open question closed); `xorc3` vector.
  2026-10-12, round-43 errata: felling's return command clears the blocked counter, as every command does (stated
  explicitly); round-43 vectors (`orc8`).
  2026-10-12, round-44 errata: after the no-route re-aim to a tree, order 23's logic runs again in the same update, so a
  worker next to the new tree starts chopping at once; the round-43 `orc8-10000` vector's evidence corrected (worker
  1501 fells its tree at n = 8036); `orc4-40000` vector.
  2026-10-13, round-46 errata: after a no-route re-aim to a tree that isn't adjacent no move starts in that update
  (default reading, open question).
  2026-10-13, round-47 errata: the hall-sized exit is a mark set by every delivery at a gold-accepting building (a
  completed hall's builder included) and cleared by the next come-out; order 23 at an unfinished mine or platform, or
  with its target gone, gives a pending idle; the build-job facing on coming out and the walk goal can differ; the
  re-aim open question is closed (static study); round-47 vectors.
  2026-10-13, round-48 errata: a lumber worker that finds its tree gone and no other tree goes idle through a pending
  order (frame kept, wait 3); every entry into a gold-accepting building sets the hall-sized exit mark, gather mode none
  included, and a ship's delivery never does; `xhuman2-10000` vector; the no-route open question updated.
  2026-10-13, round-49 errata: a tanker's harvest command on an oil platform placed in the same cycle, before the oil
  patch's own update, fails and leaves the tanker idle; `orc12-10000` and `xhuman8-10000` vectors; open question on the
  cause.
  2026-10-13, round-50 errata: a worker coming out with a computer player's repair job aims at the building's top-left
  tile and faces the building; `human8-10000` vector; open questions on the same-cycle platform's slot order and other
  AI jobs.
  2026-10-13, round-51 errata: a worker coming out with a build job faces the build goal worked out when the command was
  given, while it was still inside, not a goal worked out again from its exit tile; `xhuman9-10000` vector; the
  synthetic build-job vector names the hall's top-left tile.
  2026-10-13, round-52 errata: after the no-route tree search, a worker not within reach of the new tree gets the
  harvest command again, so its goal is pulled back a second time; a worker coming out with a return job is placed
  toward the pulled-back goal, still facing the depot; `xhuman12-10000` and `human13-10000` vectors.
  2026-10-13, round-54 errata: a collapse also clears the target of the unit that took the last trip (wait 1, out at its
  next step done); `human12-25000` vector.
  2026-10-13, round-56 errata: a worker with no route and no tree goes idle as an immediate order and its idle logic
  (for a computer player, the job choice) runs in the same update; `orc11-25000` vector.
  2026-10-13, round-58 errata: coming out toward a gold mine, the exit search aims at the mine's top-left tile
  pulled back; the worker still faces the top-left tile plus (1, 1); round-58 vectors.
  2026-10-13, round-60 errata: coming out with a build job aims the exit search at the stored build goal (the
  pulled-back goal, or the shore corner), and the worker faces the same tile (static study only).
  2026-10-13, round-62 errata: return goods given a unit takes the target as given (any carrying worker or
  tanker, no owner or depot test), and gold taken to a wood-only depot is lost; a lost or busy depot re-issues
  return goods at once as a pending order; entering drops a dying, gone or unseen target first, so a unit
  stepping into a dying building dies; round-62 vectors.
  2026-10-13, round-63 errata: a worker delivers only at a building that stores gold or wood; at any other building
  (an oil-only one included) it goes inside like a mine or platform, keeps its load, and the building keeps frame 2;
  coming out lowers the return-to source's count whatever it comes out of; coming out always makes idle the pending
  order (wait 3 when it takes over); stepping in faces a non-building target's own tile; command 24's target unit gets
  the command check ("inside something" fails it: pending idle, no depot search); order 24's reaction runs only when
  its walk has ended (static study only); entering's target check is detection, not fog (static study only); the
  no-depot return is traced; round-63 vectors.
  2026-10-13, round-64 errata: order 24 drops a target that is dying, gone, inside something or undetected at the next
  step boundary and gives return goods to the nearest depot (traced for a target gone inside); with no route its own
  logic doesn't run; a target already inside something on arrival is dropped by order 25's target check, which counts
  "inside something" (static study only); a tanker in order 23 with no target walks to its goal and then gets a pending
  idle; coming out, the harvest command goes to the return-to source whatever it has become, so a tanker whose platform
  is gone comes out toward it in order 23 with no target; round-64 vectors.

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
   - Otherwise the mode becomes **lumber**, and the command's goal tile, after the pull-back of
     `movement/pathfinding.md` ("Goal pulled back"), is kept as the tree to cut, with no target unit. The pull-back
     runs whoever gives the command (a player, the computer job choice, or the order itself, below), so the tree tile
     is usually a forest tile nearer the worker, and it need not be a forest tile at all: it can be a building tile.
3. **Tanker:** if the target is an **oil platform of the tanker's own side** and the tanker isn't carrying, the platform
   becomes the target. Otherwise the target unit is cleared. A platform placed in this same cycle, while the oil patch
   under it hasn't yet had its own update in this cycle, doesn't count: the target is cleared and the tanker goes idle
   (order 2, still animation, wait 3, target null). In a later cycle the same command works as above (traced:
   `orc12-10000`, `xhuman8-10000`, round-49 vectors; the reason is open).
4. The order becomes 23 (harvest).

Mines aren't owned, so any worker can mine any gold mine. A worker already carrying goods is never put in gold mode.

### Gold and oil: going to the source

While in order 23 with a target mine or platform:

- If the target is gone (dying, dead, or inside something), idle (order 2) becomes the unit's **pending** order and
  takes over in the same update (still animation, wait 3; `orders/orders.md`, "Taking over"). The idle logic doesn't run
  in that update; it first runs at the still animation's next step done, so a computer player's job choice comes then
  (static study only).
- If the unit hasn't arrived yet (arrival radius 1, `movement/movement.md`), it moves toward the target. If its route
  search finds no route, it looks for the nearest tree ("No route" under "Lumber"), although it is in gold mode. This is
  the only case here that makes an immediate idle (when no tree is found).
- On arrival, if the mine or platform is **completed**, its order becomes 25 (enter). A mine or platform that isn't
  completed (still under construction) makes idle the unit's **pending** order instead, taken over in the same update as
  above (wait 3), with the idle logic first running 3 cycles later (traced: `xhuman3-10000`, vectors below).

**A tanker with no target.** A tanker in order 23 with **no** target (its command's target was cleared, "Starting to
harvest", step 3) walks to the command's goal tile like any walking order. Its logic runs only when it gets there, and
then finds no target: idle becomes the pending order, as for a target that is gone (still animation, wait 3; traced:
`xhuman5-30000` n = 26447 to 26642, round-64 vectors).

### Entering a mine, platform or depot (order 25)

Order 25 can't be interrupted by new orders (see `orders/orders.md`).

1. **First time the order's logic runs:**
   - The unit turns to face the tile one in from the building's top-left corner (top-left tile + (1, 1)); if the
     target is not a building (command 24 takes any unit as given, "Returning goods"), it faces the target's own tile
     (traced: `orc9-orc09-return-nodepot-2200` n = 1196, a peon stepping into another peon; round-63 vectors).
   - It is taken off the occupancy marks.
   - A **land worker** moves one tile in that direction, playing its move animation; the order's logic runs again
     when that move loop ends.
   - A **tanker** moves two tiles and goes in immediately, in the same cycle (step 2 follows at once).
2. **Going in:**
   - The unit becomes **inside** the building: it is hidden, untargetable, its attacker is forgotten, and its tile
     position becomes the building's position.
   - Its order becomes 26 (inside) at once, and its wait is set to **150 cycles**.
   - If the unit is a worker and the building stores gold or wood, or the unit is a tanker and the building isn't an
     oil platform, the goods are **delivered** (below), and nothing else happens, even if the building doesn't accept
     what the unit carries: the delivery table then adds nothing and the load is lost (traced for gold at a lumber
     mill: `orc9-orc09-return-goods-2200` n = 1482, round-62 vectors; static study only for the other cases). A
     tanker that enters its own oil platform takes the branch below and delivers nothing (traced: same trace,
     n = 1287), and so does a worker at a building that stores neither gold nor wood (an oil-only building such as a
     shipyard included): it keeps its load, and the building keeps frame 2 from then on (traced:
     `orc9-orc09-enter-nothing-1900` n = 910 at an orc blacksmith and n = 1367 at an orc shipyard, round-63 vectors).
   - Otherwise (a mine or platform, or any building where a worker doesn't deliver):
     - the building's occupant count goes up by 1. If its frame is 0, the frame becomes 1 for a gold mine (type
       0x5C) and 2 for any other building (an oil platform, or a building that stores neither gold nor wood): its
       "active" look. Otherwise the frame doesn't change. Every building starts with frame 0 and an occupant count
       of 0, and a building entered by the delivery branch above keeps its frame;
     - if the building has trips left, the unit is now **carrying**, and the building loses one trip. If that was the
       last trip, the building **collapses** (below). Otherwise the building becomes the unit's return-to source.
     - A building with no trips (one that stores nothing or only oil) gives nothing: the unit keeps its load and its
       return-to source. Its count and frame are **never lowered** again, since coming out lowers only the return-to
       source's count ("Coming out", step 2), so such a building keeps frame 2. A building that stores nothing but has
       trips doesn't exist in the game's data (static study only).
3. **Target check first.** Each time the logic runs, before steps 1 and 2, the target is dropped if it is **dying or
   gone**, **inside something** (a unit target in a mine, platform, depot or transport; static study only), or no
   longer **detected** by the unit's player: the detection test of `orders/orders.md` ("Terms"); fog
   and explored tiles play no part (the detection part is static study only). A building in its
   death animation is already lost to an entering unit. Then, with no target: a unit that already stepped in (step 1
   done) **dies**, and its target shows as none; otherwise idle becomes the pending order (traced:
   `orc9-orc09-enter-dying-1800` n = 1441, round-62 vectors; also the uncommitted `xorc8-30000` n = 19330, a
   peasant stepping into a gold mine that had collapsed).

### Coming out (order 26)

When the 150-cycle wait ends and the inside order's logic runs:

1. The unit becomes idle (order 2) at once, idle is also made its **pending** order, and its goal is set to its own
   stored tile.
2. If it is carrying, its return-to source exists, and that source is a gold mine or oil platform (whatever building or
   unit it is coming out of), the source's occupant count goes down by 1: if the count is 0 it counts as 1, and when it
   reaches 0 the source's frame becomes 0 (the "active" look ends). Both frame changes happen at the moment of going
   in or coming out, in that unit's update. So a carrying worker coming out of a depot, a building that stores
   nothing or another unit lowers its mine's count, and the building it comes out of keeps its count and frame
   (traced: `orc9-orc09-enter-nothing-1900` n = 1060, out of an orc blacksmith; `orc9-orc09-return-nodepot-2200`
   n = 1368, out of a peon; round-63 vectors).
3. A **computer player's** unit (`orders/orders.md`, "Terms") hands control to the AI here (a later spec). If the AI
   gives it an order, steps 4 and 5 are skipped. Every other player's unit goes straight on to steps 4 and 5.
4. If it is **carrying**, it looks for the nearest depot (below) and moves there in order 24 (return goods). If there
   is none, it stays idle, and idle is its pending order, which takes over when the 25-cycle wait ends (wait 3).
   The depot search uses the unit's tile, which is still the building's top-left tile here (the unit is placed only
   in step 6), so a worker coming out of a shore building whose top-left tile is in another region than every depot
   finds none (traced: `orc9-orc09-enter-nothing-1900` n = 1517 and 1542, round-63 vectors, a peon coming out of a
   shore shipyard idle with no target although it is placed on land; which tile the search reads is static study).
5. Otherwise, if it has a **return-to source**, it forgets it and heads back there with a harvest command (so a
   worker that just delivered goes straight back to its mine or tree). The harvest command is given whatever the
   source has become: a tanker whose platform is gone gets order 23 with no target ("Starting to harvest", step 3),
   aimed at the platform's top-left tile pulled back (step 6), and so comes out on that side (traced: `xhuman5-30000`
   n = 26422, a computer tanker inside a shipyard whose platform collapsed meanwhile, round-64 vectors; a computer
   player's unit whose AI gives an order in step 3 is static study only).
6. Its wait is set to **25 cycles**, and it is placed on a free tile next to the building, found by the ring search of
   `movement/placement.md` (limit 12). The search's source is placed at the building's top-left tile and sized as for a
   town hall (4 × 4) if the unit has the **hall-sized exit mark**, which this come-out then clears, and as for a gold
   mine (3 × 3) otherwise. The mark is set by every delivery at a building that accepts gold ("Delivering goods"), and
   it is carried until the unit's next come-out from **any** building, a mine, a platform or a depot (traced:
   `orc5-10000`, vectors below). It is aimed at the tile the unit is heading for, so the unit comes out on that side.
   That tile is the goal its new command set in steps 3 to 5, as any command sets it: for a **build** order (a computer
   player's job), the **build goal the command stored**: the building goal (for a shore building the shore
   corner goal), pulled back (`economy/build-train.md`, "Placing a building"; static study only: no trace
   separates this from the site's top-left tile, the aim given before round 60); for a return, the depot's top-left tile
   **pulled back** as for any command to a building (`movement/pathfinding.md`, "Goal pulled back"; traced:
   `human13-10000`, round-52 vectors), though the worker still faces the depot's top-left tile plus (1, 1); for a
   harvest at a gold mine (or another source that is a unit), the source's top-left tile **pulled back** in the same
   way, though the worker still faces the source's top-left tile plus (1, 1) (traced: `mp-rose-petal-bne-10000`,
   `mp-gold-rush-bne-6000`, round-58 vectors); for a harvest at a tree, the tree tile (already the pulled-back goal);
   for a **repair** (a computer player's job), the building's top-left tile, and the worker then faces the building as
   any come-out faces its target (traced:
   `human8-10000`, round-50 vectors). With no new command (the unit stays idle), it is the unit's own stored tile, from
   step 1. The comparer runs over the trace set support this: aiming nowhere breaks 24 traces at their first worker
   coming out, aiming one tile off (the target's top-left plus (1, 1)) breaks 18, and the commanded goal tile runs them
   all. If no tile is found, the unit dies. The placed unit then faces as `movement/placement.md` says, except that a
   worker with a **build** job faces its **build goal tile**: the goal the build command stored when it was given in
   step 3, while the worker was still inside, from the unit's tile then (the building's top-left tile): the building
   goal rule (`movement/pathfinding.md`) on the site's footprint, or the shore rule, **pulled back** (static study for
   the pull-back; `economy/build-train.md`, "Placing a building"). It is **not** worked out again from the exit
   tile, so the facing and the walk use the same goal (traced:
   `xhuman9-10000`, round-51 vectors). The facing rule is "Facing a tile" of `core/animation-scripts.md`, from the exit
   tile. Every other come-out faces the target as before (a building at its top-left tile plus (1, 1), a
   tree at its own tile).

The order given in steps 3 to 5 (the AI's, or the return or harvest command) replaces the pending idle of step 1. It
takes over when the 25-cycle wait ends, with a wait of 3 (`orders/orders.md`, "Taking over"), not in the same update.
With no new command the pending idle takes over the same way: still animation, wait 3, and the idle logic first runs
3 cycles later (traced: `orc9-orc09-enter-nothing-1900` n = 1542, round-63 vectors). A computer player's unit whose AI
gives no order in step 3 is static study only.

### Lumber

While in order 23 in lumber mode, each time the order's logic runs:

1. **Not yet there** (more than 1 tile from the tree tile on either axis): it moves toward the tree tile.
2. **There, and not yet chopping:**
   - If the tree tile is still **unreserved forest**, the worker starts chopping:
     - the chop counter is set to 0;
     - the worker turns to face the tree;
     - the tree tile is marked **reserved** (see `movement/occupancy.md`);
     - the tree tile becomes its return-to source.
   - Otherwise (the tree was cut, or another worker reserved it), it looks for the **nearest tree** (below) from its own
     tile and moves there. If none is found, idle becomes the **pending** order and takes over in the same update: still
     animation, **frame kept**, wait 3. The idle logic doesn't run in that update (traced: `xhuman2-10000`, round-48
     vectors).
3. **Chopping**, and the tree tile is still reserved:
   - While the counter is below **50**, it goes up by 1. A cheat sets it straight to 50.
   - When the counter is already 50, the tree is **felled** (`movement/occupancy.md` gives the map changes). The
     worker is now carrying lumber (its graphic changes to the carrying one) and its frame is set to 0. It is given
     a return command to the nearest depot (order 24), or becomes idle if there is none, and that order takes over in
     the same update: order 24, its still animation, wait 3 (`orders/orders.md`, "Same-update take-over"). As a command,
     it clears the blocked counter (`movement/movement.md`, "Blocked rules"), so blocks met on the way to the tree don't
     carry over into the walk back.
4. **Chopping, but the tile is no longer reserved:** the reservation is dropped, and the worker looks for the nearest
   tree from its own tile, as in step 2.

Whenever these steps send the worker to a tile (step 1, and the nearest tree of steps 2 and 4), they don't just move
it: they give it a **new harvest command** to that tile, as a player would. So "Starting to harvest" runs again, the
goal is pulled back again and becomes the tree tile, the blocked counter is cleared (`movement/movement.md`), and
order 23 takes over **in the same update** with its attack animation (4) and a wait of 3, without running its logic
(`orders/orders.md`, "Same-update take-over"). The logic first runs at that animation's next step done, 3 cycles
later, and the worker then searches for a route and starts walking. If the nearest-tree search finds nothing, the
worker goes idle as in step 2.

The order's logic runs once per loop of the chop animation (the attack animation, 25 cycles per loop for workers;
see `core/animation-scripts.md`). The tree falls on the 51st run after chopping started: 1 run to start, 50 to
count up, then the felling run.

Each chop also picks a chop sound at random (one of 4). That draw uses the **setup-shuffle generator**, not the
simulation generator (see `core/timing-and-rng.md` and `ERRATA-round2.md`).

**No route.** When a unit whose type has the **peon flag** is in order 23, in **either gather mode**, and its route
search finds no route, it looks for the nearest tree ("Finding the nearest tree", below) from its own tile. A gold
worker heading for a mine does this too.

- **A tree is found:** the unit is given a **new harvest command** to that tree tile with no target unit. "Starting to
  harvest" runs, so the gather mode becomes lumber, a target mine is dropped and the goal is pulled back. Order 23 takes
  over in the same update: attack animation (4), wait 3. Unlike the other new harvest commands of "Lumber", order 23's
  logic then **runs once more in the same update**. If the new tree tile is within 1 tile, chopping starts at once (step
  2): the worker turns to face the tree, reserves it, and the chop makes its chop-sound draw; the attack animation still
  restarts with wait 3. If the tree is further away, the worker is given the harvest command **again**, to the goal the
  first command set, so the goal is pulled back a second time, from the first pulled-back tile (facing kept, animation
  4, wait 3). No move is started in that update; the route search and first step come at the next run, and if that
  search fails again the whole sequence repeats every 3 cycles and the worker doesn't move (traced: `xhuman12-10000`,
  round-52 vectors). Its logic next runs 3 cycles later (traced: `orc4-40000`, round-44 vectors).
- While there is still no route, this repeats at each run, every 3 cycles, with the same tree.
- **No tree is found:** the unit becomes idle as an **immediate** order (order 2, still animation) and its idle logic
  runs in the same update, as in "No route" of `orders/orders.md`. For a computer player's worker that is the job
  choice (`ai/worker-ai.md`), which can give it a new job at once, with the job choice's own, larger tree search
  (traced: `orc11-25000`, round-56 vectors). A human player's worker stays idle; its idle logic is the look-around.
- Only the peon flag (type flags bit 8) counts. Tankers have the tanker flag (bit 9) and not the peon flag in the
  unit data, so they don't make this search: a tanker in order 23 with no route goes idle as in "No route" of
  `orders/orders.md`.

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

A return-goods command (command 24) is taken by a worker (peasant or peon type) or a **tanker** that is **carrying**.
With a target unit, that unit is the target and the order becomes 24, **as given**: there is no test of the target's
owner, its kind, or whether it accepts what is carried (traced: `orc9-orc09-return-goods-2200` n = 1150 and 1265,
round-62 vectors). It still gets the command check of `orders/orders.md` ("Commands forget a stale attacker"): a
target that is dying, gone, inside something or undetected is dropped and idle becomes the **pending** order in place of
order 24. No depot is looked up then: the unit stays in its current order, still carrying, until the pending idle takes
over (wait 3) (traced for a target inside a gold mine: `orc9-orc09-return-nodepot-2200` n = 1374, round-63 vectors;
static study only for a dying, gone or undetected target). With no target, the command looks up the nearest depot.
The depot becomes the target, and the order becomes 24. If no depot exists, the order becomes 2. A worker sent to a
building that stores neither gold nor wood goes inside and keeps its load ("Entering", step 2; traced:
`orc9-orc09-enter-nothing-1900`, round-63 vectors). A tanker sent to a building other than an oil platform delivering
its oil there (as in the delivery table) is static study only.

While in order 24:

- While the unit is walking, only the walk runs; the order's logic runs when the walk has ended: on arrival, or when,
  at a step boundary, the target is found dying or gone, inside something, or no longer detected by the unit's player
  (`orders/orders.md`, "The reaction"). The step-boundary test doesn't wait for arrival: a target that goes inside
  something while the unit walks is dropped at the unit's next step boundary. A route search that finds no route
  doesn't run the order's logic (next bullet). Each time the logic runs, it first runs the reaction to an attacker: a
  worker hit by a worker fights back (command 8) instead of entering; order 24 isn't one of the orders that flee, so
  any other valid attacker is kept; an attacker that is no longer valid, or any attacker of a tanker, is forgotten
  (static study only: no trace has an attacker on a returning unit). Then, if the target was dropped at a step
  boundary or is **dying or gone**, the unit looks up the nearest depot ("Finding the nearest depot") and is given
  return goods to it **as a pending order** that takes over in the same update: still animation, wait **3**, cached
  route dropped, the new route starting from where it stands. If no depot is found, idle becomes the pending order
  (traced for a dying depot: `orc9-orc09-enter-dying-1800` n = 1435, round-62 vectors; with no depot left, a dying
  great hall: `orc9-orc09-return-nodepot-2200` n = 1530, round-63 vectors; a target that went inside something while
  the unit walked: `orc9-orc09-return-inside-1200` n = 558, round-64 vectors; the "no longer detected" part is static
  study only).
- If its route search finds no route, it becomes idle as an **immediate** order and its idle logic runs in the same
  update (`orders/orders.md`, "No route"). For a computer player's worker or tanker that is the job choice, whose new
  command takes over at once (wait 3).
- On arrival (radius 1): if the depot is completed, the order becomes 25 (enter); otherwise 2. The step-boundary test
  isn't made on arrival, and "inside something" isn't one of the states that send the unit to another depot, so a
  target that is inside something when the unit arrives isn't caught by order 24: the unit starts order 25, whose
  target check drops it ("Entering", step 3), and idle becomes the pending order (static study only).

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

Delivery happens when a unit enters a depot, and for each builder inside a building when its construction is completed
(`economy/build-train.md`, "Completion"). A worker delivers only at a building that stores gold or wood: at an oil-only building, such as a
shipyard, it goes inside without delivering ("Entering", step 2).

| Unit and depot | Amount added to the owner's stock |
| --- | --- |
| Ship (tanker) at a building that isn't an oil platform | Oil: **100**, or **125** if the owner has a completed refinery |
| Gold mode, at a gold-storing building | Gold: **100**; **110** if the owner has a completed keep or stronghold; **120** if a completed castle or fortress (castle checked first) |
| Lumber mode, at a gold-storing or wood-storing building | Lumber: **100**, or **125** if the owner has a completed lumber mill |
| Gold mode, at a building that stores wood only | Nothing; the load is lost |
| Anything else | Nothing |

- Each amount also goes into the owner's "total gathered" statistic for that resource.
- After a delivery the carrying mark is cleared. The gather mode and return-to source stay.
- Every delivery at a building that accepts gold (a town hall, keep or castle, of either race) sets the unit's
  **hall-sized exit mark**, whatever its gather mode and whether or not it carries anything. Every unit starts without
  the mark; only a come-out (order 26) uses and clears it ("Coming out", step 6). Entering such a building always counts
  as a delivery here, so a unit with gather mode none gets the mark too. A ship's delivery never sets it (static study).
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
- **Collapse:** when the last trip is taken, the mine or platform is destroyed. Every unit inside that has it as its
  target is let out at its next chance: its target is cleared and its wait cut to 1. That includes the unit that took
  the last trip: it carries its load as usual, but has no target and no return-to source, and comes out at its next step
  done like the others (traced: `human12-25000`, vectors below).

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

### Round-38 vectors (oracle traces)

| Input | Expected |
| --- | --- |
| `human12-7000`, computer peon 1572 (type 0x03) on (91, 11) given lumber at n = 4, tree (105, 0); the farm at (104, 1) makes (104, 1) unusable, (103, 2) is usable | Tree tile and goal (104, 1). Route NE, NE, NE, E, E, E, NE, N: (92, 10) at n = 7, (93, 9) at 23, (94, 8) at 39, (95, 8) at 55, (96, 8) at 71, (97, 8) at 87, (98, 7) at 103, (98, 6) at 119; the route has run out at n = 135 and a new search (line only) gives NE to (99, 5) |
| Same worker arrives at (103, 1) (n = 199); (104, 1) isn't forest | n = 215: order 23, animation 4, wait 3, same tile (a new harvest command to (105, 0), the nearest tree from (103, 1)). n = 218: (104, 0), animation 3. n = 234: animation 4, facing 2, chopping (105, 0). n = 1509: order 24 |
| `human5-10000`, computer peasant 1512 (type 0x02) on (34, 105) given lumber at n = 4, tree (29, 107), pulled back to (31, 106) | Route SW, SW: (33, 106) at n = 7, (32, 107) at 23. n = 39: animation 4, wait 3 (the new aim at (29, 107)). n = 42: SW to (31, 108) |
| Lumber worker arrives next to its tree tile; the tile was cut meanwhile; a tree 2 rings away | Same-update take-over: order 23, animation 4, wait 3; its route starts 3 cycles later |

### Round-39 vectors (oracle traces)

| Input | Expected |
| --- | --- |
| `human5-10000`: computer worker 1567 (type 0x03, owner 5) walks from (107, 43) toward gold mine 1555 at (100, 54), in gold mode with the mine as its target. At n = 119, at (105, 48), its route search finds no route | n = 119: order 23, animation 4, wait 3, target null (a lumber harvest command). n = 122: the same. n = 125: it steps east to (106, 48), facing 2, animation 3 |
| `orc8`: lumber worker 1499 at (123, 92); no route from n = 754 | Order 23, animation 4, wait 3 at n = 754, 757 and every 3 cycles to n = 1993; it steps north at n = 1996 |
| A worker in order 23 with no route and no tree within 6 rings | Idle as in "No route" (`orders/orders.md`) |
| A tanker in order 23 whose route search finds no route (static study and unit data only) | Idle (order 2, wait 1); no tree search |

### Round-40 vectors (oracle traces)

| Input | Expected |
| --- | --- |
| `xhuman2-10000`, n = 1113: computer peon 1592 (owner 0) comes out of 1585 with a build order for a site with top-left (26, 20) | Tile (24, 17), order 2, wait 25, facing 3, target null |
| `orc4-20000`, n = 10595: computer peasant 1540 (type 0x02) comes out of 1577 with a build order | Tile (9, 11), order 2, wait 25, facing 1 |
| `human7-10000`, n = 7444: peon 1582 (type 0x03, owner 5) comes out of 1594 with a build order | Tile (71, 5), wait 25, facing 3 |
| `xorc3-10000`, n = 2964: peon 1471 (owner 3) comes out of 1589 with a build order | Tile (34, 9), wait 25, facing 7 |

### Round-41 vectors

| Input | Expected |
| --- | --- |
| `xorc3-10000` (oracle trace), n = 8895: computer peon 1462 (type 0x03, owner 3) in order 24 on depot 1589 finds no route | Immediate idle; the job choice in the same update gives order 24 on 1589 again: order 24, wait 3, target 1589 |
| `xorc7-7000` (oracle trace, with a break log of the commands given): computer tanker 1587 (type 0x1A) in order 23 (harvest, not a return) on 1590, no route | The same immediate idle: at n = 1932, 1935 and 1938 the job choice in the same update gives order 23 on 1590 again (animation 4, wait 3) |
| A human player's peon comes out of a town hall with no source and no load | Order 2; the ring search aims at its own stored tile |
| Same, after delivering, its return-to source a gold mine | The ring search aims at the mine's top-left tile |

### Round-42 vectors

| Input | Expected |
| --- | --- |
| `xorc3-10000` (oracle trace), n = 9180: computer peon 1485 (type 0x03, owner 2) comes out at (35, 117) with a build job | Order 2, wait 25, facing 0 (toward its build goal tile) |
| A worker comes out at (10, 10) of a hall whose top-left tile is (8, 8), with a build job for a 2 × 2 site at (14, 10) | The goal worked out from (8, 8) is (14, 10); it faces it from (10, 10): east (2) |

### Round-43 vectors

| Input | Expected |
| --- | --- |
| A worker blocked 9 times on its way to a tree fells it | Its return walk starts with the blocked counter at 0 |
| `orc8-10000` (oracle trace): worker 1501 fells its tree at n = 8036 with a blocked counter of 8 (the counter isn't in the trace) and is given the return command (order 24 on 1489, wait 3). Computer peasant 1497 (type 0x02, owner 1) comes out of gold mine 1509 at n = 8054 and gets its own return command at n = 8079 | 1501's return command cleared its counter. n = 8082: 1497's route search treats 1501 as a moving ally, which it does only when 1501's counter is 0: 1497 is at (123, 88), facing 4, in its move animation |

### Round-44 vectors

| Input | Expected |
| --- | --- |
| `orc4-40000` (oracle trace), n = 23680: peasant 1584 (type 0x02, owner 1) in order 23 at (3, 10) finds no route to its tree; the nearest tree is (2, 11) | Same-update logic run: facing 7 → 5 (toward the tree), animation 4, frame 0, wait 3; the tree is reserved |
| `human5-10000`, n = 39: peasant 1512, new tree not adjacent | Facing kept, animation 4, wait 3 |
| `human12-7000`, n = 215: worker 1572, new tree not adjacent | Facing kept, animation 4, wait 3 |

### Round-47 vectors

| Input | Expected |
| --- | --- |
| `orc5-10000` (oracle trace): peasant 1560 (type 0x02, owner 1) built hall 1511 (inside from n = 88, let out at (105, 48) at n = 3159), then enters gold mine 1562 (type 0x5C, top-left (106, 42)) at n = 3230 | It comes out at n = 3380 at (105, 45), facing 3, order 2, wait 25, target 1511: the 4 × 4 source aimed at (106, 48) gives (105, 45) as the first free tile; the 3 × 3 source would give (105, 44) |
| A worker delivers gold at a town hall, goes to a mine and comes out | The mine exit uses the 4 × 4 source; mark cleared |
| The same worker's following exit from the mine | 3 × 3 source |
| `xhuman3-10000` (oracle trace), n = 8749: computer tanker 1418 (type 0x1B, owner 3) in order 23 arrives at (108, 60) next to its platform 1404 (type 0x57), still under construction | Order 2, animation 2, wait 3, target null (pending idle). n = 8752: order 23 again, animation 4, wait 3, target 1404. n = 8755: order 2, wait 3 |

### Round-48 vectors

| Input | Expected |
| --- | --- |
| `xhuman2-10000` (oracle trace), n = 5820: computer peon 1530 (type 0x03, owner 7) at (88, 94) in order 23 finds its tree gone and no other tree | Order 2, animation 2, frame 25 kept, wait 3; the worker AI gives order 23 again at n = 5823 |
| A worker with gather mode none enters a town hall | Hall-sized exit mark set |

### Round-49 vectors

| Input | Expected |
| --- | --- |
| `orc12-10000` (oracle trace), n = 2877: computer tanker 1409 (type 0x1B, owner 1), idle, given the harvest command on oil platform 1403 (type 0x57), placed in that cycle | Order 2, animation 2, wait 3, target null |
| `xhuman8-10000` (oracle trace), n = 732: tanker 1468 (type 0x1B, owner 3) on platform 1466 (type 0x57), placed in that cycle | Order 2, animation 2, wait 3, target null |

### Round-50 vectors

| Input | Expected |
| --- | --- |
| `human8-10000` (oracle trace), n = 2486: computer peasant 1508 (type 0x02, owner 2) comes out of town hall 1512 (type 0x4A, top-left (74, 72)) with a repair job on farm 1540 (type 0x3A, top-left (74, 59)) | Tile (73, 72), order 2, target 1540, facing 0, wait 25 |

### Round-51 vectors

| Input | Expected |
| --- | --- |
| `xhuman9-10000` (oracle trace), n = 9065: computer peon 1440 (type 0x03, owner 2) comes out of great hall 1438 (type 0x4B, top-left (114, 109)) at (113, 110) with a pig farm job at site (110, 109); the goal worked out from inside is (111, 109) | Order 2, wait 25, facing 7 (toward (111, 109), not 6 toward (111, 110), the goal from the exit tile). The walk goes (112, 109) at n = 9100, then (111, 109); it builds at n = 9125 |

### Round-52 vectors

| Input | Expected |
| --- | --- |
| `xhuman12-10000` (oracle trace), n = 5598: computer peon 1386 (type 0x03, owner 6) at (25, 88) in order 23; its route to (23, 90) fails at n = 5595; the nearest tree is (27, 92), whose pulled-back goal is (26, 91) | Second pull-back: goal (25, 90); tile (25, 88), order 23, facing 6, animation 4, wait 3 |
| `human13-10000` (oracle trace), n = 196: peon 1550 (type 0x03, owner 0) comes out of gold mine 1544 (type 0x5C) carrying, with a return to fortress 1584 (type 0x5B, top-left (81, 2)); the pulled-back goal is (77, 6) | Placed at (75, 8) (the ring search starts at (74, 8) going east), order 2, wait 25, facing 1 |

### Round-54 vectors

| Input | Expected |
| --- | --- |
| `human12-25000` (oracle trace), n = 12948: tanker 1410 (type 0x1B, owner 0) takes the last trip of oil platform 1530, which collapses | 1410: order 26, wait 1, target null (it was in order 25 on 1530 at n = 12947) |

### Round-56 vectors

| Input | Expected |
| --- | --- |
| `orc11-25000` (oracle trace, Enhanced), n = 10428: computer peasant 1441 (type 0x02, owner 1) at (22, 122), in order 23 on gold mine 1457 (at (5, 123)) since n = 10425, finds no route and no tree within the no-route search | Immediate idle, then the same-update job choice gives a lumber job: order 23, target null, animation 4, wait 3, with one draw more than a plain idle. It then walks to a tree and is chopping at (31, 111) by n = 10728 |
| A human player's peasant in the same position (static study) | Order 2 as an immediate order; its idle logic runs in the same update (no job choice for a human player) |

### Round-58 vectors

| Input | Expected |
| --- | --- |
| `mp-rose-petal-bne-10000` (oracle trace, Enhanced), n = 1731: computer peasant 1567 (type 0x02, owner 3) comes out of town hall 1556 (top-left (55, 84), hall-sized exit) toward gold mine 1571 (top-left (59, 78)). Pulling back from (55, 84): the first tile tested, (58, 79), is forest; the next, (58, 80), is usable, so the goal is (58, 79) | Placed at (55, 83), facing 1 (toward (60, 79)); order 2, target 1571. Aimed at (59, 78) instead, it would come out at (59, 84) facing 0 |
| `mp-gold-rush-bne-6000` (oracle trace, Enhanced), n = 2353: peon 1511 comes out of great hall 1534 (top-left (11, 49)) toward gold mine 1547 (top-left (6, 43)) | Placed at (10, 50), facing 7, toward (7, 44): the mine's top-left plus (1, 1), not the pulled-back goal (8, 45) plus (1, 1), which would give 0 |

### Round-62 vectors

| Input | Expected |
| --- | --- |
| `orc9-orc09-return-goods-2200` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-return-goods.txt`), n = 1150: peon 1482 (type 0x03, owner 5), carrying gold, told to return goods to its own lumber mill 1477 (type 0x4D) | Target 1477 (n = 1152), not the great hall; order 24 from n = 1168; order 25 at n = 1457, inside at n = 1482; player 5's gold doesn't change; at n = 1632 it heads back to mine 1480 (order 23 from n = 1657) |
| Same trace, n = 1265: tanker 1473 (type 0x1B), carrying oil, told to return goods to its own oil platform 1468 | Order 24 at n = 1284; enters at n = 1287 with no oil delivered; at n = 1440 heads for shipyard 1499; oil +100 at n = 1599 |
| `orc9-orc09-enter-dying-1800` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-enter-dying.txt`): lumber mill 1477 starts dying at n = 1428; peon 1482 is in order 24 to it | n = 1435: return goods to great hall 1488, still animation, wait 3; it walks from n = 1438 |
| Same trace: peon 1478 (type 0x03), carrying lumber, starts entering mill 1477 at n = 1416 and is stepping in when the mill starts dying | 1478 dies (order 1) at n = 1441 with no target |

### Round-63 vectors

| Input | Expected |
| --- | --- |
| `orc9-orc09-enter-nothing-1900` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-enter-nothing.txt`), n = 452: peon 1482 (type 0x03, owner 5), carrying gold, told to return goods to orc blacksmith 1495 (type 0x53) | Order 24 from n = 464; inside (order 26) at n = 910; 1495's frame becomes 2 at n = 910 and stays 2 to the end; player 5's gold stays 1100 |
| Same trace, n = 1060: 1482 comes out of the blacksmith, still carrying, its return-to source gold mine 1480 (type 0x5C), while peon 1478 is inside 1480 (since n = 941, frame 1) | 1480's frame becomes 0 at n = 1060; 1482 gets return goods to great hall 1488 (type 0x4B) and delivers there at n = 1201 (gold 1200). When 1478 comes out of 1480 at n = 1091 the frame stays 0 |
| Same trace, n = 1132: peon 1478, carrying gold, told to return goods to orc shipyard 1499 (type 0x49, stores oil only) | Inside at n = 1367; 1499's frame becomes 2 and stays 2; gold stays 1200 (no delivery; the load is kept) |
| Same trace, n = 1517: 1478 comes out of the shore shipyard (top-left (44, 78)), still carrying | Order 2, target none, wait 25, placed at (43, 80): no depot found. At n = 1542 the pending idle takes over: still animation, wait 3; the idle logic first runs 3 cycles later |
| `orc9-orc09-return-nodepot-2200` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-return-nodepot.txt`), n = 1152: peon 1482 (type 0x03, owner 5), carrying gold, told to return goods to peon 1478, which is walking | Order 24 from n = 1168; order 25 at n = 1193 at (48, 92). n = 1196: facing 0, stepping to (48, 91), 1478's tile (not facing 2 toward (49, 92), 1478's tile + (1, 1)). Inside 1478 (order 26) from n = 1218 |
| Same trace, n = 1368: 1482 comes out of 1478, which is inside gold mine 1480 (since n = 1242) | 1480's frame drops from 1 to 0 at n = 1368 |
| Same trace, n = 1374: 1482, idle with return goods to great hall 1488 pending, told to return goods to 1478, which is inside the gold mine | Target none in that turn record; order 2 kept; the pending idle takes over at n = 1393 (still animation, wait 3); 1482 stays idle to the end |
| Same trace: great hall 1488 starts dying at n = 1522 while peon 1478 is returning gold to it (order 24 since n = 1417); no other depot | n = 1530: 1478 idle (order 2), target none |
| A worker hit by an enemy peon as it reaches its depot in order 24 (static study) | It fights back (command 8) instead of entering |
| A human player's worker in order 24 hit by an enemy archer while walking to its depot (static study) | No reaction while walking; on arrival the reaction runs, and it doesn't flee (order 24 isn't one of the flee rule's orders, `orders/orders.md`, "Fleeing") |

### Round-64 vectors

| Input | Expected |
| --- | --- |
| `orc9-orc09-return-inside-1200` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-return-inside.txt`), n = 526 (the turn record of the input at n = 524): peon 1482 (type 0x03, owner 5), carrying gold and in order 24 to great hall 1488, told to return goods to peon 1478, which is walking to gold mine 1480 (type 0x5C) | Target 1478 in that turn record; the pending order 24 takes over at n = 533 (still animation, wait 3); 1482 is at tile (47, 89) from n = 536 |
| Same trace: 1478 goes into the mine at n = 537 (order 26) while 1482 walks toward it | n = 558, 1482's next step boundary, at (47, 89): target great hall 1488 (type 0x4B), still animation, wait 3, frame 0. It walks back (order 25 from n = 693) and goes inside the hall at n = 718. It doesn't walk on toward 1478 |
| A peon in order 24 whose target unit is already inside a mine when the peon arrives within reach (static study) | Order 25 starts; at its first run the target check drops the target; idle becomes the pending order |
| A worker in order 24 with an attacker whose route search finds no route (static study) | Idle as an immediate order; the idle logic runs, not order 24's reaction |
| `xhuman5-30000` (oracle trace, Enhanced, no input): computer tanker 1542 (type 0x1B, owner 7) is inside orc shipyard 1541 (type 0x49, top-left (100, 100)); its return-to source, oil platform 1554 (type 0x57, top-left (101, 85)), collapses at n = 26347 when tanker 1495 takes its last trip (ruin type 0x6C from then) | n = 26422: out at (104, 98), facing 0, order 2, target none, wait 25 (it comes out toward the platform, not on the far side of the shipyard). n = 26447: order 23 with no target takes over (wait 3); it walks north to (100, 86) by n = 26610. n = 26642: order 2, still animation, wait 3 (the pending idle) |

## Open questions

- Step 1's new harvest command (not yet there) is from the static study only: in the traces the order's logic runs
  on arrival, when the worker is already within 1 tile.
- How the AI routines for computer-controlled workers and tankers decide at the mine and depot.
- Whether any normal command can get an empty-handed unit into a depot in Classic mode (the Classic delivery rule
  would then pay a full load).
- The return-goods button: when it is offered and what message appears if there is no depot.
- **No route details.** Whether the idle after "no tree found" in "No route" is pending or immediate isn't separated by
  the traces: a research run made it a pending idle, as in step 2, and no trace changed. The spec keeps the immediate
  idle there. Also untraced: whether the computer's job counts change when a miner becomes a lumber worker this way (the
  static study shows no count change on this path).
- **A platform placed in the same cycle.** Why a tanker's harvest command on it fails ("Starting to harvest", step 3)
  wasn't found; the rule is the narrowest one that fits both traces. In both, the oil patch's slot comes after the
  builder's and the tanker's; whether a platform is usable at once when the builder's slot comes after the patch's is
  untraced.
- **Other AI jobs at a come-out.** Only the repair job's aim is traced; the aims for the AI's other jobs (attack,
  patrol) aren't checked.
