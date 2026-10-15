# Movement: occupancy and passability

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program, plus the tileset data files that ship with the game (the per-tileset
  conversion table and the wall, forest and rock tables named below). The test vectors that use tileset data use the
  Forest tileset. Not yet confirmed against the running game.
- **Changes:** 2026-10-11, round-3 errata: the search-mask note now says which player kinds drop bit 0x04, and when.
  2026-10-11, round-3 errata: "Units not on the map" now says what entering a transport, mine, depot, platform or
  construction site, and coming out, do to the marks and pointers (open question closed). The forest and rock table
  bases are read from each tileset's table headers instead of fixed row counts (41 forest rows is Forest only).
  2026-10-11, round-7 errata: the run-time marks table lists 0x0400, the computer players' keep-clear mark.
  2026-10-12, round-13 errata: "Rock" step 4 now points to the blast's damage rules.
  2026-10-12, round-38 errata: the keep-clear mark's description includes the creation marks of `ai/worker-ai.md`.
  2026-10-15, harvest audit round 70 (`ERRATA-audit-harvest.md`, erratum 7; static study, reviewed by a spec-review
  session): leaving the chopping state other than by felling also forgets the return-to source.

## Behaviour

Every map tile has one 16-bit **movement word**. Its low byte describes the terrain. Its high byte holds marks that
the game sets while it runs. A unit may enter a tile only if the tile's movement word shares no bit with the
unit's **blocking mask** (below). `pathfinding.md` and `movement.md` describe how the search and the step use this
test.

### Terrain bits (low byte)

The movement word is loaded as-is from the map's `SQM ` section. The map editor fills that section from a flags word
that each tileset keeps for each tile group, so the same terrain carries the same bits in every tileset.

| Bit | Meaning | Set on |
| --- | --- | --- |
| 0x01 | Land | Ground, coast (sand), forest, mountains, walls |
| 0x02 | Shore water | The boundary tiles between water and coast. They block land units and ordinary ships alike |
| 0x04 | Wall owned by a human-controlled player | Walls built by a human-controlled player. In the map editor's data, human-style walls also carry it |
| 0x08 | Wall owned by a computer-controlled player | Walls built by a computer-controlled player. In the map editor's data, every wall carries it |
| 0x10 | No building here | Coast (sand) tiles and built walls. Land units may walk on these tiles, but land buildings can't be placed on them |
| 0x40 | Water | Light and dark water and the boundaries between them |
| 0x80 | Obstacle | Forest, mountains (rock), walls, and shore boundaries that include rock or trees |

Flag words per tileset tile group (the same in all four tilesets):

| Tile group | Flags |
| --- | --- |
| Light water, dark water, and boundaries between them | 0x40 |
| Water/coast boundaries | 0x02, or 0x82 where the boundary includes an obstacle |
| Light coast, dark coast, coast boundaries, coast/ground boundaries | 0x11 |
| Light ground, dark ground, and boundaries between them | 0x01 |
| Forest, forest/ground boundaries, mountains, mountain/coast boundaries | 0x81 |
| Human-style wall tiles | 0x8D |
| Orc-style wall tiles | 0x89 |

Values that the map data may hold beyond these:

| Movement word | Effect |
| --- | --- |
| 0x0000 ("bridge") | Passable for land units, ships and air units |
| High byte 0x0F ("space") | The high byte overlaps the run-time marks below, so every layer is blocked |
| High byte 0x02 ("cave") | Overlaps the air-unit mark, so air units are blocked and land units and ships are not |

Old maps that still contain an `OILM` section have bit 0x04 converted to 0x08 when they load.

### Run-time marks (high byte)

| Bit | Meaning |
| --- | --- |
| 0x0100 | A ground-layer unit (land unit or ship) stands here. The game also keeps a pointer to that unit for the tile |
| 0x0200 | An air unit is here. The game keeps a separate air-unit pointer for the tile |
| 0x0400 | Keep-clear mark set by computer players on planned building sites, around their buildings as they are created (map buildings included) and on mining paths. Only their own site search tests it (`ai/worker-ai.md`); it is in no movement mask and no ordinary placement mask |
| 0x0800 | Part of a building's footprint. The game sets the ground-unit pointer of each footprint tile to the building |
| 0x2000, 0x8000 | Temporary marks made by the path search. The search clears them again |

- **Units occupy one tile:** the tile at their position, whatever their size. That includes ships and air units,
  which stand on even coordinates (see `movement.md`).
- **Building footprints:** width and height in tiles come from the unit data.
- **Oil patch:** marks nothing, so it doesn't block movement.
- **Circle of power:** sets bit 0x10 (no building here) on its footprint instead of 0x0800, and keeps no unit
  pointers, so units walk over it.

When a building is removed, the game clears 0x0800 on its footprint and **zeroes the ground-unit pointer on every
footprint tile**. A circle of power removed while units stand on it therefore leaves those units' 0x0100 marks
without a pointer. Its 0x10 marks stay.

When a unit leaves a tile, the game clears the tile's 0x0100 (or 0x0200) mark. It zeroes the pointer only if the
pointer names that unit. If two units ever share a tile, the first one to leave clears the mark for both.

### Movement layers and blocking masks

A unit's layer comes from the unit data's "unit type" field. One extra layer exists for a transport that is moving to
land at a shore.

| Layer | Who uses it | Blocking mask | Blocked by |
| --- | --- | --- | --- |
| Land | Land units | 0x09CE | buildings, ground-layer units, obstacles, water, either wall-owner bit, shore water |
| Air | Flying units | 0x0200 | other air units only |
| Sea | Ships | 0x0903 | buildings, ground-layer units, land, shore water |
| Landing | A transport moving to a shore | 0x0901 | buildings, ground-layer units, land |

- **Shared ground layer:** land units and ships share the ground-unit mark, so they block each other on tiles both
  can use, such as bridges.
- **Air units ignore terrain.** Only other air units and the "cave" and "space" values block them.
- **Units of players that aren't human-controlled** (computer, passive computer and rescue players, and neutral or
  empty slots) drop bit 0x04 from the mask the path search uses, but only while the unit is not invisible and has no
  attacker (see `pathfinding.md`, "Edge cases at the start"). Every tile that carries bit 0x04 also carries 0x80, so
  this does not open a path. The rule that makes a blocked unit attack a wall (`movement.md`) applies to computer
  players only.
- **Units the search may treat as free:** the rules are in `pathfinding.md` ("Units seen by the search").

### Building placement masks

A building may be placed only where no footprint tile shares a bit with its placement mask.

| Building kind | Mask | Extra condition |
| --- | --- | --- |
| Land building | 0x09DE | none (0x10 is in the mask, so coast sand and walls refuse buildings) |
| Shore building (shipyard, foundry, refinery) | 0x0911 | at least one footprint tile must carry 0x02 (shore water); no footprint tile may be land (0x01) or no-build (0x10), so in practice the footprint lies on water and shore water |
| Oil platform | 0x0913 | must be placed on an oil patch |
| Gold mine | own check | — |

### Units not on the map

- **Entering a transport, a gold mine, a depot or an oil platform:** the unit leaves its tile as in "When a unit
  leaves a tile" above: the 0x0100 mark is cleared, and the tile's ground-unit pointer is zeroed if it names the unit.
  The unit's tile and pixel position then become the container's (for a building, its top-left tile). It is **not**
  entered there: while inside it sets no mark and no pointer anywhere, and blocks nothing.
- **Entering a construction site:** just before the building is placed, the 0x0100 mark on the builder's tile is
  cleared (the pointer is left alone), so the builder doesn't block its own site. The building then marks its
  footprint (0x0800 and the pointer, as usual), and the builder goes inside as above, at the building's top-left
  tile. If the building can't be placed, the builder's 0x0100 mark is put back, unless that tile is a footprint tile.
- **Coming out** (unloaded from a transport, leaving a mine, platform or depot, or leaving a finished or destroyed
  building): the unit's tile becomes the tile found by the ring search of `movement/placement.md`, its pixel
  position becomes that tile × 32, and it is entered there: the 0x0100 mark is set and the tile's ground-unit pointer
  names the unit.

Boarding and unloading are specified in `orders/transports.md`.

## How passability changes

Three kinds of tile can be removed during play: **walls**, **forest** and **rock**. A second per-tile map, the
**region map**, records for each tile which connected area it belongs to. The region map is loaded from the map's
`REGM` section.

| Region value | Meaning |
| --- | --- |
| below 0x4000 | water area |
| 0x4000 bit set, below 0xFFFA | land area |
| 0xFFFA | land not yet assigned to an area |
| 0xFFFB | wall |
| 0xFFFC | forest tile reserved by a woodcutter (run time only) |
| 0xFFFD | rock |
| 0xFFFE | forest |

### Walls

**Building a wall:** the tile becomes intact wall graphics (orc or human style by the owner's race). The movement
word gains 0x91 (obstacle, no building here, land). It also gains 0x04 if the owner is human-controlled, or 0x08 if
computer-controlled. The tile's four orthogonal neighbours, and the tile itself, pick the wall graphic that joins up
with adjacent walls.

**Damaging a wall:**

- Each wall tile has a damage counter, from 0 to 19.
- A hit with damage d adds `max(1, floor((d mod 256) / 4))`. Damage reaches the wall as a single byte, so values of
  256 or more wrap.
- If the new counter would exceed 19, the wall **advances one stage** and the counter goes back to 0. The excess is
  lost. Otherwise the counter keeps the new value.
- There are three stages: **intact**, then **damaged**, then **rubble**. An intact wall therefore needs two
  overflows to become rubble.

**Rubble:**

- When a tile becomes rubble, its movement word loses bits 0x80, 0x10, 0x08 and 0x04. What remains (normally 0x01)
  is open land that anyone may walk on and build on.
- Every orthogonal neighbour that is still an **intact** wall advances one stage, to damaged. This doesn't cascade
  further, because a damaged wall doesn't pass the effect on.
- The rubble tile joins a land area. The game scans the 3 × 3 block around it, row by row from the top-left, and
  takes the first land-area value it finds; if there is none, it uses 0x4000. Then the "area merge" step below runs.

### Forest (tree chopping)

**Reservation:** when a woodcutter commits to a tree tile, that tile's region value becomes "reserved forest". The
value goes back to "forest" when the woodcutter dies, goes idle, or takes another order while still holding the
reservation. A free tree is a tile whose value is "forest" (not "reserved"). Leaving the chopping state this way (any
way but felling) also makes the woodcutter forget its return-to source (`../economy/harvest.md`, "Gather mode and
carried goods"); no reachable case is known where that shows (static study; audit round 70, erratum 7).

**Felling:** when the woodcutter finishes its strikes (the count is in `../economy/harvest.md`), the game updates the
3 × 3 block centred on the felled tile. It visits the block row by row from the top-left, clipped at the map edge.
Each visited tile:

1. is skipped unless its region value is "forest" or "reserved forest";
2. looks up its shape row in the tileset's **forest table**, and is skipped if that row is the "cleared" row;
3. otherwise takes the shape given by the table for its position k in the block, where
   `k = 1 + 3 × (dy + 1) + (dx + 1)` and (dx, dy) is its offset from the felled tile (k = 5 is the felled tile
   itself);
4. if the new shape is the cleared shape, also loses movement bit 0x80, takes the woodcutter's own tile's region
   value, and the "area merge" step runs for it.

The table's column for k = 5 is always the cleared shape, so the felled tile always opens. A neighbour opens too when
its table entry says so. For example, a thin edge of forest whose remaining trees can't stand alone is cleared with
it.

### Rock

Rock is removed only by a **demolition blast** (dwarven demolition squad or goblin sapper exploding). The blast at
tile (x, y) runs these steps in order:

1. **Rock:** for each of these 12 positions, in this order, if the tile's region value is "rock", run the 3 × 3
   update centred on that tile, using the tileset's **rock table**:
   (x+1, y+1), (x+1, y−1), (x−1, y+1), (x−1, y−1), (x, y+1), (x, y−1), (x+1, y), (x−1, y), (x+2, y), (x−2, y),
   (x, y+2), (x, y−2).
   The rules are the same as for forest, except that only "rock" tiles are visited. A cleared rock tile takes the
   region value of the demolition unit's own tile.
2. **Forest:** the same 12 positions, in the same order. Each tile that is "forest" or "reserved forest" gets the
   forest 3 × 3 update centred on it, exactly as if it had been felled.
3. **Walls:** each of the 8 neighbours of (x, y) that is a wall advances one stage, then, if it is still a wall,
   one more. Every wall next to the blast becomes rubble.
4. **Units:** the ground occupants of the 8 neighbouring tiles take the blast damage, and then the demolition unit
   dies (`combat/attack-ground-demolish.md`, "The blast").

Each step sees the changes made by the steps and positions before it. A position whose tile an earlier position
already cleared is skipped.

### Area merge

After a tile is cleared and given a land-area value A:

- each tile in the 3 × 3 block around it that is "land not yet assigned" (0xFFFA) gives A to every unassigned tile
  in its own 3 × 3 block;
- each tile in the 3 × 3 block that holds a different land-area value B relabels **every** tile on the map that has
  value B to A.

Two areas joined by a felled tree or a broken wall therefore become one area.

## Data

### Tile index conversion

The map's tile graphics section (`MTXM`) holds tileset tile numbers. At load, each number t becomes an internal tile
index via the tileset's conversion table: 16-bit entry number `(t >> 4) × 21 + (t AND 0x0F)`. Each table row has
21 entries: 16 tile variants, then 4 corner-type words, then the tile group's flags word (the flags table above).
The wall damage counter lives in the top 5 bits of the internal tile word.

### Wall, forest and rock tables

Each tileset ships three tables (wall, forest and rock). Each starts with two 16-bit words: the number of columns and
the number of rows. The 16-bit entries follow.

- **Wall table:** 2 columns, 87 rows. Wall internal indices start at 16. A wall tile with internal index i uses row
  `i − 16 + 1`, and stepping one stage gives index `16 + (column-1 entry) − 1`.
  - Intact walls are internal indices 16 to 51.
  - Damaged walls are 52 to 87.
  - Rubble starts at 88.
- **Forest table:** 10 columns; the row count differs per tileset. Forest indices start at
  `F = 16 + (wall table rows) − 1`.
- **Rock table:** 10 columns, 39 rows in every shipped tileset. Rock indices start at
  `R = F + (forest table rows) − 1`.
- Read every row count from the table's own header (its second 16-bit word); don't use fixed numbers. In the shipped
  tilesets:

  | Tileset | Wall rows | Forest rows | Rock rows | F | R |
  | --- | --- | --- | --- | --- | --- |
  | Forest | 87 | 41 | 39 | 102 | 142 |
  | Swamp | 87 | 38 | 39 | 102 | 139 |
  | Winter | 87 | 36 | 39 | 102 | 137 |
  | Wasteland | 87 | 36 | 39 | 102 | 137 |

- An internal tile index is the tileset's megatile number (the tile graphics in the tileset's `.cv4` data). On a
  Forest map, forest tiles draw megatiles 102 to 141, rock tiles 142 to 179, and intact walls 16 to 51.
- **Forest and rock entries:**
  - A tile with internal index i uses row `i − base + 1`.
  - Column 0 is the row itself. Columns 1 to 9 give the new shape for block position k.
  - An entry with any of bits 0xF000 set means "cleared". Its low 12 bits still give the shape to draw (the stump
    or cleared ground).
  - The new internal index is `base + (entry AND 0x0FFF) − 1`.
  - Row 25 (all columns 0x1019) is the cleared row.

## Test vectors

Bits use the tables above.

| Case | Expected |
| --- | --- |
| Land unit, tile 0x0001 / 0x0011 / 0x0000 | may enter |
| Land unit, tile 0x0040 / 0x0081 / 0x0002 / 0x0082 | blocked (water / obstacle / shore / shore) |
| Land unit, tile 0x0201 (air unit overhead) | may enter |
| Land unit, tile 0x0101 or 0x0801 | blocked (unit / building) |
| Ship, tile 0x0040 / 0x0000 | may enter |
| Ship, tile 0x0001 / 0x0011 / 0x0002 | blocked |
| Air unit, tile 0x0081 / 0x0101 / 0x0801 | may enter |
| Air unit, tile 0x0201 / 0x0200 ("cave") / 0x0F00 ("space") | blocked |
| Land unit or ship, tile 0x0200 ("cave") | may enter |
| Human-controlled player builds a wall on ground 0x0001 | movement word 0x0095 |
| Computer-controlled player builds a wall on ground 0x0001 | movement word 0x0099 |
| Intact wall hit 19 times for 4 damage each (+1 each) | counter 19, still intact |
| ... then a 20th hit for 4 damage | becomes damaged, counter 0 |
| Intact wall hit once for 110 damage (+27) | becomes damaged, counter 0 (excess lost); a second such hit makes rubble |
| Wall hit for 300 damage | counts as 44 damage: +11 |
| Wall 0x0095, 0x0099 or map-placed 0x008D becomes rubble | movement word 0x0001 |
| Intact wall at (4,4), (5,4) and (6,4); (5,4) becomes rubble | (4,4) and (6,4) become damaged, counters 0 |
| Forest tileset; solid forest (row 24 everywhere); fell (5,5) | (5,5) cleared (row 25, bit 0x80 removed); (4,4) → row 10, (5,4) → 23, (6,4) → 11, (4,5) → 8, (6,5) → 2, (4,6) → 7, (5,6) → 5, (6,6) → 4; all neighbours keep bit 0x80 |
| Then fell (5,4) | (5,4) cleared; (4,3) → 10, (5,3) → 23, (6,3) → 11, (4,4) → 8, (6,4) → 2, (4,5) → 8, (6,5) → 2; (5,5) skipped (already cleared) |
| Forest tileset; tile of row 20 directly above a felled tile (k = 2) | entry 0x1019: it is cleared too and loses bit 0x80 |
| Woodcutter holding a reservation is killed | the tile goes from "reserved forest" back to "forest" |
| Felled tile joins two land areas A (woodcutter's) and B | every B tile on the map becomes A |

## Open questions

- How a woodcutter picks its tree (search shape, distance and tie-breaks); see `../economy/harvest.md`.
- The gold mine's own placement check (distance from town halls and so on).
- Whether any map other than old beta maps has bit 0x04 on a tile without 0x80.
