use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use log::warn;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock};

/// Tiny mono click played on the default output device when dictation starts
/// or stops. If the device is unavailable, the failure is logged once and the
/// feature is silently disabled for the session.
struct Feedback {
    sender: Sender<Vec<f32>>,
    start_click: Vec<f32>,
    stop_click: Vec<f32>,
}

impl Feedback {
    fn play_start(&self) {
        let _ = self.sender.send(self.start_click.clone());
    }

    fn play_stop(&self) {
        let _ = self.sender.send(self.stop_click.clone());
    }
}

static FEEDBACK: OnceLock<Option<Feedback>> = OnceLock::new();

pub fn play_start() {
    if let Some(fb) = FEEDBACK.get_or_init(init) {
        fb.play_start();
    }
}

pub fn play_stop() {
    if let Some(fb) = FEEDBACK.get_or_init(init) {
        fb.play_stop();
    }
}

fn init() -> Option<Feedback> {
    let host = cpal::default_host();
    let device = host.default_output_device()?;
    let default_config = device.default_output_config().ok()?;
    let sample_rate = default_config.sample_rate().0;
    let channels = default_config.channels() as usize;

    // Try to use an f32 output stream; if the default device can't do f32,
    // fall back silently rather than crash the daemon.
    let mut config: cpal::StreamConfig = default_config.into();
    config.channels = channels as u16;

    let current = Arc::new(Mutex::new(Vec::new()));
    let current_cb = current.clone();
    let (tx, rx) = channel::<Vec<f32>>();

    std::thread::spawn(move || {
        let err_fn = |err| warn!("Audio feedback stream error: {err}");
        let stream = match device.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                let mut buf = current_cb.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                for frame in data.chunks_mut(channels) {
                    let sample = if buf.is_empty() { 0.0 } else { buf.remove(0) };
                    for ch in frame.iter_mut() {
                        *ch = sample;
                    }
                }
            },
            err_fn,
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("Audio feedback init failed: {e}");
                return;
            }
        };

        if let Err(e) = stream.play() {
            warn!("Audio feedback play failed: {e}");
            return;
        }

        loop {
            if let Ok(buf) = rx.recv() {
                let mut guard = current.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                *guard = buf;
            }
        }
    });

    Some(Feedback {
        sender: tx,
        start_click: click(sample_rate, 1000.0, 0.055),
        stop_click: click(sample_rate, 600.0, 0.075),
    })
}

/// A short sinusoidal click with a Hann envelope.
#[must_use]
pub fn click_buffer_len(sample_rate: u32, duration_secs: f32) -> usize {
    (sample_rate as f32 * duration_secs).max(1.0) as usize
}

fn click(sample_rate: u32, frequency: f32, duration_secs: f32) -> Vec<f32> {
    let samples = (sample_rate as f32 * duration_secs).max(1.0) as usize;
    let two_pi = 2.0 * std::f32::consts::PI;
    let mut out = Vec::with_capacity(samples);
    for i in 0..samples {
        let t = i as f32 / sample_rate as f32;
        let phase = two_pi * frequency * t;
        // Hann window.
        let envelope =
            0.5 - 0.5 * (two_pi * i as f32 / (samples.saturating_sub(1).max(1)) as f32).cos();
        out.push(phase.sin() * envelope * 0.35);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_buffer_length_matches_duration() {
        let sr = 48000;
        let buf = click(sr, 1000.0, 0.06);
        assert_eq!(buf.len(), click_buffer_len(sr, 0.06));
        assert!(!buf.is_empty());
    }

    #[test]
    fn click_amplitude_is_bounded() {
        let buf = click(48000, 1000.0, 0.06);
        for &s in &buf {
            assert!(s.abs() <= 1.0);
        }
    }

    #[test]
    fn click_starts_and_ends_near_zero() {
        let buf = click(48000, 1000.0, 0.06);
        assert!(buf[0].abs() < 0.01);
        assert!(buf.last().unwrap().abs() < 0.01);
    }
}
