use crate::physical_shortcut::{ModifierKind, PhysicalKeyId, ShortcutBinding};

const INVALID_BINDING: &str = "invalid_binding";
const RESERVED_BINDING: &str = "reserved_binding";

#[derive(Debug)]
pub(super) struct EditFailure {
    pub(super) code: &'static str,
    pub(super) message: String,
}

pub(super) fn validate_trace_id(trace_id: &str) -> Result<(), String> {
    if trace_id.is_empty()
        || trace_id.len() > 64
        || !trace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("快捷键 traceId 无效。".into());
    }
    Ok(())
}

pub(super) fn validate_candidate(binding: &ShortcutBinding) -> Result<(), EditFailure> {
    let trigger = binding.trigger;
    if trigger == PhysicalKeyId::new(0x58, false) {
        return Err(EditFailure {
            code: RESERVED_BINDING,
            message: "F12 由 Windows 调试器保留，不能作为语音快捷键。".into(),
        });
    }
    binding.validate().map_err(|message| EditFailure {
        code: INVALID_BINDING,
        message,
    })?;
    let has = |kind| {
        binding
            .modifiers
            .iter()
            .any(|modifier| modifier.kind == kind)
    };
    let reserved = if trigger == PhysicalKeyId::new(0x53, true)
        && has(ModifierKind::Control)
        && has(ModifierKind::Alt)
    {
        Some("Ctrl+Alt+Delete 是系统安全快捷键。")
    } else if trigger == PhysicalKeyId::new(0x01, false)
        && has(ModifierKind::Control)
        && has(ModifierKind::Shift)
    {
        Some("Ctrl+Shift+Escape 是系统任务管理器快捷键。")
    } else if trigger == PhysicalKeyId::new(0x0f, false) && has(ModifierKind::Alt) {
        Some("Alt+Tab 是系统切换窗口快捷键。")
    } else if trigger == PhysicalKeyId::new(0x3e, false) && has(ModifierKind::Alt) {
        Some("Alt+F4 是系统关闭窗口快捷键。")
    } else if trigger == PhysicalKeyId::new(0x26, false) && has(ModifierKind::Win) {
        Some("Win+L 是系统锁屏快捷键。")
    } else {
        None
    };
    if let Some(message) = reserved {
        return Err(EditFailure {
            code: RESERVED_BINDING,
            message: message.into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical_shortcut::{ModifierBinding, ModifierSide};

    fn binding(modifier: Option<ModifierKind>, trigger: PhysicalKeyId) -> ShortcutBinding {
        ShortcutBinding {
            modifiers: modifier
                .map(|kind| {
                    vec![ModifierBinding {
                        kind,
                        side: ModifierSide::Left,
                    }]
                })
                .unwrap_or_default(),
            trigger,
        }
    }

    #[test]
    fn reserved_windows_combinations_are_rejected() {
        for candidate in [
            binding(Some(ModifierKind::Alt), PhysicalKeyId::new(0x0f, false)),
            binding(Some(ModifierKind::Alt), PhysicalKeyId::new(0x3e, false)),
            binding(Some(ModifierKind::Win), PhysicalKeyId::new(0x26, false)),
            binding(None, PhysicalKeyId::new(0x58, false)),
        ] {
            assert_eq!(
                validate_candidate(&candidate).unwrap_err().code,
                RESERVED_BINDING
            );
        }
    }

    #[test]
    fn ordinary_copy_and_standalone_space_are_allowed() {
        assert!(validate_candidate(&binding(
            Some(ModifierKind::Control),
            PhysicalKeyId::new(0x2e, false),
        ))
        .is_ok());
        assert!(validate_candidate(&binding(None, PhysicalKeyId::new(0x39, false),)).is_ok());
    }
}
