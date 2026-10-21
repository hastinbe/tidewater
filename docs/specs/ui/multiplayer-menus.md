# UI: multiplayer menus (main menu to the game setup lobby)

- **Status:** approved (Reviewed-by: spec-review session, 2026-10-21; round 89, B-057). The review's rulings are in
  `ERRATA-round89-multiplayer.md`, "Review".
- **Sources:** oracle observation of the original game (round 89: `oracle/multiplayer.py` driving two Wine
  instances over a null-modem link, once in Enhanced and once in Classic; screenshots kept in the round 89 scratch
  folder, not in this repo; round 70 and 73 captures of the IPX error, `ui/screen-and-cursors.md`); inspection of the
  dialog files `rez\multmode.bin`, `rez\mltiplay.bin` (the copy in `War2Patch`), `rez\gamename.bin`, `rez\sdirect.bin`,
  `rez\smodem.bin`, `rez\mdmcnfg.bin`, `rez\conn_dlg.bin`, `rez\viewgame.bin`, `rez\viewtcp.bin`, `rez\viewatlk.bin`,
  `rez\netwait.bin`, `rez\ok.bin`, `rez\bigok.bin`, `rez\copyprot.bin`; the string tables `rez\connect.tbl`,
  `rez\network.tbl` and `rez\gluAll.tbl`; study of the program's menu, connection and game-list code. Each section
  says whether it is **traced** (seen on the running original), **static study** (read from the program, not seen) or
  a **Gap-fill:** (our own choice). Rules: Enhanced first; anything that applies only to Classic rules is labelled
  "Classic mode" and never changes an Enhanced rule. The lobby that follows is in `specs/ui/game-setup-lobby.md`.

## Bottom line

Pressing **Multi Player Game** opens a chain of small screens that all sit on the main menu's picture. In Enhanced the
order is: **Enhanced or Classic**, then **connection method**, then (for Direct Connect and Modem only) a **setup
screen**, then **name entry**, then a **connecting** box, then either the **lobby** (creator) or a **game list** (joiner).
Classic mode asks for the **name first**, before the connection list. **Battle.net is the first row, and only in
Enhanced.** Esc or Previous Menu backs out one step. Only Direct Connect was seen end to end; IPX was seen up to its
error; Modem, the game list and the join wait are static study. Tidewater swaps the rows for Direct IP and Local Network
and shows Battle.net disabled (last section).

## Behaviour

Table entries are numbered from 1: "network entry n" is entry n of `Rez\network.tbl`, "connect entry n" of
`Rez\connect.tbl`, "glue entry n" of `Rez\gluAll.tbl`. Dialog controls are named by their control id in the dialog
file. Rectangles are screen coordinates, (x, y, width x height), unless a section says "in the panel". Every screen
here uses the main menu's picture, palette and orc arrow with no fade (`ui/screen-and-cursors.md`, "Menu screens").

### The flow at a glance (static study; the Enhanced and Direct Connect path traced)

| Step | Enhanced | Classic mode |
| --- | --- | --- |
| 1 | Enhanced or Classic choice | same |
| 2 | Connection method list | **Name entry** |
| 3 | Setup screen (Direct Connect, Modem only) | Connection method list |
| 4 | **Name entry** | Setup screen (Direct Connect, Modem only) |
| 5 | Connecting box; the provider is started | Connecting box; the provider is started |
| 6 | Creator: lobby. Joiner: game list, then join wait, then lobby | same |

Which of step 6 you get depends on the chosen method. Direct Connect and Modem have a **Mode** drop-down:
**Call** makes you the creator (no game list, you go straight to the lobby); **Answer** makes you a joiner (the game
list opens). IPX has no mode; it always opens the game list, where a player either joins a listed game or presses
Create Game.

### 1. Enhanced or Classic choice (`rez\multmode.bin`)

**Traced** (layout, default focus, both buttons); transitions **static study**.

| Control | Id | Rectangle | Hotkey | Result |
| --- | --- | --- | --- | --- |
| Enhanced | 1 | 208, 276, 224 x 28 | E | rules are Enhanced; go to step 2 |
| Classic | 2 | 208, 312, 224 x 28 | C | rules are Classic mode; go to step 2 |
| Previous Menu | -3 | 208, 348, 224 x 28 | P | back to the main menu |

- **Default and focus:** Enhanced has the yellow focus ring when the screen opens. Tab goes Enhanced, Classic,
  Previous Menu. Enter presses the focused button; Esc presses Previous Menu.
- **Asked every time:** the choice is not remembered between visits; the screen opens each time Multi Player Game is
  pressed, with Enhanced focused. The choice sets the rules for the next game (`ui/game-setup-lobby.md`; Classic shows
  the Ready box).
- **Sound:** the button click (see "Sounds and keys" below). **Picture:** the main menu's, no fade.

### 2. Connection method (`rez\mltiplay.bin`)

**Traced** (list contents, order, description text, the Battle.net extras, Classic and Enhanced); remembering the row,
keys and remapping **static study**. The patch folder's copy of the file is the one the game uses; the base copy has a
different, smaller layout and is never shown.

**Layout.**

| Control | Id | Rectangle | Notes |
| --- | --- | --- | --- |
| "Select connection method:" | none | text at 62, 168 | heading, white shade map |
| Method list | 1 | 60, 196, 240 x 176 | rows 18 px high, first row's top at 197; the list has the focus ring |
| Description | 2 | 80, 380, 500 x 60 | two lines, see below |
| "Select Gateway:", gateway drop-down (id 3, 366, 190, 200 x 24), seven help lines (ids 5 to 12, 370, 220 and down, 10 px apart) | 3 to 12 | right half | **Battle.net row only** |
| Connect | -2 | 360, 302, 224 x 28 | hotkey C |
| Previous Menu | -3 | 360, 336, 224 x 28 | hotkey P |

**Rows.** Each row is a connect-entry name; the description under the list is the connect-entry text for the selected
row, with the number of players filled in.

| Rows, top to bottom | Enhanced | Classic mode |
| --- | --- | --- |
| 1 | **Battle.net** (connect entry 10) | **Direct Connect** (connect entry 4) |
| 2 | Direct Connect | Modem (connect entry 3) |
| 3 | Modem | IPX network (connect entry 5) |
| 4 | IPX network | none |

| Row | Description (connect entry) | "Players supported" |
| --- | --- | --- |
| Battle.net | "Players supported: %d\nRequirements: All computers are connected to the Battle.net service." (entry 18) | 8 (traced) |
| Direct Connect | "...Requirements: Two computers directly connected together using a NULL MODEM cable." (entry 12) | 2 (traced) |
| Modem | "...Requirements: Two computers each with their own modem and phone line." (entry 11) | 2 (static study) |
| IPX network | "...Requirements: All computers must be connected to an IPX-compatible network." (entry 13) | 8 (static study) |

The description is drawn at the left of its rectangle, gold, wrapped to its width; the first line is
"Players supported: N" and the rest is the requirement text (traced).

**Behaviour.**

- **Selection:** the list opens with the row you used last, or row 1 the first time (static study). The selected row's
  text is white and the others gold (traced). Moving the selection updates the description at once.
- **Keys in the list (held; trace request MM8):** `ui/screen-and-cursors.md` states that the arrow keys do nothing on
  the menu screens, so by default they don't move the selection. Static study (round 89) reads the list as taking Up
  or Left (one row up), Down or Right (one row down), Page Up and Page Down (one page), Home (the first row) and End
  (the last row), stopping at the ends, with the same keys on the numeric keypad (digits 1 to 9, Num Lock on).
  Clicking a row selects it. Tab goes list, gateway drop-down (Battle.net row only), Connect, Previous Menu.
- **Gateway extras:** the gateway drop-down, its "Select Gateway:" label and the seven help lines are shown only while
  the Battle.net row is selected; Classic mode never shows them (traced). The drop-down opens on "Europe" (traced).
- **Remapping between rules:** if the previous visit used the other rules, the selection moves to the row of the same
  method in the new list; if the method does not exist there (Battle.net moving to Classic), row 1 is selected
  (static study). Example: Modem is row 3 in Enhanced and row 2 in Classic.
- **Connect:** goes to the method's setup screen (section 4) when it has one; then to name entry in Enhanced (section 3)
  and then the connecting box (section 5). **Battle.net:** a different path, not specified here.
- **Previous Menu, Esc:** leave multiplayer, back to the main menu (static study).
- **Sound:** the button click. No fade.

### 3. Name entry (`rez\gamename.bin`)

**Traced** (position, text, focus, the Enhanced order after Connect, and the Direct Connect run); rules **static
study**.

When it appears: **Enhanced**, after Connect (after the setup screen, if the method has one) and before the connecting
box, on **every** Connect press (traced for IPX in round 73 and Direct Connect in round 89). **Classic mode**: once,
straight after choosing Classic and before the connection list; pressing Connect later does not ask again, even after
a failed connection (static study).

| Part | In the panel (288 x 128 at 176, 256) | Screen |
| --- | --- | --- |
| "Enter your name:" | 8, 12, 264 x 18 | 184, 268 |
| Name edit box (id 1) | 38, 38, 190 x 18 | 214, 294 |
| OK (id -2, hotkey O) | 22, 84, 106 x 28 | 198, 340 |
| Cancel (id -3, hotkey C) | 156, 84, 106 x 28 | 332, 340 |

The panel is `Art\o144x64.bmp` (`ui/screen-and-cursors.md`, "Panels of the small dialogs"). It is centred: x = (640 -
width) / 2, y = (320 - height) / 2 + 160.

**Name rules (static study):**

- **Starting text:** the stored name. The first time it is empty, so the box opens with **Anonymous** (network entry
  82), caret at the end (traced). The box is not selected: typing adds to the end. The name you leave stays for the
  next visit and the rest of the run.
- **Length:** 1 to 11 characters. A 12th character is ignored.
- **A leading space is ignored.** Spaces after the first character are allowed, and trailing spaces are kept.
- **OK is disabled while the box is empty**; Enter does nothing then. Backspace deletes the last character.
- **Characters:** any character the game's font can draw. There is **no list of forbidden characters for the player
  name.** Network entry 105 (`*\/:><|&+^?"`) is the filter for **saved-game file names** (`game/save-load.md`), not for
  this box.
- **Ctrl and Alt combinations** are not typed into the box.
- **Duplicate names:** this box does not check them. A name that matches another player's is refused later, in the
  lobby, with network entry 89 ("Duplicate name - pick another"; `ui/game-setup-lobby.md`).
- **OK or Enter:** keep the name and go on. **Cancel or Esc:** Enhanced goes back to the connection list and shuts the
  half-started connection down; Classic mode leaves multiplayer for the main menu (static study).
- **Focus:** OK has the ring and the caret is in the box (traced). Tab: OK, Cancel, the box. Sound: the button click.

### 4. Setup screens

The screen heading is drawn at 106, 206, 400 x 18 (centred, white shade map), the same on both screens.

#### 4a. Direct Connect (`rez\sdirect.bin`)

**Traced** (layout, defaults, Call and Answer, the Connect result); the drop-down contents and the failure of the port
**static study**.

| Control | Id | Rectangle | Notes |
| --- | --- | --- | --- |
| "Direct Connection Setup" | none | 106, 206, 400 x 18 | heading |
| "COM:" label, COM drop-down | 2 | label 20, 336; box 90, 330, 296 x 24 | opens on **1** (traced) |
| "Mode:" label, Mode drop-down | 4 | label 20, 370; box 90, 364, 96 x 24 | rows **Call**, **Answer** (connect entries 2, 1); opens on Call (traced) |
| "Baud:" label, Baud drop-down | 1 | label 218, 370; box 288, 364, 96 x 24 | opens on **9600** (traced) |
| help text | 5 | 36, 228, 570 x 58 | **hidden**; holds connect entry 20, a placeholder ("Direct play requires a lot of stuff...") that the game never shows (traced) |
| Connect | -2 | 406, 336, 224 x 28 | hotkey C; default; has the ring on open (traced) |
| Previous Menu | -3 | 406, 376, 224 x 28 | hotkey P |

- **Mode** is remembered for the next visit within the run. Call or Answer is the only choice that changes what comes
  next (see "The flow at a glance").
- **Baud values (static study):** 2400, 4800 and 9600 are offered. The COM drop-down offers the ports the machine has;
  the exact rows were not seen (Trace request MM2).
- **Connect:** pressing it records the three choices. In **Answer** mode the screen closes and the flow goes on. In
  **Call** mode the original first checks that the game's CD is present and shows a "please insert your CD" box
  (`rez\copyprot.bin`, glue entries 155 and 156) until it is, or the player cancels, which returns to the connection
  list; a copy "spawned" from another player's CD shows network entry 131 ("You must own the full version of Warcraft
  to create a multiplayer game.") and returns to the list. **Tidewater has no CD and no spawned copies, so neither
  check exists.**
- **Previous Menu, Esc:** back to the connection list, nothing started.
- **Sound:** the button click only.

#### 4b. Modem (`rez\smodem.bin`, `rez\mdmcnfg.bin`)

**Static study only**; the screen was not opened (Trace request MM1).

| Control | Id | Rectangle | Notes |
| --- | --- | --- | --- |
| "Modem Setup" heading | none | 106, 206, 400 x 18 | |
| "Dial:" label, phone number edit box | 6 | label 20, 302; box 90, 302, 296 x 18 | holds the last dialled number |
| "Modem:" label, modem drop-down | 2 | label 20, 336; box 90, 330, 296 x 24 | |
| "Mode:" label, Mode drop-down | 4 | label 20, 370; box 90, 364, 96 x 24 | Call, Answer |
| "Baud:" label (id -8), Baud drop-down | 1 | label 218, 370; box 288, 364, 96 x 24 | **hidden** when the screen opens for Modem |
| Modem Setup button | 7 | 406, 336, 224 x 28 | hotkey M; **hidden** when the screen opens for Modem |
| Connect | -2 | 406, 376, 224 x 28 | hotkey C |
| Previous Menu | -3 | 406, 416, 224 x 28 | hotkey P |

- The hidden Baud row and Modem Setup button make the modem setup dialog below unreachable in normal play (static
  study; unconfirmed by a trace).
- **Connect** behaves as for Direct Connect: Answer continues; Call runs the CD check (Tidewater: none).
- **Modem setup dialog (`rez\mdmcnfg.bin`), if it were reachable:** heading "Modem Setup" (120, 186); edit boxes
  Clear (128, 224), Init (128, 258) and Hangup (128, 292), each 296 x 18, ids 1 to 3; radio buttons **Tone** (hotkey T,
  id 4) and **Pulse** (hotkey P, id 5) at x 478; the help text network entry 62 ("For more information about modem
  setup strings, please refer to the 'Warcraft II BNE Read Me' file."); OK (400, 360) and Cancel (400, 400), 224 x 28.
- **Phone book:** none. No dialog file or text for one exists; Modem has only the single "Dial:" box.

#### 4c. IPX network, Battle.net

IPX has **no setup screen**: Connect goes straight on (to name entry in Enhanced, or the connecting box in Classic
mode). Battle.net's row is handled by the Battle.net code and has no screen here (static study).

### 5. Connecting box and provider errors

**Traced** (the IPX error and the box that carries it); the rest **static study**.

When the method has been chosen and the name given, the game starts the provider while showing `rez\conn_dlg.bin`:
text "Connecting..." at 12, 8 and a Cancel button at 92, 92 (hotkey C). The two lines below the heading hold progress
text: network entry 145 "Establishing connection...", 146 "Contacting remote computer...", 147 "Dialtone...". Cancel
changes the heading to "Canceling..." and closes the box; the connection list returns (static study).

A failure is shown in a one-button box on `rez\ok.bin` (text 12, 8, 264 x 80; OK at 92, 92, hotkey O) over the same
panel as the name box, and OK returns to the connection list with nothing started. A taller `rez\bigok.bin` box (OK at
92, 220) is used for the "full version" messages.

| Method | Failure | Message |
| --- | --- | --- |
| IPX network | no IPX network | network entry 3, **"Unable to locate IPX network."** (traced, round 73) |
| Direct Connect | the COM port cannot be opened | network entry 1, "Connection terminated." (static study; Tidewater: not applicable) |
| Modem | no modem driver, or the modem refuses its strings | network entry 1, or entry 39 "Error in modem string." (static study) |
| Modem | the call fails | entries 40 to 44: "Modem error: MODEM NOT RESPONDING", "BUSY", "NO CARRIER", "NO DIALTONE", "NO ANSWER" (text from the table; when each shows is not traced) |
| Modem or Direct Connect | the port is busy, or a port error | entries 143 "The selected serial port is already in use.", 144 "An serial port error occured." (text from the table; not traced) |
| any | the provider cannot start | entries 118 to 121 (provider, standard.snp, events) (text from the table; not traced) |
| any | a spawned copy tries to create | entry 131, above |

### 6. Game list, creating and joining

**Static study.** Only the lobby that follows was traced (Direct Connect); the screens below were on screen too briefly
or not at all in the run (Trace request MM3).

After the connecting box succeeds, a method that supports more than one player moves to the game-list stage.

- **Creator path:** a player in Call mode (Direct Connect, Modem) skips the list. After the CD check (Tidewater: none)
  the game opens the lobby as creator (`ui/game-setup-lobby.md`, creator rows).
- **List path:** a player in Answer mode, or any IPX player, sees the game list. The file is `rez\viewgame.bin` for
  IPX, Direct Connect and Modem. `rez\viewtcp.bin` (adds an address box, "Creator's TCP/IP address (if joining):" and
  "Your TCP/IP address is %d.%d.%d.%d") and `rez\viewatlk.bin` ("Current Appletalk games:", "Choose Zone" button) belong
  to providers this game does not have and are **never opened**.

| Control (`viewgame.bin`) | Id | Rectangle | Notes |
| --- | --- | --- | --- |
| "Current games:" | 8 | 38, 186, 278 x 16 | heading |
| Game list | 6 | 34, 210, 240 x 142 | with a scroll bar at 332, 216, 18 x 128 |
| Game info | 5 | 34, 368, 566 x 98 | for the selected game: network entry 27, "game owner: %s" and "players: %d/%d" on two lines |
| "Waiting for response" | 7 | 390, 230, 192 x 16 | shown while the game asks the network and nothing has answered |
| Join Game | 1 | 378, 262, 224 x 28 | hotkey J; the default button |
| Cancel Join | 2 | 378, 294, 224 x 28 | hotkey C; hidden until a join is under way; shares its place with Create Game |
| Create Game | 3 | 378, 294, 224 x 28 | hotkey C |
| Disconnect | 4 | 378, 326, 224 x 28 | hotkey D |

- **Join Game** opens the **join wait** (`rez\netwait.bin`): the text "Waiting for response from remote system..." at
  52, 332 and a Cancel button at 198, 420, 224 x 28 (no hotkey letter; Esc presses it). On success the lobby opens as
  joiner. **Cancel** or Esc ends the attempt and returns to the list.
- **Create Game** opens the lobby as creator (after the CD check; Tidewater: none).
- **Disconnect** drops the connection and returns to the main menu, not to the connection list.
- **Spawned copies** (Tidewater: none) cannot create from here either: network entry 131.
- **Sound:** the button click only.

**Join failures (static study):**

| What happens | Message |
| --- | --- |
| the creator closes the game while you wait | network entry 85, "Game terminated by creator" |
| no answer in time | network entry 84, "Unable to join - timed out" |

After either message the wait closes and the list returns. Other join refusals (full game, an incompatible version, a
map you do not have, a duplicate name) show in the lobby code's boxes (`ui/game-setup-lobby.md`).

### 7. When a connection fails: summary

| Where you are | Failure | Result |
| --- | --- | --- |
| Name entry | Cancel or Esc | Enhanced: back to the connection list (provider shut down). Classic mode: main menu |
| Setup screen | Previous Menu or Esc | the connection list |
| Connecting box | Cancel, or any error above | message (if an error), then the connection list |
| Game list | Disconnect | main menu |
| Join wait | Cancel, Esc, timeout, creator left | the game list (after the message, if any) |
| Any list or choice screen | Previous Menu or Esc | one step back; the connection list's Previous Menu leaves multiplayer |

### 8. Sounds and keys (all screens above)

- **Click sound (static study):** `Sfx\Button.wav` plays when a **button** is pressed: mouse down on it, or its
  hotkey. It plays again if you hold the button down and slide off and back on. It follows the digital volume setting
  (`ui/sounds.md`) and is silent at volume 0 or with sound off. Nothing else makes a sound: rows, drop-downs, the
  name box and message boxes are silent, and the music is not touched (`ui/music.md`).
- **Hotkeys:** a button's hotkey is the first character of its label text in the dialog file (`ui/screen-and-cursors.md`,
  "Label placement"). A button whose label has no leading hotkey letter (the join wait's Cancel) has none.
- **Enter and Esc:** Enter presses the focused button, or the dialog's default (-2) when a list or box is focused; Esc
  presses -3. Tab order is the dialog file's order (`ui/screen-and-cursors.md`, "Keyboard focus").

### 9. Tidewater slots (Gap-fill)

The original's screens are specified above; Tidewater changes only the contents of the connection list. Direct IP and
Local Network games follow the original's non-Battle.net ("chat-room") behaviour wherever it differs from Battle.net
(DECISIONS.md, 2026-10-21; `net/leave-and-drop.md`).

| Row | Original (Enhanced) | Tidewater |
| --- | --- | --- |
| 1 | Battle.net | **Battle.net, shown disabled:** drawn in the disabled shade, cannot be chosen; the description reads "Battle.net is not available." |
| 2 | Direct Connect | **Direct IP** (takes the Direct Connect slot) |
| 3 | Modem | removed (Direct IP covers it) |
| 4 | IPX network | **Local Network** (takes the IPX slot, last row) |

- **Classic mode** shows the same list without the Battle.net row: Direct IP, then Local Network.
- **First selection:** the first enabled row (Direct IP).
- **Local Network** has no setup screen, as IPX. Its description: "Players supported: 8\nRequirements: All computers must be
  on the same local network." It opens the game list.
- **Direct IP** reuses the Direct Connect setup screen's place and the Mode drop-down's meaning: **Call** hosts a
  game and goes straight to the lobby; **Answer** opens the game list. The COM and Baud rows are replaced by an
  address box (the layout of `viewtcp.bin`'s "Creator's TCP/IP address (if joining):" is a suitable model) and a
  port. These details are our own.
- **Name entry, connecting box, join wait and message boxes** are unchanged. The CD check and the spawned-copy
  message do not exist.
- The wire protocol is Tidewater's own; players see only the screens above.

## Data

- **Dialog files:** `rez\multmode.bin`, `rez\mltiplay.bin` (patch copy), `rez\gamename.bin`, `rez\sdirect.bin`,
  `rez\smodem.bin`, `rez\mdmcnfg.bin`, `rez\conn_dlg.bin`, `rez\viewgame.bin`, `rez\netwait.bin`, `rez\ok.bin`,
  `rez\bigok.bin`, `rez\copyprot.bin`. Not used: `rez\viewtcp.bin`, `rez\viewatlk.bin`.
- **Connect entries used:** names 3 Modem, 4 Direct Connect, 5 IPX network, 10 Battle.net; descriptions 11 (modem), 12
  (direct), 13 (IPX), 18 (Battle.net); Mode rows 2 Call and 1 Answer; 19 and 20 placeholder help (never shown).
- **Network entries used:** 1, 3, 27, 39, 40 to 44, 62, 82, 84, 85, 89, 105 (not used here), 118 to 121, 131, 143 to
  147.
- **Remembered between visits (within a run):** the player name, the selected connection row, the Mode (Call or
  Answer). The rules choice is asked each time.
- **Limits:** name 1 to 11 characters.

## Test vectors

| Input | Expected |
| --- | --- |
| Main menu, Multi Player Game | the choice screen; Enhanced focused (traced) |
| Enhanced, then the list opens | rows Battle.net, Direct Connect, Modem, IPX network; Battle.net selected on first use; the gateway drop-down shows "Europe" (traced) |
| Classic, then OK on the name box | rows Direct Connect, Modem, IPX network; Direct Connect selected; description "Players supported: 2" and the NULL MODEM text; no gateway controls (traced) |
| Classic: choose Classic | the name box comes **before** the list (traced) |
| Enhanced: Direct Connect, Connect | the setup screen first (COM 1, Mode Call, Baud 9600); the name box only after pressing Connect there (traced) |
| Enhanced, row 3 (Modem) selected, Previous Menu, then Classic | Classic opens with Modem (row 2) selected (static study) |
| Enhanced, Battle.net selected, then Classic | Classic opens with Direct Connect (row 1) selected (static study) |
| Name box, type 12 characters | the first 11 stay (static study) |
| Name box, clear all text | OK is disabled (static study) |
| Name box, press Space as the first key | nothing is typed (static study) |
| Name box, type `a*b` | accepted: no forbidden characters (static study) |
| Enhanced, IPX, Connect, name OK on a machine with no IPX | "Unable to locate IPX network."; OK returns to the list (traced) |
| Direct Connect, Mode Call, Connect | no game list; lobby as creator (traced) |
| Direct Connect, Mode Answer, Connect | game list stage, then lobby as joiner when a creator is found (traced end to end; list not seen) |
| Name box in Classic, Cancel | main menu (static study) |
| Name box in Enhanced, Cancel | the connection list (static study) |
| Join wait, Cancel | the game list (static study) |
| Game list, Disconnect | the main menu (static study) |

## Open questions

- **Trace request MM1: the Modem screen.** Choose Modem in both rules. Record which of Baud and Modem Setup are really
  hidden, the Modem drop-down's rows without a modem, and what Connect does with none. Default: as in 4b.
- **Trace request MM2: drop-down contents.** Open the COM and Baud drop-downs on the Direct Connect screen. Record
  every row. Default: Baud 2400, 4800 and 9600; COM the ports present. (Not needed for Tidewater.)
- **Trace request MM3: the game list and join wait.** Hold the joiner's Connect until the list shows (needs a second
  host to be slow to answer, or a pause under a debugger). Record the list rows, the "Waiting for response" text, the
  info box, which button has the ring, and the join wait's text. Default: as in section 6.
- **Trace request MM4: the name box in Enhanced.** Take a screenshot after Connect on the setup screen to confirm the
  box and its default text in the run (traced only for IPX in round 73 and indirectly by the driver clicking OK).
- **Trace request MM5: Esc on the game list and on message boxes.** Neither dialog file has a -3 control for the game
  list; `ok.bin` has only OK. Default: Esc does nothing on the game list; Esc closes a message box like OK.
- **Trace request MM6: which sounds play.** Record the menu click on Enter-on-focused-button, and whether list rows and
  drop-downs click. Default: buttons only, as in section 8.
- **Trace request MM7: double click and Enter in the connection list.** Default: Enter presses Connect; a double click
  only selects.
- **Trace request MM8: arrow keys in a list.** Held: static study says the arrow, Page, Home and End keys move a
  list's selection; `ui/screen-and-cursors.md` says the arrow keys do nothing. Focus the load-game screen's list
  (reachable today, round 75) and the connection list, press Down, End and Up, and record the selection. Default:
  nothing moves.
- **Failure messages for Modem and serial ports** are known only from the string tables; which condition raises which
  entry is not traced. Not needed for Tidewater, which has neither.
- **Battle.net's Connect path** (login, gateway list, chat) is not covered; Tidewater shows the row disabled.
