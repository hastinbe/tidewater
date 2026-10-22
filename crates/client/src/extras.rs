//! Optional improvements that only change the client (`docs/design/extras.md`). Everything here is **off by default**:
//! with every option at its default the client behaves, draws and sounds exactly as without this module.
//!
//! **One table.** [`REGISTRY`] lists every option once: its name, kind, default, scope and description. The `[extras]`
//! section of `settings.txt`, the `--extra-NAME` and `--no-extra-NAME` flags, the `--help` text and the documentation
//! table are all produced from it, so a new option is one table row plus the code that reads it.
//!
//! **Where a value comes from.** Defaults, then the settings file ([`ClientExtras::read_line`]), then the command line
//! ([`ExtraFlags`]): `--extra-NAME` sets an option for this run and `--no-extra-NAME` puts it back to its default.
//! The command line is not written back to the settings file.
//!
//! **Scope.** An option is [`Scope::Client`] (**C**: it changes nothing the player could not see anyway) or
//! [`Scope::ClientLobby`] (**C+L**: client-only, but it gives an edge, so in a game with other players the lobby sets
//! it for everyone). [`effective`] picks a game's values: a local session uses the player's own, any other session
//! ([`Session::lobby_client_extras`](crate::session::Session::lobby_client_extras)) supplies its own for the C+L
//! options. The options that change the simulation are not here: they are [`tidewater_sim::Extras`].

use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, ArgMatches, Command, FromArgMatches};

/// An option, as an index into [`REGISTRY`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id {
    HealthBars,
    IdleWorker,
    GameClock,
    ControlGroupBar,
    SmartAttackMove,
    VolumeVoices,
    VolumeEffects,
    VolumeMusic,
    Scaling,
    RallyPoints,
    ProductionQueue,
}

/// How many options there are.
pub const COUNT: usize = 11;

/// Who decides an option in a game with other players.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// **C**: the player's own setting, always.
    Client,
    /// **C+L**: client-only but gives an edge; a non-local session sets it.
    ClientLobby,
    /// **S**: changes the simulation (`tidewater_sim::Extras`). The player's setting starts a local game; in a game
    /// with other players the lobby's value is used for everyone, and a replay uses the recording's.
    Sim,
}

/// What values an option takes. Every kind is stored as a number 0 to 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `on` or `off` (1 or 0).
    Flag,
    /// One of these names; the number is the name's place in the list, and the first name is the default.
    Choice(&'static [&'static str]),
    /// A percentage, 0 to 100.
    Percent,
}

/// One option's row.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub id: Id,
    /// The name in the settings file and after `--extra-` / `--no-extra-`.
    pub name: &'static str,
    pub kind: Kind,
    /// The value when nothing says otherwise: off, the first choice, or (for a volume) 100, which leaves the sound as
    /// the game's own dialog sets it.
    pub default: u8,
    pub scope: Scope,
    pub description: &'static str,
}

/// The health bar modes, in the order of the choice list.
pub const HEALTH_BAR_MODES: &[&str] = &["off", "hovered", "selected", "always"];
/// The clock overlay modes.
pub const CLOCK_MODES: &[&str] = &["off", "clock", "clock-apm"];

/// The picture scaling modes (option `scaling`).
pub const SCALING_MODES: &[&str] = &["integer", "sharp"];

/// Every option. The order is the settings file's and the help text's.
pub const REGISTRY: [Spec; COUNT] = [
    Spec {
        id: Id::HealthBars,
        name: "health-bars",
        kind: Kind::Choice(HEALTH_BAR_MODES),
        default: 0,
        scope: Scope::ClientLobby,
        description: "Hit point bars over units: only those under the pointer, only selected ones, or all",
    },
    Spec {
        id: Id::IdleWorker,
        name: "idle-worker",
        kind: Kind::Flag,
        default: 0,
        scope: Scope::Client,
        description: "An idle-worker button at the top of the screen and the F6 key: select your next idle worker",
    },
    Spec {
        id: Id::GameClock,
        name: "game-clock",
        kind: Kind::Choice(CLOCK_MODES),
        default: 0,
        scope: Scope::Client,
        description: "The game clock over the map view, with or without your actions per minute",
    },
    Spec {
        id: Id::ControlGroupBar,
        name: "group-bar",
        kind: Kind::Flag,
        default: 0,
        scope: Scope::Client,
        description: "A strip over the map view's top left listing your control groups with their sizes: click one to recall it",
    },
    Spec {
        id: Id::SmartAttackMove,
        name: "smart-attack-move",
        kind: Kind::Flag,
        default: 0,
        scope: Scope::ClientLobby,
        description: "A right-click on open ground with only fighting units selected attack-moves there instead of moving",
    },
    Spec {
        id: Id::VolumeVoices,
        name: "volume-voices",
        kind: Kind::Percent,
        default: 100,
        scope: Scope::Client,
        description: "Unit voices, in percent of the sound volume (100 leaves them as the game sets them)",
    },
    Spec {
        id: Id::VolumeEffects,
        name: "volume-effects",
        kind: Kind::Percent,
        default: 100,
        scope: Scope::Client,
        description: "Sound effects, in percent of the sound volume",
    },
    Spec {
        id: Id::VolumeMusic,
        name: "volume-music",
        kind: Kind::Percent,
        default: 100,
        scope: Scope::Client,
        description: "Music, in percent of the music volume",
    },
    Spec {
        id: Id::Scaling,
        name: "scaling",
        kind: Kind::Choice(SCALING_MODES),
        default: 0,
        scope: Scope::Client,
        description: "How the picture fits the window: only whole-number scales, or any size drawn sharp bilinear (touch platforms always use sharp)",
    },
    Spec {
        id: Id::RallyPoints,
        name: "rally-points",
        kind: Kind::Flag,
        default: 0,
        scope: Scope::Sim,
        description: "Right-click with a training building selected to set a rally point: the units it trains go there",
    },
    Spec {
        id: Id::ProductionQueue,
        name: "production-queue",
        kind: Kind::Flag,
        default: 0,
        scope: Scope::Sim,
        description: "Buildings queue up to five jobs, paid when queued; Cancel removes the last",
    },
];

impl Id {
    pub fn spec(self) -> &'static Spec {
        &REGISTRY[self as usize]
    }
}

impl Spec {
    /// The long flag that sets the option.
    pub fn flag(&self) -> String {
        format!("--extra-{}", self.name)
    }

    /// The long flag that puts the option back to its default.
    pub fn no_flag(&self) -> String {
        format!("--no-extra-{}", self.name)
    }

    /// The value a text means, if it is one the kind takes.
    pub fn parse(&self, text: &str) -> Option<u8> {
        let t = text.trim();
        match self.kind {
            Kind::Flag => match t {
                "on" | "true" | "yes" | "1" => Some(1),
                "off" | "false" | "no" | "0" => Some(0),
                _ => None,
            },
            Kind::Choice(names) => names.iter().position(|n| *n == t).map(|i| i as u8),
            Kind::Percent => t.parse::<u8>().ok().filter(|&v| v <= 100),
        }
    }

    /// A value as the settings file writes it.
    pub fn text(&self, value: u8) -> String {
        match self.kind {
            Kind::Flag => if value != 0 { "on" } else { "off" }.to_string(),
            Kind::Choice(names) => names.get(value as usize).copied().unwrap_or(names[0]).to_string(),
            Kind::Percent => value.min(100).to_string(),
        }
    }

    /// The values the option takes, for help text and the docs: `on, off`, the choices, or `0 to 100`.
    pub fn values(&self) -> String {
        match self.kind {
            Kind::Flag => "on, off".to_string(),
            Kind::Choice(names) => names.join(", "),
            Kind::Percent => "0 to 100".to_string(),
        }
    }

    /// The C or C+L mark.
    pub fn scope_mark(&self) -> &'static str {
        match self.scope {
            Scope::Client => "C",
            Scope::ClientLobby => "C+L",
            Scope::Sim => "S",
        }
    }
}

/// How health bars show (option `health-bars`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthBars {
    Off,
    Hovered,
    Selected,
    Always,
}

/// What the clock overlay shows (option `game-clock`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockMode {
    Off,
    Clock,
    ClockApm,
}

/// How the picture is fitted to the window (option `scaling`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    /// Whole numbers of window pixels per screen pixel, black around.
    Integer,
    /// Any size inside the window's safe area, drawn sharp bilinear (`display`, "Fractional scale").
    Sharp,
}

/// The values of every option for a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientExtras {
    values: [u8; COUNT],
}

impl Default for ClientExtras {
    fn default() -> Self {
        ClientExtras {
            values: REGISTRY.map(|s| s.default),
        }
    }
}

impl ClientExtras {
    pub fn get(&self, id: Id) -> u8 {
        self.values[id as usize]
    }

    pub fn set(&mut self, id: Id, value: u8) {
        self.values[id as usize] = value;
    }

    /// Whether every option is at its default (nothing differs from the original game).
    pub fn is_default(&self) -> bool {
        *self == ClientExtras::default()
    }

    pub fn health_bars(&self) -> HealthBars {
        match self.get(Id::HealthBars) {
            1 => HealthBars::Hovered,
            2 => HealthBars::Selected,
            3 => HealthBars::Always,
            _ => HealthBars::Off,
        }
    }

    pub fn scaling(&self) -> Scaling {
        match self.get(Id::Scaling) {
            1 => Scaling::Sharp,
            _ => Scaling::Integer,
        }
    }

    pub fn idle_worker(&self) -> bool {
        self.get(Id::IdleWorker) != 0
    }

    pub fn group_bar(&self) -> bool {
        self.get(Id::ControlGroupBar) != 0
    }

    pub fn smart_attack_move(&self) -> bool {
        self.get(Id::SmartAttackMove) != 0
    }

    /// The options that change the simulation, as a new local game starts with them (`tidewater_sim::Extras`). Each
    /// sim option has a registry row of scope [`Scope::Sim`] and is read here.
    pub fn sim(&self) -> tidewater_sim::Extras {
        tidewater_sim::Extras {
            rally_points: self.get(Id::RallyPoints) != 0,
            production_queue: self.get(Id::ProductionQueue) != 0,
            ..tidewater_sim::Extras::default()
        }
    }

    pub fn game_clock(&self) -> ClockMode {
        match self.get(Id::GameClock) {
            1 => ClockMode::Clock,
            2 => ClockMode::ClockApm,
            _ => ClockMode::Off,
        }
    }

    /// The mixer's master trims for the three volumes.
    pub fn trims(&self) -> crate::audio::Trims {
        crate::audio::Trims::from_percent(
            self.get(Id::VolumeVoices),
            self.get(Id::VolumeEffects),
            self.get(Id::VolumeMusic),
        )
    }

    /// One `name=value` line of the settings file's `[extras]` section. Unknown names and values the kind doesn't take
    /// are ignored (the option keeps its value), like the rest of the settings file. Returns whether the line was used.
    pub fn read_line(&mut self, name: &str, value: &str) -> bool {
        let Some(spec) = REGISTRY.iter().find(|s| s.name == name.trim()) else {
            return false;
        };
        match spec.parse(value) {
            Some(v) => {
                self.set(spec.id, v);
                true
            }
            None => false,
        }
    }

    /// The `[extras]` section of the settings file: a comment and a line per option, from the table.
    pub fn section_text(&self) -> String {
        let mut out = String::from("[extras]\n");
        for s in &REGISTRY {
            out.push_str(&format!("# {}: {} ({})\n", s.name, s.description, s.values()));
            out.push_str(&format!("{}={}\n", s.name, s.text(self.get(s.id))));
        }
        out
    }
}

/// The values a game runs with. A local session uses `local`; a session that holds its own ([`Session::lobby_client_extras`])
/// gives the C+L options, and the player's own settings are not read for them.
///
/// [`Session::lobby_client_extras`]: crate::session::Session::lobby_client_extras
pub fn effective(local: &ClientExtras, lobby: Option<ClientExtras>) -> ClientExtras {
    let Some(lobby) = lobby else { return *local };
    let mut out = *local;
    for s in REGISTRY.iter().filter(|s| s.scope == Scope::ClientLobby) {
        out.set(s.id, lobby.get(s.id));
    }
    out
}

/// The sim options a new local game starts with: the player's own choice (the registry's [`Scope::Sim`] rows). A session
/// that isn't local never calls this ([`Session::sim_extras`](crate::session::Session::sim_extras)): a network game
/// takes the lobby's set and a replay the recording's.
pub fn local_sim_extras(local: &ClientExtras) -> tidewater_sim::Extras {
    local.sim()
}

/// What the command line says about the options: values to set and options to put back to their default, in the order
/// the table lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtraFlags {
    changes: Vec<(Id, Option<u8>)>,
}

impl ExtraFlags {
    /// Flags from `(name, value)` pairs, as a scripted scene asks for them; a value `"default"` resets the option.
    /// Panics on a name or value the table doesn't have (a scene is code, so that is a bug).
    pub fn from_pairs(pairs: &[(&str, &str)]) -> ExtraFlags {
        let changes = pairs
            .iter()
            .map(|&(name, value)| {
                let spec = REGISTRY
                    .iter()
                    .find(|s| s.name == name)
                    .unwrap_or_else(|| panic!("no extra named {name}"));
                if value == "default" {
                    (spec.id, None)
                } else {
                    let v = spec
                        .parse(value)
                        .unwrap_or_else(|| panic!("{value} is not a value of {name}"));
                    (spec.id, Some(v))
                }
            })
            .collect();
        ExtraFlags { changes }
    }

    /// Puts the command line's changes over `extras`.
    pub fn apply(&self, extras: &mut ClientExtras) {
        for &(id, v) in &self.changes {
            extras.set(id, v.unwrap_or(id.spec().default));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

fn leak(s: String) -> &'static str {
    // Built once while the command line is built; the program ends soon after.
    Box::leak(s.into_boxed_str())
}

fn on_id(s: &Spec) -> &'static str {
    leak(format!("extra-{}", s.name))
}

fn off_id(s: &Spec) -> &'static str {
    leak(format!("no-extra-{}", s.name))
}

impl clap::Args for ExtraFlags {
    fn augment_args(mut cmd: Command) -> Command {
        const HEADING: &str = "Optional improvements (all off by default)";
        for s in &REGISTRY {
            let (on, off) = (on_id(s), off_id(s));
            let help = format!("{} [{}]", s.description, s.scope_mark());
            let mut arg = Arg::new(on)
                .long(on)
                .help(help)
                .help_heading(HEADING)
                .overrides_with(off);
            arg = match s.kind {
                Kind::Flag => arg.action(ArgAction::SetTrue),
                Kind::Choice(names) => arg
                    .value_name("MODE")
                    .value_parser(PossibleValuesParser::new(names.iter().copied())),
                Kind::Percent => arg
                    .value_name("PERCENT")
                    .value_parser(clap::value_parser!(u8).range(0..=100)),
            };
            cmd = cmd.arg(arg);
            cmd = cmd.arg(
                Arg::new(off)
                    .long(off)
                    .action(ArgAction::SetTrue)
                    .hide(true)
                    .overrides_with(on),
            );
        }
        cmd.after_help(
            "Each --extra-NAME option has a matching --no-extra-NAME that puts it back to its default for this run. \
             The settings file's [extras] section sets them for good. C: client only. C+L: client only, but in a game \
             with other players the lobby sets it. S: changes the simulation, so in a game with other players the \
             host's lobby sets it for everyone.",
        )
    }

    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

impl FromArgMatches for ExtraFlags {
    fn from_arg_matches(m: &ArgMatches) -> Result<Self, clap::Error> {
        let mut changes = Vec::new();
        for s in &REGISTRY {
            let (on, off) = (on_id(s), off_id(s));
            if m.get_flag(off) {
                changes.push((s.id, None));
                continue;
            }
            let v = match s.kind {
                Kind::Flag => m.get_flag(on).then_some(1),
                Kind::Choice(_) => m.get_one::<String>(on).and_then(|t| s.parse(t)),
                Kind::Percent => m.get_one::<u8>(on).copied(),
            };
            if let Some(v) = v {
                changes.push((s.id, Some(v)));
            }
        }
        Ok(ExtraFlags { changes })
    }

    fn update_from_arg_matches(&mut self, m: &ArgMatches) -> Result<(), clap::Error> {
        *self = Self::from_arg_matches(m)?;
        Ok(())
    }
}

/// The documentation table of the options, as `docs/design/extras.md` and the README keep it (a test checks they do).
pub fn doc_table() -> String {
    let mut out =
        String::from("| Option | Flag | Values | Scope | Default | What it does |\n|---|---|---|---|---|---|\n");
    for s in &REGISTRY {
        let flag = match s.kind {
            Kind::Flag => format!("`--extra-{}`", s.name),
            _ => format!("`--extra-{} VALUE`", s.name),
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {} |\n",
            s.name,
            flag,
            s.values(),
            s.scope_mark(),
            s.text(s.default),
            s.description
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Args;
    use clap::Parser;

    #[test]
    fn the_registry_is_in_id_order_with_unique_names() {
        for (i, s) in REGISTRY.iter().enumerate() {
            assert_eq!(s.id as usize, i, "{} is out of place", s.name);
            assert_eq!(REGISTRY.iter().filter(|o| o.name == s.name).count(), 1);
            assert_eq!(
                s.parse(&s.text(s.default)),
                Some(s.default),
                "{} default round trip",
                s.name
            );
        }
        assert_eq!(REGISTRY.len(), COUNT);
    }

    #[test]
    fn every_extra_is_off_by_default() {
        let e = ClientExtras::default();
        assert!(e.is_default());
        assert_eq!(e.health_bars(), HealthBars::Off);
        assert!(!e.idle_worker());
        assert!(!e.group_bar());
        assert!(!e.smart_attack_move());
        assert_eq!(e.game_clock(), ClockMode::Off);
        // A volume's "off" leaves the sound alone.
        assert_eq!(e.trims(), crate::audio::Trims::default());
    }

    #[test]
    fn rally_points_are_off_by_default_and_the_flag_turns_them_on() {
        let mut e = ClientExtras::default();
        assert!(e.sim().is_default());
        Args::parse_from(["tidewater", "--extra-rally-points"])
            .extras
            .apply(&mut e);
        assert!(e.sim().rally_points);
        assert_eq!(local_sim_extras(&e), e.sim());
        assert_eq!(Id::RallyPoints.spec().scope_mark(), "S");
        Args::parse_from(["tidewater", "--no-extra-rally-points"])
            .extras
            .apply(&mut e);
        assert!(e.sim().is_default());
    }

    #[test]
    fn values_parse_by_kind() {
        let s = Id::IdleWorker.spec();
        assert_eq!(
            (s.parse("on"), s.parse("off"), s.parse("maybe")),
            (Some(1), Some(0), None)
        );
        let s = Id::HealthBars.spec();
        assert_eq!((s.parse("selected"), s.parse("sometimes")), (Some(2), None));
        let s = Id::VolumeMusic.spec();
        assert_eq!((s.parse("40"), s.parse("101"), s.parse("-1")), (Some(40), None, None));
    }

    #[test]
    fn the_settings_section_round_trips_and_ignores_bad_lines() {
        let mut e = ClientExtras::default();
        e.set(Id::HealthBars, 3);
        e.set(Id::VolumeMusic, 25);
        e.set(Id::IdleWorker, 1);
        let text = e.section_text();
        assert!(text.starts_with("[extras]\n"));
        let mut back = ClientExtras::default();
        for line in text.lines().filter(|l| !l.starts_with('#') && l.contains('=')) {
            let (n, v) = line.split_once('=').unwrap();
            assert!(back.read_line(n, v));
        }
        assert_eq!(back, e);
        // A bad value, an unknown name: the option keeps its value.
        assert!(!back.read_line("health-bars", "loud"));
        assert!(!back.read_line("no-such-extra", "on"));
        assert_eq!(back, e);
    }

    #[test]
    fn flags_set_and_reset_and_the_last_one_wins() {
        let parse = |argv: &[&str]| {
            let mut v = vec!["tidewater"];
            v.extend(argv);
            Args::parse_from(v).extras
        };
        let mut base = ClientExtras::default();
        base.set(Id::VolumeMusic, 30);
        base.set(Id::IdleWorker, 1);
        // Nothing given: nothing changes.
        let mut e = base;
        parse(&[]).apply(&mut e);
        assert_eq!(e, base);
        // Set a flag, a choice and a percentage; reset another.
        let mut e = base;
        parse(&[
            "--extra-health-bars",
            "always",
            "--extra-game-clock=clock-apm",
            "--extra-volume-voices",
            "60",
            "--no-extra-idle-worker",
            "--no-extra-volume-music",
        ])
        .apply(&mut e);
        assert_eq!(e.health_bars(), HealthBars::Always);
        assert_eq!(e.game_clock(), ClockMode::ClockApm);
        assert_eq!(e.get(Id::VolumeVoices), 60);
        assert!(!e.idle_worker(), "reset to the default");
        assert_eq!(e.get(Id::VolumeMusic), 100, "reset to the default");
        // The last of a pair wins.
        let mut e = ClientExtras::default();
        parse(&["--extra-idle-worker", "--no-extra-idle-worker"]).apply(&mut e);
        assert!(!e.idle_worker());
        let mut e = ClientExtras::default();
        parse(&["--no-extra-idle-worker", "--extra-idle-worker"]).apply(&mut e);
        assert!(e.idle_worker());
    }

    #[test]
    fn bad_flag_values_are_refused() {
        assert!(Args::try_parse_from(["tidewater", "--extra-health-bars", "loud"]).is_err());
        assert!(Args::try_parse_from(["tidewater", "--extra-volume-music", "101"]).is_err());
    }

    #[test]
    fn the_help_lists_every_option_from_the_table() {
        use clap::CommandFactory;
        let help = Args::command().render_long_help().to_string();
        for s in &REGISTRY {
            assert!(help.contains(&s.flag()), "--help lacks {}", s.flag());
            assert!(help.contains(s.description), "--help lacks the text of {}", s.name);
        }
        assert!(help.contains("all off by default"));
    }

    /// Scope C+L options follow the session's setting in a game with other players, not the local settings. (A stub
    /// until the lobby exists, M3: the sessions hand over what the lobby will.)
    #[test]
    fn a_non_local_session_sets_the_c_plus_l_options_and_leaves_the_c_ones() {
        let mut local = ClientExtras::default();
        local.set(Id::HealthBars, 3);
        local.set(Id::IdleWorker, 1);
        local.set(Id::VolumeMusic, 10);
        // A local session: the player's own.
        assert_eq!(effective(&local, None), local);
        // A session that holds its own (lobby setting: no health bars, and it shouts about nothing else).
        let mut lobby = ClientExtras::default();
        lobby.set(Id::IdleWorker, 0);
        lobby.set(Id::VolumeMusic, 99);
        let e = effective(&local, Some(lobby));
        assert_eq!(e.health_bars(), HealthBars::Off, "C+L follows the session");
        assert!(e.idle_worker(), "C stays the player's");
        assert_eq!(e.get(Id::VolumeMusic), 10, "C stays the player's");
        // And the session's own value wins when it allows them.
        lobby.set(Id::HealthBars, 2);
        assert_eq!(effective(&local, Some(lobby)).health_bars(), HealthBars::Selected);
    }

    #[test]
    fn a_replay_follows_its_recording_and_a_local_game_the_player() {
        use crate::session::{LocalSession, ReplaySession, Session};
        use tidewater_replay::Replay;
        let mut local = ClientExtras::default();
        local.set(Id::HealthBars, 3);
        assert_eq!(LocalSession::new().lobby_client_extras(), None);
        let header = tidewater_replay::Header {
            sim_build: "test".into(),
            map_name: "m.pud".into(),
            map_hash: 1,
            seed: 2,
            setup: Default::default(),
            lobby: None,
            rules: tidewater_replay::RulesTag {
                enhanced: true,
                multiplayer: false,
                cheats_disabled: false,
                extras: Default::default(),
            },
            start_cheats: 0,
            local: 0,
        };
        let replay = ReplaySession::new(Replay::new(header));
        let lobby = replay.lobby_client_extras();
        assert_eq!(
            lobby,
            Some(ClientExtras::default()),
            "a recording has no health bars of its own"
        );
        assert_eq!(effective(&local, lobby).health_bars(), HealthBars::Off);
    }

    /// Prints the table for the docs: `cargo test -p tidewater-client print_the_doc_table -- --ignored --nocapture`.
    #[test]
    #[ignore = "prints the table to paste into docs/design/extras.md"]
    fn print_the_doc_table() {
        println!("{}", doc_table());
    }

    /// The documentation keeps the generated table, so the docs and the code can't drift apart.
    #[test]
    fn the_docs_carry_the_generated_table() {
        let doc = include_str!("../../../docs/design/extras.md");
        let table = doc_table();
        assert!(
            doc.contains(&table),
            "docs/design/extras.md must hold this table exactly (copy it from `extras::doc_table`):\n{table}"
        );
        let readme = include_str!("../../../README.md");
        for s in &REGISTRY {
            assert!(readme.contains(&s.flag()), "README.md lacks {}", s.flag());
        }
    }
}
