# Core: timing and random numbers

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), game data (`gluAll.tbl` speed
  names, `multmode.bin` mode names), and arithmetic checks of the generator. Not yet confirmed against the running game.
- **Changes:** 2026-10-11, round-2 errata: the setup-shuffle generator also picks sound variants during play. The
  open question on unit slot order is answered by `core/unit-slots.md`; the construction pass is placed in phase 4
  as `economy/build-train.md` states.
  2026-10-11, round-3 errata: going idle makes no draw; the `(r AND 15) + 44` wait belongs to computer players' idle
  workers; fleeing makes 2 draws.
  2026-10-11, round-3 errata: phase 1 corrected. An invisible unit is detected by its owner only (Classic) or its
  owner's shared-vision set (Enhanced), not by allies as such; submarine detection needs a complete detector and is
  shared through shared vision in Enhanced mode.
  2026-10-11, round-7 errata: the victory check runs every 201 cycles, first at cycle 2 (its countdown is
  reloaded to 200 without being lowered in that cycle), not every 200.
  2026-10-11, round-8 errata: the draw table gains rows for missile flight steps (all kinds, and the gryphon hammer
  and dragon breath); the area-spells row is replaced by a row pointing to `spells/spells.md`, "Random draws".
  2026-10-12, round-19 errata (reviewer addition): the launch rows exclude cannon towers as well as guard towers, and
  a shot that finds no free effect slot makes no draws.
  2026-10-12: the setup-shuffle generator also picks a non-campaign game's music track (`ui/music.md`).
  2026-10-12: the game-saved draw happens only for saves in an Enhanced game in a network session (`game/save-load.md`).
  2026-10-12, round-30 errata: the draw table gains the idle look-around and bob rows (replacing "critter wandering,
  ship bobbing"); the two start-up cycles run before the deadline and counter are reset, as full cycles with draws.
  2026-10-12, round-31 errata: the draw table gains a creation row for a computer land unit's post choice
  (`ai/attack-ai.md`), which comes before the first-wait draw.
  2026-10-12, round-32 errata: the fog refresh and victory countdowns count the start-up cycles. Their first runs
  are the first and second start-up cycles, and later runs are named by cycle counter value. The open question on
  "cycle 1" for the fog refresh is closed.

## Behaviour

### Simulation cycles

The simulation advances in fixed **cycles**. Everything that changes game state (orders, animation scripts, missiles,
timers, AI) happens inside a cycle; drawing and input polling happen between cycles. All durations in other specs are
in cycles unless stated otherwise.

There are two pacing modes. The player chooses between them in multiplayer as "Enhanced" and "Classic".

- **Enhanced** applies to single player, campaign, skirmish, and Enhanced multiplayer. Each speed has one fixed cycle
  length (table below).
- **Classic** applies to Classic multiplayer and to the attract-mode demo. Each speed has a three-cycle pattern of
  lengths that repeats. Classic offers 7 speeds. A stored 9-level speed maps onto them as 0,0,1,2,3,4,5,6,6, so the
  two slowest settings share a pattern and so do the two fastest.
- **The demo** always runs at the "Faster" enhanced length (26 ms) or at classic row 5.

### Scheduler

The scheduler keeps a **deadline** in milliseconds on the system tick clock.

1. When a game starts, two cycles run at once (the **start-up cycles**). Then the deadline is set to now and the
   cycle counter to 0, so the first normal cycle ends with the counter at 1. The start-up cycles are full cycles in
   every other respect: units update and draw.
2. On each frame, if the deadline has passed, up to **3** cycles run back to back. After each cycle the deadline
   moves forward by the current cycle length, and the loop stops early once the deadline is in the future again.
3. The deadline is reset to now (dropping any backlog) when any of these happen:
   - the speed setting changes;
   - the deadline is more than **500 ms** away from now, in either direction;
   - the game resumes after a network stall.
4. While the game is paused, cycle slots still elapse on schedule and the cycle counter still increases, but no game
   state changes. In single player, while the game window is inactive, cycle slots elapse with no state change and
   no counter increase. Multiplayer games keep simulating in the background.
5. In multiplayer, when a cycle has to wait for other players' commands, the deadline is recomputed once the commands
   arrive. If the wait lasted under 100 ms, the deadline moves forward by half a cycle length only. If it lasted
   longer, the deadline becomes now plus a speed-dependent multiple of half a cycle length. The network turn length
   is still an open question.

### Order of work inside one cycle

Each cycle does the following, in this order. Later specs refer to these as **phases**.

1. **Detection.** Each unit's set of detecting players is worked out again:
   - An **invisible** unit (Invisibility timer running) is detected only by its owner in Classic mode. In Enhanced
     mode it is detected by its owner's shared-vision set (nobody, if the owner isn't one of players 0 to 7).
     Alliance alone doesn't count.
   - A **submarine** is detected by its owner and by every player that has a living submarine-detecting unit within
     6 tiles on both axes. In Enhanced mode that unit must also be complete, and detection is also shared through
     shared vision.
   - Every other unit is detected by everyone.

   The full rule is in `vision/fog.md`, "Detection (phase 1)".
2. **Computer players.** Each computer-controlled player runs one step of its AI script.
3. **Fog refresh counter.** A counter counts down from 100 (101 in Classic). It starts at 1 and counts every cycle,
   the two start-up cycles included, so the first refresh happens in the first start-up cycle. Then it happens every
   100 cycles (101 in Classic): in the normal cycles that end with the cycle counter at 99, 199, … (Classic: 100,
   201, …; this follows from the period, as no Classic run was traced). On a refresh cycle each unit's sight is
   re-applied to the fog during phase 4.
4. **Units.** Every unit is updated in slot order (`core/unit-slots.md`):
   - its sight is re-applied if this is a refresh cycle;
   - its per-unit countdowns are advanced;
   - its animation script is advanced (see the animation-scripts spec);
   - its current order is advanced.
   
   Then all **missiles** are advanced. Then the **construction pass** advances buildings under construction
   (`economy/build-train.md`). Then the **timers pass** runs over units: spell durations, regeneration, mana,
   decay of summoned units, and runes.
5. **Visibility** for the local player is recomputed.
6. **Minimap** is refreshed every 40 cycles.
7. **Fog** bookkeeping for the cycle is closed.
8. **Victory and defeat** are checked every 201 cycles, first in the second start-up cycle (`game/victory.md`).
9. The **cycle counter** increases by 1. This also happens on paused cycles (see the scheduler, rule 4).

### Random number generator

All simulation randomness comes from one shared generator. Units, missiles, spells and AI all draw from it in the
order they run in the cycle above. Replays and lockstep multiplayer depend on every client making the same draws in the
same order.

Let the state `s` be an unsigned 32-bit integer.

- **Step:** `s ← (s × 22695477 + 1) mod 2^32`. The multiplier is `0x015A4E35`.
- **Output:** after stepping, the draw is `r = floor(s / 65536) mod 32768`. This is bits 16 to 30 of the new state, a
  value from 0 to 32767.
- **Ranged draw** of an integer from `lo` to `hi` inclusive, made with one draw `r`:
  `lo + floor((hi − lo + 1) × r / 32768)`.
- Other uses take `r mod n` or `r AND mask` directly. Each spec says which form it uses.

**Seeding:**

- **Single player:** the seed is the current time as whole seconds since 1970-01-01 UTC, taken when the game starts.
  Single-player slot setup re-seeds the same way just before it chooses the human player's slot, and makes its slot
  draws from that seed.
- **Multiplayer:** the host sends the seed in the game-start message, and every client loads it before cycle 0.
- The same seed also seeds the platform C library's generator. The simulation never draws from that one; it is used
  only by interface and sound effects, which must not touch the simulation generator.

**Setup shuffles:** shuffling start positions and player colours at game setup uses a separate generator with the common
recurrence `t ← t × 1103515245 + 12345`. Shuffles happen only at game setup. During play the same generator picks sound
variants, for example one of four chop sounds each time a worker chops (`economy/harvest.md`, "Lumber"), and the music
track of a game that isn't a campaign mission, at game start and when the disco cheat is turned off (`ui/music.md`). No
simulation decision found so far draws from it, so the simulation generator's sequence is unaffected.

### What draws random numbers

These are the known simulation consumers. Each draw happens at the moment the event happens, in the phase listed.

| Event | Phase | Draws | Use |
| --- | --- | --- | --- |
| Unit created (mobile, except summoned units created during play) | where created | 1 | Initial facing = `r AND 7` |
| Building created | where created | 1 | A stored value `r mod 6` (variation; use not yet known) |
| Computer player's mobile land unit created, post choice off the map | where created | 2 | Post near the unit, x then y (`ai/attack-ai.md`, "Unit goals"); before the next row's draw |
| Any unit created | where created | 1 | Adds `r AND 7` cycles to the first animation wait. This draw always comes last of the creation draws. |
| Melee hit lands | units | 0 or 1 | Damage roll (combat spec); no draw when the damage total is 0 |
| Non-splash missile launched | units | 0 to 3 | Damage roll at launch (none when the damage total is 0), then 2 scatter draws for missiles fired by a unit's attack (not by towers, guard or cannon). None at all if no effect slot is free for the missile (combat spec) |
| Splash missile launched by a unit's attack (not by a tower) | units | 0 or 2 | Target scatter; none if no effect slot is free for the missile (combat spec) |
| Splash missile lands | missiles | 1 per unit hit | Damage roll for each unit in the blast (combat spec) |
| Computer player's idle worker finds no job (no harvest, build or repair) | units | 1 | Wait of `(r AND 15) + 44` cycles, 44 to 59, before it looks again (the routine is to be specified with the AI) |
| Unit flees from its attacker | units | 2 | Flee tile offset `(r mod 8) − 2`, x first, then y (`orders/orders.md`, "Fleeing") |
| Idle look-around: a unit in order 2 or 13, or in order 7 when it reaches the idle logic, each time that logic runs (every 5 cycles for a unit at rest), except catapults, ships and fliers | units | 1, plus 1 when a critter wanders | Turn or wander (`orders/orders.md`, "Idle look-around") |
| Bob: a ship or flier at the same moments, when its bob countdown is 0 | units | 1 | Countdown `(r AND 7) + 8`; the bob value flips (`orders/orders.md`, "Idle look-around") |
| Missile flight step (unit attacks and towers; all kinds except hammer and breath) | missiles | 1 at the end of the step, landing step included; catapult rock and small cannon: 1 more while their travel is below 5 × ceil(D / 5) (combat spec) | Discarded |
| Gryphon hammer or dragon breath step | missiles | 1 at the start of the step, then 1 per unit hit by a splash on that step (combat spec) | Discarded; splash damage |
| Spell effects | units or missiles | as `spells/spells.md`, "Random draws", lists | Scatter, start offsets, wander, splash damage |
| Computer player decisions | computer players | varies | AI choices |
| Cheat entered in multiplayer with cheats disabled | when processed | 1 | Discarded |
| Game saved, Enhanced game in a network session (`game/save-load.md`) | when processed | 1 | Discarded; other saves make no draw |

A replay or desync check must reproduce the last two rows too, even though they look like no-ops.

## Data

### Enhanced speeds

The names are the game's own, from `gluAll.tbl`.

| Level | Name | Cycle length (ms) | Cycles per second |
| --- | --- | --- | --- |
| 0 | Slowest | 80 | 12.50 |
| 1 | Even Slower | 60 | 16.67 |
| 2 | Slower | 53 | 18.87 |
| 3 | Slow | 46 | 21.74 |
| 4 | Normal | 40 | 25.00 |
| 5 | Fast | 33 | 30.30 |
| 6 | Faster | 26 | 38.46 |
| 7 | Even Faster | 20 | 50.00 |
| 8 | Fastest | 13 | 76.92 |

### Classic speeds

Each row is the pattern of lengths for three cycles in a row; the pattern then repeats.

| Row | 9-level settings mapped here | Pattern (ms) | Mean (ms) | Cycles per second |
| --- | --- | --- | --- | --- |
| 0 | 0, 1 | 60, 60, 60 | 60.00 | 16.67 |
| 1 | 2 | 50, 50, 60 | 53.33 | 18.75 |
| 2 | 3 | 40, 50, 50 | 46.67 | 21.43 |
| 3 | 4 | 40, 40, 40 | 40.00 | 25.00 |
| 4 | 5 | 30, 30, 40 | 33.33 | 30.00 |
| 5 | 6 | 20, 30, 30 | 26.67 | 37.50 |
| 6 | 7, 8 | 20, 20, 20 | 20.00 | 50.00 |

### Periodic events, in cycles

| Event | Period |
| --- | --- |
| Fog refresh | 100 (Enhanced), 101 (Classic); first in the first start-up cycle, then at counter 99 (Classic 100) |
| Minimap refresh | 40 |
| Victory check | 201; first in the second start-up cycle, then at counter 201 |
| Mana regeneration (+1, max 255) | 40 |
| Berserker regeneration (+1 HP) | 240 |

### Generator constants

| Item | Value |
| --- | --- |
| Multiplier | 22695477 (`0x015A4E35`) |
| Increment | 1 |
| Output | bits 16 to 30 of the new state (15 bits) |
| Setup-shuffle generator | multiplier 1103515245, increment 12345 |

## Test vectors

Outputs are listed in draw order; each draw steps the state first.

| Input | Expected |
| --- | --- |
| Seed 0, 6 draws | 0, 346, 130, 10982, 1090, 11656 |
| Seed 0, state after 6 draws | `0xAD889112` |
| Seed 1, 6 draws | 346, 130, 10982, 1090, 11656, 7117 |
| Seed `0x12345678`, 6 draws | 5470, 22030, 16144, 7932, 18879, 26633 |
| Seed `0x68E77800`, 6 draws | 28283, 3278, 20871, 28228, 18503, 1342 |
| Seed 0, ranged draw 0..1, 6 times | 0, 0, 0, 0, 0, 0 |
| Seed 0, ranged draw 1..4, 6 times | 1, 1, 1, 2, 1, 2 |
| Enhanced Normal, 1000 ms of real time with no stalls | 25 cycles |
| Classic row 1, 9 cycles | 480 ms (3 × (50 + 50 + 60)) |
| Frame arrives 130 ms after the deadline at 40 ms per cycle | 3 cycles run; the deadline is still 10 ms in the past |
| Frame arrives 600 ms late | Deadline reset to now; at most 3 cycles run, backlog dropped |
| Oracle trace Human01 (`oracle/trace-format.md`), clock 946684800: state after setup 0xF55E08A8 | First start-up cycle: no draw. Second: 1 draw (an idle look-around), state 0x262AFAC9. First normal cycle: counter 1, state 0xF2F9299E |
| Human01, Enhanced, no input, 700 cycles (research-side run that logs each refresh and check) | Fog refresh in trace records n = 1, 101, 201, 301, 401, 501, 601 (first start-up cycle, then counter 99, 199, …). Victory check in n = 2, 203, 404, 605 (second start-up cycle, then counter 201, 402, 603). Neither runs in any other record |

Seed 1 gives the same sequence as seed 0 shifted by one draw, because one step from 0 gives a state of 1.

## Open questions

- How the multiplayer host picks the seed. It is probably the time, as in single player, but this isn't confirmed.
- On a reset, whether the cycles due that frame still run before the deadline moves to now, or the reset replaces them.
  The two choices differ by up to 3 cycles of latency, which is invisible to players but matters for exact replays.
- Whether the Classic three-cycle pattern restarts at the first phase after a speed change.
- Exact draw counts for each area spell.
- What the building creation value `r mod 6` is used for.
- How the setup-shuffle generator turns its state into a choice (which bits, and the shuffle method).
