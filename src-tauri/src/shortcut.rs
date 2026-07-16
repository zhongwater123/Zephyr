use serde::Serialize;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Space";

#[derive(Debug, Clone, Serialize)]
pub struct ShortcutValidation {
    pub shortcut: String,
    pub normalized: String,
    pub valid: bool,
    pub available: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParsedShortcut {
    pub shortcut: Shortcut,
    pub normalized: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutParseError {
    Empty,
    MissingMainKey,
    MissingModifier,
    UnsupportedKey(String),
    DuplicateMainKey,
    Reserved(String),
}

impl ShortcutParseError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => "请按下一个组合快捷键。".to_string(),
            Self::MissingMainKey => "快捷键需要包含一个主键。".to_string(),
            Self::MissingModifier => "快捷键至少需要两个按键：一个修饰键加一个主键。".to_string(),
            Self::UnsupportedKey(key) => format!("暂不支持按键：{key}。"),
            Self::DuplicateMainKey => "快捷键只能包含一个主键。".to_string(),
            Self::Reserved(message) => message.clone(),
        }
    }
}

pub fn parse_shortcut(input: &str) -> Result<ParsedShortcut, ShortcutParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ShortcutParseError::Empty);
    }

    let mut mods = Modifiers::empty();
    let mut key: Option<(Code, String)> = None;
    for raw in input.split('+') {
        let token = raw.trim();
        if token.is_empty() {
            return Err(ShortcutParseError::Empty);
        }

        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" | "option" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "win" | "meta" | "super" | "cmd" | "command" => mods |= Modifiers::SUPER,
            _ => {
                if key.is_some() {
                    return Err(ShortcutParseError::DuplicateMainKey);
                }
                key = Some(parse_key(token)?);
            }
        }
    }

    if mods.is_empty() {
        return Err(ShortcutParseError::MissingModifier);
    }
    let Some((key, key_label)) = key else {
        return Err(ShortcutParseError::MissingMainKey);
    };

    let normalized = normalize_parts(mods, &key_label);
    reject_reserved(mods, key, &normalized)?;

    Ok(ParsedShortcut {
        shortcut: Shortcut::new(Some(mods), key),
        normalized,
    })
}

pub fn default_shortcut() -> ParsedShortcut {
    parse_shortcut(DEFAULT_SHORTCUT).expect("default shortcut must be valid")
}

pub fn validation_error(input: &str, error: ShortcutParseError) -> ShortcutValidation {
    ShortcutValidation {
        shortcut: input.to_string(),
        normalized: String::new(),
        valid: false,
        available: false,
        reason: Some(error.message()),
    }
}

pub fn validation_success(input: &str, normalized: String, available: bool, reason: Option<String>) -> ShortcutValidation {
    ShortcutValidation {
        shortcut: input.to_string(),
        normalized,
        valid: true,
        available,
        reason,
    }
}

fn normalize_parts(mods: Modifiers, key_label: &str) -> String {
    let mut parts = Vec::new();
    if mods.contains(Modifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if mods.contains(Modifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if mods.contains(Modifiers::SHIFT) {
        parts.push("Shift".to_string());
    }
    if mods.contains(Modifiers::SUPER) {
        parts.push("Win".to_string());
    }
    parts.push(key_label.to_string());
    parts.join("+")
}

fn reject_reserved(mods: Modifiers, key: Code, normalized: &str) -> Result<(), ShortcutParseError> {
    let ctrl = mods.contains(Modifiers::CONTROL);
    let alt = mods.contains(Modifiers::ALT);
    let shift = mods.contains(Modifiers::SHIFT);
    let win = mods.contains(Modifiers::SUPER);

    if alt && !ctrl && !shift && !win && key == Code::Tab {
        return Err(ShortcutParseError::Reserved(
            "Alt+Tab 是系统切换窗口快捷键，不能作为语音输入快捷键。".to_string(),
        ));
    }
    if ctrl && alt && key == Code::Delete {
        return Err(ShortcutParseError::Reserved(
            "Ctrl+Alt+Delete 是系统安全快捷键，不能覆盖。".to_string(),
        ));
    }
    if win && !ctrl && !alt && !shift && matches!(key, Code::KeyL | Code::KeyD | Code::Tab) {
        return Err(ShortcutParseError::Reserved(format!(
            "{normalized} 是 Windows 系统保留快捷键。"
        )));
    }
    Ok(())
}

fn parse_key(token: &str) -> Result<(Code, String), ShortcutParseError> {
    let upper = token.to_ascii_uppercase();
    let key = match upper.as_str() {
        "SPACE" | " " => (Code::Space, "Space".to_string()),
        "TAB" => (Code::Tab, "Tab".to_string()),
        "ENTER" | "RETURN" => (Code::Enter, "Enter".to_string()),
        "ESC" | "ESCAPE" => (Code::Escape, "Escape".to_string()),
        "BACKSPACE" => (Code::Backspace, "Backspace".to_string()),
        "DELETE" | "DEL" => (Code::Delete, "Delete".to_string()),
        "INSERT" | "INS" => (Code::Insert, "Insert".to_string()),
        "HOME" => (Code::Home, "Home".to_string()),
        "END" => (Code::End, "End".to_string()),
        "PAGEUP" => (Code::PageUp, "PageUp".to_string()),
        "PAGEDOWN" => (Code::PageDown, "PageDown".to_string()),
        "ARROWUP" | "UP" => (Code::ArrowUp, "ArrowUp".to_string()),
        "ARROWDOWN" | "DOWN" => (Code::ArrowDown, "ArrowDown".to_string()),
        "ARROWLEFT" | "LEFT" => (Code::ArrowLeft, "ArrowLeft".to_string()),
        "ARROWRIGHT" | "RIGHT" => (Code::ArrowRight, "ArrowRight".to_string()),
        value if value.len() == 1 && value.as_bytes()[0].is_ascii_alphabetic() => {
            let ch = value.chars().next().unwrap();
            (letter_code(ch)?, ch.to_string())
        }
        value if value.len() == 1 && value.as_bytes()[0].is_ascii_digit() => {
            let ch = value.chars().next().unwrap();
            (digit_code(ch)?, ch.to_string())
        }
        value if value.starts_with('F') => parse_function_key(value)?,
        _ => return Err(ShortcutParseError::UnsupportedKey(token.to_string())),
    };
    Ok(key)
}

fn letter_code(ch: char) -> Result<Code, ShortcutParseError> {
    Ok(match ch {
        'A' => Code::KeyA,
        'B' => Code::KeyB,
        'C' => Code::KeyC,
        'D' => Code::KeyD,
        'E' => Code::KeyE,
        'F' => Code::KeyF,
        'G' => Code::KeyG,
        'H' => Code::KeyH,
        'I' => Code::KeyI,
        'J' => Code::KeyJ,
        'K' => Code::KeyK,
        'L' => Code::KeyL,
        'M' => Code::KeyM,
        'N' => Code::KeyN,
        'O' => Code::KeyO,
        'P' => Code::KeyP,
        'Q' => Code::KeyQ,
        'R' => Code::KeyR,
        'S' => Code::KeyS,
        'T' => Code::KeyT,
        'U' => Code::KeyU,
        'V' => Code::KeyV,
        'W' => Code::KeyW,
        'X' => Code::KeyX,
        'Y' => Code::KeyY,
        'Z' => Code::KeyZ,
        _ => return Err(ShortcutParseError::UnsupportedKey(ch.to_string())),
    })
}

fn digit_code(ch: char) -> Result<Code, ShortcutParseError> {
    Ok(match ch {
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        _ => return Err(ShortcutParseError::UnsupportedKey(ch.to_string())),
    })
}

fn parse_function_key(value: &str) -> Result<(Code, String), ShortcutParseError> {
    let number = value
        .strip_prefix('F')
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| ShortcutParseError::UnsupportedKey(value.to_string()))?;
    let code = match number {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        6 => Code::F6,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        13 => Code::F13,
        14 => Code::F14,
        15 => Code::F15,
        16 => Code::F16,
        17 => Code::F17,
        18 => Code::F18,
        19 => Code::F19,
        20 => Code::F20,
        21 => Code::F21,
        22 => Code::F22,
        23 => Code::F23,
        24 => Code::F24,
        _ => return Err(ShortcutParseError::UnsupportedKey(value.to_string())),
    };
    Ok((code, format!("F{number}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_shortcuts() {
        assert_eq!(parse_shortcut("ctrl + alt + space").unwrap().normalized, "Ctrl+Alt+Space");
        assert_eq!(parse_shortcut("Ctrl+V").unwrap().normalized, "Ctrl+V");
        assert_eq!(parse_shortcut("Alt+Space").unwrap().normalized, "Alt+Space");
        assert_eq!(parse_shortcut("Ctrl+Shift+V").unwrap().normalized, "Ctrl+Shift+V");
        assert_eq!(parse_shortcut("Alt+Shift+Space").unwrap().normalized, "Alt+Shift+Space");
    }

    #[test]
    fn rejects_single_keys_and_modifier_only_shortcuts() {
        assert_eq!(parse_shortcut("A").unwrap_err(), ShortcutParseError::MissingModifier);
        assert_eq!(parse_shortcut("Ctrl+Alt").unwrap_err(), ShortcutParseError::MissingMainKey);
    }

    #[test]
    fn rejects_system_reserved_shortcuts() {
        assert!(matches!(parse_shortcut("Alt+Tab"), Err(ShortcutParseError::Reserved(_))));
        assert!(matches!(parse_shortcut("Ctrl+Alt+Delete"), Err(ShortcutParseError::Reserved(_))));
        assert!(matches!(parse_shortcut("Win+L"), Err(ShortcutParseError::Reserved(_))));
    }
}
