//! The Bevy side of the music (`music.rs`, spec `docs/specs/ui/music.md`): one resource that owns the state machine
//! and the open track's voice in the core's mixer (`audio`), and the system that runs it every frame. Every screen starts and stops music through the
//! methods here; the music files are read from the install's `Music` folder through the front end's [`Front`].
//!
//! The game menu's sound dialog changes the volume through [`Jukebox::set_volume`] (the same call for the countdown
//! ending, the cancel and the menu closing: "Every time the music volume is applied").

use std::time::Duration;

use bevy::prelude::*;

use crate::audio::{self, VoiceId};
use crate::frontend::Front;
use crate::music::{self, Action, Music, Playback};
use crate::shell::{CoreAudio, CoreTime};
use crate::sound::Shuffle;

/// The music state machine and the open track's voice.
#[derive(Resource, Default)]
pub struct Jukebox {
    music: Music,
    /// Actions to apply on the next frame (from starts and key presses).
    pending: Vec<Action>,
    /// The open track's voice in the core's mixer.
    voice: Option<VoiceId>,
    /// Bytes of audio in the open track's file.
    length: u32,
    /// The attenuation the music asks for, and the one last given to the voice.
    atten: i32,
    applied: Option<i32>,
    /// Frame time not yet handed to the music, below a millisecond.
    carry: Duration,
}

#[allow(dead_code)] // not every screen calls every method yet
impl Jukebox {
    fn queue(&mut self, actions: Vec<Action>) {
        self.pending.extend(actions);
    }

    /// The music volume, 0 to 100.
    pub fn volume(&self) -> u8 {
        self.music.volume()
    }

    /// Applies the music volume (0 to 100). The game menu calls this whenever the volume takes effect.
    pub fn set_volume(&mut self, v: u8) {
        let a = self.music.apply_volume(v);
        self.queue(a);
    }

    /// Ctrl+M.
    pub fn toggle_mute(&mut self) {
        let a = self.music.toggle_mute();
        self.queue(a);
    }

    /// Starts a track at once (`None` is no music).
    pub fn start(&mut self, track: Option<u8>) {
        let a = self.music.start(track);
        self.queue(a);
    }

    /// Starts a track if none is playing (main menu, briefing).
    pub fn start_unless_playing(&mut self, track: u8) {
        let a = self.music.start_unless_playing(track);
        self.queue(a);
    }

    /// Starts a track if none has started (credits).
    pub fn start_unless_started(&mut self, track: u8) {
        let a = self.music.start_unless_started(track);
        self.queue(a);
    }

    /// A fade stop, or a fade of all sound, then `then` starts if given.
    pub fn fade_stop(&mut self, all_sound: bool, then: Option<u8>) {
        let a = self.music.fade_stop(all_sound, then);
        self.queue(a);
    }

    /// The music state, for the screens that need a test.
    pub fn music(&self) -> &Music {
        &self.music
    }

    /// A game starts: the game's track plays if no music has started. The track is drawn after the map has loaded, from
    /// the setup-shuffle generator.
    pub fn start_game(&mut self, orc: bool, campaign: Option<u8>, shuffle: &mut Shuffle) {
        let m = &mut self.music;
        m.set_campaign(campaign);
        let track = music::game_track(orc, campaign, false, shuffle);
        let a = m.start_game(track);
        self.queue(a);
    }

    /// A saved game is loaded: the saved track plays if no music has started (a loaded game "uses the saved track",
    /// `ui/music.md`). The campaign bump has been applied to the saved track already, and applying it again changes
    /// nothing.
    pub fn resume_game(&mut self, campaign: Option<u8>, track: Option<u8>) {
        self.music.set_campaign(campaign);
        if let Some(t) = track {
            let a = self.music.start_game(t);
            self.queue(a);
        }
    }

    /// The disco cheat turned `on` or off: track 25, or the game's normal track from `normal` (called only when turning
    /// off, as only that draws from the setup-shuffle generator).
    pub fn disco(&mut self, on: bool, normal: impl FnOnce() -> u8) {
        let normal = if on { music::DISCO } else { normal() };
        let (_, a) = self.music.disco(on, normal);
        self.queue(a);
    }

    /// The game is over: the bump no longer applies.
    pub fn end_game(&mut self) {
        self.music.set_campaign(None);
    }
}

/// Runs the music each frame and applies what it asks for in the core's mixer: loads a file from the install's
/// `Music` folder, sets its loudness and stops it. The open track's position is the mixer's, so it runs on the mixer's
/// clock; a loop track loops in the mixer, from its loop point, without a gap (`audio`), so the loop point needs no
/// seek.
pub fn play_music(time: Res<CoreTime>, mut jb: ResMut<Jukebox>, mut front: ResMut<Front>, mut core: ResMut<CoreAudio>) {
    let jb = &mut *jb;
    jb.carry += time.delta();
    let ms = jb.carry.as_millis() as u32;
    jb.carry -= Duration::from_millis(ms as u64);
    let length = jb.length;
    // A voice the mixer no longer has has played to its end.
    let playback = jb.voice.filter(|_| length > 0).map(|v| Playback {
        position: core
            .position(v)
            .map_or(length, |frame| frame as u32 * music::BYTES_PER_FRAME),
        length,
    });
    let mut actions = std::mem::take(&mut jb.pending);
    actions.extend(jb.music.update(ms, playback));
    for action in actions {
        match action {
            Action::Stop => {
                if let Some(v) = jb.voice.take() {
                    core.stop(v);
                }
            }
            Action::Load {
                track,
                looped,
                attenuation,
                ..
            } => {
                if let Some(v) = jb.voice.take() {
                    core.stop(v);
                }
                jb.atten = attenuation;
                jb.length = 0;
                // Our choice: the whole file is read at once; a file the install lacks leaves the music silent.
                let Some(bytes) = front.install.read(&music::path(track)).ok() else {
                    continue;
                };
                let Some(sound) = audio::Sound::from_wav(&bytes) else {
                    continue;
                };
                jb.length = music::wav_data_len(&bytes).unwrap_or(0);
                let loop_from = match music::TRACKS[track as usize].end {
                    music::End::Loop { point } if looped => Some(point as usize),
                    _ => None,
                };
                let volume = audio::decibels_to_linear(music::decibels(attenuation));
                jb.voice = Some(core.play_music(&sound, volume, audio::attenuation_gain(attenuation), loop_from));
                jb.applied = Some(attenuation);
            }
            Action::SetAttenuation(a) => jb.atten = a,
            // The mixer loops from the loop point itself.
            Action::SeekLoop { .. } => {}
            // The game's sound effects aren't cut during a fade of all sound yet.
            Action::CutEffects => {}
        }
    }
    if let Some(v) = jb.voice
        && jb.applied != Some(jb.atten)
    {
        core.set_gain(v, audio::attenuation_gain(jb.atten));
        jb.applied = Some(jb.atten);
    }
}
