mod helpers;

use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn test_paste_text_exists_and_has_correct_signature() {
    // Only verify the function signature compiles. Do NOT call paste_text
    // with actual text — it invokes xdotool/wtype which types into the
    // focused window.
    let _fn_ptr: fn(&str) -> anyhow::Result<()> = mist::paste::paste_text;
}

#[test]
fn test_paste_text_error_when_no_typing_tool_available() {
    let _guard = ENV_LOCK.lock().unwrap();
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", "/dev/null");

    let result = mist::paste::paste_text("hello");
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

// NOTE: The old test_paste_text_actual_typing test is removed.
// It called xdotool/wtype which types into the user's active window,
// injecting "mist_test" into whatever app is focused. That is not safe
// in a test environment.
