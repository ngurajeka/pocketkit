//! Sample-rate conversion, an FFT and the spectrum analyzer's bands.

use std::f32::consts::PI;

/// Linear-interpolating resampler for interleaved stereo. Good enough for music
/// on a handheld speaker; skipped entirely when the rates already match.
#[derive(Debug, Clone)]
pub struct Resampler {
    step: f64,
    pos: f64,
    prev: [f32; 2],
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Self { step: from as f64 / to as f64, pos: 0.0, prev: [0.0; 2] }
    }

    pub fn passthrough(&self) -> bool {
        self.step == 1.0
    }

    pub fn reset(&mut self) {
        self.pos = 0.0;
        self.prev = [0.0; 2];
    }

    /// Appends the resampled version of `input` (interleaved stereo) to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = input.len() / 2;
        if self.passthrough() {
            out.extend_from_slice(&input[..n * 2]);
            return;
        }
        // Virtual stream: prev frame at index 0, then the input frames.
        while self.pos < n as f64 {
            let i = self.pos as usize;
            let t = (self.pos - i as f64) as f32;
            let a = if i == 0 { self.prev } else { [input[(i - 1) * 2], input[(i - 1) * 2 + 1]] };
            let b = [input[i * 2], input[i * 2 + 1]];
            out.push(a[0] + (b[0] - a[0]) * t);
            out.push(a[1] + (b[1] - a[1]) * t);
            self.pos += self.step;
        }
        self.pos -= n as f64;
        if n > 0 {
            self.prev = [input[(n - 1) * 2], input[(n - 1) * 2 + 1]];
        }
    }
}

/// Mixes any channel count down/up to interleaved stereo.
pub fn to_stereo(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => samples.iter().for_each(|&s| out.extend_from_slice(&[s, s])),
        2 => out.extend_from_slice(samples),
        c => samples.chunks_exact(c).for_each(|f| out.extend_from_slice(&f[..2])),
    }
}

/// Converts with volume (0.0-1.0) and clipping.
pub fn to_i16(samples: &[f32], volume: f32, out: &mut Vec<i16>) {
    // Squared curve: the volume steps feel even to the ear.
    let gain = volume * volume;
    out.extend(samples.iter().map(|&s| ((s * gain).clamp(-1.0, 1.0) * 32767.0) as i16));
}

/// In-place radix-2 FFT; `re.len()` must be a power of two.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

pub const FFT_SIZE: usize = 1024;

/// Levels 0.0-1.0 for `bands` log-spaced bands between 50 Hz and 16 kHz.
pub fn spectrum(samples: &[f32], rate: u32, bands: usize) -> Vec<f32> {
    let n = FFT_SIZE;
    let mut re = vec![0.0; n];
    let mut im = vec![0.0; n];
    let take = samples.len().min(n);
    for (i, &s) in samples[samples.len() - take..].iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * PI * i as f32 / (n - 1) as f32).cos(); // Hann
        re[i] = s * w;
    }
    fft(&mut re, &mut im);
    let bin_hz = rate as f32 / n as f32;
    let (lo, hi) = (50.0f32, 16000.0f32.min(rate as f32 / 2.0));
    (0..bands)
        .map(|b| {
            let f0 = lo * (hi / lo).powf(b as f32 / bands as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / bands as f32);
            let k0 = ((f0 / bin_hz) as usize).max(1);
            let k1 = ((f1 / bin_hz) as usize).max(k0 + 1).min(n / 2);
            let peak = (k0..k1).map(|k| (re[k] * re[k] + im[k] * im[k]).sqrt()).fold(0.0, f32::max);
            // Normalise: a full-scale sine with a Hann window peaks around n/4.
            let db = 20.0 * (peak / (n as f32 / 4.0)).max(1e-6).log10();
            ((db + 60.0) / 60.0).clamp(0.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames).map(|i| (2.0 * PI * freq * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn resamples_length_and_shape() {
        let input: Vec<f32> = sine(440.0, 48000, 4800).into_iter().flat_map(|s| [s, s]).collect();
        let mut r = Resampler::new(48000, 44100);
        let mut out = Vec::new();
        for chunk in input.chunks(1000) {
            r.process(chunk, &mut out);
        }
        let frames = out.len() / 2;
        assert!((frames as i64 - 4410).abs() <= 2, "{frames}");
        assert!(out.iter().all(|s| s.abs() <= 1.0001));
        let same = Resampler::new(44100, 44100);
        assert!(same.passthrough());
    }

    #[test]
    fn stereo_and_volume() {
        let mut out = Vec::new();
        to_stereo(&[0.5, -0.5], 1, &mut out);
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
        out.clear();
        to_stereo(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 3, &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.4, 0.5]);
        let mut pcm = Vec::new();
        to_i16(&[1.0, -2.0, 0.5], 1.0, &mut pcm);
        assert_eq!(pcm, vec![32767, -32767, 16383]);
        pcm.clear();
        to_i16(&[1.0], 0.5, &mut pcm);
        assert_eq!(pcm, vec![8191]);
    }

    #[test]
    fn spectrum_finds_the_tone() {
        let bands = spectrum(&sine(1000.0, 44100, FFT_SIZE), 44100, 20);
        let loudest = bands.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        // The log midpoint of 50 Hz-16 kHz is ~900 Hz, so 1 kHz lands in band 10 of 20.
        assert!((9..=11).contains(&loudest), "{loudest} {bands:?}");
        assert!(bands[loudest] > 0.8);
        assert!(spectrum(&[0.0; 10], 44100, 20).iter().all(|&b| b == 0.0));
    }
}
