# Vision: fog of war, sight and detection

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11). Round-87 consolidation (B-038, the vision
  area) reviewed by a spec-review session, 2026-10-21; the changes and the review are in
  `ERRATA-vision-consolidation.md`.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the unit flags in the
  public PUD format specification (pudspec.txt), the default unit data (`Rez\unitdata.dat`), and Enhanced oracle
  traces named at each rule and vector. The sight shapes were checked by expanding the program's shape data into
  quarter tiles and comparing them with the rule below; they match for every sight value. Each subsection ends with an
  **Evidence** line: *traced* (an oracle trace, named), *static study* (the program, not yet seen running) or *game
  data* (the shipped data files). Oracle traces don't record the fog maps (`oracle/trace-format.md`), so vision is
  traced only through what it changes: target searches, lost targets and the units that act on them.
- **Rules mode:** every rule here is Enhanced unless it says **Classic mode**. The Classic-mode rules, all from static
  study, are: the 101-cycle refresh, holy vision leaving the refresh countdown alone, whose display a reveal changes,
  the reveal's "seen" quirk, detection of invisible units and submarines without shared vision and without the
  "complete" test, and the diplomacy command leaving vision alone. No Classic trace was used. F47 was read from the
  attract demo, which plays with the Enhanced flag clear (the recording's own setting): it states demo-only facts and
  changes no Enhanced rule.
- **Changes:** 2026-10-11 to 2026-10-15: rounds 3, 4, 30, 32, 35, 36, 53, 65, 70 and 71 and the review of
  2026-10-12 (listed in the errata file's Inventory; the old text is tidewater-docs `e45ac76`).
- **Changes:** 2026-10-21, round 87 (B-038 consolidation, `ERRATA-vision-consolidation.md`; static study; reviewed
  by a spec-review session, 2026-10-21): rewritten as one model. Changes of meaning: no reveal when a builder comes
  out (C2) or a passenger is unloaded (C3), accepted as static gap-fills. Held with the old text as the default: a
  game set up with fog off keeping its "seen" bits (C1, Q2) and inside units' detection (C4, Q3). Statements X1 to
  X12 made explicit. Old vectors kept verbatim as F01 to F26; new vectors F27 to F42.

- **Changes:** 2026-10-21, round 89 (B-057 multiplayer, `ERRATA-round89-multiplayer.md`; reviewed by a spec-review
  session, 2026-10-21): "What uses vision": the unexplored test is placement-mode only in network games and the
  hidden-units exception reads the builder's owner's bits everywhere; "Shared vision": a leaving player's removal
  happens when the leave is carried out (chat-room games, Enhanced; Classic mode never), and a defeat changes the sets
  only in a Battle.net game (static study; with `ERRATA-audit-game.md` erratum 7). Fog off from the start is a
  multiplayer lobby choice only (`ERRATA-audit-vision.md` erratum 1, setup half, data-backed): Q2, F27 and the two
  fog-off passages now name multiplayer games only.
- **Changes:** 2026-10-21, round 90 (oracle traces with the new `vision` extra, `ERRATA-round90-traces.md`, and the
  B-040 vision audit, `ERRATA-audit-vision.md`; reviewed by a spec-review session, 2026-10-21): a game set up with fog
  off starts with every "seen" bit set (C1, traced; Q2 closed); a unit inside something is detected by everyone (C4's
  passenger half, traced; Q3 narrowed to the detector half, still held). F27, F28 and F31 state the traced readings;
  new traced vectors F43 and F44. Labelled static notes from the audit: an undefined controller code is tested like a
  human player (erratum 3), a neutral invisible unit's set (erratum 4, not mirrored), the "detected by" test for
  players 8 to 15 (erratum 6), the gold-mine inside sight's test (erratum 7). Erratum 2 is an implementation note on
  X12 (vector F45). Erratum 8 (demo playback's visible map) is held as Q4.
- **Changes:** 2026-10-21, round 91 (oracle traces with the `fogmaps` extra and read-only demo logs,
  `ERRATA-round91-traces.md`; reviewed by a spec-review session, 2026-10-21): switching the option off assigns the
  local set on unexplored tiles, and switching it on changes no map until the next refresh (traced, F45 now a traced
  vector). Demo playback has the fog-of-war option **on** and the reveal-map cheat on; its visible map and "seen" bits
  start clear but the first start-up cycle's refresh fogs them as in any game (traced, F47; Q4 closed). The old
  reading "demo playback = option off from the start" is withdrawn.
- **Changes:** 2026-10-21, round 92 (`ERRATA-round92-traces.md`, item 2a; reviewed by a spec-review session,
  2026-10-21): phase 5 states that map owner codes 0 and 1 are the human and computer kinds (static study, untraced;
  it restates `game/setup.md`, "Other owner codes", and changes no rule).
- **Changes:** 2026-10-22, round 99 (`ERRATA-audit2-net.md` erratum 2; reviewed by a spec-review session,
  2026-10-22): Q1 closed: the diplomacy command (now `game/diplomacy.md`) sets the sender's whole set, its own entry
  included, so a player can leave its own set; Tidewater mirrors it (static study; review ruling). The default "a
  player is always in its own set" is withdrawn.
- **Changes:** 2026-10-22, round 100 (`ERRATA-audit2-net.md`, "Round 100"; reviewed by a spec-review session,
  2026-10-22): the team-game rule of "Shared vision" is a Battle.net team game's (static study), labelled Battle.net
  only, not mirrored. The sets of players 0 to 7 are saved in Enhanced mode (`game/save-load.md`).
- **Changes:** 2026-10-22, round 104 (oracle sweep, `ERRATA-round104-sweep.md` SW5; reviewed by a spec-review session,
  2026-10-22): "Unit visibility (phase 5)", rule 1: a unit with the used mark is hidden from everyone, dying or not
  (traced for the oil patch, SW5a, and a covered ruin, SW8a; static study for the other cases).

## Behaviour

### The model

Vision is four pieces of state, all kept per game and changed only by the rules below:

1. **Two fog maps for the local player**: what has ever been seen (**explored**) and what is seen now (**visible**),
   at quarter-tile resolution. They drive the display, and the explored map also limits where the local player may
   place a building. Nothing else in the simulation reads them.
2. **A per-player "seen" map**: one bit per player per tile, cleared when the player (or a player sharing vision with
   it) reveals the tile, set again at each fog refresh. Game rules use it, through the hidden-from sets.
3. **Detected-by sets** (phase 1): per unit, the players that can perceive it at all. Only invisibility and the
   submarine flag make a unit undetected; fog has nothing to do with it.
4. **Hidden-from sets** (phase 5): per unit, the players whose "seen" map doesn't cover any of its tiles. This is the
   fog test.

Phase numbers are those of `core/timing-and-rng.md`. One cycle, as far as vision goes:

| Step | What happens to vision |
| --- | --- |
| Command turn (before phase 1) | Players' commands are carried out. A command's target check sees the detected-by sets of the previous cycle. **Multiplayer:** the diplomacy command changes shared vision here ("Shared vision") |
| Phase 1, detection | Every unit's detected-by set is worked out again ("Detection (phase 1)") |
| Phase 2, computer players | Uses the sets as they stand: detected-by from phase 1, hidden-from from the previous cycle's phase 5 |
| Phase 3, fog refresh counter | On a refresh cycle: the visible map is kept for redrawing, then (fog-of-war option on) the visible map is fully fogged and every "seen" bit set ("The fog refresh") |
| Phase 4, units | On a refresh cycle each unit applies its sight just before its own update. During the updates: movement steps, workers entering, strike reveals and holy vision reveal at once ("When sight is applied") |
| Phase 5, unit visibility | Every unit's hidden-from set is worked out again, for all players, from the "seen" map as phase 4 left it ("Unit visibility (phase 5)") |
| Phases 6 and 7 | Minimap every 40 cycles; the screen is redrawn where the visible map changed. Display only |

So a hidden-from test made during phase 4 (an attack search, say) sees the sets of the previous cycle's phase 5,
which already include every reveal of that cycle's phase 4. A reveal made in phase 4 of cycle K is first seen by
searches in cycle K + 1.

**Evidence:** static study (the cycle's order); traced through its consequences (`xhuman10-spells-700` and
`human06-31357`, vectors F23 and F24).

### Terms

- **Human, computer, passive, nobody:** the player kinds of `movement/movement.md`, "Player kinds".
- **Classic and Enhanced:** the two rule modes of `core/timing-and-rng.md`.
- **Shared-vision set of player p:** in Enhanced mode, the set of players that p shares its vision with ("Shared
  vision"). It always starts with p in it; only a diplomacy command can take p out (`game/diplomacy.md`). **Classic
  mode:** only p counts.
- **Unit (as opposed to building):** a type with any of the flag bits 0–4 (land, air, explode, sea, critter).
- **Complete:** a building whose construction has finished. Units count as complete.
- **Tile of a unit:** the tile it has claimed, its top-left tile for a building (as in `movement/movement.md`). The
  **drawn tile** is the tile containing its top-left pixel; while a unit moves, the two can differ.
- **Inside:** a passenger in a transport, or a worker inside a building, mine, oil platform or construction site.

### The fog maps

Each map tile has, in both the explored map and the visible map, four **quarter flags**: top-left, top-right,
bottom-left and bottom-right. A set flag means that quarter is fogged.

- **Explored map.** At the start of a game every quarter of every tile is fogged. Revealing clears quarters, and
  nothing sets them again. A tile with all four quarters fogged is unexplored and is drawn black.
- **Visible map.** At the start of a game every quarter is fogged. (Demo playback starts with it clear, but the fog
  refresh of its first start-up cycle fogs it again, so this is never seen; traced, round 91, F47.) Revealing clears
  quarters. On each **fog refresh**
  (below), if the fog-of-war option is on, every quarter is fogged again. If the fog-of-war option is off, the
  visible map is never re-fogged.

Revealing an area clears the same quarters in both maps. The result for one tile is the **intersection** of its old
fogged quarters and the fogged quarters of the area being revealed, so revealed quarters only accumulate between
refreshes.

The state of one tile is stored as one of 17 codes. The code is what selects the fog overlay graphic, so a renderer
needs the mapping:

| Code | Fogged quarters | Shape of the fog on the tile |
| --- | --- | --- |
| 0 | none | clear |
| 1 | TL, TR, BL | all but the bottom-right quarter |
| 2 | TL, TR | top half |
| 3 | TL, TR, BR | all but the bottom-left quarter |
| 4 | TL, BL | left half |
| 5 | none | clear (never produced; behaves like 0) |
| 6 | TR, BR | right half |
| 7 | TL, BL, BR | all but the top-right quarter |
| 8 | BL, BR | bottom half |
| 9 | TR, BL, BR | all but the top-left quarter |
| 10 | TL | top-left quarter |
| 11 | TR | top-right quarter |
| 12 | BL | bottom-left quarter |
| 13 | BR | bottom-right quarter |
| 14 | TL, BR | two opposite quarters |
| 15 | TR, BL | two opposite quarters |
| 16 | all | fully fogged |

TL, TR, BL and BR are top-left, top-right, bottom-left and bottom-right. A code 0 tile in the explored map that is
code 16 in the visible map is explored but fogged; the main view covers it with a black checkerboard
(`ui/map-drawing.md`, "Fog").

**Evidence:** static study.

### The "seen" map

One byte per tile, one bit per player (bit p for player p, players 0 to 7). A set bit means the tile is **not seen**
by that player during the current refresh period. Players 8 and up have no bit: nothing is revealed for them.

- At the start of a game all bits are set. Demo playback starts with them all clear, and the refresh of its first
  start-up cycle sets them all (traced, round 91, F47). A multiplayer game set up
  with the fog-of-war option off is no exception: it starts with every bit set too (a single-player custom game
  always has fog on, `game/setup.md`). With the option off the refresh never sets or clears them (below), so in such a
  game a bit clears only where a reveal touches the tile, and a unit on ground its side has never revealed stays
  hidden (static study, round 87, C1; traced, round 90: `mp-dark-paths-bne-fogoff-game-vision-300`, F43).
- On each fog refresh, if the fog-of-war option is on, all bits are set again. With the option off they are left as
  they are, so a tile seen once stays seen.
- When an area is revealed for player p, every tile that the area touches (any quarter revealed) has the bits of p's
  shared-vision set cleared, whoever the local player is.
- **Classic mode:** a reveal for the local player leaves a tile's "seen" bits alone when that tile's visible code was
  already 0 before the reveal. Such a tile was revealed earlier in the same period, so its bits are already clear;
  no outcome depends on it (X9).

**Evidence:** static study.

### Sight areas

A sight area is defined by a **centre tile** (cx, cy) and a **sight value** r from 0 to 9.

Measure each quarter tile by how far it lies from the centre point of the centre tile, in tiles, separately on each
axis: take the distance from that point to the quarter's far edge and round it up. Call these K_x and K_y. The four
quarters of the centre tile have K = 1 on both axes; so do the near halves of the four tiles next to it.

A quarter is inside the area when:

| Sight value r | Quarter is inside when |
| --- | --- |
| 0 or 1 | K_x ≤ 1 and K_y ≤ 1 |
| 2 to 4 | K_x ≤ r and K_y ≤ r and K_x + K_y ≤ 2r − 1 |
| 5 to 9 | K_x ≤ r and K_y ≤ r and K_x + K_y ≤ 2r − 2 |

In words: a square 2r tiles wide centred on the centre point of the centre tile (2 tiles wide for r = 0 or 1), with
its corners cut. For r from 2 to 4 one tile-sized square is cut at each corner; for r from 5 to 9 a two-step
staircase is cut (3 tile-sized squares per corner). Because the square is centred on the middle of a tile, its edges
run through the middles of the outermost tiles, so those tiles are half revealed.

**Tiles touched.** A tile is touched (and so marked seen) when any of its quarters is inside the area. With the tile's
offset (dx, dy) from the centre tile, let a = max(|dx|, 1) and b = max(|dy|, 1). The tile is touched when |dx| ≤ r,
|dy| ≤ r (with r taken as 1 for sight 0), and a + b ≤ 2r − 1 (r from 2 to 4) or a + b ≤ 2r − 2 (r from 5 to 9).

| Sight value | Tiles touched |
| --- | --- |
| 0, 1 | 3 × 3 = 9 |
| 2 | 5 × 5 minus 4 corners = 21 |
| 3 | 45 |
| 4 | 77 |
| 5 | 11 × 11 minus 12 = 109 |
| 6 | 157 |
| 7 | 213 |
| 8 | 277 |
| 9 | 349 |

**Map edges.** The part of the area outside the map is ignored: a tile is used when both coordinates are from 0 to
the map width − 1. Maps are square, so the width serves for both axes (X8).

**Whose display changes.** The explored and visible maps change only when the local player is in the revealing
player's shared-vision set. **Classic mode:** only when the revealing player is the local player. The "seen" map
changes for every revealing player.

**Sight values above 9** (custom unit data only) have no area in the program; the behaviour is undefined, and maps
are assumed never to use them.

**Evidence:** static study (shape data expanded and compared, every sight value).

### Who reveals, and when

**Sight of a unit.** Its centre tile is its tile plus half its size in tiles, rounded down, on each axis. For a 1 × 1
unit that is its own tile; for a 2 × 2 building at (x, y), (x + 1, y + 1); for a 3 × 3 building also (x + 1, y + 1);
for a 4 × 4 building (x + 2, y + 2). Its sight value is:

| Unit | Sight value |
| --- | --- |
| A building that isn't complete | 3 |
| A ranger, or a berserker, whose player has researched the matching scouting upgrade | 9 |
| Anything else | the type's sight range from the unit data (0 to 9) |

The info panel shows a related number, not this one (`ui/info-panel.md`). Scouting changes sight only, not range
(`combat/damage.md`).

#### When sight is applied

These are **all** the reveals. The first three happen only for units of **human** players; the last two for any
player 0 to 7 (X5).

1. **Fog refresh cycles.** In phase 4 of a refresh cycle, each human player's unit whose slot is in use applies its
   sight, when it is updated and before its order runs. That includes a unit in its **death animation** (order 1;
   traced: `xorc4-30000` n = 19507, F25 and F26 below) and bodies, ruins and units being removed (order 0), until
   the slot is released (`core/corpses.md`; static study). A unit inside something doesn't, with one exception: a
   worker inside a **gold mine** reveals with the gold mine's sight (3 in the default data), centred on the gold
   mine's centre tile (X7). A tanker inside an oil platform, a passenger in a transport and a worker inside any
   other building reveal nothing. The test is on the kinds, not on "worker": any **land** unit inside a unit of the
   **gold-mine type** reveals this way (static study, `ERRATA-audit-vision.md` erratum 7). With the default data the
   two are the same, as only workers enter mines; the difference matters only for custom unit data.
2. **Each movement step.** When a human player's unit moves onto a new tile, its sight is applied at once from the
   new tile (`movement/movement.md`). This covers walking, flying and sailing steps, a transport's landing steps and
   its push-off steps.
3. **A worker entering a building.** A human player's worker or tanker that goes inside a gold mine, an oil
   platform, a depot or a construction site reveals a sight-3 area at once (`economy/harvest.md`, order 25, step 2;
   `economy/build-train.md`, V67). Its centre is the building's top-left tile moved one step
   in the direction the worker faces: facings 0 to 7 run clockwise from north, and the steps are (0, −1), (1, −1),
   (1, 0), (1, 1), (0, 1), (−1, 1), (−1, 0), (−1, −1) (X6). The facing is the one the unit has as it goes in.
4. **Strike reveal.** At each strike point of a unit whose order is attack-type (orders 9, 12, 15, 16, 17, 20 and 21,
   and the spells Slow and Haste), if the order has a target unit whose owner isn't the attacker's owner and is one
   of players 0 to 7, a sight-2 area centred on the attacker's tile is revealed for the **target's owner** (with its
   shared-vision set, as for any reveal), whatever kind of player the target's owner is. It comes before the strike
   itself, after the attacker's invisibility ends (`combat/damage.md`; `spells/spells.md`, "The strike point").
   Towers have no strike point of this kind and make no strike reveal (`combat/damage.md`). The area stays seen by
   that owner until the next fog refresh, so the target's owner can see the attacker until then, and the attacker
   passes the fog test of a search made in that time, the attacker seed included (`orders/orders.md`, "Choice";
   traced: `human06-31357` n = 9642 and `xhuman11-10000`). Only order 12 is traced; the other orders and the spells
   are static study only.
5. **Holy vision.** Seven sight-9 areas are revealed for the caster's player, whatever its kind; the points and their
   bounds are in `spells/spells.md`, "The spells". In Enhanced mode the cast also sets the fog refresh countdown to
   150 ("The fog refresh"; X1).

**No other event reveals.** In particular a unit that is created, trained, placed by a spell (the eye of Kilrogg),
let out of a building (a builder after construction, a worker leaving a mine or depot) or unloaded from a transport
reveals nothing at that moment. It reveals at its first movement step or at the next fog refresh, whichever comes
first (C2, C3).

**Evidence:** static study (all five, and that nothing else reveals); traced for rule 1's dying units
(`xorc4-30000`) and rule 4's order 12 (`human06-31357`, `xhuman11-10000`).

#### The fog refresh

The **refresh countdown** starts at 1 and is lowered in phase 3 of every cycle, the two start-up cycles included;
when it reaches 0 the cycle is a **refresh cycle**, and the countdown is reloaded to 100 (**Classic mode:** 101) when
it is next lowered. So the first refresh is in the first start-up cycle, and then one every 100 cycles (101): in the
normal cycles that end with the cycle counter at 99, 199, … (**Classic mode:** 100, 201, …; `core/timing-and-rng.md`,
which with `game/lifecycle.md` says how a paused cycle moves them: a paused cycle doesn't lower the countdown).

**Holy vision moves the schedule** (Enhanced; X1). A holy vision strike, by any player's caster (a computer player's
included), sets the countdown to 150. The next refresh then comes in the 150th cycle after the cast cycle, and the
refreshes go on every 100 cycles from there. So "99, 199, …" holds only until the first holy vision of the game; a
tile can stay visible up to 150 cycles after its last viewer has left. **Classic mode:** holy vision leaves the
countdown alone.

In phase 3 of a refresh cycle the previous visible map is kept for redrawing. Then, if the fog-of-war option is on,
the visible map is fully fogged and every "seen" bit is set. Phase 4 then re-applies all sight. So a tile that no unit
sees any more stays visible until the next refresh: **up to 100 cycles late** (101 in Classic, 150 after a holy
vision). Phase 7 only redraws the screen where the visible map changed.

**Evidence:** static study; traced for the first refreshes (`core/timing-and-rng.md`, the Human01 vector, and
`xhuman10-spells-700`, F23).

### Detection (phase 1)

Each cycle, each unit gets a **detected-by** set of players. Rules, first match wins:

| Unit | Detected by |
| --- | --- |
| Dead, dying, removed or inside something (a passenger, a worker in a mine or building) | everyone |
| Invisible (the Invisibility spell's timer is running) | its owner's shared-vision set; nobody if the owner is not one of players 0–7. **Classic mode:** its owner only (nobody if the owner is not one of players 0–7) |
| Its type has the submarine flag (bit 6) | see below |
| Anything else | everyone |

**Submarines.** For each player p (0 to 7), p detects the submarine when any of these holds:

- p is its owner;
- p is in the owner's shared-vision set (**Classic mode:** not used);
- p owns a **detector** whose tile is within 6 tiles of the submarine's tile on both axes (tile positions, so the
  top-left tile of each). A detector is a unit whose type has the "can see submarine" flag (bit 7), that is alive
  (not dead, dying or removed), and that is complete (**Classic mode:** an incomplete one counts too).

A unit **inside something** is detected by everyone, whatever else holds: an invisible passenger's set is everyone
from the first phase 1 after it goes in, though its invisibility timer still runs, and its owner's set again from the
first phase 1 after it comes out still invisible (static study, round 87, C4; boarding traced, round 90:
`human12-human12-invisible-passenger-game-vision-4550-5600`, F44; coming out isn't traced). Being inside also hides it from everyone
("Unit visibility", rule 1), so no search finds it either way. A **detector** inside something still counts as a
detector. **Held (Q3):** static study (C4's detector half) finds that an inside detector doesn't count; that reading
isn't the default until a trace shows it. No detector can be inside anything with the default data.

**Players 8 to 15** (Enhanced). An invisible unit or submarine of a player 8 to 15 starts from an empty set (the
invisible row's "nobody"). The original reads a value past its per-player table here, which for the neutral player is
another player's "has left or been defeated" mark; Tidewater doesn't mirror that read (static study,
`ERRATA-audit-vision.md` erratum 4; no shipped map has such a unit).

The set is then **widened** (Enhanced; **Classic mode:** not done): players p = 0, 1, …, 7 are taken in turn, and
if p is in the set at that moment, p's shared-vision set is added to it. A player added by an earlier p is itself
widened when its turn comes; one added by a later p is not, because its turn has passed (X4; F34).

**Which types detect** (default unit data): Kurdan and Sky'ree, Deathwing, the gnomish submarine, the giant turtle,
the gnomish flying machine, the goblin zeppelin, the gryphon rider, the dragon, the eye of Kilrogg, the daemon, and
the scout, guard and cannon towers of both races ("Data"). Destroyers, battleships and juggernauts don't. None of
them can be inside anything with the default data, so Q3's detector part matters only for custom unit data.

**"Detected by" a player 8 or more.** In an Enhanced game whose local player number is 8 or more (not possible in
single player), the UI tests that use "detected by the local player" accept detection by any player
(`ui/map-drawing.md`). The game's own "detected by p" test (target searches and lost targets, `orders/orders.md`,
"Terms") does the same for a player p from 8 to 15: a unit counts as detected by p when any player detects it.
**Classic mode:** never. This is unreachable with shipped data: only a neutral unit's search would use it, and the
neutral player is allied with everyone, so its search skips every unit first (static study, `ERRATA-audit-vision.md`
erratum 6).

#### A unit created during a cycle

A unit created during a cycle (trained, built, a new construction site or oil platform, or any other new unit)
starts with an **empty** detected-by set, and this phase first fills it in at the start of the next cycle. For the
rest of its first cycle no player detects it, its owner included, so every "detected" test fails for it
(`orders/orders.md`, "Terms"): a command aimed at it later in that cycle (by a computer player or by a unit's own
logic) drops the target and makes idle pending ("Commands forget a stale attacker"), and target searches skip it. A
builder's link to its new construction site is set without a detection test, so building isn't affected. (Static
study, and the oracle traces behind `economy/harvest.md` vectors V55 and V56; a scan of every committed trace finds
no unit other than a builder targeting a unit in the cycle it was created. Audit round 70, erratum 2 in
`specs/ERRATA-audit-harvest.md`.) The players' commands are carried out (the **command turn**) at the start of a
cycle, before its phase 1. So the command turn after the creation cycle comes **before** the detection phase that
first detects the new unit, and a player's command carried out in that turn also finds it undetected; from that
cycle's phase 1 on it is detected as usual (static study, round 71, `ERRATA-round71.md`, Q1). A unit placed before
the first cycle (the map's units) is detected by the first start-up cycle's phase 1, before any test can look at it,
so its starting set makes no difference (Q2).

**Evidence:** static study; game data (the detector list); traced through V55, V56 and the target-loss vectors of
`orders/orders.md` (V208, V209).

### Shared vision

Each player 0 to 7 has a **shared-vision set**. It decides whose "seen" bits a reveal clears, whose display a reveal
changes, and (Enhanced) who detects invisible units and submarines.

- **Game setup** puts each player alone in its own set (`game/setup.md`). **Battle.net only (not mirrored):** in a
  Battle.net team game, team-mates are added to each other's sets (static study; X10, narrowed in round 100).
- **Saves** keep the sets of players 0 to 7 in Enhanced mode (`game/save-load.md`; static study, round 100).
- **The diplomacy command** (multiplayer; `game/diplomacy.md`): in Enhanced mode it sets the sending player's alliance
  row and its shared-vision set to the values the command carries, in the command turn. The set is written whole,
  the sender's own entry included, so a command can take the sender out of its own set (a mirrored quirk; static
  study, round 99). **Classic mode:** it changes the
  alliances only; shared vision never changes during play (static study; X10).
- **A player who leaves a network game** is removed from every set, and its own set becomes just itself: in a
  chat-room game (Direct Connect, modem, IPX; Tidewater's Direct IP and Local Network) when its leave is carried out
  in the turn (**Classic mode:** never; shared vision doesn't change during play), and in a Battle.net game when its
  leave is carried out or the victory check finds it out (`net/leave-and-drop.md`, `game/victory.md`; static study,
  rounds 87 and 89; X10). A player defeated in a chat-room game keeps its sets (DECISIONS.md, 2026-10-21).

Nothing else changes the sets. A set is used as it is, also when it doesn't hold its own player (after a diplomacy
command; `game/diplomacy.md`, "A player outside its own set"): nothing puts the player back until another diplomacy
command or a leave changes the set.

**Evidence:** static study.

### Unit visibility (phase 5)

Each cycle, in phase 5, each unit gets a **hidden-from** set of players. This is done for every player, not only the
local one, and it is game state: target searches read it (X2). The sets depend only on game state (the "seen" map,
the player kinds, the units and the reveal-map cheat), never on which player is local, so in a multiplayer game every
machine works out the same sets in the same cycle. The same holds for the "seen" map itself (the Classic-mode quirk
under "The "seen" map" reads the local visible map but leaves the same bits). Only the two fog maps, the remembered
building looks and what they drive (drawing, the minimap, sounds, the placement test) belong to the local player.
Rules, first match wins:

1. A unit that is dead, removed, inside something or **carries the used mark** (`core/corpses.md`, "The used mark";
   the inside mark is that same mark) is hidden from everyone, **even with the reveal-map cheat on** (X3; the old text
   said both this and "with the cheat, nothing is hidden", and this order settles it). So a unit given the used mark
   while dying is hidden from that cycle on: an oil patch under a new oil platform (traced: `sweep-xhuman5`, SW5a), a
   ruin or body a new building covers (traced: SW8a in `core/corpses.md`), and by static study a polymorph victim, a
   body raised by raise dead and an oil platform killed with oil left. A dying unit without the used mark is not
   covered by this rule. No target search depends on the order: the search skip list leaves out dying, dead
   and inside units by itself (`orders/orders.md`, "A candidate is skipped if").
2. With the reveal-map cheat on, nothing else is hidden from anyone.
3. Otherwise, for each player p:

| Player p | The unit is hidden from p when |
| --- | --- |
| Computer | never |
| Nobody (empty slot) | always |
| Human or passive | no tile of the unit's footprint is seen by p (its "seen" bit is set on every one of them) |

A slot whose map owner code is 0 or 1 keeps that code through setup, and the codes are the human and the computer
kind in every later test (`game/setup.md`, "Other owner codes"): a code-0 player gets the footprint test and a code-1
player is never hidden from (static study, round 92; untraced, no shipped map has either code). A player whose
controller is none of these kinds (only a custom map's owner code 8 or more can leave one) is tested like a human
player (static study, `ERRATA-audit-vision.md` erratum 3; no shipped map has one). Detection (phase 1) has no
controller test, so none of these codes changes it.

The footprint is the unit's type size in tiles, placed at the tile containing its top-left **pixel** (so a moving unit
counts from where it is drawn, not from the tile it has claimed). Parts outside the map are ignored. The remembered
building looks of `ui/map-drawing.md` go through the same pass.

A new unit starts hidden from everyone (`core/unit-slots.md`, "Creating a unit") until the next visibility pass.
The units update before the first visibility pass of the first start-up cycle, so no automatic target search can
succeed in that cycle; the first that can are in the second start-up cycle. A unit created in play is likewise not a
candidate for searches made earlier in its cycle than the visibility pass (static study only).

Because computer players never apply sight (except through strike reveals and holy vision), their "seen" bits are
mostly set, but that doesn't matter: computer players treat every unit as not hidden. Passive players never apply
sight, so with the fog-of-war option on every unit is hidden from them after the first refresh; passive players
don't attack, so nothing is known to depend on it.

**Evidence:** static study; traced through the first start-up cycle (`xhuman10-spells-700`, F23) and the dying units'
sight (`xorc4-30000`, F25).

### What uses vision

What the fog hides, in the simulation and in the interface. Everything not listed here ignores vision; in particular
computer players' decisions never test fog (`ai/computer-player.md`, `ai/attack-ai.md`, `ai/caster-ai.md`).

| Rule | Uses |
| --- | --- |
| Attack target search (`orders/orders.md`, the acquisition search) | The candidate must not be hidden from the searcher's owner and must be detected by it |
| An order aimed at a unit loses its target (`orders/orders.md`) | Checked on each movement step: the target is lost if it is no longer detected by the owner. The hidden-from set is not checked |
| Placing a building (human owner) | In placement mode (display): the local player can't place on an unexplored tile (any tile of the site fully fogged in the explored map) unless the reveal-map cheat is on. The arrival check makes the same test in single player only; the command turn never does (`ui/placing-buildings.md`). In placement mode, the command turn and on arrival (game state): units standing on tiles the builder's owner doesn't see don't block the placement; the owner's "seen" bits are used, never the local player's |
| Clicking a building on the map | Possible when its tile is explored and either the building isn't hidden from the local player or it is a building (buildings stay selectable in fog once explored) |
| Minimap units | A unit (not a building) is drawn when it isn't hidden from the local player and is detected by it. A building is drawn when it has been seen (see the open questions) and is detected |

More users, each specified where it stands:

| Area | Rule | Spec |
| --- | --- | --- |
| Simulation | A command's target check: detected only (a fogged target is accepted) | `orders/orders.md`, "Terms"; F22 |
| Simulation | A recorded attacker that isn't detected is ignored by searches; fleeing tests no vision | `orders/orders.md`, "Terms" |
| Simulation | Repair, boarding and transports lose an undetected target | `economy/repair.md`, `orders/transports.md` |
| Simulation | A spell's walk and strike test no vision; only the command does | `spells/spells.md` (S92) |
| Interface | Picking and selecting units | `ui/selecting-and-commanding.md` |
| Interface | Drawing units, remembered building looks, effects (the 3 × 3 "seen" test), the fog itself | `ui/map-drawing.md` |
| Interface | Minimap terrain, units and remembered buildings | `ui/minimap.md` |
| Interface | Unit sounds silent when hidden; placed sounds silent on unseen tiles | `ui/sounds.md` |
| Interface | "Can't build on unexplored ground" message | `ui/placing-buildings.md` |

**Evidence:** static study; the search rules are traced (`orders/orders.md`).

### The minimap

The minimap is redrawn every 40 cycles. It is cosmetic.

- **Terrain:** a minimap pixel shows the terrain colour when its tile is fully clear (code 0) in the map it samples,
  and black otherwise. On a 128 × 128 map, even pixel columns sample the explored map and odd columns sample the
  visible map. So explored but fogged ground shows as every other column black. On smaller maps the alternation is
  still by minimap pixel column, at the pixel's tile (`ui/minimap.md`, "Fog and the terrain option").
- With the reveal-map cheat on, all terrain is shown.

**Evidence:** static study.

### Fog-of-war option and cheat

- **Option off from the start** (a multiplayer game set up with fog off in the lobby): the fog maps start fogged as in
  any game, and refreshes never set or clear the "seen" bits. The game starts with every bit set, so a unit stays
  hidden from a player until a reveal of that player's side touches its footprint, and then stays visible (traced,
  round 90; F27, F28, F43).
- **Option off:** the visible map is not re-fogged and the "seen" bits are not set again on refresh, so everything
  seen once stays visible. When the option is switched off during play (the game menu; the change takes effect when
  the dialog closes, `ui/game-menu.md`), the visible map is set equal to the explored map. The "seen" bits are then
  cleared on every tile that isn't fully unexplored; on a fully unexplored tile they become exactly the local
  player's shared-vision set (**Classic mode:** the local player's bit), so only the local side still counts it
  unseen (X12). The option can be switched only in single player (`ui/game-menu.md`), where the local player's set is
  just itself; there it gives the same bits as the old wording ("cleared, except the local player's bits on
  unexplored tiles") whenever the local player's bit on a fully unexplored tile is set, which it is unless the fog
  cheat cleared it and the option was turned on and off again within one refresh period (traced, round 91: F45).
  Switching the option **on** changes no map until the next fog refresh (traced, F45).
- **A cheat** turns fog off in single-player games: the `showpath` developer word, when it turns its flag on during
  a game (`game/cheats.md`, "Word codes"). It clears the option, makes the visible map fully clear and clears all
  "seen" bits (traced, round 91, F45).
- **The reveal-map cheat** makes nothing hidden (except dead, removed and inside units, rule 1 above) and draws the
  whole minimap. It doesn't change the maps. Switching it resets the remembered look of buildings whose whole
  footprint is unexplored (`ui/map-drawing.md`).
- **Demo playback** (the attract demo) has the fog-of-war option on and the reveal-map cheat on (`game/cheats.md`).
  Its visible map and every "seen" bit start clear, the refresh of its first start-up cycle fogs the visible map and
  sets every bit as in any game, and refreshes go on; the reveal-map cheat hides nothing and draws no fog, so none of
  it shows (traced, round 91: F47). The attract demo plays with the program's Enhanced rules flag clear (Classic rules,
  `core/timing-and-rng.md`): that is the recording's own setting, observed and not chosen. Nothing in this bullet
  changes an Enhanced rule.

**Evidence:** static study; switching the option off and on traced (round 91, F45); demo playback traced (round 91,
F47).

### Random draws

Vision makes **no** draws from the game's generator: detection, the refresh, the reveals and the visibility pass
are all deterministic (X11). Holy vision's sounds follow `ui/sounds.md`.

**Evidence:** static study.

## Data

| Item | Value |
| --- | --- |
| Fog map size | 128 × 128 tiles (the largest map), one code per tile per map |
| Codes per tile | 17 (table above) |
| "Seen" map | 1 byte per tile, bit p = player p not seen |
| Sight values | 0 to 9 (0 and 1 give the same area) |
| Sight of an incomplete building | 3 |
| Scouting sight (ranger, berserker) | 9 |
| Worker entry sight | 3, one step from the building's top-left tile in the worker's facing |
| Strike reveal sight | 2, at the attacker's tile |
| Holy vision | 7 areas of sight 9; refresh countdown 150 (Enhanced) |
| Refresh interval | 100 cycles (Enhanced), 101 (Classic); first in the first start-up cycle |
| Submarine detection range | 6 tiles on each axis, tile positions |
| Minimap redraw | every 40 cycles |
| PUD unit flags used | bit 5 building, bit 6 submarine, bit 7 can see submarine, bits 0–4 "is a unit" |
| Submarine flag (default data) | 0x26 gnomish submarine, 0x27 giant turtle |
| "Can see submarine" flag (default data) | 0x16 Kurdan and Sky'ree, 0x23 Deathwing, 0x26, 0x27, 0x28 gnomish flying machine, 0x29 goblin zeppelin, 0x2A gryphon rider, 0x2B dragon, 0x2D eye of Kilrogg, 0x38 daemon, 0x40 and 0x41 scout towers, 0x60 and 0x61 guard towers, 0x62 and 0x63 cannon towers |
| Gold mine sight (default data) | 3 (0x5C, 3 × 3) |

## Test vectors

Coordinates are (x, y); y grows downwards. "Fresh map" means fog on and nothing revealed yet. F01 to F26 are the
vectors of the old text, word for word.

| ID | Input | Expected |
| --- | --- | --- |
| F01 | Human footman, sight 4, alone at (20, 20) on a fresh map, first cycle | 77 tiles become seen and explored. Tile (20, 20) is code 0. Tile (24, 20) is code 6 (left half clear). Tile (24, 24) is untouched (code 16). Tile (23, 23) is code 13 (only the bottom-right quarter fogged) |
| F02 | Sight 2 at (10, 10), fresh map | Tile (8, 8) untouched; (9, 9) code 10; (8, 9) code 1 (only the bottom-right quarter clear); (8, 10) code 4 (right half clear); (10, 8) code 2 (bottom half clear); 21 tiles touched |
| F03 | Sight 0 and sight 1 at (10, 10) | Identical: (10, 10) code 0, (9, 10) code 4, (11, 10) code 6, (9, 9) code 1, (11, 11) code 9; 9 tiles touched |
| F04 | Sight 9 at (50, 50) | 349 tiles touched; (59, 50) code 6; (59, 56) code 6; (59, 57) code 9 (only the top-left quarter clear); (58, 58) code 9; (59, 58) and (59, 59) untouched |
| F05 | A tile at code 4 (left half fogged) is revealed by an area that leaves it at code 2 (top half fogged) | Result code 10: only the top-left quarter stays fogged |
| F06 | Human scout walks out of an area in the normal cycle that ends with the cycle counter at 50, Enhanced, fog on | The area stays visible until the refresh in the normal cycle that ends with the counter at 99; from then it is fogged (visible map) but stays explored |
| F07 | Same, fog-of-war option off | The area stays visible for the rest of the game |
| F08 | Human town hall (4 × 4, complete) at (30, 30), sight 4 | Centre tile (32, 32) |
| F09 | Human guard tower under construction at (30, 30) (2 × 2) | Sight 3 at (31, 31), whatever its own sight range |
| F10 | Ranger of a player with ranger scouting, sight 9 | Centre its tile; 349 tiles |
| F11 | Computer player's grunt | Applies no sight; the fog maps and "seen" bits don't change |
| F12 | Sight 4 at (1, 1) | Only tiles with x ≥ 0 and y ≥ 0 are processed; the rest is ignored |
| F13 | Classic. Enemy submarine at (40, 40). Your destroyer (can see submarine) at (46, 34) | Detected by you (6 on each axis) |
| F14 | Same, destroyer at (47, 40) | Not detected (7 on x) |
| F15 | Enhanced. Detecting unit is a tower still under construction, 3 tiles away | Not detected |
| F16 | Classic. Invisible enemy mage | Detected by its owner only, so no other player's search can choose it |
| F17 | Enemy grunt (1 × 1) standing on a tile the human player p has not seen this refresh period | Hidden from p. Not hidden from any computer player |
| F18 | Enemy farm (2 × 2) with one of its 4 tiles seen by p | Not hidden from p |
| F19 | Human archer searching for a target; the only enemy in range is hidden from its owner | No target found |
| F20 | Human player places a farm whose site includes an unexplored tile | Refused |
| F21 | Human player places a farm on explored but unseen tiles where an enemy unit stands | The enemy unit doesn't block the placement |
| F22 | A computer player's footman given an attack command (command 8) by its AI on an enemy unit trained earlier in the same cycle (static study; audit round 70, V91) | The command's target check drops the target: idle becomes the footman's pending order. In the next cycle the same command works |
| F23 | `xhuman10-spells-700` (oracle trace), computer player 2's grunt 1475, axe thrower 1496 and ogres 1538 and 1543; player 1's units in range | n = 1: order 2, animation 2, wait 1, no target (everything is hidden in the first start-up cycle); n = 2: wait 4; n = 6: order 12, animation 4, wait 3, targets 1492, 1492, 1536 and 1542 |
| F24 | `human06-31357` (oracle trace): axe thrower 1516 (type 0x09, owner 5) at (68, 11) in order 12 on footman 1584 (owner 1, the human player) at (72, 7) strikes after a fog refresh; (68, 11) is otherwise unseen by player 1 | The strike reveals a sight-2 area around (68, 11) for player 1; at n = 9642 footman 1584's search takes 1516 as its attacker seed: order 12, animation 4, wait 3 |

F13 and F14 use a hypothetical destroyer with the "can see submarine" flag; in the default data destroyers don't
have it (F40).

### Round-65 vectors

| ID | Input | Expected |
| --- | --- | --- |
| F25 | `xorc4-30000` (oracle trace, Enhanced, no input; local player 5): axethrowers 1592 and 1594 (type 0x09, owner 5) at (91, 2) and (91, 4) and ogre 1583 (type 0x07, owner 5) at (89, 13) are dying (order 1, animation 1) through the fog refresh at n = 19501; archer 1474 (type 0x08, owner 2) attacks from (87, 8), and at n = 19506 its tile is (88, 7) while its drawn position is still in tile (87, 8) | The dying units' sight is applied at the refresh, so (87, 8) is seen by player 5. n = 19507: axethrower 1585 (type 0x09, owner 5), idle at (92, 3), takes order 16 on 1474 (animation 4, wait 3). Without the dying units' sight 1474 fails the fog test and 1585 stays idle |
| F26 | A human player's unit being removed (order 0, its slot not yet released) through a fog refresh (static study) | It applies its sight (only human players' units reveal, so a body or ruin that has turned neutral reveals nothing) |

### Round-87 vectors (static study unless marked)

| ID | Input | Expected |
| --- | --- | --- |
| F27 | Multiplayer game, fog-of-war option off in the lobby, Enhanced (a single-player custom game can't set fog off). An enemy grunt stands on a tile that no unit of the human player (or its shared-vision set) has ever revealed, after the first refresh | The bit is still set: the grunt is hidden from that player, no search takes it and it isn't drawn on the minimap (C1, traced through the hidden-from sets: F43) |
| F28 | Same game, a human footman walks next to that tile | Its step reveals the tile; from the next phase 5 the grunt isn't hidden, and it stays so for the rest of the game (fog off: nothing re-sets the bit) (C1; static study for the step) |
| F29 | A human peasant finishes a farm and is let out on a tile next to it, in a cycle that is not a refresh cycle (C2) | No area is revealed when it comes out. Its sight is next applied at its first movement step, or at the next fog refresh if it stands still |
| F30 | A human transport unloads a footman onto a shore tile, not a refresh cycle (C3) | No area is revealed for the footman as it lands; its first step or the next refresh reveals |
| F31 | Enhanced. An invisible human mage boards a human transport | While it is inside, its detected-by set is everyone and its hidden-from set is everyone (C4, traced for a footman: F44). The next phase 1 after it is unloaded, still invisible, gives it its owner's set again (static study) |
| F32 | Enhanced, fog on. A computer player's paladin casts holy vision; the strike is in the normal cycle that ends with the cycle counter at 120 (X1) | The refresh countdown becomes 150: the next fog refresh is in the normal cycle that ends with the counter at 270 (not 199), then 370, 470, … The human player's "seen" bits and visible map are re-fogged on that schedule |
| F33 | Enhanced. Holy vision whose strike falls in a refresh cycle (the cycle ending with the counter at 199) | The refresh of that cycle has already happened in phase 3; the next is in the cycle ending with the counter at 349 |
| F34 | Enhanced, multiplayer after diplomacy: player 3's submarine; player 2 owns a detector 4 tiles away; player 2's shared-vision set is {1, 2}; player 1's set is {0, 1}; player 3's set is {3} (X4) | Before widening: {2, 3}. Widening: p = 0 and p = 1 aren't in the set at their turn; p = 2 adds 1; p = 3 adds 3. Result {1, 2, 3}: player 0 doesn't detect it |
| F35 | A human peasant facing east (facing 2) enters a gold mine whose top-left tile is (40, 40) (X6) | A sight-3 area centred on (41, 40) is revealed at once |
| F36 | A human peon inside a gold mine at (40, 40) (3 × 3) through a fog refresh (X7) | It reveals with sight 3 centred on (41, 41). A human oil tanker inside an oil platform at the same refresh reveals nothing |
| F37 | A human archer of player 0 strikes a computer player 3's grunt (X5) | A sight-2 area around the archer is revealed for player 3 (its "seen" bits; no display change, as the local player isn't in player 3's set). Player 3 being a computer player, no search of its units depends on it |
| F38 | Reveal-map cheat on; a footman inside a transport (X3) | Still hidden from everyone; every other unit is hidden from no one |
| F39 | Classic. Invisible mage owned by player 15 (neutral) | Detected by nobody |
| F40 | Default unit data. Enemy gnomish submarine 4 tiles from your elven destroyer, nothing else of yours nearby (game data) | Not detected by you: destroyers lack the "can see submarine" flag. A gnomish flying machine at the same place detects it |
| F41 | Enhanced, local player 0 alone in its set, fog option switched off in the game menu. Tile A explored (code 0 in the explored map, code 16 in the visible map); tile B fully unexplored (X12) | Tile A: visible code 0, all "seen" bits clear. Tile B: visible code 16, "seen" bits = {0} (only player 0 still counts it unseen) |
| F42 | Enhanced. A human paladin's holy vision strikes in phase 4 of cycle K and uncovers an enemy grunt; a human archer in range of the grunt is updated later in the same cycle, then again in cycle K + 1 (phase timing) | In cycle K the archer's search still sees the grunt as hidden (the sets of cycle K − 1's phase 5) and finds no target; phase 5 of cycle K un-hides the grunt, and the archer's search in cycle K + 1 can take it |

### Round-90 vectors

| ID | Input | Expected |
| --- | --- | --- |
| F43 | `mp-dark-paths-bne-fogoff-game-vision-300` (oracle trace, Enhanced, Direct Connect, fog of war off in the lobby; traced player 1) and its control `mp-dark-paths-bne-game-vision-300` (fog on) | Every record of the two traces is the same apart from the fog option, the hidden-from sets included. Player 0's three units have hidden-from 250 (players 1 and 3 to 7) from the first start-up cycle to n = 300 in both: they stay hidden from player 1 with fog off |
| F44 | `human12-human12-invisible-passenger-game-vision-4550-5600` (oracle trace, Enhanced): mage 1438 casts invisibility on footman 1445 (owner 1), timer 1999, at n = 4628; the footman boards transport 1428 at n = 5242 | Detected-by 2 (only player 1) from n = 4629. At n = 5242 hidden-from becomes 255 (everyone); from n = 5243 detected-by is 255 (everyone), with the invisibility timer still running (1384). Neither change waits for a fog refresh |
| F45 | `human1-human01-fog-switch-fogmaps-game-400` (oracle trace, Enhanced single player; local player 1 alone in its set): `showpath` at n = 12 (every "seen" bit cleared, visible map all clear), the option switched on at n = 18 and off at n = 21 (X12; traced, round 91, `ERRATA-audit-vision.md` erratum 2) | At n = 18 no map changes. At n = 21 every fully unexplored tile's "seen" byte is 2 (exactly {1}: the local set is assigned, not kept-where-set), every explored or edge tile's is 0, and the visible map equals the explored map (184 clear, 768 fogged, 72 edge tiles). The same assignment at the second switch-off (n = 302, after refreshes had set every bit again) |
| F46 | A custom map whose slot 3 has owner code 9; a slot-3 unit next to a player-0 unit whose footprint tiles all have slot 3's "seen" bit set (static study, erratum 3) | The player-0 unit is hidden from slot 3 by the footprint test, as for a human player. Where a reveal of a set including slot 3 has cleared the bit, it isn't hidden from slot 3 |

### Round-91 vectors

| ID | Input | Expected |
| --- | --- | --- |
| F47 | Attract demo after about 172 s idle at the main menu (startup launch, no words; read-only state log `startup/startup-none-200-peekwatch.jsonl`, round 91). The demo runs with the program's Enhanced flag clear (its own setting) | Before the first start-up cycle: visible map all clear, every "seen" bit clear, fog countdown 1. After the first start-up cycle a refresh has run: visible map all fogged, every "seen" bit set. Refreshes recur (the next one about 100 cycles later, on the demo's Classic schedule). Fog option on and reveal-map cheat on throughout; no fog is drawn on screen |

### Round-104 vectors

| ID | Input | Expected |
| --- | --- | --- |
| SW5a | `sweep-xhuman5` (oracle sweep trace, Enhanced, kept outside the repository; reproducible with `oracle/sweep.py capture`), n = 27632: tanker 1542 (owner 7, in a slot above the patch's) starts oil platform 1594 on oil patch 1530 at (75, 107); the patch's hidden-from set was 99 | Patch 1530: order 1, death animation, wait 1, and hidden-from set **255** from n = 27632 on; order 0 (remove) from n = 27633, still 255 |

## Open questions

- **Q2 (round 87, C1):** closed in round 90: a multiplayer game set up with fog off starts with every "seen" bit set
  (traced, F43).
- **Q3 (round 87, C4; narrowed in round 90):** the passenger half is closed (an inside unit is detected by everyone,
  traced, F44). Still held: whether a **detector** inside something stops counting (static study: yes; default, the
  old text: it counts). No detector can be inside anything with the default data. Trace request VT5: a research-side
  copy of the unit data that lets a detector board a transport (for example a custom map's flying machine data
  changed to a land unit), with the `vision` extra, and an enemy submarine in range.
- **Q4 (B-040 vision audit, erratum 8):** closed in round 91: demo playback starts with the visible map and the
  "seen" bits clear, and the first start-up cycle's refresh fogs both, because the fog option is on in the demo
  (traced, F47).
- **Q1 (from round 87):** closed in round 99: a player **can** drop itself from its own shared-vision set through the
  diplomacy command, which writes the whole set (static study, `ERRATA-audit2-net.md` erratum 2). Tidewater mirrors
  it (`game/diplomacy.md`); whether the original's diplomacy screen can send such a set is open there.
- The marker that keeps a building drawn on the minimap after it was seen is the building's remembered look in
  `ui/map-drawing.md`, set whenever the building isn't hidden from the local player. Whether that also happens while
  the building is off the map view is open there.
- Sight values above 9 in custom unit data: the program has no area for them (the behaviour is undefined; assume
  maps never use them).
- Whether a passive player's units are ever hidden-from-checked in a way that matters (passive players don't
  attack).
