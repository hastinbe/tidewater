# Combat: damage, armour, attack timing and missiles

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-11; round-78 consolidation reviewed by a
  spec-review session, 2026-10-16; round 79 and the B-040 combat audit reviewed by a spec-review session,
  2026-10-16). The text before the consolidation is at tidewater-docs `b19dbc8`; the changes are
  listed in `specs/ERRATA-combat-consolidation.md`.
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`), unit statistics from
  `unitdata.dat` and the PUD `UDTA` layout (`pudspec.txt`), attack timing measured from `Rez\script.bin`, and Enhanced
  oracle traces named at each rule and vector. Each subsection ends with an **Evidence** line: *traced* (an oracle
  trace, named), *static study* (the program, not yet seen running), *game data* (the shipped data files) or
  *observed* (the running game watched by a person).
- **Rules mode:** every rule here is Enhanced, from Enhanced traces or static study; none is labelled Classic mode.
  The parts of combat that differ between the rule sets live in other specs and are labelled there: which units a
  player detects (`core/timing-and-rng.md`; it decides what a killer forgets), the shared-vision set of the strike
  reveal (`vision/fog.md`), the number of effect slots (`core/unit-slots.md`; it decides when a shot finds no free
  slot) and Classic attack-move (`orders/orders.md`).
- **Changes:** 2026-10-11, round-2 errata: wall damage advances one stage per counter overflow; target-acquisition
  distance measure added; demolition blast terrain effects now in `movement/occupancy.md`.
  2026-10-11, round-3 errata: the 7 × 7 splash scan order is row by row from the top, left to right, with a tile's
  ground-layer unit before its air-layer unit (open question closed).
  2026-10-11, round-8 errata: new subsection "Missile flight" (speed, flight length, landing, impact delay and
  draws per kind, the computer players' wall exception, and the gryphon hammer and dragon breath, which splash along
  their path); two open questions closed.
  2026-10-12, round-10 errata: a target with the used mark gets no notification and no damage; the notification
  points to `ai/attack-ai.md`, "Call for help", and the under-attack alert is no longer described as part of it.
  2026-10-12, round-14 errata: the Dark Portal special case is defined (game kind, campaign entries, types).
  2026-10-12, round-15 errata: new paragraph "Impact effects" (the effect each missile kind leaves at its impact, its
  slot and its lifetime).
  2026-10-12, round-19 errata: cannon smoke is made only by destroyers, battleships and juggernaughts (never towers),
  at the missile's start point, in the lowest free effect slot after the missile's; a shot with no free effect slot
  creates nothing and draws nothing; cannon towers, like guard towers, fire without aim scatter.
- **Changes:** 2026-10-12, round-20 errata: two test vectors renumbered so effect slots 0 to 2 stay in use
  (`core/unit-slots.md`).
- **Changes:** 2026-10-12, round-23 errata: the Dark Portal special case names its three maps, and the two other
  shipped maps with a Dark Portal are noted as unprotected; four test vectors added.
- **Changes:** 2026-10-12, round-24 errata: the cannon smoke starts 11 cycles after its first update and is removed in
  C + 24; an impact effect's slot time and its sound count from its start step, one cycle after its first update.
  2026-10-12, round-37 errata: a dying unit keeps its HP and target, and the killer forgets its target at once; an
  attack cooldown (attack loop + 1) exists and delays the first strike after a target switch; towers fire once per loop
  from their order logic.
  2026-10-12, round-39 errata: impact effects are created with only kind and position set (destination (0, 0), no
  target, no shooter, damage 0); a hammer or breath splash needs a free effect slot (none: no splash, sound or effect)
  and creates a kind 0x17 explosion there; `orc09-repair` and `xorc10` vectors.
  2026-10-12, round-40 errata: a building destroyed by a missile makes its explosion before the missile's impact effect.
  2026-10-12, round-41 errata: a melee strike at a target that started dying earlier in the same cycle is still made and
  rolls damage; an order-12 switch to a target in reach while cooling down keeps the cooldown wait (no wait of 3); open
  question on ranged attackers.
  2026-10-12, round-42 errata: only the killer forgets its victim as its attacker; a ranged attacker launches at a dying
  target (traced, `xhuman4` n = 121; the open question is closed); warships turn broadside to strike (`core/animation-
  scripts.md`).
  2026-10-13, round-45 errata: in orders 12, 15 and 16 a unit whose target is lost while it is cooling down keeps its
  order until the cooldown runs out.
  2026-10-13, round-47 errata: after any kill, including a splash kill of a unit other than the target, the killer
  forgets its own target and attacker if they are dying, gone or not detected; `xhuman10-blizzard-600` vector.
  2026-10-13, round-48 errata: a killing building keeps its target and attacker records (static study); a building
  killed during a job has the job cancelled at the kill.
  2026-10-13, round-50 errata: a unit keeps through its death animation the target recorded when it dies, so a killer
  that forgot its target dies with none; `xhuman12-10000` and `xhuman8-10000` vectors.
  2026-10-13, round-53 errata: a hammer or breath splash takes its effect slot before the splash's damage, so a killed
  unit's explosion comes in a later slot; the strike point first makes the strike reveal of `vision/fog.md`;
  `xorc8-10000` vector.
  2026-10-13, round-54 errata: the killer forget runs on a missile's shooter whatever its state, so a dead shooter whose
  missile kills forgets its target; `human13-subs-2000` vector.
  2026-10-13, round-56 errata: no strike while a command given to the attacker is waiting to take over;
  `orc14-25000` vector.
  2026-10-13, round-59 errata: the invulnerability cheat keeps a human player's basic damage (piercing 200 set
  after upgrades, not doubled by bloodlust); a missile's minimum flight value v skips acquisition candidates at
  d < v − 1; `human2-human02-all-out-4000` vectors.
  2026-10-13, round-61 errata: a hammer or breath keeps its effect slot through its final hit and the effects that
  hit makes, so a death explosion never reuses its slot; `xorc11-16000` vector.
- **Changes:** 2026-10-16 (round 76, B-038 build/train consolidation, `ERRATA-buildtrain-consolidation.md`): the
  job-cancel reference points at "A building killed" in `economy/build-train.md` (section renamed; no rule change).
- **Changes:** 2026-10-16 (round 78, B-038 combat consolidation, `ERRATA-combat-consolidation.md`, reviewed): the
  combat rules are one model ("The model"), with the attack cycle before the four deliveries and one apply-damage and
  kill step; vectors get stable IDs V01 to V85 (text unchanged) and V86 to V92 are added. **One change of meaning
  (C1, traced):** haste doesn't change the attack cooldown, so a hasted unit strikes every attack loop + 1 cycles, not
  every hasted loop (`xorc01-death-knight-2100`, V86, V87). Static statements made explicit: a melee strike with no
  target unit damages the wall at its target tile; no range test at the strike point; tower shots; slow and the
  cooldown; kill credit. Six old open questions answered, two new ones (a melee strike at a target that is gone or
  inside; kill credit for an ally's kill).
- **Changes:** 2026-10-16 (round 79 traces, `ERRATA-round79.md`, reviewed by a spec-review session, 2026-10-16): a
  melee strike or a launch at a target that has gone inside since the decision is still made, with its draws, and
  "Applying damage" does nothing (V93, V94); a small-cannon splash at an air unit considers the impact tile's ground
  unit first and the air unit in its tile's scan place (V95); an ally's kill is credited (V92 traced, V96). Both
  round-78 open questions answered for what was traced; the released-slot melee case (V16) keeps the stated rule and
  becomes the open question "A melee strike at a target whose slot is released".
- **Changes:** 2026-10-16 (B-040 combat audit, `ERRATA-audit-combat.md`, static study; reviewed by a spec-review
  session, 2026-10-16): the attacker's invisibility ends at every attack strike point, a target inside included
  (erratum 4); the hammer and breath splash order with an air unit on the impact tile (erratum 2) and its damage kept
  to the low 8 bits (erratum 5, V97); a missile's shooter is its slot for the attacker record, the killer's check and
  the splash's self-exclusion (erratum 3, V98). Held: the target-slot part of erratum 3 (new open question "A missile
  whose target's slot is reused in flight") and the released-slot part of erratum 1.

## Behaviour

All quantities below are integers. "Draw" means one output `r` (0 to 32767) of the simulation generator in
`specs/core/timing-and-rng.md`. Unit statistics are the per-type `UDTA` values: basic damage, piercing damage, armour,
attack range, weapons upgradable, armour upgradable, missile weapon, and flags.

### The model

Every attack, by a unit, a tower or a demolisher, goes through the same chain:

1. **Attack values.** The attacker's basic and piercing damage and the target's armour, from the unit data and the
   owners' upgrades ("Effective attack values"), give either a rolled damage ("Damage roll") or the attacker's maximum
   damage (for splash missiles).
2. **The attack cycle.** The order's logic decides to strike: it runs the cooldown test, the target checks and the
   range test, then turns the unit and starts its cooldown (`orders/orders.md`, "Attack orders"; here, "Attack timing
   (cooldown)"). The strike itself happens later, at the **strike point** of the attack script ("How an attack
   delivers damage"). Towers are the exception: they decide and shoot in one logic run ("Towers").
3. **One of four deliveries** carries the damage to its target:
   - a **melee blow**, applied at once at the strike point;
   - a **missile** that carries rolled damage to one unit (or a wall) at its impact ("Missile flight");
   - a **splash** at a splash missile's impact, or along a gryphon hammer's or dragon breath's path, that rolls damage
     for each unit near the point ("Splash on impact");
   - a **demolition blast**, a flat 200 (twice) to the 8 neighbours (`combat/attack-ground-demolish.md`, "The blast").
4. **Applying damage.** Every delivery, and the spell damage of `spells/spells.md`, ends in the same step: checks,
   the notification, then either HP lowered or a **kill** ("Applying damage", "A kill").
5. **After the kill.** The victim's death (order, sound, explosion, units inside, slot) is in `core/unit-slots.md`,
   "Death, corpses and release"; what it leaves behind is in `core/corpses.md`; its score in `game/campaign-flow.md`,
   "Mission score"; what the player sees and hears is gathered in "What the player sees and hears" below.

The combat rules are in this small set of specs:

| Spec | What it holds |
| --- | --- |
| `combat/damage.md` (this spec) | The model; attack values; the attack cycle and cooldown; strikes, missiles and splash; applying damage and the kill; towers; reach and range |
| `combat/attack-ground-demolish.md` | Orders 17 and 18 (attack ground) and 20 and 21 (demolish), and the blast |
| `core/corpses.md` | Corpses, wrecks and ruins |
| `orders/orders.md` | The attack orders' step table, order 11, orders 14 to 16, friendly fire, losing the target and **target acquisition** (not repeated here) |
| `core/unit-slots.md` | The victim's state at and after its death, and slot release |

### Effective attack values

An attacker's **basic damage** is its type's basic damage, plus an upgrade bonus if the type's "weapons upgradable"
byte is set. The bonus is chosen by the first matching row:

| Attacker | Basic bonus |
| --- | --- |
| Sea unit (flag bit 3) | owner's ship-cannon level × 5 |
| Unit with flag bit 2 (ballista, catapult) | owner's siege level × 15 |
| Archer, axe thrower, ranger, berserker | none |
| Any other unit | owner's sword/axe level × 2 |

**Piercing damage** is the type's piercing damage plus:

- for archers, axe throwers, rangers and berserkers: the owner's arrow/spear level × 1;
- for a ranger whose owner has researched Marksmanship: the ranger's basic damage (3) moves into piercing. Basic becomes
  0 and piercing goes up by 3, so armour no longer reduces that part.

**Bloodlust:** while an attacker's bloodlust timer is running, both basic and piercing damage are doubled, after
upgrades.

**Cheat:** with the "invulnerability and extra damage" cheat on, a unit of a **human-controlled** player keeps its
**basic** damage, worked out as usual (upgrades, then the bloodlust doubling), and its **piercing** damage is a flat
**200**. The 200 is set after the upgrades and isn't doubled by bloodlust. Units of every other player (computer,
passive computer, rescue, neutral) deal basic 0 and piercing 0, so they do no damage at all (traced:
`human2-human02-all-out-4000`, round-59 vectors).

A target's **armour** is its type's armour, plus an upgrade bonus if the type's "armour upgradable" byte is set:

- sea units (flag bit 3): the target owner's ship-armour level × 5;
- other units: the target owner's shield level × 2.

Upgrade levels are counted per player as 0, 1 or 2.

The **maximum damage** of an attacker is its basic plus piercing damage as above (upgrades, bloodlust, the cheat),
capped at 255. Splash missiles carry it ("How an attack delivers damage").

**Evidence:** game data (the unit statistics and upgrade amounts); static study (the bonus rows, bloodlust); traced for
the cheat (`human2-human02-all-out-4000`, V78). The maximum-damage sentence restates the splash-missile rule below.

### Damage roll

This is used for a melee hit and for a non-splash missile. Let B be the attacker's basic damage, P its piercing damage
and A the target's armour.

1. `b = max(0, B − A)`. If there is no target (a missile aimed at the ground), A is taken as 0.
2. `T = b + P`. If `T = 0` the damage is 0 and **no draw is made**.
3. `h = floor((T + 1) / 2)`, that is, half of T rounded up.
4. Make one draw r. The damage is `min(255, h + (r mod (h + 1)))`.

So the damage is spread evenly from roughly half of T up to T (`h` to `2h`). There is no minimum of 1: a unit with no
piercing damage hitting armour equal to or above its basic damage does 0.

**Evidence:** static study; traced: the stored damage of `xhuman4-7000` (V17) and the draw and result of
`human2-human02-all-out-4000` (V78).

### Attack timing (cooldown)

Each unit has an attack **cooldown**, a count of cycles. When a unit decides to strike, it turns to face its target (the
facing rule of `core/animation-scripts.md`; warships turn broadside instead, by the table there) and its cooldown is set
to its type's **attack loop + 1** (the loop column of the table below): 26 for footmen and the other melee units, 66 for
archers and axe throwers, 41 for mages and death knights, 201 for ballistas and catapults, and so on. The cooldown
counts down by 1 every cycle while it is above 0, for every unit except transports. Buildings have none.

Each time the logic of an attack order (9, 11, 12, 15, 16 or 17) reaches the strike step of a unit that isn't a flyer:
if the unit's current wait is **less than** the cooldown left, its attack animation restarts from its first frame, the
unit waits for the cooldown left, and the run ends. Otherwise the order goes on (range checks, friendly fire and so on).
When the logic runs at a step done, the wait is 1 at that moment. So in steady fire nothing is delayed: the next strike
decision comes one loop later with 1 cycle left, and a wait of 1 isn't less than 1. After a target switch, though, the
new attack starts with most of a cooldown still to run: the run that acquires the new target reaches the strike decision
and starts the cooldown, the order given again restarts the animation with wait 3, and 3 cycles later the unit waits for
the rest. A unit in order 12 whose search switches to a new target in reach while still cooling down gets no such wait
of 3: the new target takes effect at once and the cooldown test restarts the animation with the cooldown left as its
wait (`orders/orders.md`, "Resulting order"). In orders 12, 15 and 16 the cooldown test comes **before** the target
check, so a unit whose target dies or is lost while it is still cooling down keeps its order, with no target, until the
cooldown runs out (`orders/orders.md`, "Losing the target while cooling down"; traced: `xorc01-death-knight-2100`).

**Haste and slow don't touch the cooldown.** The value set is always the type's unhasted attack loop + 1, and it
counts down by 1 every cycle whatever the unit's speed timer. Haste and slow change only the attack script's waits
(below), so:

- **A hasted unit that isn't a flyer** plays its attack script faster than its cooldown runs out. Each time the
  shortened loop ends, the cooldown test finds cooldown left and restarts the animation with that as its wait. So a
  hasted unit strikes every
  **attack loop + 1** cycles, one cycle **slower** than the same unit unhasted in steady fire (every attack loop).
  What haste does gain is an earlier strike point after each decision (the "Strike" cycle of the hasted script). For a
  hasted ogre (loop 25, cooldown 26, hasted loop 11): decision, strike 3 cycles later, the cooldown test 11 cycles
  after the decision restarts the animation with wait 15, and the next decision comes 26 cycles after the first
  (traced: `xorc01-death-knight-2100`, V86, V87).
- **A slowed unit's** attack loop is longer than its cooldown for every type (`core/animation-scripts.md`, "Measured
  timings", the slow column), so the cooldown has run out at each decision and its rate of fire is its slowed loop
  (static study and game data).
- **Flyers** skip the cooldown test, so a hasted or slowed flyer (gryphon rider, dragon, daemon) strikes every hasted
  or slowed loop (static study).

The **rate of fire** of a unit that isn't hasted, and of any flyer, is therefore the length of the attack script loop
in `Rez\script.bin` (a slowed or hasted loop when the speed timer runs), and that of a hasted unit that isn't a flyer
is its cooldown. Damage happens at the script's strike point inside that loop. A unit that keeps attacking repeats the
loop. Between loops it may re-evaluate its target and order. Each script's "wait" steps scale with haste and slow:

- Haste halves the wait, rounding down, with a minimum of 1.
- Slow doubles the wait, kept to 8 bits.

The timings below are measured from `script.bin`. "Strike" is the cycle on which damage or launch happens, counting
the first cycle of the attack as 1. "Loop" is the number of cycles between strikes at normal speed (no haste or slow).

| Unit types | Strike | Loop |
| --- | --- | --- |
| Footman, grunt, peasant, peon, knight, paladin, attack peasant or peon, Danath, Khorgath, Grom, Turalyon, Lothar, Uther | 11 | 25 |
| Ogre, ogre-mage, Dentarg, Cho'gall | 8 | 25 |
| Dwarven demolition squad, goblin sapper | 5 | 25 |
| Archer, ranger, Alleria | 12 | 65 |
| Axe thrower, berserker, Zuljin | 11 | 65 |
| Mage, death knight, Teron Gorefiend, Khadgar, Gul'dan | 12 | 40 |
| Ballista | 27 | 200 |
| Catapult | 2 | 200 |
| Oil tanker (either side) | 2 | 130 |
| Destroyer, transport (either side) | 2 | 120 |
| Battleship, juggernaught | 2 | 230 |
| Gnomish submarine, giant turtle | 37 | 100 |
| Gryphon rider, Kurdan and Sky'ree | 38 | 190 |
| Dragon, Deathwing | 26 | 190 |
| Skeleton | 10 | 35 |
| Daemon | 34 | 42 |
| Guard tower (either side) | see note | 60 |
| Cannon tower (either side) | see note | 151 |

Towers have no strike point in their scripts. A tower fires from its order's logic (`orders/orders.md`, "Order 14
(towers)"), which runs at the step done that starts each loop, so a tower shoots at most once per loop: every 60
cycles for a guard tower and every 151 for a cannon tower. Towers don't use the cooldown.

**Evidence:** game data (the script timings); traced: the cooldown and the switch waits (`orc02-assault-3000`, V18 to
V24), steady fire (V22), the cooldown kept while the target is lost (`xorc01-death-knight-2100`), haste (V86, V87),
towers (`xhuman10-spells-700`, V25). The wait of 1 at the decision fits the decision records (frame 0, wait 1, in V86
and in unhasted steady fire alike) and steady fire every loop (V22); the trace records the end of each cycle, so it
doesn't show the wait at the moment of the test itself. Slow and the flyers are static study and game data only.

### How an attack delivers damage

An attacking unit plays its **attack script** (see "Attack timing" above). At the script's strike point, before the
strike itself, the **strike reveal** of `vision/fog.md` ("When sight is applied", item 4) is made when the order has a
target unit owned by another player. Then:

- **A command waiting.** If the attacker has been given a command whose order hasn't taken over yet, the strike does
  nothing at all: no strike reveal, no damage roll, no draw, no launch, and the cooldown isn't started again
  (`core/animation-scripts.md`, "No strike while a command waits"; traced: `orc14-25000`, vectors below).
- **Invisibility ends.** Otherwise the attacker's invisibility timer becomes 0 at the strike point, before the strike
  reveal and the strike, whatever the target's state, so a strike at a target that has gone inside ends it too
  (`spells/spells.md`, "Timers"; static study for the order of the steps and the inside target; a worker's chop and
  repair strikes do the same, traced: `human13-invisible-work-hits-900`).

- **No range test.** The strike point doesn't test range or the friendly-fire count again, and doesn't test whether the
  target is dying: those tests belong to the order's logic, which made them at the decision (`orders/orders.md`,
  "Attack orders"). The strike acts on whatever the attacker's target (or target tile) is at that moment (static
  study). It doesn't test whether the target is inside something either: a target that has gone inside since the
  decision is still struck or shot at, and the checks of "Applying damage" decide the outcome (traced:
  `orc09-strike-inside-hits-400`, `orc09-missile-inside-hits-600`, V93, V94). The one target test the stated rules
  keep at the strike point is the melee rule below for a target whose slot has been released (open question "A melee
  strike at a target whose slot is released").
- **Melee:** if the type's missile weapon is "none", the damage is rolled and applied to the current target at once.
  The strike is made even if the target started dying earlier in the same cycle (killed by another unit's strike):
  the damage is still rolled with its usual draws, and has no further effect. The same holds for a target that is
  inside something (a mine, a transport, a building): the damage is rolled against the target's armour, with its draw
  when T > 0, and handed to "Applying damage", which does nothing to it (the used mark) (traced for a target inside a
  gold mine: `orc09-strike-inside-hits-400`, V93). A target that is gone (its slot released) isn't struck, and nothing
  is drawn (V16; the stated rule, held against static study by the open question "A melee strike at a target whose
  slot is released").
- **Melee with no target unit** (order 11, a wall tile): the damage is rolled with armour 0 (step 1 of "Damage roll"),
  so it makes its draw when T > 0, and the result damages the wall on the attacker's target tile (order 11's goal
  tile, the pulled-back goal of `orders/orders.md`, "Order 11: attacking a wall tile") by the wall rule of "Splash on
  impact", step 1 (counter + max(1, floor(damage / 4))). A tile that isn't a wall by then takes nothing (static study;
  the order 11 section also says when the order stops).
- **Ranged:** otherwise a missile is launched. It starts from the attacker's centre and aims at the target's centre,
  or at the centre of the target tile if the attacker is attacking the ground. As with a melee strike, the launch is
  made even if the target is dying or inside something: it has no test of the target's state. A missile launched at a
  unit inside aims at that unit's centre at launch (for a worker in a mine, the centre it has at the mine's top-left
  tile), flies as usual with its draws and hits it; "Applying damage" then does nothing (traced:
  `orc09-missile-inside-hits-600`, V94).
  - **Non-splash missile:** the damage is rolled now, against the target's current armour, and stored in the missile.
    On impact it is applied to the unit that was the target at launch. A missile aimed at a tile with no unit damages a
    wall on that tile instead. (Static study reads the target as the target's unit slot, so a slot released and taken
    by a new unit during the flight would pass the hit to the new unit; held by the open question "A missile whose
    target's slot is reused in flight".)
  - **Splash missile** (big cannon, catapult rock, ballista bolt, small cannon): it stores the attacker's **maximum
    damage**, which is basic plus piercing including upgrades (and bloodlust), capped at 255. No draw is made at
    launch for the damage.
  - **Aim scatter:** after the damage is set, every missile launched by a unit's attack script makes two more draws.
    The first moves the aim point `(r AND 7) − 3` pixels horizontally and the second does the same vertically, so each
    offset is from −3 to +4. Towers (guard and cannon) fire without this scatter.
  - **Flight length:** a missile's flight is at least its minimum flight distance (table below). It is measured as the
    larger of the horizontal and vertical pixel distance.
  - **Cannon smoke:** a big or small cannon missile launched by a unit's attack (destroyers, battleships and
    juggernaughts in the default data; never towers) is followed by a cosmetic effect of kind 0x19 at the missile's
    start point (the attacker's centre, not moved by the aim scatter), facing 0. It is created after the missile,
    damage and scatter are set, in the lowest free effect slot left, so its slot is after the missile's. With no slot
    left there is no smoke, and the missile still flies. Like the missile, it gets its first update in the strike cycle
    C (`spells/spells.md`, "When a new effect first runs"). Its start animation waits 11 cycles; it then plays four
    frames with a wait of 3 each and is removed in cycle C + 24. It makes no sound and draws no random numbers.

Draws for one ranged shot, in order: one damage draw (non-splash only, and only if `T > 0`), then the horizontal
scatter, then the vertical scatter. If no effect slot is free for the missile, the shot creates nothing (no missile
and no smoke) and makes no draws at all.

Only the attack orders (9, 11, 12, 15, 16 and 17) strike this way. Towers shoot from their logic ("Towers"); spells
have their own strike rules (`spells/spells.md`); a worker's strike points chop or repair (`economy/harvest.md`,
`economy/repair.md`) and deal no damage.

**Evidence:** traced: a command waiting (`orc14-25000`, V76), a melee strike at a dying target (`human01-basics-9500`,
V15), a launch at a dying target (`xhuman4-7000`, V17), a melee strike and a launch at a target inside a mine
(`orc09-strike-inside-hits-400`, `orc09-missile-inside-hits-600`, V93, V94). Static study: no range test at the strike
point, the melee wall blow, the launch rules, scatter and smoke. The smoke's timing and slots are vectors V60 to V67
(static study). A melee target whose slot is released (V16) is the stated round-41 rule, not traced.

### Missile flight

**Start.** A missile launched by a unit's attack in cycle C gets its first update in C, which only starts it
(`spells/spells.md`, "When a new effect first runs"). From C + 1 it makes one **flight step every cycle**; every
kind in the table below steps every cycle.

**Flight length D.** D is the larger of the horizontal and vertical pixel distances from the missile's start (the
attacker's centre) to its aim point **after** the aim scatter. If D is below the kind's minimum flight distance,
D becomes that minimum. The aim point isn't moved, so the missile then flies on past it. Shots by towers (guard and
cannon) use the same rule without the scatter and without the wall exception below.

**Computer players' wall exception.** If the minimum is more than 2 tiles (catapult rock, ballista bolt), the
attacker belongs to a computer-controlled player, and the attack's target tile carries a wall flag (0x04 or 0x08
in `movement/occupancy.md`), the minimum is 2 tiles (64 px) instead.

**Steps.** Each step moves the missile s pixels on the major axis, and s pixels or 0 on the minor axis, by the line
rule of `spells/spells.md` ("How flying effects move"), with s the kind's speed. The remaining distance starts at
D and goes down by s per step. The step on which it goes **below 0** is the **landing step**. So a flight has
N = floor(D / s) + 1 steps, landing at C + N, and the missile ends N × s pixels from its start on the major axis
(1 to s pixels past D).

**Impact.** The missile then stays where it is. Its impact (the hit on its target, or the splash) comes a fixed
number of cycles after the landing step: the impact delay in the table. The impact happens at the missile's
final position.

**Impact effects.** At every impact of these missile kinds, after the impact sound (`ui/sounds.md`) and after the
hit or splash, a cosmetic effect is created at the missile's final pixel position:

| Missile kind | Impact effect |
| --- | --- |
| 0x0D catapult rock, 0x0E ballista bolt | kind 0x15 (ballista impact) |
| 0x07 big cannon, 0x18 small cannon | kind 0x1A (cannon tower explosion) |
| 0x11 submarine missile, 0x12 turtle missile | kind 0x17 (explosion) |
| Every other kind | none |

- The effect is made whatever the damage, even 0, and whether or not anything was hit.
- It takes the lowest free effect slot. If there is none, no effect is made. The missile's own slot is freed only
  after this, so the effect never reuses the missile's slot. A gryphon hammer or dragon breath likewise keeps its
  slot until after its final hit ("Missile flight").
- Its first update follows `spells/spells.md`, "When a new effect first runs" (it is created by another effect).
- It is created with only its kind and position set: destination (0, 0), no target, no shooter, damage 0
  (`spells/spells.md`, "Cosmetic effects"). Nothing is copied from the missile.
- If the hit destroys a building, the building's own explosion (`core/unit-slots.md`, "Death, corpses and release")
  is created first, so the impact effect takes the next free slot.
- It changes nothing in the game. It holds its slot until its script's step done, counted from its start step, one cycle
  after its first update (`spells/spells.md`, "When a new effect first runs"): for kind 0x15, six frames with a wait of
  2 each (12 cycles); for kind 0x1A, four frames with a wait of 3 each (12 cycles); for kind 0x17, sixteen frames with a
  wait of 3 each (48 cycles; the cosmetic effect of `spells/spells.md`, "Cosmetic effects"). So 0x15 and 0x1A are
  removed in U + 14 and 0x17 in U + 50, with U the first update.

**Draws.** Each flight step makes one draw, at the end of the step and after the move, including the landing
step. The draw is discarded. The catapult rock and the small cannon make one **more** draw on every step,
including the landing step, after which their total travel n × s (for step n) is still below 5 × ceil(D / 5). A
landing step's draws always come before the impact's draws, which happen in a later cycle. A non-splash missile's
impact makes no draw of its own (only what "Applying damage" makes).

| Value | Missile | Speed (px per step) | Extra draws | Impact delay (cycles after landing) |
| --- | --- | --- | --- | --- |
| 0x00 | lightning | 16 | no | 11 |
| 0x07 | big cannon | 12 | no | 1 |
| 0x0A | touch of death | 12 | no | 16 |
| 0x0D | catapult rock | 8 | yes | 5 |
| 0x0E | ballista bolt | 8 | no | 1 |
| 0x0F | arrow | 12 | no | 1 |
| 0x10 | axe | 12 | no | 1 |
| 0x11 | submarine missile | 16 | no | 1 |
| 0x12 | turtle missile | 16 | no | 1 |
| 0x18 | small cannon | 16 | yes | 1 |
| 0x1B | demon fire | 12 | no | 5 |
| 0x01, 0x02 | gryphon hammer, dragon breath | 12, then 6 | no | see the next entry |

**Gryphon hammer and dragon breath.** Each step makes **one draw at its start**, before moving. The step moves
12 px, or 6 px if the remaining distance is already below 50 at the start of the step, by the same line rule; the
remaining distance goes down by the amount moved. It keeps falling below 0, and the missile flies on past the aim
point. Every step that leaves the remaining distance below 50 is **counted**:

- if the count has already passed the **limit**, the missile ends on this step (after moving);
- otherwise the count goes up by 1, and when the new count is a multiple of 8 the missile **splashes** at its position,
  if an effect slot is free. The lowest free effect slot is **taken** first; if there is none, nothing more happens on
  this step: no splash, no sound and no effect. Otherwise the explosion sound (`misc\explode`) plays at the missile's
  tile, the splash below is applied (its draws included), and a cosmetic **explosion** (kind 0x17) is then written into
  the slot taken, at the missile's pixel position, with its other fields zero (`spells/spells.md`, "Cosmetic effects").
  It changes nothing in the game, is removed in U + 50, and its first update follows `spells/spells.md`, "When a new
  effect first runs" (it is created by another effect). Because the slot is taken before the splash's damage, the death
  explosion of a unit the splash kills takes a later slot than the splash's own explosion (traced: `xorc8-10000`,
  vectors below).

The limit is 25, or 40 if the stored damage is exactly 40. So with the default data (stored damage 8 to 16) the
count runs 1 to 26, splashes come on counts 8, 16 and 24, and the missile ends on the step after count 26. One
cycle after the end step, the stored damage is applied to the unit that was the target at launch, as for a
non-splash missile: wherever the hammer is, with no further armour and no draw. If it had no target, a wall on its
final tile takes the damage. The missile's effect slot stays **in use** through that final hit and everything that
follows from it (the death explosion of a unit it kills, and any other effect made then); it is freed **last**, as for
every other missile ("Impact effects"). So a death explosion made by the final hit never reuses the missile's slot:
it takes the lowest slot free without it (traced: `xorc11-16000` n = 15733, round-61 vectors).

**Splashes** use the splash rule ("Splash on impact") with the stored damage as that rule's D (not the flight
length) and these differences: **armour isn't subtracted**, so every unit within 42 px is hit, even with a base of 0;
air-layer units are included, each after its tile's ground-layer unit, so an air unit on the impact tile is reached in
its place in the scan, after the impact tile's ground-layer unit and every unit before it in the scan (static study;
the same order is traced for the small cannon, V95); the firing unit is never hit. One draw is made per unit hit,
after the step's own draw. Each unit's damage `h + (r mod (h + 1))` is kept to its low 8 bits, as in step 5 of "Splash
on impact": only a stored damage of 255 can reach 256, which wraps to 0 damage; the default data never stores 255
(static study, V97). Walls take D and floor(D / 4) as usual. A unit can be hit by several splashes of the same missile
and by the final hit.

**Evidence:** static study for the flight, steps, draws and delays (vectors V34 to V50 and V54 to V58); traced: the impact
effect (`orc09-repair-600`, V59), the breath splash (`xorc10`, V51), the splash's slot order (`xorc8-10000`, V52), a
dead shooter's missile (`human13-subs-2000`, V53), the final hit's slot (`xorc11-16000`, V85).

### Splash on impact

When a splash missile lands with stored damage D at its impact pixel:

1. **Walls.** The wall on the impact tile takes D, and the walls on the four orthogonal neighbour tiles take
   `floor(D / 4)`. Each wall tile keeps a damage counter from 0 to 19. Damage reaches the wall as a single byte, so
   values of 256 or more wrap first. A hit adds `max(1, floor((damage mod 256) / 4))`. If the new counter would
   exceed 19, the wall **advances one stage** (intact, then damaged, then rubble) and the counter goes back to 0; the
   excess is lost. An intact wall therefore needs two overflows to become rubble. When a tile becomes rubble, each
   orthogonal neighbour that is still intact advances to damaged. The full rule is in `movement/occupancy.md`,
   "Walls".
2. **Units.** The ground-layer unit on the impact tile is considered first. Then every ground-layer unit in the 7 × 7
   tiles centred on the impact tile is considered, in the scan order of the area; when a small-cannon missile was fired
   at an air unit, the air-layer unit of each tile is considered after that tile's ground-layer unit, so an air unit on
   the impact tile comes in its tile's place in the scan, not first (traced: `human13-splash-air-hits-1700`, V95).
   Air units are considered only when a small-cannon missile was fired at an air unit. Each unit is hit at most once
   per impact. The firing unit is never hit. **Friendly units are hit too.**
3. For each unit considered, let dx and dy be the pixel offsets from the impact point to the unit's centre, and
   `d² = max(dx², dy²)`:
   - if `d² ≥ 1792` (`|d| ≥ 43` px), the unit is not hit;
   - if `d² ≤ 511` (`|d| ≤ 22` px), the base is D;
   - otherwise, from 23 to 42 px, the base is `floor(D / 4)`.
4. The target's armour, including upgrades, is subtracted from the base. If the result is below 1 the unit is not hit
   and no draw is made. Note that this subtracts armour from the whole amount, piercing part included, unlike the
   single-target roll.
5. Otherwise, with `v` the result, `h = floor((v + 1) / 2)`. Make one draw r. The damage is `h + (r mod (h + 1))`,
   kept to its low 8 bits. Only `v = 255` can reach 256, and that wraps to 0 damage. It is then applied as in
   "Applying damage".

Units are processed in the order found, so draws are made in that order: the impact tile's ground-layer unit first,
then the 7 × 7 block **row by row from the top, each row left to right**. When air units are included, each tile's
ground-layer unit comes before its air-layer unit, and the impact tile's air-layer unit is reached only in its place
in the scan (row 4, column 4). A unit already considered (the impact tile's ground-layer unit) isn't considered again.

A dying unit is no longer on the occupancy marks (`core/unit-slots.md`, "Death, corpses and release"), so a splash
never considers it and makes no draw for it.

**Evidence:** static study (vectors V26 to V30); traced: a splash kill of a unit other than the target
(`xhuman10-blizzard-600`, V72) and the breath splash's units (`xorc10`, V51). The dying-unit sentence restates the
death rule of `core/unit-slots.md`.

### The demolition blast

A demolisher's blast is the fourth delivery: a flat 200, then 200 again, with no armour and no draw, to each ground
occupant of the 8 neighbouring tiles, applied as in "Applying damage" with the demolisher as the attacker. The full
rule is in `combat/attack-ground-demolish.md`, "The blast".

### Applying damage

When damage D from an attacker reaches a target:

- Nothing happens if the target is already dying or has been removed.
- **Dark Portal special case:** in a single-player campaign game (not a network game, not a custom scenario) on
  campaign entries 32, 34 and 50 (`game/victory.md`, "Mission rules"), a hit on a type 0x65 (Dark Portal) does
  nothing, not even the notification, unless the attacker is type 0x18 (Khadgar). These are *Beyond the Dark Portal*
  human missions 3, 4 and 12 (`2XHum03`, `2XHum04`, `2XHum12`), each of which places one neutral Dark Portal. The
  portals placed by `2XOrc12` and `Human14` aren't protected.
- Nothing happens if the target has the used mark (`core/corpses.md`), for example a passenger inside a transport.
- Otherwise the target is first **notified that it was attacked**, even if D is 0 (`ai/attack-ai.md`, "Call for
  help": which attacker is recorded, and who comes to help). The under-attack alert is a separate step
  (`ui/sounds.md`, "Ready, work complete and under attack").
- If the target's unholy-armour timer is running, or D is 0, it takes no damage.
- If the target's HP ≤ D, it dies: see "A kill" below. Otherwise the target's HP drops by D, and the under-attack
  alert of `ui/sounds.md` is then asked for.

D is at most 255 in every delivery. Spell damage goes through the same step (`spells/spells.md`).

**Evidence:** static study (the checks and their order, the Dark Portal special case, with game data for its maps);
traced: unholy armour (`spells/spells.md` vectors), the notification and help (`ai/attack-ai.md` vectors).

#### A kill

If the target's HP ≤ D, it dies and the kill is credited to the attacker's owner. Its HP is **not** lowered: it keeps
its HP from before the blow, and its recorded target, through the death animation. A target it forgot after a kill of
its own stays forgotten: a unit that killed its target and has had no new target since dies with no target, even
though its order was still the attack order (traced: `xhuman12-10000`, `xhuman8-10000`, vectors below). Then, if the
attacker is a unit (not a building), it checks **its own** records, whichever units they are, whatever the attacker's
own state (a shooter that is itself dying or dead, its missile still in flight, makes the check too and reports no
target from then on; traced: `human13-subs-2000`, vectors below): it at once forgets its target (the unit it is
attacking for its command) if that unit is now dying, gone or not detected by the attacker's player, and likewise
forgets its recorded attacker if that unit is now dying, gone or not detected. The attacker's order doesn't change in
that step. The victim is dying by then, so it is always forgotten when it is the target or the attacker. This applies
to every kill: a missile's kill counts as the shooter's, and a splash can kill a unit other than the target, even one
of the shooter's own, after which a target that is dying from someone else's kill is forgotten too (traced:
`xhuman10-blizzard-600`, vectors below). Other units attacking the same target aren't touched; each forgets it on its
own next logic run (`orders/orders.md`, "Arriving and losing the target"). Only the killer forgets the victim as its
**attacker**; every other unit that recorded the victim as its attacker keeps that record while the victim is dying
(it counts in the resulting-order table of `orders/orders.md`). A building that kills (a tower) makes none of these
checks and keeps both its records (static study). If the victim is a building with a job running, the job is cancelled
at the kill, with its refund (`economy/build-train.md`, "A building killed").

The kill happens in this order: the credit, then the victim's death (`core/unit-slots.md`, "Death, corpses and
release": occupancy cleared, units inside killed, the death sound and a building's explosion, the order "dying"), then
the killer's check of its records.

**Credit and score.** The kill is credited only when the attacker's owner isn't the victim's owner. The attacker's
owner then counts a **kill** (a victim of a type below the first building type) or a **razing** (a building), and its
mission score goes up by the victim type's point value (`game/campaign-flow.md`, "Mission score", which holds the
score's later use). Ownership is the only test: a kill of an allied unit is credited too (traced:
`human05-ally-kill-hits-1000`, V92, V96). A missile or splash is credited to the unit in the shooter's slot
(`spells/spells.md`, "The shooter after its slot is released"). Kills that don't come through this step are never
credited: a demolisher's own death and other outright kills (polymorph, a builder with no exit tile, a passenger
killed with its transport).

**A missile's shooter is its slot.** The same slot rule holds for everything else a missile does with its shooter at
impact: the victim's attacker record, the killer's check of its records, and the splash's "the firing unit is never
hit" all use the unit that holds the shooter's slot then, as for spell effects (static study, V98). A slot is released
only long after its unit has died, so this differs from naming the shooter by unit only if the shooter's slot is
released, or released and reused, during the flight, which normal play doesn't reach.

**Evidence:** traced: the HP and target kept (`human01-basics-9500`, V13, V14), the killer's forget (V72, V53), dying
with no target (V74, V75); static study: the order of the steps, a building killer, the credit and the score (the trace
format has no score fields).

### Towers

A tower (guard or cannon, either side) attacks only through order 14 (`orders/orders.md`, "Order 14 (towers)"): each
logic run, at the step done that starts each loop of its attack animation, it searches with its attack range and, if it
finds a target, **shoots at once** in that run. The shot:

- is always a missile of the tower type's missile kind (arrow for guard towers, small cannon for cannon towers, in the
  default data). With no free effect slot nothing is created and nothing is drawn, as for any ranged shot;
- carries a rolled damage ("Damage roll", against the target's current armour) for a non-splash kind, or the tower's
  maximum damage for a splash kind, as for a unit's shot;
- makes **no aim scatter** draws, uses no wall exception and makes no cannon smoke;
- doesn't turn the tower and doesn't start any cooldown; there is no strike reveal and no strike point;
- plays the tower sound (`ui/sounds.md`, "Attacks": guard towers `misc\bowfire`, cannon towers `misc\fireball`).

So a guard tower shoots at most every 60 cycles and a cannon tower every 151 ("Attack timing (cooldown)"). A tower
that kills keeps its records ("A kill"). A building whose order isn't 14 never attacks.

**Evidence:** traced: the shot times (`xhuman10-spells-700`, V25); static study: the shot rules (vector V65 for the
cannon tower), the missile kind.

### Reach and range

- A unit can attack a target within its type's attack range, in tiles.
- Rangers get +1 range once Longbow is researched, and berserkers get +1 once Lighter Axes is researched. No other
  upgrade changes range.
- The Ranger Scouting and Berserker Scouting upgrades change those units' sight, not their range.
- **Target acquisition** measures distance as the larger of the horizontal and vertical tile distances from the
  searcher's tile to a reference tile of the target: a mobile target's own tile, or for a building its nearest edge
  column or row, or its middle one (the exact rule is in `orders/orders.md`, "Automatic target acquisition"). The
  acquisition range check and minimum-range check use this measure.
- **The range test of the attack orders** (whether a target is in reach to strike) is the arrival measure of
  `movement/movement.md`, "Arriving", with the attack range as the radius and no extra tile (`orders/orders.md`,
  "Attacking without moving"). It is made by the order's logic only; the strike point makes none ("How an attack
  delivers damage").
- **Minimum range.** A missile's minimum flight value v makes acquisition skip a candidate at d < v − 1 ("Missile
  weapon kinds"). The minimum flight distance itself only lengthens the flight ("Missile flight").

**Evidence:** game data (ranges, upgrades); traced: the minimum flight in acquisition (`orc5-40000`, V82); the range test
is stated in `orders/orders.md`; no range test at the strike point is static study.

### What the player sees and hears

None of these change the game state, but each is part of combat:

| What | Rule |
| --- | --- |
| The attacker turns to its target (warships broadside) | `core/animation-scripts.md` (facing) |
| The attack sound (sword draws) and the tower sounds | `ui/sounds.md`, "Attacks" |
| The strike reveal of the attacker's area for the target's owner | `vision/fog.md`, "When sight is applied", item 4 |
| Missiles in flight, cannon smoke, impact effects and hammer splashes | "How an attack delivers damage", "Missile flight" |
| Missile impact sounds | `ui/sounds.md`, "Missiles, explosions and other effects" |
| The under-attack alert ("help" voice), quiet counters and help timer | `ui/sounds.md`, "Ready, work complete and under attack" |
| Death sounds and building explosions | `ui/sounds.md`, "Deaths"; `core/unit-slots.md`, "Death, corpses and release" |
| The death animation, corpses, wrecks and ruins | `core/corpses.md` |
| Wall damage stages | `movement/occupancy.md`, "Walls" |
| The HP shown for a dying unit (unchanged) | "A kill"; `ui/info-panel.md` |
| The score screen's kills and razings | `game/campaign-flow.md`, "The score screen" |

## Data

### Upgrade amounts per level

| Upgrade line | Adds per level | Applies to |
| --- | --- | --- |
| Arrows / spears | +1 piercing | Archer, axe thrower, ranger, berserker |
| Swords / axes | +2 basic | Other land units with upgradable weapons |
| Shields | +2 armour | Land units with upgradable armour |
| Ship cannons | +5 basic | Sea units with upgradable weapons |
| Ship armour | +5 armour | Sea units with upgradable armour |
| Siege (catapult/ballista) | +15 basic | Units with flag bit 2 |
| Marksmanship | basic 3 moves into piercing | Ranger |
| Longbow / Lighter Axes | +1 range | Ranger / berserker |

One more upgrade line exists with an amount of 10 per level. Its use hasn't been found.

### Missile weapon kinds

The values come from the `UDTA` missile weapon byte. The minimum flight distance is in tiles of 32 px.
For target acquisition, a missile with minimum flight value v skips a candidate whose acquisition distance d is
less than v − 1 (`orders/orders.md`, "Automatic target acquisition"): catapults and ballistas (3) skip d ≤ 1 only;
cannons (2) and missiles with no value skip nothing, since d is at least 1 (traced: `orc5-40000`, round-59 vectors).

| Value | Missile | Splash | Minimum flight |
| --- | --- | --- | --- |
| 0x07 | big cannon | yes | 2 |
| 0x0D | catapult rock | yes | 3 |
| 0x0E | ballista bolt | yes | 3 |
| 0x18 | small cannon | yes | 2 |
| 0x01, 0x02 | gryphon hammer, dragon breath | along the path (see "Missile flight") | none |
| 0x0F, 0x10, 0x11, 0x12, 0x1B and others | arrow, axe, submarine, turtle, demon fire | no | none |
| 0x1D | none (melee) | not applicable | not applicable |

### Unit statistics used in the test vectors

These are the default `unitdata.dat` values.

| Unit | Basic | Piercing | Armour | Range |
| --- | --- | --- | --- | --- |
| Footman / grunt | 6 | 3 | 2 | 1 |
| Archer | 3 | 6 | 0 | 4 |
| Ranger | 3 | 6 | 0 | 4 |
| Ballista / catapult | 80 | 0 | 0 | 8 |
| Guard tower | 4 | 12 | 20 | 6 |

## Test vectors

`r` is the draw used for the damage. Where it isn't given, the full range is listed.

| ID | Input | Expected |
| --- | --- | --- |
| V01 | Footman (6/3) hits footman (armour 2), no upgrades | b = 4, T = 7, h = 4; damage = 4 + (r mod 5), range 4 to 8 |
| V02 | Same, r = 0 | 4 |
| V03 | Same, r = 346 | 4 + (346 mod 5) = 5 |
| V04 | Footman with swords 2 (basic 10) hits footman with shields 2 (armour 6) | b = 4, T = 7, damage 4 to 8 (the upgrades cancel out) |
| V05 | Archer (3/6) hits footman (armour 2) | b = 1, T = 7, damage 4 to 8 |
| V06 | Archer with arrows 2 hits footman | piercing 8, b = 1, T = 9, h = 5, damage 5 to 10 |
| V07 | Ranger with Marksmanship hits footman | basic 0, piercing 9, T = 9, damage 5 to 10 |
| V08 | Bloodlusted grunt (12/6) hits footman (armour 2) | b = 10, T = 16, h = 8, damage 8 to 16 |
| V09 | Unit with basic 2, piercing 0 hits armour 2 | T = 0, damage 0, no draw made |
| V10 | Basic 1, piercing 0 hits armour 0 | T = 1, h = 1, damage 1 + (r mod 2): 1 or 2 |
| V11 | Unholy-armoured target takes any hit | HP unchanged; it is still notified it was attacked |
| V12 | Target with 5 HP takes 5 damage | Dies (HP ≤ damage); its HP stays **5** during the death animation and its target is kept |
| V13 | `human01-basics-9500` (oracle trace), n = 288: footman 1592 (HP 2 since n = 276) is killed by footman 1597; footman 1598 also attacks it | 1592: order 1, animation 1, HP 2, target 1598. 1597: target null, still order 9. 1598: target 1592 |
| V14 | Same, n = 291 | 1598 forgets 1592 on its own logic run: order 2, wait 3 |
| V15 | `human01-basics-9500`, n = 576: footmen 1593 and 1597 (type 0x00), both in order 9 on 1591, reach the strike point in the same cycle; 1593 strikes first and kills 1591 | 1593: target null. 1597 still strikes: both strikes roll damage, and the cycle's draws include 12374 and 6732 |
| V16 | A melee unit whose target's slot was released before its strike point | No strike, no draws (the stated round-41 rule; static study differs, see the open question "A melee strike at a target whose slot is released") |
| V17 | `xhuman4-7000` (oracle trace), n = 121: archers 1486 and 1501 (type 0x08) in order 16 on grunt 1505, dying since n = 119 | Both launch: new effects in slots 4 and 5, kind 0x0F, target 1505, damage 5 (shooter 1486) and 7 (shooter 1501) |
| V18 | `orc02-assault-3000` (oracle trace), archer 1584 (type 0x08, cooldown 66) at (47, 28), n = 343 | Takes target 1578: animation 4, wait 3; cooldown set to 66 |
| V19 | Same, n = 346 | Cooldown left 63 > wait 1: attack animation restarted, frame 0, wait 63 |
| V20 | Same, n = 409 to 412 and n = 475 to 478 | New targets 1573 and 1564, each with wait 3 then wait 63 |
| V21 | Same, n = 541 to 562 | Cooldown run out: facing 1 at n = 541; frame 25 (n = 542), frame 30 (n = 552), frame 0 with wait 44 (n = 562) |
| V22 | Archer firing at the same target without a break | One strike every 65 cycles (frame 25 at n = 706 and n = 771 in the same trace); the cooldown never restarts the animation |
| V23 | `orc02-assault-3000`, footman 1582 (type 0x00, cooldown 26), n = 376 and n = 402 | New targets 1578 and 1576: wait 3, then wait 23 at n = 379 and n = 405 |
| V24 | `orc02-assault-3000`, footman 1587 in order 12, cooling down, n = 617: its search switches from 1568 to 1573, in reach | Order 12, target 1573, wait 23 (the cooldown left) in the same record; no wait of 3 |
| V25 | `xhuman10-spells-700` (oracle trace), guard tower 1468 and cannon tower 1501 (order 14 from n = 2) | Shots at n = 5; then 1468 every 60 cycles (n = 65, 125, 185, …) and 1501 every 151 (n = 156, 307) while targets are in range |
| V26 | Catapult (80/0, no upgrades) splash, footman 10 px from impact | v = 80 − 2 = 78, h = 39, damage 39 to 78 |
| V27 | Same, footman 30 px from impact | base 20, v = 18, h = 9, damage 9 to 18 |
| V28 | Same, footman 43 px from impact | not hit, no draw |
| V29 | Catapult with siege 2 (basic 110) splash on an intact wall tile with counter 0 | centre wall takes 110 (+27): it becomes damaged with counter 0, not destroyed. Each neighbour wall takes 27 (counter +6) |
| V30 | Splash base 5 at 30 px (quarter = 1) on armour 2 | v < 1, not hit, no draw |
| V31 | Arrow fired by a unit | draws: damage (if T > 0), then x scatter, then y scatter; each scatter is (r AND 7) − 3 |
| V32 | Footman attacking continuously, no spells | hits on cycles 11, 36, 61, … of the attack |
| V33 | Hasted footman | each wait 3 becomes 1, 5 becomes 2, 10 becomes 5; loop = 1 + 1 + 1 + 1 + 2 + 5 = 11, strike on cycle 5 |

Missile flight (C is the cycle of the strike; the scatter draws are taken as giving 0 offset, so the aim point is the
target's centre):

| ID | Input | Expected |
| --- | --- | --- |
| V34 | Arrow, D = 100 px | Draws at C: damage (if T > 0), x scatter, y scatter. Steps at C + 1 to C + 9, with remaining 88, 76, 64, 52, 40, 28, 16, 4, −8: landing at C + 9, 108 px from the start. One draw at the end of each step (9). Impact at C + 10: the stored damage is applied, with no draw of its own. 12 draws in all |
| V35 | Catapult rock, D = 160 px | Draws at C: x scatter, y scatter. 5 × ceil(160 / 5) = 160. Steps at C + 1 to C + 21 (remaining 0 after step 20, −8 after step 21): landing at C + 21, 168 px out. Steps 1 to 19 make 2 draws each (travel 8 to 152, below 160) and steps 20 and 21 make 1 each: 40 flight draws. Splash at C + 26, with 1 draw per unit hit |
| V36 | Catapult rock, D = 103 px | N = 13 (104 px); 5 × ceil(103 / 5) = 105, so all 13 steps, the landing step included, make 2 draws: 26 |
| V37 | Ballista bolt, D = 100 px | N = 13, landing at C + 13 (104 px out), 13 flight draws, splash at C + 14 |
| V38 | Lightning (mage), D = 100 px | N = 7, landing at C + 7 (112 px out), 7 flight draws, hit at C + 18 |
| V39 | Human-controlled catapult attacks a wall tile, D = 70 px before the minimum | Minimum 96, so D = 96: N = 13, the rock lands 104 px out (34 px past the wall's centre) |
| V40 | Same, computer-controlled | Minimum 64, so D = 70: N = 9, the rock lands 72 px out |
| V41 | Gryphon rider (basic 0, piercing 16) strikes footman A (armour 2) at cycle C. A's centre is 96 px due east of the gryphon's centre. Footman B's centre is 20 px south of A's. No upgrades. Draws: launch r1 with r1 mod 9 = 4, scatter draws with r AND 7 = 3 | Stored damage 8 + 4 = 12; aim = A's centre, D = 96. Steps at C + 1 to C + 3 move 12 px (remaining 84, 72, 60). Step 4 moves 12 (remaining 48): count 1, 48 px out. Then 6 px a step: count k at step 3 + k, at 42 + 6k px |
| V42 | Same, step 11 (count 8, 90 px out) | Step draw, then a splash at 6 px west of A's centre. A (6 px): base 12, h = 6, damage 6 + (r mod 7), 6 to 12. Then B (max(6, 20) = 20 px): the same, 6 to 12. A is drawn for first if the splash point is on A's tile |
| V43 | Same, step 19 (count 16, 138 px out) | A and B are both 42 px away on x: base floor(12 / 4) = 3, h = 2, damage 2 to 4 each, A first (row order) |
| V44 | Same, step 27 (count 24, 186 px out) | Nobody within 42 px: no splash draws |
| V45 | Same, steps 28 and 29 | Counts 25 and 26 |
| V46 | Same, step 30 (C + 30) | Step draw, move to 204 px; count 26 is past 25, so it ends |
| V47 | Same, C + 31 | A takes the stored 12, with no draw |
| V48 | Same, totals | Draws: 3 at launch, 30 step draws, 4 splash draws = 37. A takes 20 to 28 in all, and B 8 to 16 |
| V49 | Same, each of the three splash steps, with an effect slot free | `misc\explode` at the hammer's tile, the splash, then a kind 0x17 explosion at the hammer's pixel position in the lowest free effect slot, with zero fields |
| V50 | A hammer reaches count 8 while every effect slot is in use (static study only) | No splash damage, no draws for hits, no sound, no explosion; the hammer flies on and the count keeps going |
| V51 | `xorc10` (oracle trace), n = 329: the breath of dragon 1562 (type 0x2B; missile in effect slot 3) splashes at pixel (978, 950) | Effect slot 5: kind 0x17 at (978, 950), destination (0, 0), target null, shooter null, damage 0. Units hit: 1513 (40 to 36 HP), 1519 (40 to 27) and 1520 (60 to 58) |
| V52 | `xorc8-10000` (oracle trace), n = 8035: the hammer of gryphon rider 1550 (effect slot 3) splashes at pixel (8, 2898) and kills goblin zeppelin 1400 (type 0x29); slots 4 and up free | Slot 4: the splash's explosion (kind 0x17) at (8, 2898); slot 5: the zeppelin's death explosion (kind 0x17) at (16, 2888) |
| V53 | `human13-subs-2000` (oracle trace): dragon 1436 (type 0x2B, owner 4) dies at n = 1476 with target 1407; its last breath, still in flight, kills gryphon rider 1407 (type 0x2A, owner 1) at n = 1487 | n = 1487: 1436 in order 1, target null (it showed 1407 at n = 1486) |

Impact effects:

| ID | Input | Expected |
| --- | --- | --- |
| V54 | A catapult rock lands and splashes; effect slots 0 to 4 in use, slot 5 free; the rock is in slot 3 | No impact sound; the splash; a kind 0x15 effect in slot 5 at the rock's position; the rock's slot 3 is freed. Slot 5 comes after slot 3, so the effect's first update is in the impact cycle, and its start step, with `misc\explode` at its tile, one cycle later. It is removed 14 cycles after the impact |
| V55 | A ballista bolt with stored damage 0 lands | No impact sound (damaging only); the kind 0x15 effect is still made, and plays `misc\explode` at its start step, one cycle after its first update |
| V56 | A battleship's big cannon lands; slot 3 free, the missile in slot 6 | `misc\explode` (impact); splash; the kind 0x1A effect in slot 3. Slot 3 comes before slot 6, so its first update is in the next cycle and its start step, with `misc\explode`, in the cycle after that |
| V57 | A small cannon lands with every effect slot in use except the missile's own | No effect is made (the missile's slot is freed only afterwards); no sound at all |
| V58 | A submarine missile lands | `misc\explode` (impact); a kind 0x17 effect; no further sound |
| V59 | `orc09-repair-600` (oracle trace), n = 65: the catapult rock of catapult 1494 (in effect slot 3 at n = 64) lands | Effect slot 4: kind 0x15 at (1072, 2768), the rock's final position; destination (0, 0), target null, shooter null, damage 0 |

Cannon smoke and launch slots:

| ID | Input | Expected |
| --- | --- | --- |
| V60 | A battleship strikes in cycle C; effect slots 0 to 2 in use, 3 and up free | Missile in slot 3, smoke (kind 0x19) in slot 4 at the battleship's centre, facing 0. Both first update in C; the smoke is removed in C + 24 |
| V61 | A destroyer strikes; slots 3 and 5 in use, 4 and 6 free | Missile in slot 4, smoke in slot 6 |
| V62 | A destroyer strikes; only one effect slot free | Missile in that slot; no smoke; the two scatter draws as usual (the small cannon is a splash missile, so no damage draw) |
| V63 | A juggernaught strikes; no effect slot free | Nothing created; no draws for this shot |
| V64 | An archer strikes; no effect slot free | Nothing created; no damage draw and no scatter draws |
| V65 | A human cannon tower fires at a ship | Small cannon missile, no scatter draws, no smoke; cannon sound |
| V66 | A catapult or ballista strikes | Missile and scatter as usual; no smoke |
| V67 | Smoke updated and removed in cycle C + 24; later in that cycle a higher-slot effect creates a new effect | The new effect may take the smoke's old slot |
| V68 | *Beyond the Dark Portal* human mission 12 (entry 50), single player, a knight hits the Dark Portal | Nothing: no damage, no notification |
| V69 | Same, Khadgar (type 0x18) hits it | Damage applied as usual |
| V70 | *Beyond the Dark Portal* orc mission 12 (entry 51), a grunt hits the Dark Portal | Damage applied as usual |
| V71 | `2XHum12` played as a custom scenario | Damage applied as usual (the check needs a campaign game) |
| V72 | `xhuman10-blizzard-600` (oracle trace): ballista 1483 (type 0x04, owner 1) in order 16, cooling down (animation 4, wait 88 at n = 308), keeps target grunt 1490, dying since n = 277 from another unit's kill; at n = 309 its own rock's splash kills footman 1479 (owner 1, HP 10 kept) | n = 309: order 16, animation 4, frame 15, wait 87, target null |
| V73 | A killer whose target is alive and isn't the victim | Target kept |
| V74 | `xhuman12-10000` (oracle trace): grunt 1504 (type 0x01, owner 2) kills footman 1398 at n = 4837 and forgets it, staying in order 12 while it cools down; it dies at n = 4851 | n = 4851: order 1, animation 1, target null |
| V75 | `xhuman8-10000` (oracle trace): juggernaut 1535 (type 0x21, owner 3) in order 12 forgets destroyer 1557 at n = 6749 and dies at n = 6831 | n = 6831: order 1, animation 1, target null |
| V76 | `orc14-25000` (oracle trace, Enhanced), n = 12307: computer paladin 1456 (type 0x0C, owner 6) in order 12 reaches its strike with the caster AI's healing command on paladin 1433 not yet taken over; peon 1448 (type 0x03, owner 0) has 11 HP | No draw; 1448 keeps 11 HP |
| V77 | A footman at its strike point is given a stop command earlier in the same cycle (static study) | No damage and no draw |

### Round-59 vectors

| ID | Input | Expected |
| --- | --- | --- |
| V78 | `human2-human02-all-out-4000` (oracle trace, Enhanced; the invulnerability cheat turned on by player 1 in turn 6), n = 514: archer 1570 (type 0x08, owner 1, human) shoots at 1593, player 0's watch tower (destroyed at n = 510, so armour 0 for this hit); no upgrades, no bloodlust, draw r = 7266 | Arrow effect 4 (kind 0x0F) with damage **158**: basic term 3 + piercing 200 = 203, h = floor(204 / 2) = 102, 102 + 7266 mod 103 = 102 + 56. With basic 0 it would be 100 + 7266 mod 101 = 195 |
| V79 | Cheat on; a human player's footman (basic 6, piercing 3), sword level 1, bloodlust running | Basic (6 + 2) × 2 = 16, piercing 200 (not 400, not 203) |
| V80 | Cheat on; a human player's archer (basic 3, piercing 6), arrow level 1, no bloodlust | Basic 3, piercing 200 |
| V81 | Cheat on; a computer player's footman, any upgrades or bloodlust | Basic 0, piercing 0 |
| V82 | `orc5-40000` (oracle trace, Enhanced), n = 21489: computer catapult 1580 (type 0x05, owner 5) at (122, 7), idle, and transport 1516 (type 0x1C, owner 0) landing at (120, 6): d = 2, minimum flight 3 | Not skipped: order 16, animation 4, wait 3, target 1516 |
| V83 | A ballista with an adjacent candidate, d = 1 (as in `human06-31357` n = 23683, `orders/orders.md`) | Skipped |
| V84 | A cannon (minimum flight 2), candidate at d = 1 | Not skipped |

### Round-61 vectors

| ID | Input | Expected |
| --- | --- | --- |
| V85 | `xorc11-16000` (oracle trace, Enhanced), n = 15733: the hammer of gryphon rider 1490 (type 0x2A, owner 6), in effect slot 3 (slots 0 to 2 and 4 to 6 in use), makes its final hit on catapult 1495 (type 0x05, owner 5), which dies | The death explosion (kind 0x17) is in slot 7, not slot 3; slot 3 is free at the end of that cycle |


### Round-78 vectors

| ID | Input | Expected |
| --- | --- | --- |
| V86 | `xorc01-death-knight-2100` (oracle trace, Enhanced): ogre 1488 (type 0x07, cooldown 26), hasted since n = 37, in order 12 on 1515, steady fire | n = 826: strike decision (frame 0, wait 1). n = 829: the blow (1515: 40 to 34 HP). n = 832: the hasted loop's last step (frame 0, wait 5). n = 837, the step done that ends it: the cooldown test restarts the animation with wait **15** (26 − 11). n = 852: next decision; n = 855: the blow (34 to 25). Blows 26 cycles apart, not 11 |
| V87 | Same trace, ogre 1488 switches to target 1516 at n = 748 | n = 748: target 1516, wait 3. n = 751: wait 23 (the cooldown left). n = 774: decision; n = 777: the blow, in the same cycle as a blow of grunt 1497 (1516: 35 to 19 HP for the two). n = 785: wait 15. Grunt 1497 (type 0x01, not hasted), on the same target, strikes at n = 752, 777, 802 and 827: every 25 cycles, one fewer than the hasted ogre |
| V88 | A hasted footman (cooldown 26, hasted loop 11) attacking one target without a break (static study, from V86) | One blow every 26 cycles, each on cycle 5 of the hasted script (V33, whose loop of 11 is the script's, not the rate of fire); 11 cycles after each decision the animation restarts with wait 15 |
| V89 | A slowed footman (slowed attack loop 49, cooldown 26) attacking one target without a break (static study) | One blow every 49 cycles; the cooldown has run out at each decision, so the animation is never restarted |
| V90 | A footman (6/3, no upgrades) in order 11 strikes an intact wall tile with counter 0 (static study) | Armour 0: b = 6, T = 9, h = 5, one draw, damage 5 to 10; the wall counter goes up by 1 or 2 (max(1, floor(damage / 4))) |
| V91 | A catapult's splash kills a footman of its own player (static study) | No kill, razing or score for anybody; the catapult still forgets its records as after any kill |
| V92 | A unit kills a unit of an allied player (traced: `human05-ally-kill-hits-1000`, V96) | The killer's player gets a kill and the victim type's points |

### Round-79 vectors

| ID | Input | Expected |
| --- | --- | --- |
| V93 | `orc09-strike-inside-hits-400` (oracle trace, Enhanced): grunt 1481 (type 0x01, 6/3, T = 9) decides to strike peon 1482 at n = 316 (order 9, animation 4, frame 0); the peon enters the gold mine at n = 325 | n = 329: the strike is made: a melee hit of 9 on 1482 (the `hits` entry), whose HP stays 30. The cycle makes 4 draws, one more than the 3 of the cycles at the same place in the run's five-cycle background pattern (n = 324, 334) and of a capture without the attack line (not committed); one of the 4 is the damage draw (h = 5, damage 9 = 5 + (r mod 6), r = 15256 or 10000) |
| V94 | `orc09-missile-inside-hits-600` (oracle trace, Enhanced): axe thrower 1473 (type 0x09) decides to throw at peon 1478 at n = 520; the peon enters the mine at n = 528 | n = 533: an axe (kind 0x10) is launched in effect slot 3 from (1488, 2960), damage 6, aimed at (1613, 2926): the inside peon's centre (1616, 2928) plus scatter; 10 draws in the cycle (7 in a capture without the attack line, not committed). It flies 12 px a cycle; n = 545: it hits 1478 for 6, HP stays 30 |
| V95 | `human13-splash-air-hits-1700` (oracle trace, Enhanced): destroyer 1431 fires small cannon (D = 35) at gryphon 1407 over the coast at tile (54, 94); footman 1398 is on the same tile and archer 1409 at (53, 94); every impact is at that tile | n = 949: hits in order footman 19, archer 4, gryphon 18 (draws 28010, 7680, 13875). The same order at n = 1069 (23, 5, 20) and n = 1189 (32, 7, 30; the footman dies); then archer and gryphon at n = 1309 and 1429. The ground unit on the impact tile is first and the air unit comes after the archer, in its tile's scan place |
| V96 | `human05-ally-kill-hits-1000` (oracle trace, Enhanced, with a break on the step that adds the score): archer 1490 of player 1 kills peasant 1512 of allied computer player 0 at n = 947 | The score step is reached once, at n = 947, for owner 1 against a victim of owner 0: the ally's kill is credited |
| V97 | A gryphon hammer with stored damage 255 (custom data) splashes a unit at its impact point; the draw has `r mod 129 = 128` (static study; combat audit erratum 5) | v = 255, h = 128, 128 + 128 = 256, kept to its low 8 bits: 0 damage, not 255 |
| V98 | A catapult rock in flight whose shooter's slot is released and taken by a unit of another player before impact (constructed; not reachable in normal play; static study, combat audit erratum 3) | The new unit isn't hit by the splash; a kill made by the splash is credited to the new unit's owner; units hit record the new unit as their attacker |

## Open questions

- What the extra upgrade line with 10 per level is used for.
- **A melee strike at a target whose slot is released.** The stated rule (round 41, V16) is that a melee unit whose
  target's slot has been released isn't struck, and nothing is drawn. Static study (combat audit erratum 1) finds the
  blow rolled (one draw when T > 0) and handed to "Applying damage", which does nothing to a released slot; and if the
  slot has been taken by a new unit, the new unit is struck. Round 79 traced the same question for a target inside a
  mine and found the static reading (V93), but no trace has a released slot. **Default:** the stated rule (no strike,
  no draw), by the B-038 ruling (a static-only difference keeps the stated rule). It is rare in play: an attacker
  forgets a dying target at its next logic run, long before the slot is released.
- **A missile whose target's slot is reused in flight.** "Non-splash missile" applies the hit to the unit that was
  the target at launch. Static study (combat audit erratum 3) reads the target as its unit slot, so a slot released
  and taken by a new unit during the flight would pass the hit to the new unit. **Default:** as stated (the unit that
  was the target; a released slot takes nothing). Not reachable in normal play; no trace request.

### Answered in round 79

- *A melee strike at a target that is gone or inside* (trace request CB1). For a target **inside** something: the
  strike is rolled with its draw and handed to "Applying damage", which does nothing (traced:
  `orc09-strike-inside-hits-400`, V93); the round-41 rule is withdrawn for that case. A ranged launch at a target inside
  is made too (`orc09-missile-inside-hits-600`, V94). For a target whose slot is **released** (V16) no trace exists, so
  the stated rule stays, held by the new open question "A melee strike at a target whose slot is released".
- *Kill credit for an ally's kill* (trace request CB2). Credited: ownership is the only test (traced:
  `human05-ally-kill-hits-1000`, V92, V96). The ally part of the `spells/spells.md` rule is withdrawn.

### Answered in round 78

- *Whether a strike in progress uses the acquisition distance measure, and whether range is checked again at the strike
  point.* The attack orders' range test uses the arrival measure (`orders/orders.md`, "Attacking without moving"), and
  the strike point makes no range test (static study): "Reach and range".
- *Whether a tower's shot is always a missile.* Yes, of the tower type's missile kind (static study): "Towers".
- *Whether haste or slow change the cooldown value set, or how fast it counts down.* Neither (traced for haste:
  `xorc01-death-knight-2100`, V86, V87; static study for slow): "Attack timing (cooldown)". This is change C1.
- *The wait of 1 when the logic runs at a step done.* Consistent with every decision record, hasted (V86) or not
  (frame 0, wait 1), and needed for steady fire every loop (V22). The trace records the end of each cycle, so the
  moment of the test isn't seen directly; this is closed as consistent with every observation, not as a new trace.
- *What a killer whose target was forgotten at the kill does on its next logic run.* The cooldown test comes first
  (`orders/orders.md`, "Attack orders", step 4), so it keeps its order with no target until the cooldown runs out, then
  loses the target by "Arriving and losing the target". `human01-basics` footman 1597 (V13) is given a new command at
  n = 302, 14 cycles after its kill, still inside its cooldown of 26.
- *Demolition squads' damage to the 8 neighbouring tiles.* Specified in `combat/attack-ground-demolish.md`, "The blast".
- *Spell damage* is not an open question: it is in `spells/spells.md` (runes deal a flat 50) and goes through
  "Applying damage".
