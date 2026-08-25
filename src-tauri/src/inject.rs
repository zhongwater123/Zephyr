use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InjectionMethod {
    Unicode,
    ClipboardCompatibility,
}

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("clipboard compatibility mode is unavailable: {0}")]
    Clipboard(String),
    #[error(
        "SendInput wrote {actual} of {expected} keyboard events (Windows error {windows_error})"
    )]
    PartialSendInput {
        expected: usize,
        actual: u32,
        windows_error: u32,
    },
    #[error("text injection is only supported on Windows")]
    Unsupported,
}

pub trait TextInjector: Send + Sync {
    fn inject_text(&self, text: &str, method: InjectionMethod) -> Result<(), InjectError>;
}

#[derive(Debug, Default)]
pub struct UnicodeTextInjector;

impl TextInjector for UnicodeTextInjector {
    fn inject_text(&self, text: &str, method: InjectionMethod) -> Result<(), InjectError> {
        match method {
            InjectionMethod::Unicode => send_unicode_text(text),
            InjectionMethod::ClipboardCompatibility => paste_text_via_clipboard(text),
        }
    }
}

#[cfg(target_os = "windows")]
pub fn send_unicode_text(text: &str) -> Result<(), InjectError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{KEYEVENTF_KEYUP, KEYEVENTF_UNICODE};

    let utf16: Vec<u16> = text.encode_utf16().collect();
    let mut inputs = Vec::with_capacity(utf16.len() * 2);
    for code_unit in utf16 {
        inputs.push(unicode_input(code_unit, KEYEVENTF_UNICODE.0));
        inputs.push(unicode_input(
            code_unit,
            KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0,
        ));
    }
    send_inputs(&inputs)
}

#[cfg(not(target_os = "windows"))]
pub fn send_unicode_text(_text: &str) -> Result<(), InjectError> {
    Err(InjectError::Unsupported)
}

#[cfg(target_os = "windows")]
pub fn paste_text_via_clipboard(text: &str) -> Result<(), InjectError> {
    paste_with_clipboard(text, Duration::from_millis(80), || Ok(()))
}

#[cfg(target_os = "windows")]
fn paste_with_clipboard(
    text: &str,
    restore_delay: Duration,
    before_paste: impl FnOnce() -> Result<(), InjectError>,
) -> Result<(), InjectError> {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    use windows::Win32::System::Ole::{
        OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard, OleUninitialize,
    };

    struct OleApartment;
    impl Drop for OleApartment {
        fn drop(&mut self) {
            unsafe { OleUninitialize() };
        }
    }

    unsafe { OleInitialize(None) }
        .map_err(|error| InjectError::Clipboard(format!("OLE 初始化失败: {error}")))?;
    let _apartment = OleApartment;
    let original = unsafe { OleGetClipboard() }.map_err(|error| {
        InjectError::Clipboard(format!(
            "无法取得完整 IDataObject 剪贴板快照，已拒绝兼容模式注入: {error}"
        ))
    })?;

    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| InjectError::Clipboard(error.to_string()))?;
    clipboard
        .set_text(text.to_string())
        .map_err(|error| InjectError::Clipboard(error.to_string()))?;
    let sequence_after_write = unsafe { GetClipboardSequenceNumber() };

    let paste_result = before_paste().and_then(|_| send_ctrl_v());
    thread::sleep(restore_delay);

    let sequence_before_restore = unsafe { GetClipboardSequenceNumber() };
    if !clipboard_sequence_unchanged(sequence_after_write, sequence_before_restore) {
        return paste_result.and(Err(InjectError::Clipboard(
            "剪贴板已被用户或其他程序修改，已跳过恢复".to_string(),
        )));
    }

    drop(clipboard);
    let restore_result = unsafe { OleSetClipboard(&original) }
        .and_then(|_| unsafe { OleFlushClipboard() })
        .map_err(|error| InjectError::Clipboard(format!("完整恢复剪贴板失败: {error}")));

    paste_result.and(restore_result)
}

#[cfg(not(target_os = "windows"))]
pub fn paste_text_via_clipboard(_text: &str) -> Result<(), InjectError> {
    Err(InjectError::Unsupported)
}

#[cfg(target_os = "windows")]
fn send_ctrl_v() -> Result<(), InjectError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{KEYEVENTF_KEYUP, VK_CONTROL, VK_V};

    let inputs = [
        virtual_key_input(VK_CONTROL, 0),
        virtual_key_input(VK_V, 0),
        virtual_key_input(VK_V, KEYEVENTF_KEYUP.0),
        virtual_key_input(VK_CONTROL, KEYEVENTF_KEYUP.0),
    ];
    send_inputs(&inputs)
}

#[cfg(target_os = "windows")]
fn unicode_input(code_unit: u16, flags: u32) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: code_unit,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: crate::windows_keyboard::SELF_INJECTED_MARKER,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn virtual_key_input(
    key: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY,
    flags: u32,
) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: crate::windows_keyboard::SELF_INJECTED_MARKER,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn send_inputs(
    inputs: &[windows::Win32::UI::Input::KeyboardAndMouse::INPUT],
) -> Result<(), InjectError> {
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};

    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    validate_send_input_count(inputs.len(), sent, unsafe { GetLastError().0 })
}

fn validate_send_input_count(
    expected: usize,
    actual: u32,
    windows_error: u32,
) -> Result<(), InjectError> {
    if actual == expected as u32 {
        Ok(())
    } else {
        Err(InjectError::PartialSendInput {
            expected,
            actual,
            windows_error,
        })
    }
}

fn clipboard_sequence_unchanged(sequence_after_write: u32, sequence_before_restore: u32) -> bool {
    sequence_after_write == sequence_before_restore
}

#[cfg(test)]
mod tests {
    #[test]
    fn utf16_surrogate_pairs_are_preserved() {
        let units: Vec<u16> = "A🙂中".encode_utf16().collect();
        assert_eq!(units, vec![0x0041, 0xD83D, 0xDE42, 0x4E2D]);
    }

    #[test]
    fn partial_send_input_is_an_error() {
        assert!(matches!(
            super::validate_send_input_count(4, 2, 5),
            Err(super::InjectError::PartialSendInput {
                expected: 4,
                actual: 2,
                windows_error: 5,
            })
        ));
    }

    #[test]
    fn clipboard_restore_requires_an_unchanged_sequence() {
        assert!(super::clipboard_sequence_unchanged(10, 10));
        assert!(!super::clipboard_sequence_unchanged(10, 11));
    }
}
