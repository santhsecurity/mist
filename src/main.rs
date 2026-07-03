use mist::{audio, cleanup, config, hotkey, overlay, paste, stt};

use anyhow::Result;
use clap::{Parser, Subcommand};
use global_hotkey::{GlobalHotKeyManager, HotKeyState};
use log::{error, info, warn};
use notify_rust::Notification;
use std::fs::OpenOptions;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::ControlFlow;

#[derive(Parser)]
#[command(name = "mist", about = "Local voice dictation daemon", version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the dictation daemon (default)
    Run,
    /// Interactive configuration
    Setup,
    /// Manage the global dictionary
    Dictionary {
        #[command(subcommand)]
        action: DictAction,
    },
    /// Show daemon status and configuration
    Status,
    /// Generate overlay screenshots for documentation
    Screenshot {
        /// Output directory (default: assets/screenshots)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
}

#[derive(Subcommand)]
enum DictAction {
    /// Add a word to the global dictionary
    Add { word: String },
    /// Remove a word from the global dictionary
    Remove { word: String },
    /// List dictionary, corrections, and replacements
    List,
    /// Import a TOML dictionary file into the global config
    Import { path: std::path::PathBuf },
    /// Export the global dictionary to a TOML file
    Export { path: std::path::PathBuf },
}

enum Job {
    Preview(Vec<f32>),
    Final(Vec<f32>),
}

/// Minimum recording duration in seconds. Anything shorter is discarded as an
/// accidental press — not enough audio for Whisper to produce anything useful.
const MIN_RECORDING_SECS: f32 = 0.4;

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Setup) => config::Config::setup_interactive(),
        Some(Commands::Dictionary { action }) => handle_dictionary(action),
        Some(Commands::Status) => show_status(),
        Some(Commands::Screenshot { output }) => generate_screenshots(output),
        Some(Commands::Run) | None => run_daemon(),
    }
}

/// Maximum log file size before we rotate (10 MB).
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

fn init_logging() {
    if let Ok(val) = std::env::var("RUST_LOG") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&val)).init();
    } else {
        // Default: log to file with rotation.
        if let Some(dirs) = directories::ProjectDirs::from("", "", "mist") {
            let log_path = dirs.data_dir().join("mist.log");
            let _ = std::fs::create_dir_all(dirs.data_dir());

            // Simple size-based rotation: if the log exceeds MAX_LOG_SIZE,
            // rename it to .log.old and start a fresh one.
            if log_path.exists() {
                if let Ok(meta) = std::fs::metadata(&log_path) {
                    if meta.len() > MAX_LOG_SIZE {
                        let old = log_path.with_extension("log.old");
                        let _ = std::fs::rename(&log_path, &old);
                    }
                }
            }

            let file = OpenOptions::new().create(true).append(true).open(&log_path);
            if let Ok(file) = file {
                env_logger::Builder::from_default_env()
                    .filter_level(log::LevelFilter::Info)
                    .target(env_logger::Target::Pipe(Box::new(file)))
                    .init();
                return;
            }
        }
        // Fallback to stderr.
        env_logger::init();
    }
}

fn run_daemon() -> Result<()> {
    let config = config::Config::load()?;
    let config_clone = config.clone();

    // Merge per-project dictionary terms with global dictionary.
    let effective_dict = config.effective_dictionary();
    let effective_dict_clone = effective_dict.clone();

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
                Job::Preview(samples) => {
                    if samples.len() < 16000 {
                        continue;
                    }
                    match engine.transcribe(
                        &samples,
                        &config_clone.language,
                        &effective_dict_clone,
                        config_clone.n_threads,
                    ) {
                        Ok(text) if !text.is_empty() => {
                            info!("[live] {}", text);
                            let _ = result_tx.send(TranscriptionResult::Preview(text));
                        }
                        _ => {}
                    }
                }
                Job::Final(samples) => {
                    let start = Instant::now();
                    info!(
                        "Transcribing {} samples ({:.1}s audio)...",
                        samples.len(),
                        samples.len() as f32 / 16000.0
                    );

                    match engine.transcribe(
                        &samples,
                        &config_clone.language,
                        &effective_dict_clone,
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
    manager.register(hotkey)?;
    let receiver = global_hotkey::GlobalHotKeyEvent::receiver();

    // Event loop for overlay + hotkey polling.
    let event_loop = tao::event_loop::EventLoopBuilder::new().build();
    let mut overlay = overlay::Overlay::new(&event_loop)?;

    let mut recording = false;
    let mut recorder: Option<audio::AudioRecorder> = None;
    let mut preview_buffer: Option<std::sync::Arc<std::sync::Mutex<Vec<f32>>>> = None;
    let mut last_preview_len: usize = 0;
    let mut last_draw = Instant::now();
    let mut recording_start = Instant::now();

    let running_loop = running.clone();

    event_loop.run(move |event, _, control_flow| {
        // Adaptive tick rate: fast during recording (smooth animation),
        // slow when idle (saves CPU — 2 Hz is plenty for hotkey polling).
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
                if !effective_dict.is_empty() {
                    info!("Dictionary: {:?}", effective_dict);
                }
                let _ = Notification::new()
                    .summary("Mist")
                    .body(&format!("Ready — press {} to dictate", config.hotkey))
                    .timeout(3000)
                    .show();

                if !paste::typing_backend_available() {
                    warn!("No typing backend available");
                    let _ = Notification::new()
                        .summary("Mist — Typing tool missing")
                        .body("Install xdotool (X11), wtype (Wayland), or ydotool so Mist can type text.")
                        .timeout(0)
                        .show();
                }
            }
            Event::MainEventsCleared => {
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
                                .summary("Mist — Preview")
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
                                .summary("Mist — Paste failed")
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
                                .summary("Mist — Error")
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
                                        if config.show_overlay {
                                            overlay.show_near_cursor();
                                            overlay.set_state(overlay::OverlayState::Listening);
                                            overlay.set_text("");
                                        }
                                    }
                                }
                                Err(e) => error!("Recorder init failed: {}", e),
                            }
                        }
                        HotKeyState::Released if recording => {
                            // Stop recording on key release (hold-to-talk).
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
                        HotKeyState::Pressed if recording => {
                            // Fallback: second press also stops (toggle mode).
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
                            warn!("No audio samples received — microphone may be muted or permission denied.");
                            if config.show_overlay {
                                overlay.set_state(overlay::OverlayState::Error);
                                overlay.set_text("Mic blocked");
                                overlay.dismiss_after(Duration::from_secs(3));
                            }
                            let _ = Notification::new()
                                .summary("Mist — Microphone blocked")
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
                                let _ = stt_tx.send(Job::Preview(lock.clone()));
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
                                overlay.set_levels(&overlay::audio_levels(&lock, 12));
                            }
                        }
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

fn generate_screenshots(output: Option<std::path::PathBuf>) -> Result<()> {
    let dir = output.unwrap_or_else(|| std::path::PathBuf::from("assets/screenshots"));
    std::fs::create_dir_all(&dir)?;

    let mut renderer = overlay::Renderer::new(280, 52);

    // Listening: active waveform.
    renderer.set_state(overlay::OverlayState::Listening);
    renderer.set_levels(&[
        0.9, 0.6, 0.3, 0.7, 0.95, 0.4, 0.2, 0.55, 0.85, 0.6, 0.3, 0.45,
    ]);
    renderer.clear_text();
    save_overlay_png(&mut renderer, &dir.join("listening.png"))?;

    // Processing: low activity with status text.
    renderer.set_state(overlay::OverlayState::Processing);
    renderer.set_levels(&[0.2; 12]);
    renderer.set_text("Thinking…");
    save_overlay_png(&mut renderer, &dir.join("processing.png"))?;

    // Done: final transcribed text.
    renderer.set_state(overlay::OverlayState::Done);
    renderer.set_levels(&[0.0; 12]);
    renderer.set_text("Deploy to Kubernetes after lunch.");
    save_overlay_png(&mut renderer, &dir.join("done.png"))?;

    println!("Screenshots saved to {:?}", dir);
    Ok(())
}

fn save_overlay_png(renderer: &mut overlay::Renderer, path: &std::path::Path) -> Result<()> {
    use image::{ImageBuffer, Rgba};
    let width = renderer.width();
    let height = renderer.height();
    let buf = renderer.render();
    let rgba: Vec<u8> = buf
        .iter()
        .flat_map(|&p| {
            if p == 0 {
                vec![0, 0, 0, 0]
            } else {
                let r = ((p >> 16) & 0xff) as u8;
                let g = ((p >> 8) & 0xff) as u8;
                let b = (p & 0xff) as u8;
                vec![r, g, b, 255]
            }
        })
        .collect();
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgba)
        .ok_or_else(|| anyhow::anyhow!("invalid image buffer"))?;
    img.save(path)?;
    Ok(())
}

fn handle_dictionary(action: DictAction) -> Result<()> {
    let mut config = config::Config::load()?;
    match action {
        DictAction::Add { word } => {
            if config.add_dictionary_word(&word) {
                config.save()?;
                println!("Added '{}' to dictionary.", word);
            } else {
                println!("'{}' is already in the dictionary.", word);
            }
        }
        DictAction::Remove { word } => {
            if config.remove_dictionary_word(&word) {
                config.save()?;
                println!("Removed '{}' from dictionary.", word);
            } else {
                println!("'{}' is not in the dictionary.", word);
            }
        }
        DictAction::List => {
            println!("Dictionary terms: {:?}", config.dictionary);
            println!("Corrections: {:?}", config.corrections);
            println!("Replacements: {:?}", config.replacements);
        }
        DictAction::Import { path } => {
            config.import_dictionary(&path)?;
            config.save()?;
            println!("Imported dictionary from {:?}.", path);
        }
        DictAction::Export { path } => {
            config.export_dictionary(&path)?;
            println!("Exported dictionary to {:?}.", path);
        }
    }
    Ok(())
}

fn show_status() -> Result<()> {
    let config = config::Config::load()?;
    let data_dir =
        directories::ProjectDirs::from("", "", "mist").map(|d| d.data_dir().to_path_buf());
    let typing_ok = paste::typing_backend_available();

    println!("Mist status");
    println!("  Config path:   {:?}", config::Config::path()?);
    println!("  Config exists: {}", config::Config::path()?.exists());
    println!("  Data dir:      {:?}", data_dir);
    println!("  Model:         {}", config.model);
    println!("  Model file:    {:?}", config.model_path()?);
    println!("  Model exists:  {}", config.model_path()?.exists());
    println!("  Hotkey:        {}", config.hotkey);
    println!("  Cleanup:       {}", config.cleanup_backend);
    println!("  Overlay:       {}", config.show_overlay);
    println!("  Live stream:   {}", config.live_stream);
    println!(
        "  Typing backend: {}",
        if typing_ok { "ok" } else { "missing" }
    );
    Ok(())
}

enum TranscriptionResult {
    Ready,
    Preview(String),
    Done(String, Duration),
    Empty,
    PasteFailed(String),
    Error(String),
}

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
                    // Less than 0.25s of audio after VAD trim — nothing useful.
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
                    overlay.set_text("");
                }
                let _ = stt_tx.send(Job::Final(samples));
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
