# Economy: building walls

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:**
  - static study of the 2.02 program (research notes under `research/notes/`);
  - the default unit data (`Rez\unitdata.dat`) and animation data (`Rez\script.bin`);
  - the tilesets' wall tables (for example `Art\bgs\Forest\nf_wall.bin`).

  Not yet confirmed against the running game.

## Behaviour

A wall is built like any other building: a worker places a 1 × 1 **wall piece** and builds it. When the piece is
finished, it stops being a unit and its tile becomes **wall terrain**. From then on, the wall is part of the map.
Damage, rubble and passability of that terrain are in `movement/occupancy.md`, "Walls".

There are two wall-piece types: the human wall (PUD type 0x67) and the orc wall (0x68). Each race's workers build
their own.

### When the button is offered

The build button sits in slot 7 of the basic build menu. It is shown only when:

- ALOW bit 31 is set;
- the player has ever had a hall;
- the game is a multiplayer game with more than one human player.

The full rule is in `ui/command-buttons.md` ("Build buttons", row "Wall"). The cost check is the usual one of
`economy/build-train.md`.

### Placing and building

- **Placement:** a wall piece is an ordinary **land building** of size 1 × 1. Its tile must pass the land-building
  placement mask (`movement/occupancy.md`, "Building placement masks"), so it can't go on coast sand, water, shore,
  forest, rock, an existing wall, or a tile with a unit or a building on it. The worker's own tile doesn't block it,
  as for any building. Computer players use their own masks, which also respect their keep-clear marks
  (`ai/worker-ai.md`).
- **Paying and construction:** these follow `economy/build-train.md` exactly ("Placing a building", "Construction").
  With the default data (Data below), M = 40 and T = 30. So a piece starts with 4 hit points, gains the other 36
  over 30 steps, and, if undamaged, completes on construction pass 12 × 31 = 372.
- **While under construction** the piece is a building like any other. It occupies its tile (the footprint mark of
  `movement/occupancy.md`), can be attacked and repaired, and counts as a building under construction.
- **Destroyed before completion:** it dies like any building. It has no death explosion, and when its death
  animation ends it leaves a 1 × 1 ruin (type 0x6A, `core/corpses.md`).

### Completion: the piece becomes terrain

When the construction pass completes a wall piece, the ordinary completion runs first (`economy/build-train.md`,
"Completion"). The builder comes out and the piece briefly counts as a completed building. Then, in the same pass:

1. **The piece is killed.** It goes through the ordinary kill of a building, but without the death explosion that
   other buildings make. Everything a building's kill does happens here too:
   - its owner's completed-building count drops again;
   - a computer-controlled owner's "building destroyed" rule runs, as listed in `ai/worker-ai.md` ("Other effects of
     a computer player's losses"). That rule clears a done mark of the build list, and can start the all-out attack
     if fewer than 3 buildings other than oil platforms remain.
2. **The piece gets the used mark** (`core/corpses.md`, "The used mark"). So when its death animation signals step
   done, it is removed and leaves no ruin. The wall pieces' death animation signals at once, so this happens on the
   piece's next update. Its slot is released 513 cycles after that, as for any removed unit.
3. **The tile becomes wall:**
   - **Graphic:** the tile's word is replaced by the first intact graphic of the owner's race style. That is internal
     tile index 16 (human style) if the owner's race is human, or 34 (orc style) if it is orc; the race comes from
     the map's `SIDE` section. Writing the whole word also clears the tile's wall damage counter to 0.
   - **Movement word:** it gains 0x91 (obstacle, no building here, land).
   - **Owner bit:** it gains 0x04 if the owner is human-controlled, or 0x08 if not (`movement/occupancy.md`).
4. **Joining:** five tiles pick the graphic that joins them to their neighbours, in this order:
   - the tile north of the new wall;
   - the tile west of it;
   - the new wall's own tile;
   - the tile east of it;
   - the tile south of it.

   See "Joining" below.

The tile's region value (`movement/occupancy.md`, "How passability changes") is **not** changed. A wall built during
play keeps the land-area value its tile had.

Until the piece's death step done, its tile has the building footprint mark and the wall bits together.
`movement/occupancy.md` describes the footprint marks: they are cleared when the piece is removed, and the wall bits
stay.

### Joining

The joining step only acts on a tile that is inside the map and carries a wall bit (0x04 or 0x08 in its movement
word). A tile without one is left alone. Walls in that sense are finished walls of any player and any style,
including walls placed by the map. Wall pieces still under construction, and rubble, don't count.

1. **Neighbour mask.** Build a 4-bit mask from the tile's orthogonal neighbours that are walls in that sense:

   | Neighbour | Bit |
   | --- | --- |
   | South (y + 1) | 1 |
   | East (x + 1) | 2 |
   | North (y − 1) | 4 |
   | West (x − 1) | 8 |

   A neighbour outside the map counts as no wall.
2. **Variant.** Turn the mask m into a variant v. If m ≥ 6, add 1. Then, if the result is 12 or more, add 1 again.
   So variants 6 and 12 are never chosen (table in Data).
3. **Style.** Keep the tile's style but not its damage:
   - if its current internal index is 16 to 33 (intact, human style) or 52 to 69 (damaged, human style), its new index
     is 16 + v;
   - if it is 34 to 51 (intact, orc style) or 70 to 87 (damaged, orc style), its new index is 34 + v.

   The whole tile word is written, so the wall damage counter goes back to 0.

So **joining repairs**: a damaged wall next to a newly built wall becomes an intact wall again, with its damage
counter at 0. Only the 4 orthogonal neighbours of the new wall are affected, and only their graphic and counter. Their
movement word, owner bit and region value stay as they were.

Damage and rubble (`movement/occupancy.md`) don't run the joining step. A wall stepping to damaged keeps its variant
(the wall table maps intact index i to damaged index i + 36). A neighbour of a new rubble tile isn't re-joined, so it
keeps showing a join toward the rubble.

## Data

### Wall pieces (default unit data, both races alike)

| Field | Value |
| --- | --- |
| Hit points (M) | 40 |
| Build time (T) | 30 |
| Cost | 20 gold, 10 lumber, 0 oil (stored 2, 1, 0; × 10) |
| Size | 1 × 1 |
| Armour | 0 |
| Sight | 1 |
| Annoy computer | 45 |
| Death animation | signals step done at its first instruction |

### Variant by neighbour mask

| Mask m | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 | 15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Variant v | 0 | 1 | 2 | 3 | 4 | 5 | 7 | 8 | 9 | 10 | 11 | 13 | 14 | 15 | 16 | 17 |

### Internal tile index ranges of walls

| Indices | Meaning |
| --- | --- |
| 16 to 33 | intact, human style (16 + v) |
| 34 to 51 | intact, orc style (34 + v) |
| 52 to 69 | damaged, human style |
| 70 to 87 | damaged, orc style |
| 88 onward | rubble (no wall bit) |

## Test vectors

Positions are tile coordinates (x, y) on a map large enough that no tile named is at an edge, unless stated.

| Input | Expected |
| --- | --- |
| A human player (human-controlled) completes a wall at (10, 10) on ground 0x0001, no walls nearby | Tile index 16 (v = 0), damage counter 0, movement word 0x0095 |
| A computer-controlled orc player completes a wall at (10, 10) on ground 0x0001, no walls nearby | Tile index 34, movement word 0x0099 |
| Human wall at (10, 10) (index 16); an orc player completes a wall at (11, 10) | (10, 10): mask 2 (east), v = 2, index 18. (11, 10): mask 8 (west), v = 9, index 43 |
| Walls at (10, 9), (9, 10), (11, 10) and (10, 11); a human completes (10, 10) | (10, 10): mask 15, v = 17, index 33. Each neighbour gains one bit, for example (10, 9) gains bit 1 (its south) |
| Mask 6 (east and north) | v = 7 |
| Mask 11 (south, east, west) | v = 13 |
| Damaged human wall at (10, 9), index 52 + 0 = 52, counter 12; a wall is completed at (10, 10) | (10, 9) gets mask 1, index 17: intact again, counter 0 |
| Wall completed at (0, 0) | Neighbours north and west are outside the map: they count as no wall and aren't joined |
| Wall piece placed at cycle C, undamaged | 4 HP at creation; completes on construction pass 372 (counting the creation cycle as pass 1); the tile becomes wall in that pass; the piece is removed on its next update, and its slot is released 513 cycles after that |
| Wall piece destroyed while under construction | A 1 × 1 ruin (type 0x6A) when its death animation ends; no death explosion |
| Computer player whose only buildings are a town hall and a farm completes a wall | When the piece is killed, 2 buildings other than oil platforms remain: the all-out attack starts (`ai/worker-ai.md`) |
| Draws | Building a wall makes the creation draws of a building (`core/timing-and-rng.md`). Completion, the kill and joining make none |

## Open questions

- **Region map.** A wall built during play leaves the tile's region value unchanged, so the region map still treats
  the tile as part of its land area. Whether path search uses the region value to decide that a goal is reachable,
  and so could try to reach an area that walls have cut off, belongs to `movement/pathfinding.md`. Default: the
  region value is left alone, exactly as described.
- **Score.** Whether the kill of a completed wall piece counts in the end-of-game statistics (buildings lost or
  built). Default: it counts as built once, and is not counted as lost.
- **Joining repairs.** The repair of a damaged neighbour follows directly from the joining rule, but it hasn't been
  seen in the running game. Default: it happens as described.
- **Computer players.** Whether the computer's build lists ever contain walls, and therefore whether the
  "building destroyed" rule above matters in ordinary play. Default: the rule runs whenever a computer player's wall
  piece completes.
