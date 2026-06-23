//! Regression test for tensor loading issues in whisper-rs.
//!
//! Earlier versions could panic or abort when given partial or corrupt model
//! files. `SttEngine::new` must reject such files gracefully by returning
//! `Result::Err` instead of crashing the process.

use flow::stt::SttEngine;

/// Partial, corrupt, and empty model files must all be rejected gracefully.
#[test]
fn test_corrupt_model_files_rejected_gracefully() {
    let tmp = tempfile::tempdir().unwrap();

    // Empty file
    let empty = tmp.path().join("ggml-empty.bin");
    std::fs::write(&empty, b"").unwrap();
    assert!(
        SttEngine::new(&empty).is_err(),
        "Empty model file should be rejected, not panic"
    );

    // Truncated / partial file
    let partial = tmp.path().join("ggml-partial.bin");
    std::fs::write(&partial, b"ggml").unwrap();
    assert!(
        SttEngine::new(&partial).is_err(),
        "Partial model file should be rejected, not panic"
    );

    // Random garbage
    let garbage = tmp.path().join("ggml-garbage.bin");
    std::fs::write(&garbage, vec![0xDEu8; 1024]).unwrap();
    assert!(
        SttEngine::new(&garbage).is_err(),
        "Corrupt model file should be rejected, not panic"
    );
}
