# Orders: transports (boarding, landing, unloading)

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), and the unit flags in the
  public PUD specification text. Not yet confirmed against the running game, except "Land and shore targets"
  (oracle trace, research-side command logs and map dumps of Human04, round 31).
- **Changes:** 2026-10-11, round-4 errata: a lost target or attacker (pushing off) is one that is no longer valid or
  no longer **detected** by the owner; fog over it doesn't matter (was "no longer visible").
  2026-10-11, round-5 errata: boarding moves split computer players (command 4, leaves order 34) from every other
  player (smart move, stays in order 34), the other way round from "human-controlled" and "any other".
  2026-10-12, round-31 errata: new paragraph "Land and shore targets" (a land or shore-water target drops the target
  unit and, from open water, pulls the target tile back to shore water); the open question on it is closed.
  2026-10-12, round-33 errata: landing and pushing off keep animation 6 on one-tile steps; on two-tile steps the move
  animation is started and run at once.
  2026-10-12, round-36 errata: the landing logic runs only within 3 tiles of the goal; farther out the transport takes
  ordinary two-tile steps.
  2026-10-12, round-39 errata: a passenger put ashore becomes an immediate idle whose idle logic runs at its own update
  later in the cycle, and keeps its facing (new open question); order 32 takes over for the transport in the same update
  (wait 3); `human12-transport` unloading vectors.
  2026-10-12, round-40 errata: "Land and shore targets" runs for every command given to a transport.
  2026-10-12, round-42 errata: order 34 is a walking order whose logic runs on arrival, so a computer player's boarding
  unit stays in order 34 while it walks; with no route it restarts order 34 at once (destination in its region) or goes
  idle at once; step 3 is marked static study only; `human10-24000` and `human12-7000` vectors; two open questions.
  2026-10-12, round-43 errata: a human player's no-route restart of order 34 tells the transport to come again (a docked
  transport's order 32 takes over again with wait 3); `human12-transport` vectors; the open question on the restart now
  covers computer players' units only.

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

### Commands given to a transport

A smart move (command 3) to a transport, or the **unload** command (command 29), is decided as follows. G is the
target tile and S the transport's tile.

**What the transport stores.** Giving a transport any command records three things: the **target unit** (if the
command has one), the **target tile** (the target unit's tile, or the command's tile), and the **destination**, a
copy of the target tile made at that moment. Later steps may change the target tile (docking at S, pushing off), but
they never change the destination, which is what "its destination tile" means below.

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
   plays the docking sound.
6. If there is **no route**:
   - on shore water: order 29 if marked unload on arrival, else order 32;
   - elsewhere on an even tile: back to the sea class, idle (order 2);
   - elsewhere on an odd tile: order 32.

   The unload-on-arrival mark is cleared.

### Pushing off (order 31)

Order 31 moves the transport by itself: it is not a moving order in the sense of `orders/orders.md`, so the general
movement logic (path cache, arrival test) doesn't run for it. Instead, each time the order's logic runs:

1. **Done:** if the transport stands on an even tile of **open water**, it returns to the sea class, becomes a
   two-tile mover and becomes idle, and is then given a smart move to its **destination** tile, with the same
   **target unit** if it still has one (a smart move with a target unit aims at that unit's current tile instead).
   The command rules above then run again from open water. Nothing else happens in this run.
2. **Pick a tile:** otherwise it runs the push-off ring search of `movement/placement.md`: a 1 × 1 source at the
   transport's tile, aimed at its current **target tile**, limit **10** (5 rings); a tile passes when both
   coordinates are even and no bit of the sea mask (0x0903) is set. If none passes, the transport docks (order 32)
   and the run ends. If one passes, it becomes the target tile (the destination is not touched), the cached path is
   dropped and the blocked counter is cleared.
3. **Forget lost targets:** a target unit or attacker that is no longer valid or no longer detected by the
   transport's owner (fog doesn't matter; see `vision/fog.md`, "What uses vision") is forgotten. If the target unit
   was forgotten, idle (order 2) is made the pending order, so the push-off ends at the next step boundary.
4. **Step size:** if the transport stands on an even tile, it becomes a two-tile mover and its move animation is
   started and run at once. On an odd tile nothing changes (the transport keeps animation 6, the order's own), so a
   transport that docked on an odd tile (a one-tile mover after landing) keeps one-tile steps until it reaches an
   even tile. Its movement class is not changed: it stays in the class it had (the landing class after a landing).
5. **Step:** it takes one step toward the target tile, using its cached path or a new search (`movement/pathfinding.md`).
   If the search finds no route, it docks (order 32). If the next tile is blocked, the blocked rules of
   `movement/movement.md` apply. Otherwise the step is taken as described in `movement/movement.md` (leaving a
   shore-water tile plays the docking sound).

The logic runs again at the next "step done" of the transport's animation, so the transport takes one step per run,
and re-runs the ring search before every step, now aimed at the tile it found last time.

### Unloading (order 29)

When the order's logic runs, if the transport stands on shore water, its passengers are put ashore **from slot 6
down to slot 1**. Then order 32 (docked) takes over in the same update, as a pending order does: still animation,
wait 3. It doesn't wait for a later step. If it isn't on shore water, it just becomes order 32.

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
unit of **any** player, computer players included, stays in order 34 while it walks to the transport. The order's
logic (the steps below) runs when the walk ends:

- on **arrival**, the steps below run;
- with **no route**, the unit restarts order 34 at once if the transport's destination tile is in its region, and
  becomes idle at once otherwise (`orders/orders.md`, "No route", the boarding case). For a human player's unit the
  restart also tells the transport to come again, as the boarding command does, unless it is landing: a docked transport
  stays in order 32, and order 32 takes over again at the end of its current wait (still animation, wait 3;
  `human12-transport`, vectors below). Whether a computer player's restart does the same isn't settled (open questions).

Each time the order's logic runs:

1. If the transport is no longer a valid target, the unit becomes idle.
2. **Goal:** the transport's tile if the transport stands in the unit's region; otherwise the transport's own
   destination tile (where it is heading). The arrival radius is 1 (`orders/orders.md`).
3. **Not arrived** (static study only: no trace shows a run of the logic that finds the unit not arrived): the unit
   is given a move aimed at the transport (`orders/orders.md`, "Boarding moves"):
   - a **computer player's** unit gets command 4 (move and attack on the way) and **leaves order 34**: its order
     becomes 4 at the next step boundary. It boards only if it is given the boarding command again;
   - **every other player's** unit (human, passive computer, passive rescue, neutral; `orders/orders.md`, "Terms")
     gets a smart move (command 3). That is the boarding command again, so the unit **stays in order 34**, and the
     transport is told to come again unless it is landing.
4. **Arrived:** the unit boards if all of these hold:
   - it is within **2 tiles** of the transport's tile on both axes;
   - the transport's order is **32** (docked);
   - the transport has an empty slot (the lowest-numbered empty slot is used).

   Boarding takes the unit off the map:
   - on its old tile, the ground-unit mark (0x0100) is cleared, and the tile's ground-unit pointer is zeroed if it
     names this unit (`movement/occupancy.md`);
   - its tile position and pixel position become the transport's. It is **not** entered on that tile: it sets no
     mark and no pointer anywhere while aboard;
   - it records the transport as its container, forgets its attacker, and gets order **36** at once.
5. **Arrived, but can't board:**
   - a computer player's unit whose transport is landing (order 30) in the unit's region moves to a nearby tile and
     tries again (which tile is still open);
   - otherwise, if the transport's destination tile is in the unit's region, the unit keeps waiting in order 34;
   - otherwise it gives up and becomes idle.

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

| Input | Expected |
| --- | --- |
| Transport docked (order 32) on shore water, 4 passengers in slots 1, 2, 5, 6, unload command at its own tile | G = S, so order 30; arrives at once, marked unload on arrival, so order 29; passengers leave in the order slot 6, 5, 2, 1; then order 32 |
| Transport at sea (order 2), smart move to a shore-water tile | Order 30 |
| Transport at sea, smart move to an open-water tile | Order 3 |
| Transport docked on shore water, smart move to an open-water tile far out | Order 31; on reaching an even open-water tile, idle and then order 3 to the tile |
| Transport docked on shore water at S, smart move to a land tile G of the same region, with no open water between | Order 32; target tile becomes S; no movement |
| Land unit, smart move on its own side's docked transport with 6 passengers | Order 34; on arrival it can't board (no empty slot); if the transport's destination is in its region it keeps waiting, else it becomes idle |
| Land unit (human player) in order 34 at (24, 21), in region L. Its transport is docked (order 32) at (20, 20), whose tile is in another region W, with a free slot; the transport's destination tile is (23, 20), in region L (for example left by a landing command whose route failed on the shore) | Goal: the destination (23, 20), because the transport isn't in the unit's region. The unit is within 1 tile of it, so it has arrived, but it is 4 tiles from the transport on x, so it doesn't board. The transport isn't landing and its destination is in the unit's region, so the unit keeps waiting in order 34, without moving, at every run until something changes |
| Same, but the destination tile (23, 20) is in a third region | The unit gives up: order 2 |
| Passive computer player's grunt in order 34, transport 5 tiles away on the shore, not landing, and its logic runs (static study only) | Gets a smart move again: stays in order 34, and the transport is told to come |
| `human10-24000` (oracle trace): computer grunt 1473 (type 0x01, owner 1) at (6, 4) gets order 34 on transport 1480 (docked at (23, 7)) at n = 16951 | Order 34 throughout, walking round a wall (animation 3 from n = 16954); n = 17216 at (20, 5) after a no-route step: order 34 restarted, animation 2, wait 3; n = 17254: order 36 (aboard) |
| `human12-7000` (oracle trace), n = 6371: computer grunt 1407 (owner 0) walking to transport 1522 | Order 34, animation 3, tile (84, 16) |
| `human12-transport-650` (oracle trace), n = 45: transport 1428 (owner 1, a human player) docks at (35, 114) (order 32, wait 1); in the same cycle, after it, unit 1434 (order 34 on 1428) at (34, 112) finds no route and restarts (animation 2, wait 3) | n = 46: transport order 32, still animation, wait 3 (its still animation would otherwise show wait 1 or 4) |
| Same trace, n = 73, 76, 79 and 82: 1434 restarts again (no route), the transport's wait at 1 | n = 74, 77, 80 and 83: transport order 32, wait 3 |
| Same trace, n = 29 to 44: the units' restarts while transport 1428 is landing (order 30) | The transport isn't affected: animation 6, wait 1 throughout |
| Transport docked at (21, 20) (odd x, one-tile mover, landing class) on shore water, smart move to an open-water tile G = (40, 20); open water (no sea-mask bit) is every tile with x ≥ 22 | Order 31, target tile G. Run 1: odd tile, so the ring search runs from (21, 20) aimed at (40, 20): start corner (22, 19), first tile tested (22, 20) passes. The target tile becomes (22, 20) (the destination stays (40, 20)); one-tile step east to (22, 20). Run 2: (22, 20) is an even open-water tile: sea class, two-tile mover, idle, then a smart move to (40, 20): order 3 |
| Transport docked at (20, 20) (even) on shore water, smart move to G = (40, 20); open water is every tile with x ≥ 24 | Run 1: even tile but not open water. The ring search aimed at (40, 20) finds (24, 18) after 53 tests (counting repeated tiles); it becomes the target tile; two-tile mover, move animation, first step toward (24, 18). Each later run repeats the search aimed at (24, 18) before stepping. On reaching (24, 18): sea class, idle, smart move to (40, 20) |
| Straight line, S = (20, 20), G = (25, 22) | Tiles tested: (20, 20), (21, 21), (22, 21), (23, 21), (24, 22); the walk stops at (25, 22) |
| Unloading at a shore whose land tiles within 3 tiles are all occupied | Each passenger stays aboard; the transport still docks (order 32) |
| Transport carrying 3 passengers is destroyed | All 3 passengers die |
| Human04 (oracle trace), transport slot 1543, landing at (70, 87) from (68, 88) | n = 1: order 30, animation 6, wait 3. Steps to (69, 87) at n = 4 and (70, 87) at n = 20, still animation 6 (the goal has odd y, so one-tile steps). n = 36: order 32, animation 2 |
| Human04 n = 1 (oracle trace): transport (slot 1543) at (68, 88), open water, region 0. Smart move with the great hall (slot 1550) as the target unit, G = (81, 84), land | Target unit dropped. The line from (68, 88) to (81, 84) has M = 13, m = 4, and the walk back passes (81, 84), (80, 84), (79, 85), (78, 85), (77, 85), (76, 86), (75, 86), (74, 86), (73, 87), (72, 87), (71, 87), (70, 87), (69, 88). The first usable tile is (69, 88) (open water, region 0), so K = (70, 87). (70, 87) is shore water, with no bit of 0x0901 and the same region value as (81, 84), so the target tile and destination are (70, 87): order 30, wait 3 at n = 1. It steps to (69, 87) at n = 4 and (70, 87) at n = 20, and docks at n = 36: order 32 |
| Same transport at n = 51, docked on (70, 87) (shore water); the same smart move | Target unit dropped. It isn't on open water, so G stays (81, 84). First rule (S shore water, same region, land first): order 32, target tile S. It stays docked; the same happens every 50 cycles |
| `human12-transport` (oracle trace): transport 1428 (type 0x1C) arrives at (16, 120) on shore water at n = 533 carrying demolition squads 1430, 1431, 1433 and 1434 (type 0x0E) | n = 534: the four squads at (18, 120), (18, 119), (16, 119) and (17, 119), order 2, wait 1, target null, each with its facing from boarding. 1428: order 32, wait 3 |
| Same cycle, draws | 12 draws. The squads' look-around draws (values 25617, 7919, 24438 and 18033) come before critter 1566's draw (22974, no wander) |
| Same cycle, with each passenger's idle logic run at the unload (wrong) | 9 draws; the critter gets 25617, a wander. Differs from the trace |
| `human12-transport` (oracle trace): transport 1522 at (68, 34), order 30 from n = 8, goal (72, 29) | n = 11 (4 and 5 tiles away): ordinary two-tile step north-east to (70, 32), animation 3. n = 43 (within 3 tiles): animation 6, the landing logic steps one tile north-east to (71, 31); n = 59 (72, 30); n = 75 (72, 29) |

## Open questions

- Which tile a computer player's boarding unit moves to when its transport is landing in its region.
- **Order 34 not arrived.** The static study shows step 3's two branches (command 4 for a computer player's unit, a
  smart move for every other), but the logic seems to run only on arrival or with no route. Whether any path reaches
  step 3 is open.
- **No-route restart by a computer player's unit.** A human player's restart tells the transport to come again
  (`human12-transport`). For computer players' units the traces are mixed: in `human12-7000` the restart of grunt 1407
  at n = 6403 is followed by the docked transport's take-over (wait 3 at n = 6405), and in `human10-24000` that of grunt
  1473 at n = 17251 (wait 3 at n = 17254); but after the restarts of 1473 at n = 17216 and of 1407 at n = 6454 the
  transport's still animation runs on unchanged. What decides it is open.
- In Classic mode, a passenger that dies aboard stays in its slot. What unloading such a slot does is unknown.
- The docking sound's identity and the exact sound rules.
- Whether the interface offers the unload command with a target tile at all, or always at the transport's own tile.
- **Passenger facing.** The static study shows no turn when a passenger is put ashore (an earlier text said it faces
  its target). The `human12-transport` passengers keep their boarding facing, which doesn't settle it. A trace where a
  passenger's target differs from its boarding facing would. Default: no turn.
