//! The command line (`clap`).

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(about = "Tidewater map viewer")]
pub struct Args {
    /// Warcraft II: Battle.net Edition installation directory.
    #[arg(long, env = "TIDEWATER_INSTALL")]
    pub install: PathBuf,
    /// A .pud file path, or a name inside the archives; it starts at once. Without one the program starts with its
    /// startup videos and title (`docs/specs/game/campaign-flow.md`).
    pub map: Option<String>,
    /// Development aid: open a screen of the campaign flow: `title`, `menu`, `campaign`, `mission:E`, `act:E`,
    /// `briefing:E`, `score-victory:E` or `score-defeat:E` (synthetic statistics), `ending:E`, `credits:tod`,
    /// `credits:bdp` or `video:N`, with E a mission entry (0 to 51).
    #[arg(long)]
    pub goto: Option<String>,
    /// Don't play the intro video at startup (the "play the intro" preference bit cleared).
    #[arg(long)]
    pub no_intro: bool,
    /// Development aid: the game ends at once with `victory` or `defeat` (to see the dialog and the score screen).
    #[arg(long, value_parser = ["victory", "defeat"])]
    pub demo_outcome: Option<String>,
    /// Game setup options: `campaign:ENTRY`, or `custom` with choices such as
    /// `custom:race=orc,opponents=2,resources=high,terrain=swamp,units=one-worker,fixed-starts,fog=off`.
    /// By default a campaign map starts as its campaign mission and any other map as a custom game.
    #[arg(long, value_parser = tidewater_sim::setup::Setup::parse)]
    pub setup: Option<tidewater_sim::setup::Setup>,
    /// The folder for saved games. Without it, a `Save` folder in Tidewater's own user data directory; the program
    /// never writes into the installation.
    #[arg(long)]
    pub save_dir: Option<PathBuf>,
    /// Development aid, not part of the original: the mouse wheel zooms the map view, in powers of two from 1/4 to 8
    /// map pixels per screen pixel. Without it the map view shows one map pixel per screen pixel, as the original.
    #[arg(long)]
    pub free_zoom: bool,
    /// Development aid, not part of the original: the starting zoom in map pixels per screen pixel (2 shows twice as
    /// much map), taken to the nearest power of two from 1/4 to 8. Implies `--free-zoom`.
    #[arg(long)]
    pub zoom: Option<f32>,
    /// The window size in logical points (the system's "looks like" size) per screen pixel: N opens a 640N × 480N
    /// window. Without it, the window is the largest whole number of physical pixels per screen pixel that fits the
    /// monitor (on a display with 2 pixels per point, 3 gives a 960 × 720-point window). Either way the 640 × 480
    /// screen is drawn at the largest whole number of physical pixels that fits the window, with black around it.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=8))]
    pub ui_scale: Option<u32>,
    /// Development aid: order all of your units to tile `X,Y` at the start.
    #[arg(long, value_parser = parse_tile)]
    pub demo_move: Option<(i32, i32)>,
    /// Development aid: send every player's units against the nearest enemy within 16 tiles at the start.
    #[arg(long)]
    pub demo_fight: bool,
    /// Development aid: select your first N units at the start and centre the view on them.
    #[arg(long)]
    pub demo_select: Option<usize>,
    /// Development aid: with `--demo-select`, select only units of this type number (any owner).
    #[arg(long)]
    pub demo_type: Option<u8>,
    /// Development aid: with `--demo-select`, start in placement mode for this building type number.
    #[arg(long)]
    pub demo_place: Option<u8>,
    /// Development aid: with `--demo-place`, the pointer's tile `X,Y` while the pointer is off the map view, and one
    /// placement click there at the start.
    #[arg(long, value_parser = parse_tile)]
    pub demo_site: Option<(i32, i32)>,
    /// Don't lock the pointer to the window during a game (not part of the original, which runs full screen; see
    /// `display`, "Mouse lock"). Cmd (the Windows key elsewhere) frees it while held, and switching away frees it.
    #[arg(long)]
    pub no_mouse_lock: bool,
    /// The Preferences dialog's second mouse style: right click centres the view instead of giving a command.
    #[arg(long)]
    pub second_mouse_style: bool,
    /// The speed dialog's mouse scroll speed, 0 to 6 (fastest). Without it, the stored setting.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=6))]
    pub mouse_scroll_speed: Option<u8>,
    /// The speed dialog's key scroll speed, 0 to 6 (fastest). Without it, the stored setting.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=6))]
    pub key_scroll_speed: Option<u8>,
    /// The settings file, read at start and written when the program exits normally. Without it, `settings.txt` in
    /// the `--save-dir` folder if one is given, else in Tidewater's own user data directory.
    #[arg(long)]
    pub settings: Option<PathBuf>,
    /// Development aid: run a scripted scene (`script`, `docs/design/client-core.md`): a fixed frame time and wall
    /// clock, the scene's input instead of the system's, no sound, a 640 × 480 window at one pixel per screen pixel.
    /// It writes the frame and sound fingerprints to `--script-out`, and with a window also the window's own pixels
    /// and the composed frame at each of the scene's shots, compared.
    #[arg(long, requires = "script_out")]
    pub script: Option<String>,
    /// The folder a scripted run writes to (its own saved games and settings go there too, cleared first).
    #[arg(long, requires = "script")]
    pub script_out: Option<PathBuf>,
    /// With `--script`: no window and no GPU; the frames are only composed.
    #[arg(long, requires = "script")]
    pub headless: bool,
}

pub fn parse_tile(s: &str) -> Result<(i32, i32), String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok((
        x.trim().parse().map_err(|_| "bad X")?,
        y.trim().parse().map_err(|_| "bad Y")?,
    ))
}
