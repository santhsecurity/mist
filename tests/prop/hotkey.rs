use flow::hotkey::parse_hotkey;
use proptest::prelude::*;
use proptest::sample;

fn random_string() -> impl Strategy<Value = String> {
    proptest::collection::vec(any::<char>(), 0..200)
        .prop_map(|chars| chars.into_iter().collect())
}

fn valid_hotkey_strategy() -> impl Strategy<Value = String> {
    let modifiers = &["Alt", "Ctrl", "Shift", "Meta", "Cmd", "Super", "Win"];
    let keys = &[
        "Space", "Tab", "Enter", "Return", "Esc", "Escape",
        "Up", "Down", "Left", "Right",
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
        "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m",
        "n", "o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9",
    ];
    (sample::subsequence(modifiers, 0..=4), sample::select(keys))
        .prop_map(|(mods, key)| {
            if mods.is_empty() {
                key.to_string()
            } else {
                format!("{}+{}", mods.join("+"), key)
            }
        })
}

fn modifiers_only_strategy() -> impl Strategy<Value = String> {
    let modifiers = &["Alt", "Ctrl", "Shift", "Meta", "Cmd", "Super", "Win"];
    sample::subsequence(modifiers, 1..=4)
        .prop_map(|mods| mods.join("+"))
}

proptest! {
    #[test]
    fn parse_hotkey_never_panics(s in random_string()) {
        let _ = parse_hotkey(&s);
    }

    #[test]
    fn parsed_hotkey_has_nonzero_id(s in random_string()) {
        if let Ok(hk) = parse_hotkey(&s) {
            prop_assert_ne!(hk.id, 0, "hotkey id should be non-zero for: {}", s);
        }
    }

    #[test]
    fn valid_known_hotkeys_always_parse(s in valid_hotkey_strategy()) {
        prop_assert!(parse_hotkey(&s).is_ok(), "failed for {}", s);
    }

    #[test]
    fn only_modifiers_always_fail(s in modifiers_only_strategy()) {
        prop_assert!(parse_hotkey(&s).is_err(), "should fail for {}", s);
    }
}
