//! Tidewater's client core as a library (`docs/design/client-core.md`): everything but a platform shell, with no
//! engine types and no `unsafe` code. The `tidewater` binary (`shell/main.rs`) runs it in an SDL 3 window and starts
//! it with [`host::launch`].

#![forbid(unsafe_code)]

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
pub mod firstrun;
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
mod menuscreen;
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
mod sound;
mod states;
pub mod storage;
mod video;
mod view;
