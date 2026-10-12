# UI: placing a building, and the error sound

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes, "Round 16 (error sound, building placement)"), the
  default `Rez\unitdata.dat`, and the campaign and custom maps shipped with the game (for where oil patches lie).
  Sound names come from the game's sound list. Not yet confirmed against the running game.
- **Changes:** 2026-10-12: the pointer-tile open question is closed by `ui/screen-and-cursors.md` ("Placement"):
  the site is the tile under the pointer's hot spot, not centred, and it updates only while the pointer is in the
  map view.

## Behaviour

The error sound (`misc\error`) has exactly one use: a click that tries to place a building where it can't go. This
spec covers that click: the site check, what each failure shows, and what a successful click plays. Placement mode
itself starts from a Build button (`ui/command-buttons.md`, "build X").

Strings are entries of `stat_txt.tbl`, counting from 1, as in `ui/command-buttons.md`.

### The site

While placement mode is on, the site's top-left tile follows the pointer: it is the tile under the pointer's hot spot,
updated only while the pointer is inside the map view (`ui/screen-and-cursors.md`, "Placement"). For a building whose
movement layer is **sea** in the unit data (in the default data: the two oil platforms), the site moves onto odd
coordinates. Each coordinate c of the pointer's tile becomes **(c with bit 0 set) − 2**, and becomes 1 if that is
negative. So pointer columns 2 and 3 both give column 1, and 4 and 5 both give 3. (Oil patches in the shipped maps all
lie on odd coordinates, so a platform site can line up with them.) Every other building's site is the pointer's tile.

### The site check

The check is run for the building type, the site and the builder (the primary selected unit). Its result is one of
the outcomes below. When several apply, the **highest** row in this table wins:

| Priority | Outcome | When | Message |
| --- | --- | --- | --- |
| 1 | Off the map | The site's top-left is negative, or the footprint runs past the map's right or bottom edge | 442 |
| 2 | No shore | The building has flag bit 16 (shore building) and no footprint tile has the shore-water bit (0x02) | 444 |
| 3 | Unexplored | The builder belongs to the local player, the reveal-map cheat is off, and some footprint tile is unexplored (`vision/fog.md`) | 445 |
| 4 | Too close to a mine | The building has flag bit 12 (gold store) and a gold mine is too close (below) | 447 |
| 4 | Too close to oil | The building has flag bit 16 and oil is too close (below) | 448 |
| 4 | No oil patch | The building has flag bit 11 (oil platform) and no oil patch has its top-left tile exactly on the site's top-left tile | 446 |
| 5 | Blocked | Some footprint tile has a bit of the building's placement mask (`movement/occupancy.md`, "Building placement masks") | 443 |
| — | Allowed | None of the above | — |

Only one of the three priority-4 checks applies to a building: gold store first, then shore building, then oil
platform. A building with none of those flags uses the land mask and has no priority-4 check.

An off-map site is reported at once, and no other check is made.

**"Too close".** A unit or building X is too close at distance d when X is alive (not dying, dead or inside
something) and X's footprint, grown by d tiles on every side, overlaps the new footprint. That is, on each axis
`X.start − d < site.end` and `site.start − d < X.end`, where "end" is one past the last tile. Who counts as X:

| Building | X | d |
| --- | --- | --- |
| Gold store (halls, keeps, castles) | anything with flag bit 22 (gold mine) | 3 |
| The builder's own race's foundry (types 0x4E and 0x4F, chosen by the builder's race) | anything with flag bit 11 or 21 (oil platform, oil patch) | Enhanced: 1. Classic: **not checked** |
| Every other shore building (shipyards, refineries) | the same | 4 |

**Hidden units don't block.** For a human player's builder, when the "blocked" test looks at a tile, the marks that
units and buildings leave (0x0100, 0x0200, 0x0800) are ignored if the builder's owner doesn't currently see that tile
(`vision/fog.md`, "Placing a building"). The terrain bits are always tested.

**The builder doesn't block itself.** If the builder is a worker or tanker, the ground-unit mark (0x0100) on its own
tile is ignored during the check, so a builder standing inside the site doesn't make it "blocked".

### The click

When the player clicks on the map in placement mode:

- **Allowed:** `misc\thunk` plays, placement mode ends, and the build command is sent for the builder with the site
  (`economy/build-train.md`, "Placing a building").
- **Any failure:** the outcome's message is shown in the message area, and `misc\error` plays. Nothing is sent, and
  placement mode **stays on**, so the player can click again.
- **Cancelling** placement mode (the Cancel button, or the other ways the mode is ended) plays nothing.

Both `misc\thunk` and `misc\error` are plain sounds: not placed at a tile, always at the normal level, and not under
any sound option. The off-screen rule in `ui/sounds.md` that silences the error sound therefore never applies to it.

The site check is a local filter. When the worker reaches the site, the game checks the placement again
(`economy/build-train.md`); that check plays no error sound.

## Data

| Item | Value |
| --- | --- |
| Error sound | `misc\error`, only on a failed placement click |
| Placement sound | `misc\thunk`, on an allowed placement click |
| Messages | 442 off the map, 443 blocked, 444 no shore, 445 unexplored, 446 no oil patch, 447 too close to a mine, 448 too close to oil |
| Mine distance (gold stores) | 3 |
| Oil distance | 4 for shipyards and refineries; 1 for foundries in Enhanced, none in Classic |
| Sea-layer site coordinates | (c with bit 0 set) − 2, at least 1 |

## Test vectors

The default unit data is used. Sizes: town hall 4 × 4, gold mine 3 × 3, shipyard and foundry 3 × 3, oil platform and
oil patch 2 × 2.

| Input | Expected |
| --- | --- |
| Town hall at (10, 10), a gold mine at (15, 10) to (17, 12) | X starts at 15; 15 − 3 = 12 < 14 (site end): too close. Message 447, `misc\error`, placement mode stays on |
| Town hall at (10, 10), the mine at (17, 10) | 17 − 3 = 14, not below 14: allowed if nothing else fails. `misc\thunk`, build command sent, mode ends |
| Human shipyard at columns 10 to 12 (end 13), on good shore; an oil patch in the same rows starting at column 16 (three free columns between) | 16 − 4 = 12 < 13: too close, message 448 |
| Same, the patch starts at column 17 | 17 − 4 = 13, not below 13: no oil failure |
| Human foundry next to an oil patch (no gap), Classic | No oil check; allowed if the shore and mask tests pass |
| Same, Enhanced | Too close at d = 1: message 448 |
| Oil platform, pointer on tile (12, 9) | x = (12 with bit 0 set) − 2 = 11, y = (9 with bit 0 set) − 2 = 7. Site (11, 7); allowed only if an oil patch's top-left is (11, 7) (and nothing else fails) |
| Oil platform, pointer on tile (0, 1) | Both coordinates give −1, so the site is (1, 1) |
| Farm with one footprint tile on a tree | Blocked: message 443 |
| Farm with one footprint tile unexplored and another on a tree, local human builder | Unexplored outranks blocked: message 445 |
| Shipyard with its footprint entirely on land | No shore: message 444 (outranks everything except off the map) |
| Shipyard whose footprint runs past the map edge | Off the map: message 442 (the shore test isn't made) |
| Town hall whose footprint runs one column past the map edge | Off the map: message 442 |
| Human builder; an enemy footman stands on a site tile that the builder's owner can't see now (explored, fogged) | The footman's mark is ignored: not blocked |
| Same, the tile is visible | Blocked: message 443 |
| The builder, a peasant, stands on one of a farm's site tiles; nothing else is there | Not blocked by the peasant: allowed if nothing else fails |
| The player cancels placement mode | No sound |

## Open questions

- **A setup-time switch.** The unexplored test is also skipped while one game-setup step runs. It is never on when the
  player can click, so it has no known effect.
- **Other error sounds.** No other use of the error sound was found, including clicks on an invalid target in target
  mode. If play shows the sound elsewhere, this spec should be extended.
