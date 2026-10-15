//! The player's stored settings (spec `docs/specs/ui/game-menu.md`, "Stored settings"): game speed, the two scroll
//! speeds, the two volumes, the next tip number and the option bits. No Bevy types here.
//!
//! Choices where the spec leaves a gap:
//! - The text form is `name=value` lines. Unknown names, bad numbers and numbers outside their range fall back to
//!   the default ("A stored number outside its range is replaced by the default").
//! - "Where they are stored is the reimplementation's choice": the file `settings.txt` in Tidewater's own per-user data
//!   directory, or in the folder given with `--save-dir` (so a run with its own save folder leaves the real settings
//!   alone), or the file given with `--settings`. [`Prefs::load`] reads it at start-up (a missing or unreadable file
//!   means first-run defaults) and [`Prefs::store`] writes it when the program exits normally (`main.rs`).
//! - Command-line options (`--mouse-scroll-speed`, `--key-scroll-speed`, `--second-mouse-style`) given on top of the
//!   file override it for the run, and are stored at exit like any other change ([`Prefs::with_args`]).
//! - The minimap terrain mode is a field here, but it is not stored and starts on ("Not stored").

/// The option bits of the preferences word ("Option bits").
#[allow(dead_code, reason = "the spec's bit table is kept whole")]
pub mod bit {
    pub const UNIT_SPEECH: u32 = 0x1;
    pub const ACKNOWLEDGEMENTS: u32 = 0x2;
    pub const BUILDING_SOUNDS: u32 = 0x4;
    pub const MUSIC: u32 = 0x8;
    pub const COLOUR_CYCLING: u32 = 0x40;
    pub const TIPS_AT_START: u32 = 0x100;
    pub const OPENING_FILM: u32 = 0x200;
    /// The first mouse style (right-click commands).
    pub const FIRST_MOUSE_STYLE: u32 = 0x800;
    pub const SYSTEM_POINTER: u32 = 0x1000;
    pub const STEREO: u32 = 0x1_0000;
    pub const SOUND_3D: u32 = 0x2_0000;
    pub const SUBTITLES: u32 = 0x4_0000;
    pub const SPEECH_RECOGNITION: u32 = 0x8_0000;
    pub const SPEAK_MESSAGES: u32 = 0x10_0000;

    /// The bits that are on by default.
    pub const DEFAULT: u32 = UNIT_SPEECH
        | ACKNOWLEDGEMENTS
        | BUILDING_SOUNDS
        | MUSIC
        | COLOUR_CYCLING
        | TIPS_AT_START
        | OPENING_FILM
        | FIRST_MOUSE_STYLE
        | STEREO
        | SOUND_3D;
}

/// The settings file's name in the user folder.
pub const FILE_NAME: &str = "settings.txt";

pub const MAX_GAME_SPEED: u8 = 8;
pub const MAX_SCROLL_SPEED: u8 = 6;
pub const MAX_VOLUME: u8 = 100;
/// The tips table has this many entries.
pub const TIPS: u32 = 60;

pub const DEFAULT_GAME_SPEED: u8 = 5;
pub const DEFAULT_SCROLL_SPEED: u8 = 4;
pub const DEFAULT_MUSIC_VOLUME: u8 = 75;
pub const DEFAULT_DIGITAL_VOLUME: u8 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// The game speed as a 9-level value.
    pub game_speed: u8,
    pub mouse_scroll: u8,
    pub key_scroll: u8,
    pub music_volume: u8,
    pub digital_volume: u8,
    /// The next tip to show; 0 means "start at 1".
    pub next_tip: u32,
    pub bits: u32,
    /// The minimap's terrain mode. Not stored: on when the program starts.
    pub minimap_terrain: bool,
    /// Ctrl+M's remembered music volume, while the music is off by it.
    music_before: Option<u8>,
    /// Ctrl+S's remembered digital volume.
    digital_before: Option<u8>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            game_speed: DEFAULT_GAME_SPEED,
            mouse_scroll: DEFAULT_SCROLL_SPEED,
            key_scroll: DEFAULT_SCROLL_SPEED,
            music_volume: DEFAULT_MUSIC_VOLUME,
            digital_volume: DEFAULT_DIGITAL_VOLUME,
            next_tip: 0,
            bits: bit::DEFAULT,
            minimap_terrain: true,
            music_before: None,
            digital_before: None,
        }
    }
}

impl Prefs {
    /// These settings with the scroll speeds and the mouse style the command line gave, where it gave them.
    pub fn with_args(mut self, mouse_scroll: Option<u8>, key_scroll: Option<u8>, second_mouse_style: bool) -> Self {
        if let Some(s) = mouse_scroll {
            self.mouse_scroll = s.min(MAX_SCROLL_SPEED);
        }
        if let Some(s) = key_scroll {
            self.key_scroll = s.min(MAX_SCROLL_SPEED);
        }
        if second_mouse_style {
            self.set(bit::FIRST_MOUSE_STYLE, false);
        }
        self
    }

    /// Reads the settings file at program start ("Loading"); no file, or one that can't be read, gives the defaults.
    pub fn load(storage: &dyn crate::storage::Storage) -> Self {
        storage
            .read_settings()
            .and_then(|b| String::from_utf8(b).ok())
            .map_or_else(Self::default, |s| Self::from_text(&s))
    }

    /// Writes the settings file ("Saving": when the program exits normally).
    pub fn store(&self, storage: &dyn crate::storage::Storage) -> std::io::Result<()> {
        storage.write_settings(self.to_text().as_bytes())
    }

    /// Reads the text form: every setting takes its default, then the stored value when it is in range.
    pub fn from_text(s: &str) -> Self {
        let mut p = Self::default();
        for line in s.lines() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            let Ok(n) = value.trim().parse::<u32>() else {
                continue;
            };
            let within = |max: u8| u8::try_from(n).ok().filter(|&v| v <= max);
            match name.trim() {
                "game_speed" => p.game_speed = within(MAX_GAME_SPEED).unwrap_or(p.game_speed),
                "mouse_scroll" => p.mouse_scroll = within(MAX_SCROLL_SPEED).unwrap_or(p.mouse_scroll),
                "key_scroll" => p.key_scroll = within(MAX_SCROLL_SPEED).unwrap_or(p.key_scroll),
                "music_volume" => p.music_volume = within(MAX_VOLUME).unwrap_or(p.music_volume),
                "digital_volume" => p.digital_volume = within(MAX_VOLUME).unwrap_or(p.digital_volume),
                "next_tip" => p.next_tip = n,
                "bits" => p.bits = n,
                _ => {}
            }
        }
        p
    }

    /// The text form of everything stored.
    pub fn to_text(&self) -> String {
        format!(
            "game_speed={}\nmouse_scroll={}\nkey_scroll={}\nmusic_volume={}\ndigital_volume={}\nnext_tip={}\nbits={}\n",
            self.game_speed,
            self.mouse_scroll,
            self.key_scroll,
            self.music_volume,
            self.digital_volume,
            self.next_tip,
            self.bits
        )
    }

    pub fn has(&self, bit: u32) -> bool {
        self.bits & bit != 0
    }

    pub fn set(&mut self, bit: u32, on: bool) {
        if on {
            self.bits |= bit;
        } else {
            self.bits &= !bit;
        }
    }

    /// Whether palette cycling is on (`ui/palette-effects.md`).
    // TODO: palette.rs reads this once it is on main.
    #[allow(dead_code, reason = "palette.rs reads it once it is on main")]
    pub fn colour_cycling(&self) -> bool {
        self.has(bit::COLOUR_CYCLING)
    }

    /// Ctrl+M: music volume to 0 with the old value remembered, or the remembered value back. Returns whether the
    /// music is now on. No bit changes.
    #[allow(dead_code, reason = "music.rs calls it from its Ctrl+M key arm")]
    pub fn toggle_music(&mut self) -> bool {
        toggle(&mut self.music_volume, &mut self.music_before)
    }

    /// Ctrl+S: the same for the digital volume.
    pub fn toggle_digital(&mut self) -> bool {
        toggle(&mut self.digital_volume, &mut self.digital_before)
    }

    /// Takes the tip to show now and moves the stored number on ("Tips at the start of a game").
    pub fn take_tip(&mut self) -> u32 {
        let n = if self.next_tip == 0 || self.next_tip > TIPS {
            1
        } else {
            self.next_tip
        };
        self.next_tip = if n + 1 > TIPS { 1 } else { n + 1 };
        n
    }

    /// The digital volume on the sound module's 0 to 255 scale (our range).
    pub fn digital_level(&self) -> u8 {
        (self.digital_volume as u32 * 255).div_ceil(100) as u8
    }
}

fn toggle(volume: &mut u8, before: &mut Option<u8>) -> bool {
    match before.take() {
        Some(v) => {
            *volume = v;
            true
        }
        None => {
            *before = Some(*volume);
            *volume = 0;
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_defaults() {
        let p = Prefs::from_text("");
        assert_eq!(
            (
                p.game_speed,
                p.mouse_scroll,
                p.key_scroll,
                p.music_volume,
                p.digital_volume
            ),
            (5, 4, 4, 75, 100)
        );
        assert_eq!(p.bits, bit::DEFAULT);
        assert!(p.has(bit::UNIT_SPEECH | bit::MUSIC | bit::COLOUR_CYCLING | bit::FIRST_MOUSE_STYLE));
        assert!(!p.has(bit::SUBTITLES) && !p.has(bit::SYSTEM_POINTER));
        assert!(!p.has(bit::SPEECH_RECOGNITION) && !p.has(bit::SPEAK_MESSAGES));
        assert!(p.minimap_terrain);
    }

    #[test]
    fn out_of_range_values_take_the_default() {
        let p = Prefs::from_text("mouse_scroll=9\ngame_speed=8\nmusic_volume=101\nkey_scroll=x\nnext_tip=60");
        assert_eq!(p.mouse_scroll, 4);
        assert_eq!(p.game_speed, 8);
        assert_eq!(p.music_volume, 75);
        assert_eq!(p.key_scroll, 4);
        assert_eq!(p.next_tip, 60);
    }

    #[test]
    fn text_round_trip() {
        let mut p = Prefs {
            mouse_scroll: 6,
            music_volume: 0,
            next_tip: 17,
            ..Prefs::default()
        };
        p.set(bit::SUBTITLES, true);
        assert_eq!(Prefs::from_text(&p.to_text()), p);
    }

    #[test]
    fn tip_numbers() {
        let mut p = Prefs::from_text("next_tip=60");
        assert_eq!((p.take_tip(), p.next_tip), (60, 1));
        let mut p = Prefs::default();
        assert_eq!((p.take_tip(), p.next_tip), (1, 2));
        p.next_tip = 61;
        assert_eq!((p.take_tip(), p.next_tip), (1, 2));
    }

    #[test]
    fn ctrl_m_swaps_the_volume_and_keeps_the_bits() {
        let mut p = Prefs::default();
        let bits = p.bits;
        assert!(!p.toggle_music());
        assert_eq!(p.music_volume, 0);
        // A program that exits now stores 0.
        assert_eq!(Prefs::from_text(&p.to_text()).music_volume, 0);
        assert!(p.toggle_music());
        assert_eq!(p.music_volume, 75);
        assert_eq!(p.bits, bits);
        assert!(!p.toggle_digital());
        assert_eq!(p.digital_volume, 0);
        assert!(p.toggle_digital());
        assert_eq!(p.digital_volume, 100);
    }

    #[test]
    fn the_command_line_picks_scroll_and_mouse_style() {
        let p = Prefs::default().with_args(Some(2), Some(9), true);
        assert_eq!((p.mouse_scroll, p.key_scroll), (2, 6));
        assert!(!p.has(bit::FIRST_MOUSE_STYLE));
        assert!(
            Prefs::default()
                .with_args(None, None, false)
                .has(bit::FIRST_MOUSE_STYLE)
        );
        // Options not given leave the stored values.
        let stored = Prefs::from_text("mouse_scroll=1\nkey_scroll=2");
        let p = stored.clone().with_args(None, Some(5), false);
        assert_eq!((p.mouse_scroll, p.key_scroll), (1, 5));
    }

    #[test]
    fn the_settings_file_round_trips_and_a_missing_one_gives_defaults() {
        let dir = std::env::temp_dir().join(format!("tidewater-prefs-{}", std::process::id()));
        let path = dir.join("sub").join("settings.txt");
        let storage = crate::storage::Desktop::new(dir.join("install"), dir.join("Save"), path, dir.clone());
        assert_eq!(Prefs::load(&storage), Prefs::default());
        let mut p = Prefs {
            game_speed: 7,
            digital_volume: 30,
            ..Prefs::default()
        };
        p.set(bit::COLOUR_CYCLING, false);
        p.store(&storage).unwrap();
        assert_eq!(Prefs::load(&storage), p);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn digital_level_spans_the_sound_range() {
        let mut p = Prefs::default();
        assert_eq!(p.digital_level(), 255);
        p.digital_volume = 0;
        assert_eq!(p.digital_level(), 0);
    }
}
