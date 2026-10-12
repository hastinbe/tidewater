# UI: messages, notices and the status bar

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-12)
- **Sources:** static study of the 2.02 program (research notes under `research/notes/`, "Round 19 (messages,
  notices, status bar)"), the status bar layout `Rez\textbox.bin`, the font `Art\font10x.fnt` (line height), the
  message table `Rez\network.tbl` and the string table `Rez\stat_txt.tbl`. Not yet confirmed against the running game.

## Behaviour

This spec covers the text the game shows over the map view (the **message list** and the **notice line**), the
**status bar** under the map view, what an attack on the local player shows on the minimap, and the Enhanced
**event history** that the space key steps through. Table entries are numbered from 1: "network entry n" is entry n
of `Rez\network.tbl`, "stat_txt entry n" is entry n of `Rez\stat_txt.tbl`.

Positions are screen pixels (the screen layout is in `ui/screen-and-cursors.md`). All times here are **real time**
in milliseconds from the system clock. They don't depend on the game speed and keep running while the game is paused.

### The message area

The message area is drawn over the lower part of the map view. It has 13 text lines, each 20 pixels high (the font's
14-pixel height plus 6). All lines start at x 186 and are clipped at x 614, so the text is at most 428 pixels wide.

| Line | Top y | Use |
| --- | --- | --- |
| List lines 1 to 12 | 194, 214, … 414 | the message list; line 12 is the newest |
| Notice line | 434 | one notice at a time |

Each line has its own text (up to 99 characters), its own text colour and its own expiry time. A line whose text is
empty isn't drawn. The text colours are the font colour codes used elsewhere in the interface (colour 2 for chat,
colour 4 for game and network notices).

**Expiry.** Several times per frame, any line whose expiry time has passed is emptied in place. The other lines
don't move up to fill the gap, so gaps can appear in the list.

**Adding to the list.** A message is added with a colour and a time to live (7000 ms in every use in the game). It is
first split into pieces that each fit the 428-pixel width:

- **Normal break:** a piece ends before the first word that would end past 428 pixels. The space before that word
  stays on the earlier piece; the next piece starts with the word.
- **Long word:** if not even the first word fits, that piece is instead broken before the first character that would
  end past 428 pixels.
- **New line:** a line-feed character always ends a piece.
- A piece longer than 99 characters is cut to 99.

Then each piece in turn scrolls the list up by one line and takes list line 12. List line 1's text is lost, and every
other line moves up with its own colour and expiry time unchanged. Every piece of one message gets the same colour and
expiry time (now + time to live). Character widths come from the font (the width of a space is an open question).

**Showing a notice.** A notice replaces whatever is on the notice line, in colour 4, with an expiry time of now plus
its time to live. A time to live under 1000 ms is replaced by 7000 ms, and every notice in the game asks for less than
1000, so **every notice lasts 7 seconds**. A notice is not split; text that is too wide is clipped at x 614.

**Chat lines.** A chat line from player p is added to the list as "*name*: *text*" in colour 2, where *name* is p's
player name. It wraps like any other message, and only the first piece carries the name.

### What goes where

**Notice line:**

| Notice | Text | Shown when |
| --- | --- | --- |
| Can't place a building here | stat_txt entries 442 to 448 | `ui/placing-buildings.md` |
| Not enough lumber / gold to finish repairs | network entries 73 / 74 | `economy/repair.md` |
| Unit can't be created | network entry 69 | `ui/sounds.md` (unit ready) |
| Subtitles | network entries 75 to 79 | `ui/sounds.md`, "Subtitles" |
| A player paused the game | network entry 86, filled in (with the player's name, at least; see the open questions) | when a pause takes effect |
| A player resumed the game | network entry 87, filled in (see the open questions) | when the game resumes |
| A player is gone (multiplayer) | network entry 63 if that player's two unit counters are both 0 (taken to mean no units and no buildings left); otherwise 64 for one particular leave reason from the session layer, and 65 for any other. Filled in with the player's name | the player leaves the session |
| Chat sent (Classic rules) | network entry 66 | the local player sends a chat line |

**"Under attack"** is the subtitle with network entry 77 (a unit) or 78 (a building). It shows only when the
under-attack sound event happens (`ui/sounds.md`, "Ready, work complete and under attack") and the subtitle rule
allows it. There is no other under-attack text.

**Message list:**

- chat lines received from other players, when the local player is one of the line's recipients;
- under Enhanced rules, the local player's own chat line, added as soon as it is sent (under Classic rules the
  notice "chat sent" is shown instead);
- notices from the network session layer (Battle.net and the multiplayer connection), in colour 4. Which texts these
  are is outside this spec.

### The status bar

The status bar is the strip at x 176 to 623, y 464 to 479. It holds a text area and four cost items. Positions are
from `Rez\textbox.bin` (x relative to the bar's left edge, screen x 176):

| Part | x | Icon (frame of `Art\icons.grp`) |
| --- | --- | --- |
| Text | from 2 | none |
| Gold cost | 290 to 341 | 0 |
| Lumber cost | 342 to 393 | 1 |
| Oil cost | 394 to 445 | 2 |
| Mana cost | 290 to 341 (the same place as gold) | 3 |

A cost item is drawn like an item of the resource bar at the top of the screen (`ui/screen-and-cursors.md`,
"Resource bar"): the 14 × 14 icon at its left edge and the number 18 pixels to the right.

**What sets the text.** The text shows what the pointer is over:

- **A command button:** the button's tooltip text, and its costs by cost kind (`ui/command-buttons.md`, "Tooltips,
  hotkeys and costs"):

  | Cost kind | Gold, lumber, oil items | Mana item |
  | --- | --- | --- |
  | 0 | all hidden | hidden |
  | 1 | the unit or building costs (unit data × 10) | **left as it was** |
  | 2 | the upgrade or research costs | set to 0 |
  | 3 | set to 0 | the spell's button mana cost |

  With cost kind 1 the program doesn't reset the mana item. If the pointer moves straight from a spell button to a
  kind-1 button, without passing over anything else, the old mana cost can stay visible over the gold item (see the
  open questions).
- **A unit icon in the information panel:** that unit's name (`ui/info-panel.md`); no costs.
- **Nothing:** the text is cleared and all four items are set to 0.

When the text is set with cost kind 0, all four items are hidden. Otherwise each item whose value is 0 is hidden and
the others are shown. The text area ends at x 289 when costs can be shown, and at x 341 when they can't.

The text doesn't change while the game is in demo playback, or while a **fixed prompt** is set:

| Mode | Fixed prompt |
| --- | --- |
| Target mode (choosing a target) | network entry 70 |
| Placement mode (placing a building) | network entry 71 |

The prompt is set when the mode starts and cleared when it ends, which also hides the cost items.

**Chat line.** Enter opens a chat line in the status bar. The cost items are hidden and the bar shows a label (from
`Rez\textbox.bin`) and a text field instead. Enter sends the text and Escape closes the line without sending. In
single player nothing is sent. Typed text is first checked as a cheat code, which is a later spec. Who a line is sent
to, and the Enhanced multiplayer version of the bar (`Rez\textbox_BNE.bin`), are also later specs.

### Attacks on the minimap

When an attack hits one of the local player's units or buildings, takes hit points and doesn't kill it (the hits
that `ui/sounds.md` sends to its under-attack check), the unit is **marked**. This happens before that check's other
tests (help timer, quiet counter, critter types), so every such hit marks the unit, even when no sound plays. Nothing
is marked during demo playback.

The minimap is redrawn every 40 game cycles. In a redraw, a marked unit that is drawn on the minimap is drawn in
palette colour 3 instead of its usual colour, and its mark is then cleared. So an attacked unit shows in colour 3 for
one redraw period (40 cycles) after the hit. A unit under steady attack keeps getting marked and stays in colour 3.
Which units the minimap draws and their usual colours are in `ui/minimap.md`.

### Event history (Enhanced rules)

Under Enhanced rules the game remembers the 8 most recent **event locations**, newest first. Each is stored as a
view position: the event's tile minus (7, 7), with a negative coordinate replaced by 0.

- **At the start of a game** all 8 entries are the starting view.
- **A new event** pushes the oldest entry out, becomes entry 1, resets the step to entry 1, and stamps the time. The
  events are those that `ui/sounds.md` says make "the latest event location": a unit ready, work complete, and an
  under-attack sound.
- **The space key** jumps the view to an entry. If more than 1000 ms have passed since the stamp, it starts from entry
  1. It moves the view to the current entry (clamped to the view range) and then steps to the next entry, wrapping
  from 8 back to 1. It also stamps the time.

So pressing space shows the newest event; more presses within a second of each other go back through older events.
The history is saved with a saved game. Under Classic rules the space key does nothing here.

## Data

| Constant | Value |
| --- | --- |
| Message lines | 12 list lines + 1 notice line |
| Line height | 20 pixels |
| First list line top | y 194 |
| Notice line top | y 434 |
| Text left / clip right | x 186 / x 614 (428 pixels) |
| Characters per line | at most 99 |
| Time to live | 7000 ms (list and notices; shorter requests become 7000) |
| Chat colour / notice colour | text colour 2 / 4 |
| Minimap redraw | every 40 game cycles |
| Attack mark colour | palette colour 3 |
| Event history | 8 entries; space-key window 1000 ms; offset (7, 7) |
| Fixed prompts | network entry 70 (target), 71 (placement) |

## Test vectors

| Situation | Expected |
| --- | --- |
| Empty list; message A added at time 0 | A on list line 12 (y 414); emptied at 7000 |
| A added at 0, B at 2000 | A on line 11, B on line 12; A empties at 7000, B at 9000; B stays on line 12 when A goes |
| A at 0, B at 2000; A expires; C added at 8000 | Line 10 empty (A's old place moved up), B on line 11, C on line 12 |
| A 600-pixel message of short words | Two pieces: the first ends at the last word that fits in 428 pixels; it uses list lines 11 and 12 with the same expiry |
| One 500-pixel word | Broken inside the word at the 428-pixel point |
| 13 one-line messages in a row | The first is gone; the last 12 fill lines 1 to 12 |
| Building placement fails over water | stat_txt notice on the notice line at y 434, gone 7 s later |
| A second notice 3 s after the first | Replaces it; gone 7 s after the second |
| Game paused for 10 s right after a notice | The notice is gone when the game resumes |
| Enhanced, player 2 sends "hi" to all, local player is 1 | Sender sees "*name 2*: hi" in colour 2 on list line 12; player 1 sees the same |
| Classic, local player sends a chat line | Network entry 66 on the notice line; nothing added to the sender's list |
| Pointer moves onto a footman training button | Text = the button's tooltip; gold 600, lumber 0 hidden, oil 0 hidden (footman costs from unit data × 10) |
| Pointer moves onto a spell button (cost kind 3) | Mana item shows the spell's cost; gold, lumber and oil hidden |
| Pointer moves off all buttons | Text cleared; all cost items hidden |
| Target mode started | Network entry 70 in the status bar; pointing at buttons doesn't change it |
| Enemy archer hits own footman, help timer running | No sound and no subtitle, but the footman is drawn in colour 3 at the next minimap redraw |
| Own footman killed by one hit | Not marked |
| Enhanced: own barracks attacked at tile (40, 30), then space pressed at once | View moves to (33, 23) |
| Enhanced: an event at tile (3, 50) | Stored as (0, 43) |
| Enhanced: space pressed 3 times, 0.5 s apart, then again 2 s later | Entries 1, 2, 3, then entry 1 again |
| Classic: space pressed | Nothing happens |

## Open questions

- **Text shadow.** The message lines are drawn with a second text setting (probably a shadow colour) that wasn't
  decoded. How it looks belongs to the drawing spec.
- **Unused line.** The program can also draw a 14th line at (420, 24) at the top right, but nothing ever writes to it.
- **Mana item left over.** Whether the leftover mana cost after a spell button can really be seen depends on how often
  the pointer handler runs between buttons. It wasn't checked in the running game.
- **Network notices.** The texts and triggers of the network-layer notices in the list weren't traced. One of the
  network paths passes a player number of 255 to the chat-line format; what it shows wasn't checked.
- **Leave reasons.** Which session events give the reason that shows network entry 64 rather than 65 wasn't
  traced; the names of the two entries suggest leaving versus being dropped.
- **Space key and the chat line.** Whether the space key reaches the event history while a chat line is open wasn't
  checked; most likely the chat line takes the key.
- **Other failure messages.** The mana, return-goods and affordability messages of `ui/command-buttons.md` and
  `economy/build-train.md` (stat_txt entries 435, 436 and 438 to 441) weren't traced to a line. Default: the notice
  line, like the placement messages.
- **Pause notices.** Which values fill in network entries 86 and 87 beyond the player's name, and in which kinds of
  game the notices appear, weren't traced. Default: the player's name only, in every game that can pause.
- **Space width.** How wide a space is in the message font wasn't traced. Default: the font's own space character.
- **Chat recipients and Enhanced multiplayer chat.** These are left to a later spec, as is the cheat check.
