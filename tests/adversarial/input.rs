use mist::config::Config;
use mist::hotkey::parse_hotkey;

fn fast_cleanup(text: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    mist::cleanup::cleanup(text, &cfg)
}

#[test]
fn hotkey_empty_string() {
    assert!(parse_hotkey("").is_err());
}

#[test]
fn hotkey_one_mb_string() {
    let s = "A+".repeat(512 * 1024);
    let _ = parse_hotkey(&s);
}

#[test]
fn hotkey_null_bytes() {
    let s = "Ctrl\0+A";
    let _ = parse_hotkey(s);
}

#[test]
fn hotkey_unicode() {
    assert!(parse_hotkey("你好").is_err());
}

#[test]
fn hotkey_rtl_text() {
    assert!(parse_hotkey("שלום").is_err());
}

#[test]
fn hotkey_emoji() {
    assert!(parse_hotkey("😀").is_err());
}

#[test]
fn cleanup_empty_string() {
    assert_eq!(fast_cleanup("").unwrap(), "");
}

#[test]
fn cleanup_only_whitespace() {
    assert_eq!(fast_cleanup("     ").unwrap(), "");
}

#[test]
fn cleanup_huge_um_chain() {
    let text = " um ".repeat(100_000 / 3);
    let result = fast_cleanup(&text).unwrap();
    assert!(!result.contains(" um "));
    assert!(result.len() < text.len());
}

#[test]
fn cleanup_mixed_scripts() {
    let text = "um hello 世界 um こんにちは";
    let result = fast_cleanup(text).unwrap();
    assert!(result.contains("世界"));
    assert!(result.contains("こんにちは"));
}

#[test]
fn config_empty_toml() {
    let cfg: Config = toml::from_str("").unwrap();
    assert_eq!(cfg.hotkey, Config::default().hotkey);
}

#[test]
fn config_malformed_toml() {
    let cases = [
        "hotkey = 42",
        "cleanup_enabled = maybe",
        "dictionary = \"not a list\"",
        "[[invalid",
    ];
    for s in &cases {
        let result: Result<Config, _> = toml::from_str(s);
        assert!(result.is_err(), "should fail for: {}", s);
    }
}

#[test]
fn config_extra_unknown_fields() {
    let toml_str = r#"
hotkey = "F12"
unknown_field = "hello"
another_unknown = 42
"#;
    let cfg: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.hotkey, "F12");
}

#[test]
fn config_wrong_types() {
    let cases = [
        ("hotkey = true", "bool instead of string"),
        ("live_stream = \"yes\"", "string instead of bool"),
        ("dictionary = 123", "int instead of array"),
    ];
    for (toml, desc) in &cases {
        let result: Result<Config, _> = toml::from_str(toml);
        assert!(result.is_err(), "should fail for {}: {}", desc, toml);
    }
}

// NOTE: paste_text tests are deliberately limited to the error path.
// Calling paste_text with actual text invokes xdotool/wtype/ydotool which
// types into whatever window is focused - not safe in a test environment.
#[test]
fn paste_no_tool_available() {
    // With PATH neutered, paste should fail cleanly.
    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", "/dev/null");
    let result = mist::paste::paste_text("safe because no tool can run");
    assert!(result.is_err());
    if let Some(val) = original_path {
        std::env::set_var("PATH", val);
    } else {
        std::env::remove_var("PATH");
    }
}
