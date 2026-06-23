mod helpers;

use std::sync::Mutex;
use tempfile::tempdir;
use flow::config::Config;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_default_config_values() {
    let config = Config::default();
    assert_eq!(config.hotkey, "Alt+Shift+D");
    assert_eq!(config.model, "small.en");
    assert_eq!(config.language, "en");
    assert_eq!(config.cleanup_backend, "fast");
    assert!(config.cleanup_enabled);
    assert!(!config.live_stream);
    assert!(config.show_overlay);
    assert_eq!(config.ollama_model, "qwen3:0.6b");
    assert_eq!(config.ollama_url, "http://localhost:11434");
    assert!(config.cleanup_prompt.contains("dictation cleanup assistant"));
    assert!(config.dictionary.is_empty());
    assert_eq!(config.max_recording_secs, 120);
    assert!(config.n_threads > 0);
    assert!(config.corrections.is_empty());
}

#[test]
fn test_show_overlay_defaults_to_true() {
    let config = Config::default();
    assert!(config.show_overlay);
}

#[test]
fn test_round_trip_save_load() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let original = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", temp.path());

    let config = Config {
        hotkey: "Ctrl+Space".to_string(),
        model: "base.en".to_string(),
        ..Config::default()
    };

    config.save().unwrap();
    let loaded = Config::load().unwrap();

    assert_eq!(loaded.hotkey, config.hotkey);
    assert_eq!(loaded.model, config.model);
    assert_eq!(loaded.cleanup_backend, config.cleanup_backend);
    assert_eq!(loaded.max_recording_secs, config.max_recording_secs);

    if let Some(val) = original {
        std::env::set_var("XDG_CONFIG_HOME", val);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_missing_config_file_creates_defaults() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let original = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", temp.path());

    let loaded = Config::load().unwrap();
    assert_eq!(loaded.hotkey, Config::default().hotkey);
    assert_eq!(loaded.model, Config::default().model);

    if let Some(val) = original {
        std::env::set_var("XDG_CONFIG_HOME", val);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_malformed_toml_returns_error_gracefully() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let original = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", temp.path());

    let config_dir = temp.path().join("flow");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "not valid toml <<<").unwrap();

    let result = Config::load();
    assert!(result.is_err());

    if let Some(val) = original {
        std::env::set_var("XDG_CONFIG_HOME", val);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_model_path_returns_expected_subpath() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let original = std::env::var_os("XDG_DATA_HOME");
    std::env::set_var("XDG_DATA_HOME", temp.path());

    let config = Config::default();
    let path = config.model_path().unwrap();
    let path_str = path.to_string_lossy();
    assert!(path_str.contains("ggml-small.en.bin"));

    if let Some(val) = original {
        std::env::set_var("XDG_DATA_HOME", val);
    } else {
        std::env::remove_var("XDG_DATA_HOME");
    }
}

#[test]
fn test_path_returns_expected_subpath() {
    let _guard = ENV_LOCK.lock().unwrap();
    let temp = tempdir().unwrap();
    let original = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", temp.path());

    let path = Config::path().unwrap();
    let path_str = path.to_string_lossy();
    assert!(path_str.contains("config.toml"));

    if let Some(val) = original {
        std::env::set_var("XDG_CONFIG_HOME", val);
    } else {
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

#[test]
fn test_new_config_fields_have_sane_defaults() {
    let config = Config::default();
    assert_eq!(config.max_recording_secs, 120);
    // n_threads should be > 0 and <= 16 on any machine.
    assert!(config.n_threads > 0 && config.n_threads <= 16);
    assert!(config.corrections.is_empty());
}
