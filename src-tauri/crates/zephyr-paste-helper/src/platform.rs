use crate::snapshot::{ClipboardFormat, Snapshot};
use paste_protocol::{SubmissionState, TargetIdentity, SELF_INJECTED_MARKER};
use sha2::{Digest, Sha256};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;
use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, GlobalFree, SetLastError, HANDLE, HGLOBAL, HWND, WIN32_ERROR,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardFormatNameW, GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
    HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
};

const MAX_FORMAT_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 128 * 1024 * 1024;
const MARKER_FORMAT_NAME: &str = "com.gy.typing.clipboard-transaction.v1";
// The windows crate exposes these numeric Win32 constants through its Ole module.
// Keep the helper independent of OLE by using the stable values from WinUser.h.
const FORMAT_TEXT: u32 = 1;
const FORMAT_BITMAP: u32 = 2;
const FORMAT_OEMTEXT: u32 = 7;
const FORMAT_DIB: u32 = 8;
const FORMAT_PALETTE: u32 = 9;
const FORMAT_UNICODETEXT: u32 = 13;
const FORMAT_HDROP: u32 = 15;
const FORMAT_LOCALE: u32 = 16;
const FORMAT_DIBV5: u32 = 17;
const FORMAT_OWNERDISPLAY: u32 = 128;

pub struct ClipboardOwner(HWND);

pub enum RestoreOutcome {
    Restored,
    SkippedConcurrentChange,
}

impl ClipboardOwner {
    pub fn create() -> Result<Self, String> {
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!(""),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                None,
                None,
                None,
            )
        }
        .map_err(|error| format!("clipboard owner window creation failed: {error}"))?;
        Ok(Self(hwnd))
    }

    fn hwnd(&self) -> HWND {
        self.0
    }
}

impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn open_clipboard(owner: &ClipboardOwner) -> Result<ClipboardGuard, String> {
    let started = Instant::now();
    loop {
        match unsafe { OpenClipboard(owner.hwnd()) } {
            Ok(()) => return Ok(ClipboardGuard),
            Err(error) if started.elapsed() < Duration::from_millis(250) => {
                thread::sleep(Duration::from_millis(10));
                let _ = error;
            }
            Err(error) => return Err(format!("clipboard busy: {error}")),
        }
    }
}

pub fn capture_clipboard(owner: &ClipboardOwner, transaction_id: Uuid) -> Result<Snapshot, String> {
    let _guard = open_clipboard(owner)?;
    let captured_sequence = unsafe { GetClipboardSequenceNumber() };
    let mut ids = Vec::new();
    let mut current = 0u32;
    loop {
        unsafe { SetLastError(WIN32_ERROR(0)) };
        current = unsafe { EnumClipboardFormats(current) };
        if current == 0 {
            let error = unsafe { GetLastError() };
            if error.0 != 0 {
                return Err(format!("clipboard format enumeration failed: {error:?}"));
            }
            break;
        }
        ids.push(current);
        if ids.len() > 256 {
            return Err("clipboard has too many formats".to_string());
        }
    }
    let has_dib = ids
        .iter()
        .any(|id| *id == FORMAT_DIB || *id == FORMAT_DIBV5);
    let has_unicode = ids.contains(&FORMAT_UNICODETEXT);
    let mut formats = Vec::new();
    let mut total = 0usize;
    for format_id in ids {
        if (format_id == FORMAT_BITMAP || format_id == FORMAT_PALETTE) && has_dib {
            continue;
        }
        if (format_id == FORMAT_TEXT || format_id == FORMAT_OEMTEXT) && has_unicode {
            continue;
        }
        if format_id == FORMAT_OWNERDISPLAY {
            return Err("clipboard OwnerDisplay format is unsupported".to_string());
        }
        let registered_name = registered_format_name(format_id);
        if !is_hglobal_snapshot_candidate(format_id, registered_name.as_deref()) {
            return Err(format!("unsupported clipboard format: {format_id}"));
        }
        let handle = unsafe { GetClipboardData(format_id) }.map_err(|error| {
            format!("clipboard format {format_id} is not materialized: {error}")
        })?;
        let data = copy_global_data(handle)?;
        validate_format(format_id, registered_name.as_deref(), &data)?;
        total = total
            .checked_add(data.len())
            .ok_or("clipboard size overflow")?;
        if data.len() > MAX_FORMAT_BYTES || total > MAX_TOTAL_BYTES {
            return Err("clipboard snapshot exceeds safety limit".to_string());
        }
        formats.push(ClipboardFormat {
            format_id,
            registered_name,
            data,
        });
    }
    Ok(Snapshot {
        transaction_id,
        captured_sequence,
        phase: None,
        payload_sequence: None,
        payload_sha256: None,
        formats,
    })
}

pub fn write_payload(
    owner: &ClipboardOwner,
    transaction_id: Uuid,
    text: &str,
) -> Result<(u32, [u8; 32]), String> {
    let payload_sha256: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    let marker = marker_bytes(transaction_id, &payload_sha256);
    let marker_format = register_format(MARKER_FORMAT_NAME)?;
    let mut utf16 = text.encode_utf16().collect::<Vec<_>>();
    utf16.push(0);
    let unicode_bytes =
        unsafe { std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), utf16.len() * 2) };
    {
        let _guard = open_clipboard(owner)?;
        unsafe { EmptyClipboard() }.map_err(|error| format!("EmptyClipboard failed: {error}"))?;
        set_global_data(marker_format, &marker)?;
        set_global_data(FORMAT_UNICODETEXT, unicode_bytes)?;
    }
    Ok((unsafe { GetClipboardSequenceNumber() }, payload_sha256))
}

pub fn restore_clipboard_if_current(
    owner: &ClipboardOwner,
    snapshot: &Snapshot,
    expected_sequence: u32,
    expected_sha256: &[u8; 32],
) -> Result<RestoreOutcome, String> {
    let marker_format = register_format(MARKER_FORMAT_NAME)?;
    let _guard = open_clipboard(owner)?;
    if unsafe { GetClipboardSequenceNumber() } != expected_sequence {
        return Ok(RestoreOutcome::SkippedConcurrentChange);
    }
    let marker = match unsafe { GetClipboardData(marker_format) } {
        Ok(handle) => copy_global_data(handle)?,
        Err(_) => return Ok(RestoreOutcome::SkippedConcurrentChange),
    };
    if marker != marker_bytes(snapshot.transaction_id, expected_sha256) {
        return Ok(RestoreOutcome::SkippedConcurrentChange);
    }
    let text_handle = match unsafe { GetClipboardData(FORMAT_UNICODETEXT) } {
        Ok(handle) => handle,
        Err(_) => return Ok(RestoreOutcome::SkippedConcurrentChange),
    };
    let text_bytes = copy_global_data(text_handle)?;
    let text = decode_unicode_text(&text_bytes)?;
    let actual: [u8; 32] = Sha256::digest(text.as_bytes()).into();
    if &actual != expected_sha256 {
        return Ok(RestoreOutcome::SkippedConcurrentChange);
    }
    unsafe { EmptyClipboard() }
        .map_err(|error| format!("restore EmptyClipboard failed: {error}"))?;
    for format in &snapshot.formats {
        let format_id = match &format.registered_name {
            Some(name) => register_format(name)?,
            None => format.format_id,
        };
        set_global_data(format_id, &format.data)?;
    }
    Ok(RestoreOutcome::Restored)
}

pub fn verify_target(target: &TargetIdentity) -> Result<(), String> {
    let hwnd = HWND(target.hwnd as isize as *mut _);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err("original target window is closed".to_string());
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != target.process_id {
        return Err("target process identity changed".to_string());
    }
    let (started_at, executable_path) = process_identity(process_id)?;
    if started_at != target.process_started_at
        || !executable_path.eq_ignore_ascii_case(&target.executable_path)
    {
        return Err("target executable identity changed".to_string());
    }
    if unsafe { GetForegroundWindow() } != hwnd {
        return Err("original target is no longer foreground".to_string());
    }
    Ok(())
}

pub fn send_unicode(text: &str, injected_count: Option<u32>) -> SubmissionState {
    let mut inputs = Vec::with_capacity(text.encode_utf16().count() * 2);
    for code_unit in text.encode_utf16() {
        inputs.push(keyboard_input(0, code_unit, KEYEVENTF_UNICODE.0));
        inputs.push(keyboard_input(
            0,
            code_unit,
            KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0,
        ));
    }
    send_inputs(&inputs, injected_count)
}

pub fn send_ctrl_v(injected_count: Option<u32>) -> SubmissionState {
    let inputs = [
        keyboard_input(VK_CONTROL.0, 0, 0),
        keyboard_input(VK_V.0, 0, 0),
        keyboard_input(VK_V.0, 0, KEYEVENTF_KEYUP.0),
        keyboard_input(VK_CONTROL.0, 0, KEYEVENTF_KEYUP.0),
    ];
    let state = send_inputs(&inputs, injected_count);
    if state == SubmissionState::Unknown {
        let releases = [
            keyboard_input(VK_V.0, 0, KEYEVENTF_KEYUP.0),
            keyboard_input(VK_CONTROL.0, 0, KEYEVENTF_KEYUP.0),
        ];
        let _ = send_inputs(&releases, None);
    }
    state
}

fn send_inputs(inputs: &[INPUT], injected_count: Option<u32>) -> SubmissionState {
    if inputs.is_empty() {
        return SubmissionState::Submitted;
    }
    let actual = injected_count
        .map(|count| count.min(inputs.len() as u32))
        .unwrap_or_else(|| unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) });
    if actual == 0 {
        SubmissionState::NotSubmitted
    } else if actual == inputs.len() as u32 {
        SubmissionState::Submitted
    } else {
        SubmissionState::Unknown
    }
}

fn keyboard_input(virtual_key: u16, scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(virtual_key),
                wScan: scan,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: SELF_INJECTED_MARKER,
            },
        },
    }
}

fn process_identity(process_id: u32) -> Result<(u64, String), String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| error.to_string())?;
    let result = (|| {
        let mut creation = Default::default();
        let mut exit = Default::default();
        let mut kernel = Default::default();
        let mut user = Default::default();
        unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) }
            .map_err(|error| error.to_string())?;
        let started = ((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64;
        let mut buffer = vec![0u16; 32_768];
        let mut length = buffer.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
            )
        }
        .map_err(|error| error.to_string())?;
        let queried = String::from_utf16_lossy(&buffer[..length as usize]);
        let normalized = std::fs::canonicalize(&queried)
            .unwrap_or_else(|_| std::path::PathBuf::from(&queried))
            .to_string_lossy()
            .into_owned();
        Ok((started, normalized))
    })();
    let _ = unsafe { CloseHandle(process) };
    result
}

fn registered_format_name(format_id: u32) -> Option<String> {
    if format_id < 0xC000 {
        return None;
    }
    let mut buffer = vec![0u16; 256];
    let length = unsafe { GetClipboardFormatNameW(format_id, &mut buffer) };
    (length > 0).then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

fn is_hglobal_snapshot_candidate(format_id: u32, registered_name: Option<&str>) -> bool {
    // Registered formats are candidates, not automatically trusted. The subsequent
    // GlobalSize/GlobalLock copy is the capability check that proves this instance
    // is bounded, materialized memory owned by the snapshot.
    matches!(
        format_id,
        value if value == FORMAT_UNICODETEXT
            || value == FORMAT_TEXT
            || value == FORMAT_OEMTEXT
            || value == FORMAT_DIB
            || value == FORMAT_DIBV5
             || value == FORMAT_HDROP
             || value == FORMAT_LOCALE
    ) || (format_id >= 0xC000 && registered_name.is_some())
}

fn validate_format(
    format_id: u32,
    registered_name: Option<&str>,
    data: &[u8],
) -> Result<(), String> {
    if data.is_empty() {
        return Err("clipboard format has an empty global handle".to_string());
    }
    if format_id == FORMAT_UNICODETEXT {
        decode_unicode_text(data)?;
    } else if format_id == FORMAT_TEXT || format_id == FORMAT_OEMTEXT {
        if data.last() != Some(&0) {
            return Err("clipboard text is not terminated".to_string());
        }
    } else if format_id == FORMAT_DIB || format_id == FORMAT_DIBV5 {
        if data.len() < 40 {
            return Err("DIB header is truncated".to_string());
        }
        let header_size = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let minimum_header = if format_id == FORMAT_DIBV5 { 124 } else { 40 };
        if header_size < minimum_header || header_size > data.len() {
            return Err("DIB header size is invalid".to_string());
        }
    } else if format_id == FORMAT_HDROP {
        if data.len() < 20 {
            return Err("CF_HDROP header is truncated".to_string());
        }
        let offset = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        if offset < 20 || offset >= data.len() {
            return Err("CF_HDROP file list offset is invalid".to_string());
        }
        let wide = u32::from_le_bytes(data[16..20].try_into().unwrap()) != 0;
        let terminated = if wide {
            data[offset..]
                .windows(4)
                .any(|window| window == [0, 0, 0, 0])
        } else {
            data[offset..].windows(2).any(|window| window == [0, 0])
        };
        if !terminated {
            return Err("CF_HDROP file list is not terminated".to_string());
        }
    } else if format_id == FORMAT_LOCALE && data.len() < 4 {
        return Err("clipboard locale is truncated".to_string());
    } else if registered_name.is_some_and(|name| name.eq_ignore_ascii_case("PNG"))
        && !data.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10])
    {
        return Err("PNG signature is invalid".to_string());
    } else if registered_name.is_some_and(|name| name.eq_ignore_ascii_case("Rich Text Format"))
        && !data.starts_with(br"{\rtf")
    {
        return Err("RTF header is invalid".to_string());
    } else if registered_name.is_some_and(|name| name.eq_ignore_ascii_case("HTML Format"))
        && !(data.starts_with(b"Version:")
            && data.windows(10).any(|window| window == b"StartHTML:"))
    {
        return Err("HTML clipboard header is invalid".to_string());
    }
    Ok(())
}

fn decode_unicode_text(data: &[u8]) -> Result<String, String> {
    if !data.len().is_multiple_of(2) {
        return Err("Unicode clipboard text has odd byte length".to_string());
    }
    let units = data
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    let terminator = units
        .iter()
        .position(|unit| *unit == 0)
        .ok_or_else(|| "Unicode clipboard text is not terminated".to_string())?;
    String::from_utf16(&units[..terminator]).map_err(|error| error.to_string())
}

fn copy_global_data(handle: HANDLE) -> Result<Vec<u8>, String> {
    let global = HGLOBAL(handle.0);
    let size = unsafe { GlobalSize(global) };
    if size == 0 || size > MAX_FORMAT_BYTES {
        return Err("clipboard handle is not a bounded HGLOBAL".to_string());
    }
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        return Err("clipboard HGLOBAL cannot be locked".to_string());
    }
    let data = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size).to_vec() };
    let _ = unsafe { GlobalUnlock(global) };
    Ok(data)
}

fn set_global_data(format_id: u32, data: &[u8]) -> Result<(), String> {
    let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) }
        .map_err(|error| error.to_string())?;
    let pointer = unsafe { GlobalLock(global) };
    if pointer.is_null() {
        let _ = unsafe { GlobalFree(global) };
        return Err("allocated HGLOBAL cannot be locked".to_string());
    }
    unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.cast::<u8>(), data.len()) };
    let _ = unsafe { GlobalUnlock(global) };
    match unsafe { SetClipboardData(format_id, HANDLE(global.0)) } {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = unsafe { GlobalFree(global) };
            Err(error.to_string())
        }
    }
}

fn register_format(name: &str) -> Result<u32, String> {
    let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let format = unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) };
    (format != 0)
        .then_some(format)
        .ok_or_else(|| format!("cannot register clipboard format {name}"))
}

fn marker_bytes(transaction_id: Uuid, payload_sha256: &[u8; 32]) -> Vec<u8> {
    let mut marker = transaction_id.to_string().into_bytes();
    marker.push(b':');
    for byte in payload_sha256 {
        marker.extend_from_slice(format!("{byte:02x}").as_bytes());
    }
    marker.push(0);
    marker
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_registered_hglobal_candidates_are_attempted() {
        assert!(is_hglobal_snapshot_candidate(FORMAT_UNICODETEXT, None));
        assert!(is_hglobal_snapshot_candidate(0xC001, Some("HTML Format")));
        assert!(is_hglobal_snapshot_candidate(
            0xC364,
            Some("Chromium internal source RFH token")
        ));
        assert!(is_hglobal_snapshot_candidate(
            0xC365,
            Some("Chromium internal source URL")
        ));
        assert!(!is_hglobal_snapshot_candidate(0x0200, None));
        assert!(!is_hglobal_snapshot_candidate(0x0300, None));
        assert!(!is_hglobal_snapshot_candidate(0xC366, None));
    }

    #[test]
    fn opaque_registered_data_still_requires_owned_nonempty_bytes() {
        assert!(validate_format(
            0xC364,
            Some("Chromium internal source RFH token"),
            &[1, 2, 3]
        )
        .is_ok());
        assert!(validate_format(0xC364, Some("Chromium internal source RFH token"), &[]).is_err());
    }

    #[test]
    fn unicode_and_dib_validators_reject_truncated_data() {
        assert!(validate_format(FORMAT_UNICODETEXT, None, &[65, 0, 0, 0]).is_ok());
        assert!(validate_format(FORMAT_UNICODETEXT, None, &[65]).is_err());
        assert!(validate_format(FORMAT_DIB, None, &[0; 12]).is_err());
    }

    #[test]
    fn send_input_counts_have_three_distinct_outcomes() {
        let inputs = [keyboard_input(VK_V.0, 0, 0); 4];
        assert_eq!(send_inputs(&inputs, Some(0)), SubmissionState::NotSubmitted);
        assert_eq!(send_inputs(&inputs, Some(4)), SubmissionState::Submitted);
        assert_eq!(send_inputs(&inputs, Some(2)), SubmissionState::Unknown);
    }
}
