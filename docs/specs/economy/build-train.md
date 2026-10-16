# Economy: construction, training, research and food

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11; consolidation reviewed by the spec-review
  session, 2026-10-16; round 77 and the B-040 audit reviewed by a spec-review session, 2026-10-16).
  **Consolidated 2026-10-16 into one model (B-038).** Every rule and vector of the earlier text
  is kept; the vectors have stable IDs (V01 to V49, in their old order) and are grouped by topic. Four traced changes
  of meaning (a builder dies with a killed site; the job's progress bar starts at 1; in `ui/sounds.md`, the
  building-upgrade completion sound is for halls only and the construction one plays for a platform's tanker too) and
  the static findings are listed in `specs/ERRATA-buildtrain-consolidation.md`, with a map from every earlier rule and
  vector to its place here. The earlier text is tidewater-docs `03c64f7`.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the animation scripts in
  `script.bin`, the public PUD specification text (unit data and upgrade data sections), and oracle traces of the
  running game. Each rule says which: **traced** (an oracle trace, named in the rule or in a vector), **static study**,
  or **game data** (unit data, upgrade data or the animation scripts). The erratum rounds each rule came from are
  listed under "History" at the end.
- **Changes:** 2026-10-16 (round 76, B-038, `ERRATA-buildtrain-consolidation.md`, reviewed): one model; the job
  start procedure moves here from `economy/tech-tree.md` (which keeps each job kind's own check); the "valid when"
  column of the job table is replaced by a pointer to that check; a killed building kills the units inside it
  (traced, V50); the progress shown for a job is the mana byte, 1 from the start (traced, V52); the completion sounds
  of a building upgrade and of a platform's tanker (V51, V53) are recorded in `ui/sounds.md`; static findings on
  cancel timing, the take-down of a site, a full unit table and refunds are added as statements or open questions.
  Review: the pay-on-arrival affordability check stays before the site check (the earlier order), with the static
  reading as an open question; a take-down clears the inside units' target before its kill; V52's numbers name the
  build-speed cheat. (Round 77 reversed the arrival order on traced evidence, V61.)
  2026-10-16 (round 77, `ERRATA-round77.md`, reviewed by a spec-review session, 2026-10-16): traces of the cancel
  commands settle three open questions. A builder let out of a cancelled site faces the site's top-left tile and
  becomes idle as a pending order (V54, V55); a cancelled job ends at its next work run, as stated (V57 to V59); on
  arrival a failed site check wins over a lack of money, and the idle for a lack of money takes over in the same
  update (V60, V61). A worker inside a killed depot dies with it (V62). The construction's mana byte changes only at a
  step (V63). Defaults for the open questions on summoned units' food and the Classic-mode builder's delivery.
  2026-10-16 (B-040 build and train audit, `ERRATA-audit-buildtrain.md`, static study, reviewed by the same session):
  a new site is idle first and takes order 33 as a pending order (closes "Order at creation", V64); the build-speed
  cheat makes every pass a step (V65); a rescued hall sets "ever had a hall" (V66); entering a site forgets the
  attacker and gives the entry sight (V67); the arrival site check is the full one; a full unit table shows its own
  message; a builder loses a load its new store doesn't accept (V68); the two cancel commands (V69); a building
  upgrade's hit-point gain isn't clamped; the command-button refresh. Held as open questions: the fast first hall read
  on each pass (Enhanced multiplayer) and the unit-ready event position.

## Behaviour

Order numbers refer to `orders/orders.md`. "Build time", "cost" and "hit points" are the unit data (UDTA) fields of
the type being made; upgrade time and costs come from the upgrade data (UGRD). Repair is specified in
`economy/repair.md`. What may be trained, researched or become what, and what research does when it finishes, is in
`economy/tech-tree.md`.

### The model

A building made during play goes through three states. A small set of events moves it between them, and each event
is one procedure below.

| State | Order | Mana byte | Frame | Units inside |
| --- | --- | --- | --- | --- |
| **Under construction** | 33 | the construction's shown progress | 0, then 1 | its builder (order 22), or the tanker of an oil platform |
| **Complete, resting** | 33 (a guard or cannon tower: 14) | 0 | 0 | no builder (workers delivering goods may be inside a depot, `economy/harvest.md`) |
| **Working** on one job | 37 | the job's shown progress, 1 at the start | 0, or 1 during a building upgrade | none |

| Event | Procedure | From → to |
| --- | --- | --- |
| A worker arrives at its site in order 28 | "Placing a building" | (none) → under construction |
| The construction pass reaches the end | "Construction", "Completion" | under construction → complete |
| A cancel-construction command | "Cancelling construction" (the building is **taken down**) | under construction → gone |
| A job command | "Starting a job" | complete → working |
| The building's work animation | "Job progress" | working → working, or → complete when it ends |
| A cancel-job command | "Cancelling a job" | working → complete, at the next work run |
| The building is killed (damage, or any other kill) | "A building killed" | any → gone |

Four rules are shared by these procedures:

- **Paying.** Every cost goes through one **cost lookup** for the thing being made (a unit type, a building type or
  an upgrade). The affordability check ("Can the player afford it?") makes the lookup; paying takes the looked-up
  cost; a refund gives back the cost looked up **at the moment of the refund**, in full. The one exception is
  cancelling construction, which gives back 75 % of the building's stored cost. (Static study. It is why a barracks
  whose job type was converted refunds the new type's cost, `economy/tech-tree.md`, "Converting units".)
- **Two ways a building goes.** A building is either **taken down** (cancelling construction; also the collapse of an
  exhausted mine or platform, `economy/harvest.md`, "Mine and platform amounts") or **killed**. Taking down first
  clears the target of every unit inside whose target is the building and cuts its wait to 1; each builder among
  them (order 22) is let out at once, and any other unit (a worker in a collapsing mine or platform) comes out at its
  next chance, as `economy/harvest.md` says ("Collapse"). Then it kills the building, which kills nobody inside,
  since no unit inside has it as its target any more (static study). A kill by any other cause kills every unit
  inside whose target is the building (traced: V50 for a builder in a site, V62 for a worker in a depot; the target
  test is static study).
- **Letting a unit out.** A unit leaves a building by the ring search of `movement/placement.md` (limit 12, starting
  from the tile west of the building's top-left tile); if no tile is found, it **dies**. A trained unit that can't be
  placed isn't created at all.
- **The local player sees it.** Messages, completion sounds and the command-button refresh at the end of a
  construction are for the owner only if the owner is the local player. Every other effect happens for every player
  on every machine (static study). The command buttons are refreshed at every job end and every building kill,
  whoever the owner is (static study); the refresh only redraws the local player's panel, so nothing else changes.

### Costs in the data

- **Unit and building costs** are stored divided by 10: the real gold, lumber and oil costs are 10 × the stored
  values (game data).
- **Upgrade and spell research costs** are stored at full value (game data).

### Can the player afford it?

When a unit, building or building upgrade is started, these checks run in order, and the first one that fails stops
it with its message (static study; traced for the build command, V45):

1. **Food** (mobile units only): the owner's food supply must be **greater** than the food in use. In Enhanced mode
   the supply counts as at most **200**.
2. **Gold:** the owner's gold must be at least the gold cost.
3. **Lumber**, then 4. **Oil**, the same way.

Research and upgrades check gold, lumber and oil only. The messages are `ui/command-buttons.md`'s affordability
messages (entries 438 to 441).

### Food

- **Supply:** +4 for each completed farm (pig farm), +1 for each completed town hall, great hall, keep, stronghold,
  castle or fortress (static study).
- **In use:** the number of the owner's units that aren't buildings, minus its skeletons, daemons and critters. A unit
  stops counting when it is **killed**, at the start of its death animation, not when it leaves play; the training food
  test and the computer players' farm rule both see the lower count while it dies (traced: `xhuman2-10000`,
  `ai/worker-ai.md` vectors).
- Units still being trained **don't count**. So two buildings can each start a unit when only one food is spare, and
  the player ends up over the supply (static study; V12).

### Placing a building

**Carrying out a player's build command.** When a build command sent by a player (the click of
`ui/placing-buildings.md`) is carried out in the command turn, it is checked first, before any order is given:

1. the command must be valid, all on the command as received: the site's top-left tile is on the map (x and y below
   the map size); the builder belongs to the player who sent the command; the building type is from 0x3A to 0x68,
   has the building flag (PUD flag bit 5) and none of flag bits 19 (can attack), 21 (oil patch) and 22 (mine); and
   the builder's type is the one the **builder table** below names for that building type. If any of these fails,
   the command does nothing: no order, no message, nothing paid;
2. the full site check of `ui/placing-buildings.md` ("The site check") runs for the builder, its own tile not
   blocking, **except the unexplored test**, which is never made while commands are carried out;
3. the affordability check (traced: V45).

**Builder table** (unit types of the PUD specification's unit list; static study: it is fixed in the program, not
in the game data):

| Building types | Builder |
| --- | --- |
| 0x3A to 0x55, even (human buildings), and 0x67 (human wall) | Peasant (0x02) |
| 0x3B to 0x55, odd (orc buildings), and 0x68 (orc wall) | Peon (0x03) |
| 0x56 (human oil platform) | Human tanker (0x1A) |
| 0x57 (orc oil platform) | Orc tanker (0x1B) |
| 0x58 to 0x66 (keeps, strongholds, castles and fortresses, gold mine, oil patch, start locations, guard and cannon towers, circle of power, dark portal, runestone) | None: a build command for these always fails |

The command panel never sends a command that fails the table; it matters for a command naming the wrong builder or
a type with no builder, which an input script or a network peer can send (traced: V46, V47).

If step 2 or 3 fails, the builder gets **no order** and nothing is paid; the outcome's message is shown only if the
builder belongs to the local player (traced: V42; the unexplored test's absence: V43, whose barracks site is partly
unexplored and is built). Otherwise the build goes ahead as below; the payment and the check on arrival are
unchanged. A shore site with no footprint corner in the worker's region also gives no order and no payment
(`movement/pathfinding.md`, "Shore building"; traced: V48).

**The build walk.** A worker ordered to build goes to the site in order 28 (arrival radius 0), with no target unit.
Its goal is a tile of the planned footprint, worked out from the planned building's type and top-left tile and the
worker's current tile as if the building already stood there:

- a building placed on an oil patch (an oil platform): the top-left tile plus (1, 1);
- a shore building: the shore rule of `movement/pathfinding.md` ("Building goal rule", "Shore building"): the
  top-left tile if it is in the worker's region, else the first corner in it (bottom-right, top-right, bottom-left,
  top-left), else the top-left tile;
- any other building: the building goal rule of `movement/pathfinding.md` on the planned footprint.

The build command then pulls this goal back as any command does (`movement/pathfinding.md`, "Goal pulled back"): the
**stored goal** is the pulled-back tile, and the site itself is unchanged (traced: V39 to V41). The goal is worked
out when the order is given, from the worker's tile **at that moment**; while the worker walks, it stays as it was
(traced: V23). A computer player's worker given its build job as it comes out of a building (`economy/harvest.md`,
"Coming out", step 3) is still at the building's top-left tile then, before it is placed outside, so its goal is
worked out from that tile (traced: V35). It comes out aimed at this stored goal and faces it (`economy/harvest.md`,
"Coming out", step 6; static study). The walk runs instead of the order's logic, so the goal isn't worked out again
as the worker's own tile changes, even when the building goal rule would now pick another footprint tile.

**Arriving.** Order 28's logic runs only on arrival. On arrival at the stored goal it works the goal out **again**
from the worker's current tile (the same three cases as above), **not** pulled back, and applies the arrival test to
it (radius 0, or 1 for a land worker and a shore building, `movement/movement.md`, "The +1"). If the worker is there,
placement goes ahead (below). If not, the build is given again as a **command**, with the new goal pulled back; the
pending order takes over **in the same update** (still animation, wait 3), so the next step comes 3 cycles later.
This shows only when the pull-back moved the goal by two tiles or more (traced: V41). A build walk ends without
arriving in two ways, both leaving the worker idle: with **no route**, idle as an immediate order (`orders/orders.md`,
"No route"), and at the **blocked counter's limit**, idle as a pending order (`movement/movement.md`, "Blocked rules",
row C). The worker arrives when it is within the arrival radius of the goal tile; the placement check ignores the
worker's own tile (below), so that doesn't stop the building.

**Paying for a building.**

- **Computer players** (`movement/movement.md`, "Player kinds") pay when the AI decides to build, before the worker sets
  out. A build walk that ends with **no route** doesn't give the payment back; the job choice may give the same build
  again and the player pays again each time (traced: V37, V38). Only a failed site check on arrival (below) refunds
  it.
- **Every other player** (human, passive computer, neutral) pays on arrival. The affordability check runs then,
  after the site check ("Placement on arrival"), and a failure shows its message and makes the worker idle. In
  ordinary games only human players reach this, since passive and neutral players don't build.

**Placement on arrival**, in this order (static study, with the outcomes traced where named):

1. The site is checked for placement: the full site check of `ui/placing-buildings.md` ("The site check"), with
   its masks (`movement/occupancy.md`), shore and oil-patch tests and spacing rules, and, unlike the command turn,
   **with** the unexplored test for a builder of the local player (static study; no reachable case is known where
   this differs from the command turn's check, since the spacing doesn't change during a walk in the shipped data and
   the builder's own sight explores the site as it arrives). The worker's own tile doesn't
   block it. A failure shows a placement message. For a computer player the cost is given back, and the worker becomes
   idle: idle **takes over in the same update** (order 2, still animation, wait 3; `orders/orders.md`, "Same-update
   take-over"), not as an immediate order, and the worker's tile stays marked as occupied by it (traced: V33). For a
   player that pays on arrival this failure **wins over a lack of money**: when the site is blocked and the player
   can't afford the building either, the placement message shows, not the affordability message (traced: V61).
2. For a player that pays on arrival, the affordability check (above). A failure shows its message and ends it:
   the worker becomes idle the same way, taking over in the same update (order 2, still animation, wait 3; traced:
   V60), and nothing is paid.
3. The building is created under construction ("Construction"). If it can't be created because the unit table is
   full, the owner (if local) is shown the unit table's own "table full" message, not a placement message, a computer
   player's cost is given back, and the worker becomes idle the same way (static study, checked again in round 77 and
   by the B-040 audit; no trace reaches a full table). A player that pays on arrival has paid nothing at this point,
   so it gets nothing back.
4. On success the cost of a player that pays on arrival is taken, and the worker **goes inside** the building, in
   order 22, the same way a worker enters a mine (`economy/harvest.md`, step 2 of "Entering"). Its target is the new
   building and its target tile the building's top-left tile. As for any entering worker, it **forgets its attacker
   at once** and, for a human owner, reveals the entry sight (`vision/fog.md`, "Who reveals, and when", rule 3: sight
   3 at the building's top-left tile moved one step in its facing) (static study; the attacker isn't in the trace
   format). Going inside takes it out of every selection and group
   (`ui/selecting-and-commanding.md`, "Losing a unit from the selection"). Order 22's logic, which first runs 150
   cycles later, also forgets the unit's attacker and sets a 255-cycle wait (`orders/orders.md`); the worker stays
   inside until the building is finished, taken down or killed.

**Oil platform.** The oil patch under the new platform is marked used and killed. It leaves no ruin: at its own next
update its order becomes "remove" (order 0, animation 0, wait 1; its type and other fields kept), and its slot is
released 513 cycles after that, as for any removed unit (`core/corpses.md`). If the patch's slot comes after the
builder's, that update is in the same cycle, so the removal starts there; the patch's wait of 1 comes from that update,
and after it the wait keeps counting down (0 at the next cycle, then 255) (traced: V24). If the patch's slot comes
**before** the builder's, it has already been updated in that cycle: its record for that cycle shows the killed state,
**order 1, animation 1, wait 1**, and its removal starts in the next cycle, with the slot released 513 cycles after
that (traced: V49). A tanker given the harvest command on the new platform in that same cycle doesn't take it as its
target (`economy/harvest.md`, "Starting to harvest", step 3).

### Construction

A building placed during play starts **under construction**. (Buildings from the map start complete, at full hit
points.)

**At creation**, with M = the type's hit points (at least 1) and T = its build time (static study):

- **Starting hit points:** H₀ = min(floor(M / 10), 40).
- **Hit points still to add:** D = M − H₀.
- **Fast first hall (Enhanced multiplayer only):** if the owner has never had a completed town hall, keep or castle (of
  either race), a gold-storing building's T is lowered to the same race's farm build time, if that is shorter. In
  Enhanced mode T is at least 1. The owner's **ever had a hall** mark records this: it is set for every player at the
  start of a new game except in Enhanced multiplayer (`game/setup.md`), so this rule applies only there, until the
  player's first hall is completed. The mark is set whenever a completed hall is counted as the player's: at map
  load, at a hall's completion, and when a completed hall changes owner to the player (a rescued hall,
  `game/rescue.md`) (static study; V66). Whether T is fixed here or read again on each pass is an open question
  ("Fast first hall on each pass").
- The owner's "buildings under construction" count goes up by 1.
- The building is given idle (order 2, still animation) as an immediate order, and then its resting order 33 as a
  **pending** order, which takes over at the still animation's next step, 5 cycles later (wait 3) (static study;
  traced: V64). (A type with the tower flag would be given order 14, and the circle of power its own order 59, but
  neither can be placed in play: see the builder table.) The construction itself isn't an order: it is advanced by a
  separate pass.

**The construction pass** runs once per cycle in phase 4 of `core/timing-and-rng.md`, after the missiles and before
the timers pass. It visits units in slot order. For each building under construction (not dying or dead) (static
study):

1. A pass counter P (0 at creation) goes up by 1. Nothing else happens unless P is a multiple of 12. With the
   build-speed cheat on, P goes up by 12 instead and the multiple-of-12 test isn't made, so every pass is a step
   (or reaches the total, step 3), whatever P was when the cheat went on (static study; V65).
2. Let the total be 12 × (T + 1).
3. **If P has reached the total:**
   - if P equals the total exactly, the progress shows as full and nothing else happens;
   - if P is past it, the building is **completed** (below).
4. **Otherwise, a step:** the building gains hit points so that after k steps it has received floor(k × D / T) of
   them (the remainder carries over between steps). Then:
   - if its hit points have reached M, they are set to M, and P jumps to the total.
5. The shown progress is the larger of P / total and hit points / M, as a fraction of 255.
6. **Look:** below 64/255 the building shows the first construction-site frame (frame 0), and from 64/255 the second
   (frame 1). From 128/255 the building's own picture replaces the construction site; the frame stays 1. The shown
   progress (0 to 255) is kept in the building's mana byte at every construction step, and only then: between steps
   it doesn't change, though the pass counter does (traced: V63). On completion the frame and the mana byte go back
   to 0 (traced: V18).

An undamaged building reaches M at step T, so it completes on pass **12 × (T + 1)**, counting the creation cycle as
pass 1. A building that took damage during construction doesn't reach M. It shows full progress at step T + 1 and
completes 12 passes later, with its hit points still below M.

### Completion

- The building becomes completed: it plays its still animation, the under-construction count goes down by 1, and it
  now counts for food, bonuses, depots and requirements (see `economy/harvest.md` for the delivery bonuses). If its
  owner is the local player, the command buttons are refreshed (static study).
- Every unit inside it (the builder, or the tanker of an oil platform) gets the work-complete sound (`ui/sounds.md`;
  traced: V53) and goes through delivery (`economy/harvest.md`, "Delivering goods"): it gets any goods it carries
  delivered if the building is a depot for them; if the building stores gold or wood but doesn't accept the load (a
  peasant carrying gold that builds a lumber mill), the load is **lost** and the builder comes out empty-handed, as
  that table says; a building that stores neither leaves the load as it is (static study; V68; the same loss on
  entering is traced, `economy/harvest.md` V66). A builder of a town hall, keep or castle gets the **hall-sized
  exit mark**, carrying or not (`economy/harvest.md`, "Coming out", step 6). Being let out here neither uses nor
  clears the mark. Then:
  - a **tanker** (the builder of an oil platform) gets order 25 as an **immediate order** (`orders/orders.md`, "Pending
    orders and immediate orders"): order 25's animation (3, move) starts with a wait of 1, and the order's logic runs in
    the next cycle, which takes the tanker into the new platform (order 26) to collect oil (traced: V31, V32);
  - any other unit is let out ("Letting a unit out", aimed at the building's top-left tile), faces the building
    ("Facing a tile", `core/animation-scripts.md`, with B the building's top-left tile + (1, 1)) and becomes idle:
    order 2, still animation, wait 1, target cleared (an immediate order). The number of rings depends on the
    building's size. If no tile is found, it **dies**. From the tile west of the top-left tile the builder faces SE
    (3); other exit tiles give other facings by the same rule (traced: V25 to V30). A tanker set to order 25 doesn't
    turn.
- A completed **wall** piece (human or orc) is removed as a unit and becomes wall terrain (`movement/occupancy.md`,
  "Walls").

### Cancelling construction

The cancel-construction command (`ui/command-buttons.md`, "cancel construction") is carried out for the first unit of
the sender's selection if that unit is the sender's, isn't dead, is a building and isn't completed; otherwise it does
nothing (static study). The building is **taken down**:

1. **Refund** per resource: floor(stored cost × 30 / 4), which is 75 % of the real cost, rounded down (static study):

   | Mode | Refunded |
   | --- | --- |
   | Classic | Gold and lumber (not oil) |
   | Enhanced | Gold, lumber and oil |

2. Any builder inside is let out at once, in the command turn, by the same ring search as on completion (or dies if
   there is no room). It differs from completion in two ways, both because its target was cleared first ("Two ways
   a building goes"):
   - it **faces the building's top-left tile** ("Facing a tile", `core/animation-scripts.md`, with B the top-left
     tile itself, not top-left + (1, 1)). From the tile west of the top-left tile it faces E (2), where a completed
     building's builder faces SE (3);
   - it becomes idle as a **pending** order, not an immediate one: in the record of the cancel it is placed, with no
     target, still in order 22 with wait 1; at its next update idle takes over (order 2, still animation, wait 3).
   (Traced: V54, V55.)
3. The building is killed ("A building killed"); no unit inside has it as its target any more, so nobody else dies.
   The site's refund and its kill show in the same record as the builder's exit (traced: V56).

A cancel-construction command naming a completed building does nothing (traced: V56), whether it is resting or
working: the two cancel commands are separate, and each acts only in its own state, so cancel construction never
ends a job and cancel job never takes down a site (static study; V69). The command panel always sends the one that
fits; only an input script or a network peer can send the other. The same take-down, with no
refund in Enhanced mode, is how an exhausted mine or platform collapses (`economy/harvest.md`, "Mine and platform
amounts"; static study).

### A building killed

When a building is killed (by damage, a spell, a demolition, or any other kill of `core/unit-slots.md`):

- if it is working, its job is **cancelled at the kill**, as in the "Cancelled" column of "Job end": the cost is
  refunded in full (for research and upgrades, on the same condition), and the job's progress counter and the mana
  byte go to 0 in the same cycle (traced for training: V36; the other job kinds follow the same rule, static study);
- if it is under construction, the owner's under-construction count goes down by 1 (static study);
- every unit inside it whose target is the building **is killed in the same cycle**, where it stands (the building's
  top-left tile): a builder inside a site that is destroyed doesn't come out (traced: V50), and neither does a worker
  delivering goods inside a depot (traced: V62) or, by the same rule, a worker inside a mine or a tanker inside a
  platform or depot (static study). It dies like any killed unit, leaving a dead body (`core/corpses.md`). (The
  collapse of an exhausted mine or platform isn't a kill: it is a take-down, and the units inside come out,
  `economy/harvest.md`, "Mine and platform amounts".)
- the command buttons are refreshed, whoever the owner is ("The local player sees it"; static study).

The building then dies as any unit does (`core/unit-slots.md`).

### Starting a job

A completed building that isn't already working can start **one** job at a time. There is no queue. The job command
names a building, a job kind (train, research a spell, upgrade, or upgrade the building) and a unit type or upgrade
number. Computer players start their jobs through the same procedure. The checks run in this order; any failure ends
it **silently** unless a message is named (static study):

1. The building isn't already working, and it is completed.
2. The job kind is one of the four.
3. **The kind's own check** and, for the upgrade kind, the **level check** (`economy/tech-tree.md`, "Job checks for
   each kind"): the producer, the requirements while they are enforced, and the upgrade level.
4. **Affordability** ("Can the player afford it?"). A failure shows its message, to the owner only if the owner is the
   local player.
5. The cost is paid, and the job starts:
   - **train:** the owner's "units in training" count goes up by 1;
   - **upgrade:** the upgrade data's "affects" bits are set in the owner's **upgrades being researched** word;
   - **research a spell:** the "affects" bits are set in the owner's **spells being researched** word;
   - **upgrade the building:** the frame is set to 1 and the new type's picture is shown, while the type stays the
     old one until the upgrade ends (traced: V19);
   - every kind: the building becomes working (order 37, which takes over as a pending order), its progress counter
     is set to 0 and its mana byte to 1 (traced: V17, V52).

| Job | Time (progress steps) | Paid |
| --- | --- | --- |
| Train a unit | 2 × the unit's build time | unit costs × 10 |
| Research a spell | 2 × the upgrade time | upgrade costs |
| Weapon, armour or similar upgrade | 2 × the upgrade time | upgrade costs |
| Upgrade the building (town hall to keep, keep to castle, scout tower to guard or cannon tower, and the orc versions) | 2 × the new type's build time | new type's costs × 10 |

### Job progress

The working order's logic runs once per loop of the building's work animation, and each run adds 1 to the counter
(with the build-speed cheat, up to 10 at a time). Building animation timing (game data, from the scripts):

- Work animation: a 10-cycle wait, then a loop with one "step done" every 5 cycles.
- Still animation: one "step done" every 5 cycles.

So a job of N steps runs N + 1 times: the first run is 13 cycles after the job takes over (the 3-cycle first wait,
then the 10-cycle wait), then every 5 cycles. The job ends on the run after the counter reaches N, which is
**13 + 5 × N cycles** after it took over. The job takes over at the still animation's next "step done", up to 5
cycles after the command (traced: V17).

After each run that adds to the counter, the building's mana byte holds floor(counter × 255 / total), its shown
progress (for a total above 255 that can be 0 again after the start's 1). Until the first run it stays 1 (traced:
V17, V52).

### Cancelling a job

The cancel-job command (`ui/command-buttons.md`, "cancel job") is carried out for the first unit of the sender's
selection if that unit is the sender's building; it only marks the job as **to be cancelled**, and does nothing if the
building isn't working. The job ends, cancelled, at the building's **next work run** ("Job progress"), not in the
command turn: within 5 cycles once the runs have begun; a job marked before its first run ends at that first run, 13
cycles after order 37 takes over. The end is the ordinary one ("Job end"): order 33 in the same update (wait 3), the
mana byte to 0 and the full refund, all in the record of that run (traced: V57 to V59). A job marked this way ends
cancelled even if its counter has reached the total (static study). A cancel-job command for a building that isn't
working does nothing (traced: V59).

A job is also cancelled when the building is killed ("A building killed") and when the building changes owner
(`game/rescue.md`; the refund goes to the old owner). Neither waits for the next work run. A killed building goes
straight to its death (order 1 in the same cycle; traced: V36). On an owner change the job ends at once, and order 33
is given as a **pending** order, which takes over at the building's next step (static study; untraced, since in
ordinary games only buildings of players that never start jobs change owner).

### Job end

When the job ends, finished or cancelled: the order returns to 33, taking over in the same update (still animation,
wait 3; a guard or cannon tower made by a building upgrade takes order 14 instead, below; traced: every job end in
the Enhanced oracle traces that isn't a kill, 2593 of them), the progress counter and the mana byte go to 0, the cost
is looked up again for the job's item (so a refund is the current cost, "Paying"), the job's own ending below runs,
and the command buttons are refreshed, whoever the owner is ("The local player sees it") (static study; traced: V17
for the mana byte). A building upgrade, finished or cancelled, sets the frame back to 0 (cancelled: the
old picture returns) (traced: V19).

| Job | Finished | Cancelled |
| --- | --- | --- |
| Train | The unit is created next to the building, on the first tile found by the ring search of `movement/placement.md` (limit 12, starting from the tile west of the building's top-left tile), and the ready sound plays (`ui/sounds.md`). If it can't be placed (no room, or the unit table is full), the player gets a message and a **full refund** | Full refund |
| Research a spell | The spell becomes known, if its "being researched" bits are still set (`economy/tech-tree.md`) | Full refund, on the same condition |
| Upgrade | The level in its line goes up by 1, on the same condition | Full refund, on the same condition |
| Building upgrade | The type changes, and the hit points go up by (new type's M − old type's M), each maximum counted as at least 1, with no clamp on the result (static study; it differs from a clamped reading only for custom unit data). Towers get order 14, which takes over in the same update (animation 4, wait 3) (traced: V34) | Full refund |

In every case the "units in training" count (for training) goes back down. The completion sounds of finished
research, upgrades and building upgrades are in `ui/sounds.md` ("Work complete"; traced: V51).

### What the player sees

These are stated in other specs; the list says where each step of a building's life shows.

| Step | Selection and panel | Where |
| --- | --- | --- |
| Worker goes inside a new site | it leaves every selection and group | `ui/selecting-and-commanding.md`, "Losing a unit from the selection" |
| Under construction or working, own building | busy button set (one Cancel, strings 408 to 410); info panel frame 3 with the progress bar | `ui/command-buttons.md`, rule 6; `ui/info-panel.md`, "Progress bar" |
| Progress bar | the building's mana byte above: construction progress, or the job's (1 at the start) | `ui/info-panel.md`, "Progress bar" |
| Completion, job end, kill | command buttons refreshed (at completion for the local owner only); work-complete or ready sound | this spec; `ui/sounds.md` |
| Builder let out, or unit trained | not selected: nothing adds it to a selection (the builder left every selection when it went inside) | `ui/selecting-and-commanding.md`, "Losing a unit from the selection" |

## Data

| Constant | Value |
| --- | --- |
| Cost scale (units and buildings) | stored value × 10 |
| Starting hit points under construction | min(floor(M / 10), 40) |
| Construction pass step | every 12 passes |
| Construction length | 12 × (T + 1) passes when undamaged |
| Cancel refund | floor(stored cost × 30 / 4) = 75 % rounded down |
| Training / research steps | 2 × time |
| Building work loop | first run 13 cycles after take-over, then every 5 cycles |
| Food per farm / per hall-type building | 4 / 1 |
| Enhanced food cap | 200 |

Units not counted as using food: skeleton, daemon, critter.

## Test vectors

Vector IDs are stable: V01 to V49 number the vectors in the order the spec held them before the consolidation, so
the review file can map each one; V50 to V53 are new in round 76, V54 to V63 in round 77 and V64 to V69 in the B-040
audit (`ERRATA-audit-buildtrain.md` numbered its draft vectors V54 to V63 too; its review renumbered them, and the
ones that repeat a round-77 vector were merged into it). "Source" says whether a vector is from an oracle trace
(named in the row), from static study, or a worked example of the rules. The values of M and T in the worked examples are
examples, not quoted from the game data. Rows that begin "Same" follow the row they continue.

### Costs, affordability and food

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V10 | Player with 2 farms and a town hall (supply 9) and 8 non-building units | can start a unit (9 > 8) | Worked example |
| V11 | Same player with 9 units, 2 of them skeletons | can start a unit (9 > 7) | Worked example |
| V12 | Same player with 8 units, two barracks each start a footman | both start; after training, 10 units on a supply of 9 | Worked example |
| V13 | Enhanced, 60 farms and a hall (supply 241) | counts as 200 | Worked example |

### Carrying out a build command

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V42 | `orc9-orc09-shore-build-3000` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-shore-build.txt`): player 5's peon 1482 (type 0x03) at (34, 89) is sent to build a shipyard (type 0x49) at (52, 81), too close to oil; the command is carried out at n = 26 | No order: 1482 stays in order 2 (wait 1 at n = 26, 27) and never takes order 28; nothing is paid | Trace |
| V43 | `human01-basics-9500` (oracle trace): the barracks at (5, 8), partly unexplored when the build command is carried out | Built: no unexplored test in the command turn | Trace |
| V44 | A player's build command it can't afford when carried out (traced: round-61 vectors, n = 10) | No order, nothing paid | Trace (V45) |
| V45 | `orc9-orc09-smart-move-2000` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-smart-move.txt`), n = 10: player 5's peon 1482 (type 0x03) given a great hall (type 0x4B, 1200 gold) on a clear site at (45, 84), with 1000 gold | Nothing: 1482 stays in order 2, nothing paid | Trace |
| V46 | Same trace, n = 14: peon 1482 given a human farm (type 0x3A) on a clear site | Nothing (the farm's builder is the peasant type): no order 28 at n = 17 | Trace |
| V47 | Same trace, n = 50: peon 1482 (in order 23) given a keep (type 0x58) on a clear site, with 10600 gold | Nothing (no builder): 1482 keeps its harvest order | Trace |
| V48 | `orc9-orc09-shore-no-corner-600` (oracle trace, Enhanced; scenario `oracle/scenarios/orc09-shore-no-corner.txt`), n = 20: player 5's peon 1478 (type 0x03) at (38, 92) sent to build a shipyard (type 0x49) at (22, 65), on the coast of another island: no footprint corner in its region | 1478 stays in order 2; gold and lumber stay 1000 | Trace |

### The build walk and arriving

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V20 | `human01-basics` (oracle trace): peasant 1596 on (14, 9) is told to build a barracks (type 60, 3 × 3) at top-left (5, 8) | Goal (7, 9): x = 7 because 14 ≥ 7; y = 8 + 1 because 9 lies inside 8 to 10. Order 28 takes over at n = 19 (wait 3); the peasant walks west along row 9, one tile every 16 cycles: (13, 9) at n = 22, (12, 9) at 38, (10, 9) at 70, (7, 9) at 118. At n = 134 the barracks is placed at (5, 8) and the peasant is inside it (order 22, wait 150) | Trace |
| V21 | A worker at (2, 2) told to build a 2 × 2 farm at (5, 5) | Goal (5, 5) | Worked example |
| V22 | A worker at (6, 9) told to build a 3 × 3 building at (5, 5) | Goal (6, 7): x = 5 + 1 (6 lies between 5 and 7), y = 7 (9 ≥ 7) | Worked example |
| V23 | `xorc7-7000` (oracle trace), n = 2028: peasant 1421 (type 0x02, owner 1) in order 28, at (95, 118), building a farm with top-left (96, 118); goal (96, 119), worked out when the order was given | It steps south-east to (96, 119), facing 3, toward the stored goal (working the goal out again from (95, 118) would give another tile) | Trace |
| V35 | `xorc4-10000` (oracle trace): computer peasant 1580 (type 0x02, owner 2) comes out of 1565 (top-left (26, 11)) at n = 7963, at (25, 12), with a build job for a 3 × 3 site at (14, 11) | Its walk goal, worked out from (26, 11), is (16, 11), not (16, 12): it walks row 12 to (20, 12), steps north-west to (19, 11) at n = 8071, reaches (16, 11) at n = 8119 and is in order 22 at n = 8135 | Trace |
| V39 | `mp-frosty-fjords-bne-30000` (oracle trace, Enhanced): computer peasant 1548 (type 0x02, owner 2) builds a shipyard with top-left (120, 52), which is in its region | Goal (120, 52) (shore rule, step 1); it walks to (119, 51) and the shipyard 1518 is placed at n = 16576 | Trace |
| V40 | Same trace: peasant 1560 (type 0x02, owner 0) at (52, 96), given a shipyard with top-left (67, 92) at n = 16187 | Stored goal (66, 92), the raw goal (67, 92) pulled back | Trace |
| V41 | Same: 1560 reaches (65, 93) at n = 16718, within 1 of (66, 92); its logic runs at n = 16734 | Worked out again from (65, 93): (67, 92), 2 tiles away, so the build is given again: order 28, still animation, wait 3 at n = 16734; (66, 93) at n = 16737; shipyard 1516 placed at (67, 92) at n = 16753 | Trace |

### Paying and placement on arrival

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V33 | `xhuman2` (oracle trace), n = 85: computer peon 1562 (type 0x03, owner 5) at (65, 59) in order 28 arrives and the site check fails | Order 2, animation 2, wait 3; its tile stays marked (ogre 1549 is blocked by it at n = 113). n = 88: order 28 again (the computer gives the job again) | Trace |
| V37 | `xhuman12-10000` (oracle trace), n = 116: a computer player 6 build walk finds no route | Player 6 gold 3700 and lumber 3650: the payment made that cycle stays spent | Trace |
| V38 | `human14-10000` (oracle trace), n = 1040: the same for computer player 5 | Player 5 gold 33556 and lumber 35906 (no refund) | Trace |
| V60 | `human06-arrive-broke-700` (oracle trace, Enhanced; scenario `oracle/scenarios/human06-arrive-broke.txt`; no cheats): peasant 1587 (owner 1, a human player) given a farm at the clear site (84, 2) with 1000 gold (turn record n = 254); a footman's training leaves 400 gold (n = 258); 1587 arrives at (84, 3) | n = 339: still order 28, wait 1 (the record is there; 1587 is unchanged from n = 338). n = 340: order 2, animation 2, wait 3, target none (idle in the same update); gold stays 400; no site placed | Trace |
| V61 | Same trace, captured with a break on the arrival's site-failure message (`oracle/README.md`, "Round 77 traces"): peasant 1586 given a farm at (76, 5) (turn record n = 252); footman 1584 then stands on (76, 5); 1586 arrives at (77, 6) with 400 gold | n = 513: still order 28 (wait 1). n = 514: order 2, animation 2, wait 3; gold stays 400; the site-failure message branch runs for 1586 at n = 514 and never for 1587 (so the placement message shows, not the affordability one) | Trace (break log) |
| V67 | A peasant hit by an enemy archer while walking to its farm site enters the site | Its attacker is clear in the entry cycle (not 150 cycles later, when order 22's logic first runs); for a human owner, the sight-3 area around the site's top-left tile moved one step in its facing is revealed that cycle | Static study |

### Oil platform

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V24 | `orc09-naval-1500` (oracle trace), n = 268: a platform (slot 1468) placed on the oil patch in slot 1502 at (57, 75) | Same cycle: patch order 0, animation 0, wait 1, type 0x5D; its slot is released at n = 781 (268 + 513) | Trace |
| V49 | `xhuman5-30000` (oracle trace, Enhanced, no input), n = 27632: computer tanker 1542 (type 0x1B, owner 7) starts orc oil well 1594 (type 0x57) on oil patch 1530 (type 0x5D, owner 15) at (75, 107); the patch's slot is lower than the builder's | 1542: order 22, wait 150, target 1594. Patch 1530: order 1, animation 1, wait 1 at n = 27632; order 0, animation 0, wait 1 at n = 27633; wait 0 at n = 27634; its slot is released at n = 28146 (27633 + 513) | Trace |

### Construction

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V01 | Building with M = 400, T = 100 placed | starts at 40 HP; D = 360 | Worked example |
| V02 | Same, after 1, 50 and 100 steps | 43, 220, 400 HP | Worked example |
| V03 | Same, undamaged | completes on pass 1212 (12 × 101) | Worked example |
| V04 | Same, takes 10 damage at step 30 | at step 100 it has 390 HP; full progress at step 101 (pass 1212); completes on pass 1224 with 390 HP | Worked example |
| V05 | Building with M = 1200, T = 255 | starts at 40 HP (120 capped to 40); D = 1160; completes on pass 3072 | Worked example |
| V06 | Building with M = 60 | starts at 6 HP | Worked example |
| V16 | Enhanced multiplayer, first town hall of a player who never had one | built in the farm's build time if shorter | Worked example |
| V64 | `human06-cancel-site-600` (oracle trace, Enhanced): farm site 1521 placed at n = 78 | n = 78: order 2, animation 2, wait 5; the wait counts down to 1 at n = 82; n = 83: order 33, wait 3 (the pending order takes over). The same in `human06-site-kill-800`, n = 78 to 83, and `human01-basics-9500`, n = 134 to 139 | Trace |
| V65 | A farm site at P = 5, built without cheats; the build-speed cheat goes on | P = 17, 29, 41, …: each pass is a step and the farm completes (a reading that still needs P to be a multiple of 12 would never step again) | Static study |
| V66 | A player that has never had a hall rescues a completed town hall, then builds a town hall (Enhanced multiplayer) | The full build time, not the fast first hall time | Static study |
| V63 | `human06-cancel-site-600` (oracle trace): farm site 1521 placed at n = 78 (40 hit points, mana byte 0) | Mana byte 27 (43 hit points) at n = 89, 29 at n = 101, 31 at n = 113: it changes only at the steps, 12 passes apart | Trace |
| V18 | orc4 (oracle trace), farm slot 1534 under construction | Mana 27 at n = 10714, rising with hit points (41 at 10786); 64 and frame 1 at n = 10906; frame 1 to completion; frame 0 and mana 0 at n = 11914 (hit points 400) | Trace |

### Completion

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V25 | `human06-tech` (oracle trace), n = 178: farm 1521 (type 0x3A, top-left (76, 5)) completes; builder 1586 | 1586 at (75, 5), facing 3 (SE, toward (77, 6)), order 2, wait 1, target null | Trace |
| V26 | `human7` (oracle trace), n = 1251: farm 1465 (type 0x3B, top-left (63, 2)) completes; builder 1582 | 1582 at (62, 2), facing 3, order 2, wait 1, target null | Trace |
| V27 | `xorc3` (oracle trace), n = 1295: farm 1475 (type 0x3B, top-left (27, 113)) completes; builder 1482 | 1482 at (26, 113), facing 3, order 2, wait 1, target null | Trace |
| V28 | `human06-tech` (oracle trace), n = 202: builder 1528 let out at (16, 82) from a farm with top-left (17, 80) | dx 2, dy −1 to (18, 81): facing 1 (NE) | Trace |
| V29 | `orc09-naval` (oracle trace), n = 249: builder 1587 let out at (80, 7) from a farm with top-left (81, 6) | dx 2, dy 0 to (82, 7): facing 2 (E) | Trace |
| V30 | `xhuman2` (oracle trace), n = 3438: builder 1588 let out at (21, 4) from a tower (type 0x41) with top-left (20, 2) | dx 0, dy −1 to (21, 3): facing 0 (N) | Trace |
| V31 | `orc09-naval` (oracle trace), n = 468: oil platform 1468 completed with tanker 1473 inside | Tanker: order 25, animation 3, wait 1, target 1468, facing kept (2) | Trace |
| V32 | Same, n = 469 | Tanker: order 26, still animation, wait 150 | Trace |
| V68 | A peasant carrying gold (from a mine, before delivering) builds a lumber mill | At completion it comes out carrying nothing, and no gold is added. Trace request AB2 (`ERRATA-audit-buildtrain.md`) | Static study |
| V53 | `orc09-naval-1500` (oracle trace), n = 468, re-run with a break on the work-complete alert (`oracle/README.md`, "Round 76 traces"): oil platform 1468 completes with tanker 1473 (type 0x1B, owner 5, the local player) inside | The alert runs for tanker 1473 at n = 468, the cycle it takes order 25 (V31) | Trace (break log) |

### Cancelling construction

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V07 | Cancel a building whose stored costs are gold 50, lumber 25, oil 10 (real 500 / 250 / 100), Classic | +375 gold, +187 lumber, +0 oil | Worked example |
| V08 | Same, Enhanced | +375 gold, +187 lumber, +75 oil | Worked example |
| V54 | `human06-cancel-site-600` (oracle trace, Enhanced; scenario `oracle/scenarios/human06-cancel-site.txt`; glittering prizes): peasant 1586 (type 0x02, owner 1) inside farm site 1521 (type 0x3A, top-left (76, 5)); the cancel is carried out in the turn after n = 121 | Turn record n = 122: 1521 order 1, animation 1; 1586 at (75, 5), facing 2 (E, toward (76, 5)), order 22, wait 1, target none. n = 123: 1586 order 2, animation 2, wait 3 | Trace |
| V55 | Same trace: peasant 1587 inside church site 1519 (type 0x3E, 3 × 3, top-left (85, 1)); cancel in the turn record n = 202 | 1587 at (84, 1), facing 2 (E, toward (85, 1); a completed church's builder would face toward (86, 2)), order 22, wait 1, target none; n = 203: order 2, animation 2, wait 3 | Trace |
| V56 | Same trace: the refunds, and a cancel naming the completed barracks 1588 (turn record n = 262) | Player 1's gold and lumber 10200 / 5450 to 10575 / 5637 at n = 122 (farm: +375, +187); 9675 / 5137 to 10350 / 5512 at n = 202 (church: +675, +375); oil unchanged (no oil cost). 1588 unchanged | Trace |
| V69 | A cancel-construction command naming a working barracks; a cancel-job command naming a construction site | Nothing happens in either case: the job runs on, the site goes on being built | Static study |

### A building killed

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V50 | `human06-site-kill-800` (oracle trace, Enhanced; scenario `oracle/scenarios/human06-site-kill.txt`): peasant 1586 (type 0x02, owner 1) is inside farm site 1521 (type 0x3A, top-left (76, 5)) from n = 78 (order 22, target 1521); player 1's ballista, footmen and knight attack the site from n = 150 | n = 416: site 1521 is killed (order 1; mana byte 70, frame 1 in that record); in the same record peasant 1586 is in order 1, animation 1, wait 1, at (76, 5), hit points 30 (unchanged), target 1521; it doesn't come out. n = 523: 1586 is a dead body (type 0x69) | Trace |
| V62 | `human06-depot-kill-2400` (oracle trace, Enhanced; scenario `oracle/scenarios/human06-depot-kill.txt`; no cheats): peasant 1586 (type 0x02, owner 1) delivering gold goes inside town hall 1591 (type 0x4A, top-left (82, 4)) at n = 2166 (order 26, target 1591); player 1's units attack the hall from n = 240 | n = 2290: 1591 order 1; in the same record 1586 is in order 1, animation 1, wait 1, at (82, 4), hit points 30 (unchanged), target 1591; it doesn't come out. n = 2397: 1586 is a dead body (type 0x69) | Trace |
| V36 | `human5-10000` (oracle trace), n = 8086: town hall 1525 (type 0x4A, owner 0) is killed while training (order 37, mana byte 70) | Same cycle: order 1, player 0's gold 300 to 700 (the 400 paid for the unit refunded), mana byte 0 | Trace |

### Jobs

| ID | Case | Expected | Source |
| --- | --- | --- | --- |
| V09 | Unit with build time 60 | 120 steps; done 13 + 600 = 613 cycles after the job takes over | Worked example |
| V17 | Human04 (oracle trace), great hall slot 1550 trains a peon (total 90) | Mana 1 from n = 21 (the payment). After the runs at n = 39, 44, 49, …: 2, 5, 8, 11, 14, 17, 19, … 252, 255 (n = 484); 0 at n = 489, when the peon appears. Frame 0 throughout | Trace |
| V52 | `human06-tech-650` (oracle trace; build-speed cheat on): scout tower 1516 (type 0x40) is given its upgrade to a guard tower in the command turn after n = 420 (total 2 × 140 = 280 steps, 10 per run, so 28 runs) | n = 422: mana byte 1, frame 1, still order 33. n = 425: order 37 takes over (wait 3). Mana byte 1 through n = 437; 9 at n = 438 (the first run, 13 cycles after the take-over: floor(10 × 255 / 280)); type 0x60 at n = 578 (425 + 13 + 5 × 28) | Trace |
| V19 | xhuman4 (oracle trace), tower slot 1423 (type 0x41) | Frame 0 at completion (n = 821), frame 1 from the building-upgrade start (n = 832) through order 37, frame 0 when it becomes type 0x61 (n = 2250) | Trace |
| V34 | `human06-tech` (oracle trace), n = 578: building 1516 at (72, 4) finishes its upgrade from type 0x40 to 0x60 | The same record: type 0x60, order 14, animation 4, wait 3 | Trace |
| V51 | `human06-alerts-1400` (oracle trace, Enhanced; scenario `oracle/scenarios/human06-alerts.txt`, captured with a break on the work-complete alert): town hall 1591 (type 0x4A, owner 1) becomes a keep (job from n = 35); scout tower 1516 (type 0x40) becomes a guard tower (job from n = 425); lumber mill 1583 finishes the first arrow upgrade; church 1519 finishes healing | The alert runs for 1591 at n = 248 (type 0x58, order 33, wait 3 in that record), for 1583 at n = 241 and for 1519 at n = 530. It doesn't run when 1516 becomes type 0x60 at n = 578 (order 14, wait 3) | Trace (break log) |
| V15 | Keep (old M) upgraded to castle (new M), keep damaged to h HP | castle with h + (new M − old M) HP | Worked example |
| V14 | A trained unit can't be placed | message; costs fully refunded | Static study |
| V57 | `human06-cancel-job-500` (oracle trace, Enhanced; scenario `oracle/scenarios/human06-cancel-job.txt`; glittering prizes): barracks 1588 (type 0x3C, owner 1) trains a footman (paid in the turn record n = 22; order 37 takes over at n = 25, wait 3); cancel job carried out in the turn record n = 32, before the first run | Order 37 through n = 37; n = 38 (25 + 13, the first run): order 33, animation 2, wait 3, mana byte 0, gold 10100 to 10700 | Trace |
| V58 | Same trace: 1588 trains again (order 37 from n = 106, runs at n = 119, 124, …); cancel job in the turn record n = 302 | Order 37, mana byte 78, through n = 303; n = 304 (the next run): order 33, wait 3, mana byte 0, gold 10400 to 11000 | Trace |
| V59 | Same trace: lumber mill 1583 (type 0x4C) on its first arrow upgrade (upgrade 4) since n = 24; cancel job in the turn record n = 202 (runs at n = 201, 206); then, at n = 402, a cancel job on the resting barracks 1588 | n = 206: order 33, wait 3, mana byte 0, gold and lumber 10100 / 5700 to 10400 / 6000. 1588 unchanged by its cancel | Trace |

## Open questions

The requirement rules, the setting that turns them off and the research jobs' effects are in `economy/tech-tree.md`.
Trace requests BT1 to BT5 are listed in `specs/ERRATA-buildtrain-consolidation.md`; BT1, BT2 and BT5 were run in
round 77 (`specs/ERRATA-round77.md`). Trace requests AB1 to AB5 are listed in `specs/ERRATA-audit-buildtrain.md`.

- **Food of summoned units.** Whether eyes of Kilrogg and other summoned units count as food in use. Default (static
  study, round 77): the food-exempt types are exactly the skeleton, the daemon and the critter, so an eye of Kilrogg
  counts as food in use like any other non-building unit while it exists, and a summoned skeleton doesn't. Not
  traced.
- **Upgrade picture.** Which frame of the new type's graphics a building upgrade shows with frame 1 (a construction
  stage of the new type, or something else) wasn't checked. Default: frame 1 of the new type's picture.
- **No room for a let-out builder.** Whether a builder that finds no tile and dies still gets the work-complete sound
  at completion. Default (static study): it does, since the sound is asked for before the unit is placed. Trace
  request BT3 (low value; not run).
- **Fast first hall on each pass (Enhanced multiplayer only).** "At creation" fixes T when the site is created. The
  B-040 audit's static study reads T again from the type's build time on every construction pass, with the fast
  first hall rule applied at that moment. The two differ only in Enhanced multiplayer, for a player with two first
  halls under way at once: when one completes, the other's T would go back to the full build time on its next pass,
  so its hit points stop rising until floor(k × D / T) passes the value already given, and its shown progress drops.
  Default: as stated (T fixed at creation). Trace request AB3 (multiplayer launch, Enhanced).
- **Unit-ready event position.** "Job end" and `ui/sounds.md` put the latest event location at the new unit's tile.
  The audit's static study puts it at the building's top-left tile (the minimap ping and the "go to last event" key
  would point there). UI only. Default: as stated (the new unit's tile). Observation request AB5.
- **Classic-mode builder's delivery.** Whether a builder in gold or lumber mode with empty hands gets a load delivered
  when its building is completed in Classic mode (the static reading says yes, as for a Classic delivery on entering)
  isn't separated by any trace. Default (Classic mode, static study): it does.
- **Classic mode: refund on collapse.** In Classic mode the static reading gives the 75 % gold and lumber refund
  whenever a building is taken down, so an exhausted oil platform's collapse would refund 75 % of the platform's
  gold and lumber. Enhanced mode refunds only on the cancel command. Default for Classic mode: the static reading.
  Trace request BT4 (Classic, research use only). This never changes the Enhanced rule.

## History

Erratum rounds that shaped this spec and the sections where their content now lives. The erratum notes for each
round are kept with the research-side specs.

| Round | Content | Now in |
| --- | --- | --- |
| 3 | Trained units and builders coming out use the ring search (limit 12, west of the top-left tile); repair moved to `economy/repair.md` | "Letting a unit out"; "Completion"; "Job end" |
| 4 | Computer players pay when the AI decides; every other player pays on arrival | "Placing a building", "Paying for a building" |
| 6 | Research isn't checked against "known" or "being researched" at the command; finishing or cancelling research depends on the "being researched" bits; training requirements only while enforced | "Starting a job", step 3 (`economy/tech-tree.md`); "Job end" |
| 33 | Shown progress of construction and jobs in the mana byte; construction look and building upgrade are frames 0 and 1 | "Construction", step 6; "Starting a job"; "Job progress"; vectors V17 to V19 |
| 35 | Build walk goal on the planned footprint (oil platform, shore rule) | "The build walk" |
| 39 | Builder let out of a finished building faces it; exit-facing open question (closed in round 77) | "Completion"; vectors V25 to V30; "Cancelling construction" |
| 40 | A failed placement on arrival: idle takes over in the same update, tile stays marked; a tower's order 14 takes over in the same update | "Placement on arrival", step 1; "Job end"; vectors V33, V34 |
| 41 | The build walk's goal stays fixed while walking; the oil patch is marked used and killed, no ruin, slot released 513 cycles later | "The build walk"; "Oil platform"; vectors V23, V24 |
| 42 | Order 28's logic runs only on arrival; the two ways a build walk ends without arriving | "Arriving" |
| 43 | A finished platform gives its tanker order 25 as an immediate order | "Completion"; vectors V31, V32 |
| 47 | Food in use drops at the kill; the "ever had a hall" mark; goal from the worker's tile when the command is given; hall-sized exit mark for a hall's builder | "Food"; "Construction", "At creation"; "The build walk"; "Completion"; vector V35 |
| 48 | A building killed during a job cancels it at the kill | "A building killed"; vector V36 |
| 49 | A computer player's build walk with no route keeps its payment; a tanker can't take a platform placed in the same cycle | "Paying for a building"; "Oil platform"; vectors V37, V38 |
| 59 | Shore building goal; the build goal is pulled back; order 28 works the goal out again on arrival | "The build walk"; "Arriving"; vectors V39 to V41 |
| 60 | The build command's checks in the command turn (no unexplored test); come-out with a build job aims at and faces the stored goal; shore site with no corner | "Carrying out a player's build command"; "The build walk"; vectors V42 to V44 |
| 61 | The builder table; the affordability check in the command turn | "Carrying out a player's build command"; vectors V45 to V47 |
| 62 | Shore site with no corner traced | vector V48 |
| 65 | An oil patch in a lower slot than its builder shows order 1 for that cycle | "Oil platform"; vector V49 |
| 76 | Consolidation (B-038): one model; job start moved here from `economy/tech-tree.md`; a killed site kills its builder; the job's mana byte is 1 from the start; completion sounds; static statements on cancel commands, a full unit table, job end and refunds | "The model"; "A building killed"; "Starting a job"; "Cancelling a job"; "Job end"; vectors V50 to V53 |
| 77 | Traced the cancel commands and the broke arrival: a cancelled site's builder faces the top-left tile with a pending idle; a cancelled job ends at the next work run; the site failure wins on arrival; a worker in a killed depot dies; reviewed | "Placement on arrival"; "Construction"; "Cancelling construction"; "A building killed"; "Cancelling a job"; "Open questions"; vectors V54 to V63 |
| 77 (audit) | B-040 build and train audit (`ERRATA-audit-buildtrain.md`), static study, reviewed: order at creation; the cheat pass is always a step; a rescued hall sets "ever had a hall"; entering a site forgets the attacker and gives the entry sight; the full site check on arrival; "table full" message; a load the new store doesn't accept is lost; two cancel commands; no clamp on an upgrade's hit-point gain; command-button refresh. The fast first hall per pass and the unit-ready position stay open | "The local player sees it"; "Paying for a building"; "Placement on arrival"; "Construction"; "Completion"; "Cancelling construction"; "A building killed"; "Job end"; "Open questions"; vectors V64 to V69 |
