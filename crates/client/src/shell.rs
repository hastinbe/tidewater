//! The Bevy shell: the platform side of the client core (`core`, `docs/design/client-core.md` step 9). It opens the
//! window, hands the core the system's input and the real time once a frame, shows the frame the core composed at the
//! window's whole-number scale with black around it (`display::Letterbox`), plays the core's mixer on the sound device,
//! holds the system's pointer (the mouse lock, `display`) and sizes the window to the monitor. Everything else is the
//! core's, and engine-neutral.
//!
//! A Bevy frame: `First` hands the window's key, button and wheel events to the core's input; `PreUpdate`, after
//! Bevy's input systems, runs [`Core::advance`](crate::core::Core::advance) and applies what the frame asked of the
//! window (title, system pointer, exit, size); `Last` writes the settings on a normal exit, places the screen's picture
//! and records a scripted run's frame (`script`).

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::ecs::message::MessageUpdateSystems;
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{AccumulatedMouseMotion, MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::compose::{self, Frame};
use crate::core::{Core, Platform};
use crate::display;
use crate::input::{Button, Buttons, Key, KeyEvent, Wheel, WindowGeom};

/// The client core, as the Bevy app's one resource of its own.
#[derive(Resource, Deref, DerefMut)]
struct CoreRes(Core);

/// Runs the core in a Bevy app until it exits. `headless`: a scripted run without a window (no window system, no
/// sound device, no GPU); a scripted run with a window mixes its sound itself (`script`) and plays none.
pub fn run(core: Core, title: String, ui_scale: Option<u32>, headless: bool) {
    let scripted = core.script.is_some();
    // Until `size_window` knows the monitor: the explicit size, or the plain 640 × 480 points.
    let points = ui_scale.unwrap_or(1);
    let mut resolution = bevy::window::WindowResolution::new(display::SCREEN.0 * points, display::SCREEN.1 * points);
    if scripted {
        // One physical pixel per screen pixel whatever the display, so the screen fills the window exactly.
        resolution = resolution.with_scale_factor_override(1.0);
    }
    let mut window = Window {
        title,
        resolution,
        ..default()
    };
    if scripted {
        // A scripted window doesn't take the keyboard from what the user is doing (its input is the scene's), and
        // stays on top, since a covered window may not be drawn and its screenshots come out black.
        window.focused = false;
        window.window_level = bevy::window::WindowLevel::AlwaysOnTop;
    }
    let plugins = DefaultPlugins.set(ImagePlugin::default_nearest());
    let mut app = App::new();
    if headless {
        app.add_plugins(
            plugins
                .set(WindowPlugin {
                    primary_window: Some(window),
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    close_when_requested: false,
                    ..default()
                })
                .set(bevy::render::RenderPlugin {
                    render_creation: bevy::render::settings::WgpuSettings {
                        backends: None,
                        ..default()
                    }
                    .into(),
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>()
                .disable::<bevy::audio::AudioPlugin>(),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::ZERO));
    } else if scripted {
        app.add_plugins(
            plugins
                .set(WindowPlugin {
                    primary_window: Some(window),
                    ..default()
                })
                .disable::<bevy::audio::AudioPlugin>(),
        );
    } else {
        app.add_plugins(plugins.set(WindowPlugin {
            primary_window: Some(window),
            ..default()
        }));
        // The device pulls the core's mixer.
        use bevy::audio::AddAudioSource;
        app.add_audio_source::<CoreStream>().add_systems(Startup, start_stream);
    }
    if scripted {
        // A fixed frame time, so a run comes out the same every time.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(crate::script::FRAME_SECS),
        ));
    }
    app.insert_resource(ClearColor(Color::BLACK))
        .insert_resource(WindowSizing {
            explicit: ui_scale.is_some(),
            frames: 0,
        })
        .insert_resource(LockedPointer(None))
        .insert_resource(CoreRes(core))
        .add_systems(Startup, spawn_screen)
        .add_systems(First, take_system_input.after(MessageUpdateSystems))
        .add_systems(PreUpdate, run_core.after(bevy::input::InputSystems))
        .add_systems(Last, (store_settings, show_screen, record).chain());
    app.run();
}

// ---------------------------------------------------------------------------------------------------------------
// The frame

/// The window as the core sees it (`core::Platform`).
struct BevyWindow<'a> {
    window: Option<(&'a mut Window, &'a mut CursorOptions)>,
    locked: &'a mut LockedPointer,
    motion: Vec2,
    lock_enabled: bool,
}

impl Platform for BevyWindow<'_> {
    fn window(&self) -> Option<WindowGeom> {
        self.window.as_ref().map(|(w, _)| WindowGeom {
            size: (w.physical_width(), w.physical_height()),
            factor: w.scale_factor(),
        })
    }

    fn pointer(&mut self, keys: &Buttons<Key>, game_pointer: bool) -> (Option<(f32, f32)>, bool) {
        let Some((window, cursor)) = self.window.as_mut() else {
            return (None, false);
        };
        mouse_lock(
            window,
            cursor,
            self.locked,
            self.motion,
            keys,
            game_pointer,
            self.lock_enabled,
        );
        let at = self.locked.0.or(window.cursor_position());
        (at.map(|p| (p.x, p.y)), window.focused)
    }
}

/// One frame of the core, and what it asked of the window.
#[allow(clippy::too_many_arguments)]
fn run_core(
    mut core: ResMut<CoreRes>,
    real: Res<Time<Real>>,
    mut windows: Query<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut locked: ResMut<LockedPointer>,
    motion: Res<AccumulatedMouseMotion>,
    mut sizing: ResMut<WindowSizing>,
    monitors: Query<&bevy::window::Monitor, With<bevy::window::PrimaryMonitor>>,
    mut images: ResMut<Assets<Image>>,
    screen: Option<Res<Screen>>,
    mut exit: MessageWriter<AppExit>,
) {
    let lock_enabled = !core.opts.no_mouse_lock;
    let mut platform = BevyWindow {
        window: windows.single_mut().ok().map(|(w, c)| (w.into_inner(), c.into_inner())),
        locked: &mut locked,
        motion: motion.delta,
        lock_enabled,
    };
    core.advance(real.delta(), &mut platform);
    if std::mem::take(&mut core.exit) {
        exit.write(AppExit::Success);
    }
    if let Some(mut img) = screen.and_then(|s| images.get_mut(&s.image)) {
        img.data = Some(core.frame.rgba.clone());
    }
    let Ok((mut window, mut cursor)) = windows.single_mut() else {
        return;
    };
    if let Some(title) = core.title.take() {
        window.title = title;
    }
    if cursor.visible != core.cursor_visible {
        cursor.visible = core.cursor_visible;
    }
    size_window(&mut sizing, monitors.single().ok(), &mut window);
    fit_screen(&mut window, core.script.is_some());
}

/// "Saving": the settings are written when the program exits normally, not on an error exit.
fn store_settings(mut exits: MessageReader<AppExit>, core: Res<CoreRes>) {
    if exits.read().any(|e| *e == AppExit::Success) {
        core.store_settings();
    }
}

/// A scripted run notes the frame the window shows (at one window pixel per screen pixel), asks for the window's
/// screenshots of its shots, and ends the program when the scene is done.
fn record(
    mut commands: Commands,
    mut core: ResMut<CoreRes>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut exits: ResMut<Messages<AppExit>>,
) {
    let c = &mut core.0;
    let Some(run) = c.script.as_mut() else { return };
    let shown = windows
        .single()
        .ok()
        .filter(|w| display::Letterbox::fit((w.physical_width(), w.physical_height())).scale == 1)
        .map(|_| &c.frame);
    let output = c.audio.output();
    let asked = crate::script::record(run, shown, &output, !exits.is_empty());
    for name in asked.screenshots {
        commands.spawn(Screenshot::primary_window()).observe(
            move |ev: On<ScreenshotCaptured>, mut core: ResMut<CoreRes>| {
                let Some(run) = core.script.as_mut() else { return };
                let img = ev.image.clone().try_into_dynamic().ok().map(|i| i.to_rgba8());
                let shot = img
                    .as_ref()
                    .map(|i| (i.width() as usize, i.height() as usize, i.as_raw().as_slice()));
                crate::script::compare_shot(run, name, shot);
            },
        );
    }
    if asked.finished {
        exits.write(AppExit::Success);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The screen

/// The render layer only the screen's own camera and picture are on.
const SCREEN_LAYER: usize = 30;

/// The composed screen's picture for the window.
#[derive(Resource)]
struct Screen {
    image: Handle<Image>,
}

#[derive(Component)]
struct ScreenCamera;

#[derive(Component)]
struct ScreenSprite;

fn spawn_screen(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = images.add(Image::new(
        bevy::render::render_resource::Extent3d {
            width: compose::W as u32,
            height: compose::H as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        Frame::black().rgba,
        bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    ));
    commands.spawn((
        Camera2d,
        Camera {
            clear_color: ClearColorConfig::Custom(Color::BLACK),
            ..default()
        },
        RenderLayers::layer(SCREEN_LAYER),
        ScreenCamera,
    ));
    commands.spawn((
        Sprite::from_image(image.clone()),
        RenderLayers::layer(SCREEN_LAYER),
        ScreenSprite,
    ));
    commands.insert_resource(Screen { image });
}

fn letterbox(window: &Window) -> display::Letterbox {
    display::Letterbox::fit((window.physical_width(), window.physical_height()))
}

/// Puts the screen's picture on the screen's place in the window, at the whole-number scale.
fn show_screen(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cam: Query<&mut Camera, With<ScreenCamera>>,
    mut sprite: Query<&mut Transform, With<ScreenSprite>>,
) {
    let (Ok(window), Ok(mut cam), Ok(mut tf)) = (windows.single(), cam.single_mut(), sprite.single_mut()) else {
        return;
    };
    let lb = letterbox(window);
    let (x, y, w, h) = lb.screen();
    let (pos, size) = (UVec2::new(x, y), UVec2::new(w.max(1), h.max(1)));
    if cam
        .viewport
        .as_ref()
        .is_none_or(|v| v.physical_position != pos || v.physical_size != size)
    {
        cam.viewport = Some(Viewport {
            physical_position: pos,
            physical_size: size,
            ..default()
        });
    }
    let k = lb.scale as f32 / window.scale_factor();
    let scale = Vec3::new(k, k, 1.0);
    if tf.scale != scale {
        tf.scale = scale;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The window

/// The window's size: with `--ui-scale` the size asked for; otherwise it opens at the plain screen size (640 × 480
/// points) and then becomes the largest whole number of physical pixels per screen pixel that fits the monitor,
/// centred on it.
#[derive(Resource)]
struct WindowSizing {
    explicit: bool,
    /// Frames spent waiting for the monitor.
    frames: u32,
}

/// Frames to wait for the monitor before keeping the starting size.
const MONITOR_WAIT: u32 = 30;

fn size_window(sizing: &mut WindowSizing, monitor: Option<&bevy::window::Monitor>, window: &mut Window) {
    if sizing.explicit || sizing.frames > MONITOR_WAIT {
        return;
    }
    sizing.frames += 1;
    let Some(m) = monitor else { return };
    let scale = display::monitor_scale((m.physical_width, m.physical_height), m.scale_factor);
    let (w, h) = display::window_size(scale);
    // In physical pixels: the window's own scale factor may not be the monitor's yet.
    window.resolution.set_physical_resolution(w, h);
    window.position = WindowPosition::Centered(bevy::window::MonitorSelection::Primary);
    sizing.frames = MONITOR_WAIT + 1;
}

/// Keeps the window at least the 640 × 480 screen in physical pixels.
fn fit_screen(window: &mut Window, scripted: bool) {
    let f = window.scale_factor();
    let (min_w, min_h) = (display::SCREEN.0 as f32 / f, display::SCREEN.1 as f32 / f);
    let c = window.resize_constraints;
    // A scripted run's window keeps its size: the system takes the minimum in its own points, which a scale
    // override doesn't change, and would double the window on a display with two pixels per point.
    if !scripted && (c.min_width != min_w || c.min_height != min_h) {
        window.resize_constraints.min_width = min_w;
        window.resize_constraints.min_height = min_h;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The pointer

/// The pointer while the system's is locked and held still, in the window's logical points.
#[derive(Resource)]
struct LockedPointer(Option<Vec2>);

/// How the system holds the pointer while it is locked (`display`, "Mouse lock"): macOS can't confine it to the
/// window, so there it is held still and our pointer moves by the mouse's motion.
const LOCK_MODE: CursorGrabMode = if cfg!(target_os = "macos") {
    CursorGrabMode::Locked
} else {
    CursorGrabMode::Confined
};

/// Locks the pointer to the window during a game and frees it otherwise (`display::lock_wanted`). With the system
/// pointer held still, moves the locked pointer by the mouse's motion and keeps it on the screen; when the lock ends,
/// puts the system pointer where ours was.
fn mouse_lock(
    window: &mut Window,
    cursor: &mut CursorOptions,
    locked: &mut LockedPointer,
    motion: Vec2,
    keys: &Buttons<Key>,
    game_pointer: bool,
    enabled: bool,
) {
    let want = display::lock_wanted(display::LockInput {
        enabled,
        // The game menus and the outcome dialog are unlocked; the screen draws the arrow there.
        in_game: game_pointer,
        focused: window.focused,
        held_free: keys.any_pressed([Key::SuperLeft, Key::SuperRight]),
    });
    let mode = if want { LOCK_MODE } else { CursorGrabMode::None };
    if cursor.grab_mode != mode {
        cursor.grab_mode = mode;
    }
    if LOCK_MODE == CursorGrabMode::Locked {
        let (lb, f) = (letterbox(window), window.scale_factor());
        locked.0 = match (want, locked.0) {
            (true, None) => {
                let (x, y) = window
                    .cursor_position()
                    .map_or_else(|| lb.centre(f), |p| lb.keep_on_screen((p.x, p.y), f));
                Some(Vec2::new(x, y))
            }
            (true, Some(p)) => {
                let (x, y) = lb.keep_on_screen((p.x + motion.x, p.y + motion.y), f);
                Some(Vec2::new(x, y))
            }
            (false, Some(p)) => {
                window.set_cursor_position(Some(p));
                None
            }
            (false, None) => None,
        };
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Sound

/// Sample frames mixed at a time for the device: about 12 ms.
const STREAM_CHUNK: usize = 256;

/// The core's mixed sound as a Bevy audio source that never ends: the device pulls it through Bevy's audio output
/// (rodio), which resamples it to the device's rate.
#[derive(Asset, TypePath)]
struct CoreStream(crate::audio::Output);

impl bevy::audio::Decodable for CoreStream {
    type Decoder = StreamDecoder;

    fn decoder(&self) -> StreamDecoder {
        StreamDecoder {
            output: self.0.clone(),
            buf: vec![0; STREAM_CHUNK * 2],
            at: STREAM_CHUNK * 2,
        }
    }
}

/// Pulls [`STREAM_CHUNK`] sample frames at a time from the mixer and hands them out one sample at a time.
struct StreamDecoder {
    output: crate::audio::Output,
    buf: Vec<i16>,
    at: usize,
}

impl Iterator for StreamDecoder {
    type Item = bevy::audio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at == self.buf.len() {
            self.output.mix(&mut self.buf);
            self.at = 0;
        }
        let s = self.buf[self.at];
        self.at += 1;
        Some(s as bevy::audio::Sample / 32_768.0)
    }
}

impl bevy::audio::Source for StreamDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> bevy::audio::ChannelCount {
        bevy::audio::ChannelCount::new(2).expect("two channels")
    }

    fn sample_rate(&self) -> bevy::audio::SampleRate {
        bevy::audio::SampleRate::new(crate::audio::SAMPLE_RATE).expect("a rate")
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

fn start_stream(mut commands: Commands, core: Res<CoreRes>, mut streams: ResMut<Assets<CoreStream>>) {
    let h = streams.add(CoreStream(core.audio.output()));
    commands.spawn((AudioPlayer(h), PlaybackSettings::ONCE));
}

// ---------------------------------------------------------------------------------------------------------------
// Input

/// Hands the window's key, button and wheel events to the core's input, as they came.
fn take_system_input(
    mut core: ResMut<CoreRes>,
    mut keys: MessageReader<KeyboardInput>,
    mut lost: MessageReader<KeyboardFocusLost>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut wheel: MessageReader<MouseWheel>,
) {
    let input = &mut core.input;
    for e in keys.read() {
        input.key(KeyEvent {
            key: key_of(e.key_code),
            pressed: e.state.is_pressed(),
            text: e.text.as_ref().map(|t| t.to_string()),
        });
    }
    for e in buttons.read() {
        input.button(button_of(e.button), e.state.is_pressed());
    }
    for e in wheel.read() {
        input.wheel(Wheel {
            y: e.y,
            lines: e.unit == MouseScrollUnit::Line,
        });
    }
    if lost.read().count() > 0 {
        input.focus_lost();
    }
}

/// A key by its place.
pub fn key_of(k: KeyCode) -> Key {
    use KeyCode as K;
    let letter = |c: u8| Key::Letter(c as char);
    match k {
        K::KeyA => letter(b'A'),
        K::KeyB => letter(b'B'),
        K::KeyC => letter(b'C'),
        K::KeyD => letter(b'D'),
        K::KeyE => letter(b'E'),
        K::KeyF => letter(b'F'),
        K::KeyG => letter(b'G'),
        K::KeyH => letter(b'H'),
        K::KeyI => letter(b'I'),
        K::KeyJ => letter(b'J'),
        K::KeyK => letter(b'K'),
        K::KeyL => letter(b'L'),
        K::KeyM => letter(b'M'),
        K::KeyN => letter(b'N'),
        K::KeyO => letter(b'O'),
        K::KeyP => letter(b'P'),
        K::KeyQ => letter(b'Q'),
        K::KeyR => letter(b'R'),
        K::KeyS => letter(b'S'),
        K::KeyT => letter(b'T'),
        K::KeyU => letter(b'U'),
        K::KeyV => letter(b'V'),
        K::KeyW => letter(b'W'),
        K::KeyX => letter(b'X'),
        K::KeyY => letter(b'Y'),
        K::KeyZ => letter(b'Z'),
        K::Digit0 => Key::Digit(0),
        K::Digit1 => Key::Digit(1),
        K::Digit2 => Key::Digit(2),
        K::Digit3 => Key::Digit(3),
        K::Digit4 => Key::Digit(4),
        K::Digit5 => Key::Digit(5),
        K::Digit6 => Key::Digit(6),
        K::Digit7 => Key::Digit(7),
        K::Digit8 => Key::Digit(8),
        K::Digit9 => Key::Digit(9),
        K::Numpad0 => Key::Numpad(0),
        K::Numpad1 => Key::Numpad(1),
        K::Numpad2 => Key::Numpad(2),
        K::Numpad3 => Key::Numpad(3),
        K::Numpad4 => Key::Numpad(4),
        K::Numpad5 => Key::Numpad(5),
        K::Numpad6 => Key::Numpad(6),
        K::Numpad7 => Key::Numpad(7),
        K::Numpad8 => Key::Numpad(8),
        K::Numpad9 => Key::Numpad(9),
        K::NumpadAdd => Key::NumpadAdd,
        K::NumpadSubtract => Key::NumpadSubtract,
        K::NumpadEnter => Key::NumpadEnter,
        K::F1 => Key::F(1),
        K::F2 => Key::F(2),
        K::F3 => Key::F(3),
        K::F4 => Key::F(4),
        K::F5 => Key::F(5),
        K::F6 => Key::F(6),
        K::F7 => Key::F(7),
        K::F8 => Key::F(8),
        K::F9 => Key::F(9),
        K::F10 => Key::F(10),
        K::F11 => Key::F(11),
        K::F12 => Key::F(12),
        K::F13 => Key::F(13),
        K::F14 => Key::F(14),
        K::F15 => Key::F(15),
        K::ArrowUp => Key::Up,
        K::ArrowDown => Key::Down,
        K::ArrowLeft => Key::Left,
        K::ArrowRight => Key::Right,
        K::Home => Key::Home,
        K::End => Key::End,
        K::PageUp => Key::PageUp,
        K::PageDown => Key::PageDown,
        K::Enter => Key::Enter,
        K::Escape => Key::Escape,
        K::Backspace => Key::Backspace,
        K::Space => Key::Space,
        K::Minus => Key::Minus,
        K::Equal => Key::Equal,
        K::PrintScreen => Key::PrintScreen,
        K::CapsLock => Key::CapsLock,
        K::ShiftLeft => Key::ShiftLeft,
        K::ShiftRight => Key::ShiftRight,
        K::ControlLeft => Key::ControlLeft,
        K::ControlRight => Key::ControlRight,
        K::AltLeft => Key::AltLeft,
        K::AltRight => Key::AltRight,
        K::SuperLeft => Key::SuperLeft,
        K::SuperRight => Key::SuperRight,
        K::Unidentified(_) => Key::Other(0),
        // Any other key by its name, which is the same for the same key.
        k => Key::Other(1 + (crate::script::fnv64(format!("{k:?}").as_bytes()) % u32::MAX as u64) as u32),
    }
}

fn button_of(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
        MouseButton::Back => Button::Other(0),
        MouseButton::Forward => Button::Other(1),
        MouseButton::Other(n) => Button::Other(n.saturating_add(2)),
    }
}
