# UI: workers and tankers carrying goods

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-15)
- **Sources:** the sprite files `Art\Unit\Human\peon.grp`, `peong.grp`, `peonl.grp`, `tanker.grp`, `tankero.grp` and
  their orc counterparts under `Art\Unit\Orc\` (headers and frame data compared), the default unit data
  (`unitdata.dat`: type flags bit 8 "peon" and bit 9 "tanker", as named in the PUD specification), the animation
  scripts in `Rez\script.bin`, static study of the 2.02 program (research notes under `research/notes/`, "Round 69"),
  and observation of the running game (a Human01 peasant carrying lumber, Enhanced campaign launch). Round-69 errata
  (`ERRATA-round69.md`, finding 3). Only the lumber walk was seen on the running game; the rest is file data and
  static study, as each test vector says.

## Behaviour

A worker or tanker that carries a load is drawn from a different sprite file. The animation scripts, frame numbers,
facing columns, mirroring and placement don't change (`core/animation-scripts.md`, `ui/map-drawing.md`, "Frame and
mirroring" and "Placement"); only the file the frame is taken from does. The carrying walk is slower (move loop 22
instead of 16, `core/animation-scripts.md`, "Measured timings"; whether lumber counts is open there), which is a
script matter, not a file one.

### Which file

**Workers** (unit types with the peon flag, type flags bit 8; in the default data the peasant, type 2, and the peon,
type 3):

1. If the unit is carrying (`economy/harvest.md`, the carrying mark) and the animation is not animation 4 (attack,
   which is also the chopping and mining swing):
   - gather mode gold: the gold file (`peong.grp`);
   - any other gather mode (lumber, or none): the lumber file (`peonl.grp`).
2. Otherwise: the plain file (`peon.grp`).

The race folder is chosen by unit type, not by owner: type 2 uses `Art\Unit\Human\`, every other worker type uses
`Art\Unit\Orc\`.

**Tankers** (unit types with the tanker flag, type flags bit 9; in the default data types 0x1A human and 0x1B orc):

- carrying: the oil file (`tankero.grp`), whatever the animation;
- not carrying: the plain file (`tanker.grp`).

Type 0x1A uses `Art\Unit\Human\`, every other tanker type `Art\Unit\Orc\`.

For every other unit type the carrying mark plays no part: its picture is as in `ui/map-drawing.md` ("Terms").

### When the file is chosen

The file is chosen again only at these moments; between them the unit keeps the file it has:

- **The carrying mark changes.** The rule above is applied with the unit's current animation:
  - set when the unit takes a load inside a gold mine or oil platform, so it comes out carrying (`economy/harvest.md`,
    "Entering a mine, platform or depot");
  - set when a worker fells its share of a tree (`economy/harvest.md`, "Lumber"; the frame goes to 0 at the same
    time). Its animation is still 4 (chopping) at that moment, so the plain file stays; the return order (or idle)
    that takes over in the same update then chooses the lumber file;
  - cleared when the load is delivered (`economy/harvest.md`, "Delivering goods");
  - cleared when the unit is killed, so a carrying worker or tanker dies in the plain file's death frames.
- **An animation is started** by an order taking over or the unit going idle (`core/animation-scripts.md`, "Running
  a script"). The rule above is applied with the animation being started: the animation of the new order in the
  order list of `orders/orders.md`.

A unit that switches to its move animation for a step, or back to its order's own animation on arrival, within the
same order doesn't have its file chosen again. So a carrying worker whose new order has animation 4 (an attack, a
harvest order 23 or a repair order 27, for example) is chosen with animation 4 and shows the plain file (no load in its
hands) while it walks to its target and works there. It shows the carrying file again when it next goes idle or takes
an order with another animation. A carrying worker walking home in a return order (animation 2) or a move order shows
the carrying file throughout.

### Frames while carrying

The carrying files use the same frame numbers as the plain file. Each frame drawn is the facing column (0 to 4,
mirrored for south-west, west and north-west) plus the script's frame number. The worker and tanker frame numbers set
by the default scripts are:

| Animation | Worker frame numbers, in script order | Drawn from, while carrying |
| --- | --- | --- |
| Still (2) | 0 | carrying file |
| Move (3) | 0 at the start, then a loop of 5, 10, 0, 15, 20 | carrying file |
| Attack, chop, mine (4) | 0 at the start, then a loop of 25, 30, 35, 40, 45, 25 | plain file (the rule above) |
| Death (1) | 50, 55, 60 | plain file (the mark is cleared at the kill) |

| Animation | Tanker frame numbers | Drawn from, while carrying |
| --- | --- | --- |
| Still and move | 0 | oil file |
| Death | 5, 10 | plain file |

So a carrying worker shows only the 25 walk frames 0 to 24 (5 poses × 5 columns) of its carrying file in normal play.
In the carrying files, frames 25 to 49 are exact copies of frames 0 to 24 and frames 50 to 64 exact copies of frames
0 to 14 (the walk poses, not attack or death pictures).

## Data

| File | Frames | Size | Contents |
| --- | --- | --- | --- |
| `Art\Unit\Human\peon.grp`, `Art\Unit\Orc\peon.grp` | 65 | 72 × 72 | walk 0–24, attack 25–49, death 50–64 |
| `Art\Unit\Human\peong.grp`, `Art\Unit\Orc\peong.grp` | 65 | 72 × 72 | carrying gold: walk 0–24; 25–49 = 0–24; 50–64 = 0–14 |
| `Art\Unit\Human\peonl.grp`, `Art\Unit\Orc\peonl.grp` | 65 | 72 × 72 | carrying lumber: same layout as the gold file |
| `Art\Unit\Human\tanker.grp`, `Art\Unit\Orc\tanker.grp` | 15 | 72 × 72 | move 0–4, death 5–14 |
| `Art\Unit\Human\tankero.grp`, `Art\Unit\Orc\tankero.grp` | 15 | 72 × 72 | carrying oil: same layout |

| Unit type | Plain | Carrying |
| --- | --- | --- |
| 2 peasant | `Human\peon.grp` | gold `Human\peong.grp`; lumber `Human\peonl.grp` |
| 3 peon | `Orc\peon.grp` | gold `Orc\peong.grp`; lumber `Orc\peonl.grp` |
| 0x1A human tanker | `Human\tanker.grp` | `Human\tankero.grp` |
| 0x1B orc tanker | `Orc\tanker.grp` | `Orc\tankero.grp` |

## Test vectors

| Input | Expected |
| --- | --- |
| Peasant fells its share of a tree (gather mode lumber), then walks south toward the town hall | Column 4 plus the move script's frame numbers (4, then 9, 14, 4, 19, 24 repeating) from `Human\peonl.grp`, not mirrored. Seen on the running game (Human01) |
| Peon comes out of a gold mine with a load, facing west | Frames from `Orc\peong.grp`, column 2, mirrored (static study) |
| Peon carrying gold delivers it at a great hall | Comes out drawn from `Orc\peon.grp` (static study) |
| Peasant carrying lumber is given an attack command on a grunt | `Human\peon.grp` from the attack order's take-over (walking and swinging); after the attack ends and it goes idle, `Human\peonl.grp` frame 0 + column (static study) |
| Peasant carrying gold is given a harvest command on a tree | `Human\peon.grp` from the take-over of order 23 (animation 4) while it walks and chops (static study) |
| Peasant carrying gold is killed | Death frames 50, 55, 60 + column from `Human\peon.grp` (static study) |
| Orc tanker leaves an oil platform with oil | `Orc\tankero.grp` until it delivers; `Orc\tanker.grp` after (static study) |
| Human tanker carrying oil is sunk | Death frames 5, 10 + column from `Human\tanker.grp` (static study) |
| A peasant (type 2) owned by an orc player carrying gold | `Human\peong.grp`: the folder follows the type, not the owner (static study) |

## Open questions

- **Not yet seen on the running game:** the gold file, the tanker files and the animation-4 exception (a carrying
  worker given an attack or a harvest command). A worker chopping a tree isn't carrying yet (the mark is set when it
  fells its share), so the exception shows only for a worker that already carries a load. Trace request: stage a
  carrying worker given an attack, and a carrying tanker, in Human01 or Orc09, and capture the framebuffer while they
  walk.
- **Other worker types.** Custom data could give the peon flag to other types; they would use the orc folder and
  must have matching files there. Not tested.
