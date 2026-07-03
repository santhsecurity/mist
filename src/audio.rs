use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::warn;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
    max_samples: usize,
    capped: Arc<AtomicBool>,
    samples_received: Arc<AtomicUsize>,
}

impl AudioRecorder {
    pub fn new(max_recording_secs: u32) -> Result<Self> {
        // Pre-allocate for expected duration at 48 kHz (common mic rate).
        // Actual rate is set in start(). The cap is enforced in the callback.
        let max_samples = max_recording_secs as usize * 48_000;
        Ok(Self {
            samples: Arc::new(Mutex::new(Vec::with_capacity(
                max_recording_secs as usize * 16_000,
            ))),
            stream: None,
            sample_rate: 16000,
            max_samples,
            capped: Arc::new(AtomicBool::new(false)),
            samples_received: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub fn buffer(&self) -> Arc<Mutex<Vec<f32>>> {
        self.samples.clone()
    }

    /// Returns true if the recording was stopped because it hit the maximum
    /// duration cap.
    pub fn was_capped(&self) -> bool {
        self.capped.load(Ordering::Relaxed)
    }

    /// Number of audio samples captured since `start()` was called. Useful for
    /// detecting mic permission failures or disconnected devices.
    pub fn samples_received(&self) -> usize {
        self.samples_received.load(Ordering::Relaxed)
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device found"))?;

        let config = device
            .default_input_config()
            .map_err(|e| anyhow::anyhow!("No default input config: {}", e))?;

        let sample_format = config.sample_format();
        let sample_rate = config.sample_rate().0;
        self.sample_rate = sample_rate;
        // Recompute max_samples at actual rate.
        self.max_samples = (self.max_samples / 48_000) * sample_rate as usize;

        let samples = self.samples.clone();
        let max = self.max_samples;
        let capped = self.capped.clone();
        let received = self.samples_received.clone();
        let err_fn = |err| warn!("Audio stream error: {}", err);

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if capped.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut buf) = samples.lock() {
                        let remaining = max.saturating_sub(buf.len());
                        if remaining == 0 {
                            capped.store(true, Ordering::Relaxed);
                            return;
                        }
                        let n = data.len().min(remaining);
                        received.fetch_add(n, Ordering::Relaxed);
                        buf.extend_from_slice(&data[..n]);
                    }
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => {
                let samples = self.samples.clone();
                let capped = self.capped.clone();
                let received = self.samples_received.clone();
                device.build_input_stream(
                    &config.into(),
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if capped.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut buf) = samples.lock() {
                            let remaining = max.saturating_sub(buf.len());
                            if remaining == 0 {
                                capped.store(true, Ordering::Relaxed);
                                return;
                            }
                            let n = data.len().min(remaining);
                            received.fetch_add(n, Ordering::Relaxed);
                            for &s in &data[..n] {
                                buf.push(s as f32 / 32768.0);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let samples = self.samples.clone();
                let capped = self.capped.clone();
                let received = self.samples_received.clone();
                device.build_input_stream(
                    &config.into(),
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if capped.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut buf) = samples.lock() {
                            let remaining = max.saturating_sub(buf.len());
                            if remaining == 0 {
                                capped.store(true, Ordering::Relaxed);
                                return;
                            }
                            let n = data.len().min(remaining);
                            received.fetch_add(n, Ordering::Relaxed);
                            for &s in &data[..n] {
                                // Correct U16→f32: map [0, 65535] → [-1.0, 1.0]
                                buf.push((s as f32 / 32767.5) - 1.0);
                            }
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            _ => anyhow::bail!("Unsupported sample format: {:?}", sample_format),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<Vec<f32>> {
        drop(self.stream.take());
        let samples = {
            let mut buf = self
                .samples
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *buf)
        };

        let resampled = if self.sample_rate != 16000 {
            resample(&samples, self.sample_rate, 16000)?
        } else {
            samples
        };

        // Trim leading/trailing silence via energy-threshold VAD.
        Ok(vad_trim(&resampled, 16000))
    }
}

/// Simple energy-threshold VAD: trims leading and trailing silence from audio.
/// Returns the trimmed slice (or the full buffer if everything is above
/// threshold). A frame of 400 samples (~25ms at 16 kHz) with RMS below the
/// threshold is considered silence.
fn vad_trim(samples: &[f32], _sample_rate: u32) -> Vec<f32> {
    const FRAME_SIZE: usize = 400; // 25ms at 16 kHz
    const SILENCE_THRESHOLD: f32 = 0.008; // empirical; typical mic noise floor

    if samples.len() < FRAME_SIZE {
        return samples.to_vec();
    }

    let frame_count = samples.len() / FRAME_SIZE;
    let mut first_voiced = 0;
    let mut last_voiced = frame_count;

    for i in 0..frame_count {
        let frame = &samples[i * FRAME_SIZE..(i + 1) * FRAME_SIZE];
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / FRAME_SIZE as f32).sqrt();
        if rms > SILENCE_THRESHOLD {
            first_voiced = i;
            break;
        }
    }

    for i in (0..frame_count).rev() {
        let frame = &samples[i * FRAME_SIZE..(i + 1) * FRAME_SIZE];
        let rms = (frame.iter().map(|s| s * s).sum::<f32>() / FRAME_SIZE as f32).sqrt();
        if rms > SILENCE_THRESHOLD {
            last_voiced = i + 1;
            break;
        }
    }

    // Keep a small margin (2 frames ≈ 50ms) around the voiced region.
    let start = first_voiced.saturating_sub(2) * FRAME_SIZE;
    let end = ((last_voiced + 2) * FRAME_SIZE).min(samples.len());

    if start >= end {
        return samples.to_vec();
    }

    samples[start..end].to_vec()
}

fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>> {
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 128,
        window: WindowFunction::BlackmanHarris2,
    };

    let ratio = to_rate as f64 / from_rate as f64;
    let mut resampler = SincFixedIn::<f64>::new(ratio, 1.0, params, input.len(), 1)?;

    let input_f64: Vec<f64> = input.iter().map(|&s| s as f64).collect();
    let waves_out = resampler.process(&[input_f64], None)?;
    Ok(waves_out[0].iter().map(|&s| s as f32).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vad_trim_keeps_voiced_audio() {
        // 1 second of silence + 1 second of tone + 1 second of silence
        let mut samples = vec![0.0f32; 16000];
        for i in 16000..32000 {
            let t = i as f32 / 16000.0;
            samples.push((t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5);
        }
        samples.extend(vec![0.0f32; 16000]);

        let trimmed = vad_trim(&samples, 16000);
        // Trimmed should be significantly shorter than the original.
        assert!(
            trimmed.len() < samples.len(),
            "VAD should trim silence: {} vs {}",
            trimmed.len(),
            samples.len()
        );
        // But should still contain the voiced portion.
        assert!(trimmed.len() > 15000, "VAD should keep the tone");
    }

    #[test]
    fn vad_trim_all_silence_returns_full() {
        let samples = vec![0.0f32; 16000];
        let trimmed = vad_trim(&samples, 16000);
        assert_eq!(trimmed.len(), samples.len());
    }
}
