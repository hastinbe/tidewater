# AI: computer workers and tankers

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the AI script file
  `Rez\ai.bin` and the unit data `Rez\unitdata.dat` in the game data, and the `ALOW` bit list and unit flags in the
  public PUD specification text. Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-21 errata: build-list step 2 makes no other test, so an entry the player can never
  research blocks the rest of the list; the done marks now list every way an entry is marked or cleared.
- **Changes:** 2026-10-12, round-22 errata: the job choice's tree search reaches radius 2k + 1 < floor(3N / 4); a gold
  store under construction can be the site-search centre; map-placed buildings get no keep-clear mark; a carrying
  worker with no path is given command 24 to the same depot again.
- **Changes:** 2026-10-12, round-36 errata: a computer player's building marks the first open entry of its own type when
  it is created (map load or construction start); the game-start tower, keep and castle cases are removed.
- **Changes:** 2026-10-12, round-38 errata: a computer player's building gets a keep-clear mark by type when it is
  created, map buildings included (this replaces round 22's "no mark"); the keep-clear rectangle's edge rules, including
  the skew at the right edge.
- **Changes:** 2026-10-12, round-39 errata: the site search's last test is the full site check of `ui/placing-
  buildings.md` (Allowed), including "too close" to a gold mine; `xhuman10` vectors. Creation marks for computer
  players' map buildings are confirmed as they are.
- **Changes:** 2026-10-12, round-42 errata: the computer's site search refuses a footprint that reaches the map's last
  column or last row; `xorc7` vector.
- **Changes:** 2026-10-13, round-47 errata: the farm's "ever had a hall" mark is set at the start of every new game
  except Enhanced multiplayer; a killed unit leaves the food in use at the start of its death animation; `orc5-10000`
  and `xhuman2-10000` vectors.
- **Changes:** 2026-10-13, round-54 errata: the job choice's order is a command, so the cached route is dropped;
  `human12-25000` vector.
  2026-10-13, round-58 errata: the mine score's worker term is the larger axis distance from the mine's top-left tile
  to the worker's tile; `mp-gold-rush-bne-25000` vector (Classic-mode traces agree).
  2026-10-13, round-65 errata: the new-mine search skips a mine if any tile of a gold-storing building is in its
  13 × 13 square, not only the building's top-left tile; `xorc4-30000` vector.
- **Changes:** 2026-10-16, round 82 (AI consolidation, `ERRATA-ai-consolidation.md`, reviewed by a spec-review
  session, 2026-10-16): the model of the whole computer player is in `ai/computer-player.md`. C1 (traced): the losses
  rule counts the buildings at the kill, the destroyed one included. X3: the come-out hand-off is at every come-out,
  not only a depot's. X2: the all-out target is the human player with the most non-building units. Two open questions
  answered. Held as open questions with the old text as the default: the job choice in acquiring orders other than
  order 2 (C2, static study only) and whether an unfinished oil platform counts in the losses rule.
- **Changes:** 2026-10-16, round 83 (traces, `ERRATA-round83.md`, reviewed by a spec-review session, 2026-10-16): an
  oil platform under construction counts in the losses rule (AI5, traced: `orc3-orc03-platform-losses-9500`); the
  job choice also runs in order 4, as for the worker sent by an all-out attack (AI1, traced:
  `xhuman5-xhuman05-worker-all-out-goals-1100`); an active rescue player's map buildings get no creation marks (AI2,
  traced: setup dumps of `human5` and `human8`). Two open questions closed; the done marks of an active rescue
  player's map buildings added as one (default: none). Review fixes: the Data row and the 3-building vector state the
  platform clause; the order-4 command takes over at the next step done; the passes after the job choice are within
  the leash, so the AI1 vector no longer reads them as evidence about the goal; tankers in order 4 and the other
  acquiring orders are labelled defaults with trace requests.
- **Changes:** 2026-10-16, round 84 (traces, `ERRATA-round84.md`, reviewed by a spec-review session, 2026-10-16): the
  job choice runs in order 12 (AI8, traced: `xhuman5-xhuman05-worker-order12-goals-3000`) and for a tanker in order 4
  (AI8, traced: `orc3-orc03-tanker-order4-goals-7500-10500`); the other acquiring orders get it too as a labelled static
  gap-fill. The done marks of an active rescue player's map buildings (AI7): the default stays, untraced; by the stated
  rules it changes no outcome in any shipped mission. Review fixes: in a walking order the job choice runs at the walk's
  step 1, before the step, so a step that starts the move animation overwrites the wait (the question on the first
  order-12 run is closed); an order-4 job whose target is already in reach takes over in the same update (n = 2464);
  the tanker vector counts 36 order-4 runs (to n = 9808), not 48.
- **Changes:** 2026-10-16, round 85 (AI audit errata, `ERRATA-audit-ai.md`, with traces, `ERRATA-round85.md`;
  reviewed by a spec-review session, 2026-10-16): repair safety looks at a tile's air layer only when its ground layer
  holds a unit (erratum 2, traced: `xorc10-xorc10-farm-repair-1700`); a build job writes the site into the worker's
  goal tile (erratum 7, traced for a farm job: `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500`; the town hall and
  build-list jobs a static gap-fill). Static gap-fills: the site-search centres measure the nearest gold store from its
  top-left tile, with no alive test (erratum 1; this spec gave no measure); the no-depot hall search with no mine
  centres on the worker (erratum 3); tankers break a tie between oil patches by the neutral list, first in the list
  (newest) winning (erratum 10; review fix: the draft said the latest in the list); vectors for a second-pass hall's
  bordered mark (erratum 11) and the bordered test at column 0 (erratum 16). Held as open questions, stated rules
  kept: which flag decides a worker's or tanker's job choice (erratum 6) and the shipyard's search centre and water
  region (erratum 8).
- **Changes:** 2026-10-16, round 86 (`ERRATA-round86.md`; reviewed by a spec-review session, 2026-10-16): no rule
  here changes. `ai/attack-ai.md` now measures its nearest gold store as the site-search centres do (top-left tile);
  request AA4b (erratum 8) was tried, skipped and taken off the queue.
- **Changes:** 2026-10-21, round 93 (oracle traces, `ERRATA-round93-traces.md`; reviewed by a spec-review session,
  2026-10-21): after a load the done marks are cleared and made again, with the load's building marks (towers mark the
  scout-tower entry, keeps and castles the hall-upgrade entries); traced, `game/save-load.md` SL9 to SL12. The
  creation marks are unchanged.
- **Changes:** 2026-10-21, round 94 (oracle traces, `ERRATA-round94-traces.md`; reviewed by a spec-review session,
  2026-10-21): the load's building marks are made for every building that isn't dying or removed, construction sites
  included (not only complete ones); the keep, castle and level marks after a load are traced (`game/save-load.md`
  SL16 to SL20).
- **Changes:** 2026-10-22, round 95 (oracle traces, `ERRATA-round95-traces.md`; reviewed by a spec-review session,
  2026-10-22): the job counts are 16-bit unsigned and wrap; a load starts them at 0 while the workers keep their jobs,
  so a loaded job's release wraps ("Counts after a load"; traced, `game/save-load.md` SL22 to SL25; the farm,
  build-list and hall tests on a wrapped count are static study). A known spell's mark after a load and an upgrading
  building's mark are traced (SL26, SL27).
- **Changes:** 2026-10-22, round 96 (oracle trace, `ERRATA-round96-traces.md`; reviewed by a spec-review session,
  2026-10-22): the repairer count's clearing and wrap and a build-list type's count in progress wrapping are traced
  (`game/save-load.md` SL31, SL32). The repair, farm, build-list and hall tests on a wrapped count stay static study.
- **Changes:** 2026-10-22, round 98 (`ERRATA-audit2-ai.md`, "Round 98 additions"; reviewed by a spec-review session,
  2026-10-22): a build job's release clears the keep-clear mark at the worker's **goal tile**, sized by the type
  number the job recorded, not at the site (W1, static study); a no-depot town hall job is recorded as a build-list job
  (W2, static study; trace job bit 16). The build-list job's goal tile write and the recorded type numbers are traced.
  The clear's edge rule is stated (open question closed).
- **Changes:** 2026-10-22, round 99 (oracle trace, `ERRATA-round99-traces.md`; reviewed by a spec-review session,
  2026-10-22): the no-depot town hall (step 5) is traced: job bit 16, payment at once, cooldown 10 and its fall by 1 per
  try (W2 vector traced, a second traced vector). No rule change.
- **Changes:** 2026-10-22, round 104 (oracle sweep, `ERRATA-round104-sweep.md` SW7; reviewed by a spec-review session,
  2026-10-22): "Done marks": a new game's game-start marks come before the mission start, so what it grants marks
  nothing (traced outcome, SW7a; the place in the setup is static study).
- **Changes:** 2026-10-22, round 106 (oracle trace, `ERRATA-round106-sweep.md` SW7; reviewed by a spec-review
  session, 2026-10-22, see its "Review"): the spells a map's `ALOW` section gives a computer player are marked done at
  the game start (traced, SW7b). `ALOW` is a header section, read before the marks, so the open question "Game-start
  marks and the map's own sections" rested on a misplaced example and is closed. No rule changed.

## Behaviour

A computer-controlled player's workers and tankers choose their own jobs. Each time a job is chosen, the unit
returns goods, builds, repairs or harvests, or else waits a short random time and looks again. This spec covers that
choice. The script slots it reads are in `ai/ai-scripts.md`. Training, research and attacks are not covered.

"Worker" here means a unit whose type has the peon flag (PUD flag bit 8) and moves on land. A unit with the peon flag
that moves on water is handled as a tanker (below).

### When a job is chosen

The job choice runs for a unit of a **computer-controlled** player (`ai/computer-player.md`, "Where the job choice
runs"):

- when its idle order (order 2) logic runs (`orders/orders.md`), at the point where a combat unit would run the
  target search ("Automatic target acquisition"). With an attacker, the kind check of `orders/orders.md`, "Reacting
  to an attacker", step 1 comes first: a worker or tanker that can't attack its attacker's kind runs the flee rule and
  makes no job choice in that run. The look-around's draw comes before the job choice;
- **in order 4** (round 83): a computer worker given command 4, as by the attack goal after an all-out attack
  (`ai/attack-ai.md`, "Attack (2)"), makes its job choice at its next order-logic run in that order, at the point
  where a combat unit would run the target search, after the order's arrival test (traced:
  `xhuman5-xhuman05-worker-all-out-goals-1100`, vectors below; a peon carrying gold gets return goods 3 cycles after
  the pass that gave it command 4). Its command is pending and takes over at the next step done, like any command
  that isn't from idle (`orders/orders.md`, "Commands"; traced: 22 cycles later). One traced exception: when the
  job's target is already within order 4's radius, the walk's arrival test after the choice ends the walk and the
  job's order takes over in that same update (round 84; traced: `xhuman5-xhuman05-worker-all-out-goals-3000` n = 2464,
  peon 1575 beside gold mine 1573: order 23, animation 4, wait 3; that the arrival's idle doesn't replace the pending
  command there is a reading of the trace, Open questions). A **tanker** in order 4 makes it too, at its first order-4
  run and at each step after (round 84; traced: `orc3-orc03-tanker-order4-goals-7500-10500`, vectors below);
- **in order 12** (round 84): a computer worker given command 12, as by the attack goal's enemy search when it is idle
  within its leash (`ai/attack-ai.md`, "Attack (2)", step 2), makes its job choice in that order (traced:
  `xhuman5-xhuman05-worker-order12-goals-3000`, vectors below: every traced run is a step boundary of the walk toward
  its target; at the order's logic run when in reach, static study). A worker that finds no job there gets the wait
  (below) and keeps order 12, so it chases one step per wait;
- in the other orders marked "acquires" (`orders/orders.md`: 5, 7, 10, 13 to 16), at the same point and in the move
  step of order 5 (round 84, **static gap-fill**, accepted by review, untraced: no AI rule gives a computer worker or
  tanker these orders, and none is in them in 10 long traces, `ERRATA-round84.md`); and
- when it comes out of a mine, a platform or a depot (`economy/harvest.md`, "Coming out", step 3; traced for a mine
  by that spec's V45). If a job is given there, the usual return to the old source is skipped.

In a walking order (4, 12, and the gap-fill orders while they walk) the choice is made at the walk's step 1
(`orders/orders.md`, "Walking orders"), where a combat unit's move step would run its target search; the walk then goes
on to its arrival test and its step in the same update (round 84; traced in orders 4 and 12: the unit steps in the
update of each choice).

The job choice gives its order as a **command**, so the unit's cached route is dropped and its blocked counter cleared,
as for a player's command (`orders/orders.md`, "Commands"); the unit plans a new route at its next movement decision
(traced: `human12-25000`, vectors below).

### Worker: the steps, in order

1. **Release the old job.** The bookkeeping of the job the worker had is undone:

   | Old job | Undone |
   | --- | --- |
   | Repairing | the player's repairer count −1 |
   | Mining gold | the player's miner count −1 |
   | Cutting lumber | the player's lumberjack count −1 |
   | Building | the player's count of that building type in progress −1, and the **keep-clear mark** (below) is cleared on a width × width rectangle (no border) whose top-left is the worker's **goal tile** (`ai/attack-ai.md`, "Unit goals"); the width is that of the building type whose number the job recorded ("The goal tile of a build job") |

   Each build job writes its site into the goal tile when it starts, so usually the rectangle is the site's footprint.
   Anything that sets the worker's goal tile while it walks to its site moves the rectangle. The case found: the
   all-out attack (`ai/attack-ai.md`, "The all-out attack") gives an attack goal to each worker that isn't inside
   something, and command 4, so at its
   next job choice the release clears a rectangle at the attack goal's tile (whatever marks are there) and the site's
   own mark stays for the rest of the game; the count in progress is lowered either way. A worker already inside its
   site can't be given a goal, so a building under construction keeps the usual rectangle. Keep-clear marks only steer
   the computer's site search ("Keep-clear marks and site search"). Static study (round 98, W1): keep-clear marks
   aren't in the traces. The clear clips its width at the map's edge to N − x (N the map size, x the column) with no
   skew, unlike the set; no site reaches the last row or column, so the difference never shows.

   Then the worker has no job. Each count goes down by 1 as a 16-bit unsigned value, so a release at 0 gives 65535
   ("Counts after a load", below).
2. **Attacker.** If the worker has an attacker, the reaction of `orders/orders.md` ("Reacting to an attacker") runs.
   If that gives the worker an order (it fights back or flees), the choice ends here.
3. **Depot.** The nearest reachable depot is found as in `economy/harvest.md` ("Finding the nearest depot"): a
   completed building of the player in the worker's region that stores gold, or stores wood while the worker is in
   lumber mode. On a tie the candidate latest in the list wins.
4. **Carrying goods, depot found:** command 24 (return goods) to it. Done. If the worker then gets no path, its order
   ends (`movement/movement.md`, "Paths") and the choice runs again. Nothing marks the depot as unreachable and there
   is no wait, so it gets command 24 to the same depot again.
5. **No depot found:**
   - **New town hall**, tried only if the map allows the player town halls (`ALOW` unit bit 26). Each player has a
     **hall cooldown**. If it is not 0, it goes down by 1 and no hall is tried. Otherwise no hall is tried if one is
     already in progress or the player can't afford one (`economy/build-train.md`). Otherwise a site is searched
     (below) for the worker's race's town hall, first with the border, then without. On success, the build command is
     given, the player pays at once, the hall cooldown becomes **10**, and the site is marked keep-clear with the
     border, whichever of the two searches found it (round 85; static vector, erratum 11). The worker's job is
     recorded as a **build-list** job (the trace's job bit 16, `oracle/trace-format.md`, "Computer worker jobs"), not
     as the farm job; its release is the same, so play doesn't change (round 98, W2; traced, round 99: the job bit,
     the payment at once and the cooldown of 10, then 1 down per try, vectors below).
   - If no hall was started: when not carrying, **repair**, then **harvest**, then **wait** (below), the first that
     gives a job. When carrying, **wait**.
6. **Depot found, not carrying:** the first of these that gives a job:
   1. **Farm** (below);
   2. **Build list** (below);
   3. **Repair**;
   4. **Harvest**;
   5. **Wait**.

**The wait:** the worker's wait counter is set to `(r AND 15) + 44` cycles (44 to 59), where r is one draw of the
game's RNG (`core/timing-and-rng.md`). It gets no order. When the wait ends, its idle logic runs and the choice is
made again. A worker in an order that runs the job choice (order 4 or 12) keeps that order through the wait and makes
the choice again at its next run, so with no job it moves one step per wait (round 84; traced in order 12: waits of
44 to 59, one tile per 65 to 80 cycles). There the draw is made before the walk's step: if that step starts the move
animation (the unit is still in its order's own animation, as at its first run after the command took over), the
animation start sets the wait to 1 and runs its first instruction in the same update (`movement/movement.md`, "Which
first wait applies when"), so the drawn wait is lost (traced: n = 2505 of the same trace, the draw made, wait 1). Once
the unit is in its move animation the wait stands (n = 2527: 59).

**Counts after a load.** Each player's miner, lumberjack and repairer counts and its count of each building type in
progress are 16-bit unsigned values: every +1 and −1 above wraps (65535 + 1 is 0, 0 − 1 is 65535); the hall cooldown
is a separate small count that only goes down while it isn't 0. Every game start sets all of them to 0, a load
included, while a loaded worker keeps its saved job (`game/save-load.md`, "What a loaded game starts afresh"). So the
first release of each loaded job wraps its count (traced, round 95: `game/save-load.md` SL23 to SL25, for the
lumberjacks, the miners and the farms in progress; round 96: SL31 and SL32, for the repairers and a build-list type
in progress). Nothing recounts the jobs: a count stays off by the loaded jobs of
its kind for the rest of the game. The tests read the wrapped value as it is:

| Test | How it reads the count | With a wrapped count |
| --- | --- | --- |
| Lumber: lumberjacks < T ("Harvest", step 1) | unsigned | never fewer, so no worker is sent for lumber by this test while the count is wrapped; step 3's fallback (no depot or no mine) still tries lumber. **Traced** (SL23, SL24: lumberjacks 65535 and 65534 with T = 6, gold given) |
| Repair: repairers < limit ("Repair") | unsigned | never fewer: no new repairer (static study) |
| Farm: S + 4·F ≤ U + 3 ("Farm") | each side taken modulo 65536, then compared unsigned | F = 65535 makes the left side S − 4 when S ≥ 4, so a farm is wanted with up to 4 more spare food than usual; with S < 4 the left side is 65532 or more and no farm is wanted (static study) |
| Build list step 4, new town hall: "one already in progress" ("Build list", "Worker: the steps, in order", step 5) | the count isn't 0 | a wrapped count reads as in progress: that entry is skipped, or no hall is tried, until enough starts bring the count back to 0 (static study) |
| Miners | read by no test | none |

The same 0 at the load means a building the loaded game is still constructing isn't counted as in progress: until a
new start of that type, "one already in progress" reads false for it and the farm test counts F without it (follows
from SL22; the tests are static study).

**The goal tile of a build job** (round 85, erratum 7). Each build job (the new town hall above, the farm and the
build list below) also writes the site's top-left tile into the worker's goal tile (`ai/attack-ai.md`, "Unit goals"),
whatever its goal kind; the kind isn't changed. Traced for a farm job
(`xhuman5-xhuman05-worker-all-out-aigoals-goals-2500`, vectors below) and for build-list jobs (round 98:
`xhuman9-aigoals-goals-8630-8830`, `xhuman12-xhuman12-picker-flip-aigoals-1700`, each job's goal tile the top-left of
the building that then appears); for the town hall job it is a labelled static gap-fill (static study: the same
write). So a worker with an attack goal from the all-out attack keeps it, and pass B measures its leash from the site
(`ai/attack-ai.md`, "Attack (2)"). The job also records a building type number next to the goal (the trace's `aigoals`
`extra`), and setting a goal doesn't change it (traced: in `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500` pass B
moves peon 1575's goal tile at n = 2401 and the value stays 59). The farm job records the farm's own type (59, the pig
farm, for an orc); a build-list job records the list entry as the list holds it, the **human** building's number for
either race (an orc barracks job records 60 and the site becomes an orc barracks, 61; traced in
`xhuman9-aigoals-goals-8630-8830` and `xhuman12-xhuman12-picker-flip-aigoals-1700`); the town hall job records the
hall's type (static study). Only the release reads it, for the width (step 1). The human and orc buildings of a pair
have the same size in the default data, so this changes no outcome there; with custom unit data that gives them
different sizes, the release of an orc build-list job uses the human building's width (static study).

### Farm

Tried only if the player's **ever had a hall** mark is set (the mark of the "fast first hall" in
`economy/build-train.md`) and the map allows farms (`ALOW` unit bit 16). The mark is set for every player at the start
of a new game, except in Enhanced multiplayer, where it is set when the player's first town hall, keep or castle is
completed (`game/setup.md`). So a computer player builds farms before its first hall is complete (traced: `orc5-10000`,
vectors below).

Let S be the player's food supply (`economy/build-train.md`, "Food"; in Enhanced mode at most 200), F the number of
farms in progress, and U the food in use (units that are not buildings, minus skeletons, daemons and critters). A unit
stops counting in U when it is killed, at the start of its death animation (`economy/build-train.md`, "Food").

- No farm if S ≥ 200. This applies in both modes. In Enhanced mode S is capped at 200, so the test means "the cap
  is reached"; in Classic mode S is the uncapped supply, and a computer player builds no farm once it reaches 200.
- A farm is wanted when **S + 4·F ≤ U + 3**: that is, fewer than 4 spare food, counting farms in progress.
- It must be affordable. A site is searched with the farm settings (below). On success, the build command, immediate
  payment, the farm count in progress +1, and the footprint is marked keep-clear **without** a border.

### Build list

The player's build list (`ai/ai-scripts.md`) is read from entry 0, up to but not including entry min(slot 0x22, 64),
stopping at an 0xFF entry. For each entry:

1. **Done** entries (below) are skipped.
2. An entry of **0x80 or more** marks that research item as requested for the player and ends the scan. (The request
   is handled by the computer player's building logic, `ai/training-research.md`.) No other test is made: not whether
   the player has the building that does the research or can ever get one, nor whether the map allows the item, its
   prerequisites are met or it is already known. An entry the player can never research therefore blocks every later
   entry for the rest of the game.
3. The entry is a building type, with its race changed to the worker's race. If the map doesn't allow that type
   (`ALOW` unit bits), its prerequisites aren't met, or the player can't afford it, the **scan ends** with no job.
4. If one of that type is already in progress, the entry is **skipped**.
5. A site is searched. A town hall uses the **new-mine search** (below); every other type uses the ordinary search
   with its family's settings. If no site is found, the scan **goes on** to the next entry.
6. On success: the build command, immediate payment, the count of that type in progress +1, and the site is marked
   keep-clear with a 1-tile border (a farm without). The job is given.

**Done marks.** Each entry can be marked done:

- When a building of a computer player is **created**, the first entry that is not done and has the same type (after the
  race change) is marked done. "Created" covers the buildings the map places (when the map is read, before the first
  cycle) and a construction site placed in play, so the mark comes when construction starts, not when it ends. There are
  no special cases: a guard or cannon tower marks a guard or cannon tower entry, never the scout-tower entry (0x40), and
  a keep or castle marks a keep or castle entry, never 0x98, 0x99 or a town hall entry. (The load's marks, last bullet,
  follow other rules and do have such cases.) An active rescue player's map buildings mark nothing (round 83, default,
  untraced; it follows from `game/setup.md`'s order of steps): they are created before the player becomes a computer
  player, as the creation marks show. Round 84: the rule stays an untraced default; by the stated rules it changes no
  outcome in any shipped mission, since every shipped active rescue player with map buildings has an empty build list
  (`human5-30000`: no building started in 30000 cycles, which either reading predicts; `ERRATA-round84.md`, AI7).
- When a building of a computer player is **destroyed**, the first entry in list order that is done and whose type
  (after changing its race to the destroyed building's race) is the destroyed building's type loses its done mark,
  so it will be built again. If no entry matches, nothing changes.
- Entries of 0x80 or more are marked done when the research or building upgrade starts (`ai/training-research.md`).
- At the start of a game, entries of 0x80 or more for what the player already has are marked done. In a new game these
  marks are made **before the mission start** of `game/victory.md` (traced: `sweep-xorc9`, SW7a), so the spells,
  research and upgrade levels the mission start grants mark nothing: those entries stay open until the research or
  upgrade starts, or until a load makes the game-start marks again. By static study they are made when the player
  slots are chosen (`game/setup.md`, "Order of the steps", step 4), before the map's units are created. The map's
  header sections, `ALOW` among them, are read before that (step 2), so the spells an `ALOW`
  section makes known to a player are marked (traced: SW7b). The codes: 0x80, 0x81
  and 0x86 to 0x8F when the upgrade line has already reached the level the code asks for; codes 0x82 to 0x85 when
  the item is already researched; and codes 0x90 to 0x97 when the player already knows the human or the orc item of
  the code (its spells-known bit; Data, "Spell bits for done marks at game start").
- **After a load** (`game/save-load.md`, "What a loaded game starts afresh") a computer player's saved marks aren't
  kept. They are made again: the game-start marks above, from the loaded levels, researched items and known spells;
  then the **load's building marks** for each building of the player that isn't dying or removed, construction sites
  included (completeness isn't tested): a guard or cannon tower marks the
  first open scout-tower entry (0x40); a keep or stronghold every 0x98 entry, then the first open 0x4A entry; a castle
  or fortress every 0x98 and 0x99 entry, then the first open 0x4A entry; any other building the first open entry of
  its type with the race bit cleared. Loaded buildings make no creation marks. A new game never makes the load's
  building marks: its map buildings get the creation marks instead. A research or upgrade still running at the save
  isn't marked. A building working on an upgrade at the save marks as its current type (a great hall becoming a
  stronghold marks a 0x4A entry, not 0x98), and its upgrade's entry stays open like a running research's. Traced,
  rounds 93 and 94: `game/save-load.md` SL9 to SL12 and SL16 to SL20 (towers marking 0x40 entries, keeps and castles
  marking 0x98, 0x99 and 0x4A entries, construction sites marking, an upgrade level marking its entry, two running
  researches left open). Traced, round 95: a known spell marks its entry (SL26), and an upgrading building marks as
  its current type, leaving the upgrade's entry open (SL27).

Nothing else marks an entry done or clears its mark. In particular nothing marks a research entry done because the
player can't research it.

So the list works like a wish list: each entry stands for one building, and a lost building is replaced.

**The losses rule** and the other effects of a computer player's losses (they don't change done marks):

- When one of its buildings is destroyed, the rule runs inside the kill, at the start of the death, before the
  building leaves the player's counts. Let B be the player's buildings at that moment, **the destroyed one
  included**, complete or under construction, less its **complete** oil platforms (round 83, traced: an oil platform
  under construction counts). If B < 3, it launches the all-out attack
  (`ai/attack-ai.md`, "The all-out attack") on the human player with the most non-building units. So a player with
  exactly 3 buildings that are not platforms doesn't attack when it loses one, and does when it loses the next
  (traced: `xorc10-xorc10-losses-4000`, vectors below and `ai/computer-player.md` CP1, CP2). With 2 other buildings
  and a platform under construction, B = 3 and a loss starts nothing (traced: `orc3-orc03-platform-losses-9500`,
  vectors below).
- When one of its workers dies, the worker's job is released as in step 1 (the matching count goes down by 1).

### Repair

- **Limit:** a new repairer is only sent while repairers < trunc((W − 1) / 2) + 1, where W is the player's worker
  count (`ai/ai-scripts.md`). So 1 repairer for 1 or 2 workers, 2 for 3 or 4, and so on; with 0 workers the limit
  is 1.
- **Low stock:** if gold < 200 and lumber < 100, only buildings that store gold are considered.
- **Candidates:** the player's own units, in owner-list order (`core/unit-slots.md`), that are:
  - buildings, completed, and not dying, removed or hidden;
  - not oil platforms (PUD flag bit 11);
  - in the worker's region (the region value at the building's top-left tile equals the worker's);
  - below three quarters of their maximum: HP < trunc(3 × M / 4), with M the type's hit points from the unit data;
  - **safe:** in the 9 × 9 tiles from (x − 3, y − 3) to (x + 5, y + 5), where (x, y) is the building's top-left
    tile, no tile holds an enemy that can attack (a unit of a player the owner doesn't count as allied; the occupant
    isn't tested for dying). Each tile is tested like this (round 85; erratum 2, traced): if its ground layer
    is empty, the tile is safe and its air layer isn't looked at; if the ground unit is such an enemy, the building is
    unsafe; otherwise the air unit there, if any, is tested the same way. So an enemy flyer over open ground doesn't
    stop a repair (traced: `xorc10-xorc10-farm-repair-1700`, three enemy dragons over grass inside the area, the farm
    repaired; vectors below).
- **Choice:** the candidate with the highest "annoy computer" value from the unit data. It must be strictly higher
  than the best so far, which starts at 0, so the earliest of equal values wins and a type with value 0 is never
  repaired.
- The worker gets command 27 (repair, `economy/repair.md`) on it, and the repairer count goes up by 1.

### Harvest

**Lumber target.** With gold G, lumber L and W workers, the wanted number of lumberjacks T is the first row that
matches:

| Condition | T |
| --- | --- |
| G < 500 and W < 5 | 0 |
| L ≥ 2000 | 0 |
| G < 1000 and L ≥ 500 | 0 |
| G < 1000 (so L < 500) | floor(W / 2) |
| L ≥ 500 (so G ≥ 1000) | floor(W / 2) |
| otherwise (G ≥ 1000, L < 500) | floor(3 (W + 1) / 4) |

1. **Lumber** if lumberjacks < T: the worker switches to lumber mode and the tree search of `economy/harvest.md`
   ("Finding the nearest tree") runs from the worker's tile, but with a larger limit: rings of radius k are searched
   while 2k + 1 < floor(3N / 4), N being the map size (radius 47 on a 128 × 128 map). A worker coming out of a depot
   is still inside it, so the search starts from the depot's top-left tile (`economy/harvest.md`, "Entering a mine,
   platform or depot"). If a tree is found: command 23 on it, lumberjacks +1. Done. If no tree is
   found, the worker goes on to gold.
2. **Gold** otherwise, and also when the tree search found nothing: the worker switches to gold mode and finds the
   nearest depot as in step 3. Then, among gold mines that have trips left and stand in the worker's region, it takes
   the one that minimises **d(depot, mine) + floor(d(worker, mine) / 2)**. d(depot, mine) is in tiles as for target
   acquisition, from the depot's top-left tile. d(worker, mine) is the larger of the two axis distances between the
   **mine's top-left tile** and the worker's tile, not measured to the mine's nearest tile (traced:
   `mp-gold-rush-bne-25000`, vectors below). Only a strictly smaller value replaces the best, so the earliest in the
   list wins ties. Command 23 on
   that mine, miners +1, and a **keep-clear line** is marked from the depot to the mine (below).
3. If there is no depot or no mine, lumber is tried as in 1 (whatever T is).

The job choice is the only place where a computer worker is given a lumber job; the 50-cycle AI routines don't
assign workers.

### Keep-clear marks and site search

**Keep-clear mark:** a per-tile mark that the computer players put on planned building sites and mining paths. It is
the run-time tile mark 0x0400 in the tile flags of `movement/occupancy.md`. The computer's site search refuses tiles with it, so its buildings leave paths and a ring of
space. It isn't in the movement or ordinary placement masks, so it doesn't affect movement or human placement.

**Creation marks.** When a building is created for a computer player (the player kind of blocked rows A and B,
`movement/movement.md`, "Player kinds"), whether the map places it at load or it is started in play, it is marked by
its type. Human players, passive computer players and the neutral player get no creation marks. **An active rescue
player's map buildings get none either** (round 83): it becomes a computer player only after the map's units
are read (`game/setup.md`), so its buildings are created while it is still a rescue player (traced: the `human5` and
`human8` setup dumps, vectors below). Buildings it starts in play, as a computer player, are marked as usual.

| Types | Mark |
| --- | --- |
| Barracks, church and altar, stables and ogre mound, inventor and alchemist, aviary and roost, town hall and great hall, lumber mills, mage tower and temple, blacksmiths, keep and stronghold, castle and fortress (0x3C to 0x3F, 0x42 to 0x47, 0x4A to 0x4D, 0x50 to 0x53, 0x58 to 0x5B) | The keep-clear rectangle with the border around the footprint (below) |
| Shipyards, foundries, refineries (0x48, 0x49, 0x4E, 0x4F, 0x54, 0x55) | Every tile whose movement flags include 0x0002 (shore water, `movement/occupancy.md`) in the 13 × 13 square from (x − 6, y − 6), with (x, y) the building's top-left |
| Farms, scout towers, oil platforms, guard and cannon towers, gold mines, oil patches and every other type | Nothing |

The shore square is clipped like this: if x − 6 is below 0, the square starts at column 0 and loses that many
columns, and the same for rows. If its start plus its width reaches the map size N, its width becomes N − 1 − start,
so column N − 1 is never marked; the same for rows. The shore square's rows don't skew.

The creation mark is made at the same moment as the build list's done mark ("Done marks"). A building created
complete keeps the mark on its own tiles: the clearing in the job-undone table applies only to a building job. For a
site started in play by the computer's own job, the job has already marked the rectangle, so only a shore building's
square adds anything. Other marks come from the computer's building jobs and its keep-clear lines.

**Keep-clear rectangle.** Every keep-clear rectangle (the hall, farm and build-list jobs, and the creation marks) is
marked the same way, from a top-left (x, y) and a size s:

- **With the border:** (x, y) is the footprint's top-left minus 1 on each axis, except that a coordinate that is
  already 0 stays 0; s is the width + 2 either way. So at the left or top map edge the mark reaches 2 tiles past the
  footprint on the far side.
- **Without the border** (farms): (x, y) is the footprint's top-left, and s is the width.
- **Clipping at the right and bottom:** the width marked is s, or N − 1 − x if x + s ≥ N. The height is s, or
  N − 1 − y if y + s ≥ N. So column N − 1 and row N − 1 are never marked.
- **The skew:** each row after the first starts s − (width marked) tiles further left than the row above. When the
  width isn't clipped that is 0 and the rows line up. When it is, the rows shift left step by step. Tiles are counted
  in rows across the whole map, so a row that would start before column 0 continues at the right end of the map row
  above it. Clipping at the bottom doesn't skew.

For example, the bordered rectangle of a 3 × 3 building at (93, 23) on a 96 × 96 map starts at (92, 22) with size
5, width cut to 3, and marks (92 to 94, 22), (90 to 92, 23), (88 to 90, 24), (86 to 88, 25) and (84 to 86, 26): none
of the building's own tiles.

**Keep-clear line.** With the depot's top-left tile (dx, dy) and the mine's top-left tile (mx, my):

- If dx = mx or dy = my, nothing is marked.
- Otherwise the line walks from the depot's tile toward the mine's tile, one tile per step. Let D = the larger of
  |mx − dx| and |my − dy| (the major axis) and d the smaller. A counter starts at floor(D / 2), or 1 if that is 0.
  Each step moves 1 tile on the major axis; the counter then goes down by d, and if it is 0 or less the step also
  moves 1 tile on the minor axis and the counter goes up by D.
- The tile reached by each step is marked. The depot's own tile is not.
- The walk stops after marking the first tile that is in the mine's column (x = mx) or the mine's row (y = my). So
  the line often stops short of the mine, and marks nothing on the mine's footprint unless that tile is on it.

**Ordinary search.** A search has a **centre** and a **step** s that depend on the building's family:

| Family | Centre | Step s |
| --- | --- | --- |
| Most buildings | the top-left tile of the player's nearest gold-storing building, complete or not, in the worker's region (strictly nearest, so the earliest wins ties); the distance is the larger of the x and y differences between the building's **top-left tile** and the worker's tile, and there is no alive test, so a building in its death animation counts until it leaves the list (round 85; static gap-fill, erratum 1: this spec didn't give the measure; `ai/attack-ai.md`, "The nearest gold store", uses the same measure since round 86); the worker's tile if none | 6 |
| Farm | as above | 2 |
| Foundry, refinery | as above; no site if the player has no completed shipyard | 1 |
| Shipyard | the nearest town hall (any tier); the water region of the oil patch nearest to it is noted, and the footprint must contain a tile of that region (held: Open questions, "The shipyard's search centre", round 85) | 1 |
| Town hall (not from the build list) | the nearest gold mine with trips left in the worker's region, measured from the mine's top-left tile; with no such mine, the worker's own tile, searched as usual (round 85; static gap-fill, erratum 3) | 1 |

The search visits square **rings** of radius k·s around the centre, for k = 1, 2, …. Each ring is walked clockwise from
its top-left corner (right along the top, down the right side, left along the bottom, up the left side), in steps of
s; corners are visited twice. It continues while (2k + 1)·s < max(cx, N − cx, cy, N − cy) − 1, where (cx, cy) is the
centre and N the map size. If a ring has no position in the worker's region, the search fails.

Only the first try of a new town hall when no depot is found ("Worker", step 5) uses the border; every other search
uses the plain footprint.

A position (x, y) is accepted if:

- it is on the map and its region value equals the worker's;
- the **footprint** ends before the map's last column and last row: width × width tiles from (x, y), or, with the
  border, (width + 2) × (width + 2) tiles from (x − 1, y − 1) (from x or y itself where that is 0). For the plain
  footprint that means x + width and y + width are both less than the map's size, so a footprint may not touch the
  last column or the last row;
- no footprint tile carries a bit of the computer's placement mask: **0x0DDE** for land buildings, **0x0D11** for
  shore buildings. These are the masks of `movement/occupancy.md` plus the keep-clear bit;
- the site check of `ui/placing-buildings.md` ("The site check") gives **Allowed** for the building type at
  (x, y). Besides the placement masks (`movement/occupancy.md`, "Building placement masks") this applies "no shore",
  "too close" to a gold mine (gold stores, distance 3) or to oil (shore buildings), and "no oil patch". The
  "unexplored" outcome and the fog exception ("Hidden units don't block") apply only to the local player's builder,
  never to a computer worker.

This applies to every site search: the ordinary search, the new-mine search and the town hall searches.

The first accepted position is the site; the building's top-left is (x, y).

**New-mine search** (a town hall from the build list): every gold mine with **at least 50 trips** and no tile of a
gold-storing building within the 13 × 13 tiles from (mx − 5, my − 5) to (mx + 7, my + 7) (mine top-left (mx, my)) is
tried, in the neutral player's list order (`core/unit-slots.md`; every gold mine belongs to the neutral player). A
gold-storing building (town hall, keep, castle, great hall, stronghold, fortress) counts whatever its owner and even if
it isn't complete. A building counts if any of its tiles is in the square, not only its top-left tile (traced:
`xorc4-30000` n = 27870, round-65 vectors). For each mine, an ordinary search
with step 1 is made around it, and its site is kept only if the **distance from the site's top-left tile to the
mine** is at most 10 tiles, measured as for target acquisition (the same distance the mine choice in "Harvest" uses).
The mine whose site is nearest is chosen; only a strictly nearer site replaces the best, so the earliest mine wins
ties. The search is then run again at that mine to give the site.

### Tankers

The job choice runs at the same moments for a computer player's tanker (in order 4 too, round 84, traced: "When a
job is chosen"), and for any peon-flagged sea unit, which only custom unit data creates (which flag decides is held:
Open questions, "Worker or tanker choice", round 85):

1. **Attacker:** the reaction, as for a worker.
2. **Depot:** the nearest completed oil-storing building of the player whose footprint touches the tanker's water
   region (`economy/harvest.md`); on a tie the latest in the list wins.
3. **Carrying:** command 24 to that depot. If there is none, nothing happens.
4. **No depot:** nothing happens, and there is **no wait**, so the choice runs again the next time the idle logic runs.
5. **Platform:** among the player's own oil platforms that have oil left and are not dying, the one that minimises
   **d(depot, platform) + floor(d(platform, tanker) / 2)**, strictly (earliest wins ties). There is no region test.
   Command 23 on it.
6. **No platform:** the oil patch nearest to the depot's top-left tile (strictly nearest). The patches are visited in
   the neutral player's list order (`core/unit-slots.md`: newest first) and only a strictly nearer one replaces the
   best, so on a tie the **first in the list**, the most recently created patch, wins (round 85; static gap-fill,
   erratum 10). If an oil platform is affordable, the tanker gets the build command for it there,
   and the player pays at once.

## Data

| Item | Value |
| --- | --- |
| Idle wait | (r AND 15) + 44 cycles, one RNG draw |
| Hall cooldown after a hall is started | 10 tries |
| Job counts (miners, lumberjacks, repairers, each building type in progress) | 16-bit unsigned, +1 and −1 wrap; 0 at every game start, a load included (the units keep their jobs) |
| Farm rule | S + 4·F ≤ U + 3 and S < 200 |
| Repair limit | trunc((W − 1) / 2) + 1 repairers |
| Repair threshold | HP < trunc(3M / 4) |
| Repair safety area | 9 × 9 tiles, (x − 3, y − 3) to (x + 5, y + 5) |
| Low-stock repair rule | gold < 200 and lumber < 100: gold-storing buildings only |
| Lumber targets | table above (thresholds 500, 1000, 2000 gold/lumber, 5 workers) |
| Site steps | 6 most buildings, 2 farm, 1 shore buildings and halls |
| Job-choice tree search | rings while 2k + 1 < floor(3N / 4): largest radius 11, 23, 35, 47 for N = 32, 64, 96, 128 |
| Computer placement masks | 0x0DDE land, 0x0D11 shore |
| Keep-clear bit | 0x0400 |
| New-mine search | mines with ≥ 50 trips; no gold-store tile in (−5, −5) to (+7, +7); site within 10 tiles; nearest site wins |
| All-out attack threshold | at the kill: buildings, complete or under construction (the destroyed one included), less complete oil platforms, < 3 |
| Build-list length | min(slot 0x22, 64) entries |

"Annoy computer" values of the buildings in the default unit data (both races alike):

| Value | Buildings |
| --- | --- |
| 50 | scout towers, guard towers, cannon towers, castle / fortress |
| 45 | farms, town hall / great hall, walls |
| 40 | keep / stronghold |
| 35 | barracks, church / altar of storms, runestone |
| 20 | inventor / alchemist, aviary / roost, shipyards, foundries, mage tower / temple, blacksmiths, refineries, oil platforms |
| 15 | stables / ogre mound, lumber mills |
| 0 | gold mine, oil patch, start locations, circle of power, dark portal |

### Spell bits for done marks at game start

An entry is marked done at the start of a game when either bit of its code is set in the player's spells-known word
(`economy/tech-tree.md`).

| Code | Bits (human, orc) |
| --- | --- |
| 0x90 | 20 (the same bit for both) |
| 0x91 | 1, 11 |
| 0x92 | 3, 18 |
| 0x93 | 4, 13 |
| 0x94 | 6, 16 |
| 0x95 | 7, 15 |
| 0x96 | 8, 17 |
| 0x97 | 9, 19 |

## Test vectors

| Input | Expected |
| --- | --- |
| `xorc7-7000` (oracle trace), the computer's farm site search started at n = 5866 (peasant 1416) | Farm 1415 (type 0x3A) at (86, 116), the first site of ring 3; it appears at n = 6016 |
| 128 × 128 map, farm (2 × 2) at (88, 126) | Refused (126 + 2 = 128, the map's size) |
| Same, at (88, 125) | Accepted if the other conditions hold |
| Computer peon, idle, not carrying, depot reachable, supply 5, food in use 3, no farm in progress, farms allowed, player had a hall, can afford | Farm: 5 + 0 ≤ 3 + 3, so a pig farm is placed, paid at once, farms in progress 1 |
| Same, supply 9, food in use 5 | 9 > 8: no farm; goes on to the build list |
| Same, supply 5, food in use 3, one farm in progress | 5 + 4 = 9 > 6: no farm |
| Enhanced, supply 200 | No farm, whatever the food in use |
| Build list barracks, lumber mill, 0xFF; slot 0x22 = 1; barracks done | Entry 1 is not looked at (limit 1): no build-list job |
| Build list farm, 0x80 + n, barracks; nothing done | Entry 0 (farm) built if a site is found; next worker: entry 0 in progress so skipped, entry 1 requests research n and the scan ends |
| Build list entry barracks, not affordable; entry after it a farm, affordable | Scan ends at the barracks; the farm is not built |
| Build list entry blacksmith, no site found; next entry barracks with a site | Barracks is built |
| A done barracks is destroyed | Its entry's done mark is cleared; a worker builds it again |
| Build list barracks, barracks (both done); one barracks is destroyed | Entry 0 loses its done mark; entry 1 stays done |
| Depot top-left (10, 10), mine top-left (14, 11) | D = 4, d = 1, counter 2. Step 1: (11, 10), counter 1. Step 2: counter 0, so (12, 11), counter 4. (12, 11) is in the mine's row: stop. Marked: (11, 10), (12, 11) |
| Depot top-left (10, 10), mine top-left (13, 16) | D = 6 (y), d = 3, counter 3. Step 1: counter 0, so (11, 11), counter 6. Step 2: (11, 12), counter 3. Step 3: counter 0, so (12, 13), counter 6. Step 4: (12, 14), counter 3. Step 5: counter 0, so (13, 15), in the mine's column: stop. Marked: (11, 11), (11, 12), (12, 13), (12, 14), (13, 15) |
| Depot top-left (10, 10), mine top-left (10, 20) | Same column: nothing marked |
| New-mine search: mine A's site is 6 tiles from A, mine B's site 4 tiles from B, mine C's site 11 tiles from C | C is out (over 10); B is chosen |
| Classic, supply 210, food in use 209 | No farm (S ≥ 200) |
| T = 2, 0 lumberjacks, no reachable tree, a depot and a mine | Tree search fails; the worker mines gold |
| Computer player with 3 buildings (one a complete oil platform) loses a farm | B = 2 at the kill (the farm and the other building; the platform left out): the all-out attack starts |
| Computer player with 3 guard towers loses one (`xorc10-xorc10-losses-4000`, n = 1358) | 3 at the kill: no all-out attack |
| Same player loses a second tower (n = 1543) | 2 at the kill: the all-out attack starts at the next pass (n = 1551) |
| 7 workers, gold 400, lumber 300 | First three rows don't match (W ≥ 5; L < 2000; L < 500); fourth row: T = floor(7/2) = 3 |
| 4 workers, gold 400 | T = 0: all mine gold |
| 8 workers, gold 1500, lumber 100 | T = floor(27/4) = 6 |
| 8 workers, gold 1500, lumber 2500 | T = 0 |
| 2 lumberjacks, T = 3, a tree is found | Command 23 on the tree; lumberjacks 3 |
| T = 0, depot at distance 10 from mine A, worker 4 from A; mine B 8 from depot, 12 from worker | A: 10 + 2 = 12; B: 8 + 6 = 14: mine A |
| 6 workers, 3 repairing | Limit trunc(5/2) + 1 = 3: no new repairer |
| Damaged farm (300/400 HP) | 300 ≥ 300: not a candidate |
| Farm 299/400 and barracks 500/800, both safe, plenty of stock | Barracks 500 < 600 is a candidate too; the one with the higher annoy value (farm 45, barracks 35) wins: farm |
| Same, gold 150, lumber 50 | Only gold-storing buildings: neither is repaired |
| Damaged farm with an enemy grunt 4 tiles right of its top-left | Inside (x + 5): not repaired |
| Peon carrying gold, no depot, halls allowed, cooldown 0, can afford, site found | Builds a great hall; cooldown 10 |
| Same, cooldown 10 | Cooldown → 9; no hall; carrying, so it waits 44 to 59 cycles |
| No job at all | Wait (r AND 15) + 44 cycles; one RNG draw |
| Computer tanker, empty, an oil refinery touching its water, two platforms with oil: P1 depot 6 and tanker 10, P2 depot 9 and tanker 2 | P1: 6 + 5 = 11, P2: 9 + 1 = 10: P2 |
| Computer tanker, no oil depot touches its water | Nothing; no wait |
| Computer tanker, depot, no platform, oil patch 7 tiles from the depot, platform affordable | Builds an oil platform on that patch, paid at once |
| Computer player, build list 0x93, 0x90, 0x91, 0x94; limit 7; a temple, no altar; raise dead already researched | Every scan skips 0x93 (done), sets the request for 0x90 and ends. The requests for 0x91 and 0x94 are never set, so the temple never researches haste |
| Same list; the player gets an altar and ogre-mage research starts | 0x90 marked done; the next scan reaches 0x91 |
| Map gives a computer player a castle; build list 0x4A, 0x98, 0x99, 0x4A (static study only) | No entry is marked by the castle (a castle entry would be) |
| `xorc12-ogre-mage-180` (oracle trace), player 3 (human computer): the map gives it a town hall, two barracks, a lumber mill, a blacksmith, two mage towers, three guard towers, three cannon towers and eleven farms; its build list starts town hall, barracks, lumber mill, blacksmith, scout tower, barracks, 0x98 | Entries 0, 1, 2, 3 and 5 are done at n = 1; entry 4 (scout tower) stays open. n = 4: peasant 1540 takes order 28 for a scout tower; player 3's gold 4000 to 3450 and lumber 3000 to 2800 |
| Computer player whose map gives it a keep; build list starts 0x4A, 0x98 (static study only) | Neither entry is marked by the keep (a keep entry would be) |
| Spells known at game start 0x4020 (the setup default); build list 0x90 | Not marked done (bit 20 isn't set) |
| 128 × 128 map; a computer worker comes out of its town hall (top-left (12, 71)) and lumber is wanted; the nearest reachable forest is 20 tiles away | The job choice finds it (radius 20 ≤ 47) and gives command 23 |
| Same, on a 32 × 32 map, nearest reachable forest 12 tiles from the depot's top-left | Not found (largest radius 11); the worker tries gold |
| Computer player with only a town hall under construction; a worker looks for a farm site | The search centres on the unfinished hall's top-left tile |
| Computer player's town hall (4 × 4) placed by the map at (85, 2), 96 × 96 map | Keep-clear on x 84 to 89, y 1 to 6, footprint included |
| Computer player's church (3 × 3) placed by the map at (80, 0) | Keep-clear on x 79 to 83, y 0 to 4 (the top edge keeps the size) |
| Computer player's farm placed by the map | No keep-clear mark |
| Human player's barracks placed by the map | No keep-clear mark |
| `orc09` setup (map dump at n = 0): computer player 0's shipyard at (84, 63) | The 22 shore-water tiles in x 78 to 90, y 57 to 69 are marked; nothing else from it |
| `orc09`, `human12` and `human10` setup map dumps (n = 0) | Every keep-clear bit matches the creation marks of the computer players' map buildings: 325, 334 and 236 tiles, no difference |
| `orc09-repair-600` (oracle trace) n = 2: computer peasant 1587 (type 0x02) on (89, 4) is told to build a farm; town hall 1592 at (85, 2), church 1597 at (80, 0), both from the map | The farm search (centre (85, 2), step 2) refuses all of ring 1 and ring 2 up to (83, 6), which touches (84, 6); the site is (81, 6). Without the creation marks (83, 0) would be taken. n = 5: (90, 5), facing 3. Farm 1473 appears at (81, 6) at n = 149 |
| Bordered rectangle, footprint 3 × 3 at (93, 23), N = 96 | Marked (92–94, 22), (90–92, 23), (88–90, 24), (86–88, 25), (84–86, 26) (the `orc09` stables, as in the dump) |
| `human12` setup: troll lumber mill (3 × 3) at (125, 12), N = 128 | Rows (124–126, 11), (122–124, 12), (120–122, 13), (118–120, 14), (116–118, 15); none of its own tiles, as in the dump |
| Bordered rectangle, footprint 3 × 3 at (0, 0) | Marked x 0 to 4, y 0 to 4 |
| Bordered rectangle, footprint 2 × 2 at (10, 10), N = 64 | Marked x 9 to 12, y 9 to 12 (no clipping) |
| Farm rectangle (no border), footprint 2 × 2 at (62, 5), N = 64 | Marked (62, 5) and (61, 6) only: the width is cut to 1, and the second row starts 1 tile further left |
| Hall (4 × 4) site (43, 72); gold mine (3 × 3) at (40, 73), so columns 40 to 42 and rows 73 to 75 (ends 43 and 76) | x: 40 − 3 = 37 < 47 and 43 − 3 = 40 < 43; y: 73 − 3 = 70 < 76 and 72 − 3 = 69 < 76. Too close: refused |
| Hall site (40, 79), same mine | y: 79 − 3 = 76, not below the mine's end row 76. Not too close: accepted if the other tests pass |
| `xhuman10-spells` and `xhuman10-blizzard` (oracle traces): player 2's peon 1553 looks for a new-mine hall site. The site at the mine at (16, 116) is refused by the keep-clear ring of player 6's barracks 1424 (type 0x3D) at (21, 113); mine 1514 is at (40, 73) | (43, 72) is refused as too close to mine 1514. Site **(40, 79)**: the great hall (type 0x4B) stands there by n = 551, slot 1410 in `xhuman10-spells` and 1411 in `xhuman10-blizzard` |
| Computer peon carrying wood gets command 24 to its hall and the path search returns no steps | Its order ends; the next job choice gives command 24 to the same hall again, with no wait |
| `orc5-10000` (oracle trace), n = 1641: computer player 1's only hall (1511, type 0x4A) is still under construction; peasant 1564 (type 0x02) comes out of 1565 and a farm is wanted | Farm started: 500 gold and 250 lumber paid at n = 1641; 1564 in order 28 at n = 1666 |
| `human12-25000` (oracle trace): tanker 1412 (type 0x1B, owner 0) goes idle at n = 12948 when its platform collapses; at n = 12951 the job choice gives it a harvest on platform 1493 at (87, 59) | A new route: at n = 12954 tile (46, 24), facing 3, order 23 (the old route would step west to (42, 22)) |
| `xhuman2-10000` (oracle trace), n = 2487: computer player 0 with supply S = 21 and 18 units that aren't buildings, 2 of them (1579 and 1583) dying | U = 16, so 21 > 16 + 3: no farm. It goes on to its build list: a scout tower is paid (550 gold, 200 lumber) by peon 1588; tower 1554 (type 0x41) appears at (20, 2) at n = 2707 |
| A player with 10 units that aren't buildings, 2 of them dying | Food in use 8 |
| Depot top-left (11, 49), worker at (11, 55), gold mine top-left (9, 43) | Depot term 4, worker term max(2, 12) = 12: score 4 + 6 = 10 (not 4 + 5 = 9) |
| `mp-gold-rush-bne-25000` (oracle trace, Enhanced), n = 6461: computer peon 1505 comes out of lumber mill 1517 at (11, 55); its depot is great hall 1534 (11, 49). Mines 1523 (18, 55), 1528 (18, 52), 1533 (18, 49), 1548 (9, 43), 1549 (12, 43) and 1550 (15, 43) all score 10, and 1523 is first in the list | Mine 1523: placed at (14, 55), facing 2, order 2 with target 1523. With the old worker term 1548 scored 9 and won |
| Classic mode: `mp-gold-rush-bne-classic-8000` n = 6461 (peon 505, mine 523) and `mp-gold-rush-bne-classic-20000` n = 19728 (peasant 586 in town hall 590 at (11, 11): mine 582 (18, 15) scores 7 + 3 = 10, mine 575 (3, 18) 7 + 4 = 11) | The same choices as the Enhanced rule gives (582 at n = 19728). Classic-mode evidence only agrees with the rule; it isn't its source |

### Round-65 vectors

| Input | Expected |
| --- | --- |
| `xorc4-30000` (oracle trace, Enhanced, no input), n = 27870: computer peasant 1478 (type 0x02, owner 3) comes out of keep 1512 (type 0x58) with a town hall build job. Gold mine 1525 (type 0x5C) has top-left (66, 62), so its square runs from (61, 57) to (73, 69); castle 1535 (type 0x5A, owner 1, 4 × 4) has its top-left at (60, 54), outside the square, and its tiles (61, 57) to (63, 57) inside. Gold mine 1508 (500 trips) is at (96, 99) | Mine 1525 is skipped. The site is (89, 105), next to mine 1508 (a break capture on the build command, whose log isn't committed). The trace shows order 28 from n = 27895 and the peasant walking east from (27, 92) along rows 92 to 94; counting only the castle's top-left tile, the site would be (72, 56), next to mine 1525, to the north-east |

### Round-83 vectors

| Input | Expected |
| --- | --- |
| `orc3-orc03-platform-losses-9500` (oracle trace, Enhanced, campaign launch): computer player 1 has farms 1589 and 1595 and oil platform 1584 under construction (463 of 650 HP); farm 1595 is destroyed (n = 6765) | B = 3 (two farms and the unfinished platform): no all-out attack; no unit of player 1 gets order 4 at the passes of n = 6801 to 7601 |
| Same trace: platform 1584 completes (n = 7490), then farm 1589 is destroyed (n = 7634) | B = 1 (the complete platform left out): all-out attack, order 4 from the pass of n = 7651 (7 units by n = 7655) |
| `xhuman5-xhuman05-worker-all-out-goals-1100`: computer player 2's farm 1582 is destroyed (n = 754; B = 2); peon 1575 walks back from the mine carrying gold | Smart move at n = 754 (pending 3, to the knight's tile (9, 19)); command 4 at the pass of n = 801 (to (9, 20), 8 tiles away; leash 7); at n = 804, its next order-logic run, return goods to great hall 1579 (pending 24, order 24 at n = 826) |
| Same trace, the passes of n = 851 to 1051 | No command 4: the peon is within its leash at each (6 tiles from (9, 20) at n = 851, entering the hall; inside it at (8, 23), 3 tiles, to n = 1001; 2 tiles at n = 1051), so these passes don't show whether its attack goal is kept (closed in round 84 by later passes: `ai/attack-ai.md`, "Attack (2)"). It comes out of the hall at n = 1004 with a farm job |
| `human5` setup dump (n = 0): active rescue player 0's town hall (40, 96), barracks (46, 92), lumber mill (34, 109) and blacksmith (37, 105) | No keep-clear bit on or around them; computer player 5's buildings have their rectangles and shore squares (152 tiles) |
| `human8` setup dump (n = 0): active rescue player 2's town hall (74, 72) and blacksmith (66, 71) | No keep-clear bit; all 329 marked tiles are computer players 0, 1 and 3's creation marks |

### Round-84 vectors

| Input | Expected |
| --- | --- |
| `xhuman5-xhuman05-worker-order12-goals-3000` (oracle trace, Enhanced): computer player 2 has lost its great hall (n = 1041); peon 1575 is full of lumber with no depot and idle within its leash at the pass of n = 2501 | Command 12 on knight 1588 (order 12 at n = 2502); the job choice at its order-12 runs, each a step of its walk (n = 2505, 2527, 2607, 2687, 2752, 2824, 2891, 2958): carrying, no depot, so no job and the wait. At n = 2505 the wait's draw is made but the step starts the move animation: wait 1. From n = 2527 the waits are 59, 59, 44, 51, 46, 46 and 44, one tile per 65 to 80 cycles |
| `orc3-orc03-tanker-order4-goals-7500-10500` (oracle trace, Enhanced): computer tanker 1591 (owner 1), idle and empty at (58, 2), its platform 1584 destroyed at n = 8580 (all-out attack) | Smart move at n = 8580; command 4 at the pass of n = 8601 (goal (9, 46)), order 4 at n = 8616; the job choice at n = 8619 and at each step after (36 runs in order 4, to n = 9808); nothing carried and no depot, so it goes on in order 4 with no wait ("Tankers", step 4). Idle at (8, 46) from n = 9840, every 5 cycles in order 2 |
| `xhuman5-xhuman05-worker-all-out-goals-3000` (oracle trace, Enhanced), n = 2461 to 2464: peon 1575, given command 4 at the pass of n = 2451 while walking to gold mine 1573, takes order 4 at n = 2461 (wait 3) at (6, 29), beside the mine | At n = 2464 the job choice gives harvest at 1573 and order 23 takes over in the same update (animation 4, wait 3), not at the next step done; order 25 at n = 2467 |
| `human5-30000` (oracle trace, Enhanced, no input): active rescue player 0 (empty build list) with its map town hall, barracks, lumber mill, blacksmith and farms | No building started in 30000 cycles, whether or not the map buildings marked entries done; its lost barracks, hall and farms aren't rebuilt |

### Round-85 vectors

| Input | Expected |
| --- | --- |
| `xorc10-xorc10-farm-repair-1700` (oracle trace, Enhanced; erratum 2): computer player 6's farm 1437 (top-left (46, 115), 233 of 400 HP); enemy dragons 1561, 1562 and 1575 at (46, 112), (50, 118) and (50, 116), over grass with nothing on the ground below; peasant 1448 comes out at n = 1441 | Repair: target farm 1437 at n = 1441, order 27 at n = 1466 (by the ground-or-air reading: no repair, target gold mine 1445) |
| `xhuman5-xhuman05-worker-all-out-aigoals-goals-2500` (oracle trace, Enhanced; erratum 7): peon 1575, attack goal (9, 20), takes a farm job at n = 1004 | Goal tile (6, 19), kind still 2, at n = 1004, 1080 and 2291 (`aigoals` extra value 59) |
| A computer peasant at (16, 12); its player's 4 × 4 town halls at (10, 10) and (20, 10), both in its region, in that list order (static, erratum 1) | Distances from the halls' top-left tiles 6 and 4: the hall at (20, 10) is the farm search's centre |
| A computer peon with no depot, every mine in its region exhausted, the player able to afford a great hall (static, erratum 3) | The hall search centres on the peon's tile; a site near it is used, with no wait draw |
| A no-depot hall whose bordered search fails and whose unbordered search finds (20, 20), a 4 × 4 hall (static, erratum 11) | Keep-clear tiles (19, 19) to (24, 24) |
| A bordered site test for a 4 × 4 hall at x = 0 (static, erratum 16) | Columns 0 to 5 tested (w + 2 = 6 wide from 0) |
| Two oil patches at the same distance from a computer tanker's depot (static, erratum 10) | The patch first in the neutral player's list (the one created later) is chosen |

### Round-95 vectors

| Input | Expected |
| --- | --- |
| `xhuman8-xhuman8-r95-workers-saveload-aigroups-aiworkers-game-1500` (oracle trace, Enhanced), loaded game n = 38: computer peon 1455 (player 3) comes out of town hall 1496 with its saved lumber job; lumberjacks 0 (cleared by the load), 12 workers, gold 150, lumber 125, so T = 6 | Release: lumberjacks 65535; 65535 isn't < 6, so gold: command 23 on mine 1481, miners 1 (`game/save-load.md` SL23) |
| A computer player's farm in progress at the save, finished after a load, no other farm started (static study for the test; the wrap is traced, SL25) | Farms in progress 0 → 65535; the next farm test reads S + 4 · 65535 as S − 4 (S ≥ 4): with S 20 and U 14, 16 ≤ 17, so a farm is wanted, where a right count (0) would give 20 > 17 and no farm |
| A computer player's lumberjack count wrapped to 65534, T = 9, its depot and a mine reachable (static study) | No lumber job from step 1; the worker mines |

### Round-98 vectors

| Input | Expected |
| --- | --- |
| A computer peon with a farm job for a site at (20, 30), keep-clear marked on (20, 30) to (21, 31), walking to it; its goal tile is (20, 30). It releases the job at its next choice (static study, W1) | The mark is cleared on (20, 30) to (21, 31), as before |
| The same peon, but before the release the all-out attack sets its goal tile to (60, 10) and gives it command 4 (static study, W1) | The release clears (60, 10) to (61, 11), whatever marks are there; (20, 30) to (21, 31) stay marked. Pig farms in progress −1 |
| A computer peasant with no depot in its region, hall cooldown 0, no town hall in progress, enough gold and lumber, a site found (W2; traced: `mp-r99-tests-mp-r99-tests-fixed-aigoals-airesearch-aiworkers-5400`, generated map `R99-tests.pud`, player 3 with two peasants, a gold mine, no hall, 2000 gold and 1000 lumber) | Build command for a town hall; `aiworkers` `jobs` shows job 16 for it; town halls in progress +1; hall cooldown 10. Traced at n = 2 (a start-up record): peasant 1583 `jobs` `[16, 0]`, `building` `"3:0x4a"` 1, `hall_cooldown` 10, gold 2000 → 800 and lumber 1000 → 200 (paid at once); its AI goal tile (52, 20), extra 0x4A. The hall's site appears at (52, 20) at n = 181 |
| The same trace, player 3's other peasant 1584, not carrying, no depot, at each of its job choices while the cooldown runs | Cooldown 10 → 9 at n = 6, then one down per choice (n = 59, 110, 171, 230, 283, 338, 400, 456, 517: 8 … 0); no hall is tried; no repair or harvest job, so it waits (wait 49 at n = 6, 47 at n = 59, 57 at n = 110, each within 44 to 59). From n = 181 a hall is in progress too; once the cooldown is 0 no second hall is started (none to n = 5400) |
| `xhuman12-xhuman12-picker-flip-aigoals-1700` (oracle trace, Enhanced): orc peon 1487 (player 0) takes a build-list job at n = 2 | `ai_goal` tile (86, 34), `extra` 76 (the human lumber mill's number); troll lumber mill (77) at (86, 34) at n = 117 |

### Round-104 vectors

| Input | Expected |
| --- | --- |
| **SW7a.** `sweep-xorc9` (oracle sweep trace, Enhanced, kept outside the repository; reproducible with `oracle/sweep.py capture`; Beyond the Dark Portal orc 9, mode Q; the map has no `ALOW` section), the setup record: player 1, a computer player, whose build list has entries 21, 22 and 23 with codes 0x90, 0x91 and 0x92; after the mission start it knows spell bits 1, 3 and 20 | Player 1's done marks: none (still none at n = 3000). With the research fields left out, the trace's other fields match a build that marks 21, 22 and 23 up to n = 60000 |

### Round-106 vectors

| Input | Expected |
| --- | --- |
| **SW7b.** `mp-r99-tests-mp-r99-tests-fixed-aigoals-airesearch-aiworkers-5400` (oracle trace, Enhanced, committed in round 99; generated map `R99-tests.pud`): the map's `ALOW` section grants computer player 2 invisibility (spells-known word 128); player 2's build list has entry 32 with code 0x95 (invisibility). Computer player 3 has the same list and no grant | Setup record: player 2's done marks 0, 14 and **32**; player 3's none |

## Open questions

- **Game-start marks and the map's own sections** (round 104, SW7): **closed in round 106.** `ALOW` is a header
  section (`game/setup.md`, step 2), read before the game-start marks of step 4, so its spells are marked (traced:
  SW7b). The body sections read after the marks (`game/setup.md`, step 6: the tile, movement, oil and region maps
  and `UNIT`) give a player no spell, research or level, so nothing is left open.

- **Worker or tanker choice** (round 85; held, erratum 6). Static study reads the job-choice dispatch as
  testing the **worker** flag first: any worker takes the worker's job choice (a sea worker only gets the oil-depot
  search for its depot step), and only a tanker-flagged unit that isn't a worker takes the tanker's choice; at a
  come-out a worker takes the worker's choice and every other unit the tanker's. That contradicts "Tankers" as stated
  (a peon-flagged sea unit takes the tanker's choice). Only custom unit data can show it: the default tankers aren't
  workers. Default: as stated. No trace requested.
- **The shipyard's search centre** (round 85; held, erratum 8). Static study reads the shipyard row as: centre
  the **nearest gold store** (as the "Most buildings" row, with its region test), or the worker's
  own tile if there is none; the water region is that of the complete oil patch **nearest the worker** (tile to
  footprint, as "Distance"), a later patch in the neutral list winning a tie. That contradicts the stated row (the
  nearest town hall, and the patch nearest it). In the committed traces all 39 computer shipyard searches give the same
  site under both readings (`ERRATA-round85.md`, AA4). Default: as stated. Trace request (AA4b): a computer base with
  oil in two water regions, the worker nearer a patch in the other region than the hall's nearest patch. Tried in
  round 86 and skipped: no shipped setup, and commands can't start a computer shipyard search
  (`ERRATA-round86.md`, AA4b); off the trace queue, the default stays.
- **Job choice in other acquiring orders** (round 82, C2): closed for order 4 in round 83 (AI1) and **for order 12
  and a tanker in order 4 in round 84** (AI8, `ERRATA-round84.md`). The other acquiring orders are a labelled static
  gap-fill, unreachable for a computer worker in the traces.
- **An unfinished oil platform in the losses rule** (round 82): **closed in round 83** (AI5): it counts.
- **An active rescue player's map units** (round 83): **not decidable in shipped missions** (round 84, AI7). The
  defaults (no done marks, no first goal) stay as untraced defaults; by the stated rules they change no outcome in any
  shipped mission (no shipped active rescue player has both map buildings and a build list, and all such players' map
  soldiers are held). Only custom data can show them; no trace is requested.
- **The first order-12 run** (round 84, AI8): **closed by the round-84 review**: the draw at n = 2505 with wait 1 is
  the stated rules (the choice before the walk's step, whose start of the move animation sets the wait to 1; "The
  wait").
- **An order-4 job already in reach** (round-84 review, AI11): at n = 2464 of `xhuman5-xhuman05-worker-all-out-goals-3000`
  the job's order takes over in the same update ("When a job is chosen"). Reading: the walk's arrival test finds the
  new target in reach, the walk ends, and order 4's arrival takes over the pending job instead of idle. Only the
  outcome is traced; the stated outcome governs.
- "Prerequisites met" in the build list: the rules are in `economy/tech-tree.md`. Whether the computer applies them
  even when requirements aren't enforced must be checked. Default: it uses the same rules as the player's interface.
- Confirm in the running game that a worker whose tree search fails mines gold in the same choice (default: it does,
  as described above).
- Confirm the keep-clear line in the running game, for example by watching where a computer player's later buildings
  avoid a diagonal strip between its hall and a mine.
- The general placement check adds rules for the site (beyond the masks) that are in `movement/placement.md`; their
  interaction with the border option is assumed to use the unbordered footprint.
- That a construction site placed in play marks its entry when construction starts (not when it ends) comes from
  the static study only; no trace shows it yet.
- Creation marks by type: the setup dumps confirm the rectangle for town hall, keep, barracks, church, stables,
  inventor, lumber mill and blacksmith, the shore mark for shipyard, foundry and refinery, and no mark for farms,
  scout towers and guard towers. The other types' marks (aviaries, roosts, mage towers, temples, castles, oil
  platforms and the rest) come from the static study only.
- That a computer site started in play gets the creation mark too (so a shore building adds its shore mark) is from
  the static study only.
- Whether clearing a keep-clear footprint (the job-undone table) has the same right-edge skew: **answered in round
  98** (static study): no; the clear clips to N − x with no skew (step 1). Unreachable with the site search's rules.
- A worker inside a depot starts the job choice's tree search from the depot's top-left tile and uses that tile's
  region value. This assumes region values ignore buildings, as `movement/occupancy.md` implies; not checked
  separately.
