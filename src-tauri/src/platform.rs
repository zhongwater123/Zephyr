use crate::command_error::{CommandError, CommandResult};
use crate::config::EndpointPurpose;
use tauri::WebviewWindow;

pub mod tray;

pub trait NativeConfirmation: Send + Sync {
    fn authorize_endpoint(
        &self,
        window: &WebviewWindow,
        origin: &str,
        purpose: &EndpointPurpose,
    ) -> CommandResult<bool>;

    fn enable_clipboard_compatibility(
        &self,
        window: &WebviewWindow,
        executable_name: &str,
    ) -> CommandResult<bool>;
}

#[derive(Debug, Default)]
pub struct WindowsNativeConfirmation;

#[cfg(target_os = "windows")]
fn message_box(window: &WebviewWindow, title: &str, message: &str) -> CommandResult<bool> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO,
    };

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
        #[cfg(target_os = "windows")]
        {
            let purpose_label = match purpose {
                EndpointPurpose::Asr => "语音识别",
                EndpointPurpose::HotwordAgent => "热词整理 Agent",
                EndpointPurpose::TextProcessing => "智能成稿处理",
            };
            message_box(
                window,
                "授权新的凭据接收主机",
                &format!(
                    "GY Typing 即将向以下新主机发送 {purpose_label} 凭据：\n\n{origin}\n\n该主机可能读取并使用你的密钥。是否授权？"
                ),
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (window, origin, purpose);
            Err(CommandError::new(
                "native_dialog_unavailable",
                "endpoint 授权仅支持 Windows 原生确认框",
            ))
        }
    }

    fn enable_clipboard_compatibility(
        &self,
        window: &WebviewWindow,
        executable_name: &str,
    ) -> CommandResult<bool> {
        #[cfg(target_os = "windows")]
        {
            message_box(
                window,
                "剪贴板兼容模式风险确认",
                &format!(
                    "是否为 {executable_name} 启用剪贴板兼容模式？\n\n该模式会临时替换系统剪贴板并发送 Ctrl+V。GY Typing 只会在取得完整 OLE IDataObject 快照、目标身份仍有效且剪贴板 sequence 未变化时执行和恢复；否则结果进入待处理区。"
                ),
            )
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (window, executable_name);
            Err(CommandError::new(
                "native_dialog_unavailable",
                "剪贴板兼容模式仅支持 Windows 原生确认框",
            ))
        }
    }
}
