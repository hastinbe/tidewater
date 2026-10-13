# UI: selecting and commanding

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 19 (selecting and
  commanding)"), the default unit data (`unitdata.dat`: rect-selectable, flags, right-click action), the keyboard
  accelerator table of the localisation library, and the public PUD specification text (unit types, flag bits,
  right-click actions). Not yet confirmed against the running game.
- **Changes:** 2026-10-13, round-55 errata: each unit that takes a player's command forgets its recorded attacker
  after the command is given (traced: `human09-fleet-3000`); a right-click is read the same way (static).

## Behaviour

This spec covers how the local player selects units with the mouse and keyboard, what a right-click on the map or
minimap does, how a target or placement click turns into a command, which selected units take part in a command,
and the in-game keys that aren't command-panel hotkeys. What a command then does is in `orders/orders.md` and the
specs it names. Buttons and their hotkeys are in `ui/command-buttons.md`. Sounds are in `ui/sounds.md`.

Unit types and flag bits are those of the PUD specification (appendix A and the UDTA "flags" list). "Enhanced
rules" and "Classic rules" are the two rule sets of `core/timing-and-rng.md`, which says where each applies.
**Mobile** means a unit whose type has any of flag bits 0 to 4 (land, air, explode, sea, critter).

### Two selections

- **Local selection:** what the local player sees as selected. It has up to 9 places and is used by the panel, the
  sounds and the mouse rules below. The **primary unit** is the unit in its first filled place (the lowest filled
  place).
- **Synced selection:** each player has a selection of up to 9 units that is part of the game state. Every change of
  the local selection made by the rules below is **sent** to all players as a player command, unless a rule says it
  is not sent. When that command runs, it replaces the issuing player's synced selection. Commands from the mouse and
  keyboard act on the **issuing player's synced selection**, not on the local one.

**Setting the selection.** When a rule below selects units:

1. The selection sound (`ui/sounds.md`, "Selecting units") is asked for, if the rule says "with sound". Classic
   rules ask only when exactly one unit is selected; Enhanced rules ask for the first unit of several too.
2. If exactly one unit is selected and it is dying or dead, nothing more happens.
3. The change is sent (unless the rule says otherwise), each unit is marked as selected, and the panel is rebuilt.

**Losing a unit from the selection.** When a unit dies, enters a gold mine or oil platform, or goes inside
something:

- it is removed from every remembered group (below);
- it is removed from every player's synced selection. Under Enhanced rules the remaining units close up; under
  Classic rules the place is left empty;
- it is removed from the local selection, leaving its place empty;
- if the local selection is then empty, placement mode and target mode end.

### Who may be selected

A unit **can be picked** by the local player when all of these hold:

- it isn't dying or dead and isn't inside anything;
- it isn't hidden from the local player (`vision/fog.md`, "Unit visibility");
- **Enhanced rules only:** its order and its pending order are none of 22 (inside a building it is constructing),
  25 (entering a mine or platform) and 26 (inside a mine or platform);
- **Enhanced rules only:** it is detected by the local player.

A unit is **group-selectable** when its type's "selectable via rectangle" value is 1 and it belongs to the local
player. In the default data that excludes the daemon, the critter, every building and the unused types.

The **selection box** of a unit is a rectangle of its type's box size, centred in the unit's tile footprint: its left
edge is the unit's pixel x + 16 × W − floor(box width / 2), with W the type's unit size in tiles; the same for y. A
point or rectangle **overlaps** it when it touches it, edges included.

### Mouse regions

The screen is split into regions, and the left and right buttons do different things in each:

| Region | Where (screen pixels, edges included) |
| --- | --- |
| Minimap | x 24 to 151, y 26 to 153 (128 × 128) |
| Panel | x 0 to 175, y 160 to 479, outside the minimap |
| Map view | the visible map area, x 176 to 623, y 16 to 463 (448 × 448 pixels, 14 × 14 tiles) |
| Elsewhere | anything else |

**Panel and elsewhere.** The mouse rules of this spec do nothing with a left click on the panel or elsewhere; the
command buttons handle their own clicks (`ui/command-buttons.md`). A right click on the panel or elsewhere ends
target mode or placement mode if one is active. Otherwise it returns the command panel from a sub-menu to the
selection's own button set, and nothing is sent.

**Scrolling edge.** While the pointer is at a screen edge that scrolls the view (a later spec), mouse buttons do
nothing.

While the game is **paused**, only key presses are handled; mouse buttons do nothing.

| Mode | Map view, left | Map view, right | Minimap, left | Minimap, right |
| --- | --- | --- | --- | --- |
| Normal, first mouse style | select | right-click command | move the view | right-click command to a tile |
| Normal, second mouse style | select | centre the view on the clicked tile | move the view | move the view |
| Target mode | give the target | end target mode | give the target tile | move the view |
| Placement mode | place the building | end placement mode | move the view | move the view |

The **mouse style** is a preference (the Preferences dialog's two-way choice); the first style is the one with
right-click commands. "Move the view" on the minimap is in `ui/minimap.md` (see also `vision/fog.md`, "The
minimap").

### Selecting with the left button

**Press.** Pressing the left button in the map view ends placement and target mode, and remembers the point and
whether Shift is held. Under Enhanced rules it also remembers Ctrl and whether the press was a double-click. While the
button is held, the dragged rectangle is drawn.

**Release.** The rectangle runs from the press point to the release point, put in order (smaller corner first), in
map pixels. It is **small** when it is at most 4 pixels wide and at most 4 pixels high (right minus left ≤ 4 and
bottom minus top ≤ 4).

**Enhanced rules, small rectangle:** the click rules below are tried first, at the rectangle's top-left corner. If
they finish the click, the box rules aren't used.

#### Click rules (Enhanced rules only)

1. **Find the unit.** Every unit that can be picked and whose selection box contains the point is a candidate.
   Candidates are looked at in slot order; the first one is the current best. A later candidate C replaces the best:
   - if the best is not an air unit: when the point is on a non-transparent pixel of C's current sprite frame
     (frames for facings 5 to 7 are mirrored), or else when C's footprint (W × H tiles) is smaller than the best's;
   - if the best is an air unit: only when the point is not on a non-transparent pixel of the best, and C's footprint
     is smaller.

   With no candidate, the click does nothing (the selection is kept).
2. **Shift alone:** if Shift is held, the selection isn't empty, and neither Ctrl nor a double-click applies, the
   click rules stop and the box rules run instead (adding or removing one unit).
3. A **double-click** counts only when the unit found is already selected.
4. **Ctrl or double-click: select all of this type on screen.** Search outward from the unit's tile in square rings
   (side 3, then 5, 7 and so on). Look at the air units if the found unit is an air unit, otherwise at the ground
   units. Each ring is walked one side at a time, east, south, west, then north; after the first side, the first
   tile of each side is skipped. Stop after a ring that touched no map tile, or once the side would reach twice the
   view width in tiles (28). The found unit itself is always the first one taken. Another unit is taken when:
   - it can be picked, is group-selectable, and has the same owner and type as the found unit;
   - its selection box overlaps the visible map view;
   - fewer than 9 have been taken.

   **With Shift as well**, the new units are added to the current selection, without duplicates, up to 9. This
   needs the primary unit to be the local player's own with a type whose "selectable via rectangle" value is 1, and
   the found unit's type to have that value 1 too. If either fails, the click does nothing at all (the selection is
   kept, nothing is sent). Without Shift, the new units replace the selection.

   Then: if one unit results and Alt is held, the remembered group containing it is selected (see "Remembered
   groups"). If several result, they are stored as a remembered group. The result is selected with sound.
5. **Plain click:** if Alt is held, the remembered group that contains the unit is selected, without sound.
   Otherwise the unit alone is selected, with sound.

#### Box rules (both rule sets)

1. **Shift:** keep the units of the current selection that are the local player's own. Without Shift the list
   starts empty. If Shift is held and the rectangle is small, a selected unit whose selection box overlaps the
   rectangle is the **toggled unit**: it is taken out. If 9 units are kept, stop here.
2. Look at every unit in slot order and skip it if:
   - it can't be picked (under Classic rules the order test and the detected test don't apply);
   - it is the toggled unit;
   - its selection box doesn't overlap the rectangle;
   - Shift is held and it is already selected.
3. A unit that isn't group-selectable becomes the **fallback**; a later one replaces an earlier one. A
   group-selectable unit is added, up to 9 in all.
4. If the rectangle is small, more than one unit was found, and Shift is not held, only one is kept: the first air
   unit found, or else the first unit found.
5. If nothing was found and nothing was toggled: the fallback alone is selected; with no fallback, nothing happens.
6. If exactly one unit results and Alt is held, the remembered group containing it is selected (no sound, sent).
   If more than one results, they are stored as a remembered group.
7. The result is selected, with sound unless a unit was toggled.

So a box over the enemy army selects one enemy unit (the last one found), and a box over a mix of own units and an
enemy selects only the own units.

#### Remembered groups

The local client remembers up to **16 groups** of up to 9 units each, with the time each was last used. They aren't
part of the game state. A new group of several units first removes those units from every remembered group. It then
goes into an empty group, or replaces the one used longest ago. Alt with a click recalls the remembered group that
contains the clicked unit and marks it as just used.

### Hovering

Each time the mouse moves over the map view, the game lists the units under the cursor. It looks at the 5 × 5 tiles
around the cursor's tile, at column offsets 0, −1, +1, −2, +2 (outer) and row offsets in the same order (inner).
At each tile it checks the ground unit (a building only once) and then the air unit. A unit is listed when:

- it isn't dying, dead or inside anything;
- it isn't hidden from the local player and is detected by it;
- the cursor is inside its selection box;
- its selection box overlaps the map view.

Up to 9 are listed; the first is the **hovered unit**. The list also chooses the pointer's shape (a later spec).

### Choosing the target of a click

The target of a right-click or a target-mode click is:

1. the hovered unit; or, if there is none, the ground unit on the clicked tile, but only if it is a building;
2. none, if that unit is dying or dead;
3. the unit, if the reveal-map cheat is on;
4. none, if the clicked tile is unexplored;
5. the unit, if it isn't hidden from the local player or is a building; otherwise none.

### Right-click on the map (first mouse style)

Nothing happens if either of these holds:

- Enhanced rules apply and the game is paused or a menu is open;
- the primary unit isn't the local player's own, or isn't mobile.

Otherwise:

- With exactly one unit selected and that unit as the target, nothing happens.
- Otherwise the **right-click command** is sent with the clicked tile and the target (if any). The order
  acknowledgement plays (`ui/sounds.md`, "Acknowledging orders").
- **Feedback:** a target unit flashes for 31 cycles. The flash uses palette colour 250 if the unit is the local
  player's own, 251 if allied, and 249 otherwise. With no target, a ground marker (effect kind 0x1C) appears at the
  cursor's pixel.

**Right-click on the minimap** (first mouse style): the same test on the primary unit. The command is sent for the
tile under the click, with no target. The marker appears at that tile's pixel position plus 8 on each axis.

### What a right-click command does

When the command runs (for the issuing player):

1. **Formation offset** (also used by target-mode commands; see below).
2. For each unit of the issuing player's synced selection that is the issuing player's own, mobile, and not itself
   the target, a command is chosen by its type's **right-click action** (UDTA "2nd mouse button action"; only types
   0x00 to 0x39 have one). Under Enhanced rules the unit's remembered patrol or attack-move is forgotten first. The
   command is then given as in `orders/orders.md`. A unit whose current order can't be interrupted is skipped there.
   After the command is given, the unit forgets its recorded attacker, as in "Which units take part in a command"
   (static study: read as the same step; no trace has a right-click on a unit with a recorded attacker).

| Action | Command given |
| --- | --- |
| 0 none | Nothing |
| 2 move, 6 sail | 3 (smart move) |
| 1 attack | Type without flag bit 19 (can attack): 3. Target unit not allied: 8 (attack). Otherwise: 11 if the target tile has a wall flag (0x04 or 0x08 in `movement/occupancy.md`), else 3 |
| 3 harvest | See "Harvest action" |
| 4 haul oil | Target unit is the issuing player's own: carrying oil and the target stores oil (flag bit 24): 24 (return goods). Not carrying and the target is an oil platform (flag bit 11): 23 (harvest). Anything else: 3 |
| 5 demolish | Target unit not allied: 19 (demolish). Otherwise: 3 |

**Harvest action** (peasant, peon), first match:

| Target | Command |
| --- | --- |
| A unit that isn't allied | 8 (attack) |
| An allied unit of another player | 3. Gold mines belong to the neutral player, who is allied to everyone, so a gold mine always takes this row |
| Own unit, worker not carrying, the target is a gold mine | 23 |
| Own unit, worker carrying lumber, the target stores lumber (flag bit 18) | 24 |
| Own unit, worker carrying anything, the target stores gold (flag bit 12) and not lumber | 24 (this includes a worker carrying lumber at a gold-only store) |
| Any other own unit | Repair check: a building below its maximum hit points gives 27 (repair); anything else gives 3 |
| No target unit, the tile is a tree or a reserved tree (`movement/occupancy.md`), worker not carrying | 23, with no target unit |
| No target unit, anything else | 3 |

A worker carrying gold that right-clicks a store taking both lumber and gold gets the repair check, not 24 (the
lumber test comes first). In the default data every gold store also stores lumber, so a worker carrying gold gets
command 3 on an undamaged hall; the smart move then returns the goods (`orders/orders.md`, "Smart move").

**Formation offset.** Done only when the target tile has no wall flag. Over the issuing player's synced selection,
take each unit's tile, the count, the mean of each axis (rounded toward zero) and the minimum and maximum of each
axis. Do the following only if the target tile lies outside that bounding box. On each axis where max − min < 4,
the offset is target − mean. Each unit then aims at its own position plus the offset on that axis, clamped to the
map. On an axis where the selection is spread 4 or more tiles, every unit aims at the clicked coordinate. So a small
group keeps its shape when sent somewhere away from it; a group spread wide converges.

### Target mode and placement mode clicks

**Target mode** is entered by a command button (`ui/command-buttons.md`). A left click on the map view:

- gives the same feedback as a right-click (flash or ground marker);
- picks the target as above;
- ends target mode;
- sends the mode's command with the tile and target, and plays the acknowledgement.

A left click on the minimap gives the tile under the click with no target. A right click on the map view, the panel
or elsewhere ends the mode without sending anything.

**Placement mode:** a left click on the map view runs the site check of `ui/placing-buildings.md` ("The click").
A failed check plays the error sound, shows its message and leaves placement mode **on**, so the player can click
again. Only an allowed click ends the mode and sends the build command. A right click on the map view, the panel or
elsewhere ends the mode without sending anything.

### Which units take part in a command

A command sent from target mode, the command buttons or the keys is carried out only if it is one the game handles
(2 to 6, 8 to 17, 19, 23, 24, 27, 29 and 38 to 56) and its tile is on the map. Then the formation offset is applied
and the command goes to each unit of the issuing player's synced selection that:

- is the issuing player's own and isn't the target;
- passes the test of the first matching row below.

Under Enhanced rules each unit that takes the command first forgets its remembered patrol or attack-move
(`orders/orders.md`); commands 5 and 8 then set a new one. After the command is given, each unit that takes it also
forgets its recorded attacker, whatever the attacker's state, so the command's own checks still see the record
(`orders/orders.md`, "A player's command forgets the attacker"; traced: `human09-fleet-3000`). A selected unit that
doesn't take the command (another player's, the target itself, or one that fails the filter below) keeps its record.

| Command | The unit must |
| --- | --- |
| 23 (harvest) | be a worker or tanker (flag bit 8 or 9) not carrying anything |
| 24 (return goods) | be a worker or tanker carrying something |
| 17 (attack the ground) | have flag bit 14 (can ground attack) and flag bit 19 (can attack) |
| 4, 8 to 16 | have flag bit 19 |
| 19 (demolish) | have flag bit 19 and flag bit 25 |
| 5 (patrol), Enhanced rules, demolition squad, sapper, flying machine, zeppelin, Eye of Kilrogg | be mobile |
| 5 (patrol), otherwise | have flag bit 19 |
| 2 (stop) | nothing more |
| 3, 6 | be mobile |
| 27 (repair) | be a worker (flag bit 8) |
| 29 (unload) | be a transport (flag bit 10) |
| 38 to 41 | cast spells (flag bit 17) and be a paladin, Turalyon, Lothar or Uther Lightbringer, or a knight (type 0x06) |
| 42 to 47 | have flag bit 17 or 26, and be a mage or Khadgar |
| 48, 49, 55 | cast spells and be an ogre-mage, Dentarg, Cho'gall, or an ogre (type 0x07) |
| 50 to 54, 56 | have flag bit 17 or 26, and be a death knight, Gul'dan or Teron Gorefiend |

This filter decides which selected units receive a command. It is separate from what the command does once a unit
has it: `combat/attack-ground-demolish.md` says command 17 itself doesn't test flag bit 14, and that still holds for
a command that reaches a unit some other way. Only this selection filter tests the flags.

The knight and ogre rows reflect the data: they are listed with the casters, but they lack flag bit 17 in the
default data, so they never qualify unless a map changes their flags.

### Control groups (Enhanced rules only)

Under Classic rules the control-group keys do nothing. In placement or target mode, set (Ctrl+digit) and recall
(digit) do nothing, but Alt+digit still works (it isn't blocked).

Each player has **18 groups** of up to 9 units, as part of the game state. The keys use groups 0 to 9.

- **Set (Ctrl+digit):** sends a set-group command. When it runs, the group is cleared and filled from the issuing
  player's synced selection in order. Filling stops at the first unit that isn't that player's own.
- **Recall (digit):**
  - Locally, the group's units are selected with sound, without sending the selection. Only units that can be
    picked under the order test (not dying, dead or inside, not in orders 22, 25, 26) are taken. They must also be
    the local player's own and group-selectable, unless the group has fewer than 2 entries. If no unit qualifies,
    nothing happens (nothing is selected or sent).
  - Then a recall-group command is sent (but see "Double press"). When it runs, the group is first tidied: units
    that are dead or dying, no longer the player's own, or (if the group has more than one entry) not group-selectable
    are removed. Each removal moves the group's last entry into the gap. Each remaining unit that isn't inside
    anything and isn't in order 22, 25 or 26 becomes part of the issuing player's synced selection.
- **Double press:** a recall of the same group within **500 ms** of the last set or sent recall of that group
  still selects the units locally (with sound), but centres the view on the group instead of sending the recall.
  The view's top-left tile becomes the members' average tile minus half the view size. Any other change of
  selection that is sent forgets the last group.
- **Alt+digit:** centres the view on the group, then recalls it as above (so the double-press rule applies to its
  recall too).

### Keys

These keys act in the game screen. While a chat line is being typed, the digit keys and + and − type text instead.
"Ctrl/Alt+K" means either modifier.

| Key | Effect |
| --- | --- |
| + or =, − or _ | Game speed one step up or down (a synced command). Levels 0 to 6 under Classic rules, 0 to 8 under Enhanced rules. Does nothing in Enhanced multiplayer |
| Digit, Ctrl+digit, Alt+digit | Control groups (above) |
| Ctrl/Alt+C | Centre the view on the primary unit, if it is the local player's own or isn't invisible (the same rule as clicking the single unit's icon in `ui/info-panel.md`) |
| Ctrl+D, F7 | Sound options dialog |
| Ctrl/Alt+H, F1 | Help menu |
| Ctrl/Alt+K | Keyboard help |
| Alt+L, F12 | Load game |
| Ctrl+M | Music on or off |
| Alt+M | Game menu |
| Ctrl/Alt+O, F5 | Options |
| Ctrl/Alt+P | Pause or resume (below) |
| Ctrl/Alt+Q | Multiplayer: surrender or quit dialog. Otherwise: quit to the main menu |
| Ctrl/Alt+R | Restart the scenario (not in multiplayer) |
| Ctrl+S | Sound effects on or off |
| Alt+S, F11 | Save game |
| Ctrl/Alt+T | Tips |
| F2, F3, F4 | Move the view to remembered view position 1, 2 or 3 |
| Shift+F2, F3, F4 | Remember the current view position as 1, 2 or 3 |
| F8 | Game speed dialog |
| F9 | Preferences dialog |
| Ctrl/Alt+X | Quit dialog |

The mission objectives dialog has no key.

**Pause** is a synced command (pause or resume) and shows a notice (a later spec). In Enhanced multiplayer
each player has a limited number of pauses: pausing uses one, and a player with none left can't pause.

**Preferences dialog** settings used here and elsewhere: the mouse style; fog of war on or off (not changeable
in multiplayer; `vision/fog.md`); minimap terrain on or off (`ui/minimap.md`); colour cycling.

## Data

| Constant | Value |
| --- | --- |
| Selection size | 9 |
| Small rectangle | ≤ 4 pixels on both axes |
| Remembered groups (local) | 16 × 9 |
| Control groups (per player) | 18 × 9, keys use 0 to 9 |
| Double-press window | 500 ms |
| Same-type ring search limit | ring side < 2 × view width in tiles (28) |
| Hover search | 5 × 5 tiles, up to 9 units |
| Target flash | 31 cycles; palette colours 250 own, 251 allied, 249 other |
| Ground marker | effect kind 0x1C |
| Formation offset | axes with spread < 4 tiles |

Right-click action in the default data:

| Action | Types |
| --- | --- |
| 1 attack | every type 0x00 to 0x38 not listed below |
| 2 move | 0x28 flying machine, 0x29 zeppelin, 0x2D Eye of Kilrogg, 0x39 critter |
| 3 harvest | 0x02 peasant, 0x03 peon |
| 4 haul oil | 0x1A, 0x1B tankers |
| 5 demolish | 0x0E dwarven demolition squad, 0x0F goblin sapper |
| 6 sail | 0x1C, 0x1D transports |
| 0 none | 0x22, 0x30, 0x36 (unused) |

Types 0x00 to 0x39 without flag bit 19 in the default data: tankers, transports, flying machine, zeppelin, Eye of
Kilrogg, critter, 0x24, 0x25 and the unused types. Flag bit 25 is set only on the demolition squad and sapper.

## Test vectors

| Situation | Expected |
| --- | --- |
| Enhanced; left click (no drag) on an own footman | Footman alone selected, selection sound, selection sent |
| Enhanced; left click on empty ground, footman selected | Nothing changes |
| Classic; left click on empty ground, footman selected | Box rules find nothing, no fallback: nothing changes |
| Enhanced; box over 12 own footmen | First 9 in slot order selected, with sound for the first |
| Classic; box over 12 own footmen | First 9 in slot order selected, no sound (several units) |
| Box over 3 own footmen and an enemy grunt | The 3 footmen |
| Box over 2 enemy grunts only | The grunt found last, alone |
| Box over own footmen and an own farm | Only the footmen (a farm isn't group-selectable) |
| Classic; small click on a spot where a gryphon rider overlaps a footman | The gryphon rider (first air unit) |
| Shift + click on a selected footman (3 selected) | Footman removed; 2 remain; no sound |
| Enhanced; Shift + click on an unselected own footman (3 selected) | Box rules: 4 selected, with sound |
| Classic; Shift + click on an unselected own footman (3 selected) | 4 selected, no sound |
| Enhanced; Shift + Ctrl + click on an own archer while an enemy unit is selected | Nothing happens; the selection is kept |
| Enhanced; Ctrl + click on an own archer with 12 archers on screen | 9 archers found by the ring search, nearest rings first; stored as a remembered group |
| Enhanced; double-click on an archer that wasn't selected | Treated as a plain click: the archer alone |
| Enhanced; Ctrl+1 with 4 knights selected, later press 1 | Set, then recall: the 4 knights, sound; recall sent |
| Enhanced; press 1 twice within 500 ms | Second press centres the view on group 1; nothing sent |
| Classic; press 1 | Nothing |
| Enhanced; target mode on, press 1 | Nothing |
| Enhanced; target mode on, Alt+1 | View centred on group 1, group recalled |
| Placement mode; left click on a site that fails the check (for example too close to a gold mine) | Error sound and the check's message; placement mode stays on; nothing sent |
| Placement mode; right click on the panel | Placement mode ends; nothing sent |
| Right-click on the panel while the build sub-menu is shown, no mode on | The panel returns to the selection's own button set |
| Right-click enemy grunt with 3 footmen selected | Command 8 for each footman; grunt flashes colour 249 for 31 cycles |
| Right-click own damaged barracks with a peasant (empty-handed) | Command 27 (repair) |
| Right-click gold mine with a peasant (empty-handed) | Command 3 (smart move, which harvests) |
| Right-click own town hall with a peasant carrying gold, hall at full hit points | Command 3 (the hall also stores lumber); the smart move then returns the gold (`orders/orders.md`) |
| Right-click own town hall with a peasant carrying lumber | Command 24 |
| Right-click own lumber mill with a peasant carrying gold, mill at full hit points | Command 3 |
| A map gives an own building flag bit 12 but not 18; right-click it with a peasant carrying lumber | Command 24 |
| Right-click a tree tile with an empty-handed peon | Command 23, no target unit |
| Right-click an own oil platform with an empty tanker | Command 23 |
| Right-click enemy farm with a sapper | Command 19 |
| Right-click with only an enemy unit selected | Nothing |
| Right-click the single selected unit itself | Nothing |
| 3 footmen at tiles (10, 10), (11, 10), (12, 11); right-click tile (30, 20) | Mean (11, 10); offsets (19, 10); targets (29, 20), (30, 20), (31, 21) |
| Footmen at x = 10 and x = 20, same y = 5; right-click tile (40, 30) | x spread 10: both aim at x = 40; y offset 25: both aim at y = 30 |
| Right-click a tile inside the selection's bounding box | No offset: every unit aims at the clicked tile |
| Target-mode attack ground (command 17) with a footman and a catapult selected | Only the catapult takes it |
| Target-mode patrol, Enhanced, flying machine and footman selected | Both patrol |
| Target-mode patrol, Classic, flying machine and footman selected | Only the footman |
| `human09-fleet-3000` (oracle trace, Enhanced), turn n = 1502: command 4 to (28, 40) for the human player's selection, including battleship 1539 (type 0x20, owner 6) in order 12 on its recorded attacker, catapult 1519 | 1539 takes the command and forgets 1519: with no route it is idle at n = 1507 (order 2, wait 1, target null) and nothing seeds its search |
| Stop command with a footman selected whose recorded attacker is alive, detected and in range (static study) | The attacker is forgotten |
| Box over a peasant inside a mine, Enhanced | Not picked |
| Press + with speed at the top level | Nothing changes |

## Open questions

- **F10 and Enter** aren't in the in-game key table. They are probably handled by the game screen itself (menu and
  chat line). The scrolling keys belong to a later spec.
- **Ring search order** decides which 9 units are taken when more match. The ring walk above is from static study
  only.
- **The pause notices** and the pause count's starting value in Enhanced multiplayer belong to later specs.
- **Minimap left-click in target mode** gives the tile with no target. Whether the command then gets the
  formation offset the same way as a map click is assumed, not checked.
- **Control groups in some network games.** In some kinds of network game, setting and recalling a group take
  another route that wasn't traced. Default: the rules above.
- **Right-click and the attacker record.** The right-click's commands are read as carried out by the same step as a
  button or key command, so each unit that takes one forgets its recorded attacker. No trace has a right-click on a
  unit with a recorded attacker.
- **Box selecting with Alt and a single enemy unit** would recall a remembered group containing an enemy. That can't
  happen, because only groups of own units are stored, but this isn't tested.

