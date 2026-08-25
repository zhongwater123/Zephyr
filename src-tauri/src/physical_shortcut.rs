use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub const DEFAULT_SHORTCUT_LABEL: &str = "左 Ctrl+左 Shift+Space";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalKeyId { pub scan_code: u16, pub extended: bool }

impl PhysicalKeyId {
    pub const fn new(scan_code: u16, extended: bool) -> Self { Self { scan_code, extended } }
    pub(crate) const fn packed(self) -> u32 { self.scan_code as u32 | ((self.extended as u32) << 16) }
    pub(crate) const fn from_packed(value: u32) -> Self { Self::new((value & 0xffff) as u16, value & (1 << 16) != 0) }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModifierKind { Control, Alt, Shift, Win }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModifierSide { Any, Left, Right }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModifierBinding { pub kind: ModifierKind, pub side: ModifierSide }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutBinding { pub modifiers: Vec<ModifierBinding>, pub trigger: PhysicalKeyId }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompiledBinding { pub trigger: PhysicalKeyId, pub sided_modifiers: u8, pub any_modifiers: u8 }

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
                ModifierBinding { kind: ModifierKind::Control, side: ModifierSide::Left },
                ModifierBinding { kind: ModifierKind::Shift, side: ModifierSide::Left },
            ],
            trigger: PhysicalKeyId::new(0x39, false),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.trigger.scan_code == 0 { return Err("快捷键需要包含一个主键。".into()); }
        if self.modifiers.is_empty() { return Err("快捷键至少需要一个 Ctrl、Alt 或 Shift 修饰键。".into()); }
        let compiled = self.compile()?;
        if compiled.any_modifiers & ANY_WIN != 0 || compiled.sided_modifiers & (LEFT_WIN | RIGHT_WIN) != 0 {
            return Err("Windows 键组合由操作系统保留，不能作为语音快捷键。".into());
        }
        Ok(())
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
            match modifier.side { ModifierSide::Left => sided |= left, ModifierSide::Right => sided |= right, ModifierSide::Any => any |= any_bit }
        }
        for (mask, any_bit) in modifier_groups() {
            if any & any_bit != 0 && sided & mask != 0 { return Err("同一种修饰键不能同时配置任意侧和固定侧。".into()); }
            if (sided & mask).count_ones() > 1 { return Err("同一种修饰键只能选择一侧。".into()); }
        }
        Ok(CompiledBinding { trigger: self.trigger, sided_modifiers: sided, any_modifiers: any })
    }

    pub fn from_legacy_label(input: &str) -> Result<Self, String> {
        let mut modifiers = Vec::new();
        let mut trigger = None;
        for token in input.split('+').map(str::trim).filter(|value| !value.is_empty()) {
            match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.push(ModifierBinding { kind: ModifierKind::Control, side: ModifierSide::Any }),
                "alt" | "option" => modifiers.push(ModifierBinding { kind: ModifierKind::Alt, side: ModifierSide::Any }),
                "shift" => modifiers.push(ModifierBinding { kind: ModifierKind::Shift, side: ModifierSide::Any }),
                "win" | "meta" | "super" | "cmd" | "command" => return Err("Windows 键组合由操作系统保留，不能作为语音快捷键。".into()),
                value => {
                    if trigger.is_some() { return Err("快捷键只能包含一个主键。".into()); }
                    trigger = Some(physical_key_from_legacy_name(value)?);
                }
            }
        }
        let binding = Self { modifiers, trigger: trigger.ok_or_else(|| "快捷键需要包含一个主键。".to_string())? };
        binding.validate()?;
        Ok(binding)
    }

    pub fn label_with_trigger(&self, trigger_label: &str) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().copied().map(modifier_label).collect();
        parts.push(trigger_label.to_string());
        parts.join("+")
    }
}

impl CompiledBinding {
    pub(crate) fn matches_modifiers(self, held: u8) -> bool {
        modifier_groups().into_iter().all(|(mask, any_bit)| {
            let actual = held & mask;
            if self.any_modifiers & any_bit != 0 { actual.count_ones() == 1 } else { actual == self.sided_modifiers & mask }
        })
    }

    pub(crate) fn required_modifiers_still_held(self, held: u8) -> bool {
        held & self.sided_modifiers == self.sided_modifiers
            && modifier_groups().into_iter().all(|(mask, any_bit)| self.any_modifiers & any_bit == 0 || held & mask != 0)
    }
}

const fn modifier_groups() -> [(u8, u8); 4] {
    [(LEFT_CTRL | RIGHT_CTRL, ANY_CTRL), (LEFT_ALT | RIGHT_ALT, ANY_ALT), (LEFT_SHIFT | RIGHT_SHIFT, ANY_SHIFT), (LEFT_WIN | RIGHT_WIN, ANY_WIN)]
}

pub(crate) fn modifier_bit(key: PhysicalKeyId) -> Option<u8> {
    match (key.scan_code, key.extended) {
        (0x1d, false) => Some(LEFT_CTRL), (0x1d, true) => Some(RIGHT_CTRL),
        (0x38, false) => Some(LEFT_ALT), (0x38, true) => Some(RIGHT_ALT),
        (0x2a, false) => Some(LEFT_SHIFT), (0x36, false) => Some(RIGHT_SHIFT),
        (0x5b, true) => Some(LEFT_WIN), (0x5c, true) => Some(RIGHT_WIN), _ => None,
    }
}

pub(crate) fn modifiers_from_bits(bits: u8) -> Vec<ModifierBinding> {
    [
        (LEFT_CTRL, ModifierKind::Control, ModifierSide::Left), (RIGHT_CTRL, ModifierKind::Control, ModifierSide::Right),
        (LEFT_ALT, ModifierKind::Alt, ModifierSide::Left), (RIGHT_ALT, ModifierKind::Alt, ModifierSide::Right),
        (LEFT_SHIFT, ModifierKind::Shift, ModifierSide::Left), (RIGHT_SHIFT, ModifierKind::Shift, ModifierSide::Right),
        (LEFT_WIN, ModifierKind::Win, ModifierSide::Left), (RIGHT_WIN, ModifierKind::Win, ModifierSide::Right),
    ].into_iter().filter_map(|(bit, kind, side)| (bits & bit != 0).then_some(ModifierBinding { kind, side })).collect()
}

fn modifier_label(modifier: ModifierBinding) -> String {
    let kind = match modifier.kind { ModifierKind::Control => "Ctrl", ModifierKind::Alt => "Alt", ModifierKind::Shift => "Shift", ModifierKind::Win => "Win" };
    match modifier.side { ModifierSide::Any => kind.into(), ModifierSide::Left => format!("左 {kind}"), ModifierSide::Right => format!("右 {kind}") }
}

fn physical_key_from_legacy_name(name: &str) -> Result<PhysicalKeyId, String> {
    let vk = legacy_name_to_vk(name).ok_or_else(|| format!("无法迁移旧快捷键主键：{name}，请重新录制。"))?;
    if vk == VK_F12.0 as u32 { return Err("F12 由 Windows 调试器保留，不能作为语音快捷键。".into()); }
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
    if scan == 0 { return Err(format!("无法把旧快捷键 {name} 映射为物理键，请重新录制。")); }
    Ok(PhysicalKeyId::new((scan & 0xff) as u16, scan & 0xff00 == 0xe000))
}

fn legacy_name_to_vk(name: &str) -> Option<u32> {
    let upper = name.to_ascii_uppercase();
    if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() { return Some(upper.as_bytes()[0] as u32); }
    Some(match upper.as_str() {
        "SPACE" => VK_SPACE.0, "TAB" => VK_TAB.0, "ENTER" | "RETURN" => VK_RETURN.0,
        "ESC" | "ESCAPE" => VK_ESCAPE.0, "BACKSPACE" => VK_BACK.0, "DELETE" | "DEL" => VK_DELETE.0,
        "INSERT" | "INS" => VK_INSERT.0, "HOME" => VK_HOME.0, "END" => VK_END.0,
        "PAGEUP" => VK_PRIOR.0, "PAGEDOWN" => VK_NEXT.0, "ARROWUP" | "UP" => VK_UP.0,
        "ARROWDOWN" | "DOWN" => VK_DOWN.0, "ARROWLEFT" | "LEFT" => VK_LEFT.0, "ARROWRIGHT" | "RIGHT" => VK_RIGHT.0,
        value if value.starts_with('F') => { let n = value[1..].parse::<u32>().ok()?; if !(1..=24).contains(&n) { return None; } VK_F1.0 as u32 + n - 1 },
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
        let legacy = ShortcutBinding::from_legacy_label("Ctrl+Alt+Space").unwrap().compile().unwrap();
        assert!(legacy.matches_modifiers(LEFT_CTRL | RIGHT_ALT));
        assert!(legacy.matches_modifiers(RIGHT_CTRL | LEFT_ALT));
        assert!(!legacy.matches_modifiers(LEFT_CTRL | RIGHT_CTRL | LEFT_ALT));
        let exact = ShortcutBinding::default_physical().compile().unwrap();
        assert!(exact.matches_modifiers(LEFT_CTRL | LEFT_SHIFT));
        assert!(!exact.matches_modifiers(RIGHT_CTRL | LEFT_SHIFT));
    }
}
