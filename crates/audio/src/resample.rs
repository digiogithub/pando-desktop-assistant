//! Streaming mono resampler on top of rubato 5.
//!
//! Fixed-ratio FFT resampling (`Fft`, 10 ms chunks) whenever it can be built, which covers the
//! usual 48 / 44.1 / 32 / 24 / 22.05 / 16 kHz combinations; otherwise an asynchronous sinc
//! resampler. Runs on supervisor threads only (it allocates on construction).

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{
    Async, Fft, FixedAsync, FixedSync, Resampler, SincInterpolationParameters,
    SincInterpolationType, WindowFunction,
};

use crate::AudioError;

/// Converts a mono stream from one rate to another, any input block size.
pub struct StreamResampler {
    inner: Option<Box<dyn Resampler<f32>>>,
    pending: Vec<f32>,
    out: Vec<f32>,
    from: u32,
    to: u32,
}

impl std::fmt::Debug for StreamResampler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StreamResampler({} -> {} Hz)", self.from, self.to)
    }
}

fn err(e: impl std::fmt::Display) -> AudioError {
    AudioError::Resample(e.to_string())
}

impl StreamResampler {
    pub fn new(from: u32, to: u32) -> Result<Self, AudioError> {
        if from == 0 || to == 0 {
            return Err(err("zero sample rate"));
        }
        let inner: Option<Box<dyn Resampler<f32>>> = if from == to {
            None
        } else {
            let chunk = (from as usize / 100).max(64);
            match Fft::<f32>::new(from as usize, to as usize, chunk, 1, FixedSync::Input) {
                Ok(r) => Some(Box::new(r)),
                Err(_) => {
                    let params = SincInterpolationParameters {
                        sinc_len: 128,
                        f_cutoff: Some(0.95),
                        interpolation: SincInterpolationType::Linear,
                        oversampling_factor: 128,
                        window: WindowFunction::BlackmanHarris2,
                    };
                    let r = Async::<f32>::new_sinc(
                        to as f64 / from as f64,
                        1.1,
                        &params,
                        chunk,
                        1,
                        FixedAsync::Input,
                    )
                    .map_err(err)?;
                    Some(Box::new(r))
                }
            }
        };
        let out = inner
            .as_ref()
            .map_or_else(Vec::new, |r| vec![0.0; r.output_frames_max()]);
        Ok(Self {
            inner,
            pending: Vec::new(),
            out,
            from,
            to,
        })
    }

    pub fn from_rate(&self) -> u32 {
        self.from
    }

    pub fn to_rate(&self) -> u32 {
        self.to
    }

    /// Resample `input`, appending the produced samples to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), AudioError> {
        let Some(r) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            return Ok(());
        };
        self.pending.extend_from_slice(input);
        loop {
            let need = r.input_frames_next();
            if self.pending.len() < need {
                return Ok(());
            }
            let max = r.output_frames_max();
            if self.out.len() < max {
                self.out.resize(max, 0.0);
            }
            let inp = InterleavedSlice::new(&self.pending[..need], 1, need).map_err(err)?;
            let mut outb = InterleavedSlice::new_mut(&mut self.out, 1, max).map_err(err)?;
            let (read, written) = r.process_into_buffer(&inp, &mut outb, None).map_err(err)?;
            out.extend_from_slice(&self.out[..written]);
            self.pending.drain(..read);
        }
    }

    /// Drop buffered input and reset the filter state (after a stop or a stream rebuild).
    pub fn reset(&mut self) {
        self.pending.clear();
        if let Some(r) = self.inner.as_mut() {
            r.reset();
        }
    }
}

/// One-shot helper for whole clips (TTS chunks).
pub fn resample_all(x: &[f32], from: u32, to: u32) -> Result<Vec<f32>, AudioError> {
    let mut r = StreamResampler::new(from, to)?;
    let mut out = Vec::with_capacity(x.len() * to as usize / from as usize + 64);
    r.process(x, &mut out)?;
    // Flush the tail with silence so the last samples of the clip are emitted.
    if let Some(inner) = r.inner.as_ref() {
        let pad =
            vec![
                0.0;
                inner.input_frames_next() + inner.output_delay() * from as usize / to as usize + 8
            ];
        r.process(&pad, &mut out)?;
    }
    let delay = r.inner.as_ref().map_or(0, |i| i.output_delay());
    let expected = x.len() * to as usize / from as usize;
    let start = delay.min(out.len());
    let end = (start + expected).min(out.len());
    Ok(out[start..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(f: f32, sr: u32, secs: f32) -> Vec<f32> {
        (0..(sr as f32 * secs) as usize)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * f * i as f32 / sr as f32).sin())
            .collect()
    }

    fn rms(x: &[f32]) -> f32 {
        crate::level::rms(x)
    }

    #[test]
    fn passthrough_same_rate() {
        let mut r = StreamResampler::new(16_000, 16_000).expect("new");
        let mut out = Vec::new();
        r.process(&[0.1, 0.2], &mut out).expect("p");
        assert_eq!(out, vec![0.1, 0.2]);
    }

    #[test]
    fn downsample_48k_to_16k_keeps_level_and_length() {
        let x = sine(1000.0, 48_000, 1.0);
        let mut r = StreamResampler::new(48_000, 16_000).expect("new");
        let mut y = Vec::new();
        // odd block sizes, like real callbacks
        for c in x.chunks(1024) {
            r.process(c, &mut y).expect("p");
        }
        assert!((y.len() as i64 - 16_000).abs() < 600, "len {}", y.len());
        let mid = &y[2000..y.len() - 2000];
        assert!(
            (rms(mid) - 0.5 / 2f32.sqrt()).abs() < 0.01,
            "rms {}",
            rms(mid)
        );
    }

    #[test]
    fn aliasing_is_rejected() {
        // 12 kHz at 48 kHz must not show up in a 16 kHz stream
        let x = sine(12_000.0, 48_000, 1.0);
        let y = resample_all(&x, 48_000, 16_000).expect("r");
        assert!(rms(&y[1000..y.len() - 1000]) < 0.01);
    }

    #[test]
    fn upsample_tts_rate_to_device_rate() {
        for (from, to) in [(22_050u32, 48_000u32), (24_000, 48_000), (22_050, 44_100)] {
            let x = sine(440.0, from, 0.5);
            let y = resample_all(&x, from, to).expect("r");
            let want = (x.len() as u64 * to as u64 / from as u64) as i64;
            assert!(
                (y.len() as i64 - want).abs() <= 2,
                "{from}->{to}: {} vs {want}",
                y.len()
            );
            assert!((rms(&y[2000..y.len() - 2000]) - 0.5 / 2f32.sqrt()).abs() < 0.02);
        }
    }

    #[test]
    fn odd_rates_still_work() {
        let x = sine(500.0, 44_100, 0.5);
        let y = resample_all(&x, 44_100, 16_000).expect("r");
        assert!((y.len() as i64 - 8000).abs() < 400, "{}", y.len());
        assert!(rms(&y[500..y.len() - 500]) > 0.3);
        assert!(StreamResampler::new(0, 16_000).is_err());
    }
}
