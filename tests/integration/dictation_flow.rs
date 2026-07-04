mod helpers;

use mist::audio::AudioRecorder;
use mist::cleanup::cleanup;
use mist::config::Config;
use mist::hotkey::parse_hotkey;
use mist::stt::SttEngine;

/// Test the full conceptual flow: create config → parse hotkey → create audio recorder
/// (if possible) → generate synthetic audio → verify cleanup chains together.
#[test]
fn test_full_conceptual_flow() {
    let config = Config {
        hotkey: "Ctrl+Shift+R".to_string(),
        model: "tiny.en".to_string(),
        language: "en".to_string(),
        cleanup_backend: "fast".to_string(),
        cleanup_enabled: true,
        live_stream: false,
        show_overlay: true,
        ollama_model: "qwen3:0.6b".to_string(),
        ollama_url: "http://localhost:11434".to_string(),
        cleanup_prompt: "Clean up this text.".to_string(),
        cleanup_command: String::new(),
        dictionary: vec!["Rust".to_string()],
        max_recording_secs: 120,
        n_threads: 4,
        corrections: Vec::new(),
        replacements: Vec::new(),
        ..Config::default()
    };

    // Config → hotkey parses correctly
    let hotkey = parse_hotkey(&config.hotkey);
    assert!(hotkey.is_ok(), "hotkey from config should parse");

    // AudioRecorder can be instantiated
    let mut recorder =
        AudioRecorder::new(config.max_recording_secs).expect("AudioRecorder should instantiate");

    // Try to start recording; skip if no device is present
    if let Err(e) = recorder.start() {
        eprintln!("No audio device available, skipping recording start: {}", e);
    } else {
        let _samples = recorder.stop().expect("stop should succeed after start");
    }

    // Synthetic audio stands in for captured microphone data
    let samples = helpers::synthetic_audio(16000, 1.0);
    assert!(
        !samples.is_empty(),
        "synthetic audio should produce samples"
    );

    // Cleanup is callable through the config backend
    let cleaned = cleanup(" um hello world uh ", &config).expect("cleanup should succeed");
    assert_eq!(cleaned, "hello world");
}

/// Config + hotkey integration: loading a config string yields a parsable hotkey.
#[test]
fn test_config_hotkey_integration() {
    let toml_str = helpers::config_toml("Alt+Shift+D", "small.en", "fast");
    let config: Config = toml::from_str(&toml_str).expect("config should parse from TOML");
    let hotkey = parse_hotkey(&config.hotkey);
    assert!(
        hotkey.is_ok(),
        "hotkey loaded from config TOML should parse"
    );
}

/// Config + cleanup integration: setting backend to "fast" makes cleanup callable.
#[test]
fn test_config_cleanup_integration() {
    let config = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    let result = cleanup(" um test text uh ", &config);
    assert!(result.is_ok(), "fast cleanup should be callable via config");
    assert_eq!(result.unwrap(), "test text");
}

/// AudioRecorder can be instantiated. Recording is skipped when no audio device exists.
#[test]
fn test_audio_recorder_can_be_instantiated() {
    let recorder = AudioRecorder::new(120);
    assert!(recorder.is_ok(), "AudioRecorder::new should succeed");

    if let Ok(mut rec) = recorder {
        if let Err(e) = rec.start() {
            eprintln!("No audio device available, skipping start/stop: {}", e);
            return;
        }
        let samples = rec.stop();
        assert!(samples.is_ok(), "stop should succeed after start");
    }
}

/// SttEngine must return an error (not panic) when the model file is missing/invalid.
#[test]
fn test_stt_engine_reports_error_on_invalid_model() {
    let tmp = tempfile::tempdir().unwrap();
    // Create an empty file so the model "exists" but is corrupt/invalid.
    // This bypasses the auto-download path and forces WhisperContext to reject it.
    let model_path = tmp.path().join("ggml-empty.bin");
    std::fs::write(&model_path, b"").unwrap();

    let result = SttEngine::new(&model_path);
    assert!(
        result.is_err(),
        "SttEngine should return an error for an invalid model file, not panic"
    );
}
