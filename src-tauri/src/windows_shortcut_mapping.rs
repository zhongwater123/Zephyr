use crate::physical_shortcut::PhysicalKeyId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub(crate) fn physical_key_from_legacy_name(name: &str) -> Result<PhysicalKeyId, String> {
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
