use crate::target::TargetWindowIdentity;
use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryMode {
    Unicode,
    ClipboardPaste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionState {
    NotSubmitted,
    Submitted,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestorationState {
    NotNeeded,
    Restored,
    SkippedConcurrentChange,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub transaction_id: Uuid,
    pub submission: SubmissionState,
    pub restoration: RestorationState,
}

#[derive(Clone, Debug)]
pub struct DeliveryRequest {
    pub transaction_id: Uuid,
    pub text: String,
    pub target: TargetWindowIdentity,
    pub mode: DeliveryMode,
}

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("clipboard compatibility is temporarily unavailable")]
    ClipboardTemporarilyUnavailable,
    #[error("delivery worker failed: {0}")]
    Worker(String),
    #[error("text injection is only supported on Windows")]
    Unsupported,
}

#[async_trait]
pub trait DeliveryExecutor: Send + Sync {
    async fn deliver(&self, request: DeliveryRequest) -> Result<DeliveryReceipt, InjectError>;
}

#[derive(Debug, Default)]
pub struct SafeModeDeliveryExecutor;

#[async_trait]
impl DeliveryExecutor for SafeModeDeliveryExecutor {
    async fn deliver(&self, request: DeliveryRequest) -> Result<DeliveryReceipt, InjectError> {
        if request.mode == DeliveryMode::ClipboardPaste {
            return Err(InjectError::ClipboardTemporarilyUnavailable);
        }

        let transaction_id = request.transaction_id;
        let text = request.text;
        let submission = tauri::async_runtime::spawn_blocking(move || send_unicode_text(&text))
            .await
            .map_err(|error| InjectError::Worker(error.to_string()))??;
        Ok(DeliveryReceipt {
            transaction_id,
            submission,
            restoration: RestorationState::NotNeeded,
        })
    }
}

#[cfg(target_os = "windows")]
fn send_unicode_text(text: &str) -> Result<SubmissionState, InjectError> {
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
fn send_unicode_text(_text: &str) -> Result<SubmissionState, InjectError> {
    Err(InjectError::Unsupported)
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
fn send_inputs(
    inputs: &[windows::Win32::UI::Input::KeyboardAndMouse::INPUT],
) -> Result<SubmissionState, InjectError> {
    use windows::Win32::Foundation::GetLastError;
    use windows::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT};

    if inputs.is_empty() {
        return Ok(SubmissionState::Submitted);
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    let state = classify_send_input_count(inputs.len(), sent);
    if state != SubmissionState::Submitted {
        log::warn!(
            "SendInput submitted {sent}/{} events; state={state:?}; windows_error={}",
            inputs.len(),
            unsafe { GetLastError().0 }
        );
    }
    Ok(state)
}

fn classify_send_input_count(expected: usize, actual: u32) -> SubmissionState {
    if actual == 0 {
        SubmissionState::NotSubmitted
    } else if actual == expected as u32 {
        SubmissionState::Submitted
    } else {
        SubmissionState::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_input_counts_have_three_distinct_outcomes() {
        assert_eq!(
            classify_send_input_count(4, 0),
            SubmissionState::NotSubmitted
        );
        assert_eq!(classify_send_input_count(4, 4), SubmissionState::Submitted);
        assert_eq!(classify_send_input_count(4, 2), SubmissionState::Unknown);
    }
}
