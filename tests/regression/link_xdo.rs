//! Regression test: On Linux the crate must compile without `enigo`.
//!
//! `enigo` is declared only under `[target.'cfg(not(target_os = "linux"))'.dependencies]`,
//! so the Linux paste implementation must use xdotool / wtype / ydotool.

use mist::paste::paste_text;

/// Verify that on Linux `paste_text` uses xdotool/wtype/ydotool, not enigo.
/// If a typing tool is installed, paste succeeds (correct behavior).
/// If no tool is installed, the error must reference the Linux typing tools.
#[cfg(target_os = "linux")]
#[test]
fn test_paste_on_linux_uses_xdo_tools_not_enigo() {
    let result = paste_text("hello");
    match result {
        Ok(()) => {
            // A typing tool is installed and paste succeeded — that's correct.
        }
        Err(e) => {
            let err = e.to_string();
            assert!(
                err.contains("xdotool") || err.contains("wtype") || err.contains("ydotool"),
                "Error should reference Linux typing tools, not enigo. Got: {}",
                err
            );
            assert!(
                !err.to_lowercase().contains("enigo"),
                "Error should not mention enigo on Linux. Got: {}",
                err
            );
        }
    }
}

/// Compile-time guard: this test only exists when built for Linux.
/// If `enigo` were unconditionally required, compilation on Linux would fail.
#[cfg(target_os = "linux")]
#[test]
fn test_crate_compiles_on_linux_without_enigo() {
    // The presence of this compiled test is the verification.
    // (The #[cfg(target_os = "linux")] guard above ensures this only runs on Linux.)
}
