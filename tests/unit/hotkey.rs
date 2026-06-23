mod helpers;

use flow::hotkey::parse_hotkey;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};

#[test]
fn test_valid_hotkeys() {
    assert!(parse_hotkey("Alt+Shift+D").is_ok());
    assert!(parse_hotkey("Ctrl+Space").is_ok());
    assert!(parse_hotkey("F12").is_ok());
    assert!(parse_hotkey("Meta+R").is_ok());
}

#[test]
fn test_invalid_hotkeys() {
    assert!(parse_hotkey("").is_err());
    assert!(parse_hotkey("Ctrl+").is_err());
    assert!(parse_hotkey("Banana").is_err());
}

#[test]
fn test_extra_unknown_token_is_ignored_by_current_impl() {
    // The current parser ignores unknown tokens rather than failing.
    let hk = parse_hotkey("Alt+Ctrl+Shift+Meta+F1+Extra").unwrap();
    assert_eq!(
        hk,
        HotKey::new(
            Some(Modifiers::ALT | Modifiers::CONTROL | Modifiers::SHIFT | Modifiers::META),
            Code::F1,
        )
    );
}

#[test]
fn test_case_insensitivity() {
    let lower = parse_hotkey("alt+shift+d").unwrap();
    let upper = parse_hotkey("ALT+SHIFT+D").unwrap();
    assert_eq!(lower, upper);
}

#[test]
fn test_all_modifier_combinations() {
    let combos = [
        "Alt+A",
        "Ctrl+B",
        "Shift+C",
        "Meta+D",
        "Alt+Ctrl+E",
        "Alt+Shift+F",
        "Alt+Meta+G",
        "Ctrl+Shift+H",
        "Ctrl+Meta+I",
        "Shift+Meta+J",
        "Alt+Ctrl+Shift+K",
        "Alt+Ctrl+Meta+L",
        "Alt+Shift+Meta+M",
        "Ctrl+Shift+Meta+N",
        "Alt+Ctrl+Shift+Meta+O",
    ];
    for combo in &combos {
        assert!(parse_hotkey(combo).is_ok(), "failed to parse {}", combo);
    }
}

#[test]
fn test_single_key_without_modifiers() {
    let space = parse_hotkey("Space").unwrap();
    assert_eq!(space, HotKey::new(Some(Modifiers::empty()), Code::Space));

    let f12 = parse_hotkey("F12").unwrap();
    assert_eq!(f12, HotKey::new(Some(Modifiers::empty()), Code::F12));

    let a = parse_hotkey("a").unwrap();
    assert_eq!(a, HotKey::new(Some(Modifiers::empty()), Code::KeyA));
}
