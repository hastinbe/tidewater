# Core: unit animation scripts

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** inspection of `Rez\script.bin` from the game data, with its instruction meanings established by static
  study of the 2.02 program (research notes under `research/notes/`). Timings below are computed from the data. Not
  yet confirmed against the running game, except the frame rules of the round-33 errata (oracle trace Human04).
- **Changes:** 2026-10-11, round-2 errata: when an animation's first instruction runs depends on how it was started.
  2026-10-12, round-33 errata: starting an animation never changes the frame; the events that do are listed.
  2026-10-12, round-35 errata: new rule "Facing a tile" (eight-way, on tiles, a building faced at its top-left tile plus
  (1, 1)), with trace vectors.
  2026-10-12, round-42 errata: warships (sea units that aren't submarines) turn broadside to strike, by a table of the
  direction to the target and the current facing; `orc09-naval` and `human12-7000` vectors.
  2026-10-13, round-56 errata: the strike does nothing while a command given to the unit is waiting to take over;
  `orc14-25000` vector.
- **Changes:** 2026-10-16 (round 78, B-038 combat consolidation, `ERRATA-combat-consolidation.md`, reviewed; change C1,
  traced): the hasted attack loop is the script's loop, not the rate of fire: a hasted unit that isn't a flyer is held
  to its attack cooldown (`combat/damage.md`, "Attack timing (cooldown)").

## Behaviour

Every unit and missile runs a small **animation script** from `Rez\script.bin`. Scripts are game state, not just
visuals. They decide:

- how far a unit moves each cycle, and when it has finished a step;
- when an attack lands (the strike point) and how often (the loop length);
- how long work actions take, such as chopping;
- the sprite frame shown.

So the timing of movement and combat follows from this data.

### Running a script

Each unit holds a **script position**, a **wait counter** (8 bits), its current **animation number** and its
**frame**.

- **Starting an animation** looks up the script for (unit type, animation number), points the position at its first
  instruction and sets the wait counter to 1. When the first instruction runs depends on how the animation was
  started:

  | Situation | First instruction runs |
  | --- | --- |
  | Ordinary start (wait 1) | On the unit's next update |
  | A pending order takes over | The wait is overwritten with 3, so on the third cycle after the takeover. The 3 is not adjusted for haste or slow |
  | A new unit is created | The wait is 1 plus a random 0 to 7 cycles (see the timing-and-rng spec) |
  | Switch to the move animation to take a step, or back to the order's own animation on arrival | In the same cycle |

  The movement spec (`movement/movement.md`, "Which first wait applies when") uses the same table.

  Starting an animation, in any of these ways or as an immediate order, sets the script position and the wait and
  **never changes the frame**. The frame changes only when the script runs a frame instruction (including the
  instructions run at once by the table's last row), or on these events: a worker fells a tree (frame 0,
  `economy/harvest.md`); a unit goes into or comes out of a gold mine or oil platform (that building's frame,
  `economy/harvest.md`); a building's construction look changes, or a building upgrade starts or ends
  (`economy/build-train.md`). So a unit keeps the frame its last animation left until one of these happens.
- **Each cycle,** during the unit update phase, the wait counter decreases by 1. If it is still above 0, nothing else
  happens this cycle. When it reaches 0, instructions run one after another until one of them sets a new wait.
- **The "step done" signal:** the instruction that marks the end of a step (see the table) sets the wait to 1 and
  reports to the unit's order logic that a step has finished. The order logic uses this to decide what to do next:
  pick the next tile, stop, retarget, or switch animation. Scripts are loops that begin with this instruction, so the
  signal arrives once per loop.
- **Halting** sets the wait to 0. The next decrement wraps it to 255, so a halted script sits still for 256 cycles and
  then carries on with the next instruction, unless a new animation is started first. A new animation is expected to start
  well before then, but this isn't confirmed.
- **Waits and speed spells.** A unit's haste/slow state is a signed timer: positive means hasted, negative means
  slowed, and 0 means normal. Depending on the instruction, a wait of n is adjusted like this:
  - **Hasted:** `max(1, floor(n / 2))`.
  - **Slowed:** `(2 × n) mod 256`.
  - A plain wait ignores both. Other wait kinds react to haste only, to slow only, or to both (see the table).
  - The "carry" wait adds 1 to n first if the unit is carrying gold (whether lumber counts too is still
    open), then applies haste and slow.

### Movement by scripts

Move instructions add a signed count times the unit's **facing direction vector** to the unit's pixel position. The
vector depends on the instruction: either a 1-pixel vector or a 2-pixel vector. Facing 0 is north and the values go
clockwise:

| Facing | 0 N | 1 NE | 2 E | 3 SE | 4 S | 5 SW | 6 W | 7 NW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| dx (1-px) | 0 | +1 | +1 | +1 | 0 | −1 | −1 | −1 |
| dy (1-px) | −1 | −1 | 0 | +1 | +1 | +1 | 0 | −1 |
| dx, dy (2-px) | double the above | | | | | | | |

A diagonal step moves the same count on both axes, so a diagonal tile crossing covers 32 px on each axis in the same
number of cycles as a straight one.

**Facing a tile.** Wherever a spec says a unit faces, or turns to face, a tile or a target (placement, entering a
mine or depot, felling, repair, point spells, and turning to strike), the new facing comes from this rule. Let dx and
dy be measured from tile A (the unit's own tile) to tile B, with ax = |dx| and ay = |dy|:

1. If ax < ay (mostly vertical): north (dy < 0) or south (dy ≥ 0) when ax < max(1, floor(ay / 2)); otherwise the
   diagonal.
2. If ax ≥ ay (mostly horizontal, ties included): west (dx < 0) or east (dx ≥ 0) when ay < max(1, floor(ax / 2));
   otherwise the diagonal.
3. The diagonal is NE, SE, SW or NW by the signs, with dx ≥ 0 counting as east and dy ≥ 0 as south.

So a diagonal is chosen only when the smaller distance is at least half the larger one (rounded down, and at least
1), and a unit facing its own tile faces east (2). B is a target building's top-left tile plus (1, 1), any other
target unit's own tile (for a moving unit, the tile it has claimed, `movement/movement.md`), or the target tile when
there is no target unit. Both ends are always tiles, never pixel positions, even while either unit is between tiles.
**Warships turn broadside to strike.** When a **sea unit that isn't a submarine** (unit flag bit 3 "sea unit" set
and bit 6 "is a submarine" clear, in the PUD specification) decides to strike, including an attack on the ground
(`combat/attack-ground-demolish.md`), it doesn't face its target. It takes the direction D from its tile to B by the
rule above, and then picks its new facing from D and its **current** facing with the table below. The new facing puts
the target on one of its sides. The table is applied at every strike decision, so a ship turns at most once per
decision. Submarines use the ordinary rule.

| D (to the target) | Current 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 (N) | 6 | 2 | 2 | 2 | 2 | 6 | 6 | 6 |
| 1 (NE) | 7 | 7 | 3 | 3 | 3 | 3 | 7 | 7 |
| 2 (E) | 0 | 0 | 4 | 4 | 4 | 4 | 0 | 0 |
| 3 (SE) | 1 | 1 | 1 | 1 | 5 | 5 | 5 | 5 |
| 4 (S) | 2 | 2 | 2 | 2 | 2 | 6 | 6 | 6 |
| 5 (SW) | 7 | 7 | 3 | 3 | 3 | 3 | 7 | 7 |
| 6 (W) | 0 | 0 | 4 | 4 | 4 | 4 | 0 | 0 |
| 7 (NW) | 1 | 1 | 1 | 1 | 5 | 5 | 5 | 5 |

Rows 0 and 4 differ only for current facing 0 (6 against 2).

## Data

### File layout of `script.bin`

All values are little-endian. An offset is a byte offset from the start of the file.

| Bytes | Content |
| --- | --- |
| 0 to 219 | 110 × u16: for each unit type (PUD unit numbering), the offset of that type's animation table |
| 220 onward | the same for missile kinds |
| animation table | u16 offsets, one per animation number |
| scripts | byte instructions |

**Animation numbers:**

| Number | Animation |
| --- | --- |
| 0 | none / stand still (halts) |
| 1 | death |
| 2 | still |
| 3 | move |
| 4 | attack |
| 5, 6 | type-specific work (for example chopping, or the building and construction states) |

### Instruction set

Each instruction is one opcode byte followed by zero to two argument bytes. "s8" is a signed byte, "u8" unsigned and
"u16" a little-endian offset.

| Opcode | Arguments | Effect | Stops running this cycle? |
| --- | --- | --- | --- |
| 0 | none | Step done: wait = 1 and signal the order logic | yes |
| 1 | u8 n | Wait n cycles (plain) | yes |
| 2 | none | Halt (wait = 0, see above) | yes |
| 3 | u16 offset | Continue at offset | no |
| 4 | u8 f | Set frame f | no |
| 5 | s8 d | Move d × 1-px direction | no |
| 6 | s8 d, u8 f | Move d × 1-px direction, then set frame f | no |
| 7 | u8 n | Wait n, adjusted for haste and slow | yes |
| 8 | u8 n | Wait n, adjusted for haste only | yes |
| 9 | u8 n | Wait n, adjusted for slow only | yes |
| 10 | none | Play the unit's pending sound (no game-state effect) | no |
| 11 | none | Strike: the current order's hit action (attack damage or launch, chop, and so on), unless a command is waiting to take over (below) | no |
| 12 | u8 n | Wait n (+1 if carrying), adjusted for haste and slow | yes |
| 13 | s8 d | Move d × 2-px direction | no |
| 14 | s8 d, u8 f | Move d × 2-px direction, then set frame f | no |
| 15 | none | Unit changes owner to the neutral player (player 15) | no |
| 16 to 255 | none | Ignored (one byte) | no |

Setting a frame has no game-state effect; the sprite only needs redrawing if the frame actually changed.

**No strike while a command waits.** A unit carries a mark from the moment it is given a command (by a player, a
computer player's AI routine, or its own order logic) until that command's order takes over. While the mark is set,
the strike instruction does **nothing**: no damage roll, no draw, no launch, no strike reveal, no spell effect, and the
attack cooldown isn't started again. The take-over clears the mark, so a strike after it works as before (traced:
`orc14-25000`, vectors below).

### Measured timings

All cycle counts are at normal speed.

- **Move loop** is the cycles from one "step done" signal to the next, and **Distance** is how far that loop moves.
- **Attack loop** is the cycles between strikes. **Strike** is the cycle of the first strike, counting the first
  cycle of the animation as 1.
- **Haste** and **slow** columns give the move loop, then the attack loop. The hasted attack loop is the script's
  loop only: the attack cooldown, which haste doesn't change, holds a hasted unit that isn't a flyer to one strike every
  attack loop + 1 cycles (`combat/damage.md`, "Attack timing (cooldown)"; traced, `xorc01-death-knight-2100`).

| Unit types | Move loop | Distance | Haste | Slow | Attack loop | Strike |
| --- | --- | --- | --- | --- | --- | --- |
| Footman, grunt, Grom, Danath, Khorgath | 16 | 32 px | 12, 11 | 31, 49 | 25 | 11 |
| Peasant, peon (and attack variants) | 16, or 22 if carrying | 32 px | 12, 10 | 31, 49 | 25 | 11 |
| Knight, paladin, Turalyon, Lothar, Uther | 12 | 32 px | 10, 11 | 23, 49 | 25 | 11 |
| Ogre, ogre-mage, Dentarg, Cho'gall | 12 | 32 px | 12, 11 | 23, 49 | 25 | 8 |
| Archer, ranger, Alleria | 16 | 32 px | 12, 33 | 31, 129 | 65 | 12 |
| Axe thrower, berserker, Zuljin | 16 | 32 px | 12, 31 | 31, 129 | 65 | 11 |
| Mage, Khadgar | 18 | 32 px | 12, 18 | 35, 79 | 40 | 12 |
| Death knight, Teron Gorefiend, Gul'dan | 18 | 32 px | 10, 18 | 35, 79 | 40 | 12 |
| Dwarven demolition squad | 14 | 32 px | 10, 11 | 27, 49 | 25 | 5 |
| Goblin sapper | 14 | 32 px | 12, 11 | 27, 49 | 25 | 5 |
| Ballista | 32 | 32 px | 17, 99 | 63, 399 | 200 | 27 |
| Catapult | 32 | 32 px | 17, 100 | 63, 399 | 200 | 2 |
| Skeleton | 20 | 32 px | 12, 18 | 39, 69 | 35 | 10 |
| Critter | 48 | 32 px | 17 | 95 | none | none |
| Oil tanker | 32 | 64 px | 32, 65 | 63, 259 | 130 | 2 |
| Transport, destroyer | 32 | 64 px | 32, 60 | 63, 239 | 120 | 2 |
| Battleship, juggernaught | 53 | 64 px | 32, 115 | 105, 459 | 230 | 2 |
| Gnomish submarine, giant turtle | 43 | 64 px | 32, 49 | 85, 199 | 100 | 37 |
| Gnomish flying machine, goblin zeppelin | 20 | 64 px | 20 | 39 | none | none |
| Gryphon rider, Kurdan and Sky'ree | 24 | 64 px | 13, 96 | 47, 379 | 190 | 38 |
| Dragon, Deathwing | 24 | 64 px | 13, 96 | 47, 379 | 190 | 26 |
| Daemon | 24 | 64 px | 15, 22 | 47, 83 | 42 | 34 |
| Eye of Kilrogg | 8 | 64 px | 8 | 15 | none | none |

Units that move 64 px per loop are all ships and flyers, plus the daemon and the eye. They use the 2-pixel
instructions and signal "step done" only every two tiles.

## Test vectors

| Input | Expected |
| --- | --- |
| Footman starts its move animation facing east at pixel x = 100 | step done on its first update; then x = 103 one cycle later; 32 px moved after 16 cycles; step done again on cycle 17 |
| Footman move, per-cycle pixel steps (1-px vector × count) | 3, 3, 3, 2, 3, 2, 3, 3, 3, 2, 3, 2 (sum 32), separated by waits 2, 1, 2, 1, 1, 1, 2, 1, 2, 1, 1 plus the 1-cycle step-done wait |
| Footman attack, normal | frames shown for 3 cycles each, strike on cycle 11, next strike on cycle 36 |
| Wait 3, hasted | 1 |
| Wait 1, hasted | 1 (minimum) |
| Wait 100, slowed | 200 |
| Wait 130, slowed | 4 (8-bit wrap) |
| Carry wait 2, carrying and hasted | floor(3 / 2) = 1 |
| Halt, then no new animation | the unit's next instruction runs 256 cycles later |
| Opcode 20 | skipped as a one-byte no-op |
| Human04 (oracle trace), peon 1531 fells its tree at n = 1913 | Frame 25 at n = 1912, 0 at n = 1913 (the felling, not the take-over) |
| Human04, peon 1548 reaches the mine at n = 89 | n = 88: order 23, animation 3, frame 20. n = 89: order 25, animation 3, frame 0, wait 3 (the arrival switch ran the order's own animation; the take-over kept that frame) |
| Human04, peon 1548 returning gold, order 25 takes over at n = 374 | Frame 0 at n = 374 to 380 (left by the arrival switch), then 5, 10, 0, 15, 20 as the move script runs; order 26 at n = 399 with frame 0 |
| Facing a tile: `orc4` slot 1584 (type 0x02) comes out of building 1577 at n = 1273 onto (9, 12); its target, gold mine 1599, has top-left (12, 1), so B = (13, 2) | dx = +4, dy = −10: facing 0 (north) |
| Facing a tile: `human01-basics` n = 116, footman 1598 on (26, 10) (pixel (832, 320)) strikes grunt 1592, which has claimed (26, 11) (pixel (845, 365)) | dx = 0, dy = +1: facing 4 (south), not the pixel direction |
| Facing a tile: (76, 78) → (78, 78); (87, 80) → (89, 78) | 2; 1 |
| Facing a tile: (0, 0) → (3, 1); → (4, 1); → (−2, −5); → (−2, −6); (5, 5) → (5, 5) | 3; 2; 7; 0; 2 |
| `orc09-naval-1500` (oracle trace), n = 270: destroyer 1497 (type 0x1F) at (66, 68) in order 9, facing 1; its target, destroyer 1506, at (66, 64), due north | Facing 2 at the strike decision |
| `human12-7000` (oracle trace), n = 5988: destroyer 1417 (type 0x1E) at (18, 124) in order 16, facing 2; target zeppelin 1503 at (20, 120) (dx +2, dy −4: D = 1) | Facing 3 |
| A battleship facing 0, target due south | Facing 2 |
| A submarine facing 2, target due north | Facing 0 (the ordinary rule) |
| `orc14-25000` (oracle trace, Enhanced), n = 12307: computer paladin 1456 (type 0x0C, owner 6) at (10, 114) in order 12 reaches its strike (animation 4, frame 40); since n = 12301 its target is paladin 1433 (owner 6), from the caster AI's healing command, which hasn't taken over yet; before that its target was peon 1448 (type 0x03, owner 0), now at (12, 115) with 11 HP | No strike: no draw, and 1448 keeps 11 HP |
| A footman at its strike point is given a stop command earlier in the same cycle (static study) | No damage and no draw |

## Open questions

- How the order logic reacts to each "step done" signal for each order. The movement and order specs cover this.
- Whether ships and flyers really occupy only even tile coordinates, as the 64 px step suggests.
- What the missile-kind scripts after byte 220 drive in missile flight: speed, impact frame, and so on.
- Whether carrying lumber also triggers the +1 on the carry wait.
- Which work animations (5 and 6) each order uses, and the strike action for each order (chop, mine, repair, build).
