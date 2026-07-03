use mist::config::Config;
use proptest::prelude::*;

fn random_string() -> impl Strategy<Value = String> {
    r"[a-zA-Z0-9_\-]{1,20}"
}

proptest! {
    #[test]
    fn config_roundtrips_through_toml(
        hotkey in random_string(),
        model in random_string(),
        language in random_string(),
        backend in random_string(),
        ollama_model in random_string(),
        ollama_url in random_string(),
        cleanup_prompt in random_string(),
        cleanup_command in random_string(),
        cleanup_enabled in any::<bool>(),
        live_stream in any::<bool>(),
        show_overlay in any::<bool>(),
        max_recording_secs in 1u32..600u32,
        n_threads in 1u32..32u32,
    ) {
        let cfg = Config {
            hotkey,
            model,
            language,
            cleanup_backend: backend,
            ollama_model,
            ollama_url,
            cleanup_prompt,
            cleanup_command,
            cleanup_enabled,
            live_stream,
            show_overlay,
            max_recording_secs,
            n_threads,
            ..Config::default()
        };

        let toml_str = toml::to_string_pretty(&cfg).unwrap();
        let loaded: Config = toml::from_str(&toml_str).unwrap();

        prop_assert_eq!(loaded.hotkey, cfg.hotkey);
        prop_assert_eq!(loaded.model, cfg.model);
        prop_assert_eq!(loaded.language, cfg.language);
        prop_assert_eq!(loaded.cleanup_backend, cfg.cleanup_backend);
        prop_assert_eq!(loaded.ollama_model, cfg.ollama_model);
        prop_assert_eq!(loaded.ollama_url, cfg.ollama_url);
        prop_assert_eq!(loaded.cleanup_prompt, cfg.cleanup_prompt);
        prop_assert_eq!(loaded.cleanup_command, cfg.cleanup_command);
        prop_assert_eq!(loaded.cleanup_enabled, cfg.cleanup_enabled);
        prop_assert_eq!(loaded.live_stream, cfg.live_stream);
        prop_assert_eq!(loaded.show_overlay, cfg.show_overlay);
        prop_assert_eq!(loaded.max_recording_secs, cfg.max_recording_secs);
        prop_assert_eq!(loaded.n_threads, cfg.n_threads);
    }

    #[test]
    fn boolean_defaults_when_missing(
        hotkey in random_string(),
        model in random_string(),
    ) {
        let toml = format!(
            "hotkey = \"{}\"\nmodel = \"{}\"\n",
            hotkey, model
        );
        let cfg: Config = toml::from_str(&toml).unwrap();
        prop_assert_eq!(cfg.cleanup_enabled, true);
        prop_assert_eq!(cfg.live_stream, false);
        prop_assert_eq!(cfg.show_overlay, true);
        prop_assert_eq!(cfg.max_recording_secs, 120u32);
    }
}
