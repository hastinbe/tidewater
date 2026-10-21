# Game: starting a game from a map

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 19 (game setup)"),
  the public PUD specification text (section layouts, unit type ids) and the shipped maps. Not yet confirmed against
  the running game, except the `SIDE` races (oracle traces, round 31).
- **Changes:** 2026-10-12, round-31 errata: the map's `SIDE` race is used as it is for all 16 slots; slots 8 to 15
  are not set to neutral.
- **Changes:** 2026-10-12, round-35 errata: trace vectors confirm the starting alliances (no change to the rules).
- **Changes:** 2026-10-13, round-47 errata: every slot starts a new game with the "ever had a hall" mark set (cleared
  only in Enhanced multiplayer).
- **Changes:** 2026-10-13, round-58 errata: how a multiplayer start places its computer opponents (one spent draw,
  then custom step 3 on code-5 slots: 1 + opponents draws); `mp-dark-paths-bne-1000`, `mp-gold-rush-bne-6000` and
  `mp-rose-petal-bne-10000` vectors.
- **Changes:** 2026-10-13, round-59 corrections: a multiplayer "Fixed Order" start places no opponents by draw
  (Computer rows take their own slot numbers); zero opponents make no placement draw (static study).
- **Changes:** 2026-10-13, round-60: Fixed Order is traced (`mp-*-fixed` traces); vectors.
- **Changes:** 2026-10-21, round 88 (B-038 consolidation of the game area, `ERRATA-game-consolidation.md`, reviewed by a
  spec-review session, 2026-10-21): the body reading starts again at the beginning of the file (X9, static study; closes
  "Where the body reading starts"); a single-player custom game runs under Enhanced rules (X10, from
  `core/timing-and-rng.md`); the intro points at `game/save-load.md` and the new model `game/lifecycle.md` for loaded
  games (X11).
- **Changes:** 2026-10-21, round 87 (B-038 consolidation of vision, `ERRATA-vision-consolidation.md`, reviewed by a
  spec-review session, 2026-10-21): "Starting alliances and shared vision" gains the team-game rule (X10, static
  study, multiplayer only).

- **Changes:** 2026-10-21, round 89 (B-057 multiplayer, `ERRATA-round89-multiplayer.md`; reviewed by a spec-review
  session, 2026-10-21): "Custom choices": fog of war is a multiplayer lobby choice only, and the single-player custom
  screen never sets fixed start positions (dialog file and static study; with `ERRATA-audit-vision.md` erratum 1's
  setup half). The custom step 3 rule for fixed starts is kept as what the setting does.

## Behaviour

This spec covers how a new single-player game is built from a map file: reading the map's sections, choosing which
player slots are played and by whom, the custom-game choices, the starting alliances, creating the map's units and
the starting view. Loading a saved game is in `game/save-load.md`, and how a new and a loaded game start compare is
in `game/lifecycle.md`. The multiplayer lobby (slot choice, start-position and colour shuffles, teams) is in a later
spec; how a multiplayer start places its computer opponents is in "Choosing the players".
Choosing a campaign mission and the briefing are in `game/campaign-flow.md`.

A **campaign map** is a map started from the campaign in single player. A **custom game** is a map started from the
custom-scenario screen in single player. **Slot** means one of the 16 players of the map file (0 to 15); 15 is
neutral. The **local player** is the slot the person at this computer plays.

### Order of the steps

1. Defaults are set (see "Before the map is read").
2. The map's **header sections** are read, in the order of the header table (Data). If this fails, the game doesn't
   start.
3. Unit data and upgrade data come from the game's own files unless the map supplied its own (see "Section effects").
4. The player slots are chosen (see "Choosing the players"). In a custom game the custom choices are applied at the
   end of this step.
5. The starting alliances and shared-vision sets are set.
6. The map's **body sections** are read in the order of the body table, from the beginning of the file again: the
   header sections are skipped there as unknown tags (static study, round 88). The last one, `UNIT`, creates the
   map's units. If this step fails the game stops with a fatal error.
7. Rescue players become computer or passive computer players (see "After the units").
8. The starting view is set.
9. The sound set is chosen, then the mission start of `game/victory.md` runs, then the screen parts are set up.

### Reading sections

The file is a run of sections, each a 4-character tag, a 32-bit little-endian length and that many bytes of data.
Each of the two tables (Data) lists the tags it expects in order and marks each as required or optional. The reader
keeps a position in the table, starting at the first entry:

- A section whose length is more than the bytes left ends the reading, as if the file ended there.
- If the tag is the entry at the current position, that entry's rule reads the data and the position moves on by one.
- Otherwise the tag is looked for among the later entries of the table.
  - Not found (an unknown tag, or a tag whose entry has already been passed, such as a second copy): the section's data
    is skipped.
  - Found, and every entry between the current position and it is optional: that entry reads the data, and the
    position moves on to just after it. The skipped optional entries are never read later.
  - Found, but a required entry lies in between: the load fails.
- Reading stops with success once every entry of the table has been handled. If the file ends first, the load
  succeeds only when exactly one entry is left and it is optional.
- Each entry's rule checks the section length (Data); a wrong length fails the load.

### Before the map is read

- **Resources:** every slot's gold, lumber and oil are set to 1000. The map's `SGLD`, `SLBR` and `SOIL` sections are
  required, so these are always replaced.
- **Starting view:** "none".
- **Per-player words** (`economy/tech-tree.md`, "Per-player words from the map"), the same for all 16 slots:

  | Word | Campaign map | Any other map |
  | --- | --- | --- |
  | Units and buildings allowed | from the campaign table (Data), by mission entry | all bits set |
  | Spells known | 0x4020 (bits 5 and 14) | 0x4020 |
  | Spells allowed to research | from the campaign table | all bits set |
  | Spells being researched | 0 | 0 |
  | Upgrades allowed | from the campaign table | all bits set |
  | Upgrades being researched | 0 | 0 |

  The mission entry is the one in `game/victory.md` ("Mission rules"). A map's `ALOW` section replaces all six words
  for every slot. None of the shipped maps has one, so the campaign table decides what each mission allows.
- **Mission mode and required count:** as in `game/victory.md`.
- **Ever had a hall:** every slot's "ever had a completed town hall, keep or castle" mark (`economy/build-train.md`,
  "Fast first hall"; `ai/worker-ai.md`, "Farm") is **set**, for a campaign map and a custom game alike. Only an Enhanced
  multiplayer game (a later spec) starts with every mark cleared; there the mark is set by the player's first completed
  hall. A loaded game restores the marks as saved (traced: `orc5-10000`, `ai/worker-ai.md` vectors).

### Section effects

| Section | Effect |
| --- | --- |
| `TYPE` | The data must start with the 8 characters `WAR2 MAP` followed by a zero byte. Bytes 12 to 15 are the map's id tag |
| `VER ` | Must be 0x11 or 0x13; any other value fails the load |
| `DESC` | The description text |
| `OWNR` | One **owner code** per slot (codes below). Slot 15's code is then set to passive computer, whatever the map says |
| `ERA `, `ERAX` | The tileset is the value's low two bits: 0 forest, 1 winter, 2 wasteland, 3 swamp. `ERAX` comes later in the table, so when both are present `ERAX` wins |
| `SIGN` | Read and not used here |
| `DIM ` | Both words are read, but only the **second** (the height) is kept. The map is always square, with that size as both width and height. The map's tile sections are read as rows of that size |
| `UDTA`, `UGRD` | The first word is "use default". When it is 0 the section replaces the unit data or upgrade data. Otherwise, or when the section is absent, the game reloads its own data files (`rez\unitdata.dat`, `rez\upgrades.dat`), so nothing from an earlier map carries over |
| `ALOW` | Length 0: nothing changes. Length 384: the six per-player words for all 16 slots, in the PUD order |
| `SIDE` | One race per slot, for all 16 slots: 0 human, 1 orc, anything else neutral. The map's value is used as it is. In a multiplayer game, a slot taken by a player in the lobby keeps the race chosen there instead, unless that race is neutral |
| `SGLD`, `SLBR`, `SOIL` | Each slot's starting gold, lumber and oil (16-bit values) |
| `AIPL` | Each slot's AI script number (`ai/ai-scripts.md`, "Which script a player runs") |
| `MTXM`, `SQM `, `REGM` | The tile, movement and region maps, copied as they are (up to 128 × 128 words) |
| `OILM` | The data is ignored. Its presence converts bit 0x04 to 0x08 in every movement word (`movement/occupancy.md`) |
| `UNIT` | Creates the map's units ("Creating the map's units") |

**Owner codes** (the PUD `OWNR` values that matter here): 2 passive computer, 3 nobody, 4 computer, 5 human,
6 rescue (passive), 7 rescue (active). Codes 4 and 5 only mark which slots may be played. The setup below turns every
slot into one of: **human-controlled** (the local player), **computer**, **passive computer**, **nobody**, or a rescue
player. These are the player kinds of `movement/movement.md`.

### Choosing the players

**Campaign map.** The local player is the lowest slot (0 to 7) with code 5. Every slot with code 4 becomes a computer
player. There are no random draws. The local player's race setting comes from the campaign that was chosen (a later
spec), not from the map.

**Custom game.** The game's random generator is seeded as in `core/timing-and-rng.md` ("Seeding"), and then:

1. **Local slot.** Draw r. Starting at slot r mod 8 and going up (7 wraps to 0), the first slot with code 5 becomes
   the local player. If no slot 0 to 7 has code 5, the game stops with a fatal error.
2. **Your race.** If a race (human or orc) was chosen, it replaces the local slot's race. With "map default" the
   map's race stays. The local player's race setting is the local slot's race.
3. **Opponents.** The number chosen, or with "map default" the number of slots with code 4 (at least 1). For each
   opponent, draw r (with fixed start positions, no draw and r = 0; the single-player custom screen never sets them,
   see "Fixed start positions" below). Starting at slot r mod 8 and going up, the first
   slot with code 4 becomes a computer player. When a search finds no code-4 slot, that opponent and the rest are
   instead taken from the slots with code 5 that are left, the same way, with a new draw for each try. Opponents stop
   when the number is reached or no code-5 slot is left.
4. **Custom choices** (below).

**Multiplayer game.** The generator is seeded with the lobby's game seed (`ui/game-setup-lobby.md`). The lobby's
humans take the slots they were given, with no draw; there is no local-slot draw (custom step 1). The computer
opponents are then placed exactly as custom step 3 places them, and the humans' slots don't count as code 5: a draw
r per try, then a search from slot r mod 8 upward for code 4. When a
search finds no code-4 slot, that draw is spent and placement moves to the code-5 slots for good, with a new draw for
each opponent. These draws come before the unit-creation draws. Every multiplayer map traced so far has code 5 in
all its player slots, so the first draw always finds no code-4 slot and the setup makes **1 + (number of
opponents)** placement draws (traced: `mp-dark-paths-bne-1000`, `mp-gold-rush-bne-6000`, `mp-rose-petal-bne-10000`;
vectors below).
This count holds when there is at least one opponent: with **none**, placement stops before its first draw and
makes **no** draw. If the code-5 slots run out, fewer computers are placed than lobby rows set to Computer, and
each failed search has still spent its draw (static study; no traced map does either).

**Fixed Order.** The lobby's "Location: Fixed Order" option is the fixed-starts setting. With it, a multiplayer start
doesn't run the opponent placement at all: each human takes the slot with its own lobby row number (row i, slot i), each
lobby row set to Computer becomes a computer in the **same-numbered slot** if the map gives that slot code 4 or 5, and
there are **no** placement draws. The "no draw, r = 0" search of custom step 3 is for the single-player custom game only
(static study; traced: `mp-dark-paths-bne-fixed-1000`, `mp-gold-rush-bne-fixed-6000` and `mp-rose-petal-bne-fixed-3000`,
vectors below).

**Both.** Every slot that still has code 4 or 5 becomes **nobody**. Codes 2, 3, 6 and 7 are kept for now. Any other
code is kept as it is (see the open questions).

### Custom choices

These apply to a custom game only, after the opponents are chosen. They change slots 0 to 7 only.

**Resources:**

| Choice | Gold | Lumber | Oil |
| --- | --- | --- | --- |
| Low | 2000 (Enhanced rules: 2100) | 1000 (Enhanced: 1100) | 1000 |
| Medium | 5000 | 2000 | 2000 |
| High | 10000 | 5000 | 5000 |
| Random (Enhanced rules only) | one of Low, Medium, High: draw r, choice number 1 + floor(3r / 32768) (1 Low, 2 Medium, 3 High) | | |
| Map default, units "map default" | each slot keeps the map's values | | |
| Map default, units "one worker only" | every slot gets slot 0's values. Under Enhanced rules each is raised to at least 2100 gold, 1100 lumber and 1000 oil | | |

**Terrain:** forest, winter, wasteland or swamp replaces the map's tileset. Random (Enhanced rules only): draw r,
1 + floor(4r / 32768) gives 1 forest, 2 winter, 3 wasteland, 4 swamp. "Map default" keeps the map's tileset.
The resource draw, when there is one, comes before the terrain draw.

**Units:** "map default" or "one worker only" (see "Creating the map's units"). There is no hall: with "one worker
only" each played slot gets a single worker at its start location.

**Fog of war.** Multiplayer only: the lobby sets fog of war on or off; a single-player custom game always has fog on
(its screen has no fog control: `rez\loadcstm.bin` has "Opponents:" in that place; `ui/game-setup-lobby.md`;
`vision/fog.md`).

**Fixed start positions** only changes the opponent search above (no draws, every search starts at slot 0). The local
slot is still chosen with a draw. The single-player custom screen doesn't offer it: its Location control is hidden in
the dialog file and the screen clears the setting (static study, round 89; `ui/game-setup-lobby.md`), so a
single-player custom game always draws. Multiplayer's Fixed Order is described under "Multiplayer game" above.

### Starting alliances and shared vision

The alliance table holds, for every ordered pair of slots (a, b), whether a counts b as allied. It is first set to
"allied" everywhere. Then every pair of different slots a and b from 0 to 14 is set to "not allied", unless:

- either slot has code 6 (passive rescue), or
- one is the local player and the other has code 7 (active rescue), or
- both are computer players.

So slot 15 (neutral) counts everyone as allied and everyone counts it as allied, every slot counts itself as allied,
computer players are allied with each other, and the local player is allied with the rescue players. Passive computer
players and nobody slots are allied with no one but rescue (passive) slots, neutral and themselves. The mission start
(`game/victory.md`) may change some entries afterwards.

Each slot from 0 to 7 starts with a shared-vision set holding only itself (`vision/fog.md`). **Multiplayer:** in a
team game, team-mates are then added to each other's sets (static study; the lobby and its teams are a later spec;
`vision/fog.md`, "Shared vision").

### Creating the map's units

The `UNIT` section is a list of 8-byte records: x and y (16-bit tile coordinates), type, owner, and a 16-bit data
value. They are handled in file order (`core/unit-slots.md`, "Starting a map"). For each record:

1. If the owner is a **nobody** slot, the record is skipped.
2. If the type is the circle of power (0x64) and the mission mode is K (`game/victory.md`), the region value at the
   record's tile is remembered as "the circle's region". If several circles are listed, the last one wins. The circle
   is then handled like any other unit.
3. If the owner is a **passive computer** player (including slot 15), the unit is created, whatever its type.
4. Otherwise, if the type is a start location (0x5E human, 0x5F orc):
   - if the owner is the local player, the starting view becomes (x − 7, y − 7), each clamped at 0 (a later start
     location of the local player replaces it);
   - the unit is created only in a custom game with "one worker only"; otherwise nothing more happens.
5. Otherwise, in a custom game with "one worker only", the record is skipped; in any other game the unit is created.

**Creating** a unit converts its type to the owner's race first: an orc owner gets the orc version, any other race
(human or neutral) the human version, and types without a race pair stay as they are. A start location becomes a
worker: a peon (0x03) for an orc owner and a peasant (0x02) otherwise. The unit is created at the record's tile as in
`core/unit-slots.md` ("Creating a unit"). If creation fails the record is dropped. Then:

- an owner with code 6 or 7 gives the unit the rescuable mark (`game/rescue.md`);
- a gold mine (0x5C), an oil patch (0x5D) or an oil platform takes its amount from the data value
  (`economy/harvest.md`, "Mine and platform amounts");
- any other unit whose data value has a non-zero low byte gets the held mark (`ai/attack-ai.md`, "Held units").

### After the units

- Every slot with code 6 becomes a passive computer player, and every slot with code 7 a computer player.
- **Starting view.** If a start location of the local player set it, that is the view. Otherwise the game looks at the
  local player's units in list order (most recently created first, `core/unit-slots.md`), skipping units that are
  dying or removed; a unit inside something is not skipped. The view is centred on the first one that stores gold
  (a town hall, keep or castle, or the orc versions). If none does, the last one looked at (the earliest created) is
  used. The view position is that unit's tile − (7, 7). With no units at all it is (−7, −7). The view is then clamped
  to the view range. The saved views F2 to F4 start as this view.
- **Sound set.** The human or orc set (`ui/sounds.md`) follows the local player's race setting. One exception: in
  campaign mission entry 48 (*Beyond the Dark Portal*, human mission 11) the orc set is loaded.
- **Colours.** In single player every slot keeps its own colour: slot p uses player colour p.

## Data

### Header table (read first)

| Order | Tag | Required | Length |
| --- | --- | --- | --- |
| 1 | `TYPE` | yes | 16 (the 8 characters and a zero byte, then the id tag at bytes 12 to 15); the length isn't checked |
| 2 | `VER ` | yes | 2 |
| 3 | `DESC` | yes | 32 |
| 4 | `OWNR` | yes | 16 |
| 5 | `ERA ` | yes | 2 |
| 6 | `ERAX` | no | 2 |
| 7 | `SIGN` | no | 4 |
| 8 | `DIM ` | yes | 4 |
| 9 | `UDTA` | no | as in the PUD text (the check wasn't traced) |
| 10 | `UGRD` | no | as in the PUD text (the check wasn't traced) |
| 11 | `ALOW` | no | 0 or 384 |
| 12 | `SIDE` | yes | 16 |
| 13 | `SGLD` | yes | 32 |
| 14 | `SLBR` | yes | 32 |
| 15 | `SOIL` | yes | 32 |
| 16 | `AIPL` | no | 16 |

### Body table (read after the players are chosen)

| Order | Tag | Required | Length |
| --- | --- | --- | --- |
| 1 | `MTXM` | yes | at most 32768 |
| 2 | `SQM ` | yes | at most 32768 |
| 3 | `OILM` | yes | any |
| 4 | `REGM` | yes | at most 32768 |
| 5 | `UNIT` | yes | 8 per unit |

### Campaign allowance table

Hex words by mission entry (`game/victory.md`): units and buildings allowed, spells allowed to research, upgrades
allowed. Bit orders are those of the PUD `ALOW` section; the upgrade bit order is not decoded here.

| Entries | Units | Spells | Upgrades |
| --- | --- | --- | --- |
| 0, 1 | 04030003 | 0 | 0 |
| 2, 3 | 04070013 | 0 | 1 |
| 4, 5 | 440700D3 | 0 | 3 |
| 6, 7 | 466701D3 | 0 | 143 |
| 8, 9 | 6EE709D7 | 0 | 1157 |
| 10 | 6EEF09DF | 0 | F317F |
| 11 | 0 | 0 | 0 |
| 12 | 6EEF0BDF | 0 | F33FF |
| 13 | 6EEF09DF | 0 | F317F |
| 14 | 7FEF2BDF | 100003 | F33FF |
| 15 | 6EEF0BDF | 0 | F33FF |
| 16 | 0 | 100003 | 0 |
| 17 | 7FEF2BDF | 100C00 | F33FF |
| 18 | 7FEF2BDF | 10000B | F33FF |
| 19 | 7FEF6BDF | 140C00 | F33FF |
| 20 | 7FFF6BFF | 10005B | F33FF |
| 21 | 7FFF6BFF | 152C00 | F33FF |
| 22 | 7FFF6FFF | 1000DB | F33FF |
| 23 | 7FFF6FFF | 15AC00 | F33FF |
| 24, 26 | 7FFFFFFF | 1003DB | F33FF |
| 25, 27 | 7FFFFFFF | 1FAC00 | F33FF |
| 28 | 00000100 | 1FFFFE | F3FFF |
| 29, 31, 33, 35, 36 | 7FFF7FFF | 1FFFFF | F3FFF |
| 30 | 3FFF7FFF | 1FFFFF | F3FFF |
| 37 | 7FFF3FFF | 1FFFFF | F3FFF |
| 40 | 444500C2 | 1FFEFE | E3FFF |
| 45 | 7FFFF91F | 1C7FFF | F3C3F |
| 48 | 7FFFFBFF | 1FFFFF | F3FFF |
| 32, 34, 38, 39, 41–44, 46, 47, 49–51 | 7FFFFFFF | 1FFFFF | F3FFF |

### Constants

| Constant | Value |
| --- | --- |
| Resources before the map is read | 1000 each, all 16 slots |
| Spells known at start | 0x4020 (bits 5 and 14) |
| Start-location view offset | 7 tiles left and up |
| Low resources | 2000 / 1000 / 1000 (Enhanced 2100 / 1100 / 1000) |
| Medium resources | 5000 / 2000 / 2000 |
| High resources | 10000 / 5000 / 5000 |
| "One worker only", map-default minimums (Enhanced) | 2100 / 1100 / 1000 |
| Mine amount for data value 0 | as 120 (`economy/harvest.md`) |

## Test vectors

Draws are from the game generator; "seed 0" means the generator holds seed 0 when the slot choice starts, so the
draws are 0, 346, 130, 10982, 1090, … (`core/timing-and-rng.md`).

| Situation | Expected |
| --- | --- |
| Section order `TYPE VER DESC OWNR ERA DIM SIDE SGLD SLBR SOIL` (no optional ones) | Header loads; tileset from `ERA `; unit and upgrade data from the game's files; no AI script bytes from the map |
| `ERA ` = 1, `ERAX` = 2 | Wasteland |
| `ERA ` = 6 | Wasteland (6 & 3 = 2) |
| `DIM ` = 64, 32 | A 32 × 32 map |
| `VER ` = 0x12 | Load fails |
| `SIDE` before `UDTA` (`UDTA` comes later in the file) | `SIDE` is read (the optional `UDTA`, `UGRD`, `ALOW` are skipped over); the later `UDTA` is unknown by then and skipped; default unit data |
| `SGLD` missing, `SLBR` present | Load fails (a required entry is skipped over) |
| File ends right after `SOIL` | Header loads (only `AIPL` left, optional) |
| Unknown section `ABCD` anywhere | Skipped |
| Campaign map, codes 3, 5, 4, 4, 5, 3, 3, 3 | Local player slot 1; slots 2 and 3 computer; slot 4 nobody |
| Custom game, seed 0, codes 5, 4, 4, 5, 3, 3, 3, 3, opponents "map default" | Local slot 0 (draw 0). Opponents 2: draw 346 → slot 2; draw 130 → search from 2: slot 1. Slot 3 nobody |
| Same, opponents 3 | As above, then draw 10982 (mod 8 = 6) finds no code-4 slot; draw 1090 (mod 8 = 2) → slot 3 computer. 5 draws in all |
| Same as the first custom vector with fixed start positions | Local slot 0 (draw 0); opponents: slot 1, then slot 2 (no more draws) |
| Custom game, your race orc, local slot's map race human | Local slot plays orc; its human units in the map become the orc versions |
| Custom, Enhanced, resources random, draw 20000 | 1 + floor(60000 / 32768) = 2: Medium (5000 / 2000 / 2000) |
| Custom, Enhanced, terrain random, draw 30000 | 1 + floor(120000 / 32768) = 4: swamp |
| Custom, "one worker only", map default resources; slot 0 has 500 gold in the map, slot 1 has 9000 | Both slots: 2100 gold (Enhanced), lumber and oil likewise from slot 0, raised to the minimums |
| Custom, "one worker only"; local slot 0 (human) start location at (10, 10) and a town hall; computer slot 1 (orc) start location at (50, 50) and grunts; a neutral gold mine | A peasant for slot 0 at (10, 10), a peon for slot 1 at (50, 50), the gold mine. No town hall, no grunts. View (3, 3) |
| Campaign map, local start location at (3, 20) | View (0, 13); the start location is not created |
| Campaign map, no local start location; the local player's units listed as town hall at (30, 30), then keep at (60, 60) | View (53, 53) (the keep was created later, so it is met first) |
| Same with only two footmen, listed at (40, 30) then (20, 20) | View (33, 23) (no gold store; the earliest created) |
| A passive computer player's start location (human race) | A peasant is created for it |
| A nobody slot's units | None created |
| Campaign: slot 0 local, 1 and 2 computer, 3 passive computer, 4 code 6, 5 code 7 | 1 and 2 allied with each other; 0 not allied with 1, 2, 3; 4 allied with everyone both ways; 0 and 5 allied both ways; 1 and 5 not allied; every slot allied with 15 both ways |
| Mission entry 48 started as a campaign map | Orc sound set |
| Mission mode K, circles of power listed at (10, 10) then (40, 40) | The circle's region is the region value at (40, 40) |
| Code 6 slot after the units | Passive computer, its units keep the rescuable mark |
| Human01 and Human04 (oracle traces), `SIDE` = 1, then 0 fourteen times, then 2 | Races: orc (slot 0), human (slots 1 to 14), neutral (slot 15) |
| Campaign; slot 4 has code 6; its units are rescued into the local player's slot during the second start-up cycle (`human10` oracle trace: 1501 and 1505, owner 4 to 6, the local player, at n = 2) | The rescued units never pick the remaining slot-4 units (such as 1507) as targets: both directions are allied |
| The start-up cases of the round-35 long traces (`human5`, `human7`, `orc8`, `xhuman2`, `xorc7`): units next to code-6 or code-7 units, or computer next to computer | No acquisition between them in the first 400 cycles; `human5` slot 1531 (owner 5, computer) does acquire 1529 (owner 0, code 7) at n = 6, since code 7 is allied with the local player only |
| Multiplayer, generator seeded with 5, slot codes 0 to 7 (5, human, human, 5, 3, 3, 3, 3) (Gold Rush BNE, humans in slots 1 and 2), 2 opponents | Draw 1731 (1731 mod 8 = 3): no slot has code 4, so the draw is spent. Draw 32036 (mod 8 = 4): searching up from slot 4, the first code 5 is slot 0. Draw 21622 (mod 8 = 6): from slot 6 up, the first code 5 is slot 3. Computers in slots 0 and 3, 3 placement draws |
| `mp-gold-rush-bne-6000` (oracle trace, Enhanced): game seed 5, humans in slots 1 and 2, 2 opponents, 82 map units | `controllers` has computers in slots 0 and 3; the setup `rng` is 167 steps from 5: 3 placement draws + 2 × 82 creation draws |
| `mp-dark-paths-bne-1000` (oracle trace, Enhanced): game seed 5, codes (5, 5, 5, 3, 3, 3, 3, 3), humans in slots 0 and 1, 1 opponent, 13 map units | Computer in slot 2; setup `rng` 0x2E4642F9, 28 steps from 5 (2 placement + 26 creation draws) |
| `mp-rose-petal-bne-10000` (oracle trace, Enhanced): game seed 5, codes (5, 5, 5, 5, 5, 5, 3, 3), humans in slots 0 and 4, 4 opponents, 43 map units | Computers in slots 1, 2, 3 and 5; the setup `rng` is 91 steps from 5 (5 placement + 86 creation draws) |
| Multiplayer, 0 computer opponents | No placement draw: the setup's first draws are the unit-creation draws (static study) |
| Multiplayer with Fixed Order: humans in lobby rows 0 and 1, row 2 set to Computer, map slot 2 has code 5 | Humans in slots 0 and 1, a computer in slot 2, no placement draw (static study) |
| `mp-gold-rush-bne-fixed-6000` (oracle trace, Enhanced): Gold Rush BNE, Fixed Order, start seed 1, humans in lobby rows 0 and 1, rows 2 and 3 set to Computer | Humans in slots 0 and 1, computers in slots 2 and 3; no placement draw: the setup's 164 draws are the unit-creation draws alone (setup `rng` 0x0452EB91) |
| `mp-dark-paths-bne-fixed-1000` and `mp-rose-petal-bne-fixed-3000` (oracle traces, Enhanced), Fixed Order, 1 and 4 Computer rows | Computers in slot 2, and in slots 2 to 5; 26 and 86 setup draws, the unit-creation draws alone (their random-start twins make 28 and 91) |
| Multiplayer with Fixed Order, row 3 set to Computer, map slot 3 has code 3 | No computer in slot 3 (static study) |
| **G14.** A map whose `MTXM` section comes before its `OWNR` section in the file (static study) | The header reading skips `MTXM`; the body reading starts at the beginning again, skips `OWNR` and the other header sections as unknown, and reads `MTXM` |

## Open questions

- **Other owner codes.** A slot whose `OWNR` code is not 2 to 7 keeps the code unchanged. Code 0 is then the same
  value as "human-controlled" and code 1 the same as "computer" in every later test; 8 and up match nothing. The
  public PUD text calls 0 and 8–255 passive computer and 1 computer. No shipped map uses them; what the game does with
  a code-0 slot wasn't checked in the running game.
- **Rule mode in single player.** The Low, Random and minimum rules above depend on Enhanced rules. A single-player
  custom game runs under Enhanced rules (`core/timing-and-rng.md`, "Simulation cycles"; round 88). Whether the custom
  screen offers "random" for resources and terrain wasn't checked.
- **Random choice rewrites the options.** After a random resource or terrain choice the game stores the picked value
  in its options in place of "random". The terrain choice also clears the resource choice, so a restart with the same
  options would use "map default" resources. Whether a restart keeps the options wasn't traced.
- **Short `UNIT` section.** A `UNIT` length that isn't a multiple of 8 reads past the section. Default: treat the
  map as invalid.
- **Map sizes.** Sizes other than 32, 64, 96 and 128 load, but how the rest of the game copes with them wasn't
  checked.
- **Opponent race.** No custom choice sets the opponents' races; they keep the map's. Whether the custom screen offers
  one wasn't checked.
- **Where the body reading starts.** Answered (round 88, static study): from the beginning of the file, with the
  header's sections skipped as unknown, as the default said. Every shipped map lists `AIPL` just before `MTXM`, so
  both readings load them the same way.
- **Starting view with no units.** The view (−7, −7) is clamped to (0, 0); only a map with no units for the local
  player reaches this.
