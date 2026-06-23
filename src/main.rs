use flow::{audio, cleanup, config, hotkey, overlay, paste, stt};

use anyhow::Result;
use clap::{Parser, Subcommand};
use global_hotkey::{GlobalHotKeyManager, HotKeyState};
use log::{error, info, warn};
use notify_rust::Notification;
use std::fs::OpenOptions;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::ControlFlow;

#[derive(Parser)]
#[command(name = "flow", about = "Local voice dictation daemon", version = env!("CARGO_PKG_VERSION"))]
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
}

enum Job {
    Preview(Vec<f32>),
    Final(Vec<f32>),
}

fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Setup) => config::Config::setup_interactive(),
        Some(Commands::Run) | None => run_daemon(),
    }
}

/// Maximum log file size before we truncate (10 MB).
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

fn init_logging() {
    if let Ok(val) = std::env::var("RUST_LOG") {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(&val)).init();
    } else {
        // Default: log to file with rotation.
        if let Some(dirs) = directories::ProjectDirs::from("", "", "flow") {
            let log_path = dirs.data_dir().join("flow.log");
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

            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path);
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

    let (stt_tx, stt_rx) = channel::<Job>();

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
                return;
            }
        };

        let mut engine = match stt::SttEngine::new(&model_path) {
            Ok(e) => e,
            Err(err) => {
                error!("Failed to load STT engine: {}", err);
                return;
            }
        };

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
                        }
                        _ => {}
                    }
                }
                Job::Final(samples) => {
                    info!("Transcribing {} samples...", samples.len());
                    match engine.transcribe(
                        &samples,
                        &config_clone.language,
                        &effective_dict_clone,
                        config_clone.n_threads,
                    ) {
                        Ok(mut text) => {
                            if text.is_empty() {
                                continue;
                            }
                            if config_clone.cleanup_enabled {
                                info!("Cleaning up ({} backend)...", config_clone.cleanup_backend);
                                match cleanup::cleanup(&text, &config_clone) {
                                    Ok(cleaned) => {
                                        if !cleaned.is_empty() {
                                            text = cleaned;
                                        }
                                    }
                                    Err(e) => warn!("Cleanup failed: {}", e),
                                }
                            }
                            info!("Result: {}", text);
                            if let Err(e) = paste::paste_text(&text) {
                                error!("Paste failed: {}", e);
                                let _ = Notification::new()
                                    .summary("Flow")
                                    .body(&format!(
                                        "Copied: {}",
                                        text.chars().take(80).collect::<String>()
                                    ))
                                    .timeout(3000)
                                    .show();
                            } else {
                                let _ = Notification::new()
                                    .summary("Flow")
                                    .body("Dictation pasted")
                                    .timeout(2000)
                                    .show();
                            }
                        }
                        Err(e) => error!("Transcription failed: {}", e),
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

    let running_loop = running.clone();

    event_loop.run(move |event, _, control_flow| {
        // Sleep for 16ms between iterations (~60 Hz) instead of busy-looping.
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16));

        // Check for graceful shutdown.
        if !running_loop.load(Ordering::Relaxed) {
            *control_flow = ControlFlow::Exit;
            return;
        }

        match event {
            Event::NewEvents(StartCause::Init) => {
                info!("Flow running. Press {} to dictate.", config.hotkey);
                info!("Config: {:?}", config::Config::path().unwrap_or_default());
                info!("Threads: {}, max recording: {}s", config.n_threads, config.max_recording_secs);
                if !effective_dict.is_empty() {
                    info!("Dictionary: {:?}", effective_dict);
                }
            }
            Event::MainEventsCleared => {
                // Check hotkey events.
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
                                        last_preview_len = 0;
                                        preview_buffer = Some(r.buffer());
                                        recorder = Some(r);
                                        if config.show_overlay {
                                            overlay.show();
                                        }
                                        let _ = Notification::new()
                                            .summary("Flow")
                                            .body("Recording...")
                                            .timeout(10000)
                                            .show();
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
                                &config,
                                &overlay,
                                &stt_tx,
                            );
                        }
                        HotKeyState::Pressed if recording => {
                            // Fallback: second press also stops (toggle mode).
                            // This handles keyboards/systems that don't emit
                            // Released events reliably.
                            stop_recording(
                                &mut recorder,
                                &mut recording,
                                &mut preview_buffer,
                                &mut last_preview_len,
                                &config,
                                &overlay,
                                &stt_tx,
                            );
                        }
                        _ => {}
                    }
                }

                // Check if recording was capped at max duration.
                if recording {
                    if let Some(ref r) = recorder {
                        if r.was_capped() {
                            warn!("Max recording duration ({} s) reached, auto-stopping.", config.max_recording_secs);
                            stop_recording(
                                &mut recorder,
                                &mut recording,
                                &mut preview_buffer,
                                &mut last_preview_len,
                                &config,
                                &overlay,
                                &stt_tx,
                            );
                        }
                    }
                }

                // Live stream preview.
                if config.live_stream && recording {
                    if let Some(ref buf) = preview_buffer {
                        if let Ok(lock) = buf.lock() {
                            let current_len = lock.len();
                            let new_samples = current_len.saturating_sub(last_preview_len);
                            if new_samples >= 24000 {
                                // 1.5 seconds of new audio at 16kHz
                                last_preview_len = current_len;
                                let _ = stt_tx.send(Job::Preview(lock.clone()));
                            }
                        }
                    }
                }

                // Render overlay animation (cap at ~30 FPS).
                if recording && config.show_overlay && last_draw.elapsed() >= Duration::from_millis(33)
                {
                    last_draw = Instant::now();
                    let _ = overlay.draw();
                }
            }
            _ => {}
        }
    });
}

fn stop_recording(
    recorder: &mut Option<audio::AudioRecorder>,
    recording: &mut bool,
    preview_buffer: &mut Option<std::sync::Arc<std::sync::Mutex<Vec<f32>>>>,
    last_preview_len: &mut usize,
    config: &config::Config,
    overlay: &overlay::Overlay,
    stt_tx: &std::sync::mpsc::Sender<Job>,
) {
    if let Some(mut r) = recorder.take() {
        *recording = false;
        *preview_buffer = None;
        *last_preview_len = 0;
        if config.show_overlay {
            overlay.hide();
        }
        let _ = Notification::new()
            .summary("Flow")
            .body("Transcribing...")
            .timeout(5000)
            .show();
        match r.stop() {
            Ok(samples) => {
                let _ = stt_tx.send(Job::Final(samples));
            }
            Err(e) => error!("Failed to stop recording: {}", e),
        }
    }
}
