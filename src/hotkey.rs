use anyhow::Result;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};

pub fn parse_hotkey(s: &str) -> Result<HotKey> {
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let mut modifiers = Modifiers::empty();
    let mut key: Option<Code> = None;

    for part in &parts {
        match part.to_lowercase().as_str() {
            "alt" => modifiers |= Modifiers::ALT,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "meta" | "cmd" | "command" | "super" | "win" => modifiers |= Modifiers::META,
            "f1" => key = Some(Code::F1),
            "f2" => key = Some(Code::F2),
            "f3" => key = Some(Code::F3),
            "f4" => key = Some(Code::F4),
            "f5" => key = Some(Code::F5),
            "f6" => key = Some(Code::F6),
            "f7" => key = Some(Code::F7),
            "f8" => key = Some(Code::F8),
            "f9" => key = Some(Code::F9),
            "f10" => key = Some(Code::F10),
            "f11" => key = Some(Code::F11),
            "f12" => key = Some(Code::F12),
            "space" => key = Some(Code::Space),
            "tab" => key = Some(Code::Tab),
            "enter" | "return" => key = Some(Code::Enter),
            "esc" | "escape" => key = Some(Code::Escape),
            "up" => key = Some(Code::ArrowUp),
            "down" => key = Some(Code::ArrowDown),
            "left" => key = Some(Code::ArrowLeft),
            "right" => key = Some(Code::ArrowRight),
            k => {
                if k.len() == 1 {
                    match k.chars().next().unwrap() {
                        'a' => key = Some(Code::KeyA),
                        'b' => key = Some(Code::KeyB),
                        'c' => key = Some(Code::KeyC),
                        'd' => key = Some(Code::KeyD),
                        'e' => key = Some(Code::KeyE),
                        'f' => key = Some(Code::KeyF),
                        'g' => key = Some(Code::KeyG),
                        'h' => key = Some(Code::KeyH),
                        'i' => key = Some(Code::KeyI),
                        'j' => key = Some(Code::KeyJ),
                        'k' => key = Some(Code::KeyK),
                        'l' => key = Some(Code::KeyL),
                        'm' => key = Some(Code::KeyM),
                        'n' => key = Some(Code::KeyN),
                        'o' => key = Some(Code::KeyO),
                        'p' => key = Some(Code::KeyP),
                        'q' => key = Some(Code::KeyQ),
                        'r' => key = Some(Code::KeyR),
                        's' => key = Some(Code::KeyS),
                        't' => key = Some(Code::KeyT),
                        'u' => key = Some(Code::KeyU),
                        'v' => key = Some(Code::KeyV),
                        'w' => key = Some(Code::KeyW),
                        'x' => key = Some(Code::KeyX),
                        'y' => key = Some(Code::KeyY),
                        'z' => key = Some(Code::KeyZ),
                        '0' => key = Some(Code::Digit0),
                        '1' => key = Some(Code::Digit1),
                        '2' => key = Some(Code::Digit2),
                        '3' => key = Some(Code::Digit3),
                        '4' => key = Some(Code::Digit4),
                        '5' => key = Some(Code::Digit5),
                        '6' => key = Some(Code::Digit6),
                        '7' => key = Some(Code::Digit7),
                        '8' => key = Some(Code::Digit8),
                        '9' => key = Some(Code::Digit9),
                        _ => {}
                    }
                }
            }
        }
    }

    let key = key.ok_or_else(|| anyhow::anyhow!("Invalid hotkey: {s}"))?;
    Ok(HotKey::new(Some(modifiers), key))
}
