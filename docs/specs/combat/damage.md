# Combat: damage, armour, attack timing and missiles

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), unit statistics from
  `unitdata.dat` and the PUD `UDTA` layout (`pudspec.txt`), and attack timing measured from `Rez\script.bin`. Not yet
  confirmed against the running game.
- **Changes:** 2026-10-11, round-2 errata: wall damage advances one stage per counter overflow; target-acquisition
  distance measure added; demolition blast terrain effects now in `movement/occupancy.md`.

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
- **Dark Portal special case:** on three Beyond the Dark Portal campaign maps, a Dark Portal takes damage only from
  Khadgar. This doesn't apply in multiplayer.
- Otherwise the target is first **notified that it was attacked**, even if D is 0. This feeds its reaction and its
  owner's under-attack alert.
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
    offset is from −3 to +4. Guard towers fire without this scatter.
  - **Flight length:** a missile's flight is at least its minimum flight distance (table below). It is measured as the
    larger of the horizontal and vertical pixel distance.
  - **Cannon smoke:** a big or small cannon shot also spawns a cosmetic effect at the muzzle. It doesn't draw random
    numbers.

Draws for one ranged shot, in order: one damage draw (non-splash only, and only if `T > 0`), then the horizontal
scatter, then the vertical scatter.

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

Units are processed in the order found, so draws are made in that order: the impact-tile unit first, then the scan in
tile order. The scan order within the 7 × 7 block is still an open question.

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
| 0x0F, 0x10, 0x11, 0x12, 0x01, 0x02, 0x1B and others | arrow, axe, submarine, turtle, hammer, breath, demon fire | no | none |
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

## Open questions

- Whether a strike in progress uses the acquisition distance measure, and whether range is checked again at the
  strike point.
- When towers fire relative to their 60- and 151-cycle scripts, and whether a tower's shot is always a missile.
- The scan order of the 7 × 7 splash block. This affects draw order when several units are hit.
- What a missile's flight time is in cycles. The research points to `ceil(flight length / 5)` cycles, about 5 px per
  cycle, but this isn't confirmed.
- A computer-player rule may shorten the minimum flight distance to 2 tiles when certain target tiles are involved.
  Not yet understood.
- Spell damage (fireball, death coil, blizzard, death and decay, whirlwind, flame shield, exorcism, runes) is covered
  separately. Runes deal a flat 50.
- Demolition squads and sappers: the terrain part of their blast (rock, forest and walls) is specified in
  `movement/occupancy.md`, "Rock". The damage to units on the 8 neighbouring tiles is still to be specified.
- What the extra upgrade line with 10 per level is used for.
