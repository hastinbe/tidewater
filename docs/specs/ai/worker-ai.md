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

## Behaviour

A computer-controlled player's workers and tankers choose their own jobs. Each time a job is chosen, the unit
returns goods, builds, repairs or harvests, or else waits a short random time and looks again. This spec covers that
choice. The script slots it reads are in `ai/ai-scripts.md`. Training, research and attacks are not covered.

"Worker" here means a unit whose type has the peon flag (PUD flag bit 8) and moves on land. A unit with the peon flag
that moves on water is handled as a tanker (below).

### When a job is chosen

The job choice runs for a unit of a **computer-controlled** player:

- when its idle order (order 2) logic runs (`orders/orders.md`); and
- when it comes out of a depot after delivering goods (`economy/harvest.md`, "Coming out", step 3). If a job is given
  there, the usual return to the old source is skipped.

### Worker: the steps, in order

1. **Release the old job.** The bookkeeping of the job the worker had is undone:

   | Old job | Undone |
   | --- | --- |
   | Repairing | the player's repairer count −1 |
   | Mining gold | the player's miner count −1 |
   | Cutting lumber | the player's lumberjack count −1 |
   | Building | the player's count of that building type in progress −1, and the **keep-clear mark** (below) is cleared on the building's footprint (width × width tiles from its top-left tile, no border) |

   Then the worker has no job.
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
     border.
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
made again.

### Farm

Tried only if the player has ever had a completed town hall, keep or castle (the same flag as the "fast first hall" in
`economy/build-train.md`) and the map allows farms (`ALOW` unit bit 16).

Let S be the player's food supply (`economy/build-train.md`, "Food"; in Enhanced mode at most 200), F the number of
farms in progress, and U the food in use (units that are not buildings, minus skeletons, daemons and critters).

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

- When a building of a computer player is **created**, the first entry that is not done and has the same type (after
  the race change) is marked done. "Created" covers the buildings the map places (at load, before the first cycle)
  and a construction site placed in play, so the mark comes when construction starts, not when it ends. There are no
  special cases: a guard or cannon tower marks a guard or cannon tower entry, never the scout-tower entry (0x40), and
  a keep or castle marks a keep or castle entry, never 0x98, 0x99 or a town hall entry.
- When a building of a computer player is **destroyed**, the first entry in list order that is done and whose type
  (after changing its race to the destroyed building's race) is the destroyed building's type loses its done mark,
  so it will be built again. If no entry matches, nothing changes.
- Entries of 0x80 or more are marked done when the research or building upgrade starts (`ai/training-research.md`).
- At the start of a game, entries of 0x80 or more for what the player already has are marked done: codes 0x80, 0x81
  and 0x86 to 0x8F when the upgrade line has already reached the level the code asks for; codes 0x82 to 0x85 when
  the item is already researched; and codes 0x90 to 0x97 when the player already knows the human or the orc item of
  the code (its spells-known bit; Data, "Spell bits for done marks at game start").

Nothing else marks an entry done or clears its mark. In particular nothing marks a research entry done because the
player can't research it.

So the list works like a wish list: each entry stands for one building, and a lost building is replaced.

**Other effects of a computer player's losses** (they don't change done marks):

- When one of its buildings is destroyed and it then has fewer than 3 buildings that are not oil platforms, it
  launches an all-out attack on the human player with the most units. That attack belongs to the attack AI and is
  not covered here.
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
  - **safe:** no unit that can attack and belongs to a player the owner doesn't count as allied stands, on the ground
    or air layer, in the 9 × 9 tiles from (x − 3, y − 3) to (x + 5, y + 5), where (x, y) is the building's top-left
    tile.
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
   the one that minimises **d(depot, mine) + floor(d(worker, mine) / 2)**, distances in tiles as for target
   acquisition. Only a strictly smaller value replaces the best, so the earliest in the list wins ties. Command 23 on
   that mine, miners +1, and a **keep-clear line** is marked from the depot to the mine (below).
3. If there is no depot or no mine, lumber is tried as in 1 (whatever T is).

The job choice is the only place where a computer worker is given a lumber job; the 50-cycle AI routines don't
assign workers.

### Keep-clear marks and site search

**Keep-clear mark:** a per-tile mark that the computer players put on planned building sites and mining paths. It is
the run-time tile mark 0x0400 in the tile flags of `movement/occupancy.md`. The computer's site search refuses tiles with it, so its buildings leave paths and a ring of
space. It isn't in the movement or ordinary placement masks, so it doesn't affect movement or human placement.
Buildings placed by the map get no keep-clear mark. Marks come only from the computer's own building jobs and its
keep-clear lines.

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
| Most buildings | the top-left tile of the player's nearest gold-storing building, complete or not, in the worker's region (strictly nearest, so the earliest wins ties); the worker's tile if none | 6 |
| Farm | as above | 2 |
| Foundry, refinery | as above; no site if the player has no completed shipyard | 1 |
| Shipyard | the nearest town hall (any tier); the water region of the oil patch nearest to it is noted, and the footprint must contain a tile of that region | 1 |
| Town hall (not from the build list) | the nearest gold mine with trips left in the worker's region | 1 |

The search visits square **rings** of radius k·s around the centre, for k = 1, 2, …. Each ring is walked clockwise from
its top-left corner (right along the top, down the right side, left along the bottom, up the left side), in steps of
s; corners are visited twice. It continues while (2k + 1)·s < max(cx, N − cx, cy, N − cy) − 1, where (cx, cy) is the
centre and N the map size. If a ring has no position in the worker's region, the search fails.

Only the first try of a new town hall when no depot is found ("Worker", step 5) uses the border; every other search
uses the plain footprint.

A position (x, y) is accepted if:

- it is on the map and its region value equals the worker's;
- the **footprint** fits on the map: width × width tiles from (x, y), or, with the border, (width + 2) × (width + 2)
  tiles from (x − 1, y − 1) (from x or y itself where that is 0);
- no footprint tile carries a bit of the computer's placement mask: **0x0DDE** for land buildings, **0x0D11** for
  shore buildings. These are the masks of `movement/occupancy.md` plus the keep-clear bit;
- the ordinary placement check (`movement/occupancy.md`, "Building placement masks") also passes.

The first accepted position is the site; the building's top-left is (x, y).

**New-mine search** (a town hall from the build list): every gold mine with **at least 50 trips** and no
gold-storing building within the 13 × 13 tiles from (mx − 5, my − 5) to (mx + 7, my + 7) (mine top-left (mx, my)) is
tried, in the neutral player's list order (`core/unit-slots.md`; every gold mine belongs to the neutral player). A
gold-storing building counts whatever its owner and even if it isn't complete. For each mine, an ordinary search
with step 1 is made around it, and its site is kept only if the **distance from the site's top-left tile to the
mine** is at most 10 tiles, measured as for target acquisition (the same distance the mine choice in "Harvest" uses).
The mine whose site is nearest is chosen; only a strictly nearer site replaces the best, so the earliest mine wins
ties. The search is then run again at that mine to give the site.

### Tankers

The job choice runs at the same two moments for a computer player's tanker (and for any peon-flagged sea unit, which
only custom unit data creates):

1. **Attacker:** the reaction, as for a worker.
2. **Depot:** the nearest completed oil-storing building of the player whose footprint touches the tanker's water
   region (`economy/harvest.md`); on a tie the latest in the list wins.
3. **Carrying:** command 24 to that depot. If there is none, nothing happens.
4. **No depot:** nothing happens, and there is **no wait**, so the choice runs again the next time the idle logic runs.
5. **Platform:** among the player's own oil platforms that have oil left and are not dying, the one that minimises
   **d(depot, platform) + floor(d(platform, tanker) / 2)**, strictly (earliest wins ties). There is no region test.
   Command 23 on it.
6. **No platform:** the oil patch nearest to the depot's top-left tile (strictly nearest). If an oil platform is
   affordable, the tanker gets the build command for it there, and the player pays at once.

## Data

| Item | Value |
| --- | --- |
| Idle wait | (r AND 15) + 44 cycles, one RNG draw |
| Hall cooldown after a hall is started | 10 tries |
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
| New-mine search | mines with ≥ 50 trips; no gold store in (−5, −5) to (+7, +7); site within 10 tiles; nearest site wins |
| All-out attack threshold | fewer than 3 buildings other than oil platforms |
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
| Computer player with 3 buildings (one an oil platform) loses a farm | 1 building other than platforms left: the all-out attack starts |
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
| Map-placed town hall at game start | No keep-clear mark on or around it |
| Computer peon carrying wood gets command 24 to its hall and the path search returns no steps | Its order ends; the next job choice gives command 24 to the same hall again, with no wait |

## Open questions

- "Prerequisites met" in the build list: the rules are in `economy/tech-tree.md`. Whether the computer applies them
  even when requirements aren't enforced must be checked. Default: it uses the same rules as the player's interface.
- Exactly which entries of 0x80 or more stand for which research, and how the request is acted on.
- Confirm in the running game that a worker whose tree search fails mines gold in the same choice (default: it does,
  as described above).
- Confirm the keep-clear line in the running game, for example by watching where a computer player's later buildings
  avoid a diagonal strip between its hall and a mine.
- The general placement check adds rules for the site (beyond the masks) that are in `movement/placement.md`; their
  interaction with the border option is assumed to use the unbordered footprint.
- The all-out attack started by losing buildings is not specified here (it belongs to the attack AI).
- That a construction site placed in play marks its entry when construction starts (not when it ends) comes from
  the static study only; no trace shows it yet.
- A worker inside a depot starts the job choice's tree search from the depot's top-left tile and uses that tile's
  region value. This assumes region values ignore buildings, as `movement/occupancy.md` implies; not checked
  separately.
