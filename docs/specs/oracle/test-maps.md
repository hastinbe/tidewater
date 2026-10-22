# Oracle: generated test maps

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-22, round 99).
- **Sources:** our own maps, written by the oracle harness's map generator (research side; `oracle/README.md`,
  "Multiplayer launch", `CUSTOMMAP`) in the public PUD layout (the PUD specification text: section tags, lengths,
  the `ALOW` bit orders, the `MTXM` tile code for light ground, the `SQM ` and `REGM` land values, the `UNIT` entry
  layout). A generated map holds no game data: the unit and upgrade data sections say "use the default data", so the
  game uses its own data files at run time (`game/setup.md`, "Section effects"). Nothing here comes from the program.
- **Changes:** 2026-10-22: first version (round 99: the map `R99-tests.pud` and its trace).

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
| 15 | `MTXM` | 2 N² | Every tile 0x0050 (light ground) |
| 16 | `SQM ` | 2 N² | Every word 0x0001 (land) |
| 17 | `OILM` | N² | Zero bytes |
| 18 | `REGM` | 2 N² | Every word 0x4000 (land) |
| 19 | `UNIT` | 8 per unit | The map's units in the map's order: x (16-bit), y (16-bit), type (8-bit, the PUD unit number), owner (8-bit), data (16-bit: 0 for a unit that isn't held, 1 for a held unit, or a gold mine's gold / 2500) |

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

## Test vectors

| Input | Expected |
| --- | --- |
| Build `R99-tests.pud` from the tables above | 36 062 bytes with the SHA-256 above |
| Load it as a multiplayer map with the header of the trace above | The trace's setup record: 18 units in slots 1582 to 1599 as listed; gold 2000 and lumber 1000 for player 3, 0 elsewhere; units 1588 to 1590 held (AI goal kind 1 in the `aigoals` research extra); player 2's spells known 128 at n = 1 (`airesearch`). The research extras are only a check here: a comparer still ignores them |

## Open questions

- Generated maps write light ground only. A coast or water tile (for the transport requests OA3 and later) needs the
  water tile codes and the matching `SQM ` and `REGM` values; they will be added here with the first map that uses
  them.
