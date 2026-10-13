# Spells: casting, effects, durations and summoned units

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the animation data in
  `Rez\script.bin` (decoded with the instruction set of `core/animation-scripts.md`), the default unit data
  (`Rez\unitdata.dat`) and the unit flags in the public PUD specification text. Not yet confirmed against the running
  game.
- **Changes:** 2026-10-11, round-8 errata: the fireball's count goes on to 41 and it ends on the step after count 41.
  2026-10-12, round-16 errata: the cosmetic effects' lifetimes (each plays its script once; removed in cycle
  U + 1 + W).
- **Changes:** 2026-10-12, round-24 errata: a new effect starts at its start step, A cycles after its first update
  (per-kind table); the cosmetic effects are removed in cycle U + A + 1 + W (cannon smoke U + 24, not U + 13); the
  open question on each kind's own script is closed. Kinds 0x13 and 0x14 are the building fires made at map start.
  2026-10-12, round-30 errata: a test vector confirms the building fires' start step in the running game.
  2026-10-12, round-35 errata: the mana gain of the timers pass has no building exception; a building with the caster
  flag starts its counter at 0.
  2026-10-12, round-37 errata: a kind 0x16 marker on a unit is placed from the type's box size (as for kinds 8 and 9),
  with no destination, target or shooter.
  2026-10-12, round-39 errata: every cosmetic effect (kinds 0x0B, 0x15, 0x16, 0x17, 0x19, 0x1A and 0x1C) is created with
  only its kind and position set (destination (0, 0), no target, no shooter, damage 0), not only kind 0x16; `xorc12`
  rune vector.
  2026-10-12, round-40 errata: each fireball splash follows the hammer and breath rule: it needs a free effect slot, and
  its kind-0x17 explosion has only kind and position set.
  2026-10-12, round-44 errata: a blizzard shard's destination is its landing point from creation, through its initial
  wait; `xhuman10-blizzard-600` vector.

## Behaviour

Phase numbers are those of `core/timing-and-rng.md`. "Classic" and "Enhanced" are its two rule modes. Flag bits are
the PUD unit flags. A **tile centre** is the tile's pixel position plus (16, 16). **Effects** are the game's missile
objects: they live in the effect slots of `core/unit-slots.md` and are updated in phase 4 after the units.

### Orders and mana costs

Each spell is an order from 38 to 56 (`orders/orders.md`, "Spell orders", which also gives each spell's radius). Mana
is a byte from 0 to 255.

| Order | Spell | Mana | Aimed at | Effect happens |
| --- | --- | --- | --- | --- |
| 38 | Holy vision | 70 | a point | once |
| 39 | Healing | 6 per hit point healed | a unit | once |
| 40 | Not used (no effect; its table cost is 5) | — | — | — |
| 41 | Exorcism | 4 per hit point of damage | a point | once |
| 42 | Flame shield | 80 | a unit | once |
| 43 | Fireball | 100 | a point | once |
| 44 | Slow | 50 | a unit | once |
| 45 | Invisibility | 200 | a unit | once |
| 46 | Polymorph | 200 | a unit | once |
| 47 | Blizzard | 25 per strike | a point | repeatedly (channelled) |
| 48 | Eye of Kilrogg | 70 | a point | once |
| 49 | Bloodlust | 50 | a unit | once |
| 50 | Raise dead | 50 per skeleton | a point | once |
| 51 | Death coil | 100 | a point | once |
| 52 | Whirlwind | 100 | a point | once |
| 53 | Haste | 50 | a unit | once |
| 54 | Unholy armour | 100 | a unit | once |
| 55 | Runes | 200, partly refunded | a point | once |
| 56 | Death and decay | 25 per strike | a point | repeatedly (channelled) |

Which spells a player may cast (research, the button bar) is not covered here.

### Giving the order

When a spell command is given:

- If the **all-spells cheat** is on (the cheat that makes every spell known for every player at once), the caster's
  mana is set to 255 first. The same cheat lets the cast buttons skip their mana test (`ui/command-buttons.md`).
- The spell order becomes the caster's pending order (`orders/orders.md`).
- For the spells aimed at a point (orders 38, 40, 41, 43, 47, 48, 50, 51, 52, 55 and 56) the caster's target unit is
  cleared and only the target tile is kept.

### Moving and casting

Each time the order's logic runs, the caster moves toward the target until it is within the spell's radius
(`movement/movement.md`), and then casts:

- **Point spells:** once arrived, the caster turns to face the point and plays its casting animation.
- **Unit spells** (flame shield, slow, invisibility, haste, unholy armour): the target must still be valid and must not
  be a building.
- **Organic-target spells** (healing, polymorph, bloodlust): the target must also have flag bit 27 (organic).

If a check fails, the caster becomes idle (order 2) and no mana is spent.

The spell takes effect at the **strike** event of the caster's attack animation (`core/animation-scripts.md`): cycle 12
of a 40-cycle loop for mages and death knights, cycle 11 of a 25-cycle loop for paladins, and cycle 8 of a 25-cycle
loop for ogre-mages, without haste or slow.

### At the strike

Unless the spell's row below says otherwise, the strike works like this:

1. **Target check** (unit spells only): the target must exist and must not be dying, dead remains or otherwise
   inactive. If it fails, the caster becomes idle and nothing else happens.
2. **Mana check:** if the caster's mana is below the cost, it becomes idle and nothing happens.
3. **Pay** the cost.
4. The caster becomes **idle** (order 2 pending). The two channelled spells skip this step, so their order stays and
   they strike again on every loop of the casting animation until a mana check fails.
5. The caster's **invisibility timer is cleared**. Any spell cast ends the caster's own invisibility.
6. The spell's effect.

| Spell | Strike, in order |
| --- | --- |
| Holy vision | Steps 2 to 5. Then 7 areas are revealed for the caster's player, centred on the target point T and on T + (0, −8), T + (0, +8), T + (+6, +4), T + (+6, −4), T + (−6, +4) and T + (−6, −4), in that order. Each is revealed exactly like a unit with **sight 9** standing on that tile (`vision/fog.md`). Each point also gets a cosmetic effect (kind 0x16) at its tile's pixel position + (8, 8). Points off the map are skipped: in Enhanced mode a point is used only if both coordinates are from 0 to map width − 1; in Classic mode a coordinate may also equal the map width, and such a point reveals nothing but still gets its cosmetic effect. A negative coordinate is off the map in both modes. (Maps are square, so the width is also the height.) In Enhanced mode the fog refresh countdown is then set to 150, so the next refresh comes 150 cycles later |
| Healing | Step 1 (no target: idle). The caster becomes idle and its invisibility ends. Let missing = maximum HP − HP. If missing is 0, nothing more. Otherwise heal = min(missing, floor(mana / 6), 40). If heal is 0, nothing more. The target gains heal HP, the caster pays 6 × heal mana, and a healing effect (kind 9) appears on the target |
| Exorcism | The caster becomes idle and its invisibility ends (no up-front cost). Then the 7 × 7 tiles around the target tile are scanned in 4 rings (below). At the start of each ring, if mana is below 4, the scan stops. Each ground-layer unit found that has flag bit 15 (undead) is exorcised, except, in Enhanced mode, a unit of players 0 to 7 that is allied with the caster. Exorcising: damage = min(floor(mana / 4), the unit's HP); an exorcism effect (kind 8) appears on it; the damage is applied with the caster as the attacker (`combat/damage.md`, "Applying damage"); the caster pays 4 × damage |
| Flame shield | Step 1. The caster becomes idle and its invisibility ends **before** the mana check. If the target's flame-shield timer is not 0, or the target has flag bit 1 (air), nothing more happens and no mana is spent. Otherwise step 2 (mana check: idle), pay, set the target's flame-shield timer to **500**, and launch the flame-shield bolt (below) |
| Fireball | Steps 2 to 5, then launch the fireball (below) |
| Slow | Steps 1 to 5. Then the target's speed timer: if it is negative, it becomes −1000; otherwise it becomes timer − 1000. A cosmetic effect (kind 0x16) appears on the target |
| Invisibility | Steps 1 to 5. The target's invisibility timer becomes **2000**. Effect kind 0x16 on the target |
| Polymorph | Steps 1 to 5. The target gets the **used** mark (`core/corpses.md`) and is killed: it plays its own death animation and then, if its type has a corpse kind, leaves a dead body that keeps the used mark, so raise dead can't use it; the body decays on the normal timetable. A critter (type 0x39) is created for the neutral player (15) at the target's tile (2 creation draws, below), and effect kind 0x16 appears at that tile's centre |
| Blizzard | Step 2, pay 25, step 5 (no idle). Five shard chains are started, each with 10 repeats (below) |
| Eye of Kilrogg | Steps 2 to 5. An eye of Kilrogg (type 0x2D) is created for the caster's player next to the **caster** (not the target point), with the free-tile search of `movement/placement.md` ("a spell creates a unit next to its caster"). The search makes no draws; the new eye makes 1 creation draw (below). If no tile is found, no eye is created, the mana stays spent, and the blocked-exit message (entry 67 of `network.tbl`) is set |
| Bloodlust | Steps 1 to 5. The target's bloodlust timer becomes **1000**. Effect kind 0x16 on the target |
| Raise dead | The caster becomes idle and its invisibility ends. Then every unit slot is scanned in ascending order (`core/unit-slots.md`) while the caster's mana is at least 50. A slot qualifies if it holds a dead body (type 0x69) in its remains state that has not been used yet, and the squared tile distance from the body's tile to the target tile, dx² + dy², is at most **36**. For each: a skeleton (type 0x37) is created for the caster's player directly on the body's tile (no free-tile search; only the creation checks of `core/unit-slots.md` apply); the skeleton makes 1 creation draw (below); if that worked, effect kind 0x16 appears on it and the body gets the **used** mark; in either case the caster pays **50**. The used mark only stops the body being raised again: it keeps decaying and is removed on its normal timetable (`core/corpses.md`) |
| Death coil | Steps 2 to 5. Then victims are chosen and missiles launched (below), and the caster gains HP |
| Whirlwind | Steps 2 to 5, then the whirlwind is created (below) |
| Haste | Steps 1 to 5. Then the target's speed timer: if it is greater than 0, it becomes 1000; otherwise it becomes timer + 1000. Effect kind 0x16 on the target |
| Unholy armour | Steps 1 to 5. The target's unholy-armour timer becomes **500**. Its HP is halved, rounding down, if it is at least 2 (1 HP stays 1). Effect kind 0x16 on the target |
| Runes | Steps 2 to 5, then up to 5 runes are placed and part of the cost is refunded (below) |
| Death and decay | Step 2, pay 25, step 5 (no idle). Five decay clouds are created, each with 10 pulses (below) |

A "unit spell" with no target at its strike (for example, the target died) makes the caster idle without paying.

**Exorcism rings.** With the target tile at (x, y), ring k (k = 0 to 3) is visited like this:

1. For each column from x − k to x + k, left to right: the tile (column, y + k), then the tile (column, y − k).
2. Then, if k ≥ 1, for each row from y − k + 1 to y + k − 1, top to bottom: the tile (x + k, row), then the tile
   (x − k, row).

Ring 0 therefore visits the target tile **twice**. Tiles off the map are skipped. Only the ground-layer unit of each tile
is looked at (`movement/occupancy.md`), so a building is visited once per footprint tile it covers.

**Death coil victims.** The 5 × 5 tiles centred on the target tile are scanned row by row from the top, left to right.
On each tile the ground-layer unit is looked at first, then the air-layer unit. A unit is a candidate unless it is:
allied with the caster (Enhanced mode only), owned by the caster's player, dying or dead, or without flag bit 27
(organic). A building covering several scanned tiles is found once per tile. The candidates are then sorted by HP,
lowest first. Candidates with equal HP don't keep their scan order. The order is the one you get by going through the
list positions from the first, and for each position comparing its candidate with every later candidate in turn,
swapping the two whenever the one at the position has more HP. For example, scan order A (30 HP), B (30 HP), C (5 HP)
becomes C, B, A. A budget of
**50** is shared out in that order: each victim takes its full HP while the budget lasts; the first victim that doesn't
fit takes what is left, and the victims after it get nothing and are dropped. For each kept victim, in order, the
caster launches its own missile weapon at it through the normal launch of `combat/damage.md` (so the launch makes its
usual damage draw and two scatter draws), and the missile's stored damage is then replaced by the victim's share. On
impact the share is applied to the victim with the caster as the attacker. The caster then gains the total of the
shares at once, capped at its maximum HP. With no candidates, the 100 mana is still spent.

**Runes.** The tiles tried are, in order: the target tile, then the tiles north, east, south and west of it. A rune is
placed on a tile if the tile is on the map, no ground-layer unit or building is on it, no live rune is already there,
and one of the 50 rune slots (shared by all players) is free. There is no terrain test: runes can go on water, trees
and rock. Each placed rune gets a lifetime of **2048** and a cosmetic effect (kind 0x0B) at the tile centre. With n
runes placed (0 to 5), the caster is refunded (5 − n) × 40 mana, so it pays 200 − (5 − n) × 40 in all.

### Timers

Each unit has five spell timers. They are advanced by the **timers pass** at the end of phase 4, once per cycle, in
ascending slot order. Units that are dying or are remains, and units in one other inactive state, are skipped.

| Timer | Set by | Each cycle | While running |
| --- | --- | --- | --- |
| Invisibility | Invisibility (2000) | −1 toward 0 | The unit is detected only as `vision/fog.md` describes. A unit's own spell cast or attack strike sets it to 0 |
| Unholy armour | Unholy armour (500) | −1 toward 0 | The unit takes no damage from hits (`combat/damage.md`), and runes don't hurt it |
| Bloodlust | Bloodlust (1000) | −1 toward 0 | Basic and piercing damage are doubled (`combat/damage.md`) |
| Speed (signed) | Haste (positive), slow (negative) | moves 1 toward 0 | Positive: hasted; negative: slowed (`core/animation-scripts.md`) |
| Flame shield | Flame shield (500) | −1 toward 0 | Only blocks a second flame shield; the flames themselves last on their own (below) |

Also in the timers pass, for each unit, in this order:

1. **Berserker regeneration:** a berserker (type 0x13) whose player has the regeneration upgrade counts cycles in
   its mana byte. When the count reaches 240 it goes back to 0, and the berserker gains 1 HP if below its maximum.
2. **Demolition squads and sappers** (types 0x0E and 0x0F) are killed at once if their invisibility or unholy-armour
   timer is running.
3. The five timers above.
4. **Mana** for units with flag bit 17 (caster): if mana is below 255, a per-unit counter counts down; when it reaches
   0 it is reset to 40 and mana goes up by 1. So a caster gains 1 mana every 40 cycles, up to 255. This includes
   buildings with the flag. Every newly created caster that isn't a building (placed by the map, trained, or created
   any other way) starts with **85 mana** and its counter at **40**, so its first extra mana point comes on its 40th
   timers pass. A building with the flag starts with its type's "magic" value and its counter at **0**, which wraps
   to 255 on the first pass, so its first point comes on its 256th pass. While it is under construction or working,
   its mana byte holds its progress (`economy/build-train.md`), and these gains add to it. In the default data no
   building has the flag.
5. **Decay** for other units whose type has a non-zero decay rate, if they were created during play (below).

The runes are advanced before the units in the same pass (below).

### Summoned units and decay

A unit whose type has a non-zero **decay rate** (the `UDTA` decay byte) and which is created during play (not while
the map loads) gets a **period counter** of 255 and a **countdown** equal to the decay rate. Each cycle the countdown
goes down by 1. When it reaches 0 it is reset to the decay rate and:

- if the period counter is above 0, it goes down by 1;
- otherwise, if the unit has at least 5 HP, it loses 5 HP;
- otherwise it is killed (one special case, see the open questions).

In the default unit data only the skeleton (decay 100) and the eye of Kilrogg (decay 3) decay. The unit doesn't die on
reaching 0 HP: it is killed one period later.

| Unit | HP | Decay | Killed on timers pass |
| --- | --- | --- | --- |
| Eye of Kilrogg | 100 | 3 | 3 × (255 + 20 + 1) = 828 |
| Skeleton | 40 | 100 | 100 × (255 + 8 + 1) = 26400 |

"Timers pass n" counts the pass in the cycle the unit was created as pass 1.

### Runes on the map

Each cycle, before the units' timers, every live rune is advanced:

1. Its lifetime goes down by 1. (Every 256 cycles, when the low byte of the lifetime becomes 1, the rune's cosmetic
   effect is shown again.)
2. If a ground-layer unit or building is on the rune's tile, the rune is used up (its slot is freed) with an explosion
   effect (kind 0x17). Then, if that unit's unholy-armour timer is 0: a unit with 50 HP or less is killed (with no
   attacker); otherwise it loses **50 HP**. Armour doesn't count, and the unit isn't told it was attacked.

A rune whose lifetime reaches 0 is gone after that cycle's check. Runes hit **any** ground unit, including the
caster's own and allied units, and ships on water. Air units never trigger them.

### Spell effects in flight

All of these are effects in the effect slots. A spell that needs a free slot and finds none simply makes that piece
of the spell fail. Every effect runs a small script from `script.bin`; the "step" below is its "step done" signal.

**How flying effects move.** An effect flying from S to G uses a fixed line rule. Let D = the larger of |Gx − Sx| and
|Gy − Sy| (the **major** distance) and d the smaller. A counter starts at floor(D / 2), or 1 if that is 0. On each
step the counter goes down by d; if it is then 0 or less, the effect also moves on the minor axis and the counter goes
up by D. The effect always moves on the major axis. With a **speed** of s pixels, each axis move is s pixels toward G.
The flight's remaining distance starts at D and goes down by s per step. For speed 1 this reaches G after exactly D
steps (for any G other than S itself).

| Effect | Kind | Speed (px per step) | Steps |
| --- | --- | --- | --- |
| Fireball | 2 | 12, then 6 | every cycle |
| Flame-shield bolt | 3 | 12 | every cycle |
| Flame | 4 | — (orbits) | every cycle |
| Blizzard shard | 5 | 12 | every cycle |
| Death-and-decay cloud | 6 | — (doesn't move) | every 24 cycles |
| Death-coil missile (default death knight) | 10 | 12 | every cycle |
| Whirlwind | 12 | 1 | every 3 cycles |

**Splash.** Several spells damage an area through the splash rule of `combat/damage.md` ("Splash on impact"), with
two differences: the effect kinds above (2, 4, 5, 6, 12) **ignore armour**, and the 7 × 7 scan also looks at the
air-layer unit of each tile, after its ground-layer unit. The unit that owns the effect (the caster, or the shielded
unit for flames) is never hit. So for a stored damage D:

| Distance from the effect to the unit's centre | Damage |
| --- | --- |
| up to 22 px | h + (r mod (h + 1)) with h = floor((D + 1) / 2) |
| 23 to 42 px | the same with D replaced by floor(D / 4) |
| 43 px or more | none |

One draw r is made per unit hit, in scan order. Walls on the effect's tile take D and the four neighbour walls take
floor(D / 4) (`movement/occupancy.md`, "Walls").

**Fireball.** Launched from the caster's centre toward the target tile's centre with stored damage **40**. On each step
it makes one draw (discarded), then moves 12 px, or 6 px if its remaining distance is already below 50. Once its
remaining distance is below 50 it counts its steps: on counts 8, 16, 24, 32 and 40 it **splashes** (damage 40) at its
position and makes a cosmetic explosion (kind 0x17); the count goes on to 41 (no splash), and on the step after count 41
it ends. Each splash follows the rule of the gryphon hammer and dragon breath, with which the fireball shares its
missile behaviour (`combat/damage.md`): it first needs a free effect slot (if there is none, there is no splash, sound
or explosion on that step), and the explosion is created in that slot with only kind and position set ("Cosmetic
effects"). No fireball trace shows this yet. It flies on past the target point, so the first explosion lands near the
point and the next four follow every 48 px along the same line beyond it. When it ends, it touches a wall on its final
tile with damage 40.

**Flame shield.** The bolt flies from the caster to the target unit's centre at 12 px per step, making one draw at
the start of every step. On the step after it arrives (remaining distance 0 or less), it creates **5 flames** on the
target unit and ends. Each flame circles the unit's centre through a fixed 51-point path (Data), one point per cycle,
starting at points 0, 10, 20, 30 and 40. The flames' lifetimes are 512, 524, 518, 521 and 527 cycles. A flame
counts its lifetime down by 1 each step and splashes with damage **4** whenever the new count's low 4 bits equal 1
(so about every 16 cycles). A flame ends when its count reaches 0, or as soon as the shielded unit is dying or gone.
The shielded unit is never hurt by its own flames; its neighbours, friend or foe, are.

**Blizzard shards.** Each of the 5 chains picks its landing point once, when it starts: the target tile's centre plus
((r1 mod 5) − 2) × 32 px on x and ((r2 mod 5) − 2) × 32 px on y (2 draws). Each shard of the chain starts at the landing
point minus (110, 170) px plus ((r mod 3) × 11 − 11) px on x, then on y (2 more draws), and flies to the landing point
at 12 px per step (14 to 16 steps), after an initial wait of 12 cycles. Its destination is the landing point from the
moment it is created, through the initial wait, not only once it starts flying (traced: `xhuman10-blizzard-600`, vectors
below). It makes one draw after each step that doesn't land. When it lands it splashes with damage **10**, and then, if
the chain has repeats left and a slot is free, it starts the next shard of the chain (repeats − 1, 2 draws) and makes
one more draw. A chain therefore lands **11 shards** on the same point. Damage per shard: 5 to 10 within 22 px, 1 or 2
from 23 to 42 px.

**Death and decay.** Each of the 5 clouds is placed at the target tile's centre plus ((r1 mod 5) − 2) × 32 px on x
and ((r2 mod 5) − 2) × 32 on y (2 draws), and doesn't move. Counting its first update as cycle 0, its first step
comes at cycle 18 (no effect). It then **splashes** with damage **10** at cycles 34, 58, 82, … , 250 (10 pulses, every
24 cycles). At its next step (cycle 274) it ends, and one cycle later it touches a wall on its tile with damage 10.

**Whirlwind.** Created with a lifetime of **800** and damage **4**. It starts at the target point's centre plus
((r & 127) − 64) px on x and on y (2 draws) and walks toward the target point at 1 px per step, one step every
3 cycles. On each step, if the lifetime is odd it splashes; the lifetime then goes down by 1, and the whirlwind ends
when it reaches 0 (touching a wall on its final tile with damage 4). When it reaches its goal exactly, it picks a new
goal at its position plus ((r & 127) − 64) px on x and on y (2 draws). It lasts 2400 cycles and splashes 400 times. A
new goal equal to its position is never reached, so it then drifts east until it ends.

**Cosmetic effects.** Kinds 0x0B, 0x15, 0x16, 0x17, 0x19 and 0x1A change nothing but slot use, and draw no random
number. Each plays its script once, with plain waits (haste and slow don't apply), and is removed at the script's step
done. Its first update ("When a new effect first runs") is in cycle U. Its start step comes A cycles later and restarts
its script with a wait of 1; it is removed during its update in cycle **U + A + 1 + W**, where W is the total of the
restarted script's waits. From that update on, its slot is free for any effect created later.

| Kind | Use | Script | A (cycles) | W (cycles) | Removed in cycle |
| --- | --- | --- | --- | --- | --- |
| 0x0B | rune marker | six frames with waits 3, 2, 3, 2, 3, 2 | 2 | 15 | U + 18 |
| 0x16 | spell marker (holy vision, slow, invisibility and others) | six frames with a wait of 3 each | 1 | 18 | U + 20 |
| 0x17 | explosion | sixteen frames with a wait of 3 each | 1 | 48 | U + 50 |
| 0x19 | cannon smoke (`combat/damage.md`) | four frames with a wait of 3 each | 11 | 12 | U + 24 |
| 0x15, 0x1A | impact effects (`combat/damage.md`, "Impact effects") | as given there | 1 | 12 | U + 14 |

Every cosmetic effect (kinds 0x0B, 0x15, 0x16, 0x17, 0x19, 0x1A and 0x1C) is created with only its kind and position
set: its destination is (0, 0), and it has no target, no shooter and damage 0, whoever creates it (a rune trap's
marker, a rune that goes off, a missile's impact effect, a cannon's smoke, a spell marker, an explosion). Nothing is
copied from the missile or the caster that makes it.

A kind 0x16 marker "on the target" (slow, invisibility, bloodlust, haste, unholy armour) is placed at the unit's pixel
position plus (floor(w / 2), floor(h / 2) − 4), with w × h the unit type's box size, as for kinds 8 and 9 below. Holy
vision and polymorph keep their own positions.

Kinds 0x13 and 0x14 would also be removed at step done, but their scripts loop and never reach one. Only the map
start creates them: they are the building fires in effect slots 0 and 1 (`core/unit-slots.md`, `ui/map-drawing.md`).

Kinds
8 (exorcism) and 9 (healing) are not quite cosmetic: they start at the unit's pixel position plus (floor(w / 2),
floor(h / 2) − 4), with w × h the unit type's box size, make one flight step of 12 px up and to the left (one draw),
and end on their next step by touching a wall on their final tile with damage 0, which still adds 1 to that wall's
damage counter (`movement/occupancy.md`, "Walls").

### When a new effect first runs

Effects are updated in ascending slot order after all units in phase 4, and a new effect takes the lowest free
effect slot.

- An effect created by a unit (a spell strike, the timers pass excepted) gets its **first update in the same cycle**.
- An effect created by another effect gets its first update in the same cycle if its slot comes after the creator's
  slot, otherwise in the next cycle.
- An effect created in the timers pass (rune effects) gets its first update in the next cycle.

The effect starts at its **start step**: in its first update if its kind's start animation has no wait, otherwise A
cycles later (table below). The start step puts the effect into its started state, so its picture shows from then on
(`ui/map-drawing.md`), and restarts its script with the kind's next animation and a wait of 1. Its first **step** comes
one cycle after the start step at the earliest (the cloud's own timing is given above, counted from its first update).
So a fireball cast at cycle C (A = 0) makes its first step and first draw at C + 1.

| Kinds | A (cycles) |
| --- | --- |
| 0x00 to 0x03, 0x07 to 0x0A, 0x0C to 0x12, 0x18, 0x1B | 0 |
| 0x15, 0x16, 0x17, 0x1A, 0x1C | 1 |
| 0x0B | 2 |
| 0x04, 0x13, 0x14 | 5 |
| 0x05, 0x19 | 11 |
| 0x06 | 18 (waits 3, 2, 3, 2, 3, 2, 3) |

A is the total of the waits in the kind's start animation (animation 0 of its script). All missiles have A = 0. The
blizzard shard's initial wait of 12 cycles and the death and decay cloud's first step at cycle 18 (above) already
include their start animations.

### Random draws

Creation draws follow `core/timing-and-rng.md`. A unit whose type has a decay rate and that is created during play
makes no facing draw; every other mobile unit does; every created unit makes the first-wait draw last.

| Spell | At the strike | Per effect step | Per unit hit |
| --- | --- | --- | --- |
| Holy vision, slow, invisibility, bloodlust, haste, unholy armour, flame shield (strike), fireball (strike), runes | 0 | — | — |
| Healing | 0 | 1 (the healing effect's one flight step) | — |
| Exorcism | 0 | 1 for each exorcism effect's flight step | 0 |
| Polymorph | 2: the critter's facing, then its first wait (after the kill, before the cosmetic effect) | — | — |
| Eye of Kilrogg | 1 (the eye's first wait), only if a tile was found | — | — |
| Raise dead | 1 per skeleton created (its first wait), in slot order, each before that skeleton's cosmetic effect | — | — |
| Fireball | 0 | 1 at the start of every step | 1 per unit hit by each of the 5 explosions |
| Flame shield | 0 | bolt: 1 at the start of every step, including the step that creates the flames; flames: 0 | 1 per unit hit by a flame's splash |
| Blizzard | 20: per chain, in chain order, 2 for the landing point then 2 for the first shard's start offset | 1 after each step that doesn't land | 1 per unit hit; then, if a next shard starts, 2 for its start offset and 1 more |
| Death and decay | 10: per cloud, in cloud order, 2 for its position | 0 | 1 per unit hit by each pulse |
| Whirlwind | 2: start offset, x then y | 0, except 2 (new goal, x then y) on the step that reaches the goal | 1 per unit hit by a splash |
| Death coil | 3 per kept victim, in order (the missile launch's damage draw and two scatter draws) | 1 at the end of each of the missile's flight steps, including its last | 0 |

On a step that both splashes and moves (whirlwind), the splash draws come first. On a fireball or bolt step the step
draw comes before any splash draws.

## Data

| Item | Value |
| --- | --- |
| Mana costs, orders 38 to 56 | 70, 6, 5, 4, 80, 100, 50, 200, 200, 25, 70, 50, 50, 100, 100, 50, 100, 200, 25 |
| Healing cap per cast | 40 HP |
| Timer values | invisibility 2000; unholy armour 500; bloodlust 1000; haste +1000 or set 1000; slow −1000; flame shield 500 |
| Holy vision offsets (tiles) | (0, 0), (0, −8), (0, +8), (+6, +4), (+6, −4), (−6, +4), (−6, −4); sight 9 each |
| Enhanced fog refresh after holy vision | 150 cycles |
| Raise dead | 50 mana each; squared tile distance ≤ 36 |
| Death coil | 5 × 5 tiles; budget 50 |
| Runes | 5 tiles; 50 slots; lifetime 2048; 50 damage; refund 40 per rune not placed |
| Fireball | damage 40; 5 explosions at counts 8 to 40 |
| Flame shield | 5 flames; damage 4; lifetimes 512 + 3 × {0, 4, 2, 3, 5} |
| Blizzard | 5 chains × 11 shards; damage 10; start offset (−110, −170) px |
| Death and decay | 5 clouds × 10 pulses every 24 cycles; damage 10 |
| Whirlwind | lifetime 800 steps of 3 cycles; damage 4; wander ±64 px |
| Caster mana regeneration | +1 per 40 cycles, cap 255 |
| Berserker regeneration | +1 HP per 240 cycles |
| Decay periods before HP loss | 255 |
| Effect speeds (px per step), kinds 0 to 28 | 16, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 8, 8, 12, 12, 16, 16, 0, 0, 0, 0, 0, 16, 0, 0, 12, 0 |

**Flame path** (pixel offsets from the shielded unit's centre, points 0 to 50):

| Points | x | y |
| --- | --- | --- |
| 0–9 | 1, 4, 7, 10, 13, 16, 19, 22, 25, 28 | −34, −34, −34, −33, −32, −31, −30, −28, −26, −23 |
| 10–19 | 30, 31, 32, 32, 32, 31, 30, 28, 25, 22 | −20, −17, −14, −11, −8, −5, −2, 1, 4, 6 |
| 20–29 | 19, 16, 13, 10, 7, 4, 1, −2, −5, −8 | 8, 9, 10, 11, 12, 12, 12, 12, 11, 11 |
| 30–39 | −11, −14, −17, −20, −23, −25, −27, −28, −28, −28 | 10, 8, 7, 5, 2, −1, −4, −7, −10, −13 |
| 40–50 | −27, −26, −24, −22, −19, −16, −13, −10, −7, −4, −1 | −16, −19, −22, −25, −27, −29, −31, −32, −33, −34, −34 |

After point 50 the path goes back to point 0.

**Default unit data used here:** flag bit 15 (undead): death knight 0x0B, Teron Gorefiend 0x15, Gul'dan 0x33,
skeleton 0x37. Flag bit 1 (air): 0x16, 0x23, 0x28, 0x29, 0x2A, 0x2B, 0x2D, 0x38. Flag bit 17 (caster): 0x0A, 0x0B,
0x0C, 0x0D, 0x15, 0x17, 0x18, 0x2C, 0x31, 0x33, 0x34. Decay: skeleton 100, eye 3, all others 0.

## Test vectors

| Input | Expected |
| --- | --- |
| Paladin with 100 mana heals a footman at 30 of 60 HP | missing 30, floor(100 / 6) = 16, heal 16: footman 46 HP, mana 4 |
| Paladin with 255 mana heals a footman at 0 missing HP | Nothing healed, no mana spent; the paladin becomes idle |
| Paladin with 255 mana heals a knight missing 60 HP | heal = min(60, 42, 40) = 40; mana 255 − 240 = 15 |
| Paladin with 100 mana, exorcism with a 40-HP skeleton alone on the target tile | Ring 0, first visit: damage min(25, 40) = 25, mana 0. Second visit of the same tile: damage 0 (applied, so the skeleton is still told it was attacked), mana 0. Ring 1: mana below 4, stop. Skeleton at 15 HP |
| Paladin with 255 mana, same skeleton | First visit: damage 40, skeleton dies, mana 255 − 160 = 95. Second visit: the tile is empty. Rings 1 to 3 continue while mana ≥ 4 |
| Flame shield on a unit whose flame-shield timer is 120 | Nothing; no mana spent; the caster is idle |
| Flame shield on a gryphon rider | Nothing; no mana spent (air unit) |
| Slow on a unit with speed timer +300 (hasted) | −700 |
| Slow on a unit with speed timer −200 | −1000 |
| Haste on a unit with speed timer −300 (slowed) | +700 |
| Haste on a unit with speed timer +400 | +1000 |
| Unholy armour on a grunt at 61 HP | 30 HP; unholy timer 500 |
| Unholy armour on a unit at 1 HP | stays 1 HP; timer 500 |
| Invisibility on a goblin sapper | It is killed in that cycle's timers pass |
| Runes, mana 200, target tile free, the north and east tiles hold units | 3 runes (centre, south, west); refund 80; mana 80 |
| Runes, all 5 tiles free but only 2 rune slots free | 2 runes (centre and north); refund 120 |
| Footman (60 HP) walks onto a rune | Rune used up; footman at 10 HP |
| Footman at 50 HP walks onto a rune | Footman killed |
| Unholy-armoured knight walks onto a rune | Rune used up; no damage |
| Raise dead, mana 120, three unused bodies at squared distances 4, 25 and 49 | Two skeletons, from the two nearer bodies (in slot order); mana 20; the third body is too far |
| Raise dead, mana 60, one body, but the caster's player is at its unit cap | No skeleton; the body stays unused; mana 10 |
| Death coil, candidates with 10, 30 and 25 HP | Order 10, 25, 30. Shares 10, 25, 15; the caster gains 50 HP (capped at its maximum) |
| Death coil, candidates with 60 and 5 HP | Order 5, 60. Shares 5 and 45; caster gains 50 |
| Death coil, no candidates | 100 mana spent; nothing else |
| Blizzard, caster with 60 mana, channelled | Two strikes (60 → 35 → 10); at the third strike 10 < 25, so the caster becomes idle. 10 chains, 110 shards in all |
| One blizzard shard lands 10 px from a footman | 5 to 10 damage (armour ignored) |
| Fireball cast at a point 200 px east of the caster's centre | Steps of 12 px until the remaining distance drops below 50 (13 steps: 200 → 44). Then 6-px steps; counts 1, 2, … from that step. Explosions on counts 8, 16, 24, 32 and 40 (steps 20, 28, 36, 44 and 52), at remaining distances 2, −46, −94, −142 and −190 (east of the point by that many px). Count 41 on step 53, and it ends on step 54 (still moving 6 px): 54 step draws |
| Eye of Kilrogg | Created next to the caster. Killed on its 828th timers pass |
| Skeleton raised | Killed on its 26400th timers pass |
| Caster at 254 mana | 255 after its counter next reaches 0; then no more gains |
| A mage is trained | It starts with 85 mana; 86 on its 40th timers pass |
| Holy vision, Enhanced, 64 × 64 map, target tile (10, 60) | Points (10, 60), (10, 52), (16, 56) and (4, 56) are revealed with sight 9 and get a cosmetic effect; (10, 68), (16, 64) and (4, 64) are skipped. The fog refresh countdown becomes 150 |
| Same, Classic | The same 4 areas are revealed. (16, 64) and (4, 64) reveal nothing but each gets a cosmetic effect (6 effects in all); (10, 68) is skipped. The refresh countdown is unchanged |
| Holy vision at tile (3, 30), either mode, large map | (−3, 34) and (−3, 26) are skipped; the other 5 points are used |
| Whirlwind at tile (10, 20); strike draws give 100 then 10 | Goal (336, 656); start (372, 602) (x + 36, y − 54). One 1-px step every 3 cycles; it splashes on steps 2, 4, 6, … (before moving). It reaches the goal on step 54 (after that step's splash), and then draws twice for a new goal: draws 64 then 0 give the goal (336, 592) |
| Same whirlwind, no units near it | No draws between the strike and step 54; 27 splashes by then; lifetime 746 after step 54 |
| Flame shield on a footman: the 5 flames at creation | Offsets from the footman's centre: (1, −34), (30, −20), (19, 8), (−11, 10), (−27, −16) (points 0, 10, 20, 30, 40) |
| Same, the flame that started at point 10, at its 5th step | Point 15: offset (31, −5) |
| Same, the flame that started at point 40, at its 11th step | Point 0 again (wraps after point 50): offset (1, −34) |
| Same, the first flame (lifetime 512) | Splashes on its steps 15, 31, 47, … , 511 (32 splashes); ends at step 512 |
| Same, the second flame (lifetime 524) | Splashes on its steps 11, 27, … , 523 (33 splashes); ends at step 524 |
| Death coil, candidates in scan order A (30 HP), B (30 HP), C (5 HP) | Order C, B, A. Shares C 5, B 30, A 15; the caster gains 50 |
| Polymorph on a footman | The critter makes 2 draws (facing, then first wait). The footman plays its death animation and leaves a body with the used mark; a later raise dead skips it |
| Raise dead with 120 mana over two unused bodies | 2 draws (one per skeleton), each body gets the used mark and still decays normally |
| Blizzard strike | 20 draws at the strike, before any shard flies |
| Fireball cast at cycle C | First update at C; first step and first draw at C + 1 |
| A spell puts a spell marker (0x16) on a unit in cycle C | First update in C; start step and showing from C + 1; removed in C + 20 |
| `xorc01-death-knight-2100` (oracle trace), n = 37: haste on ogre 1488 (type 0x07, box 42 × 42) at pixel (2848, 2944) | Effect kind 0x16 at (2869, 2961), destination (0, 0), target null, shooter null, damage 0 |
| `xorc12-ogre-mage-180` (oracle trace), n = 36: bloodlust on ogre 1547 (type 0x07) at pixel (3712, 288) | Effect kind 0x16 at (3733, 305), destination (0, 0), target null, shooter null |
| `xorc12-ogre-mage-180` (oracle trace), n = 77: runes are cast; five rune markers (kind 0x0B) are created | Effect slots 3 to 7 at (3600, 432), (3600, 400), (3632, 432), (3600, 464) and (3568, 432); each with destination (0, 0), target null, shooter null, damage 0 |
| Runes cast: a rune marker (0x0B) made in the timers pass of cycle C | First update in C + 1; start step in C + 3; removed in C + 19 |
| Map start, oracle trace Human01: the building fires in effect slots 0 and 1 | First update in the first start-up cycle; start step 5 cycles later, in the 4th normal cycle; never removed |
| Custom unit data gives a farm the caster flag and magic 0; the farm is placed by the map | Mana 0 until its 256th timers pass, then 1; +1 every 40 passes after that, up to 255 |
| Default data | No building gains mana from the timers pass |
| `xhuman10-blizzard-600` (oracle trace), n = 145: new shards in effect slots 4, 5 and 7, waiting | Destinations (2480, 2864), (2512, 2928) and (2576, 2800), each chain's landing point, unchanged through n = 160; they start moving at n = 157, 12 cycles after creation |

## Open questions

- Whether raise dead's marker on a new skeleton uses the same placement as the other kind 0x16 markers.
- Confirm the death coil order of equal-HP candidates in the running game: two 30-HP victims and one 5-HP victim
  should split the budget as the vector says (the second-found 30-HP victim gets its full 30).
- Units created while the map loads don't get the decay countdown. What happens to a decaying type placed in the map
  (for example a skeleton in a PUD) is not settled. Default: it never decays.
- A unit that would be killed by decay is spared in one case: a global setting is on and the unit carries a state
  flag. Neither has been identified. Default: never spared.
- Whether a used dead body is drawn: see `core/corpses.md` (default: drawn like any body).
- The inactive unit state skipped by the timers pass besides dying and remains. Default: the state that the unit
  update also skips.
- Whether the raise-dead scan can pick up a body in a slot that has already been released (the scan reads the body
  type and remains state only). Default: no; a released slot never qualifies.
