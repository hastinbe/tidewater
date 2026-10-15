//! Tidewater's client core as a library (`docs/design/client-core.md`): everything but a platform shell, with no
//! engine types. The `tidewater` binary (`main.rs`) runs it in the Bevy shell (`shell`, the `bevy` feature, on by
//! default); the trial shells (`crates/shell-*`) link it without Bevy and start it with [`host::launch`].

pub mod args;
pub mod audio;
mod campaign;
mod carry;
mod chat;
mod cheats;
mod clock;
pub mod compose;
pub mod core;
pub mod display;
mod fog;
mod frontend;
mod game;
mod gamemenu;
pub mod host;
mod hud;
mod info;
pub mod input;
mod jukebox;
mod layers;
mod mapview;
mod menu;
mod messages;
mod minimap;
mod mouse;
#[allow(dead_code)] // the spec's track table and fade helpers are kept whole; some screens don't use them yet
mod music;
mod palette;
mod panel;
mod prefs;
mod savedlg;
mod savefile;
mod savegame;
mod saves;
mod scene;
mod screen;
mod screenshot;
pub mod script;
#[cfg(feature = "bevy")]
pub mod shell;
mod sound;
mod states;
pub mod storage;
mod video;
mod view;
