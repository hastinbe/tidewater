# UI: drawing the map view

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 21 (drawing)"), the
  sprite files under `Art\Unit\Other\` (`shadow.grp`, `charms.grp`, `ltfire.grp`, `hvfire.grp`, `blackx.grp`), and the
  tileset files `Art\bgs\*\*.vx4` / `*.vr4` (the fog shapes). Not yet confirmed against the running game.

## Behaviour

This spec covers what the 448 × 448 map view shows and in what order. It covers the layers, how units, buildings and
effects are placed and coloured, selection and flash boxes, spell charms, building fires, the fog, the overlays drawn
on top, the building-placement image and remembered buildings ("ghosts") in fog. The view's place on the screen and
its scrolling are in `ui/screen-and-cursors.md`. Terrain tile pictures are not covered here.

Positions are in **map pixels** (32 per tile) unless they say "screen". A map pixel (x, y) is drawn at screen
(176 + x − vx, 16 + y − vy), where (vx, vy) is the map pixel of the view's top-left corner (the view's tile position
× 32). Everything is clipped to the view.

The program only redraws parts of the view that changed. Every rule below is written as if the whole view were drawn
again each frame, which gives the same picture.

### Terms

- **Local player:** the player at this computer (`game/setup.md`).
- **Hidden** and **detected:** the per-unit sets of `vision/fog.md` ("Unit visibility", "Detection"). "Hidden" means
  hidden from the local player; "detected" means detected by the local player.
- **Building:** a type with the building flag (PUD flag bit 5). Everything else is a **unit**.
- **Air unit:** a unit whose movement layer is air.
- **Picture:** the sprite file a unit or building is drawn from. It is its type's sprite, replaced while it shows a
  construction site (`economy/build-train.md`) and by the remains' sprite after death (`core/corpses.md`).
- **Frame number:** the frame set by the animation scripts (`core/animation-scripts.md`).
- **Facing:** 0 to 7, 0 = north, clockwise.
- **Effect:** an entry in the effect slots of `core/unit-slots.md` (missiles, explosions, spell effects and the three
  permanent effects in slots 0 to 2).
- **Reveal-map cheat:** as in `vision/fog.md`.

### Layers

From bottom to top, the view is drawn in this order:

| # | Layer | Section |
| --- | --- | --- |
| 1 | Terrain tiles | not covered here |
| 2 | Remembered buildings, then buildings, in draw-list order | "Buildings and remembered buildings" |
| 3 | Ground and sea units, each with its shadow, box and charms; building boxes and building fires | "Units" |
| 4 | Air units, the same way | "Units" |
| 5 | Effects from slot 3 up | "Effects" |
| 6 | Fog | "Fog" |
| 7 | Message lines | `ui/messages.md` |
| 8 | The ground marker | "Overlays" |
| 9 | The selection rectangle being dragged | "Overlays" |

The building-placement image is drawn over the view as a separate screen picture ("Placement image").

So the fog covers units and missiles, but not the message lines, the ground marker or the selection rectangle.

### The draw list

Every unit and building is in one **draw list**, sorted by its pixel y (the top edge of its 32-pixel cell, for a
building the top edge of its footprint), smallest first. Units and buildings with a smaller y are drawn first, so
lower ones on the screen cover higher ones.

- The list is re-sorted at the end of every game cycle, after all units have been updated. The sort is **stable**:
  units with the same y keep their earlier order.
- A newly created unit goes in front of every unit with the same or greater y. So among equal y, the newest is drawn
  first (underneath).
- A unit that is dead, dying or removed sorts as if its y were the smallest possible. Dead and removed units aren't
  drawn, but dying ones are (`core/corpses.md`): a death animation or a corpse is drawn before, and so under, every
  other unit in its pass.

### Buildings and remembered buildings

Buildings are drawn into the background with the terrain, before any unit. Remembered buildings (below) are drawn
first, then the buildings in draw-list order.

Each building has a **remembered look**: a picture and a frame number. It starts as "never seen".

- **Not hidden:** the building is drawn with its current picture and frame. The remembered look is set to them.
- **Hidden:** the building is drawn with its remembered look. If it was never seen, it isn't drawn at all.

So an enemy building in fog shows as it was when the local player last saw it: still under construction, or
undamaged-looking, even if it has since changed.

Which building states keep a building out of this drawing wasn't traced (see the open questions).

#### Remembered buildings ("ghosts")

There are **25 ghost records**. They are the remembered-building copies of `ui/minimap.md`, and the "seen" marker there
is the remembered look above: a building carries it once its remembered look has been set. When a building is removed
from play (`core/unit-slots.md`):

- if it is hidden at that moment, it was seen at least once, and its remembered picture isn't a ruin (below), a copy
  of it goes into the first free ghost record. If all 25 are in use, no ghost is made;
- otherwise no ghost is made, and the building disappears from the view.

The ruin pictures excluded are those of the ruin types for building widths 2, 3 and 4 in Classic mode, and widths 1
to 4 in Enhanced mode (`core/corpses.md`, "Ruin type"). So in Classic mode a 1 × 1 ruin can leave a ghost.

Each ghost gets a hidden set, worked out every cycle like a unit's (`vision/fog.md`, "Unit visibility"). While it is
hidden it is drawn with its remembered look. As soon as it is not hidden (its footprint is seen again, or the
reveal-map cheat is on), it is deleted and its record is free. So a destroyed enemy building stays on the main view
until the local player sees its site again.

Ghosts are also drawn on the minimap (`ui/minimap.md`).

### Units

Units are drawn in two passes over the draw list: first everything that isn't an air unit, then the air units. In
each pass, entries are taken in list order. Entries are skipped when any of these holds:

- it is dead, removed or inside something;
- it is hidden;
- it isn't detected. In an Enhanced-mode game where the local player number is 8 or more, any player's detection is
  enough. This can't happen in single player.

For each remaining entry, in this order:

1. **Shadow (air units only):** frame 2 of `shadow.grp`, placed as below with a lift of −(42 + b), where b is the
   unit's bob offset (below). So the shadow is drawn 42 + b pixels lower than the unit's centre, and 42 + 2b below
   the unit itself.
2. **A unit:** its box (if any, below), then its sprite, then its charms (below). The box is under the sprite.
3. **A building:** its box (if any) is drawn over the building, which is already in the background. Then its fire
   (below), if it has one.

**Bob offset.** Every unit is drawn b pixels higher than its position, where b is its bob value (the "misc
animation" toggle that makes ships rock). For most units b is 0.

**Colour.** A unit's sprite pixels with palette indices 208 to 211 (the team-colour ramp) are drawn as indices
base, base + 1, base + 2 and base + 3. The base comes from the unit's colour player (`ui/minimap.md`):

| Colour player | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8–14 | 15 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Base | 208 | 212 | 216 | 220 | 224 | 228 | 232 | 12 | 208 | 12 |

All other indices are drawn as they are, and index 0 is transparent. Buildings and remembered buildings use the same
rule. Effects, shadows and fires don't set a colour: they use whatever base was set last. This makes no visible
difference unless their pictures use indices 208 to 211.

**Frame and mirroring.** The frame drawn is the **column** for the facing plus the frame number. The columns are:

| Facing | 0 N | 1 NE | 2 E | 3 SE | 4 S | 5 SW | 6 W | 7 NW |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Column | 0 | 1 | 2 | 3 | 4 | 3 | 2 | 1 |
| Mirrored | no | no | no | no | no | yes | yes | yes |

A frame number at or past the picture's frame count draws nothing.

**Placement.** A sprite file has a size W × H, and each frame has an offset (fx, fy) and a size fw × fh inside it.
The sprite's **centre point** is:

- for a unit or building: its pixel position plus (16 × width in tiles, 16 × height in tiles), the centre of its
  footprint;
- for an effect or the ground marker: its position.

The frame's top-left corner is drawn at:

- x = centre x − floor(W / 2) + fx, or when mirrored, centre x + floor(W / 2) − fw − fx, and the frame is flipped
  left to right;
- y = centre y − lift − floor(H / 2) + fy, where the lift is the bob offset for units, the shadow lift above for
  shadows, and 0 for everything else.

### Boxes

A unit's **box** is the outline of its selection box (`ui/selecting-and-commanding.md`): with L and T the selection
box's left and top pixels and (bw, bh) the type's box size, the outline is a 1-pixel rectangle from (L − 1, T − 1) to
(L − 1 + bw, T − 1 + bh), both corners included. Each of its four edges is drawn only if that edge's line lies inside
the view; an edge that does is clipped to the view.

Which box is drawn, and its colour, is the first row that matches:

| Unit | Box |
| --- | --- |
| Flashing (`game/rescue.md`, "Flash"), and it is dead, dying, removed or inside something, or not detected | None, and the flash stops at once |
| Flashing, countdown bit 3 clear | None, **even if the unit is selected** |
| Flashing, countdown bit 3 set | The flash colour (250 own, 251 allied, 249 other) |
| Selected | 250 if its owner is the local player, 251 if the owner is allied with the local player, 249 otherwise |
| A building, while the local player is placing a building | 246 |
| Anything else | None |

For buildings, the last row applies only to the local player's own buildings that aren't dead, dying, removed or
inside something. So while placing a building, all of the local player's buildings are outlined.

The selected colours are recomputed when alliances change. "Allied" is the local player's alliance setting for that
owner. The neutral player is "other" unless allied.

### Charms

After a unit's sprite, a small icon from `charms.grp` (16 × 16) is drawn for each spell running on it
(`spells/spells.md`). The icons go in a row along the top edge of the box, starting at the box outline's top-left
corner (L − 1, T − 1):

| Spell | Icon (frame) | x offset |
| --- | --- | --- |
| Bloodlust | 0 | 0 |
| Haste | 1 | 16 |
| Slow | 2 | 16 |
| Invisibility | 3 | 32 |
| Unholy armour | 4 | 48 |

Haste and slow share a timer and a place, so only one of them shows. An icon is drawn only if the 20 × 20 area at its
corner lies wholly inside the view. Near the view's edges the icon is left out, not clipped. Charms show whether or not
the unit is selected. Buildings never show charms.

### Building fires

A complete building that has lost hit points shows a fire:

| Hit points h, maximum M | Fire |
| --- | --- |
| h < floor(M / 2) | heavy fire (`hvfire.grp`, 10 frames) |
| floor(M / 2) ≤ h < floor(3 × max(M, 1) / 4) | light fire (`ltfire.grp`, 6 frames) |
| otherwise | none |

(With M = 0 both bounds are 0, so no fire shows.) The fire is re-checked every cycle.

The two fires are the permanent effects in effect slots 0 (light) and 1 (heavy) (`core/unit-slots.md`). Their
animations run all the time. Every burning building shows the same animation, offset by its own phase:

- **Frame:** (the fire's frame, found as for any effect, plus the building's phase) mod n, with n = 6 for light and
  10 for heavy. The phase is the 0-to-5 value the building drew at creation (`core/timing-and-rng.md`).
- **Position:** centre point = the building's footprint centre, moved up by 24 pixels for a building 2 tiles high, 20
  for 3 tiles and 26 for 4 tiles (no change for 1 tile).

A building that isn't drawn in layer 3 (hidden or not detected) shows no fire, and ghosts never do.

### Effects

After both unit passes, the effects in slots 3 and up are drawn in slot order (missiles, explosions, spell effects).
Free slots are skipped.

Unless the reveal-map cheat is on, an effect is left out when the local player sees none of the 3 × 3 tiles around it:
the tile containing its position and its 8 neighbours (parts off the map are ignored), tested with the local
player's "seen" bits (`vision/fog.md`, "The "seen" map"). This test is separate from the fog drawing, which then
covers the effect anyway where the fog is drawn.

An effect is drawn only while its slot is **showing**: neither free nor in the state a new effect is given when it is
made (see the open questions).

Each effect kind has its own sprite file (see the open questions). The frame is the column for the effect's facing plus
its frame number, as for units, and mirrored the same way.

### Fog

Unless the reveal-map cheat is on, every view tile gets fog drawn over it after the effects, from the tile's codes in
the visible map (v) and the explored map (e) of `vision/fog.md` ("The fog maps").

**Fog shapes.** For code c from 1 to 15, the **shape** S(c) is a 32 × 32 picture: bytes 1024·c to 1024·c + 1023 of the
tileset's `.vr4` file, read as 32 rows of 32 bytes. Drawing a shape copies its non-zero pixels with their own values
(239 in the forest tileset), and its zero pixels are transparent. Codes 0 and 16 have no shape. The `.vx4` file's
megatile c points at the same data. Unlike a terrain tile, the bytes are read as one 32 × 32 block, not as 8 × 8
minitiles.

**Checkerboard.** "Even pixels" are those where (x + y) is even, with x and y counted from the tile's top-left. Tile
corners on the screen are at even coordinates, so screen coordinates give the same result.

| v | e | Drawn over the tile |
| --- | --- | --- |
| 0 | any | nothing |
| 16 | 16 | the whole tile in colour 0 (black) |
| 16 | 0 | colour 0 on the even pixels |
| 16 | 1 to 15 | colour 0 on the even pixels, and S(e)'s non-zero pixels on the odd pixels |
| 1 to 15 | 0 | S(v)'s non-zero pixels on the even pixels only |
| 1 to 15 | 1 to 15, e = v | S(e)'s non-zero pixels, all of them |
| 1 to 15 | 1 to 15, e ≠ v | S(v)'s non-zero pixels on the even pixels, then all of S(e)'s non-zero pixels |
| 1 to 15 | 16 | S(v) on the even pixels, then nothing more (code 16 has no shape) |

Code 5 is never produced (`vision/fog.md`), so its shape isn't used. So unexplored ground is black, explored ground out
of sight is a 50 % black checkerboard, and the edges between them use the shapes. The case v = 16 with e = 0 is the
usual "explored but not in sight". Since the explored map is never more fogged than the visible map, the last row
doesn't occur in normal play.

### Overlays

After the fog:

1. **Message lines:** as in `ui/messages.md`.
2. **The ground marker:** the permanent effect in slot 2 (`blackx.grp`), shown where a command without a target was
   given (`ui/selecting-and-commanding.md`). Its slot is never freed, so it is drawn only while the slot is showing
   (as in "Effects"), with no fog test, at its position, frame = facing column + frame number. When its animation
   ends, the slot goes back to the state of a newly made effect, and the marker stops showing.
3. **The selection rectangle:** while the left button is held after a press on the map
   (`ui/selecting-and-commanding.md`), a 1-pixel outline in colour 250 from the press point to the pointer, both
   corners included. It disappears when the button is released.

### Placement image

While the local player is placing a building (`ui/placing-buildings.md`), an image the size of the building's
footprint (32 × width by 32 × height pixels) follows the pointer.

- **Contents:** the building's frame 0 in the local player's colours, placed in the image as its sprite would be in
  the footprint (centre point at the footprint centre). Pixels it doesn't cover are transparent.
- **Tile colouring:** each 32 × 32 tile of the image gets a checkerboard: the pixels where (x + y) is odd are set to
  colour 150 if the tile is fine and colour 10 if it is bad. The pixels where (x + y) is even keep the building
  image (or stay transparent).
- **Good and bad tiles** come from the site check (`ui/placing-buildings.md`, "The site check"). A tile is bad when it
  is unexplored or carries a blocking bit. When the outcome is off the map, no shore, too close to a mine, too close
  to oil or no oil patch, every tile is bad. A builder standing on the site doesn't make its own tile bad.
- **Position:** the image's top-left is on the site's top-left tile. It is only shown while the pointer is inside
  the map view; it disappears when the pointer leaves the view and comes back when it returns.

## Data

| Item | Value |
| --- | --- |
| View | 448 × 448 pixels at screen (176, 16) |
| Draw-list order | pixel y ascending, stable; new entries before equal y |
| Passes | ground and sea, then air; effects from slot 3 up after both |
| Team-colour ramp | source indices 208–211; base by colour player (table in "Units") |
| Facing columns | 0, 1, 2, 3, 4, 3, 2, 1; facings 5–7 mirrored |
| Shadow | `shadow.grp` frame 2 (23 × 23), lift −(42 + bob) |
| Box outline | (L − 1, T − 1) to (L − 1 + bw, T − 1 + bh) |
| Box colours | flash colour; selected 250 / 251 / 249; placing 246 |
| Charms | `charms.grp`, 5 frames 16 × 16 stored unpacked (256 bytes each); x offsets 0, 16, 16, 32, 48; shown only when the 20 × 20 area is inside the view |
| Fires | light (slot 0, `ltfire.grp`, 6 frames) for floor(M/2) ≤ h < floor(3·max(M,1)/4); heavy (slot 1, `hvfire.grp`, 10 frames) for h < floor(M/2) |
| Fire height offsets | building height 2: −24; 3: −20; 4: −26; 1: 0 |
| Effect fog test | the local player's "seen" bits on the 3 × 3 tiles around the effect |
| Fog shapes | `.vr4` bytes 1024·c … 1024·c + 1023, codes 1–15 |
| Ghost records | 25 |
| Placement checkerboard | colour 150 good, 10 bad, on pixels with (x + y) odd |
| Selection rectangle | colour 250 |

## Test vectors

| Situation | Result |
| --- | --- |
| Footman A at pixel y 320, grunt B at y 352, overlapping on screen | A drawn first, B over it |
| Two units at the same y; A in the list before B; a new unit C created at that y | Order C, A, B (C underneath) |
| A gryphon rider and a footman at the same place | The footman is drawn in the first pass and the gryphon (and its shadow) in the second, so the gryphon is on top |
| Unit facing 6 (W) with frame number 10 | Frame 12, mirrored left to right |
| Sprite file 72 × 72, frame offset (20, 18), size 30 × 36; unit at pixel (320, 160), 1 × 1, bob 0, facing 2 | Centre (336, 176); frame top-left at (336 − 36 + 20, 176 − 36 + 18) = (320, 158) |
| Same, facing 6 | Mirrored: x = 336 + 36 − 30 − 20 = 322; same y |
| Selected own footman, box 31 × 31, selection box left/top (321, 161) | Outline from (320, 160) to (351, 191) in colour 250, drawn before the footman's sprite |
| Selected enemy unit, not allied | Box in colour 249 |
| Selected rescued unit flashing, countdown 100 | 100 has bit 3 (value 8) clear: no box this frame, even though it is selected |
| Same, countdown 104 | Bit 3 set: box in the flash colour |
| Local player starts placing a farm; it owns a town hall and a barracks under construction | Both outlined in colour 246 |
| Unit with bloodlust and slow | Icon 0 at the box corner, icon 2 at +16 |
| Bloodlusted unit whose box corner is 10 pixels from the view's right edge | The icon is left out |
| Town hall (M = 1200) at 700 hit points | Light fire (600 ≤ 700 < 900) |
| Same at 599 | Heavy fire |
| Same at 900 | No fire |
| Two 4 × 4 halls with phases 0 and 3, both heavy fire, fire frame 8 | Frames 8 and (8 + 3) mod 10 = 1 |
| A missile in a fogged area where none of the 9 tiles around it is seen by the local player | Not drawn (and the fog would cover it anyway) |
| Tile with v = 16, e = 0 | Black on even pixels; terrain shows on odd pixels |
| Tile with v = 16, e = 16 | All black |
| Tile with v = 2 (top half fogged), e = 0 | Shape 2's non-zero pixels on the even pixels only |
| Reveal-map cheat on | No fog drawn; every effect drawn |
| Enemy farm seen under construction, then fogged; it completes out of sight | Still drawn as the construction site |
| Enemy barracks destroyed out of sight, after being seen | A ghost stays until its footprint is seen again, then disappears |
| Same, Enhanced mode, a 1 × 1 ruin was the last thing seen there | No ghost |
| 26 enemy buildings destroyed out of sight, all hidden and seen, no ghost removed meanwhile | 25 ghosts; the 26th leaves none |
| Placing a farm; one site tile has a tree | That tile's odd pixels in colour 10; the other three tiles in 150 |
| Placing a shipyard on dry land far from water (no shore) | All 9 tiles in colour 10 |
| Command with no target given at a ground pixel | The ground marker is drawn there, over the fog and the message lines |

## Open questions

- **Palette hues.** Colours 10 and 150 of the placement checkerboard and 246 of the placement boxes are named by index
  only. Their hues depend on the tileset palette and weren't checked.
- **Colour player in multiplayer.** The team-colour base is taken straight from the colour player number. Whether the
  multiplayer lobby's colour choices change this table (as they change the minimap colours) wasn't traced.
- **Placement image and the pointer.** Which of the two is drawn on top, and whether the image is clipped at the
  view's edges when the site is partly outside, wasn't traced. The image is a separate screen picture, like the
  pointer.
- **Terrain tiles.** How terrain tiles are drawn from `.vx4` / `.vr4` (minitile flips and so on) belongs to a later
  spec.
- **Shadow frames.** Only frame 2 of `shadow.grp` (the largest, 23 × 23) is used. Frames 0 and 1 are not used by the
  map view; whether anything else uses them wasn't checked.
- **Effect sprite files.** Each effect kind (29 kinds, 0x00 to 0x1C) has its own sprite file under
  `Art\Unit\Other\`, from a table in the program. The kind-to-file table isn't in the research notes yet, so it
  isn't given here. Known so far: 0x13 is `ltfire.grp`, 0x14 is `hvfire.grp` and 0x1C is `blackx.grp`.
- **Effect slot states.** A new effect is made in a state that isn't "showing", and the step that makes it showing
  wasn't traced. Default: an effect shows from its first update. The ground marker's slot goes
  back to the new-effect state at the end of its animation; how long that takes is in the animation data.
- **Building states in the background.** Which states keep a building out of the background drawing wasn't traced.
  Default: as in the unit passes, a building that is dead, removed or inside something isn't drawn; a dying one
  (a ruin) is.
- **Remembered look outside the view.** Whether a building's remembered look is also set while the building is
  outside the map view (and so whether a building that was in sight only while off the view gets the "seen" marker
  and can leave a ghost) wasn't traced. Default: every building, in the view or not.
- **Ghost state.** A ghost is a copy of the building's whole record at its removal. If that record's state already
  says "removed", the visibility rule of `vision/fog.md` would keep the ghost hidden for good. Default: a ghost's
  hidden set uses only the footprint rule, so it goes once its site is seen.
- **Running game.** None of this has been compared with screenshots of the running game.
