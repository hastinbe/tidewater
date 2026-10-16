# Spells: casting, effects, durations and summoned units

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11). Round-80 consolidation (B-038 step 2):
  approved (Reviewed-by: spec-review session, round 80, 2026-10-16). The changes are listed in
  `ERRATA-spells-consolidation.md`.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), the animation data in
  `Rez\script.bin` (decoded with the instruction set of `core/animation-scripts.md`), the default unit data
  (`Rez\unitdata.dat`), the unit flags in the public PUD specification text, and Enhanced oracle traces named at each
  rule and vector. Each subsection ends with an **Evidence** line: *traced* (an oracle trace, named), *static study*
  (the program, not yet seen running), *game data* (the shipped data files) or *observed* (the running game watched
  by a person).
- **Rules mode:** every rule here is Enhanced unless it says **Classic mode**. The Classic-mode rules, all from static
  study, are: holy vision's map bounds and its fog refresh ("The spells"), exorcism's and death coil's ally exclusion
  (Enhanced only, so Classic mode includes allies), and the 200 effect slots ("Effect slots"). Which units an
  invisible unit is detected by (`vision/fog.md`) and the caster AI's random search size (`ai/caster-ai.md`) also
  differ, and are labelled there. No Classic trace was used.
- **Changes:** 2026-10-11, round-8 errata: the fireball's count goes on to 41 and it ends on the step after count 41.
  2026-10-12, round-16 errata: the cosmetic effects' lifetimes (each plays its script once; removed in cycle
  U + 1 + W).
- **Changes:** 2026-10-12, round-24 errata: a new effect starts at its start step, A cycles after its first update
  (per-kind table); the cosmetic effects are removed in cycle U + A + 1 + W (cannon smoke U + 24, not U + 13); the
  open question on each kind's own script is closed. Kinds 0x13 and 0x14 are the building fires made at map start.
  2026-10-12, round-30 errata: a test vector confirms the building fires' start step in the running game.
  2026-10-12, round-35 errata: the mana gain of the timers pass has no building exception; a building with the caster
  flag starts its counter at 0.
  2026-10-12, round-37 errata: a kind 0x16 marker on a unit is placed from the type's box size (as for kinds 8 and 9),
  with no destination, target or shooter.
  2026-10-12, round-39 errata: every cosmetic effect (kinds 0x0B, 0x15, 0x16, 0x17, 0x19, 0x1A and 0x1C) is created with
  only its kind and position set (destination (0, 0), no target, no shooter, damage 0), not only kind 0x16; `xorc12`
  rune vector.
  2026-10-12, round-40 errata: each fireball splash follows the hammer and breath rule: it needs a free effect slot, and
  its kind-0x17 explosion has only kind and position set.
  2026-10-12, round-44 errata: a blizzard shard's destination is its landing point from creation, through its initial
  wait; `xhuman10-blizzard-600` vector.
  2026-10-13, round-45 errata: a death coil missile's destination is its aim point from creation, it flies as a kind
  0x0A missile with impact delay 16, and the caster's target is each victim in turn; a whirlwind's destination is its
  current goal; every death-and-decay cloud's destination is the target tile's centre; raise dead's marker is placed
  like the other kind 0x16 markers (open question closed); a blizzard shard lands on the step that reaches its landing
  point, then lingers 4 cycles; trace vectors.
  2026-10-13, round-47 errata: a fireball's destination is the target tile's centre and a flame shield bolt's the target
  unit's centre, from creation; trace vectors.
  2026-10-13, round-48 errata: the fireball's last step marks it as ending and it is removed at its next update (limit
  40, or 25 for other stored damage, static study); healing and exorcism have no shooter, target or destination and wait
  18 cycles before their step; the flame shield bolt is removed an update after making its flames, and each flame waits
  5 cycles before its first step and reports no target and destination (0, 0); `xhuman10-spells-700` and
  `xhuman10-blizzard-600` vectors.
  2026-10-13, round-49 errata: a flame's first step is counted from its first update, so a flame in a slot before the
  bolt's steps a cycle later, and a flame ignores the shielded unit until its first step (both static study).
  2026-10-13, round-50 errata: a spell walk with no route goes idle at once and runs the idle logic; a flame's every
  step checks the shielded unit and an ending flame's slot is freed at its next update (static study); `human14-10000`
  vector.
  2026-10-13, round-54 errata: a spell command's target unit gets the command check (a dead or undetected target is
  dropped and idle becomes pending); a flame's step counts down first and a step that reaches 0 ends it without moving
  or splashing; `xhuman10-slow-3000` vectors.
  2026-10-13, round-56 errata: an effect keeps its shooter's slot after the slot is released, and its splash credits that
  slot (static); a spell's strike makes no distance test (traced for healing); `xorc12-spells-1500` and `orc14-25000`
  vectors.
  2026-10-13, round-57 answers: the strike's missing distance test is traced for bloodlust too (`xorc12-reach-400`);
  a reused shooter slot credits its new unit in every respect, and an own or allied unit there records nothing
  (static study).
- **Changes:** 2026-10-16 (round 78, B-038 combat consolidation, `ERRATA-combat-consolidation.md`, reviewed): new
  open question "No score for an ally's kill", pointing at `combat/damage.md`, and a note on the rule it holds ("The
  shooter after its slot is released"); default: an ally's kill is credited.
- **Changes:** 2026-10-16 (round 79 traces, `ERRATA-round79.md`, reviewed by a spec-review session, 2026-10-16):
  invisibility also ends at a worker's chop or repair strike (`human13-invisible-work-hits-900`, new vectors); an
  ally's kill is credited (traced; open question "No score for an ally's kill" answered).
- **Changes:** 2026-10-16 (B-040 combat audit, `ERRATA-audit-combat.md`, erratum 4, static study; reviewed by a
  spec-review session, 2026-10-16): an attack strike ends invisibility whatever its target's state; the failed-cast
  part is held as the open question "Invisibility at a failed cast". A unit's missile follows the shooter-slot rule
  (erratum 3).
- **Changes:** 2026-10-16 (round 80, B-038 step 2, spells consolidation, `ERRATA-spells-consolidation.md`, reviewed
  by a spec-review session, 2026-10-16):
  the spec is rewritten as one model (mana, one cast pipeline, then one entry per spell in a uniform shape, then the
  effects and how each reuses a delivery of `combat/damage.md`), with an Evidence line per subsection and IDs on the
  test vectors (every old row kept verbatim). Changes of meaning: a cast that fails its target or mana check at the
  strike also ends the caster's invisibility, because the strike point clears it before the spell's strike runs
  (traced: `xhuman10-failcast-520`; open question "Invisibility at a failed cast" answered); a rune's damage doesn't go
  through `combat/damage.md`, "Applying damage" (traced: `xorc12-spells-hits-1150-1260`; static study for the rest).
  Made explicit: the strike reveal of slow and haste, the walk decision's checks (no detection test), the eye of
  Kilrogg's order (no walk, no decision checks), the fireball as a hammer-and-breath missile, the death coil as a
  normal missile, the spell splash as the hammer's armour-free splash, food, and the Classic-mode labels.
- **Changes:** 2026-10-16 (round 81 traces, B-040 spells audit, `ERRATA-round81.md`, reviewed by a spec-review session,
  2026-10-16): a failed decision makes idle pending with wait 3 (audit erratum 3, traced); the building and organic
  tests come after the walk (erratum 6, traced); a caster already in reach casts at a target it no longer detects, and
  the walk drops an undetected one (erratum 5, traced); a demolition squad or sapper killed in the timers pass still
  counts its timers (erratum 7, traced); flames go on around a unit inside a building (erratum 4, traced); a flame
  shield on the caster itself can't be commanded (erratum 2, note); slot before draws (erratum 1), holy vision's sound
  per point (erratum 8) and the eye's sound only with an eye (erratum 9) as labelled static gap-fills; open questions
  "Blizzard shard landing at exactly 0" and "Eye of Kilrogg's blocked-exit message" closed; vectors S90 to S100. Review
  fixes: S95 counts only the hits made while the peasant is inside (4 on the hall, not 5), S100's scenario blocks the
  eye's air-unit search, and the traced and static parts of each rule are labelled apart.
- **Changes:** 2026-10-16, round 83 (traces, `ERRATA-round83.md`, reviewed by a spec-review session, 2026-10-16):
  the all-spells cheat's gloss in "Giving the order" corrected: it makes every spell known only for players
  controlled by a person (`game/cheats.md`), while the mana top-up applies to every caster's spell command, a
  computer player's included (AI3, traced: `xhuman2-xhuman02-cheat-scope-1600`); vector S101. Review (labelled static
  gap-fill, raised by the round-82 review): "The timers pass" lists the clearing of the computer casters' target marks
  as step 6, as `ai/caster-ai.md` ("Marks") states it.

## Behaviour

Phase numbers are those of `core/timing-and-rng.md`. "Classic" and "Enhanced" are its two rule modes. Flag bits are
the PUD unit flags. A **tile centre** is the tile's pixel position plus (16, 16). **Effects** are the game's missile
objects: they live in the effect slots of `core/unit-slots.md` and are updated in phase 4 after the units.

### The model

Every spell goes through the same chain:

1. **Mana.** Casters hold mana from 0 to 255; it grows by 1 every 40 cycles in the timers pass ("Mana").
2. **The command.** A spell command makes the spell order pending ("Giving the order").
3. **The walk and the decision.** The order's logic walks the caster into the spell's radius, checks the target once,
   and starts the casting animation ("Walking and the decision").
4. **The strike point** of the casting animation ends the caster's invisibility, makes the strike reveal, then runs the
   spell's strike ("The strike point").
5. **The spell's strike** checks the target and the mana, pays, makes the caster idle (except the two channelled
   spells) and does the spell's effect ("The spell's strike", "The spells").
6. **The effect** is one of: a timer set on the target ("Timers"), a unit created ("Summoned units and decay"), runes
   placed ("Runes on the map"), a direct change (healing, unholy armour's halving, polymorph's kill), damage applied
   at once through `combat/damage.md`, "Applying damage" (exorcism), or an effect in an effect slot whose damage
   reuses a delivery of `combat/damage.md` ("How spell damage is delivered", "Spell effects in flight").
7. **The timers pass** at the end of phase 4 runs the timers, mana, decay and runes ("Timers", "The timers pass").

| Spec | What it holds for spells |
| --- | --- |
| `orders/orders.md` | The spell orders (38 to 56), each spell's radius, the command target check, a player's spell command forgetting the attacker |
| `core/animation-scripts.md` | The casting animation, its strike point, haste and slow, "No strike while a command waits" |
| `combat/damage.md` | The deliveries spells reuse: missile flight, the gryphon hammer and dragon breath, splash, "Applying damage", credit |
| `vision/fog.md` | The strike reveal, holy vision's reveal, who detects an invisible unit |
| `core/unit-slots.md` | Effect slots, unit creation, mana at creation |
| `core/corpses.md` | The used mark (polymorph, raise dead) |
| `ai/caster-ai.md` | How the computer chooses spells and targets (its spell mechanics are in "The computer's casters") |

### Mana

Mana is a byte from 0 to 255 on units with flag bit 17 (caster). A newly created caster that isn't a building starts
with **85** and its mana counter at **40**; a building with the flag starts with its type's "magic" value and its
counter at **0** (`core/unit-slots.md`). The timers pass adds 1 mana every 40 cycles up to 255 ("The timers pass",
step 4). Mana is spent only at the spell's strike ("The spell's strike"), never at the command or the decision. The
all-spells cheat sets it to 255 at each spell command ("Giving the order"); the cast buttons test it against the
button cost (`ui/command-buttons.md`).

**Evidence:** traced: the 85 start, the strike costs and the gains (`xhuman10-spells-700`, `xhuman10-failcast-520`:
+1 every 40 cycles while channelling); static study: the counters, buildings and the cost table (a table in the
program, not in the game data).

### Orders and mana costs

Each spell is an order from 38 to 56 (`orders/orders.md`, "Spell orders", which also gives each spell's radius). Mana
is a byte from 0 to 255.

| Order | Spell | Mana | Aimed at | Effect happens |
| --- | --- | --- | --- | --- |
| 38 | Holy vision | 70 | a point | once |
| 39 | Healing | 6 per hit point healed | a unit | once |
| 40 | Not used (no effect; its table cost is 5) | — | — | — |
| 41 | Exorcism | 4 per hit point of damage | a point | once |
| 42 | Flame shield | 80 | a unit | once |
| 43 | Fireball | 100 | a point | once |
| 44 | Slow | 50 | a unit | once |
| 45 | Invisibility | 200 | a unit | once |
| 46 | Polymorph | 200 | a unit | once |
| 47 | Blizzard | 25 per strike | a point | repeatedly (channelled) |
| 48 | Eye of Kilrogg | 70 | a point | once |
| 49 | Bloodlust | 50 | a unit | once |
| 50 | Raise dead | 50 per skeleton | a point | once |
| 51 | Death coil | 100 | a point | once |
| 52 | Whirlwind | 100 | a point | once |
| 53 | Haste | 50 | a unit | once |
| 54 | Unholy armour | 100 | a unit | once |
| 55 | Runes | 200, partly refunded | a point | once |
| 56 | Death and decay | 25 per strike | a point | repeatedly (channelled) |

Which spells a player may cast (research, the button bar) is not covered here.

**Evidence:** static study (the cost table is a table in the program, not in the game data; the cast buttons use the
same values, `ui/command-buttons.md`); traced: the costs of the spells cast in the spell traces, for example holy
vision, polymorph, invisibility and fireball (`xhuman10-spells-700`), healing (S64), bloodlust (S65) and blizzard
(S84).

### Giving the order

When a spell command is given:

- If the **all-spells cheat** is on (the cheat that makes every spell known for every player controlled by a person,
  `game/cheats.md`), the caster's mana is set to 255 first, whoever owns the caster: a computer player's caster gets
  it too (round 83; traced: `xhuman2-xhuman02-cheat-scope-1600`, S101). The same cheat lets the cast buttons skip
  their mana test (`ui/command-buttons.md`).
- The spell order becomes the caster's pending order (`orders/orders.md`). The target unit gets the same check as any
  command's target (`orders/orders.md`, "Commands forget a stale attacker"): if it is dying, dead (a body or ruin) or
  not detected, it is dropped and idle becomes the caster's pending order instead, which counts as a command and takes
  over at the next step done with wait 3 (traced: `xhuman10-slow-3000`, vectors below).
- For the spells aimed at a point (orders 38, 40, 41, 43, 47, 48, 50, 51, 52, 55 and 56) the caster's target unit is
  cleared and only the target tile is kept.

A player's spell command also forgets the caster's recorded attacker (`orders/orders.md`). The computer's eye of
Kilrogg try sets order 48 as the pending order directly, with no command, so neither the check nor the cheat applies
("The computer's casters").

**Evidence:** traced: the cheat (every spell trace), the dropped target (`xhuman10-slow-3000`, S70); static study:
the point spells' target.

### Walking and the decision

Each time the order's logic runs, the caster moves toward the target until it is within the spell's radius
(`movement/movement.md`), and then casts:

- **Point spells:** once arrived, the caster turns to face the point and plays its casting animation.
- **Unit spells** (flame shield, slow, invisibility, haste, unholy armour): the target must still be valid and must not
  be a building.
- **Organic-target spells** (healing, polymorph, bloodlust): the target must also have flag bit 27 (organic).

"Valid" here and at the strike means the same thing: the target unit exists and is not dying, dead remains or otherwise
inactive. Neither check tests whether the caster's player detects the target. Detection is tested when the command is
given ("Giving the order") and, while the caster walks, at each step boundary of the walk, which drops a target the
caster's player no longer detects (`orders/orders.md`, "Walking orders", step 2); the decision then fails for lack of a
target. A caster already within the radius doesn't walk, so **it casts at a target its player no longer detects** (round
81; traced: `xhuman10-invisible-reach-goals-400`, S92). **Eye of Kilrogg** (order 48, radius "always") has no walk and
no decision checks: the caster casts where it stands, and only the strike's mana check applies.

The not-a-building and organic tests are made only once the caster is within the radius: a caster ordered to cast
such a spell on a building or a non-organic unit first walks into reach, and only then becomes idle. Out of reach,
only the walk's lost-target test applies (round 81; traced for the organic test, healing on a ballista:
`xhuman10-heal-ballista-goals-500`, S91; static study for the building test).

If a check fails, idle becomes the caster's **pending** order (with the Enhanced resume of `orders/orders.md`, "Resuming
patrol and attack-move") and takes over in the same update ("Same-update take-over"): order 2 (or the resumed order),
the still animation, wait 3. The idle logic first runs at that animation's next step done, 3 cycles later, as on arrival
(`orders/orders.md`, "Arriving and losing the target"). No mana is spent (round 81; traced:
`xhuman10-slow-killed-goals-400`, S90, for a target that died during the walk, and S91; the resume follows from the
stated rule of `orders/orders.md`, untraced for a cast). If the walk toward the target finds **no route**, the caster
becomes idle at once, as an immediate order, and its idle logic runs in the same update (`orders/orders.md`, "No route";
traced: `human14-10000`, vectors below).

The spell takes effect at the **strike** event of the caster's attack animation (`core/animation-scripts.md`): cycle 12
of a 40-cycle loop for mages and death knights, cycle 11 of a 25-cycle loop for paladins, and cycle 8 of a 25-cycle
loop for ogre-mages, without haste or slow.

The distance to the target is tested only here, when the order's logic decides to cast. Once the casting animation is
playing, its strike takes effect even if the target has since moved outside the spell's radius (traced for healing,
`orc14-25000`, and for bloodlust, `xorc12-reach-400`; vectors below; static study for the other spells).

**Evidence:** traced: no route (`human14-10000`, S81), no distance test at the strike (S64, S65), the strike cycles
(`xhuman10-failcast-520`: decision at n = 23, strikes every 40 cycles from n = 37); static study: the three decision
checks and the missing detection test; traced (round 81): the pending idle (S90, S91), the tests after the walk
(S91), the cast at an undetected target in reach (S92); traced: the eye cast where the caster stands
(`xorc12-ogre-mage-180`: order 48 from n = 103 at (114, 9), 10 tiles from the target point, the eye at (114, 10) at n =
113, no step taken); game data: the strike cycles (`Rez\script.bin`).

### The strike point

At each strike point of the casting animation (`core/animation-scripts.md`), in this order:

1. If the caster has been given a command that hasn't taken over yet, nothing happens at all ("No strike while a
   command waits").
2. The caster's **invisibility timer is set to 0**. Every strike point does this, whatever the spell and whatever its
   strike then does: a cast that fails its target or mana check, or that does nothing, ends the caster's invisibility
   too (traced: `xhuman10-failcast-520`, S84). It is the same step that ends invisibility at an attack, chop or repair
   strike (`combat/damage.md`, "How an attack delivers damage").
3. The **strike reveal** (`vision/fog.md`, item 4): for slow and haste, the two spells in its list, if the target's
   owner isn't the caster's owner and is one of players 0 to 7, a sight-2 area around the caster is revealed for the
   target's owner.
4. The **spell's strike** (below).

**Evidence:** traced: a failed cast ends invisibility (`xhuman10-failcast-520`, S84); static study: a successful
cast ends it too (no trace has an invisible caster whose cast succeeds), the order of the steps and the strike
reveal's spell list.

### The spell's strike

Unless the spell's entry below says otherwise, the strike works like this. None of the strike's checks is a distance:
a target that has left the radius during the casting animation is still affected.

1. **Target check** (unit spells only): the target must exist and must not be dying, dead remains or otherwise
   inactive. If it fails, the caster becomes idle and nothing else happens.
2. **Mana check:** if the caster's mana is below the cost, it becomes idle and nothing happens.
3. **Pay** the cost.
4. The caster becomes **idle** (order 2 pending). The two channelled spells skip this step, so their order stays and
   they strike again on every loop of the casting animation until a mana check fails.
5. The spell's effect.

"Becomes idle" at the strike means idle as the pending order: the caster keeps its spell order and plays its casting
animation to the end of the loop, and idle takes over then (traced: in `xhuman10-failcast-520` the failed strike at n =
437 leaves order 47 until n = 466). A "unit spell" with no target at its strike (for example, the target died) makes the
caster idle without paying.

(Before round 80 this list had a step "the caster's invisibility timer is cleared" between steps 4 and 5; it is now
step 2 of "The strike point", which comes first.)

**Evidence:** traced: the mana check and its pending idle (`xhuman10-failcast-520`, S84); static study: the target
check at the strike (S86; the `xhuman10-slow-3000` vector S70 is the command's check, not the strike's) and the
order of the steps.

### The spells

Each entry gives the strike's steps from "The spell's strike", then the effect. "Marker" means a cosmetic effect
kind 0x16 on the target ("Cosmetic effects").

| Spell | Strike, in order |
| --- | --- |
| Holy vision | Steps 2 to 4. Then 7 areas are revealed for the caster's player, centred on the target point T and on T + (0, −8), T + (0, +8), T + (+6, +4), T + (+6, −4), T + (−6, +4) and T + (−6, −4), in that order. Each is revealed exactly like a unit with **sight 9** standing on that tile (`vision/fog.md`). Each point also gets a cosmetic effect (kind 0x16) at its tile's pixel position + (8, 8), and the spell's sound, placed on that point's tile, so up to 7 sounds (round 81, static gap-fill, audit erratum 8; S99). Points off the map are skipped: in Enhanced mode a point is used only if both coordinates are from 0 to map width − 1; **Classic mode:** a coordinate may also equal the map width, and such a point reveals nothing but still gets its cosmetic effect (and its sound). A negative coordinate is off the map in both modes. (Maps are square, so the width is also the height.) In Enhanced mode the fog refresh countdown is then set to 150, so the next refresh comes 150 cycles later (**Classic mode:** unchanged) |
| Healing | Step 1 (no target: idle). The caster becomes idle. Let missing = maximum HP − HP. If missing is 0, nothing more. Otherwise heal = min(missing, floor(mana / 6), 40). If heal is 0, nothing more. The target gains heal HP, the caster pays 6 × heal mana, and a healing effect (kind 9) appears on the target |
| Exorcism | The caster becomes idle (no up-front cost). Then the 7 × 7 tiles around the target tile are scanned in 4 rings (below). At the start of each ring, if mana is below 4, the scan stops. Each ground-layer unit found that has flag bit 15 (undead) is exorcised, except, in Enhanced mode, a unit of players 0 to 7 that is allied with the caster (**Classic mode:** allies are exorcised too). Exorcising: damage = min(floor(mana / 4), the unit's HP); an exorcism effect (kind 8) appears on it; the damage is applied with the caster as the attacker (`combat/damage.md`, "Applying damage"); the caster pays 4 × damage |
| Flame shield | Step 1. The caster becomes idle **before** the mana check. If the target's flame-shield timer is not 0, or the target has flag bit 1 (air), nothing more happens and no mana is spent. Otherwise step 2 (mana check: idle), pay, set the target's flame-shield timer to **500**, and launch the flame-shield bolt (below) |
| Fireball | Steps 2 to 4, then launch the fireball (below) |
| Slow | Steps 1 to 4. Then the target's speed timer: if it is negative, it becomes −1000; otherwise it becomes timer − 1000. Marker on the target |
| Invisibility | Steps 1 to 4. The target's invisibility timer becomes **2000**. Marker on the target |
| Polymorph | Steps 1 to 4. The target gets the **used** mark (`core/corpses.md`) and is killed: it plays its own death animation and then, if its type has a corpse kind, leaves a dead body that keeps the used mark, so raise dead can't use it; the body decays on the normal timetable. The kill is outright: no damage step, no credit (`combat/damage.md`, "Credit and score"). A critter (type 0x39) is created for the neutral player (15) at the target's tile (2 creation draws, below), and effect kind 0x16 appears at that tile's centre |
| Blizzard | Step 2, pay 25 (no idle). Five shard chains are started, each with 10 repeats (below) |
| Eye of Kilrogg | Steps 2 to 4. An eye of Kilrogg (type 0x2D) is created for the caster's player next to the **caster** (not the target point), with the free-tile search of `movement/placement.md` ("a spell creates a unit next to its caster"). The search makes no draws; the new eye makes 1 creation draw (below). If a tile is found, the eye is created and the spell's sound plays. If no tile is found, no eye is created, the mana stays spent, the blocked-exit message (entry 67 of `network.tbl`) is set, and no sound plays (round 81, static gap-fill, audit erratum 9; S100) |
| Bloodlust | Steps 1 to 4. The target's bloodlust timer becomes **1000**. Marker on the target |
| Raise dead | The caster becomes idle. Then every unit slot is scanned in ascending order (`core/unit-slots.md`) while the caster's mana is at least 50. A slot qualifies if it holds a dead body (type 0x69) in its remains state that has not been used yet, and the squared tile distance from the body's tile to the target tile, dx² + dy², is at most **36**. For each: a skeleton (type 0x37) is created for the caster's player directly on the body's tile (no free-tile search; only the creation checks of `core/unit-slots.md` apply); the skeleton makes 1 creation draw (below); if that worked, effect kind 0x16 appears on it and the body gets the **used** mark; in either case the caster pays **50**. The used mark only stops the body being raised again: it keeps decaying and is removed on its normal timetable (`core/corpses.md`) |
| Death coil | Steps 2 to 4. Then victims are chosen and missiles launched (below), and the caster gains HP |
| Whirlwind | Steps 2 to 4, then the whirlwind is created (below) |
| Haste | Steps 1 to 4. Then the target's speed timer: if it is greater than 0, it becomes 1000; otherwise it becomes timer + 1000. Marker on the target |
| Unholy armour | Steps 1 to 4. The target's unholy-armour timer becomes **500**. Its HP is halved, rounding down, if it is at least 2 (1 HP stays 1); this is a direct change, not damage. Marker on the target |
| Runes | Steps 2 to 4, then up to 5 runes are placed and part of the cost is refunded (below) |
| Death and decay | Step 2, pay 25 (no idle). Five decay clouds are created, each with 10 pulses (below) |

**Exorcism rings.** With the target tile at (x, y), ring k (k = 0 to 3) is visited like this:

1. For each column from x − k to x + k, left to right: the tile (column, y + k), then the tile (column, y − k).
2. Then, if k ≥ 1, for each row from y − k + 1 to y + k − 1, top to bottom: the tile (x + k, row), then the tile
   (x − k, row).

Ring 0 therefore visits the target tile **twice**. Tiles off the map are skipped. Only the ground-layer unit of each tile
is looked at (`movement/occupancy.md`), so a building is visited once per footprint tile it covers.

**Death coil victims.** The 5 × 5 tiles centred on the target tile are scanned row by row from the top, left to right.
On each tile the ground-layer unit is looked at first, then the air-layer unit. A unit is a candidate unless it is:
allied with the caster (Enhanced mode only; **Classic mode:** allies are candidates), owned by the caster's player,
dying or dead, or without flag bit 27 (organic). A building covering several scanned tiles is found once per tile. The
candidates are then sorted by HP, lowest first. Candidates with equal HP don't keep their scan order. The order is the
one you get by going through the list positions from the first, and for each position comparing its candidate with
every later candidate in turn, swapping the two whenever the one at the position has more HP. For example, scan order
A (30 HP), B (30 HP), C (5 HP) becomes C, B, A. A budget of **50** is shared out in that order: each victim takes its
full HP while the budget lasts; the first victim that doesn't fit takes what is left, and the victims after it get
nothing and are dropped. For each kept victim, in order, the caster launches its own missile weapon at it through the
normal launch of `combat/damage.md` (so the launch makes its usual damage draw and two scatter draws), and the
missile's stored damage is then replaced by the victim's share. Each coil missile is a normal missile of kind 0x0A
(`combat/damage.md`, "Missile flight"): its destination is its aim point (the victim's centre plus the scatter) from
the moment it is created; it flies N = floor(D / 12) + 1 steps, landing on the step where the remaining distance goes
below 0, and stays there; its share is applied at its impact, **16** cycles after the landing step. As each missile is
launched, the caster's target becomes that victim, so after the launches the caster's target is the last victim, until
the idle takes over (traced: `xorc01-death-coil-2500`, vectors below). On impact the share is applied to the victim
with the caster as the attacker. The caster then gains the total of the shares at once, capped at its maximum HP. With
no candidates, the 100 mana is still spent.

**Runes.** The tiles tried are, in order: the target tile, then the tiles north, east, south and west of it. A rune is
placed on a tile if the tile is on the map, no ground-layer unit or building is on it, no live rune is already there,
and one of the 50 rune slots (shared by all players) is free. There is no terrain test: runes can go on water, trees
and rock. Each placed rune gets a lifetime of **2048** and a cosmetic effect (kind 0x0B) at the tile centre. With n
runes placed (0 to 5), the caster is refunded (5 − n) × 40 mana, so it pays 200 − (5 − n) × 40 in all.

**Evidence:** traced: holy vision, polymorph, fireball, healing and invisibility (`xhuman10-spells-700`), blizzard,
slow and flame shield (`xhuman10-blizzard-600`, `xhuman10-slow-3000`), haste, unholy armour, whirlwind, death and
decay and raise dead (`xorc01-death-knight-2100`), death coil (`xorc01-death-coil-2500`), bloodlust, runes and eye of
Kilrogg (`xorc12-ogre-mage-180`, `xorc12-spells-1500`), exorcism (`xhuman10-exorcism-3000`); static study: the order
of each strike's steps, exorcism's rings, death coil's sort, raise dead's scan and the Classic-mode rules.

### How spell damage is delivered

Spells reuse the deliveries of `combat/damage.md` and add two of their own. This table is the map; the details are in
the sections named.

| Spell | Delivery | Where |
| --- | --- | --- |
| Fireball | The **gryphon hammer and dragon breath** missile (kind 2 shares their behaviour): stored damage 40 (limit 40), no target, no damage roll and no scatter; its final hit goes to a wall on its final tile | `combat/damage.md`, "Missile flight"; here, "Fireball" |
| Death coil | A **normal missile** (kind 0x0A) launched through the normal launch, its stored damage replaced by the victim's share | `combat/damage.md`, "Missile flight"; here, "Death coil victims" |
| Flame shield, blizzard, death and decay, whirlwind | The hammer and breath **splash**: armour-free, air units included | `combat/damage.md`, "Missile flight" (the splash paragraph) and "Splash on impact"; here, "Splash" |
| Exorcism | Damage applied at once through "Applying damage" (no missile), with the caster as the attacker | `combat/damage.md`, "Applying damage" |
| Runes | A **direct HP change**: not through "Applying damage" | here, "Runes on the map" |
| Polymorph | An **outright kill** | here, "The spells" |
| Unholy armour | HP halved directly (not damage) | here, "The spells" |

Every delivery but the runes, polymorph and unholy armour's halving ends in `combat/damage.md`, "Applying damage",
with its credit and score. The caster, or the unit in the effect's shooter slot, is the attacker ("The shooter after
its slot is released").

**Evidence:** traced: the fireball's flight (`xhuman10-spells-700`), the coil missiles (`xorc01-death-coil-2500`), the
blizzard shards' landing (`xhuman10-blizzard-600`, S69), the rune path (`xorc12-spells-hits-1150-1260`, S85); static
study: that fireball and hammer share one behaviour, that the four splash kinds share the hammer's splash.

### Timers

Each unit has five spell timers. They are advanced by the **timers pass** at the end of phase 4, once per cycle, in
ascending slot order. Units that are dying or are remains, and units in one other inactive state, are skipped.

| Timer | Set by | Each cycle | While running |
| --- | --- | --- | --- |
| Invisibility | Invisibility (2000) | −1 toward 0 | The unit is detected only as `vision/fog.md` describes. Every strike point of the unit sets it to 0: a spell cast, whether the cast then succeeds or fails ("The strike point"), an attack strike, and a worker's chop or repair strike (traced: `human13-invisible-work-hits-900` and `xhuman10-failcast-520`, vectors below). An attack strike sets it to 0 whatever its target's state, a target that has gone inside included (static study; `combat/damage.md`, "How an attack delivers damage") |
| Unholy armour | Unholy armour (500) | −1 toward 0 | The unit takes no damage from hits (`combat/damage.md`), and runes don't hurt it |
| Bloodlust | Bloodlust (1000) | −1 toward 0 | Basic and piercing damage are doubled (`combat/damage.md`) |
| Speed (signed) | Haste (positive), slow (negative) | moves 1 toward 0 | Positive: hasted; negative: slowed (`core/animation-scripts.md`) |
| Flame shield | Flame shield (500) | −1 toward 0 | Only blocks a second flame shield; the flames themselves last on their own (below) |

The computer's casters also set timers to 1 or −1 as marks; they are real timers ("The computer's casters").

**Evidence:** traced: the timer values and countdowns (every spell trace; S84 for invisibility), the strikes that end
invisibility (S82, S83, S84); static study: the skipped states.

### The timers pass

Also in the timers pass, for each unit, in this order:

1. **Berserker regeneration:** a berserker (type 0x13) whose player has the regeneration upgrade counts cycles in
   its mana byte. When the count reaches 240 it goes back to 0, and the berserker gains 1 HP if below its maximum.
2. **Demolition squads and sappers** (types 0x0E and 0x0F) are killed at once if their invisibility or unholy-armour
   timer is running. The killed unit's remaining steps still run in that pass, so its running timers count down by one;
   from the next pass on it is dying and skipped, so the timer keeps that value (round 81; traced:
   `xhuman10-invisible-dwarves-150`, S93, invisibility on a demolition squad, which reads 1999 until the squad is
   removed; static study for a sapper and for unholy armour; S14 agrees).
3. The five timers above.
4. **Mana** for units with flag bit 17 (caster): if mana is below 255, a per-unit counter counts down; when it reaches
   0 it is reset to 40 and mana goes up by 1. So a caster gains 1 mana every 40 cycles, up to 255. This includes
   buildings with the flag. Every newly created caster that isn't a building (placed by the map, trained, or created
   any other way) starts with **85 mana** and its counter at **40**, so its first extra mana point comes on its 40th
   timers pass. A building with the flag starts with its type's "magic" value and its counter at **0**, which wraps
   to 255 on the first pass, so its first point comes on its 256th pass. While it is under construction or working,
   its mana byte holds its progress (`economy/build-train.md`), and these gains add to it. In the default data no
   building has the flag.
5. **Decay** for other units whose type has a non-zero decay rate, if they were created during play (below).
6. **Target marks:** the seven target marks that the computer's casters set (death coil, polymorph, holy vision,
   exorcism, healing, raise dead and whirlwind; `ai/caster-ai.md`, "Marks") are cleared (round 83 review, labelled
   static gap-fill: static study, untraced). Where this falls among the steps isn't known and changes no outcome,
   since only the computer's 50-cycle pass B reads the marks.

The runes are advanced before the units in the same pass (below).

**Evidence:** traced: mana gains (`xhuman10-failcast-520`: 14 to 15 at n = 414, 15 to 16 at n = 454); static study:
the order of the steps, berserkers, sappers and buildings, and step 6; traced (round 81): a killed demolition squad's
timer counted in the same pass (S93).

### Summoned units and decay

A unit whose type has a non-zero **decay rate** (the `UDTA` decay byte) and which is created during play (not while
the map loads) gets a **period counter** of 255 and a **countdown** equal to the decay rate. Each cycle the countdown
goes down by 1. When it reaches 0 it is reset to the decay rate and:

- if the period counter is above 0, it goes down by 1;
- otherwise, if the unit has at least 5 HP, it loses 5 HP;
- otherwise it is killed (one special case, see the open questions).

In the default unit data only the skeleton (decay 100) and the eye of Kilrogg (decay 3) decay. The unit doesn't die on
reaching 0 HP: it is killed one period later.

| Unit | HP | Decay | Killed on timers pass |
| --- | --- | --- | --- |
| Eye of Kilrogg | 100 | 3 | 3 × (255 + 20 + 1) = 828 |
| Skeleton | 40 | 100 | 100 × (255 + 8 + 1) = 26400 |

"Timers pass n" counts the pass in the cycle the unit was created as pass 1.

**Food.** Skeletons and critters (polymorph's) don't count as food in use; an eye of Kilrogg counts like any other
non-building unit while it exists (`economy/build-train.md`, "Food"; its open question "Food of summoned units"
holds the eye's case, not traced).

**Evidence:** static study (the counters, the 5-HP steps, food); game data: the decay rates and HP.

### Runes on the map

Each cycle, before the units' timers, every live rune is advanced:

1. Its lifetime goes down by 1. (Every 256 cycles, when the low byte of the lifetime becomes 1, the rune's cosmetic
   effect is shown again.)
2. If a ground-layer unit or building is on the rune's tile, the rune is used up (its slot is freed) with an explosion
   effect (kind 0x17). Then, if that unit's unholy-armour timer is 0: a unit with 50 HP or less is killed (with no
   attacker); otherwise it loses **50 HP**. Armour doesn't count, and the unit isn't told it was attacked.

A rune whose lifetime reaches 0 is gone after that cycle's check. Runes hit **any** ground unit, including the
caster's own and allied units, and ships on water. Air units never trigger them.

A rune's hit is a direct change to the unit's HP, not a delivery to `combat/damage.md`, "Applying damage": there is no
notification, no attacker record, no call for help and no under-attack alert, and a rune's kill is an outright kill
with no credit and no score (traced: the hit leaves no `hits` entry, `xorc12-spells-hits-1150-1260`, S85; static
study for the rest).

**Evidence:** traced: placement (`xorc12-ogre-mage-180`, S50), a hit (`xorc12-spells-hits-1150-1260`, S85); static
study: the lifetime, the kill, the unholy-armour test.

### Spell effects in flight

All of these are effects in the effect slots. A spell that needs a free slot and finds none simply makes that piece
of the spell fail. Every effect runs a small script from `script.bin`; the "step" below is its "step done" signal.

**How flying effects move.** An effect flying from S to G uses a fixed line rule. Let D = the larger of |Gx − Sx| and
|Gy − Sy| (the **major** distance) and d the smaller. A counter starts at floor(D / 2), or 1 if that is 0. On each
step the counter goes down by d; if it is then 0 or less, the effect also moves on the minor axis and the counter goes
up by D. The effect always moves on the major axis. With a **speed** of s pixels, each axis move is s pixels toward G.
The flight's remaining distance starts at D and goes down by s per step. For speed 1 this reaches G after exactly D
steps (for any G other than S itself).

| Effect | Kind | Speed (px per step) | Steps |
| --- | --- | --- | --- |
| Fireball | 2 | 12, then 6 | every cycle |
| Flame-shield bolt | 3 | 12 | every cycle |
| Flame | 4 | — (orbits) | every cycle |
| Blizzard shard | 5 | 12 | every cycle |
| Death-and-decay cloud | 6 | — (doesn't move) | every 24 cycles |
| Death-coil missile (default death knight) | 10 | 12 | every cycle |
| Whirlwind | 12 | 1 | every 3 cycles |

**Splash.** Flames, blizzard shards, death-and-decay clouds, whirlwinds and the fireball (kinds 4, 5, 6, 12 and 2)
damage an area through the splash of the gryphon hammer and dragon breath (`combat/damage.md`, "Missile flight"), which
is the rule of "Splash on impact" with these differences: armour **isn't subtracted** (every unit within 42 px is hit,
even with a base of 0), and the air-layer unit of each tile is looked at after its ground-layer unit. As in "Splash on
impact", the ground-layer unit on the effect's tile is considered first, then the 7 × 7 tiles row by row from the top,
each row left to right, and each unit at most once; so an air unit on the effect's tile comes in its tile's place in the
scan, not straight after that tile's ground-layer unit (`combat/damage.md`, V95). The unit that owns the effect (the
caster, or the shielded unit for flames) is never hit. So for a stored damage D:

| Distance from the effect to the unit's centre | Damage |
| --- | --- |
| up to 22 px | h + (r mod (h + 1)) with h = floor((D + 1) / 2) |
| 23 to 42 px | the same with D replaced by floor(D / 4) |
| 43 px or more | none |

One draw r is made per unit hit, in scan order. Walls on the effect's tile take D and the four neighbour walls take
floor(D / 4) (`movement/occupancy.md`, "Walls"). Each unit's damage is then applied as in `combat/damage.md`,
"Applying damage", with the effect's shooter slot as the attacker.

**Fireball.** Launched from the caster's centre toward the target tile's centre with stored damage **40**. Its
destination is the target tile's centre from creation (traced: `xhuman10-spells-700`, vectors below). On each step it
makes one draw (discarded), then moves 12 px, or 6 px if its remaining distance is already below 50. Once its remaining
distance is below 50 it counts its steps: on counts 8, 16, 24, 32 and 40 it **splashes** (damage 40) at its position and
makes a cosmetic explosion (kind 0x17); the count goes on to 41 (no splash). The step after count 41 still draws and
moves, and then, the count having passed the limit, marks the fireball as **ending** instead of counting. The limit is
40 when the stored damage is 40, as it is for every fireball, and 25 otherwise, as for the hammer and breath (static
study). A fireball marked as ending stays in its slot, unmoved, and is removed at its **next update**, with no draw and
no move (traced: `xhuman10-spells-700`, vectors below).
Each splash follows the rule of the gryphon hammer and dragon breath, with which the fireball shares its missile
behaviour (`combat/damage.md`): it first needs a free effect slot (if there is none, there is no splash, sound or
explosion on that step), and the explosion is created in that slot with only kind and position set ("Cosmetic effects").
No fireball trace shows this yet. It flies on past the target point, so the first explosion lands near the point and the
next four follow every 48 px along the same line beyond it. When it is removed, it touches a wall on its final tile
with damage 40.

**Flame shield.** The bolt flies from the caster to the target unit's centre at 12 px per step, making one draw at the
start of every step. The bolt (effect kind 3) has the target unit's centre as its destination from creation (traced:
`xhuman10-blizzard-600`, vectors below). On the step after it arrives (remaining distance 0 or less), it makes its draw
and creates **5 flames** on the target unit, whatever the target's state (round 81; static study, the flames'
going on traced in S94); it then stays in its slot, unmoved, and is removed at its **next update**, with no draw. Each
flame circles the unit's centre through a fixed 51-point path (Data), one point per cycle, starting at points 0, 10, 20,
30 and 40. A flame first **waits** at its start point: its start step comes 5 cycles after its first update (A = 5,
"When a new effect first runs") and its first path step one cycle later, so 6 cycles after its first update; its
lifetime count starts with that step. The flames are made by the bolt, so a flame in a slot after the bolt's gets its
first update in the cycle it is made and steps 6 cycles after creation (traced), and one in a slot before the bolt's
steps 7 cycles after creation (static study). A flame doesn't look at the shielded unit during that wait: it checks the
unit at its first step, and if the unit is dying or gone it ends there, its slot freed one update later (static study).
In the trace a flame has the shielded unit as its shooter, no target, destination (0, 0) and damage 4; it follows the
shielded unit. The flames' lifetimes are 512, 524, 518, 521 and 527 cycles. Each step a flame first counts its lifetime
down by 1. If the count is now 0, the flame ends at once: it doesn't move or splash on that step, and its slot is freed
at its next update (traced: `xhuman10-slow-3000`, vectors below). Otherwise it moves with the shielded unit and splashes
with damage **4** whenever the new count's low 4 bits equal 1 (so about every 16 cycles). A flame also ends, from its
first step on, as soon as the shielded unit is dying or gone; each step makes that check, and an ending flame's slot is
freed at its next update (static study). A unit **inside** something (a gold mine, a building, a transport) is neither:
its flames go on circling its stored position with the same offset as outside, and splash as usual until their
lifetimes run out. Inside a gold mine or a building the stored position is the building's top-left tile, and the
splash hits that building (round 81; traced: `xhuman10-shield-mine-hits-800`, S94 to S96). That the flames also go on
around a unit aboard a transport is static study; where they circle then isn't traced. Flames made for a unit that is
already dying end at their first step (static study).

The bolt tests its arrival after it moves. This matters only for a bolt launched at distance 0, a caster shielding
itself, which would arrive at its first step and make its flames at its second; but a unit isn't sent a command whose
target is itself, and no computer try aims flame shield at its own caster, so it can't happen in the default data (round
81; traced: `xhuman10-shield-self-200`, S97; static study for the computer). The shielded unit is never hurt by
its own flames; its neighbours, friend or foe, are.

**Blizzard shards.** Each of the 5 chains picks its landing point once, when it starts: the target tile's centre plus
((r1 mod 5) − 2) × 32 px on x and ((r2 mod 5) − 2) × 32 px on y (2 draws). Each shard of the chain starts at the landing
point minus (110, 170) px plus ((r mod 3) × 11 − 11) px on x, then on y (2 more draws), and flies to the landing point
at 12 px per step (14 to 16 steps), after an initial wait of 12 cycles. Its destination is the landing point from the
moment it is created, through the initial wait, not only once it starts flying (traced: `xhuman10-blizzard-600`, vectors
below). It makes one draw after each step that doesn't land. It **lands in the same update** as the flight step that
takes it to or past its landing point; that last step may overshoot, and the position isn't clamped. In that update it
splashes with damage **10**, and then, if the chain has repeats left and a slot is free, it starts the next shard of the
chain in the lowest free effect slot (repeats − 1, 2 draws) and makes one more draw. The landed shard's own slot isn't
free yet, so the next shard never reuses it. The landed shard then stays where it is, unmoving, for 4 more cycles, and
is removed in its 5th update after landing (traced: `xhuman10-blizzard-600`, vectors below). A remaining distance of
exactly 0 never happens: every shard starts 159, 170 or 181 px from its landing point and steps 12 px, and none of those
is a multiple of 12 (round 81, static study and arithmetic; open question closed). A chain therefore lands
**11 shards** on the same point. Damage per shard: 5 to 10 within 22 px, 1 or 2 from 23 to 42 px.

**Death and decay.** Each of the 5 clouds is placed at the target tile's centre plus ((r1 mod 5) − 2) × 32 px on x and
((r2 mod 5) − 2) × 32 on y (2 draws), and doesn't move. Every cloud's destination is the target tile's centre, whatever
its own position (traced: `xorc01-death-knight-2100`, vectors below). Counting its first update as cycle 0, its first
step comes at cycle 18 (no effect). It then **splashes** with damage **10** at cycles 34, 58, 82, … , 250 (10 pulses,
every 24 cycles). At its next step (cycle 274) it ends, and one cycle later it touches a wall on its tile with damage
10.

**Whirlwind.** Created with a lifetime of **800** and damage **4**. It starts at the target point's centre plus ((r &
127) − 64) px on x and on y (2 draws) and walks toward the target point at 1 px per step, one step every 3 cycles. On
each step, if the lifetime is odd it splashes; the lifetime then goes down by 1, and the whirlwind ends when it reaches
0 (touching a wall on its final tile with damage 4). When it reaches its goal exactly, it picks a new goal at its
position plus ((r & 127) − 64) px on x and on y (2 draws). It lasts 2400 cycles and splashes 400 times. A new goal equal
to its position is never reached, so it then drifts east until it ends. Its destination is always its current goal: the
target point's centre from creation, then each new goal as it is picked (traced: `xorc01-death-knight-2100`, vectors
below).

**The shooter after its slot is released.** An effect records its shooter as a unit slot and never clears it. When the
shooter dies and its slot is released, the effect goes on (a whirlwind doesn't look at its shooter at all), and the
trace still shows the old slot number as its shooter (traced: `xorc12-spells-1500`, vectors below). Its splash damage
is applied with that slot as the attacker, whatever unit the slot holds by then. If a new unit U holds the slot, U is
the attacker in every respect: the victim records U as its attacker and calls for help against U, and a kill gives
U's owner the kill count and the score. If U belongs to the victim's owner or an ally, nothing is recorded and there
is no call for help, but the damage still lands. A kill scores for U's owner unless U belongs to the victim's owner:
an ally's kill is credited, as `game/campaign-flow.md` says (traced for a direct kill: `human05-ally-kill-hits-1000`,
`combat/damage.md` V96). If the slot was released and not reused, the values left in it are read (static study; the
attacker record and the scores aren't in the trace format, so no trace can show this). A unit's missile follows the
same rule for its shooter (`combat/damage.md`, "Credit and score", V98).

**Cosmetic effects.** Kinds 0x0B, 0x15, 0x16, 0x17, 0x19 and 0x1A change nothing but slot use, and draw no random
number. Each plays its script once, with plain waits (haste and slow don't apply), and is removed at the script's step
done. Its first update ("When a new effect first runs") is in cycle U. Its start step comes A cycles later and restarts
its script with a wait of 1; it is removed during its update in cycle **U + A + 1 + W**, where W is the total of the
restarted script's waits. From that update on, its slot is free for any effect created later.

| Kind | Use | Script | A (cycles) | W (cycles) | Removed in cycle |
| --- | --- | --- | --- | --- | --- |
| 0x0B | rune marker | six frames with waits 3, 2, 3, 2, 3, 2 | 2 | 15 | U + 18 |
| 0x16 | spell marker (holy vision, slow, invisibility and others) | six frames with a wait of 3 each | 1 | 18 | U + 20 |
| 0x17 | explosion | sixteen frames with a wait of 3 each | 1 | 48 | U + 50 |
| 0x19 | cannon smoke (`combat/damage.md`) | four frames with a wait of 3 each | 11 | 12 | U + 24 |
| 0x15, 0x1A | impact effects (`combat/damage.md`, "Impact effects") | as given there | 1 | 12 | U + 14 |

Every cosmetic effect (kinds 0x0B, 0x15, 0x16, 0x17, 0x19, 0x1A and 0x1C) is created with only its kind and position
set: its destination is (0, 0), and it has no target, no shooter and damage 0, whoever creates it (a rune trap's
marker, a rune that goes off, a missile's impact effect, a cannon's smoke, a spell marker, an explosion). Nothing is
copied from the missile or the caster that makes it.

A kind 0x16 marker "on the target" (slow, invisibility, bloodlust, haste, unholy armour, and raise dead's marker on each
new skeleton) is placed at the unit's pixel position plus (floor(w / 2), floor(h / 2) − 4), with w × h the unit type's
box size, as for kinds 8 and 9 below. Holy vision and polymorph keep their own positions. Raise dead's marker is traced
(`xorc01-death-knight-2100`, vectors below).

Kinds 0x13 and 0x14 would also be removed at step done, but their scripts loop and never reach one. Only the map
start creates them: they are the building fires in effect slots 0 and 1 (`core/unit-slots.md`, `ui/map-drawing.md`).

Kinds 8 (exorcism) and 9 (healing) are not quite cosmetic. They are created with no shooter, no target and destination
(0, 0), at the unit's pixel position plus (floor(w / 2), floor(h / 2) − 4), with w × h the unit type's box size. In
their first update they start their flight animation, six frames with a wait of 3 each (18 cycles). They make their one
flight step, 12 px up and to the left (one draw), in their update 19 cycles after their first, and end in the next
update by touching a wall on their final tile with damage 0, which still adds 1 to that wall's damage counter
(`movement/occupancy.md`, "Walls"). Traced for healing: `xhuman10-spells-700`, vectors below.

**Evidence:** traced: the fireball (`xhuman10-spells-700`, S73, S75), flame shield (`xhuman10-blizzard-600`,
`xhuman10-slow-3000`, S71, S72, S74, S78 to S80), blizzard (S55, S69), death and decay (S67), whirlwind (S59, S61),
death coil (S56 to S58), the shooter slot (S61), the cosmetic and wisp effects (S48 to S50, S52, S68, S76, S77);
static study: the line rule, the splash rule's details, the fireball's limit and splash slot, the flames' wait before
a slot, the released-and-reused shooter slot (S62, S63), the cosmetic timings in cycles (S47, S51).

### When a new effect first runs

Effects are updated in ascending slot order after all units in phase 4, and a new effect takes the lowest free
effect slot.

- An effect created by a unit (a spell strike, the timers pass excepted) gets its **first update in the same cycle**.
- An effect created by another effect gets its first update in the same cycle if its slot comes after the creator's
  slot, otherwise in the next cycle.
- An effect created in the timers pass (rune effects) gets its first update in the next cycle.

The effect starts at its **start step**: in its first update if its kind's start animation has no wait, otherwise A
cycles later (table below). The start step puts the effect into its started state, so its picture shows from then on
(`ui/map-drawing.md`), and restarts its script with the kind's next animation and a wait of 1. Its first **step** comes
one cycle after the start step at the earliest (the cloud's own timing is given above, counted from its first update).
So a fireball cast at cycle C (A = 0) makes its first step and first draw at C + 1.

| Kinds | A (cycles) |
| --- | --- |
| 0x00 to 0x03, 0x07 to 0x0A, 0x0C to 0x12, 0x18, 0x1B | 0 |
| 0x15, 0x16, 0x17, 0x1A, 0x1C | 1 |
| 0x0B | 2 |
| 0x04, 0x13, 0x14 | 5 |
| 0x05, 0x19 | 11 |
| 0x06 | 18 (waits 3, 2, 3, 2, 3, 2, 3) |

A is the total of the waits in the kind's start animation (animation 0 of its script). All missiles have A = 0. The
blizzard shard's initial wait of 12 cycles and the death and decay cloud's first step at cycle 18 (above) already
include their start animations.

**Evidence:** traced: the building fires' start step (S52) and the waiting shards (S55); static study: the other
start steps (S46, S47, S51); game data: the start animations (`Rez\script.bin`).

### Effect slots

Every spell effect takes the **lowest free effect slot** (`core/unit-slots.md`, "Effect slots"): 400 slots in Enhanced
mode (**Classic mode:** 200), slots 0 to 2 holding the map's permanent effects. A spell that needs a free slot and finds
none makes only that piece of the spell fail: no marker, no shard, no splash explosion, and so on. Nothing is refunded:
mana the strike has already paid stays paid. The slot is found **before** the piece makes any draw, so a piece with no
slot makes none of its draws: a blizzard chain neither its landing-point nor its start-offset draws, a landed shard
neither the next shard's start-offset draws nor the draw after them (and the chain ends), a death-and-decay cloud none
of its position draws, a whirlwind none of its start-offset draws (round 81, static gap-fill, audit erratum 1;
S98). A removed effect's slot is free for any effect created later; when that happens is given per kind above ("Cosmetic
effects", the flight sections).

**Evidence:** static study (the slot counts, the lowest-free search); traced: the slot numbers in every spell vector.

### Random draws

Creation draws follow `core/timing-and-rng.md`. A unit whose type has a decay rate and that is created during play
makes no facing draw; every other mobile unit does; every created unit makes the first-wait draw last.

| Spell | At the strike | Per effect step | Per unit hit |
| --- | --- | --- | --- |
| Holy vision, slow, invisibility, bloodlust, haste, unholy armour, flame shield (strike), fireball (strike), runes | 0 | — | — |
| Healing | 0 | 1 (the healing effect's one flight step) | — |
| Exorcism | 0 | 1 for each exorcism effect's flight step | 0 |
| Polymorph | 2: the critter's facing, then its first wait (after the kill, before the cosmetic effect) | — | — |
| Eye of Kilrogg | 1 (the eye's first wait), only if a tile was found | — | — |
| Raise dead | 1 per skeleton created (its first wait), in slot order, each before that skeleton's cosmetic effect | — | — |
| Fireball | 0 | 1 at the start of every step | 1 per unit hit by each of the 5 explosions |
| Flame shield | 0 | bolt: 1 at the start of every step, including the step that creates the flames; flames: 0 | 1 per unit hit by a flame's splash |
| Blizzard | 20: per chain, in chain order, 2 for the landing point then 2 for the first shard's start offset | 1 after each step that doesn't land | 1 per unit hit; then, if a next shard starts, 2 for its start offset and 1 more |
| Death and decay | 10: per cloud, in cloud order, 2 for its position | 0 | 1 per unit hit by each pulse |
| Whirlwind | 2: start offset, x then y | 0, except 2 (new goal, x then y) on the step that reaches the goal | 1 per unit hit by a splash |
| Death coil | 3 per kept victim, in order (the missile launch's damage draw and two scatter draws) | 1 at the end of each of the missile's flight steps, including its last | 0 |

The blizzard, death and decay and whirlwind draws are made only by pieces that found an effect slot ("Effect slots";
round 81, static gap-fill). On a step that both splashes and moves (whirlwind), the splash draws come first. On a
fireball or bolt step the step draw comes before any splash draws.

**Evidence:** static study (vectors S43 to S46); the traces' `rng` fields hold the draw counts for checking.

### The computer's casters

How the computer picks a spell and a target is in `ai/caster-ai.md`. Three of its rules are spell mechanics and use
this spec unchanged:

- **Timer marks.** A computer try may set a target's spell timer to 1 (or −1) as a mark. It is a real timer: it has
  its normal effect until the timers pass of the same cycle runs it out (an unholy-armour mark of 1 blocks that
  cycle's damage and kills a sapper or demolition squad in that pass; "The timers pass", step 2).
- **Eye of Kilrogg.** The eye try sets order 48 as the pending order directly: there is no command, so no target
  check and no cheat top-up ("Giving the order"); the strike's mana check still applies.
- **A unit spell at a tile with no unit** fails the decision's target check and the caster becomes idle by the pending
  form (wait 3), with nothing paid ("Walking and the decision"; round 81: the form is traced for player casts, S90 and
  S91, and is static study for this case).

**Evidence:** static study (`ai/caster-ai.md`).

## Data

| Item | Value |
| --- | --- |
| Mana costs, orders 38 to 56 | 70, 6, 5, 4, 80, 100, 50, 200, 200, 25, 70, 50, 50, 100, 100, 50, 100, 200, 25 |
| Healing cap per cast | 40 HP |
| Timer values | invisibility 2000; unholy armour 500; bloodlust 1000; haste +1000 or set 1000; slow −1000; flame shield 500 |
| Holy vision offsets (tiles) | (0, 0), (0, −8), (0, +8), (+6, +4), (+6, −4), (−6, +4), (−6, −4); sight 9 each |
| Enhanced fog refresh after holy vision | 150 cycles |
| Raise dead | 50 mana each; squared tile distance ≤ 36 |
| Death coil | 5 × 5 tiles; budget 50 |
| Runes | 5 tiles; 50 slots; lifetime 2048; 50 damage; refund 40 per rune not placed |
| Fireball | damage 40; 5 explosions at counts 8 to 40 |
| Flame shield | 5 flames; damage 4; lifetimes 512 + 3 × {0, 4, 2, 3, 5} |
| Blizzard | 5 chains × 11 shards; damage 10; start offset (−110, −170) px |
| Death and decay | 5 clouds × 10 pulses every 24 cycles; damage 10 |
| Whirlwind | lifetime 800 steps of 3 cycles; damage 4; wander ±64 px |
| Caster mana regeneration | +1 per 40 cycles, cap 255 |
| Berserker regeneration | +1 HP per 240 cycles |
| Decay periods before HP loss | 255 |
| Effect speeds (px per step), kinds 0 to 28 | 16, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 12, 8, 8, 12, 12, 16, 16, 0, 0, 0, 0, 0, 16, 0, 0, 12, 0 |

**Flame path** (pixel offsets from the shielded unit's centre, points 0 to 50):

| Points | x | y |
| --- | --- | --- |
| 0–9 | 1, 4, 7, 10, 13, 16, 19, 22, 25, 28 | −34, −34, −34, −33, −32, −31, −30, −28, −26, −23 |
| 10–19 | 30, 31, 32, 32, 32, 31, 30, 28, 25, 22 | −20, −17, −14, −11, −8, −5, −2, 1, 4, 6 |
| 20–29 | 19, 16, 13, 10, 7, 4, 1, −2, −5, −8 | 8, 9, 10, 11, 12, 12, 12, 12, 11, 11 |
| 30–39 | −11, −14, −17, −20, −23, −25, −27, −28, −28, −28 | 10, 8, 7, 5, 2, −1, −4, −7, −10, −13 |
| 40–50 | −27, −26, −24, −22, −19, −16, −13, −10, −7, −4, −1 | −16, −19, −22, −25, −27, −29, −31, −32, −33, −34, −34 |

After point 50 the path goes back to point 0.

**Default unit data used here:** flag bit 15 (undead): death knight 0x0B, Teron Gorefiend 0x15, Gul'dan 0x33,
skeleton 0x37. Flag bit 1 (air): 0x16, 0x23, 0x28, 0x29, 0x2A, 0x2B, 0x2D, 0x38. Flag bit 17 (caster): 0x0A, 0x0B,
0x0C, 0x0D, 0x15, 0x17, 0x18, 0x2C, 0x31, 0x33, 0x34. Decay: skeleton 100, eye 3, all others 0.

**Evidence:** game data (`Rez\unitdata.dat`: the HP, decay rates and flag bits; checked in review), static study (the
cost and timer values, the flame path and the effect speeds, read from the program's tables).

## Test vectors

IDs S01 to S83 are the rows before round 80, unchanged apart from the ID column; S84 on are new.

| ID | Input | Expected |
| --- | --- | --- |
| S01 | Paladin with 100 mana heals a footman at 30 of 60 HP | missing 30, floor(100 / 6) = 16, heal 16: footman 46 HP, mana 4 |
| S02 | Paladin with 255 mana heals a footman at 0 missing HP | Nothing healed, no mana spent; the paladin becomes idle |
| S03 | Paladin with 255 mana heals a knight missing 60 HP | heal = min(60, 42, 40) = 40; mana 255 − 240 = 15 |
| S04 | Paladin with 100 mana, exorcism with a 40-HP skeleton alone on the target tile | Ring 0, first visit: damage min(25, 40) = 25, mana 0. Second visit of the same tile: damage 0 (applied, so the skeleton is still told it was attacked), mana 0. Ring 1: mana below 4, stop. Skeleton at 15 HP |
| S05 | Paladin with 255 mana, same skeleton | First visit: damage 40, skeleton dies, mana 255 − 160 = 95. Second visit: the tile is empty. Rings 1 to 3 continue while mana ≥ 4 |
| S06 | Flame shield on a unit whose flame-shield timer is 120 | Nothing; no mana spent; the caster is idle |
| S07 | Flame shield on a gryphon rider | Nothing; no mana spent (air unit) |
| S08 | Slow on a unit with speed timer +300 (hasted) | −700 |
| S09 | Slow on a unit with speed timer −200 | −1000 |
| S10 | Haste on a unit with speed timer −300 (slowed) | +700 |
| S11 | Haste on a unit with speed timer +400 | +1000 |
| S12 | Unholy armour on a grunt at 61 HP | 30 HP; unholy timer 500 |
| S13 | Unholy armour on a unit at 1 HP | stays 1 HP; timer 500 |
| S14 | Invisibility on a goblin sapper | It is killed in that cycle's timers pass |
| S15 | Runes, mana 200, target tile free, the north and east tiles hold units | 3 runes (centre, south, west); refund 80; mana 80 |
| S16 | Runes, all 5 tiles free but only 2 rune slots free | 2 runes (centre and north); refund 120 |
| S17 | Footman (60 HP) walks onto a rune | Rune used up; footman at 10 HP |
| S18 | Footman at 50 HP walks onto a rune | Footman killed |
| S19 | Unholy-armoured knight walks onto a rune | Rune used up; no damage |
| S20 | Raise dead, mana 120, three unused bodies at squared distances 4, 25 and 49 | Two skeletons, from the two nearer bodies (in slot order); mana 20; the third body is too far |
| S21 | Raise dead, mana 60, one body, but the caster's player is at its unit cap | No skeleton; the body stays unused; mana 10 |
| S22 | Death coil, candidates with 10, 30 and 25 HP | Order 10, 25, 30. Shares 10, 25, 15; the caster gains 50 HP (capped at its maximum) |
| S23 | Death coil, candidates with 60 and 5 HP | Order 5, 60. Shares 5 and 45; caster gains 50 |
| S24 | Death coil, no candidates | 100 mana spent; nothing else |
| S25 | Blizzard, caster with 60 mana, channelled | Two strikes (60 → 35 → 10); at the third strike 10 < 25, so the caster becomes idle. 10 chains, 110 shards in all |
| S26 | One blizzard shard lands 10 px from a footman | 5 to 10 damage (armour ignored) |
| S27 | Fireball cast at a point 200 px east of the caster's centre | Steps of 12 px until the remaining distance drops below 50 (13 steps: 200 → 44). Then 6-px steps; counts 1, 2, … from that step. Explosions on counts 8, 16, 24, 32 and 40 (steps 20, 28, 36, 44 and 52), at remaining distances 2, −46, −94, −142 and −190 (east of the point by that many px). Count 41 on step 53; step 54 still moves 6 px and marks it as ending; removed at the next update with no draw: 54 step draws |
| S28 | Eye of Kilrogg | Created next to the caster. Killed on its 828th timers pass |
| S29 | Skeleton raised | Killed on its 26400th timers pass |
| S30 | Caster at 254 mana | 255 after its counter next reaches 0; then no more gains |
| S31 | A mage is trained | It starts with 85 mana; 86 on its 40th timers pass |
| S32 | Holy vision, Enhanced, 64 × 64 map, target tile (10, 60) | Points (10, 60), (10, 52), (16, 56) and (4, 56) are revealed with sight 9 and get a cosmetic effect; (10, 68), (16, 64) and (4, 64) are skipped. The fog refresh countdown becomes 150 |
| S33 | Same, Classic | The same 4 areas are revealed. (16, 64) and (4, 64) reveal nothing but each gets a cosmetic effect (6 effects in all); (10, 68) is skipped. The refresh countdown is unchanged |
| S34 | Holy vision at tile (3, 30), either mode, large map | (−3, 34) and (−3, 26) are skipped; the other 5 points are used |
| S35 | Whirlwind at tile (10, 20); strike draws give 100 then 10 | Goal (336, 656); start (372, 602) (x + 36, y − 54). One 1-px step every 3 cycles; it splashes on steps 2, 4, 6, … (before moving). It reaches the goal on step 54 (after that step's splash), and then draws twice for a new goal: draws 64 then 0 give the goal (336, 592) |
| S36 | Same whirlwind, no units near it | No draws between the strike and step 54; 27 splashes by then; lifetime 746 after step 54 |
| S37 | Flame shield on a footman: the 5 flames at creation | Offsets from the footman's centre: (1, −34), (30, −20), (19, 8), (−11, 10), (−27, −16) (points 0, 10, 20, 30, 40) |
| S38 | Same, the flame that started at point 10, at its 5th step | Point 15: offset (31, −5) |
| S39 | Same, the flame that started at point 40, at its 11th step | Point 0 again (wraps after point 50): offset (1, −34) |
| S40 | Same, the first flame (lifetime 512) | Splashes on its steps 15, 31, 47, … , 511 (32 splashes); ends at step 512 without moving, staying where step 511 left it |
| S41 | Same, the second flame (lifetime 524) | Splashes on its steps 11, 27, … , 523 (33 splashes); ends at step 524 |
| S42 | Death coil, candidates in scan order A (30 HP), B (30 HP), C (5 HP) | Order C, B, A. Shares C 5, B 30, A 15; the caster gains 50 |
| S43 | Polymorph on a footman | The critter makes 2 draws (facing, then first wait). The footman plays its death animation and leaves a body with the used mark; a later raise dead skips it |
| S44 | Raise dead with 120 mana over two unused bodies | 2 draws (one per skeleton), each body gets the used mark and still decays normally |
| S45 | Blizzard strike | 20 draws at the strike, before any shard flies |
| S46 | Fireball cast at cycle C | First update at C; first step and first draw at C + 1 |
| S47 | A spell puts a spell marker (0x16) on a unit in cycle C | First update in C; start step and showing from C + 1; removed in C + 20 |
| S48 | `xorc01-death-knight-2100` (oracle trace), n = 37: haste on ogre 1488 (type 0x07, box 42 × 42) at pixel (2848, 2944) | Effect kind 0x16 at (2869, 2961), destination (0, 0), target null, shooter null, damage 0 |
| S49 | `xorc12-ogre-mage-180` (oracle trace), n = 36: bloodlust on ogre 1547 (type 0x07) at pixel (3712, 288) | Effect kind 0x16 at (3733, 305), destination (0, 0), target null, shooter null |
| S50 | `xorc12-ogre-mage-180` (oracle trace), n = 77: runes are cast; five rune markers (kind 0x0B) are created | Effect slots 3 to 7 at (3600, 432), (3600, 400), (3632, 432), (3600, 464) and (3568, 432); each with destination (0, 0), target null, shooter null, damage 0 |
| S51 | Runes cast: a rune marker (0x0B) made in the timers pass of cycle C | First update in C + 1; start step in C + 3; removed in C + 19 |
| S52 | Map start, oracle trace Human01: the building fires in effect slots 0 and 1 | First update in the first start-up cycle; start step 5 cycles later, in the 4th normal cycle; never removed |
| S53 | Custom unit data gives a farm the caster flag and magic 0; the farm is placed by the map | Mana 0 until its 256th timers pass, then 1; +1 every 40 passes after that, up to 255 |
| S54 | Default data | No building gains mana from the timers pass |
| S55 | `xhuman10-blizzard-600` (oracle trace), n = 145: new shards in effect slots 4, 5 and 7, waiting | Destinations (2480, 2864), (2512, 2928) and (2576, 2800), each chain's landing point, unchanged through n = 160; they start moving at n = 157, 12 cycles after creation |
| S56 | `xorc01-death-coil-2500` (oracle trace), n = 1191: death knight 1491's coil launches effect 3 (kind 0x0A, victim 1521, share 40) and effect 4 (victim 1514, share 10) from (2320, 2224) | Destinations (2642, 2157) and (2704, 2223) from creation; the caster's target is 1514 until the idle takes over at n = 1220 |
| S57 | Same: effect 3, D = 322 | 27 steps: lands at n = 1218 at (2644, 2152) and stays; removed at n = 1234, its impact 16 cycles after landing |
| S58 | Same: effect 4, D = 384 | 33 steps: lands at n = 1224 at x = 2716 |
| S59 | `xorc01-death-knight-2100` (oracle trace): whirlwind cast on tile (80, 68) at n = 1300 | n = 1318: effect 3 (kind 12) at (2614, 2171), destination (2576, 2192) |
| S60 | A whirlwind after a new goal (x, y) is picked | Destination (x, y) |
| S61 | `xorc12-spells-1500` (oracle trace, Enhanced), n = 1166: whirlwind effect 3 (kind 12, created at n = 327 with destination (3248, 1008)), cast by Teron Gorefiend 1583 (type 0x15, owner 5), who died at n = 633; his slot is released in this cycle | Shooter 1583, unchanged; the whirlwind goes on |
| S62 | A whirlwind whose caster's slot was released and then reused by a new unit U splashes a footman (static study) | The footman's recorded attacker is U |
| S63 | The same, with U owned by the footman's owner (static study) | The footman takes the damage; no attacker is recorded, no call for help, no score |
| S64 | `orc14-25000` (oracle trace, Enhanced), n = 18992: paladin 1433 (type 0x0C, owner 6) at (8, 109) in order 39 (healing) on paladin 1429 (owner 6), which moved to (15, 114) at n = 18990, 7 tiles away (healing's radius is 6) | The strike heals: 1429 goes from 85 to 87 HP, 1433's mana from 16 to 4, and a healing effect (kind 9) appears at 1429 |
| S65 | `xorc12-reach-400` (oracle trace, Enhanced): Dentarg 1548 (ogre-mage) at (114, 9) is ordered to bloodlust ogre 1547, which is walking west (order 3); the cast (order 49) starts at n = 131 with 1547 at (108, 11), 6 tiles away (bloodlust's radius is 6); 1547 reaches (107, 11), 7 tiles away, at n = 140 | At the strike, n = 141, 1547 gets bloodlust (timer 999) and 1548's mana drops from 255 to 205 |
| S66 | A mage casts slow; its target walks 12 tiles away during the casting animation (static study) | Slow still takes effect at the strike |
| S67 | `xorc01-death-knight-2100`, n = 1569: death and decay on tile (69, 68) | Every cloud (kind 6, slots 4 to 8, at (2224, 2160), (2256, 2192) and others) has destination (2224, 2192) |
| S68 | `xorc01-death-knight-2100`, n = 1920: raise dead makes skeleton 1526 (type 0x37) at pixel (2208, 2144) | Effect 4, kind 0x16, at (2223, 2155) (the skeleton's centre would be (2224, 2160)) |
| S69 | `xhuman10-blizzard-600`, n = 170: the shards in slots 7 and 8 step to (2585, 2809) and (2523, 2841), past their landing points (2576, 2800) and (2512, 2832) | They land in that update: the chains' next shards appear in slots 12 and 13 (destinations (2576, 2800) and (2512, 2832)), before slot 10's missile's impact effect (slot 14, kind 0x1A) |
| S70 | `xhuman10-slow-3000` (oracle trace), n = 302: Slow from mage 1481 (type 0x0A, owner 1) on grunt 1490, dead since n = 170 and a body (type 0x69) since n = 277 | n = 302: 1481 in order 2, target null; n = 303: wait 3 (the pending idle took over) |
| S71 | `xhuman10-slow-3000`, n = 740: the flame in effect slot 4 (shooter 1480) at (2676, 2862) on its last step | Unmoved at (2676, 2862); slot 4 free at n = 741 |
| S72 | Same, n = 174 and 175 | Slots 7 and 8 unmoved at n = 174; free at n = 175 |
| S73 | `xhuman10-spells-700` (oracle trace), n = 329: mage 1481 (type 0x0A) casts fireball (order 43) at catapult 1487 on tile (74, 89) | Effect 3: kind 2 at (2704, 2832), destination (2384, 2864), shooter 1481, damage 40 |
| S74 | `xhuman10-blizzard-600` (oracle trace), n = 417: mage 1481 casts flame shield (order 42) on knight 1480 at pixel (2592, 2880) | Effect 9: kind 3 at (2704, 2832), destination (2608, 2896), target 1480, shooter 1481 |
| S75 | `xhuman10-spells-700`, n = 393: the fireball in effect slot 3 (kind 2, shooter 1481, damage 40, destination (2384, 2864)) | Still in use at (2182, 2880); free at n = 394, with no draw for it |
| S76 | `xhuman10-spells-700`, n = 619: healing on knight 1489 | Effect slot 3: kind 9 at (2421, 2897), destination (0, 0), target and shooter null, damage 0 |
| S77 | Same effect | Unmoved through n = 637; one step to (2409, 2885) at n = 638 (619 + 19); free at n = 639 |
| S78 | `xhuman10-blizzard-600`, n = 426: the flame shield bolt in effect slot 9 | Still in use at (2608, 2892), as at n = 425; free at n = 427 |
| S79 | Same, n = 426: the flames (slots 11 and 20 to 23) | Kind 4, shooter 1480, target null, destination (0, 0), damage 4 |
| S80 | Same, the flame in slot 11 | At (2609, 2862) from n = 426 to 431; (2612, 2862) at n = 432 |
| S81 | `human14-10000` (oracle trace), n = 3740: death knight 1577 (type 0x0B, owner 0) in order 52 finds no route | Order 2, animation 2, wait 1; 5 random draws in that cycle, the idle look-around among them |
| S82 | `human13-invisible-work-hits-900` (oracle trace, Enhanced): peasant 1402 (type 0x02), made invisible by mage 1415 and chopping (order 23, animation 4) | Invisibility timer 1999 from n = 94, 1984 at n = 109 (frame 35); 0 at n = 112, the chop strike (frame 40) |
| S83 | Same trace: peasant 1401, made invisible at n = 238, repairs farm 1413 (order 27) | The repair animation starts at n = 436 and the farm goes from 389 to 393 HP while the timer still runs (1792 at n = 445); 0 at n = 446, the first repair strike (frame 40) |
| S84 | `xhuman10-failcast-520` (oracle trace, Enhanced): mage 1481 (type 0x0A) channels blizzard (order 47) at tile (95, 94) from n = 23, striking at n = 37, 77, … , 397; Khadgar 1462 makes it invisible at n = 417 (timer 1999); the 11th strike comes at n = 437 (frame 35) with mana 15 | Mana stays 15 (25 needed: nothing paid, no shards); the invisibility timer goes from 1980 (n = 436) to 0 (n = 437). Order 47 and the casting animation go on until n = 466, when the pending idle takes over (order 2, animation 2, wait 3) |
| S85 | `xorc12-spells-hits-1150-1260` (oracle trace, Enhanced, `EXTRAS=hits`), n = 1213: footman 1492 (type 0x00, owner 2, 60 HP, attacking 1547) steps onto the rune at tile (112, 13) | 10 HP; an explosion (kind 0x17) in effect slot 8 at (3600, 432), destination (0, 0), no target or shooter, damage 0; no `hits` entry in the record (the damage step isn't called) |
| S86 | An invisible mage casts slow; its target dies during the casting animation (static study) | At the strike the invisibility timer becomes 0; the target check fails, nothing is paid, and the mage becomes idle |
| S87 | A mage of player 1 slows a grunt of player 2 (static study) | At the strike point, before the speed timer changes, a sight-2 area around the mage is revealed for player 2 |
| S88 | A footman at 40 HP of player 2 walks onto a rune placed by player 5 (static study) | The footman is killed outright: no notification, and player 5 gets no kill and no score |
| S89 | An invisible ogre-mage casts eye of Kilrogg with 60 mana (static study) | At the strike the invisibility timer becomes 0; 60 < 70, so nothing is paid, no eye is made, and the caster becomes idle |
| S90 | `xhuman10-slow-killed-goals-400`: mage 1481 (type 0x0A) at (96, 90) ordered to slow grunt 1500 of player 2, at (79, 86) at the command, 17 tiles west; the grunt reaches order 1 at n = 86 while the mage walks (round 81) | n = 98, the next step boundary, at (2944, 2816): order 2, animation 2, wait 3, target null, nothing pending, mana 255. Waits 3, 2, 1, then the idle logic runs at n = 101 |
| S91 | `xhuman10-heal-ballista-goals-500`: Turalyon 1466 (type 0x2C) at (105, 97) ordered to heal its own ballista 1483 (not organic) at (88, 90) (round 81) | Order 39 from n = 25; walks from n = 28; n = 160 at tile (94, 91), within radius 6: order 2, animation 2, wait 3, target null, mana 255 (nothing paid) |
| S92 | `xhuman10-invisible-reach-goals-400`: mage 1481 ordered to slow critter 1411 (owner 15) 3 tiles away while visible; Khadgar 1462 makes the critter invisible at n = 287 (round 81) | The slow's decision at n = 299 casts; strike at n = 310: mana 255 to 205, the critter's speed timer −999 |
| S93 | `xhuman10-invisible-dwarves-150`: mage 1481 makes demolition squad 1494 (type 0x0E) invisible; strike at n = 37 (round 81) | In record n = 37: squad in order 1, animation 1, invisibility 1999 (counted in the pass that killed it); order 0 at n = 53 |
| S94 | `xhuman10-shield-mine-hits-800`: mage 1481 shields peasant 1458; flames (kind 4) in effect slots 5 to 9 from n = 108; the peasant enters gold mine 1460 at n = 204 (round 81) | n = 204 to 353: the 5 flames go on, circling the peasant's stored position (3232, 3136) (x 3220 to 3280, y 3118 to 3164) |
| S95 | Same trace, `hits` (round 81) | Flame splashes credited to 1458 hit mine 1460 at n = 214, 220, 240, …, 351 (14 hits; their `hp` fields, the HP before each hit, read 25499 to 25475) while the peasant is inside the mine, and town hall 1474 at n = 544, 592, 607 and 617 (4 hits) while it is inside the hall. The hits on the mine at n = 188 and on the hall at n = 470 come just before each entry, from outside (ordinary splash) |
| S96 | Same trace (round 81) | The flames end at the end of their lifetimes, not at either entry: they leave the records from n = 626 (slot 5) to n = 641 (slot 9) |
| S97 | `xhuman10-shield-self-200`: mage 1481 (85 mana) ordered to cast flame shield on itself, cheat on (round 81) | Nothing: mana 85 (no top-up), order 2, no bolt |
| S98 | Blizzard with exactly 2 effect slots free (static study) (round 81) | 8 draws at the strike (chains 1 and 2, 4 each); chains 3 to 5 start nothing and draw nothing |
| S99 | Holy vision at the map centre; then at tile (0, 0) (static study) (round 81) | 7 placed sounds, one per point; at (0, 0), 3 (T, T + (0, 8), T + (6, 4)) |
| S100 | An ogre-mage casts eye of Kilrogg with 100 mana when the free-tile search finds no tile for the eye, an air unit (every air-layer tile in the search occupied; ground units around the caster don't block it) (static study, constructed) (round 81) | 70 paid, no eye, the blocked-exit message, no sound |
| S101 | `xhuman2-xhuman02-cheat-scope-1600` (oracle trace, Enhanced; round 83): all-spells cheat on; computer death knight 1557 (player 5) is given death coil by its spell step at the pass of n = 751 with 103 mana | Mana 255 in that record; 155 after the strike (n = 896); the same at the passes of n = 901 to 1051. It never casts a spell its owner didn't know |

## Open questions

- Confirm the death coil order of equal-HP candidates in the running game: two 30-HP victims and one 5-HP victim
  should split the budget as the vector says (the second-found 30-HP victim gets its full 30).
- Units created while the map loads don't get the decay countdown. What happens to a decaying type placed in the map
  (for example a skeleton in a PUD) is not settled. Default: it never decays.
- A unit that would be killed by decay is spared in one case: a global setting is on and the unit carries a state
  flag. Neither has been identified. Default: never spared.
- Whether a used dead body is drawn: see `core/corpses.md` (default: drawn like any body).
- The inactive unit state skipped by the timers pass besides dying and remains. Default: the state that the unit
  update also skips.
- **A released shooter slot that isn't reused:** its left-over values are read as the attacker; whether a release
  clears them isn't checked (static study; the trace format can't show the attacker record).
- **The strike's missing distance test for spells other than healing and bloodlust** rests on static study. In
  `xorc12-reach-400`, haste and unholy armour are cast only after their targets stop, so they don't test it.
- Whether the raise-dead scan can pick up a body in a slot that has already been released (the scan reads the body
  type and remains state only). Default: no; a released slot never qualifies.
- **Organic-target spells on a building** (round 80). "Walking and the decision" says these spells need the organic
  flag "also", that is besides the unit spells' not-a-building test; static study of the decision finds only the
  organic test. No building in the default data has the organic flag, so the two readings differ only with custom unit
  data. **Default:** as stated (both tests). **Trace request:** none cheap (it needs a custom map with an organic
  building).
- **Food of the eye of Kilrogg.** Held in `economy/build-train.md` ("Food of summoned units"). **Default:** it counts.
  **Trace request:** cast eyes at a player's food cap and try to train a unit.

### Answered in round 80

- *Invisibility at a failed cast.* A cast that fails at the strike ends the caster's invisibility: the strike point
  sets the timer to 0 before the spell's strike runs (traced: `xhuman10-failcast-520`, S84; static study agrees).
  The stated step 5 of the old "At the strike" is gone.
- *The rune damage path* (`combat/damage.md` and this spec disagreed). A rune's hit is a direct HP change, not a
  delivery to "Applying damage" (traced: `xorc12-spells-hits-1150-1260`, S85; static study for the kill and the missing
  credit). `combat/damage.md` is corrected to match.

### Answered in round 81

- *Blizzard shard landing at exactly 0.* It can't happen: every shard starts 159, 170 or 181 px from its landing point
  and steps 12 px, and none is a multiple of 12 ("Blizzard shards"; static study and arithmetic).
- *Eye of Kilrogg's blocked-exit message.* The stated rule stands: the message is set only when no eye is made. What
  the round-80 reading took for a message on an eye's creation is the spell's sound, which plays only when an eye is
  made ("The spells"; static study, `ERRATA-round81.md`, audit erratum 9).
