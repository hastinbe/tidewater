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
- **Changes:** 2026-10-15, round-72 errata (`ERRATA-round72.md`, C1 and C3; observed on the running game,
  Enhanced): the version line's pixel position and colours; no pointer seen on the campaign finale (ToD human) while
  its text hadn't started scrolling. Reviewed by a spec-review session, 2026-10-15.
- **Changes:** 2026-10-15, round-73 errata (`ERRATA-round73.md`, M1 to M4; observed on the running game, Enhanced,
  backed by game data and static study): new section "Menu screens" names the menu picture
  (`Art\TitleMenu_BNE.pcx`) and its palette, which menu screens share it, the small dialogs' panel, and the button
  and text colours; the version line's font is `Art\font10x.fnt` and its face has a grey shade (the "Version line
  font" open question is closed); the menus' pointer row names the palette; the round-69 vector for a menu button
  that changes screen now names only buttons that fade (Show Credits, Custom Scenario); open questions on the
  network and Battle.net pictures, the button face, label placement, the pointer during menu switches without a fade
  and unused palette entries. Reviewed by a spec-review session, 2026-10-15.
- **Changes:** 2026-10-15, round-74 errata (`ERRATA-round74.md`, B1 to B5; observed on the running game, Enhanced,
  backed by game data and static study): the button faces are frames of `Art\orc.grp`; the rings' entries; the
  label placement rule, pressed buttons and headings; the campaign menu's layout; keyboard focus (Tab, Shift+Tab,
  Enter); the version line's shade 3 default. The "Menu button face" and "Menu label placement" open questions are
  closed. Reviewed by a spec-review session, 2026-10-15: the frame table corrected against the game data (four sizes
  have no grey disabled frame; frames 0 to 2 have no rings; pressed frames of three sizes have no 239 ring); the
  static reading that the 247 outline marks the default button rather than the disabled state is held as an open
  question with a trace request, so round 73's observed rule stays the default; focus on in-game dialogs' default
  buttons added from the round-74 captures.
- **Changes:** 2026-10-15, round-75 errata (`ERRATA-round75.md`, U3 to U7; observed on the running game, Enhanced):
  the 247 outline marks the dialog's default button, enabled or disabled, and not a disabled button; the focus each
  in-game settings, Options, Help and Tips dialog opens with; dragging off a held button and back; Tab over sliders,
  checkboxes, radio buttons, lists and text fields, with their focus marks; the label rule checked on a 106 × 28
  button and a 368-wide heading. The open questions on the 247 outline and on keyboard focus in other dialogs are
  closed; the one on other button sizes is narrowed.

- **Changes:** 2026-10-21, round 89 (B-057 multiplayer, `ERRATA-round89-multiplayer.md`; reviewed by a spec-review
  session, 2026-10-21): pointer rule 1's watch comes after more than two retry periods of 1,000 ms (static study; the
  retry period's open question is answered, MP-L4 confirms); hotkey letters need the control's hotkey mark (static
  study); the flow-screens open question narrowed (Direct Connect reaches the lobby). Held: static study's arrow keys
  in a list (new open question, MM8); the stated "arrow keys do nothing" stays the default.

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

1. **Waiting for the network:** in multiplayer, once the game has asked a missing player to resend more than once
   in the current wait, that is after more than two retry periods (2 s with the default 1,000 ms period;
   `net/lockstep.md`, "Stalls"; static study), the watch.
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
| Introduction video | none (hidden) | | yes (other videos: not checked for a pointer, see "Open questions"; the finale has its own row) |
| Title picture | none (hidden) | | yes |
| Main menu, single-player menu, campaign menu, custom-scenario setup, load-game screen, the multiplayer menus up to and including the connection-method screen (Enhanced or Classic choice, connection method, direct-connection setup) and their small dialogs (name entry, "Unable to locate IPX network") | orc normal arrow (`o_arrow.cur`) for every race and campaign | the menu picture's palette: all 256 entries of `Art\TitleMenu_BNE.pcx` ("Menu screens") | yes (round 73: exact pixel match on every one of these screens) |
| Act screen | none (hidden) | | yes |
| Mission briefing | 3D arrow (`arrow_3d.cur`) | the briefing picture's palette | yes |
| Score screen | 3D arrow (`arrow_3d.cur`) | the score picture's palette | yes (human victory) |
| Credits (picture `Art\credits.pcx`) | 3D arrow (`arrow_3d.cur`) | the credits picture's palette | yes |
| Campaign finale | none seen | | partly (round 72: the ToD human finale, `art\humanv.pcx`, for about 4 minutes while its text hadn't started scrolling, with the mouse moved to three places; the other finales, the scroll and the finale's credits page not seen; see "Open questions") |
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
- It stays hidden through the fade to black, any black gap, and the next screen's fade-in. On the main menu the
  buttons are also not drawn during the fade-in: the fade-in shows the picture and the version line
  ("Warcraft II BNE v2.02", bottom left, drawn by the program over the picture), which fade in together. (The
  custom-scenario setup, the other menu screen with a fade, has no version line: "Menu screens".)
  The version line is drawn at a fixed place on the 640 × 480 menu picture (round 72, measured on the main menu):
  its text pixels fill x **60 to 211** and y **460 to 471**, so its top-left is **(60, 460)** from the
  picture's top-left. It is drawn in `Art\font10x.fnt` with the glyph cell's top-left at (60, 460) (round 73: all
  918 glyph pixels match). The face is near-white (255, 251, 243) with grey (111, 111, 111) edge pixels, in rows 460
  to 469, with black shading to its right and below ("Menu screens", "Text colours"). The original's display is
  always 640 × 480, so only this placement can be observed; the line is placed relative to the picture, wherever the
  picture is drawn. The buttons and the pointer appear on the frame that ends the fade-in.
- It appears, at full colour, when the next screen's fade-in has ended (on the frame that shows the screen at its own
  palette).
- Screens with no fade-in (the briefing, and the score screen after the game's 1 ms fade-out) or a 10 ms one (the
  game) show the pointer from their first frame.

So no frame ever shows a pointer in a partly faded palette, and no pointer at all (not even the watch) shows while
one screen fades into the next. Switches between the menus drawn on the main menu's picture have no fade ("Menu
screens"); whether the pointer hides during them wasn't observed (see "Open questions").

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

### Menu screens

**One picture, one palette.** The main menu and every menu reached from it without starting a game are drawn over
the picture `Art\TitleMenu_BNE.pcx` (640 × 480): a parchment with the game's logo at the top left and two drawn
figures. It is not the title picture `Art\Title_Rel_BNE.pcx`, which shows only before the main menu
(`game/campaign-flow.md`, "Startup"). When the menu screen opens, the screen's palette becomes **all 256 entries**
of the picture's own palette (the last 768 bytes of the file), unchanged: the game sets no entries of its own on
top (static study; every colour seen on these screens is exactly the file's entry, observed). Everything else on
these screens (buttons, text, panels, the orc arrow) is drawn with indices into that same palette. The picture
itself uses entries 0, 16 to 94 and 255; the interface uses others, for example:

| Entries | Colours (of `TitleMenu_BNE.pcx`) | Used by |
| --- | --- | --- |
| 114 to 118, 131, 144 to 150, 152, 153, 155, 161 to 166, 192, 194, 196, 198, 239 (game data: the file's 453 opaque pixels) | browns, the green ramp (27, 55, 0) to (147, 187, 43), the red ramp (47, 0, 0) to (163, 0, 0), golds, black | the orc arrow `o_arrow.cur` |
| 192, 197, 199, 200 | (95, 51, 0), (171, 143, 19), (211, 195, 31), (247, 227, 35) | gold button text |
| 104, 108, 246 | (111, 111, 111), (159, 159, 159), (255, 251, 243) | white text, the version line |
| 102, 105, 108 | (91, 91, 91), (123, 123, 123), (159, 159, 159) | the text of a disabled button |
| 251 | (255, 255, 0) | the focus ring |

These entries hold the same colours as the tilesets' palettes, but the title picture's palette has unrelated
colours there (for example entry 150 is (104, 72, 56) and entry 239 is (8, 8, 8)), which is why the arrow looks
scrambled if the menu is drawn in the title picture's palette.

**Which screens.** All observed on the running game (round 73, Enhanced, plain launch):

| Screen | Picture | Fade when it opens | Version line |
| --- | --- | --- | --- |
| Main menu | `TitleMenu_BNE.pcx`, loaded when the menu screen opens (static study) | 200 ms in (`ui/palette-effects.md`; seen after the custom-scenario setup and the credits) | yes |
| Single-player menu, campaign menu (Tides of Darkness / Beyond the Dark Portal, orc / human), load-game screen | the same picture, not reloaded (static study): the new menu replaces the old one's buttons and text | none | yes |
| Multiplayer: Enhanced or Classic choice, connection method (with gateway), direct-connection setup | the same picture, not reloaded | none | yes |
| Small dialogs over these menus: name entry, "Unable to locate IPX network." | the same picture, with the dialog's panel drawn over it (below) | none | yes |
| Custom-scenario setup ("Custom Game Setup") | `TitleMenu_BNE.pcx` again, reloaded with its full palette (static study; the picture and palette observed) | 200 ms out, then 200 ms in | **no** |

So every screen in the pointer table's "orc arrow" row shares the main menu's picture and palette; none has a
picture of its own. Leaving the custom-scenario setup (Cancel Game) fades out and back into the main menu.
Switching between the menus of the first three rows shows no darker frame at all (frames 10 to 20 ms apart for
main menu to multiplayer, campaign menu to single-player menu and single-player menu to load game; 50 ms apart for
load game back to the single-player menu; 300 ms apart for main menu to single-player menu).

**Panels of the small dialogs.** The name-entry and IPX dialogs are drawn on a 288 × 128 stone panel, the file
`Art\o144x64.bmp` (the orc version, matching the menus' orc interface race, "Pointers outside a game"; the human
`h144x64.bmp` matches 65 of its 36,864 pixels), with its indices in the menu palette. The file is two 16-bit
numbers (width, height) followed by width × height palette indices. Observed at (176, 256): every panel pixel not
covered by the dialog's text or button equals the file.

**Text colours.** Menu text uses the game's fonts, whose glyph pixels have shades 1 to 5. Shade 5 is the black
shadow (entry 0) in every case. Shades 1 to 4 map as follows:

| Text | Font | Shade 1 | Shade 2 | Shade 3 | Shade 4 |
| --- | --- | --- | --- | --- | --- |
| Button label | `font12x.fnt` | 200 (247, 227, 35) | 199 (211, 195, 31) | 197 (171, 143, 19) | 192 (95, 51, 0) |
| The label's hotkey letter, and the whole label while the pointer is over the button | `font12x.fnt` | 246 (255, 251, 243) | 246 | 108 (159, 159, 159) | 104 (111, 111, 111) |
| Disabled button label | `font12x.fnt` | 108 | 108 | 105 (123, 123, 123) | 102 (91, 91, 91) |
| Screen headings ("Tides of Darkness", "Load Game", "Custom Game Setup", "Direct Connection Setup") | `font12x.fnt` | 246 | 246 | 108 | 104 |
| Dialog text ("Unable to locate IPX network.") | `font12x.fnt` | 200 | 199 | 197 | 192 |
| Version line | `font10x.fnt` | 246 | 246 | 108 (default; this text has no shade-3 pixels, see "Open questions") | 104 |

Glyphs sit 1 pixel apart; a space leaves 10 pixels between the glyphs either side in `font12x` (as on the credits
screen, `game/campaign-flow.md`) and 8 pixels in `font10x`.

**Buttons.** A menu button's face is a picture from the game data, not drawn by the program: a frame of the
interface file `Art\orc.grp` (the menus use the orc interface race, as for the pointer; `Art\human.grp` holds the
human set in the same layout). These files are uncompressed: a 16-bit frame count, width and height, then 8 bytes
per frame (x, y, width, height as single bytes, then a 32-bit offset into the file), and each frame is width × height
palette indices. The button faces come in threes, one three per button size, in the order disabled, normal,
pressed. For some sizes the first frame is the same picture as the normal one, so those sizes have no grey disabled
face (game data):

| Frames | Size | Disabled frame | Outer ring | Second ring |
| --- | --- | --- | --- | --- |
| 0 to 2 | 106 × 28 | same as normal | none: all three frames use entries 6 to 15 only, and are the same in `Art\human.grp` | none |
| 3 to 5 | 128 × 20 | same as normal | 128 to 136 | 239; pressed 161 |
| 6 to 8 | 80 × 20 | same as normal | 128 to 136 | 239; pressed 161 |
| 9 to 11 | 106 × 28 | grey | 128 to 136 | 239 |
| 12 to 14 | 164 × 28 | grey | 128 to 136 | 239 |
| 15 to 17 | 224 × 28 (the main, single-player and campaign menus) | grey | 128 to 136 | 239 |
| 48 to 50 | 80 × 15 | same as normal | 128 to 136 | 239; pressed 161 |

The main menu's normal face is frame 16 pixel for pixel, apart from the label (observed: 5,080 of 6,272 pixels;
the other 1,192 are exactly the label's glyph pixels). The rings are 1 pixel wide; entry 239 is black. Inside the
rings, the 224 × 28 faces use entries 160 to 165, 210 and 211 (normal, frame 16), 97 to 108 (disabled, grey,
frame 15) and 161 to 163 and 211 (pressed, frame 17). The other sizes use entries from the same ranges, plus 166
in frames 3 and 4, 96 in the grey frames 9 and 12, and 160 or 210 in some pressed frames (game data). Tidewater
draws the frames as they are, so these lists only describe them. Which size each other dialog's buttons use comes
from its dialog file.

Over the face the program draws:

- **Default button.** The dialog's **default button** (the dialog file marks it) has a 1-pixel outline in entry 247
  ((163, 163, 167) on the menu picture's palette, (160, 160, 164) in a game) instead of its outer ring, enabled or
  disabled. Every other button keeps its frame's own ring, disabled ones included. Observed: Load on the load-game
  screen, enabled with a saved game listed and the focus tabbed to Cancel or the list, has the 247 outline (round
  75), as it does disabled with no saved games (round 73); OK on the Tips dialog with the focus on Next Tip, and Save
  on the save dialog, disabled with no name typed and enabled with one, have it too. Delete on the save dialog and Up
  on the Help dialog, disabled but not the default, keep their frame's ring (round 75).
- **Focus.** The **focused** button gets a 1-pixel outline in entry 251, (255, 255, 0), over its outer ring. It shows
  on a focused default button too: Quit to Menu on the in-game quit-to-menu confirmation, which is the dialog's
  default button, has the full 251 ring (round 74 captures). The ring stays on the focused button when the pointer
  is over another one. Seen focused when each menu opened: the first button on the main, single-player, campaign
  and Enhanced-or-Classic menus; Start Game on the custom-scenario setup; Cancel on the load-game screen with no
  saved games, Load with a saved game listed; OK on the name entry. In a game, the dialog's default button: Return
  to Game on the game menu, Previous on End Scenario, and Quit to Menu on the quit-to-menu confirmation (round 74
  captures); Previous on the Options and Help menus, OK on the Sound, Speeds, Preferences, Help and Tips dialogs
  (round 75 captures; the Options menu reopened from a settings dialog is focused on Previous again). The save
  dialog opens focused on Cancel, its default button (Save) being disabled while no name is typed (round 75).
- **Label.** The label text (below).

**Pressing.** While the mouse button is held down on a menu button, its pressed frame is drawn (observed: frame 17
on Show Credits, 4,755 of 6,272 pixels with the label moved) and the label moves 2 pixels right and 2 down. Pressing
a button also gives it the focus. Releasing outside the button does nothing, and the focus stays on it. While the
mouse button is still held, moving the pointer off the button draws it normal again (its normal frame, label back in
place); moving back onto it draws the pressed frame again, and releasing there chooses it. Passing over another
button while held doesn't press that one (observed on Single Player Game, round 75).

**Label placement.** A button's label text in its dialog file starts with the button's hotkey letter, which isn't
drawn; in the rest, byte 4 switches to the white shade map and byte 1 back to the normal one (so "Orc Campaign" on
*Tides of Darkness* has a white "O", and on *Beyond the Dark Portal* a white "r"). With the font's glyphs:

- **Text width** W = the sum, over the label's characters, of (the glyph's x offset + its width + 1); a space adds
  floor(the font's widest glyph / 2) + 1 (9 in `font12x`). The 1-pixel gap after the last glyph counts.
- **Button labels:** the text cell's top-left is x = left + floor((width + 1) / 2) + 1 − floor(W / 2),
  y = top + floor((height + 1) / 2) + 1 − floor(font height / 2), where left, top, width and height are the
  button's. For a 224 × 28 button in `font12x` (height 17) that is x = left + 113 − floor(W / 2), y = top + 7.
  Pressed, add 2 to both.
- **Headings** (a dialog's static text, such as the campaign menu's "Tides of Darkness"): x = left + floor((width −
  1) / 2) − floor(W / 2), y = top, with no vertical centring (measured on 640-wide headings only).

Observed on 14 labels of the main, single-player and campaign menus (both odd and even W) and the two campaign
headings: every one sits exactly where the rule puts it (the best-fitting cell, most with every glyph pixel
exact). The round-73 "Load Game" (W 93) at 275 is this rule; plain centring would give 274. Round 75 checked other
sizes on the load-game screen: "Cancel" (W 52) on a 106 × 28 button at (402, 408) sits at (430, 415), and the
heading "Load Game" (W 93) on a 368-wide rectangle at (136, 208) at (273, 208), both as the rules give, with every
glyph pixel exact.

**Keyboard focus.** On the main menu, the load-game screen and the in-game dialogs (observed):

- **Tab** moves the focus to the next control in the dialog file's order, and from the last back to the first;
  **Shift+Tab** moves it back, and from the first to the last. Tab stops on buttons, sliders, checkboxes, each radio
  button on its own, lists and text fields; it skips headings, plain text and hidden controls (the Sound dialog's
  3-D sound and player-message checkboxes are hidden in a single-player game). Moving the focus doesn't change a
  checkbox or a radio button (round 75).
- **Focus marks** (round 75): a button or text field gets the 251 outline over its rectangle's edge; a slider gets a
  251 rectangle as wide as the slider and 2 pixels taller (212 × 20 for a 212 × 18 slider); a list gets a 251
  rectangle one pixel inside its left, top and right edges, around its rows (298 × 146 for the load-game screen's
  300 × 164 list); a checkbox or radio button gets a 1-pixel rectangle in entry 247 around its label text, not
  around the box. Typing into the save dialog's name field moves the focus to the field.
- Observed orders (round 75): Tips, OK, Next Tip, the checkbox; Sound, OK, Cancel, the two sliders, the four shown
  checkboxes; Speeds, OK, Cancel, the three sliders; Preferences, OK, Cancel, the six radio buttons, the checkbox;
  load-game screen, Load, Cancel, the list.
- The arrow keys do nothing (observed on the main menu; for a list, see "Open questions", "Arrow keys in a list").
  Moving the pointer over a button doesn't move the focus.
- A button label shows a hotkey letter only when its control is marked in the dialog file as having a hotkey; a label
  without the mark (the join wait's Cancel, `ui/multiplayer-menus.md`) is drawn whole and has no hotkey (static
  study, round 89).
- **Enter** activates the focused button, as a click on it does (for example the focus on Multi Player Game and
  Enter opens the multiplayer menu; Enter on the quit-to-menu confirmation in a game confirms it).
- Each time the main menu opens, the focus is on its first button again.

**Main menu and campaign menu.** The main menu's five buttons are 224 × 28 at x 208, y 240, 276, 312, 348 and 384, as
the dialog file `rez\MainMenu.bin` lists them; their hotkeys are S, M, R, C and x (Single Player Game, Multi Player
Game, Replay Introduction, Show Credits, Exit Program). The campaign menu (`rez\newcmpgn.bin`, game data, observed):

| Item | Kind | Rectangle (x, y, width × height) | Hotkey | Label cell (observed) |
| --- | --- | --- | --- | --- |
| "Tides of Darkness" | heading, white heading map | 0, 216, 640 × 28 | — | (241, 216) |
| "Orc Campaign" | button | 208, 240, 224 × 28 | O | (262, 247) |
| "Human Campaign" | button | 208, 276, 224 × 28 | H | (248, 283) |
| "Beyond the Dark Portal" | heading, white heading map | 0, 324, 640 × 28 | — | (219, 324) |
| "Orc Campaign" | button | 208, 348, 224 × 28 | r | (262, 355) |
| "Human Campaign" | button | 208, 384, 224 × 28 | u | (248, 391) |
| "Previous Menu" | button | 208, 420, 224 × 28 | P | (257, 427) |

The four campaign buttons choose ToD orc, ToD human, BDP orc and BDP human (`game/campaign-flow.md`); Previous Menu
returns to the single-player menu. Full layouts of the other menus are outside this spec.

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
| A menu button that changes screen clicked (Show Credits; Custom Scenario on the single-player menu) | Pointer gone from the first frame of the 200 ms fade-out until the next screen's fade-in has ended; then the orc arrow at full colour |
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
| Main menu, pointer at (320, 240), after the fade-in (round 73) | Every pixel outside the buttons (x 208 to 431, y 240 to 411), the version line and the pointer equals `TitleMenu_BNE.pcx` drawn with its own palette; all 453 orc-arrow pixels equal that palette's entry at the arrow's index (for example entry 150 → (63, 131, 11), 239 → (0, 0, 0)); none matches the title picture's palette |
| Main menu, version line (round 73) | `font10x.fnt` "Warcraft II BNE v2.02", cell top-left (60, 460), 1 pixel between glyphs, 8 at the spaces: 918 of 918 glyph pixels exact; shades 1 and 2 → (255, 251, 243), 4 → (111, 111, 111), 5 → (0, 0, 0); text pixels x 60 to 211, y 460 to 471 |
| Main menu, Single Player Game (round 73) | Same picture, no darker frame (frames 300 ms apart); buttons New Campaign (focused, yellow ring), Load Game, Custom Scenario, Previous Menu; the version line stays |
| Main menu, Multi Player Game; campaign menu, Previous Menu; single-player menu, Load Game (round 73) | Same picture and version line; no darker frame in frames 10 to 20 ms apart, so no fade |
| Single-player menu, pointer off the buttons (round 73) | "Load Game" label in `font12x`, cell top-left (275, 283) on the button at x 208 to 431, y 276 to 303: 671 of 671 glyph pixels exact, "L" in the white map, "oad Game" in the gold map |
| Custom-scenario setup open (round 73) | 200 ms fade-out and 200 ms fade-in; the picture is `TitleMenu_BNE.pcx` in its own palette; no version line (the picture shows at x 60 to 211, y 460 to 471); "Start Game" focused |
| Custom-scenario setup, pointer over Select Scenario (round 73) | Its whole label in the white map; "Start Game" keeps the yellow ring and its gold label with a white "S" |
| Load-game screen, no saved games (round 73) | "Load" button grey with label shades 108, 108, 105, 102; "Cancel" gold with a white "C" |
| Multiplayer, IPX network, name entry, OK (round 73) | "Unable to locate IPX network." on `Art\o144x64.bmp` at (176, 256) in the menu palette; text `font12x`, gold map; the orc arrow exact |
| Campaign menu, pointer off the buttons (round 74) | Labels at the cells in the campaign-menu table; headings "Tides of Darkness" (W 156) at (241, 216) and "Beyond the Dark Portal" (W 201) at (219, 324) in the white heading map |
| Main menu, labels (round 74) | Single Player Game (W 166) at (238, 247), Multi Player Game (160) at (241, 283), Replay Introduction (168) at (237, 319), Show Credits (114) at (264, 355), Exit Program (116) at (263, 391) |
| Main menu, mouse button held on Show Credits (round 74) | Frame 17 drawn; label at (266, 357); the focus ring moves to Show Credits; released outside the button: nothing opens, the ring stays |
| Main menu opened, Tab pressed 5 times (round 74) | Focus ring on Multi Player Game, Replay Introduction, Show Credits, Exit Program, then Single Player Game again |
| Main menu opened, Shift+Tab (round 74) | Focus ring on Exit Program |
| Main menu, Down or Up arrow (round 74) | The focus ring doesn't move |
| Main menu, Tab, then Enter (round 74) | The multiplayer menu (Enhanced or Classic) opens |
| Main menu, normal button face (round 74) | Equals `Art\orc.grp` frame 16 at (208, y) except the label's glyph pixels |
| Main menu, mouse button held on Single Player Game, pointer moved off it, then back, then released (round 75) | Pressed frame with the label 2 px right and down; off the button: drawn normal, label back; back on: pressed again; released: the single-player menu opens |
| Load-game screen, one saved game listed (round 75) | Focus ring (251) on Load; Tab: ring on Cancel and Load's outer ring in entry 247 (163, 163, 167); Tab: a 251 rectangle 298 × 146 at (171, 233) inside the list, Load still in 247; Tab: back on Load |
| Load-game screen, labels (round 75) | "Cancel" (W 52) on the 106 × 28 button at (402, 408): cell (430, 415); "Load" (W 39) on the 224 × 28 button at (154, 408): cell (248, 415); heading "Load Game" (W 93) on the 368-wide rectangle at (136, 208): cell (273, 208) |
| Tips dialog at the start of a mission (round 75) | Ring on OK; Tab: ring on Next Tip, OK's outer ring (160, 160, 164) (entry 247); Tab: a 247 rectangle round "Show Tips at Startup", no ring on a button; Tab: ring on OK |
| Sound dialog opened from the Options menu, single-player game (round 75) | Ring on OK; Tab order: Cancel, music slider (251 rectangle 212 × 20), digital slider, then four checkboxes (247 rectangle round each label), then OK |
| Save dialog opened, no name typed (round 75) | Ring on Cancel; Save (disabled, default) has the 247 outline; Delete (disabled) keeps its frame's ring. After typing a name: the 251 ring on the name field, Save enabled with the 247 outline |
| Help dialog (Keystroke Help) opened (round 75) | Ring on OK; Up (disabled, not the default) keeps its frame's ring |
| Orc campaign mission lost (defeat dialog up) | Orc normal arrow (`o_arrow.cur`), game palette (round 70, seen; shape match) |
| Orc defeat, score screen | Picture `Art\olosglu.pcx`, 3D arrow, exact pixel match (round 70, seen) |

## Open questions

- **The large-number placeholder.** Values of 1,000,000 or more show a fixed six-character word from the program,
  not from a string table. The implementation needs its own placeholder text.
- **The two extra multiplayer bar areas** (`infobox2.bin` items 5 and 6) weren't traced.
- **Unused pointers.** A second arrow (`h_arrow2.cur`, `o_arrow2.cur`) and a cross (`cross.cur`) are loaded. No
  use of them was found in a game or on the flow screens. The 3D arrow is the briefing, score and credits pointer
  (above); it is also set when the game screen opens and replaced at the first input check.
- **Flow screens not observed.** Battle.net wasn't reached: it needs a live connection to an outside service, which
  wasn't attempted. The oracle has no IPX network, but Direct Connect over a linked pair of serial ports reaches the
  lobby (round 89); the multiplayer menus and the lobby are in `ui/multiplayer-menus.md` and
  `ui/game-setup-lobby.md`. The other videos weren't checked for a pointer (the introduction showed none). The campaign finale was
  reached in round 72 (ToD human, by the finale cheat): no pointer was drawn, even when the mouse moved, but its
  scrolling text never started in the oracle (`game/campaign-flow.md`, "Open questions"), so the pointer once the
  text scrolls, and on the finale's credits page, is unobserved. Default: no pointer on the finale; like the main menu
  elsewhere (orc arrow, hidden through fades), and no pointer on any video.
- **Network game setup and Battle.net pictures.** Static study only (not reachable in the oracle): the game setup
  screen of a network game uses `Art\Chat_IPX.pcx` instead of the menu picture, and the Battle.net screens use
  `Art\Chat_BNET.pcx` and the `glue\battle.net\` pictures. `Chat_IPX.pcx`'s palette equals the menu picture's at
  entries 0 to 15 and 96 to 255 (game data), so the orc arrow and the button colours would look the same there.
  Whether these screens fade, show the version line or use the orc arrow wasn't observed. Default: the static-study
  pictures; nothing in Tidewater needs these screens yet.
- **Version line shade 3.** The version line's text has no shade-3 pixels, so the colour that shade would take
  can't be seen. Its other shades equal the white heading map's. Default: 108, as in that map.
- **Other button sizes.** The placement rule was measured on 224 × 28 and 106 × 28 buttons and on 640- and 368-wide
  headings (round 75). The 128 × 20, 80 × 20, 164 × 28 and 80 × 15 sizes weren't measured. Default: the same
  formulas for them.
- **Pointer during menu switches without a fade.** Whether the pointer hides, even for a frame, when one menu on the
  main menu's picture replaces another wasn't settled by the round-73 captures. Default: the orc arrow stays shown,
  since no fade runs.
- **Unused palette entries.** Every colour seen on the menu screens is the menu picture's own entry; entries that
  nothing on these screens draws can't be observed. Default: all 256 entries come from the picture, as static study
  shows ("Menu screens").
- **Network wait.** The retry period (1,000 ms by default) and the watch after more than two of them are static study
  (`net/lockstep.md`); trace request MP-L4 there confirms them.
- **Arrow keys in a list.** Held (round 89 review): static study reads a list (the connection list, the load-game
  list) as moving its selection with Up or Left, Down or Right, Page Up, Page Down, Home and End
  (`ui/multiplayer-menus.md`). Default: the arrow keys do nothing, as stated above. Trace request MM8 in
  `ui/multiplayer-menus.md`.
- **Text placement.** The exact vertical pixel of the bar text (centred, with a 2-pixel offset in the program's
  arithmetic) and the font's baseline belong to the drawing spec.
- **Bar font.** Which font the bar text uses is set by the dialog framework, not by the item drawing, and wasn't
  traced.
- **Input rate.** The 20 ms tick is a minimum; how often input is read (and so the real tick length) depends on
  the main loop, which wasn't measured.

