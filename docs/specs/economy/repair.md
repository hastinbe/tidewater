# Economy: repair

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the message table in the
  game data (`network.tbl`), the unit flags in the public PUD specification text, and oracle traces of the running
  game where a rule says so (the repair counter and the end of a repair at full hit points).
- **Changes:** 2026-10-11, round-4 errata: a lost target or attacker is one that is no longer valid or no longer
  **detected** by the owner; fog over it doesn't matter (was "no longer visible").
  2026-10-11, round-5 errata: fleeing while repairing splits computer players (don't flee) from every other player
  (flee), replacing "computer-controlled" and "human-controlled".
  2026-10-22, round 97 (second B-040 audit, `ERRATA-audit2-economy.md`, reviewed by a spec-review session,
  2026-10-22): the repair counter is the target's construction pass counter (a building) or its stored goal's x (a
  mobile unit), so a building completed by construction pays on every run (traced, RP1); a repair that stops for lack
  of resources or a lost target ends with the same pending idle as at full hit points (static study); repair on a
  unit that can't be repaired keeps it as the move's target (static study). Open question on the shared value closed.
  2026-10-22, round 98 (`ERRATA-round98-traces.md`, reviewed by a spec-review session, 2026-10-22): the transport
  row is traced (RP8: 20 repair runs, each paid, the goal x 110 to 90); a docked transport isn't moved by the changed
  goal. Open question "Repairing a transport" closed.
  2026-10-22, round 100 (`ERRATA-round100-traces.md` and `ERRATA-audit2-economy.md` erratum 4, reviewed by a
  spec-review session, 2026-10-22): repair on a building still under construction holds construction back one pass
  per paid run (traced, RP10; it follows from the counter being P); the hall-to-keep upgrade row is traced (RP9).
  New open question on a run in the cycle right after a construction step.
  2026-10-22, round 101 (`ERRATA-round101-traces.md` section 2, trace request RA4; reviewed by a spec-review session,
  2026-10-22): a run on a site in the cycle a step would fall in moves the step one cycle later, and a run in the
  cycle right after a step makes the same step again (both traced, RP11 and RP12, as the construction pass is
  written: no rule change); a site filled by two repairers (RP13). Open question "A run right after a construction
  step" closed.

## Behaviour

A worker repairs with **command 27**, which carries a target unit. Repair is order **27** (`orders/orders.md`): it
uses the attack animation and an arrival radius of 1 tile.

### The command

| Target | Result |
| --- | --- |
| A building, or a unit whose type has the transport flag (bit 10 of the PUD unit flags) | Order 27 on that target |
| Any other unit | Order 3 **with that unit kept as its target**: the worker walks toward the unit where it is now, as a move with a target unit does (`orders/orders.md`, "Walking orders"; order 3's radius, 0) (static study, round 97) |
| No target | Order 3 to the tile, with no target |

In the default unit data only the two transports (types 0x1C and 0x1D) carry flag bit 10, so with that data **only
buildings and transports can be repaired**. Every other unit, including the other ships and the siege weapons, gets
order 3: the worker just walks toward it, and keeps heading for it if it moves.

### Each run of the order's logic

The logic runs once per loop of the worker's attack animation, so every **25 cycles** without haste or slow (10 with
haste, 49 with slow; `core/animation-scripts.md`). Each run, first match wins:

1. **Attacked:** a target or attacker that is no longer valid or no longer detected by the worker's owner (fog
   doesn't matter; see `vision/fog.md`, "What uses vision") is forgotten first. Then, if the worker still has an
   attacker, the reaction of `orders/orders.md` ("Reacting to an attacker") runs. Its outcome decides whether the run
   goes on:

   | Outcome of the reaction | Run |
   | --- | --- |
   | The attacker is forgotten (it is no longer valid, its owner is not playing, or the worker's type can't attack) | Goes on to step 2 |
   | The attacker is a worker: the repairer fights back (command 8 on it) | Ends |
   | Worker of any player other than a computer player (`orders/orders.md`, "Terms"): it flees (order 27 is one of the orders that flee) | Ends |
   | Computer player's worker: it doesn't flee, because it isn't idle; the attacker is kept | Goes on to step 2 |

   So a computer player's worker keeps repairing while it is hit, unless its attacker is a worker.
2. **No target** (the target died or is no longer valid): the worker stops repairing with the **pending idle** of
   step 4.
3. **Not arrived:** the worker is given the repair again, which moves it toward the target.
4. **Target at full hit points:** the worker stops repairing: idle (order 2) becomes its **pending** order by the
   common path, with the Enhanced resume of a remembered patrol or attack-move (`orders/orders.md`, "Resuming patrol
   and attack-move"; the resume here is static study). With nothing to resume, the record of that run shows order 2, still animation, wait 3, target
   none (traced: `orc9-orc09-r97-built-farm-repair-goals-2400` n = 1799, `xorc10-xorc10-r96-repair-saveload-airesearch-aiworkers-game-6000`
   n = 631 and 650, `human13-invisible-work-hits-900` n = 511).
5. **The player has no gold or no lumber** (either stock at 0 or below): the worker stops repairing with the same
   pending idle as in step 4 (static study, round 97; not the immediate idle). If the worker
   belongs to the local player, a message is shown: when lumber is out, the one saying that lumber is needed to
   finish the repair (entry 73, 0x49, of `network.tbl`, counting from 1); otherwise the one saying that gold is
   needed (entry 74, 0x4A). Lumber is checked first.
6. **Repair:**
   - the worker turns to face the target (a building is faced at its top-left tile plus (1, 1));
   - the target gains **4 hit points**, but not above its maximum;
   - the target's **repair counter** (below) goes up by 1. Then, if it is **2 or more**, it drops by 2 and the player
     pays **1 gold and 1 lumber**.
   - The order continues.

### Cost and rate

- The repair counter belongs to the **target**, not the worker, and it is kept between repairs. Every repairer of
  the same target adds to the same counter. It is a signed 16-bit value.
- **It isn't a value of its own** (round 97). On a **building** it is the construction pass counter P of
  `economy/build-train.md` ("The construction pass"), which nothing resets when construction ends. On a **mobile
  unit** (a transport, the only mobile units repair can reach in the default data) it is the x of the unit's stored
  goal tile (the tile its last command sent it to). So the counter at the first repair is:

  | Target | Counter at the first repair run |
  | --- | --- |
  | Building placed by the map (never under construction) | 0 |
  | Building completed by construction, build-speed cheat off | 12 × (T + 2), T = the type's build time: 1,224 for a farm (T = 100), 744 for a scout tower (T = 60), 2,424 for a barracks (T = 200) |
  | Building completed while the build-speed cheat was on | P's last value, which needn't be a multiple of 12 (static study) |
  | Building upgraded after it was completed (scout tower to guard tower, hall to keep or castle) | Unchanged by the upgrade: the value the building had before it. Jobs (training, research) don't change it either |
  | Transport | The x of its stored goal tile (traced, round 98: RP8) |

  A building completed by construction shows 12 × (T + 2) whether or not it was damaged while being built
  (`economy/build-train.md`, "Construction": it completes 12 passes after P reaches the total 12 × (T + 1)).
  Traced: undamaged farms at 1,224 in `orc9-orc09-carry-chop-goals-1400` and `xhuman5-xhuman05-worker-all-out-goals-3000`,
  a farm damaged while being built at 1,224 in `orc9-orc09-r97-built-farm-repair-goals-2400`, and computer
  players' barracks (2,424) and orc guard towers in `xhuman9-aigoals-goals-8630-8830`. The two guard towers (slots
  1400 and 1430) were built as scout towers and upgraded at n = 4682 and 4209 (`xhuman9-10000`); both still hold 744,
  the scout tower's value. The barracks in slot 1408 ran eight training jobs and still holds 2,424. A hall upgrade is
  the same kind of job, traced in round 100 (RP9): a map-placed town hall repaired during its upgrade to a keep keeps
  the alternating counter of a map-placed building (1, 0, 1, 0), the counter is 0 when the upgrade ends, and the keep
  goes on with the same alternation. (The counter happened to be 0 at the upgrade's end, so the trace doesn't
  separate "kept" from "set to 0" for the hall; the guard towers above show a nonzero value kept.)
- **On a building still under construction** (round 100, traced: RP10). The counter is then the live pass counter P
  of "The construction pass" (`economy/build-train.md`), which goes up by 1 every cycle. A run adds 1 to P and, as P
  is 2 or more after the first pass, takes 2 off and is paid. Repair runs in the units phase, before the construction
  pass of the same cycle (`core/timing-and-rng.md`, phase 4), so over that cycle P doesn't move: the pass's +1 only
  makes up the run's net −1. So:
  - every paid run delays every later construction step, and so the site's own hit-point gains and its completion, by
    one cycle (one pass);
  - every run is paid, at 1 gold and 1 lumber per 4 hit points, as for a building completed by construction;
  - the repair's own hit points can fill the site before construction does. The next construction step then finds
    the hit points at the maximum and sets P to the total 12 × (T + 1) ("The construction pass", step 4), and the
    building completes 12 passes later, as a site that reaches its maximum by construction does;
  - a run that comes when P is 1 short of a multiple of 12 moves that cycle's step to the next cycle (traced, round
    101: RP11);
  - a run that comes in the cycle right after a step brings P back to that same multiple of 12 at the cycle's pass,
    and the pass makes the step **again**: one more share of the hit points (the next step's share, since the step
    count goes on) and the shown progress worked out again (traced, round 101: RP12). Both follow from "The
    construction pass" as written.
- **So the cost depends on the target.** A building **completed by construction** starts far above 2: each paid run
  takes 1 off (the +1 then −2), so it pays on **every** run until the counter is below 2, which takes over 1,200 runs
  for a farm (about 4,800 hit points of repair, more than any default building's maximum): in practice **1 gold and
  1 lumber per run, per 4 hit points**, for its whole life. A building **placed by the map** pays every second run:
  **1 gold and 1 lumber per 8 hit points**, the counter going 1, 0, 1, 0. The last run before full health can add
  fewer than 4 hit points and still pays if the counter says so.
- **A transport's stored goal changes** as it is repaired: each run adds 1 to the goal's x and each paid run takes 2
  off, so while the x is 2 or more every run pays and the x falls by 1. The goal's y doesn't change, and neither does
  the transport's destination (`orders/transports.md`, "What the transport stores"). A transport's stored goal is
  usually its last command's destination, so its x is usually 2 or more and repair usually pays on every run (traced,
  round 98: RP8, 20 runs, all paid).
- **The changed goal doesn't move a docked transport** (order 32): in RP8 the transport stays where it is, in order 32,
  for the 1,335 cycles after the repair (traced). The next command that sends it somewhere sets a new goal (static
  study), so nothing reads the changed x.
- The counter is part of the unit's stored state, so a saved and loaded game keeps it (static study; the map-placed
  case's counter is traced across a load in round 96, `xorc10-xorc10-r96-repair-saveload-airesearch-aiworkers-game-6000`).
- One worker repairs **4 hit points per 25 cycles**. Several workers add up: each runs its own logic.
- The resource check comes before the hit points are added, and the payment after, so a run is never refused for
  lack of resources at the moment of payment; the stock can't go below 0 because both must be at least 1 before
  the run.

## Data

| Item | Value |
| --- | --- |
| Hit points per run | 4 |
| Runs per payment | By the target's repair counter: 1 for a building completed by construction or still under construction, 2 for one placed by the map, 1 for a transport whose goal x is 2 or more |
| Payment | 1 gold, 1 lumber |
| Run interval (worker attack loop) | 25 cycles; haste 10; slow 49 |
| Arrival radius | 1 tile |
| Messages | `network.tbl` entries 73 (lumber needed) and 74 (gold needed), counting from 1 |
| Repairable targets in the default unit data | buildings, and unit types 0x1C and 0x1D (transports) |

## Test vectors

| Input | Expected |
| --- | --- |
| Peasant repairs a farm **placed by the map** (counter 0) at 300 of 400 HP, player has plenty of gold and lumber | Runs every 25 cycles. After 25 runs: 400 HP, paid 12 gold and 12 lumber, counter 1. Run 26: full, so the peasant becomes idle |
| Same map-placed farm (300 of 400 HP, counter 0), two peasants P1 in slot 10 and P2 in slot 11 whose logic runs in the same cycles, t, t + 25, t + 50, … | Units update in ascending slot order (`core/unit-slots.md`), so in each of those cycles P1 runs first (+4 HP, counter 1) and P2 second (+4 HP, counter 2, so P2's run pays 1 gold and 1 lumber; counter back to 0). After the runs at t + 275 (12 cycles of runs, 24 runs): 396 HP, 12 gold and 12 lumber paid, counter 0. At t + 300: P1 adds 4 (400 HP, counter 1, no payment); P2 finds the farm full and becomes idle. At t + 325, P1 becomes idle. Total: 12 gold, 12 lumber; the farm keeps counter 1 |
| Repair with 0 lumber and 500 gold, local player | First run: the peasant becomes idle; message entry 73 (lumber needed) |
| Repair with 5 lumber and 0 gold, local player | Idle; message entry 74 (gold needed) |
| Map-placed target at 398 of 400 HP, counter 0 | One run: 400 HP (+2); counter 1, no payment; next run: pending idle |
| Command 27 on an enemy footman | Order 3 with the footman as its target (it walks toward where the footman is) |
| Command 27 on an own damaged destroyer | Order 3: no flag bit 10 in the default data, so no repair |
| Command 27 on an own damaged catapult | Order 3 |
| Command 27 on an own damaged transport | Order 27 |
| Human player's peasant repairing is hit by an archer | It flees (the reaction), the repair stops |
| Computer player's peon repairing a farm is hit by an archer | It doesn't flee (it isn't idle); the run goes on and the farm gains 4 HP. It keeps repairing on later runs |
| Computer player's peon repairing is hit by an enemy peasant | It fights back (command 8 on the peasant); the repair stops |
| **RP1** (traced: `orc9-orc09-r97-built-farm-repair-goals-2400`, Enhanced; scenario `oracle/scenarios/orc09-r97-built-farm-repair.txt`): peon 1478 is told at n = 34 to build pig farm 1473 (type 0x3B) at (44, 89); catapult 1494 hits the site with attack-ground from n = 520 to 700, so the farm completes at n = 1361 (builder out, mana byte 0) with 367 of 400 HP; peon 1482 is told to repair it at n = 1420. Player 5 has 500 gold and 750 lumber | The farm's stored goal x is 1,224 from completion. Order 27 from n = 1427. Runs at n = 1574, 1599, …, 1774 (every 25 cycles): each adds 4 HP (the last, 399 to 400, adds 1), takes the counter down by 1 (1,224 to 1,215) and pays 1 gold and 1 lumber: 491 gold and 741 lumber after the nine runs. n = 1799: the farm is full; order 2, still animation, wait 3, target none |
| **RP2** A farm completed by construction (counter 1,224), damaged, repaired by one peon; the player has 500 gold and 500 lumber | Each run: +4 HP, counter −1, 1 gold and 1 lumber. After 10 runs: 490 gold, 490 lumber, counter 1,214 |
| **RP3** The same farm placed by the map (counter 0) | Runs alternate between no payment (counter 1) and a payment (counter 0); after 10 runs 495 gold, 495 lumber, counter 0 |
| **RP4** Two peons repairing the RP2 farm, their runs in the same cycles | Every run of each peon pays: 2 gold and 2 lumber per cycle of runs; the counter falls by 2 per cycle of runs |
| **RP5** A farm completed while the build-speed cheat was on, P's last value 1,213 (static study) | Pays on every run until the counter is below 2 |
| **RP6** A worker repairing a damaged building when its player's lumber reaches 0 (any gold) (static study) | At its next run: the lumber message (local player) and the pending idle of step 4 (order 2, still animation, wait 3 with nothing to resume); a remembered patrol or attack-move is resumed (Enhanced) |
| **RP7** A peasant given repair on an own footman that is walking away (static study) | Order 3 with the footman as its target; each walk heads for the footman's tile at that moment (`movement/pathfinding.md`, "Step 1"); it ends as any order 3 with a target unit |
| **RP9** (traced: `mp-r100-tests-mp-r100-tests-fixed-goals-3000`, Enhanced, generated map `R100-tests.pud`, `oracle/test-maps.md`; scenario `oracle/scenarios/mp-r100-tests.txt`): map-placed town hall 1591 (counter 0) of player 0, damaged by ballista 1589 to 1,067 of 1,200 HP, starts its upgrade to a keep at n = 102 (order 37); peasant 1588 is told to repair it at n = 520 and is in order 27 from n = 558 | Runs every 25 cycles from n = 577, +4 HP each; the counter goes 1, 0, 1, 0 (n = 577 no payment, n = 602 pays 1 gold and 1 lumber, and so on), during the upgrade; the hall is full (1,200, the last run adding 1) at n = 1402 and the peasant idle at n = 1427. n = 2115: the upgrade ends: type keep, 1,200 → 1,400 HP (+200), order 33, counter 0. The ballista brings the keep to 1,315 HP; repair from n = 2478: counter 1 (no payment), n = 2503 counter 0 and paid, every second run paid; 1,399 HP at n = 2978 |
| **RP10** (traced: the same trace): peasant 1587 starts farm 1583 (type 0x3A, T = 100, M = 400) at (28, 20) at n = 111 (P = 1, 40 HP); peasant 1586 is told to repair it at n = 100 and is in order 27 on it from n = 137. Player 0 has 10,000 gold and 10,000 lumber | Runs at n = 156, 181, …, 931 (every 25 cycles, 32 runs): each adds 4 HP (the last, 398 to 400, adds 2), pays 1 gold and 1 lumber, and leaves P where it was the cycle before (P = 45 at n = 155 and 156; in general P = n − 110 − the runs so far). Construction steps at P = 12, 24, …, as usual. n = 931: 400 HP, P = 789 (821 without the runs). n = 934 (P = 792, a step): P = 1,212. n = 946: completed, P = 1,224, builder 1587 out (order 2). n = 956: peasant 1586 finds the farm full: order 2, still animation, wait 3 |
| **RP11** (traced: `mp-r100-tests-mp-r101-site-repair-two-fixed-goals-1000`, Enhanced, generated map `R100-tests.pud`; scenario `oracle/scenarios/mp-r101-site-repair-two.txt`): peasant 1587 starts farm 1583 (type 0x3A) at (28, 20) at n = 76 (P = 1, 40 HP); peasants 1586 (order 27 from n = 137, runs n = 156, 181, …) and 1588 (walking from (11, 8), order 27 from n = 163, first run n = 422, then every 25 cycles) both repair it; player 0 has about 9,500 gold and lumber. Two repairers make P gain 23 per 25 cycles, so the runs pass through every point of the 12-pass step cycle. A run in the cycle a step would fall in: P = 335 before it | n = 422 (1588's run): P stays 335, 185 HP (+4, the run only), 1 gold and 1 lumber paid. n = 423: P = 336, a step: 188 HP (+3), mana byte 115 → 119. The step comes one cycle later. The same at n = 631 (P 527 kept, 310 → 314 HP) and n = 632 (P = 528, a step, mana byte 195 → 202) |
| **RP12** (traced: the same trace) A run in the cycle right after a step | n = 605: P = 504, a step (the 42nd, share 4): 291 → 295 HP, mana byte 182 → 188. n = 606 (1586's run): P stays 504, 295 → 302 HP: +4 for the run and +3 for a second step at P = 504 (the 43rd, share 3), mana byte 188 → 192; 1 gold and 1 lumber paid. n = 607: P = 505 |
| **RP13** (traced: the same trace) The site filled by repair, two repairers | n = 772 (1588's run): 397 → 400 HP, P = 657. n = 775: the step at P = 660 finds 400 HP: P = 1,212, mana byte 1, frame 0 (`economy/build-train.md`, V74). n = 781: peasant 1586's next run finds the site full: order 2, still animation, wait 3. n = 787: completed, P = 1,224, mana byte 0, builder 1587 out (order 2). n = 797: peasant 1588's next run finds the farm full: order 2, still animation, wait 3 |
| **RP8** (traced: `xhuman6-xhuman06-r98-transport-repair-goals-2500`, Enhanced; scenario `oracle/scenarios/xhuman06-r98-transport-repair.txt`): destroyer 1564 attacks own transport 1572 from n = 5 to 300 (150 to 71 HP); the transport is moved to coast tile (110, 8), next to land at (110, 7), and is docked (order 32) from n = 399 with goal (110, 8); peasant 1594 is told to repair it at n = 600. Player 1 has 5,000 gold and 2,800 lumber | Order 27 from n = 607; the peasant reaches (111, 7) at n = 674. Runs at n = 690, 715, …, 1165 (20 runs, every 25 cycles): each adds 4 HP (the last, 147 to 150, adds 3), lowers the goal x by 1 and pays 1 gold and 1 lumber. At n = 1165: 150 HP, goal (90, 8), 4,980 gold, 2,780 lumber; `transport_dest` stays (110, 8) throughout. n = 1190: the peasant is idle (order 2). The transport stays at (110, 8) in order 32 to n = 2500 |

## Open questions

- Whether custom unit data in a map (which can set flag bit 10 on other types) is meant to make them repairable; the
  rule above follows the flag, whatever the type.
- Whether the player's interface offers repair on a building still under construction. The order's logic itself
  doesn't refuse it, and a repair command on a site is carried out (traced, round 100: RP10, a scripted command).
- **Repairing a transport** (round 97): **closed in round 98.** The counter is the transport's stored goal x (traced,
  RP8), and a docked transport isn't moved by the change (traced). That the next command overwrites the goal is
  static study.
- **A run right after a construction step:** **closed in round 101.** The pass as written makes the step a second
  time (traced: RP12), and a run on the step point moves the step one cycle later (traced: RP11).
- **The build-speed cheat's last value** (round 97). RP5 is static study: with the cheat on, P goes up by 12 per pass
  from whatever it was, so its last value can be any number. Default as stated.
- Whether the maximum hit points used here include armour or health upgrades.
