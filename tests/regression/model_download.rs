//! Regression tests for model path handling in the STT engine.

use flow::config::Config;
use flow::stt::SttEngine;

/// `SttEngine::new` with a non-existent model path must return an error,
/// never panic. The implementation attempts to download the missing model;
/// because the model name is fake the download fails (404 or network error)
/// and the error propagates back to the caller.
#[test]
fn test_stt_new_nonexistent_model_returns_error_not_panic() {
    let tmp = tempfile::tempdir().unwrap();
    // Path does not exist; SttEngine::new will attempt to download and fail.
    let model_path = tmp.path().join("ggml-nonexistent-model.bin");

    let result = SttEngine::new(&model_path);
    assert!(
        result.is_err(),
        "SttEngine::new should return an error for a missing/undownloadable model, not panic"
    );
}

/// `Config::model_path` must place the model inside the correct `flow` data
/// directory and use the `ggml-{model}.bin` naming convention.
#[test]
fn test_model_path_resolution_directory_structure() {
    let config = Config {
        model: "small.en".to_string(),
        ..Config::default()
    };

    let path = config.model_path().expect("model_path should resolve");
    let file_name = path
        .file_name()
        .expect("path should have a file name")
        .to_str()
        .expect("file name should be valid UTF-8");
    assert_eq!(file_name, "ggml-small.en.bin");

    let path_str = path.to_string_lossy();
    assert!(
        path_str.contains("flow"),
        "Model path should be inside the flow data directory: {}",
        path_str
    );
}
