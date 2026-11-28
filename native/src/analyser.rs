//! The browser's `AnalyserNode.getByteFrequencyData`, rebuilt so the demos hear the music the
//! same way here as in Chromium: fftSize 1024, a Blackman window, smoothing over time, then
//! decibels mapped onto 0..255 between minDecibels (-100) and maxDecibels (-30).
//! (Web Audio API spec, "AnalyserNode: FFT windowing and smoothing over time".)

pub const FFT: usize = 1024;
pub const BINS: usize = FFT / 2;
/// What ufo.js sets on every analyser it makes.
pub const SMOOTHING: f32 = 0.55;
const MIN_DB: f32 = -100.0;
const MAX_DB: f32 = -30.0;

pub struct Analyser {
    window: Vec<f32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
    smooth: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
}

impl Default for Analyser {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyser {
    pub fn new() -> Self {
        use std::f32::consts::TAU;
        let n = FFT as f32;
        let window = (0..FFT)
            .map(|i| {
                let x = i as f32 / n;
                0.42 - 0.5 * (TAU * x).cos() + 0.08 * (2.0 * TAU * x).cos()
            })
            .collect();
        Analyser {
            window,
            cos: (0..FFT / 2).map(|k| (TAU * k as f32 / n).cos()).collect(),
            sin: (0..FFT / 2).map(|k| -(TAU * k as f32 / n).sin()).collect(),
            smooth: vec![0.0; BINS],
            re: vec![0.0; FFT],
            im: vec![0.0; FFT],
        }
    }

    /// The latest `FFT` samples (mono, -1..1, oldest first) in, 512 bytes out.
    pub fn bytes(&mut self, samples: &[f32], out: &mut [u8; BINS]) {
        debug_assert_eq!(samples.len(), FFT);
        for i in 0..FFT {
            // bit-reversed order for the in-place FFT below
            let j = (i as u32).reverse_bits() >> (32 - FFT.trailing_zeros());
            self.re[j as usize] = samples[i] * self.window[i];
            self.im[j as usize] = 0.0;
        }
        let mut len = 2;
        while len <= FFT {
            let step = FFT / len;
            for start in (0..FFT).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let (a, b) = (start + k, start + k + len / 2);
                    let tr = self.re[b] * wr - self.im[b] * wi;
                    let ti = self.re[b] * wi + self.im[b] * wr;
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
            }
            len *= 2;
        }
        let scale = 255.0 / (MAX_DB - MIN_DB);
        for k in 0..BINS {
            let mag = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() / FFT as f32;
            let s = SMOOTHING * self.smooth[k] + (1.0 - SMOOTHING) * mag;
            // NaN or infinity (a broken stream) would stick forever in the smoothing
            self.smooth[k] = if s.is_finite() { s } else { 0.0 };
            let db = 20.0 * self.smooth[k].max(1e-12).log10();
            out[k] = (scale * (db - MIN_DB)).clamp(0.0, 255.0) as u8;
        }
    }

    /// Silence in: what the browser analyser shows once the sound stops.
    pub fn decay(&mut self, out: &mut [u8; BINS]) {
        let zeros = [0.0; FFT];
        self.bytes(&zeros, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, amp: f32, rate: f32) -> Vec<f32> {
        (0..FFT).map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / rate).sin()).collect()
    }

    #[test]
    fn a_tone_lands_in_its_bin() {
        let mut a = Analyser::new();
        let mut out = [0; BINS];
        // 46.875 Hz per bin at 48 kHz: 1500 Hz is bin 32. 0.01 is about -53 dB in its bin
        // (Blackman's coherent gain 0.42, halved for one side), 170 or so on the byte scale.
        let s = tone(1500.0, 0.01, 48_000.0);
        for _ in 0..20 {
            a.bytes(&s, &mut out);
        }
        let peak = (0..BINS).max_by_key(|&k| out[k]).unwrap();
        assert_eq!(peak, 32);
        assert!((160..180).contains(&out[32]), "{}", out[32]);
        // louder than maxDecibels (-30 dB) is the top of the range
        a.bytes(&tone(1500.0, 0.5, 48_000.0), &mut out);
        a.bytes(&tone(1500.0, 0.5, 48_000.0), &mut out);
        assert_eq!(out[32], 255);
        // far away from the tone, the Blackman window keeps the floor low
        assert!(out[200] < 40, "{}", out[200]);
    }

    #[test]
    fn silence_is_zero_and_smoothing_decays() {
        let mut a = Analyser::new();
        let mut out = [0; BINS];
        a.bytes(&tone(200.0, 0.01, 48_000.0), &mut out);
        let loud = out[4];
        assert!(loud > 0);
        a.decay(&mut out);
        let after = out[4];
        assert!(after > 0 && after < loud, "{loud} -> {after}");
        for _ in 0..200 {
            a.decay(&mut out);
        }
        assert!(out.iter().all(|&b| b == 0));
    }

    #[test]
    fn broken_input_does_not_stick() {
        let mut a = Analyser::new();
        let mut out = [0; BINS];
        let mut s = vec![0.0; FFT];
        s[10] = f32::NAN;
        a.bytes(&s, &mut out);
        a.decay(&mut out);
        assert!(out.iter().all(|&b| b == 0));
    }
}

#[cfg(test)]
mod compare {
    use super::*;

    /// The same signals through Chromium's AnalyserNode (an OfflineAudioContext at 48 kHz, an
    /// oscillator and a gain, getByteFrequencyData every 0.05 s from 0.5 s, the 10th kept)
    /// gave exactly these bytes in November 2026.
    #[test]
    fn matches_chromium() {
        let chromium: [(f32, f32, usize, [u8; 14]); 4] = [
            (1500.0, 0.01, 26, [0, 0, 0, 0, 94, 152, 169, 152, 94, 0, 0, 0, 0, 0]),
            (1500.0, 0.2, 26, [0, 0, 0, 0, 189, 247, 255, 247, 189, 0, 0, 0, 0, 0]),
            (200.0, 0.05, 0, [9, 0, 117, 193, 218, 211, 166, 37, 0, 0, 0, 0, 0, 0]),
            (60.0, 0.3, 0, [234, 255, 255, 224, 93, 66, 55, 42, 29, 18, 7, 0, 0, 0]),
        ];
        for (f, amp, from, want) in chromium {
            let mut a = Analyser::new();
            let mut out = [0; BINS];
            for i in 0..10 {
                // 0.05 s apart, as the page samples it
                // Chromium suspends at the next 128-frame render quantum boundary
                let end = ((0.5 + i as f64 * 0.05) * 48_000.0 / 128.0).ceil() as usize * 128;
                let s: Vec<f32> = (end - FFT..end).map(|n| amp * (std::f64::consts::TAU * f as f64 * n as f64 / 48_000.0).sin() as f32).collect();
                a.bytes(&s, &mut out);
            }
            let got = &out[from..from + 14];
            // one step either way is float rounding in a different FFT
            assert!(got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 1), "{f} Hz {amp}: {got:?} != {want:?}");
        }
    }
}
