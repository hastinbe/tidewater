# Combat: damage, armour, attack timing and missiles

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), unit statistics from
  `unitdata.dat` and the PUD `UDTA` layout (`pudspec.txt`), and attack timing measured from `Rez\script.bin`. Not yet
  confirmed against the running game.
- **Changes:** 2026-10-11, round-2 errata: wall damage advances one stage per counter overflow; target-acquisition
  distance measure added; demolition blast terrain effects now in `movement/occupancy.md`.
  2026-10-11, round-3 errata: the 7 × 7 splash scan order is row by row from the top, left to right, with a tile's
  ground-layer unit before its air-layer unit (open question closed).
  2026-10-11, round-8 errata: new subsection "Missile flight" (speed, flight length, landing, impact delay and
  draws per kind, the computer players' wall exception, and the gryphon hammer and dragon breath, which splash along
  their path); two open questions closed.
  2026-10-12, round-10 errata: a target with the used mark gets no notification and no damage; the notification
  points to `ai/attack-ai.md`, "Call for help", and the under-attack alert is no longer described as part of it.
  2026-10-12, round-14 errata: the Dark Portal special case is defined (game kind, campaign entries, types).
  2026-10-12, round-15 errata: new paragraph "Impact effects" (the effect each missile kind leaves at its impact, its
  slot and its lifetime).
  2026-10-12, round-19 errata: cannon smoke is made only by destroyers, battleships and juggernaughts (never towers),
  at the missile's start point, in the lowest free effect slot after the missile's; a shot with no free effect slot
  creates nothing and draws nothing; cannon towers, like guard towers, fire without aim scatter.
- **Changes:** 2026-10-12, round-20 errata: two test vectors renumbered so effect slots 0 to 2 stay in use
  (`core/unit-slots.md`).
- **Changes:** 2026-10-12, round-23 errata: the Dark Portal special case names its three maps, and the two other
  shipped maps with a Dark Portal are noted as unprotected; four test vectors added.

## Behaviour

All quantities below are integers. "Draw" means one output `r` (0 to 32767) of the simulation generator in
`specs/core/timing-and-rng.md`. Unit statistics are the per-type `UDTA` values: basic damage, piercing damage, armour,
attack range, weapons upgradable, armour upgradable, missile weapon, and flags.

### Effective attack values

An attacker's **basic damage** is its type's basic damage, plus an upgrade bonus if the type's "weapons upgradable"
byte is set. The bonus is chosen by the first matching row:

| Attacker | Basic bonus |
| --- | --- |
| Sea unit (flag bit 3) | owner's ship-cannon level × 5 |
| Unit with flag bit 2 (ballista, catapult) | owner's siege level × 15 |
| Archer, axe thrower, ranger, berserker | none |
| Any other unit | owner's sword/axe level × 2 |

**Piercing damage** is the type's piercing damage plus:

- for archers, axe throwers, rangers and berserkers: the owner's arrow/spear level × 1;
- for a ranger whose owner has researched Marksmanship: the ranger's basic damage (3) moves into piercing. Basic becomes
  0 and piercing goes up by 3, so armour no longer reduces that part.

**Bloodlust:** while an attacker's bloodlust timer is running, both basic and piercing damage are doubled, after
upgrades.

**Cheat:** with the "invulnerability and extra damage" cheat on, units of human-controlled players deal basic 0 and
piercing 200. Computer-controlled units deal basic 0 and piercing 0, so they do no damage at all.

A target's **armour** is its type's armour, plus an upgrade bonus if the type's "armour upgradable" byte is set:

- sea units (flag bit 3): the target owner's ship-armour level × 5;
- other units: the target owner's shield level × 2.

Upgrade levels are counted per player as 0, 1 or 2.

### Damage roll

This is used for a melee hit and for a non-splash missile. Let B be the attacker's basic damage, P its piercing damage
and A the target's armour.

1. `b = max(0, B − A)`. If there is no target (a missile aimed at the ground), A is taken as 0.
2. `T = b + P`. If `T = 0` the damage is 0 and **no draw is made**.
3. `h = floor((T + 1) / 2)`, that is, half of T rounded up.
4. Make one draw r. The damage is `min(255, h + (r mod (h + 1)))`.

So the damage is spread evenly from roughly half of T up to T (`h` to `2h`). There is no minimum of 1: a unit with no
piercing damage hitting armour equal to or above its basic damage does 0.

### Applying damage

When damage D from an attacker reaches a target:

- Nothing happens if the target is already dying or has been removed.
- **Dark Portal special case:** in a single-player campaign game (not a network game, not a custom scenario) on
  campaign entries 32, 34 and 50 (`game/victory.md`, "Mission rules"), a hit on a type 0x65 (Dark Portal) does
  nothing, not even the notification, unless the attacker is type 0x18 (Khadgar). These are *Beyond the Dark Portal*
  human missions 3, 4 and 12 (`2XHum03`, `2XHum04`, `2XHum12`), each of which places one neutral Dark Portal. The
  portals placed by `2XOrc12` and `Human14` aren't protected.
- Nothing happens if the target has the used mark (`core/corpses.md`), for example a passenger inside a transport.
- Otherwise the target is first **notified that it was attacked**, even if D is 0 (`ai/attack-ai.md`, "Call for
  help": which attacker is recorded, and who comes to help). The under-attack alert is a separate step
  (`ui/sounds.md`, "Ready, work complete and under attack").
- If the target's unholy-armour timer is running, or D is 0, it takes no damage.
- If the target's HP ≤ D, it dies and the kill is credited to the attacker's owner. Otherwise its HP drops by D.

### How an attack delivers damage

An attacking unit plays its **attack script** (see "Attack timing" below). At the script's strike point:

- **Melee:** if the type's missile weapon is "none", the damage is rolled and applied to the current target at once.
- **Ranged:** otherwise a missile is launched. It starts from the attacker's centre and aims at the target's centre,
  or at the centre of the target tile if the attacker is attacking the ground.
  - **Non-splash missile:** the damage is rolled now, against the target's current armour, and stored in the missile.
    On impact it is applied to the unit that was the target at launch. A missile aimed at a tile with no unit damages a
    wall on that tile instead.
  - **Splash missile** (big cannon, catapult rock, ballista bolt, small cannon): it stores the attacker's **maximum
    damage**, which is basic plus piercing including upgrades (and bloodlust), capped at 255. No draw is made at
    launch for the damage.
  - **Aim scatter:** after the damage is set, every missile launched by a unit's attack script makes two more draws.
    The first moves the aim point `(r AND 7) − 3` pixels horizontally and the second does the same vertically, so each
    offset is from −3 to +4. Towers (guard and cannon) fire without this scatter.
  - **Flight length:** a missile's flight is at least its minimum flight distance (table below). It is measured as the
    larger of the horizontal and vertical pixel distance.
  - **Cannon smoke:** a big or small cannon missile launched by a unit's attack (destroyers, battleships and
    juggernaughts in the default data; never towers) is followed by a cosmetic effect of kind 0x19 at the missile's
    start point (the attacker's centre, not moved by the aim scatter), facing 0. It is created after the missile,
    damage and scatter are set, in the lowest free effect slot left, so its slot is after the missile's. With no slot
    left there is no smoke, and the missile still flies. Like the missile, it gets its first update in the strike
    cycle C (`spells/spells.md`, "When a new effect first runs"). It plays four frames with a wait of 3 each and is
    removed in cycle C + 13. It makes no sound and draws no random numbers.

Draws for one ranged shot, in order: one damage draw (non-splash only, and only if `T > 0`), then the horizontal
scatter, then the vertical scatter. If no effect slot is free for the missile, the shot creates nothing (no missile
and no smoke) and makes no draws at all.

### Missile flight

**Start.** A missile launched by a unit's attack in cycle C gets its first update in C, which only starts it
(`spells/spells.md`, "When a new effect first runs"). From C + 1 it makes one **flight step every cycle**; every
kind in the table below steps every cycle.

**Flight length D.** D is the larger of the horizontal and vertical pixel distances from the missile's start (the
attacker's centre) to its aim point **after** the aim scatter. If D is below the kind's minimum flight distance,
D becomes that minimum. The aim point isn't moved, so the missile then flies on past it. Shots by towers (guard and
cannon) use the same rule without the scatter and without the wall exception below.

**Computer players' wall exception.** If the minimum is more than 2 tiles (catapult rock, ballista bolt), the
attacker belongs to a computer-controlled player, and the attack's target tile carries a wall flag (0x04 or 0x08
in `movement/occupancy.md`), the minimum is 2 tiles (64 px) instead.

**Steps.** Each step moves the missile s pixels on the major axis, and s pixels or 0 on the minor axis, by the line
rule of `spells/spells.md` ("How flying effects move"), with s the kind's speed. The remaining distance starts at
D and goes down by s per step. The step on which it goes **below 0** is the **landing step**. So a flight has
N = floor(D / s) + 1 steps, landing at C + N, and the missile ends N × s pixels from its start on the major axis
(1 to s pixels past D).

**Impact.** The missile then stays where it is. Its impact (the hit on its target, or the splash) comes a fixed
number of cycles after the landing step: the impact delay in the table. The impact happens at the missile's
final position.

**Impact effects.** At every impact of these missile kinds, after the impact sound (`ui/sounds.md`) and after the
hit or splash, a cosmetic effect is created at the missile's final pixel position:

| Missile kind | Impact effect |
| --- | --- |
| 0x0D catapult rock, 0x0E ballista bolt | kind 0x15 (ballista impact) |
| 0x07 big cannon, 0x18 small cannon | kind 0x1A (cannon tower explosion) |
| 0x11 submarine missile, 0x12 turtle missile | kind 0x17 (explosion) |
| Every other kind | none |

- The effect is made whatever the damage, even 0, and whether or not anything was hit.
- It takes the lowest free effect slot. If there is none, no effect is made. The missile's own slot is freed only
  after this, so the effect never reuses the missile's slot.
- Its first update follows `spells/spells.md`, "When a new effect first runs" (it is created by another effect).
- It changes nothing in the game. It holds its slot until its script's step done, counted from the effect's
  restart at its first update: for kind 0x15, six frames with a wait of 2 each (12 cycles); for kind 0x1A, four
  frames with a wait of 3 each (12 cycles); for kind 0x17, sixteen frames with a wait of 3 each (48 cycles; the
  cosmetic effect of `spells/spells.md`, "Cosmetic effects").

**Draws.** Each flight step makes one draw, at the end of the step and after the move, including the landing
step. The draw is discarded. The catapult rock and the small cannon make one **more** draw on every step,
including the landing step, after which their total travel n × s (for step n) is still below 5 × ceil(D / 5). A
landing step's draws always come before the impact's draws, which happen in a later cycle. A non-splash missile's
impact makes no draw of its own (only what "Applying damage" makes).

| Value | Missile | Speed (px per step) | Extra draws | Impact delay (cycles after landing) |
| --- | --- | --- | --- | --- |
| 0x00 | lightning | 16 | no | 11 |
| 0x07 | big cannon | 12 | no | 1 |
| 0x0A | touch of death | 12 | no | 16 |
| 0x0D | catapult rock | 8 | yes | 5 |
| 0x0E | ballista bolt | 8 | no | 1 |
| 0x0F | arrow | 12 | no | 1 |
| 0x10 | axe | 12 | no | 1 |
| 0x11 | submarine missile | 16 | no | 1 |
| 0x12 | turtle missile | 16 | no | 1 |
| 0x18 | small cannon | 16 | yes | 1 |
| 0x1B | demon fire | 12 | no | 5 |
| 0x01, 0x02 | gryphon hammer, dragon breath | 12, then 6 | no | see the next entry |

**Gryphon hammer and dragon breath.** Each step makes **one draw at its start**, before moving. The step moves
12 px, or 6 px if the remaining distance is already below 50 at the start of the step, by the same line rule; the
remaining distance goes down by the amount moved. It keeps falling below 0, and the missile flies on past the aim
point. Every step that leaves the remaining distance below 50 is **counted**:

- if the count has already passed the **limit**, the missile ends on this step (after moving);
- otherwise the count goes up by 1, and when the new count is a multiple of 8 the missile **splashes** at its
position.

The limit is 25, or 40 if the stored damage is exactly 40. So with the default data (stored damage 8 to 16) the
count runs 1 to 26, splashes come on counts 8, 16 and 24, and the missile ends on the step after count 26. One
cycle after the end step, the stored damage is applied to the unit that was the target at launch, as for a
non-splash missile: wherever the hammer is, with no further armour and no draw. If it had no target, a wall on its
final tile takes the damage.

**Splashes** use the splash rule ("Splash on impact") with the stored damage as that rule's D (not the flight length) and these differences: **armour
isn't subtracted**, so every unit within 42 px is hit, even with a base of 0; air-layer units are included,
each after its tile's ground-layer unit; the firing unit is never hit. One draw is made per unit hit, after the
step's own draw. Walls take D and floor(D / 4) as usual. A unit can be hit by several splashes of the same missile
and by the final hit.

### Splash on impact

When a splash missile lands with stored damage D at its impact pixel:

1. **Walls.** The wall on the impact tile takes D, and the walls on the four orthogonal neighbour tiles take
   `floor(D / 4)`. Each wall tile keeps a damage counter from 0 to 19. Damage reaches the wall as a single byte, so
   values of 256 or more wrap first. A hit adds `max(1, floor((damage mod 256) / 4))`. If the new counter would
   exceed 19, the wall **advances one stage** (intact, then damaged, then rubble) and the counter goes back to 0; the
   excess is lost. An intact wall therefore needs two overflows to become rubble. When a tile becomes rubble, each
   orthogonal neighbour that is still intact advances to damaged. The full rule is in `movement/occupancy.md`,
   "Walls".
2. **Units.** The unit on the impact tile is considered first. Then every ground-layer unit in the 7 × 7 tiles centred
   on the impact tile is considered. Air units are considered only when a small-cannon missile was fired at an air
   unit. Each unit is hit at most once per impact. The firing unit is never hit. **Friendly units are hit too.**
3. For each unit considered, let dx and dy be the pixel offsets from the impact point to the unit's centre, and
   `d² = max(dx², dy²)`:
   - if `d² ≥ 1792` (`|d| ≥ 43` px), the unit is not hit;
   - if `d² ≤ 511` (`|d| ≤ 22` px), the base is D;
   - otherwise, from 23 to 42 px, the base is `floor(D / 4)`.
4. The target's armour, including upgrades, is subtracted from the base. If the result is below 1 the unit is not hit
   and no draw is made. Note that this subtracts armour from the whole amount, piercing part included, unlike the
   single-target roll.
5. Otherwise, with `v` the result, `h = floor((v + 1) / 2)`. Make one draw r. The damage is `h + (r mod (h + 1))`,
   kept to its low 8 bits. Only `v = 255` can reach 256, and that wraps to 0 damage. It is then applied as in
   "Applying damage".

Units are processed in the order found, so draws are made in that order: the impact-tile unit first, then the 7 × 7
block **row by row from the top, each row left to right**. When air units are included, each tile's ground-layer unit
comes before its air-layer unit. A unit already considered (the impact-tile unit) isn't considered again.

### Attack timing (cooldown)

There is no separate cooldown statistic. The **rate of fire is the length of the attack script loop** in
`Rez\script.bin`, and damage happens at the script's strike point inside that loop. A unit that keeps attacking repeats
the loop. Between loops it may re-evaluate its target and order. Each script's "wait" steps scale with haste and
slow:

- Haste halves the wait, rounding down, with a minimum of 1.
- Slow doubles the wait, kept to 8 bits.

The timings below are measured from `script.bin`. "Strike" is the cycle on which damage or launch happens, counting
the first cycle of the attack as 1. "Loop" is the number of cycles between strikes.

| Unit types | Strike | Loop |
| --- | --- | --- |
| Footman, grunt, peasant, peon, knight, paladin, attack peasant or peon, Danath, Khorgath, Grom, Turalyon, Lothar, Uther | 11 | 25 |
| Ogre, ogre-mage, Dentarg, Cho'gall | 8 | 25 |
| Dwarven demolition squad, goblin sapper | 5 | 25 |
| Archer, ranger, Alleria | 12 | 65 |
| Axe thrower, berserker, Zuljin | 11 | 65 |
| Mage, death knight, Teron Gorefiend, Khadgar, Gul'dan | 12 | 40 |
| Ballista | 27 | 200 |
| Catapult | 2 | 200 |
| Oil tanker (either side) | 2 | 130 |
| Destroyer, transport (either side) | 2 | 120 |
| Battleship, juggernaught | 2 | 230 |
| Gnomish submarine, giant turtle | 37 | 100 |
| Gryphon rider, Kurdan and Sky'ree | 38 | 190 |
| Dragon, Deathwing | 26 | 190 |
| Skeleton | 10 | 35 |
| Daemon | 34 | 42 |
| Guard tower (either side) | see note | 60 |
| Cannon tower (either side) | see note | 151 |

Towers have no strike point in their scripts. They fire from their attack behaviour on a schedule not yet confirmed,
and the loop length is given only as the script's period.

### Range

- A unit can attack a target within its type's attack range, in tiles.
- Rangers get +1 range once Longbow is researched, and berserkers get +1 once Lighter Axes is researched. No other
  upgrade changes range.
- The Ranger Scouting and Berserker Scouting upgrades change those units' sight, not their range.
- **Target acquisition** measures distance as the larger of the horizontal and vertical tile distances from the
  searcher's tile to a reference tile of the target: a mobile target's own tile, or for a building its nearest edge
  column or row, or its middle one (the exact rule is in `orders/orders.md`, "Automatic target acquisition"). The
  acquisition range check and minimum-range check use this measure. Whether a strike in progress uses the same
  measure is still open.

## Data

### Upgrade amounts per level

| Upgrade line | Adds per level | Applies to |
| --- | --- | --- |
| Arrows / spears | +1 piercing | Archer, axe thrower, ranger, berserker |
| Swords / axes | +2 basic | Other land units with upgradable weapons |
| Shields | +2 armour | Land units with upgradable armour |
| Ship cannons | +5 basic | Sea units with upgradable weapons |
| Ship armour | +5 armour | Sea units with upgradable armour |
| Siege (catapult/ballista) | +15 basic | Units with flag bit 2 |
| Marksmanship | basic 3 moves into piercing | Ranger |
| Longbow / Lighter Axes | +1 range | Ranger / berserker |

One more upgrade line exists with an amount of 10 per level. Its use hasn't been found.

### Missile weapon kinds

The values come from the `UDTA` missile weapon byte. The minimum flight distance is in tiles of 32 px.

| Value | Missile | Splash | Minimum flight |
| --- | --- | --- | --- |
| 0x07 | big cannon | yes | 2 |
| 0x0D | catapult rock | yes | 3 |
| 0x0E | ballista bolt | yes | 3 |
| 0x18 | small cannon | yes | 2 |
| 0x01, 0x02 | gryphon hammer, dragon breath | along the path (see "Missile flight") | none |
| 0x0F, 0x10, 0x11, 0x12, 0x1B and others | arrow, axe, submarine, turtle, demon fire | no | none |
| 0x1D | none (melee) | not applicable | not applicable |

### Unit statistics used in the test vectors

These are the default `unitdata.dat` values.

| Unit | Basic | Piercing | Armour | Range |
| --- | --- | --- | --- | --- |
| Footman / grunt | 6 | 3 | 2 | 1 |
| Archer | 3 | 6 | 0 | 4 |
| Ranger | 3 | 6 | 0 | 4 |
| Ballista / catapult | 80 | 0 | 0 | 8 |
| Guard tower | 4 | 12 | 20 | 6 |

## Test vectors

`r` is the draw used for the damage. Where it isn't given, the full range is listed.

| Input | Expected |
| --- | --- |
| Footman (6/3) hits footman (armour 2), no upgrades | b = 4, T = 7, h = 4; damage = 4 + (r mod 5), range 4 to 8 |
| Same, r = 0 | 4 |
| Same, r = 346 | 4 + (346 mod 5) = 5 |
| Footman with swords 2 (basic 10) hits footman with shields 2 (armour 6) | b = 4, T = 7, damage 4 to 8 (the upgrades cancel out) |
| Archer (3/6) hits footman (armour 2) | b = 1, T = 7, damage 4 to 8 |
| Archer with arrows 2 hits footman | piercing 8, b = 1, T = 9, h = 5, damage 5 to 10 |
| Ranger with Marksmanship hits footman | basic 0, piercing 9, T = 9, damage 5 to 10 |
| Bloodlusted grunt (12/6) hits footman (armour 2) | b = 10, T = 16, h = 8, damage 8 to 16 |
| Unit with basic 2, piercing 0 hits armour 2 | T = 0, damage 0, no draw made |
| Basic 1, piercing 0 hits armour 0 | T = 1, h = 1, damage 1 + (r mod 2): 1 or 2 |
| Unholy-armoured target takes any hit | HP unchanged; it is still notified it was attacked |
| Target with 5 HP takes 5 damage | dies (HP ≤ damage) |
| Catapult (80/0, no upgrades) splash, footman 10 px from impact | v = 80 − 2 = 78, h = 39, damage 39 to 78 |
| Same, footman 30 px from impact | base 20, v = 18, h = 9, damage 9 to 18 |
| Same, footman 43 px from impact | not hit, no draw |
| Catapult with siege 2 (basic 110) splash on an intact wall tile with counter 0 | centre wall takes 110 (+27): it becomes damaged with counter 0, not destroyed. Each neighbour wall takes 27 (counter +6) |
| Splash base 5 at 30 px (quarter = 1) on armour 2 | v < 1, not hit, no draw |
| Arrow fired by a unit | draws: damage (if T > 0), then x scatter, then y scatter; each scatter is (r AND 7) − 3 |
| Footman attacking continuously, no spells | hits on cycles 11, 36, 61, … of the attack |
| Hasted footman | each wait 3 becomes 1, 5 becomes 2, 10 becomes 5; loop = 1 + 1 + 1 + 1 + 2 + 5 = 11, strike on cycle 5 |

Missile flight (C is the cycle of the strike; the scatter draws are taken as giving 0 offset, so the aim point is the
target's centre):

| Input | Expected |
| --- | --- |
| Arrow, D = 100 px | Draws at C: damage (if T > 0), x scatter, y scatter. Steps at C + 1 to C + 9, with remaining 88, 76, 64, 52, 40, 28, 16, 4, −8: landing at C + 9, 108 px from the start. One draw at the end of each step (9). Impact at C + 10: the stored damage is applied, with no draw of its own. 12 draws in all |
| Catapult rock, D = 160 px | Draws at C: x scatter, y scatter. 5 × ceil(160 / 5) = 160. Steps at C + 1 to C + 21 (remaining 0 after step 20, −8 after step 21): landing at C + 21, 168 px out. Steps 1 to 19 make 2 draws each (travel 8 to 152, below 160) and steps 20 and 21 make 1 each: 40 flight draws. Splash at C + 26, with 1 draw per unit hit |
| Catapult rock, D = 103 px | N = 13 (104 px); 5 × ceil(103 / 5) = 105, so all 13 steps, the landing step included, make 2 draws: 26 |
| Ballista bolt, D = 100 px | N = 13, landing at C + 13 (104 px out), 13 flight draws, splash at C + 14 |
| Lightning (mage), D = 100 px | N = 7, landing at C + 7 (112 px out), 7 flight draws, hit at C + 18 |
| Human-controlled catapult attacks a wall tile, D = 70 px before the minimum | Minimum 96, so D = 96: N = 13, the rock lands 104 px out (34 px past the wall's centre) |
| Same, computer-controlled | Minimum 64, so D = 70: N = 9, the rock lands 72 px out |
| Gryphon rider (basic 0, piercing 16) strikes footman A (armour 2) at cycle C. A's centre is 96 px due east of the gryphon's centre. Footman B's centre is 20 px south of A's. No upgrades. Draws: launch r1 with r1 mod 9 = 4, scatter draws with r AND 7 = 3 | Stored damage 8 + 4 = 12; aim = A's centre, D = 96. Steps at C + 1 to C + 3 move 12 px (remaining 84, 72, 60). Step 4 moves 12 (remaining 48): count 1, 48 px out. Then 6 px a step: count k at step 3 + k, at 42 + 6k px |
| Same, step 11 (count 8, 90 px out) | Step draw, then a splash at 6 px west of A's centre. A (6 px): base 12, h = 6, damage 6 + (r mod 7), 6 to 12. Then B (max(6, 20) = 20 px): the same, 6 to 12. A is drawn for first if the splash point is on A's tile |
| Same, step 19 (count 16, 138 px out) | A and B are both 42 px away on x: base floor(12 / 4) = 3, h = 2, damage 2 to 4 each, A first (row order) |
| Same, step 27 (count 24, 186 px out) | Nobody within 42 px: no splash draws |
| Same, steps 28 and 29 | Counts 25 and 26 |
| Same, step 30 (C + 30) | Step draw, move to 204 px; count 26 is past 25, so it ends |
| Same, C + 31 | A takes the stored 12, with no draw |
| Same, totals | Draws: 3 at launch, 30 step draws, 4 splash draws = 37. A takes 20 to 28 in all, and B 8 to 16 |

Impact effects:

| Input | Expected |
| --- | --- |
| A catapult rock lands and splashes; effect slots 0 to 4 in use, slot 5 free; the rock is in slot 3 | No impact sound; the splash; a kind 0x15 effect in slot 5 at the rock's position; the rock's slot 3 is freed. Slot 5 comes after slot 3, so the effect's first update is in the same cycle: `misc\explode` plays at its tile. It is freed after 12 cycles of waits |
| A ballista bolt with stored damage 0 lands | No impact sound (damaging only); the kind 0x15 effect is still made, and plays `misc\explode` at its first update |
| A battleship's big cannon lands; slot 3 free, the missile in slot 6 | `misc\explode` (impact); splash; the kind 0x1A effect in slot 3. Slot 3 comes before slot 6, so its first update, and its `misc\explode`, are in the next cycle |
| A small cannon lands with every effect slot in use except the missile's own | No effect is made (the missile's slot is freed only afterwards); no sound at all |
| A submarine missile lands | `misc\explode` (impact); a kind 0x17 effect; no further sound |

Cannon smoke and launch slots:

| Input | Expected |
| --- | --- |
| A battleship strikes in cycle C; effect slots 0 to 2 in use, 3 and up free | Missile in slot 3, smoke (kind 0x19) in slot 4 at the battleship's centre, facing 0. Both first update in C; the smoke is removed in C + 13 |
| A destroyer strikes; slots 3 and 5 in use, 4 and 6 free | Missile in slot 4, smoke in slot 6 |
| A destroyer strikes; only one effect slot free | Missile in that slot; no smoke; the two scatter draws as usual (the small cannon is a splash missile, so no damage draw) |
| A juggernaught strikes; no effect slot free | Nothing created; no draws for this shot |
| An archer strikes; no effect slot free | Nothing created; no damage draw and no scatter draws |
| A human cannon tower fires at a ship | Small cannon missile, no scatter draws, no smoke; cannon sound |
| A catapult or ballista strikes | Missile and scatter as usual; no smoke |
| Smoke updated and removed in cycle C + 13; later in that cycle a higher-slot effect creates a new effect | The new effect may take the smoke's old slot |
| *Beyond the Dark Portal* human mission 12 (entry 50), single player, a knight hits the Dark Portal | Nothing: no damage, no notification |
| Same, Khadgar (type 0x18) hits it | Damage applied as usual |
| *Beyond the Dark Portal* orc mission 12 (entry 51), a grunt hits the Dark Portal | Damage applied as usual |
| `2XHum12` played as a custom scenario | Damage applied as usual (the check needs a campaign game) |

## Open questions

- Whether a strike in progress uses the acquisition distance measure, and whether range is checked again at the
  strike point.
- When towers fire relative to their 60- and 151-cycle scripts, and whether a tower's shot is always a missile.
- Spell damage (fireball, death coil, blizzard, death and decay, whirlwind, flame shield, exorcism, runes) is covered
  separately. Runes deal a flat 50.
- Demolition squads and sappers: the terrain part of their blast (rock, forest and walls) is specified in
  `movement/occupancy.md`, "Rock". The damage to units on the 8 neighbouring tiles is still to be specified.
- What the extra upgrade line with 10 per level is used for.
