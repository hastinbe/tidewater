# UI: command buttons

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the button panel layout
  in `statbtn.bin`, the string table `stat_txt.tbl`, and the public PUD specification text (unit types, ALOW
  section). Not yet confirmed against the running game.
- **Changes:** 2026-10-16 (round 77 and the B-040 audit, reviewed by a spec-review session, 2026-10-16): the cancel
  job button names the cancel-job command, which is separate from cancel construction (`economy/build-train.md`;
  traced: V57 to V59 there). No rule change.

## Behaviour

The command panel is the 3 × 3 grid of buttons in the lower left of the screen. This spec says which buttons it
shows, what each one shows and does, and what hides it. What happens after a command is sent is in
`orders/orders.md`, `economy/build-train.md`, `economy/tech-tree.md` and a later spells spec.

Strings are entries of the string table `stat_txt.tbl`, **counting from 1**. Icons are frame numbers of the icon
sprite file. Unit types and upgrade numbers are those of the PUD specification (appendices A and B).

### The panel

- **Size and place:** a 176 × 144 pixel panel whose top edge is at screen y = 336.
- **Slots:** 9 buttons, each 54 × 46 pixels. Slot n is in row floor(n / 3) and column n mod 3, so slots 0 to 2
  are the top row and 6 to 8 the bottom row.
- **Button positions** inside the panel: x = 5, 61 or 117 for columns 0, 1, 2; y = 0, 47 or 94 for rows 0, 1, 2.

### Which set of buttons is shown

The panel shows one **button set**. Every unit type has its own set (several types share the same contents; see
"Button sets"). The **primary unit** is the first unit of the selection.

The set is chosen as follows; the first matching rule wins:

1. **Nothing selected, or the primary unit isn't owned by the local player:** no buttons.
2. **Choosing a target** (after Move, Attack, Patrol, Attack ground, Demolish, Harvest, Repair, Unload or a
   targeted spell): the "Choosing a target" set (one Cancel button).
3. **Placing a building** (after a Build button): the "Placing a building" set (one Cancel button).
4. **A build menu is open** (after the basic or advanced build button): that menu's set. Its Cancel button closes
   the menu and goes back to the primary unit's own set.
5. **Several units selected that don't all have the same set:** the "Mixed selection" set for the local player's
   race. Each unit's set is identified by its type (or by the busy set), so two types count as different even
   when they use the same buttons, for example a footman and a ballista.
6. **Otherwise:** the primary unit's own set. A building that is **under construction** or **working** (training,
   researching or upgrading itself) uses the "Building busy" set instead. It goes back to its own set when the
   construction or the job ends. When a unit changes type (a knight becoming a paladin, a town hall becoming a keep
   and so on) its set changes with it.


### Filling the slots

Each set is a list of entries in slot order. An entry has a slot, an icon, a condition ("Shown when"), an action
("Does") and a string. Several entries can share a slot, for example the three Stop icons for the three shield
levels.

- For each slot, the **first entry for that slot whose condition passes** is shown. If none passes, the slot is
  empty.
- Conditions are tested against the **local player's** state (its upgrade levels, known spells, buildings and so
  on) and the current selection. Since only the local player's own units show buttons, this is the owner's state.
- Conditions are tested again each time the panel is rebuilt, so a button appears as soon as, for example, a
  building it needs is completed.

### Tooltips, hotkeys and costs

Every button's string entry has this layout:

- **First character:** the button's **hotkey**. Pressing that key does what clicking the button does. Only the
  buttons currently shown are matched. The Cancel entries use a control character here, taken to be the Escape key.
- **Second byte:** the **cost kind**, which says what the tooltip shows next to the text:

  | Kind | Tooltip shows |
  | --- | --- |
  | 0 | the text only |
  | 1 | the gold, lumber and oil cost of the unit or building the button makes (the unit data costs × 10) |
  | 2 | the gold, lumber and oil cost of the upgrade or research the button starts (upgrade data) |
  | 3 | the mana cost of the spell the button casts |

- **The rest:** the tooltip text.

### What the buttons do

"Target mode" means the panel switches to the "Choosing a target" set and the next click on the map gives the
target. The command is then sent for every selected unit. "At once" means the command is sent for every selected
unit without a target.

| Does | Detail |
| --- | --- |
| target mode, command n | Command n (`orders/orders.md`, "Commands") with the clicked unit or tile. The Move button sends command 3, the smart move |
| command n at once | Command n without a target: 2 is stop, 13 is stand ground |
| return goods | Looks for a depot that takes what the primary unit carries (as order 24 does; `economy/harvest.md`). If there is one, command 24 is sent at once with no target. If not, message 436 is shown and nothing is sent |
| unload | If the transport is docked (its order or pending order is 32), command 29 is sent at once on its own tile. Otherwise target mode, command 29 (`orders/transports.md`) |
| cast: command n | If the primary unit's mana is below the button cost of command n (table below), message 435 is shown and nothing happens. A cheat state skips this test (which cheat is open in a later spells spec). Otherwise command 48 (eye of Kilrogg) is sent at once, and every other spell enters target mode |
| build X | The affordability check of `economy/build-train.md` runs first (food doesn't apply to buildings); a failure shows its message. Otherwise the panel switches to "Placing a building" with X; a valid click sends the build command |
| train X | The affordability check runs (food, gold, lumber, oil); a failure shows its message. Otherwise a job command (train X) is sent for the primary building |
| research X, upgrade X | The research cost check runs (gold, lumber, oil); a failure shows its message. Otherwise a job command (research or upgrade X) is sent |
| become X | The affordability check runs with X's costs; a failure shows its message. Otherwise a job command (upgrade the building to X) is sent |
| cancel construction | Sends the cancel-construction command for the primary building (`economy/build-train.md`) |
| cancel job | Sends the cancel-job command for the primary building's job (`economy/build-train.md`, "Cancelling a job": full refund, at the job's next work run) |
| show the … menu, go back | Changes the set only. Nothing is sent |
| leave target mode, leave placement mode | Ends the mode and goes back to the set of rule 4 to 6 above. Nothing is sent |

**Button mana cost** by spell command. These are the values the button compares; what each spell then spends is in
a later spec (spells), and for healing, exorcism, blizzard, raise dead and death and decay the button cost is only a
minimum (per hit point, per strike or per skeleton):

| Command | Spell | Cost | Command | Spell | Cost |
| --- | --- | --- | --- | --- | --- |
| 38 | holy vision | 70 | 48 | eye of Kilrogg | 70 |
| 39 | healing | 6 | 49 | bloodlust | 50 |
| 41 | exorcism | 4 | 50 | raise dead | 50 |
| 42 | flame shield | 80 | 51 | death coil | 100 |
| 43 | fireball | 100 | 52 | whirlwind | 100 |
| 44 | slow | 50 | 53 | haste | 50 |
| 45 | invisibility | 200 | 54 | unholy armour | 100 |
| 46 | polymorph | 200 | 55 | runes | 200 |
| 47 | blizzard | 25 | 56 | death and decay | 25 |

Affordability messages: 438 (food), 439 (gold), 440 (lumber), 441 (oil). A job command is checked again when it is
carried out (`economy/tech-tree.md`). The buttons are only a filter for the local player.

### Conditions

**Simple conditions**

| Shown when | Passes when |
| --- | --- |
| always | always |
| one unit selected | exactly one unit is selected |
| Enhanced mode | the game is in Enhanced mode |
| shield level = n | the player's shield upgrade level is n (0, 1 or 2) |
| weapon level = n | the player's sword (axe) upgrade level is n |
| missile level = n | the player's arrow (spear) upgrade level is n |
| cannon level = n | the player's ship cannon upgrade level is n |
| ship armour level = n | the player's ship armour upgrade level is n |
| a selected unit can attack the ground | some selected unit's type has unit flag bit 14 (ballista, catapult, battleship, juggernaught in the default data) |
| a selected harvester is not carrying | walking the selection in order, a worker or tanker that **isn't** carrying is found before any unit that isn't a worker or tanker. A carrying harvester is skipped |
| a selected harvester is carrying | the same walk, looking for one that **is** carrying |
| one unit selected, it has passengers | exactly one unit is selected and at least one of its 6 passenger places is filled |
| one unit selected, spell bit n known | exactly one unit is selected and the player knows spell n (spell bit numbers as in the PUD ALOW section) |
| one unit selected, some basic (advanced) building allowed | exactly one unit is selected and at least one **building** button of that build menu would be shown ("Build buttons" below). The menu's always-shown Cancel (go back) entry doesn't count |
| under construction | the primary building is not completed |
| working, job is training | the primary building is working and its job is training a unit |
| working, job is not training | the primary building is working and its job is research, an upgrade or a building upgrade |

Every "level" above counts completed upgrades: 0 before any, 1 after the first, 2 after the second. Cho'gall's spell
buttons are always shown: unlike other casters, he doesn't need the spell known or a single selection.

**Job buttons** (in buildings). "Ready" means the primary building is **completed** and **not working**. "ALOW bit
n" means bit n of the player's "units and buildings allowed" word from the map (PUD ALOW section). "Has X" means the
player has at least one **completed** X (either race's version). A keep or castle doesn't count as a town hall, and
a castle doesn't count as a keep.

| Button | Shown when |
| --- | --- |
| Train peasant (peon) | ready, ALOW bit 1 |
| Train footman (grunt) | ready, ALOW bit 0 |
| Train archer (axethrower) | ready, ALOW bit 4, has lumber mill, ranger training **not** done |
| Train ranger (berserker) | ready, ALOW bit 4, has lumber mill, ranger training done |
| Build ballista (catapult) | ready, ALOW bit 2, has blacksmith and lumber mill |
| Train knight (ogre) | ready, ALOW bit 3, has blacksmith and stables, paladin upgrade **not** known |
| Train paladin (ogre-mage) | ready, ALOW bit 3, has blacksmith and stables, paladin upgrade known |
| Train mage (death knight) | ready, ALOW bit 5 |
| Build tanker | ready, ALOW bit 6 |
| Build destroyer | ready, ALOW bit 7 |
| Build transport | ready, ALOW bit 8, has foundry |
| Build battleship (juggernaught) | ready, ALOW bit 9, has foundry |
| Build submarine (turtle) | ready, ALOW bit 10, has inventor |
| Build flying machine (zeppelin) | ready, ALOW bit 11, has lumber mill and inventor |
| Train gryphon rider (dragon) | ready, ALOW bit 12, has aviary |
| Train demolition squad (sappers) | ready, ALOW bit 14, has inventor |
| Upgrade to keep (stronghold) | the primary building is a town hall (great hall), ready, ALOW bit 27, has barracks |
| Upgrade to castle (fortress) | the primary building is a keep (stronghold), ready, ALOW bit 28, has stables, lumber mill and blacksmith |
| Upgrade to guard tower | the primary building is a scout tower, has lumber mill (nothing else: no ALOW, completed or working test) |
| Upgrade to cannon tower | the primary building is a scout tower, has blacksmith (likewise) |

For the paladin and ogre-mage upgrade, "known" is spell bit 20 (below).

**Upgrade and research buttons.** Each player has, from the map's ALOW section, an **upgrade allowed** word, an
**upgrade being researched** word, a **spell allowed to research** word, a **spell being researched** word and a
**spells known** word.

| Button | Shown when |
| --- | --- |
| Two-level upgrade, level n | the building isn't working; the upgrade-allowed bit for this upgrade and level is set; its being-researched bit is clear; the player's level for it equals n |
| Ranger (berserker) training | the player has a keep or castle (either race's version); the building isn't working; upgrade bit 16 allowed and not being researched; ranger training not yet done |
| Longbow, scouting, marksmanship (lighter axes, scouting, regeneration) | ranger training **done**; the building isn't working; upgrade bit 17, 18 or 19 allowed and not being researched; this upgrade not yet done |
| Research spell (mage tower, temple), paladin or ogre-mage upgrade | the building isn't working; the spell bit is not known, not being researched, and allowed to research |
| Research spell at a church or altar | as the line above, and spell bit 20 (paladins or ogre-mages) is known |

Upgrade bits of the two-level upgrades, level 0 / level 1:

| Upgrade | Bits |
| --- | --- |
| Arrows (spears) | 0 / 1 |
| Swords (axes) | 2 / 3 |
| Shields | 4 / 5 |
| Ship cannons | 6 / 7 |
| Ship armour | 8 / 9 |
| Ballista (catapult) | 12 / 13 |

The **being-researched** bit set when a two-level upgrade starts is taken from the upgrade data (its "affects"
value; `economy/tech-tree.md`), while the button tests the fixed bits above. For every upgrade but one the two agree.
**Arrow 2** (upgrade 5) has "affects" bit 0 instead of bit 1 in the default upgrade data (`upgrades.dat`, which
anyone can check). So while one lumber mill researches arrow 2, the level-1 arrow button, which tests bit 1, is still
shown on a second lumber mill, which can start arrow 2 again. This follows from the data and a static study; the
second start **hasn't been observed in the running game yet** (`economy/tech-tree.md`, "Research twice", and its
open question).

**Build buttons.** "Ever had a hall" means the player has at some time had a completed town hall, keep or castle
(either race's version); the mark stays after the building is destroyed.

| Button | ALOW bit | Also needs |
| --- | --- | --- |
| Farm | 16 | ever had a hall |
| Barracks | 17 | ever had a hall |
| Town hall (great hall) | 26 | nothing else |
| Lumber mill | 18 | ever had a hall |
| Blacksmith | 29 | ever had a hall |
| Tower | 25 | ever had a hall |
| Wall | 31 | ever had a hall, and a multiplayer game (more than one human player) |
| Shipyard | 30 | ever had a hall, has lumber mill |
| Foundry | 21 | ever had a hall, has shipyard |
| Refinery | 22 | ever had a hall, has shipyard |
| Inventor (alchemist) | 23 | has keep or castle |
| Stables (ogre mound) | 19 | has keep or castle |
| Church (altar of storms) | 24 | has castle |
| Mage tower (temple) | 20 | has castle |
| Aviary (roost) | 15 | has castle |
| Oil platform (tanker set) | — | exactly one unit selected; nothing else |

The PUD specification calls ALOW bit 31 unused; the game uses it for walls.

**Legend: "Shown when" labels in the button sets.** Each label used in "Button sets" maps to a row above:

| Label | Meaning |
| --- | --- |
| unit requirement | the "Job buttons" row for the unit the button trains (for example "train ranger" uses the row "Train ranger (berserker)") |
| building requirement | the "Build buttons" row for the building the button builds |
| hall to keep (see requirements) | the "Job buttons" row "Upgrade to keep (stronghold)" |
| keep to castle (see requirements) | the "Job buttons" row "Upgrade to castle (fortress)" |
| guard tower (see requirements) | the "Job buttons" row "Upgrade to guard tower" |
| cannon tower (see requirements) | the "Job buttons" row "Upgrade to cannon tower" |
| two-level upgrade, level n (line) | the "Upgrade and research buttons" row "Two-level upgrade, level n", for the line named: missile (arrows, spears), weapon (swords, axes), shield, cannon (ship cannons), ship armour, siege (ballista, catapult); the bits are in "Upgrade bits" |
| ranger line: training | the row "Ranger (berserker) training": upgrade bit 16, and the ranger-training level is 0 |
| ranger line: range | longbow (lighter axes): ranger training done, upgrade bit 17 allowed and not being researched, its level 0 |
| ranger line: scouting | ranger (berserker) scouting: ranger training done, upgrade bit 18, its level 0 |
| ranger line: last | marksmanship (regeneration): ranger training done, upgrade bit 19, its level 0 |
| research bit n | the row "Research spell (mage tower, temple), paladin or ogre-mage upgrade", for spell bit n (bit 20 is the paladin or ogre-mage upgrade) |
| church or altar research bit n | the row "Research spell at a church or altar", for spell bit n |

"Not yet done" in the ranger rows means that upgrade's level is 0; "done" means above 0. All four ranger rows also
need the building not working.

### Button sets

Each heading lists the unit types (PUD numbers in brackets) that use the set. Types not listed have no buttons:
types 48 and 54, the critter, farms, stables, ogre mound, refineries, oil platforms, gold mine, oil patch, start
locations, circle of power, dark portal, runestone and walls.

**footman (0), ballista (4), Kurdan and Sky'ree (22), unused type 36 (36), gryphon rider (42), Danath (46)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 2 | 116 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 117 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 118 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 178 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |
| 5 | 182 | 402 | a selected unit can attack the ground | target mode, command 17 |

**grunt (1), catapult (5), Grom Hellscream (25), Deathwing (35), unused type 37 (37), dragon (43), Korgath Bladefist (47), skeleton (55), daemon (56)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 119 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 120 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 121 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 179 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |
| 5 | 183 | 402 | a selected unit can attack the ground | target mode, command 17 |

**peasant (2)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | always | command 2 at once |
| 2 | 116 | 394 | always | target mode, command 8 |
| 3 | 85 | 395 | always | target mode, command 27 |
| 4 | 86 | 396 | a selected harvester is not carrying | target mode, command 23 |
| 5 | 89 | 401 | a selected harvester is carrying | return goods |
| 6 | 87 | 398 | one unit selected, some basic building allowed | show the human basic build menu |
| 7 | 88 | 399 | one unit selected, some advanced building allowed | show the human advanced build menu |

**peon (3)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | always | command 2 at once |
| 2 | 119 | 394 | always | target mode, command 8 |
| 3 | 85 | 395 | always | target mode, command 27 |
| 4 | 86 | 396 | a selected harvester is not carrying | target mode, command 23 |
| 5 | 90 | 401 | a selected harvester is carrying | return goods |
| 6 | 87 | 398 | one unit selected, some basic building allowed | show the orc basic build menu |
| 7 | 88 | 399 | one unit selected, some advanced building allowed | show the orc advanced build menu |

**knight (6)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 2 | 116 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 117 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 118 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 178 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |

**ogre (7)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 119 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 120 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 121 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 179 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |

**archer (8), ranger (18), Alleria (20)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 2 | 124 | 394 | missile level = 0 | target mode, command 8 |
| 2 | 125 | 394 | missile level = 1 | target mode, command 8 |
| 2 | 126 | 394 | missile level = 2 | target mode, command 8 |
| 3 | 178 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |

**axethrower (9), berserker (19), Zuljin (53)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 127 | 394 | missile level = 0 | target mode, command 8 |
| 2 | 128 | 394 | missile level = 1 | target mode, command 8 |
| 2 | 129 | 394 | missile level = 2 | target mode, command 8 |
| 3 | 179 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |

**mage (10), Khadgar (24)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | always | command 2 at once |
| 2 | 99 | 348 | always | target mode, command 8 |
| 3 | 101 | 349 | one unit selected, spell bit 5 known | cast: command 43 |
| 4 | 94 | 351 | one unit selected, spell bit 6 known | cast: command 44 |
| 5 | 100 | 350 | one unit selected, spell bit 4 known | cast: command 42 |
| 6 | 95 | 352 | one unit selected, spell bit 7 known | cast: command 45 |
| 7 | 115 | 353 | one unit selected, spell bit 8 known | cast: command 46 |
| 8 | 105 | 354 | one unit selected, spell bit 9 known | cast: command 47 |

**death knight (11), Teron Gorefiend (21), Gul'dan (51)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | always | command 2 at once |
| 2 | 102 | 355 | always | target mode, command 8 |
| 3 | 108 | 357 | one unit selected, spell bit 14 known | cast: command 51 |
| 4 | 96 | 358 | one unit selected, spell bit 16 known | cast: command 53 |
| 5 | 114 | 369 | one unit selected, spell bit 13 known | cast: command 50 |
| 6 | 104 | 361 | one unit selected, spell bit 15 known | cast: command 52 |
| 7 | 98 | 359 | one unit selected, spell bit 17 known | cast: command 54 |
| 8 | 103 | 356 | one unit selected, spell bit 19 known | cast: command 56 |

**paladin (12), Turalyon (44), Lothar (50), Uther Lightbringer (52)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 2 | 116 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 117 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 118 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 178 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |
| 6 | 106 | 362 | one unit selected, spell bit 0 known | cast: command 38 |
| 7 | 107 | 363 | one unit selected, spell bit 1 known | cast: command 39 |
| 8 | 110 | 365 | one unit selected, spell bit 3 known | cast: command 41 |

**ogre-mage (13), Dentarg (23)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 119 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 120 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 121 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 179 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |
| 6 | 111 | 366 | one unit selected, spell bit 10 known | cast: command 48 |
| 7 | 112 | 367 | one unit selected, spell bit 11 known | cast: command 49 |
| 8 | 97 | 360 | one unit selected, spell bit 18 known | cast: command 55 |

**dwarven demolition squad (14)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 2 | 116 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 117 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 118 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 184 | 404 | Enhanced mode | target mode, command 5 |
| 8 | 142 | 406 | always | target mode, command 19 |

**goblin sappers (15)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 119 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 120 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 121 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 185 | 404 | Enhanced mode | target mode, command 5 |
| 8 | 143 | 406 | always | target mode, command 19 |

**attack peasant (16)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | always | command 2 at once |

**attack peon (17)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | always | command 2 at once |

**human tanker (26)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 157 | 392 | always | target mode, command 3 |
| 1 | 153 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 154 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 155 | 393 | ship armour level = 2 | command 2 at once |
| 3 | 54 | 309 | one unit selected | build human oil platform |
| 4 | 161 | 397 | a selected harvester is not carrying | target mode, command 23 |
| 5 | 158 | 401 | a selected harvester is carrying | return goods |

**orc tanker (27)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 156 | 392 | always | target mode, command 3 |
| 1 | 150 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 151 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 152 | 393 | ship armour level = 2 | command 2 at once |
| 3 | 55 | 309 | one unit selected | build orc oil platform |
| 4 | 160 | 397 | a selected harvester is not carrying | target mode, command 23 |
| 5 | 159 | 401 | a selected harvester is carrying | return goods |

**human transport (28)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 157 | 392 | always | target mode, command 3 |
| 1 | 153 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 154 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 155 | 393 | ship armour level = 2 | command 2 at once |
| 2 | 162 | 405 | one unit selected, it has passengers | unload |

**orc transport (29)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 156 | 392 | always | target mode, command 3 |
| 1 | 150 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 151 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 152 | 393 | ship armour level = 2 | command 2 at once |
| 2 | 163 | 405 | one unit selected, it has passengers | unload |

**elven destroyer (30), battleship (32), unused type 34 (34)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 157 | 392 | always | target mode, command 3 |
| 1 | 153 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 154 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 155 | 393 | ship armour level = 2 | command 2 at once |
| 2 | 144 | 394 | cannon level = 0 | target mode, command 8 |
| 2 | 145 | 394 | cannon level = 1 | target mode, command 8 |
| 2 | 146 | 394 | cannon level = 2 | target mode, command 8 |
| 3 | 184 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |
| 5 | 182 | 402 | a selected unit can attack the ground | target mode, command 17 |

**troll destroyer (31), juggernaught (33)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 156 | 392 | always | target mode, command 3 |
| 1 | 150 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 151 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 152 | 393 | ship armour level = 2 | command 2 at once |
| 2 | 147 | 394 | cannon level = 0 | target mode, command 8 |
| 2 | 148 | 394 | cannon level = 1 | target mode, command 8 |
| 2 | 149 | 394 | cannon level = 2 | target mode, command 8 |
| 3 | 185 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |
| 5 | 183 | 402 | a selected unit can attack the ground | target mode, command 17 |

**gnomish submarine (38)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 157 | 392 | always | target mode, command 3 |
| 1 | 153 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 154 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 155 | 393 | ship armour level = 2 | command 2 at once |
| 2 | 144 | 394 | cannon level = 0 | target mode, command 8 |
| 2 | 145 | 394 | cannon level = 1 | target mode, command 8 |
| 2 | 146 | 394 | cannon level = 2 | target mode, command 8 |
| 3 | 184 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |

**giant turtle (39)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 156 | 392 | always | target mode, command 3 |
| 1 | 150 | 393 | ship armour level = 0 | command 2 at once |
| 1 | 151 | 393 | ship armour level = 1 | command 2 at once |
| 1 | 152 | 393 | ship armour level = 2 | command 2 at once |
| 2 | 147 | 394 | cannon level = 0 | target mode, command 8 |
| 2 | 148 | 394 | cannon level = 1 | target mode, command 8 |
| 2 | 149 | 394 | cannon level = 2 | target mode, command 8 |
| 3 | 185 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |

**gnomish flying machine (40)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 3 | 184 | 404 | Enhanced mode | target mode, command 5 |

**goblin zeppelin (41), eye of Kilrogg (45)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 3 | 185 | 404 | Enhanced mode | target mode, command 5 |

**Cho'gall (49)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 119 | 394 | weapon level = 0 | target mode, command 8 |
| 2 | 120 | 394 | weapon level = 1 | target mode, command 8 |
| 2 | 121 | 394 | weapon level = 2 | target mode, command 8 |
| 3 | 179 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |
| 6 | 111 | 366 | always | cast: command 48 |
| 7 | 112 | 367 | always | cast: command 49 |
| 8 | 97 | 360 | always | cast: command 55 |

**human barracks (60)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 2 | 283 | unit requirement | train footman |
| 1 | 4 | 272 | unit requirement | train archer |
| 1 | 6 | 273 | unit requirement | train ranger |
| 2 | 16 | 284 | unit requirement | train ballista |
| 3 | 8 | 275 | unit requirement | train knight |
| 3 | 10 | 277 | unit requirement | train paladin |

**orc barracks (61)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 3 | 282 | unit requirement | train grunt |
| 1 | 5 | 271 | unit requirement | train axethrower |
| 1 | 7 | 274 | unit requirement | train berserker |
| 2 | 17 | 285 | unit requirement | train catapult |
| 3 | 9 | 276 | unit requirement | train ogre |
| 3 | 11 | 278 | unit requirement | train ogre-mage |

**church (62)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 10 | 370 | research bit 20 | research paladins (33) |
| 0 | 107 | 385 | church or altar research bit 1 | research healing (35) |
| 1 | 110 | 387 | church or altar research bit 3 | research exorcism (36) |

**altar of storms (63)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 11 | 371 | research bit 20 | research ogre-mages (32) |
| 0 | 112 | 389 | church or altar research bit 11 | research bloodlust (44) |
| 1 | 97 | 382 | church or altar research bit 18 | research runes (50) |

**human scout tower (64), human guard tower (96), human cannon tower (98)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 75 | 319 | guard tower (see requirements) | become human guard tower |
| 0 | 76 | 320 | cannon tower (see requirements) | become human cannon tower |

**orc scout tower (65), orc guard tower (97), orc cannon tower (99)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 77 | 319 | guard tower (see requirements) | become orc guard tower |
| 0 | 78 | 320 | cannon tower (see requirements) | become orc cannon tower |

**gnomish inventor (68)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 28 | 294 | unit requirement | train gnomish flying machine |
| 1 | 12 | 297 | unit requirement | train dwarven demolition squad |

**goblin alchemist (69)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 29 | 293 | unit requirement | train goblin zeppelin |
| 1 | 13 | 298 | unit requirement | train goblin sappers |

**gryphon aviary (70)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 30 | 295 | unit requirement | train gryphon rider |

**dragon roost (71)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 31 | 296 | unit requirement | train dragon |

**human shipyard (72)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 18 | 286 | unit requirement | train human tanker |
| 1 | 22 | 287 | unit requirement | train elven destroyer |
| 2 | 20 | 288 | unit requirement | train human transport |
| 3 | 24 | 289 | unit requirement | train battleship |
| 4 | 26 | 291 | unit requirement | train gnomish submarine |

**orc shipyard (73)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 19 | 286 | unit requirement | train orc tanker |
| 1 | 23 | 287 | unit requirement | train troll destroyer |
| 2 | 21 | 288 | unit requirement | train orc transport |
| 3 | 25 | 290 | unit requirement | train juggernaught |
| 4 | 27 | 292 | unit requirement | train giant turtle |

**town hall (74), keep (88), castle (90)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 0 | 270 | unit requirement | train peasant |
| 1 | 66 | 317 | hall to keep (see requirements) | become keep |
| 1 | 68 | 315 | keep to castle (see requirements) | become castle |

**great hall (75), stronghold (89), fortress (91)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 1 | 269 | unit requirement | train peon |
| 1 | 67 | 318 | hall to keep (see requirements) | become stronghold |
| 1 | 69 | 316 | keep to castle (see requirements) | become fortress |

**elven lumber mill (76)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 125 | 332 | two-level upgrade, level 0 (missile) | upgrade arrow 1 (4) |
| 0 | 126 | 332 | two-level upgrade, level 1 (missile) | upgrade arrow 2 (5) |
| 3 | 6 | 338 | ranger line: training | upgrade ranger training (24) |
| 3 | 133 | 340 | ranger line: scouting | upgrade ranger scouting (26) |
| 4 | 132 | 339 | ranger line: range | upgrade longbow (25) |
| 5 | 134 | 341 | ranger line: last | upgrade marksmanship (27) |

**troll lumber mill (77)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 128 | 330 | two-level upgrade, level 0 (missile) | upgrade spear 1 (6) |
| 0 | 129 | 330 | two-level upgrade, level 1 (missile) | upgrade spear 2 (7) |
| 3 | 7 | 342 | ranger line: training | upgrade berserker training (28) |
| 3 | 136 | 344 | ranger line: scouting | upgrade berserker scouting (30) |
| 4 | 135 | 343 | ranger line: range | upgrade lighter axes (29) |
| 5 | 137 | 345 | ranger line: last | upgrade regeneration (31) |

**human foundry (78)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 145 | 335 | two-level upgrade, level 0 (cannon) | upgrade human cannon 1 (12) |
| 0 | 146 | 335 | two-level upgrade, level 1 (cannon) | upgrade human cannon 2 (13) |
| 1 | 154 | 336 | two-level upgrade, level 0 (ship armour) | upgrade human ship armour 1 (16) |
| 1 | 155 | 336 | two-level upgrade, level 1 (ship armour) | upgrade human ship armour 2 (17) |

**orc foundry (79)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 148 | 335 | two-level upgrade, level 0 (cannon) | upgrade orc cannon 1 (14) |
| 0 | 149 | 335 | two-level upgrade, level 1 (cannon) | upgrade orc cannon 2 (15) |
| 1 | 151 | 336 | two-level upgrade, level 0 (ship armour) | upgrade orc ship armour 1 (18) |
| 1 | 152 | 336 | two-level upgrade, level 1 (ship armour) | upgrade orc ship armour 2 (19) |

**mage tower (80)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 14 | 281 | unit requirement | train mage |
| 1 | 94 | 374 | research bit 6 | research slow (39) |
| 2 | 100 | 373 | research bit 4 | research flame shield (37) |
| 3 | 95 | 375 | research bit 7 | research invisibility (40) |
| 4 | 115 | 376 | research bit 8 | research polymorph (41) |
| 5 | 105 | 377 | research bit 9 | research blizzard (42) |

**temple of the damned (81)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 15 | 279 | unit requirement | train death knight |
| 1 | 96 | 380 | research bit 16 | research haste (48) |
| 2 | 114 | 391 | research bit 13 | research raise dead (45) |
| 3 | 104 | 383 | research bit 15 | research whirlwind (47) |
| 4 | 98 | 381 | research bit 17 | research unholy armour (49) |
| 5 | 103 | 378 | research bit 19 | research death and decay (51) |

**human blacksmith (82)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 117 | 329 | two-level upgrade, level 0 (weapon) | upgrade sword 1 (0) |
| 0 | 118 | 329 | two-level upgrade, level 1 (weapon) | upgrade sword 2 (1) |
| 1 | 165 | 333 | two-level upgrade, level 0 (shield) | upgrade human shield 1 (8) |
| 1 | 166 | 333 | two-level upgrade, level 1 (shield) | upgrade human shield 2 (9) |
| 2 | 140 | 347 | two-level upgrade, level 0 (siege) | upgrade ballista 1 (22) |
| 2 | 141 | 347 | two-level upgrade, level 1 (siege) | upgrade ballista 2 (23) |

**orc blacksmith (83)**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 120 | 331 | two-level upgrade, level 0 (weapon) | upgrade axe 1 (2) |
| 0 | 121 | 331 | two-level upgrade, level 1 (weapon) | upgrade axe 2 (3) |
| 1 | 168 | 334 | two-level upgrade, level 0 (shield) | upgrade orc shield 1 (10) |
| 1 | 169 | 334 | two-level upgrade, level 1 (shield) | upgrade orc shield 2 (11) |
| 2 | 138 | 346 | two-level upgrade, level 0 (siege) | upgrade catapult 1 (20) |
| 2 | 139 | 346 | two-level upgrade, level 1 (siege) | upgrade catapult 2 (21) |

**Human basic build menu**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 38 | 306 | building requirement | build farm |
| 1 | 42 | 299 | building requirement | build human barracks |
| 2 | 40 | 307 | building requirement | build town hall |
| 3 | 44 | 300 | building requirement | build elven lumber mill |
| 4 | 46 | 314 | building requirement | build human blacksmith |
| 6 | 60 | 303 | building requirement | build human scout tower |
| 7 | 92 | 312 | building requirement | build human wall |
| 8 | 91 | 407 | always | go back to the unit's own set |

**Human advanced build menu**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 48 | 313 | building requirement | build human shipyard |
| 1 | 52 | 311 | building requirement | build human foundry |
| 2 | 50 | 310 | building requirement | build human refinery |
| 3 | 58 | 321 | building requirement | build gnomish inventor |
| 4 | 56 | 301 | building requirement | build stables |
| 5 | 62 | 304 | building requirement | build church |
| 6 | 64 | 325 | building requirement | build mage tower |
| 7 | 72 | 323 | building requirement | build gryphon aviary |
| 8 | 91 | 407 | always | go back to the unit's own set |

**Orc basic build menu**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 39 | 306 | building requirement | build pig farm |
| 1 | 43 | 299 | building requirement | build orc barracks |
| 2 | 41 | 308 | building requirement | build great hall |
| 3 | 45 | 300 | building requirement | build troll lumber mill |
| 4 | 47 | 314 | building requirement | build orc blacksmith |
| 6 | 61 | 303 | building requirement | build orc scout tower |
| 7 | 93 | 312 | building requirement | build orc wall |
| 8 | 91 | 407 | always | go back to the unit's own set |

**Orc advanced build menu**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 49 | 313 | building requirement | build orc shipyard |
| 1 | 53 | 311 | building requirement | build orc foundry |
| 2 | 51 | 310 | building requirement | build orc refinery |
| 3 | 59 | 322 | building requirement | build goblin alchemist |
| 4 | 57 | 302 | building requirement | build ogre mound |
| 5 | 63 | 305 | building requirement | build altar of storms |
| 6 | 65 | 326 | building requirement | build temple of the damned |
| 7 | 73 | 324 | building requirement | build dragon roost |
| 8 | 91 | 407 | always | go back to the unit's own set |

**Choosing a target**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 8 | 91 | 407 | always | leave target mode |

**Placing a building**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 8 | 91 | 407 | always | leave placement mode |

**Building busy**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 8 | 91 | 408 | under construction | cancel construction |
| 8 | 91 | 410 | working, job is not training | cancel job |
| 8 | 91 | 409 | working, job is training | cancel job |

**Mixed selection, human local player**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 83 | 392 | always | target mode, command 3 |
| 1 | 164 | 393 | shield level = 0 | command 2 at once |
| 1 | 165 | 393 | shield level = 1 | command 2 at once |
| 1 | 166 | 393 | shield level = 2 | command 2 at once |
| 2 | 116 | 394 | always | target mode, command 8 |
| 3 | 178 | 404 | always | target mode, command 5 |
| 4 | 180 | 403 | always | command 13 at once |
| 5 | 182 | 402 | a selected unit can attack the ground | target mode, command 17 |

**Mixed selection, orc local player**

| Slot | Icon | String | Shown when | Does |
| --- | --- | --- | --- | --- |
| 0 | 84 | 392 | always | target mode, command 3 |
| 1 | 167 | 393 | shield level = 0 | command 2 at once |
| 1 | 168 | 393 | shield level = 1 | command 2 at once |
| 1 | 169 | 393 | shield level = 2 | command 2 at once |
| 2 | 119 | 394 | always | target mode, command 8 |
| 3 | 179 | 404 | always | target mode, command 5 |
| 4 | 181 | 403 | always | command 13 at once |
| 5 | 183 | 402 | a selected unit can attack the ground | target mode, command 17 |

## Data

| Item | Value |
| --- | --- |
| Panel | 176 × 144 px, top at screen y = 336 |
| Button | 54 × 46 px |
| Button x (columns 0, 1, 2) | 5, 61, 117 |
| Button y (rows 0, 1, 2) | 0, 47, 94 |
| Selection size | up to 9 units |
| Passenger places tested by Unload | 6 |
| Affordability messages | 438 food, 439 gold, 440 lumber, 441 oil |
| Not enough mana | 435 |
| Nowhere to return goods | 436 |
| Cancel strings | 407 (target, placement, build menus), 408 (construction), 409 (training), 410 (upgrade or research) |

## Test vectors

| Case | Expected |
| --- | --- |
| Local player selects one of its footmen, shields level 1, swords level 0 | slots 0 to 4: Move (icon 83), Stop (icon 165), Attack (icon 116), Patrol, Stand ground; slot 5 empty (a footman can't attack the ground); slots 6 to 8 empty |
| Same footman, but owned by another player | no buttons |
| A footman and a ballista selected, local player human | different types, so the human mixed set: Move, Stop, Attack, Patrol, Stand ground, and Attack ground in slot 5 (the ballista can attack the ground) |
| Two footmen selected | the footman set |
| A footman and a peasant selected, local player human | the human mixed set: Move, Stop, Attack, Patrol, Stand ground; no Attack ground |
| One peasant selected, carrying gold | Move, Stop, Attack, Repair, Return goods (slot 5); no Harvest; both build menus if anything can be built |
| Three peasants selected, the first carrying, the second not | Harvest and Return goods both shown; no build menu buttons (more than one selected) |
| A peasant and a footman selected, peasant first, peasant not carrying | they have different sets, so the mixed set; no Harvest |
| Player has never had a hall, ALOW all set | basic build menu shows only "Build town hall"; the advanced build menu button is hidden |
| Human player with a castle and a lumber mill, ALOW all set, single player | basic menu: farm, barracks, town hall, lumber mill, blacksmith, tower; no wall. Advanced menu: shipyard, inventor, stables, church, mage tower, aviary; no foundry or refinery (no shipyard) |
| Same, network game with two humans | basic menu also shows Wall in slot 7 |
| Barracks completed and idle; player has a lumber mill but no blacksmith; ALOW all set | footman (slot 0), archer (slot 1); no ballista, no knight |
| Same barracks after ranger training is done | slot 1 shows ranger instead of archer |
| Barracks working (training a footman) | only Cancel (string 409) in slot 8 |
| Lumber mill while it is under construction | only Cancel (string 408) in slot 8 |
| Blacksmith, swords level 1, sword upgrades allowed | slot 0 shows the second sword icon (118), which starts upgrade 1 |
| Blacksmith, swords level 2 | slot 0 empty |
| Two lumber mills; the first starts arrow 2 (level 1 done) | the second still shows the level-1 arrow button and can start arrow 2 again |
| Mage, single selection, knows fireball and slow only | slots 3 (fireball) and 4 (slow) shown; 5 to 8 empty |
| Two mages selected, both know fireball | no spell buttons (more than one selected) |
| Cho'gall selected, nothing researched | eye, bloodlust and runes shown in slots 6 to 8 |
| Mage clicks Fireball with 50 mana (fireball costs more) | message 435; no target mode |
| Ogre-mage with eye of Kilrogg known clicks Eye | command 48 is sent at once, no target mode |
| Transport with 2 passengers, docked | Unload sends command 29 at once on its own tile |
| Transport with no passengers | no Unload button |
| Peasant clicks Return goods, no depot reachable | message 436; nothing sent |
| Town hall, idle, ALOW bit 27 set, barracks completed | peasant (slot 0) and "Upgrade to keep" (slot 1) |
| Keep, idle, stables, lumber mill and blacksmith completed, ALOW bit 28 | peasant and "Upgrade to castle" |
| Castle | peasant only |
| Scout tower while the player has a blacksmith but no lumber mill | "Upgrade to cannon tower" in slot 0 |
| Guard tower | no buttons |
| Footman's Move clicked | panel shows only Cancel (slot 8); a click on the map sends command 3 to each selected unit |
| Mage with 99 mana selected, fireball button clicked | message 435; nothing sent |
| Mage with 100 mana, fireball button | target mode, command 43 |
| Peasant selected, player never had a hall, ALOW allows only advanced buildings | advanced build menu button hidden (Cancel in that menu doesn't count) |
| Elven lumber mill, ranger training done, longbow level 0, upgrade bit 17 allowed | longbow button ("ranger line: range") shown |

## Open questions

- Whether the hotkey compares letters regardless of case, and whether the Cancel entries' control character is
  really the Escape key.
- Each button entry also carries a word with one bit per basic command (stop, move, patrol, attack, stand ground,
  attack ground or demolish, harvest, return goods, repair). Nothing found uses it.
- Whether "the same set" in rule 5 compares the units' types (as assumed) or the buttons themselves.
- The order in which the selection is kept, which decides the primary unit and the harvest/return walk.
- The exact precedence when several of rules 2 to 5 in "Which set of buttons is shown" apply at once (for example
  target mode with a mixed selection). The cancel set of rule 2 is expected to win.
- Whether a map without an ALOW section allows everything (all bits set), including bit 31 for walls.
- Whether the human and orc versions of a building share one count for "has X" in every case (expected, since the
  same tests serve both races).
