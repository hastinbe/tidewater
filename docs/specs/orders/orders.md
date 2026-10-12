# Orders: order list, commands, pending orders and target acquisition

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`) and the public PUD
  specification text (unit data section). Not yet confirmed against the running game, except the idle look-around
  (oracle traces).
- **Changes:** 2026-10-12, round-10 errata: a hit records its attacker only under the rules of `ai/attack-ai.md`
  ("Call for help"), and the first live attacker is kept, so the attacker is not always the last unit that hit.
  2026-10-12, round-13 errata: command 17 gives order 17 at once when the target tile is in the unit's own region.
  2026-10-12, round-14 errata: orders 33 and 14 forget the building's attacker each time their logic runs.
  2026-10-12, round-30 errata: new section "Idle look-around" (turn, wander and bob draws of idle units) with trace
  vectors; a unit is created in order 2 and its first order, if different, is pending until its first step done.
  2026-10-12, round-31 errata: a take-over replaces that step done's logic run; the new order's logic first runs at
  its animation's next step done.
  2026-10-12, round-33 errata: a move that arrives goes idle through a pending order (same-update take-over, wait 3,
  no idle logic in that update); a move with no route goes idle as an immediate order and runs the idle logic at
  once (static study only). More same-update take-overs confirmed; coming out of a building is the exception. The
  open question "Idle logic after a move" is closed.
  2026-10-12, round-35 errata: an idle unit's acquired order takes over in the same update (wait 3, animation 4); a
  watching tower stays in its attack animation and searches once per loop; the Enhanced resume applies to every pending
  idle, arrival included, and never to an immediate idle; the no-route attack fallbacks never apply to orders 3, 4, 5
  and 10.
  2026-10-12, round-36 errata: order 30's radius 3 is used (ordinary steps until within 3 tiles, then the landing
  logic); patrol arrival takes over with wait 3 and steps 3 cycles later; round 35's resume vector replaced (a command
  clears the remembered order).
  2026-10-12, round-37 errata: a killer forgets its dead target at once and other attackers on their next run; the
  strike decision tests the attack cooldown; a firing tower stores no target and doesn't turn; the round-36 capture
  reference names its folder.
  2026-10-12, round-38 errata: a lumber worker's own new aim is added to the confirmed same-update take-overs.

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
| 30 | Transport landing (`orders/transports.md`) | 6 | 3, no +1 (see the note below the table) | yes | no |
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

**Order 30's radius.** The general movement logic uses radius 3 for order 30, with no +1: until the transport is
within 3 tiles of its goal on both axes it moves with ordinary steps (sea class, two-tile mover, move animation 3),
and the landing logic doesn't run. Within 3 tiles the arrival switch starts the order's own animation (6) and the
landing logic of `orders/transports.md` runs; from then on it does the moving, with its own exact arrival test
(radius 0).

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

A unit is created in order 2, with its animation's first wait of 1 + (r AND 7) (`core/unit-slots.md`). For a unit
whose first order isn't 2, that order is its pending order and takes over at the unit's first step done ("Pending
orders and immediate orders"). In the Human01 trace the map's buildings and gold mines show order 2 at setup and 33
from their first step done.

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

The take-over replaces the logic run of that step done: the new order's logic doesn't run in that update, and first
runs at the next step done of its animation. Only a step done with no pending order runs the order's logic.

**Same-update take-over.** A command that this run gives the unit itself can take over at once, in the same update,
without running its logic. The traces confirm it for the critter's wander, a computer worker's job choice from idle,
going idle on arriving at the end of a move ("Arriving and losing the target"), a harvest or return order that reaches
its building (order 25: wait 3, animation 3), felling a tree (`economy/harvest.md`), a lumber worker's own new aim (a
new harvest command: order 23, animation 4, wait 3; `economy/harvest.md`, "Lumber") and the order an idle unit's target
search gives (12, 15 or 16; "Automatic target acquisition"). Coming out of a building is the exception: the order given
then stays pending through a 25-cycle wait (`economy/harvest.md`, "Coming out").

**Immediate orders** (used for dying, giving up and a move with no route; a move that arrives goes idle through a
pending order instead): the order is set at once, the pending order is
cleared, the order's animation starts with a wait of 1, and any reserved tree is released. When the immediate order
is 2 (idle):

- a mobile unit forgets its target;
- a transport whose tile is on even coordinates and isn't shore water settles there (sea class, two-tile mover);
  otherwise its order becomes **32** instead, which does nothing, so it waits where it is until it gets a new order.

Orders 0 and 1 also wipe the remembered order (Enhanced, below).

### Resuming patrol and attack-move (Enhanced only)

In Enhanced mode a unit can **remember** a patrol (both ends) or an attack-move (the target tile), set by commands 5
and 8. Each unit also has a **resume countdown**, which goes down by 1 every cycle until it reaches 0.

When idle (order 2) is made such a unit's **pending** order (on arriving, on losing its target, and wherever else
idle becomes pending) and its countdown is 0, the remembered order may be given instead of the idle; it takes over in
the same update, as the pending idle would have. An immediate idle (dying, giving up, no route) never resumes, and
keeps the remembered order. Nothing happens for a unit for which a pending order can't be set (dying, dead, inside
something, or in order 25):

| Remembered | Result |
| --- | --- |
| Patrol | It is sent back toward the patrol end it was heading for, and the countdown is set to **40** |
| Attack-move, with the target tile more than 1 tile away (either axis) | The attack-move is given again, and the countdown is set to **20** |
| Attack-move, already within 1 tile | It stays idle (the attack-move is complete) |

If nothing is resumed (no remembered order, the countdown running, or the attack-move complete), the pending idle is
set as usual.

So a unit that broke off a patrol or attack-move to fight goes back to it once the fight is over. Every command a
unit takes in Enhanced mode first clears its remembered order (`ui/selecting-and-commanding.md`), and only commands
5 and 8 set it again, so a move command ends a remembered attack-move: the move arrives at idle. The countdown stops
it from re-issuing every cycle when the move can't progress.

### Arriving and losing the target

- **Orders 3, 4 and 10:** on arrival the unit forgets its attacker, and idle (order 2) becomes its pending order and
  takes over in the same update (still animation, wait 3). The idle logic doesn't run in that update; it first runs
  at the still animation's next step done, 3 cycles later. (Before that, at the step done, the arrival switch of
  `core/animation-scripts.md` runs the order's own animation at once, and then the order's logic runs.)
- **No route:** if the route search finds nothing and none of the special cases of `movement/movement.md` applies
  (the harvest tree search, arrived after all, boarding, the attack-type fallbacks), the unit becomes idle as an
  **immediate** order (order 2, pending order cleared, still animation, wait 1). Its order is now 2, so the idle
  logic runs in the same update, straight after; a computer worker's job choice or a critter's wander can follow
  in that update too. The attack-type fallbacks apply only to orders whose animation is 4, so they never apply to
  orders 3, 4, 5 and 10. This rests on the static study only; no trace shows it yet.
- **Order 5 (patrol):** on arrival the unit is given a patrol to its other end, with the tile it stands on as the new
  other end. The new patrol takes over in the same update (still animation, wait 3); the first step comes at its next
  logic run, 3 cycles later.
- **Order 18:** on arrival (in range) it becomes order 17 and attacks the ground.

**Orders aimed at a unit** (follow, attack and similar) check their target every time their logic runs. The target
is **lost** if it is no longer valid (dead, removed or inside something) or is no longer **detected** by the unit's
owner (see "Terms"). Fog over the target doesn't matter: a target that walks into fog stays the target. When the
target is lost:

1. If a pending order exists, it takes over now.
2. Otherwise, order 15 (attacking in place) becomes **13** (stand ground), and every other order becomes **2**.

A target that dies keeps its HP and its own target through its death animation (`combat/damage.md`, "Applying
damage"). The unit that killed it forgets it at once, in the step that applies the blow, without changing its order.
Every other unit attacking it keeps it as its target until its own next logic run, where the target is lost as above.

**Strike decision (attack orders 9, 11, 12, 15, 16 and 17).** Before a unit strikes, the attack cooldown of
`combat/damage.md` ("Attack timing (cooldown)") is tested: a unit that isn't a flyer and whose wait is less than its
cooldown left restarts its attack animation and waits for the cooldown left. When it strikes, it turns to face its
target (the facing rule of `core/animation-scripts.md`; ships use a different rule) and its cooldown starts again.

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

**Order 14 (towers).** A tower doesn't keep a target from one run to the next. Each time its logic runs, it forgets its
attacker and runs the target search of "Automatic target acquisition" with two differences: the box is always its
**attack range** (even for a computer player), and the search does **not** start with the attacker as the best
candidate. If the search finds a target, the tower fires at it in that run; if not, it does nothing until its next run.
Either way the tower stores no target (its target stays null) and doesn't turn: its facing isn't changed by firing. The
shot is made at once in the logic run, not at a strike point of the animation (tower scripts have none), and the attack
cooldown doesn't apply to towers. So a target that walks out of range is simply not found again, and the tower fires at
whatever is best in range.

Either way the tower stays in its attack animation (4), which order 14 starts and never changes: finding nothing
doesn't make it idle or start its still animation. A tower's attack animation is a "step done" followed by one long
wait, so the tower looks for a target once per loop: in the default data every 151 cycles for type 0x63 (orc cannon
tower, wait 150) and every 60 for type 0x61 (orc guard tower, wait 59).

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

When the searching unit's order is idle-type, the resulting order is given as a pending order that takes over in the
same update ("Same-update take-over"): attack animation, wait 3, and its logic first runs at that animation's next
step done.

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

### Idle look-around

Every time a unit's **idle logic** runs, the first thing it does is the look-around. Except for catapults, this makes
random draws (`core/timing-and-rng.md`), so it decides the draw sequence of every game, even one with no fighting.

**When it runs.**

- The idle logic is the order logic of order **2** (stop / idle) and order **13** (stand ground). Order **7** (follow
  and guard) runs it after its follow steps ("Order 7 then also runs the idle logic", above). A follower whose follow
  steps give it a new order in that update doesn't reach it, so it makes no draw.
- No other order makes the look-around. That includes the attack orders that share parts of the idle logic (9, 11,
  12, 15, 16 and 17), and order 32 (transport docked), whose logic does nothing.
- Order logic runs at the "step done" signal of the unit's animation (`core/animation-scripts.md`). A unit at rest
  in order 2 or 13 shows its still animation (animation 2), which loops once every **5 cycles**, so the unit draws
  once every 5 cycles. Its wait reads 4, 3, 2, 1, 1 at the ends of those cycles, and the draw comes in the cycle of
  the second 1, which is the step done.
- A newly created unit starts in order 2 with a wait of 1 + (r AND 7) from its creation draw (`core/unit-slots.md`).
  It draws at its first step done, unless a pending first order takes over then ("Orders given when a unit is
  created"): a building or gold mine makes no look-around.
- When a move (order 3) arrives, idle takes over with a wait of 3, and the idle logic (and its draw) first runs 3
  cycles later ("Arriving and losing the target"). A move that finds no route goes idle at once and draws in that
  same update.

**Where in the cycle.** The draw is in phase 4 (units), during the unit's own update, so units draw in ascending slot
order (`core/unit-slots.md`).

- Within the update it comes after any draw made by the unit's animation step in that cycle, because the step done
  comes first.
- It comes before every other draw that the rest of the idle logic may make in the same update.
- A unit created during another unit's update makes its creation draws there. For example, a great hall that finishes
  training a peon makes the peon's two creation draws in the hall's own update, between the look-around draws of the
  units in the slots below and above the hall.

**Who does what.** One rule applies per run, chosen by the unit's type:

| Unit | Each run of the idle logic |
| --- | --- |
| Type with flag bit 2 ("explode when killed"; in the default data the two catapults, 0x04 and 0x05) | Nothing: no look-around and no draw |
| Type with flag bit 3 (sea; every ship, 0x1A to 0x21 and 0x24 to 0x27 in the default data), or a unit on the air layer (`movement/occupancy.md`, "Movement layers"; every flier) | **Bob** (below) |
| Any other unit (land units, critters) | **Turn** (below), then for the critter (type 0x39) possibly **Wander** |

**Turn.** Make 1 draw r and look at its low 8 bits, `r AND 255`:

| `r AND 255` | Effect |
| --- | --- |
| 0 | Facing turns one step counter-clockwise: `facing ← (facing − 1) mod 8` |
| 1, 2 or 3 | Facing turns one step clockwise: `facing ← (facing + 1) mod 8` |
| 4 to 50, and the unit is a critter (type 0x39) | **Wander** |
| Anything else | Nothing more |

So an idle land unit turns on 4 runs in 256, and a critter wanders on 47 runs in 256. A turn changes only the
facing. The unit is redrawn, but its animation, frame and wait are unchanged.

**Wander** (critter only, after the turn draw). Make a 2nd draw r2, and let d = `r2 AND 7` be a direction in the
facing order of `core/animation-scripts.md` ("Movement by scripts"):

| d | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| dx | 0 | +1 | +1 | +1 | 0 | −1 | −1 | −1 |
| dy | −1 | −1 | 0 | +1 | +1 | +1 | 0 | −1 |

- The target tile is the critter's tile plus (dx, dy), with each coordinate clamped to 0 … map size − 1.
- If the clamped target is the critter's own tile, nothing more happens.
- Otherwise the critter gets **command 3 (smart move)** to that tile, as "Commands" describes. Its wait is cut to at
  most 10 cycles, because a critter's owner isn't a computer player. The target is an empty tile, so the result is
  order 3 (move). In the traces the critter's order is already 3, with a wait of 3, at the end of that same update.
- The move then follows the movement specs.

**Bob** (ships and fliers). Each unit has a **bob countdown** and a **bob value**, both 0 when the unit is created.
On each run:

- If the countdown is above 0, it decreases by 1. Nothing else happens, and there is no draw.
- If it is 0, make 1 draw r. The countdown becomes `(r AND 7) + 8`, from 8 to 15. The bob value flips: 0 becomes 1,
  anything else becomes 0. The unit is redrawn.

So a ship at rest draws at its first run, then again every `(r AND 7) + 9` runs, which is every 5 × ((r AND 7) + 9)
cycles. The bob value is the b of `ui/map-drawing.md`, "Bob offset": the unit is drawn b pixels higher.

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
| Transport landing (order 30), goal 2 tiles away on both axes | The general test (radius 3) is met, so the landing logic runs; its own test (radius 0) isn't, so it steps on |
| Transport landing (order 30), goal 5 tiles away | An ordinary two-tile step, move animation 3; the landing logic doesn't run |
| `human12-transport` (oracle trace): transport 1522 at (68, 34), order 30 from n = 8 (animation 6, wait 3), goal (72, 29) | n = 11 (4 and 5 tiles away): two-tile step north-east to (70, 32), animation 3. n = 43 (within 3): animation 6, one-tile step to (71, 31) (the goal is odd); n = 59 (72, 30); n = 75 (72, 29) |
| `xorc10-dragons` (oracle trace): computer destroyer 1483 (type 0x1E) at (124, 74); its patrol takes over at n = 4 (wait 3), heading for its own tile | n = 7: arrived; the new patrol takes over: tile (124, 74), animation 2, wait 3. n = 10: tile (122, 74), facing 6, animation 3. Destroyer 1484 does the same |
| Unit entries with pixel y 100, 200, 50 in list order (none dying or inside) | after one sort: 100, 50, 200 (the third entry can't pass the first in one pass); after the next cycle's sort: 50, 100, 200 |
| Footman guarding (order 7) a peasant that starts harvesting (order 23) | the footman is given a move to its own tile and stops |
| Footman following (order 6, nothing pending) a peasant that is harvesting, arrives within 1 tile | idle (order 2) |
| Footman follows a footman that is moving (order 3), and arrives within 1 tile | order 7 (guard) |

### Idle look-around (oracle traces)

These come from traces of the running game (`oracle/trace-format.md`): record n = 1 and 2 are the start-up cycles,
n = 3 is the first normal cycle, and "setup" is the state after the map has loaded. Clock 946684800 in both.

**Human01, the first draws.** Setup state 0xF55E08A8. Eight units are idle: grunts (type 0x01) in slots 1588, 1590,
1591 and 1592, footmen (0x00) in 1593, 1597 and 1598, and a peasant (0x02) in 1596. The map's buildings and gold
mines draw nothing.

| n | Slot (type) | r | `r AND 255` | Facing before → after | State after record n |
| --- | --- | --- | --- | --- | --- |
| 1 | (no draw) | | | | 0xF55E08A8 |
| 2 | 1598 (0x00) | 9770 | 42 | 5 → 5 | 0x262AFAC9 |
| 3 | 1593 (0x00) | 29433 | 249 | 2 → 2 | 0xF2F9299E |
| 4 | 1590 (0x01) | 30671 | 207 | 2 → 2 | |
| 4 | 1596 (0x02) | 27742 | 94 | 5 → 5 | 0xEC5EDCE4 |
| 5 | 1597 (0x00) | 25369 | 25 | 6 → 6 | 0x63193335 |
| 6 | 1592 (0x01) | 26483 | 115 | 7 → 7 | 0x6773BFFA |
| 7 | 1588 (0x01) | 10072 | 88 | 7 → 7 | |
| 7 | 1591 (0x01) | 2174 | 126 | 0 → 0 | |
| 7 | 1598 (0x00) | 4396 | 44 | 5 → 5 | 0x912C27E1 |
| 8 | 1593 (0x00) | 26724 | 100 | 2 → 2 | 0x6864CF96 |

From n = 3 on, 1, 2, 1, 1 and 3 draws per cycle repeat every 5 cycles; each unit draws in every 5th cycle (slot
1598 at n = 2, 7, 12, …). That makes 3197 draws over n = 1 to 2000, all of them look-arounds: 47 turn clockwise, 17
counter-clockwise and the other 3133 change nothing.

**Human01, the first turns.**

| n | Slot (type) | r | `r AND 255` | Facing before → after | State after record n |
| --- | --- | --- | --- | --- | --- |
| 50 | 1597 (0x00) | 7682 | 2 | 6 → 7 | 0x1E02158D |
| 64 | 1590 (0x01) | 13824 | 0 | 2 → 1 | 0x5C886184 |
| 73 | 1593 (0x00) | 17152 | 0 | 2 → 1 | 0x4300342E |
| 104 | 1596 (0x02) | 4352 | 0 | 5 → 4 | |
| 209 | 1596 (0x02) | 7682 | 2 | 4 → 5 | |
| 351 | 1592 (0x01) | 14083 | 3 | 7 → 0 | |

**Human04, the first wanders.** Setup state 0xF07C3BB8, map size 96. Critters (type 0x39):

| n | Slot | Turn draw r (`AND 255`) | Wander draw r2 (d) | Tile → target | Result |
| --- | --- | --- | --- | --- | --- |
| 7 | 1579 | 23301 (5) | 23615 (7) | (59, 44) → (58, 43) | Order 3 at n = 7; tile (58, 43) from n = 10 |
| 7 | 1580 | 12067 (35) | 4352 (0) | (63, 44) → (63, 43) | Order 3 at n = 7; tile (63, 43) from n = 10 |
| 8 | 1557 | 6160 (16) | 16610 (2) | (40, 79) → (41, 79) | Order 3 at n = 8; tile (41, 79) from n = 11 |
| 9 | 1593 | 19475 (19) | 8785 (1) | (37, 19) → (38, 18) | Order 3 at n = 9; tile (38, 18) from n = 12 |

**Human04, bob.** The troll destroyer (type 0x1F) in slot 1598 sits in order 2. Its countdown is 0 at creation, so
it draws at its first run.

| n | r | New countdown `(r AND 7) + 8` | Next bob draw |
| --- | --- | --- | --- |
| 2 | 21216 | 8 | n = 2 + 5 × 9 = 47 |
| 47 | 17609 | 9 | n = 47 + 5 × 10 = 97 |
| 97 | 11139 | 11 | n = 97 + 5 × 12 = 157 |
| 157 | 9246 | 14 | n = 157 + 5 × 15 = 232 |

**Human04, whole trace.** Setup and n = 1 to 3000 hold 13772 draws: 142 creation draws (68 map units at setup and 3
trained peons, 2 each), 12438 turn draws, 757 wander draws and 435 bob draws. The computer player makes none.

### Taking over (oracle traces)

| Input | Expected |
| --- | --- |
| Human04 great hall (slot 1550, computer, created with wait 8) | Order 33 takes over at n = 8 (wait 3). Order 33 logic runs at n = 11, 16, 21 (wait reads 3, 2, 1, 1, then 4, 3, 2, 1, 1). Its think counter reaches the third run at n = 21: it trains a peon (gold −400 at n = 21), and order 37 takes over at n = 26 |
| Same hall, the peon appears at n = 489 (463 cycles after n = 26) | Order 33 takes over at once with wait 3. Its logic runs at n = 492, 497, 502; it thinks at the third run, n = 502 (gold −400), and order 37 takes over at n = 507. The next peon appears at n = 970; then payment at n = 983, take-over at n = 988 and peon at n = 1451 |
| Human04 peon (slot 1548, computer, created with wait 6) | First step done at n = 6: idle logic, then the worker job choice (`ai/worker-ai.md`) picks gold mine 1559, the only mine in its region, with the great hall 1550 as depot. At the end of n = 6: order 23, target 1559, wait 3 |
| Human01 gold mine (slot 1589, wait 1 at setup) and farm (slot 1594, wait 2 at setup) | The mine: order 33 with wait 3 at n = 1. The farm: wait 1 at n = 1, order 33 with wait 3 at n = 2. Each then has a step done every 5 cycles |
| Human04, critter slot 1579, moving (order 3) to tile (58, 43) | n = 57: order 3, animation 3, wait 1. n = 58 (reaches (58, 43)): order 2, animation 2, wait 3; no idle logic. n = 61: first idle logic run; the critter wanders (order 3 takes over at once: wait 3, animation 2). n = 64: the move logic runs, animation 3, wait 1, facing 5 |
| Human04, critter slot 1580, moving to (63, 43) | n = 58: order 2, animation 2, wait 3. n = 61: idle logic, no wander (wait 1 at n = 61 and 62) |
| Human04, peon 1548, harvest order 23, reaches the mine | n = 89: order 25, animation 3, wait 3 |
| Human04, peons 1548 and 1531, return order 24, reach the depot | Order 25, animation 3, wait 3 at n = 374 (1548, great hall) and n = 1938 (1531, lumber mill) |
| A land unit given a move to a tile it can't reach, where the route search finds nothing (static study only) | In the update where the search fails: order 2, wait 1, and the idle logic runs at once (for a critter, its wander can start in the same update) |
| `xhuman4`, slot 1490 (type 0x09), idle, finds 1518 at its first step done | n = 2: order 12, animation 4, wait 3, target 1518; n = 3: animation 4, wait 2 |
| `xhuman4`, slot 1505 (type 0x01), the same | n = 2: order 12, animation 4, wait 3; n = 5: first chase step, animation 3 |
| `human5`, slot 1531 (owner 5, computer), idle, finds 1529 | n = 6: order 12, animation 4, wait 3, target 1529; it strikes from where it stands (still animation 4 at n = 30) |
| `xhuman4`, slot 1435 (type 0x09), enemy 1444 in reach from the start | n = 1: order 2, animation 2, wait 1 (no acquisition in the first start-up cycle); n = 2: wait 4; n = 6: order 16, animation 4, wait 3, target 1444 |
| `human12`, slot 1480 (type 0x63), no enemy in range | n = 1: order 14, animation 4, wait 3; n = 4: logic runs, finds nothing; n = 5: animation 4, wait 150; next logic run at n = 155 (wait 150 again at n = 156) |
| `xorc3`, slot 1506 (type 0x61), no enemy in range | n = 4: logic runs, finds nothing; n = 5: animation 4, wait 59; next run at n = 64 |
| Enhanced (`research/oracle-round36/xhuman10-r36b-400.jsonl.gz`, oracle trace): hero 1463 (type 0x14) on (107, 98) gets command 8 on tile (113, 106) in turn 42, then a move to (111, 101) in turn 62 | Order 10 at n = 43; order 3 at n = 78; arrives at n = 113: order 2, animation 2, wait 3, and stays idle (the move command cleared the remembered attack-move) |
| `xhuman10-spells-700` (oracle trace), player 1's guard tower 1468 (type 0x60) and cannon tower 1501 (type 0x62), grunts 1477 on (78, 91) and 1495 on (79, 88) in range | n = 2: order 14, animation 4, wait 3. n = 5: new effect slot 3, kind 0x0F, shooter 1468, target 1477, damage 7; slot 4, kind 0x18, shooter 1501, target 1495, damage 50. Both towers: target null, facing 0. n = 6: 1468 wait 59, 1501 wait 150, both animation 4 |
| `human01-basics-9500` (oracle trace), n = 288: footman 1597 kills footman 1592; footman 1598 also targets 1592 | 1597: target null at once, still order 9. 1598: keeps target 1592 until its logic runs at n = 291, then order 2, wait 3 |
| Enhanced (static study only): a unit on an attack-move acquires an enemy on the way and kills it; countdown 0; the target tile more than 1 tile away | The attack-move is given again instead of the pending idle (countdown 20) |

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
- **No route, traced.** The immediate idle of a move that finds no route, with its idle logic in the same update,
  rests on the static study alone.
- **Untraced look-around cases.** Fliers and catapults at rest, orders 7 and 13, and a critter whose wander target is
  clamped back to its own tile don't occur in the traces; those rules rest on the static study alone.
- **Same-update take-over.** The order logic signals when a command it gave should take over at once. The cases
  confirmed are listed in "Taking over"; coming out of a building is the one known exception. Default: every other
  command a unit's own order logic gives it takes over in the same update.
