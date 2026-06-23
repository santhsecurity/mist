mod helpers;

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_paste_text_exists_and_has_correct_signature() {
    // Verify the public API is callable and returns Result<()>
    let result = flow::paste::paste_text("");
    let _result: anyhow::Result<()> = result;
}

#[test]
fn test_paste_text_error_when_no_typing_tool_available() {
    let _guard = ENV_LOCK.lock().unwrap();
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", "/dev/null");

    let result = flow::paste::paste_text("hello");
    assert!(
        result.is_err(),
        "expected error when no typing tool is available"
    );

    if let Some(val) = original_path {
        std::env::set_var("PATH", val);
    } else {
        std::env::remove_var("PATH");
    }
}

#[test]
fn test_paste_text_actual_typing() {
    let has_display = std::env::var_os("DISPLAY").is_some()
        || std::env::var_os("WAYLAND_DISPLAY").is_some();

    let tool_available = std::process::Command::new("xdotool")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        || std::process::Command::new("wtype")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        || std::process::Command::new("ydotool")
            .arg("--version")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

    if !has_display || !tool_available {
        eprintln!("Skipping actual typing test: no display or typing tool available");
        return;
    }

    // We only verify the call completes without panicking.
    // Actual typing success depends on the environment (focus, permissions).
    let _result = flow::paste::paste_text("flow_test");
}
