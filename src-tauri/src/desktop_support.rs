use crate::command_error::{CommandError, CommandResult};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopCapability {
    GlobalShortcut,
    AutomaticTextDelivery,
    NativeConfirmation,
    // The macOS bootstrap advertises this shared capability as unsupported;
    // only the Windows assembly constructs the capability today.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    PreinputOverlay,
}

impl DesktopCapability {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GlobalShortcut => "globalShortcut",
            Self::AutomaticTextDelivery => "automaticTextDelivery",
            Self::NativeConfirmation => "nativeConfirmation",
            Self::PreinputOverlay => "preinputOverlay",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DesktopSupportPolicy {
    global_shortcut: bool,
    automatic_text_delivery: bool,
    native_confirmation: bool,
    preinput_overlay: bool,
}

impl DesktopSupportPolicy {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub const fn windows() -> Self {
        Self {
            global_shortcut: true,
            automatic_text_delivery: true,
            native_confirmation: true,
            preinput_overlay: true,
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub const fn macos_bootstrap() -> Self {
        Self {
            global_shortcut: false,
            automatic_text_delivery: false,
            native_confirmation: false,
            preinput_overlay: false,
        }
    }

    pub const fn supports(&self, capability: DesktopCapability) -> bool {
        match capability {
            DesktopCapability::GlobalShortcut => self.global_shortcut,
            DesktopCapability::AutomaticTextDelivery => self.automatic_text_delivery,
            DesktopCapability::NativeConfirmation => self.native_confirmation,
            DesktopCapability::PreinputOverlay => self.preinput_overlay,
        }
    }

    pub fn require(&self, capability: DesktopCapability) -> CommandResult<()> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(unsupported_platform(capability))
        }
    }
}

pub fn unsupported_platform(capability: DesktopCapability) -> CommandError {
    CommandError::with_details(
        "unsupported_platform",
        "当前 macOS 版本暂不支持此能力",
        serde_json::json!({ "capability": capability.as_str() }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_error_has_stable_shape() {
        let error = DesktopSupportPolicy::macos_bootstrap()
            .require(DesktopCapability::GlobalShortcut)
            .expect_err("shortcut must remain unsupported during bootstrap");
        assert_eq!(error.code, "unsupported_platform");
        assert_eq!(error.details.unwrap()["capability"], "globalShortcut");
    }

    #[test]
    fn windows_policy_keeps_existing_capabilities_enabled() {
        let policy = DesktopSupportPolicy::windows();
        for capability in [
            DesktopCapability::GlobalShortcut,
            DesktopCapability::AutomaticTextDelivery,
            DesktopCapability::NativeConfirmation,
            DesktopCapability::PreinputOverlay,
        ] {
            assert!(policy.supports(capability));
        }
    }
}
