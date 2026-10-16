# UI: unit information panel

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 11"), the panel
  layout in `statport.bin`, the frame pictures `hframe.grp` / `oframe.grp`, the string table `stat_txt.tbl`, the
  default animation scripts `script.bin`, and the public PUD specification text (unit type numbers, unit data fields).
  Not yet confirmed against the running game.
- **Changes:** 2026-10-16 (round 76, B-038 build/train consolidation, `ERRATA-buildtrain-consolidation.md`, reviewed):
  the bar's value is the building's mana byte, so a job shows p = 1 from its start until its first run (the byte is
  traced at 1 then; that the bar reads the byte is static study); the "job at 0 steps" vector changes from p = 0, 2 px
  to p = 1, 3 px.

## Behaviour

The information panel is the 176 × 176 box on the left of the screen, above the command buttons
(`ui/command-buttons.md`). It shows the selected units: for one unit its portrait, name, hit points, level and
statistics; for several units a grid of their icons. All positions below are in pixels relative to the panel's
top-left corner, which is at screen position (0, 160). Strings are named by their `stat_txt.tbl` entry number,
counting from 1.

"Own" means owned by the local player. The **primary unit** is the one the selection treats as first (the one
`ui/command-buttons.md` uses).

### Panel elements

| Element | Position (x1, y1)–(x2, y2) | Used for |
| --- | --- | --- |
| Icon grid, 3 × 3 slots of 54 × 53 | columns x = 5, 61, 117; rows y = 5, 60, 115 | units; slot 0 (top left) is the portrait when one unit is selected |
| Name, line 1 and line 2 | (60, 10)–(168, 23) and (60, 25)–(168, 38), centred | one unit's name |
| Level line | (60, 40)–(168, 53), centred | units only |
| Hit point text | (9, 60)–(62, 69), centred, under the portrait | own units and buildings |
| Six statistic rows | labels in (9, y)–(102, y + 13), values in (105, y)–(167, y + 13), for y = 72, 87, 102, 117, 132, 147 (rows 0 to 5) | statistics, production, food, resources left |
| Production icon | (107, 78)–(160, 123) | the unit, spell or upgrade a building is working on |
| Progress bar | (12, 153)–(163, 166) | own building that is working or under construction |

Text uses the game's normal small font. The two name lines and the level line are centred. Values are drawn from the
left edge of their area. See the open questions for the labels' alignment and the hit point text's font.

### Background frame

The background is one of four 176 × 176 frames from the local player's race's frame picture: `hframe.grp` for
human and `oframe.grp` for orc.

| Frame | Picture |
| --- | --- |
| 0 | no border |
| 1 | border |
| 2 | the same picture as frame 1 |
| 3 | border and a box for the progress bar |

The frame is chosen whenever the panel is rebuilt, by the first rule that applies:

1. No selection, several units, or one unit that isn't own: frame 0.
2. One own building (unit flag bit 5) that is **working** or **under construction** (`ui/command-buttons.md`,
   "Building busy"): frame 3.
3. Otherwise by the type's **panel kind** (table under "Data"): kind 0 gives frame 0, kind 1 frame 1, kind 2 (spell
   casters) frame 2, and kind 3 (buildings) frame 1, or frame 3 if the unit is working.

### What the panel shows

The panel is rebuilt whenever the selection changes. Grid slots are then assigned in two passes over the new
selection: first, every unit that was already in a slot goes back to that same slot; then each remaining unit takes
the lowest-numbered empty slot. The **primary unit** is the unit in the lowest-numbered filled slot. Empty slots are
hidden.

**No selection or several units.** Each slot with a unit shows that unit's icon with its health bar (below). Empty
slots show nothing. The name, level, hit point text, statistic rows, production icon and progress bar are all hidden.

**One unit.** The unit moves to slot 0, which shows its icon (the portrait) with its health bar, and slots 1 to 8
are emptied, except for an own **transport** (unit flag bit 10, owned by the local player): passenger place 1 to 6
fills slot 3 to 8 (the second and third rows) with that passenger's icon, and an empty place leaves its slot empty.
A transport that isn't own shows no passengers. The text depends on the unit's
**panel class**, which comes from its type (table under "Data"); the rules for each class follow. In every class:

- **Name:** the type's name entry (table under "Data"), split over the two name lines (see "Name lines").
- **Hit point text:** only for an own unit or building, and only if both its hit points and its type's maximum hit
  points (1 if the unit data says 0) are at most 9999: "HP/max", for example `60/60`.
- **Progress bar:** only for an own building that is working or under construction (see "Progress bar").
- A class not listed for a unit or building shows only these three, and for units the level line.

**Clicking an icon** (on button release):

- **Several units selected:** the clicked unit becomes the only selected unit. With Shift held, the clicked unit is
  removed from the selection instead and the others stay selected.
- **One unit selected, a transport, and the click is on any icon other than the transport's own:** a request to unload
  that one passenger is sent (see the open questions). The click itself checks only that the selected unit is a
  transport. It still only happens for own transports, because only they show passengers and empty slots can't be
  clicked.
- **One unit selected, any other click:** the view centres on the unit, if it is own or it isn't invisible.

**Pointing at an icon** shows the name entry of that slot's unit (table under "Data") on the status line. Moving off
the icons clears the status line. The status line itself is outside this spec.

### Name lines

The name is measured in the panel font.

1. If it is at most 109 pixels wide, line 1 is empty and line 2 holds the name.
2. Otherwise, if the name has a space among its first 20 characters and the part before the first space is at most
   109 pixels wide, line 1 holds that part and the rest (after the space) goes to line 2.
3. Otherwise the name is hyphenated: line 1 gets the longest start of the name that, followed by a hyphen, is less
   than 109 pixels wide, with a hyphen after it, and the rest goes to line 2. Line 1 has at most 18 characters before
   the hyphen.
4. Line 2 holds at most 19 characters. Characters are removed from its end until it is at most 109 pixels wide.

### Level line

Units of every unit class show "*entry 411* *N*" (a space between), where N is 1 plus the owner's levels below,
counted for the unit's owner. Buildings have no level line. Levels are those of `economy/tech-tree.md`: an upgrade
**group** level (0 to 2 for two-level lines; 0 or 1 for the one-step upgrades), or 1 for each spell bit that is
known. "Weapons upgradable" and "armour upgradable" are the type's unit data fields of those names.

| Class | Added to the base 1 |
| --- | --- |
| Melee | group 1 (swords and axes) if weapons upgradable; group 2 (shields) if armour upgradable; and for the four spell casters below |
| — mage (0x0A) | 1 for each of spell bits 4, 6, 7, 8, 9 known (flame shield, slow, invisibility, polymorph, blizzard) |
| — death knight (0x0B) | 1 for each of spell bits 13, 15, 16, 17, 19 known (raise dead, whirlwind, haste, unholy armour, death and decay) |
| — paladin (0x0C) | 1, plus 1 for each of spell bits 1 and 3 known (healing, exorcism) |
| — ogre-mage (0x0D) | 1, plus 1 for each of spell bits 11 and 18 known (bloodlust, runes) |
| Siege | group 6 (catapult or ballista upgrades) |
| Archer | groups 7, 10, 9, 8 and 0 (ranger training, marksmanship or regeneration, scouting, longbow or lighter axes, arrows or spears), all regardless of type; group 2 if armour upgradable |
| Sea | group 5 (no default upgrade uses it); group 3 (ship cannons) if weapons upgradable; group 4 (ship armour) if armour upgradable |
| Transport | groups 5 and 4, regardless of the upgradable fields |
| Bare | nothing (always level 1) |

### Statistic rows of own units

A unit of another player shows no statistic rows. For an own unit the rows are:

| Row | Label | Value |
| --- | --- | --- |
| 0 | entry 412 | the type's armour |
| 1 | entry 413 | damage, "min-max" (below) |
| 2 | entry 414 | the type's attack range |
| 3 | entry 415 | the type's sight (below) |
| 4 | entry 416 | the type's speed (below) |
| 5 | entry 417 | the unit's mana, as a bar (below); only for types with unit flag bit 17 (casters) |

All values are the **type's** default unit data values (from the map's unit data when it has one), not the unit's own
modified values. Upgrades show as a **bonus** after the value instead: "+B" written straight after the number, drawn
in the font's alternate colour (the text switches to colour code 4 for the bonus and back to code 1 after it). A bonus
of 0 isn't shown. The bonus is worked out in 8 bits (it wraps above 255).

| Class | Rows shown | Damage bonus | Armour bonus | Other bonuses |
| --- | --- | --- | --- | --- |
| Melee | 0, 1, 2, 3, 4; 5 for casters | 2 × group 1, if weapons upgradable | 2 × group 2, if armour upgradable | none |
| Siege | 0, 1, 2, 3, 4 | 15 × group 6, if weapons upgradable | 2 × group 2, if armour upgradable | none |
| Archer | 0, 1, 2, 3, 4 | if weapons upgradable: group 0; plus 3 for a ranger (0x12, not the berserker) whose owner has marksmanship | 2 × group 2, if armour upgradable | range: +1 for a ranger or berserker (0x12, 0x13) whose owner has longbow or lighter axes; sight: see below |
| Sea | 0 (only if armour upgradable), 1, 2, 3, 4 | 5 × group 3, if weapons upgradable | 5 × group 4 | none |
| Transport, no passengers | 0, 1, 2, 3, 4 | 5 × group 3, if weapons upgradable | 5 × group 4, if armour upgradable | none |
| Transport with passengers | none; the hit point text is hidden too | | | |
| Bare | none | | | |

**Damage.** With B the type's basic damage and P its piercing damage: max = B + P, and
min = max(0, floor((B − 30) / 2)) + floor((P + 1) / 2). If min equals max the value is that one number, otherwise
"min-max".

**Sight.** The type's sight radius, except that a sight of 1 shows as 0. A ranger or berserker whose owner has the
matching scouting upgrade gets the bonus 9 − (its type's sight).

**Speed.** A fixed number per type, worked out once when the program starts from the default animation scripts
(`core/animation-scripts.md`):

- D(t) is the **move duration** of type t: walk its move animation (animation 3) from the start, following
  continue-at (opcode 3) jumps; add 1 for each step-done (opcode 0) and the argument of each wait (opcodes 1, 7, 8, 9,
  12); stop after the second step-done. Other opcodes are skipped with their arguments. (This is the same walk as the
  attack duration of the weight in `ai/attack-ai.md`, "The threat map", on a different animation.)
- For a type whose unit data movement type is air or naval, D(t) is halved (rounded down).
- speed(t) = floor(10 × D(0) / D(t)), kept to 8 bits; 0 when D(t) is 0. D(0) is the footman's.

**Mana bar.** The value area of row 5 shows a one-pixel outline (palette colour 246) around the area, a second
outline one pixel inside it, and a bar from (107, 149) to (165, 158). The bar's left floor(59 × mana / 255) pixels are palette colour
186 and the rest palette colour 99. The mana number is drawn centred on the bar.

### Buildings

Buildings show the name, the hit point text and the progress bar as above, and no level line. The rows below appear
only for the cases given; in every other case the rows are empty. Each "with icon" case also hides the value column
(rows 0 to 5) and shows the production icon, in the place of the values.

| Class | Case | Rows |
| --- | --- | --- |
| Training (barracks, inventor or alchemist, aviary or roost), shipyard, mage tower, hall | own, complete, working on a **train** job | row 1 label entry 422; production icon: the trained type's icon |
| Church or altar, mage tower | own, complete, working on a **spell** job | row 1 label entry 425; production icon: the spell's icon |
| Lumber mill, smith (blacksmith or foundry) | own, complete, working on an **upgrade** job | row 1 label entry 423; production icon: the upgrade's icon |
| Hall (town hall to fortress), scout tower | own, complete, working on a **building upgrade** | row 1 label entry 423; production icon: the new type's icon |
| Hall | own, complete, not in one of the cases above | row 0: entry 427, empty value; row 1: entry 428, 100, bonus +20 if the owner has a completed castle or fortress, else +10 if a completed keep or stronghold; row 2: entry 429, 100, bonus +25 if the owner has a completed lumber mill; row 3: entry 430, 100, bonus +25 if the owner has a completed refinery |
| Lumber mill | own, complete, not working on an upgrade | row 1: entry 427, empty value; row 2: entry 429, 100, bonus +25 if the owner has a completed lumber mill |
| Refinery | own, complete and not working | row 1: entry 427, empty value; row 2: entry 430, 100, bonus +25 if the owner has a completed refinery |
| Farm | own, complete | row 0: entry 419, empty value; row 1: entry 420, the owner's food supply (counted as at most 200 in Enhanced mode); row 2: entry 421, the owner's food in use, in the alternate colour when it is greater than the supply shown |
| Gold mine | any owner | row 1: entry 426, the gold left (trips left × 100, `economy/harvest.md`) |
| Oil platform, oil patch | unless it is an own building that is working or under construction | row 1: entry 418, the oil left (trips left × 100) |
| Guard and cannon towers | own, complete | rows 0, 1, 2, 3 as for units: armour (2 × group 2 if armour upgradable), damage (2 × group 1 if weapons upgradable), range, sight |

Food supply and food in use are those of `economy/build-train.md`, "Food". The bonuses are the delivery bonuses of
`economy/harvest.md`. The production values use the same "+B" form as the unit bonuses.

**Production icon.** It is drawn like a command button (`ui/command-buttons.md`): the icon in the local player's
colour, with the button frame. For a spell or upgrade job the icon is the spell's or upgrade's icon from the upgrade
data, except that icon 108 is shown as icon 103. It is hidden whenever the panel is rebuilt.

### Progress bar

The bar shows the building's **shown progress** p, from 0 to 255, which is the value kept in the building's mana byte
(`economy/build-train.md`):

- under construction: the shown progress of `economy/build-train.md` ("Construction", step 5);
- working on a job: **1** from the job's start until its first run, then floor(255 × steps done / the job's total
  steps) (`economy/build-train.md`, "Job progress"; traced there, V17 and V52).

The left floor(152 × (p + 5) / 256) pixels of the bar area, at most all 152, are drawn in palette colour 149; the rest
shows the frame's box. Entry 434 is drawn centred on the bar. No number is drawn.

### Icons and health bars

Each grid icon is the unit type's icon (table under "Data") from the icon set of the map's terrain (`portrait.grp`
forest, `s_port.grp` winter, `l_port.grp` wasteland, `x_port.grp` swamp), drawn in the unit's colour inside a button
frame, 4 pixels in from the slot's top-left corner (5 while the icon is pressed).

The health bar is below the icon, for every unit including the portrait:

1. A strip 54 × 7 at (slot x, slot y + 46) is filled with the background colour.
2. The bar area is (slot x + 2, slot y + 46)–(slot x + 51, slot y + 50), 50 × 5. With HP the unit's hit points and M
   its type's maximum (1 if 0), its left floor(50 × HP / M) pixels (at most 50) are drawn in the colour below and the
   rest in palette colour 99.
3. The colour is chosen by q = floor(100 × HP / M): palette colour 149 if q ≥ 75, else 251 if q ≥ 50, else 249.

An icon is redrawn whenever its unit's hit points change.

### Updating

The checks below run every frame, and only while exactly one unit is selected. Each type has one check set (by
type number, from the public PUD specification). When any check finds a change, the panel's text is redrawn.

**Common checks** (part of every set except "none"):

- the unit's hit points change;
- its mana changes (for a building, its shown progress);
- it starts or stops being an own building that is working or under construction (the frame 3 condition of
  "Background frame").

For mobile units, including workers, tankers and transports, the first two checks cover every filled slot, not
only slot 0; for a transport that includes its passengers' slots. For buildings they look at the selected unit only.

| Types | Check set |
| --- | --- |
| Mobile units not listed below | Common checks only |
| 0x02, 0x03, 0x10, 0x11 (workers), 0x1A, 0x1B (tankers) | Common, plus the "carrying" state changes (this also refreshes the command buttons) |
| 0x1C, 0x1D (transports) | Common, plus any passenger place changes |
| 0x3A, 0x3B (farms) | Common, plus, for an own farm, the owner's food in use or the supply shown changes |
| 0x5C (gold mine), 0x56, 0x57 (oil platforms), 0x5D (oil patch) | Common, plus the amount left changes |
| 0x3C to 0x41, 0x44 to 0x53 (barracks, church and altar, scout towers, inventor and alchemist, aviary and roost, shipyards, town hall and great hall, lumber mills, foundries and smiths, mage tower and temple) | Common, plus the job target changes |
| 0x42, 0x43, 0x54, 0x55, 0x58 to 0x5B (keep, stronghold, castle, fortress), 0x60 to 0x68 | Common checks only. A keep or higher hall that switches straight from one job target to another redraws only when another check also fires |
| 0x30, 0x36, 0x5E, 0x5F, 0x69 | None: never redrawn by a check |

Separately from these checks, while any number of units are selected, an icon is redrawn whenever its unit's hit
points change.

## Data

**Panel table per unit type** (type numbers from the public PUD specification). "Icon" is the icon number in the
icon set. "Name" is the name entry; for every type not listed in the exceptions it is type number + 2.

| Types | Class | Panel kind |
| --- | --- | --- |
| 0x00, 0x01, 0x06, 0x07, 0x0E, 0x0F, 0x16, 0x17, 0x19, 0x23, 0x28 to 0x2F, 0x31 to 0x34, 0x37 | Melee | 1 |
| 0x0A to 0x0D, 0x15, 0x18 | Melee | 2 |
| 0x02, 0x03, 0x10, 0x11 (workers) | Melee | 1 |
| 0x04, 0x05 | Siege | 1 |
| 0x08, 0x09, 0x12, 0x13, 0x14, 0x35 | Archer | 1 |
| 0x1A, 0x1B (tankers), 0x1E to 0x21, 0x26, 0x27 | Sea | 1 |
| 0x1C, 0x1D | Transport | 1 |
| 0x22, 0x24, 0x25, 0x38, 0x39 | Bare | 1 |
| 0x30, 0x36, 0x5E, 0x5F, 0x69 | none | 0 |
| 0x3A, 0x3B | Farm | 1 |
| 0x3C, 0x3D, 0x44 to 0x47 | Training | 3 |
| 0x3E, 0x3F | Church or altar | 3 |
| 0x40, 0x41 | Scout tower | 3 |
| 0x42, 0x43, 0x65, 0x67, 0x68 | plain building | 3 |
| 0x64, 0x66 | plain building | 1 |
| 0x48, 0x49 | Shipyard | 3 |
| 0x4A, 0x4B, 0x58 to 0x5B | Hall | 3 |
| 0x4C, 0x4D | Lumber mill | 3 |
| 0x4E, 0x4F, 0x52, 0x53 | Smith | 3 |
| 0x50, 0x51 | Mage tower | 3 |
| 0x54, 0x55 | Refinery | 3 |
| 0x56, 0x57, 0x5D | Oil platform, oil patch | 3 |
| 0x5C | Gold mine | 1 |
| 0x60 to 0x63 | Guard and cannon towers | 3 |

**Icons:**

| Types | Icon |
| --- | --- |
| 0x00 to 0x0F | 2, 3, 0, 1, 16, 17, 8, 9, 4, 5, 14, 15, 10, 11, 12, 13 |
| 0x10 to 0x1F | 0, 1, 6, 7, 187, 189, 191, 194, 193, 190, 18, 19, 20, 21, 22, 23 |
| 0x20 to 0x2F | 24, 25, 0, 192, 0, 0, 26, 27, 28, 29, 30, 31, 195, 111, 188, 186 |
| 0x30 to 0x3F | 0, 36, 32, 33, 34, 35, 0, 114, 37, 115, 38, 39, 42, 43, 62, 63 |
| 0x40 to 0x4F | 60, 61, 56, 57, 58, 59, 72, 73, 48, 49, 40, 41, 44, 45, 52, 53 |
| 0x50 to 0x5F | 64, 65, 46, 47, 50, 51, 54, 55, 66, 67, 70, 71, 74, 79, 0, 0 |
| 0x60 to 0x69 | 75, 77, 76, 78, 81, 80, 82, 92, 93, 0 |

**Name entry exceptions:** 0x0E: 244; 0x10, 0x11: 4, 5 (as the peasant and peon); 0x1A, 0x1B: 245, 246; 0x1C, 0x1D:
247, 248; 0x22, 0x24, 0x25, 0x69: 5; 0x2D: 267; 0x3C, 0x3D: 249, 250; 0x48, 0x49: 251, 252; 0x4E, 0x4F: 253, 254;
0x50, 0x51: 261, 262; 0x52, 0x53: 255, 256; 0x54, 0x55: 257, 258; 0x56, 0x57: 259, 260; 0x60 to 0x63: 263 to 266;
0x68: 105 (as 0x67); 0x30, 0x36, 0x5E, 0x5F: none. Entries 244 to 266 are shorter names that fit the panel.

**Default speeds** (from the default `script.bin` and the default movement types):

| Speed | Types |
| --- | --- |
| 3 | critter |
| 5 | ballista, catapult, 0x22 |
| 6 | battleships, juggernauts |
| 7 | submarines, turtles |
| 8 | mages, death knights, Teron Gorefiend, Khadgar, Gul'dan, skeleton |
| 10 | footman, grunt, workers, archer line, Alleria, Zuljin, Grom, Danath, Korgath, tankers, transports, destroyers, 0x24, 0x25, 0x30, 0x36 |
| 11 | dwarven demolition squad, goblin sappers |
| 13 | knights, ogres, paladins, ogre-mages, Dentarg, Turalyon, Cho'gall, Lothar, Uther |
| 14 | gryphon riders, dragons, Kurdan and Sky'ree, Deathwing, daemon |
| 17 | flying machine, zeppelin |
| 42 | eye of Kilrogg |

**Constants:**

| Constant | Value |
| --- | --- |
| Name line width limit | 109 px |
| Hit point text limit | 9999 (hit points and maximum) |
| Health bar colours (palette) | 149 (≥ 75 %), 251 (≥ 50 %), 249 (below); empty part 99 |
| Progress bar | 152 px, colour 149, fill floor(152 × (p + 5) / 256) |
| Mana bar | 59 px, colour 186, empty 99, full at 255 |
| Bonus multipliers by group | 0: 1, 1: 2, 2: 2, 3: 5, 4: 5, 6: 15 (8: 1 for range) |
| Marksmanship display bonus | +3 (ranger only) |
| Scouting sight | 9 |

## Test vectors

| Input | Expected |
| --- | --- |
| Own footman, B 6, P 3, armour 2, no upgrades, weapons and armour upgradable | level 1; damage `2-9`; armour `2`; no bonus |
| Same, swords level 2 and shields level 1 | level 4; damage `2-9` then `+4`; armour `2` then `+2` |
| Damage with B 0, P 0 | `0` |
| Damage with B 40, P 1 | min 5 + 1 = 6, max 41: `6-41` |
| Own paladin, healing known, exorcism not, swords 1, shields 0, both upgradable | level 1 + 1 + 1 + 1 = 4 |
| Own mage, only fireball known | level 1 (fireball doesn't count) |
| Own death knight with haste and death and decay known | level 3 |
| Own ranger, arrows 2, ranger training, longbow, scouting, marksmanship, type sight 5, weapons upgradable | level 1 + 2 + 1 + 1 + 1 + 1 (+ group 2 if armour upgradable); damage bonus `+5`; range bonus `+1`; sight `5` then `+4` |
| Same, but a berserker with all its upgrades and spears 2 | damage bonus `+2` (no marksmanship +3) |
| Own ballista, ballista upgrades 2, weapons upgradable | level 3; damage bonus `+30` |
| Own battleship, not armour upgradable | no armour row |
| Own transport with no passengers | rows armour, damage, range, sight, speed; level 1 + ship armour level |
| Own transport with passengers in its first two places | portrait in slot 0; passengers in slots 3 and 4; name and level only; no hit point text, no rows |
| Grunt of another player | name, level line; no hit point text, no rows; frame 0 |
| Own unit with 10 000 maximum hit points | no hit point text |
| Health 45 of 60 | q = 75: colour 149; bar 37 px of 50 |
| Health 29 of 60 | q = 48: colour 249; bar 24 px |
| Own barracks training a footman | frame 3; row 1 label entry 422; production icon 2; value column hidden; progress bar |
| Own town hall, idle, owner has a keep and a lumber mill, no refinery | rows: 427; 428 `100+10`; 429 `100+25`; 430 `100` |
| Own lumber mill researching arrows | row 1 label entry 423; production icon from the upgrade data |
| Own farm, supply 9, 10 units using food | 420 `9`; 421 `10` in the alternate colour |
| Enhanced mode, supply 241 | 420 `200` |
| Gold mine with 3000 trips left | 426 `300000` |
| Job just started (0 of 120 steps, before its first run) | p = 1; bar 3 px |
| Job after its first run (1 of 120 steps) | p = 2; bar 4 px |
| Construction at p = 255 | bar 152 px (capped) |
| Name 100 px wide | line 1 empty; line 2 the name |
| Name of two words, the first 80 px wide, the whole 150 px | line 1 the first word; line 2 the rest |
| Name of 30 characters with no space, 200 px wide | line 1 the longest start (at most 18 characters) that with a hyphen is under 109 px, plus the hyphen; line 2 the rest, cut to 19 characters and 109 px |
| Several units selected, Shift-click the second | the second is removed from the selection |
| Mana 255 | mana bar fully colour 186 |
| Units A, B, C in slots 0, 1, 2; the selection becomes B, C, D | B in slot 1, C in slot 2, D in slot 0 (lowest empty); D is the primary unit (lowest filled slot) |
| Units A, B in slots 0, 1; the selection becomes B alone | B moves to slot 0; slots 1 to 8 empty |
| Another player's transport with two passengers, selected alone | portrait in slot 0; slots 1 to 8 empty |
| Pointing at the slot holding a footman | status line shows entry 2 (type 0x00 + 2) |
| Own keep, selected alone, nothing else changes, its upgrade job target changes | no redraw from the checks |
| Own town hall, selected alone, its job target changes | panel redrawn |
| Gold mine selected alone; a worker takes a trip out | panel redrawn (amount left changed) |
| Two footmen selected; one takes damage | that slot's icon and health bar redrawn; no panel text (none is shown) |

## Open questions

- **Label alignment and fonts:** the label areas use a text style that is probably right-aligned (values are drawn
  from the left); the hit point text's style flag probably selects a smaller font. Neither has been seen in the
  running game.
- **Background colour** of the health bar strip and the mana bar's inner outline is a colour set at run time; which
  palette entry it is hasn't been traced (assumed black).
- **Production icon after a job ends:** the job cases hide the value column and show the production icon, and only a
  rebuild of the panel hides the icon and shows the values again. Whether the end of a job causes a rebuild (for
  example through the command buttons changing) hasn't been traced. Default: assume the panel is rebuilt then.
- **Passenger click:** the request sent is assumed to be the one-passenger unload of `orders/transports.md`; the
  message's handling wasn't traced.
- **Sight above 9:** the sight shown is looked up in a list of 10 sight radii (0 to 9); a type with a larger sight
  would show an unreliable number. No default type has one (assumed).
- **The scouting, longbow and marksmanship bonuses** in the melee, sea and transport classes are also coded for types
  0x12 and 0x13, but those types are in the archer class, so they never apply there.
- **Enhanced-mode camera keys:** the panel also holds a hidden key list used to cycle the view through the last 8 event
  positions. That belongs to a later spec on events and the camera.
- Entries 431 to 433 look like a debug display for this panel; no use was found.
