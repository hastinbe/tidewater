# Orders: order list, commands, pending orders and target acquisition

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11). Consolidated 2026-10-14 into one model with no
  rule change (B-038; the review notes, with a map from every earlier rule and vector to its place here, are kept
  with the research-side specs). The consolidation was checked by a second spec-review session, 2026-10-14.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the public PUD
  specification text (unit data section), and oracle traces of the running game. Each rule says which: "traced"
  with the trace name and record number n, or "static study only". The erratum rounds each rule came from are listed
  under "History" at the end.

## Behaviour

Every unit always has exactly one **order** (a small number, listed below) and at most one **pending order**. The
order says what the unit is doing; its logic runs only when the unit's animation signals "step done" (see
`movement.md` and `animation-scripts.md`). Player and AI **commands** don't set orders directly: each command is
turned into an order by the rules in "Commands" below, and the result is normally stored as the pending order.

### The model

A unit's update at a step done follows the same steps for every order ("How an update runs"):

1. If the unit has a **pending order**, it takes over, and nothing else happens for the unit in that update
   ("Taking over").
2. Otherwise, if the order **moves** the unit (the "Moves" column of "The order list") and the unit isn't yet in
   reach of its goal, the **walk** runs instead of the order's logic ("Walking orders"). The order's logic runs in
   that update only if the walk **ends**.
3. When the order's logic runs, the **arrival switch** comes first, and a command the logic gives the unit itself can
   take over **in the same update** ("Same-update take-over").

The orders fall into a few families. Each family's logic is built from the shared rules listed after the table.

| Family | Orders | What the logic does when it runs | Where it goes next |
| --- | --- | --- | --- |
| Moves | 3, 4, 10 | Arrives: forgets the attacker; idle becomes pending ("Arriving and losing the target") | 2, or the resumed order; 12 after an acquisition on the way (4, and 10 in Enhanced mode) |
| Patrol | 5 | Turns round: a new patrol command to the other end | 5; 12 after an acquisition on the way |
| Follow | 6, 7 | The follow steps; order 7 then runs the idle logic ("Follow (orders 6 and 7)") | 3, 6, 7, 2 |
| Idle and standing | 2, 13, 16 | Look-around (2 and 13), then attacker check and target search ("Idle look-around", "Automatic target acquisition") | 12, 15, 16, 3 (flee) |
| Attack | 9, 11, 12, 15, 16, 17 | The attack run: search or forget, cooldown, target and range checks, strike ("Attack orders") | 2, 13, the pending order |
| Tower | 14 | Forgets the attacker, searches its attack range, fires ("Attacking without moving") | 14 |
| Move into range | 18 | Walks to the pulled-back goal; on arrival gives order 17 on the target tile ("Arriving and losing the target") | 17 |
| Boarding | 34 | `orders/transports.md`; the boarding move ("Boarding moves") | 34, 4, 36 |
| Harvest, return, enter, inside | 23, 24, 25, 26 | `economy/harvest.md` | |
| Repair, build, demolish, spells | 27, 28, 20, 21, 38 to 56 | Their own specs | |
| Buildings and the rest | 0, 1, 22, 29 to 33, 35 to 37, 57 to 60 | Their own specs, or nothing | |

The rules below are each stated once and used by every order that reaches them:

- **Commands** ("Commands", "What every command does"): every command, whoever gives it (a player, a computer
  player, or the unit's own order logic), stores its target and goal, pulls the goal back, drops the route, clears
  the blocked counter, checks the attacker and the target, and is made the pending order.
- **Pending, immediate and same-update orders** ("Pending orders and immediate orders"): one way to set a pending
  order, one way to take over, one immediate path.
- **Making idle pending, and resuming** ("Resuming patrol and attack-move"): wherever idle becomes the pending order,
  the Enhanced resume can replace it.
- **Walking and the walk's end** ("Walking orders"): one walk for every order that moves, with three ends: in reach,
  target dropped at a step boundary, no route.
- **No route** ("Arriving and losing the target", "No route"): one immediate idle, with the listed special cases.
- **Losing the target** ("Arriving and losing the target"): one test for the walk, the command check and the follow and
  attack orders aimed at a unit. The harvest orders' own target checks differ (`economy/harvest.md`).
- **Reacting to an attacker** and **automatic target acquisition**: one reaction, one flee rule and one target
  search, used by every order that acquires or reacts.

Static study finds no case outside these rules, except those each rule names: every order that moves uses the same
walk and ends it in the same three ways; every command an order gives itself goes through the same command steps; and
every attack order's run makes the same checks in the same order, each order using the checks the table in "Attack
orders" gives it. Where a rule rests on the static study alone, it says "static study only", and the points the
consolidation made explicit are listed under "Open questions".

### Terms: detected, not hidden, and player kinds

**Two vision tests.** Both are computed each cycle by `vision/fog.md`, and the rules in this spec use them as follows:

- **Detected** by a player: the unit is in that player's detected-by set (`vision/fog.md`, "Detection"). Only
  Invisibility and submarines make a unit undetected. Fog has nothing to do with it.
- **Not hidden** from a player: the unit isn't in that player's hidden-from set (`vision/fog.md`, "Unit
  visibility"). This is the fog test: a unit standing in fog is hidden from a human player.

| Rule | Test |
| --- | --- |
| An order aimed at a unit loses its target ("Arriving and losing the target", and orders 14 to 16) | **Detected** only. A target that walks into fog stays the target |
| The same check on the unit's attacker | An attacker that is alive but not **detected** is ignored by that run's target search and its seed ("Reacting to an attacker", step 2 on) and stays recorded; only a command forgets it ("Commands forget a stale attacker"). Step 1's kind check and the flee rule look at the record as it is, with no detection test, so a unit that can't hit an undetected attacker flees from it (traced: `human12-25000`, vectors V208 and V209) |
| Automatic target acquisition (the candidate skip list, and the attacker seed in "Choice") | **Detected** and **not hidden** |

`economy/repair.md` and `orders/transports.md` use the same test: a lost target or attacker is one that is no longer
valid or no longer **detected** by the unit's owner.

**A recorded attacker outlives its slot** (static study only). Releasing a unit's slot doesn't clear other units'
attacker records, so a record can name a slot that has been released. A released slot reads as dying or gone, so every
test of a recorded attacker (the forget at the start of a target search, the reaction's checks, the candidate checks)
treats it like a dying attacker, and the kind check uses the last type the slot held. For a unit that left remains, that
is the remains type, which the slot keeps until it is freed; a unit removed with no remains is freed with its own type.
If the slot is reused before the record is read, the record names the new unit.

**One target** (static study only). A unit has a single target: the unit it strikes is its order's target. A missile
takes its shooter's target when it is launched and keeps its own copy from then on, so a later change of the shooter's
target doesn't redirect it.

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
- Order **18** has two arrival tests, and either one is enough: the unit is within its type's **base attack range**
  (no range upgrades) of the **target tile** (the tile commanded) on both axes; or it is within the table radius
  (**2**, plus the +1 of `movement/movement.md` for sea and air units, so 3) of its **stored goal**, which the command
  pulled back (`movement/pathfinding.md`, "Goal pulled back"). The cached path is dropped at arrival. Traced:
  `orc9-orc09-ground-move-goals-1100`, vectors V267 and V268. The radius test is separated by V268 (13 tiles from the
  target tile); the base-range test is static study only (in V267 both tests pass in the same update), and so is the
  radius 3 for sea and air units (no trace has a ship in order 18).

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

### How an update runs

A unit's order logic runs only at the "step done" signal of its animation (`movement/movement.md`, "When decisions
are made"; `core/animation-scripts.md`). At each step done, in this order:

1. **A pending order takes over** if there is one ("Taking over"). That replaces the logic run: nothing else happens
   for the unit in that update, and the new order's logic first runs at its animation's next step done.
2. **The walk.** If the order moves the unit ("Moves" in "The order list") and the unit isn't in reach of its goal
   (the arrival test of `movement/movement.md`, "Arriving", with the exceptions in "The order list"), the walk runs
   instead of the order's logic ("Walking orders", below). If the walk ends, the update goes on to step 3; if the unit
   steps or is blocked, nothing more happens for it in that update. A unit already in reach doesn't walk at all, so
   nothing the walk does (its target search or reaction, its target test) happens in that update (static study only,
   beyond the traced cases: V196 and "Walking orders react only when the walk has ended").
3. **The arrival switch.** If the unit still shows its move animation (3), it is switched to its order's own
   animation **before** the order's logic runs (stated, with its evidence, under "Walking orders react only when the
   walk has ended" below).
4. **The order's logic runs.** If it gives the unit a command of its own that takes over at once, the new order takes
   over now, without its logic running ("Same-update take-over").
5. **The skip mark** of "Order 12 with no route" is cleared at the end of an update in which the order's logic ran
   and the unit didn't step, if it was already set when the update began ("Order 12 with no route", step 3,
   "Clearing").

### Walking orders

Every order that moves the unit walks the same way. While the unit walks, only the walk runs at each step boundary;
the order's logic doesn't. At each step boundary the walk, in this order:

1. **Looks at the attacker, by order** (static study for the split; the traced cases are named below):
   - an order that **acquires** (4, 5, 7 and 12, and in Enhanced mode 10) runs the target search of "Automatic target
     acquisition". This is the move step's search, which the skip mark doesn't stop ("Order 12 with no route", step 3),
     and which heads the unit for a new target ("Attack-move: orders 4, 5 and (Enhanced) 10"). For order 7 (a
     guard walking after the unit it follows) this search is static study only;
   - an order that **reacts without acquiring** (attack-move in Classic mode, and the demolish orders 20 and 21) runs
     the reaction of "Reacting to an attacker" (static study only, untraced);
   - every other order only forgets a dying attacker. So a walking unit in any other order doesn't react: see
     "Walking orders react only when the walk has ended" below.
2. **Drops a lost target.** If the order holds a target unit that is dying, gone, inside something or no longer
   detected by the unit's player, the target is dropped and the walk **ends** at that step boundary: the order's
   logic runs in that update with no target. Orders 3 and 4 end there as on arrival ("Arriving and losing the
   target"; traced for order 3 with a dying target: `orc9-orc09-smart-target-2100` n = 1877, vector V240); orders 23
   and 24 follow `economy/harvest.md`. That every other walking order with a target unit ends its walk the same way
   is static study only.
3. **Tests arrival.** A unit within its order's radius has arrived: its cached route is dropped and the walk ends.
4. **Steps.** The route search of `movement/pathfinding.md` gives a step, a blocked step (`movement/movement.md`,
   "Blocked rules"), or **no route** ("Arriving and losing the target", "No route"). After no route, the order's
   logic runs in that update for whatever order the no-route rule left (normally idle).

A walk whose order holds a target unit heads for the target where it stands now, not for the stored goal
(`movement/pathfinding.md`, "Step 1"; traced for order 4 on a transport: `xhuman5-30000` n = 26764, vector V256;
static study only for other orders and for building targets).

**Walking orders react only when the walk has ended.** Orders 23, 24, 27 and 6 run the reaction only in a logic run
where the unit is **not walking**: it has reached its goal (in reach), or its target was found lost at a step boundary
(order 23's no-route re-aim also counts: `orc9-orc09-flee-chop-walk-900` n = 573). **No route is not such a run:**
orders 24, 27 and 6 then become idle as an immediate order and the **idle** logic runs, not the order's own logic, so
the order's reaction doesn't run ("Arriving and losing the target", "No route"; static study only, no trace has it).
While the unit is walking, only the walk runs, and the attacker stays recorded for later. For order 6 the reaction's
result doesn't end the run: the follow logic goes on with its current target, which the reaction's attack command has
set to the unit the search picked ("Follow (orders 6 and 7)"). **Order 34** runs the reaction only when it has reached
its transport, can't board and keeps waiting (`orders/transports.md`, "Order 34 (board)"): a unit that walks to its
transport with an attacker recorded boards when it gets there and doesn't turn on its attacker (traced:
`orc12-orc12-board-attacked-1000` n = 661, vector V253). **The arrival switch:** whenever the order's logic runs
at a step done while the unit still shows its move animation (with or without an attacker), it is switched to its
order's own animation (`core/animation-scripts.md`) **before** the order's logic runs, so a flee given there takes over
at once: order 3, still animation, wait 3 (traced for order 23: `orc9-orc09-flee-walk-1200` n = 590 and
`orc9-orc09-flee-chop-walk-900` n = 483 to 573, vectors V244 and V243). No reaction while walking is traced for orders 23,
6 (`orc9-orc09-follow-attacked-1200`, hit at n = 323 and 326, no reaction until n = 493) and 34
(`orc12-orc12-board-attacked-1000`, hit at n = 608, walking from n = 622 to 661), vectors V248 and V253; it is static study
only for orders 24 and 27, where no trace has an attacker on a walking unit.

### Commands

A command carries a target unit or a target tile (or neither). It turns into an order as follows.

| Command | Result |
| --- | --- |
| 3, smart move (right-click) | See "Smart move" below |
| 4, move and attack on the way | A non-combat unit is first handled as a smart move; the order then becomes 4. With a target unit, the unit walks to the target's tile and also records the unit as its target (the trace shows it from the command). Each time order 4's logic runs (at a step boundary), if that target is dying or lost ("Arriving and losing the target"), the order ends as on arrival: idle becomes the pending order, with the Enhanced resume, and takes over in the same update (wait 3). As on arrival, the unit also forgets its attacker (static study) |
| 5, patrol | Order 5 between the unit's position and the target tile. In Enhanced mode the unit also remembers "patrol", with both ends: the command's goal and the tile the unit stands on when the command is given ("Resuming patrol and attack-move") |
| 8, attack | Target unit, of any owner (the player's own, an ally's and a neutral unit included): order 9. Target tile, as given, is a wall tile: order 11 ("Order 11: attacking a wall tile"). Otherwise: order 10 (attack-move); in Enhanced mode the unit also remembers "attack-move" with the target tile |
| 9 | Target unit, of any owner: order 9; target tile, as given, a wall tile: order 11; otherwise order 2 |
| 11 | As command 9 |
| 12 | Order 12 |
| 13, 15 | Order 13 (stand ground) |
| 14 | Order 14 |
| 16 | Order 16 |
| 17, attack ground | Order 17 if the target tile has the same region value as the unit's tile, or is already within the unit's attack range; otherwise order 18 (move into range, then attack). Details: `combat/attack-ground-demolish.md` |
| 19, demolish | Target unit: order 20. Target tile holds a tree, rock or wall (or a reserved tree): order 20. Otherwise: order 21 |

**Checks on an attack command.** Commands 8 and 9 make no check on the target unit: no owner test and no dying test (a
dying target is handled by "Commands forget a stale attacker"). They check only the commanded unit: it must be the
player's own, must not be the target itself, and its type must be able to attack. The attack order's logic then checks
only that the attacker can hit the target's kind (air, sea or land), with the submarine rule of "Automatic target
acquisition", so an air target still needs an attacker that can hit air. Orders 12, 15 and 16 have no owner test on
their target either. The kind check uses the attacker type's can-target value from the unit data (land, sea and air), in
this order: a target in the **air** layer needs air; a **submarine** attacker can hit only types that are sea units, oil
platforms (the oil-source type) or shore buildings, all by the target type's own flags, not the layer it stands in; a
target of a **transport** type standing on **shore water** (its own tile has the shore-water flag, the same test as a
transport's wreck in `core/corpses.md`) can be hit by an attacker with either land or sea; any other target in the sea
layer needs sea; and any other target needs land, first match wins. One check serves both the attack order and target
acquisition ("Automatic target acquisition"), and the attacker seed passes it too (static study; the transport case is
traced in acquisition: `orc12-30000` n = 26982, vector V217).

Commands 2, 6, 23, 24, 27, 29 and the spell commands have their own handlers, covered by later specs (stop, harvest,
return goods, repair, build and spells).

**Smart move (command 3):**

1. If the unit is currently waiting, the wait is cut to at most **40 cycles** for a computer player's unit and
   **10 cycles** for every other player's unit, so it reacts sooner.
2. With **no target unit**, the building that covers the goal tile (in any layer) is looked at. If it passes the
   **harvest test** below, it becomes the target and the harvest row applies; otherwise the unit goes on with no
   target.

   **The harvest test** (made on the target unit, or on that building) passes in two cases only:
   - the unit is a **worker** (its type has flag bit 8: the peasant and peon types) carrying **nothing**, neither
     gold nor lumber, and the building is a **gold mine**. What the worker last harvested or was last sent to
     harvest plays no part: a worker that has never harvested, or a lumber worker between loads, passes;
   - the unit is a **tanker** (flag bit 9) not carrying oil, and the building is an **oil platform** (flag bit 11)
     of the unit's own player.

   A worker carrying gold or lumber, or a tanker carrying oil, sent onto a mine or its own platform fails the test
   (and goes on down the table). Trees and oil patches never pass: a smart move never starts a tree or oil-patch
   harvest, and a forest tile gives the move. Traced: `orc9-orc09-smart-move-2000`, vectors V224 to V228 (a worker that
   has never harvested, a worker cutting wood, a worker carrying gold, an empty tanker and a carrying tanker);
   `orc9-orc09-shore-build-3000` n = 599. An oil patch as the target unit gives the move (traced:
   `orc9-orc09-tanker-move-2200` n = 1700, vector V239); with no target unit, the oil-patch case is static
   study only.
3. Then the first matching rule applies:

| Unit and target | Result |
| --- | --- |
| The unit is a transport | Transport rules (`orders/transports.md`) |
| A land unit, and the target is its own side's transport | The transport is told to come; the unit gets order 34 (board) |
| A worker (peasant or peon type) carrying goods, and the target is a depot that accepts them | Order 24 (return goods). Workers only: a tanker carrying oil sent onto a refinery or shipyard goes on down the table and gets the move (traced for its own shipyard: `orc9-orc09-tanker-move-2200` n = 1265; static study for a refinery) |
| The target passes the harvest test of step 2 | Harvest (order 23) on that building (see the economy specs) |
| The target is its own side's unit (not a building) | Order 6 (follow) |
| Anything else | Order 3 (move); with a target unit, see below |

**A smart move with a target unit that becomes the move** keeps that unit as the move's **target** until the move
ends. At each step boundary (each time order 3's logic runs), a target that is dying, gone, inside something or no
longer detected by the unit's player ends the order as on arrival: the target is dropped, the attacker forgotten, and
idle becomes the pending order and takes over in the same update (still animation, wait 3), with the Enhanced resume.
The walk doesn't go on to the stored tile. It is the same test as order 4's (command 4 in the table above; traced for a
dying target: `orc9-orc09-smart-target-2100` n = 1877, vector V240; the inside and undetected parts are static
study only). Its stored goal is the target's tile, **not pulled back**
(`movement/pathfinding.md`, "Goal pulled back"), and its route goal G comes from the building goal rule on the
target's footprint, from the unit's tile, as for any walk whose order holds a target unit. Traced only for a
**tanker** (a sea unit): a carrying tanker onto its own shipyard and an empty tanker onto an oil patch
(`orc9-orc09-tanker-move-2200` n = 1265 and 1700, vectors V238 and V239). Traced for **land units** too, onto their own
side's buildings: a grunt, an ogre and a peon keep the building as their target to arrival, and a land unit arrives
when its walk ends next to the target's footprint (its walk goal is the footprint tile of the building goal rule,
`movement/pathfinding.md`; order 3's arrival radius is 0, so the footprint, not the radius, gives this)
(`orc9-orc09-smart-target-2100` n = 31, 67 and 78, vector V241). A land unit's smart move onto a neutral or
enemy unit, or a worker carrying goods onto a building that doesn't accept them, is **static study only**. Nothing
else in a land unit's smart move changes: the rows above still come first. The target goal zone of
`movement/pathfinding.md` ("Step 1") for this walk is static study only too (no trace needs it).

**What every command does.** Every command goes through the same steps, whoever gives it: a player, a computer
player, or the unit's own order logic (a patrol's turnaround, a resumed patrol or attack-move, a flee move, a target
search's resulting order, a follower's new follow, order 18's switch to order 17, the harvest orders' new aims, and
every other command listed in `movement/movement.md`, "Blocked rules"):

1. The target unit (if any) and the goal tile are stored; with a target unit the goal is the target's tile. A
   transport's goal gets the adjustment of `orders/transports.md`.
2. The order chosen by the table above (or by the giver) is made the **pending** order ("Setting a pending order").
3. The goal is **pulled back** (`movement/pathfinding.md`, "Goal pulled back") for the orders listed there.
4. The cached route is **dropped** and the blocked counter **cleared** (`movement/movement.md`, "Blocked rules").
5. For a unit that isn't a building, the recorded attacker and the target unit are checked ("Commands forget a stale attacker", below): a stale
   attacker is forgotten, and a target that fails the check is dropped and idle becomes the pending order in place of
   the command's order.

A command from a player or a computer player takes over at the unit's next step done, except a computer worker's job
choice from idle, which its idle logic gives ("Same-update take-over"). A command that the unit's own
order logic gives it takes over either in the same update or at the next step done, as each case says ("Same-update
take-over" lists the cases). Which goals are pulled back, and which commands keep or clear the
remembered order (Enhanced), are given with each case below.

Order 18's switch to order 17 goes through these steps and takes over in the same update (traced:
`orc9-orc09-ground-move-goals-1100` n = 158, vector V269; "Arriving and losing the target", order 18).

**A player's command forgets the attacker.** When a player's unit command is carried out
(`ui/selecting-and-commanding.md`, "Which units take part in a command"), every selected unit that takes the command
forgets its recorded attacker, whatever the attacker's state (alive, detected, in range or not). This comes after the
command is given, so the command's own checks, including the stale-attacker check below, still see the record. A unit
that doesn't take the command (another player's, the target itself, or one that fails the selection filter) keeps its
record. Commands that a computer player gives keep the record, apart from the check below, as before (traced:
`human09-fleet-3000`, vector V204). A spell command given by the player (commands 38 to 56) is a unit command like
any other and forgets the record. Two player commands take a different path and **keep** the record: a **right-click**
(`ui/selecting-and-commanding.md`, "What a right-click command does") and a **build** command, which also leaves the
remembered order as it is (static study for these three; no trace has the case).

**Commands forget a stale attacker.** Every command given to a unit that isn't a building (a player's or a computer
player's command, a resumed patrol or attack-move, a flee move, a target search's resulting order and every other
command listed in `movement/movement.md`, "Blocked rules") checks the unit's recorded attacker: if the attacker is
dying, dead or removed, or isn't detected by the unit's owner, it is forgotten. So a kept dying attacker is also
forgotten at the unit's next command (traced: `xorc01-death-knight-2100`, vector V168). A target unit given with the
command, a spell command's included (`spells/spells.md`, "Giving the order"; traced: `xhuman10-slow-3000`), gets the
same check, and for a target unit being **inside something** (a mine, platform, depot or transport, or another unit)
also fails it (traced for command 24, return goods, given a peon inside a gold mine: `orc9-orc09-return-nodepot-2200`
n = 1374, vector V245; static study only for the other commands, which share the check): if it fails, the target is dropped and idle becomes the unit's **pending** order in place of the command's
order (with the resume rules of "Resuming patrol and attack-move"); the command still counts as a command. The pending
idle takes over at the unit's next step done with wait 3, and the idle logic doesn't run in that update, so an idle
unit makes no look-around draw there. A **player's** command clears the remembered order before this check (in
Enhanced mode, "Resuming patrol and attack-move"), so after a player's command there is nothing to resume: a unit on
an attack-move told to attack a unit that is inside something keeps walking until its next step done, then idles
there, and the attack-move isn't resumed (traced: `orc9-orc09-lost-resume-goals-500` n = 332 to 346, vector V272). A
resume here needs a command from the game's own logic (a call for help, a resumed order) to a unit that still
remembers an order; that case is static study only. A call for help answered against a dying attacker ends this way
(`ai/attack-ai.md`, "Call for help"; traced: `xhuman10-spells-700`, vector V177).
A player's attack command (8 or 9) on a **dying** unit ends the same way: it is still given to each selected unit, the
target fails the check, and the unit is left in order 2 with no target and a pending idle, as if told to stop. Its still
animation restarts with wait 3 when the pending idle takes over, and it makes no look-around draw in that update (traced
with the player's own dying unit: `orc02-dyingcmd-3760`, vector V185; static study gives the same for an enemy unit).

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
- a tree the unit had reserved is released (see `occupancy.md`);
- the blocked counter is **not** cleared: only commands clear it (`movement/movement.md`, "Blocked rules"). A
  patrol's turnaround and the order given on coming out of a building are commands, so they do clear it;
- the cached route (`movement/movement.md`) is **kept**. Only commands drop it. The next step uses the cached route
  if its goal still applies;
- when the order taking over is idle (2), a unit that can move forgets its target (static study only; see "Open
  questions").

The take-over replaces the logic run of that step done: the new order's logic doesn't run in that update, and first
runs at the next step done of its animation. Only a step done with no pending order runs the order's logic.

**Same-update take-over.** A command that this run gives the unit itself can take over at once, in the same update,
without running its logic. The traces confirm it for the critter's wander, a computer worker's job choice from idle,
going idle on arriving at the end of a move ("Arriving and losing the target"), a harvest or return order that reaches
its building (order 25: wait 3, animation 3), felling a tree (`economy/harvest.md`), a lumber worker's own new aim (a
new harvest command: order 23, animation 4, wait 3; `economy/harvest.md`, "Lumber"), the order an idle unit's target
search gives (12, 15 or 16; "Automatic target acquisition"), the guard a follower is given inside its radius
(order 7, wait 3; "Follow (orders 6 and 7)"), order 18's switch to order 17 (order 17, attack animation, wait 3;
vector V269) and the friendly-fire hold's pending idle or resumed order (vectors V270 and V271). Coming out of a
building is the exception: the order given then stays pending through a 25-cycle wait (`economy/harvest.md`,
"Coming out").

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
and 8. Each unit also has a **resume countdown**. Each time a patrol or attack-move is remembered (commands 5 and 8
with a tile, and every patrol turnaround), the countdown is set to **0**; the resume step below then sets it to 40
or 20. The countdown goes down by 1 every cycle until it reaches 0, but **only while the unit remembers something**;
with nothing remembered it stays where it is (static study only for this paragraph's countdown rules: no trace gives
a remembering command while a countdown runs). So a unit that just resumed and is then given a new patrol or
attack-move can resume it at its next pending idle without waiting out the old countdown.

When idle (order 2) is made such a unit's **pending** order (on arriving, on losing its target, and wherever else
idle becomes pending) and its countdown is 0, the remembered order may be given instead of the idle; it takes over in
the same update, as the pending idle would have. An immediate idle (dying, giving up, no route) never resumes, and
keeps the remembered order. Nothing happens for a unit for which a pending order can't be set (dying, dead, inside
something, or in order 25):

| Remembered | Result |
| --- | --- |
| Patrol | It is sent back toward the patrol end it was heading for, **as a command** (goal pulled back, cached route dropped, blocked counter cleared, attacker checked), and the countdown is set to **40** |
| Attack-move, with the target tile more than 1 tile away (either axis) | The attack-move is given again **as a command** (goal pulled back, cached route dropped, blocked counter cleared, attacker checked), and the countdown is set to **20** |
| Attack-move, already within 1 tile | It stays idle, and the attack-move is **forgotten** (the countdown must be 0, as for every row) |

With the countdown running, the pending idle is set and **nothing else happens**: the remembered order is neither
resumed nor forgotten, so a later pending idle at countdown 0 can still resume it (if the unit is then more than 1
tile away) or forget it (static study only: no trace has a complete attack-move with the countdown running). The
forgetting at countdown 0 is traced: `orc9-orc09-attack-move-1500`, vectors V233 and V234.

**Giving up when blocked.** When a unit gives up at the blocked counter's limit (`movement/movement.md`, "Blocked
rules", row C), idle is made its pending order with these rules. If it remembers a patrol, or an attack-move whose
target tile is more than 1 tile away on either axis, and its countdown is 0, it starts its **still** animation with wait
**1** (not row C's 15), and the remembered order is given as a command, as in the table (countdown 40 or 20). It is the
**pending** order and takes over at the next step done, a cycle later (traced: `xhuman8-25000` n = 11284, vector
V220). Otherwise (no remembered order, the countdown running, or the attack-move complete) idle becomes the pending
order and **nothing else changes**: the unit keeps the walking animation the blocked step restarted (the move animation,
or animation 6 in orders 30 and 31), with row C's wait of 15; only the resume case starts the still animation (traced:
`orc10-10000` n = 3505 and `xhuman12-10000` n = 4144, vectors V231 and V232). A remembered attack-move that is complete is
forgotten there if the countdown is 0, as everywhere idle becomes pending (the table above). The static study finds
that every other place that makes idle pending shares this step: the still animation with wait 1, then the remembered
order as a command (static study only for them; see "Open questions"). Where the pending order then takes over in the
same update, as on arrival, the take-over's wait of 3 follows at once.

If nothing is resumed (no remembered order, the countdown running, or the attack-move complete), the pending idle is
set as usual.

**What a patrol remembers.** Every patrol command, whether from a player, a computer player or a turnaround ("Arriving
and losing the target", order 5), remembers its goal and the tile the unit stands on when the command is given; these
are the two ends the resumed patrol runs between. The computer players' sea patrol and base-edge patrol
(`ai/attack-ai.md`) then set the unit's current patrol ends to their anchor and far end, but leave what was remembered
unchanged. So the first leg of such a patrol runs to the anchor and turns toward the far end, while a patrol broken off
during its first leg and resumed after the fight runs to the anchor and turns toward the tile the unit stood on when the
patrol was given, not toward the far end (traced: `xorc11-air-sea-3000`, vector V207). Each turnaround is a new patrol
command, so from the first turnaround on the remembered ends are that turnaround's goal and tile.

The goal remembered is the goal **as given**, before it is pulled back (`movement/pathfinding.md`, "Goal pulled
back"); for a transport it is the goal after the adjustment every command to a transport gets (`orders/transports.md`).
**An unremembered patrol** (Enhanced, static study only): a patrol command given to an **air** or **sea** unit whose
goal is within 1 tile of the unit on both axes doesn't write the remembered order, and the old remembered order stays.
`xorc7-7000` n = 1002 (a sea patrol toward the destroyer's own tile) is such a case, with no visible effect.

The resumed order goes through the same steps as a player's command: the goal is stored and pulled back
(`movement/pathfinding.md`, "Goal pulled back"), the cached route is dropped, the blocked counter is cleared
(`movement/movement.md`, "Blocked rules") and the recorded attacker is checked as for every command ("Commands forget a
stale attacker", below). A unit that resumes doesn't keep its old route (traced: `xorc01-death-coil-2500`, vectors
below).

So a unit that broke off a patrol or attack-move to fight goes back to it once the fight is over. Every command a player
gives in Enhanced mode first clears the unit's remembered order (`ui/selecting-and-commanding.md`), and only commands 5
and 8 set it again, so a move command ends a remembered attack-move: the move arrives at idle. A **computer player's
command 4** (move, attacking on the way), with a target unit or a tile, leaves the remembered order as it is, so a
computer unit sent off its patrol by command 4 resumes the patrol when it later goes idle through a pending order with
its countdown at 0 (traced: `xhuman7-10000`, `xorc8-10000`, vectors V188, V189 and V191). Whether a computer player's other commands
clear it isn't known. The countdown stops it from re-issuing every cycle when the move can't progress.

### Arriving and losing the target

- **Orders 3, 4 and 10:** on arrival the unit forgets its attacker, and idle (order 2) becomes its pending order and
  takes over in the same update (still animation, wait 3). The idle logic doesn't run in that update; it first runs
  at the still animation's next step done, 3 cycles later. (Before that, at the step done, the arrival switch of
  `core/animation-scripts.md` runs the order's own animation at once, and then the order's logic runs.)
  **Order 3 with a target unit, and order 4,** end the same way, before arriving, at the first step boundary where the
  target is dying, gone, inside something or no longer detected by the unit's player ("Smart move" and "Commands";
  traced for order 3 with a dying target: `orc9-orc09-smart-target-2100` n = 1877, vector V240).
  For orders 4 and 10 the arrival test comes before the target search: a unit in order 4 or 10 whose logic runs while it
  stands at its move goal arrives, and runs **no** target acquisition in that run, even with a target in range (traced:
  `xorc11-10000`, vector V196). Whether patrol (order 5) does the same at a patrol end isn't traced. At the move goal
  the whole move step is skipped, so in Classic mode, where order 10 only reacts, the reaction doesn't run there either
  (static study).
- **No route:** if the route search finds nothing and none of the special cases of `movement/movement.md` applies (the
  harvest tree search, arrived after all, boarding, the attack-type fallbacks), the unit becomes idle as an
  **immediate** order (order 2, pending order cleared, still animation, wait 1). Only arrival makes orders 3, 4 and 10
  forget the attacker: a unit that goes idle with no route **keeps** it. Its order is now 2, so the idle logic runs in
  the same update, straight after, and may react to that attacker (a unit that fled may flee again, traced:
  `xorc10-dragons-800`, vector V170); a computer worker's job choice or a critter's wander can follow in that update
  too. The attack-type fallbacks apply only to orders whose animation is 4, so they never apply to orders 3, 4, 5 and
  10. Traces show it for patrols (`human10`, `xorc7`; vectors V101 to V103). It also applies to order 12 when its own fallback
  ("Order 12 with no route", below) finds nothing, **and to order 9**, which has no such fallback: the unit goes idle at
  once and its idle logic runs in the same update, so a unit whose target is still in its search box gets order 12 again
  at once (attack animation, wait 3), with the look-around draw made in that update. It also applies to a worker or
  tanker returning goods (order 24, `economy/harvest.md`): for a computer player's worker or tanker the idle logic is
  the job choice, whose new command takes over in the same update (wait 3). Guard (order 7) has no special case
  either: a guarding unit with no route goes idle at once, forgets its target and runs the idle logic, look-around
  draw included, in the same update (traced: `orc12-orc12-guard-board-full-900` n = 520, vector V262). Repair
  (order 27) is the same (traced: `orc9-orc09-repair-water-1300` n = 887, vector V265; that it doesn't react is
  static study only). Order 9 with a building target is traced too (`orc9-orc09-repair-water-1300` n = 942, vector V266).
- **A shore building as the target.** A unit that is neither a sea nor an air unit, with a shore building as its
  target, walks to the shore corner goal (`movement/pathfinding.md`, "Shore building") and is within reach when the
  larger of its x and y distances to that tile is at most its attack range (`movement/movement.md`, "Arriving").
  For other buildings the goal is the footprint tile the building goal rule picks, so measuring to the goal is the
  same as today's rule (traced: `orc9-orc09-shore-build-3000` n = 343, vector V221; static study agrees).
- **Order 12 with no route.** Order 12, and not order 9 or 11, has its own fallback when its route search finds none:
  1. A separate target search runs ("Automatic target acquisition") with the box of the unit's **attack range**, not its
     reaction range (the extra tile for towers applies only to tower types), and with no attacker seed.
  2. If it finds a target, order 12 on that target is given **as a command** and takes over at once, in the same
     update: its attack animation starts, the wait is set to 1 and the animation's first step runs. The take-over itself
     makes no turn and no strike decision. The update then runs order 12's logic **once** (the move step stood in for
     the logic earlier in the update, so this is its only run): it sees the mark of step 3 and skips its search, and its
     strike decision turns the unit to face the target ("Strike decision") and starts the cooldown (static study for the
     order of these parts). That run doesn't move the unit, so it can't find "no route" again, and the logic doesn't run
     a third time in the update (static study). The attack sound and the hit come later from the attack animation (in `human5-10000` the
     take-over is at n = 1759, the hit at n = 1769 and the next step at n = 1774 with wait 10, the cooldown left).
  3. The unit is marked so that its **next** order-logic run skips the target search at its start (and with it the
     acquisition). If the order at that run is idle or an attack order (2, 9, 11, 12, 13, 15, 16 or 17), that run skips
     its search (static study; no trace separates the case of a changed order). **Clearing:** at the end of an update in
     which the unit's order logic runs and the unit doesn't step, the mark is cleared only if it was already set when
     that update began; a mark first set in an update lasts at least until the next one. Only a pending order that
     takes over at the step done, **before** the order logic runs, leaves it set, and so does a move step. A take-over
     that the order logic's own run causes (for example, the order loses its target and the resumed or idle order
     takes over in that update) doesn't: the mark is still cleared at the end of that update (traced: `orc12-30000`
     n = 29162, vector V219; static study agrees). So a mark set again in an update that began marked
     (`xorc8-10000` n = 7373) is cleared at that update's end (static study).
     **The mark gates only** the search at the start of the order logic. A unit in an attack-type order that is out of
     reach of its target takes a move step instead of running its order logic, and the move step's own target search
     runs whatever the mark. That search can change the target, and the move step then looks for a route to the new
     target. If it finds none, the order's no-route fallback runs: for order 12, the attack-range search of step 1 runs
     first, again whatever the mark; if it finds a target, order 12 restarts on it and takes over at once as in step 2,
     and the mark is set again. Only if it finds none does the unit go idle at once. Either way the new order's logic
     then runs in the same update, sees the mark and skips its search (static study for these last steps; the idle case
     is traced: `xorc8-10000`, vector V203).
  4. If nothing is found, the unit goes idle at once, as above.
  Traced: `human5-10000`, vector V175. Each part is needed there: the facing, the same-update step, and a mark that
  skips the search in the take-over update's run and in the next update's run, and is then cleared.
- **Order 5 (patrol):** on arrival the unit is given a patrol to its other end, with the tile it stands on as the new
  other end. As a command, the new goal is **pulled back** (`movement/pathfinding.md`, "Goal pulled back"), for every
  patrol, human or computer, land or sea. In Enhanced mode the turnaround, like every patrol command, remembers its new
  goal and the tile the unit stands on ("Resuming patrol and attack-move"). The new patrol takes over in the same
  update (still animation, wait 3); the first step comes at its next logic run, 3 cycles later.
- **Order 18:** it walks to the goal the command pulled back and arrives by either test of "The order list" (arrival
  radius exceptions). On arrival it is given order 17 on the **target tile** as a command (order 17's goal is the
  target tile, not pulled back: `movement/pathfinding.md`), which takes over in the same update: attack animation (4),
  wait 3, facing unchanged. It doesn't strike in that update: order 17's logic first runs at its next step done, where
  the unit turns to the target tile and its attack starts (`combat/attack-ground-demolish.md`). Traced:
  `orc9-orc09-ground-move-goals-1100` n = 157 to 162, vector V269.

**Orders aimed at a unit** (follow, attack and similar) check their target every time their logic runs. The target
is **lost** if it is no longer valid (dead, removed or inside something) or is no longer **detected** by the unit's
owner (see "Terms"). Fog over the target doesn't matter: a target that walks into fog stays the target. When the
target is lost:

1. If a pending order exists, it takes over now.
2. Otherwise, order 15 (attacking in place) becomes **13** (stand ground), and every other order becomes **2**.

In **order 12** each run first runs the target search of "Automatic target acquisition". Only if the search finds
nothing new does the lost-target rule above apply. So a chaser whose target died takes the best target in its box,
if there is one, instead of going idle.

**Losing the target while cooling down.** In orders 12, 15 and 16 each run is, in this order: the target search, the
cooldown test of the strike decision (below), then the target check. A unit whose target has died or been lost while it
is still cooling down stops at the cooldown test as usual: its attack animation restarts with the cooldown left as its
wait, and its pending order is cleared. It stays in its order, with no target, until the cooldown runs out; the target
is found lost at the first run after that, and the unit goes idle (or resumes, in Enhanced mode) then (traced:
`xorc01-death-knight-2100`, vector V165).

A target that dies keeps its HP and its own target through its death animation (`combat/damage.md`, "Applying
damage"). The unit that killed it forgets it at once, in the step that applies the blow, without changing its order.
Every other unit attacking it keeps it as its target until its own next logic run, where the target is lost as above.

**A dying attacker in orders 9, 11 and 17.** Each time the logic of order 9 (attack a unit), 11 (attack a wall tile) or
17 (attack the ground) runs, the unit first checks its recorded attacker: if the attacker is dying or no longer in play,
the record is cleared. This comes **before** the target check, so it also applies in the run in which the unit then
loses its target, and the idle search that follows sees no attacker (traced: `orc09-naval`, vectors V136 and V137). Orders 12,
15 and 16 don't do this at the start of their run.

### Follow (orders 6 and 7)

In order 6 the logic first runs the attacker reaction ("Reacting to an attacker"; only
in a run where the follower isn't walking). Any order the reaction gives is replaced by what steps 2 and 3 give, but
the target it set stays.
When the reaction's search attacked a unit, that attack command has already made the picked unit the target (and order
12 the pending order), so the steps below run on the picked unit: outside the radius the follow is given again toward
it and takes over in the same update (order 6, still animation, wait **3**, target the picked unit; the pending order
12 is replaced); inside it, step 3 on the picked unit. So a follower that is attacked doesn't turn on its attacker at
once: it walks next to the picked unit and then, usually, goes idle there, and its idle logic attacks 3 cycles later
(traced: `orc9-orc09-follow-attacked-1200` n = 493 and 560, vectors V249 and V250; the guard case, a follower whose target
was lost at a step boundary, and step 1 after the reaction are static study only).

Then, each time the logic runs, with the target still valid:

1. If the follower's order is **7**, or a follow order (6) is pending, and the followed unit's own order is **not**
   one of 2, 3, 4, 5, 6, 7 or 10, the follower is given a plain move (command 3) to **its own tile**: it stops where
   it is. (A follower in order 6 with nothing pending skips this step.)
2. Otherwise, if the follower is outside its radius, it is given the follow again toward the followed unit. The
   radius is **1**, plus the +1 of `movement/movement.md` ("The +1") for a sea or air follower, as in the general
   arrival test: a ship following a ship is inside at 2 tiles.
3. When it is inside the radius, a follower in order 6 is given order **7** (guard) as its **pending** order, which
   takes over in the same update (still animation, wait **3**, target kept; its logic first runs 3 cycles later), if
   the followed unit's order is one of 2, 3, 4, 5, 6, 7 or 10 (traced: `orc9-orc09-follow-flee-1300` n = 605 and
   `orc12-orc12-guard-board-full-900` n = 220, vectors V259 and V261). Otherwise idle is made its **pending** order (with
   the Enhanced resume, "Resuming patrol and attack-move") and takes over in the same update: still animation, wait
   **3**. It is not a stop command (traced: `orc9-orc09-tanker-move-2200` n = 614, vector V237).

Order 7 then also runs the idle logic (target acquisition as for order 2), so a guarding unit attacks enemies that
come near and goes on following. A follower in order 6 never flees: order 6 isn't one of the flee rule's orders
("Fleeing", rule 3), and the reaction's result isn't used anyway. A non-combat follower that was hit keeps following
until it guards; then order 7's idle logic flees, and the flee move (order 3, no target) ends the follow (traced:
`orc9-orc09-follow-flee-1300` n = 482 to 608, vectors V258 to V260).

### Attack orders

The attack orders 9, 11, 12, 15, 16 and 17 and the idle logic (orders 2 and 13, and order 7 after its follow steps)
make the same checks in the same order, each order making only the steps the table gives it. Each time the logic
runs, in this order:

| Step | What happens | Orders |
| --- | --- | --- |
| 1. Look-around | "Idle look-around" | 2, 7, 13 |
| 2. Attacker and target search | The attacker check and the target search of "Reacting to an attacker" and "Automatic target acquisition", unless the skip mark of "Order 12 with no route" is set | 2, 7, 12, 13, 15, 16 |
| 2. Dying attacker | Instead of step 2's search: a dying attacker is forgotten ("A dying attacker in orders 9, 11 and 17") | 9, 11, 17 |
| 3. End of the idle logic | Nothing more happens; an order the search gave takes over as "Resulting order" says | 2, 7, 13 |
| 4. Cooldown test | A unit (not a flyer) still cooling down restarts its attack animation in its own order, with the cooldown left as its wait and its pending order cleared, and the run ends ("Strike decision", "Losing the target while cooling down") | 9, 11, 12, 15, 16, 17 |
| 5. Target checks | The kind check ("Checks on an attack command"); order 11's wall test ("Order 11: attacking a wall tile"); the lost-target check ("Arriving and losing the target"); the friendly-fire check ("Friendly-fire check (orders 12, 15 and 16)"); the range test ("Attacking without moving", and order 11's step 4.4) | as each rule says |
| 6. Strike | The unit turns to face its target and strikes; its cooldown starts ("Strike decision") | 9, 11, 12, 15, 16, 17 |

Statements this table makes explicit, static study only: the cooldown test comes before the target checks in orders 9
and 17, as in 11, 12, 15 and 16; a target that fails the kind check gives a
pending idle; and in order 9 or 12 a target that is valid but out of range when the logic runs is attacked again as a
command, so the unit walks on.

**Strike decision (attack orders 9, 11, 12, 15, 16 and 17).** Before a unit strikes, the attack cooldown of
`combat/damage.md` ("Attack timing (cooldown)") is tested: a unit that isn't a flyer and whose wait is less than its
cooldown left restarts its attack animation and waits for the cooldown left. When it strikes, it turns to face its
target (the facing rule of `core/animation-scripts.md`; ships use a different rule) and its cooldown starts again.

### Order 11: attacking a wall tile

A unit in order 11 walks to within its attack range of its goal tile and strikes the tile until it is no longer a wall.
The goal is the tile given with the command, pulled back as for every command; the wall test that chose order 11 was
made on the tile as given.

1. **The command.** Command 8 or 9 with no target unit tests the target tile **as given**. If it's a wall tile (a human
   or an orc wall), order 11 becomes the pending order; otherwise the command gives the order of the table in
   "Commands". Then, as for every command, the goal is **pulled back** (`movement/pathfinding.md`, "Goal pulled back")
   and the blocked counter is cleared. So when the clicked tile lies deep in a wall, the tile the unit walks to and
   strikes is the pulled-back goal: the wall tile nearest the unit along the line. A click on a tile that isn't a wall
   never gives order 11, even when its pulled-back goal is a wall tile.
2. **Walking.** Order 11 moves the unit with the normal movement rules until it's within its **attack range** of the
   goal tile (no extra tile). Its order logic runs only once it has arrived.
3. **No route.** If the route search finds no route, the unit becomes idle at once and its idle logic runs in the same
   update, as in "No route" ("Arriving and losing the target"). Order 12's own no-route fallback doesn't apply to order
   11.
4. **On arrival**, each time its logic runs, in this order:
   1. A dying attacker is forgotten ("A dying attacker in orders 9, 11 and 17").
   2. While the unit is **cooling down**, it waits for the cooldown left and nothing more happens (the cooldown test of
      "Strike decision").
   3. If the goal tile is **no longer a wall** (the wall was destroyed), idle becomes the **pending** order and takes
      over in the same update (still animation, wait 3).
   4. If the goal tile is out of the unit's attack range, idle becomes the pending order the same way. This is the same
      test as arrival, so it fails only if the unit or its goal has moved since (static study).
   5. Otherwise the unit turns to face the goal tile (ships use their own rule, as for strikes), strikes (its attack
      animation starts) and its cooldown starts again.
5. **Blocked row A.** The pending order 11 that row A of the blocked rules gives (`movement/movement.md`) is set
   directly, not through a command, so its goal is the blocking tile, not pulled back.

Traced: `orc02-victory-7639` and `orc02-defeat-4021` (the pulled-back goal, no route, and the wall gone; vectors V179 to V181).
Steps 4.2 and 4.4 rest on static study alone: they agree with the traces, but no trace separates them.

### Attacking without moving (orders 14, 15 and 16)

These orders never move the unit. They differ in what happens when the target is valid and detected but **outside the
attack range** (on either axis, measured as for arrival in `movement/movement.md`), whether it was out of range from
the start or walked out:

| Order | Target valid, detected, in range | Target valid and detected, out of range |
| --- | --- | --- |
| 14, tower watching | Fires at it | Can't happen: the tower keeps no target (below) |
| 15, attack without moving | Strikes | The target is forgotten and order **13** (stand ground) takes over **in the same update** (still animation, wait 3), as for order 16 |
| 16, idle reaction | Strikes | The target is forgotten and order **2** (idle) takes over **in the same update** (still animation, wait 3); the attacker is kept |

A lost target (invalid or not detected) is handled as in "Arriving and losing the target" for all three. For orders
15 and 16 the target search of "Automatic target acquisition" runs first, as for order 12; the lost target is
handled only if it finds nothing new. So a dying target stays the target until then.

In order 16 the idle logic first runs 3 cycles after that take-over, and may pick the same target again. When the search
in order 16 picks a **new** target that is out of range, the unit doesn't switch to it: the out-of-range rule above
applies instead (idle in the same update, wait 3, attacker kept), and its idle logic, 3 cycles later, attacks as usual.
Order 15 goes to order 13 in the same update by the same path (orders 15 and 16 share one logic; the out-of-range branch
gives 13 for order 15 and 2 otherwise). Both are traced; order 15's by `human01-order15-range-700` (vector V123). The
logic runs at the step done that ends the attack cycle, after the cooldown frames, so a target that walks out of range
is seen only then. While the unit is still **cooling down**, the cooling-down rule of "Automatic target acquisition"
wins: the search takes the new target with no range test, and the unit restarts order 16 on it with the cooldown left as
its wait, whatever order the search gave ("Automatic target acquisition", the cooling-down case). The out-of-range
idle can come only at a later run with no cooldown left (static study only).

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
specification; in the default unit data: ballista, catapult, battleship and juggernaught) checks, each time the
logic of these orders runs, the 3 × 3 tiles centred on the target's tile (a building's top-left tile; the target point
for a ground target), clipped to the map. It counts the units and buildings of **its own player** standing there
(not dying, dead or inside anything; one count per tile, using the unit or building recorded for the tile in
`movement/occupancy.md`, so a building counts once for each of its footprint tiles in the area):

- for a human-controlled player, one such unit is enough;
- for any player that isn't human-controlled (see "Terms"), it takes three.

The check is part of the order's logic: it comes after the cooldown test and the target check, and **before** the range
test, so it applies whether or not the target is in range. A run that is still cooling down ends at the cooldown test,
so the check isn't made then. It isn't made again at the strike point. In the run in which the target search has just
switched to a new target (orders 12, 15 and 16 search first), the run goes on with the new target: the cooldown test,
the target check, then this check, made on the **new** target. So a ground attacker whose new target stands next to its
own units doesn't strike it (traced for order 16: `xhuman4`, `xhuman10-spells`). If the count is reached, the unit
doesn't strike: it forgets the target, and idle becomes its **pending** order by the common path, with the resume
rules of "Resuming patrol and attack-move"; the pending order takes over **in the same update** (still animation,
wait 3; facing kept). So with nothing to resume, order **2** (idle) takes over; its idle logic first runs 3 cycles
later and may pick the same target again. In Enhanced mode a remembered attack-move or patrol is given again as a
command instead, and takes over in the same update (traced: a catapult on an attack-move, order 12 on a footman that
stands next to the catapult's own units: order 10, still animation, wait 3, no target, goal the attack-move's tile;
`orc9-orc09-hold-resume-hits-goals-1600` n = 960, vector V270). A complete attack-move (within 1 tile) is forgotten
there and the unit stays idle (same trace, n = 1112, vector V271).

### Boarding moves (order 34)

Order 34 is a walking order: while the unit walks to its goal only the movement runs at each step, and the order's logic
runs when the unit arrives (`orders/transports.md`). So a unit of **any** player, computer players included, stays in
order 34 while it walks to the transport; traces show it for computer players (vectors in `orders/transports.md`). The
moves below are given only when the order's logic runs and finds the unit not yet arrived. The computer players' move
is traced after a no-route restart (`orders/transports.md`: `human13-10000` n = 9154 and `xhuman5-30000` n = 26761);
the other players' move is static study only:

- **Every player except computer players** (human players, passive computer players, neutral; see "Terms"): a smart
  move (command 3). It meets the boarding rule of "Smart move" again, so the unit **stays in order 34** and the
  transport is told to come again (unless it is landing).
- **Computer players:** command 4 (move and attack on the way). The unit **leaves order 34**: its order becomes 4,
  aimed at the transport's tile, wherever the transport stands at each walk ("Attack-move"). A non-combat unit is
  first handled as a smart move, as for any command 4, but its
  order still becomes 4. It boards only if it is later given the boarding command again.

### Automatic target acquisition

Units in an order marked "acquires" look for an enemy each time their logic runs. Enhanced mode adds acquisition to
attack-move (order 10). A unit in an attack-type order that is out of reach of its target runs a move step instead of
its order logic, and the move step makes the same search; the skip mark of "Order 12 with no route" doesn't stop that
one.

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
  units, shore buildings and oil platforms. The check is the attack command's kind check ("Commands", "Checks on an
  attack command"), so a candidate of a transport type standing on shore water can be picked by a searcher that can
  target land **or** sea (traced: `orc12-30000` n = 26982, a capture not committed but re-captured identically later; with this rule off the comparison fails there; static study agrees);
- both are air units and the candidate can't attack;
- it is inside the searcher's minimum range: its distance d (below) is less than the searcher's missile's minimum
  flight value minus 1 (`damage.md`, "Missile weapon kinds"). So catapults and ballistas skip d ≤ 1, and cannons
  and other missiles skip nothing (traced: `orc5-40000` n = 21489, vector V214);
- in Enhanced mode, on attack-move (order 10) only, a sea unit only considers sea units, shore buildings and oil
  platforms. A sea unit on patrol (order 5) uses the ordinary skip list (traced: `xorc11-10000`, vector V187).

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

**Forgetting a dying attacker first.** Every target search (acquisition, the search in the reaction and order 12's
fallbacks) starts, for a unit that can move, by forgetting its recorded attacker if that attacker is dying or no longer
in play. A unit can move if its type has any movement-class flag (the five lowest type flag bits of the unit data);
buildings never do, and nor do the remains types (0x67 to 0x6D) or the unused types 0x22, 0x30 and 0x36. The search then
runs with no attacker seed. The resulting order below is still picked from the record as it was before the search, so
the first search after the attacker started dying can still give order 12; later searches see no attacker and use the
"otherwise" cases (traced: `xorc6-10000`, vector V197).

**Choice.** The search starts with the unit's current **attacker** (the first attacker that is still alive; see
`ai/attack-ai.md`, "Call for help", steps 1 to 4) as the best candidate, with its own score, but only when that score is
**above 0**. A scored candidate always has at least 1 ("Score"), so the attacker's score is 0 only when the candidate
checks reject it before scoring: it is of a kind the unit **can't target**, it is **inside the unit's minimum range**,
it isn't **detected** by the unit's owner, or it is **hidden** from that owner by the fog ("Terms"). An attacker's
strikes reveal its surroundings to its target's owner until the next fog refresh (`vision/fog.md`, "When sight is
applied", item 4), so an attacker that has just struck usually passes the fog test; one that struck before a refresh and
stands in fog doesn't. Such an attacker doesn't seed the search, which then runs as if there were no attacker; the
attacker still counts for the resulting order below (traced: `human06-31357` and `xhuman11-10000`, vectors V157, V198 and V199). A
candidate replaces the best only with a **strictly greater** score. So on a tie the attacker wins, and otherwise the
first candidate in vertical order wins.

**Resulting order** when a target is chosen:

| The searching unit's order | Every other player | Computer player |
| --- | --- | --- |
| An idle-type order (2, 7, 16) | 12 (chase) whenever the unit has an attacker (a dying one included); otherwise 16 (attack without moving). Order 16 still cooling down: stays 16 (below) | 12 whenever the unit has an attacker (a dying one included); otherwise 12 if the target is a building or can attack, else 16. Order 16 still cooling down: stays 16 (below) |
| A moving order (4, 5, 10, 12) | 12 | 12 |
| Anything else (13, 14, 15) | 15 | 15 |

In every case the resulting order is given as a **command**: the blocked counter is cleared and the cached route
dropped, as for every command. A recorded attacker that is dying counts as an attacker in the table above in the search
that forgets it ("Forgetting a dying attacker first"). Its death clears no record by itself: only the killer forgets its
victim as its attacker (`combat/damage.md`).

When the searching unit's order is idle-type, the resulting order then takes over in the same update ("Same-update
take-over"): attack animation, wait 3, and its logic first runs at that animation's next step done. The exception is a
unit in order 16 that is still cooling down, which keeps order 16 (below).

When the searching unit is in order 12 and the search picks a different target:

- **In reach:** the order's logic continues in the same update with the new target: the cooldown test, then the
  strike decision (with its turn toward the target). If the unit is still cooling down, the new target and order
  take effect at once and the order's animation restarts with the cooldown left as its wait (no wait of 3).
- **Out of reach while still cooling down:** handled as in reach, whichever of the out-of-reach cases below would apply:
  the new target and order take effect at once and the attack animation restarts with the cooldown left as its wait.
  There is no command, no wait of 3 and no step in that update, and a strike already decided on the old target still
  lands (traced: `xhuman12-10000`, a unit within strike reach of its old target; vector V193).
- **Out of reach:** order 12 on the new target is given as a **command**: the target changes at once, the cached
  route is dropped and the blocked counter cleared (as for every command). The logic carries on in the same update
  and the unit starts its next step, toward the new target, in that update. The new order takes over at the next
  step done ("Pending orders and immediate orders") with the attack animation and wait 3, and the unit walks on at
  the run after that. A unit standing in its attack animation doesn't step: it keeps standing until the take-over.
  This case applies whenever the unit still records an old target, whatever that target's state: dying, inside a
  building (a worker in a gold mine, for example) or otherwise not detected. The exceptions are the next two cases: this
  unit killed the old target (and so forgot it) in this update, or it's within strike reach of the old target. The
  unit's animation doesn't matter (traced: `human5-10000`, vector V186).
- **Out of reach, but within strike reach of the old target:** order 12 on the new target takes over at once
  (attack animation, wait 3), as in the next case. No step is taken in that update.
- **The old target is gone or was forgotten** (this unit killed it, or its slot is gone): the new order takes over at
  once (attack animation, wait 3).

The cooling-down case applies also to a unit in order 15 or 16 whose search picks a different target, whatever order
the table gives. The search's order is given as a command, as always, and the run goes on to the cooldown test. A unit
(not a flyer) that is still cooling down restarts its attack animation there **in its own order** (15 or 16), with the
cooldown left as its wait, and that restart clears the pending order, as in "Losing the target while cooling down". So
the unit keeps its order and takes the new target at once, and the order the search gave is dropped. For a unit in
order 16 whose search gives order **12** (it has an attacker, or it is a computer player's unit and the target is a
building or can attack), the unit stays in **order 16** on the new target, waiting for the cooldown left: there is no
take-over with wait 3. With no cooldown left the table applies as usual. Traced for order 16 when the search gives the
same order: `xhuman12-10000`, vector V195. Traced for order 16 when the search gives order 12:
`orc9-orc09-order16-keep-2000` n = 937 (vector V264; with this rule off the comparison fails there), and the
uncommitted capture `xorc5-30000` n = 20526 (vector V255). It agrees with the traced restart of "Losing the target
while cooling down". The computer-player case with no attacker is static study only, and so is order 15 (its table order
is 15 anyway).

**Attack-move: orders 4, 5 and (Enhanced) 10.** When a unit in order 4 (move, attacking on the way), 5 (patrol) or, in
Enhanced mode, 10 picks a target, order 12 on that target is likewise given as a command: the target is set and the
cached route dropped at once, but the order stays 4, 5 or 10 until the next step done. The order's logic carries on in
the same update, **heading for the new target**: a unit at a step boundary starts its next step on a new route toward
the target, not toward the order's own goal. For a **building** target the new route is the one an attack on that
building would take: toward the footprint tile picked by the building goal rule, with the building's goal zone (its
footprint widened by one tile; `movement/pathfinding.md`, "Step 1"). A unit target is headed for at its tile (traced:
`human06-31357`, vector V120), with the **target's** goal zone, as for a building: its footprint widened by one tile,
rounded out to even for a two-tile mover (`movement/pathfinding.md`, "Step 1"), not the 3 × 3 around its tile. While
order 4 keeps a target unit, its own walk uses that zone too, and G is the target's tile where it stands now (for a
building, the tile the building goal rule picks; static study), not the stored goal, and the zone comes from the target
there. Every walk sets G again from the target, so the goal the command stored, and its pull-back, last only until the
first walk (traced: `xhuman5-30000` n = 26764, order 4 on a transport, vector V256). The zone is traced at
`orc5-40000` n = 33260 and `orc12-30000` n = 23612 (vectors V215 and V216); they don't separate G from the stored goal:
in `orc12-30000` G is the picked ogre's tile (rounded to even) either way, and in `orc5-40000` the transport has moved
two tiles off the stored goal, but both goals give the same first step. The zone rule and the goal rule are wider: they
serve every walk whose order holds a target unit (`movement/pathfinding.md`, "Step 1"; static study only for orders
other than 4 and for building targets). Order 12 takes over at the next step done (attack
animation, wait 3). This holds even when the new target is already within strike reach: there is no strike in that
update, the walk steps toward the target, and order 12 takes over at the next step done. Orders 4 and 10 are traced;
patrol changes no trace in the set. A unit in order 4 or 10 standing at its move goal doesn't search at all: it arrives
("Arriving and losing the target").

In order 16 a new target out of range isn't taken unless the unit is cooling down ("Attacking without moving").

Order 32 (docked transport) carries the idle-type mark in the order data too, but its logic does nothing, and the
reaction in "Reacting to an attacker" gives order 12 without this table. So a unit in order 32 never reaches this
table; only transports use order 32, and they can't attack anyway.

**Special cases:**

- **Mages** (mage and death knight types, including heroes of those types) search like any other unit, scoring every
  candidate. The rule is applied to the **chosen** target: if the best candidate can attack and isn't a non-combat unit
  (workers, tankers, transports, demolition units), they acquire nothing in that run; if they have an attacker, they run
  the reaction instead, which treats them as non-combat units, so they **flee** ("Reacting to an attacker"). A harmless
  target is never chosen in place of a dangerous one that scores higher.
- In Enhanced mode a **demolition unit** (dwarven demolition squad, goblin sapper) with a remembered order uses the
  target to start a demolish instead of an attack.
- Workers and tankers of **any** player never run the search. A computer player's workers and tankers run their AI
  routines here instead (later spec); every other player's use the reaction ("Reacting to an attacker") when they
  have an attacker, and acquire nothing otherwise.

**Vertical order.** The game keeps all units in a list sorted by **pixel y** (the vertical part of the pixel
position defined in `movement/movement.md`):

- The list is re-sorted **once per cycle**, at the end of phase 4 of `core/timing-and-rng.md`, after the units,
  missiles, construction pass and timers pass. So it is not re-sorted between one unit's update and the next, and a
  search made during phase 4 sees the order left by the previous cycle's sort (plus inserted new units).
- The sort is a stable insertion sort with one quirk: walking down the list, each entry that is above its predecessor
  is swapped backwards one place at a time, but the backward walk never compares the first two entries of the list.
  Only the first step of the pass (entries 1 and 2) can change which entry is first. So an entry can move up to
  the **second** place in one pass, and reaches first place only in a later cycle.
- Units that are dying, dead or removed sort as if at pixel y −32768. A unit **inside** something (a transport, mine,
  platform or building under construction) keeps its place: it sorts by the pixel y it had when it went in, which
  doesn't change while it is inside.
- A unit is put into the list only when it is created (inserted before any units with the same pixel y) or when a saved
  game is loaded, and taken out only when its slot is released. A unit **coming out** of a building or transport isn't
  re-inserted: it starts from its old place and moves only by the once-per-cycle sort, with the sort's one-place quirk.
- The search uses **tile rows**, not pixels. It skips the dying, dead and removed entries at the front. Inside units are
  visited like any entry, by their recorded tile row, so they take part in the halving search's middle tests and can
  narrow its range; the per-candidate checks then reject them. Its **top row** is the box's top row − 3 (but not below
  0), to catch tall buildings whose footprint reaches into the box, and its **bottom row** is the box's bottom row. It
  visits the entries from a first entry to a last entry in list order, found by the halving search below. Because a
  moving unit's tile changes at the start of a step while its pixel y changes gradually, tile rows in the list are only
  nearly sorted, so this search can include or miss different units than an exact partition would.

**The halving search.** "Above" means a smaller row number, "below" a larger one; a row is an entry's tile row.
Entry positions count from the first entry after the skipped front entries ("the first") to the list's last entry.

- **No range:** if the top row is below the last entry's row, or the bottom row is above the first's, nothing is
  visited.
- **First entry.** Keep a low and a high position, starting at the first and the last. Repeatedly take the middle,
  floor((low + high) / 2). Stop when the middle entry's row is above the top row and the next entry's row isn't, or
  when low equals high. Otherwise, when the middle entry's row is above the top row, low becomes
  floor((low + high + 1) / 2); else high becomes floor((low + high − 1) / 2). The first entry visited is the middle
  entry, or the one after it if the middle entry's row is above the top row.
- **Last entry.** The same from the other side, starting with low at the **first entry found** (the result of the
  first-entry search, not the first position) and high at the last entry. Stop when the middle entry's row is below the
  bottom row and the entry before it isn't, or when low equals high. Otherwise, when the middle entry's row is below the
  bottom row, high becomes floor((low + high − 1) / 2); else low becomes floor((low + high + 1) / 2). The last entry
  visited is the middle entry, or the one before it if the middle entry's row is below the bottom row.
- If the first entry visited has a row below the bottom row, nothing is visited and the last-entry search isn't made.

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

1. If the unit can't attack its attacker's kind (the kind check alone, air, sea or land, as in "Checks on an attack
   command"; there is no fog, detection, alliance or minimum-range test here, so an attacker rejected from the seed only
   by those goes on to step 2 unseeded; static study, and `xhuman11-10000` n = 9065 agrees), then: if the attacker is
   dying or no longer in play, it is forgotten, there is no flee and no target search in that run, and the rest of the
   order's logic goes on as usual (traced: `xorc11-10000`, where a battleship that has just killed its target forgets a
   dragon that is no longer in play, finds its target lost and resumes its patrol; vector V192). Otherwise the unit
   runs the **flee** rule below, and nothing else happens in that run. Step 1 looks at the recorded attacker as it is:
   an attacker that the unit's owner doesn't **detect** still leads to the flee, and the "Terms" rule that ignores an
   undetected attacker applies only from step 2 on, to the target search and its seed (traced: `human12-25000`
   n = 16061, a transport that flees from a giant turtle it doesn't detect; vector V208). Step 1 comes first for every
   unit with an attacker and for every player, workers and tankers included: only a unit that **can** attack its
   attacker's kind goes on to steps 2 and 3. A tanker can attack no kind, so a tanker with a valid attacker always runs
   the flee rule; a worker hit by a ship or a flyer flees the same way (traced: `human12-25000` n = 16612).
2. Otherwise, for a combat unit, the ordinary target search runs once, starting with the attacker as the best
   candidate. If the result differs from the unit's current target, the unit attacks it with the order from the
   resulting-order table, which for the idle-type orders is **12 whenever the unit has an attacker**, for human and
   computer players alike. There is no second search in the same run.
3. Workers and tankers that pass step 1 (so only workers, never tankers) use the reaction below instead
   (computer players' workers and tankers first run their AI routines, a later spec); a computer player's worker or
   tanker that fails step 1 runs the flee rule and not its AI routine. A **mage-type unit** (mage and death knight types, heroes included) runs the search: if the
   chosen target is one it may take (a non-combat unit or one that can't attack; "Special cases") and differs from its
   current target, it attacks it; otherwise it runs the reaction, and so flees from the attacker (traced:
   `xorc01-death-coil-2500`, vector V172). An invisible mage-type unit does nothing more in that run after the
   attacker check (static study only).

**The reaction**, used by the orders that react without acquiring (attack-move in Classic mode, demolish orders
19 to 21) and by several other orders' logic: harvest (23), return goods (24), repair (27), follow (6) and board (34).
When a walking unit reacts, and when it doesn't, is given in "Walking orders" and "Walking orders react only when the
walk has ended".

- If the attacker is no longer valid (dying, dead or inside something), or the unit's owner is not playing, or the
  unit is a building, or it can't attack, the attacker is simply forgotten.
- Otherwise, if the unit is a non-combat unit (worker, tanker, transport, demolition unit or mage-type unit: mage and
  death knight types, heroes included), it **flees** (below). This path has no detection test either: an attacker the
  unit's owner doesn't detect is fled from as in step 1 (static study; no trace changes).
- Otherwise it runs the target search (above), which starts with the attacker as the best candidate. If the result
  differs from the unit's current target, it attacks the result with order 12, whatever the resulting-order table
  says.

**Fleeing** (the attacker must still be valid: not dying, dead or inside something; otherwise it is forgotten and there
is no flee). The flee rule has no detection test: an attacker the unit's owner doesn't detect is fled from like any
other (traced: `human12-25000`, vectors V208 and V209).

1. A worker attacked by a worker fights back (command 8 against the attacker). Done.
2. A **computer player's** unit flees only if it is idle (order 2).
3. **Every other player's** unit (see "Terms") flees only if its order is 2, 7 or 27, or its order is 23 (harvest)
   and it has **no target unit**. Only the target unit is tested: a gold worker's target unit is its mine and a
   tanker's is its platform, while a lumber worker never has one (`economy/harvest.md`, "Starting to harvest"). The
   tree tile, the chopping state and the return-to source are not looked at. So:
   - a gold worker or tanker on its way to its mine or platform doesn't flee;
   - a lumber worker flees while chopping, and on reaching its tree; while it is walking to the tree it doesn't react
     ("Walking orders react only when the walk has ended", above);
   - a tanker in order 23 with no platform (its target was cleared) flees;
   - a lumber worker whose tree search found nothing is idle (order 2), so it flees by the order-2 rule.

   Order 23's logic runs the reaction each time it runs while the worker isn't walking, so a chopping worker reacts at
   its next chop-loop run. A fleeing lumber worker's reserved tree is released when the flee move takes over ("Taking
   over").
4. The flee tile is the unit's tile plus, on each axis, `(r mod 8) − 2 + 4·k`, where r is a fresh random draw (the
   game's RNG, `timing-and-rng.md`; x is drawn first, then y), and k is the attacker's **facing** direction as a unit
   step: facing 0 to 7 gives (k_x, k_y) = (0,−1), (1,−1), (1,0), (1,1), (0,1), (−1,1), (−1,0), (−1,−1).
   Two-tile movers (sea and air units) use twice those steps (so 8 tiles instead of 4).
5. The tile is clamped to the map, and for two-tile movers each coordinate is rounded down to even.
6. The unit is given a **smart move** (command 3, "Commands") to that tile, and its attacker is forgotten. Two of the
   smart move's parts apply. Its wait cap cuts a waiting unit's wait to at most 40 cycles for a computer player's unit
   and 10 for every other player's unit (static study only; no trace shows it). A transport takes the transport outcome,
   so a transport whose flee tile is on the shore lands (order 30; traced: `xorc8-10000`, vector V201). The harvest
   outcomes need the harvest test of "Smart move" step 2: a worker carrying nothing whose flee tile is a gold
   mine's tile harvests that mine (order 23; traced: `orc9-orc09-shore-build-3000` n = 599), and so would a tanker
   not carrying oil whose flee tile is its own player's oil platform's tile. A flee never starts a tree harvest, even
   to a tile next to a forest: every other unit gets a move (order 3). As a command, its goal is pulled back
   (`movement/pathfinding.md`, "Goal pulled back"), its blocked counter is cleared and its cached route dropped. A unit
   that **isn't in its move animation** takes the new order in the same update. The take-over starts that order's
   animation (for order 3 the still animation, for order 30 the landing animation 6) with wait 3, whatever animation the
   unit was showing (attack, walking or still), and the unit steps at its next run. A unit in its move animation keeps
   the order pending until its next step done, where it takes over the same way.

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

Vector IDs are stable: they number the vectors in the order the spec held them before the consolidation, so the
review file can map each one. "Source" says whether a vector is from an oracle trace (the trace is named in the
row and in the Source column), from static study, or a worked example of the rules. A row that continues another
row's trace starts "As" that row's ID and names the trace.

### Pending orders and taking over

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V020 | New order given while the unit's order is 25 | ignored | Example |
| V080 | Human04 great hall (slot 1550, computer, created with wait 8) | Order 33 takes over at n = 8 (wait 3). Order 33 logic runs at n = 11, 16, 21 (wait reads 3, 2, 1, 1, then 4, 3, 2, 1, 1). Its think counter reaches the third run at n = 21: it trains a peon (gold −400 at n = 21), and order 37 takes over at n = 26 | Trace (Human04) |
| V081 | The hall of V080 (Human04 trace), the peon appears at n = 489 (463 cycles after n = 26) | Order 33 takes over at once with wait 3. Its logic runs at n = 492, 497, 502; it thinks at the third run, n = 502 (gold −400), and order 37 takes over at n = 507. The next peon appears at n = 970; then payment at n = 983, take-over at n = 988 and peon at n = 1451 | Trace (Human04) |
| V082 | Human04 peon (slot 1548, computer, created with wait 6) | First step done at n = 6: idle logic, then the worker job choice (`ai/worker-ai.md`) picks gold mine 1559, the only mine in its region, with the great hall 1550 as depot. At the end of n = 6: order 23, target 1559, wait 3 | Trace (Human04) |
| V083 | Human01 gold mine (slot 1589, wait 1 at setup) and farm (slot 1594, wait 2 at setup) | The mine: order 33 with wait 3 at n = 1. The farm: wait 1 at n = 1, order 33 with wait 3 at n = 2. Each then has a step done every 5 cycles | Trace (Human01) |
| V084 | Human04, critter slot 1579, moving (order 3) to tile (58, 43) | n = 57: order 3, animation 3, wait 1. n = 58 (reaches (58, 43)): order 2, animation 2, wait 3; no idle logic. n = 61: first idle logic run; the critter wanders (order 3 takes over at once: wait 3, animation 2). n = 64: the move logic runs, animation 3, wait 1, facing 5 | Trace (Human04) |
| V085 | Human04, critter slot 1580, moving to (63, 43) | n = 58: order 2, animation 2, wait 3. n = 61: idle logic, no wander (wait 1 at n = 61 and 62) | Trace (Human04) |
| V086 | Human04, peon 1548, harvest order 23, reaches the mine | n = 89: order 25, animation 3, wait 3 | Trace (Human04) |
| V087 | Human04, peons 1548 and 1531, return order 24, reach the depot | Order 25, animation 3, wait 3 at n = 374 (1548, great hall) and n = 1938 (1531, lumber mill) | Trace (Human04) |

### Commands

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V011 | Smart move given to a human unit with 37 cycles left to wait | its wait becomes 10 | Example |
| V012 | As V011, a computer player's unit with 37 cycles left | wait stays 37 (cap 40) | Example |
| V013 | As V011, a passive computer player's unit with 37 cycles left | its wait becomes 10 (not a computer player) | Example |
| V014 | Command 8 on an empty tile, Enhanced | order 10, attack-move remembered | Example |
| V015 | Command 17 on a point 6 tiles away, catapult (range 8) | order 17 at once | Example |
| V016 | Command 17 on a point 12 tiles away in the catapult's own land area | order 17 at once; it walks until within 8 tiles | Example |
| V017 | Command 17 on a forest tile 12 tiles away, catapult | order 18; becomes 17 when within 8 tiles on both axes | Example |
| V018 | Command 19 on a tree tile | order 20 | Example |
| V019 | Command 19 on open grass | order 21 | Example |
| V184 | `orc02-defeat-4021`, turn n = 3602: command 8 on the player's own Zul'jin 1594 (type 0x35) for grunts 1568, 1572, 1575, 1576 and 1578 | Each records target 1594 with order 9 pending; 1575 and 1576 are in order 9 at n = 3603 | Trace (`orc02-defeat-4021`) |

### Smart move

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V224 | `orc9-orc09-smart-move-2000` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-smart-move.txt`), n = 32: peon 1482 (type 0x03, owner 5) at (34, 89), which has never harvested, given a smart move to (51, 92), a tile of gold mine 1480, no target unit | Target 1480 at once (n = 32); order 23 on 1480 from n = 37 (animation 4, wait 3) | Trace (`orc9-orc09-smart-move-2000`) |
| V225 | As V224 (`orc9-orc09-smart-move-2000`), n = 201: peon 1478 cutting wood (order 23, no target, not carrying), smart move to (51, 92) | Order 23 on 1480 | Trace (`orc9-orc09-smart-move-2000`) |
| V226 | As V224 (`orc9-orc09-smart-move-2000`), n = 481: peon 1482 carrying gold (order 24), smart move to (51, 92) | No harvest: target cleared, order 3 at n = 499 | Trace (`orc9-orc09-smart-move-2000`) |
| V227 | As V224 (`orc9-orc09-smart-move-2000`), n = 965: orc tanker 1473 (type 0x1B), stopped and empty, smart move to (58, 76), a tile of its own player's oil platform 1468 | Target 1468, wait cut to 10; order 23 on 1468 from n = 975 | Trace (`orc9-orc09-smart-move-2000`) |
| V228 | As V224 (`orc9-orc09-smart-move-2000`), n = 1262: tanker 1473 carrying oil, smart move to (58, 76) | No harvest: wait cut to 10, target cleared; order 3 at n = 1272 | Trace (`orc9-orc09-smart-move-2000`) |
| V229 | A smart move onto a forest tile or an oil patch, no target unit (static study for the oil patch) | The move (order 3) | Example; static study for the oil patch |
| V230 | A smart move with an oil patch as the target unit (traced: V239) | The move (order 3), keeping the patch as its target | Example; traced by V239 (`orc9-orc09-tanker-move-2200`) |
| V223 | A tanker not carrying oil given a smart move to a tile of its own player's oil platform, no target unit | Harvest (order 23) on the platform (traced: V227) | Example; traced by V227 (`orc9-orc09-smart-move-2000`) |
| V238 | As V237 (`orc9-orc09-tanker-move-2200`), n = 1265: orc tanker 1473 (type 0x1B), carrying oil, smart-moved onto its own shipyard 1499 (type 0x49) | The move: order 3 from n = 1277 with target 1499; at (48, 80) it arrives, order 2 with no target at n = 1408 | Trace (`orc9-orc09-tanker-move-2200`) |
| V239 | As V237 (`orc9-orc09-tanker-move-2200`), n = 1700: tanker 1473, empty, smart-moved onto oil patch 1517 (type 0x5D, top-left (47, 51)) | The move: order 3 from n = 1720 with target 1517, route goal from the building goal rule and stored goal the patch's tile, not pulled back (at n = 1819 it steps to (50, 70)); it arrives at (48, 54), order 2 with no target, at n = 2107 | Trace (`orc9-orc09-tanker-move-2200`) |
| V240 | `orc9-orc09-smart-target-2100` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-smart-target.txt`), n = 1792: peon 1478 (type 0x03, owner 5) smart-moved onto its own pig farm 1496 (type 0x3B), which player 5's catapults (type 0x05) are attacking | Order 3 with target 1496 from n = 1794. The farm is dying from n = 1869 (ruin type 0x6B from n = 1870). At the peon's next step boundary, n = 1877 at (41, 87): order 2, target none, still animation, wait 3; it doesn't walk on to the farm's tile | Trace (`orc9-orc09-smart-target-2100`) |
| V241 | As V240 (`orc9-orc09-smart-target-2100`): grunt 1481 (type 0x01) smart-moved onto pig farm 1479, next to it; ogre 1493 (type 0x07) onto orc barracks 1500 (type 0x3D, top-left (33, 78)); grunt 1492 onto pig farm 1489 (top-left (33, 86)) | Each keeps its target to arrival: 1481 idle at n = 31; 1493 arrives at (34, 81) at n = 67; 1492 at (35, 87) at n = 78; each then order 2 with no target | Trace (`orc9-orc09-smart-target-2100`) |
| V242 | As V240 (`orc9-orc09-smart-target-2100`), n = 102: ogre 1493 smart-moved onto peon 1482 (its own side's unit, in order 23) | Order 6 from n = 103; idle (order 2, no target) at n = 298, when the peon has gone into the gold mine (order 26 from n = 294) | Trace (`orc9-orc09-smart-target-2100`) |
| V246 | A smart move whose target unit goes into a transport or becomes undetected mid-walk (static study) | Ends as on arrival at the next step boundary: order 2, wait 3, target dropped | Static study |

### Commands and the attacker record

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V169 | A command to a unit whose recorded attacker is dying | The attacker is forgotten | Example |
| V177 | `xhuman10-spells-700` (oracle trace): at n = 324 an axe from 1478 (type 0x09, owner 2, dying since n = 323) hits knight 1493 (type 0x06, owner 1); idle knight 1480 (type 0x06, owner 1) at (83, 90) answers with command 12 against 1478 | The target fails the check: idle becomes its pending order. n = 328, its next step done: order 2, animation 2, wait 3, target null, no look-around draw (7 draws in that cycle, not 8) | Trace (`xhuman10-spells-700`) |
| V178 | An idle S next to V; V hit by a missile from A, A already dying | S answers; its order stays 2; the pending idle takes over at its next step done | Example |
| V185 | `orc02-dyingcmd-3760` (oracle trace): 1594 is dying from n = 3718; the command at turn n = 3722 sends idle grunts 1572 (wait 4) and 1575 (wait 1) to attack it | n = 3723: both in order 2, target null, wait 3; two draws in that cycle, against three in `orc02-defeat-4021`, where 1575 looks around | Trace (`orc02-dyingcmd-3760`) |
| V204 | `human09-fleet-3000` (oracle trace, Enhanced), command at turn n = 1502: battleship 1539 (type 0x20, owner 6, the human player) at (76, 36) in order 12 on catapult 1519 (type 0x05, owner 1) at (83, 42), its recorded attacker; command 4 to (28, 40), which has no route | n = 1504: order 4, wait 3; n = 1507: order 2, wait 1, target null (the attacker was forgotten at the command, so nothing seeds the idle search) | Trace (`human09-fleet-3000`) |
| V205 | A footman with a recorded attacker (alive, detected, within its reaction range) gets a stop command from its player (static study) | The attacker is forgotten; the idle logic doesn't seed its search with it | Static study |
| V206 | A computer player's footman with a recorded attacker gets command 4 from the attack AI (static study) | The record is kept | Static study |
| V245 | `orc9-orc09-return-nodepot-2200` (oracle trace, Enhanced), n = 1374: peon 1482 (type 0x03), idle with return goods to its great hall pending, told to return goods to peon 1478, which is inside a gold mine | Target none in the same turn record; order 2 kept; the pending idle takes over at n = 1393 (still animation, wait 3) | Trace (`orc9-orc09-return-nodepot-2200`) |
| V272 | `orc9-orc09-lost-resume-goals-500` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-lost-resume.txt`): grunt 1481 (type 0x01, owner 5) on an attack-move to (8, 87) since n = 30; at turn n = 332 (input at n = 330) the player tells it to attack peon 1482, which is inside gold mine 1480 (order 26) | n = 332: order 10, move animation, wait 1, target none, goal (50, 91) (the peon's tile), pending 2. n = 346, its next step done: order 2, still animation, wait 3, nothing pending; the attack-move isn't resumed (the player's command forgot it) | Trace (`orc9-orc09-lost-resume-goals-500`) |

### Resuming patrol and attack-move

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V023 | Enhanced, footman on attack-move kills a target 10 tiles short of the goal and goes idle, countdown 0 | attack-move re-issued; countdown 20 | Example |
| V152 | Enhanced (static study only): a unit on an attack-move acquires an enemy on the way and kills it; countdown 0; the target tile more than 1 tile away | The attack-move is given again instead of the pending idle (countdown 20) | Static study |
| V166 | `xorc01-death-coil-2500` (oracle trace): ogre 1489 (type 0x07, owner 5) loses its target at n = 901 and resumes its attack-move (order 10, wait 3) | The cached route is dropped: n = 904, it steps to (56, 70), facing 1 | Trace (`xorc01-death-coil-2500`) |
| V167 | A resumed attack-move whose unit had a cached route and a blocked counter of 3 | Route dropped, counter 0 | Example |
| V168 | `xorc01-death-knight-2100`: ogre 1489 resumes its attack-move at n = 1191 while its recorded attacker, a skeleton, is dying | The attacker is forgotten. n = 1329: blocked at (67, 67), goal (68, 68), no attacker, so row I: order 10, wait 1. n = 1330: no route, immediate idle: order 2, wait 1 | Trace (`xorc01-death-knight-2100`) |
| V188 | `xhuman7-10000` (oracle trace, Enhanced): submarine 1422 (type 0x26, owner 1) hits troll destroyer 1420 (type 0x1F, owner 6); at n = 52 the submarine call gives giant turtle 1511 (type 0x27, owner 6), patrolling since n = 7, command 4 on 1420 | n = 52: target 1420 (order still 5); n = 53: order 4, animation 2, wait 3, target 1420 | Trace (`xhuman7-10000`) |
| V189 | As V188 (`xhuman7-10000`): 1420 dying from n = 252; 1511 reaches its next step boundary at n = 271 | Order 5 (the patrol resumed), animation 2, wait 3, target null; it heads for (24, 60) by n = 274 | Trace (`xhuman7-10000`) |
| V190 | As V188 (`xhuman7-10000`), n = 151: submarine 1422 hits 1420 again; 1420's owner doesn't detect it | 1420 still records 1422, so there is no new call; 1511 keeps its order 4 on 1420 | Trace (`xhuman7-10000`) |
| V191 | `xorc8-10000` (oracle trace, Enhanced): computer submarine 1473 (type 0x26, owner 2) patrols from n = 552, gets command 4 on a tile at n = 4553, is in order 12 on 1411 from n = 5433, and loses it at n = 5581 | Order 5, animation 2, wait 3, target null | Trace (`xorc8-10000`) |
| V192 | `xorc11-10000` (oracle trace), n = 628: battleship 1539 (type 0x20, owner 6), on order 12 after killing its target, with patrol remembered; its recorded attacker, dragon 1526 (type 0x2B), which it can't target, is no longer in play (order 0, owner 15) | Order 5, animation 2, wait 3, target null; no search | Trace (`xorc11-10000`) |
| V207 | `xorc11-air-sea-3000` (oracle trace, Enhanced): computer battleship 1539 (type 0x20, owner 6) at (6, 24) gets the sea patrol at n = 3 (anchor (21, 34), far end oil platform 1556 at (5, 19)); it leaves the patrol for order 12 at n = 112 at (10, 28), resumes it at n = 1145 (order 5, wait 3, at (10, 32)) and turns at (20, 34) at n = 1413 (order 5, still animation, wait 3) | Goal (6, 24), the tile it stood on at n = 3: at n = 1643 it steps west from (12, 28) to (10, 28), facing 6 | Trace (`xorc11-air-sea-3000`) |
| V220 | `xhuman8-25000` (oracle trace, Enhanced), n = 11284: computer troll destroyer 1441 (type 0x1F, owner 7) on patrol (order 5) at (66, 58), blocked by tanker 1446 for the 15th time, resume countdown 0 | Still animation (2), wait 1 at n = 11284; the patrol takes over at the next step done (wait 3 at n = 11285). Not animation 3, wait 15 | Trace (`xhuman8-25000`) |
| V231 | `orc10-10000` (oracle trace), n = 3505: computer tanker 1533 (type 0x1A, owner 1) in order 24 gives up blocked, nothing to resume | Animation 3 (the restarted move animation), wait 15; not the still animation | Trace (`orc10-10000`) |
| V232 | `xhuman12-10000` (oracle trace), n = 4144: axe thrower 1521 (type 0x09, owner 2) in order 4 gives up blocked, nothing resumed | Animation 3, wait 15 | Trace (`xhuman12-10000`) |
| V233 | `orc9-orc09-attack-move-1500` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-attack-move.txt`): ogre 1493 (type 0x07, owner 5) given an attack-move to (57, 91) at n = 24 (order 10 from n = 28) | Arrives at (57, 91) at n = 295 (order 2, countdown 0: the complete attack-move is forgotten) | Trace (`orc9-orc09-attack-move-1500`) |
| V234 | As V233 (`orc9-orc09-attack-move-1500`): 1493 then attacks player 0's archers from (60, 92) (order 12 from n = 363); at n = 653 its target is lost and idle becomes pending, 3 tiles from (57, 91) | Order 2, still animation, wait 3; the attack-move is **not** resumed (order 10 would be, had it been remembered) | Trace (`orc9-orc09-attack-move-1500`) |
| V235 | A unit whose remembered attack-move is complete gets a pending idle with its countdown running (static study) | Idle; the attack-move is kept, and a later pending idle at countdown 0 resumes or forgets it | Static study |
| V236 | A unit that resumed (countdown 20) is given a new attack-move command, and soon gets a pending idle more than 1 tile from its target tile (static study) | The new attack-move resumes: the command set the countdown to 0 | Static study |

### Arriving

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V095 | Enhanced (`research/oracle-round36/xhuman10-r36b-400.jsonl.gz`, oracle trace): hero 1463 (type 0x14) on (107, 98) gets command 8 on tile (113, 106) in turn 42, then a move to (111, 101) in turn 62 | Order 10 at n = 43; order 3 at n = 78; arrives at n = 113: order 2, animation 2, wait 3, and stays idle (the move command cleared the remembered attack-move) | Trace (`research/oracle-round36/xhuman10-r36b-400.jsonl.gz`) |
| V196 | `xorc11-10000` (oracle trace), n = 6046: gryphon rider 1400 (type 0x2A, owner 6) in order 4 standing at its goal, (8, 24); juggernaut 1512 (type 0x21) nearby, at (4, 32) | Arrives: order 2, animation 2, wait 3, target null (no acquisition) | Trace (`xorc11-10000`) |
| V050 | `xorc10-dragons` (oracle trace): computer destroyer 1483 (type 0x1E) at (124, 74); its patrol takes over at n = 4 (wait 3), heading for its own tile | n = 7: arrived; the new patrol takes over: tile (124, 74), animation 2, wait 3. n = 10: tile (122, 74), facing 6, animation 3. Destroyer 1484 does the same | Trace (`xorc10-dragons`) |
| V163 | `orc09-naval-1500` (oracle trace), n = 754: computer destroyer 1460 (type 0x1E) on its sea patrol arrives at (84, 66); the far end (67, 57) is under an oil platform | The new patrol goal is pulled back to (69, 58); it steps from n = 757 and at n = 853 is at (76, 60), facing 6 | Trace (`orc09-naval-1500`) |
| V164 | A patrol whose other end is usable | Goal unchanged | Example |
| V221 | `orc9-orc09-shore-build-3000` (oracle trace, Enhanced), archer 1476 (type 0x08, owner 0, range 4) attacking shipyard 1469 (type 0x49, top-left (55, 86)), whose only corner in the archer's region is the bottom-left (55, 88) | At (61, 92) the footprint is 4 away but the corner 6: it walks on, (60, 91) at n = 343, (59, 91) at n = 359 (4 from the corner), and strikes there (animation 4 at n = 375) | Trace (`orc9-orc09-shore-build-3000`) |
| V047 | Transport landing (order 30), goal 2 tiles away on both axes | The general test (radius 3) is met, so the landing logic runs; its own test (radius 0) isn't, so it steps on | Example |
| V048 | Transport landing (order 30), goal 5 tiles away | An ordinary two-tile step, move animation 3; the landing logic doesn't run | Example |
| V049 | `human12-transport` (oracle trace): transport 1522 at (68, 34), order 30 from n = 8 (animation 6, wait 3), goal (72, 29) | n = 11 (4 and 5 tiles away): two-tile step north-east to (70, 32), animation 3. n = 43 (within 3): animation 6, one-tile step to (71, 31) (the goal is odd); n = 59 (72, 30); n = 75 (72, 29) | Trace (`human12-transport`) |
| V267 | `orc9-orc09-ground-move-goals-1100` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-ground-move.txt`): catapult 1486 (type 0x05, owner 5, base range 8) at (44, 87) told to attack the ground at the water tile (50, 75) (turn n = 22, input at n = 20) | n = 22: goal (47, 81) (pulled back), order 18 pending; order 18 from n = 27. It walks via (45, 86), (46, 85), (46, 84) and arrives at (46, 83) at n = 158: 8 from (50, 75) and 2 from (47, 81) on the larger axis, so both arrival tests pass there | Trace (`orc9-orc09-ground-move-goals-1100`) |
| V268 | As V267 (`orc9-orc09-ground-move-goals-1100`): catapult 1494 (type 0x05, owner 5) at (41, 84) told to attack the ground at the island tile (14, 70) (turn n = 42, input at n = 40) | n = 42: goal (25, 76) (pulled back), order 18 from n = 44. Arrives at (27, 74) at n = 879, 2 from (25, 76) and 13 from (14, 70): the goal-radius test. At (28, 73), 3 from the goal, it walked on | Trace (`orc9-orc09-ground-move-goals-1100`) |
| V269 | As V267 (`orc9-orc09-ground-move-goals-1100`), catapult 1486, n = 157 to 162 | n = 157: order 18, move animation, wait 1, facing 0, goal (47, 81). n = 158: order 17, attack animation, wait 3, facing 0 (unchanged), goal (50, 75). n = 161: facing 1 (turned at order 17's first run), wait 1. n = 162: wait 4, and its missile (shooter 1486) appears, aimed at (50, 75) | Trace (`orc9-orc09-ground-move-goals-1100`) |

### Walking toward a target

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V114 | `xorc01-death-knight-2100` (oracle trace, Enhanced), n = 718: ogre 1488 (type 0x07, owner 5) in order 10 reaches (57, 74) at a step boundary and acquires 1516 at (55, 70) | Order 10, target 1516, tile (56, 73), facing 7, animation 3, wait 1 | Trace (`xorc01-death-knight-2100`) |
| V115 | As V114 (`xorc01-death-knight-2100`), ogre 1488, n = 730 | Pixel (1792, 2336), order 12, animation 4, wait 3, target 1516 | Trace (`xorc01-death-knight-2100`) |
| V116 | `xorc01-death-coil-2500` (oracle trace, Enhanced), n = 718: ogre 1496 (type 0x07, owner 5) in order 10, heading north (facing 0 at n = 717), acquires 1516 | Order 10, target 1516, tile (56, 73), facing 7; n = 730: order 12, animation 4, wait 3 | Trace (`xorc01-death-coil-2500`) |
| V117 | `orc02-victory-7639` (oracle trace), n = 4789: axe thrower 1591 (type 0x09, owner 0) in order 10 acquires 1583, already within strike reach | No strike: order 10, target 1583, it steps to (18, 22), facing 3. n = 4805: order 12, animation 4, wait 3 | Trace (`orc02-victory-7639`) |
| V118 | `xhuman2-10000` (oracle trace), n = 2048: ogre 1581 (type 0x07, owner 0) in order 4 acquires 1518 at a step done | Order 4, target 1518, new step: tile (3, 106), facing 3 | Trace (`xhuman2-10000`) |
| V119 | As V118 (`xhuman2-10000`), ogre 1581, n = 2051 to 2075 | n = 2051: target cleared, order 4 kept (its computer player gives the order-4 move again); n = 2060: that move takes over (animation 2, wait 3); n = 2063: order 4, target 1518, tile (4, 107); n = 2075: order 12, animation 4, wait 3 | Trace (`xhuman2-10000`) |
| V120 | `human06-31357` (oracle trace), axe thrower 1515 (type 0x09, owner 5) in order 4 | The same sequence: target 1585 and a step to (92, 13) at n = 9343; target cleared at n = 9351; animation 2, wait 3 at n = 9359; target 1585 and a step to (92, 12) at n = 9362 | Trace (`human06-31357`) |
| V153 | `human06-31357` (oracle trace): axe thrower 1512 (owner 5) in order 4 acquires building 1583 near n = 16277 at (70, 9) | The head-for aims at the building's goal zone: its next step is (71, 8) (heading for the building's tile would give (71, 9)); order 12 at n = 16295; n = 16296 matches | Trace (`human06-31357`) |
| V154 | A unit in order 4 acquiring a unit target at a step boundary | Unchanged: the head-for aims at the target's tile | Example |
| V215 | `orc5-40000` (Enhanced), n = 33260: computer footman 1485 (type 0x00, owner 0) at (43, 103) in order 4 since n = 33257, target transport 1474, stored goal (46, 106); 1474 moved to (46, 104) at n = 33258 | With the transport's widened footprint as the zone the walk finds a route: 1485 steps east to (44, 103) (animation 3, facing 2). With the 3 × 3 zone around (46, 106) there is no route and it would go idle | Trace (`orc5-40000`) |
| V216 | `orc12-30000` (Enhanced; capture not committed, re-capture with the setup `rng` 0xF12C6CAA), n = 23612: computer juggernaut 1394 (owner 1, two-tile mover) at (24, 112) picks ogre 1449 at (19, 116), G = (18, 116) | Zone: even tiles of columns 18 to 20 and rows 114 to 118; first step south-west to (22, 114). The 3 × 3 zone would give W, W, W, SW | Trace (`orc12-30000`) |
| V256 | `xhuman5-30000` (oracle trace, Enhanced, no input), n = 26761: computer grunt 1480 (type 0x01, owner 7) at (102, 108) gets command 4 on orc transport 1520 (type 0x1D, owner 7), landing (order 30) at (96, 98) (the no-route restart, `orders/transports.md`) | Order 4, still animation, wait 3, target 1520. n = 26764: it steps north to (102, 107), facing 0: its walk aims at 1520's tile (96, 98) (a break capture shows that goal), not at the goal the command stored and pulled back, toward which it would step west | Trace (`xhuman5-30000`) |

### No route

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V088 | A land unit given a move to a tile it can't reach, where the route search finds nothing (static study only) | In the update where the search fails: order 2, wait 1, and the idle logic runs at once (for a critter, its wander can start in the same update) | Static study |
| V101 | `human10-10000` (oracle trace), destroyer 1483 (type 0x1F) on patrol at (24, 8): bob draw at n = 6786, countdown 10, decremented at n = 6791 and 6796 | n = 6868: its patrol finds no route: order 2, wait 1 (immediate idle), and the idle logic runs in the same update (countdown decremented). It runs again at n = 6869 and every 5 cycles to n = 6899, where the countdown reaches 0 | Trace (`human10-10000`) |
| V102 | `human10-10000`, n = 6904: a computer patrol command for 1483 takes over (order 5, wait 3); at n = 6907 it finds no route | Order 2, wait 1, and the same-update idle logic makes a bob draw. That draw comes before every other draw of the cycle; without it, ship 1534's facing differs | Trace (`human10-10000`) |
| V103 | `xorc7-7000` (oracle trace), destroyer 1597 (type 0x1E) on patrol at (10, 16) | n = 806: no route: order 2, wait 1, and its first bob draw in the same update | Trace (`xorc7-7000`) |
| V104 | `xorc3-10000` (oracle trace), n = 8895: computer peon 1462 (type 0x03) in order 24 (return goods) finds no route | Immediate idle, and the job choice in the same update gives order 24 again on depot 1589: order 24, wait 3, target 1589 | Trace (`xorc3-10000`) |
| V170 | `xorc10-dragons-800` (oracle trace): footman 1520 (type 0x00, owner 2) has fled toward (36, 38) and is at (35, 38) at n = 464 with no route on | It keeps its attacker and flees again in the same-update idle logic: order 3, still animation, wait 3, 2 draws. n = 467: (34, 39), facing 5 | Trace (`xorc10-dragons-800`) |
| V171 | A move with no route for a unit whose attacker it can't target | Idle at once, then flees in the same update | Example |
| V176 | Order 9 with no route | Immediate idle (unchanged) | Example |
| V252 | A follower in order 6 with an attacker whose route search finds no route (static study) | Idle as an immediate order; the idle logic runs, not order 6's reaction | Static study |
| V262 | As V261 (`orc12-orc12-guard-board-full-900`), n = 502 (turn record of the input at n = 500): 1455 told to fly to (10, 108), over the sea | Order 3 from n = 503; 1450 walks after it to (18, 116) (n = 508). n = 520: 1450 finds no route: order 2, wait 1, target none, and the look-around draw in that cycle (4 draws in the cycle) | Trace (`orc12-orc12-guard-board-full-900`) |
| V263 | A peasant repairing (order 27) whose route search finds no route (traced: `orc9-orc09-repair-water-1300` n = 887, V265; "no reaction" is static study only) | Idle as an immediate order (wait 1), target forgotten; the idle logic runs in the same update; no reaction | Trace (`orc9-orc09-repair-water-1300`); no reaction: static study |
| V265 | `orc9-orc09-repair-water-1300` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-repair-water.txt`), n = 722 (turn record of the input at n = 720): peon 1478 (type 0x03, owner 5) at (38, 92) told to repair its own oil well 1468 (type 0x57) on the patch at (57, 75), out at sea | Order 27 from n = 724 (animation 4, wait 3); it walks to (48, 83) on the shore. n = 887: order 2, wait 1, target none (the immediate idle) | Trace (`orc9-orc09-repair-water-1300`) |
| V266 | As V265 (`orc9-orc09-repair-water-1300`), n = 762 (turn record of the input at n = 760): grunt 1481 (type 0x01, owner 5) at (42, 91) told to attack player 0's shipyard 1503 (type 0x48) at (84, 63), on another island | Order 9 from n = 763 (animation 4, wait 3); it walks to (53, 84) on the shore. n = 942: order 2, wait 1, target none | Trace (`orc9-orc09-repair-water-1300`) |

### Order 12 with no route and the skip mark

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V105 | `xhuman4-7000` (oracle trace), n = 5: axe thrower 1490 (type 0x09, owner 0) at (78, 62) in order 12 on footman 1518 at (72, 60), every route blocked by its own units | Order 12, animation 4, wait 3, target 1518, facing 4; one look-around draw in the cycle. It steps to (77, 61) at n = 8 | Trace (`xhuman4-7000`) |
| V106 | A unit in order 12 with no route and no other target in its search box (static study only) | Order 2, still animation, wait 1, target forgotten | Static study |
| V135 | `orc02-assault-3000`, n = 679: grunt 1573 (type 0x01, owner 0) in order 9 on 1588 finds no route | Same update: order 12 on its attacker 1586, animation 4, wait 3; 4 draws in the cycle (with the look-around draw) | Trace (`orc02-assault-3000`) |
| V175 | `human5-10000` (oracle trace), n = 1759: grunt 1531 (type 0x01, owner 5) at (43, 95) in order 12 on 1521 finds no route; town hall 1525 (type 0x4A, top-left (40, 96)) is within its attack range | Order 12 on 1525 at once: facing 5, animation 4, frame 0, wait 1. n = 1760: frame 25, wait 3. n = 1809: target 1518 | Trace (`human5-10000`) |
| V203 | `xorc8-10000` (oracle trace, Enhanced), n = 7372: elven destroyer 1468 (type 0x1E, owner 2, 1 HP) at (116, 116) in order 12 on fleeing transport 1402 (type 0x1D, owner 5) at (104, 114), out of reach, skip mark set by a no-route retarget at n = 7252; ogre 1392 (type 0x07, owner 5) on land at (121, 117) | The move step's search takes 1392 and finds no route: order 2 at once, animation 2, wait 1, target null; tile (116, 116), facing 7 | Trace (`xorc8-10000`) |
| V219 | `orc12-30000` (oracle trace, Enhanced; capture not committed), juggernaut 1439 (type 0x21, owner 3) at (24, 116): order 12 with no route at n = 28926; the fallback finds a target and marks the unit. At its next order-logic run (n = 29156) order 12 loses its target and its remembered patrol takes over in that run | The mark is cleared at the end of n = 29156. At n = 29159 the patrol picks catapult 1426 (owner 0) at (17, 124), finds no route, goes idle at once, and the idle logic (unmarked) searches: order 12 on 1426, animation 4, wait 3 | Trace (`orc12-30000`) |

### Losing the target

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V021 | Attack target dies, no pending order, order 9 | order 2 | Example |
| V022 | Attack target dies, no pending order, order 15 | order 13 | Example |
| V030 | Human footman attacking an enemy grunt (order 9); the grunt walks into fog | the grunt stays the target | Example |
| V031 | Human footman attacking an enemy mage (order 9); the mage casts Invisibility on itself | the target is lost at the next check: order 2 (or the pending order) | Example |
| V097 | `human01-basics-9500` (oracle trace), n = 288: footman 1597 kills footman 1592; footman 1598 also targets 1592 | 1597: target null at once, still order 9. 1598: keeps target 1592 until its logic runs at n = 291, then order 2, wait 3 | Trace (`human01-basics-9500`) |
| V113 | A unit in order 12 whose target died, with nothing in its search box (static study only) | Order 2, wait 3, as before | Static study |
| V129 | Order 16, target dead, nothing else in range | Order 2 takes over (lost target) | Example |
| V136 | `orc09-naval-1500`, n = 510: destroyer 1497 (type 0x1F, owner 5) in order 9 on 1506, which is its recorded attacker and dying since n = 493 (killed by 1501) | Order 2, still animation, wait 3, target null; attacker cleared | Trace (`orc09-naval-1500`) |
| V137 | As V136 (`orc09-naval-1500`), destroyer 1497, n = 513 | Order 16 on 1511, animation 4, wait 3 (not order 12) | Trace (`orc09-naval-1500`) |
| V138 | A unit in order 9 whose recorded attacker is alive and whose target is valid | Attacker kept | Example |
| V165 | `xorc01-death-knight-2100` (oracle trace): ogre 1488 (type 0x07, owner 5) in order 12 kills its target at n = 907 (target null) while cooling down | n = 915: order 12, animation 4, frame 0, wait 15, target null. n = 930: order 10 (the resumed attack-move), still animation, wait 3 | Trace (`xorc01-death-knight-2100`) |

### Follow

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V052 | Footman guarding (order 7) a peasant that starts harvesting (order 23) | the footman is given a move to its own tile and stops | Example |
| V053 | Footman following (order 6, nothing pending) a peasant that is harvesting, arrives within 1 tile | idle (order 2) | Example |
| V054 | Footman follows a footman that is moving (order 3), and arrives within 1 tile | order 7 (guard) | Example |
| V237 | `orc9-orc09-tanker-move-2200` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-tanker-move.txt`): juggernaut 1501 (type 0x21, owner 5) smart-moved onto its own destroyer 1497 (type 0x1F) at n = 20 (order 6 from n = 27); 1497 sails to (60, 62) and attacks without moving (order 16 from n = 359) | 1501 reaches (60, 64), 2 tiles away, inside the radius 1 + 1: at n = 614 order 2, still animation, wait 3; order 16 from n = 617. Not order 6, and not a stop command's wait 1 | Trace (`orc9-orc09-tanker-move-2200`) |
| V248 | `orc9-orc09-follow-attacked-1200` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-follow-attacked.txt`): grunt 1481 (type 0x01, owner 5), sent to (61, 92) at n = 22 (turn record), is attacked by player 0's archers 1475 and 1476 (type 0x08; order 12 at n = 271 and 267). At n = 302 (turn record) it is smart-moved onto its own peon 1478 (type 0x03) | Order 6 from n = 314; hit while walking (hp 55 at n = 323, 47 at n = 326) and no reaction while it walks | Trace (`orc9-orc09-follow-attacked-1200`) |
| V249 | As V248 (`orc9-orc09-follow-attacked-1200`), n = 493: 1481 at (49, 92), next to the peon at (48, 91) (order 23, sent to gold mine 1480 at n = 342), runs its logic; archer 1476 at (54, 90) | Order 6, target **1476**, still animation, wait 3 (the follow toward the unit the reaction's search picked), not order 12 | Trace (`orc9-orc09-follow-attacked-1200`) |
| V250 | As V248 (`orc9-orc09-follow-attacked-1200`), n = 560: 1481 at (52, 90) reaches 1476 at (53, 90), whose order is 12 (not followable) | Order 2, no target, still animation, wait 3 (pending idle); n = 563: its idle logic attacks 1476 (order 12, attack animation, wait 3) | Trace (`orc9-orc09-follow-attacked-1200`) |
| V251 | A follower in order 6 whose reaction's search picks a unit that is idle, and that is already within the radius (static study) | Order 7 (guard) on the picked unit | Static study |
| V257 | `orc9-orc09-follow-flee-1300` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-follow-flee.txt`), n = 472 (turn record of the input at n = 470): peon 1478 (type 0x03, owner 5) smart-moved onto its own grunt 1481 (type 0x01), idle at (50, 90) | Order 6 from n = 474, target 1481 | Trace (`orc9-orc09-follow-flee-1300`) |
| V258 | As V257 (`orc9-orc09-follow-flee-1300`), n = 482 and 483: player 0's archers 1475 and 1476 (type 0x08) hit 1478 (hp 30 to 23 to 17) | It keeps order 6 and target 1481 (no flee) | Trace (`orc9-orc09-follow-flee-1300`) |
| V259 | As V257 (`orc9-orc09-follow-flee-1300`), n = 605: 1478 at (51, 89), next to 1481 (order 2) | Order 7, still animation, **wait 3**, target 1481 (the guard takes over in the same update) | Trace (`orc9-orc09-follow-flee-1300`) |
| V260 | As V257 (`orc9-orc09-follow-flee-1300`), peon 1478, n = 608 | Order 3, still animation, wait 3, target none (the flee from order 7's idle logic); it is at (50, 89) from n = 611 | Trace (`orc9-orc09-follow-flee-1300`) |
| V261 | `orc12-orc12-guard-board-full-900` (oracle trace, Enhanced; scenario `oracle/scenarios/orc12-guard-board-full.txt`), n = 202 (turn record of the input at n = 200): ogre 1450 (type 0x07, owner 0) smart-moved onto goblin zeppelin 1455 (type 0x29), idle at (18, 116) | Order 6 from n = 205; n = 220, at (19, 117): order 7, still animation, wait 3, target 1455 | Trace (`orc12-orc12-guard-board-full-900`) |

### Order 11

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V179 | `orc02-victory-7639` and `orc02-defeat-4021` (oracle traces), n = 2720: grunt 1576 (type 0x01, owner 0), in order 11 since the n = 2600 command on wall tile (55, 15) | Strikes from (56, 17) at the pulled-back goal (55, 16): facing 7, animation 4, wait 1 | Trace (`orc02-victory-7639`, `orc02-defeat-4021`) |
| V180 | `orc02-defeat-4021`, n = 2752: grunt 1575 at (55, 18) in order 11 finds no route | Order 2 at once, wait 1; its look-around draw is made in that update | Trace (`orc02-defeat-4021`) |
| V181 | `orc02-defeat-4021`, n = 2967: grunt 1568 at (56, 15) in order 11; the wall at (55, 15) is gone | Order 2 as a pending order: still animation, wait 3 | Trace (`orc02-defeat-4021`) |
| V182 | Command 8 on a tile that isn't a wall, next to a wall | Not order 11 (order 10, or order 2 for command 9), even if the pulled-back goal is a wall tile | Example |
| V183 | Blocked row A gives order 11 on a wall tile | Goal kept as the blocking tile (no pull-back) | Example |

### Attacking without moving, towers and friendly fire

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V035 | Human footman standing ground (order 13) hit by an enemy archer 5 tiles away (out of range); the archer's strike revealed its surroundings to the footman's owner (`vision/fog.md`, strike reveal) | the archer is outside the footman's box (radius 1), but the search starts with it as the best candidate (base 65536), so the result is the archer: order 15. At the next logic run the archer is valid and detected but out of range, so the footman forgets it and order 13 takes over in the same update: still animation, wait 3, target null ("Attacking without moving") | Example |
| V036 | Human footman idle, an enemy grunt 1 tile away; the footman gets order 16 on it; the grunt then steps 2 tiles away | Next run: out of range, so order 2 takes over and the grunt is forgotten; the footman's idle search may then pick a new target | Example |
| V037 | Human guard tower (range 6), attacked by an enemy catapult 8 tiles away; an enemy footman 4 tiles away | The tower ignores its attacker (no attacker seed, box of 6): it fires at the footman | Example |
| V038 | Human guard tower, its target walks from 6 to 7 tiles away | Next run the search doesn't find it; with nothing else in range the tower does nothing | Example |
| V039 | Human catapult at (14, 20) on order 16 against a grunt at (20, 20); one of the catapult player's own footmen stands at (21, 19) | No strike; order 2 takes over | Example |
| V040 | Computer catapult, same, two own footmen in the 3 × 3 | Strikes; a third own unit there would stop it | Example |
| V041 | Passive computer player's catapult, same, one own footman in the 3 × 3 | Strikes (not human-controlled: it takes three) | Example |
| V042 | Human catapult, same target, own 2 × 2 farm whose footprint covers (19, 21) and (20, 21) only inside the 3 × 3 | Counts 2 (one per tile); 1 is enough for a human player, so no strike | Example |
| V043 | Human destroyer, same situation as the first catapult vector | Strikes: destroyers can't attack the ground, so they skip the check | Example |
| V093 | `human12`, slot 1480 (type 0x63), no enemy in range | n = 1: order 14, animation 4, wait 3; n = 4: logic runs, finds nothing; n = 5: animation 4, wait 150; next logic run at n = 155 (wait 150 again at n = 156) | Trace (`human12`) |
| V094 | `xorc3`, slot 1506 (type 0x61), no enemy in range | n = 4: logic runs, finds nothing; n = 5: animation 4, wait 59; next run at n = 64 | Trace (`xorc3`) |
| V096 | `xhuman10-spells-700` (oracle trace), player 1's guard tower 1468 (type 0x60) and cannon tower 1501 (type 0x62), grunts 1477 on (78, 91) and 1495 on (79, 88) in range | n = 2: order 14, animation 4, wait 3. n = 5: new effect slot 3, kind 0x0F, shooter 1468, target 1477, damage 7; slot 4, kind 0x18, shooter 1501, target 1495, damage 50. Both towers: target null, facing 0. n = 6: 1468 wait 59, 1501 wait 150, both animation 4 | Trace (`xhuman10-spells-700`) |
| V107 | `xhuman4-7000`, n = 6: axe thrower 1437 (type 0x09, owner 6) at (75, 113) in order 16 on flying machine 1444 (type 0x28) at (78, 108): distance 5, range 4 | Order 2, animation 2, wait 3, target null; n = 9: order 16, animation 4, wait 3, target 1444 | Trace (`xhuman4-7000`) |
| V108 | `xhuman2-10000` (oracle trace), n = 34: footman 1548 (owner 1) at (60, 68) in order 16 on 1547, hit by catapult 1555 at (56, 63), out of range | n = 34: order 2, animation 2, wait 3, target null; n = 37: order 12, animation 4, wait 3, target 1555 | Trace (`xhuman2-10000`) |
| V121 | `xhuman2-10000`, n = 2223 to 2232: ballista 1503 (type 0x04, owner 1) idle at (8, 119) acquires ogre 1583, 8 tiles away (within range 8), which is fighting 1503's own footman 1517 | n = 2223: order 16, animation 4, wait 3, target 1583. n = 2226: order 2, animation 2, wait 3, target null, facing 3. It acquires again at n = 2229 and drops it again at n = 2232 | Trace (`xhuman2-10000`) |
| V122 | `human06-31357`, n = 9483: ballista 1582 (type 0x04) at (84, 9) in order 16 on 1515, which is fighting 1582's own unit 1585 | Order 2, animation 2, wait 3, target null | Trace (`human06-31357`) |
| V123 | `human01-order15-range-700` (oracle trace; Human01 with input, 700 cycles; commands: n = 10 move footman 1593 to (23, 8), n = 12 footman 1598 attacks grunt 1592, n = 240 stand ground for 1593, n = 250 move 1598 to (12, 4)). 1593 stands at (21, 8) in order 13 and takes order 15 on 1592 at n = 391; 1592 walks off at n = 406 | Order 15, animation 4 to n = 418 (the attack cycle ends with its cooldown frames); n = 419: order 13, animation 2, wait 3, target null | Trace (`human01-order15-range-700`) |
| V139 | `xhuman4-7000`, n = 210: ballista 1488 (type 0x04, owner 1) in order 16 on dying 1505, cooldown over; the search switches to 1515, whose 3 × 3 area holds units of player 1 | Order 2, still animation, wait 3, target null | Trace (`xhuman4-7000`) |
| V140 | As V139 (`xhuman4-7000`), ballista 1488, n = 213 and 216 | n = 213: order 16 on 1515, animation 4, wait 3. n = 216: order 2, wait 3 | Trace (`xhuman4-7000`) |
| V141 | `xhuman10-spells-700`, n = 227: ballista 1483 (owner 1) in order 16; its search's new target 1475 stands next to unit 1489 of player 1 | Order 2, wait 3, target null | Trace (`xhuman10-spells-700`) |
| V142 | A ballista in order 16 still cooling down whose search switches to such a target (static study only) | Keeps the new target and waits out the cooldown; no check in that run | Static study |
| V270 | `orc9-orc09-hold-resume-hits-goals-1600` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-hold-resume.txt`): catapult 1494 (type 0x05, owner 5, the human player) on an attack-move to (8, 87) (order 10 from n = 24); its walking search picks footman 1485 (owner 0), which fights grunts of player 5 near (5, 86) | n = 957: order 12, attack animation, wait 3, target 1485, goal (5, 86). n = 960: fire held: order 10, still animation, wait 3, target none, goal (8, 87) (the attack-move given again). n = 963: order 10, move animation, wait 1, a step to (12, 89); the walking search pends 12 on 1485 again. The same repeats at n = 995/998, 1033/1036 and 1071/1074 | Trace (`orc9-orc09-hold-resume-hits-goals-1600`) |
| V271 | As V270 (`orc9-orc09-hold-resume-hits-goals-1600`), catapult 1494 at (9, 87), 1 tile from (8, 87) | n = 1109: order 12, wait 3, target 1485. n = 1112: fire held: order 2, still animation, wait 3, target none, goal (5, 86) (the attack-move is complete, so it is forgotten). From n = 1115 its idle logic picks 1485 (order 16, wait 3) and holds again at n = 1118: orders 16 and 2 alternate every 3 cycles | Trace (`orc9-orc09-hold-resume-hits-goals-1600`) |

### Boarding moves

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V044 | Human footman in order 34, transport 5 tiles away on the shore, not landing | Gets a smart move again: stays in order 34; the transport is told to come | Example |
| V045 | Computer grunt in order 34, same | Gets command 4 on the transport: order 4 at the next step boundary; it is no longer boarding | Example |
| V046 | Passive computer player's grunt in order 34, same | Gets a smart move, as the human footman: stays in order 34 | Example |
| V253 | `orc12-orc12-board-attacked-1000` (oracle trace, Enhanced; scenario `oracle/scenarios/orc12-board-attacked.txt`): ogre 1439 (type 0x07, owner 0), fighting player 3's ogre 1442 (type 0x07), is smart-moved onto its own docked transport 1434 (type 0x1D) at n = 602 (turn record) and hit at n = 608 | Order 34 from n = 622; it walks to the transport and is aboard at n = 661 (order 36); it never turns on 1442 (`orders/transports.md`, round-64 vectors) | Trace (`orc12-orc12-board-attacked-1000`) |

### Acquisition: candidates, score and choice

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V001 | Human footman idle, an enemy grunt (priority p) 1 tile away, footman attack range 1 | score 131072 + p − 0; order 16 | Example |
| V002 | As V001, but the footman belongs to a computer player | order 12 (the grunt can attack) | Example |
| V003 | Computer footman idle, only target is an enemy farm 1 tile away | order 12 (building) | Example |
| V004 | Computer footman idle, only target an enemy peasant 1 tile away | order 16 | Example |
| V005 | Two equal candidates (same type, same distance), A above B on screen | A is chosen | Example |
| V006 | Candidate tied with the current attacker | the attacker is kept | Example |
| V007 | Archer (range 4) sees an enemy footman at d = 6 and an enemy farm at d = 2 | footman: 65536 + p − 9; farm: 0 + p_farm − 1, at least 1; the footman wins | Example |
| V008 | The archer of V007, a gryphon rider in range at d = 4 and a footman in range at d = 2 | gryphon base 196608 beats footman base 131072, whatever the priorities | Example |
| V009 | Score of a non-attacking target with priority 5 at d = 6 | 5 − 9 = −4, raised to 1 | Example |
| V010 | Building of width 4 at column 10; searcher column 8, 11, 13, 14 | reference column 10, 12, 12, 13 | Example |
| V032 | Human footman idle; an enemy grunt 1 tile away but in fog for the footman's owner | not a candidate (hidden); no attack | Example |
| V130 | `human06-31357`, n = 13557: idle peasant 1587 (type 0x02, owner 1, a human player) at (79, 5); enemy grunt 1515 adjacent at (78, 4); no attacker | Order 2, no target | Trace (`human06-31357`) |
| V131 | `human10-24000` (oracle trace), n = 18741: idle peasant 1558 (type 0x02, owner 6, a human player) at (108, 26); grunt 1473 at (108, 27) targeting it, not yet struck | Order 2, no target | Trace (`human10-24000`) |
| V157 | `human06-31357`, n = 23683: ballista 1582 (type 0x04, owner 1) at (84, 9) in order 16 on grunt 1515, its first attacker, adjacent (score 0, inside the minimum range); axe thrower 1516 in its box | The attacker doesn't seed the search: order 12 on 1516, animation 4, facing 7, wait 3 (order 12, not 16, because the unit still has an attacker) | Trace (`human06-31357`) |
| V158 | An attacker with score s > 0 and a box candidate with the same score | The attacker is kept (tie) | Example |
| V187 | `xorc11-10000` (oracle trace, Enhanced), n = 38: computer destroyer 1542 (type 0x1E, owner 6) on patrol at (10, 24); dragon 1526 (type 0x2B, owner 5) in its box | Order 5, target 1526, wait 15 | Trace (`xorc11-10000`) |
| V197 | `xorc6-10000` (oracle trace), n = 6106: axe thrower 1475 (type 0x09, owner 5) idle at (82, 90); its recorded attacker died earlier and was forgotten by an earlier search | Order 16 (not 12) on footman 1459, animation 4, wait 3 | Trace (`xorc6-10000`) |
| V198 | `human06-31357`, n = 9642: footman 1584 (type 0x00, owner 1, the human player) idle at (72, 7); its recorded attacker, axe thrower 1516 (type 0x09, owner 5) at (68, 11), stands in a tile its own sight doesn't cover, but 1516's strike revealed it after the last fog refresh | 1516 seeds the search: order 12 on 1516, animation 4, wait 3 | Trace (`human06-31357`) |
| V199 | `xhuman11-10000` (oracle trace), n = 9065: paladin 1513 (type 0x0C, owner 1, the human player) at (84, 67), in order 12 on 1473, which it loses, and idle grunt 1518 (owner 1) at (85, 67); their recorded attacker, catapult 1467 (type 0x05, owner 2) at (84, 59), stands in fog | The seed is rejected (hidden): 1513 goes idle (order 2, animation 2, wait 3, target null) and 1518 stays idle; no chase | Trace (`xhuman11-10000`) |
| V200 | The seed rule of V198 and V199, an attacker that isn't detected (an invisible unit; static study only) | Not a seed | Static study |
| V214 | `orc5-40000` (Enhanced), n = 21489: computer catapult 1580 (type 0x05, owner 5) at (122, 7), idle; transport 1516 (type 0x1C, owner 0) landing at (120, 6), d = 2 | Picked: order 16, animation 4, wait 3, target 1516 (a minimum of 3 would have skipped it) | Trace (`orc5-40000`) |
| V217 | `orc12-30000` (as V216; re-captured identically in round 60), n = 26982: computer ogre mage 1442 (owner 3) at (36, 119), idle; transport 1434 (owner 0) landing on the shore tile (29, 121) | Picked: order 16, animation 4, wait 3, target 1434 | Trace (`orc12-30000`) |
| V218 | A land-only searcher; candidate a transport in the sea layer on a shore-water tile (static study) | Not skipped for its kind | Static study |

### Acquisition: resulting order and target switches

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V089 | `xhuman4`, slot 1490 (type 0x09), idle, finds 1518 at its first step done | n = 2: order 12, animation 4, wait 3, target 1518; n = 3: animation 4, wait 2 | Trace (`xhuman4`) |
| V090 | `xhuman4`, slot 1505 (type 0x01), the same | n = 2: order 12, animation 4, wait 3; n = 5: first chase step, animation 3 | Trace (`xhuman4`) |
| V091 | `human5`, slot 1531 (owner 5, computer), idle, finds 1529 | n = 6: order 12, animation 4, wait 3, target 1529; it strikes from where it stands (still animation 4 at n = 30) | Trace (`human5`) |
| V092 | `xhuman4`, slot 1435 (type 0x09), enemy 1444 in reach from the start | n = 1: order 2, animation 2, wait 1 (no acquisition in the first start-up cycle); n = 2: wait 4; n = 6: order 16, animation 4, wait 3, target 1444 | Trace (`xhuman4`) |
| V098 | `orc02-assault`, `orc02-defeat` and `orc02-victory` (oracle traces), n = 570: footman 1582 (owner 1) in order 12 at (52, 26), target 1564, acquires 1572 at (54, 23), out of reach | Order 12, target 1572; it keeps walking: tile (53, 25), animation 3, wait 1, frame 0 | Trace (`orc02-assault`, `orc02-defeat`, `orc02-victory`) |
| V099 | A unit in order 12 acquires a new target already in reach, its cooldown run out (static study only) | The strike decision in the same update: it turns to the target and strikes as usual | Static study |
| V100 | `orc02-assault-3000` (oracle trace; the same in `orc02-defeat` and `orc02-victory`), n = 617: footman 1587 (type 0x00) in order 12 on 1568, still cooling down, switches to 1573, in reach | Order 12, target 1573, animation 4, wait 23 (the cooldown left) in the same record; no wait of 3 | Trace (`orc02-assault-3000`) |
| V109 | `xhuman4-7000`, n = 66: footman 1497 (owner 1) at (71, 61) in order 12 on 1490, standing in its attack animation (animation 4, wait 1 at n = 65), acquires 1505 at (73, 60), out of reach | No step: n = 66: tile and pixel unchanged, animation 3, target 1505; n = 67: animation 4, wait 3, target 1505; first step at n = 70 | Trace (`xhuman4-7000`) |
| V110 | `orc02-assault` (oracle trace), n = 586: footman 1582 (switched to 1572 at n = 570) reaches its step done at (53, 25) | Order 12, animation 4, wait 3, target 1572; n = 589: it steps to (54, 24) | Trace (`orc02-assault`) |
| V111 | `xhuman2-10000`, n = 206: catapult 1556 (owner 5) at (67, 60), order 12 with no target, acquires guard tower 1546 (type 0x60) at (56, 69) | Target 1546, animation 4, wait 3 at once, facing unchanged (5) | Trace (`xhuman2-10000`) |
| V112 | `xhuman2-10000`, n = 205: catapult 1555 at (56, 63) in order 12; its target 1548 has just died; guard tower 1546 in range | Order 12, target 1546, facing 4, wait 3; it fires (wait 198 at n = 208) | Trace (`xhuman2-10000`) |
| V124 | `xorc01-death-knight-2100`, n = 845: ogre 1490 (type 0x07, owner 5) in order 12 on skeleton 1516 (type 0x37, dying since n = 840, killed by another unit), switches to skeleton 1515, out of reach | Target 1515, order 12, animation 3: it steps on to (54, 72), facing 7 (the command case) | Trace (`xorc01-death-knight-2100`) |
| V125 | A unit in order 12 kills its target, and its search picks a new one out of reach (static study only) | Take-over at once (attack animation, wait 3) | Static study |
| V126 | `orc02-assault-3000`, n = 800: footman 1582 (owner 1) at (57, 21), next to dying grunt 1573, switches to grunt 1575, out of reach | Order 12, target 1575, animation 4, wait 3 at n = 800; first step at n = 803, to (57, 20) | Trace (`orc02-assault-3000`) |
| V127 | `xhuman10-spells-700` (oracle trace), knight 1493 (type 0x06) at (84, 88): its switch to 1495 at n = 51 is a command (route found, wait 15 blocked); the new order takes over at n = 66 (animation 4, wait 3) | The cached route is kept: n = 69 animation 3 on (84, 88); n = 70 the step south to (84, 89), facing 4 | Trace (`xhuman10-spells-700`) |
| V128 | `xhuman4-7000`, n = 160 to 172: archer 1512 (type 0x08, owner 1) in order 16 on grunt 1505, dying since n = 119 | Target 1505 kept to n = 171; n = 172: target 1515, order 16, wait 3 | Trace (`xhuman4-7000`) |
| V132 | `xhuman10-spells-700`, n = 89: grunt 1497 (owner 2) had been blocked twice before its acquisition; grunt 1490 (owner 2) at (78, 87) is blocked by it | 1497's counter was cleared by the acquisition, so row F applies to it as a moving ally: 1490 steps to (79, 87), animation 3 | Trace (`xhuman10-spells-700`) |
| V133 | An idle unit with blocked counter 2 acquires a target | Counter 0, route dropped, order 12 or 16 takes over with wait 3 | Example |
| V134 | `orc02-assault-3000`, n = 984: grunt 1572 (owner 0) in order 16 on footman 1582, which is dying; its recorded attacker is dying, killed by another unit | Order 12 on 1587 (not 16), animation 4, wait 3 | Trace (`orc02-assault-3000`) |
| V186 | `human5-10000`, n = 2754: axe thrower 1533 (type 0x09, owner 5) in order 12 on peasant 1513, which is inside a gold mine; the new target, town hall 1525 (type 0x4A), is out of strike reach | Command switch: target 1525, it steps to (36, 99), animation 3, facing 1, wait 1 | Trace (`human5-10000`) |
| V193 | `xhuman12-10000` (oracle trace): grunt 1476 (type 0x01, owner 2) in order 12 on farm 1455 decides a strike at n = 1844; at n = 1847, still cooling down, its search picks castle 1443 (type 0x5A), out of reach | n = 1847: order 12, target 1443, animation 4, wait 23. The strike lands: 1455 goes from 400 to 394 HP at n = 1869; 1476 steps at n = 1870 | Trace (`xhuman12-10000`) |
| V194 | As V193 (`xhuman12-10000`), n = 1841: the switch of V193 while not cooling down | The command case: wait 3 | Trace (`xhuman12-10000`) |
| V195 | `xhuman12-10000`, n = 5142: archer 1372 (type 0x08, owner 1) in order 16 on 1363, cooling down, picks 1480 | Order 16, target 1480, animation 4, wait 63 | Trace (`xhuman12-10000`) |
| V255 | `xorc5-30000` (oracle capture, Enhanced, **not committed**; reproducible with `oracle/capture.sh xorc5 30000`), n = 20526: ogre 1450 (type 0x07, owner 5, the human player) at (6, 89) in order 16, attacked by footmen 1474 at (6, 88) and 1403 at (7, 88) (type 0x00, owner 1); its target changed from 1474 to 1403 at n = 20523, and it was hit at n = 20525 (hp 82) | Traced (also shown by `orc9-orc09-order16-keep-2000`, V264). Its search, with that attacker, picks 1474: order 16, target 1474, wait 23 (the cooldown left), not order 12 with wait 3. It goes on striking in order 16 (n = 20550: frame 25, wait 3); order 12 never appears | Trace (`xorc5-30000`, not committed; also `orc9-orc09-order16-keep-2000`) |
| V264 | `orc9-orc09-order16-keep-2000` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-order16-keep.txt`), n = 936: grunt 1492 (type 0x01, owner 5, the human player) at (6, 87) in order 16 on footman 1485 (type 0x00, owner 0) since n = 934, wait 1, still cooling down; footman 1484 (type 0x00, owner 0) at (5, 88), in order 12 on 1492, strikes it (hp 60 to 52) | n = 937: order 16, target 1484, wait 23 (the cooldown left), animation 4, frame 0, facing 7; not order 12 with wait 3. It goes on striking in order 16 (n = 967: frame 35, wait 3) and kills 1484 (order 1 at n = 1070); from n = 937 to 2000 it is only ever in order 16 or 2. With this rule off the comparison fails at n = 937 | Trace (`orc9-orc09-order16-keep-2000`) |

### Mages

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V143 | `xhuman2-10000`, n = 7: death knight 1557 (type 0x0B, owner 5) idle at (64, 59); footman 1548 outscores barracks 1554 in its box | Order 2, no target (the footman can attack, so nothing is acquired) | Trace (`xhuman2-10000`) |
| V144 | A mage whose best candidate is a peasant (static study only) | It attacks the peasant (order 12) | Static study |
| V172 | `xorc01-death-coil-2500` (oracle trace): death knight 1491 (type 0x0B, owner 5, the human player) idle at (72, 69), hit by a skeleton at n = 1431 | n = 1433: order 3 (flee), still animation, wait 3, 2 draws for the offset. n = 1436: (71, 69), facing 6 | Trace (`xorc01-death-coil-2500`) |
| V173 | An idle mage hit by a footman, nothing else in its box | Flees | Example |
| V174 | An idle mage hit by a footman, with an enemy peasant in its box that outscores the footman | Attacks the peasant | Example |

### Vertical order and the halving search

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V051 | Unit entries with pixel y 100, 200, 50 in list order (none dying or inside) | after one sort: 100, 50, 200 (the third entry can't pass the first in one pass); after the next cycle's sort: 50, 100, 200 | Example |
| V149 | `xorc01-death-knight-2100`, n = 708: skeleton 1515 (type 0x37, owner 6) idle at (56, 69); grunt 1497 has just stepped to row 73 and sits among row-74 entries in the list | The halving search includes it: order 12, animation 4, wait 3, target 1497 (an exact partition would stop before it) | Trace (`xorc01-death-knight-2100`) |
| V150 | As V149 (`xorc01-death-knight-2100`), n = 708: 112 entries (positions 0 to 111), top row 62, bottom row 73; first entry found 69. The rows at the middles are those of the list order in the original's capture (the trace records positions, not the list order) | Last-entry search from low 69, high 111: middles 90 (row 72), 100 (row 75), 95 (row 73), 97 (row 74), 96 (row 74; the entry before is row 73): stop, last entry 95, which includes grunt 1497. Starting low at 0 would end at 92 and miss it | Trace (`xorc01-death-knight-2100`) |
| V151 | Rows 1, 2, 5, 5, 6 (positions 0 to 4), top row 3, bottom row 5 | First entry: middles 2, 0, 1; stop at 1 (row 2 above 3, next row 5 not): first entry 2. Last entry: low 2, high 4; middles 3, 3, 4; stop at 4 (row 6 below 5, the one before not): last entry 3 | Example |
| V159 | `human7-22000`, n = 19787: archer 1598 idle at (11, 0); 138 units in the list, among them four orc tankers (type 0x1B) inside oil platforms (order 26, pixel y 2464 and 2592); zeppelin 1575 at recorded tile (10, 4) | The halving search's range is list positions 0 to 19 and leaves out 1575 at position 23: order 2 (not acquired). Without the inside units the range would be 0 to 23 | Trace (`human7-22000`) |
| V160 | As V159 (`human7-22000`), archer 1598, n = 19797 | The range is 0 to 20 and includes 1575: order 16 on 1575 | Trace (`human7-22000`) |
| V161 | `xhuman10-spells-700`, n = 286: ogre 1543 (type 0x07) in order 12 | Target 1529 | Trace (`xhuman10-spells-700`) |
| V162 | A unit that comes out of a mine at pixel y 320, having gone in at pixel y 160 | Its list place is the one for y 160 until the sort moves it | Example |

### Reacting to an attacker and fleeing

| ID | Input | Expected | Source |
| --- | --- | --- | --- |
| V024 | Human peasant (order 2) hit by an archer facing east (facing 2), draws r = 5 then r = 2 | flee tile = peasant tile + (5 − 2 + 4, 2 − 2 + 0) = (+7, 0) | Example |
| V025 | Human gold worker walking to its mine (order 23, the mine as target unit) is hit by an enemy archer | does not flee | Example |
| V026 | Human lumber worker walking to its tree (order 23, no target unit) is hit by an enemy archer | doesn't flee while walking; flees (2 draws) at its first logic run after it reaches the tree (or its walk ends with no route) | Example |
| V027 | Human lumber worker chopping (order 23, no target unit) is hit by an enemy archer | flees at its next chop-loop run | Example |
| V028 | Human lumber worker whose tree search found nothing (now order 2) is hit | flees (order-2 rule) | Example |
| V029 | Computer player's lumber worker chopping is hit | does not flee (only idle units of computer players flee) | Example |
| V033 | Peasant hit by an enemy peon | attacks the peon back | Example |
| V034 | Human footman, idle (order 2), hit by an enemy grunt 1 tile away; no other enemy near | the search starts with the grunt as best; result: the grunt, order **12** (chase), not 16 | Example |
| V145 | `xorc10-dragons-800` (oracle trace), n = 333: computer footman 1520 (owner 2) idle at (30, 30), hit by the breath splash, flees | Order 3, animation 2, wait 3; n = 336: tile (31, 31), facing 3, animation 3 | Trace (`xorc10-dragons-800`) |
| V146 | `human12-7000` (oracle trace), n = 5668: zeppelin 1503 (type 0x29, owner 0) idle at (16, 108), flees | Order 3, animation 2, wait 3; n = 5671: tile (18, 106), facing 1 | Trace (`human12-7000`) |
| V147 | `xhuman10-blizzard-600` (oracle trace), n = 214: grunt 1490 (type 0x01) in order 12, attack animation, flees | Order 3, animation 2 (still), wait 3 | Trace (`xhuman10-blizzard-600`) |
| V148 | `xorc01-death-knight-2100`, n = 1569: skeleton 1521 (type 0x37), walking in order 12 (wait 1 at n = 1568), flees | Order 3, animation 2 (still), wait 3 | Trace (`xorc01-death-knight-2100`) |
| V155 | `human06-31357`, n = 16816: peasant 1587 (owner 1) in order 3 after fleeing at n = 16797 | Tile (81, 4), facing 2, animation 3, wait 1 (the flee tile pulled back as a command goal) | Trace (`human06-31357`) |
| V156 | A flee tile that is usable | Kept as it is | Example |
| V201 | `xorc8-10000` (oracle trace), n = 6177: transport 1423 (type 0x1D, owner 5), idle at (118, 110), flees to a shore tile | Order 30, animation 6, wait 3 | Trace (`xorc8-10000`) |
| V202 | A peasant flees to a tile next to a forest | Order 3 (move); no harvest | Example |
| V208 | `human12-25000` (oracle trace, Enhanced), n = 16061: transport 1428 (type 0x1C, owner 1, the human player) idle at (36, 114), hit at n = 16059 by giant turtle 1530 (type 0x27, owner 0, a computer player), which owner 1 doesn't detect | Two flee draws; order 30, animation 6, wait 3, target null; the attacker is forgotten | Trace (`human12-25000`) |
| V209 | `human12-25000`, n = 16612: tanker 1425 (type 0x1A, owner 1) idle at (32, 116), hit by giant turtle 1530 | Two flee draws; order 3, wait 3, target null | Trace (`human12-25000`) |
| V210 | A computer footman (land attacks only) idle, hit by a submarine it doesn't detect (static study) | Flees (two draws): order 3 | Static study |
| V211 | A footman hit by a footman it doesn't detect and could hit (static study) | No flee; the search ignores the attacker in that run ("Terms") | Static study |
| V212 | A computer player's tanker in order 23 on its way to a platform, hit by a destroyer (static study) | No flee (a computer player's unit flees only from idle), and its AI routine doesn't run either | Static study |
| V213 | A computer player's peon idle, hit by a grunt (static study) | It can hit land, so no flee from step 1: its worker AI routine runs | Static study |
| V222 | As V221 (`orc9-orc09-shore-build-3000`), n = 599: peon 1478 (type 0x03, owner 5), carrying nothing and never sent to harvest (a build command and two flees only), flees with a smart move to (51, 92), a tile of gold mine 1480 (top-left (50, 91)) | Order 23 on 1480, animation 4, wait 3 (not order 3) | Trace (`orc9-orc09-shore-build-3000`) |
| V243 | `orc9-orc09-flee-chop-walk-900` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-flee-chop-walk.txt`), n = 472: peon 1478 (owner 5) told to chop at (49, 93), walking west past player 0's archers 1475 and 1476 (type 0x08) | Hit at n = 483 (hp 21) and 487 (hp 12); it stays in order 23 and keeps walking (no flee at n = 493). At n = 573, at (53, 91), its walk ends and it flees: order 3 (that the walk ended with no route is static study) | Trace (`orc9-orc09-flee-chop-walk-900`); the no-route walk end: static study |
| V244 | `orc9-orc09-flee-walk-1200` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-flee-walk.txt`), n = 502: peon 1478 told to chop at (61, 94) | Hit during its last step (n = 583 and 586) and keeps order 23; at n = 590 it reaches (60, 93) and flees at once: order 3, still animation, wait 3 in the same record | Trace (`orc9-orc09-flee-walk-1200`) |
| V247 | A human player's peasant repairing (order 27) is hit by an archer while walking to the building (static study) | No reaction while walking; it flees at its first logic run after it arrives | Static study |
| V254 | `human4-15000` (oracle trace, Enhanced), n = 638: computer peon 1531 (type 0x03, owner 0), no attacker, reaches its tree at (75, 79) | The arrival switch with no attacker: animation 3 to 4, frame 0, wait 1 in that record (that chopping doesn't start its animation a second time is static study) | Trace (`human4-15000`); no second animation start: static study |

### Idle look-around (oracle traces)

These come from traces of the running game (`oracle/trace-format.md`): record n = 1 and 2 are the start-up cycles,
n = 3 is the first normal cycle, and "setup" is the state after the map has loaded. Clock 946684800 in both.

**Human01, the first draws.** Setup state 0xF55E08A8. Eight units are idle: grunts (type 0x01) in slots 1588, 1590,
1591 and 1592, footmen (0x00) in 1593, 1597 and 1598, and a peasant (0x02) in 1596. The map's buildings and gold
mines draw nothing.

| ID | n | Slot (type) | r | `r AND 255` | Facing before → after | State after record n | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V055 | 1 | (no draw) | | | | 0xF55E08A8 | Trace (Human01) |
| V056 | 2 | 1598 (0x00) | 9770 | 42 | 5 → 5 | 0x262AFAC9 | Trace (Human01) |
| V057 | 3 | 1593 (0x00) | 29433 | 249 | 2 → 2 | 0xF2F9299E | Trace (Human01) |
| V058 | 4 | 1590 (0x01) | 30671 | 207 | 2 → 2 |  | Trace (Human01) |
| V059 | 4 | 1596 (0x02) | 27742 | 94 | 5 → 5 | 0xEC5EDCE4 | Trace (Human01) |
| V060 | 5 | 1597 (0x00) | 25369 | 25 | 6 → 6 | 0x63193335 | Trace (Human01) |
| V061 | 6 | 1592 (0x01) | 26483 | 115 | 7 → 7 | 0x6773BFFA | Trace (Human01) |
| V062 | 7 | 1588 (0x01) | 10072 | 88 | 7 → 7 |  | Trace (Human01) |
| V063 | 7 | 1591 (0x01) | 2174 | 126 | 0 → 0 |  | Trace (Human01) |
| V064 | 7 | 1598 (0x00) | 4396 | 44 | 5 → 5 | 0x912C27E1 | Trace (Human01) |
| V065 | 8 | 1593 (0x00) | 26724 | 100 | 2 → 2 | 0x6864CF96 | Trace (Human01) |

From n = 3 on, 1, 2, 1, 1 and 3 draws per cycle repeat every 5 cycles; each unit draws in every 5th cycle (slot
1598 at n = 2, 7, 12, …). That makes 3197 draws over n = 1 to 2000, all of them look-arounds: 47 turn clockwise, 17
counter-clockwise and the other 3133 change nothing.

**Human01, the first turns.**

| ID | n | Slot (type) | r | `r AND 255` | Facing before → after | State after record n | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V066 | 50 | 1597 (0x00) | 7682 | 2 | 6 → 7 | 0x1E02158D | Trace (Human01) |
| V067 | 64 | 1590 (0x01) | 13824 | 0 | 2 → 1 | 0x5C886184 | Trace (Human01) |
| V068 | 73 | 1593 (0x00) | 17152 | 0 | 2 → 1 | 0x4300342E | Trace (Human01) |
| V069 | 104 | 1596 (0x02) | 4352 | 0 | 5 → 4 |  | Trace (Human01) |
| V070 | 209 | 1596 (0x02) | 7682 | 2 | 4 → 5 |  | Trace (Human01) |
| V071 | 351 | 1592 (0x01) | 14083 | 3 | 7 → 0 |  | Trace (Human01) |

**Human04, the first wanders.** Setup state 0xF07C3BB8, map size 96. Critters (type 0x39):

| ID | n | Slot | Turn draw r (`AND 255`) | Wander draw r2 (d) | Tile → target | Result | Source |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V072 | 7 | 1579 | 23301 (5) | 23615 (7) | (59, 44) → (58, 43) | Order 3 at n = 7; tile (58, 43) from n = 10 | Trace (Human04) |
| V073 | 7 | 1580 | 12067 (35) | 4352 (0) | (63, 44) → (63, 43) | Order 3 at n = 7; tile (63, 43) from n = 10 | Trace (Human04) |
| V074 | 8 | 1557 | 6160 (16) | 16610 (2) | (40, 79) → (41, 79) | Order 3 at n = 8; tile (41, 79) from n = 11 | Trace (Human04) |
| V075 | 9 | 1593 | 19475 (19) | 8785 (1) | (37, 19) → (38, 18) | Order 3 at n = 9; tile (38, 18) from n = 12 | Trace (Human04) |

**Human04, bob.** The troll destroyer (type 0x1F) in slot 1598 sits in order 2. Its countdown is 0 at creation, so
it draws at its first run.

| ID | n | r | New countdown `(r AND 7) + 8` | Next bob draw | Source |
| --- | --- | --- | --- | --- | --- |
| V076 | 2 | 21216 | 8 | n = 2 + 5 × 9 = 47 | Trace (Human04) |
| V077 | 47 | 17609 | 9 | n = 47 + 5 × 10 = 97 | Trace (Human04) |
| V078 | 97 | 11139 | 11 | n = 97 + 5 × 12 = 157 | Trace (Human04) |
| V079 | 157 | 9246 | 14 | n = 157 + 5 × 15 = 232 | Trace (Human04) |

**Human04, whole trace.** Setup and n = 1 to 3000 hold 13772 draws: 142 creation draws (68 map units at setup and 3
trained peons, 2 each), 12438 turn draws, 757 wander draws and 435 bob draws. The computer player makes none.

## Open questions

Carried over:

- What issues commands 9, 11, 12, 14 and 16. The game's own logic issues orders 11 (the computer players' wall rule
  in `movement/movement.md`), 12 (target acquisition and the reaction) and 16 (idle acquisition) through the same
  rules as these commands, and computer players' boarding units are given command 4 ("Boarding moves"). Nothing
  found issues commands 9 or 14 except the command stream itself (player interface or AI by number); which buttons
  or AI steps send them is still open.
- Order 57: nothing found sets it.
- **Target switch in order 15.** The same-update continuation after a switch of target is traced for orders 12 and 16
  (the friendly-fire check on the new target: `xhuman4`, `xhuman10-spells`). Research runs gave order 15 the same
  treatment with no trace getting worse.
- **The cleared target in order 4.** In `xhuman2` n = 2051 and `human06-31357` n = 9351 the target is cleared with
  order 4 kept. That is read as the computer player giving its order-4 move again, which hasn't been confirmed with a
  break log.
- **Classic attack-move** (static study only, untraced). Orders 4 and 5 search in both modes, and Classic order 10
  only reacts ("Reacting to an attacker"), as this spec says. Every trace in the set is Enhanced, so none confirms it.
- **A dying attacker elsewhere.** Traced so far: the resulting-order table counts a recorded dying attacker; orders 9,
  11 and 17 forget it at the start of each run; the call for help's "no attacker" test (`ai/attack-ai.md`) and the route
  search's (`movement/pathfinding.md`) ignore it. The static study shows that a unit given a new target by the no-route
  rule also forgets a dying attacker at the start of the run; no trace shows it. The forget at the start of every target
  search is traced (`xorc6-10000`, "Forgetting a dying attacker first"). The other places that look at the
  attacker (the reaction, the flee rule, the blocked rules) weren't checked one by one.
- **Losing a target during the cooldown.** In `xorc01-death-knight-2100` n = 915 ogre 1488 keeps order 12 (target
  null, animation 4, wait 15) through its cooldown frames after its target died at another unit's hand, and goes on
  only at the step done that ends the cooldown. Rules that make a cooling-down unit wait before losing its target
  broke other traces; the exact condition is open.
- **Draw order of a same-update idle.** In `human10` n = 6907 the no-route idle's draw comes before every other draw
  of the cycle. That fits destroyer 1483 updating early in the cycle, but the update order there wasn't confirmed
  separately.
- Commands 2, 6, 23, 24 and the spell commands: covered by the economy and spell specs.
- Whether the minimum-range test uses the same distance measure as the attack-range test during an attack.
- Which unit types count as "two-tile movers" for the flee rounding (expected: sea units and air units).
- The player-kind splits in "Terms" rest on the mapping of map owner codes to player kinds, which
  `movement/movement.md` still lists as open for rescue players.
- **Untraced look-around cases.** Fliers and catapults at rest, orders 7 and 13, and a critter whose wander target is
  clamped back to its own tile don't occur in the traces; those rules rest on the static study alone.
- **Same-update take-over.** The order logic signals when a command it gave should take over at once. The cases
  confirmed are listed in "Taking over"; coming out of a building is the one known exception. Default: every other
  command a unit's own order logic gives it takes over in the same update.
- **Arrival before acquiring for patrol.** Orders 4 and 10 arrive at the move goal before acquiring (`xorc11-10000`);
  whether patrol (order 5) does the same at a patrol end isn't traced.
- **The flee move's wait cap** ("Fleeing", step 6) rests on static study; no trace shows it.
- **An attack command on a dying enemy.** The pending idle of an attack command on a dying unit is traced only with the
  player's own unit as the target (`orc02-dyingcmd-3760`); the enemy case rests on static study.
- **Order 11's cooldown and range checks** ("Order 11: attacking a wall tile", steps 4.2 and 4.4) rest on static study;
  no trace reaches either on its own.
- **The order-12 skip mark across an order change** ("Order 12 with no route", step 3) rests on static study,
  except the clearing after a take-over caused by the order logic's own run, which rests on one capture that isn't
  committed (`orc12-30000` n = 29162, vector V219); commit or re-capture a trace that shows it.
- **A computer player's other commands and the remembered order.** Only command 4 is known to keep it; whether a
  computer player's commands 3 and 12, among others, clear it isn't known.
- **An attacker forgotten between runs.** In `xorc6-10000` axe thrower 1475 chases gryphon rider 1467 (type 0x2A), which
  dies at n = 5844 and is removed (order 0) from n = 5875; its slot isn't released yet. At n = 6106 1475's idle search
  picks 1459 with order 16, as if it had no attacker recorded. A kept dying attacker doesn't explain that, and what
  clears the record is open.
- **Facing in order 12's no-route take-over.** Where the facing comes from when the take-over isn't a strike decision
  is open; the `human5-10000` trace fits the strike facing.
- **A right-click, a cast and a build and the attacker record** ("A player's command forgets the attacker") rest on
  static study; no trace has any of them on a unit with a recorded attacker.

Found by the consolidation (static study, 2026-10-14; the rules above are unchanged):

- **The walk forgets a stale or undetected attacker.** "Terms" says an attacker that is alive but not **detected**
  stays recorded, and only a command forgets it. The static study finds that at each step boundary of a walk
  ("Walking orders") a unit that can move also forgets an attacker that is dying, dead, removed, inside something or
  not detected by its owner, the same test a command makes. No vector separates the two readings: in V190
  (`xhuman7-10000` n = 151) the hit destroyer keeps its undetected attacker, but whether it walked after the hit isn't
  shown. This is also one more place for "A dying attacker elsewhere" above.
- **Invisible units acquire nothing.** "Reacting to an attacker", step 3, says an invisible mage-type unit does
  nothing more in a run after the attacker check. The static study finds the same for every invisible unit that isn't
  a worker or tanker, in the order's logic and in the walk: after step 1 it makes no target search and no reaction. No
  vector has an invisible unit of another type. Round 68: switching the code to the static reading changes none of the
  158 trace rows (no trace has an invisible unit of another type); still open.
- **A mage with an attacker flees after choosing a harmless target.** "Special cases" and "Reacting to an attacker",
  step 3, say a mage-type unit attacks a chosen target it may take, and runs the reaction only otherwise. The static
  study finds that the reaction runs after the search in both cases, so a mage with a valid attacker it can flee from
  flees, and the flee move replaces the attack. Vector V174 (an example, untraced: an idle mage hit by a footman
  attacks a peasant that outscores the footman) takes the spec's reading; V144 (no attacker), V172 and V173 fit both.
  Round 68: the static study again finds the reaction after the search; switching the code changes no trace row, and
  staging the case failed. Keep V174 until a trace decides.
- **Commands that do nothing.** The static study finds that a command changes nothing at all (no target, goal, route,
  counter or attacker) for a unit whose order is 0, 1, 22, 25, 26, 36, 37 or 57. "Setting a pending order" names dying
  or dead units, units inside something and order 25; the static list adds a worker inside a building it is
  constructing (22), a building working (37) and order 57. Vector V020 fits both.
- **Commands given to a unit that can't move.** The static study finds that, for a unit whose type has no
  movement-class flag (buildings), the command steps always end with idle made the pending order, whatever the
  command. Which commands reach a building this way isn't known; "Commands forget a stale attacker" already leaves
  buildings out of the attacker check.
- **The skip mark and a dying attacker.** "Order 12 with no route", step 3, says the mark skips the run's target
  search, and "Forgetting a dying attacker first" puts the forget at the start of each search. The static study finds
  that a run whose search the mark skips still forgets a dying attacker, in orders 12, 15 and 16 too ("A dying
  attacker in orders 9, 11 and 17" says those orders don't forget it at the start of their run). This forget, like
  the one in orders 9, 11 and 17, also covers an attacker that is inside something. No vector separates the readings.
- **Friendly fire before the lost-target check.** "Friendly-fire check (orders 12, 15 and 16)" puts the check after
  the cooldown test and the target check. If "the target check" there means the lost-target check, the static study
  disagrees: it finds the check after the kind check but before the lost-target and range checks, made on the target's
  tile even when the target is lost. The outcomes differ only for a ground attacker whose target is lost, whose search
  found nothing new, and whose own units stand around the lost target's tile: idle (2) at once, rather than the lost
  rule (the pending order, or stand ground, 13, for order 15). V139 to V141 fit both readings. Round 68: the static
  study confirms its reading and switching the code changes no trace row; a trace needs a catapult in order 15 whose
  target is lost during its cooldown, with its own units around the target's tile and nothing else in range.
- **Statements made explicit by the consolidation, static study only and untraced:**
  - every place that makes idle the pending order shares the resume step of "Giving up when blocked" (the still
    animation with wait 1, then the remembered order as a command; "Resuming patrol and attack-move"); traced since
    round 68 for the friendly-fire hold (V270, V271), and a player's command that fails the target check has nothing
    to resume (V272);
  - a pending idle that takes over makes a unit that can move forget its target ("Taking over"); the traced
    take-overs of a pending idle that show the target show it null (for example V136 and V199);
  - the attack run ("Attack orders"): orders 9 and 17 make the cooldown test before the target checks; a target that
    fails the kind check gives a pending idle; and in orders 9 and 12 a valid target out of range when the logic runs
    is attacked again as a command (round 68: no trace separates these; where a unit that has arrived is out of its
    strike's reach, the ending other than the kind check isn't settled);
  - every walking order whose target unit is lost at a step boundary ends its walk there ("Walking orders", step 2;
    traced for orders 3 and 4 here and for 23 and 24 in `economy/harvest.md`);
  - while walking, the demolish orders 20 and 21 run the reaction, order 7 runs the target search, and every order that
    neither acquires nor reacts only forgets a dying attacker ("Walking orders", step 1); for order 7 the static study
    also finds that this search's resulting order is always 12, where "Resulting order" gives an idle-type order 16
    when the unit has no attacker (no vector has a walking guard acquire);
  - a unit already in reach of its goal makes none of the walk's steps in that update, so no walking search, reaction
    or target drop ("How an update runs", step 2; traced only for the acquisition of orders 4 and 10, V196, and for
    the walking orders' reaction, "Walking orders react only when the walk has ended").

Order 18's switch to order 17, listed here before round 68, is now traced (V269).

Round 68 (Enhanced):

- **Order 18's arrival by base range.** Order 18 arrives within base range of the target tile or within the goal
  radius of the pulled-back goal ("Arrival radius exceptions"). Only the goal-radius test is separated by a trace
  (V268); the base-range test and the sea and air radius of 3 rest on static study.
- **Order 18 with no route** isn't traced: in `orc9-orc09-ground-move-goals-1100` the pull-back always gave a
  reachable goal.

Found by the second review of the movement consolidation (static study, 2026-10-14; the rules above are unchanged):

- **Row A's order 11 may be given as a command.** "Order 11: attacking a wall tile", step 5, and V183 say the pending
  order 11 of blocked row A (`movement/movement.md`) is set directly, not through a command. The static study finds
  the steps of a command there ("What every command does"): the pull-back runs but keeps the blocking tile, the
  blocked counter is cleared, the target unit is dropped, and the unit is marked as having a pending command. The goal
  is the same either way; the counter, the target unit and the mark differ. `movement/movement.md` records the same
  question under "Open questions". Step 5 and V183 keep governing until a trace decides; a trace needs a computer unit
  that row A sends at a wall and that is blocked again on its way.

## History

Erratum rounds that shaped this spec and the sections where their content now lives. The erratum notes for each
round are kept with the research-side specs. Vectors added before round 59 sat in two shared tables; their IDs are
V001 to V054 (the first table) and V080 to V213 (the "Taking over" table), and the review notes map each one.

| Round | Content | Now in |
| --- | --- | --- |
| 10 | A hit records its attacker only under the call-for-help rules; the first live attacker is kept | "Reacting to an attacker" |
| 13 | Command 17 gives order 17 at once when the target tile is in the unit's own region | "Commands" (table) |
| 14 | Orders 33 and 14 forget the building's attacker each time their logic runs | "The order list"; "Attacking without moving", order 14 |
| 30 | Idle look-around (turn, wander, bob); a unit is created in order 2 and its first order is pending | "Idle look-around"; "Orders given when a unit is created"; vectors V055 to V079 |
| 31 | A take-over replaces that step done's logic run | "Taking over"; "How an update runs", step 1 |
| 33 | A move that arrives goes idle through a pending order; no route goes idle as an immediate order; same-update take-overs | "Arriving and losing the target"; "No route"; "Same-update take-over" |
| 35 | An idle unit's acquired order takes over in the same update; tower search per loop; Enhanced resume on every pending idle; no-route attack fallbacks never for orders 3, 4, 5 and 10 | "Resulting order"; "Attacking without moving"; "Resuming patrol and attack-move"; "No route" |
| 36 | Order 30's radius 3; patrol arrival takes over with wait 3; a command clears the remembered order | "Order 30's radius"; "Arriving and losing the target", order 5; "Resuming patrol and attack-move" |
| 37 | A killer forgets its dead target at once; the strike decision tests the cooldown; a firing tower stores no target | "Arriving and losing the target"; "Attack orders", "Strike decision"; "Attacking without moving" |
| 38 | A lumber worker's own new aim is a same-update take-over | "Same-update take-over" |
| 39 | An acquisition in order 12 doesn't stop the unit; no-route immediate idle for patrols | "Resulting order"; "No route" |
| 40 | Chase with no route goes idle with its idle logic; order 16 out of range; order-12 switch out of reach; Enhanced attack-move acquisition is a command; order 12 searches first; mage rule on the chosen target; flee take-over; halving search | "No route"; "Attacking without moving"; "Resulting order"; "Attack-move: orders 4, 5 and (Enhanced) 10"; "Special cases"; "Fleeing"; "The halving search" |
| 41 | Order-12 switch while cooling down; orders 4 and 5 acquire like Enhanced order 10; friendly-fire check placement; order 15 out of range; last-entry search start; order 24 with no route; a take-over keeps the blocked counter; flee animation | "Resulting order"; "Attack-move: orders 4, 5 and (Enhanced) 10"; "Friendly-fire check (orders 12, 15 and 16)"; "Attacking without moving"; "The halving search"; "No route"; "Taking over"; "Fleeing" |
| 42 | Order-12 switch from a dying target; a take-over keeps the cached route; orders 15 and 16 search first; workers and tankers never search; resulting order always a command; order 9 with no route; computer boarding unit stays in order 34 while walking; friendly fire not checked while cooling down | "Resulting order"; "Taking over"; "Attacking without moving"; "Special cases"; "No route"; "Boarding moves (order 34)"; "Friendly-fire check (orders 12, 15 and 16)" |
| 43 | Orders 9, 11 and 17 forget a dying attacker first; after a target switch the run goes on with the new target | "Arriving and losing the target", "A dying attacker in orders 9, 11 and 17"; "Friendly-fire check (orders 12, 15 and 16)"; "Attack orders" |
| 44 | Attack-move toward a building heads for its goal zone; the attacker seeds only with a score above 0; inside units keep their list place; flee move and patrol turnaround are commands | "Attack-move: orders 4, 5 and (Enhanced) 10"; "Choice"; "Vertical order"; "Fleeing"; "Arriving and losing the target", order 5 |
| 45 | A unit cooling down keeps its order when its target is lost; resumed orders are commands; commands forget a stale attacker; a move with no route keeps its attacker; mage-type units count as non-combat | "Losing the target while cooling down"; "Resuming patrol and attack-move"; "Commands forget a stale attacker"; "No route"; "Reacting to an attacker" |
| 46 | The attacker seed's score of 0 comes only from the rejections before scoring | "Choice" |
| 47 | Order 12 with no route searches its attack range, with the skip mark; a command's failed target gives a pending idle | "Order 12 with no route"; "Commands forget a stale attacker" |
| 48 | Order 11; attack command on any owner; attack command on a dying unit; order-12 switch is a command while the old target is recorded; skip-mark duration | "Order 11: attacking a wall tile"; "Commands" (table); "Commands forget a stale attacker"; "Resulting order"; "Order 12 with no route" |
| 49 | Command 4 with a target unit; a computer player's command 4 keeps the remembered order; Enhanced sea-only rule for order 10 only; undetected attacker ignored and kept; checks of commands 8 and 9 | "Commands" (table); "Resuming patrol and attack-move"; "Automatic target acquisition"; "Terms"; "Checks on an attack command" |
| 50 | Attacker the unit can't target forgotten when gone; order-12 switch out of reach while cooling down; command 4's end forgets the attacker | "Reacting to an attacker", step 1; "Resulting order"; "Commands" (table) |
| 51 | Every target search first forgets a dying attacker; orders 4 and 10 arrive before acquiring; flee move is a smart move; recorded attacker outlives its slot; kind check terms | "Forgetting a dying attacker first"; "Arriving and losing the target"; "Fleeing"; "Terms"; "Checks on an attack command" |
| 52 | Skip mark gates only the logic's search; cooldown wins in order 16; one target field; the move step is skipped at the goal; "a unit that can move" | "Order 12 with no route"; "Attacking without moving"; "Terms"; "Arriving and losing the target"; "Forgetting a dying attacker first"; "How an update runs" |
| 53 | Attacker seed's fog test; skip-mark clearing | "Choice"; "Order 12 with no route", step 3 |
| 54 | Spell command target check; reaction step 1 is the kind check alone; order 12's no-route take-over | "Commands forget a stale attacker"; "Reacting to an attacker"; "Order 12 with no route" |
| 55 | A player's command forgets the attacker; patrol remembers both ends; reaction step 1 and flee have no detection test | "A player's command forgets the attacker"; "What a patrol remembers"; "Reacting to an attacker"; "Fleeing" |
| 56 | Right-click and build keep the attacker record, cast forgets it; unremembered patrol; remembered goal as given | "A player's command forgets the attacker"; "An unremembered patrol"; "What a patrol remembers" |
| 59 | Minimum range; target goal zone; shore-transport kind rule | "Automatic target acquisition"; "Attack-move: orders 4, 5 and (Enhanced) 10"; "Checks on an attack command"; vectors V214 to V218 |
| 60 | Kind check in full; goal zone for every walk with a target unit; skip mark after a take-over; reach to a shore building; resume when giving up blocked; smart move takes a mine or own platform | "Checks on an attack command"; "Attack-move: orders 4, 5 and (Enhanced) 10"; "Order 12 with no route"; "Arriving and losing the target"; "Giving up when blocked"; "Smart move (command 3)"; vectors V219 to V223 |
| 61 | Harvest test on type and cargo; return goods by smart move for workers only; giving up blocked keeps the walking animation; a complete attack-move is forgotten | "Smart move (command 3)"; "Giving up when blocked"; vectors V224 to V232 |
| 62 | Complete attack-move forgotten wherever idle becomes pending; countdown rules; follow radius +1; guard needs a followable target; smart move keeps its target | "Resuming patrol and attack-move"; "Follow (orders 6 and 7)"; "Smart move (command 3)"; vectors V233 to V239 |
| 63 | Smart move with a lost target ends as on arrival; walking orders react only at the walk's end; the arrival switch; command check on an inside target | "Smart move (command 3)"; "Walking orders"; "Walking orders react only when the walk has ended"; "How an update runs"; "Commands forget a stale attacker"; vectors V240 to V247 |
| 64 | Order 34 reacts only when waiting at its transport; order 6 runs the reaction but not its result; no route isn't a walk end for the reaction; order 16 cooling down keeps its order | "Walking orders react only when the walk has ended"; "Follow (orders 6 and 7)"; "Resulting order"; vectors V248 to V255 |
| 65 | A walk with a target unit heads for the target where it stands; follower inside the radius guards in the same update; guard with no route | "Walking orders"; "Attack-move: orders 4, 5 and (Enhanced) 10"; "Follow (orders 6 and 7)"; "No route"; vectors V256 to V263 |
| 66 | Evidence only: order 16 keep traced; repair and order 9 with no route traced | "Resulting order"; "No route"; vectors V264 to V266 |
| 68 | Order 18 walks to the pulled-back goal and arrives by base range or goal radius; its switch to 17 is a command taking over in the same update (wait 3); the friendly-fire hold makes idle pending with the resume step; a player's command forgets the remembered order before the target check | "Arrival radius exceptions"; "Arriving and losing the target"; "Friendly-fire check"; "Same-update take-over"; "Commands forget a stale attacker"; vectors V267 to V272 |
| 2026-10-14 | Consolidation into one model (B-038), no rule change | Whole spec; "The model", "How an update runs", "Walking orders", "What every command does" and "Attack orders" are new and restate rules already given elsewhere |
