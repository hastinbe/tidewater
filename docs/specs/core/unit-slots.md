# Core: unit slots, creation order and slot reuse

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`). Not yet confirmed against the
  running game.
- **Changes:** 2026-10-11, round-8 errata: only flag-bit-10 units (the transports), not all ships, get no corpse on
  shore water.
- **Changes:** 2026-10-12, round-20 errata: the three permanent effects are never freed, so effects made during play
  take slot 3 or higher.
- **Changes:** 2026-10-12: the open question on what the three permanent effects show is closed by
  `ui/map-drawing.md`: slot 0 is the light building fire, slot 1 the heavy building fire and slot 2 the ground marker.

## Behaviour

Every unit, building, corpse and ruin lives in one fixed-size **unit table**. Missiles and visual effects live in a
second, smaller **effect table**. The update order of each cycle, and so the order of random draws (see
`timing-and-rng.md`), follows slot order in these tables. A reimplementation that wants identical results must assign
slots exactly as described here.

### Table sizes

| Mode | Unit slots | Effect slots |
| --- | --- | --- |
| Classic | 600 | 200 |
| Enhanced | 1600 | 400 |

### Owner lists

Every unit slot belongs to exactly one owner list at all times.

- **Owners:** there are 16 owner values. 0 to 7 are the players. 15 is neutral. 14 is the **free list**, which holds
  every unused slot. 8 to 13 are not used.
- **List order:** a slot joining a list always goes to the **front**. Each list is therefore ordered from most
  recently added to least recently added.
- **Counts:** the game keeps a count for each list.

### Starting a map

1. Every slot is marked unused and given owner 14.
2. The slots are added to the free list in slot order 0, 1, 2, … , N − 1. Because each goes to the front, the free
   list starts with slot N − 1 and ends with slot 0.
3. The effect table is cleared. Three permanent effect entries are created at once, so they take effect slots 0, 1
   and 2. None of the three is ever freed: two loop forever, and the third is only ever replaced in place. So effects
   made during play always take slot 3 or higher. Slots 0 and 1 are the light and heavy building fires, and slot 2 is
   the ground marker (`ui/map-drawing.md`).
4. The map's units are created in the order the map file lists them (see "Creating a unit").
   - Units of an owner that is not playing are skipped.
   - Start-location markers are normally not created (they set the starting view instead).
   - Each unit's type is first converted to its owner's race.

### Creating a unit

1. **Per-player cap:**
   - Let P be the number of players (owners 0 to 7) with at least one slot in their list. The cap is N if P ≤ 1, and
     otherwise `floor(N / P)`.
   - A request for owner o (other than neutral, 15) fails if o's list already holds at least the cap.
   - Neutral units are never capped.
2. **Free slot:** the request fails if the free list is empty. Otherwise the game takes the slot at the **front** of
   the free list and moves it to the front of the owner's list. The count changes and the cap is recalculated
   (step 1).
3. **Reset:** the slot's contents are cleared, except its list link and its fixed slot identity. The unit then gets
   the next **creation number**, a counter that starts at 1 when the map starts and goes up by 1 per created unit.
   The counter isn't reused.
4. **Placement:**
   - The position is aligned to the tile grid: 32 pixels for most units, and 64 pixels (an even tile) for ships and
     flying units.
   - If the placement check fails, the slot goes back to the free list (see "Death, corpses and release"), and creation fails
     with the "can't place" message.
   - A failed cap or empty free list shows the "too many units" message instead.
5. **Random draws:** these happen as listed in `timing-and-rng.md` (facing for mobile units, a stored 0 to 5 value for
   buildings, then the extra first wait for all).

On a fresh map, therefore:

- The first unit created takes slot N − 1, the second N − 2, and so on.
- The **update order** each cycle, which is ascending slot order, is the **reverse of creation order**.

### Updating

Each cycle the unit table is walked from slot 0 to slot N − 1, and unused slots are skipped. Then the effect table is
walked from slot 0 upward in the same way (see `timing-and-rng.md` for what each update does).

### Death, corpses and release

When a unit is killed, it keeps its slot. The game:

- clears the unit off the occupancy marks;
- releases any tree it had reserved;
- kills any units inside it (transports, and buildings holding units);
- sets its order to "dying", which plays its death animation.

When the death animation reaches its step-done signal, one of three things happens:

| Unit | What happens |
| --- | --- |
| Mobile unit whose type has a corpse | It becomes a **corpse** unit in the same slot, and the corpse animation plays. A unit whose type has flag bit 10 (the transports in the default data) and that stands on shore water at its death step done gets no corpse; other ships leave one (`core/corpses.md` has the full rule). |
| Building | Its footprint marks are cleared, and it becomes a **ruin** unit sized by the building's width, in the same slot |
| Anything else, or a corpse or ruin whose time is over | Its order becomes "remove", and it is moved to the neutral owner (15) |

The "remove" order runs at the unit's next step-done signal. It **releases the slot**:

1. The unit is unlinked from its owner list. The owner's count goes down, and if it reaches 0 for a player, P goes
   down by 1.
2. The slot is marked unused, with owner 14, and goes to the **front** of the free list.

The released slot is therefore the **first one reused**. The free list works last in, first out.

### Changing owner

A unit that changes owner leaves its old list and goes to the front of the new owner's list. The counts and P are
updated, but **the cap is not checked**. A player can go over the cap by converting units, for example with a spell.

### Saved games

When a saved game loads, the owner lists are rebuilt from scratch by walking slots 0 to N − 1 and adding each to the
front of its owner's list, as at map start. After a load, every list (including the free list) is in descending slot
order, whatever order it had before saving. Any later behaviour that depends on list order, such as slot reuse, can
differ between a game that was saved and reloaded and one that wasn't.

### Effect slots

Creating an effect takes the **lowest-numbered** unused effect slot, found by scanning from slot 0. If none is free,
no effect is created. A released effect slot is simply marked unused.

## Data

| Constant | Value |
| --- | --- |
| Unit slots | 600 classic, 1600 enhanced |
| Effect slots | 200 classic, 400 enhanced |
| Owner values | 0 to 7 players, 14 free list, 15 neutral |
| Per-player cap | N when P ≤ 1, else `floor(N / P)`, where P counts players 0 to 7 with at least one slot |
| First creation number | 1 (per map) |
| Permanent effects at map start | 3, in effect slots 0 to 2 |

## Test vectors

| Case | Expected |
| --- | --- |
| Classic, fresh map, the map lists units A, B, C (in that order) | A in slot 599, B in 598, C in 597; each cycle C updates first, then B, then A |
| Then B is killed and has no corpse; after "remove" runs, a new unit D is created | D takes slot 598 |
| Classic, players 0 and 1 each have units, neutral has gold mines | cap 300 per player; neutral uncapped |
| Player 0 has 300 units in that game and trains another | creation fails ("too many units") |
| Player 1 loses its last unit (its list becomes empty) | P drops to 1; cap becomes 600 |
| Enhanced, 4 players with units | cap 400 |
| Classic, 7 players with units | cap `floor(600 / 7)` = 85 |
| Unit converted to a player already at the cap | conversion succeeds; that player now exceeds the cap |
| Two missiles created, the first is removed, then a third is created | the third reuses the first's effect slot (the lowest free slot) |
| Game saved and reloaded, then two units die and two are created | the new units take the two released slots, most recently released first |

## Open questions

- How many cycles a corpse and a ruin last, and the decay stages between them (bones, crater and so on).
- The exact rule for which ruin type a building becomes (it depends on the building's width, and on water for
  shore buildings).
- Whether a player with only corpses left in their list still counts towards P (it appears so, since corpses keep their
  owner until removed).
