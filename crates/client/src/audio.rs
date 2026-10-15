//! The core's sound (`docs/design/client-core.md`, migration step 7): sounds decoded from WAV files, and one mixer that
//! plays them all into 22,050 Hz stereo 16-bit samples, pulled by the shell's audio output ([`Output::mix`]) or, in a
//! scripted run, by the core itself. Nothing here depends on an engine, and the mixing is integer arithmetic, so the
//! same starts give the same samples on every machine.
//!
//! **WAV files.** The canonical RIFF/WAVE layout: a `RIFF` header naming `WAVE`, then chunks of a four-byte id, a
//! little-endian 32-bit size and the body, padded to an even length; `fmt ` gives the format tag (1 is PCM), the
//! channel count, the sample rate and the bits per sample; `data` holds the samples, interleaved, 8-bit unsigned or
//! 16-bit signed little-endian (public: the "Multimedia Programming Interface and Data Specifications 1.0", 1991,
//! "WAVE" chapter).
//!
//! **Our choices.** A sound at another rate is resampled to 22,050 Hz once, when it is decoded, by linear
//! interpolation; mono plays on both channels and channels past the second are dropped. A voice's loudness is a
//! 16.16 fixed-point gain; voices are summed and the sum clipped to 16 bits. A looping voice goes on from its loop
//! point the sample after its last one, so a loop has no gap. A voice that starts past its sound's end is silent.
//! These are our own: the original's mixing is the sound card driver's and isn't specified.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::states::AppState;

/// The mixer's output rate: sample frames a second (`ui/music.md`, "Files": the music files' rate).
pub const SAMPLE_RATE: u32 = 22_050;
/// A gain of one, in 16.16 fixed point.
pub const UNITY: i32 = 1 << 16;

/// One stereo sample frame.
pub type Frame = [i16; 2];

/// A sound ready to play: its sample frames at [`SAMPLE_RATE`], and a fingerprint of the file it came from.
#[derive(Clone, Debug)]
pub struct Sound {
    /// FNV-1a of the file's bytes (the frame snapshot's sound lines).
    pub hash: u64,
    pub frames: Arc<[Frame]>,
}

impl Sound {
    /// Decodes a WAV file; `None` if it isn't PCM WAV.
    pub fn from_wav(bytes: &[u8]) -> Option<Sound> {
        Some(Sound {
            hash: fnv64(bytes),
            frames: decode_wav(bytes)?.into(),
        })
    }
}

/// FNV-1a, 64 bits.
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// The sample frames of a PCM WAV file, at [`SAMPLE_RATE`] in stereo.
pub fn decode_wav(bytes: &[u8]) -> Option<Vec<Frame>> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut format = None;
    let mut data = None;
    let mut at = 12;
    while let Some(head) = bytes.get(at..at + 8) {
        let size = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match &head[..4] {
            b"fmt " => {
                let f = bytes.get(body..end)?;
                let u16_at = |i: usize| f.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
                let tag = u16_at(0)?;
                let channels = u16_at(2)?;
                let rate = u32::from_le_bytes(f.get(4..8)?.try_into().ok()?);
                let bits = u16_at(14)?;
                format = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        if data.is_some() && format.is_some() {
            break;
        }
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    let (tag, channels, rate, bits) = format?;
    let data = data?;
    if tag != 1 || channels == 0 || rate == 0 || !(bits == 8 || bits == 16) {
        return None;
    }
    let width = bits as usize / 8;
    let step = width * channels as usize;
    let sample = |s: &[u8]| -> i16 {
        if width == 1 {
            ((s[0] as i16) - 128) << 8
        } else {
            i16::from_le_bytes([s[0], s[1]])
        }
    };
    let frames: Vec<Frame> = data
        .chunks_exact(step)
        .map(|f| {
            let l = sample(&f[..width]);
            let r = if channels > 1 { sample(&f[width..2 * width]) } else { l };
            [l, r]
        })
        .collect();
    Some(if rate == SAMPLE_RATE {
        frames
    } else {
        resample(&frames, rate)
    })
}

/// Resamples frames at `from` Hz to [`SAMPLE_RATE`] by linear interpolation, in whole numbers.
fn resample(src: &[Frame], from: u32) -> Vec<Frame> {
    if src.is_empty() {
        return Vec::new();
    }
    let to = SAMPLE_RATE as u64;
    let n = (src.len() as u64 * to / from as u64) as usize;
    (0..n)
        .map(|i| {
            let num = i as u64 * from as u64;
            let j = (num / to) as usize;
            let frac = (num % to) as i64;
            let a = src[j];
            let b = *src.get(j + 1).unwrap_or(&a);
            let mix = |a: i16, b: i16| (a as i64 + (b as i64 - a as i64) * frac / to as i64) as i16;
            [mix(a[0], b[0]), mix(a[1], b[1])]
        })
        .collect()
}

/// Who a voice belongs to, so that it stops with its owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// The music's open track.
    Music,
    /// A game's sound effects: they stop when the game ends.
    Effect,
    /// A screen's own sounds (videos, briefings, the score screen, the endings): they stop when it is left.
    Screen(AppState),
    /// The act screen's sound: it stops when the act screen fades all sound, or is left.
    Act,
}

/// A voice playing (or played) in the mixer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(u64);

struct Voice {
    id: VoiceId,
    frames: Arc<[Frame]>,
    at: usize,
    gain: i32,
    loop_from: Option<usize>,
    group: Group,
}

/// The voices and how they sum.
#[derive(Default)]
pub struct Mixer {
    voices: Vec<Voice>,
    next: u64,
}

impl Mixer {
    /// Starts `sound` at sample frame `start` with a 16.16 `gain`; a looping voice goes on from frame `loop_from`.
    pub fn play(&mut self, sound: &Sound, gain: i32, loop_from: Option<usize>, start: usize, group: Group) -> VoiceId {
        let id = VoiceId(self.next);
        self.next += 1;
        let len = sound.frames.len();
        let loop_from = loop_from.filter(|&p| p < len);
        let at = match loop_from {
            Some(p) if start >= len => p + (start - len) % (len - p),
            _ => start,
        };
        if at < len {
            self.voices.push(Voice {
                id,
                frames: sound.frames.clone(),
                at,
                gain,
                loop_from,
                group,
            });
        }
        id
    }

    pub fn stop(&mut self, id: VoiceId) {
        self.voices.retain(|v| v.id != id);
    }

    /// Stops every voice of `group`.
    pub fn stop_group(&mut self, group: Group) {
        self.voices.retain(|v| v.group != group);
    }

    pub fn set_gain(&mut self, id: VoiceId, gain: i32) {
        if let Some(v) = self.voices.iter_mut().find(|v| v.id == id) {
            v.gain = gain;
        }
    }

    /// The next sample frame voice `id` plays, within its sound; `None` once it has ended or been stopped.
    #[allow(dead_code)] // the music reads it once the mixer plays (step 7's switch)
    pub fn position(&self, id: VoiceId) -> Option<usize> {
        self.voices.iter().find(|v| v.id == id).map(|v| v.at)
    }

    /// Voices playing.
    #[allow(dead_code)]
    pub fn playing(&self) -> usize {
        self.voices.len()
    }

    /// Mixes the next `out.len() / 2` sample frames into `out` (interleaved left, right).
    pub fn mix(&mut self, out: &mut [i16]) {
        let mut sum = vec![[0i64; 2]; out.len() / 2];
        for v in &mut self.voices {
            let len = v.frames.len();
            for s in sum.iter_mut() {
                if v.at >= len {
                    match v.loop_from {
                        Some(p) => v.at = p,
                        None => break,
                    }
                }
                let f = v.frames[v.at];
                s[0] += f[0] as i64 * v.gain as i64;
                s[1] += f[1] as i64 * v.gain as i64;
                v.at += 1;
            }
            if v.loop_from.is_some() && v.at >= len {
                v.at = v.loop_from.unwrap_or(0);
            }
        }
        self.voices.retain(|v| v.at < v.frames.len());
        for (o, s) in out.chunks_exact_mut(2).zip(&sum) {
            o[0] = (s[0] >> 16).clamp(i16::MIN as i64, i16::MAX as i64) as i16;
            o[1] = (s[1] >> 16).clamp(i16::MIN as i64, i16::MAX as i64) as i16;
        }
    }
}

/// The 16.16 gain of a level from 0 to 255 (a sound effect's level: linear loudness, our choice).
pub fn level_gain(level: u8) -> i32 {
    (level as i32 * UNITY + 127) / 255
}

/// The 16.16 gain of an attenuation in hundredths of a decibel; [`crate::music::SILENT`] and below are silence.
pub fn attenuation_gain(centi: i32) -> i32 {
    if centi <= crate::music::SILENT {
        return 0;
    }
    (10f64.powf(centi as f64 / 2000.0) * UNITY as f64).round() as i32
}

/// The mixer as the audio output reaches it: from another thread.
#[derive(Clone, Default)]
pub struct Output(Arc<Mutex<Mixer>>);

impl Output {
    /// Fills `out` with the next mixed samples (interleaved stereo at [`SAMPLE_RATE`]).
    pub fn mix(&self, out: &mut [i16]) {
        self.lock().mix(out);
    }

    fn lock(&self) -> MutexGuard<'_, Mixer> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// How a started sound plays, as the frame snapshot's sound lines name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Loops until stopped (a looping music track).
    Loop,
    /// Plays once and stays open (a music track).
    Once,
    /// Plays once and goes (sound effects, a screen's sounds).
    Despawn,
}

/// A sound started, for the frame snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Start {
    pub hash: u64,
    /// The loudness as a linear factor.
    pub volume: f32,
    pub mode: Mode,
    pub group: Group,
}

/// The core's side of the sound: starts and stops voices in the shared mixer and notes the starts.
#[derive(Clone, Default)]
pub struct Audio {
    output: Output,
    /// The sounds started since the last [`Audio::take_starts`].
    starts: Vec<Start>,
}

impl Audio {
    /// The mixer as the audio output reaches it.
    pub fn output(&self) -> Output {
        self.output.clone()
    }

    /// Starts `sound` at a `volume` (linear) and `gain` (16.16), `start` sample frames in.
    pub fn play(&mut self, sound: &Sound, volume: f32, gain: i32, mode: Mode, group: Group, start: usize) -> VoiceId {
        self.starts.push(Start {
            hash: sound.hash,
            volume,
            mode,
            group,
        });
        self.output.lock().play(sound, gain, None, start, group)
    }

    /// Starts a music track: looping from `loop_from`, or once.
    pub fn play_music(&mut self, sound: &Sound, volume: f32, gain: i32, loop_from: Option<usize>) -> VoiceId {
        self.starts.push(Start {
            hash: sound.hash,
            volume,
            mode: if loop_from.is_some() { Mode::Loop } else { Mode::Once },
            group: Group::Music,
        });
        self.output.lock().play(sound, gain, loop_from, 0, Group::Music)
    }

    pub fn stop(&mut self, id: VoiceId) {
        self.output.lock().stop(id);
    }

    pub fn stop_group(&mut self, group: Group) {
        self.output.lock().stop_group(group);
    }

    /// A screen is left: its own sounds stop (and the act screen's sound with the act screen).
    pub fn stop_screen(&mut self, state: AppState) {
        let mut m = self.output.lock();
        m.stop_group(Group::Screen(state));
        if state == AppState::Act {
            m.stop_group(Group::Act);
        }
    }

    pub fn set_gain(&mut self, id: VoiceId, gain: i32) {
        self.output.lock().set_gain(id, gain);
    }

    #[allow(dead_code)] // the music reads it once the mixer plays (step 7's switch)
    pub fn position(&self, id: VoiceId) -> Option<usize> {
        self.output.lock().position(id)
    }

    /// The sounds started since the last call.
    pub fn take_starts(&mut self) -> Vec<Start> {
        std::mem::take(&mut self.starts)
    }
}

/// The mixed sound's fingerprints for the audio snapshot: the samples in blocks of one second, one fingerprint each.
#[derive(Default)]
pub struct Fingerprints {
    /// Equal blocks in a row fold into one line: first block, last block, fingerprint.
    runs: Vec<(u32, u32, u64)>,
    block: Vec<u8>,
    blocks: u32,
}

/// Bytes in a block: one second of 16-bit stereo.
const BLOCK_BYTES: usize = SAMPLE_RATE as usize * 4;

impl Fingerprints {
    /// Adds mixed samples (interleaved stereo).
    pub fn add(&mut self, samples: &[i16]) {
        for s in samples {
            self.block.extend_from_slice(&s.to_le_bytes());
            if self.block.len() == BLOCK_BYTES {
                let h = fnv64(&self.block);
                self.block.clear();
                let n = self.blocks;
                self.blocks += 1;
                match self.runs.last_mut() {
                    Some((_, last, hash)) if *hash == h => *last = n,
                    _ => self.runs.push((n, n, h)),
                }
            }
        }
    }

    /// The fingerprint file: a line per run of equal blocks, then the last part block's length and fingerprint.
    pub fn text(&self, scene: &str) -> String {
        let mut out = format!(
            "# mixed sound fingerprints of the scripted scene `{scene}` (hashes only): {SAMPLE_RATE} Hz stereo \
             16-bit, one-second blocks\n"
        );
        for (a, b, h) in &self.runs {
            out.push_str(&format!("blocks\t{a}-{b}\t{h:016x}\n"));
        }
        out.push_str(&format!(
            "tail\t{}\t{:016x}\n",
            self.block.len() / 4,
            fnv64(&self.block)
        ));
        out
    }
}

/// Sample frames to mix for frame `n` (from 0) of a run at `fps` frames a second: the whole frames the run's time
/// reaches, so the fractions carry from frame to frame.
pub fn frames_for(n: u64, fps: u64) -> usize {
    let at = |n: u64| n * SAMPLE_RATE as u64 / fps;
    (at(n + 1) - at(n)) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, channels: u16, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut w = b"RIFF\0\0\0\0WAVE".to_vec();
        w.extend_from_slice(b"fmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&channels.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * channels as u32 * bits as u32 / 8).to_le_bytes());
        w.extend_from_slice(&(channels * bits / 8).to_le_bytes());
        w.extend_from_slice(&bits.to_le_bytes());
        w.extend_from_slice(b"LIST\x03\0\0\0abc\0");
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(data);
        w
    }

    fn sound(frames: &[Frame]) -> Sound {
        Sound {
            hash: 0,
            frames: frames.to_vec().into(),
        }
    }

    #[test]
    fn decodes_16_bit_stereo_and_8_bit_mono() {
        let d: Vec<u8> = [100i16, -200, 300, -400].iter().flat_map(|s| s.to_le_bytes()).collect();
        assert_eq!(
            decode_wav(&wav(22_050, 2, 16, &d)),
            Some(vec![[100, -200], [300, -400]])
        );
        assert_eq!(
            decode_wav(&wav(22_050, 1, 8, &[128, 255, 0])),
            Some(vec![[0, 0], [32_512, 32_512], [-32_768, -32_768]])
        );
        assert_eq!(decode_wav(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(decode_wav(&wav(22_050, 1, 24, &[0, 0, 0])), None);
    }

    #[test]
    fn other_rates_are_resampled_linearly() {
        let d: Vec<u8> = [0i16, 1000, 2000].iter().flat_map(|s| s.to_le_bytes()).collect();
        let f = decode_wav(&wav(11_025, 1, 16, &d)).unwrap();
        assert_eq!(
            f.iter().map(|f| f[0]).collect::<Vec<_>>(),
            [0, 500, 1000, 1500, 2000, 2000]
        );
        let g = decode_wav(&wav(44_100, 1, 16, &d)).unwrap();
        assert_eq!(
            g.iter().map(|f| f[0]).collect::<Vec<_>>(),
            [0],
            "three frames at twice the rate: one and a half"
        );
    }

    #[test]
    fn voices_sum_at_their_gain_and_clip() {
        let mut m = Mixer::default();
        m.play(&sound(&[[1000, -1000]; 4]), UNITY, None, 0, Group::Effect);
        m.play(&sound(&[[30_000, -30_000]; 2]), UNITY / 2, None, 0, Group::Effect);
        let mut out = [0i16; 6];
        m.mix(&mut out);
        assert_eq!(out, [16_000, -16_000, 16_000, -16_000, 1000, -1000]);
        m.play(&sound(&[[30_000, -30_000]; 1]), UNITY, None, 0, Group::Effect);
        let mut out = [0i16; 2];
        m.mix(&mut out);
        assert_eq!(out, [31_000, -31_000]);
        m.play(&sound(&[[30_000, -30_000]; 1]), UNITY * 2, None, 0, Group::Effect);
        m.mix(&mut out);
        assert_eq!(out, [i16::MAX, i16::MIN], "clipped");
    }

    #[test]
    fn a_voice_ends_and_is_gone() {
        let mut m = Mixer::default();
        let id = m.play(&sound(&[[1, 1]; 3]), UNITY, None, 0, Group::Effect);
        let mut out = [0i16; 4];
        m.mix(&mut out);
        assert_eq!(m.position(id), Some(2));
        m.mix(&mut out);
        assert_eq!(out, [1, 1, 0, 0]);
        assert_eq!(m.position(id), None);
        assert_eq!(m.playing(), 0);
    }

    #[test]
    fn a_loop_goes_on_from_its_point_without_a_gap() {
        let mut m = Mixer::default();
        let s = sound(&[[1, 1], [2, 2], [3, 3]]);
        let id = m.play(&s, UNITY, Some(1), 0, Group::Music);
        let mut out = [0i16; 14];
        m.mix(&mut out);
        let left: Vec<i16> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, [1, 2, 3, 2, 3, 2, 3]);
        assert_eq!(m.position(id), Some(1), "the position wraps to the loop point");
        let whole = m.play(&s, UNITY, Some(0), 4, Group::Music);
        assert_eq!(m.position(whole), Some(1), "a start past the end wraps round the loop");
    }

    #[test]
    fn a_start_part_way_in_and_past_the_end() {
        let mut m = Mixer::default();
        let s = sound(&[[1, 1], [2, 2], [3, 3]]);
        let id = m.play(&s, UNITY, None, 2, Group::Screen(AppState::Video));
        let mut out = [0i16; 4];
        m.mix(&mut out);
        assert_eq!(out, [3, 3, 0, 0]);
        let late = m.play(&s, UNITY, None, 5, Group::Effect);
        assert_eq!(m.position(late), None);
        assert_eq!(m.position(id), None);
    }

    #[test]
    fn groups_stop_together() {
        let mut a = Audio::default();
        let s = sound(&[[1, 1]; 10]);
        let e = a.play(&s, 1.0, UNITY, Mode::Despawn, Group::Effect, 0);
        let v = a.play(&s, 1.0, UNITY, Mode::Despawn, Group::Screen(AppState::Act), 0);
        let act = a.play(&s, 1.0, UNITY, Mode::Despawn, Group::Act, 0);
        let m = a.play_music(&s, 1.0, UNITY, None);
        a.stop_screen(AppState::Act);
        assert_eq!((a.position(v), a.position(act)), (None, None));
        a.stop_group(Group::Effect);
        assert_eq!(a.position(e), None);
        assert_eq!(a.position(m), Some(0));
        let starts = a.take_starts();
        assert_eq!(starts.len(), 4);
        assert_eq!(starts[3].mode, Mode::Once);
        assert!(a.take_starts().is_empty());
    }

    #[test]
    fn gains() {
        assert_eq!(level_gain(255), UNITY);
        assert_eq!(level_gain(0), 0);
        assert_eq!(attenuation_gain(0), UNITY);
        assert_eq!(attenuation_gain(-600), 32_846, "-6 dB is about a half");
        assert_eq!(attenuation_gain(crate::music::SILENT), 0);
    }

    #[test]
    fn a_frame_mixes_its_share_of_the_second() {
        let total: usize = (0..60).map(|n| frames_for(n, 60)).sum();
        assert_eq!(total, 22_050);
        assert_eq!((frames_for(0, 60), frames_for(1, 60)), (367, 368));
    }

    #[test]
    fn fingerprints_fold_equal_blocks() {
        let mut f = Fingerprints::default();
        f.add(&vec![0i16; SAMPLE_RATE as usize * 2 * 3 + 4]);
        let t = f.text("s");
        let lines: Vec<&str> = t.lines().skip(1).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("blocks\t0-2\t"));
        assert!(lines[1].starts_with("tail\t2\t"));
    }
}
