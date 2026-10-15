//! The window shows one picture: the 640 × 480 screen, composed on the CPU by [`compose`] each frame and drawn at the
//! window's whole-number scale with black around it (`display::Letterbox`). This is the first step of the
//! engine-neutral client core (`docs/design/client-core.md`): Bevy no longer stacks the screen's layers itself.
//!
//! The layers are the core's (`layers`, step 3 of the design): each is a picture in [`ScreenPictures`] and a
//! [`ScreenLayer`] on an entity that says where it goes, and [`compose_screen`] puts them on the screen in order. The
//! entities stay only so that the flow's states can still remove a screen's layers with it (until step 6). The map
//! view's place, centre and zoom are the core's too (`mapview`, step 2); Bevy's only camera shows the finished screen.
//!
//! | Order | Plane (`layers::Plane`) | Layers | Placed |
//! |---|---|---|---|
//! | 1 | Map | Terrain, map view, placement image (by z) | In the map view (`hud::Layout::viewport`), around the view's centre, at the zoom |
//! | 2 | Interface | The interface picture (`Hud`) | Centred on the screen, one picture pixel per screen pixel |
//! | 3 | Flow | The flow's layers (`frontend`, `gamemenu`: screens, dialogs, menus), by z | Centred on the screen at their size, with their fade |
//!
//! Outside a game the flow's screen is cleared before its layers (`frontend::FrontScreen`), so only they show there.
//!
//! The pointer goes on top of everything: while the game runs the interface picture has it (`hud`); everywhere else
//! the screen draws the normal arrow (screen spec, "Pointer shapes": the local race's in a game, the human one outside
//! it), and the system's pointer is hidden whenever the screen draws one.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, Viewport};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::message::MessageUpdateSystems;
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::DeferredWorld;
use bevy::input::keyboard::{KeyboardFocusLost, KeyboardInput};
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::clock::FrameClock;
use crate::compose::{self, Clip, Frame, Placed};
use crate::frontend::{AppState, Front, FrontScreen, LockedPointer, OutcomeDialog, Persist};
use crate::gamemenu::GameMenu;
use crate::input::{Button, Input, Key, KeyEvent, Wheel};
use crate::layers::{Layer, PictureId, Pictures, Plane};
use crate::{GameView, Hud};

/// The render layer only the screen's own camera and picture are on.
const SCREEN_LAYER: usize = 30;

/// The composed screen: its picture for the window, and the frame itself.
#[derive(Resource)]
pub struct Screen {
    pub image: Handle<Image>,
    pub frame: Frame,
}

#[derive(Component)]
struct ScreenCamera;

#[derive(Component)]
struct ScreenSprite;

/// The screen is composed after everything that changes the layers, and before the frame is drawn.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ComposeScreen;

pub fn plugin(app: &mut App) {
    app.init_resource::<PlayerInput>()
        .init_resource::<CoreTime>()
        .configure_sets(PreUpdate, ReadInput.after(bevy::input::InputSystems))
        .add_systems(
            First,
            (
                take_system_input,
                age_input.run_if(|time: Res<CoreTime>| time.logs_age()),
                advance_clock.after(bevy::time::TimeSystems),
            )
                .chain()
                .after(MessageUpdateSystems),
        )
        .add_systems(PreUpdate, begin_input_frame.in_set(ReadInput))
        .init_resource::<ScreenPictures>()
        .add_systems(Startup, spawn_screen)
        .add_systems(Last, (compose_screen, show_screen).chain().in_set(ComposeScreen));
}

fn spawn_screen(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let frame = Frame::black();
    let image = images.add(Image::new(
        bevy::render::render_resource::Extent3d {
            width: compose::W as u32,
            height: compose::H as u32,
            depth_or_array_layers: 1,
        },
        bevy::render::render_resource::TextureDimension::D2,
        frame.rgba,
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
        Persist,
    ));
    commands.spawn((
        Sprite::from_image(image.clone()),
        RenderLayers::layer(SCREEN_LAYER),
        ScreenSprite,
        Persist,
    ));
    commands.insert_resource(Screen {
        image,
        frame: Frame::black(),
    });
}

/// The layers' pictures (`layers::Pictures`).
#[derive(Resource, Default, Deref, DerefMut)]
pub struct ScreenPictures(pub Pictures);

/// A layer of the screen (`layers::Layer`); its picture goes when the layer does.
#[derive(Component, Deref, DerefMut)]
#[component(on_remove = free_picture)]
pub struct ScreenLayer(pub Layer);

fn free_picture(mut world: DeferredWorld, ctx: HookContext) {
    let Some(id) = world.get::<ScreenLayer>(ctx.entity).map(|l| l.picture) else {
        return;
    };
    if let Some(mut pictures) = world.get_resource_mut::<ScreenPictures>() {
        pictures.remove(id);
    }
}

/// A layer to compose: its picture and its place on the screen.
struct Placing {
    picture: PictureId,
    z: f32,
    entity: Entity,
    origin: (f64, f64),
    scale: (f64, f64),
    clip: Clip,
    tint: f32,
}

#[allow(clippy::too_many_arguments)]
fn compose_screen(
    mut images: ResMut<Assets<Image>>,
    pictures: Res<ScreenPictures>,
    mut screen: ResMut<Screen>,
    mut windows: Query<(&Window, &mut CursorOptions), With<PrimaryWindow>>,
    pointer: PointerState,
    hud: Option<Res<Hud>>,
    map_view: Option<Res<GameView>>,
    front_screen: Res<FrontScreen>,
    layers: Query<(Entity, &ScreenLayer)>,
) {
    let Ok((window, mut cursor)) = windows.single_mut() else {
        return;
    };
    // Outside a game the flow's screen clears the screen first (`frontend::camera_clear`).
    let front_clears = front_screen.clears;
    let (center_x, center_y) = (compose::W as f64 / 2.0, compose::H as f64 / 2.0);
    let mut map = Vec::new();
    let mut interface = Vec::new();
    let mut front = Vec::new();
    // The map view: its rectangle on the screen, the map pixel at its centre, and map pixels per screen pixel.
    let view = match (hud.as_ref().and_then(|h| h.layout), map_view) {
        (Some(layout), Some(mv)) => {
            let v = layout.viewport;
            Some((
                Clip::new(v.x.max(0), v.y.max(0), v.w.max(1) as i32, v.h.max(1) as i32),
                (mv.centre.0 as f64, mv.centre.1 as f64),
                mv.zoom as f64,
            ))
        }
        _ => None,
    };
    for (entity, l) in &layers {
        if !l.visible {
            continue;
        }
        let Some(pic) = pictures.get(l.picture) else { continue };
        let (w, h) = l.drawn_size(pic);
        let (tw, th) = (pic.width as f64, pic.height as f64);
        let mut layer = Placing {
            picture: l.picture,
            z: l.z,
            entity,
            origin: (0.0, 0.0),
            scale: (1.0, 1.0),
            clip: Clip::SCREEN,
            tint: l.tint,
        };
        match l.plane {
            Plane::Map => {
                let Some((clip, centre, per_px)) = view else { continue };
                let (vx, vy) = (
                    clip.x0 as f64 + (clip.x1 - clip.x0) as f64 / 2.0,
                    clip.y0 as f64 + (clip.y1 - clip.y0) as f64 / 2.0,
                );
                // The picture's top-left corner in map pixels.
                let (mx, my) = (l.at.0 as f64 - w as f64 / 2.0, l.at.1 as f64 - h as f64 / 2.0);
                layer.origin = (vx + (mx - centre.0) / per_px, vy + (my - centre.1) / per_px);
                layer.scale = (w as f64 / tw / per_px, h as f64 / th / per_px);
                layer.clip = clip;
                map.push(layer);
            }
            Plane::Interface => {
                // One picture pixel per screen pixel, centred.
                layer.origin = (center_x - tw / 2.0, center_y - th / 2.0);
                interface.push(layer);
            }
            Plane::Flow => {
                layer.origin = (
                    center_x + l.at.0 as f64 - w as f64 / 2.0,
                    center_y + l.at.1 as f64 - h as f64 / 2.0,
                );
                layer.scale = (w as f64 / tw, h as f64 / th);
                front.push(layer);
            }
        }
    }
    let by_z = |a: &Placing, b: &Placing| a.z.total_cmp(&b.z).then(a.entity.cmp(&b.entity));
    map.sort_by(by_z);
    interface.sort_by(by_z);
    front.sort_by(by_z);
    let order: Vec<&Placing> = if front_clears {
        front.iter().collect()
    } else {
        map.iter().chain(&interface).chain(&front).collect()
    };
    let mut frame = Frame::black();
    for l in order {
        let Some(pic) = pictures.get(l.picture) else { continue };
        frame.draw(&Placed {
            rgba: &pic.rgba,
            width: pic.width,
            height: pic.height,
            origin: l.origin,
            scale: l.scale,
            clip: l.clip,
            tint: l.tint,
        });
    }
    // The pointer: the interface picture has it while the game runs; otherwise the normal arrow goes on top.
    let in_game = *pointer.state.get() == AppState::Game;
    let hud_draws = hud.as_ref().is_some_and(|h| h.art.has_pointers())
        && in_game
        && pointer.dialog.is_none()
        && pointer.menu.is_none();
    let arrow = match (&hud, &pointer.front) {
        _ if hud_draws => None,
        (Some(h), _) if in_game => h.art.pointer(crate::screen::Pointer::Arrow),
        (_, Some(f)) if !in_game => f.arrow.as_ref(),
        _ => None,
    };
    if let (Some((pic, hot)), Some(p)) = (arrow, pointer.locked.at(window)) {
        let (x, y) = crate::letterbox(window).screen_point((p.x, p.y), window.scale_factor());
        let (x, y) = (x.floor() as i32 - hot.0, y.floor() as i32 - hot.1);
        frame.draw(&Placed {
            rgba: &pic.rgba,
            width: pic.width,
            height: pic.height,
            origin: (x as f64, y as f64),
            scale: (1.0, 1.0),
            clip: Clip::SCREEN,
            tint: 1.0,
        });
    }
    // The system's pointer shows only where the screen has no pointer of its own.
    let visible = !hud_draws && arrow.is_none();
    if cursor.visible != visible {
        cursor.visible = visible;
    }
    if let Some(mut img) = images.get_mut(&screen.image) {
        img.data = Some(frame.rgba.clone());
    }
    screen.frame = frame;
}

/// What decides the pointer the screen draws.
#[derive(SystemParam)]
struct PointerState<'w> {
    state: Res<'w, State<AppState>>,
    dialog: Option<Res<'w, OutcomeDialog>>,
    menu: Option<Res<'w, GameMenu>>,
    front: Option<Res<'w, Front>>,
    locked: Res<'w, LockedPointer>,
}

/// The frame the window shows this frame, at one window pixel per screen pixel: what a scripted run records. `None`
/// without a window, or when the window shows the screen at a larger scale (the run's frames are then Bevy's own).
pub fn shown_frame(world: &mut World) -> Option<Frame> {
    let size = {
        let mut q = world.query_filtered::<&Window, With<PrimaryWindow>>();
        q.single(world).ok()?.physical_size()
    };
    if crate::display::Letterbox::fit((size.x, size.y)).scale != 1 {
        return None;
    }
    Some(world.resource::<Screen>().frame.clone())
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
    let lb = crate::letterbox(window);
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
// Order

/// Runs the frame's schedules on one thread, so that their systems run in one order: the order their schedule's graph
/// gives (`docs/design/client-core.md`, step 5). In `Update` the systems also say where their commands take effect
/// (`frame_systems` in `main.rs`) instead of Bevy choosing the points.
///
/// `TIDEWATER_ORDER_CHECK=1` logs every pair of systems whose order the graph leaves open while they share data:
/// Bevy's ambiguity check (system names need `--features bevy/debug`).
pub fn one_order(app: &mut App) {
    use bevy::ecs::schedule::{LogLevel, ScheduleLabel, SingleThreadedExecutor};
    let check = std::env::var_os("TIDEWATER_ORDER_CHECK").is_some();
    let update = Update.intern();
    let mut schedules = app.world_mut().resource_mut::<Schedules>();
    for (_, s) in schedules.iter_mut() {
        s.set_executor(SingleThreadedExecutor::new());
        let mut settings = s.get_build_settings();
        if s.label() == update {
            settings.auto_insert_apply_deferred = false;
        }
        if check {
            settings.ambiguity_detection = LogLevel::Warn;
        }
        s.set_build_settings(settings);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Time

/// The core's clock (`clock`), fed the window's frame times.
#[derive(Resource, Default, Deref, DerefMut)]
pub struct CoreTime(pub FrameClock);

/// Starts the core's frame with the real time since the last one (Bevy's real-time clock: the system's, or a scripted
/// run's fixed frame time).
fn advance_clock(real: Res<Time<Real>>, mut time: ResMut<CoreTime>) {
    time.advance(real.delta());
}

// ---------------------------------------------------------------------------------------------------------------
// Input

/// The player's input (`input::Input`), from the window's events or a scripted run (`script`).
#[derive(Resource, Default, Deref, DerefMut)]
pub struct PlayerInput(pub Input);

/// The frame's input is applied: the systems that read keys and buttons run after this.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReadInput;

/// Hands the window's key, button and wheel events to the input, as they came.
fn take_system_input(
    mut input: ResMut<PlayerInput>,
    mut keys: MessageReader<KeyboardInput>,
    mut lost: MessageReader<KeyboardFocusLost>,
    mut buttons: MessageReader<MouseButtonInput>,
    mut wheel: MessageReader<MouseWheel>,
) {
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

/// The typed events and wheel steps age after a frame with a step, once there has been one (`clock`).
fn age_input(mut input: ResMut<PlayerInput>) {
    input.update_logs();
}

fn begin_input_frame(mut input: ResMut<PlayerInput>) {
    input.begin_frame();
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
