# Movement: placing a unit on a free tile near a building or unit

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`). The ring order was checked
  by hand-simulating the search for the test vectors below. Not yet confirmed against the running game.
- **Changes:** 2026-10-12, round-42 errata: a worker coming out with a build job faces its build goal tile
  (`economy/harvest.md`).
- **Changes:** 2026-10-16 (round 76, B-038 build/train consolidation, `ERRATA-buildtrain-consolidation.md`, reviewed;
  traced): the "destroyed" row is the cancelled (taken-down) building only; a builder inside a killed building dies with
  it (`economy/build-train.md`, V50).
- **Changes:** 2026-10-16 (round 77, `ERRATA-round77.md`, reviewed by a spec-review session, 2026-10-16; traced): a
  builder let out of a cancelled site faces the site's top-left tile itself, not top-left + (1, 1), and becomes idle
  as a pending order (`economy/build-train.md`, "Cancelling construction", V54 and V55).

## Behaviour

Several events put a unit onto the map next to something: a building finishes training a unit, a builder comes out
of a finished building, a worker comes out of a mine, platform or depot, the builder of a building whose construction
is cancelled comes out, a transport unloads, and a transport pushes off from the shore. All of them use one **ring
search**. It walks rings of tiles around a rectangle (the "source") and stops at the first tile that passes a test
for the unit's movement class.

### Inputs

| Input | Meaning |
| --- | --- |
| Source rectangle | Top-left tile (px, py) and size w × h in tiles |
| Aim tile (tx, ty) | Decides on which side and in which turning direction the search starts |
| Limit L | Rings are searched while the ring's first-side length is below L |
| Tile test | Per movement class; see "Tile test" |

### Where the search starts

The building occupies columns px to px+w−1 and rows py to py+h−1. The search starts on a corner tile just outside it,
with a first direction and a turning sense. Directions are east (+x), south (+y), west (−x) and north (−y); y grows
downwards. "Turning clockwise" means east, then south, then west, then north.

**Columns.** Exactly one of these applies:

- **Aim to the left or level:** tx ≤ px. Corner column px−1, first direction south, turning anticlockwise.
- **Aim inside the span:** px < tx < px+w. Corner column px−1, first direction east, turning clockwise.
- **Aim to the right:** tx ≥ px+w. Corner column px+w, first direction south, turning clockwise.

**Rows.** If the aim is strictly below the building (ty ≥ py+h), the corner row is py+h, a first direction of south
becomes north, and the turning sense is reversed. Otherwise the corner row is py−1.

The six resulting starts:

| Aim | Start corner | First side walked | Turning |
| --- | --- | --- | --- |
| Left or level, not below | North-west | West side, top to bottom | Anticlockwise |
| Left or level, below | South-west | West side, bottom to top | Clockwise |
| Inside the span, not below | North-west | North side, left to right | Clockwise |
| Inside the span, below | South-west | South side, left to right | Anticlockwise |
| Right, not below | North-east | East side, top to bottom | Clockwise |
| Right, below | South-east | East side, bottom to top | Anticlockwise |

### Walking the rings

Let **A** be the building's size along the first direction (h when the first direction is south or north, w when it
is east) and **B** the other size.

For each ring, while **A < L**:

1. Walk **A + 1** steps in the current direction, then turn.
2. Walk **B + 1** steps, then turn.
3. Walk **A + 1** steps, then turn.
4. Walk **B + 2** steps, then turn. The turn brings the direction back to the first one.
5. Step back one tile against the current direction. The cursor is now on the next ring's start corner.
6. Add 2 to A and to B.

Each step moves first and then tests the new tile, so a ring's start corner is tested as the second-to-last tile of
step 4. The extra step at the end of step 4 tests the first tile of the next ring; that tile is tested again when the
next ring starts. A tile outside the map (either coordinate below 0 or at least the map width; maps are square) is
skipped without a test. The search stops at the first tile that passes, and fails when the rings run out.

**Number of rings.** The rings use A = a, a+2, a+4, … while below L, where a is the starting A:

| Source size along the first direction | L = 12 | L = 10 | L = 6 |
| --- | --- | --- | --- |
| 1 | 6 rings (out to 6 tiles) | 5 rings (out to 5 tiles) | 3 rings (out to 3 tiles) |
| 2 | 5 rings | 4 rings | 2 rings |
| 3 | 5 rings | 4 rings | 2 rings |
| 4 | 4 rings | 3 rings | 1 ring |

Only the 1 × 1 row is used with L = 10 and L = 6 today; the other rows are listed for completeness.

### Tile test

Each movement class has its own test. The masks are the occupancy masks of `movement/occupancy.md` (tile bits as
listed there).

| Class | The tile passes when |
| --- | --- |
| Land | No bit of the land mask (0x09CE) is set |
| Air | Both coordinates are even, and the air-unit bit (0x0200) is clear |
| Sea | Both coordinates are even, the water bit (0x40) is set, and no bit of the sea mask (0x0903) is set |
| Landing | Both coordinates are even, the water bit (0x40) is set, and no bit of the landing mask (0x0901) is set |
| Push-off | Both coordinates are even, and no bit of the sea mask (0x0903) is set. The water bit is **not** required |

The push-off test is not a movement class: it is used only by the push-off search below, whatever the transport's
current class is.

Some callers add a **region condition**: the tile's region (from the map's region data) must equal a given region.

### Who uses the search

| Event | Source rectangle | Aim | L | Class used | Region condition | If no tile is found |
| --- | --- | --- | --- | --- | --- | --- |
| A building finishes training a unit, or a spell creates a unit next to its caster | The building (or caster): its top-left tile and type size | The source's own top-left tile | 12 | The new unit type's movement class | none | No unit is created; the owner is told that the way out of the building is blocked (message 67, 0x43, of `network.tbl`, counting from 1); the refund is in `economy/build-train.md` |
| A building finishes construction; its builder comes out | The building | The building's top-left tile (the builder's target tile is set to it first) | 12 | The builder's class | none | The builder dies |
| A building's construction is cancelled while its builder is inside (the building is taken down, `economy/build-train.md`; a **killed** building kills its builder instead) | The building | Each builder's own target tile, which is the building's top-left tile (set when the builder went in) | 12 | The builder's class | none | The builder dies |
| A worker comes out of a mine, oil platform or depot | The size given in `economy/harvest.md` (town hall or gold mine size), placed at the worker's tile, which is the building's top-left tile while it is inside | The worker's target tile (where its next order sends it) | 12 | The worker's class | none | The worker dies |
| A transport unloads one passenger | The transport's tile, as a 1 × 1 source | The transport's tile | 6 | The passenger's class | The region of the transport's tile | The passenger stays aboard |
| A transport pushes off from the shore (`orders/transports.md`) | The transport's tile, as a 1 × 1 source | The transport's target tile | 10 | Push-off test | none | The transport docks (order 32) |

A unit placed by the search stands on the found tile, faces its target (a target building is faced at its top-left tile
plus (1, 1)), and is shown on the map again. A worker coming out with a computer player's build job faces its build goal
tile instead (`economy/harvest.md`, "Coming out"). A builder coming out of a finished building becomes idle (order 2)
as an immediate order. A builder coming out of a cancelled site has had its target cleared, so it faces its target
tile, the site's top-left tile itself, and becomes idle as a pending order that takes over at its next update
(`economy/build-train.md`, "Cancelling construction"; traced: V54, V55 there). A worker leaving a mine, platform or depot keeps the order that harvesting gave it (`economy/harvest.md`). An
unloaded passenger becomes idle.

For a transport pushing off, the found tile is not a placement: the transport is not moved, and the tile becomes its
target tile (`orders/transports.md` says how it then sails there).

Because the aim decides the start, a worker leaving a mine comes out on the side facing the depot it is heading for,
while a trained unit always tries the tile west of the building's top-left tile first.

## Data

| Item | Value |
| --- | --- |
| Limit for buildings and units coming out | 12 |
| Limit for unloading | 6 |
| Limit for pushing off | 10 |
| Unloading source size | 1 × 1 (the transport's tile) |
| Direction steps | east (+1, 0), south (0, +1), west (−1, 0), north (0, −1) |

## Test vectors

Coordinates are (x, y). "Nothing passes" means every tile fails the test.

| Input | Expected |
| --- | --- |
| 3 × 3 building at (10, 10), aim (10, 10), L = 12, every tile passes | Start corner (9, 9), first direction south; first tile tested and chosen: (9, 10) |
| Same, nothing passes | Ring 1 tests (9, 10), (9, 11), (9, 12), (9, 13), (10, 13), (11, 13), (12, 13), (13, 13), (13, 12), (13, 11), (13, 10), (13, 9), (12, 9), (11, 9), (10, 9), (9, 9), then (8, 9). Ring 2 starts again at (8, 9). 5 rings in all, 165 tests (counting the repeated tiles); the last tile tested is (4, 5); the search fails |
| 3 × 3 building at (10, 10), aim (20, 20) (right and below) | Start corner (13, 13), first direction north, anticlockwise: (13, 12), (13, 11), (13, 10), (13, 9), (12, 9), (11, 9), (10, 9), (9, 9), (9, 10), … |
| 3 × 3 building at (10, 10), aim (11, 5) (inside the span, above) | Start corner (9, 9), first direction east, clockwise: (10, 9), (11, 9), (12, 9), (13, 9), (13, 10), (13, 11), (13, 12), (13, 13), (12, 13), … |
| Transport at (20, 20) unloading, nothing passes | 3 rings. Ring 1: (19, 20), (19, 21), (20, 21), (21, 21), (21, 20), (21, 19), (20, 19), (19, 19), then (18, 19). The last tile tested is (16, 17); 51 tests; the passenger stays aboard |
| 4 × 4 building at (10, 10), aim (10, 10), nothing passes | 4 rings, 132 tests, last tile (5, 6) |
| Shipyard 3 × 3 at (10, 10) trains a destroyer (sea class); aim (10, 10). Every tile with x ≤ 8 is open water (water bit set, no sea-mask bit); every other tile is land | Ring 1 tests 17 tiles, all with an odd coordinate, so all fail. Ring 2 starts at (8, 9); its first tile (8, 10) is even/even water. The destroyer is placed at **(8, 10)** after 19 tests |
| Same shipyard, but open water is every tile with y ≥ 14 (and nothing else) | Ring 1 is all land (17 tests). Ring 2 starts at (8, 9) and walks its west side south: (8, 9), (8, 10), (8, 11), (8, 12), (8, 13), (8, 14). The destroyer is placed at **(8, 14)**, the 23rd test: the ship comes out at the south-west corner, not the middle of the south side |
| Transport at (20, 20) pushing off, target tile (30, 20) (aim right, level), open water (no sea-mask bit) is every tile with x ≥ 22, land or shore water everywhere else | Start corner (21, 19), first direction south, clockwise. Tests (21, 20), (21, 21), (20, 21), (19, 21), (19, 20), (19, 19), (20, 19), (21, 19), (22, 19), then ring 2 from (22, 19): (22, 19), (22, 20). Found **(22, 20)** after 11 tests; it becomes the target tile |
| Same transport and target, open water is every tile with x ≤ 18 (land or shore water elsewhere) | Found **(18, 22)** after 17 tests (ring 2: ..., (22, 21), (22, 22), (21, 22), (20, 22), (19, 22), (18, 22)) |
| Transport pushing off, nothing passes | 5 rings, 125 tests; with target (25, 20) from (20, 20) the last tile tested is (26, 15); the transport docks |

## Open questions

- Whether the region data gives shore-water tiles the region of the land next to them. Unloading requires the
  passenger's tile to share the region of the transport's tile, and the transport sits on shore water, so it appears
  that it does.
- The tile-test details for the air class's second class flag (its meaning isn't known; it doesn't change the test
  above).
- The same ring walk is also used, with other tests and limits, to move a blocked command destination to a nearby
  usable tile (for example a landing tile for a transport). Those uses belong to the order and transport specs and
  are not listed here yet.
- A unit created this way whose type has the critter flag also triggers a visual effect at its centre. Which event
  uses that is unknown.
