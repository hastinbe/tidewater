//! Tidewater client: the client core (`core`) in the Bevy shell (`shell`). Loads the user's installation, runs the
//! startup sequence, a map given on the command line, or a scripted scene (`script`).
//!
//! The 640 × 480 screen is drawn at a whole number of physical pixels per screen pixel, black-bordered in a larger
//! window, and the map view shows one map pixel per screen pixel, as the original does (`display`). The mouse wheel
//! zooms only with the `--free-zoom` development option. The controls are in `game`.

mod args;
mod audio;
mod campaign;
mod chat;
mod cheats;
mod clock;
mod compose;
mod core;
mod display;
mod fog;
mod frontend;
mod game;
mod gamemenu;
mod hud;
mod info;
mod input;
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
mod script;
mod shell;
mod sound;
mod states;
mod storage;
mod video;
mod view;

use std::sync::Arc;

use anyhow::Result;
use args::Args;
use clap::Parser;
use storage::Store;

fn main() -> Result<()> {
    let mut args = Args::parse();
    let scripted = match (&args.script, &args.script_out) {
        (Some(name), Some(out)) => {
            let scene = script::scene(name).ok_or_else(|| anyhow::anyhow!("no scripted scene {name}"))?;
            // The run's own saved games and settings, so nothing of the user's changes what it shows.
            let saves = out.join("save");
            if saves.exists() {
                std::fs::remove_dir_all(&saves)?;
            }
            args.save_dir = Some(saves);
            args.settings = Some(out.join("settings.txt"));
            args.no_mouse_lock = true;
            args.ui_scale = Some(1);
            Some((
                scene,
                script::Out {
                    dir: out.clone(),
                    windowed: !args.headless,
                    pictures: std::env::var_os("TIDEWATER_SCRIPT_PICTURES").is_some(),
                    every: std::env::var("TIDEWATER_SCRIPT_PICTURES")
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                },
            ))
        }
        _ => None,
    };
    // The desktop's storage (`storage`): a scripted run uses its own folder and a wall clock that stands still.
    let settings = args
        .settings
        .clone()
        .unwrap_or_else(|| storage::user_dir(args.save_dir.as_deref()).join(prefs::FILE_NAME));
    let mut desktop = storage::Desktop::new(
        &args.install,
        args.save_dir.clone().unwrap_or_else(storage::default_save_dir),
        settings,
        storage::user_dir(args.save_dir.as_deref()),
    );
    if scripted.is_some() {
        desktop = desktop.with_fixed_clock(script::WALL_SECS);
    }
    let store = Store(Arc::new(desktop));
    let install = storage::open_install(&store.0)?;
    let mut front = frontend::Front::load(install)?;
    let mut flow = frontend::Flow::default();
    // The map given on the command line starts at once; otherwise the startup sequence ("Startup") or a screen
    // named by `--goto`.
    let steps = match (&args.map, &args.goto, &scripted) {
        (_, _, Some((scene, _))) => scene.start.clone(),
        (Some(map), _, _) => vec![frontend::Step::Play {
            map: map.clone(),
            setup: args.setup,
        }],
        (None, Some(g), _) => {
            let steps = frontend::goto_steps(g).map_err(anyhow::Error::msg)?;
            if let Some((what, e)) = g.split_once(':')
                && what.starts_with("score-")
            {
                let e: u8 = e.parse()?;
                frontend::synthetic_score(&mut flow, &mut front, e, what == "score-victory");
            }
            steps
        }
        (None, None, _) => frontend::startup_steps(false, flow.skip_videos, !args.no_intro),
    };
    flow.steps = steps.into();
    // A scripted run starts from the default settings.
    let prefs = if scripted.is_some() {
        prefs::Prefs::default()
    } else {
        prefs::Prefs::load(&*store.0)
    };
    let stored =
        gamemenu::Stored(prefs.with_args(args.mouse_scroll_speed, args.key_scroll_speed, args.second_mouse_style));
    let title = args
        .map
        .as_deref()
        .map_or("Tidewater".to_string(), |m| format!("Tidewater — {m}"));
    let (ui_scale, headless) = (args.ui_scale, args.headless && scripted.is_some());
    let run = scripted.map(|(scene, out)| script::Run::new(scene, out));
    let core = core::Core::new(args, store, front, flow, stored, run);
    shell::run(core, title, ui_scale, headless);
    Ok(())
}
