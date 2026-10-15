# Orders: transports (boarding, landing, unloading)

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11). Consolidated 2026-10-14 into one model with no
  rule change (B-038; the review notes, with a map from every earlier rule and vector to its place here, are kept
  with the research-side specs). The consolidation was checked by a second spec-review session, 2026-10-14.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the unit flags in the
  public PUD specification text, and oracle traces of the running game (for "Land and shore targets", the
  research-side command logs and map dumps of Human04). Each rule says which: "traced" with the trace name and record
  number n, or "static study only"; a rule with neither is from the static study. The erratum rounds each rule came
  from are listed under "History" at the end.
- **Changes:** 2026-10-15 (round 70, `ERRATA-round70.md`, M1 to M7): V10's n = 17216 record explained by break
  captures (a re-issued boarding command, not a restart); static study: the docking sound on ordinary steps, far
  landing steps don't restart the animation, the push-off and unload waits, push-off "done" clears the blocked
  counter, every command records the transport's target, tile and destination. Reviewed by a spec-review session,
  2026-10-15 (the push-off and unload waits keep their trace request under "Open questions").
- **Changes:** 2026-10-15 (round 71, `ERRATA-audit-orders.md`, erratum 5; traced): a player's command clears the
  unload-on-arrival mark ("Clearing the mark"); vector V39.
- **Changes:** 2026-10-15 (round 71, `ERRATA-round71.md`, Q3 and Q4; static study): the three things are recorded
  once per command, the smart-move stage doesn't record them again, and a command with no tile of its own records
  the tile fields it carries. Both round-71 entries reviewed by a spec-review session, 2026-10-15 ("Clearing the
  mark" moved ahead of the unload check).

## Behaviour

A **transport** is a unit whose type has the transport flag (bit 10 of the unit flags in the PUD unit data). It
carries up to **6 passengers** in 6 numbered slots; an empty slot holds "none". Transports use five orders (numbers
as in `orders/orders.md`):

| Order | Meaning |
| --- | --- |
| 29 | Unloading: put every passenger ashore, then dock |
| 30 | Landing: sailing to a shore-water tile, moving in the landing class |
| 31 | Pushing off: leaving the shore for open water, then going on to the requested destination |
| 32 | Docked at the shore. The order has no logic of its own; the transport waits |
| 36 | Passenger inside a transport (an order of the passenger, not of the transport). No logic of its own |

Land units use order **34** (board) to reach a transport.

Terms used below:

- **Shore water:** a tile with the shore-water bit (0x02, `movement/occupancy.md`).
- **Open water:** a tile with the water bit (0x40).
- **Region:** the region number of a tile in the map's region data (as in `movement/pathfinding.md`).
- **Movement class:** land, air, sea or landing (`movement/occupancy.md`). A transport normally moves in the sea
  class; while landing it uses the landing class, which may enter shore water.
- **Even tile:** both coordinates even. Ships rest on even tiles (`movement/movement.md`).

### The model

A transport goes round one cycle, and every order in this spec is a stage of it:

| Stage | Order | Ends with | Section |
| --- | --- | --- | --- |
| Passengers walk to it and board | 34 (the passenger's) | order 36 aboard; or a move (computer players), waiting, or idle | "Boarding" |
| Docked at the shore, waiting | 32 | the next command | "Commands given to a transport" |
| Leaving the shore | 31 | an even open-water tile, then a smart move to the destination; or docked | "Pushing off (order 31)" |
| Sailing | 3, 6 (the general orders) | arrival, as for any ship (`movement/movement.md`) | "Commands given to a transport" |
| Landing at a shore-water tile | 30 | order 29 (unload on arrival) or 32; with no route away from the shore, idle (even tile) or 32 (odd tile) | "Landing (order 30)" |
| Putting the passengers ashore | 29 | order 32 | "Unloading (order 29)" |

Four rules are shared by the stages and stated once:

- **One set of command rules.** Every command given to a transport records the same three things and adjusts a land
  or shore target the same way; a smart move or the unload command then picks the stage by one table, first match
  wins ("Commands given to a transport").
  A command that a stage gives the transport itself (pushing off's smart move to the destination, a boarding unit
  calling it) goes through the same rules.
- **Moving near the shore.** Landing and pushing off move the transport by their own logic, with their own step size,
  movement class and end test, but each step, block and route search is the ordinary one of `movement/movement.md`
  and `movement/pathfinding.md` ("Moving near the shore", below).
- **One ring search, three settings.** Picking a shore tile for a land target, picking an open-water tile to push off
  to, and placing a passenger ashore all use the ring search of `movement/placement.md`, with the settings in "Data".
- **Giving up.** A stage that can't go on ends in one of two ways: the transport docks (order 32), or the unit
  becomes idle (order 2). The one exception: landing with no route while on shore water and marked unload on arrival
  unloads instead (order 29; "Landing (order 30)", step 6). Each stage's steps say which, and with which wait.

### Moving near the shore

Landing and pushing off differ only where this table says; everything else is the ordinary step (`movement/movement.md`,
"Positions and steps") and the ordinary blocked rules, with the walking animation restarted as animation 6 when
blocked and row F never applying (`movement/movement.md`, "Blocked rules").

| Point | Landing (order 30) | Pushing off (order 31) |
| --- | --- | --- |
| When its own logic moves the transport | within 3 tiles of the goal; farther out, ordinary two-tile steps in the sea class, which don't restart the move animation at each step (it runs on as for any walking unit; static study, round 70) | always |
| Movement class | the landing class | unchanged (the landing class after a landing) |
| Step size | two-tile while both its tile and its goal tile are even; otherwise one-tile | two-tile on an even tile; otherwise unchanged |
| Animation | two-tile: the move animation, run at once; one-tile: its own animation 6 | the same |
| Route | a new route from the current tile at every step | the ring search before every step, then its cached path or a new search |
| End test | the goal tile exactly (radius 0, no +1) | an even tile of open water |
| No route | step 6 of "Landing (order 30)" | docks at once (order 32 as an immediate order: still animation, wait 1) |
| Docking sound | on a step **onto** shore water | on a step **leaving** a shore-water tile (whatever the tile stepped onto) |

The docking sound also plays on a transport's **ordinary** steps (any order, including order 30 farther than 3 tiles
out): on every step onto a shore-water tile, as in landing. Pushing off is the only case that tests the tile being
left instead (static study, round 70).

### Commands given to a transport

A smart move (command 3) to a transport, or the **unload** command (command 29), is decided as follows. G is the
target tile and S the transport's tile.

**What the transport stores.** Giving a transport any command (every command, including a computer player's move or
attack; static study, round 70) records three things: the **target unit** (if the
command has one), the **target tile** (the target unit's tile, or the command's tile), and the **destination**, a
copy of the target tile made at that moment. Later steps may change the target tile (docking at S, pushing off), but
they never change the destination, which is what "its destination tile" means below.

The three things are recorded **once per command**, when it is given (static study, round 71, `ERRATA-round71.md`,
Q3 and Q4). The smart-move stage that picks the order (the table below) works from the target tile the command
recorded: it doesn't record them a second time, and the land and shore steps don't run again. That holds for a smart
move given directly (a player's, the flee's, follow step 1's) and for command 4 given to a transport, which as a
non-combat unit goes on as a smart move: the command records once, not once more for the smart move. A command with
no tile of its own (stop, stand ground) records the tile fields the command carries, like any other; in the oracle's
input those are (0, 0). Which tile the game's own buttons put there isn't established.

**Land and shore targets.** After those are recorded, if the target tile has the land bit (0x01) or the shore-water
bit (0x02), three more steps run before the rules below. They run for every command given to a transport, not only
the smart move and unload:

1. The **target unit is dropped**. The command goes on with no target unit.
2. If the transport stands on **open water** (bit 0x40), its target tile is adjusted. Otherwise it is left as it is.
   T is the transport's tile and G the target tile.
   1. **Pull back.** Take the line from T to G by the line rule of `movement/pathfinding.md` ("Line rule"), all of
      its steps s1, s2, … in order, until it reaches G. Walk back from G by undoing them in that order: G, then
      G − s1, then G − s1 − s2, and so on, stopping before T. A tile is **usable** when its region value equals T's
      and it has no bit of 0x0803 (the sea blocking mask 0x0903 without 0x0100). If G itself is usable, K = G.
      Otherwise K is the tile walked **just before** the first usable tile, on G's side of it. If no tile is usable,
      K is the last tile before T.
   2. **Check.** If K has the shore-water bit, no bit of the landing blocking mask 0x0901, and the same region value
      as G, K is the new target tile. Otherwise the ring search of `movement/placement.md` runs with a 1 × 1 source
      at G, aimed at G, limit **8** (4 rings), accepting a tile that has the shore-water bit, no bit of 0x0901, and
      G's region value. The first tile found is the new target tile. If none is found, K is.
3. The destination becomes the (adjusted) target tile.

The rules below then run with G = the new target tile. From open water, a land target therefore usually ends as "G is
shore water: order 30". The target unit is gone, so the "own side's unit that isn't a building: order 6" row applies
only to a target unit standing on open water.

**Clearing the mark.** Every command a **player** gives a transport (from the command stream, a right-click included)
first clears its unload-on-arrival mark, before anything else the command does; an unload command then sets it again
if its G is shore water. So a transport told to unload and then given a smart move that still ends at the shore
lands without unloading: it docks (order 32) with its passengers aboard (traced: `human12-unload-then-move-900`
n = 427, vector V39). The commands the computer players' own routines give don't clear it (static study only; the
ferry sets the mark itself, `ai/attack-ai.md`).

The unload command first checks G: if G is shore water, the transport is marked **unload on arrival**. Then both
commands apply the same rules, first match wins:

| Situation | Result |
| --- | --- |
| S is shore water, G has the same region as S, and the straight line from S to G (below) meets a land tile of G's region before any open-water tile, or reaches G without meeting either | If G = S: order 30 (it counts as arrived at once). Otherwise: order 32, and the transport's target tile becomes S. It stays docked |
| The transport's order is 30, 31 or 32, or S is shore water | Order 31 (push off first) |
| G isn't shore water, and the target is a unit of the transport's own side that isn't a building | Order 6 (follow) |
| G isn't shore water, any other case | Order 3 (move) |
| G is shore water | Order 30 (land at G) |

**The straight line** follows the line rule of `movement/pathfinding.md` ("Line rule") from S to G, with one-tile
steps. The **first tile tested is S itself**; then each tile reached by a step is tested, and the walk stops on
reaching G (G itself isn't tested). S is shore water, which carries neither bit tested below, so in practice the
first tile that can end the walk is the first step from S. A tile with the land bit (0x01) whose region equals G's
region ends the walk with "land first". A tile with the open-water bit ends it with "water first".

### Landing (order 30)

This logic runs only when the transport is within 3 tiles of its goal on both axes. Farther out it moves with
ordinary steps (sea class, two-tile mover, move animation 3; `orders/orders.md`, "Order 30's radius").

Each time the order's logic runs:

1. The transport switches to the **landing class**.
2. It moves as a two-tile unit (even tiles) only while both its tile and its goal tile are even. Otherwise it moves
   as a one-tile unit. As a two-tile mover it starts its move animation (3) and runs it at once (the first
   instructions run in the same cycle); as a one-tile mover its animation isn't changed, so it keeps its own
   animation (6, started when the order took over, or by the arrival switch when it came within 3 tiles), which
   carries the one-tile steps.
3. **Arrival** is reaching the goal tile exactly. The sea-unit +1 on the radius doesn't apply to order 30.
4. When it **arrives**: if it has a pending order, that takes over. Otherwise it becomes order 29 if it is marked
   unload on arrival, else order 32. The unload-on-arrival mark is cleared either way.
5. While **not arrived**, it takes the next movement step as usual (`movement/movement.md`). A step onto shore water
   plays the docking sound. Within the 3 tiles every step's arrival test also discards the planned route, so the logic
   plans a new route from the current tile at every step, not the route of its first plan (traced: `human13-10000`,
   V12).
6. If there is **no route**:
   - on shore water: order 29 if marked unload on arrival, else order 32;
   - elsewhere on an even tile: back to the sea class, idle (order 2);
   - elsewhere on an odd tile: order 32.

   The unload-on-arrival mark is cleared.

### Pushing off (order 31)

Order 31 moves the transport by itself: it is not a moving order in the sense of `orders/orders.md`, so the general
movement logic (path cache, arrival test) doesn't run for it. Instead, each time the order's logic runs:

1. **Done:** if the transport stands on an even tile of **open water**, it returns to the sea class, becomes a
   two-tile mover and becomes idle (an immediate order), and is then given a smart move to its **destination** tile,
   with the same **target unit** if it still has one (a smart move with a target unit aims at that unit's current
   tile instead). The smart move is a command, so everything a command does applies (`orders/orders.md`, "What every
   command does"): among other things the cached path is dropped and the **blocked counter is cleared** (static
   study, round 70). The command rules above then run again from open water. Nothing else happens in this run.
2. **Pick a tile:** otherwise it runs the push-off ring search of `movement/placement.md`: a 1 × 1 source at the
   transport's tile, aimed at its current **target tile**, limit **10** (5 rings); a tile passes when both
   coordinates are even and no bit of the sea mask (0x0903) is set. If none passes, the transport docks: order 32
   becomes the **pending** order and takes over in the same update (still animation, wait 3; static study, round 70,
   untraced), and the run ends. If one passes, it becomes the target tile (the destination is not touched), the
   cached path is dropped and the blocked counter is cleared.
3. **Forget lost targets:** a target unit or attacker that is no longer valid or no longer detected by the
   transport's owner (fog doesn't matter; see `vision/fog.md`, "What uses vision") is forgotten. If the target unit
   was forgotten, idle (order 2) is made the pending order, so the push-off ends at the next step boundary.
4. **Step size:** if the transport stands on an even tile, it becomes a two-tile mover and its move animation is
   started and run at once. On an odd tile nothing changes (the transport keeps animation 6, the order's own), so a
   transport that docked on an odd tile (a one-tile mover after landing) keeps one-tile steps until it reaches an
   even tile. Its movement class is not changed: it stays in the class it had (the landing class after a landing).
5. **Step:** it takes one step toward the target tile, using its cached path or a new search (`movement/pathfinding.md`).
   If the search finds no route, it docks at once (order 32 as an **immediate** order: still animation, wait 1;
   static study, round 70, untraced). If the next tile is blocked, the blocked rules of
   `movement/movement.md` apply. Otherwise the step is taken as described in `movement/movement.md` (leaving a
   shore-water tile plays the docking sound).

The logic runs again at the next "step done" of the transport's animation, so the transport takes one step per run,
and re-runs the ring search before every step, now aimed at the tile it found last time.

### Unloading (order 29)

When the order's logic runs, if the transport stands on shore water, its passengers are put ashore **from slot 6
down to slot 1**. Then order 32 (docked) takes over in the same update, as a pending order does: still animation,
wait 3. It doesn't wait for a later step. If it isn't on shore water, no passenger is put ashore and order 32 takes
over the same way (pending, same update, still animation, wait 3; static study, round 70, untraced).

**One passenger** is put ashore like this, and only while the transport's order is 29 or 32:

1. Its tile is chosen by the ring search (`movement/placement.md`): a 1 × 1 source at the transport's tile, aim at
   the same tile, limit **6** (3 rings), the passenger's movement class, and the tile must be in the **same region
   as the transport's tile**.
2. If no tile passes, the passenger stays aboard.
3. Otherwise the passenger's tile position becomes the found tile and its pixel position that tile × 32, and it is
   entered there: the tile gets the ground-unit mark (0x0100) and its ground-unit pointer names the passenger. It is
   shown and becomes idle as an **immediate** order (order 2, pending order cleared, still animation, wait 1, target
   cleared; `orders/orders.md`, "No route"), but its idle logic isn't run here. Its own update comes later in the
   same cycle (after the transport's in update order) and runs as normal, so its look-around draw is made then, and
   its wait shows 1 at the end of the cycle. It keeps its facing (static study; see open questions). Its slot
   becomes empty.

The same one-passenger rule is used to unload a single passenger outside order 29 (by the player's interface and by
computer players).

### Boarding

**The command.** A smart move by a **land-class** unit whose target is a transport **of its own side**:

1. Unless the transport's current order or pending order is 30 (landing), the transport is given a smart move whose
   target is the boarding unit (so it comes to meet it, by the rules above).
2. The unit gets order 34.

**Order 34 (board)** is a walking order. While the unit walks to its goal, only the movement runs at each step, so a
unit of **any** player, computer players included, stays in order 34 while it walks to the transport. The walk's route
search has the transport as its **target unit**, so its goal zone is the transport's footprint widened by one tile on
each side (`movement/pathfinding.md`, "Step 1"), not the 3 × 3 tiles around the goal tile (traced: `orc14-10000`,
V15). The order's logic (the steps below) runs when the walk ends:

- on **arrival**, the steps below run;
- when the transport is found lost at a step boundary (the walk's own target test), step 1 below runs (static study
  only);
- with **no route**, the unit restarts order 34 at once if the transport's destination tile is in its region, and
  becomes idle at once otherwise (`orders/orders.md`, "No route", the boarding case). For a human player's unit the
  restart also tells the transport to come again, as the boarding command does, unless it is landing: a docked transport
  stays in order 32, and order 32 takes over again at the end of its current wait (still animation, wait 3;
  `human12-transport`, V16 to V18). Whether a computer player's restart does the same isn't settled (open questions).
  The restarted order's logic then runs **in the same update** (the steps below). It always reaches step 3, since
  the unit has just been found not to have arrived, so a human player's transport call is step 3's smart move
  (static study only). A computer player's unit that isn't within reach of the transport gets command 4 on it at
  step 3, which takes over at once: order 4, still animation, wait 3, target the transport (traced: `human13-10000`,
  V14; and `xhuman5-30000` n = 26761, V34 below, where the walk aimed at a landing transport's destination finds no
  route because the destination, outside the transport's zone, isn't a line tile: `movement/pathfinding.md`,
  "Step 2", V51).

Each time the order's logic runs (order 34's logic doesn't start with the attacker reaction; only step 5 runs it):

1. **Transport lost:** if the transport is dying or gone, inside something or no longer detected by the unit's owner,
   the target is forgotten, and so is the recorded attacker if it is lost in the same way, and idle becomes the
   **pending** order. No reaction (static study only).
2. **Goal:** the transport's tile if the transport stands in the unit's region; otherwise the transport's own
   destination tile (where it is heading). The arrival radius is 1 (`orders/orders.md`). The walk sets this goal again
   each time it runs, from where the transport stands then (`movement/pathfinding.md`, "Step 1"; the destination as
   the walk goal is shown by a break capture at `xhuman5-30000` n = 26740 to 26761, V34). After command 4
   (step 3) the walk aims at the transport's tile, wherever it stands (traced: `xhuman5-30000` n = 26764).
3. **Not arrived:** the unit is given a move aimed at the transport (`orders/orders.md`, "Boarding moves"), with no
   reaction (static study only). This step is reached after a no-route restart (above):
   - a **computer player's** unit gets command 4 (move and attack on the way) and **leaves order 34**. A unit standing
     still, as after the restart, takes order 4 at once (still animation, wait 3, target the transport; traced:
     `human13-10000`); a walking one at its next step boundary. It boards only if it is given the boarding command
     again;
   - **every other player's** unit (human, passive computer, passive rescue, neutral; `orders/orders.md`, "Terms") gets
     a smart move (command 3). That is the boarding command again, so the unit **stays in order 34**, and the transport
     is told to come again unless the transport's order or pending order is landing (order 30). The only split is
     computer player or not, so passive and rescue players' units take this branch too, with the smart move's wait cap
     of 10 (static study only; no trace shows it). The smart move sets order 34 again and it takes over in the same
     update; this has no visible effect, so the net result is the transport call and the wait cap, with the unit still
     in order 34 and nothing left pending (static study; `human12-transport` and the row-f traces agree).
4. **Arrived:** the unit boards if all of these hold:
   - it is within **2 tiles** of the transport's tile on both axes;
   - the transport's order is **32** (docked);
   - the transport has an empty slot (the lowest-numbered empty slot is used).

   Boarding takes the unit off the map:
   - on its old tile, the ground-unit mark (0x0100) is cleared, and the tile's ground-unit pointer is zeroed if it
     names this unit (`movement/occupancy.md`);
   - its tile position and pixel position become the transport's. It is **not** entered on that tile: it sets no
     mark and no pointer anywhere while aboard;
   - it records the transport as its container, forgets its attacker, and gets order **36** at once. No reaction runs
     first, so a unit with an attacker recorded boards and doesn't turn on it (traced:
     `orc12-orc12-board-attacked-1000` n = 661, V31).
5. **Arrived, but can't board:**
   - a **computer player's** unit whose transport is landing (order 30) with the transport's tile in the same region as
     the unit's tile walks to a random nearby tile. Two draws of the game's generator (`core/timing-and-rng.md`), x
     first: x = the unit's tile x + (r mod 12) − 6, then y = its tile y + (r mod 12) − 6. Each coordinate is clamped to
     0 to map size − 1 (the game keeps one size for both axes; maps are square). The unit gets a move command there with
     no target unit, which takes over in the same update (order 3, still animation, wait 3, target null; traced:
     `orc14-10000`, V13). This test comes before the next one;
   - otherwise, if the transport's destination tile is in the unit's region, the unit runs the **reaction to an
     attacker** (`orders/orders.md`, "Reacting to an attacker") and, if that gives it no new order, keeps waiting in
     order 34 (static study only);
   - otherwise it gives up: idle becomes the **pending** order, which takes over in the same update (still
     animation, wait 3, target none; traced: `orc12-orc12-guard-board-full-900` n = 607, V38, a unit
     waiting at a full docked transport that pushes off).

### Passengers and the transport's death

- A passenger has order 36, which does nothing. A unit inside something never receives a pending order
  (`orders/orders.md`), so it stays aboard until it is unloaded.
- When a transport dies, every passenger inside it dies too.
- In **Enhanced mode** only, a passenger that dies while aboard is also removed from its slot. In Classic mode the
  slot keeps pointing at the dead unit (open question below).

## Data

| Item | Value |
| --- | --- |
| Passenger slots | 6 |
| Unload order | slot 6 first, slot 1 last |
| Unload search | 1 × 1 at the transport's tile, limit 6 (3 rings), same region as the transport's tile |
| Push-off search | 1 × 1 at the transport's tile, limit 10 (5 rings), even tile, sea mask 0x0903 clear |
| Shore-target search | 1 × 1 at the target tile, aimed at it, limit 8 (4 rings), shore water, 0x0901 clear, target's region |
| Pull-back usable test | region of the transport's tile, 0x0803 clear |
| Boarding distance | at most 2 tiles from the transport's tile on each axis |
| Transport must be in order | 32 to accept a passenger; 29 or 32 to unload one |

## Test vectors

Vector IDs are stable: they number the vectors in the order the spec held them before the consolidation, so the
review file can map each one. "Source" says whether a vector is from an oracle trace (the trace is named in the
row and in the Source column), from static study, or a worked example of the rules. A row that builds on another
row starts "As" that row's ID, and names the trace when that row is from one.

### Commands and the straight line

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V02 | Transport at sea (order 2), smart move to a shore-water tile | Order 30 | Example |
| V03 | Transport at sea, smart move to an open-water tile | Order 3 | Example |
| V04 | Transport docked on shore water, smart move to an open-water tile far out | Order 31; on reaching an even open-water tile, idle and then order 3 to the tile | Example |
| V05 | Transport docked on shore water at S, smart move to a land tile G of the same region, with no open water between | Order 32; target tile becomes S; no movement | Example |
| V21 | Straight line, S = (20, 20), G = (25, 22) | Tiles tested: (20, 20), (21, 21), (22, 21), (23, 21), (24, 22); the walk stops at (25, 22) | Example |
| V25 | Human04 n = 1 (oracle trace): transport (slot 1543) at (68, 88), open water, region 0. Smart move with the great hall (slot 1550) as the target unit, G = (81, 84), land | Target unit dropped. The line from (68, 88) to (81, 84) has M = 13, m = 4, and the walk back passes (81, 84), (80, 84), (79, 85), (78, 85), (77, 85), (76, 86), (75, 86), (74, 86), (73, 87), (72, 87), (71, 87), (70, 87), (69, 88). The first usable tile is (69, 88) (open water, region 0), so K = (70, 87). (70, 87) is shore water, with no bit of 0x0901 and the same region value as (81, 84), so the target tile and destination are (70, 87): order 30, wait 3 at n = 1. It steps to (69, 87) at n = 4 and (70, 87) at n = 20, and docks at n = 36: order 32 | Trace (Human04) |
| V26 | As V25 (Human04): the same transport at n = 51, docked on (70, 87) (shore water); the same smart move | Target unit dropped. It isn't on open water, so G stays (81, 84). First rule (S shore water, same region, land first): order 32, target tile S. It stays docked; the same happens every 50 cycles | Trace (Human04) |

### Landing at the shore

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V12 | `human13-10000` (oracle trace), n = 3820: transport 1389 (type 0x1D, owner 0) in order 30 at (103, 13), landing goal (101, 12) | Plans afresh: tile (102, 13), facing 6, order 30 (the route planned two steps earlier would step to (102, 12), facing 7) | Trace (`human13-10000`) |
| V24 | Human04 (oracle trace), transport slot 1543, landing at (70, 87) from (68, 88) | n = 1: order 30, animation 6, wait 3. Steps to (69, 87) at n = 4 and (70, 87) at n = 20, still animation 6 (the goal has odd y, so one-tile steps). n = 36: order 32, animation 2 | Trace (Human04) |
| V30 | `human12-transport` (oracle trace): transport 1522 at (68, 34), order 30 from n = 8, goal (72, 29) | n = 11 (4 and 5 tiles away): ordinary two-tile step north-east to (70, 32), animation 3. n = 43 (within 3 tiles): animation 6, the landing logic steps one tile north-east to (71, 31); n = 59 (72, 30); n = 75 (72, 29) | Trace (`human12-transport`) |

### Pushing off from the shore

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V19 | Transport docked at (21, 20) (odd x, one-tile mover, landing class) on shore water, smart move to an open-water tile G = (40, 20); open water (no sea-mask bit) is every tile with x ≥ 22 | Order 31, target tile G. Run 1: odd tile, so the ring search runs from (21, 20) aimed at (40, 20): start corner (22, 19), first tile tested (22, 20) passes. The target tile becomes (22, 20) (the destination stays (40, 20)); one-tile step east to (22, 20). Run 2: (22, 20) is an even open-water tile: sea class, two-tile mover, idle, then a smart move to (40, 20): order 3 | Example |
| V20 | Transport docked at (20, 20) (even) on shore water, smart move to G = (40, 20); open water is every tile with x ≥ 24 | Run 1: even tile but not open water. The ring search aimed at (40, 20) finds (24, 18) after 53 tests (counting repeated tiles); it becomes the target tile; two-tile mover, move animation, first step toward (24, 18). Each later run repeats the search aimed at (24, 18) before stepping. On reaching (24, 18): sea class, idle, smart move to (40, 20) | Example |

### Unloading and passengers

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V01 | Transport docked (order 32) on shore water, 4 passengers in slots 1, 2, 5, 6, unload command at its own tile | G = S, so order 30; arrives at once, marked unload on arrival, so order 29; passengers leave in the order slot 6, 5, 2, 1; then order 32 | Example |
| V22 | Unloading at a shore whose land tiles within 3 tiles are all occupied | Each passenger stays aboard; the transport still docks (order 32) | Example |
| V23 | Transport carrying 3 passengers is destroyed | All 3 passengers die | Example |
| V27 | `human12-transport` (oracle trace): transport 1428 (type 0x1C) arrives at (16, 120) on shore water at n = 533 carrying demolition squads 1430, 1431, 1433 and 1434 (type 0x0E) | n = 534: the four squads at (18, 120), (18, 119), (16, 119) and (17, 119), order 2, wait 1, target null, each with its facing from boarding. 1428: order 32, wait 3 | Trace (`human12-transport`) |
| V28 | As V27 (`human12-transport`): the same cycle, draws | 12 draws. The squads' look-around draws (values 25617, 7919, 24438 and 18033) come before critter 1566's draw (22974, no wander) | Trace (`human12-transport`) |
| V29 | As V27 (`human12-transport`): the same cycle, with each passenger's idle logic run at the unload (wrong) | 9 draws; the critter gets 25617, a wander. Differs from the trace | Example |
| V39 | `human12-unload-then-move-900` (oracle trace, Enhanced; scenario `oracle/scenarios/human12-unload-then-move.txt`): as `human12-transport`, transport 1428 with four squads aboard told at n = 300 (scenario line; turn record n = 302) to unload at the land tile (18, 118) (order 30 from n = 306), then at n = 400 (turn record n = 402) given a smart move to the same tile | n = 405: order 31 (push off); n = 408: order 30; n = 427 at (21, 120), shore water, landing with no route: order 32, still animation, wait 1, passengers aboard (the mark was cleared by the second command; a kept mark would give order 29) | Trace (`human12-unload-then-move-900`) |

### Boarding and waiting

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V06 | Land unit, smart move on its own side's docked transport with 6 passengers | Order 34; on arrival it can't board (no empty slot); if the transport's destination is in its region it keeps waiting, else it becomes idle | Example |
| V07 | Land unit (human player) in order 34 at (24, 21), in region L. Its transport is docked (order 32) at (20, 20), whose tile is in another region W, with a free slot; the transport's destination tile is (23, 20), in region L (for example left by a landing command whose route failed on the shore) | Goal: the destination (23, 20), because the transport isn't in the unit's region. The unit is within 1 tile of it, so it has arrived, but it is 4 tiles from the transport on x, so it doesn't board. The transport isn't landing and its destination is in the unit's region, so the unit keeps waiting in order 34, without moving, at every run until something changes | Example |
| V08 | As V07, but the destination tile (23, 20) is in a third region | The unit gives up: idle becomes the pending order (order 2, still animation, wait 3 when it takes over; static study) | Static study |
| V09 | Passive computer player's grunt in order 34, transport 5 tiles away on the shore, not landing, and its logic runs (static study only) | Gets a smart move again: stays in order 34, and the transport is told to come | Static study |
| V10 | `human10-24000` (oracle trace): computer grunt 1473 (type 0x01, owner 1) at (6, 4) gets order 34 on transport 1480 (docked at (23, 7)) at n = 16951 | Order 34 throughout, walking round a wall (animation 3 from n = 16954); n = 17216 at (20, 5): order 34, animation 2, wait 3, the take-over of the boarding command that the computer player's AI gave again at n = 17201 while the grunt walked (pending order 34; break captures, round 70), not a no-route restart; n = 17254: order 36 (aboard) | Trace (`human10-24000`; break captures `human10` n = 17190 to 17270) |
| V11 | `human12-7000` (oracle trace), n = 6371: computer grunt 1407 (owner 0) walking to transport 1522 | Order 34, animation 3, tile (84, 16) | Trace (`human12-7000`) |
| V13 | `orc14-10000` (oracle trace), n = 5486: computer knight 1430 (type 0x06, owner 6) at (28, 25) in order 34, can't board; transport 1435 (type 0x1C) landing (order 30) at (28, 27), in its region | Draws give offsets −3 and +5: order 3 to (25, 30), animation 2, wait 3, target null. That tile has no route: idle at n = 5489 | Trace (`orc14-10000`) |
| V14 | `human13-10000` (oracle trace), n = 9154: computer ogre 1383 (type 0x07, owner 4) at (14, 2) in order 34 on transport 1388 (type 0x1D), docked at (25, 21), finds no route | Restart, then the same-update logic: order 4 on 1388, animation 2, wait 3. n = 9157: order 4 finds no route: order 2, target null | Trace (`human13-10000`) |
| V15 | `orc14-10000` (oracle trace), n = 2421: computer knight 1433 (type 0x06, owner 6) in order 34 to transport 1434 (type 0x1C) at (92, 32); its route search ran once, at n = 2409 from (86, 23), goal (91, 31) | Goal zone x 91 to 93, y 31 to 33 (the transport's footprint widened), not x 90 to 92, y 30 to 32: tile (87, 25), facing 4, order 34 | Trace (`orc14-10000`) |
| V16 | `human12-transport-650` (oracle trace), n = 45: transport 1428 (owner 1, a human player) docks at (35, 114) (order 32, wait 1); in the same cycle, after it, unit 1434 (order 34 on 1428) at (34, 112) finds no route and restarts (animation 2, wait 3) | n = 46: transport order 32, still animation, wait 3 (its still animation would otherwise show wait 1 or 4) | Trace (`human12-transport-650`) |
| V17 | As V16 (`human12-transport-650`), n = 73, 76, 79 and 82: 1434 restarts again (no route), the transport's wait at 1 | n = 74, 77, 80 and 83: transport order 32, wait 3 | Trace (`human12-transport-650`) |
| V18 | As V16 (`human12-transport-650`), n = 29 to 44: the units' restarts while transport 1428 is landing (order 30) | The transport isn't affected: animation 6, wait 1 throughout | Trace (`human12-transport-650`) |
| V31 | `orc12-orc12-board-attacked-1000` (oracle trace, Enhanced; scenario `oracle/scenarios/orc12-board-attacked.txt`), n = 602 (turn record of the input at n = 600): ogre 1439 (type 0x07, owner 0, the human player), fighting player 3's ogre 1442 (type 0x07, order 12 on 1439), is smart-moved onto its own transport 1434 (type 0x1D), docked (order 32) at (28, 120). 1442 hits it again at n = 608 (hp 38) | Order 34 from n = 622; it walks (31, 119), (30, 119), (29, 119) with 1442 following and still targeting it. n = 661, at its step boundary next to the transport: aboard (order 36, target 1434, at (28, 120), still animation); no attack on 1442. n = 662: 1442 targets the transport | Trace (`orc12-orc12-board-attacked-1000`) |
| V32 | A unit in order 34 with a valid attacker reaches its transport, which isn't docked; the transport's destination is in the unit's region (static study) | The reaction runs: a combat unit whose search picks a target attacks it (order 12); a worker hit by a worker fights back; any other non-combat unit doesn't flee (order 34 isn't one of the flee rule's orders, `orders/orders.md`, "Fleeing") and keeps its attacker. With no new order it keeps waiting in order 34 | Static study |
| V33 | A unit in order 34 with an attacker whose transport dies while it walks (static study) | At its next step boundary the target is forgotten and idle becomes the pending order; no reaction | Static study |
| V34 | `xhuman5-30000` (oracle trace, Enhanced, no input): computer grunt 1480 (type 0x01, owner 7) in order 34 on orc transport 1520 (type 0x1D, owner 7), standing at (102, 108) from n = 26740; 1520 is landing (order 30) at (96, 98) from n = 26737; axethrower 1587 (type 0x09, owner 7), also boarding, at (101, 107). The grunt's walk goal is 1520's destination (100, 106), in its region, outside 1520's widened footprint (a break capture from n = 26740 to 26761 whose log isn't committed) | n = 26761: the walk finds no route (the only line tile, (101, 107), is blocked, and G isn't a line tile); the restart's step 3 gives command 4: order 4, still animation, wait 3, target 1520 | Trace (`xhuman5-30000`) |
| V35 | As V34 (`xhuman5-30000`), n = 26764 | 1480 steps north to (102, 107), facing 0: order 4's walk aims at 1520's tile (96, 98) | Trace (`xhuman5-30000`) |
| V36 | `orc12-orc12-guard-board-full-900` (oracle trace, Enhanced; scenario `oracle/scenarios/orc12-guard-board-full.txt`), n = 22 (turn record of the input at n = 20): four peons (type 0x03) and two goblin sappers (type 0x0F) are told to board orc transport 1434 (type 0x1D, owner 0), docked (order 32) from n = 78 | All six aboard (order 36) by n = 300: the transport is full | Trace (`orc12-orc12-guard-board-full-900`) |
| V37 | As V36 (`orc12-orc12-guard-board-full-900`), n = 302 (turn record of the input at n = 300): ogre 1443 (type 0x07, owner 0) told to board 1434 | Order 34 from n = 303; it waits next to the transport at (20, 121) from n = 342 | Trace (`orc12-orc12-guard-board-full-900`) |
| V38 | As V36 (`orc12-orc12-guard-board-full-900`), n = 602 (turn record of the input at n = 600): 1434 told to move to (26, 118) | n = 607: 1434 pushes off (order 31); in the same cycle 1443 gives up: order 2, still animation, wait 3, target none | Trace (`orc12-orc12-guard-board-full-900`) |

## Open questions

Carried over:

- **No-route restart by a computer player's unit.** A human player's restart tells the transport to come again
  (`human12-transport`). For computer players' units, round 70's break captures of `human10` showed that grunt 1473's
  records at n = 17216 and 17251 aren't restarts at all: both are take-overs of the boarding command that the computer
  player's AI gave it again (at n = 17201 while it walked, and at n = 17251), and the transport's take-over at
  n = 17254 follows the second of these. So the static reading gains support: a computer player's restart doesn't
  tell the transport, and the take-overs seen come from the AI giving the boarding command again (`ai/attack-ai.md`).
  `human12-7000` (1407 at n = 6403 and 6454) wasn't recaptured. A capture of a computer army waiting for a ferry, with
  the boarding unit's route blocked so it really restarts, would settle it.
- In Classic mode, a passenger that dies aboard stays in its slot. What unloading such a slot does is unknown.
- The docking sound's identity. Its rules are now in "Moving near the shore" (static study, round 70).
- Whether the interface offers the unload command with a target tile at all, or always at the transport's own tile.
- **Passenger facing.** The static study shows no turn when a passenger is put ashore (an earlier text said it faces
  its target). The `human12-transport` passengers keep their boarding facing, which doesn't settle it. A trace where a
  passenger's target differs from its boarding facing would. Default: no turn.

Found by the consolidation (static study, 2026-10-14; the rules above are unchanged):

- **A transport lost while the unit walks to it.** "Order 34 (board)" says that when the walk finds the transport lost
  at a step boundary, step 1 runs, and V33 gives the outcome (target forgotten, idle pending, no reaction). The static
  study reads it differently: the walk's own target test drops the transport first, and step 1's test passes when
  there is no target at all, so step 1 doesn't fire. The logic then goes on with no transport: measured to the last
  stored goal, a unit not in reach is given its move (step 3) with no target unit, aimed at tile (0, 0) (order 4 for
  a computer player's unit, order 3 for any other), and a unit in reach tests a transport that isn't
  there, with an undefined outcome. Untraced either way; V31's transport doesn't die. The step-1 bullet and V33 keep
  governing until a trace decides. Recommendation: a trace that sinks a transport while a unit walks to it, for a
  human player's unit and a computer player's unit.
- **Same-update take-over after a block in orders 30 and 31.** When the landing or push-off logic is blocked, the
  blocked rules run inside the order's logic, which then counts as having given the unit an order. So the pending idle
  that rows C and G give takes over **in the same update** (still animation, wait 3), not at the next step done as for
  a walking unit. No trace has a transport give up or stop with an attacker while landing or pushing off.
- **The waits when a transport docks or goes idle** are now in the steps (round 70, static study): pushing off with
  no tile found docks as a pending order (same update, wait 3), with no route as an immediate order (wait 1);
  unloading docks as a pending order (same update, wait 3) on or off shore water; landing's arrival and no-route
  outcomes (orders 29, 32 and 2) are immediate orders. The Human04 vector V24 (order 32 at n = 36) and `orc8-10000`
  (movement V19: idle, wait 1) fit this; the push-off and unload cases are untraced. Trace request: a transport told
  to push off toward a tile with no open water in 5 rings, and one told to unload on open water.
- **Landing and pushing off forget a dying attacker.** Both forget a recorded attacker that is dying at each run, as
  the walk does for orders that neither acquire nor react (`orders/orders.md`, "Walking orders", step 1). Landing's
  steps don't say so. No outcome is known to differ: the attacker reaction doesn't run in either order.
- **Pushing off's tile pick makes the last steps of a command.** Besides dropping the cached path and clearing the
  blocked counter (step 2), picking a tile also pulls the new target tile back (`movement/pathfinding.md`, "Goal
  pulled back") and checks the attacker and the target as a command does. The found tile is even open water, so the
  pull-back keeps it unless it lies in another region than the transport; the check overlaps step 3. No known case has
  the tile in another region.
- **Step 3's "a walking one at its next step boundary".** The static study finds no way to reach step 3 with the unit
  walking: the order's logic runs only when the walk has ended, and command 4 always takes over in the same update.
  If confirmed, the clause can go.
- **Order 30 with no route farther than 3 tiles from its goal.** "Landing (order 30)" says the landing logic runs only
  within 3 tiles. The static study finds that when the ordinary walk finds no route farther out, the landing logic
  runs in that update too: it plans again in the landing class and, if it also finds no route, applies its own step 6
  (on an even tile away from the shore, idle in the sea class), not the general no-route rule of `orders/orders.md`
  ("No route": immediate idle with the idle logic in the same update). The two can differ in that update's draws and
  in whether the transport moves. `orc8-10000` n = 3555 (movement V19: no route, idle, wait 1) fits both, so it doesn't
  separate them. Untraced.
- **Statements made explicit by the consolidation, static study only and untraced:** the no-route restart of order 34
  always reaches step 3, so a human player's transport call is step 3's smart move, and a computer player's unit gets
  command 4 there ("Order 34 (board)"). Round 70 removed the conflict with V10: its n = 17216 record is the take-over
  of a re-issued boarding command, not a restart (break captures). The record of 1407 at `human12-7000` n = 6454 is
  still unexplained and likely the same.

## History

Erratum rounds that shaped this spec and the sections where their content now lives. The erratum notes for each
round are kept with the research-side specs.

| Round | Content | Now in |
| --- | --- | --- |
| 4 | A lost target or attacker (pushing off) is one that is no longer valid or no longer **detected** by the owner; fog over it doesn't matter (was "no longer visible") | "Pushing off (order 31)", step 3 ("Forget lost targets") |
| 5 | Boarding moves split computer players (command 4, leaves order 34) from every other player (smart move, stays in order 34), the other way round from "human-controlled" and "any other" | "Order 34 (board)", step 3 |
| 31 | New paragraph "Land and shore targets" (a land or shore-water target drops the target unit and, from open water, pulls the target tile back to shore water); the open question on it is closed; traced on Human04 | "Land and shore targets"; V25, V26 |
| 33 | Landing and pushing off keep animation 6 on one-tile steps; on two-tile steps the move animation is started and run at once | "Landing (order 30)", step 2; "Pushing off (order 31)", step 4; "Moving near the shore" |
| 36 | The landing logic runs only within 3 tiles of the goal; farther out the transport takes ordinary two-tile steps | "Landing (order 30)"; "Moving near the shore"; V30 |
| 39 | A passenger put ashore becomes an immediate idle whose idle logic runs at its own update later in the cycle, and keeps its facing (new open question); order 32 takes over for the transport in the same update (wait 3); `human12-transport` unloading vectors | "Unloading (order 29)"; V27 to V29; "Open questions" |
| 40 | "Land and shore targets" runs for every command given to a transport | "Land and shore targets" |
| 42 | Order 34 is a walking order whose logic runs on arrival, so a computer player's boarding unit stays in order 34 while it walks; with no route it restarts order 34 at once (destination in its region) or goes idle at once; step 3 is marked static study only; `human10-24000` and `human12-7000` vectors; two open questions | "Order 34 (board)"; V10, V11; "Open questions" |
| 43 | A human player's no-route restart of order 34 tells the transport to come again (a docked transport's order 32 takes over again with wait 3); `human12-transport` vectors; the open question on the restart now covers computer players' units only | "Order 34 (board)"; V16 to V18; "Open questions" |
| 44 | The open question on a computer player's no-route restart gives the static reading (the 50-cycle army pass tells the transport, the restart doesn't); still untraced | "Open questions" |
| 52 | The boarding walk's route search has the transport as its target unit, so its goal zone is the transport's footprint widened by one tile; `orc14-10000` vector | "Order 34 (board)"; V15 |
| 53 | Within 3 tiles of its goal a landing transport plans a new route at every step; a computer unit that can't board a transport landing in its region walks to a random tile (two draws); after the no-route restart the order logic runs in the same update, so a computer unit out of reach takes command 4 at once; `human13-10000` and `orc14-10000` vectors; two open questions closed or narrowed | "Landing (order 30)", step 5; "Order 34 (board)", steps 3 and 5; V12 to V14 |
| 54 | The boarding answers: every player that isn't a computer player, passive and rescue included, gets the smart move after the restart (open question closed); the random tile is clamped to the single map size | "Order 34 (board)", steps 3 and 5 |
| 56 (answers) | The boarding unit's smart move sets order 34 again and it takes over in the same update, with no visible effect (static) | "Order 34 (board)", step 3 |
| 64 | Order 34 runs the attacker reaction only when it has reached its transport, can't board and keeps waiting; a lost transport, a move toward it and boarding make no reaction (traced for boarding); a lost transport and giving up make idle the pending order (static study only); `orc12-orc12-board-attacked-1000` vector; open question on `xhuman5-30000` n = 26761 | "Order 34 (board)", steps 1, 3, 4 and 5; V31 to V33 |
| 65 | The open question on `xhuman5-30000` n = 26761 is closed: the boarding walk aimed at the transport's destination finds no route because G isn't a line tile (`movement/pathfinding.md`), and the no-route restart gives command 4; the boarding walk's goal is set again from the transport at every walk (step 2); giving up is traced (the pending idle takes over in the same update); vectors | "Order 34 (board)", steps 2 and 5; V34 to V38 |
| B-038 (2026-10-14) | Consolidated into one model: "The model" (the cycle and its four shared rules) and "Moving near the shore" added; vectors given IDs, a Source column and topic groups; the round citations moved here; one statement made explicit and eight open questions added. No rule changed | Whole spec |
| 70 | V10 explained (re-issued boarding command, break captures); docking sound on ordinary steps; far landing steps; push-off and unload waits; push-off done clears the counter; every command records target, tile and destination (static study) | "Moving near the shore"; "Pushing off"; "Unloading"; "What the transport stores"; V10; "Open questions" |
| 71 | A player's command clears the unload-on-arrival mark (traced, V39) | "Commands given to a transport"; V39 |
| 71 | Recorded once per command; no second recording at the smart-move stage; a command with no tile records the tile fields it carries (static study, implementer questions) | "What the transport stores" |
