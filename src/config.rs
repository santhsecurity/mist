use anyhow::Result;
use directories::ProjectDirs;
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const DEFAULT_CLEANUP_PROMPT: &str =
    "You are a dictation cleanup assistant. Your ONLY job is to correct formatting errors \
     in transcribed text. Fix capitalization, add proper punctuation, remove filler words \
     like 'um' and 'uh', and fix obvious false starts. Preserve the speaker's original words, \
     meaning, and voice. Do NOT add explanations or commentary. Return ONLY the cleaned text.";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_cleanup_backend")]
    pub cleanup_backend: String,
    #[serde(default = "default_cleanup_enabled")]
    pub cleanup_enabled: bool,
    #[serde(default = "default_live_stream")]
    pub live_stream: bool,
    #[serde(default = "default_show_overlay")]
    pub show_overlay: bool,
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
    #[serde(default = "default_cleanup_prompt")]
    pub cleanup_prompt: String,
    #[serde(default)]
    pub cleanup_command: String,
    #[serde(default)]
    pub dictionary: Vec<String>,
    #[serde(default = "default_max_recording_secs")]
    pub max_recording_secs: u32,
    #[serde(default = "default_n_threads")]
    pub n_threads: u32,
    #[serde(default)]
    pub corrections: Vec<CorrectionEntry>,
}

/// A vocabulary correction entry: a list of patterns that should all map to one
/// canonical spelling.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CorrectionEntry {
    pub patterns: Vec<String>,
    pub correct: String,
}

fn default_hotkey() -> String {
    "Alt+Shift+D".to_string()
}

fn default_model() -> String {
    "small.en".to_string()
}

fn default_language() -> String {
    "en".to_string()
}

fn default_cleanup_backend() -> String {
    "fast".to_string()
}

fn default_cleanup_enabled() -> bool {
    true
}

fn default_live_stream() -> bool {
    false
}

fn default_show_overlay() -> bool {
    true
}

fn default_ollama_model() -> String {
    "qwen3:0.6b".to_string()
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_cleanup_prompt() -> String {
    DEFAULT_CLEANUP_PROMPT.to_string()
}

fn default_max_recording_secs() -> u32 {
    120
}

fn default_n_threads() -> u32 {
    let n = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4);
    // Use all cores but cap at 16 to avoid diminishing returns.
    n.min(16)
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            model: default_model(),
            language: default_language(),
            cleanup_backend: default_cleanup_backend(),
            cleanup_enabled: default_cleanup_enabled(),
            live_stream: default_live_stream(),
            show_overlay: default_show_overlay(),
            ollama_model: default_ollama_model(),
            ollama_url: default_ollama_url(),
            cleanup_prompt: default_cleanup_prompt(),
            cleanup_command: String::new(),
            dictionary: Vec::new(),
            max_recording_secs: default_max_recording_secs(),
            n_threads: default_n_threads(),
            corrections: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;

            // Warn about unknown keys before deserializing (serde silently
            // ignores them with the default settings).
            Self::warn_unknown_keys(&content);

            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn path() -> Result<PathBuf> {
        let dirs = ProjectDirs::from("", "", "flow")
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    pub fn model_path(&self) -> Result<PathBuf> {
        let dirs = ProjectDirs::from("", "", "flow")
            .ok_or_else(|| anyhow::anyhow!("Could not find data directory"))?;
        Ok(dirs.data_dir().join(format!("ggml-{}.bin", self.model)))
    }

    /// Try to load a per-project dictionary from the current working directory.
    /// Looks for `.flow-dictionary.toml` in the cwd and parent directories
    /// (up to 5 levels). Returns terms that should be appended to the global
    /// dictionary.
    pub fn project_dictionary() -> Vec<String> {
        let Ok(mut dir) = std::env::current_dir() else {
            return Vec::new();
        };
        for _ in 0..5 {
            let candidate = dir.join(".flow-dictionary.toml");
            if candidate.is_file() {
                if let Ok(content) = fs::read_to_string(&candidate) {
                    #[derive(Deserialize)]
                    struct ProjectDict {
                        #[serde(default)]
                        terms: Vec<String>,
                    }
                    if let Ok(pd) = toml::from_str::<ProjectDict>(&content) {
                        return pd.terms;
                    }
                }
            }
            if !dir.pop() {
                break;
            }
        }
        Vec::new()
    }

    /// Emit warnings for any TOML keys that are not recognised config fields.
    fn warn_unknown_keys(content: &str) {
        let known: &[&str] = &[
            "hotkey",
            "model",
            "language",
            "cleanup_backend",
            "cleanup_enabled",
            "live_stream",
            "show_overlay",
            "ollama_model",
            "ollama_url",
            "cleanup_prompt",
            "cleanup_command",
            "dictionary",
            "max_recording_secs",
            "n_threads",
            "corrections",
        ];

        // Quick parse to get top-level keys.
        if let Ok(table) = content.parse::<toml::Table>() {
            for key in table.keys() {
                if !known.contains(&key.as_str()) {
                    warn!(
                        "Unknown config key '{}' in config.toml (typo?). \
                         Known keys: {}",
                        key,
                        known.join(", ")
                    );
                }
            }
        }
    }

    /// Build the effective dictionary by merging the global dictionary with
    /// any per-project terms.
    pub fn effective_dictionary(&self) -> Vec<String> {
        let mut dict = self.dictionary.clone();
        let project_terms = Self::project_dictionary();
        for term in project_terms {
            if !dict.contains(&term) {
                dict.push(term);
            }
        }
        dict
    }

    /// Build a HashMap from lowercased pattern → canonical correction.
    pub fn correction_map(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for entry in &self.corrections {
            for pattern in &entry.patterns {
                map.insert(pattern.to_lowercase(), entry.correct.clone());
            }
        }
        map
    }

    pub fn setup_interactive() -> Result<()> {
        use dialoguer::{Confirm, Input, Select};

        let mut config = Config::load().unwrap_or_default();

        println!("\n━━━ Flow Setup ━━━\n");

        // Model
        let models = vec!["tiny.en", "base.en", "small.en", "medium.en"];
        let default = models.iter().position(|m| *m == config.model).unwrap_or(2);
        let idx = Select::new()
            .with_prompt("Whisper model")
            .items(&models)
            .default(default)
            .interact()?;
        config.model = models[idx].to_string();

        // Cleanup backend
        let backends = vec!["fast", "candle", "ollama", "command", "none"];
        let default = backends
            .iter()
            .position(|b| *b == config.cleanup_backend)
            .unwrap_or(0);
        let idx = Select::new()
            .with_prompt("Cleanup backend")
            .items(&backends)
            .default(default)
            .interact()?;
        config.cleanup_backend = backends[idx].to_string();

        if config.cleanup_backend == "ollama" {
            config.ollama_model = Input::new()
                .with_prompt("Ollama model")
                .default(config.ollama_model)
                .interact_text()?;
            config.ollama_url = Input::new()
                .with_prompt("Ollama URL")
                .default(config.ollama_url)
                .interact_text()?;
        }

        if config.cleanup_backend == "command" {
            config.cleanup_command = Input::new()
                .with_prompt("Cleanup command (receives text on stdin, outputs on stdout)")
                .default(config.cleanup_command)
                .interact_text()?;
        }

        // Cleanup prompt
        if Confirm::new()
            .with_prompt("Edit cleanup prompt?")
            .default(false)
            .interact()?
        {
            config.cleanup_prompt = Input::new()
                .with_prompt("Cleanup prompt")
                .default(config.cleanup_prompt)
                .interact_text()?;
        }

        // Live stream
        config.live_stream = Confirm::new()
            .with_prompt("Enable live stream preview?")
            .default(config.live_stream)
            .interact()?;

        config.show_overlay = Confirm::new()
            .with_prompt("Show recording overlay?")
            .default(config.show_overlay)
            .interact()?;

        // Max recording duration
        let max_secs: String = Input::new()
            .with_prompt("Max recording duration (seconds)")
            .default(config.max_recording_secs.to_string())
            .interact_text()?;
        config.max_recording_secs = max_secs.parse().unwrap_or(120);

        // Dictionary
        loop {
            println!("\nCurrent dictionary: {:?}", config.dictionary);
            let choices = vec!["Add word", "Remove word", "Done"];
            let idx = Select::new()
                .with_prompt("Dictionary")
                .items(&choices)
                .default(2)
                .interact()?;
            match idx {
                0 => {
                    let word: String = Input::new()
                        .with_prompt("Word to add")
                        .interact_text()?;
                    if !word.is_empty() && !config.dictionary.contains(&word) {
                        config.dictionary.push(word);
                    }
                }
                1 => {
                    if config.dictionary.is_empty() {
                        println!("Dictionary is empty.");
                        continue;
                    }
                    let idx = Select::new()
                        .with_prompt("Word to remove")
                        .items(&config.dictionary)
                        .interact()?;
                    config.dictionary.remove(idx);
                }
                _ => break,
            }
        }

        // Hotkey
        config.hotkey = Input::new()
            .with_prompt("Global hotkey")
            .default(config.hotkey)
            .interact_text()?;

        config.save()?;
        println!("\n✓ Config saved to {:?}", Config::path()?);
        Ok(())
    }
}
