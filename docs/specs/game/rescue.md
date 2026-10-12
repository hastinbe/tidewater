# Game: rescue

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes, "Round 12 (rescue)"). Sound names come from the
  game's sound list. Not yet confirmed against the running game.

## Behaviour

Rescue happens in two separate stages, and only the second one counts toward a rescue victory:

1. **Joining.** A unit of a human-controlled player comes next to a rescuable unit or building of a non-human player.
   The rescuable one, and in some cases its whole side, changes owner to that human player. The capture sound plays.
2. **Rescuing.** A rescuable unit that a human-controlled player owns stands on a **circle of power**. It loses the
   rescuable mark, the player's **rescued count** goes up, and the rescue sound plays.

### Terms

- **Rescuable mark:** a per-unit mark. A unit with the mark is a **rescuable unit**, whoever owns it. The game keeps
  two global counts that `game/victory.md` reads: the number of rescuable units on the map, and the number of those
  whose type has flag bit 23 (heroes).
- **Human-controlled player:** a player whose controller is human (`orders/orders.md`, "Terms"). In a single-player
  game that is only the local player; in a multiplayer game it is every human player.
- **Ground occupant of a tile:** the unit or building that the tile's ground-unit pointer names
  (`movement/occupancy.md`). Air units are never ground occupants. A circle of power keeps no pointers, so it is never
  one either.
- **Unit:** a type with at least one of flag bits 0 to 4 (land, air, explode, sea, critter). Buildings have none of
  them.

### Where the mark comes from

When the map is loaded, every unit and building that the map gives to a **rescue player** (passive or active)
receives the rescuable mark. This applies to all of them, including buildings and buildings under construction.
Later in game setup:

- a passive rescue player becomes a passive computer player;
- an active rescue player becomes a computer player.

From then on they behave like those players in every other spec. Rescue works the same for both kinds. The only
difference is what the other specs give computer players: an active rescue player's units also act on their own.

Other sources of the mark are the mission rules in `game/victory.md` ("Mission start", and modes W and Z).

**What keeps the mark and what removes it:**

| Event | Mark |
| --- | --- |
| Joining (owner change) | Kept |
| Reaching a circle of power while owned by a human-controlled player | Removed |
| The unit is removed or dies | Removed, and it stops counting |
| Type conversions (research results, mission start) | Kept, except the ones `game/victory.md` lists as removing it |

### Joining

**When the check runs.** A rescuable unit checks for rescuers each time its animation signals "step done"
(`core/animation-scripts.md`), which is the moment its order logic would run. In that update the check comes first,
before the pending order takes over and before the order logic runs. It only runs while the unit's owner is **not**
human-controlled. Once a rescuable unit belongs to a human-controlled player, it never checks again.

**Frozen units.** There is also a **frozen** state, which only some network commands set, and only on workers of
the commanding player (the open questions say more). It works like this:

- A unit that is **about to freeze** skips its next "step done": no rescue check and no order logic run. From then
  on it is frozen.
- A frozen unit doesn't advance its animation at all, so it never checks.
- When a frozen unit is **released**, the next update runs the rescue check and the order logic at once, without
  waiting for a "step done".

Rescue-player units are never workers of the commanding player, so in normal play this doesn't affect them.

**The area.** Let (x, y) be the rescuable unit's tile and W × H its size in tiles from the unit data. The area is
columns x − r to x + W and rows y − r to y + H, **both ends included**, where:

- r is 2 for units that move two tiles at a time (ships and air units, `movement/movement.md`);
- r is 1 for everything else.

So the area is a ring one tile wide around the unit's footprint (the right column x + W and bottom row y + H are
tested), two tiles wide on the left and top sides for two-tile movers. It is cut off at the map edges: columns and
rows below 0 or at the map size and above are left out.

**The rescuer.** The tiles are tested row by row from the top, and left to right within a row. The first tile whose
ground occupant passes every test below decides the new owner:

- the occupant isn't the rescuable unit itself;
- it is a unit, not a building;
- it isn't dying, dead or off the map;
- its owner is human-controlled.

Nothing else is tested. The rescuer's owner doesn't need to be allied with anyone, and in a multiplayer game the first
human player found wins. Ships qualify as rescuers, since they are ground occupants. Air units never do.

**The result.** The rescuable unit changes owner to the rescuer's owner (see "Owner change"), with the announcement.
Then, if the rescuable unit's type has flag bit 12 (a gold store: the halls in the default data), every other unit
and building of the **old** owner that isn't dying, dead or off the map also changes owner to the same player,
**without** the announcement. Only one rescuer is taken per check.

### Owner change

The same steps run for joining and for the gold-store cascade. Steps 1 to 5 happen in this order:

1. **Buildings:**
   - a building that is working on a job (training, research, upgrade or building upgrade) has that job cancelled, as
     in `economy/build-train.md` ("Ending", the Cancelled column). The refund goes to the **old** owner;
   - a building still under construction moves from the old owner's under-construction count to the new owner's, and
     its construction goes on.
2. **Tally.** The new owner's **converted tally** goes up by 1. Mode F in `game/victory.md` reads this tally.
3. **Owner.** The unit is taken out of every player's current selection, out of the old owner's counts, and into the
   new owner's. Its order is not changed.
4. **Enhanced mode only.** If the new owner has researched the paladin upgrade, a knight or ogre becomes a paladin or
   ogre-mage. If the new owner's ranger level isn't 0, an archer or axe thrower becomes a ranger or berserker. Each
   change is the same as the research conversion in `economy/build-train.md`: hit points go up by the difference in
   maximum hit points, and a caster gets its starting mana. In Classic mode the type doesn't change.
5. **Counts.** The unit is counted for the new owner. A rescuable unit stays rescuable, so the global rescuable
   counts don't change.
6. **Announcement** (only when the unit itself joined, not in the cascade): the capture sound plays on the unit.
   This is `misc\hcapture`, or `misc\ocapture` when the local player's sound set is orc (`ui/sounds.md`). It is a
   placed sound, audible off screen.
7. **Flash** (always): the unit's selection box flashes for 100 cycles (see "Flash").

### Rescuing at a circle of power

Every circle of power (type 0x64) runs a rescue check from the moment it is created, whoever owns it.

1. It tests every tile of its own footprint (W × H from the unit data), row by row.
2. A ground occupant counts when it has the rescuable mark and its owner is human-controlled. Its state isn't tested.
3. For each occupant that counts:
   - the rescue sound plays on the circle: `misc\hrescue`, or `misc\orescue` for the orc sound set. It is a placed
     sound, audible off screen;
   - if the occupant's type has flag bit 23 (hero) and the mission's required rescue count has bit 0x1000, the
     owner's rescued count goes up by 0x1000;
   - the owner's rescued count goes up by 1;
   - the occupant leaves the global rescuable counts (and the hero count, for a hero) and loses the mark.
4. The circle's animation then waits 50 cycles before it goes on, so it checks about once per 50 cycles (see the open
   questions).

Units owned by a computer, passive or rescue player are never rescued here, even with the mark. A unit that has been
rescued has no mark left, so it is never counted twice.

### Flash

After an owner change, the unit flashes for 100 cycles. A countdown starts at 100 and goes down by 1 each cycle.

- While the countdown is non-zero, the unit's selection box is drawn in the cycles where the countdown has bit 3 set
  (8 cycles on, 8 off).
- Its colour is palette entry 250 when the unit belongs to the local player, 251 when its owner is allied with the
  local player, and 249 otherwise.
- If the local player can't detect the unit (`vision/fog.md`), the countdown is cleared at once.

A command's target flashes the same way, starting from 31.

## Data

| Item | Value |
| --- | --- |
| Joining ring width | 1 tile (2 on the left and top for two-tile movers) |
| Gold store flag | type flag bit 12 |
| Hero flag | type flag bit 23 |
| Hero bit in rescue counts | 0x1000 (only when the required count has it) |
| Circle of power type | 0x64 |
| Circle check pause | 50 cycles |
| Flash length | 100 cycles; on while countdown bit 3 is set |
| Flash colours | 250 own, 251 allied, 249 other |
| Capture sound (joining) | `misc\hcapture` / `misc\ocapture` |
| Rescue sound (circle) | `misc\hrescue` / `misc\orescue` |

## Test vectors

| Input | Expected |
| --- | --- |
| Passive rescue player's footman (1 × 1) at (10, 10); the local player's grunt walks to (11, 11) (the bottom-right corner of the area, columns and rows 9 to 11) | At the footman's next step done: the footman becomes the local player's, the capture sound plays on it, and it flashes for 100 cycles. Rescued count unchanged; rescuable count unchanged |
| Same, the grunt at (12, 10) | Not in the area (columns 9 to 11); nothing happens |
| Rescue player's footman at (0, 5) on a 64 × 64 map; the local player's grunt at (1, 5) | Columns 0 to 1 (column −1 left out); the footman joins |
| Rescue player's footman at (63, 5) on a 64 × 64 map | Columns 62 to 63 (column 64 left out) |
| Rescue player's destroyer at (20, 20); the local player's transport at (18, 19) | In the area (r = 2, so the area starts at column 18 and row 18); the destroyer joins |
| Rescue player's footman; the local player's gryphon rider hovers on an adjacent tile | Nothing (air units aren't ground occupants) |
| Rescue player's 4 × 4 town hall at (30, 30); the local player's peasant at (34, 32) | In the area (columns 29 to 34). The hall joins with the sound; every other unit and building of that rescue player joins silently |
| The same player has a barracks training a footman when the hall joins | The barracks' job is cancelled and refunded to the rescue player; then the barracks changes owner |
| Rescue player's footman at (10, 10); human player 2's grunt at (9, 11), human player 1's grunt at (11, 9) | Row 9 is tested before row 11, so player 1 gets the footman |
| Rescue player's knight joins in Enhanced mode; the new owner has the paladin upgrade | It becomes a paladin, hit points raised by the difference in maximum hit points |
| Same in Classic mode | It stays a knight |
| A joined footman (still marked) steps onto a circle of power | Rescue sound on the circle; owner's rescued count +1; rescuable count −1; mark gone |
| Mission required count 0x1001; a joined hero steps onto a circle of power | Rescued count + 0x1001; hero count −1 |
| Required count 1 (no 0x1000 bit); a joined hero steps onto a circle | Rescued count +1 only |
| Two marked units owned by the local player stand on the circle in the same check | Two rescue sounds; rescued count +2 |
| A computer player's unit with the mark stands on a circle | Nothing |
| A joined footman dies before reaching a circle | Rescuable count −1; rescued count unchanged |

## Open questions

- **Off-map units in the cascade.** The cascade skips units that are dying, dead or off the map. That covers units
  inside a transport, mine, oil platform or building, so those would stay with the old player after the hall joins.
  Default: they stay, and come out still owned by the old player. Not checked in play.
- **Circle timing.** The 50-cycle pause is added to whatever the circle's animation does next. The exact period
  between checks hasn't been worked out from the script. Default: one check every 50 cycles plus the animation's own
  wait (assume 1).
- **Orders of joined units.** The owner change doesn't touch the unit's order or goals. An active rescue player's unit
  that was attacking keeps that order under its new owner. Default: keep it, as the program does.
- **Which selections.** The unit is taken out of a set of 16 per-player lists of 9 units, which the command handling
  uses. These are assumed to be each player's current selection; the local player's displayed selection is assumed to
  follow.
- **Frozen state.** The commands that freeze and release a worker weren't identified (they look like a command that
  toggles the state for the workers in the command's unit list). Default: implement the state as described, with no
  command that sets it, until the command is found.
- **Two extra redraw requests** happen at the end of every owner change. What they refresh (probably the minimap or
  the status display) hasn't been traced.
