//! Daemon event loop: global hotkey handling, audio recording, STT worker,
//! overlay updates, and system-tray integration.

use crate::tray::{init_tray, open_path};
use anyhow::Result;
use global_hotkey::{GlobalHotKeyManager, HotKeyState};
use log::{error, info, warn};
use mist::{audio, audio_feedback, cleanup, config, hotkey, overlay, paste, stt};
use notify_rust::Notification;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::ControlFlow;

enum Job {
    Preview(Vec<f32>, config::DictionarySnapshot),
    Final(Vec<f32>, config::DictionarySnapshot),
}

/// Minimum recording duration in seconds. Anything shorter is discarded as an
/// accidental press - not enough audio for Whisper to produce anything useful.
const MIN_RECORDING_SECS: f32 = 0.4;

pub fn run() -> Result<()> {
    let config = config::Config::load()?;
    let config_clone = config.clone();

    let (stt_tx, stt_rx) = mpsc::channel::<Job>();

    // Channel for the STT thread to send results back to the main thread
    // so we can show them in notifications.
    let (result_tx, result_rx) = mpsc::channel::<TranscriptionResult>();

    // Graceful shutdown flag.
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        let _ = ctrlc::set_handler(move || {
            info!("Received shutdown signal, exiting...");
            running.store(false, Ordering::SeqCst);
        });
    }

    // Worker thread owns the STT engine and processes transcriptions.
    let _stt_handle = thread::spawn(move || {
        let model_path = match config_clone.model_path() {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to resolve model path: {}", e);
                let _ = result_tx.send(TranscriptionResult::Error(format!(
                    "Model path error: {}",
                    e
                )));
                return;
            }
        };

        let mut engine = match stt::SttEngine::new(&model_path) {
            Ok(e) => e,
            Err(err) => {
                error!("Failed to load STT engine: {}", err);
                let _ = result_tx.send(TranscriptionResult::Error(format!(
                    "STT load error: {}",
                    err
                )));
                return;
            }
        };

        // Pre-warm: run a tiny dummy transcription so the first real
        // dictation doesn't pay cold-start JIT/cache penalties.
        engine.warm_up();
        let _ = result_tx.send(TranscriptionResult::Ready);

        while let Ok(job) = stt_rx.recv() {
            match job {
                Job::Preview(samples, snapshot) => {
                    if samples.len() < 16000 {
                        continue;
                    }
                    match engine.transcribe(
                        &samples,
                        &config_clone.language,
                        &snapshot,
                        config_clone.n_threads,
                    ) {
                        Ok(text) if !text.is_empty() => {
                            info!("[live] {}", text);
                            let _ = result_tx.send(TranscriptionResult::Preview(text));
                        }
                        _ => {}
                    }
                }
                Job::Final(samples, snapshot) => {
                    let start = Instant::now();
                    info!(
                        "Transcribing {} samples ({:.1}s audio)...",
                        samples.len(),
                        samples.len() as f32 / 16000.0
                    );

                    match engine.transcribe(
                        &samples,
                        &config_clone.language,
                        &snapshot,
                        config_clone.n_threads,
                    ) {
                        Ok(mut text) => {
                            if text.is_empty() {
                                let _ = result_tx.send(TranscriptionResult::Empty);
                                continue;
                            }
                            if config_clone.cleanup_enabled {
                                match cleanup::cleanup(&text, &config_clone) {
                                    Ok(cleaned) if !cleaned.is_empty() => text = cleaned,
                                    Ok(_) => {}
                                    Err(e) => warn!("Cleanup failed: {}", e),
                                }
                            }
                            let elapsed = start.elapsed();
                            info!("Transcribed in {:.1}s: {}", elapsed.as_secs_f32(), text);

                            if let Err(e) = paste::paste_text(&text) {
                                error!("Paste failed: {}", e);
                                let _ = result_tx.send(TranscriptionResult::PasteFailed(text));
                            } else {
                                let _ = result_tx.send(TranscriptionResult::Done(text, elapsed));
                            }
                        }
                        Err(e) => {
                            error!("Transcription failed: {}", e);
                            let _ = result_tx.send(TranscriptionResult::Error(e.to_string()));
                        }
                    }
                }
            }
        }
    });

    // Global hotkey.
    let manager = GlobalHotKeyManager::new()?;
    let hotkey = hotkey::parse_hotkey(&config.hotkey)?;
    if let Err(e) = manager.register(hotkey) {
        eprintln!(
            "Failed to register hotkey '{}': {}\n\n\
             Common causes:\n\
             • Another application (or the OS) is already using {}\n\
             • The shortcut requires higher privileges than Mist has\n\n\
             Try a different shortcut, for example:\n\
               Alt+Shift+D\n\
               Ctrl+Shift+Space\n\
               F9\n\n\
             Change it with: mist setup",
            config.hotkey, e, config.hotkey
        );
        std::process::exit(1);
    }
    let receiver = global_hotkey::GlobalHotKeyEvent::receiver();

    // Event loop for overlay + hotkey polling.
    let event_loop = tao::event_loop::EventLoopBuilder::new().build();
    let mut overlay = overlay::Overlay::new(&event_loop)?;

    // Optional system tray icon.
    let tray = init_tray();
    let _tray_events = tray_icon::TrayIconEvent::receiver();
    let menu_events = tray_icon::menu::MenuEvent::receiver();

    let mut recording = false;
    let mut recorder: Option<audio::AudioRecorder> = None;
    let mut preview_buffer: Option<std::sync::Arc<std::sync::Mutex<Vec<f32>>>> = None;
    let mut last_preview_len: usize = 0;
    let mut last_draw = Instant::now();
    let mut last_overlay_move = Instant::now();
    let mut recording_start = Instant::now();

    let running_loop = running.clone();

    event_loop.run(move |event, _, control_flow| {
        // Adaptive tick rate: fast during recording (smooth animation),
        // slow when idle (saves CPU - 2 Hz is plenty for hotkey polling).
        let tick = if recording {
            Duration::from_millis(16) // ~60 Hz
        } else {
            Duration::from_millis(500) // 2 Hz
        };
        *control_flow = ControlFlow::WaitUntil(Instant::now() + tick);

        // Check for graceful shutdown.
        if !running_loop.load(Ordering::Relaxed) {
            *control_flow = ControlFlow::Exit;
            return;
        }

        match event {
            Event::NewEvents(StartCause::Init) => {
                info!("Mist daemon starting...");
                info!("Hotkey: {} | Model: {} | Threads: {} | Max: {}s",
                    config.hotkey, config.model, config.n_threads, config.max_recording_secs);
                if !config.dictionary.is_empty() {
                    info!("Dictionary: {:?}", config.dictionary);
                }
                let _ = Notification::new()
                    .summary("Mist")
                    .body(&format!("Ready - press {} to dictate", config.hotkey))
                    .timeout(3000)
                    .show();

                if !paste::typing_backend_available() {
                    warn!("No typing backend available");
                    let _ = Notification::new()
                        .summary("Mist - Typing tool missing")
                        .body("Install xdotool (X11), wtype (Wayland), or ydotool so Mist can type text.")
                        .timeout(0)
                        .show();
                }
            }
            Event::MainEventsCleared => {
                // --- Check tray menu events ---
                while let Ok(event) = menu_events.try_recv() {
                    if tray.quit_id.as_ref() == Some(&event.id) {
                        info!("Quit requested from tray");
                        running_loop.store(false, Ordering::Relaxed);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    if tray.open_config_id.as_ref() == Some(&event.id) {
                        if let Ok(path) = config::Config::path() {
                            let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or(path);
                            let _ = open_path(&dir);
                        }
                    }
                    if tray.open_logs_id.as_ref() == Some(&event.id) {
                        if let Some(dir) = directories::ProjectDirs::from("", "", "mist") {
                            let _ = open_path(dir.data_dir());
                        }
                    }
                }

                // --- Check for results from the STT thread ---
                while let Ok(result) = result_rx.try_recv() {
                    match result {
                        TranscriptionResult::Ready => {
                            info!("STT engine ready (warmed up)");
                        }
                        TranscriptionResult::Preview(text) => {
                            if config.show_overlay {
                                overlay.set_text(&text);
                            }
                            let _ = Notification::new()
                                .summary("Mist - Preview")
                                .body(&truncate(&text, 120))
                                .timeout(2000)
                                .show();
                        }
                        TranscriptionResult::Done(text, elapsed) => {
                            if config.show_overlay {
                                overlay.set_state(overlay::OverlayState::Done);
                                overlay.set_text(&text);
                                overlay.dismiss_after(Duration::from_millis(2500));
                            }
                            let _ = Notification::new()
                                .summary("Mist ✓")
                                .body(&format!(
                                    "\"{}\" ({:.1}s)",
                                    truncate(&text, 80),
                                    elapsed.as_secs_f32()
                                ))
                                .timeout(3000)
                                .show();
                        }
                        TranscriptionResult::Empty => {
                            if config.show_overlay {
                                overlay.set_state(overlay::OverlayState::Done);
                                overlay.set_text("No speech");
                                overlay.dismiss_after(Duration::from_secs(1));
                            }
                            let _ = Notification::new()
                                .summary("Mist")
                                .body("No speech detected")
                                .timeout(2000)
                                .show();
                        }
                        TranscriptionResult::PasteFailed(text) => {
                            if config.show_overlay {
                                overlay.set_state(overlay::OverlayState::Error);
                                overlay.set_text("Paste failed");
                                overlay.dismiss_after(Duration::from_secs(2));
                            }
                            let _ = Notification::new()
                                .summary("Mist - Paste failed")
                                .body(&format!("Text: {}", truncate(&text, 100)))
                                .timeout(5000)
                                .show();
                        }
                        TranscriptionResult::Error(msg) => {
                            if config.show_overlay {
                                overlay.set_state(overlay::OverlayState::Error);
                                overlay.set_text("Error");
                                overlay.dismiss_after(Duration::from_secs(2));
                            }
                            let _ = Notification::new()
                                .summary("Mist - Error")
                                .body(&truncate(&msg, 120))
                                .timeout(5000)
                                .show();
                        }
                    }
                }

                // --- Check hotkey events ---
                while let Ok(event) = receiver.try_recv() {
                    if event.id != hotkey.id() {
                        continue;
                    }

                    match event.state {
                        HotKeyState::Pressed if !recording => {
                            // Start recording.
                            match audio::AudioRecorder::new(config.max_recording_secs) {
                                Ok(mut r) => {
                                    if let Err(e) = r.start() {
                                        error!("Failed to start recording: {}", e);
                                    } else {
                                        recording = true;
                                        recording_start = Instant::now();
                                        last_preview_len = 0;
                                        preview_buffer = Some(r.buffer());
                                        recorder = Some(r);
                                        if config.audio_feedback {
                                            audio_feedback::play_start();
                                        }
                                        if config.show_overlay {
                                            overlay.show_near_cursor();
                                            overlay.set_state(overlay::OverlayState::Listening);
                                            overlay.set_text("LISTENING");
                                        }
                                    }
                                }
                                Err(e) => error!("Recorder init failed: {}", e),
                            }
                        }
                        HotKeyState::Released if recording && !config.toggle_mode => {
                            // Hold-to-talk: stop on key release.
                            stop_recording(
                                &mut recorder,
                                &mut recording,
                                &mut preview_buffer,
                                &mut last_preview_len,
                                recording_start,
                                &config,
                                &mut overlay,
                                &stt_tx,
                            );
                        }
                        HotKeyState::Pressed if recording && config.toggle_mode => {
                            // Toggle mode: second press stops.
                            stop_recording(
                                &mut recorder,
                                &mut recording,
                                &mut preview_buffer,
                                &mut last_preview_len,
                                recording_start,
                                &config,
                                &mut overlay,
                                &stt_tx,
                            );
                        }
                        _ => {}
                    }
                }

                // --- Check max duration cap and microphone permission ---
                if recording {
                    if let Some(ref r) = recorder {
                        if r.was_capped() {
                            warn!("Max recording duration ({}s) reached, auto-stopping.",
                                config.max_recording_secs);
                            stop_recording(
                                &mut recorder,
                                &mut recording,
                                &mut preview_buffer,
                                &mut last_preview_len,
                                recording_start,
                                &config,
                                &mut overlay,
                                &stt_tx,
                            );
                        } else if recording_start.elapsed() > Duration::from_secs(1)
                            && r.samples_received() == 0
                        {
                            warn!("No audio samples received - microphone may be muted or permission denied.");
                            if config.show_overlay {
                                overlay.set_state(overlay::OverlayState::Error);
                                overlay.set_text("Mic blocked");
                                overlay.dismiss_after(Duration::from_secs(3));
                            }
                            let _ = Notification::new()
                                .summary("Mist - Microphone blocked")
                                .body("No audio received. Check microphone permissions and that the device is not muted.")
                                .timeout(5000)
                                .show();
                            stop_recording(
                                &mut recorder,
                                &mut recording,
                                &mut preview_buffer,
                                &mut last_preview_len,
                                recording_start,
                                &config,
                                &mut overlay,
                                &stt_tx,
                            );
                        }
                    }
                }

                // --- Live stream preview ---
                if config.live_stream && recording {
                    if let Some(ref buf) = preview_buffer {
                        if let Ok(lock) = buf.lock() {
                            let current_len = lock.len();
                            let new_samples = current_len.saturating_sub(last_preview_len);
                            if new_samples >= 24000 {
                                last_preview_len = current_len;
                                let snapshot = config.dictionary_snapshot();
                                let _ = stt_tx.send(Job::Preview(lock.clone(), snapshot));
                            }
                        }
                    }
                }

                // --- Render overlay animation (~30 FPS while visible) ---
                if config.show_overlay && overlay.is_visible() && last_draw.elapsed() >= Duration::from_millis(33) {
                    last_draw = Instant::now();
                    if recording {
                        if let Some(ref buf) = preview_buffer {
                            if let Ok(lock) = buf.lock() {
                                overlay.set_waveform_samples(&overlay::waveform_from_samples(&lock, 160));
                            }
                        }
                    }
                    if last_overlay_move.elapsed() >= Duration::from_millis(50) {
                        last_overlay_move = Instant::now();
                        overlay.reposition_near_cursor();
                    }
                    if overlay.should_dismiss() {
                        overlay.hide();
                    } else {
                        let _ = overlay.draw();
                    }
                }
            }
            _ => {}
        }
    });
}

enum TranscriptionResult {
    Ready,
    Preview(String),
    Done(String, Duration),
    Empty,
    PasteFailed(String),
    Error(String),
}

#[allow(clippy::too_many_arguments)]
fn stop_recording(
    recorder: &mut Option<audio::AudioRecorder>,
    recording: &mut bool,
    preview_buffer: &mut Option<std::sync::Arc<std::sync::Mutex<Vec<f32>>>>,
    last_preview_len: &mut usize,
    recording_start: Instant,
    config: &config::Config,
    overlay: &mut overlay::Overlay,
    stt_tx: &mpsc::Sender<Job>,
) {
    if config.audio_feedback {
        audio_feedback::play_stop();
    }
    if let Some(mut r) = recorder.take() {
        *recording = false;
        *preview_buffer = None;
        *last_preview_len = 0;

        let duration = recording_start.elapsed();

        // Short recording guard: discard accidental taps.
        if duration.as_secs_f32() < MIN_RECORDING_SECS {
            info!(
                "Recording too short ({:.1}s < {:.1}s), discarding.",
                duration.as_secs_f32(),
                MIN_RECORDING_SECS
            );
            if config.show_overlay {
                overlay.hide();
            }
            let _ = r.stop(); // drain the buffer
            return;
        }

        info!("Recorded {:.1}s, sending to STT...", duration.as_secs_f32());
        match r.stop() {
            Ok(samples) => {
                if samples.len() < 4000 {
                    // Less than 0.25s of audio after VAD trim - nothing useful.
                    info!(
                        "Audio too short after VAD trim ({} samples), skipping.",
                        samples.len()
                    );
                    if config.show_overlay {
                        overlay.set_state(overlay::OverlayState::Done);
                        overlay.set_text("No speech");
                        overlay.dismiss_after(Duration::from_secs(1));
                    }
                    let _ = Notification::new()
                        .summary("Mist")
                        .body("No speech detected")
                        .timeout(2000)
                        .show();
                    return;
                }
                if config.show_overlay {
                    overlay.set_state(overlay::OverlayState::Processing);
                    overlay.set_text("PROCESSING");
                }
                let snapshot = config.dictionary_snapshot();
                let _ = stt_tx.send(Job::Final(samples, snapshot));
            }
            Err(e) => {
                error!("Failed to stop recording: {}", e);
                if config.show_overlay {
                    overlay.set_state(overlay::OverlayState::Error);
                    overlay.set_text("Mic error");
                    overlay.dismiss_after(Duration::from_secs(2));
                }
            }
        }
    }
}

/// Truncate a string to `max` characters, appending "…" if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}
