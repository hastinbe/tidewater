# AI: spell casting by computer units

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes, "Round 17 (spellcaster AI)"), the default
  `Rez\unitdata.dat` and the unit flags and spell bits in the public PUD specification text. Not yet confirmed
  against the running game.
- **Changes:** 2026-10-16 (round 80 spells consolidation, `ERRATA-spells-consolidation.md`, review): the references
  to the timers pass point at `spells/spells.md`, "The timers pass"; the open question "Eye of Kilrogg with an old
  target" no longer says the ogre-mage walks (order 48 has no walk). No rule change.
- **Changes:** 2026-10-16 (round 81 traces, `ERRATA-round81.md`, reviewed by a spec-review session,
  2026-10-16): the open question "Polymorph from the random search" notes the pending idle form; a note that try 7 never shields its own
  caster. No rule change.
- **Changes:** 2026-10-16, round 82 (AI consolidation, `ERRATA-ai-consolidation.md`, reviewed by a spec-review session,
  2026-10-16): the caster step's place in the whole computer player is in `ai/computer-player.md`; the Classic-mode
  slot count is labelled; the all-spells cheat's scope is an open question. No rule change.

## Behaviour

This spec covers the spell step that computer casters run before their goal in pass B of `ai/attack-ai.md` ("Running
goals"). It says which spell a caster picks, at whom, and what else the step does. How the spell is then walked to
and cast is in `spells/spells.md`.

Terms are as in `ai/attack-ai.md`: flag bits are the PUD unit flags, "allied" means the alliance table counts the other
unit's owner as allied with the caster's owner (a player counts as allied with itself), and goal kinds are that spec's.
The alliance table counts the neutral owner (15) as allied with every player, so neutral units, critters included, are
"allied" in every test here and count as company in the box test. Order numbers are those of `orders/orders.md`; the
spell orders 38 to 56 and their mana costs are in `spells/spells.md`, "Orders and mana costs". Spell bits are bits of
the owner's spells known word (`economy/tech-tree.md`), numbered as in the PUD text's spell list: 0 holy vision, 1
healing, 3 exorcism, 4 flame shield, 5 fireball, 6 slow, 7 invisibility, 8 polymorph, 9 blizzard, 10 eye of Kilrogg, 11
bloodlust, 13 raise dead, 14 death coil, 15 whirlwind, 16 haste, 17 unholy armour, 18 runes, 19 death and decay.

### When the step runs

In pass B, a unit that is handled (computer owner, not gone, goal kind not none) and has flag bit 17 (caster) first
runs the spell step. If the step **acts**, the goal is skipped for this pass. Otherwise the goal's rule runs as usual.
Pass B runs every 50 cycles, so a computer caster chooses spells at most once every 50 cycles.

The step depends on the caster's type:

| Types | Spell step |
| --- | --- |
| 0x0D ogre-mage, 0x17 Dentarg | "Ogre-mage" |
| 0x0C paladin, 0x2C Turalyon | "Paladin" |
| 0x0A mage, 0x18 Khadgar | "Mage" |
| 0x0B death knight, 0x15 Teron Gorefiend | "Death knight" |
| Any other caster (in the default data: 0x31 Cho'gall, 0x33 Gul'dan, 0x34 Uther Lightbringer) | none: never acts |

### The tries

Each step is a list of **tries**, made in the order given. The step acts at the first try that succeeds, and stops
there. A try succeeds when its gates pass and it finds a target that passes its **test**; the caster is then given the
spell command (below) and the try's mark is set.

**Gates.** Every spell try first checks, in this order, that the caster's owner knows the spell (its spell bit in
the owner's spells known word) and that the caster's mana is at least the spell's cost from the cost table of
`spells/spells.md` (for healing 6, exorcism 4, blizzard and death and decay 25, raise dead 50). Some tries add a
stronger mana gate, given in their row. A try whose gates fail does nothing and makes no draws.

"Invisible" below means the caster's invisibility timer is running. Many tests start with "caster not invisible", so
an invisible caster mostly uses the tries marked for that case.

**Finding a target.** A try uses one of four searches:

- **Nearby.** The 31 × 31 tiles centred on the caster's tile (from x − 15, y − 15 to x + 15, y + 15) are scanned row
  by row from the top, each row left to right. Tiles off the map are skipped. On each tile only the ground-layer
  unit is looked at (`movement/occupancy.md`), so air units are never found and a building is looked at once per
  footprint tile. The caster itself and units with the used mark (`core/corpses.md`) are skipped. The first unit
  that passes the test is the target. There is **no** fog or detection test: unseen and invisible units can be
  chosen.
- **Nearest enemy.** The enemy search of `ai/attack-ai.md` ("Enemy search") is run for the caster. If it finds a
  unit and that unit passes the test, it is the target. Only that one unit is tested.
- **Random.** Up to 10 draws `r mod N`, where N is the number of unit slots (`core/unit-slots.md`: 1600; **Classic
  mode:** 600), each picking that unit slot. Each picked slot must not carry the used mark and must pass the
  test. The first failure ends the try at once, unsuccessful, so a try makes 1 to 10 draws. After 10 passing draws
  the try succeeds, and the spell is aimed at the **tile** of the **last** slot drawn, with no target unit. Free slots
  aren't skipped (see Open questions).
- **Fixed.** No search (the ogre-mage's eye of Kilrogg).

**The spell command.** The caster is given the spell's command (`spells/spells.md`, "Giving the order") in one of
two forms:

- **At a unit:** the target tile is the target unit's tile. For the spells aimed at a point, the target unit is then
  dropped, as `spells/spells.md` says, so the spell goes to the tile where the target stood.
- **At a tile, with no unit:** used by the random search and by raise dead.

The command is refused, and nothing is given, when the caster's current order is 0, 1, 22, 25, 26, 36, 37 or 57
(`orders/orders.md`). The try still counts as having succeeded: the step acts and the try's mark is set. A caster
that runs the step is never in one of those orders, so in practice the command is always given.

**The box test.** Several tests ask whether a unit X has **company within d** for some player P. This is true when,
in the (2d + 1) × (2d + 1) tiles centred on X's tile (off-map tiles skipped, ground layer only), there is a unit
other than X, without the used mark, whose owner P counts as allied. The caster itself is not excluded, so it can be
company when it stands near X.

### Marks

When a try succeeds, its row may set a **mark** on the target, so that other casters running later in the same pass
don't pick the same unit for the same spell. There are two kinds:

- **Target marks:** seven flags, one each for death coil, polymorph, holy vision, exorcism, healing, raise dead and
  whirlwind. The timers pass of `spells/spells.md` ("The timers pass") clears all
  seven on every unit it visits, every cycle (that spec's step list doesn't name this step; its place among the steps
  changes no outcome, since only pass B reads the marks).
- **Timer marks:** a spell timer of the target is set to 1 (or −1). These are real timers. They count down in the
  timers pass of the same cycle like any other, and until then they have their normal effect:
  - bloodlust timer 1: the target's hits in this cycle do double damage (`combat/damage.md`);
  - unholy-armour timer 1: hits on the target in this cycle do nothing. A demolition squad or sapper (types 0x0E,
    0x0F) with this mark is killed in the timers pass (`spells/spells.md`, "The timers pass", step 2);
  - speed timer 1 (hasted) or −1 (slowed) for the rest of the cycle;
  - flame-shield timer 1: this only blocks a flame-shield strike on the target in this cycle.

Pass B runs in phase 2 and the timers pass at the end of phase 4 of the same cycle (`core/timing-and-rng.md`), so a
timer mark lasts for that cycle's unit updates.

### Ogre-mage

| Try | Spell (order) | Search | Test on the target | Mark |
| --- | --- | --- | --- | --- |
| 1, only if invisible | Runes (55) | random | not allied; flag bit 8 (worker) | none |
| 2 | Bloodlust (49) | nearby | caster not invisible; allied; flag bit 27 (organic); bloodlust timer 0; order 12 | bloodlust timer 1 |
| 3 | Runes (55) | random | as try 1 | none |
| 4 | Eye of Kilrogg (48) | fixed | spell bit 10 known and mana exactly **255** | none |

Try 4 doesn't use the spell command. The caster's pending order simply becomes 48 (`orders/orders.md`): its target
tile and target unit stay as they were, and the all-spells cheat's mana top-up doesn't happen.

Try 3 runs whenever bloodlust finds nothing, so an ogre-mage that knows runes and has at least 200 mana makes 1 to 10
draws on every pass.

### Paladin

| Try | Spell (order) | Search | Test on the target | Mark |
| --- | --- | --- | --- | --- |
| 1, only if invisible | Exorcism (41) | random | not allied; flag bit 15 (undead); no exorcism mark | none |
| 2 | Healing (39) | nearby | caster not invisible; allied; organic; HP below its maximum; no healing mark | healing |
| 3 | Exorcism (41) | nearby | as try 1 | exorcism |
| 4, only if mana is exactly 255 | Holy vision (38) | random | caster not invisible; not allied; no holy-vision mark | holy vision |

Try 1 has no mark. In try 4 the mark goes on the last unit drawn.

### Mage

| Try | Spell (order) | Search | Test on the target | Mark |
| --- | --- | --- | --- | --- |
| 1, only if invisible and mana ≥ 75 | Blizzard (47) | nearest enemy | not allied; flag bit 8 or 12 (worker or gold store); no company within 3 for the caster's owner | none |
| 2, only if invisible | Polymorph (46) | random | not allied; flag bits 19 (can attack), 27 (organic) and 17 (caster); no polymorph mark | none |
| 3 | Polymorph (46) | nearby | not allied; flag bits 19 and 27; no polymorph mark | polymorph |
| 4 | Fireball (43) | nearby | not allied; and either no company within 3 for the caster's owner, or the **caster's** HP is below half its maximum (HP < floor(max / 2)) | none |
| 5 | Invisibility (45) | nearby | caster not invisible; allied; not a building (bit 5); the **caster's** goal kind is attack; the target has flag bit 17 or an attack range of 1 | unholy-armour timer 1 |
| 6, only if mana ≥ 75 | Blizzard (47) | nearest enemy | not allied; a building (bit 5); no company within 3 for the caster's owner | none |
| 7 | Flame shield (42) | nearby | caster not invisible; flame-shield timer 0; no company within 2 for the caster's owner; not a building; then, if allied: order 12 and attack range 1; if not allied: company within 1 for the **target's** owner | flame-shield timer 1 |
| 8 | Slow (44) | nearby | caster not invisible; not allied; not a building; speed timer 0 or more (not slowed); order not 2 (not idle) | speed timer −1 |
| 9 | Retreat | — | below | — |

Try 2 casts a unit spell at a tile with no unit (random search), so the cast fails as `spells/spells.md` describes
for a unit spell without a target. Try 5's mark is the unholy-armour timer, not the invisibility timer.

### Death knight

| Try | Spell (order) | Search | Test on the target | Mark |
| --- | --- | --- | --- | --- |
| 1, only if invisible and mana ≥ 75 | Death and decay (56) | nearest enemy | not allied; flag bit 8 or 12; no company within 3 for the caster's owner | none |
| 2, only if invisible | Whirlwind (52) | random | not allied; no company within 4 for the caster's owner; no whirlwind mark; flag bit 8 or 12 | none |
| 3 | Raise dead (50) | nearby, cast at the target's tile with no unit | a dead body (type 0x69) in its remains state, not used, not removed; no raise-dead mark | raise dead |
| 4 | Unholy armour (54) | nearby | caster not invisible; allied; unholy-armour timer 0; order 12; HP at most floor(max HP / 2) | unholy-armour timer 1 |
| 5, only if mana ≥ 75 | Death and decay (56) | nearest enemy | not allied; a building; no company within 3 for the caster's owner | none |
| 6 | Death coil (51) | nearby | not allied; no death-coil mark; organic; company within 1 for the **target's** owner | death coil |
| 7 | Death coil (51) | nearby | caster not invisible; not allied; no death-coil mark; organic | death coil |
| 8 | Whirlwind (52) | nearest enemy | not allied; no company within 4 for the caster's owner; no whirlwind mark | whirlwind |
| 9 | Haste (53) | nearby | caster not invisible; allied; speed timer 0; order 12 | speed timer 1 |
| 10 | Retreat | — | below | — |

**Raise dead never finds a body.** A unit is taken off the ground layer when it dies, and a dead body isn't put back
on it, so the nearby search can't see bodies. Try 3 therefore never succeeds (see Open questions).

### Retreat (mage and death knight only)

If the caster's mana is below **50**:

1. If the caster has a recorded attacker (`ai/attack-ai.md`, "Call for help"), it is given command 12 against that
   attacker. The step acts. Whether the attacker is still alive isn't tested.
2. Otherwise the unit slots are scanned from slot 0 upward for the first unit of the caster's owner whose type is
   0x50 (mage tower) or 0x51 (temple of the damned), whatever its state (under construction or dying included).
   If there is one, the step acts. It sets the goal (caster, hold, that building's top-left tile), as in
   `ai/attack-ai.md`, "Setting a goal". If that succeeds, the caster is given a smart move (command 3) to the
   building's top-left tile, with no target unit. That is the tile as given, not the adjusted goal tile. If setting
   the goal fails, nothing else happens.
3. If there is no such building, the step doesn't act.

With mana of 50 or more, the retreat doesn't act.

## Data

| Item | Value |
| --- | --- |
| Nearby box | 31 × 31 tiles centred on the caster |
| Random search | up to 10 draws `r mod N` (N = 1600; Classic mode: 600) |
| Stronger mana gates | 75 (three times the blizzard or death and decay cost) for the nearest-enemy blizzard and death and decay; exactly 255 for the eye of Kilrogg and the paladin's holy vision |
| Company boxes | 3 (blizzard, death and decay, fireball), 2 and 1 (flame shield), 1 (death coil), 4 (whirlwind) |
| Retreat | mana below 50; mage tower 0x50, temple of the damned 0x51 |
| Timer marks | bloodlust 1, unholy armour 1, flame shield 1, speed +1 (haste) and −1 (slow) |
| Draws | only the random search: 1 to 10 per random try that passes its gates |

In the default data, the units with attack range 1 that aren't buildings are types 0x00 to 0x03, 0x06, 0x07, 0x0C to
0x11, 0x17, 0x19 to 0x1D, 0x24, 0x25, 0x28, 0x29, 0x2C to 0x2F, 0x31, 0x32, 0x34, 0x37 and 0x39. The undead (flag
bit 15) are 0x0B, 0x15, 0x33 and 0x37.

## Test vectors

Unless stated, the caster belongs to a computer player, is not invisible, is handled in pass B, and its owner knows
every spell. "Draws" counts the caster step's draws only.

| Input | Expected |
| --- | --- |
| Ogre-mage, 120 mana; an own grunt with order 12, bloodlust timer 0, 5 tiles away | Try 2: bloodlust command at the grunt; its bloodlust timer becomes 1, so its strikes later in this cycle do double damage. Acts; goal skipped; 0 draws |
| Two ogre-mages in the same pass, one such grunt | The first casts at the grunt (timer 1). The second's test fails on it (timer not 0); it goes on to try 3 |
| Ogre-mage, 210 mana, no bloodlust target; first draw picks a slot holding an own footman | Try 3 fails after 1 draw (allied). Mana isn't 255: doesn't act; 1 draw |
| Same, but invisible, and both draws pick slots holding own footmen | Try 1 (1 draw, fails), try 2 fails (caster invisible), try 3 (1 draw, fails): 2 draws |
| Ogre-mage, 255 mana, knows eye of Kilrogg but not runes, no bloodlust target | Try 4: pending order 48 with the old target; acts; 0 draws |
| Paladin, 30 mana; own footmen at (x − 15, y − 15) with 40 of 60 HP and at (x, y − 1) with 50 of 60 HP | Nearby scan reaches the top-left row first: healing at (x − 15, y − 15); healing mark on it |
| Paladin, 255 mana, no healing or exorcism target; draws 1 to 3 pick enemy units, draw 4 picks a slot with the used mark | Try 4 fails after 4 draws; doesn't act |
| Mage with 120 mana that doesn't know polymorph, 60 of 60 HP; an enemy grunt with an own footman 2 tiles from it | Fireball rejected: company within 3, and the mage isn't below half HP |
| Same, the mage at 29 of 60 HP | 29 < 30: fireball at the grunt's tile, no target unit |
| Same mage, full HP, standing 2 tiles from a lone enemy grunt | The mage itself is company within 3: fireball rejected |
| Same mage, full HP, 6 tiles from an enemy grunt; a neutral critter 2 tiles from the grunt and no other unit near it | The critter (owner 15, allied with everyone) is company within 3: fireball rejected |
| Mage with 200 mana and goal kind attack; an own knight (range 1) within the box, nothing earlier succeeds | Invisibility command at the knight; the knight's unholy-armour timer becomes 1, so hits on it in this cycle do nothing |
| Same, the target is an own demolition squad | Command given; the squad is killed in this cycle's timers pass |
| Mage, 80 mana; the enemy search's nearest enemy is a grunt; an enemy farm is further away | Try 6 tests only the grunt: not a building, so nothing. Try 7 and 8 follow |
| Mage, 80 mana; nearest enemy is a farm with no own unit within 3 of its top-left tile | Blizzard at the farm's tile, no target unit |
| Mage, 70 mana, same farm | Try 6 skipped (below 75) |
| Mage, 40 mana, no target for tries 3 to 8, has a recorded attacker | Retreat step 1: command 12 against the attacker |
| Mage, 40 mana, no attacker; own mage towers in slots 300 and 50 | Retreat to the tower in slot 50: goal hold at its top-left tile (moved by the ring search to a free land tile if needed); on success, command 3 to the tower's top-left tile |
| Death knight, 40 mana, no attacker, owner has no temple | Doesn't act; the goal runs |
| Death knight, 120 mana; own grunt with order 12 and 30 of 60 HP | 30 ≤ 30: unholy armour at it (if nothing earlier succeeds); its unholy-armour timer becomes 1 |
| Same, the grunt has 31 of 60 HP | Not chosen |
| Death knight, 120 mana; a lone enemy grunt, nothing earlier succeeds | Try 6 rejects it (no company within 1 for its owner); try 7 chooses it: death coil at its tile; death-coil mark |
| Death knight with dead bodies all around, 60 mana | Try 3 never succeeds; later tries run |
| Invisible mage, 200 mana; the first draw picks an enemy footman (not a caster) | Try 2 fails after 1 draw |

## Open questions

- **The all-spells cheat's scope** (round 82, Q3 of `ERRATA-ai-consolidation.md`): `game/cheats.md` gives every spell
  "for every player controlled by a person", `spells/spells.md` glosses the cheat as "every player". It decides
  whether a computer caster's spell gate passes under the cheat. Default: `game/cheats.md` (players controlled by a
  person only), the spec that states the cheat's rule. Trace request AI3: cheat on, a computer mage that doesn't know
  a spell; watch for a cast of it.
- **Free slots in the random search.** A freed unit slot keeps its last occupant's data, with owner 14 (the free
  list), and doesn't carry the used mark, so the random search can pick it. Its tests then read the stale type,
  timers and marks, and the alliance entry for owner 14. That entry wasn't fully traced. As far as traced, a computer
  player's entry for owner 14 is "not allied", so a free slot can pass the holy-vision test, which a paladin with 255
  mana then needs 10 times in a row. The random tries read, from a free slot: its owner (14), its state marks (only
  the "unused" one is set, so not the used mark), its type's flag bits, its target marks and its tile (where the
  spell is aimed, and the centre of the whirlwind box test). Default: keep those fields when a slot is released, and
  implement the alliance table literally. What a never-used slot holds wasn't checked; default all zero, owner 14.
- **Polymorph from the random search.** It aims a unit spell at a tile with no unit. Whether the caster first walks to
  the tile before it goes idle wasn't traced. Default: as `spells/spells.md` gives for a unit spell without a target.
  Round 81: the idle it ends in is the pending form, with wait 3 (`spells/spells.md`, "Walking and the decision"; the
  form is traced for player casts in S90 and S91, and is static study for this case); whether it walks first is still
  open.
- **Eye of Kilrogg with an old target.** Try 4 of the ogre-mage leaves the old target tile and unit in place.
  Default: keep them as they were. Order 48 has no walk (radius "always" in `orders/orders.md`; `spells/spells.md`,
  "Walking and the decision", round 80), so the ogre-mage casts where it stands and the old target is only kept.
- **Raise dead.** The conclusion that it never fires rests on no code path putting a dead body back on the ground
  layer. If play shows computer death knights raising skeletons, this needs another look.
- **Flame shield on the caster itself** (round 81, static study). Try 7 never picks its own caster: an allied target
  must have attack range 1 (and be in order 12), and a mage's attack range is more than 1 (game data). A player can't
  command it either (`spells/spells.md`, S97). So the self-cast's bolt timing never matters. No rule change.
- **Invisibility mark.** The mage's invisibility try marks the unholy-armour timer, not the invisibility timer, which
  looks like a slip in the original. Default: keep it, with its effects.
