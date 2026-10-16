# AI: the computer player (model)

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-16; round 82, B-038 consolidation of the AI area,
  `specs/ERRATA-ai-consolidation.md`)
- **Sources:** the five AI specs as approved at tidewater-docs `0421534` (`ai/ai-scripts.md`,
  `ai/training-research.md`, `ai/worker-ai.md`, `ai/attack-ai.md`, `ai/caster-ai.md`), the computer-only rules of the
  other specs, every errata file that touches the AI, static study of the 2.02 program where the specs had gaps (each
  such fill says "static study"), and one new Enhanced oracle trace, `xorc10-xorc10-losses-4000` (campaign launch).
- **Changes:** 2026-10-16, round 82: new file. It is the model of the whole computer player and the map of
  where each rule lives. The rules themselves stay in the five AI specs, each said once; this file adds the timeline,
  the information flow, the defence and unit-level summaries, and the full list of random draws. Review (2026-10-16):
  C2 (the job choice outside order 2) and the unfinished-platform clause of the losses rule are held as open
  questions with the old text as the default; the summaries were checked against the part specs and corrected.
- **Changes:** 2026-10-16, round 83 (traces, `ERRATA-round83.md`, reviewed by a spec-review session, 2026-10-16):
  both held items settled by traces: the job choice runs in order 4 (AI1) and an oil platform under construction
  counts in the losses rule (AI5); an active rescue player's map buildings get no creation marks (AI2), so it isn't a
  computer player for the creation step; the all-spells cheat doesn't teach a computer player its spells (AI3); a
  worker that joins another player leaves its job count behind (AI4). CP5 updated, CP7 to CP10 added; open questions
  closed. Review fixes: the passes after AI1's job choice found the peon within its leash, so they don't show what
  happens to its attack goal (CP5; open question in `ai/attack-ai.md`, default: the goal stays); the order-4 job
  choice's command takes over at the next step done, as the trace shows.

## Behaviour

A computer player is a set of small deciders that never plan ahead. Each one runs at a fixed moment, reads the game
state and a few per-player numbers, and gives ordinary commands. Nothing remembers a plan except what is stored in the
script slots, the build list's done marks, the research requests and each unit's goal.

"Computer player" means a player whose controller is computer once the game is set up (`game/setup.md` says which owner
codes and lobby choices give one; code 7, active rescue, becomes one once the map is loaded, so its map units are
created before it is one: round 83, traced for the creation marks, `ai/worker-ai.md`). Passive computer players
(code 2 and, after loading, code 6) run none of the deciders below; they are "every other player" in the rules of
`orders/orders.md`, "Terms". Where a rule splits on **human-controlled** rather than on computer, the spec that has it
says so (`orders/orders.md`, "Terms").

Every rule here is Enhanced. The few Classic-mode differences are named in "Rules mode" below.

### The parts

| Part | Runs | Phase (`core/timing-and-rng.md`) | Decides | Rules |
| --- | --- | --- | --- | --- |
| Script interpreter | every cycle, each computer player 0 to 7 in turn | 2 | the script slots: wanted counts, wave triggers and sizes, the build-list limit | `ai/ai-scripts.md` |
| Pass A | every 50 cycles, after all script steps | 2 | the threat map; attack waves (which units get an attack goal, and where) | `ai/attack-ai.md`, "The threat map", "Attack waves", "Target pickers" |
| No-workers pass | every 50 cycles, after pass A | 2 | research requests for a player with no workers | `ai/training-research.md`, "Research requests and done marks" |
| Pass B | every 50 cycles, after the no-workers pass | 2 | the base boxes; then, in slot order, each computer unit's caster step and goal | `ai/attack-ai.md`, "The base box", "Running goals", "Ferry"; `ai/caster-ai.md` |
| Building think | in a computer building's own update, from its resting order 33 | 4 | what a building trains, researches or upgrades into | `ai/training-research.md` |
| Job choice | in a computer worker's or tanker's own update, from its idle logic (order 2) and when it comes out of a building ("Where the job choice runs") | 4 | harvest, return, build, repair or wait (tankers never wait) | `ai/worker-ai.md` |
| Creation hook | when a unit or building of any player is created | where created | the first goal; the build list's done mark; the keep-clear creation mark | `ai/attack-ai.md`, "Unit goals"; `ai/worker-ai.md`, "Done marks", "Creation marks" |
| Destroyed hook | when a computer player's building is killed | where killed (4) | the done mark cleared; the losses rule | `ai/worker-ai.md`, "Done marks", "The losses rule" |
| Damage hook | when a unit is notified of a hit | where hit (4) | the attacker record, the submarine call, the call for help | `ai/attack-ai.md`, "Call for help" |
| Unit-level rules | inside the ordinary order logic of every unit | 4 | target choice, look-around, fleeing, walls, boarding | `orders/orders.md` and the specs in "Unit-level AI" |

No part runs between the cycles' phases on a timer of its own other than the 50-cycle passes. The 50-cycle passes
first run in the first start-up cycle, then in the cycles that end with the cycle counter at 49, 99, 149, …
(`ai/attack-ai.md`, "Timing").

### One cycle, in order

1. **Phase 2, every cycle.** For each player 0 to 7 in ascending order that is a computer player: its script's wait
   counter goes down by 1, or, at 0, its script runs until it pauses (`ai/ai-scripts.md`, "Each cycle"). Scripts make
   no draws and give no commands.
2. **Phase 2, in a 50-cycle pass cycle, after step 1:** pass A (threat map rebuilt, then for each computer player the
   land, sea and air waves), then the no-workers pass, then pass B (every base box, then every unit slot in slot
   order). Commands given here take effect through the units' own updates in phase 4 of the same cycle.
3. **Phase 4.** Units update in slot order. A computer building in order 33 may think; a computer worker or tanker may
   run its job choice; every unit runs its order's acquisition, look-around and reaction rules. Kills in this phase
   run the destroyed hook at once, inside the kill; hits run the damage hook at once.
4. The timers pass at the end of phase 4 clears the caster target marks and counts down the caster timer marks
   (`ai/caster-ai.md`, "Marks").

### Information flow

| Store | Written by | Read by |
| --- | --- | --- |
| Slots 0x09 to 0x0B (wave triggers) | script | pass A, which clears them after the wave |
| Slot 0x0C (ferry landing search) | set to 1 at game start, then the script | pass B, ferry |
| Slots 0x0D to 0x12 (army sizes) | script | the script's conditions 4 to 6; pass A (wave size and repeat count); pass B (the ferry escort quirk reads 0x0F) |
| Slots 0x13 to 0x1E and 0x20 (wanted counts) | script | building thinks; the hold-back rules; condition 3 (0x13) |
| Slots 0x1F and 0x21 | script | nothing (static study, round 82) |
| Slot 0x22 (build-list limit) | script | the job choice's build-list scan; the no-workers pass |
| Build list and done marks | the creation hook, the destroyed hook, research and upgrade starts, game start | the job choice's build-list scan; the no-workers pass |
| Research requests | the build-list scan, the no-workers pass | building thinks, which clear a request when its item starts |
| Second list (think intervals) | the script file | building thinks |
| Threat map | pass A | pass A's land picker; the all-out attack's land picker (in the destroyed hook); pass B's hold goal |
| Base box | pass B | the post choice at creation; the base-edge patrol |
| Unit goal and its marks | the creation hook, pass A, pass B, the all-out attack, the call for help, the caster retreat, boarding | pass B |
| Job counts (miners, lumberjacks, repairers, buildings in progress) | the job choice; released at step 1 of the worker's next job choice, or when the worker dies | the job choice |

### Where the job choice runs

A computer worker's or tanker's job choice runs:

- **from its idle logic** (order 2), at the point where a combat unit would run the target search of
  `orders/orders.md`, "Automatic target acquisition". Workers and tankers never search; a computer player's run the
  job choice there instead. With an attacker, the kind check of `orders/orders.md`, "Reacting to an attacker", step 1
  comes first: a worker or tanker that can't attack its attacker's kind runs the flee rule (a computer unit flees
  only from idle) and makes no job choice in that run. Then the job choice runs, and its step 2 is the reaction.
- **in order 4** (round 83; C2, traced for a worker): a worker given command 4 by the attack goal after an all-out
  attack (the hold goal skips workers; the attack goal doesn't) makes its job choice at its next order-logic run in
  order 4, at the same point of that order's logic, so it takes up a job instead of marching to the goal tile
  (`ai/worker-ai.md`, "When a job is chosen"; CP5). A tanker in order 4 isn't traced; default: the same. Static study
  finds the same point in every order marked "acquires" and in the move step of the walking orders; only order 4 is
  traced, and the other orders keep the old text (no job choice there).
- **when it comes out** of a mine, platform or depot (`economy/harvest.md`, "Coming out", step 3).

From the idle logic, the job choice's order is a command and takes over in the same update (`orders/orders.md`,
"Commands"); in order 4 it is pending and takes over at the unit's next step done, as any command does (traced:
pending at n = 804, taking over at n = 826, CP5); at a come-out it stays pending through the come-out wait
(`economy/harvest.md`, "Coming out").

### Resource assignment

The computer has no global planner for its economy. Each time a worker's job choice runs it releases its old job, lets the
reaction answer an attacker, and then picks, in order: return goods; a farm when food is short; the next build-list
entry; a repair; then a harvest whose kind is set by the lumber target table (gold G, lumber L, workers W give the
wanted lumberjacks T; every other worker mines); else a random wait of 44 to 59 cycles. With no depot it tries a new
town hall first, and a carrying worker then only waits (`ai/worker-ai.md`, "Worker: the steps, in order",
"Harvest"). Mines are chosen by the score d(depot, mine) + floor(d(worker, mine) / 2). Tankers choose among platforms
and oil patches by their own steps, and never wait (`ai/worker-ai.md`, "Tankers"). Payment is made when
the job is chosen (`economy/build-train.md`, "Paying for a building").

Buildings decide training and research from the slots and counts at each think (`ai/training-research.md`). The only
link between the two is the build list: an entry of 0x80 or more sets a research request that a building's think
then acts on.

### Attack waves and force gathering

A fighting unit's life under the computer:

1. **Created:** goal by type (`ai/attack-ai.md`, "Unit goals"): transports ferry, ships sea patrol, fliers roam, land
   units hold a **post** on the line from the nearest gold store toward the nearest enemy, just outside the base box.
   Map units with the held mark never join waves; those that are mobile, can attack and aren't workers get a hold
   goal at their own tile (`ai/attack-ai.md`, "Held units").
2. **Gathering:** the units wait at their posts. The script counts the army (conditions 4 to 6) and, when it is big
   enough, sets a wave trigger.
3. **Wave:** at the next pass A the trigger sends up to size × repeat units not already attacking to the target
   picker's tile in the human player with the most non-building units. Units on another land area get the
   needs-a-ferry mark instead, and transports gather and carry them (`ai/attack-ai.md`, "Ferry").
4. **March and fight:** pass B gives each attacker command 4 (move, attacking on the way) while it is past its leash
   from the goal tile and its order isn't an attacking one (9, 11, 12, 15, 16 or 17); when it is idle near it, the enemy search picks the nearest enemy anywhere and command 12
   follows; with no enemy left the unit roams.

### Defence and reaction to attacks

| Mechanism | When | What it does | Rules |
| --- | --- | --- | --- |
| Call for help | a computer unit is hit | units of the owner in the 13-tile help box that are in order 2, or in order 7 with nothing waiting, attack the attacker (command 12); held units only within 4 | `ai/attack-ai.md`, "Call for help" |
| Submarine call | a computer unit is hit by a submarine | the owner's nearest air unit or submarine without an attacker gets command 4 at the victim | `ai/attack-ai.md`, "Call for help", step 5 |
| Unit reaction | an attacked unit's order logic runs | combat units retarget (order 12 from the idle-type and moving orders; the resulting-order table gives the rest); non-combat units flee, but a computer unit flees only when idle | `orders/orders.md`, "Reacting to an attacker" |
| Worker attacker step | a computer worker's job choice runs with an attacker | the reaction runs first; a worker attacked by a worker fights back | `ai/worker-ai.md`, step 2; `orders/orders.md`, "Fleeing", step 1 |
| Hold give-up and leash | pass B, hold goal | a unit past its leash is sent back to its post if it is idle, or, when busy, only if another player dominates its block and it is badly outmatched there (10 × own < 4 × dominant) | `ai/attack-ai.md`, "Hold" |
| Repair | a worker's job choice | damaged, safe buildings are repaired, most annoying type first; a repairing worker doesn't flee | `ai/worker-ai.md`, "Repair"; `economy/repair.md` |
| Rebuilding | a building is destroyed | its build-list entry loses its done mark, so it is built again | `ai/worker-ai.md`, "Done marks" |
| Losses rule | a building is destroyed | with fewer than 3 buildings (counted at the kill, the destroyed one included, complete oil platforms left out; an unfinished one counts), every mobile unit attacks | `ai/worker-ai.md`, "The losses rule"; `ai/attack-ai.md`, "The all-out attack" |

There is no other defence: the computer never pulls back an attacking wave to defend its base, and towers defend by
their own order logic.

### Unit-level AI

The idle look-around and the automatic target choice are the same for every player (`orders/orders.md`, "Idle
look-around", "Automatic target acquisition"). The look-around's draw comes first in an idle update, before the job
choice of a computer worker. The rules that differ for computer players' units are:

| Rule | Computer player's unit | Spec |
| --- | --- | --- |
| Acquisition box | always its react range (computer) | `orders/orders.md`, "Automatic target acquisition" |
| Fleeing | only from idle (order 2) | `orders/orders.md`, "Fleeing", step 2 |
| Smart-move wait cap | 40 cycles (others 10) | `orders/orders.md`, "Terms" |
| Friendly fire | not human-controlled: 3 own units stop a strike (human-controlled: 1) | `orders/orders.md`, "Terms" |
| Attacker record on a command | the computer's own commands keep it | `orders/orders.md`, "Commands" |
| Remembered patrol | its command 4 keeps it; the sea and base-edge patrols set the current ends only | `orders/orders.md`, "Resuming patrol and attack-move" |
| Boarding (order 34) | command 4 toward the transport instead of a smart move; a random tile when its transport is landing | `orders/orders.md`, "Boarding moves"; `orders/transports.md` |
| Walls | blocked rows A and B (order 11 at a human wall) and, for units not human-controlled, the 0x04 wall bit dropped from the search mask, under the conditions those specs give | `movement/movement.md`, "Player kinds"; `movement/pathfinding.md` |
| Missile minimum flight | 2 tiles at a wall tile | `combat/damage.md` |
| Fog | never hidden from a computer player (except a unit new since the last visibility pass, hidden from everyone); computer units apply no sight | `vision/fog.md` |
| Payment | at the decision; refunded only by a failed placement on arrival | `economy/build-train.md`, "Paying for a building" |
| Come-out | hands control to the job choice | `economy/harvest.md`, "Coming out" |

### The caster AI's place

The caster step is part of pass B, not a separate timer: a computer caster (flag bit 17) with a goal runs it before
its goal, and if the step acts the goal is skipped for that pass (`ai/caster-ai.md`). Its marks last until the timers
pass of the same cycle. A mage's or death knight's retreat at low mana attacks its recorded attacker if it has one (command 12), and
otherwise sets a hold goal at its mage tower or temple, which pass B then runs like any other hold.

### Random draws

Every draw the computer player makes, by phase. All come from the simulation generator (`core/timing-and-rng.md`).
The interpreter, pass A, the no-workers pass, the threat map, the base box, hold, attack, base-edge patrol, the enemy
search, the call for help, the losses rule and the all-out attack make **no** draws.

| When | Phase, position | Draws | Use | Rules |
| --- | --- | --- | --- | --- |
| A computer player's mobile land unit is created and its post walk ends off the map | where created, after the facing draw if any, and before the first-wait draw | 2 | post x = unit x + (r mod 12) − 6, then y | `ai/attack-ai.md`, "Choosing a land unit's post" |
| Pass B, caster step, each random-search try whose gates pass | 2, in the caster's slot-order turn, before its goal | 1 to 10 | r mod N picks a unit slot (N = 1600; Classic mode 600) | `ai/caster-ai.md`, "Finding a target" |
| Pass B, roam goal, idle unit | 2, in slot order | 2 | x then y offset (r mod (width / 2)) − width / 4 | `ai/attack-ai.md`, "Roam" |
| Pass B, sea patrol, idle unit with no platform or oil patch for the far end | 2, in slot order | 2 | x then y: anchor − (r mod 5) − 2 | `ai/attack-ai.md`, "Sea patrol" |
| Pass B, ferry in order 2 or 32 with passengers, no unit waiting to board it, slot 0x0C non-zero | 2, in slot order | 1 | first direction r mod 8 of the landing-spot search | `ai/attack-ai.md`, "Ferry" |
| Barracks think: rules 3 and 4, each only when the condition written before its chance holds | 4, the building's own update | 0 to 2 | r mod 4 = 1, r mod 8 = 1 | `ai/training-research.md`, "Training buildings" |
| Shipyard think: rules 4 to 7, each only when the condition before its chance holds | 4, the building's own update | 0 to 4 | r mod 4 = 1, r mod 8 = 1 | `ai/training-research.md`, "Training buildings" |
| Worker job choice ends in a wait | 4, the worker's own update; from the idle logic, after its look-around draw | 1 | wait (r AND 15) + 44 | `ai/worker-ai.md`, "The wait" |
| A computer unit in order 34 can't board because its transport is landing | 4, the unit's own update | 2 | random tile x + (r mod 12) − 6, then y | `orders/transports.md`, "Boarding", order 34, step 5 |

The draws that every player's units make (facing, first wait, look-around, bob, flee, combat) are in
`core/timing-and-rng.md`; a computer unit's flee is the same 2 draws, made only from idle. Setup draws (opponent and
multiplayer placement) are in `game/setup.md`.

### Rules mode

Every rule is Enhanced. The Classic-mode differences known in this area are:

- **Classic mode:** the caster random search draws r mod 600 (the Classic unit-slot count).
- **Classic mode:** food supply is uncapped, so the farm rule's S < 200 test stops farms at 200 supply.
- **Classic mode:** attack-move (order 10) doesn't acquire (`orders/orders.md`, "Automatic target acquisition").
- **Classic mode:** no unit remembers a patrol or attack-move, so the remembered-patrol rows above don't apply
  (`orders/orders.md`, "Resuming patrol and attack-move").
- **Classic mode:** the "ever had a hall" mark is set at the start of a multiplayer game, as in single player; in
  Enhanced multiplayer it is set when the first hall is completed (`ai/worker-ai.md`, "Farm").

No other Classic-mode difference is known here.

## Data

| Item | Value |
| --- | --- |
| Players with a script | 0 to 7 (computer players run it every cycle) |
| 50-cycle passes | A, then the no-workers pass, then B; first in the first start-up cycle |
| Building think | from order 33, every (interval + 2) order-logic runs |
| Losses rule threshold | buildings at the kill, the destroyed one included, less complete oil platforms, < 3 |
| Help radius | 13 tiles for a computer player's unit |

## Test vectors

| ID | Input | Expected |
| --- | --- | --- |
| CP1 | `xorc10-xorc10-losses-4000` (oracle trace, Enhanced, campaign launch, scenario `oracle/scenarios/xorc10-losses.txt`): computer player 2 has 3 buildings (guard towers 1476, 1477, 1482) and 55 mobile units; with invulnerability on, player 5's dragons destroy tower 1476 (order 1 at n = 1358) | No all-out attack: at the passes of n = 1401, 1451 and 1501 no unit of player 2 gets order 4 (counted at the kill: 3 buildings, not below 3) |
| CP2 | Same trace: the dragons then destroy tower 1482 (order 1 at n = 1543) | All-out attack: at the pass of n = 1551 player 2's units take order 4, 7 at n = 1551 and 36 by n = 1555; the 9 units already in order 12 keep it (counted at the kill: 2 buildings) |
| CP3 | `human2-human02-all-out-4000`: computer player 0's only building, watch tower 1593, is destroyed (n = 510) | All-out attack from the pass of n = 551 (1 building at the kill) |
| CP4 | A pass cycle: computer player 1's script step, then pass A, the no-workers pass and pass B | In that order, all in phase 2, before any unit's phase-4 update |
| CP5 | Computer worker given goal attack by an all-out attack, smart move toward the target tile; at the next pass it is past its leash (round 83; traced: `xhuman5-xhuman05-worker-all-out-goals-1100`, peon 1575) | Pass B gives it command 4 (X9; the pass of n = 801, goal tile (9, 20)). At its next order-logic run in order 4 (n = 804) it makes its job choice: carrying gold, it gets return goods to its hall, pending until n = 826. The passes of n = 851 to 1051 give it nothing, but they find it within its leash of 7 (6 tiles from (9, 20) at n = 851, then inside the hall and building next to the tile), so they don't show whether its attack goal is kept (`ai/attack-ai.md`, Open questions) |
| CP6 | Pass B with a computer ogre-mage that knows runes, has 200 mana and no bloodlust target, then a roaming idle gryphon rider in a later slot | The ogre-mage's 1 to 10 draws come first, then the gryphon's 2 |
| CP7 | `orc3-orc03-platform-losses-9500` (round 83): computer player 1 with farms 1589 and 1595 and oil platform 1584 under construction loses farm 1595 (n = 6765) | No all-out attack (2 farms and the unfinished platform: 3); none at the passes of n = 6801 to 7601. After the platform completes, the loss of farm 1589 (n = 7634; B = 1, the farm itself, with the complete platform left out) starts it: order 4 from n = 7651 |
| CP8 | `human5` and `human8` setup dumps (round 83): active rescue players' town halls, barracks, lumber mill and blacksmiths placed by the map | No creation marks (no keep-clear bit); the computer players' buildings on the same maps have theirs |
| CP9 | `xhuman2-xhuman02-cheat-scope-1600` (round 83): all-spells cheat on; computer death knight 1557 knows only death coil | It casts death coil (mana topped up to 255 at each command) and nothing else, though haste's other tests pass at 10 passes |
| CP10 | `human5-human05-rescue-jobs-2000` (round 83): active rescue player 0's lumberjack 1517 joins player 1 | Player 0's lumberjack count keeps 1517's job (inferred from the next choice, `game/rescue.md`); its last lumberjack turns to gold at its next come-out (n = 1593) |

## Open questions

The open questions of each part stay in its spec. The ones about the model as a whole are Q1 to Q6 of
`specs/ERRATA-ai-consolidation.md`:

- **Job choice outside order 2** (C2): closed in round 83 for order 4 (CP5). The other acquiring orders
  and the move step of orders 5 and 12: static study finds the job choice there too; untraced, so the default keeps
  the old text (no job choice there). A trace would need a computer worker in order 5 or 12.
- **An unfinished oil platform in the losses rule:** closed in round 83: it counts (CP7).
- **Active rescue players at map load:** creation marks closed in round 83: none (CP8). Done marks and a
  first goal for its map units: none by default, untraced (`ai/worker-ai.md`, `ai/attack-ai.md`).
- **All-spells cheat scope:** closed in round 83: players controlled by a person only (CP9).
- **Unknown opcodes and conditions** in a script have no defined behaviour (static study). No shipped script has
  one. Default: refuse such a script file.
