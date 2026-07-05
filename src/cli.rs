//! CLI parsing, command dispatch, and one-off utility commands for Mist.

use anyhow::Result;
use clap::{Parser, Subcommand};
use cpal::traits::HostTrait;
use mist::{config, overlay, paste, stt};
use std::fs::OpenOptions;
use std::path::PathBuf;

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
        output: Option<PathBuf>,
    },
    /// Show recent daemon log output
    Logs,
    /// Download or inspect Whisper models
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Run environment diagnostics
    Doctor,
}

#[derive(Subcommand)]
enum ModelAction {
    /// List available models and whether they are installed
    List,
    /// Download a model by name (e.g. small.en)
    Download { name: String },
    /// Delete a downloaded model
    Remove { name: String },
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
    Import { path: PathBuf },
    /// Export the global dictionary to a TOML file
    Export { path: PathBuf },
}

/// Maximum log file size before we rotate (10 MB).
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

pub fn run() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Setup) => config::Config::setup_interactive(),
        Some(Commands::Dictionary { action }) => handle_dictionary(action),
        Some(Commands::Status) => show_status(),
        Some(Commands::Screenshot { output }) => generate_screenshots(output),
        Some(Commands::Logs) => show_logs(),
        Some(Commands::Model { action }) => handle_model(action),
        Some(Commands::Doctor) => show_doctor(),
        Some(Commands::Run) | None => crate::daemon::run(),
    }
}

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

fn generate_screenshots(output: Option<PathBuf>) -> Result<()> {
    let dir = output.unwrap_or_else(|| PathBuf::from("assets/screenshots"));
    std::fs::create_dir_all(&dir)?;

    let mut renderer = overlay::Renderer::new(280, 32);

    // Listening.
    renderer.set_state(overlay::OverlayState::Listening);
    renderer.set_text("LISTENING");
    save_overlay_png(&mut renderer, &dir.join("listening.png"))?;

    // Processing.
    renderer.set_state(overlay::OverlayState::Processing);
    renderer.set_text("PROCESSING");
    save_overlay_png(&mut renderer, &dir.join("processing.png"))?;

    // Done: final transcribed text.
    renderer.set_state(overlay::OverlayState::Done);
    renderer.set_text("Deploy to Kubernetes.");
    save_overlay_png(&mut renderer, &dir.join("done.png"))?;

    println!("Screenshots saved to {dir:?}");
    Ok(())
}

fn save_overlay_png(renderer: &mut overlay::Renderer, path: &std::path::Path) -> Result<()> {
    use image::{ImageBuffer, Rgba};
    let width = renderer.width();
    let height = renderer.height();
    let rgba = renderer.render();
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
                println!("Added '{word}' to dictionary.");
            } else {
                println!("'{word}' is already in the dictionary.");
            }
        }
        DictAction::Remove { word } => {
            if config.remove_dictionary_word(&word) {
                config.save()?;
                println!("Removed '{word}' from dictionary.");
            } else {
                println!("'{word}' is not in the dictionary.");
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
            println!("Imported dictionary from {path:?}.");
        }
        DictAction::Export { path } => {
            config.export_dictionary(&path)?;
            println!("Exported dictionary to {path:?}.");
        }
    }
    Ok(())
}

fn handle_model(action: ModelAction) -> Result<()> {
    match action {
        ModelAction::List => {
            for info in stt::list_models() {
                let status = if info.installed { "installed" } else { "not installed" };
                println!(
                    "  {:24} ~{:>5} MB   {}",
                    info.name, info.size_mb, status
                );
            }
        }
        ModelAction::Download { name } => {
            stt::download_model_by_name(&name)?;
            println!("Downloaded model '{name}'.");
        }
        ModelAction::Remove { name } => {
            stt::remove_model(&name)?;
            println!("Removed model '{name}'.");
        }
    }
    Ok(())
}

fn show_logs() -> Result<()> {
    let log_path = directories::ProjectDirs::from("", "", "mist")
        .map(|d| d.data_dir().join("mist.log"));
    let Some(path) = log_path else {
        anyhow::bail!("Could not determine project data directory.");
    };
    if !path.exists() {
        println!("No log file found at {path:?}");
        return Ok(());
    }
    let contents = std::fs::read_to_string(&path)?;
    let lines: Vec<&str> = contents.lines().collect();
    let tail = lines.iter().rev().take(200).rev().copied().collect::<Vec<_>>();
    for line in tail {
        println!("{line}");
    }
    Ok(())
}

fn show_status() -> Result<()> {
    let config = config::Config::load()?;
    let data_dir =
        directories::ProjectDirs::from("", "", "mist").map(|d| d.data_dir().to_path_buf());
    let typing_ok = paste::typing_backend_available();

    println!("Mist status");
    println!("  Config path:     {:?}", config::Config::path()?);
    println!("  Config exists:   {}", config::Config::path()?.exists());
    println!("  Data dir:        {data_dir:?}");
    println!("  Model:           {}", config.model);
    println!("  Model file:      {:?}", config.model_path()?);
    println!("  Model exists:    {}", config.model_path()?.exists());
    println!("  Hotkey:          {}", config.hotkey);
    println!("  Cleanup:         {}", config.cleanup_backend);
    println!("  Overlay:         {}", config.show_overlay);
    println!("  Live stream:     {}", config.live_stream);
    println!("  Toggle mode:     {}", config.toggle_mode);
    println!("  Audio feedback:  {}", config.audio_feedback);
    println!(
        "  Typing backend:  {}",
        if typing_ok { "ok" } else { "missing" }
    );
    Ok(())
}

fn show_doctor() -> Result<()> {
    let mut issues = Vec::new();

    println!("Mist environment check");
    println!();

    // Config.
    match config::Config::load() {
        Ok(config) => {
            println!("[ok] Config loads");

            // Model.
            match config.model_path() {
                Ok(path) => {
                    if path.exists() {
                        println!("[ok] Model file exists: {path:?}");
                    } else {
                        println!("[warn] Model file missing: {path:?}");
                        println!("       Run: mist model download {}", config.model);
                        issues.push("model missing");
                    }
                }
                Err(e) => {
                    println!("[err] Model path error: {e}");
                    issues.push("model path");
                }
            }

            // Hotkey parse.
            if mist::hotkey::parse_hotkey(&config.hotkey).is_ok() {
                println!("[ok] Hotkey parses: {}", config.hotkey);
            } else {
                println!("[err] Hotkey invalid: {}", config.hotkey);
                issues.push("hotkey");
            }

            // Audio input.
            let host = cpal::default_host();
            if host.default_input_device().is_some() {
                println!("[ok] Default input device available");
            } else {
                println!("[err] No default input device found");
                issues.push("input device");
            }

            // Audio output when feedback enabled.
            if config.audio_feedback {
                if host.default_output_device().is_some() {
                    println!("[ok] Default output device available");
                } else {
                    println!("[warn] Audio feedback enabled but no output device found");
                    issues.push("output device");
                }
            }
        }
        Err(e) => {
            println!("[err] Config failed to load: {e}");
            issues.push("config");
        }
    }

    // Typing backend.
    if paste::typing_backend_available() {
        println!("[ok] Typing backend available");
    } else {
        println!("[err] Typing backend missing");
        #[cfg(target_os = "linux")]
        println!("       Install one of: xdotool, wtype, ydotool");
        #[cfg(target_os = "macos")]
        println!("       Accessibility permissions may be required");
        #[cfg(target_os = "windows")]
        println!("       Enigo fallback should work; check permissions");
        issues.push("typing backend");
    }

    println!();
    if issues.is_empty() {
        println!("No issues found.");
    } else {
        println!("Found {} issue(s): {}", issues.len(), issues.join(", "));
        std::process::exit(1);
    }
    Ok(())
}
