use std::f64::consts::PI;

use sdl2::audio::{AudioQueue, AudioSpecDesired};
use sdl2::AudioSubsystem;

pub struct SoundSystem {
    queue: Option<AudioQueue<i16>>,
    nav_tick: Vec<i16>,
    launch_confirm: Vec<i16>,
    details_confirm: Vec<i16>,
    back_thud: Vec<i16>,
}

impl SoundSystem {
    pub fn new(audio_sub: Option<&AudioSubsystem>) -> Self {
        let queue = audio_sub.and_then(|sub| {
            let spec = AudioSpecDesired {
                freq: Some(22050),
                channels: Some(1),
                samples: Some(512),
            };
            let q = sub.open_queue::<i16, _>(None, &spec).ok()?;
            q.resume();
            Some(q)
        });

        Self {
            queue,
            nav_tick: Self::synth_sine(22050, 0.03, 320.0, 320.0, 0.12, 0.5),
            launch_confirm: Self::synth_sine(22050, 0.25, 440.0, 880.0, 0.18, 0.3),
            details_confirm: Self::synth_sine(22050, 0.10, 180.0, 180.0, 0.12, 0.5),
            back_thud: Self::synth_sine(22050, 0.15, 150.0, 150.0, 0.15, 0.6),
        }
    }

    fn synth_sine(
        sample_rate: usize,
        duration: f64,
        freq_start: f64,
        freq_end: f64,
        amp: f64,
        fade_ratio: f64,
    ) -> Vec<i16> {
        let n = (sample_rate as f64 * duration) as usize;
        if n == 0 {
            return Vec::new();
        }
        let fade_len = ((n as f64) * fade_ratio) as usize;
        let fade_start = n.saturating_sub(fade_len);
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let t = (i as f64) / (sample_rate as f64);
            let frac = if n > 1 {
                (i as f64) / ((n - 1) as f64)
            } else {
                0.0
            };
            let freq = freq_start + (freq_end - freq_start) * frac;
            let mut sample = (2.0 * PI * freq * t).sin() * amp;
            if i >= fade_start && fade_len > 1 {
                let fade_idx = i - fade_start;
                let fade = 1.0 - (fade_idx as f64) / ((fade_len - 1) as f64);
                sample *= fade;
            }
            out.push((sample * 32767.0).clamp(-32767.0, 32767.0) as i16);
        }
        out
    }

    fn play(&self, buf: &[i16]) {
        if let Some(ref q) = self.queue {
            q.clear();
            let _ = q.queue_audio(buf);
        }
    }

    pub fn play_nav(&self) {
        self.play(&self.nav_tick);
    }

    pub fn play_launch(&self) {
        self.play(&self.launch_confirm);
    }

    pub fn play_details_confirm(&self) {
        self.play(&self.details_confirm);
    }

    pub fn play_back(&self) {
        self.play(&self.back_thud);
    }
}
