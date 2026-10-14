//! Scripted runs: a scene's input, fed to the program frame by frame, on a fixed frame time and a fixed wall clock,
//! so that a run comes out the same every time. Development aid (`--script NAME --script-out DIR`, which opens the
//! window and saves what it shows at the scene's shots) and the driver of the frame snapshot (`snapshot`, no window).
//!
//! The input goes in where the system's input comes in: the keyboard, mouse-button and wheel messages are replaced by
//! the scene's before Bevy's input systems read them, so keys and buttons behave as typed ones do. The pointer is put
//! in [`frontend::LockedPointer`], which every pointer reading goes through; the mouse lock is off in a scripted run.
//! The window counts as focused.

use bevy::input::keyboard::{Key, KeyboardFocusLost, KeyboardInput, NativeKey, NativeKeyCode};
use bevy::input::mouse::{MouseButtonInput, MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::input::touch::TouchPhase;
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::compose::{Frame, H, W};
use crate::frontend::{AppState, LockedPointer, Step};

/// One step of a scene's input.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// Let this many frames pass.
    Wait(u32),
    /// Wait until the program shows this state.
    Until(AppState),
    /// Wait until the game has run this many cycles.
    UntilCycle(u32),
    /// The pointer goes to this screen pixel (640 × 480).
    Move(f32, f32),
    Down(MouseButton),
    Up(MouseButton),
    Press(KeyCode),
    Release(KeyCode),
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
fn tap(k: KeyCode) -> [Cmd; 3] {
    [Cmd::Press(k), Cmd::Wait(1), Cmd::Release(k)]
}

/// The left button pressed and released a frame later at a screen pixel.
fn click(x: f32, y: f32) -> [Cmd; 4] {
    [
        Cmd::Move(x, y),
        Cmd::Down(MouseButton::Left),
        Cmd::Wait(1),
        Cmd::Up(MouseButton::Left),
    ]
}

/// The scenes, by name.
pub fn scenes() -> Vec<Scene> {
    use Cmd::*;
    use KeyCode as K;
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
    campaign.extend([Move(320.0, 284.0), Wait(2), Shot("menu-hover")]);
    campaign.extend(click(320.0, 284.0));
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
    campaign.extend(tap(K::Space));
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
        Down(MouseButton::Left),
        Wait(2),
        Move(400.0, 200.0),
        Wait(2),
        Shot("drag"),
        Move(620.0, 460.0),
        Wait(2),
        Up(MouseButton::Left),
        Wait(2),
        Shot("selected"),
        Move(420.0, 120.0),
        Down(MouseButton::Right),
        Wait(1),
        Up(MouseButton::Right),
        Wait(3),
        Shot("commanded"),
        Wait(60),
        Shot("moving"),
    ]);
    // Scrolling with the keys and at the screen edge, and the minimap.
    campaign.extend([
        Press(K::ArrowDown),
        Wait(30),
        Release(K::ArrowDown),
        Wait(2),
        Shot("scrolled-keys"),
    ]);
    campaign.extend([Move(639.0, 240.0), Wait(40), Shot("scrolled-edge"), Move(400.0, 240.0)]);
    campaign.extend(click(60.0, 70.0));
    campaign.extend([Wait(3), Shot("minimap-click"), UntilCycle(300), Shot("cycle-300")]);
    // The game menu, its options, and back.
    campaign.extend(tap(K::F10));
    campaign.extend([Wait(3), Shot("game-menu")]);
    campaign.extend(tap(K::F5));
    campaign.extend([Wait(3), Shot("options")]);
    campaign.extend(tap(K::Escape));
    campaign.extend([Wait(3), Shot("back-to-menu")]);
    campaign.extend(tap(K::Escape));
    campaign.extend([Wait(3), Shot("menu-closed"), Wait(30)]);
    // The save dialog: a name typed and saved.
    campaign.extend(tap(K::F11));
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
    vec![Scene {
        name: "campaign",
        start: vec![Step::Title, Step::MainMenu],
        cmds: campaign,
    }]
}

pub fn scene(name: &str) -> Option<Scene> {
    scenes().into_iter().find(|s| s.name == name)
}

/// The run of a scene.
#[derive(Resource)]
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

/// The scripted input runs before Bevy's input systems, and the systems that read the pointer after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScriptInput;

pub fn plugin(app: &mut App) {
    app.configure_sets(PreUpdate, ScriptInput.before(InputSystems))
        .add_systems(PreUpdate, drive.in_set(ScriptInput).run_if(resource_exists::<Script>));
}

/// One frame of the scene: runs its commands until one waits, and replaces the system's input with the scene's.
#[allow(clippy::too_many_arguments)]
fn drive(
    mut script: ResMut<Script>,
    state: Res<State<AppState>>,
    sim: Option<Res<crate::Sim>>,
    window: Query<(Entity, &Window), With<PrimaryWindow>>,
    mut keys: ResMut<Messages<KeyboardInput>>,
    mut lost: ResMut<Messages<KeyboardFocusLost>>,
    mut buttons: ResMut<Messages<MouseButtonInput>>,
    mut wheel: ResMut<Messages<MouseWheel>>,
    mut motion: ResMut<Messages<MouseMotion>>,
    mut locked: ResMut<LockedPointer>,
    hud: Option<ResMut<crate::Hud>>,
) {
    keys.clear();
    lost.clear();
    buttons.clear();
    wheel.clear();
    motion.clear();
    let Ok((win, w)) = window.single() else { return };
    let s = &mut *script;
    s.frame += 1;
    s.shots.clear();
    let key = |key_code: KeyCode, state: ButtonState, text: Option<&str>| KeyboardInput {
        key_code,
        logical_key: Key::Unidentified(NativeKey::Unidentified),
        state,
        text: text.map(Into::into),
        repeat: false,
        window: win,
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
                if *state.get() != want {
                    break;
                }
            }
            Cmd::UntilCycle(n) => {
                if sim.as_ref().is_none_or(|sim| sim.world.tick < n) {
                    break;
                }
            }
            Cmd::Move(x, y) => s.pointer = Vec2::new(x, y),
            Cmd::Down(b) | Cmd::Up(b) => {
                let state = if matches!(cmd, Cmd::Down(_)) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                buttons.write(MouseButtonInput {
                    button: b,
                    state,
                    window: win,
                });
            }
            Cmd::Press(k) => {
                keys.write(key(k, ButtonState::Pressed, None));
            }
            Cmd::Release(k) => {
                keys.write(key(k, ButtonState::Released, None));
            }
            Cmd::Type(text) => {
                for c in text.chars() {
                    let t = c.to_string();
                    keys.write(key(
                        KeyCode::Unidentified(NativeKeyCode::Unidentified),
                        ButtonState::Pressed,
                        Some(&t),
                    ));
                    keys.write(key(
                        KeyCode::Unidentified(NativeKeyCode::Unidentified),
                        ButtonState::Released,
                        None,
                    ));
                }
            }
            Cmd::Wheel(y) => {
                wheel.write(MouseWheel {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y,
                    window: win,
                    phase: TouchPhase::Moved,
                });
            }
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
    let size = w.physical_size();
    let lb = crate::display::Letterbox::fit((size.x, size.y));
    let f = w.scale_factor();
    let at = |p: f32, offset: u32| (offset as f32 + p * lb.scale as f32) / f;
    locked.0 = Some(Vec2::new(at(s.pointer.x, lb.offset.0), at(s.pointer.y, lb.offset.1)));
    if let Some(mut hud) = hud {
        hud.locked = *locked;
    }
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
#[derive(Resource, Default)]
pub struct Record {
    lines: Vec<String>,
    /// The current run of equal frames: its first frame and fingerprint.
    run: Option<(u32, u64)>,
    last: u32,
    /// The scene has run all its commands.
    ended: bool,
    /// The shots' composed frames, until Bevy's screenshot of the same frame arrives (with a window).
    pending: Vec<(&'static str, Frame)>,
    /// Shots compared with Bevy's screenshot: name, differing pixels.
    pub compared: Vec<(&'static str, usize)>,
}

impl Record {
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
#[derive(Resource, Clone)]
pub struct Out {
    pub dir: std::path::PathBuf,
    pub windowed: bool,
    /// Without a window: also save the shots' composed frames as pictures.
    pub pictures: bool,
    /// With `pictures`: also every this many frames (0: none).
    pub every: u32,
}

/// Notes the sounds started this frame: the sound's bytes' fingerprint, its loudness and whether it loops.
fn note_sounds(
    script: Res<Script>,
    started: Query<(&AudioPlayer, &PlaybackSettings), Added<AudioPlayer>>,
    sources: Res<Assets<AudioSource>>,
    mut rec: ResMut<Record>,
) {
    for (player, settings) in &started {
        let bytes = sources.get(&player.0).map_or(0, |s| fnv64(&s.bytes));
        let volume = match settings.volume {
            bevy::audio::Volume::Linear(v) => v,
            v => v.to_linear(),
        };
        rec.lines.push(format!(
            "sound\t{}\t{bytes:016x}\t{volume:.4}\t{:?}",
            script.frame, settings.mode
        ));
    }
}

/// Composes the frame the window shows, notes its fingerprint and keeps the scene's shots; with a window, asks Bevy
/// for its screenshot of the same frame. Ends the run when the scene is done and every screenshot is in.
fn record(world: &mut World) {
    let Some(frame) = crate::frame::compose_world(world) else {
        return;
    };
    let (n, shots, done) = {
        let s = world.resource::<Script>();
        (s.frame, s.shots.clone(), s.done)
    };
    let hash = frame.hash();
    let out = world.resource::<Out>().clone();
    let mut rec = world.resource_mut::<Record>();
    // Frames after the scene's end (while screenshots come in) aren't part of it.
    if !rec.ended {
        rec.frame(n, hash);
    }
    rec.ended |= done;
    for &name in &shots {
        rec.lines.push(format!("shot\t{n}\t{name}\t{hash:016x}"));
    }
    if !out.windowed && out.pictures {
        for &name in &shots {
            save_png(&out.dir.join(format!("{name}-composed.png")), frame.rgba.clone());
        }
        if out.every > 0 && n % out.every == 0 {
            save_png(&out.dir.join(format!("frame{n:05}.png")), frame.rgba.clone());
        }
    }
    if out.windowed {
        for &name in &shots {
            world.resource_mut::<Record>().pending.push((name, frame.clone()));
            world
                .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
                .observe(compare_shot(name));
        }
    }
    let finished = done && world.resource::<Record>().pending.is_empty();
    // The program may end on its own (a quit key): the fingerprints so far are kept, marked.
    let ended = !world.resource::<Messages<AppExit>>().is_empty();
    if (finished || ended) && !world.contains_resource::<Finished>() {
        let rec = world.resource::<Record>();
        let scene = world.resource::<Script>().scene.name;
        let mut text = rec.text(scene);
        if !finished {
            text.push_str("# the program ended before the scene did\n");
        }
        let file = out.dir.join(format!("{scene}.txt"));
        if let Err(e) = std::fs::create_dir_all(&out.dir).and_then(|_| std::fs::write(&file, text)) {
            eprintln!("can't write {}: {e}", file.display());
        }
        if out.windowed {
            let report: String = rec.compared.iter().map(|(n, d)| format!("{n}\t{d}\n")).collect();
            let _ = std::fs::write(out.dir.join(format!("{scene}-compare.txt")), report);
        }
        world.insert_resource(Finished);
        if finished {
            world.write_message(AppExit::Success);
        }
    }
}

#[derive(Resource)]
struct Finished;

/// Compares Bevy's screenshot of a shot's frame with the composed frame, and saves both (and the differing pixels in
/// white) as pictures in the output folder.
fn compare_shot(
    name: &'static str,
) -> impl FnMut(On<bevy::render::view::screenshot::ScreenshotCaptured>, ResMut<Record>, Res<Out>) {
    move |ev, mut rec, out| {
        let Some(i) = rec.pending.iter().position(|(n, _)| *n == name) else {
            return;
        };
        let (_, cpu) = rec.pending.remove(i);
        let Ok(img) = ev.image.clone().try_into_dynamic() else {
            return;
        };
        let gpu = img.to_rgba8();
        let (w, h) = (gpu.width() as usize, gpu.height() as usize);
        let lb = crate::display::Letterbox::fit((w as u32, h as u32));
        let (ox, oy) = (lb.offset.0 as usize, lb.offset.1 as usize);
        let mut diff = vec![0u8; W * H * 4];
        let mut gpu_screen = vec![0u8; W * H * 4];
        let mut differing = 0;
        for y in 0..H {
            for x in 0..W {
                let (gx, gy) = (ox + x, oy + y);
                let g = if gx < w && gy < h {
                    let p = gpu.get_pixel(gx as u32, gy as u32).0;
                    [p[0], p[1], p[2], 255]
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
        for (suffix, rgba) in [("bevy", gpu_screen), ("composed", cpu.rgba), ("diff", diff)] {
            save_png(&out.dir.join(format!("{name}-{suffix}.png")), rgba);
        }
    }
}

fn save_png(path: &std::path::Path, rgba: Vec<u8>) {
    let img = crate::frontend::rgba_image(crate::scene::Picture {
        width: W as u32,
        height: H as u32,
        rgba,
    });
    if let Ok(d) = img.try_into_dynamic()
        && let Err(e) = d.save(path)
    {
        eprintln!("can't write {}: {e}", path.display());
    }
}

/// Sets a scripted run up: the scene's input, a fixed frame time, the fingerprints.
pub fn setup(app: &mut App, scene: Scene, out: Out) {
    if let Err(e) = std::fs::create_dir_all(&out.dir) {
        eprintln!("can't make {}: {e}", out.dir.display());
    }
    app.insert_resource(Script::new(scene))
        .insert_resource(out)
        .init_resource::<Record>()
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(FRAME_SECS),
        ))
        .insert_resource(crate::WallClock { fixed: Some(WALL_SECS) })
        .add_systems(Last, (note_sounds, record).chain().after(crate::shell::ComposeScreen));
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
        assert!(scene("campaign").is_some());
    }

    #[test]
    fn fnv_is_the_published_function() {
        // FNV-1a 64 test vectors (the FNV authors' published table).
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
