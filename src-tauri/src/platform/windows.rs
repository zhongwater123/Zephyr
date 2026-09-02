use super::{NativeConfirmation, PlatformRuntimeAdapters, PlatformServiceAdapters};
use crate::command_error::{CommandError, CommandResult};
use crate::config::EndpointPurpose;
use crate::desktop_support::DesktopSupportPolicy;
use crate::shortcut_runtime::{ShortcutEventHandler, ShortcutRuntimeFactory, ShortcutRuntimePort};
use crate::windows_keyboard::WindowsKeyboardEngine;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;
use tauri::WebviewWindow;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO,
};

#[derive(Debug, Default)]
struct WindowsNativeConfirmation;

#[derive(Debug, Default)]
struct WindowsShortcutRuntimeFactory;

impl ShortcutRuntimeFactory for WindowsShortcutRuntimeFactory {
    fn start(
        &self,
        on_event: ShortcutEventHandler,
    ) -> Result<Arc<dyn ShortcutRuntimePort>, String> {
        WindowsKeyboardEngine::start(on_event)
            .map(|runtime| Arc::new(runtime) as Arc<dyn ShortcutRuntimePort>)
    }
}

fn message_box(window: &WebviewWindow, title: &str, message: &str) -> CommandResult<bool> {
    let message_wide: Vec<u16> = std::ffi::OsStr::new(message)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let title_wide: Vec<u16> = std::ffi::OsStr::new(title)
        .encode_wide()
        .chain(Some(0))
        .collect();
    let raw_parent = window
        .hwnd()
        .map_err(|error| CommandError::new("native_dialog_failed", error.to_string()))?;
    let result = unsafe {
        MessageBoxW(
            HWND(raw_parent.0 as *mut _),
            PCWSTR(message_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
        )
    };
    Ok(result == IDYES)
}

impl NativeConfirmation for WindowsNativeConfirmation {
    fn authorize_endpoint(
        &self,
        window: &WebviewWindow,
        origin: &str,
        purpose: &EndpointPurpose,
    ) -> CommandResult<bool> {
        let purpose_label = match purpose {
            EndpointPurpose::Asr => "语音识别",
            EndpointPurpose::HotwordAgent => "热词整理 Agent",
            EndpointPurpose::TextProcessing => "智能成稿处理",
        };
        message_box(
            window,
            "授权新的凭据接收主机",
            &format!(
                "Zephyr 即将向以下新主机发送 {purpose_label} 凭据：\n\n{origin}\n\n该主机可能读取并使用你的密钥。是否授权？"
            ),
        )
    }

    fn enable_clipboard_compatibility(
        &self,
        window: &WebviewWindow,
        executable_name: &str,
    ) -> CommandResult<bool> {
        message_box(
            window,
            "剪贴板兼容模式风险确认",
            &format!(
                "是否为 {executable_name} 启用剪贴板兼容模式？\n\n该模式只会在隔离辅助进程能够安全保存剪贴板并复验原目标时启用；否则结果进入待处理区。"
            ),
        )
    }
}

pub(super) fn service_adapters() -> PlatformServiceAdapters {
    PlatformServiceAdapters {
        support: DesktopSupportPolicy::windows(),
        confirmations: Arc::new(WindowsNativeConfirmation),
    }
}

pub(super) fn runtime_adapters(app: &tauri::AppHandle) -> PlatformRuntimeAdapters {
    PlatformRuntimeAdapters {
        targets: Arc::new(crate::windows_target::WindowsTargetAdapter),
        delivery: Arc::new(
            crate::clipboard_transaction::ClipboardTransactionService::new(app.clone()),
        ),
        shortcut: Arc::new(WindowsShortcutRuntimeFactory),
    }
}
