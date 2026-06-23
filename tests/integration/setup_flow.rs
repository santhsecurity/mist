mod helpers;

use flow::config::Config;

/// Default config can be created and saved to disk.
#[test]
fn test_default_config_created_and_saved() {
    let tmp = tempfile::tempdir().unwrap();
    // Redirect the config directory into the temp folder so we don't touch
    // the user's real configuration.
    std::env::set_var("XDG_CONFIG_HOME", tmp.path());

    let config = Config::default();
    config.save().expect("default config should save without error");

    let path = Config::path().expect("config path should resolve");
    assert!(path.exists(), "saved config file should exist on disk");

    // Read back and parse to verify contents were written
    let contents = std::fs::read_to_string(&path).expect("should read saved file");
    let loaded: Config = toml::from_str(&contents).expect("saved TOML should parse");
    assert_eq!(loaded.hotkey, config.hotkey);
    assert_eq!(loaded.model, config.model);
    assert_eq!(loaded.cleanup_backend, config.cleanup_backend);
    assert_eq!(loaded.cleanup_enabled, config.cleanup_enabled);
    assert_eq!(loaded.max_recording_secs, config.max_recording_secs);
}

/// Config round-trips through TOML serialization without data loss.
#[test]
fn test_config_roundtrips_through_toml() {
    let config = Config {
        hotkey: "Ctrl+F12".to_string(),
        model: "base.en".to_string(),
        language: "en".to_string(),
        cleanup_backend: "ollama".to_string(),
        cleanup_enabled: false,
        live_stream: true,
        show_overlay: false,
        ollama_model: "mistral".to_string(),
        ollama_url: "http://127.0.0.1:11434".to_string(),
        cleanup_prompt: "Fix it.".to_string(),
        cleanup_command: "cat".to_string(),
        dictionary: vec!["foo".to_string(), "bar".to_string()],
        max_recording_secs: 60,
        n_threads: 8,
        corrections: Vec::new(),
    };

    let toml = toml::to_string_pretty(&config).expect("config should serialize to TOML");
    let roundtripped: Config = toml::from_str(&toml).expect("config should deserialize from TOML");

    assert_eq!(roundtripped.hotkey, config.hotkey);
    assert_eq!(roundtripped.model, config.model);
    assert_eq!(roundtripped.language, config.language);
    assert_eq!(roundtripped.cleanup_backend, config.cleanup_backend);
    assert_eq!(roundtripped.cleanup_enabled, config.cleanup_enabled);
    assert_eq!(roundtripped.live_stream, config.live_stream);
    assert_eq!(roundtripped.show_overlay, config.show_overlay);
    assert_eq!(roundtripped.ollama_model, config.ollama_model);
    assert_eq!(roundtripped.ollama_url, config.ollama_url);
    assert_eq!(roundtripped.cleanup_prompt, config.cleanup_prompt);
    assert_eq!(roundtripped.cleanup_command, config.cleanup_command);
    assert_eq!(roundtripped.dictionary, config.dictionary);
    assert_eq!(roundtripped.max_recording_secs, config.max_recording_secs);
    assert_eq!(roundtripped.n_threads, config.n_threads);
}

/// All documented backend options are valid strings that deserialize correctly.
#[test]
fn test_all_backend_options_are_valid_strings() {
    let valid_backends = ["fast", "candle", "ollama", "command", "none"];
    for backend in &valid_backends {
        let toml_str = helpers::config_toml("Alt+D", "tiny.en", backend);
        let config: Config = toml::from_str(&toml_str).expect("valid TOML for every backend");
        assert_eq!(&config.cleanup_backend, *backend);
    }
}

/// Mutating one field must not accidentally change unrelated defaults.
#[test]
fn test_changing_one_config_field_does_not_affect_others() {
    let base = Config::default();
    let mut modified = base.clone();
    modified.hotkey = "Ctrl+X".to_string();
    modified.model = "medium.en".to_string();

    assert_ne!(modified.hotkey, base.hotkey);
    assert_ne!(modified.model, base.model);

    // Verify all other fields stayed the same
    assert_eq!(modified.language, base.language);
    assert_eq!(modified.cleanup_backend, base.cleanup_backend);
    assert_eq!(modified.cleanup_enabled, base.cleanup_enabled);
    assert_eq!(modified.live_stream, base.live_stream);
    assert_eq!(modified.show_overlay, base.show_overlay);
    assert_eq!(modified.ollama_model, base.ollama_model);
    assert_eq!(modified.ollama_url, base.ollama_url);
    assert_eq!(modified.cleanup_prompt, base.cleanup_prompt);
    assert_eq!(modified.cleanup_command, base.cleanup_command);
    assert_eq!(modified.dictionary, base.dictionary);
    assert_eq!(modified.max_recording_secs, base.max_recording_secs);
    assert_eq!(modified.n_threads, base.n_threads);
}
