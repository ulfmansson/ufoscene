//! Turns 512 FFT bins from the browser's AnalyserNode into a handful of
//! musical control signals: bass / mid / treble levels, beats and tempo.
//! When there is no signal it switches to an "autopilot" at 125 BPM so the
//! demo keeps moving.

pub const BINS: usize = 512;
const AUTOPILOT_BPM: f32 = 125.0;

pub struct Audio {
    pub bins: [u8; BINS],
    /// Smoothed, auto-gained levels in 0..1.
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
    /// True on the frame a beat was detected.
    pub beat: bool,
    /// 1.0 on a beat, decaying towards 0.
    pub pulse: f32,
    pub beats: u32,
    /// Estimated seconds between beats.
    pub period: f32,
    pub autopilot: bool,
    avg_bass: f32,
    peak: [f32; 3],
    last_beat: f32,
    last_signal: f32,
    auto_next: f32,
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    pub fn new() -> Self {
        Audio {
            bins: [0; BINS],
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            beat: false,
            pulse: 0.0,
            beats: 0,
            period: 60.0 / AUTOPILOT_BPM,
            autopilot: true,
            avg_bass: 0.0,
            peak: [0.2; 3],
            last_beat: -10.0,
            last_signal: -10.0,
            auto_next: 0.0,
        }
    }

    fn band(&self, a: usize, b: usize) -> f32 {
        let sum: u32 = self.bins[a..b].iter().map(|&v| v as u32).sum();
        sum as f32 / ((b - a) as f32 * 255.0)
    }

    /// 0..1 position inside the current beat at time `t` (0 right on the beat).
    pub fn phase(&self, t: f32) -> f32 {
        ((t - self.last_beat) / self.period).clamp(0.0, 1.0)
    }

    pub fn update(&mut self, t: f32, dt: f32, has_audio: bool) {
        self.beat = false;
        // With fftSize 1024 at 44.1/48 kHz each bin is ~45 Hz wide.
        let raw = [self.band(1, 7), self.band(7, 60), self.band(60, 300)];
        if has_audio && raw[0] + raw[1] + raw[2] > 0.02 {
            self.last_signal = t;
        }
        self.autopilot = t - self.last_signal > 1.5;

        let target = if self.autopilot {
            let p = 60.0 / AUTOPILOT_BPM;
            if t >= self.auto_next {
                self.auto_next = if t - self.auto_next > p { t + p } else { self.auto_next + p };
                self.beat = true;
            }
            let ph = (1.0 - (self.auto_next - t) / p).clamp(0.0, 1.0);
            let kick = (1.0 - ph).powi(3);
            [
                0.25 + 0.75 * kick,
                0.55 + 0.3 * (t * 0.9).sin(),
                0.45 + 0.35 * (t * 1.7 + 1.0).sin(),
            ]
        } else {
            let mut out = [0.0; 3];
            for i in 0..3 {
                // auto-gain: follow the recent peak, release slowly
                self.peak[i] = raw[i].max(self.peak[i] * (1.0 - dt * 0.12)).max(0.04);
                out[i] = (raw[i] / self.peak[i]).clamp(0.0, 1.0);
            }
            let since = t - self.last_beat;
            if raw[0] > self.avg_bass * 1.25 + 0.015 && raw[0] > 0.08 && since > 0.28 {
                self.beat = true;
                if since < 1.2 {
                    self.period += (since - self.period) * 0.2;
                }
            }
            let k = 1.0 - (-dt * 3.0).exp();
            self.avg_bass += (raw[0] - self.avg_bass) * k;
            out
        };

        let follow = |v: &mut f32, target: f32| {
            if target > *v {
                *v = target;
            } else {
                *v += (target - *v) * (1.0 - (-dt * 7.0).exp());
            }
        };
        follow(&mut self.bass, target[0]);
        follow(&mut self.mid, target[1]);
        follow(&mut self.treble, target[2]);

        if self.beat {
            self.beats += 1;
            self.pulse = 1.0;
            self.last_beat = t;
        }
        if self.autopilot {
            self.period = 60.0 / AUTOPILOT_BPM;
        }
        self.period = self.period.clamp(0.3, 1.0);
        self.pulse *= (-dt * 5.0).exp();
    }
}
