# Economy: repair

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the message table in the
  game data (`network.tbl`), and the unit flags in the public PUD specification text. Not yet confirmed against the
  running game.
- **Changes:** 2026-10-11, round-4 errata: a lost target or attacker is one that is no longer valid or no longer
  **detected** by the owner; fog over it doesn't matter (was "no longer visible").

## Behaviour

A worker repairs with **command 27**, which carries a target unit. Repair is order **27** (`orders/orders.md`): it
uses the attack animation and an arrival radius of 1 tile.

### The command

| Target | Result |
| --- | --- |
| A building, or a unit whose type has the transport flag (bit 10 of the PUD unit flags) | Order 27 on that target |
| Anything else, or no target | Order 3 (a plain move toward the target) |

In the default unit data only the two transports (types 0x1C and 0x1D) carry flag bit 10, so with that data **only
buildings and transports can be repaired**. Every other unit, including the other ships and the siege weapons, gets
order 3: the worker just walks toward it.

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
   | Human-controlled worker: it flees (order 27 is one of the orders that flee) | Ends |
   | Computer-controlled worker: it doesn't flee, because it isn't idle; the attacker is kept | Goes on to step 2 |

   So a computer player's worker keeps repairing while it is hit, unless its attacker is a worker.
2. **No target** (the target died or is no longer valid): the worker becomes idle (order 2).
3. **Not arrived:** the worker is given the repair again, which moves it toward the target.
4. **Target at full hit points:** the worker becomes idle.
5. **The player has no gold or no lumber** (either stock at 0 or below): the worker becomes idle. If the worker
   belongs to the local player, a message is shown: when lumber is out, the one saying that lumber is needed to
   finish the repair (entry 73, 0x49, of `network.tbl`, counting from 1); otherwise the one saying that gold is
   needed (entry 74, 0x4A). Lumber is checked first.
6. **Repair:**
   - the worker turns to face the target (a building is faced at its top-left tile plus (1, 1));
   - the target gains **4 hit points**, but not above its maximum;
   - the target's **repair counter** goes up by 1. When it reaches 2, it drops back by 2 and the player pays **1 gold
     and 1 lumber**.
   - The order continues.

### Cost and rate

- The repair counter belongs to the **target**, not the worker, and it is kept between repairs. Every repairer of
  the same target adds to the same counter, so the cost is always 1 gold and 1 lumber per 2 repair runs on that
  target, whoever does them.
- So repair costs **1 gold + 1 lumber per 8 hit points** (the last runs before full health can add fewer than 4).
- One worker repairs **4 hit points per 25 cycles**. Several workers add up: each runs its own logic.
- The resource check comes before the hit points are added, and the payment after, so a run is never refused for
  lack of resources at the moment of payment; the stock can't go below 0 because both must be at least 1 before
  the run.

## Data

| Item | Value |
| --- | --- |
| Hit points per run | 4 |
| Runs per payment | 2 |
| Payment | 1 gold, 1 lumber |
| Run interval (worker attack loop) | 25 cycles; haste 10; slow 49 |
| Arrival radius | 1 tile |
| Messages | `network.tbl` entries 73 (lumber needed) and 74 (gold needed), counting from 1 |
| Repairable targets in the default unit data | buildings, and unit types 0x1C and 0x1D (transports) |

## Test vectors

| Input | Expected |
| --- | --- |
| Peasant repairs a farm at 300 of 400 HP, player has plenty of gold and lumber, counter 0 | Runs every 25 cycles. After 25 runs: 400 HP, paid 12 gold and 12 lumber, counter 1. Run 26: full, so the peasant becomes idle |
| Same farm (300 of 400 HP, counter 0), two peasants P1 in slot 10 and P2 in slot 11 whose logic runs in the same cycles, t, t + 25, t + 50, … | Units update in ascending slot order (`core/unit-slots.md`), so in each of those cycles P1 runs first (+4 HP, counter 1) and P2 second (+4 HP, counter 2, so P2's run pays 1 gold and 1 lumber; counter back to 0). After the runs at t + 275 (12 cycles of runs, 24 runs): 396 HP, 12 gold and 12 lumber paid, counter 0. At t + 300: P1 adds 4 (400 HP, counter 1, no payment); P2 finds the farm full and becomes idle. At t + 325, P1 becomes idle. Total: 12 gold, 12 lumber; the farm keeps counter 1 |
| Repair with 0 lumber and 500 gold, local player | First run: the peasant becomes idle; message entry 73 (lumber needed) |
| Repair with 5 lumber and 0 gold, local player | Idle; message entry 74 (gold needed) |
| Target at 398 of 400 HP | One run: 400 HP (+2); counter +1; next run: idle |
| Command 27 on an enemy footman | Order 3 (move toward it) |
| Command 27 on an own damaged destroyer | Order 3: no flag bit 10 in the default data, so no repair |
| Command 27 on an own damaged catapult | Order 3 |
| Command 27 on an own damaged transport | Order 27 |
| Human player's peasant repairing is hit by an archer | It flees (the reaction), the repair stops |
| Computer player's peon repairing a farm is hit by an archer | It doesn't flee (it isn't idle); the run goes on and the farm gains 4 HP. It keeps repairing on later runs |
| Computer player's peon repairing is hit by an enemy peasant | It fights back (command 8 on the peasant); the repair stops |

## Open questions

- Whether custom unit data in a map (which can set flag bit 10 on other types) is meant to make them repairable; the
  rule above follows the flag, whatever the type.
- Whether the player's interface offers repair on a building still under construction. The order's logic itself
  doesn't refuse it.
- Whether the repair counter is shared with any other use of the same per-unit value (it is kept on the target and
  is never cleared by the repair logic).
- Whether the maximum hit points used here include armour or health upgrades.
