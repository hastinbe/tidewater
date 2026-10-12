# Orders: order list, commands, pending orders and target acquisition

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`) and the public PUD
  specification text (unit data section). Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-10 errata: a hit records its attacker only under the rules of `ai/attack-ai.md`
  ("Call for help"), and the first live attacker is kept, so the attacker is not always the last unit that hit.
  2026-10-12, round-13 errata: command 17 gives order 17 at once when the target tile is in the unit's own region.
  2026-10-12, round-14 errata: orders 33 and 14 forget the building's attacker each time their logic runs.

## Behaviour

Every unit always has exactly one **order** (a small number, listed below) and at most one **pending order**. The
order says what the unit is doing; its logic runs only when the unit's animation signals "step done" (see
`movement.md` and `animation-scripts.md`). Player and AI **commands** don't set orders directly: each command is
turned into an order by the rules in "Commands" below, and the result is normally stored as the pending order.

### Terms: detected, not hidden, and player kinds

**Two vision tests.** Both are computed each cycle by `vision/fog.md`, and the rules in this spec use them as follows:

- **Detected** by a player: the unit is in that player's detected-by set (`vision/fog.md`, "Detection"). Only
  Invisibility and submarines make a unit undetected. Fog has nothing to do with it.
- **Not hidden** from a player: the unit isn't in that player's hidden-from set (`vision/fog.md`, "Unit
  visibility"). This is the fog test: a unit standing in fog is hidden from a human player.

| Rule | Test |
| --- | --- |
| An order aimed at a unit loses its target ("Arriving and losing the target", and orders 14 to 16) | **Detected** only. A target that walks into fog stays the target |
| The same check also forgets the unit's attacker | **Detected** only |
| Automatic target acquisition (the candidate skip list) | **Detected** and **not hidden** |

`economy/repair.md` and `orders/transports.md` use the same test: a lost target or attacker is one that is no longer
valid or no longer **detected** by the unit's owner.

**Player kinds.** `movement/movement.md` ("Player kinds") names the kinds of player. The rules here use two
different splits:

- **Computer players** against **every other player**. "Every other player" means human players, passive computer
  players (including passive rescue players), and neutral or empty slots. Active rescue players are computer
  players.
- **Human-controlled players** against **every player that isn't human-controlled** (computer, passive computer,
  rescue, neutral or empty).

| Rule | Split | Applies how |
| --- | --- | --- |
| Smart-move wait cap ("Commands") | Computer players / every other player | 40 cycles / 10 cycles |
| Search box R ("Automatic target acquisition") | Computer players / every other player | react range (computer) always / react range (human) in a moving order that acquires, else attack range |
| Resulting-order table | Computer players / every other player | the "computer player" column / the "every other player" column |
| Fleeing, rules 2 and 3 | Computer players / every other player | rule 2 / rule 3 |
| Friendly-fire threshold | Human-controlled / not human-controlled | 1 own unit / 3 own units |
| Boarding moves (order 34) | Computer players / every other player | command 4 / smart move |
| Reaction: "the owner is not playing" | Neutral or empty slot | the attacker is forgotten |

### The order list

Columns:

- **Animation:** the animation number the unit plays while in the order (numbers as in `animation-scripts.md`:
  1 death, 2 still, 3 move, 4 attack, 5 and 6 type-specific work).
- **Radius:** the arrival radius in tiles (see `movement.md`, "Arriving"). "Always" means the unit counts as already
  there and never moves for this order. "Attack range" means the unit's attack range.
- **Moves:** the order uses the movement logic (`movement.md`) until the unit arrives.
- **Acquires:** the unit looks for enemies by itself while in this order (see "Automatic target acquisition"). "Idle"
  marks the orders that react the way an idle unit does.

| Order | Meaning | Animation | Radius | Moves | Acquires |
| --- | --- | --- | --- | --- | --- |
| 0 | Remove (frees the slot; see `core/unit-slots.md`) | 0 | always | no | no |
| 1 | Dying | 1 | always | no | no |
| 2 | Stop / idle | 2 | always | no | yes, idle |
| 3 | Move | 2 | 0 | yes | no |
| 4 | Move, attacking on the way | 2 | 0 | yes | yes |
| 5 | Patrol | 2 | 0 | yes | yes |
| 6 | Follow a unit | 2 | 1 | yes | no |
| 7 | Follow and guard a unit | 2 | 1 | yes | yes, idle |
| 8 | Never a unit's order: number 8 is only a command (attack), which becomes order 9, 10 or 11 ("Commands"). Its order logic reports an error | 4 | attack range | no | no |
| 9 | Attack a unit | 4 | attack range | yes | no |
| 10 | Attack-move to a location | 2 | 0 | yes | yes (see below) |
| 11 | Attack a wall tile | 4 | attack range | yes | no |
| 12 | Chase and attack a found target | 4 | attack range | yes | yes |
| 13 | Stand ground | 2 | always | no | yes |
| 14 | Tower watching (towers' resting order). Each time its logic runs, it forgets the tower's attacker | 4 | attack range | no | yes |
| 15 | Attack a found target without moving | 4 | attack range | no | yes |
| 16 | Idle unit's reaction: attack without moving | 4 | attack range | no | yes, idle |
| 17 | Attack the ground | 4 | attack range | yes | no |
| 18 | Move into range to attack the ground | 3 | 2 (special, below) | yes | no |
| 19 | Never a unit's order: number 19 is only a command (demolish), which becomes order 20 or 21. Its order logic reports an error | 4 | 0 | yes | no |
| 20 | Demolish at a unit or obstacle | 4 | 1 | yes | no |
| 21 | Demolish at a location | 4 | 0 | yes | no |
| 22 | Worker inside a building it is constructing. Its logic only forgets the unit's attacker and sets a long wait (255 cycles). Such units come out when the building is finished or destroyed (`movement/placement.md`) | 2 | 0 | yes | no |
| 23 | Harvest lumber | 4 | 1 | yes | no |
| 24 | Return goods to a depot | 2 | 1 | yes | no |
| 25 | Enter a gold mine or oil platform (can't be interrupted) | 3 | 1 | no | no |
| 26 | Inside a mine or platform | 2 | always | no | no |
| 27 | Repair (`economy/repair.md`) | 4 | 1 | yes | no |
| 28 | Go to a site and build | 2 | 0 | yes | no |
| 29 | Transport unloading (`orders/transports.md`) | 2 | always | no | no |
| 30 | Transport landing (`orders/transports.md`) | 6 | 0 in effect (3 in the order data, unused; see the note below the table) | yes | no |
| 31 | Transport pushing off from the shore (`orders/transports.md`) | 6 | always | no | no |
| 32 | Transport docked at the shore, or idle on an odd tile (`orders/transports.md`). No logic: it waits. (The order is marked as acquiring, but its logic never runs a search) | 2 | 0 | no | no |
| 33 | Building's resting order (every building except towers and the circle of power, complete or under construction; see `economy/build-train.md`). Each time its logic runs, it first forgets the building's attacker (the reaction rule, for every owner), so later hits call for help again | 2 | always | no | no |
| 34 | Board a transport | 2 | 1 | yes | no |
| 35, 40, 58, 60 | No logic (do nothing) | 2 | — | — | — |
| 36 | Passenger inside a transport (`orders/transports.md`). No logic | 2 | always | — | — |
| 37 | Building working: training, research or building upgrade (`economy/build-train.md`) | 5 | — | — | — |
| 57 | Not identified. Nothing found sets it. Its logic does nothing except let a pending order take over at once | 5 | — | — | — |
| 38 to 56 | Spells (table below) | 4 | per spell | yes | no |
| 59 | Circle of power's resting order | 2 | 0 | — | — |

**Order 30's radius.** The order data lists 3 for order 30, but nothing uses that value: a landing transport's
arrival is checked against radius 0 with no +1, so it arrives only on its exact goal tile (`movement/movement.md`,
"The +1", and `orders/transports.md`, "Landing"). An implementation can store 0.

Orders that count as **stationary or followable** (used by the follow rules below): 2, 3, 4, 5, 6, 7 and 10.

**Spell orders.** Every spell order moves the caster until the target is within the spell's radius, then casts.

| Order | Spell | Radius (tiles) |
| --- | --- | --- |
| 38 | Holy vision | always |
| 39 | Healing | 6 |
| 40 | Not used | 0 |
| 41 | Exorcism | 10 |
| 42 | Flame shield | 6 |
| 43 | Fireball | 10 |
| 44 | Slow | 10 |
| 45 | Invisibility | 6 |
| 46 | Polymorph | 10 |
| 47 | Blizzard | 12 |
| 48 | Eye of Kilrogg | always |
| 49 | Bloodlust | 6 |
| 50 | Raise dead | 6 |
| 51 | Death coil | 10 |
| 52 | Whirlwind | 12 |
| 53 | Haste | 6 |
| 54 | Unholy armour | 6 |
| 55 | Runes | 10 |
| 56 | Death and decay | 12 |

**Arrival radius exceptions** (completing `movement.md`):

- The **+1** that sea and air units get on the radius doesn't apply to orders **28** (build), **30** (transport
  landing) and **31** (transport pushing off).
- Order **18** counts as arrived as soon as the unit is within its **attack range** of the stored ground point on both
  axes, whatever the table radius says. The cached path is dropped at that moment.

### Orders given when a unit is created

| Unit | First order |
| --- | --- |
| Tower | 14 |
| Circle of power | 59 |
| Any other building (complete or under construction) | 33 |
| Anything else | 2 |

### Commands

A command carries a target unit or a target tile (or neither). It turns into an order as follows.

| Command | Result |
| --- | --- |
| 3, smart move (right-click) | See "Smart move" below |
| 4, move and attack on the way | A non-combat unit is first handled as a smart move; the order then becomes 4 |
| 5, patrol | Order 5 between the unit's position and the target tile. In Enhanced mode the unit also remembers "patrol", with both ends |
| 8, attack | Target unit: order 9. Target is a wall tile: order 11. Otherwise: order 10 (attack-move); in Enhanced mode the unit also remembers "attack-move" with the target tile |
| 9 | Target unit: order 9; wall tile: order 11; otherwise order 2 |
| 11 | As command 9 |
| 12 | Order 12 |
| 13, 15 | Order 13 (stand ground) |
| 14 | Order 14 |
| 16 | Order 16 |
| 17, attack ground | Order 17 if the target tile has the same region value as the unit's tile, or is already within the unit's attack range; otherwise order 18 (move into range, then attack). Details: `combat/attack-ground-demolish.md` |
| 19, demolish | Target unit: order 20. Target tile holds a tree, rock or wall (or a reserved tree): order 20. Otherwise: order 21 |

Commands 2, 6, 23, 24, 27, 29 and the spell commands have their own handlers, covered by later specs (stop, harvest,
return goods, repair, build and spells).

**Smart move (command 3):**

1. If the unit is currently waiting, the wait is cut to at most **40 cycles** for a computer player's unit and
   **10 cycles** for every other player's unit, so it reacts sooner.
2. Then the first matching rule applies:

| Unit and target | Result |
| --- | --- |
| The unit is a transport | Transport rules (`orders/transports.md`) |
| A land unit, and the target is its own side's transport | The transport is told to come; the unit gets order 34 (board) |
| A worker carrying goods, and the target is a depot that accepts them | Order 24 (return goods) |
| The target can be harvested by this unit (gold mine, oil patch or platform, tree) | Harvest rules (see the economy specs) |
| The target is its own side's unit (not a building) | Order 6 (follow) |
| Anything else | Order 3 (move) |

### Pending orders and immediate orders

**Setting a pending order.** A new order is written as the unit's pending order, replacing any earlier pending
order, except that nothing is written if:

- the unit is dying or dead, or is inside something (a transport, mine or platform); or
- its current order is **25** (entering a mine or platform), which can't be interrupted.

The pending order takes over at the unit's next "step done" signal (`movement.md` gives the timing: the new order's
animation starts with a first wait of 3 cycles).

**Taking over.** When the pending order takes over:

- if it is 24 (return goods) and has no target, the nearest suitable depot is looked up. If there is none, the order
  becomes 2 instead; otherwise the depot becomes the target;
- the order's animation starts with a wait of 3;
- a tree the unit had reserved is released (see `occupancy.md`).

**Immediate orders** (used for going idle, dying and giving up): the order is set at once, the pending order is
cleared, the order's animation starts with a wait of 1, and any reserved tree is released. When the immediate order
is 2 (idle):

- a mobile unit forgets its target;
- a transport whose tile is on even coordinates and isn't shore water settles there (sea class, two-tile mover);
  otherwise its order becomes **32** instead, which does nothing, so it waits where it is until it gets a new order.

Orders 0 and 1 also wipe the remembered order (Enhanced, below).

### Resuming patrol and attack-move (Enhanced only)

In Enhanced mode a unit can **remember** a patrol (both ends) or an attack-move (the target tile), set by commands 5
and 8. Each unit also has a **resume countdown**, which goes down by 1 every cycle until it reaches 0.

When such a unit is set to order 2 (idle) and its countdown is 0:

| Remembered | Result |
| --- | --- |
| Patrol | It is sent back toward the patrol end it was heading for, and the countdown is set to **40** |
| Attack-move, with the target tile more than 1 tile away (either axis) | The attack-move is given again, and the countdown is set to **20** |
| Attack-move, already within 1 tile | It stays idle (the attack-move is complete) |

So a unit that broke off a patrol or attack-move to fight goes back to it once the fight is over. The countdown stops
it from re-issuing every cycle when the move can't progress.

### Arriving and losing the target

- **Orders 3, 4 and 10:** on arrival the unit becomes idle (order 2) and forgets its attacker.
- **Order 5 (patrol):** on arrival the unit swaps ends and moves again.
- **Order 18:** on arrival (in range) it becomes order 17 and attacks the ground.

**Orders aimed at a unit** (follow, attack and similar) check their target every time their logic runs. The target
is **lost** if it is no longer valid (dead, removed or inside something) or is no longer **detected** by the unit's
owner (see "Terms"). Fog over the target doesn't matter: a target that walks into fog stays the target. When the
target is lost:

1. If a pending order exists, it takes over now.
2. Otherwise, order 15 (attacking in place) becomes **13** (stand ground), and every other order becomes **2**.

**Follow (orders 6 and 7):** each time the logic runs, with the target still valid:

1. If the follower's order is **7**, or a follow order (6) is pending, and the followed unit's own order is **not**
   one of 2, 3, 4, 5, 6, 7 or 10, the follower is given a plain move (command 3) to **its own tile**: it stops where
   it is. (A follower in order 6 with nothing pending skips this step.)
2. Otherwise, if the follower is outside its radius (1), it is given the follow again toward the followed unit.
3. When it is inside the radius, a follower in order 6 becomes order **7** (guard) if the followed unit's order is
   one of 2, 3, 4, 5, 6, 7 or 10, and idle (order 2) otherwise.

Order 7 then also runs the idle logic (target acquisition as for order 2), so a guarding unit attacks enemies that
come near and goes on following.

### Attacking without moving (orders 14, 15 and 16)

These orders never move the unit. They differ in what happens when the target is valid and detected but **outside the
attack range** (on either axis, measured as for arrival in `movement/movement.md`), whether it was out of range from
the start or walked out:

| Order | Target valid, detected, in range | Target valid and detected, out of range |
| --- | --- | --- |
| 14, tower watching | Fires at it | Can't happen: the tower keeps no target (below) |
| 15, attack without moving | Strikes | Forgets the target; order **13** (stand ground) takes over at the next step boundary |
| 16, idle reaction | Strikes | Order **2** (idle) takes over at the next step boundary; on taking over, the target is forgotten |

A lost target (invalid or not detected) is handled as in "Arriving and losing the target" for all three.

**Order 14 (towers).** A tower doesn't keep a target from one run to the next. Each time its logic runs, it forgets
its attacker and runs the target search of "Automatic target acquisition" with two differences: the box is always
its **attack range** (even for a computer player), and the search does **not** start with the attacker as the best
candidate. If the search finds a target, the tower fires at it in that run; if not, it does nothing until its next
run. So a target that walks out of range is simply not found again, and the tower fires at whatever is best in range.

**Friendly-fire check (orders 12, 15 and 16).** A unit whose type can attack the ground (unit flag bit 14 in the PUD
specification; in the default unit data: ballista, catapult, battleship and juggernaught) checks, before each
strike in these orders, the 3 × 3 tiles centred on the target's tile (a building's top-left tile; the target point
for a ground target), clipped to the map. It counts the units and buildings of **its own player** standing there
(not dying, dead or inside anything; one count per tile, using the unit or building recorded for the tile in
`movement/occupancy.md`, so a building counts once for each of its footprint tiles in the area):

- for a human-controlled player, one such unit is enough;
- for any player that isn't human-controlled (see "Terms"), it takes three.

If the count is reached, the unit doesn't strike, and order **2** (idle) takes over at the next step boundary.

### Boarding moves (order 34)

While a unit in order 34 hasn't yet reached its transport (`orders/transports.md`), each run of the logic gives it
a move again, aimed at the transport:

- **Every player except computer players** (human players, passive computer players, neutral; see "Terms"): a smart
  move (command 3). It meets the boarding rule of "Smart move" again, so the unit **stays in order 34** and the
  transport is told to come again (unless it is landing).
- **Computer players:** command 4 (move and attack on the way). The unit **leaves order 34**: its order becomes 4,
  aimed at the transport's tile. A non-combat unit is first handled as a smart move, as for any command 4, but its
  order still becomes 4. It boards only if it is later given the boarding command again.

### Automatic target acquisition

Units in an order marked "acquires" look for an enemy each time their logic runs. Enhanced mode adds acquisition to
attack-move (order 10).

**Search box.** The unit searches a square box centred on its own tile, extending R tiles on each side:

| Unit | R |
| --- | --- |
| A computer player's unit | Its type's **react range (computer)** from the unit data |
| Every other player's unit, in a moving order that acquires (4, 5, 12) | Its type's **react range (human)** |
| Every other player's unit, in any other order (including idle units) | Its **attack range** |

A tower's box reaches 1 tile further on the right and bottom sides. A candidate is considered if any tile of its
footprint is inside the box.

**Candidate order.** Candidates are visited in order of their vertical pixel position, top first (see "Vertical
order" below). This order decides ties.

**A candidate is skipped if:**

- its owner is allied to the searcher's owner, or is the searcher's owner;
- it is dying, dead or inside something;
- it is not **detected** by the searcher's owner (Invisibility, submarines), or it is **hidden** from that owner by
  the fog (see "Terms");
- the searcher can't attack its kind (air, sea or land, per the searcher's type). Submarines can attack only sea
  units, shore buildings and oil platforms;
- both are air units and the candidate can't attack;
- it is closer than the searcher's minimum attack range (the minimum range of its missile; see `damage.md`);
- in Enhanced mode, on attack-move or patrol, a sea unit only considers sea units, shore buildings and oil
  platforms.

**Distance.** The distance d is the larger of the horizontal and vertical tile distances from the searcher's tile to
a reference tile of the candidate:

- for a mobile candidate, its own tile;
- for a building of width w at left column x, the reference column is x if the searcher's column is at or left of
  x; the last column x + w − 1 if the searcher's column is at or right of x + 2·floor(w/2); and otherwise the middle
  column x + floor(w/2). Rows work the same way with the height.

**Score.** Each remaining candidate gets a score:

| Candidate | Base |
| --- | --- |
| Can't attack | 0 |
| Can attack, and d is greater than the searcher's attack range | 65536 |
| Can attack and is in range (ground or sea unit) | 131072 |
| Can attack and is in range, and is an air unit or a submarine | 196608 |

score = base + priority − floor(d² / 4), and at least 1. "Priority" is the candidate type's priority value in the
unit data.

**Choice.** The search starts with the unit's current **attacker** (the first attacker that is still alive; see
`ai/attack-ai.md`, "Call for help", steps 1 to 4) as the best candidate, with its own score. A candidate replaces the
best only with a **strictly greater** score. So on a tie the attacker wins, and otherwise the first candidate in
vertical order wins.

**Resulting order** when a target is chosen:

| The searching unit's order | Every other player | Computer player |
| --- | --- | --- |
| An idle-type order (2, 7, 16) | 12 (chase) whenever the unit has an attacker; otherwise 16 (attack without moving) | 12 whenever the unit has an attacker; otherwise 12 if the target is a building or can attack, else 16 |
| A moving order (4, 5, 10, 12) | 12 | 12 |
| Anything else (13, 14, 15) | 15 | 15 |

Order 32 (docked transport) carries the idle-type mark in the order data too, but its logic does nothing, and the
reaction in "Reacting to an attacker" gives order 12 without this table. So a unit in order 32 never reaches this
table; only transports use order 32, and they can't attack anyway.

**Special cases:**

- **Mages** (mage and death knight types, including heroes of those types) attack automatically only targets that
  can't attack or are non-combat units (workers, tankers, transports, demolition units).
- In Enhanced mode a **demolition unit** (dwarven demolition squad, goblin sapper) with a remembered order uses the
  target to start a demolish instead of an attack.
- Computer players' workers and tankers run AI routines here instead of the search (later spec).

**Vertical order.** The game keeps all units in a list sorted by **pixel y** (the vertical part of the pixel
position defined in `movement/movement.md`):

- The list is re-sorted **once per cycle**, at the end of phase 4 of `core/timing-and-rng.md`, after the units,
  missiles, construction pass and timers pass. So it is not re-sorted between one unit's update and the next, and a
  search made during phase 4 sees the order left by the previous cycle's sort (plus inserted new units).
- The sort is a stable insertion sort with one quirk: walking down the list, each entry that is above its predecessor
  is swapped backwards one place at a time, but the backward walk never compares the first two entries of the list.
  Only the first step of the pass (entries 1 and 2) can change which entry is first. So an entry can move up to
  the **second** place in one pass, and reaches first place only in a later cycle.
- Units that are dying, dead or inside something sort as if at pixel y −32768.
- A new unit is inserted before any units with the same pixel y.
- The search uses **tile rows**, not pixels. It skips the dying, dead and inside entries at the front, then finds
  (by binary search on tile rows) the first entry whose tile row is at least the box's top row − 3 (but not below
  0), to catch tall buildings whose footprint reaches into the box, and the last entry whose tile row is at most the
  box's bottom row. It visits the entries between them in list order. Because a moving unit's tile changes at the
  start of a step while its pixel y changes gradually, tile rows in the list are only nearly sorted, so a unit near
  either end of that range can be missed or included.

### Reacting to an attacker

A hit can record the attacker. The rules are in `ai/attack-ai.md`, "Call for help", steps 1 to 4: a live attacker is
kept, allies are ignored, and a human player's unit doesn't record a building. There are two places where the attacker
is dealt with.

**In the acquiring orders** (those marked "acquires", and attack-move in Enhanced mode), each time the logic runs and
the unit has an attacker:

1. If the unit couldn't target its attacker at all (its score for the attacker would be 0: for example it can't
   attack that kind of unit), it runs the **flee** rule below. Nothing else happens in that run.
2. Otherwise, for a combat unit, the ordinary target search runs once, starting with the attacker as the best
   candidate. If the result differs from the unit's current target, the unit attacks it with the order from the
   resulting-order table, which for the idle-type orders is **12 whenever the unit has an attacker**, for human and
   computer players alike. There is no second search in the same run.
3. Workers and tankers use the reaction below instead (computer players' workers and tankers first run their AI
   routines, a later spec). Mages use the mage rule in "Special cases".

**The reaction**, used by the orders that react without acquiring (attack-move in Classic mode, demolish orders
19 to 21) and by several other orders' logic (follow, repair, board):

- If the attacker is no longer valid (dying, dead or inside something), or the unit's owner is not playing, or the
  unit is a building, or it can't attack, the attacker is simply forgotten.
- Otherwise, if the unit is a non-combat unit (worker, tanker, transport or demolition unit), it **flees** (below).
- Otherwise it runs the target search (above), which starts with the attacker as the best candidate. If the result
  differs from the unit's current target, it attacks the result with order 12, whatever the resulting-order table
  says.

**Fleeing** (the attacker must still be valid):

1. A worker attacked by a worker fights back (command 8 against the attacker). Done.
2. A **computer player's** unit flees only if it is idle (order 2).
3. **Every other player's** unit (see "Terms") flees only if its order is 2, 7 or 27, or its order is 23 (harvest)
   and it has **no target unit**. Only the target unit is tested: a gold worker's target unit is its mine and a
   tanker's is its platform, while a lumber worker never has one (`economy/harvest.md`, "Starting to harvest"). The
   tree tile, the chopping state and the return-to source are not looked at. So:
   - a gold worker or tanker on its way to its mine or platform doesn't flee;
   - a lumber worker flees, whether it is walking to its tree or chopping;
   - a tanker in order 23 with no platform (its target was cleared) flees;
   - a lumber worker whose tree search found nothing is idle (order 2), so it flees by the order-2 rule.

   Order 23's logic runs the reaction each time it runs, so a chopping worker reacts at its next chop-loop run. A
   fleeing lumber worker's reserved tree is released when the flee move takes over ("Taking over").
4. The flee tile is the unit's tile plus, on each axis, `(r mod 8) − 2 + 4·k`, where r is a fresh random draw (the
   game's RNG, `timing-and-rng.md`; x is drawn first, then y), and k is the attacker's **facing** direction as a unit
   step: facing 0 to 7 gives (k_x, k_y) = (0,−1), (1,−1), (1,0), (1,1), (0,1), (−1,1), (−1,0), (−1,−1).
   Two-tile movers (sea and air units) use twice those steps (so 8 tiles instead of 4).
5. The tile is clamped to the map, and for two-tile movers each coordinate is rounded down to even.
6. The unit gets a move to that tile, and its attacker is forgotten.

Since the attacker faces the victim, the victim runs **away from the attacker**, in the direction the attacker is
facing, about 4 tiles further on each axis that the facing moves along (2 to 9 tiles), with a spread of −2 to +5
tiles on each axis.

**Random draws.** Of the rules in this spec, only fleeing draws random numbers: **2 draws** per flee (x first, then
y), made in phase 4 when the flee happens. Immediate orders, including going idle, make no draw. (The idle wait of
`(r AND 15) + 44` cycles belongs to the computer players' worker routine; see `core/timing-and-rng.md`.)

## Data

| Constant | Value |
| --- | --- |
| Smart-move wait cap | 40 cycles (computer players), 10 cycles (every other player) |
| Resume countdown after re-issuing patrol | 40 cycles |
| Resume countdown after re-issuing attack-move | 20 cycles |
| Attack-move "complete" distance | within 1 tile on both axes |
| Acquisition score bases | 0, 65536, 131072, 196608 |
| Distance penalty | floor(d² / 4) |
| Minimum score | 1 |
| Search start above the box | 3 tiles |
| Flee offset per axis | `(r mod 8) − 2 + 4·k` (two-tile movers: `8·k`) |

Unit data fields used (PUD unit data section): react range (computer), react range (human), priority, attack range,
and the type flags (air, sea, building, submarine, shore building, oil platform, can attack, tower, worker, tanker,
transport, demolition unit, mage-like).

## Test vectors

Distances are in tiles; scores as defined above.

| Case | Expected |
| --- | --- |
| Human footman idle, an enemy grunt (priority p) 1 tile away, footman attack range 1 | score 131072 + p − 0; order 16 |
| Same, but the footman belongs to a computer player | order 12 (the grunt can attack) |
| Computer footman idle, only target is an enemy farm 1 tile away | order 12 (building) |
| Computer footman idle, only target an enemy peasant 1 tile away | order 16 |
| Two equal candidates (same type, same distance), A above B on screen | A is chosen |
| Candidate tied with the current attacker | the attacker is kept |
| Archer (range 4) sees an enemy footman at d = 6 and an enemy farm at d = 2 | footman: 65536 + p − 9; farm: 0 + p_farm − 1, at least 1; the footman wins |
| Same archer, a gryphon rider in range at d = 4 and a footman in range at d = 2 | gryphon base 196608 beats footman base 131072, whatever the priorities |
| Score of a non-attacking target with priority 5 at d = 6 | 5 − 9 = −4, raised to 1 |
| Building of width 4 at column 10; searcher column 8, 11, 13, 14 | reference column 10, 12, 12, 13 |
| Smart move given to a human unit with 37 cycles left to wait | its wait becomes 10 |
| Same, a computer player's unit with 37 cycles left | wait stays 37 (cap 40) |
| Same, a passive computer player's unit with 37 cycles left | its wait becomes 10 (not a computer player) |
| Command 8 on an empty tile, Enhanced | order 10, attack-move remembered |
| Command 17 on a point 6 tiles away, catapult (range 8) | order 17 at once |
| Command 17 on a point 12 tiles away in the catapult's own land area | order 17 at once; it walks until within 8 tiles |
| Command 17 on a forest tile 12 tiles away, catapult | order 18; becomes 17 when within 8 tiles on both axes |
| Command 19 on a tree tile | order 20 |
| Command 19 on open grass | order 21 |
| New order given while the unit's order is 25 | ignored |
| Attack target dies, no pending order, order 9 | order 2 |
| Attack target dies, no pending order, order 15 | order 13 |
| Enhanced, footman on attack-move kills a target 10 tiles short of the goal and goes idle, countdown 0 | attack-move re-issued; countdown 20 |
| Human peasant (order 2) hit by an archer facing east (facing 2), draws r = 5 then r = 2 | flee tile = peasant tile + (5 − 2 + 4, 2 − 2 + 0) = (+7, 0) |
| Human gold worker walking to its mine (order 23, the mine as target unit) is hit by an enemy archer | does not flee |
| Human lumber worker walking to its tree (order 23, no target unit) is hit by an enemy archer | flees (2 draws) |
| Human lumber worker chopping (order 23, no target unit) is hit by an enemy archer | flees at its next chop-loop run |
| Human lumber worker whose tree search found nothing (now order 2) is hit | flees (order-2 rule) |
| Computer player's lumber worker chopping is hit | does not flee (only idle units of computer players flee) |
| Human footman attacking an enemy grunt (order 9); the grunt walks into fog | the grunt stays the target |
| Human footman attacking an enemy mage (order 9); the mage casts Invisibility on itself | the target is lost at the next check: order 2 (or the pending order) |
| Human footman idle; an enemy grunt 1 tile away but in fog for the footman's owner | not a candidate (hidden); no attack |
| Peasant hit by an enemy peon | attacks the peon back |
| Human footman, idle (order 2), hit by an enemy grunt 1 tile away; no other enemy near | the search starts with the grunt as best; result: the grunt, order **12** (chase), not 16 |
| Human footman standing ground (order 13) hit by an enemy archer 5 tiles away (out of range) | the archer is outside the footman's box (radius 1), but the search starts with it as the best candidate (base 65536), so the result is the archer: order 15. At the next logic run the archer is valid and detected but out of range, so the footman forgets it and order 13 takes over at the next step boundary ("Attacking without moving") |
| Human footman idle, an enemy grunt 1 tile away; the footman gets order 16 on it; the grunt then steps 2 tiles away | Next run: out of range, so order 2 takes over and the grunt is forgotten; the footman's idle search may then pick a new target |
| Human guard tower (range 6), attacked by an enemy catapult 8 tiles away; an enemy footman 4 tiles away | The tower ignores its attacker (no attacker seed, box of 6): it fires at the footman |
| Human guard tower, its target walks from 6 to 7 tiles away | Next run the search doesn't find it; with nothing else in range the tower does nothing |
| Human catapult at (14, 20) on order 16 against a grunt at (20, 20); one of the catapult player's own footmen stands at (21, 19) | No strike; order 2 takes over |
| Computer catapult, same, two own footmen in the 3 × 3 | Strikes; a third own unit there would stop it |
| Passive computer player's catapult, same, one own footman in the 3 × 3 | Strikes (not human-controlled: it takes three) |
| Human catapult, same target, own 2 × 2 farm whose footprint covers (19, 21) and (20, 21) only inside the 3 × 3 | Counts 2 (one per tile); 1 is enough for a human player, so no strike |
| Human destroyer, same situation as the first catapult vector | Strikes: destroyers can't attack the ground, so they skip the check |
| Human footman in order 34, transport 5 tiles away on the shore, not landing | Gets a smart move again: stays in order 34; the transport is told to come |
| Computer grunt in order 34, same | Gets command 4 on the transport: order 4 at the next step boundary; it is no longer boarding |
| Passive computer player's grunt in order 34, same | Gets a smart move, as the human footman: stays in order 34 |
| Transport landing (order 30), goal 2 tiles away on both axes | Not arrived: radius 0, not 3 |
| Unit entries with pixel y 100, 200, 50 in list order (none dying or inside) | after one sort: 100, 50, 200 (the third entry can't pass the first in one pass); after the next cycle's sort: 50, 100, 200 |
| Footman guarding (order 7) a peasant that starts harvesting (order 23) | the footman is given a move to its own tile and stops |
| Footman following (order 6, nothing pending) a peasant that is harvesting, arrives within 1 tile | idle (order 2) |
| Footman follows a footman that is moving (order 3), and arrives within 1 tile | order 7 (guard) |

## Open questions

- What issues commands 9, 11, 12, 14 and 16. The game's own logic issues orders 11 (the computer players' wall rule
  in `movement/movement.md`), 12 (target acquisition and the reaction) and 16 (idle acquisition) through the same
  rules as these commands, and computer players' boarding units are given command 4 ("Boarding moves"). Nothing
  found issues commands 9 or 14 except the command stream itself (player interface or AI by number); which buttons
  or AI steps send them is still open.
- Order 57: nothing found sets it.
- Commands 2, 6, 23, 24 and the spell commands: covered by the economy and spell specs.
- Whether the minimum-range test uses the same distance measure as the attack-range test during an attack.
- Which unit types count as "two-tile movers" for the flee rounding (expected: sea units and air units).
- The player-kind splits in "Terms" rest on the mapping of map owner codes to player kinds, which
  `movement/movement.md` still lists as open for rescue players.
