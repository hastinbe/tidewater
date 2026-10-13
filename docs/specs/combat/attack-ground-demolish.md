# Combat: attack ground and demolish

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes, "Round 13 (attack ground, demolish)"). Sound names
  come from the game's sound list. Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-39 errata: the demolisher's HP doesn't change when it is killed after its blast;
  `human12-demolition` vector.
- **Changes:** 2026-10-12, round-42 errata: the ships' facing rule is now defined (`core/animation-scripts.md`, warships
  turn broadside).

## Behaviour

This spec gathers the two remaining command-panel orders: **attack ground** (siege weapons and big ships fire at a
tile) and **demolish** (the dwarven demolition squad and goblin sapper blow themselves up). Much of both is already
in other specs; this spec gives the missing rules and points to the rest:

| Part | Where |
| --- | --- |
| Which units get the Attack ground and Demolish buttons | `ui/command-buttons.md` |
| Order numbers, animations, radii | `orders/orders.md`, "The order list" |
| How the unit walks and when it counts as arrived | `movement/movement.md`, "Arriving" |
| The missile fired at a tile and its splash | `combat/damage.md` |
| What the blast does to rock, forest and walls | `movement/occupancy.md`, "Rock" |
| Sounds | `ui/sounds.md` |
| The command 17 rule, the blast's damage to units, attack ground's order logic | this spec |

### Attack ground

**Who can do it.** Any unit whose type has unit flag bit 14 (in the default data: ballista, catapult, battleship and
juggernaught). The command itself doesn't test the flag; only the button does.

**The command** (command 17). It carries a target tile. Any unit under the cursor is ignored, and the unit forgets
any target unit it had. Then:

1. If the target tile has the **same region value** as the unit's own tile (`movement/occupancy.md`, "How
   passability changes": the same water area, or the same land area), the order is **17**.
2. Otherwise, if the target tile is already within the unit's attack range (both axes), the order is **17**.
3. Otherwise the target tile is also stored as the unit's move goal, and the order is **18**.

So a target the unit can reach on its own ground or water uses order 17 at once, however far away it is. Order 18
is used only for a target in a different region (across water, on a cliff top of another land area, a forest, rock
or wall tile, or land for a ship) that is out of range.

**Order 18: moving into range.** The unit moves toward the stored goal as for a move order (the goal may be pulled
back to a tile it can reach, `movement/pathfinding.md`). As soon as it is within its type's attack range of the
stored goal on both axes, its path is dropped and it gets order 17 (`orders/orders.md`). This test uses the type's
base range without range upgrades; no unit that can attack the ground has one.

**Order 17: attacking the ground.** The order's goal is the target tile, and its radius is the unit's attack range.

1. While the unit is out of range, it walks toward the target tile like any moving order.
2. Each time it is in range when its order logic runs:
   - it turns to face the target tile (warships turn broadside instead, by the table in `core/animation-scripts.md`,
     "Facing a tile", as for their other attacks);
   - the attack sound plays (`ui/sounds.md`);
   - its attack script runs, and at the strike point the missile is fired at the centre of the target tile
     (`combat/damage.md`, "How an attack delivers damage").
3. If the unit is out of range at a moment when its order logic runs and doesn't move (for example when movement
   gave up), it goes idle (order 2) at once.

**What attack ground doesn't do:**

- It never ends by itself. It has no target unit, so the target-loss rules don't apply. The unit keeps firing at the
  tile, at its normal rate of fire, until it is given another command, even when nothing is left there.
- It has **no friendly-fire check**. The check that stops an idle unit firing on a tile next to its own units
  (`orders/orders.md`) isn't made, so the splash hurts whatever is there, own units included.
- It doesn't look for enemies. A unit attacking the ground ignores enemies that walk up to it.

### Demolish

**Who can do it.** The Demolish button belongs to the dwarven demolition squad and the goblin sapper
(`ui/command-buttons.md`).

**The command** (command 19), as in `orders/orders.md`:

- a target unit or building gives order **20**, aimed at that unit;
- a target tile holding forest, a reserved tree, rock or a wall (by its region value, or by the wall bits 0x04 and
  0x08 of its movement word) gives order **20**, aimed at the tile;
- any other tile gives order **21**.

**Getting there.** The demolisher walks to its goal like any moving order: within 1 tile on both axes for order 20
(for a building, measured to the footprint tile picked by the building goal rule), and onto the tile itself for order
21. If it can't get there, the movement rules make it give up and go idle; nothing explodes.

**The blast.** When it arrives, it blows up **where it stands**, not on the target. Let (x, y) be the demolisher's
own tile. In this order:

1. **Terrain.** Rock, forest and walls around (x, y) are cleared: `movement/occupancy.md`, "Rock", steps 1 to 3.
2. **Units.** The ground occupants of the 8 neighbouring tiles are hit, visiting the tiles in this order:
   (x+1, y+1), (x−1, y+1), (x+1, y−1), (x−1, y−1), (x, y+1), (x, y−1), (x+1, y), (x−1, y).
   For each tile:
   - a tile off the map, or with no ground occupant, is skipped;
   - if the occupant was already hit at an earlier tile of this blast (a building covering several of the tiles), it
     is skipped, so each unit or building is hit at most once;
   - otherwise the cosmetic explosion effect (effect kind 0x17) is created at the occupant's centre, 8 pixels up;
   - then the occupant takes **200** damage from the demolisher (`combat/damage.md`, "Applying damage");
   - then, if the tile still has a ground occupant, the same occupant takes **200** damage again.
3. **The demolisher dies.** It is killed outright, not by damage, so nobody is credited with its death. Its HP doesn't
   change (`core/unit-slots.md`, "Death, corpses and release"). It plays its own death sound (`misc\explode`,
   `ui/sounds.md`, "Deaths") and leaves no remains (`core/corpses.md`).
4. **The blast sound.** `misc\explode` plays at (x, y), audible off screen. With step 3, the blast makes two
   explosion sounds.

**The damage in detail.**

- The 200 is applied as it is: no armour, no draw. It goes through the normal rules for applying damage, so a dying or
  used target ignores it, every hit target is notified that it was attacked by the demolisher (`ai/attack-ai.md`,
  "Call for help"), unholy armour blocks it, and a kill is credited to the demolisher's owner.
- The second 200 only matters for a target that survived the first: one that died is dying, and ignores it. So in
  effect anything with more than 200 HP takes 400, and anything with 200 or less dies.
- Ownership isn't tested. The demolisher's own and allied units and buildings next to it are hit as well.
- Ground occupants include ships on neighbouring water tiles. Air units are never ground occupants, so the blast never
  hits them, even when one is the target.
- The demolisher's own tile isn't one of the 8, and it holds no other ground unit, so nothing there is hit.

## Data

| Item | Value |
| --- | --- |
| Attack-ground flag | unit flag bit 14 |
| Attack ground orders | 17 (attack), 18 (move into range) |
| Demolish orders | 20 (at a unit or obstacle, radius 1), 21 (at a location, radius 0) |
| Blast damage per hit | 200, ignoring armour |
| Hits per unit | at most 2 (the second only if the tile is still occupied) |
| Blast tiles for units | the 8 neighbours, order (+1,+1), (−1,+1), (+1,−1), (−1,−1), (0,+1), (0,−1), (+1,0), (−1,0) |
| Effect on each hit unit | cosmetic effect kind 0x17 at its centre, 8 px up |
| Blast sound | `misc\explode`, placed at the demolisher's tile |

## Test vectors

| Input | Expected |
| --- | --- |
| Catapult (range 8) on land area A at (10, 10); command 17 on a grass tile of area A at (30, 10) | Order 17 at once; it walks until within 8 tiles of (30, 10) on both axes (column 22), then fires at the tile |
| Same catapult; command 17 on a water tile at (14, 10) | Different region, in range: order 17; it fires without moving |
| Same catapult; command 17 on a forest tile at (25, 10) | Different region, out of range: order 18 toward (25, 10); at column 17 it becomes order 17 and fires |
| Battleship on water area W; command 17 on a land tile 3 tiles away, inside its range | Order 17 at once |
| Ballista attacking the ground at a tile where only its own footman stands | It keeps firing; the bolt's splash hurts the footman |
| Ballista attacking the ground at an empty tile; an enemy grunt walks up and attacks it | It keeps firing at the tile and doesn't turn on the grunt |
| Catapult attacking the ground; the player clicks Stop | The order ends (only a new command ends it) |
| Command 19 on an enemy grunt | Order 20; the squad walks next to the grunt and blows up |
| Command 19 on a tree tile at (20, 20); the squad comes from the west and stops at (19, 20) | The blast is centred on (19, 20): terrain steps around (19, 20), then units on its 8 neighbours |
| Command 19 on an open grass tile (15, 15) | Order 21; the squad walks onto (15, 15) and blows up there |
| Squad blows up at (10, 10); an enemy grunt (60 HP) at (11, 10) | Effect at the grunt; 200 damage: the grunt dies; the second 200 is ignored. Kill credited to the squad's owner |
| Same, an enemy 3 × 3 barracks (800 HP) covering (11, 9) to (13, 11) | Neighbours (11, 11), (11, 9), (11, 10) are the barracks: hit once at (11, 11) (the first in order), 200 then 200: 400 HP left. The later two positions skip it |
| Same, an own footman at (9, 9) | Hit like any other unit (no owner test) |
| Same, an enemy gryphon rider over (10, 11) | Not hit |
| Same, an enemy unit with unholy armour at (10, 11) | No damage; it is still notified it was attacked, twice |
| Squad blows up at (0, 0) | Neighbours with −1 are off the map and skipped; (1, 1), (0, 1), (1, 0) are tested |
| `human12-demolition` (oracle trace), n = 239: demolition squad 1430 (type 0x0E, 40 HP) reaches (29, 100) and blasts | Killed in the same update: order 1, animation 1, HP **40**, and still 40 through its death animation, to the trace's end at n = 450 |
| Any blast | Order of events: terrain, unit hits, squad dies (its death sound `misc\explode`), then `misc\explode` at the tile |

## Open questions

- **Order 17 when movement gives up.** A unit that can't reach range under order 17 gives up through the movement
  rules (`movement/movement.md`, "Blocked") and goes idle. Whether it ever runs its order logic out of range in other
  cases hasn't been traced; the result would be the same (idle).
- **A second cleared value.** Command 17 also clears one more per-unit value that wasn't identified. Default: no
  visible effect.
- **Target tile in another region but reachable.** Region values compare exactly, so a ship's target tile on another
  water area counts as a different region even when a path exists. Default: as stated (order 18 if out of range).
- **Arrival for order 20 at a tile obstacle.** The radius is 1 from the target tile. A rock or forest tile surrounded
  by obstacles may not be reachable; the walk then gives up as above. Not checked in play.
