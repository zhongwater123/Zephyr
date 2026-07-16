use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("clipboard is unavailable: {0}")]
    Clipboard(String),
    #[error("input simulation failed")]
    SendInput,
}

pub trait TextInjector: Send + Sync {
    fn paste_text(&self, text: &str) -> Result<(), InjectError>;
}

#[derive(Debug, Default)]
pub struct ClipboardTextInjector;

impl TextInjector for ClipboardTextInjector {
    fn paste_text(&self, text: &str) -> Result<(), InjectError> {
        paste_text_via_clipboard(text)
    }
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
    let mut clipboard =
        arboard::Clipboard::new().map_err(|error| InjectError::Clipboard(error.to_string()))?;
    let original_text = clipboard.get_text().ok();
    clipboard
        .set_text(text.to_string())
        .map_err(|error| InjectError::Clipboard(error.to_string()))?;

    before_paste()?;
    if !text.is_empty() {
        send_ctrl_v()?;
    }

    thread::sleep(restore_delay);
    if let Some(original_text) = original_text {
        clipboard
            .set_text(original_text)
            .map_err(|error| InjectError::Clipboard(error.to_string()))?;
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn paste_text_via_clipboard(_text: &str) -> Result<(), InjectError> {
    Err(InjectError::SendInput)
}

#[cfg(target_os = "windows")]
fn send_ctrl_v() -> Result<(), InjectError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    let inputs = [
        key_input(VK_CONTROL, 0),
        key_input(VK_V, 0),
        key_input(VK_V, KEYEVENTF_KEYUP.0),
        key_input(VK_CONTROL, KEYEVENTF_KEYUP.0),
    ];

    send_inputs(&inputs)
}

#[cfg(target_os = "windows")]
fn key_input(
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
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(target_os = "windows")]
fn send_inputs(
    inputs: &[windows::Win32::UI::Input::KeyboardAndMouse::INPUT],
) -> Result<(), InjectError> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};

    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err(InjectError::SendInput)
    }
}
