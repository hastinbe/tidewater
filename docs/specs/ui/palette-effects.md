# UI: colour cycling and palette effects

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 27 (colour cycling
  and palette effects; gap 12)"), the tileset palettes and tiles (`Art\bgs\<era>\*.ppl`, `*.vr4`) and the unit
  sprites (`Art\unit\**\*.grp`), scanned for the cycled palette entries. Not yet confirmed against the running game.

## Behaviour

The game draws with a 256-colour palette. Everything that seems to move on its own in the terrain (water, coasts) and
some sprite parts (the water under ships and platforms, the circle of power, the placement image) moves by changing
palette colours, not by drawing new pictures. This spec covers:

- **colour cycling** during a game;
- **screen fades** to and from black;
- the **screenshot flash**.

The greyscale palette while paused is in `ui/game-menu.md`. Flashing selection boxes are drawn shapes
(`game/rescue.md`, "Flash"), not palette effects.

### Cycles

A game has up to 8 **cycles**. Each cycle has:

- a **range** of palette entries (first to last, sometimes a single entry);
- a **period** P (in steps, below);
- either no colour list (a **rotation** cycle) or a **colour list** of n colours;
- a **countdown**, a **list position** and an **on** state.

When a cycle **turns**:

- **Rotation:** each entry in the range takes the colour of the entry before it, and the first entry takes the colour
  the last one had. Colours move towards higher entry numbers and wrap round.
- **Colour list:** the position goes up by 1, wrapping from n − 1 to 0. Entry first + k of the range then takes list
  colour (position + k) mod n.

Only the changed range is sent to the screen palette.

### Steps

Cycling runs in **steps**. A step happens during a game when all of these hold:

- the colour-cycling preference is on (`ui/game-menu.md`, preferences; on by default);
- the game isn't paused and no game menu is open;
- the game window is active;
- at least 20 ms have passed since the last step (measured with the millisecond clock).

The time of the last step is reset to "now" when the game's main loop starts. Steps are real-time, not game cycles:
the game speed doesn't change them. A step is checked about once per pass of the game's main loop, so steps come every
20 ms or a little more.

In each step:

1. **Ready test.** The step is a **turning step** if some cycle that is on has a countdown of 1 or less, looked at
   before anything changes in this step.
2. **Countdown.** Every cycle that is on, in order 0 to 7, has its countdown lowered by 1. If the result is 2 or less
   and this is a turning step, the cycle turns and its countdown is set to P + 1.

Cycles that are off keep their countdown and position unchanged. When a cycle is set up, its countdown is P and its
position 0.

So a cycle on its own first turns at step P and then every P + 1 steps. When several cycles are on, a cycle that is
due pulls along any other cycle that is within 2 steps of being due, so cycles tend to turn together. The test vectors
give exact sequences.

### Cycles per era

When a new game starts, all 8 cycles are cleared and cycles 0 upward are set up from the map's era (table under
"Data"). Each era's table also says which cycles start on. A loaded game uses the same table but takes the on/off set
from the saved game. Cycle 7 is then set up for the placement image (below). At the end of a game all cycles are
cleared.

- **Cycle 0, the pulse.** A single entry pulsing through 10 shades of red. It starts off. It turns on, for the rest of
  the game, the first time any unit changes owner (a rescue or any other change of owner, `game/rescue.md`), and the
  on state is saved with the game. Its entry is the one used by the circle of power's sprite.
- **Water and coast cycles.** The ranges used by the water and shore tiles rotate.
- **Sprite water.** Entries 240 to 244 (and 5 to 9 in the swamp era) rotate. In the shipped data they appear only in
  sprites: ships, oil platforms, the Dark Portal, rocks, portraits and the death sprite.
- **Winter only.** Entries 205 to 207 rotate. These appear in some sprites (portraits, a few unit and building
  sprites), which therefore shimmer in winter only.
- **Attack mark.** Entry 3 alternates between pure green and pure red. It is the minimap attack mark colour
  (`ui/messages.md`). Until its first turn it keeps its palette colour (pure blue in all four eras).

### Placement image cycle

Cycle 7 is set up at each game start with P = 1, the single entry 10 and a list of 32 shades of red (16 going up
to pure red, then the same 16 going down). It is on only while the player is placing a building: it turns on when
placing starts and off when the building is placed or placing is cancelled, and at the end of the game. Entry 10 is
the colour of the bad site tiles in the placement image (`ui/map-drawing.md`, "Placement image").

On its own it turns every 2 steps, so one full pulse takes 64 steps (about 1.3 s). Because it is due every other
step, while placing it pulls the other cycles along too, so the water moves slightly faster.

### Screen fades

A fade moves every palette entry in a straight line from a start palette to an end palette over T milliseconds:

- A frame's palette is start + floor-toward-zero(t × (end − start) / T) per colour channel, where t is the time in
  ms since the fade began.
- Each frame lasts at least 16 ms; the program waits out the rest.
- t grows by each frame's real length. The last frame is drawn at exactly t = T.
- Nothing else runs during a fade.

A **fade-in** starts from all black, with the new screen already drawn, and ends at the screen's palette. A
**fade-out** goes from the current palette to all black.

| Where | Fade |
| --- | --- |
| Game start | in, 10 ms |
| Game end | out, 1 ms (after the greyscale is removed if the game was paused) |
| Credits | in and out, 100 ms |
| Title | in and out, 250 ms; out 1 ms if a key or click cut it short (`game/campaign-flow.md`) |
| Act screen | 300 ms and 600 ms (`game/campaign-flow.md`) |
| Main menu, custom-scenario and multiplayer screens | in and out, 200 ms |

### Screenshot flash

Pressing Print Screen saves the screen to the first file name `screen000.pcx` to `screen999.pcx` that doesn't exist
yet. If all 1000 exist, nothing happens. While the file is written the palette is shown in negative (each channel v
becomes 255 − v); then it is put back. The player sees a short negative flash.

## Data

### Cycle tables

Rows are cycles 0, 1, 2 and so on. "List" names a colour list below; "—" is a rotation cycle.

| Era | Start on | Cycle: period, entries, list |
| --- | --- | --- |
| Forest | 1, 2, 3, 4 | 0: 4, 202, pulse · 1: 8, 38–47, — · 2: 12, 48–56, — · 3: 20, 240–244, — · 4: 8, 3, mark |
| Winter | 1, 2, 3, 4, 5 | 0: 4, 202, pulse · 1: 10, 40–47, — · 2: 12, 48–54, — · 3: 20, 240–244, — · 4: 20, 205–207, — · 5: 8, 3, mark |
| Wasteland | 1, 2, 3, 4 | 0: 4, 202, pulse · 1: 12, 38–47, — · 2: 8, 64–70, — · 3: 20, 240–244, — · 4: 8, 3, mark |
| Swamp | 1, 2, 3, 4, 5 | 0: 4, 4, pulse · 1: 12, 38–47, — · 2: 8, 88–95, — · 3: 20, 240–244, — · 4: 8, 5–9, — · 5: 8, 3, mark |
| All (cycle 7) | off | 7: 1, 10, placement |

### Colour lists (R, G, B)

| List | Colours, in order (position 0 first) |
| --- | --- |
| Pulse (10) | (255,0,0) (223,0,0) (190,0,0) (158,0,0) (125,0,0) (150,0,0) (174,0,0) (203,0,0) (227,0,0) (255,0,0) |
| Mark (2) | (255,0,0) (0,255,0) |
| Placement (32) | (77,60,60) (89,65,65) (97,69,69) (109,69,69) (121,69,69) (134,69,69) (146,69,69) (158,65,65) (170,60,60) (182,56,56) (195,48,48) (207,40,40) (219,28,28) (231,20,20) (243,8,8) (255,0,0), then the same 16 in reverse order |

### Constants

| Constant | Value |
| --- | --- |
| Step interval | at least 20 ms |
| Countdown after a turn | P + 1 |
| Pulled-along margin | countdown 2 or less after lowering |
| Fade frame | at least 16 ms |
| Screenshot names | `screen000.pcx` to `screen999.pcx`, first unused |

## Test vectors

Steps are numbered from 1, the first step of the game.

| Situation | Expected |
| --- | --- |
| A single cycle with P = 8 on its own | Turns at steps 8, 17, 26, … |
| A single cycle with P = 1 on its own | Turns at steps 1, 3, 5, … |
| Forest, new game, nobody changes owner, not placing | Turns: step 8 cycles 1 and 4; 12: 2; 17: 1, 4; 20: 3; 25: 1, 2, 4; 34: 1, 4; 38: 2; 41: 1, 3, 4; 50: 1, 2, 4; 59: 1, 4; 62: 2, 3; 68: 1, 4 |
| Forest, placing a building from the game's start | Cycle 7 at every odd step; 7: 1, 4; 11: 2; 15: 1, 4; 19: 3; 23: 1, 2, 4; 31: 1, 4; 35: 2; 39: 1, 3, 4 |
| Winter, cycle 0 on from the start (loaded game) | 4: 0; 8: 0, 1, 5; 12: 0, 2; 17: 0, 1, 5; 20: 0, 3, 4; 25: 0, 2, 5; 28: 0, 1; 33: 0, 5; 38: 0, 1, 2; 41: 0, 3, 4, 5 |
| Forest, first turn of cycle 1 | Entry 38 takes entry 47's colour, entry 39 takes entry 38's old colour, and so on |
| Forest, first turn of cycle 4 | Entry 3 becomes pure green; the next turn makes it pure red |
| A rescue in a forest game | From then on, entry 202 pulses: first turn (223, 0, 0), then 190, 158, 125, 150, 174, 203, 227, 255, 255, 223 … |
| Swamp, the circle of power | Its sprite uses entry 4, which pulses once cycle 0 is on |
| Colour cycling preference off | No cycle ever turns; water is still |
| Game paused for 10 s | No steps while paused; cycles carry on from where they were |
| Game speed changed | Step timing unchanged |
| Fade-in over 200 ms, frames of exactly 16 ms | Frames at t = 0, 16, 32, … 192, then 200 (14 frames); a channel going from 0 to 200 shows 96 in the t = 96 frame |
| Print Screen with `screen000.pcx` and `screen001.pcx` present | Writes `screen002.pcx`; the screen shows in negative while it saves |

## Open questions

- **Placement start and end.** The cycle's on and off points are taken from the start and end of placing; whether the
  pulse also stops when placing ends by the builder dying hasn't been traced. Default: off whenever placing ends.
- **Screens that fade over 200 ms.** The main menu, the custom-scenario path and the multiplayer screens are known
  to fade over 200 ms. Which of their sub-screens fade isn't listed. Default: fade in and out when one of these
  screens opens or closes.
- **Fade end.** What is shown at the instant a fade-out ends, before the next screen draws, hasn't been traced.
  Default: black.
- **Screenshot format.** The saved file's exact contents (PCX version, palette: normal or negative) haven't been
  traced. Default: an 8-bit PCX of the 640 × 480 screen with the normal palette.
- **Running game.** None of this has been watched in the running game.
