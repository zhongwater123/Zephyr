use crate::target::TargetWindowIdentity;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InjectionMethod {
    Unicode,
    ClipboardCompatibility,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardRestoration {
    Restored,
    SkippedConcurrentChange,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomicPasteReceipt {
    pub paste_submitted: bool,
    pub clipboard_restoration: ClipboardRestoration,
}

impl AtomicPasteReceipt {
    fn submitted(clipboard_restoration: ClipboardRestoration) -> Self {
        Self {
            paste_submitted: true,
            clipboard_restoration,
        }
    }
}

#[derive(Debug, Error)]
#[allow(dead_code)]
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

    fn inject_atomic_paste(
        &self,
        text: &str,
        _target: &TargetWindowIdentity,
    ) -> Result<AtomicPasteReceipt, InjectError> {
        self.inject_text(text, InjectionMethod::ClipboardCompatibility)?;
        Ok(AtomicPasteReceipt::submitted(
            ClipboardRestoration::Restored,
        ))
    }
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

    fn inject_atomic_paste(
        &self,
        text: &str,
        target: &TargetWindowIdentity,
    ) -> Result<AtomicPasteReceipt, InjectError> {
        paste_text_atomically(text, target)
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

fn atomic_paste_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[cfg(target_os = "windows")]
pub fn paste_text_via_clipboard(text: &str) -> Result<(), InjectError> {
    let receipt = paste_with_clipboard(text, Duration::from_millis(80), || Ok(()))?;
    log_restoration_issue(&receipt);
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn paste_text_atomically(
    text: &str,
    target: &TargetWindowIdentity,
) -> Result<AtomicPasteReceipt, InjectError> {
    let _guard = atomic_paste_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    paste_with_clipboard(text, Duration::from_millis(80), || {
        crate::target::validate_foreground_target(target).map_err(InjectError::Clipboard)
    })
}

#[cfg(not(target_os = "windows"))]
pub fn paste_text_atomically(
    _text: &str,
    _target: &TargetWindowIdentity,
) -> Result<AtomicPasteReceipt, InjectError> {
    Err(InjectError::Unsupported)
}

fn log_restoration_issue(receipt: &AtomicPasteReceipt) {
    match &receipt.clipboard_restoration {
        ClipboardRestoration::Restored => {}
        ClipboardRestoration::SkippedConcurrentChange => {
            log::warn!("clipboard changed after paste submission; skipped restoration");
        }
        ClipboardRestoration::Failed(message) => {
            log::warn!("paste was submitted but clipboard restoration failed: {message}");
        }
    }
}

#[cfg(target_os = "windows")]
fn paste_with_clipboard(
    text: &str,
    restore_delay: Duration,
    before_paste: impl FnOnce() -> Result<(), InjectError>,
) -> Result<AtomicPasteReceipt, InjectError> {
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
        .map_err(|error| InjectError::Clipboard(format!("OLE initialization failed: {error}")))?;
    let _apartment = OleApartment;
    let original = unsafe { OleGetClipboard() }.map_err(|error| {
        InjectError::Clipboard(format!(
            "could not snapshot the complete IDataObject clipboard: {error}"
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
    let restoration =
        if !clipboard_sequence_unchanged(sequence_after_write, sequence_before_restore) {
            ClipboardRestoration::SkippedConcurrentChange
        } else {
            drop(clipboard);
            match unsafe { OleSetClipboard(&original) }.and_then(|_| unsafe { OleFlushClipboard() })
            {
                Ok(()) => ClipboardRestoration::Restored,
                Err(error) => ClipboardRestoration::Failed(error.to_string()),
            }
        };

    paste_result?;
    Ok(AtomicPasteReceipt::submitted(restoration))
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
}

#[cfg(test)]
mod receipt_tests {
    use super::*;

    #[test]
    fn submitted_receipt_keeps_concurrent_clipboard_change_as_success_metadata() {
        let receipt = AtomicPasteReceipt::submitted(ClipboardRestoration::SkippedConcurrentChange);
        assert!(receipt.paste_submitted);
        assert_eq!(
            receipt.clipboard_restoration,
            ClipboardRestoration::SkippedConcurrentChange
        );
    }

    #[test]
    fn submitted_receipt_keeps_restore_failure_as_success_metadata() {
        let receipt = AtomicPasteReceipt::submitted(ClipboardRestoration::Failed(
            "restore failed".to_string(),
        ));
        assert!(receipt.paste_submitted);
        assert!(matches!(
            receipt.clipboard_restoration,
            ClipboardRestoration::Failed(_)
        ));
    }
}
