//! Scripted runs: a scene's input, fed to the program frame by frame, on a fixed frame time and a fixed wall clock,
//! so that a run comes out the same every time. Development aid (`--script NAME --script-out DIR`, which opens the
//! window and saves what it shows at the scene's shots) and the driver of the frame snapshot (`snapshot`, no window).
//!
//! The input goes in where the system's input comes in: the scene's key, button and wheel events replace the system's
//! in the player's input (`input::Input`) before the frame's input is applied, so keys and buttons behave as typed ones
//! do. The pointer is the scene's ([`Run::pointer`]), put where the system's would be; the mouse lock is off in a
//! scripted run. The window counts as focused.

use glam::Vec2;

use crate::compose::{Frame, H, W};
use crate::core::Core;
use crate::frontend::{AppState, Step};
use crate::input::{Button, Key, KeyEvent, Wheel, WindowGeom};

/// One step of a scene's input.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Let this many frames pass.
    Wait(u32),
    /// Wait until the program shows this state.
    Until(AppState),
    /// Wait until the game has run this many cycles.
    UntilCycle(u32),
    /// Wait until the outcome dialog (victory or defeat) is up.
    UntilOutcome,
    /// The pointer goes to this screen pixel (640 × 480).
    Move(f32, f32),
    Down(Button),
    Up(Button),
    Press(Key),
    Release(Key),
    /// Text typed (each character a press and release with its text).
    Type(&'static str),
    /// Mouse-wheel lines (positive: away from the user).
    Wheel(f32),
    /// The frame is kept under this name; the next commands run from the next frame.
    Shot(&'static str),
}

/// A scripted scene: where the flow starts and the input.
#[derive(Clone, Debug)]
pub struct Scene {
    pub name: &'static str,
    pub start: Vec<Step>,
    pub cmds: Vec<Cmd>,
}

/// The frame time of a scripted run.
pub const FRAME_SECS: f64 = 1.0 / 60.0;

/// The wall clock of a scripted run (seconds since 1970), so its games draw the same seed.
pub const WALL_SECS: u64 = 1_700_000_000;

/// A key pressed and released a frame later.
fn tap(k: Key) -> [Cmd; 3] {
    [Cmd::Press(k), Cmd::Wait(1), Cmd::Release(k)]
}

/// The left button pressed and released a frame later at a screen pixel.
fn click(x: f32, y: f32) -> [Cmd; 4] {
    [
        Cmd::Move(x, y),
        Cmd::Down(Button::Left),
        Cmd::Wait(1),
        Cmd::Up(Button::Left),
    ]
}

/// The scenes, by name.
pub fn scenes() -> Vec<Scene> {
    use Cmd::*;
    use Key as K;
    let mut campaign = vec![
        Move(320.0, 240.0),
        Until(AppState::Title),
        Wait(2),
        Shot("title-fade"),
        Wait(60),
    ];
    campaign.extend([Shot("title")]);
    campaign.extend(tap(K::Space));
    campaign.extend([Until(AppState::MainMenu), Wait(6), Shot("menu-fade"), Wait(20)]);
    campaign.extend([Move(320.0, 254.0), Wait(2), Shot("menu-hover")]);
    campaign.extend(click(320.0, 254.0));
    campaign.extend([
        Until(AppState::NewCampaign),
        Wait(2),
        Move(320.0, 300.0),
        Wait(1),
        Shot("campaigns"),
    ]);
    campaign.extend(click(320.0, 300.0));
    campaign.extend([
        Until(AppState::Act),
        Wait(20),
        Shot("act-text"),
        Wait(100),
        Shot("act-picture"),
    ]);
    campaign.extend(tap(K::Space));
    campaign.extend([
        Until(AppState::Briefing),
        Wait(10),
        Shot("briefing"),
        Wait(240),
        Shot("briefing-later"),
    ]);
    // Campaign flow, "Closing": Continue or Enter closes the briefing.
    campaign.extend(tap(K::Enter));
    campaign.extend([
        Until(AppState::Game),
        Move(400.0, 240.0),
        Wait(1),
        Shot("game-first"),
        Wait(15),
        Shot("game-fade"),
        Wait(60),
        Shot("game"),
    ]);
    // Selecting with a drag over the map view, then a right-click command.
    campaign.extend([
        Move(190.0, 30.0),
        Down(Button::Left),
        Wait(2),
        Move(400.0, 200.0),
        Wait(2),
        Shot("drag"),
        Move(620.0, 460.0),
        Wait(2),
        Up(Button::Left),
        Wait(2),
        Shot("selected"),
        Move(420.0, 120.0),
        Down(Button::Right),
        Wait(1),
        Up(Button::Right),
        Wait(3),
        Shot("commanded"),
        Wait(60),
        Shot("moving"),
    ]);
    // Scrolling with the keys and at the screen edge, and the minimap.
    campaign.extend([
        Press(K::Down),
        Wait(30),
        Release(K::Down),
        Wait(2),
        Shot("scrolled-keys"),
    ]);
    campaign.extend([Move(639.0, 240.0), Wait(40), Shot("scrolled-edge"), Move(400.0, 240.0)]);
    campaign.extend(click(60.0, 70.0));
    campaign.extend([Wait(3), Shot("minimap-click"), UntilCycle(300), Shot("cycle-300")]);
    // The game menu, its options, and back.
    campaign.extend(tap(K::F(10)));
    campaign.extend([Wait(3), Shot("game-menu")]);
    campaign.extend(tap(K::F(5)));
    campaign.extend([Wait(3), Shot("options")]);
    campaign.extend(tap(K::Escape));
    campaign.extend([Wait(3), Shot("back-to-menu")]);
    campaign.extend(tap(K::Escape));
    campaign.extend([Wait(3), Shot("menu-closed"), Wait(30)]);
    // The save dialog: a name typed and saved.
    campaign.extend(tap(K::F(11)));
    campaign.extend([
        Wait(3),
        Shot("save-dialog"),
        Wheel(-1.0),
        Wait(1),
        Type("snap"),
        Wait(2),
        Shot("save-typed"),
    ]);
    campaign.extend(tap(K::Enter));
    campaign.extend([Wait(3), Shot("saved"), Wait(30), Shot("end")]);
    vec![
        Scene {
            name: "campaign",
            start: vec![Step::Title, Step::MainMenu],
            cmds: campaign,
        },
        Scene {
            name: "outcome",
            start: vec![play(0)],
            cmds: outcome(),
        },
        Scene {
            name: "ending",
            start: vec![play(1)],
            cmds: ending(),
        },
        Scene {
            name: "load",
            start: vec![play(0)],
            cmds: load(),
        },
        Scene {
            name: "build",
            start: vec![play(0)],
            cmds: build(),
        },
        Scene {
            name: "carry",
            start: vec![play(0)],
            cmds: carry(),
        },
        Scene {
            name: "flow",
            start: crate::frontend::startup_steps(false, false, true),
            cmds: flow(),
        },
        // Not in the frame snapshot: `jukebox`'s tests run these to check the music at every frame.
        Scene {
            name: "music-quit",
            start: vec![Step::MainMenu],
            cmds: music_quit(),
        },
        Scene {
            name: "music-back",
            start: vec![Step::MainMenu],
            cmds: music_back(),
        },
    ]
}

/// The flow's state changes: the startup videos (one video straight after another), the title, the main menu and the
/// campaign choices and back, a Beyond the Dark Portal campaign with its intro video, act screen and briefing. Then in
/// the game: an unused Escape (nothing happens), the game menu opened and closed with Escape, the `noglues` cheat so
/// that a restart, a chosen mission and a surrender's replay go from one game straight into the next, and the quit to
/// the main menu.
fn flow() -> Vec<Cmd> {
    use Cmd::*;
    use Key as K;
    let mut v = vec![Move(320.0, 240.0), Until(AppState::Video), Wait(60), Shot("logo")];
    v.extend(tap(K::Escape));
    v.extend([Wait(5), Shot("intro-start"), Wait(120), Shot("intro")]);
    v.extend(tap(K::Escape));
    v.extend([Until(AppState::Title), Wait(30), Shot("title")]);
    v.extend(tap(K::Space));
    v.extend([Until(AppState::MainMenu), Wait(25)]);
    v.extend(tap(K::Letter('N')));
    v.extend([Until(AppState::NewCampaign), Wait(2), Shot("campaigns")]);
    v.extend(tap(K::Escape));
    v.extend([Until(AppState::MainMenu), Wait(25), Shot("menu-again")]);
    v.extend(click(320.0, 254.0));
    v.extend([Until(AppState::NewCampaign), Wait(2), Move(320.0, 408.0), Wait(1)]);
    v.extend(click(320.0, 372.0));
    v.extend([Until(AppState::Video), Wait(60), Shot("bdp-intro")]);
    v.extend(tap(K::Escape));
    v.extend([Until(AppState::Act), Wait(40), Shot("act")]);
    v.extend(tap(K::Space));
    v.extend([Until(AppState::Briefing), Wait(10), Shot("briefing")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Game), Move(400.0, 240.0), Wait(30), Shot("game")]);
    // An Escape nothing uses does nothing; in the game menu it returns to the game.
    v.extend(tap(K::Escape));
    v.extend([Wait(3), Shot("escape-unused")]);
    v.extend(tap(K::F(10)));
    v.extend([Wait(3), Shot("game-menu")]);
    v.extend(tap(K::Escape));
    v.extend([Wait(3), Shot("menu-closed")]);
    v.extend(chat("noglues"));
    v.extend([Wait(5), Press(K::ControlLeft), Wait(1)]);
    v.extend(tap(K::Letter('R')));
    v.extend([Release(K::ControlLeft), Wait(3), Shot("restart-confirm")]);
    v.extend(tap(K::Enter));
    v.extend([Wait(30), Shot("restarted")]);
    v.extend(chat("tigerlily"));
    v.extend(chat("human2"));
    v.extend([Wait(30), Shot("chosen-mission")]);
    v.extend(tap(K::F(10)));
    v.extend(tap(K::Letter('E')));
    v.extend([Wait(3), Shot("end-menu"), Move(320.0, 226.0), Wait(1)]);
    v.extend(click(320.0, 226.0)); // Surrender
    v.extend([Wait(3), Shot("surrender-confirm")]);
    v.extend(tap(K::Enter));
    v.extend([UntilOutcome, Wait(5), Shot("defeat")]);
    v.extend(tap(K::Enter));
    v.extend([Wait(60), Shot("replay")]);
    v.extend(tap(K::F(10)));
    v.extend(tap(K::Letter('E')));
    v.extend([Wait(3), Move(320.0, 258.0), Wait(1)]);
    v.extend(click(320.0, 258.0)); // Quit to Main Menu
    v.extend([Wait(3), Shot("quit-confirm")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::MainMenu), Wait(25), Shot("menu-end")]);
    v
}

/// The new-campaign dialog's rows (`frontend`'s menu layout): the Tides of Darkness orc and human campaigns.
const TOD_ORC_ROW: f32 = 264.0;
const TOD_HUMAN_ROW: f32 = 300.0;
/// The main menu's New Campaign button.
const NEW_CAMPAIGN_ROW: f32 = 254.0;

/// From the main menu: New Campaign, the campaign in `row`, its act screen and briefing (closed by Space and Enter), to
/// the game.
fn new_campaign(row: f32) -> Vec<Cmd> {
    use Cmd::*;
    let mut v = vec![Until(AppState::MainMenu), Wait(25)];
    v.extend(click(320.0, NEW_CAMPAIGN_ROW));
    v.extend([Until(AppState::NewCampaign), Wait(2)]);
    v.extend(click(320.0, row));
    v.extend([Until(AppState::Act), Wait(40)]);
    v.extend(tap(Key::Space));
    v.extend([Until(AppState::Briefing), Wait(10)]);
    v.extend(tap(Key::Enter));
    v.extend([Until(AppState::Game), Wait(60)]);
    v
}

/// Ctrl+`k`.
fn ctrl(k: Key) -> Vec<Cmd> {
    let mut v = vec![Cmd::Press(Key::ControlLeft), Cmd::Wait(1)];
    v.extend(tap(k));
    v.extend([Cmd::Release(Key::ControlLeft), Cmd::Wait(3)]);
    v
}

/// The music playtest: the human campaign's first mission, Ctrl+Q and its confirm back to the main menu, then the orc
/// campaign through its act screen and briefing into its game.
fn music_quit() -> Vec<Cmd> {
    use Cmd::*;
    let mut v = vec![Move(320.0, 240.0)];
    v.extend(new_campaign(TOD_HUMAN_ROW));
    v.push(Shot("first-game"));
    v.extend(ctrl(Key::Letter('Q')));
    v.extend(tap(Key::Enter));
    v.extend(new_campaign(TOD_ORC_ROW));
    v.push(Shot("second-game"));
    v
}

/// The other ways back: the campaign choice's Escape, an act screen closed by Escape, a briefing where Escape does
/// nothing (Enter closes it), a restart
/// (Ctrl+R), the game menu's end menu "Quit to Main Menu", and a campaign started with Ctrl held (no act screen or
/// briefing, so the last game's track is still playing when the next game starts).
fn music_back() -> Vec<Cmd> {
    use Cmd::*;
    use Key as K;
    let mut v = vec![Move(320.0, 240.0), Until(AppState::MainMenu), Wait(25)];
    v.extend(click(320.0, NEW_CAMPAIGN_ROW));
    v.extend([Until(AppState::NewCampaign), Wait(2)]);
    v.extend(tap(K::Escape));
    v.extend([Until(AppState::MainMenu), Wait(25)]);
    v.extend(click(320.0, NEW_CAMPAIGN_ROW));
    v.extend([Until(AppState::NewCampaign), Wait(2)]);
    v.extend(click(320.0, TOD_ORC_ROW));
    v.extend([Until(AppState::Act), Wait(40)]);
    v.extend(tap(K::Escape));
    // Escape does nothing on the briefing; Enter closes it.
    v.extend([Until(AppState::Briefing), Wait(10)]);
    v.extend(tap(K::Escape));
    v.extend([Wait(30), Shot("briefing-escape")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Game), Wait(60), Shot("orc-game")]);
    // A restart shows the mission's briefing again.
    v.extend(ctrl(K::Letter('R')));
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Briefing), Wait(10)]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Game), Wait(60), Shot("restarted")]);
    v.extend(tap(K::F(10)));
    v.extend(tap(K::Letter('E')));
    v.extend([Wait(3), Move(320.0, 258.0), Wait(1)]);
    v.extend(click(320.0, 258.0)); // Quit to Main Menu
    v.extend([Wait(3)]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::MainMenu), Wait(25)]);
    v.extend(click(320.0, NEW_CAMPAIGN_ROW));
    v.extend([
        Until(AppState::NewCampaign),
        Wait(2),
        Move(320.0, TOD_HUMAN_ROW),
        Wait(1),
    ]);
    v.extend([Press(K::ControlLeft), Wait(1)]);
    v.extend(click(320.0, TOD_HUMAN_ROW));
    v.extend([
        Until(AppState::Game),
        Release(K::ControlLeft),
        Wait(60),
        Shot("human-game"),
    ]);
    v
}

/// A campaign mission's map, started at once.
fn play(entry: u8) -> Step {
    Step::Play {
        map: crate::campaign::map_name(entry),
        setup: None,
    }
}

/// A line typed into the chat line and sent: Enter opens it, Enter sends it.
fn chat(line: &'static str) -> Vec<Cmd> {
    let mut v = tap(Key::Enter).to_vec();
    v.extend([Cmd::Wait(2), Cmd::Type(line), Cmd::Wait(2)]);
    v.extend(tap(Key::Enter));
    v
}

/// The chat line and the cheats, the victory dialog with its save dialog, the score screen, the next mission's
/// briefing, then the defeat dialog, its score screen and the replay's briefing.
fn outcome() -> Vec<Cmd> {
    use Cmd::*;
    use Key as K;
    let mut v = vec![Move(400.0, 240.0), Until(AppState::Game), Wait(5), Shot("game")];
    v.extend(tap(K::Enter));
    v.extend([Wait(2), Type("unite the"), Wait(2), Shot("chat-typing")]);
    v.extend([Type(" clans"), Wait(1)]);
    v.extend(tap(K::Enter));
    v.extend([UntilOutcome, Wait(5), Shot("victory")]);
    v.extend(click(320.0, 229.0));
    v.extend([Wait(3), Shot("victory-save")]);
    v.extend(tap(K::Escape));
    v.extend([Wait(3), Shot("victory-again")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Score), Wait(20), Shot("score-counting")]);
    v.extend(tap(K::Space));
    v.extend([Wait(3), Shot("score-end")]);
    v.extend(tap(K::Space));
    v.extend([Until(AppState::Briefing), Wait(10), Shot("briefing-next")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Game), Wait(30), Shot("game-next")]);
    v.extend(chat("you pitiful worm"));
    v.extend([UntilOutcome, Wait(5), Shot("defeat")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Score), Wait(20), Shot("score-defeat")]);
    v.extend(tap(K::Space));
    v.extend([Wait(3)]);
    v.extend(tap(K::Space));
    v.extend([Until(AppState::Briefing), Wait(10), Shot("briefing-replay")]);
    v
}

/// The finale cheat: the ending's video (left at once), the finale, the credits, then the main menu and its load
/// dialog with no saved games; then the main menu's Show Credits (C): the credits screen's fade-in and its first page,
/// closed with Space.
fn ending() -> Vec<Cmd> {
    use Cmd::*;
    use Key as K;
    let mut v = vec![Move(400.0, 240.0), Until(AppState::Game), Wait(5)];
    v.extend(chat("there can be only one"));
    v.extend([Until(AppState::Video), Wait(1)]);
    v.extend(tap(K::Escape));
    v.extend([
        Until(AppState::Finale),
        Wait(30),
        Shot("finale"),
        Wait(300),
        Shot("finale-later"),
    ]);
    v.extend(tap(K::Space));
    v.extend([
        Until(AppState::Credits),
        Wait(3),
        Shot("credits-fade"),
        Wait(300),
        Shot("credits"),
    ]);
    v.extend(tap(K::Space));
    v.extend([
        Wait(3),
        Shot("credits-closing"),
        Until(AppState::MainMenu),
        Wait(20),
        Shot("menu"),
    ]);
    v.extend(tap(K::Letter('L')));
    v.extend([Wait(3), Shot("load-empty")]);
    v.extend(tap(K::Escape));
    v.extend([Wait(3), Shot("load-closed")]);
    v.extend(tap(K::Letter('C')));
    v.extend([
        Until(AppState::Credits),
        Wait(3),
        Shot("show-credits-fade"),
        Wait(60),
        Shot("show-credits"),
    ]);
    v.extend(tap(K::Space));
    v.extend([Until(AppState::MainMenu), Wait(20), Shot("menu-after-credits")]);
    v
}

/// A game saved and loaded again from the game menu's load dialog, then quit to the main menu and loaded from there;
/// then a screenshot (Print Screen) and its flash.
fn load() -> Vec<Cmd> {
    use Cmd::*;
    use Key as K;
    let mut v = vec![
        Move(400.0, 240.0),
        Until(AppState::Game),
        UntilCycle(100),
        Shot("before-save"),
    ];
    v.extend(tap(K::F(11)));
    v.extend([Wait(3), Type("one"), Wait(1), Shot("save-dialog")]);
    v.extend(tap(K::Enter));
    v.extend([Wait(3), Shot("saved"), UntilCycle(200), Shot("cycle-200")]);
    v.extend(tap(K::F(12)));
    v.extend([Wait(3), Shot("load-dialog")]);
    v.extend(tap(K::Enter));
    v.extend([
        Wait(5),
        Until(AppState::Game),
        Wait(5),
        Shot("loaded"),
        Wait(60),
        Shot("loaded-later"),
    ]);
    v.extend([Press(K::ControlLeft), Wait(1)]);
    v.extend(tap(K::Letter('Q')));
    v.extend([Release(K::ControlLeft), Wait(3), Shot("quit-confirm")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::MainMenu), Wait(25)]);
    v.extend(tap(K::Letter('L')));
    v.extend([Wait(3), Shot("menu-load")]);
    v.extend(tap(K::Enter));
    v.extend([Until(AppState::Game), Wait(30), Shot("menu-loaded")]);
    v.extend(tap(K::PrintScreen));
    v.extend([Wait(1), Shot("printed"), Wait(5), Shot("flash-over")]);
    v
}

/// A farm placed: a peasant selected, the build commands by their keys, the placement image over good and bad ground,
/// then placed; the chat line edited and cancelled; the load dialog's list moved with the wheel.
fn build() -> Vec<Cmd> {
    use Cmd::*;
    use Key as K;
    let mut v = vec![Move(400.0, 240.0), Until(AppState::Game), Wait(5)];
    v.extend(click(416.0, 318.0));
    v.extend([Wait(3), Shot("peasant")]);
    v.extend(tap(K::Letter('B')));
    v.extend([Wait(3), Shot("build-basic")]);
    v.extend(tap(K::Letter('F')));
    v.extend([Wait(2), Move(330.0, 300.0), Wait(3), Shot("placing")]);
    v.extend([Move(250.0, 120.0), Wait(3), Shot("placing-bad")]);
    v.extend(click(330.0, 300.0));
    v.extend([Wait(3), Shot("placed"), Wait(120), Shot("building")]);
    v.extend(tap(K::Enter));
    v.extend([Wait(2), Type("abc"), Wait(1)]);
    v.extend(tap(K::Backspace));
    v.extend([Wait(2), Shot("chat-edited")]);
    v.extend(tap(K::Escape));
    v.extend([Wait(2), Shot("chat-closed")]);
    v.extend([Wheel(1.0), Wait(1), Wheel(-1.0), Wait(2), Shot("wheel-on-map")]);
    v.extend(tap(K::F(11)));
    v.extend([Wait(3), Type("two"), Wait(1)]);
    v.extend(tap(K::Enter));
    v.extend([Wait(3)]);
    v.extend(tap(K::F(11)));
    v.extend([Wait(3), Type("three"), Wait(1)]);
    v.extend(tap(K::Enter));
    v.extend([Wait(3)]);
    v.extend(tap(K::F(12)));
    v.extend([
        Wait(3),
        Shot("load-list"),
        Wheel(-1.0),
        Wait(2),
        Shot("load-wheel-down"),
    ]);
    v.extend([Wheel(1.0), Wait(2), Shot("load-wheel-up")]);
    v.extend(tap(K::Escape));
    v.extend([Wait(3), Shot("load-closed")]);
    v
}

/// Carried goods and the ground marker: a peasant sent to a tree fells it and walks home carrying lumber, then is given
/// a move on open ground with a right-click, whose marker grows, holds and shrinks.
fn carry() -> Vec<Cmd> {
    use Cmd::*;
    let mut v = vec![Move(400.0, 240.0), Until(AppState::Game), Wait(5)];
    // A peasant sent to a tree fells it and walks home with lumber.
    v.extend(click(416.0, 318.0));
    v.extend([
        Wait(3),
        Move(565.0, 85.0),
        Down(Button::Right),
        Wait(1),
        Up(Button::Right),
    ]);
    v.extend([UntilCycle(1400), Shot("carrying-lumber")]);
    // A move click on open ground: the marker grows, holds and shrinks while the load is carried.
    v.extend([Move(470.0, 330.0), Down(Button::Right), Wait(1), Up(Button::Right)]);
    v.extend([
        Wait(3),
        Shot("marker-grows"),
        Wait(10),
        Shot("marker-holds"),
        Wait(14),
        Shot("marker-shrinks"),
    ]);
    v.extend([Wait(20), Shot("carrying-walk")]);
    // The move command's target mode, then a left click on the minimap inside the view's rectangle: the marker goes at
    // the clicked tile's top-left pixel.
    v.extend(tap(Key::Letter('M')));
    v.extend([Wait(2), Move(90.0, 60.0), Wait(2), Shot("minimap-target")]);
    v.extend(click(90.0, 60.0));
    v.extend([Wait(8), Shot("minimap-target-marker")]);
    v
}

pub fn scene(name: &str) -> Option<Scene> {
    scenes().into_iter().find(|s| s.name == name)
}

/// The run of a scene.
pub struct Script {
    pub scene: Scene,
    next: usize,
    waiting: Option<u32>,
    /// Frames run so far.
    pub frame: u32,
    pointer: Vec2,
    /// The shots asked for this frame.
    pub shots: Vec<&'static str>,
    /// All commands have run.
    pub done: bool,
}

impl Script {
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            next: 0,
            waiting: None,
            frame: 0,
            pointer: Vec2::new(320.0, 240.0),
            shots: Vec::new(),
            done: false,
        }
    }
}

/// A scripted run: the scene, what it saw, where it writes, and the scene's pointer.
pub struct Run {
    pub script: Script,
    pub record: Record,
    pub out: Out,
    /// The scene's pointer in the window's logical points, where the system's would be (set by [`drive`]).
    pointer: Option<(f32, f32)>,
    /// The fingerprints are written.
    finished: bool,
}

impl Run {
    /// A run of `scene` that writes to `out`.
    pub fn new(scene: Scene, out: Out) -> Run {
        if let Err(e) = std::fs::create_dir_all(&out.dir) {
            eprintln!("can't make {}: {e}", out.dir.display());
        }
        Run {
            script: Script::new(scene),
            record: Record::default(),
            out,
            pointer: None,
            finished: false,
        }
    }

    /// The scene's pointer in the window's logical points.
    pub fn pointer(&self) -> Option<(f32, f32)> {
        self.pointer
    }
}

/// One frame of the scene, at the frame's start: runs its commands until one waits, and replaces the system's input
/// with the scene's. `window`: the window, without which the scene waits.
pub(crate) fn drive(c: &mut Core, window: Option<WindowGeom>) {
    c.input.clear_events();
    let Some(w) = window else { return };
    let state = c.states.get();
    let tick = c.sim.as_ref().map(|s| s.world.tick);
    let outcome = c.dialog.is_some();
    let Core {
        script: Some(run),
        input,
        ..
    } = c
    else {
        return;
    };
    let s = &mut run.script;
    s.frame += 1;
    s.shots.clear();
    let key = |key: Key, pressed: bool, text: Option<&str>| KeyEvent {
        key,
        pressed,
        text: text.map(Into::into),
    };
    while let Some(cmd) = s.scene.cmds.get(s.next).cloned() {
        match cmd {
            Cmd::Wait(n) => {
                let left = s.waiting.unwrap_or(n);
                if left > 0 {
                    s.waiting = Some(left - 1);
                    break;
                }
                s.waiting = None;
            }
            Cmd::Until(want) => {
                if state != want {
                    break;
                }
            }
            Cmd::UntilCycle(n) => {
                if tick.is_none_or(|t| t < n) {
                    break;
                }
            }
            Cmd::UntilOutcome => {
                if !outcome {
                    break;
                }
            }
            Cmd::Move(x, y) => s.pointer = Vec2::new(x, y),
            Cmd::Down(b) => input.button(b, true),
            Cmd::Up(b) => input.button(b, false),
            Cmd::Press(k) => input.key(key(k, true, None)),
            Cmd::Release(k) => input.key(key(k, false, None)),
            Cmd::Type(text) => {
                // A key the system doesn't name, as a typed character comes.
                for c in text.chars() {
                    let t = c.to_string();
                    input.key(key(Key::Other(0), true, Some(&t)));
                    input.key(key(Key::Other(0), false, None));
                }
            }
            Cmd::Wheel(y) => input.wheel(Wheel { y, lines: true }),
            // A shot is this frame as the input so far left it: the commands after it wait for the next frame.
            Cmd::Shot(name) => {
                s.shots.push(name);
                s.next += 1;
                break;
            }
        }
        s.next += 1;
    }
    s.done = s.next >= s.scene.cmds.len();
    // The pointer in the window's logical points, where the system's would be (`display::Letterbox`).
    let p = s.pointer;
    let lb = crate::display::Letterbox::fit(w.size);
    let f = w.factor;
    let at = |p: f32, offset: u32| (offset as f32 + p * lb.scale as f32) / f;
    run.pointer = Some((at(p.x, lb.offset.0), at(p.y, lb.offset.1)));
}

/// FNV-1a, 64 bits: the frame and sound fingerprints.
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// What a scripted run saw: the frame fingerprints (one line per run of equal frames), the shots' and the sounds'.
#[derive(Default)]
pub struct Record {
    lines: Vec<String>,
    /// The current run of equal frames: its first frame and fingerprint.
    run: Option<(u32, u64)>,
    last: u32,
    /// The scene has run all its commands.
    ended: bool,
    /// The shots' composed frames, until the window's screenshot of the same frame arrives (with a window).
    pending: Vec<(&'static str, Frame)>,
    /// Shots compared with the window's screenshot: name, differing pixels.
    pub compared: Vec<(&'static str, usize)>,
    /// The mixed sound's fingerprints (the audio snapshot).
    sound: crate::audio::Fingerprints,
    /// The kinds of sound in the order the run first started one of each (`start_lines`).
    kinds: Vec<u8>,
}

impl Record {
    /// The sound lines of the sounds started in frame `n`: in the order of their kinds' first starts in the run, then
    /// in the order they started (the order Bevy's query listed its sound entities in: by archetype, then by spawn).
    fn start_lines(&mut self, n: u32, starts: Vec<crate::audio::Start>) -> Vec<String> {
        use crate::audio::Group;
        let kind = |g: Group| match g {
            Group::Music => 0,
            Group::Effect => 1,
            Group::Screen(_) => 2,
            Group::Act => 3,
        };
        for s in &starts {
            if !self.kinds.contains(&kind(s.group)) {
                self.kinds.push(kind(s.group));
            }
        }
        let mut starts = starts;
        starts.sort_by_key(|s| self.kinds.iter().position(|&k| k == kind(s.group)));
        starts
            .iter()
            .map(|s| format!("sound\t{n}\t{:016x}\t{:.4}\t{:?}", s.hash, s.volume, s.mode))
            .collect()
    }

    fn frame(&mut self, n: u32, hash: u64) {
        match self.run {
            Some((_, h)) if h == hash => {}
            Some((from, h)) => {
                self.lines.push(format!("frames\t{from}-{}\t{h:016x}", self.last));
                self.run = Some((n, hash));
            }
            None => self.run = Some((n, hash)),
        }
        self.last = n;
    }

    /// The fingerprint file: frames, shots and sounds in the order they happened.
    pub fn text(&self, scene: &str) -> String {
        let mut out = format!("# frame and sound fingerprints of the scripted scene `{scene}` (hashes only)\n");
        for l in &self.lines {
            out.push_str(l);
            out.push('\n');
        }
        if let Some((from, h)) = self.run {
            out.push_str(&format!("frames\t{from}-{}\t{h:016x}\n", self.last));
        }
        out
    }
}

/// Where a scripted run writes, and whether it has a window.
#[derive(Clone)]
pub struct Out {
    pub dir: std::path::PathBuf,
    pub windowed: bool,
    /// Without a window: also save the shots' composed frames as pictures.
    pub pictures: bool,
    /// With `pictures`: also every this many frames (0: none).
    pub every: u32,
}

/// Notes the sounds started this frame in the core's mixer: the sound's bytes' fingerprint, its loudness and how it
/// plays.
pub(crate) fn note_sounds(c: &mut Core) {
    let Some(run) = &mut c.script else { return };
    let starts = c.audio.take_starts();
    let lines = run.record.start_lines(run.script.frame, starts);
    run.record.lines.extend(lines);
}

/// What [`record`] asks of the platform.
#[derive(Default)]
pub struct Recorded {
    /// The shots to take the window's own screenshot of (with a window), for [`compare_shot`].
    pub screenshots: Vec<&'static str>,
    /// The scene is done and every screenshot is in: the program ends.
    pub finished: bool,
}

/// Notes the fingerprint of the frame the window shows and keeps the scene's shots; with a window, asks for the
/// window's screenshot of the same frame. Ends the run when the scene is done and every screenshot is in, or when the
/// program is ending on its own (`exiting`: a quit key; the fingerprints so far are kept, marked).
///
/// `frame`: the composed screen as the window shows it at one window pixel per screen pixel, `None` without a window
/// or when the window shows it larger (the run's frames are then the platform's own). `output`: the core's mixer.
pub fn record(run: &mut Run, frame: Option<&Frame>, output: &crate::audio::Output, exiting: bool) -> Recorded {
    let mut asked = Recorded::default();
    let Some(frame) = frame else { return asked };
    let (n, shots, done) = (run.script.frame, run.script.shots.clone(), run.script.done);
    let hash = frame.hash();
    let out = run.out.clone();
    let rec = &mut run.record;
    // Frames after the scene's end (while screenshots come in) aren't part of it.
    if !rec.ended {
        rec.frame(n, hash);
        // The frame's share of the mixed sound: the sounds started in this frame start at its first sample.
        let mut samples = vec![0i16; 2 * crate::audio::frames_for(n as u64 - 1, (1.0 / FRAME_SECS).round() as u64)];
        output.mix(&mut samples);
        rec.sound.add(&samples);
    }
    rec.ended |= done;
    for &name in &shots {
        rec.lines.push(format!("shot\t{n}\t{name}\t{hash:016x}"));
    }
    if !out.windowed && out.pictures {
        for &name in &shots {
            save_png(&out.dir.join(format!("{name}-composed.png")), &frame.rgba);
        }
        if out.every > 0 && n % out.every == 0 {
            save_png(&out.dir.join(format!("frame{n:05}.png")), &frame.rgba);
        }
    }
    if out.windowed {
        for &name in &shots {
            rec.pending.push((name, frame.clone()));
            asked.screenshots.push(name);
        }
    }
    let finished = done && rec.pending.is_empty();
    if (finished || exiting) && !run.finished {
        let scene = run.script.scene.name;
        let mut text = rec.text(scene);
        text.push_str(&written_files(&out.dir.join("save")));
        if !finished {
            text.push_str("# the program ended before the scene did\n");
        }
        let file = out.dir.join(format!("{scene}.txt"));
        if let Err(e) = std::fs::create_dir_all(&out.dir).and_then(|_| std::fs::write(&file, text)) {
            eprintln!("can't write {}: {e}", file.display());
        }
        let file = out.dir.join(format!("{scene}-audio.txt"));
        if let Err(e) = std::fs::write(&file, rec.sound.text(scene)) {
            eprintln!("can't write {}: {e}", file.display());
        }
        if out.windowed {
            let report: String = rec.compared.iter().map(|(n, d)| format!("{n}\t{d}\n")).collect();
            let _ = std::fs::write(out.dir.join(format!("{scene}-compare.txt")), report);
        }
        run.finished = true;
        asked.finished = finished;
    }
    asked
}

/// A fingerprint line for each file the run wrote to its own folder (saved games, screenshots), by name.
fn written_files(dir: &std::path::Path) -> String {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    files
        .iter()
        .map(|name| {
            let bytes = std::fs::read(dir.join(name)).unwrap_or_default();
            format!("file\t{name}\t{:016x}\n", fnv64(&bytes))
        })
        .collect()
}

/// Compares the window's own screenshot of the shot `name` with the composed frame, and saves both (and the
/// differing pixels in white) as pictures in the output folder. `shot`: the screenshot's width, height and RGBA bytes,
/// or `None` if it couldn't be read.
pub fn compare_shot(run: &mut Run, name: &'static str, shot: Option<(usize, usize, &[u8])>) {
    let rec = &mut run.record;
    let Some(i) = rec.pending.iter().position(|(n, _)| *n == name) else {
        return;
    };
    let (_, cpu) = rec.pending.remove(i);
    let Some((w, h, gpu)) = shot else { return };
    let lb = crate::display::Letterbox::fit((w as u32, h as u32));
    let (ox, oy) = (lb.offset.0 as usize, lb.offset.1 as usize);
    let mut diff = vec![0u8; W * H * 4];
    let mut gpu_screen = vec![0u8; W * H * 4];
    let mut differing = 0;
    for y in 0..H {
        for x in 0..W {
            let (gx, gy) = (ox + x, oy + y);
            let g = if gx < w && gy < h {
                let at = (gy * w + gx) * 4;
                [gpu[at], gpu[at + 1], gpu[at + 2], 255]
            } else {
                [0, 0, 0, 255]
            };
            let i = (y * W + x) * 4;
            gpu_screen[i..i + 4].copy_from_slice(&g);
            let c = &cpu.rgba[i..i + 3];
            let same = c == &g[..3];
            if !same {
                differing += 1;
            }
            let v = if same { 0 } else { 255 };
            diff[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    rec.compared.push((name, differing));
    for (suffix, rgba) in [("window", gpu_screen), ("composed", cpu.rgba), ("diff", diff)] {
        save_png(&run.out.dir.join(format!("{name}-{suffix}.png")), &rgba);
    }
}

/// Saves a 640 × 480 RGBA picture as a PNG file (the `png` crate's encoder).
fn save_png(path: &std::path::Path, rgba: &[u8]) {
    let write = || -> Result<(), Box<dyn std::error::Error>> {
        let file = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut enc = png::Encoder::new(file, W as u32, H as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(rgba)?;
        Ok(())
    };
    if let Err(e) = write() {
        eprintln!("can't write {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_scene_has_shots_and_a_unique_name() {
        let all = scenes();
        for s in &all {
            assert!(s.cmds.iter().any(|c| matches!(c, Cmd::Shot(_))), "{}", s.name);
            assert_eq!(all.iter().filter(|o| o.name == s.name).count(), 1);
        }
        for name in ["campaign", "outcome", "ending", "load"] {
            assert!(scene(name).is_some(), "{name}");
        }
    }

    #[test]
    fn fnv_is_the_published_function() {
        // FNV-1a 64 test vectors (the FNV authors' published table).
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
