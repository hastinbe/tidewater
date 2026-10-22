# Oracle: generated test maps

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-22, round 99).
- **Sources:** our own maps, written by the oracle harness's map generator (research side; `oracle/README.md`,
  "Multiplayer launch", `CUSTOMMAP`) in the public PUD layout (the PUD specification text: section tags, lengths,
  the `ALOW` bit orders, the `MTXM` tile code for light ground, the `SQM ` and `REGM` land values, the `UNIT` entry
  layout). A generated map holds no game data: the unit and upgrade data sections say "use the default data", so the
  game uses its own data files at run time (`game/setup.md`, "Section effects"). The sea option's tile codes are the
  PUD text's general tile codes (its tile appendix: light water, light coast, and the "water and coast" boundary tiles)
  and its `SQM ` values (dirt 0x0011, coast 0x0082, water 0x0040); the same three tile, movement and region values,
  with a water region number of 1, are what the shipped maps hold along a straight shore (game data, checked on
  `Murky River BNE.pud`). Nothing here comes from the program.
- **Changes:** 2026-10-22: first version (round 99: the map `R99-tests.pud` and its trace).
- **Changes:** 2026-10-22, round 100 (`ERRATA-round100-traces.md`, section 4; reviewed by a spec-review session,
  2026-10-22): the optional sea along the bottom edge, the tile order of the map sections, and the map
  `R100-tests.pud` with its trace. `R99-tests.pud` is unchanged (it has no sea).

## Behaviour

A multiplayer-launch trace whose header `map` is a name of the form `R<round>-<name>.pud` (a capital R and a round
number, for example `"R99-tests.pud"`) plays a **generated test map**, not a map from the game's `Maps/` folder. Its
whole content is given below, so a comparer can build the same file byte for byte with its own code and load it as
the trace's map; nothing is read from the install except the default unit and upgrade data, as for any map. The
trace is otherwise an ordinary multiplayer-launch trace (`oracle/trace-format.md`, "Multiplayer launch"): the lobby
options, start seed, opponents and Fixed Order come from its header.

### The file

Every generated map is the same run of sections, in this order (each a 4-character tag, a 32-bit little-endian
length and the data; all numbers little-endian). N is the map size; P is the number of players the map defines (2 to
8), players 0 to P − 1.

| Order | Tag | Length | Content |
| --- | --- | --- | --- |
| 1 | `TYPE` | 16 | The characters `WAR2 MAP`, two zero bytes, the bytes 0x0A and 0xFF, then the map's id tag as a 32-bit value |
| 2 | `VER ` | 2 | 0x11 |
| 3 | `DESC` | 32 | The map's description (at most 31 characters), padded with zero bytes |
| 4 | `OWNR` | 16 | 5 (human) for players 0 to P − 1, 3 (nobody) for the slots up to 14, and 2 for slot 15. The lobby makes the rows it is told to computers |
| 5 | `ERA ` | 2 | 0 (forest) |
| 6 | `DIM ` | 4 | N, N |
| 7 | `UDTA` | 5696 | The word 1 ("use default"), then 5694 zero bytes |
| 8 | `ALOW` | 384 | The six per-player words for all 16 slots: units and buildings allowed 0xFFFFFFFF; spells known as the map gives (bit order of the PUD text; 0 for a player that knows none and for every slot from P on); spells allowed to research 0x000FFFFB; spells being researched 0; upgrades allowed 0xFFFFFFFF; upgrades being researched 0 |
| 9 | `UGRD` | 782 | The word 1 ("use default"), then 780 zero bytes |
| 10 | `SIDE` | 16 | Each player's race (0 human, 1 orc), 0 for the slots up to 14, 2 for slot 15 |
| 11 | `SGLD` | 32 | Each player's starting gold (16-bit), 0 from slot P on |
| 12 | `SLBR` | 32 | Each player's starting lumber, likewise |
| 13 | `SOIL` | 32 | Each player's starting oil, likewise |
| 14 | `AIPL` | 16 | 16 zero bytes |
| 15 | `MTXM` | 2 N² | Every tile 0x0050 (light ground), except the sea rows ("Sea") |
| 16 | `SQM ` | 2 N² | Every word 0x0001 (land), except the sea rows |
| 17 | `OILM` | N² | Zero bytes |
| 18 | `REGM` | 2 N² | Every word 0x4000 (land), except the sea rows |
| 19 | `UNIT` | 8 per unit | The map's units in the map's order: x (16-bit), y (16-bit), type (8-bit, the PUD unit number), owner (8-bit), data (16-bit: 0 for a unit that isn't held, 1 for a held unit, or a gold mine's gold / 2500) |

The four map sections (15 to 18) hold one value per tile, row by row: first row y = 0 (the top) from x = 0 to
N − 1, then row y = 1, and so on to row y = N − 1, so the value of tile (x, y) is entry y × N + x.

**Sea (optional, round 100).** A map may have a sea of K rows along its bottom edge, K from 1 to N − 4. Then the
bottom K + 2 rows of `MTXM`, `SQM ` and `REGM` hold these values instead (every tile of each row the same; `OILM` stays
zero); the rows above them are as in the table:

| Rows | `MTXM` tile | `SQM ` | `REGM` |
| --- | --- | --- | --- |
| y = N − K − 2 (coast) | 0x0030 (light coast) | 0x0011 | 0x4000 (land) |
| y = N − K − 1 (shore) | 0x02B0 (a water and coast boundary tile) | 0x0082 | 0x4000 (land) |
| y = N − K to N − 1 (water) | 0x0010 (light water) | 0x0040 | 0x0001 (sea region 1) |

A map without a sea is built exactly as before: `R99-tests.pud` has none and its bytes are unchanged.

`UGRD` comes after `ALOW`, so by the reading rule of `game/setup.md` ("Reading sections") it is skipped as an entry
already passed; the game then uses its default upgrade data, which is what the section asks for anyway. With
`ALOW` present, the spells-known word replaces the start value 0x4020 of `game/setup.md` ("Before the map is read").

### Units and slots

The first P units in each map are the players' start locations (types 0x5E and 0x5F), which create no unit in these
games. Every other unit is created by the setup of `game/setup.md`; in the traced games the first one listed after the
start locations takes slot 1599 and each later one the next lower slot (round 99: the gold mine, listed last of 18,
took slot 1582). A held unit (data 1) starts with AI goal kind 1 (`oracle/trace-format.md`, `aigoals`).

## Data

### R99-tests.pud (round 99)

Size N = 64, P = 4, id tag 0x7199, description `tidewater r99 tests`. File length 36 062 bytes, SHA-256
`ae532b4ec6bbd8836de20793c2b5f136146389e5fdbdbecaae891b41dfb86091`.

| Player | Race | Gold | Lumber | Oil | Spells known | In the trace |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | human | 0 | 0 | 0 | none | The traced human (A) |
| 1 | orc | 0 | 0 | 0 | none | The second human (B), idle |
| 2 | human | 0 | 0 | 0 | invisibility (bit 7, word 128) | Computer |
| 3 | human | 2000 | 1000 | 0 | none | Computer |

Units, in file order (type numbers as in the PUD text; slot as the traced setup gave it):

| # | Tile (x, y) | Type | Owner | Data | Slot |
| --- | --- | --- | --- | --- | --- |
| 1 | (30, 40) | 0x5E human start location | 0 | 0 | none |
| 2 | (30, 4) | 0x5F orc start location | 1 | 0 | none |
| 3 | (4, 52) | 0x5E human start location | 2 | 0 | none |
| 4 | (40, 33) | 0x5E human start location | 3 | 0 | none |
| 5 | (8, 8) | 0x0A mage | 0 | 0 | 1599 |
| 6 | (10, 8) | 0x0B death knight | 1 | 0 | 1598 |
| 7 | (8, 18) | 0x0A mage | 0 | 0 | 1597 |
| 8 | (10, 18) | 0x01 grunt | 1 | 0 | 1596 |
| 9 | (50, 5) | 0x2B dragon | 1 | 0 | 1595 |
| 10 | (44, 8) | 0x28 gnomish flying machine | 0 | 0 | 1594 |
| 11 | (40, 14) | 0x00 footman | 0 | 0 | 1593 |
| 12 | (50, 12) | 0x01 grunt | 1 | 0 | 1592 |
| 13 | (2, 56) | 0x4A town hall | 2 | 0 | 1591 |
| 14 | (12, 42) | 0x00 footman | 2 | 1 (held) | 1590 |
| 15 | (12, 44) | 0x0A mage | 2 | 1 (held) | 1589 |
| 16 | (12, 46) | 0x08 elven archer | 2 | 1 (held) | 1588 |
| 17 | (34, 58) | 0x40 scout tower | 2 | 0 | 1587 |
| 18 | (47, 58) | 0x04 ballista | 0 | 0 | 1586 |
| 19 | (47, 60) | 0x04 ballista | 0 | 0 | 1585 |
| 20 | (40, 30) | 0x02 peasant | 3 | 0 | 1584 |
| 21 | (41, 30) | 0x02 peasant | 3 | 0 | 1583 |
| 22 | (46, 26) | 0x5C gold mine | 15 | 20 (50 000 gold) | 1582 |

Trace: `mp-r99-tests-mp-r99-tests-fixed-aigoals-airesearch-aiworkers-5400` (Enhanced; Fixed Order, 2 opponents,
start seed 1; players 2 and 3 are the computer rows). Its input: at n = 2 the flying machine patrols to (56, 8); at
n = 4 the footman 1593 is given a smart move (command 3) on the flying machine 1594, which makes it follow; at n = 4650 both ballistas attack the scout
tower 1587.

### R100-tests.pud (round 100)

Size N = 64, P = 2, id tag 0x7200, description `tidewater r100 tests`, sea K = 12 (row 50 coast, row 51 shore, rows
52 to 63 water). File length 36 030 bytes, SHA-256
`009f7bed418ffde11d22aa8d2ad1835ad94b3776aee2f7dd47b85a8bd995eda8`.

| Player | Race | Gold | Lumber | Oil | Spells known | In the trace |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | human | 10 000 | 10 000 | 1 000 | none | The traced human (A) |
| 1 | orc | 0 | 0 | 0 | none | The second human (B), idle |

Units, in file order (every data value 0):

| # | Tile (x, y) | Type | Owner | Slot |
| --- | --- | --- | --- | --- |
| 1 | (10, 10) | 0x5E human start location | 0 | none |
| 2 | (55, 10) | 0x5F orc start location | 1 | none |
| 3 | (40, 54) | 0x1C human transport | 0 | 1599 |
| 4 | (44, 48) | 0x00 footman | 0 | 1598 |
| 5 | (45, 48) | 0x00 footman | 0 | 1597 |
| 6 | (46, 48) | 0x00 footman | 0 | 1596 |
| 7 | (47, 48) | 0x00 footman | 0 | 1595 |
| 8 | (48, 48) | 0x00 footman | 0 | 1594 |
| 9 | (49, 48) | 0x00 footman | 0 | 1593 |
| 10 | (24, 48) | 0x00 footman | 0 | 1592 |
| 11 | (6, 6) | 0x4A town hall | 0 | 1591 |
| 12 | (14, 6) | 0x3C human barracks | 0 | 1590 |
| 13 | (6, 17) | 0x04 ballista | 0 | 1589 |
| 14 | (11, 8) | 0x02 peasant | 0 | 1588 |
| 15 | (26, 20) | 0x02 peasant | 0 | 1587 |
| 16 | (26, 23) | 0x02 peasant | 0 | 1586 |
| 17 | (41, 47) | 0x01 grunt | 1 | 1585 |
| 18 | (32, 46) | 0x2B dragon | 1 | 1584 |

Trace: `mp-r100-tests-mp-r100-tests-fixed-goals-3000` (Enhanced; Fixed Order, no computer opponents, start seed 1;
extra `goals`). Its input (scenario `mp-r100-tests`; each command lands about 30 cycles after the n it is read at, by
the turn latency of `net/lockstep.md`): at n = 2 footmen 1598 to 1593 are given a smart move (command 3) onto the
transport and the ballista attacks the hall; n = 10 peasant 1587 builds a farm (type 0x3A) at (28, 20); n = 20 the hall
starts its upgrade to a keep (type 0x58); n = 100 peasant 1586 repairs the farm site (slot 1583, created at n = 111);
n = 480 the ballista stops; n = 520 peasant 1588 repairs the hall; n = 600 footman 1592 is given a smart move onto the
transport; n = 2150 the ballista attacks the keep and stops at n = 2400; n = 2450 peasant 1588 repairs the keep.

## Test vectors

| Input | Expected |
| --- | --- |
| Build `R99-tests.pud` from the tables above | 36 062 bytes with the SHA-256 above |
| Build `R100-tests.pud` from the tables above | 36 030 bytes with the SHA-256 above |
| Load `R100-tests.pud` as a multiplayer map with the header of its trace | The trace's setup record: 16 units in slots 1584 to 1599 as listed; player 0 has 10 000 gold, 10 000 lumber and 1 000 oil. In the trace the boarding command of n = 2 docks the transport at (42, 51) at n = 81 (order 32) |
| Load it as a multiplayer map with the header of the trace above | The trace's setup record: 18 units in slots 1582 to 1599 as listed; gold 2000 and lumber 1000 for player 3, 0 elsewhere; units 1588 to 1590 held (AI goal kind 1 in the `aigoals` research extra); player 2's spells known 128 at n = 1 (`airesearch`). The research extras are only a check here: a comparer still ignores them |

## Open questions

- Only a straight sea along the bottom edge is described. Other coast shapes need more boundary tile codes and will
  be added with the first map that uses them.
- A transport placed elsewhere on the R100 map's sea (for example at (42, 57) or (42, 55)) sailed away instead of
  docking in round-100 probe captures; the cause wasn't studied. Maps that need a docked transport use the R100
  placement.
