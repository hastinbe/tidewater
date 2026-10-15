//! The sound device for the miniquad shell, which has none of its own: a `cpal` output stream at the device's own
//! rate, whose callback pulls the core's mixer (`audio::Output::mix`, 22,050 Hz stereo) [`CHUNK`] sample frames at
//! a time and resamples it by linear interpolation. (`quad-snd`, miniquad's sound crate, only plays whole loaded
//! sounds; it has no callback to stream a mixer through.)

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tidewater_client::audio::{Output, SAMPLE_RATE};

/// Sample frames pulled from the mixer at a time: about 11.6 ms.
pub const CHUNK: usize = 256;

/// The device buffer asked for, in sample frames at the device's rate: about 10.7 ms at 48 kHz.
pub const DEVICE_FRAMES: u32 = 512;

/// The mixer's sound at another rate: linear interpolation between its sample frames.
pub struct Resampler {
    /// Mixer frames per device frame.
    step: f64,
    /// Where the next device frame falls between `prev` (0) and `next` (1).
    pos: f64,
    prev: [i16; 2],
    next: [i16; 2],
    chunk: Vec<i16>,
    at: usize,
}

impl Resampler {
    pub fn new(device_rate: u32) -> Self {
        Resampler {
            step: SAMPLE_RATE as f64 / device_rate as f64,
            pos: 1.0,
            prev: [0; 2],
            next: [0; 2],
            chunk: vec![0; CHUNK * 2],
            at: CHUNK * 2,
        }
    }

    /// The next device frame, left and right, from -1 to 1; `mix` fills a chunk of the mixer's samples.
    pub fn frame(&mut self, mix: &mut impl FnMut(&mut [i16])) -> [f32; 2] {
        while self.pos >= 1.0 {
            if self.at == self.chunk.len() {
                mix(&mut self.chunk);
                self.at = 0;
            }
            self.prev = self.next;
            self.next = [self.chunk[self.at], self.chunk[self.at + 1]];
            self.at += 2;
            self.pos -= 1.0;
        }
        let t = self.pos as f32;
        self.pos += self.step;
        let lerp = |a: i16, b: i16| (a as f32 + (b as f32 - a as f32) * t) / 32_768.0;
        [lerp(self.prev[0], self.next[0]), lerp(self.prev[1], self.next[1])]
    }
}

/// What the callback has done, for measuring.
#[derive(Default)]
pub struct Stats {
    pub calls: AtomicU64,
    /// Device frames written.
    pub frames: AtomicU64,
    pub first_ns: AtomicU64,
    pub last_ns: AtomicU64,
    pub max_gap_ns: AtomicU64,
}

impl Stats {
    fn call(&self, epoch: Instant, frames: usize) {
        let now = epoch.elapsed().as_nanos() as u64;
        if self.calls.fetch_add(1, Ordering::Relaxed) == 0 {
            self.first_ns.store(now, Ordering::Relaxed);
        } else {
            let gap = now.saturating_sub(self.last_ns.load(Ordering::Relaxed));
            self.max_gap_ns.fetch_max(gap, Ordering::Relaxed);
        }
        self.last_ns.store(now, Ordering::Relaxed);
        self.frames.fetch_add(frames as u64, Ordering::Relaxed);
    }
}

/// The open device: the stream plays while this lives.
pub struct Device {
    stream: cpal::Stream,
    pub rate: u32,
    pub channels: u16,
    /// The buffer asked for, if the device allows it.
    pub asked: Option<u32>,
}

impl Device {
    /// The stream's buffer in frames per callback, as the device reports it.
    pub fn buffer_size(&self) -> Option<u32> {
        self.stream.buffer_size().ok()
    }
}

/// Opens the default output device at its own rate and starts pulling `output`.
pub fn open(output: Output, stats: Arc<Stats>) -> Result<Device> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or_else(|| anyhow!("no output device"))?;
    let supported = device.default_output_config()?;
    let format = supported.sample_format();
    let asked = match supported.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } if (*min..=*max).contains(&DEVICE_FRAMES) => Some(DEVICE_FRAMES),
        _ => None,
    };
    let mut config = supported.config();
    config.buffer_size = asked.map_or(cpal::BufferSize::Default, cpal::BufferSize::Fixed);
    let (rate, channels) = (config.sample_rate, config.channels);
    let mut resampler = Resampler::new(rate);
    let mut mix = move |buf: &mut [i16]| output.mix(buf);
    let epoch = Instant::now();
    let ch = channels as usize;
    // Each device frame: left and right on the first two channels, both on a single one, silence on the rest.
    let mut fill = move |frames: usize, put: &mut dyn FnMut(usize, f32)| {
        stats.call(epoch, frames);
        for i in 0..frames {
            let [l, r] = resampler.frame(&mut mix);
            for c in 0..ch {
                let v = match (ch, c) {
                    (1, _) => (l + r) / 2.0,
                    (_, 0) => l,
                    (_, 1) => r,
                    _ => 0.0,
                };
                put(i * ch + c, v);
            }
        }
    };
    let err = |e| eprintln!("sound device: {e}");
    let stream = match format {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| fill(data.len() / ch, &mut |i, v| data[i] = v),
            err,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                fill(data.len() / ch, &mut |i, v| data[i] = (v * 32_767.0) as i16)
            },
            err,
            None,
        )?,
        f => return Err(anyhow!("the device's sample format {f:?} isn't handled")),
    };
    stream.play()?;
    Ok(Device {
        stream,
        rate,
        channels,
        asked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mixer whose left channel counts up by 10 and right channel down.
    fn ramp() -> impl FnMut(&mut [i16]) {
        let mut n = 0i16;
        move |buf: &mut [i16]| {
            for f in buf.chunks_mut(2) {
                f[0] = n * 10;
                f[1] = -n * 10;
                n += 1;
            }
        }
    }

    #[test]
    fn at_the_mixer_s_rate_every_frame_comes_through() {
        let mut r = Resampler::new(SAMPLE_RATE);
        let mut mix = ramp();
        let got: Vec<[f32; 2]> = (0..600).map(|_| r.frame(&mut mix)).collect();
        // The first frame is the silence before the sound; then the mixer's frames in order, across chunks.
        assert_eq!(got[0], [0.0, 0.0]);
        for (i, f) in got[1..].iter().enumerate() {
            assert_eq!(f[0], (i as f32 * 10.0) / 32_768.0, "frame {i}");
            assert_eq!(f[1], -(i as f32 * 10.0) / 32_768.0, "frame {i}");
        }
    }

    #[test]
    fn at_twice_the_rate_frames_fall_halfway() {
        let mut r = Resampler::new(SAMPLE_RATE * 2);
        let mut mix = ramp();
        let left: Vec<f32> = (0..9).map(|_| r.frame(&mut mix)[0] * 32_768.0).collect();
        assert_eq!(left, [0.0, 0.0, 0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0]);
    }
}
