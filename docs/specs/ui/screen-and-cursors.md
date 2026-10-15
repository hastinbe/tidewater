# UI: screen layout, scrolling and pointers

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 19 (screen,
  scrolling, cursors)"), the screen layout files in `rez\` (`infobox.bin`, `infobox2.bin`, `textbox.bin`,
  `sidebox.bin`, `stat_f10.bin`, `spd_dlg.bin`), the screen pictures in `Art\`, the resource icons `Art\icons.grp`
  (frame sizes) and the pointer files `Art\*.cur`.
  The in-game rules are not yet confirmed against the running game; the pointers outside a game were watched in
  round 69 (the table says which).
- **Changes:** 2026-10-12: the default-scroll-speeds open question is closed by `ui/game-menu.md` ("Stored
  settings"): both default to 4 and are kept with the player's stored settings.
- **Changes:** 2026-10-15, round-69 errata (`ERRATA-round69.md`, finding 1; observed on the running game, Enhanced
  campaign launch): the pointer on the flow screens, its colours, and how it hides through screen fades.
- **Changes:** 2026-10-15, round-70 errata (`ERRATA-round70.md`, findings I2 to I4, R1 to R3; observed on the
  running game, Enhanced): no pointer during the introduction video; the credits, load-game and multiplayer menu
  screens and the defeat dialog watched; the version line shows during the menu fade-in. Reviewed by a spec-review
  session, 2026-10-15.

## Behaviour

This spec covers the in-game screen at 640 × 480: where each part sits, the top resource bar, the two buttons above
the minimap, how the view scrolls (screen edge and keys, with their speeds), and which pointer shape shows when.
What the panels contain is in `ui/info-panel.md` and `ui/command-buttons.md`. Mouse clicks are in
`ui/selecting-and-commanding.md`. The minimap is in `ui/minimap.md` and the message line in `ui/messages.md`.

"Enhanced rules" and "Classic rules" are the rule sets of `core/timing-and-rng.md`. The **local race** is the local
player's race; several pictures have a human and an orc version (`h_…` and `o_…`), chosen by it.

### Layout

All positions are screen pixels, edges included.

| Part | Area | Picture (human / orc) |
| --- | --- | --- |
| Menu buttons | x 0 to 175, y 0 to 23 | `h_lft0.bmp` / `o_lft0.bmp` (176 × 24) |
| Minimap frame | x 0 to 175, y 24 to 159; the minimap itself is x 24 to 151, y 26 to 153 | `h_lft1.bmp` / `o_lft1.bmp` (176 × 136) |
| Information panel | x 0 to 175, y 160 to 335 (`ui/info-panel.md`) | the race's frame picture |
| Command panel | x 0 to 175, y 336 to 479 (`ui/command-buttons.md`) | `h_lft2.bmp` / `o_lft2.bmp` (176 × 144) |
| Resource bar | x 176 to 623, y 0 to 15 | `h_top.bmp` / `o_top.bmp` (448 × 16) |
| Map view | x 176 to 623, y 16 to 463: 448 × 448 pixels, 14 × 14 tiles of 32 pixels | |
| Status bar | x 176 to 623, y 464 to 479: pointed-at text, costs and the chat line (a later spec) | `h_btm.bmp` / `o_btm.bmp` (448 × 16) |
| Right edge | x 624 to 639, y 0 to 479 | `h_rgt.bmp` / `o_rgt.bmp` (16 × 480) |

The **view position** is the map tile shown at the map view's top-left corner. It ranges from 0 to (map width − 14)
across and from 0 to (map height − 14) down. A map pixel (px, py) is drawn at screen (176 + px − 32 × view x,
16 + py − 32 × view y).

### Resource bar

The bar shows the local player's gold, lumber, oil and food. Each item has an area 16 pixels high. Positions are x
offsets from the bar's left edge (screen x 176):

| Item | Single player | Multiplayer | Icon (frame of `icons.grp`) |
| --- | --- | --- | --- |
| Gold | 36 to 108 | 8 to 77 | 0 |
| Lumber | 109 to 181 | 78 to 147 | 1 |
| Oil | 182 to 254 | 148 to 217 | 2 |
| Food | 255 to 327 | 218 to 287 | 4 |

Multiplayer uses `infobox2.bin`, which has two more areas (288 to 367 and 368 to 447) whose use isn't traced.

**Drawing an item.** The 14 × 14 icon is drawn at the item's left edge. The text starts 18 pixels to the right of
the item's left edge, vertically centred (the font is an open question).

- **Gold, lumber, oil:** the shown value as a decimal number. A shown value of 1,000,000 or more is replaced by a
  fixed placeholder text (see the open questions).
- **Food:** "used/supply", where used = the food the player's units use (`economy/build-train.md`), and supply =
  the food supply, shown at most 200 (under both rule sets). The used number is drawn in text colour 3 when
  used > supply, otherwise in text colour 2; the slash and the supply are in text colour 1. The food item is **not
  drawn under Classic rules**, and under Enhanced rules it isn't drawn while the supply is 0.

**Counting up and down.** Gold, lumber and oil don't jump to a new value; each has a **shown value** that moves
toward the real one. On each interface update where they differ, with d = real − shown:

shown ← shown + (d / 16, rounded toward zero) + (1 if d > 0, otherwise −1).

So a change of 15 or less takes one step per point, and a large change closes about one sixteenth of the gap per
update. When the game screen opens, each shown value starts at real − 1, so the first update shows the real value.
Food is shown as it is, without counting.

### Menu buttons

The strip above the minimap holds two buttons, each 80 pixels wide:

- the **menu button** at x 4 to 83 opens the game menu;
- the **pause button** at x 88 to 167 pauses the game, and shows as a **resume button** while the game is
  paused. It is the same as Ctrl/Alt+P.

In Enhanced multiplayer, a player who has no pauses left sees only the menu button; Ctrl/Alt+P also does nothing
for that player.

### Scrolling

The view scrolls in steps of **2 tiles** on each axis, and is clamped to its range. Nothing scrolls while the game
is paused, while a menu is open, or during the attract-mode demo.

The game checks for scrolling each time it reads input. Scrolling is paced by the system clock in **ticks** at
least 20 ms apart. The screen edge and the keys have separate tick clocks and separate speed settings.

**Screen edge.** The pointer is at an edge when:

| Edge | Pointer position | Only if the view can still move that way |
| --- | --- | --- |
| Left | x ≤ 5 | view x > 0 |
| Right | x ≥ 630 | view x < map width − 14 |
| Top | y ≤ 5 | view y > 0 |
| Bottom | y ≥ 470 | view y < map **width** − 14 (see below) |

The bottom test really compares the view's y with the largest view x, not the largest view y. On square maps the
two are the same. On a map that is wider than it is tall, the bottom edge can still be "able to scroll" when the view
is already at the bottom: the arrow shows and clicks are blocked, but the step itself is clamped, so the view doesn't
move. On a map that is taller than it is wide, the bottom edge stops scrolling once view y reaches width − 14, short
of the real bottom (the keys and other moves still reach it). An implementation should keep the literal test.

A corner gives both directions. While the pointer is at an edge that can scroll:

- the pointer takes the matching arrow shape (below), and mouse clicks on the map, minimap and panel do nothing;
- on each tick, a step is made if the edge countdown is 0, and the countdown is then set to W; otherwise the countdown
  goes down by 1 and no step is made. With W = 6 − mouse scroll speed, a step happens every (7 − speed) ticks.

The edge countdown and the key countdown both start at 0 when the program starts. Nothing resets them when a game
starts or when the pointer leaves the edge, so they carry over from earlier scrolling.

**Keys.** Used only when the pointer isn't at a scrolling edge. On each tick the key countdown goes down by 1; when
it was 0, it is set to W and the held keys are read. W comes from the key scroll speed through the same table as
the edge (below), so speed 6 is the fastest for both. Nothing is read while Ctrl or Alt is
held.

| Keys | Direction |
| --- | --- |
| Up, Down, Left, Right arrows | up, down, left, right |
| Home | up and left |
| Page Up | up and right |
| End | down and left |
| Page Down | down and right |
| Numeric keypad 8, 2, 4, 6, 7, 9, 1, 3 | up, down, left, right, up-left, up-right, down-left, down-right (only while no chat line is being typed) |

The directions of all held keys are added up; on each axis only the sign of the total counts, so one step is at most
2 tiles per axis.

**Speeds.** The game speed dialog (F8) has a mouse scroll speed and a key scroll speed, each 0 to 6 (fastest). Both
default to 4 and are stored between runs (`ui/game-menu.md`, "Stored settings").

| Speed | 0 | 1 | 2 | 3 | 4 | 5 | 6 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| W | 6 | 5 | 4 | 3 | 2 | 1 | 0 |
| Ticks per step | 7 | 6 | 5 | 4 | 3 | 2 | 1 |

At speed 6 the view moves 2 tiles about every 20 ms.

Other things move the view too: the minimap (`ui/minimap.md`), control groups and Ctrl/Alt+C
(`ui/selecting-and-commanding.md`), and the saved view positions F2 to F4 (Shift+F2 to F4 save them). Under both
rule sets all three saved positions start as the view at the start of the game.

### Pointer shapes

The pointer is a picture from a pointer file, drawn with its **hot spot** on the mouse position. The hot spot is
the point that clicks, hovering and placement use. Pointers marked "race" have a human and an orc version, chosen by
the local race.

The shape is chosen each time input is read. The first rule that applies wins:

1. **Waiting for the network:** in multiplayer, once the game has waited for another player's data for more than
   one retry period, the watch.
2. **Scrolling edge:** the arrow for the edge (north, north-east, east and so on, as in the table below).
3. **Over the panel or elsewhere** (not the map view or minimap): the normal arrow.
4. **Over the minimap:** the target cross ("no unit" version) in target mode, otherwise the normal arrow.
5. **Over the map view:**
   - placement mode: the normal arrow (the building outline is drawn on the map);
   - target mode: from the units under the pointer (the hover list of `ui/selecting-and-commanding.md`, up to 9):
     - no unit: target cross, "no unit" version;
     - any unit of the local player: target cross, "own" version;
     - otherwise, if a neutral player's unit is among them: target cross, "no unit" version;
     - otherwise (only other players' units): target cross, "other" version;
   - otherwise: the magnifying glass if any unit is under the pointer, else the normal arrow.

While the game is paused or a menu is open, the map's pointer rules aren't used and the pointer is the normal arrow.
No pointer is animated.

**Pointers outside a game.** The screens before and after a game use a fixed pointer each, not the rules above:

| Screen | Pointer | Colours | Seen on the running game |
| --- | --- | --- | --- |
| Introduction video | none (hidden) | | yes (other videos and the campaign finale: not watched, see "Open questions") |
| Title picture | none (hidden) | | yes |
| Main menu, single-player menu, campaign menu, custom-scenario setup, load-game screen, the multiplayer menus up to and including the connection-method screen (Enhanced or Classic choice, connection method, direct-connection setup) and their small dialogs (name entry, "Unable to locate IPX network") | orc normal arrow (`o_arrow.cur`) for every race and campaign | the menu picture's palette | yes |
| Act screen | none (hidden) | | yes |
| Mission briefing | 3D arrow (`arrow_3d.cur`) | the briefing picture's palette | yes |
| Score screen | 3D arrow (`arrow_3d.cur`) | the score picture's palette | yes (human victory) |
| Credits (picture `Art\credits.pcx`) | 3D arrow (`arrow_3d.cur`) | the credits picture's palette | yes |
| In a game, and the game's own dialogs (including the victory and defeat dialog) | the in-game rules above (race arrow while a dialog is open) | the tileset palette | yes (human victory dialog, orc defeat dialog) |
| Battle.net screens, multiplayer game setup and lobby | not known | | no (not reachable in the oracle) |

The menus' arrow is the orc one because the main menu sets the interface race to orc each time it opens; the race of
a game is set again when the game starts. The 3D arrow (hot spot (0, 0), 15 × 21) uses only palette indices 6 to 13,
which every briefing and score picture shares, so it looks the same on all of them.

**Colours.** The pointer is drawn into the 8-bit screen picture, so its pixels are the current palette's entries at
the indices in the pointer file. It has no palette of its own.

**Pointer and fades.** The pointer never fades with the screen:

- When a screen starts its fade-out (the click or key that closes it), the pointer disappears at once. The fade
  lengths are in `ui/palette-effects.md` ("Screen fades").
- It stays hidden through the fade to black, any black gap, and the next screen's fade-in. On the menu screens the
  buttons are also not drawn during the fade-in: the fade-in shows the picture and the version line
  ("Warcraft II BNE v2.02", bottom left, drawn by the program over the picture), which fade in together. The buttons
  and the pointer appear on the frame that ends the fade-in.
- It appears, at full colour, when the next screen's fade-in has ended (on the frame that shows the screen at its own
  palette).
- Screens with no fade-in (the briefing, and the score screen after the game's 1 ms fade-out) or a 10 ms one (the
  game) show the pointer from their first frame.

So no frame ever shows a pointer in a partly faded palette, and no pointer at all (not even the watch) shows while
one screen gives way to the next.

| Pointer | File (human; orc if race) | Hot spot (x, y) | Size |
| --- | --- | --- | --- |
| Normal arrow (race) | `h_arrow.cur`; `o_arrow.cur` | (3, 2) | 28 × 32; 26 × 32 |
| Target cross, no unit (race) | `htarget.cur`; `otarget.cur` | (15, 15) | 32 × 32 |
| Target cross, other (race) | `htarget2.cur`; `otarget2.cur` | (15, 15) | 32 × 32 |
| Target cross, own (race) | `htarget3.cur`; `otarget3.cur` | (15, 15) | 32 × 32 |
| Magnifying glass | `select.cur` | (11, 11) | 34 × 35 |
| Watch | `watch.cur` | (11, 18) | 23 × 35 |
| Edge north / south | `ptr_n.cur` / `ptr_s.cur` | (12, 2) / (12, 22) | 32 × 24 |
| Edge east / west | `ptr_e.cur` / `ptr_w.cur` | (22, 10) / (4, 10) | 32 × 24 |
| Edge north-east / north-west | `ptr_ne.cur` / `ptr_nw.cur` | (20, 2) / (2, 2) | 32 × 24 |
| Edge south-east / south-west | `ptr_se.cur` / `ptr_sw.cur` | (20, 18) / (2, 18) | 32 × 24 |

The hot spots and sizes are read from the files, which start with four 16-bit numbers (hot spot x, hot spot y,
width, height) followed by width × height palette indices. The game uses the file's hot spot as it is.

**Placement.** Each time the pointer is checked in placement mode, while the pointer is inside the map view, the
site's top-left tile is the tile under the hot spot (the normal arrow's tip): (pointer x − 176) / 32 + view x and
(pointer y − 16) / 32 + view y, with integer division. The building is not centred on the pointer, and the hot spot
isn't moved. Outside the map view the site isn't updated. For a building whose movement layer is sea (the oil
platforms in the default data), each coordinate then goes onto odd values as `ui/placing-buildings.md` ("The site")
describes.

## Data

| Constant | Value |
| --- | --- |
| Screen | 640 × 480 |
| Map view | x 176 to 623, y 16 to 463; 14 × 14 tiles |
| Scroll step | 2 tiles |
| Scroll tick | at least 20 ms |
| Edge zones | x ≤ 5, x ≥ 630, y ≤ 5, y ≥ 470 |
| Scroll speed settings | 0 to 6; ticks per step = 7 − speed |
| Resource counting | step = trunc(d / 16) + sign(d) |
| Food display cap | 200 |
| Large-number limit | 1,000,000 |
| Resource icons | `icons.grp`, 14 × 14: gold 0, lumber 1, oil 2, food 4 |

## Test vectors

| Situation | Expected |
| --- | --- |
| 128 × 128 map, view (0, 0), pointer at (2, 240) | No edge (view x is 0): normal pointer rules |
| 128 × 128 map, view (50, 50), pointer at (2, 2) | North-west arrow; mouse speed 6: view moves to (48, 48) on the first tick, then (46, 46) about 20 ms later |
| Mouse speed 3, edge countdown 0, pointer held at the right edge, view x 0 | Steps on ticks 1, 5, 9, …: view x 2, 4, 6 |
| Mouse speed 3, edge countdown 2 (left over), pointer held at the right edge | Steps on ticks 3, 7, 11, … |
| 128 wide, 64 high map, view y 50 (the bottom), pointer at y 475 | South arrow, clicks blocked, view stays at y 50 |
| 64 wide, 128 high map, view y 50, pointer at y 475 | No edge (50 = 64 − 14): normal pointer rules; Down arrow key still scrolls |
| View x 113 on a 128-wide map, scrolling right | Step clamps to 114; then the right edge no longer scrolls and the arrow goes away |
| Key speed 6, Up and Left held | One step up and left (2 tiles each) per tick |
| Key speed 0, key countdown 0, Up held | Steps on ticks 1, 8, 15, … |
| Up and Down held | No vertical movement |
| Ctrl held with an arrow key | No scrolling |
| Chat line open, numeric keypad 8 held | No scrolling (the arrow keys still scroll) |
| Gold rises from 100 to 400 (shown 100) | Shown values per update: 119, 137, 154, … until 400 (each step trunc(d/16) + 1) |
| Gold drops from 400 to 395 (shown 400) | 399, 398, 397, 396, 395 |
| Game screen opens with 500 gold | Shown 499, then 500 at the first update |
| Enhanced, used food 7, supply 4 | "7" in colour 3, "/4" in colour 1 |
| Enhanced, supply 220 from farms | Shown as /200 |
| Classic rules | No food item |
| Target mode, pointer over a gold mine | Target cross, "no unit" version |
| Target mode, pointer over an enemy grunt | Target cross, "other" version |
| Target mode, pointer over an own footman next to an enemy grunt (both listed) | Target cross, "own" version |
| Normal mode, pointer over an own footman | Magnifying glass |
| Orc player, normal mode, pointer over empty ground | Orc normal arrow |
| Placement mode, pointer at screen (300, 100), view (10, 20) | Site top-left tile (13, 22) |
| Human campaign, main menu after the title | Orc normal arrow, colours from the menu picture's palette |
| Main menu, a button that opens another menu screen clicked | Pointer gone from the first frame of the 200 ms fade-out until the next screen's fade-in has ended; then the orc arrow at full colour |
| Custom-scenario setup, Cancel clicked | Pointer gone through the 200 ms fade-out and the main menu's 200 ms fade-in; back on the frame that shows the main menu at its own palette |
| Title picture, any frame | No pointer |
| Mission briefing, first frame | 3D arrow at full colour |
| Briefing, Continue clicked | Pointer gone from that frame; the game shows the in-game race arrow from its first frame |
| Victory dialog in a human campaign mission | Human normal arrow (`h_arrow.cur`), tileset palette |
| Score screen, first frame | 3D arrow at full colour |
| Introduction video playing, pointer moved over it | No pointer drawn (round 70, seen) |
| Main menu, Show Credits clicked | Credits picture `Art\credits.pcx` with its own palette; 3D arrow, exact pixel match (round 70, seen) |
| Credits, OK clicked | Pointer gone through the fade-out and the main menu's fade-in; the fade-in frames show the picture and the version line at the same brightness, no buttons; buttons and the orc arrow appear on the first full-colour frame (round 70, seen) |
| Single-player menu, Load Game clicked | Load-game screen, orc arrow, menu palette (round 70, seen) |
| Main menu, Multi Player Game, Enhanced, then each connection screen | Orc arrow, menu palette, on every screen up to the IPX error dialog (round 70, seen) |
| Orc campaign mission lost (defeat dialog up) | Orc normal arrow (`o_arrow.cur`), game palette (round 70, seen; shape match) |
| Orc defeat, score screen | Picture `Art\olosglu.pcx`, 3D arrow, exact pixel match (round 70, seen) |

## Open questions

- **The large-number placeholder.** Values of 1,000,000 or more show a fixed six-character word from the program,
  not from a string table. The implementation needs its own placeholder text.
- **The two extra multiplayer bar areas** (`infobox2.bin` items 5 and 6) weren't traced.
- **Unused pointers.** A second arrow (`h_arrow2.cur`, `o_arrow2.cur`) and a cross (`cross.cur`) are loaded. No
  use of them was found in a game or on the flow screens. The 3D arrow is the briefing, score and credits pointer
  (above); it is also set when the game screen opens and replaced at the first input check.
- **Flow screens not observed.** Battle.net, the multiplayer game setup and the lobby weren't reached: the oracle
  has no IPX network or serial port, and Battle.net needs a live connection to an outside service, which wasn't
  attempted. The other videos and the campaign finale weren't played (the introduction showed no pointer). Default:
  like the main menu (orc arrow, hidden through fades), and no pointer on any video.
- **Network wait.** The length of the retry period that brings up the watch wasn't measured.
- **Text placement.** The exact vertical pixel of the bar text (centred, with a 2-pixel offset in the program's
  arithmetic) and the font's baseline belong to the drawing spec.
- **Bar font.** Which font the bar text uses is set by the dialog framework, not by the item drawing, and wasn't
  traced.
- **Input rate.** The 20 ms tick is a minimum; how often input is read (and so the real tick length) depends on
  the main loop, which wasn't measured.

