use flow::config::Config;
use flow::hotkey::parse_hotkey;
use flow::paste::paste_text;

fn fast_cleanup(text: &str) -> anyhow::Result<String> {
    let cfg = Config {
        cleanup_backend: "fast".to_string(),
        ..Config::default()
    };
    flow::cleanup::cleanup(text, &cfg)
}

#[test]
fn hotkey_empty_string() {
    assert!(parse_hotkey("").is_err());
}

#[test]
fn hotkey_one_mb_string() {
    let s = "A+".repeat(512 * 1024);
    // Should not panic on very long input; may or may not parse successfully
    let _ = parse_hotkey(&s);
}

#[test]
fn hotkey_null_bytes() {
    let s = "Ctrl\0+A";
    // Should not panic; null byte is not valid but parser may ignore it
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
    // ~100 KB of " um um um " with trailing spaces so pattern matches
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
    // Unknown fields produce warnings but don't fail deserialization.
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

#[test]
fn paste_empty_string() {
    let _ = paste_text("");
}

#[test]
fn paste_unicode() {
    let _ = paste_text("Hello 世界 🌍");
}

#[test]
fn paste_very_long_string() {
    let s = "a".repeat(10_000);
    let _ = paste_text(&s);
}
