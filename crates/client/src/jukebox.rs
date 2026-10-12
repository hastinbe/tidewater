//! The Bevy side of the music (`music.rs`, spec `docs/specs/ui/music.md`): one resource that owns the state machine
//! and the player entity, and the system that runs it every frame. Every screen starts and stops music through the
//! methods here; the music files are read from the install's `Music` folder through the front end's [`Front`].
//!
//! The game menu's sound dialog changes the volume through [`Jukebox::set_volume`] (the same call for the countdown
//! ending, the cancel and the menu closing: "Every time the music volume is applied").

use std::time::Duration;

use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;

use crate::frontend::{Front, Persist};
use crate::music::{self, Action, Music, Playback};
use crate::sound::Shuffle;

/// The music state machine and the entity that plays the open track.
#[derive(Resource, Default)]
pub struct Jukebox {
    music: Music,
    /// Actions to apply on the next frame (from starts and key presses).
    pending: Vec<Action>,
    /// The entity playing the open track.
    player: Option<Entity>,
    /// Bytes of audio in the open track's file.
    length: u32,
    /// The attenuation the music asks for, and the one last given to the sink.
    atten: i32,
    applied: Option<i32>,
    /// Frame time not yet handed to the music, below a millisecond.
    carry: Duration,
}

#[allow(dead_code)] // the game menu's sound dialog and the disco cheat will call the rest
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

    /// The game is over: the bump no longer applies.
    pub fn end_game(&mut self) {
        self.music.set_campaign(None);
    }
}

/// Marks the entity that plays the music.
#[derive(Component)]
pub struct MusicPlayer;

/// Runs the music each frame and applies what it asks for: loads a file from the install's `Music` folder, sets its
/// loudness, seeks to a loop point and stops it.
pub fn play_music(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut jb: ResMut<Jukebox>,
    mut front: ResMut<Front>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut sinks: Query<&mut AudioSink, With<MusicPlayer>>,
) {
    let jb = &mut *jb;
    jb.carry += time.delta();
    let ms = jb.carry.as_millis() as u32;
    jb.carry -= Duration::from_millis(ms as u64);
    let playback = jb.player.and_then(|e| sinks.get(e).ok()).and_then(|sink| {
        let length = jb.length;
        let bytes = (sink.position().as_secs_f64() * music::BYTES_PER_SECOND as f64) as u32;
        (length > 0).then_some(Playback {
            position: if sink.empty() { length } else { bytes },
            length,
        })
    });
    let mut actions = std::mem::take(&mut jb.pending);
    actions.extend(jb.music.update(ms, playback));
    for action in actions {
        match action {
            Action::Stop => {
                if let Some(e) = jb.player.take() {
                    commands.entity(e).despawn();
                }
            }
            Action::Load {
                track,
                looped,
                attenuation,
                ..
            } => {
                if let Some(e) = jb.player.take() {
                    commands.entity(e).despawn();
                }
                jb.atten = attenuation;
                jb.applied = Some(attenuation);
                jb.length = 0;
                // Our choice: the whole file is read at once; a file the install lacks leaves the music silent.
                let Ok(bytes) = front.install.read(&music::path(track)) else {
                    continue;
                };
                jb.length = music::wav_data_len(&bytes).unwrap_or(0);
                let settings = if looped {
                    PlaybackSettings::LOOP
                } else {
                    PlaybackSettings::ONCE
                };
                let volume = Volume::Decibels(music::decibels(attenuation));
                let h = sources.add(AudioSource { bytes: bytes.into() });
                jb.player = Some(
                    commands
                        .spawn((AudioPlayer(h), settings.with_volume(volume), MusicPlayer, Persist))
                        .id(),
                );
            }
            Action::SetAttenuation(a) => jb.atten = a,
            Action::SeekLoop { frame } => {
                if let Some(sink) = jb.player.and_then(|e| sinks.get_mut(e).ok()) {
                    let _ = sink.try_seek(Duration::from_secs_f64(frame as f64 / 22_050.0));
                }
            }
            // The game's sound effects aren't cut during a fade of all sound yet.
            Action::CutEffects => {}
        }
    }
    if jb.applied != Some(jb.atten)
        && let Some(mut sink) = jb.player.and_then(|e| sinks.get_mut(e).ok())
    {
        sink.set_volume(Volume::Decibels(music::decibels(jb.atten)));
        jb.applied = Some(jb.atten);
    }
}
