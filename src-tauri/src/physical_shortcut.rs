use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const DEFAULT_SHORTCUT_LABEL: &str = "左 Ctrl+左 Shift+Space";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalKeyId {
    pub scan_code: u16,
    pub extended: bool,
}

impl PhysicalKeyId {
    pub const fn new(scan_code: u16, extended: bool) -> Self {
        Self {
            scan_code,
            extended,
        }
    }
    pub(crate) const fn packed(self) -> u32 {
        self.scan_code as u32 | ((self.extended as u32) << 16)
    }
    pub(crate) const fn from_packed(value: u32) -> Self {
        Self::new((value & 0xffff) as u16, value & (1 << 16) != 0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKind {
    Control,
    Alt,
    Shift,
    Win,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModifierSide {
    Any,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModifierBinding {
    pub kind: ModifierKind,
    pub side: ModifierSide,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding {
    pub modifiers: Vec<ModifierBinding>,
    pub trigger: PhysicalKeyId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompiledBinding {
    pub trigger: PhysicalKeyId,
    pub sided_modifiers: u8,
    pub any_modifiers: u8,
    pub trigger_modifier: u8,
}

pub(crate) const LEFT_CTRL: u8 = 1 << 0;
pub(crate) const RIGHT_CTRL: u8 = 1 << 1;
pub(crate) const LEFT_ALT: u8 = 1 << 2;
pub(crate) const RIGHT_ALT: u8 = 1 << 3;
pub(crate) const LEFT_SHIFT: u8 = 1 << 4;
pub(crate) const RIGHT_SHIFT: u8 = 1 << 5;
pub(crate) const LEFT_WIN: u8 = 1 << 6;
pub(crate) const RIGHT_WIN: u8 = 1 << 7;
const ANY_CTRL: u8 = 1 << 0;
const ANY_ALT: u8 = 1 << 1;
const ANY_SHIFT: u8 = 1 << 2;
const ANY_WIN: u8 = 1 << 3;

impl ShortcutBinding {
    pub fn default_physical() -> Self {
        Self {
            modifiers: vec![
                ModifierBinding {
                    kind: ModifierKind::Control,
                    side: ModifierSide::Left,
                },
                ModifierBinding {
                    kind: ModifierKind::Shift,
                    side: ModifierSide::Left,
                },
            ],
            trigger: PhysicalKeyId::new(0x39, false),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.trigger.scan_code == 0 {
            return Err("快捷键需要包含一个主键。".into());
        }
        if self.modifiers.len().saturating_add(1) > 3 {
            return Err("快捷键最多支持三个物理按键。".into());
        }
        for (index, modifier) in self.modifiers.iter().enumerate() {
            if self.modifiers[..index].contains(modifier) {
                return Err("快捷键包含重复的修饰键。".into());
            }
        }
        let compiled = self.compile()?;
        let modifier_trigger = compiled.trigger_modifier;
        if modifier_trigger == 0 && physical_key_label(self.trigger).is_none() {
            return Err("该物理按键暂不支持作为快捷键。".into());
        }
        if modifier_trigger == 0
            && self.modifiers.is_empty()
            && !standalone_key_allowed(self.trigger)
        {
            return Err("字母、数字、标点和 Enter 需要与修饰键组合使用。".into());
        }
        if modifier_trigger != 0
            && self.modifiers.is_empty()
            && modifier_trigger & (RIGHT_CTRL | RIGHT_ALT | RIGHT_SHIFT) == 0
        {
            return Err("单独使用修饰键时，仅支持右 Ctrl、右 Alt 或右 Shift。".into());
        }
        Ok(())
    }

    pub fn display_label(&self) -> String {
        self.label_with_trigger(physical_key_label(self.trigger).unwrap_or("未知按键"))
    }
    pub(crate) fn compile(&self) -> Result<CompiledBinding, String> {
        let mut sided = 0u8;
        let mut any = 0u8;
        for modifier in &self.modifiers {
            let (left, right, any_bit) = match modifier.kind {
                ModifierKind::Control => (LEFT_CTRL, RIGHT_CTRL, ANY_CTRL),
                ModifierKind::Alt => (LEFT_ALT, RIGHT_ALT, ANY_ALT),
                ModifierKind::Shift => (LEFT_SHIFT, RIGHT_SHIFT, ANY_SHIFT),
                ModifierKind::Win => (LEFT_WIN, RIGHT_WIN, ANY_WIN),
            };
            match modifier.side {
                ModifierSide::Left => sided |= left,
                ModifierSide::Right => sided |= right,
                ModifierSide::Any => any |= any_bit,
            }
        }
        let trigger_modifier = modifier_bit(self.trigger).unwrap_or(0);
        for (mask, any_bit) in modifier_groups() {
            if any & any_bit != 0 && sided & mask != 0 {
                return Err("同一种修饰键不能同时配置任意侧和固定侧。".into());
            }
            if trigger_modifier & mask != 0 && any & any_bit != 0 {
                return Err("修饰键触发键不能同时配置为任意侧修饰键。".into());
            }
            if sided & trigger_modifier != 0 {
                return Err("修饰键触发键不能在组合中重复。".into());
            }
        }
        Ok(CompiledBinding {
            trigger: self.trigger,
            sided_modifiers: sided,
            any_modifiers: any,
            trigger_modifier,
        })
    }

    pub fn from_legacy_label(input: &str) -> Result<Self, String> {
        let mut modifiers = Vec::new();
        let mut trigger = None;
        for token in input
            .split('+')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.push(ModifierBinding {
                    kind: ModifierKind::Control,
                    side: ModifierSide::Any,
                }),
                "alt" | "option" => modifiers.push(ModifierBinding {
                    kind: ModifierKind::Alt,
                    side: ModifierSide::Any,
                }),
                "shift" => modifiers.push(ModifierBinding {
                    kind: ModifierKind::Shift,
                    side: ModifierSide::Any,
                }),
                "win" | "meta" | "super" | "cmd" | "command" => modifiers.push(ModifierBinding {
                    kind: ModifierKind::Win,
                    side: ModifierSide::Any,
                }),
                value => {
                    if trigger.is_some() {
                        return Err("快捷键只能包含一个主键。".into());
                    }
                    trigger = Some(physical_key_from_legacy_name(value)?);
                }
            }
        }
        let binding = Self {
            modifiers,
            trigger: trigger.ok_or_else(|| "快捷键需要包含一个主键。".to_string())?,
        };
        binding.validate()?;
        Ok(binding)
    }

    pub fn label_with_trigger(&self, trigger_label: &str) -> String {
        if let Some(trigger_modifier) = modifier_binding_from_key(self.trigger) {
            let mut modifiers = self.modifiers.clone();
            if !modifiers.contains(&trigger_modifier) {
                modifiers.push(trigger_modifier);
            }
            modifiers.sort_by_key(|modifier| modifier_sort_key(*modifier));
            return modifiers
                .into_iter()
                .map(modifier_label)
                .collect::<Vec<_>>()
                .join("+");
        }
        let mut parts: Vec<String> = self.modifiers.iter().copied().map(modifier_label).collect();
        parts.push(trigger_label.to_string());
        parts.join("+")
    }

    pub(crate) fn physically_equivalent(&self, other: &Self) -> bool {
        let (Ok(left), Ok(right)) = (self.compile(), other.compile()) else {
            return false;
        };
        if left.is_modifier_only() || right.is_modifier_only() {
            return left.is_modifier_only()
                && right.is_modifier_only()
                && (left.sided_modifiers | left.trigger_modifier)
                    == (right.sided_modifiers | right.trigger_modifier)
                && left.any_modifiers == right.any_modifiers;
        }
        left == right
    }
}

impl CompiledBinding {
    pub(crate) fn matches_modifiers(self, held: u8) -> bool {
        let sided_modifiers = self.sided_modifiers | self.trigger_modifier;
        modifier_groups().into_iter().all(|(mask, any_bit)| {
            let actual = held & mask;
            if self.any_modifiers & any_bit != 0 {
                actual.count_ones() == 1
            } else {
                actual == sided_modifiers & mask
            }
        })
    }

    pub(crate) fn required_modifiers_still_held(self, held: u8) -> bool {
        let sided_modifiers = self.sided_modifiers | self.trigger_modifier;
        held & sided_modifiers == sided_modifiers
            && modifier_groups()
                .into_iter()
                .all(|(mask, any_bit)| self.any_modifiers & any_bit == 0 || held & mask != 0)
    }

    pub(crate) fn includes_modifier_bit(self, bit: u8) -> bool {
        modifier_groups()
            .into_iter()
            .find(|(mask, _)| mask & bit != 0)
            .is_some_and(|(_, any_bit)| {
                (self.sided_modifiers | self.trigger_modifier) & bit != 0
                    || self.any_modifiers & any_bit != 0
            })
    }

    pub(crate) fn is_modifier_only(self) -> bool {
        self.trigger_modifier != 0
    }
}

const fn modifier_groups() -> [(u8, u8); 4] {
    [
        (LEFT_CTRL | RIGHT_CTRL, ANY_CTRL),
        (LEFT_ALT | RIGHT_ALT, ANY_ALT),
        (LEFT_SHIFT | RIGHT_SHIFT, ANY_SHIFT),
        (LEFT_WIN | RIGHT_WIN, ANY_WIN),
    ]
}

pub(crate) fn modifier_bit(key: PhysicalKeyId) -> Option<u8> {
    match (key.scan_code, key.extended) {
        (0x1d, false) => Some(LEFT_CTRL),
        (0x1d, true) => Some(RIGHT_CTRL),
        (0x38, false) => Some(LEFT_ALT),
        (0x38, true) => Some(RIGHT_ALT),
        (0x2a, false) => Some(LEFT_SHIFT),
        (0x36, false) => Some(RIGHT_SHIFT),
        (0x5b, true) => Some(LEFT_WIN),
        (0x5c, true) => Some(RIGHT_WIN),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) fn modifier_only_binding(bits: u8) -> Option<ShortcutBinding> {
    let trigger_bit = [
        RIGHT_SHIFT,
        LEFT_SHIFT,
        RIGHT_ALT,
        LEFT_ALT,
        RIGHT_CTRL,
        LEFT_CTRL,
        RIGHT_WIN,
        LEFT_WIN,
    ]
    .into_iter()
    .find(|bit| bits & bit != 0)?;
    Some(ShortcutBinding {
        modifiers: modifiers_from_bits(bits & !trigger_bit),
        trigger: modifier_key_from_bit(trigger_bit)?,
    })
}

pub(crate) fn modifiers_from_bits(bits: u8) -> Vec<ModifierBinding> {
    [
        (LEFT_CTRL, ModifierKind::Control, ModifierSide::Left),
        (RIGHT_CTRL, ModifierKind::Control, ModifierSide::Right),
        (LEFT_ALT, ModifierKind::Alt, ModifierSide::Left),
        (RIGHT_ALT, ModifierKind::Alt, ModifierSide::Right),
        (LEFT_SHIFT, ModifierKind::Shift, ModifierSide::Left),
        (RIGHT_SHIFT, ModifierKind::Shift, ModifierSide::Right),
        (LEFT_WIN, ModifierKind::Win, ModifierSide::Left),
        (RIGHT_WIN, ModifierKind::Win, ModifierSide::Right),
    ]
    .into_iter()
    .filter_map(|(bit, kind, side)| (bits & bit != 0).then_some(ModifierBinding { kind, side }))
    .collect()
}

pub(crate) fn modifier_label(modifier: ModifierBinding) -> String {
    let kind = match modifier.kind {
        ModifierKind::Control => "Ctrl",
        ModifierKind::Alt => "Alt",
        ModifierKind::Shift => "Shift",
        ModifierKind::Win => "Win",
    };
    match modifier.side {
        ModifierSide::Any => kind.into(),
        ModifierSide::Left => format!("左 {kind}"),
        ModifierSide::Right => format!("右 {kind}"),
    }
}

pub(crate) fn physical_key_label(key: PhysicalKeyId) -> Option<&'static str> {
    Some(match (key.scan_code, key.extended) {
        (0x01, false) => "Escape",
        (0x02, false) => "1",
        (0x03, false) => "2",
        (0x04, false) => "3",
        (0x05, false) => "4",
        (0x06, false) => "5",
        (0x07, false) => "6",
        (0x08, false) => "7",
        (0x09, false) => "8",
        (0x0a, false) => "9",
        (0x0b, false) => "0",
        (0x0c, false) => "Minus",
        (0x0d, false) => "Equal",
        (0x0e, false) => "Backspace",
        (0x0f, false) => "Tab",
        (0x10, false) => "Q",
        (0x11, false) => "W",
        (0x12, false) => "E",
        (0x13, false) => "R",
        (0x14, false) => "T",
        (0x15, false) => "Y",
        (0x16, false) => "U",
        (0x17, false) => "I",
        (0x18, false) => "O",
        (0x19, false) => "P",
        (0x1a, false) => "BracketLeft",
        (0x1b, false) => "BracketRight",
        (0x1c, false) => "Enter",
        (0x1c, true) => "NumpadEnter",
        (0x1e, false) => "A",
        (0x1f, false) => "S",
        (0x20, false) => "D",
        (0x21, false) => "F",
        (0x22, false) => "G",
        (0x23, false) => "H",
        (0x24, false) => "J",
        (0x25, false) => "K",
        (0x26, false) => "L",
        (0x27, false) => "Semicolon",
        (0x28, false) => "Quote",
        (0x29, false) => "Backquote",
        (0x2b, false) => "Backslash",
        (0x2c, false) => "Z",
        (0x2d, false) => "X",
        (0x2e, false) => "C",
        (0x2f, false) => "V",
        (0x30, false) => "B",
        (0x31, false) => "N",
        (0x32, false) => "M",
        (0x33, false) => "Comma",
        (0x34, false) => "Period",
        (0x35, false) => "Slash",
        (0x35, true) => "NumpadDivide",
        (0x37, false) => "NumpadMultiply",
        (0x37, true) => "PrintScreen",
        (0x39, false) => "Space",
        (0x3b, false) => "F1",
        (0x3c, false) => "F2",
        (0x3d, false) => "F3",
        (0x3e, false) => "F4",
        (0x3f, false) => "F5",
        (0x40, false) => "F6",
        (0x41, false) => "F7",
        (0x42, false) => "F8",
        (0x43, false) => "F9",
        (0x44, false) => "F10",
        (0x45, false) => "Pause",
        (0x45, true) => "NumLock",
        (0x46, false) => "ScrollLock",
        (0x47, false) => "Numpad7",
        (0x47, true) => "Home",
        (0x48, false) => "Numpad8",
        (0x48, true) => "ArrowUp",
        (0x49, false) => "Numpad9",
        (0x49, true) => "PageUp",
        (0x4a, false) => "NumpadSubtract",
        (0x4b, false) => "Numpad4",
        (0x4b, true) => "ArrowLeft",
        (0x4c, false) => "Numpad5",
        (0x4d, false) => "Numpad6",
        (0x4d, true) => "ArrowRight",
        (0x4e, false) => "NumpadAdd",
        (0x4f, false) => "Numpad1",
        (0x4f, true) => "End",
        (0x50, false) => "Numpad2",
        (0x50, true) => "ArrowDown",
        (0x51, false) => "Numpad3",
        (0x51, true) => "PageDown",
        (0x52, false) => "Numpad0",
        (0x52, true) => "Insert",
        (0x53, false) => "NumpadDecimal",
        (0x53, true) => "Delete",
        (0x57, false) => "F11",
        (0x58, false) => "F12",
        (0x64, false) => "F13",
        (0x65, false) => "F14",
        (0x66, false) => "F15",
        (0x67, false) => "F16",
        (0x68, false) => "F17",
        (0x69, false) => "F18",
        (0x6a, false) => "F19",
        (0x6b, false) => "F20",
        (0x6c, false) => "F21",
        (0x6d, false) => "F22",
        (0x6e, false) => "F23",
        (0x76, false) => "F24",
        _ => return None,
    })
}

pub(crate) fn standalone_key_allowed(key: PhysicalKeyId) -> bool {
    matches!(
        physical_key_label(key),
        Some(
            "Space"
                | "Tab"
                | "Backspace"
                | "Insert"
                | "Delete"
                | "Home"
                | "End"
                | "PageUp"
                | "PageDown"
                | "ArrowUp"
                | "ArrowDown"
                | "ArrowLeft"
                | "ArrowRight"
                | "PrintScreen"
                | "Pause"
                | "ScrollLock"
                | "NumLock"
                | "F1"
                | "F2"
                | "F3"
                | "F4"
                | "F5"
                | "F6"
                | "F7"
                | "F8"
                | "F9"
                | "F10"
                | "F11"
                | "F13"
                | "F14"
                | "F15"
                | "F16"
                | "F17"
                | "F18"
                | "F19"
                | "F20"
                | "F21"
                | "F22"
                | "F23"
                | "F24"
        )
    )
}
fn modifier_binding_from_key(key: PhysicalKeyId) -> Option<ModifierBinding> {
    modifier_bit(key).and_then(|bit| modifiers_from_bits(bit).into_iter().next())
}

#[cfg(test)]
fn modifier_key_from_bit(bit: u8) -> Option<PhysicalKeyId> {
    Some(match bit {
        LEFT_CTRL => PhysicalKeyId::new(0x1d, false),
        RIGHT_CTRL => PhysicalKeyId::new(0x1d, true),
        LEFT_ALT => PhysicalKeyId::new(0x38, false),
        RIGHT_ALT => PhysicalKeyId::new(0x38, true),
        LEFT_SHIFT => PhysicalKeyId::new(0x2a, false),
        RIGHT_SHIFT => PhysicalKeyId::new(0x36, false),
        LEFT_WIN => PhysicalKeyId::new(0x5b, true),
        RIGHT_WIN => PhysicalKeyId::new(0x5c, true),
        _ => return None,
    })
}

fn modifier_sort_key(modifier: ModifierBinding) -> (u8, u8) {
    let kind = match modifier.kind {
        ModifierKind::Control => 0,
        ModifierKind::Alt => 1,
        ModifierKind::Shift => 2,
        ModifierKind::Win => 3,
    };
    let side = match modifier.side {
        ModifierSide::Any => 0,
        ModifierSide::Left => 1,
        ModifierSide::Right => 2,
    };
    (kind, side)
}

fn physical_key_from_legacy_name(name: &str) -> Result<PhysicalKeyId, String> {
    let vk = legacy_name_to_vk(name)
        .ok_or_else(|| format!("无法迁移旧快捷键主键：{name}，请重新录制。"))?;
    if vk == VK_F12.0 as u32 {
        return Err("F12 由 Windows 调试器保留，不能作为语音快捷键。".into());
    }
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
    if scan == 0 {
        return Err(format!("无法把旧快捷键 {name} 映射为物理键，请重新录制。"));
    }
    Ok(PhysicalKeyId::new(
        (scan & 0xff) as u16,
        scan & 0xff00 == 0xe000,
    ))
}

fn legacy_name_to_vk(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() {
        return Some(upper.as_bytes()[0] as u32);
    }
    Some(match upper.as_str() {
        "SPACE" => VK_SPACE.0,
        "TAB" => VK_TAB.0,
        "ENTER" | "RETURN" => VK_RETURN.0,
        "ESC" | "ESCAPE" => VK_ESCAPE.0,
        "BACKSPACE" => VK_BACK.0,
        "DELETE" | "DEL" => VK_DELETE.0,
        "INSERT" | "INS" => VK_INSERT.0,
        "HOME" => VK_HOME.0,
        "END" => VK_END.0,
        "PAGEUP" => VK_PRIOR.0,
        "PAGEDOWN" => VK_NEXT.0,
        "ARROWUP" | "UP" => VK_UP.0,
        "ARROWDOWN" | "DOWN" => VK_DOWN.0,
        "ARROWLEFT" | "LEFT" => VK_LEFT.0,
        "ARROWRIGHT" | "RIGHT" => VK_RIGHT.0,
        value if value.starts_with('F') => {
            let n = value[1..].parse::<u16>().ok()?;
            if !(1..=24).contains(&n) {
                return None;
            }
            VK_F1.0 + n - 1
        }
        _ => return None,
    } as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_exact_left_physical_binding() {
        let binding = ShortcutBinding::default_physical();
        assert_eq!(binding.trigger, PhysicalKeyId::new(0x39, false));
        assert_eq!(binding.label_with_trigger("Space"), DEFAULT_SHORTCUT_LABEL);
    }

    #[test]
    fn legacy_any_side_and_exact_side_are_distinct() {
        let legacy = ShortcutBinding::from_legacy_label("Ctrl+Alt+Space")
            .unwrap()
            .compile()
            .unwrap();
        assert!(legacy.matches_modifiers(LEFT_CTRL | RIGHT_ALT));
        assert!(legacy.matches_modifiers(RIGHT_CTRL | LEFT_ALT));
        assert!(!legacy.matches_modifiers(LEFT_CTRL | RIGHT_CTRL | LEFT_ALT));
        let exact = ShortcutBinding::default_physical().compile().unwrap();
        assert!(exact.matches_modifiers(LEFT_CTRL | LEFT_SHIFT));
        assert!(!exact.matches_modifiers(RIGHT_CTRL | LEFT_SHIFT));
    }

    #[test]
    fn legacy_win_modifier_is_valid_and_matches_either_physical_side() {
        let binding = ShortcutBinding::from_legacy_label("Ctrl+Win+K").unwrap();
        assert!(binding.validate().is_ok());
        let compiled = binding.compile().unwrap();
        assert!(compiled.matches_modifiers(LEFT_CTRL | LEFT_WIN));
        assert!(compiled.matches_modifiers(RIGHT_CTRL | RIGHT_WIN));
        assert!(!compiled.matches_modifiers(LEFT_CTRL));
    }

    #[test]
    fn right_modifier_keys_are_valid_single_key_bindings() {
        for trigger in [
            PhysicalKeyId::new(0x1d, true),
            PhysicalKeyId::new(0x38, true),
            PhysicalKeyId::new(0x36, false),
        ] {
            let binding = ShortcutBinding {
                modifiers: Vec::new(),
                trigger,
            };
            assert!(binding.validate().is_ok());
            assert!(binding.compile().unwrap().is_modifier_only());
        }
    }

    #[test]
    fn left_modifier_alone_is_rejected_but_modifier_chord_is_valid() {
        let left_control = ShortcutBinding {
            modifiers: Vec::new(),
            trigger: PhysicalKeyId::new(0x1d, false),
        };
        assert!(left_control.validate().is_err());

        for trigger in [
            PhysicalKeyId::new(0x5b, true),
            PhysicalKeyId::new(0x5c, true),
        ] {
            let binding = ShortcutBinding {
                modifiers: Vec::new(),
                trigger,
            };
            assert!(binding.validate().is_err());
        }

        let chord = modifier_only_binding(LEFT_CTRL | RIGHT_SHIFT).unwrap();
        assert!(chord.validate().is_ok());
        assert_eq!(chord.label_with_trigger(""), "左 Ctrl+右 Shift");
        let compiled = chord.compile().unwrap();
        assert!(compiled.matches_modifiers(LEFT_CTRL | RIGHT_SHIFT));
        assert!(!compiled.matches_modifiers(LEFT_CTRL));
        assert!(!compiled.matches_modifiers(LEFT_CTRL | RIGHT_SHIFT | RIGHT_ALT));
    }

    #[test]
    fn physical_equivalence_ignores_modifier_vector_and_chord_trigger_representation() {
        let ordered = ShortcutBinding {
            modifiers: vec![
                ModifierBinding {
                    kind: ModifierKind::Control,
                    side: ModifierSide::Left,
                },
                ModifierBinding {
                    kind: ModifierKind::Shift,
                    side: ModifierSide::Right,
                },
            ],
            trigger: PhysicalKeyId::new(0x39, false),
        };
        let reversed = ShortcutBinding {
            modifiers: ordered.modifiers.iter().copied().rev().collect(),
            trigger: ordered.trigger,
        };
        assert_ne!(ordered, reversed);
        assert!(ordered.physically_equivalent(&reversed));

        let canonical = modifier_only_binding(RIGHT_CTRL | RIGHT_SHIFT).unwrap();
        let alternate = ShortcutBinding {
            modifiers: vec![ModifierBinding {
                kind: ModifierKind::Shift,
                side: ModifierSide::Right,
            }],
            trigger: PhysicalKeyId::new(0x1d, true),
        };
        assert_ne!(canonical, alternate);
        assert!(canonical.physically_equivalent(&alternate));
    }
}
