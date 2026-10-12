# UI: the minimap

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 19 (minimap)"), the
  tileset files `Art\bgs\*\*.vx4` and `*.vr4` (how a tile's colour is picked) and the minimap frame pictures
  `Art\h_lft1.bmp` / `Art\o_lft1.bmp`. Not yet confirmed against the running game.

## Behaviour

The minimap is the 128 × 128-pixel picture of the whole map at screen x 24 to 151, y 26 to 153. This spec covers
how it is drawn (scale, terrain, fog, units, the view rectangle), when it is redrawn, and how clicking and dragging
on it moves the view. Which button does what on the minimap in each mode is in `ui/selecting-and-commanding.md`
("Mouse regions"). The fog maps it samples are in `vision/fog.md`.

### Scale

The scale depends only on the map's **width**; the same scale is used down the map. Minimap pixels are counted from
the minimap's top-left corner (0 to 127 each way).

| Map width | Pixels per tile | Tile to pixel | Pixel to tile |
| --- | --- | --- | --- |
| 32 | 4 | p = 4t | t = floor(p / 4) |
| 64 | 2 | p = 2t | t = floor(p / 2) |
| 96 | 4/3 | p = floor(4t / 3) | t = floor(3p / 4) |
| 128 (and any other width) | 1 | p = t | t = p |

A map that isn't square still uses the width's scale for both axes. Rows past the map's height aren't defined here
(see the open questions).

### When it is redrawn

The minimap is redrawn as part of the game cycle, so it doesn't change while the game is paused.

- When the game screen opens, the countdown is set to 1, so the minimap is drawn in the first game cycle; the
  countdown then restarts at 40.
- Each game cycle, after the units are updated, the countdown goes down by 1. At 0 it is reset to 40 and the
  minimap is redrawn. So a redraw happens every 40 cycles.
- If the countdown didn't reach 0, the view has moved since the last redraw, and more than 4 cycles are left, the
  countdown is cut to 4. The view rectangle therefore catches up within 4 cycles of a scroll.

### Terrain

The terrain picture is rebuilt on every sixth redraw (every 240 cycles), and also on any redraw where the reveal-map
cheat (`vision/fog.md`) has been switched on or off since the last rebuild. Between rebuilds, changes to the map, such
as trees cut down or walls built, don't show.

Each minimap pixel takes one fixed pixel of one **minitile** (the 8 × 8 picture pieces that each 32 × 32 tile is
built from, 4 × 4 of them per tile):

- **Which tile and minitile:** each pixel is assigned a tile, plus a minitile column (0 to 3) within that tile, by this
  walk along the row. Start with a counter of 128, tile 0 and column 0. After each pixel, subtract the map width from
  the counter. If the counter is now 1 or more, move to the next minitile column of the same tile. Otherwise add 128
  (repeating until it is 1 or more), move to the next tile and go back to column 0. The minitile row within the tile
  follows the same walk down the rows.

  | Map width | Pixels of one tile use minitile columns |
  | --- | --- |
  | 128 | 0 |
  | 64 | 0, 1 |
  | 32 | 0, 1, 2, 3 |
  | 96 | repeating every 3 tiles: the first tile gets 2 pixels (columns 0, 1), the next two 1 pixel each (column 0) |

- **Which pixel:** the minitile's pixel at column 7, row 6 (counting from 0), taken from the minitile's picture
  without any mirroring, whatever mirroring the tile itself uses.

### Fog and the terrain option

Then the minimap pixels are filled:

- **Terrain off.** When the minimap terrain preference is off, every pixel is palette colour 0 (the units and the
  view rectangle are still drawn).
- **Reveal-map cheat on.** Every pixel shows its terrain colour.
- **Otherwise** a pixel shows its terrain colour only when its tile is fully clear in the fog map it samples, and
  palette colour 0 otherwise. The fog map alternates by minimap pixel column, starting again on each row: even
  columns (0, 2, …) sample the explored map and odd columns the visible map (`vision/fog.md`, "The minimap"). This
  holds on every map size; the sampled tile is the pixel's tile from the walk above.

### Units and buildings

After the terrain, these are drawn as filled rectangles (**dots**), later ones over earlier ones:

1. **Remembered buildings.** When a building is removed while hidden from the local player, and it carries the
   marker that it has been seen (the marker behind "Minimap units" in `vision/fog.md`), the game keeps a copy of it
   where it stood, up to 25 copies (no copy when all 25 are in use). Some buildings are never copied (see the open
   questions). A copy is removed once it is no longer hidden from the local player. Copies are drawn first, with the
   colour, position and size rules below. Copies are also drawn in the map view (a later spec).
2. **Units and buildings**, player 15 first and player 0 last, each player's units in their list order. A unit is
   skipped while it is dying, removed or inside something. A unit that isn't a building is drawn when it isn't hidden
   from the local player; a building is drawn when the local player has seen it, or the reveal-map cheat is on. In
   both cases it must also be detected by the local player. These are the "Minimap units" rule in `vision/fog.md`,
   "What uses vision".

**Dot colour** (palette colour), first rule that applies:

| Unit | Colour |
| --- | --- |
| Oil patch (type 0x5D) | 0 |
| Critter (type 0x39) | 247 |
| Marked as attacked (below) | 3; the mark is cleared |
| Its colour player is the local player, and this isn't demo playback | 250 |
| Otherwise | the minimap colour of its colour player's colour (table below) |

A unit's **colour player** is the player that owned it when it was created.

**Attack mark.** Every hit on one of the local player's units or buildings that takes hit points without killing it
(the hits that `ui/sounds.md` passes to its under-attack check) marks the unit, even when no sound plays; nothing is
marked during demo playback. A marked unit is drawn in colour 3 at the next redraw that draws it, and the mark is
then cleared. `ui/messages.md`, "Attacks on the minimap", gives the details.

| Player colour | Red | Blue | Green | Violet | Orange | Black | White | Yellow |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Minimap colour | 208 | 1 | 216 | 220 | 224 | 228 | 255 | 2 |

Player 15 (neutral) is always 251. By default player p (0 to 7) has the p-th colour of the table. Game setup can
assign colours differently (a later spec).

**Dot position and size.** The dot's corner is the pixel of the unit's tile position (its top-left tile for a
building), from the scale table. Its width is the unit's size in tiles converted the same way (the width in tiles × 4
on a 32-wide map, × 2 on 64, floor(× 4 / 3) on 96, × 1 on 128), but at least 2. The height works the same way. A dot
that would pass the minimap's right edge (pixel 128 and beyond) is made 1 pixel narrower, and the same for the bottom
edge. This happens only once, so a dot can still be clipped by the edge.

### The view rectangle

At the end of each redraw, a one-pixel outline in palette colour 246 marks the map view. Its corner is the view
position converted by the scale table. Its outer size is 14 tiles converted: 14 pixels on 128-wide maps, 28 on 64,
56 on 32, and 19 on 96 (floor(59 / 3)).

### Moving the view with the minimap

When a button press on the minimap means "move the view" (`ui/selecting-and-commanding.md`):

- The pointer is confined to the minimap until the button is released.
- On the press, and on every pointer move until the release, the view is centred on the pointer: with h = half the
  view size in minimap pixels (7 on 128-wide maps, 14 on 64, 28 on 32, 9 on 96):
  - px = pointer x − 24 − h, then clamped to 0..127; py = pointer y − 26 − h, clamped the same way;
  - the new view position is the tile from the scale table for (px, py), then clamped to the view range;
  - the view moves only if that position differs from the current one.

A minimap press that gives a tile (a target or a right-click command) uses the tile under the pointer itself, with no
half-view offset: pixel x − 24 and y − 26, each clamped to 0..127, then the scale table.

## Data

| Constant | Value |
| --- | --- |
| Minimap | screen x 24 to 151, y 26 to 153 (128 × 128) |
| Redraw | every 40 game cycles; within 4 cycles after the view moves |
| Terrain rebuild | every 6th redraw (240 cycles), or when the reveal-map cheat changes |
| Terrain sample | minitile pixel column 7, row 6, unmirrored |
| Fogged or terrain-off colour | palette 0 |
| Special dots | oil patch 0, critter 247, attacked 3, own 250, neutral 251 |
| Player dot colours | 208, 1, 216, 220, 224, 228, 255, 2 (red to yellow) |
| Smallest dot | 2 × 2 |
| View rectangle | outline colour 246 |
| Remembered buildings | at most 25 |

## Test vectors

| Situation | Expected |
| --- | --- |
| 128-wide map, own footman (1 × 1) at tile (10, 20) | Dot colour 250 at screen (34, 46), 2 × 2 |
| 64-wide map, enemy (player 1, blue) town hall (4 × 4) at tile (10, 20) | Dot colour 1 at screen (44, 66), 8 × 8 |
| 96-wide map, farm (2 × 2) at tile (30, 30), neutral owner | Dot colour 251 at screen (64, 66), 2 × 2 (floor(8 / 3) = 2) |
| 32-wide map, footman at tile (31, 31) | Dot at screen (148, 150), 4 × 4 (ends exactly at the edge, not trimmed) |
| 128-wide map, unit at tile (127, 0) | Dot at screen (151, 26), 1 wide, 2 high |
| Gold mine owned by player 15, drawn | Neutral colour 251 |
| Sheep (critter) owned by anyone | Colour 247 |
| Own footman hit by an archer, not killed | Colour 3 at the next redraw, then 250 again at the one after |
| 128-wide map, press at screen (100, 100) | View (69, 67) |
| 64-wide map, press at (100, 100) | px 62, py 60: view (31, 30) |
| 32-wide map, press at (151, 153) | px 99, py 99: tile (24, 24), clamped to (18, 18) |
| 96-wide map, press at (88, 90) | px 55, py 55: view (41, 41) |
| 96-wide map, press at (24, 26) | px and py clamped to 0: view (0, 0) |
| 128-wide map, right-click command at screen (100, 100) | Command tile (76, 74) |
| 128-wide map, view (50, 60) | Rectangle corner at screen (74, 86), 14 × 14 |
| 96-wide map, view (12, 12) | Rectangle corner at screen (40, 42), 19 × 19 |
| Countdown at 30; the view scrolls in cycle C | Redraw in cycle C + 4 (countdown 29 cut to 4) |
| A tree is cut down | The minimap terrain changes at the next terrain rebuild, up to 240 cycles later |
| Terrain preference off | All terrain colour 0; dots and rectangle still drawn |
| Game paused | No redraws; the minimap stays as it is |
| 64-wide map, tile (5, 0) explored but not visible | Pixel column 10 (even, explored map) shows terrain; column 11 (odd, visible map) is colour 0 |
| Enemy farm carrying the seen marker is destroyed while hidden from the local player, 3 copies in use | A fourth copy is kept and drawn where the farm stood until it is no longer hidden from the local player |
| Same, all 25 copies in use | No copy; the farm's dot disappears at the next redraw |

## Open questions

- **Non-square maps.** The scale and the terrain walk use the width for both axes, and the terrain walk moves down
  by whole map rows. On a map taller than it is wide, rows past what fits aren't shown; on a map wider than it is
  tall, the walk reads past the map's last row. What the game shows there wasn't checked.
- **Remembered buildings.** A value stored on each building (not decoded) keeps some buildings from being copied. In
  Enhanced rules the excluded range is one value wider than in Classic. Which buildings these are wasn't found.
  When a copy stops being hidden is tied to the local visibility update, which belongs to the drawing spec. Whether
  the drawing tests of "Units and buildings" (dying, detected) apply to a copy wasn't traced. Default: every copy is
  drawn.
- **Colour player after a change of owner.** No program path was found that changes a unit's colour player after it
  is created. Whether rescued or converted units keep their old minimap colour wasn't checked in the running game.
- **Terrain preference.** The preference that turns the minimap terrain off is inferred from the Preferences dialog's
  control order (as in `ui/selecting-and-commanding.md`); its label and default weren't checked.
- **List order.** Within one player, dots are drawn in the player's unit list order. How that list is ordered (newest
  first or last) is in a later spec.
