# Vision: fog of war, sight and detection

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), and the unit flags in the
  public PUD specification text (`~/repos/war2-data/pudspec.txt`). The sight shapes were checked by expanding the
  program's shape data into quarter tiles and comparing them with the rule below; they match for every sight value.
  Not yet confirmed against the running game.
- **Changes:** 2026-10-12: the open question on the minimap column pattern for maps smaller than 128 × 128 is
  closed by `ui/minimap.md`: the alternation is by minimap pixel column on every map size.
- **Changes:** 2026-10-12: the open question on whether the shared-vision set contains the player itself is
  closed by `game/setup.md`: game setup puts each player in its own set.
- **Changes:** 2026-10-12: the main-view fog drawing is now in `ui/map-drawing.md`, which closes that open question
  and answers when the minimap's "seen" marker is set. The hidden-from rule now names the right states: dead, removed
  or inside something (a dying unit isn't hidden from everyone), as in the research notes.
- **Changes:** 2026-10-12: the fog-off cheat is named: the `showpath` developer word (`game/cheats.md`).
- **Changes:** 2026-10-12, round-32 errata: the first fog refresh is in the first start-up cycle, and later ones are
  named by cycle counter value (99, 199, … in Enhanced).
- **Changes:** 2026-10-12, round-35 errata: a new unit is hidden from everyone until the next visibility pass, so no
  target search succeeds in the first start-up cycle.
- **Changes:** 2026-10-12, round-36 errata: added the `xhuman10-spells` vector for the first start-up cycle (no rule
  change).

## Behaviour

Vision has four parts:

1. **Two fog maps for the local player**: what has ever been seen (**explored**) and what is seen now (**visible**).
   They are kept at quarter-tile resolution and drive the display.
2. **A per-player "seen" map**: one bit per player per tile. Game rules use it, not the display maps.
3. **Detection**: per unit and per player, whether submarines and invisible units can be seen at all.
4. **Unit visibility**: per unit and per player, whether the unit is hidden by the fog. Computed from the "seen" map.

Phase numbers are those of `core/timing-and-rng.md`.

### Terms

- **Human, computer, passive, nobody:** the player kinds of `movement/movement.md`, "Player kinds".
- **Classic and Enhanced:** the two rule modes of `core/timing-and-rng.md`.
- **Shared-vision set of player p:** in Enhanced mode, the set of players that p shares its vision with. Game setup
  puts p in it (`game/setup.md`), so it includes p itself. In Classic mode only p counts.
- **Unit (as opposed to building):** a type with any of the flag bits 0–4 (land, air, explode, sea, critter).
- **Complete:** a building whose construction has finished. Units count as complete.

### The fog maps

Each map tile has, in both the explored map and the visible map, four **quarter flags**: top-left, top-right,
bottom-left and bottom-right. A set flag means that quarter is fogged.

- **Explored map.** At the start of a game every quarter of every tile is fogged. Revealing clears quarters, and
  nothing sets them again. A tile with all four quarters fogged is unexplored and is drawn black.
- **Visible map.** At the start of a game every quarter is fogged. Revealing clears quarters. On each **fog refresh**
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

### The "seen" map

One byte per tile, one bit per player (bit p for player p, players 0 to 7). A set bit means the tile is **not seen**
by that player during the current refresh period.

- At the start of a game all bits are set, unless fog is off from the start (the demo playback mode), which clears
  them all.
- On each fog refresh, if the fog-of-war option is on, all bits are set again.
- When an area is revealed for player p, every tile that the area touches (any quarter revealed) has the bits of p's
  shared-vision set cleared, whoever the local player is.

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

**Map edges.** The part of the area outside the map is ignored.

**Whose display changes.** The explored and visible maps change only when the local player is in the revealing
player's shared-vision set (in Classic: when the revealing player is the local player). The "seen" map changes for
every revealing player.

### Who reveals, and when

Only **human** players' units reveal (sight is never applied for computer, passive or nobody players).

**Sight of a unit.** Its centre tile is its tile position plus half its size in tiles, rounded down, on each axis.
For a 1 × 1 unit that is its own tile; for a 2 × 2 building at (x, y), (x + 1, y + 1); for a 3 × 3 building also
(x + 1, y + 1); for a 4 × 4 building (x + 2, y + 2). Its sight value is:

| Unit | Sight value |
| --- | --- |
| A building that isn't complete | 3 |
| A ranger, or a berserker, whose player has researched the matching scouting upgrade | 9 |
| Anything else | the type's sight range from the unit data (0 to 9) |

**When sight is applied:**

1. **Fog refresh cycles.** In phase 4, each unit that isn't dead or removed applies its sight, when it is updated and
   before its order runs. A unit inside a building or transport doesn't, with one exception: a worker inside a
   **gold mine** reveals with the gold mine's sight, centred on the gold mine's centre tile.
2. **Each movement step.** When a human player's unit moves onto a new tile, its sight is applied at once from the
   new tile (`movement/movement.md`).
3. **A worker entering a building.** A worker that goes inside a mine, depot or construction site reveals a sight-3
   area. Its centre is the building's top-left tile moved one step in the direction the worker faces.

**The fog refresh** happens in the first start-up cycle and then every 100 cycles in Enhanced mode, every 101 in
Classic: in the normal cycles that end with the cycle counter at 99, 199, … (Classic: 100, 201, …;
`core/timing-and-rng.md`). In phase 3 of a refresh cycle the previous visible map is kept for redrawing. Then, if the
fog-of-war option is on, the visible map is fully fogged and every "seen" bit is set. Phase 4 then re-applies all
sight. So a tile that no unit sees any more stays visible until the next refresh: **up to 100 cycles late**
(101 in Classic). Phase 7 only redraws the screen where the visible map changed.

### Detection (phase 1)

Each cycle, each unit gets a **detected-by** set of players. Rules, first match wins:

| Unit | Detected by |
| --- | --- |
| Dead, dying or removed | everyone |
| Invisible (the Invisibility spell's timer is running) | Classic: its owner only. Enhanced: its owner's shared-vision set; nobody if the owner is not one of players 0–7 |
| Its type has the submarine flag (bit 6) | see below |
| Anything else | everyone |

**Submarines.** For each player p (0 to 7), p detects the submarine when any of these holds:

- p is its owner;
- Enhanced only: p is in the owner's shared-vision set;
- p owns a unit that is alive, whose type has the "can see submarine" flag (bit 7), and whose tile is within 6
  tiles of the submarine's tile on both axes (tile positions, so the top-left tile of each). Enhanced only: that
  unit must also be complete.

In Enhanced mode the set is then widened: every player in the shared-vision set of a detecting player also detects.

### Unit visibility (phase 5)

Each cycle, each unit gets a **hidden-from** set of players. A unit that is dead, removed or inside something is
hidden from everyone. Otherwise, for each player p:

| Player p | The unit is hidden from p when |
| --- | --- |
| Computer | never |
| Nobody (empty slot) | always |
| Human or passive | no tile of the unit's footprint is seen by p (its "seen" bit is set on every one of them) |

The footprint is the unit's type size in tiles, placed at the tile containing its top-left **pixel** (so a moving unit
counts from where it is drawn, not from the tile it has claimed). Parts outside the map are ignored. With the
reveal-map cheat on, nothing is hidden from anyone.

A new unit starts hidden from everyone (`core/unit-slots.md`, "Creating a unit") until the next visibility pass.
The units update before the first visibility pass of the first start-up cycle, so no automatic target search can
succeed in that cycle; the first that can are in the second start-up cycle. A unit created in play is likewise not a
candidate for searches made earlier in its cycle than the visibility pass (static study only).

Because computer players never apply sight, their "seen" bits are always set, but that doesn't matter: computer
players treat every unit as not hidden. Passive players never apply sight, so with the fog-of-war option on every unit
is hidden from them after the first refresh.

### What uses vision

| Rule | Uses |
| --- | --- |
| Attack target search (`orders/orders.md`, the acquisition search) | The candidate must not be hidden from the searcher's owner and must be detected by it |
| An order aimed at a unit loses its target (`orders/orders.md`) | Checked on each movement step: the target is lost if it is no longer detected by the owner. The hidden-from set is not checked |
| Placing a building (human owner) | The local player can't place on an unexplored tile (any tile of the site fully fogged in the explored map) unless the reveal-map cheat is on. Units standing on tiles the owner doesn't see don't block the placement |
| Clicking a building on the map | Possible when its tile is explored and either the building isn't hidden from the local player or it is a building (buildings stay selectable in fog once explored) |
| Minimap units | A unit (not a building) is drawn when it isn't hidden from the local player and is detected by it. A building is drawn when it has been seen (see the open questions) and is detected |

### The minimap

The minimap is redrawn every 40 cycles. It is cosmetic.

- **Terrain:** a minimap pixel shows the terrain colour when its tile is fully clear (code 0) in the map it samples,
  and black otherwise. On a 128 × 128 map, even pixel columns sample the explored map and odd columns sample the
  visible map. So explored but fogged ground shows as every other column black. On smaller maps the alternation is
  still by minimap pixel column, at the pixel's tile (`ui/minimap.md`, "Fog and the terrain option").
- With the reveal-map cheat on, all terrain is shown.

### Fog-of-war option and cheat

- **Option off:** the visible map is not re-fogged and the "seen" bits are not set again on refresh, so everything
  seen once stays visible. When the option is switched off during play, the visible map is set equal to the explored
  map. The "seen" bits are then cleared for every player, except the local player's (or its shared-vision set's)
  bits on unexplored tiles.
- **A cheat** turns fog off in single-player games: the `showpath` developer word, when it turns its flag on during
  a game (`game/cheats.md`, "Word codes"). It clears the option, makes the visible map fully clear and clears all
  "seen" bits.
- **The reveal-map cheat** makes nothing hidden and draws the whole minimap. It doesn't change the maps.

## Data

| Item | Value |
| --- | --- |
| Fog map size | 128 × 128 tiles (the largest map), one code per tile per map |
| Codes per tile | 17 (table above) |
| "Seen" map | 1 byte per tile, bit p = player p not seen |
| Sight values | 0 to 9 (0 and 1 give the same area) |
| Sight of an incomplete building | 3 |
| Scouting sight (ranger, berserker) | 9 |
| Refresh interval | 100 cycles (Enhanced), 101 (Classic); first in the first start-up cycle |
| Submarine detection range | 6 tiles on each axis, tile positions |
| Minimap redraw | every 40 cycles |
| PUD unit flags used | bit 5 building, bit 6 submarine, bit 7 can see submarine, bits 0–4 "is a unit" |

## Test vectors

Coordinates are (x, y); y grows downwards. "Fresh map" means fog on and nothing revealed yet.

| Input | Expected |
| --- | --- |
| Human footman, sight 4, alone at (20, 20) on a fresh map, first cycle | 77 tiles become seen and explored. Tile (20, 20) is code 0. Tile (24, 20) is code 6 (left half clear). Tile (24, 24) is untouched (code 16). Tile (23, 23) is code 13 (only the bottom-right quarter fogged) |
| Sight 2 at (10, 10), fresh map | Tile (8, 8) untouched; (9, 9) code 10; (8, 9) code 1 (only the bottom-right quarter clear); (8, 10) code 4 (right half clear); (10, 8) code 2 (bottom half clear); 21 tiles touched |
| Sight 0 and sight 1 at (10, 10) | Identical: (10, 10) code 0, (9, 10) code 4, (11, 10) code 6, (9, 9) code 1, (11, 11) code 9; 9 tiles touched |
| Sight 9 at (50, 50) | 349 tiles touched; (59, 50) code 6; (59, 56) code 6; (59, 57) code 9 (only the top-left quarter clear); (58, 58) code 9; (59, 58) and (59, 59) untouched |
| A tile at code 4 (left half fogged) is revealed by an area that leaves it at code 2 (top half fogged) | Result code 10: only the top-left quarter stays fogged |
| Human scout walks out of an area in the normal cycle that ends with the cycle counter at 50, Enhanced, fog on | The area stays visible until the refresh in the normal cycle that ends with the counter at 99; from then it is fogged (visible map) but stays explored |
| Same, fog-of-war option off | The area stays visible for the rest of the game |
| Human town hall (4 × 4, complete) at (30, 30), sight 4 | Centre tile (32, 32) |
| Human guard tower under construction at (30, 30) (2 × 2) | Sight 3 at (31, 31), whatever its own sight range |
| Ranger of a player with ranger scouting, sight 9 | Centre its tile; 349 tiles |
| Computer player's grunt | Applies no sight; the fog maps and "seen" bits don't change |
| Sight 4 at (1, 1) | Only tiles with x ≥ 0 and y ≥ 0 are processed; the rest is ignored |
| Classic. Enemy submarine at (40, 40). Your destroyer (can see submarine) at (46, 34) | Detected by you (6 on each axis) |
| Same, destroyer at (47, 40) | Not detected (7 on x) |
| Enhanced. Detecting unit is a tower still under construction, 3 tiles away | Not detected |
| Classic. Invisible enemy mage | Detected by its owner only, so no other player's search can choose it |
| Enemy grunt (1 × 1) standing on a tile the human player p has not seen this refresh period | Hidden from p. Not hidden from any computer player |
| Enemy farm (2 × 2) with one of its 4 tiles seen by p | Not hidden from p |
| Human archer searching for a target; the only enemy in range is hidden from its owner | No target found |
| Human player places a farm whose site includes an unexplored tile | Refused |
| Human player places a farm on explored but unseen tiles where an enemy unit stands | The enemy unit doesn't block the placement |
| `xhuman10-spells-700` (oracle trace), computer player 2's grunt 1475, axe thrower 1496 and ogres 1538 and 1543; player 1's units in range | n = 1: order 2, animation 2, wait 1, no target (everything is hidden in the first start-up cycle); n = 2: wait 4; n = 6: order 12, animation 4, wait 3, targets 1492, 1492, 1536 and 1542 |

## Open questions

- How the shared-vision sets are set during play (alliance with "shared vision" in the diplomacy screen is assumed).
- Which unit types have the "can see submarine" flag in the default unit data.
- Sight values above 9 in custom unit data: the program has no area for them (the behaviour is undefined; assume
  maps never use them).
- The marker that keeps a building drawn on the minimap after it was seen is the building's remembered look in
  `ui/map-drawing.md`, set whenever the building isn't hidden from the local player. Whether that also happens while
  the building is off the map view is open there. It is reset for buildings whose whole footprint is unexplored when
  the reveal-map cheat is switched.
- The direction step used for a worker entering a building (rule 3 under "When sight is applied") was read from a
  direction table and not checked against the game.
- Whether a passive player's units are ever hidden-from-checked in a way that matters (passive players don't
  attack).
