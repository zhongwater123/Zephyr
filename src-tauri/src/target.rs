use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_OUTPUT_CHARACTERS: usize = 8_000;
pub const MAX_PENDING_OUTPUTS: usize = 5;
pub const PENDING_OUTPUT_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetWindowIdentity {
    pub hwnd: isize,
    pub process_id: u32,
    pub process_started_at: u64,
    pub executable_name: String,
    pub executable_path: String,
    pub window_title: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOutput {
    pub id: String,
    pub session_id: u64,
    pub text: String,
    pub executable_name: String,
    pub window_title: Option<String>,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub target_available: bool,
    pub reason_code: String,
    pub reason_message: String,
    pub delivery_certainty: DeliveryCertainty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryCertainty {
    Retryable,
    MayHaveBeenSubmitted,
}

#[derive(Clone, Debug)]
pub struct PendingOutputRecord {
    pub dto: PendingOutput,
    pub target: TargetWindowIdentity,
    expires_at: Instant,
}

#[derive(Debug, Default)]
pub struct PendingOutputStore {
    entries: VecDeque<PendingOutputRecord>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PendingOutputError {
    Full,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputValidationError {
    Empty,
    TooLong,
    ForbiddenCharacter { index: usize, codepoint: u32 },
}

/// Normalizes and validates text for SmartDictation's atomic-paste path.
///
/// Legacy delivery intentionally keeps using validate_output_text, whose
/// historical contract rejects every control character, including newlines.
/// SmartDictation permits LF as the sole control character after normalizing
/// CRLF and bare CR to LF.
pub fn normalize_smart_output_text(text: &str) -> Result<String, OutputValidationError> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    validate_output_text_inner(&normalized, true)?;
    Ok(normalized)
}

/// Returns true for targets where pasting a multiline payload can execute
/// commands according to the captured executable identity.
pub fn is_multiline_unsafe_target(executable_name: &str) -> bool {
    const UNSAFE_EXECUTABLES: &[&str] = &[
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "windowsterminal.exe",
        "openconsole.exe",
        "conhost.exe",
    ];

    UNSAFE_EXECUTABLES
        .iter()
        .any(|candidate| executable_name.eq_ignore_ascii_case(candidate))
}

impl PendingOutputStore {
    pub fn is_full(&mut self) -> bool {
        self.purge_expired();
        self.entries.len() >= MAX_PENDING_OUTPUTS
    }

    pub fn push(
        &mut self,
        session_id: u64,
        text: String,
        target: TargetWindowIdentity,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
    ) -> Result<PendingOutput, PendingOutputError> {
        self.push_with_certainty_and_ttl(
            session_id,
            text,
            target,
            reason_code,
            reason_message,
            DeliveryCertainty::Retryable,
            PENDING_OUTPUT_TTL,
        )
    }

    pub fn push_with_certainty(
        &mut self,
        session_id: u64,
        text: String,
        target: TargetWindowIdentity,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
        certainty: DeliveryCertainty,
    ) -> Result<PendingOutput, PendingOutputError> {
        self.push_with_certainty_and_ttl(
            session_id,
            text,
            target,
            reason_code,
            reason_message,
            certainty,
            PENDING_OUTPUT_TTL,
        )
    }

    fn push_with_certainty_and_ttl(
        &mut self,
        session_id: u64,
        text: String,
        target: TargetWindowIdentity,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
        certainty: DeliveryCertainty,
        ttl: Duration,
    ) -> Result<PendingOutput, PendingOutputError> {
        self.purge_expired();
        if self.entries.len() >= MAX_PENDING_OUTPUTS {
            return Err(PendingOutputError::Full);
        }

        let now = Instant::now();
        let created_at_unix_ms = unix_time_ms();
        let ttl_ms = ttl.as_millis() as u64;
        let dto = PendingOutput {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            text,
            executable_name: target.executable_name.clone(),
            window_title: target.window_title.clone(),
            created_at_unix_ms,
            expires_at_unix_ms: created_at_unix_ms.saturating_add(ttl_ms),
            target_available: validate_target_exists(&target).is_ok(),
            reason_code: reason_code.into(),
            reason_message: reason_message.into(),
            delivery_certainty: certainty,
        };

        self.entries.push_back(PendingOutputRecord {
            dto: dto.clone(),
            target,
            expires_at: now + ttl,
        });
        Ok(dto)
    }

    pub fn list(&mut self) -> Vec<PendingOutput> {
        self.purge_expired();
        self.entries
            .iter()
            .map(|record| {
                let mut dto = record.dto.clone();
                dto.target_available = validate_target_exists(&record.target).is_ok();
                dto
            })
            .collect()
    }

    pub fn get(&mut self, id: &str) -> Option<PendingOutputRecord> {
        self.purge_expired();
        self.entries
            .iter()
            .find(|record| record.dto.id == id)
            .cloned()
    }

    pub fn remove(&mut self, id: &str) -> Option<PendingOutputRecord> {
        self.purge_expired();
        let index = self.entries.iter().position(|record| record.dto.id == id)?;
        self.entries.remove(index)
    }

    pub fn update_delivery_failure(
        &mut self,
        id: &str,
        certainty: DeliveryCertainty,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
    ) -> bool {
        self.purge_expired();
        let Some(record) = self.entries.iter_mut().find(|record| record.dto.id == id) else {
            return false;
        };
        record.dto.delivery_certainty = certainty;
        record.dto.reason_code = reason_code.into();
        record.dto.reason_message = reason_message.into();
        true
    }

    fn purge_expired(&mut self) {
        let now = Instant::now();
        self.entries.retain(|record| record.expires_at > now);
    }
}

pub fn validate_output_text(text: &str) -> Result<(), OutputValidationError> {
    validate_output_text_inner(text, false)
}

fn validate_output_text_inner(text: &str, allow_lf: bool) -> Result<(), OutputValidationError> {
    if text.is_empty() {
        return Err(OutputValidationError::Empty);
    }

    let mut count = 0usize;
    for (index, character) in text.chars().enumerate() {
        count += 1;
        if count > MAX_OUTPUT_CHARACTERS {
            return Err(OutputValidationError::TooLong);
        }

        let codepoint = character as u32;
        let is_bidi_override_or_isolate = matches!(codepoint, 0x202A..=0x202E | 0x2066..=0x2069);
        if (character.is_control() && !(allow_lf && character == '\n'))
            || is_bidi_override_or_isolate
        {
            return Err(OutputValidationError::ForbiddenCharacter { index, codepoint });
        }
    }

    Ok(())
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(target_os = "windows")]
fn process_started_at(process_id: u32) -> Result<u64, String> {
    use windows::Win32::Foundation::{CloseHandle, FILETIME};
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| format!("无法打开目标进程: {error}"))?;
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let result =
        unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) };
    unsafe {
        let _ = CloseHandle(process);
    }
    result.map_err(|error| format!("无法读取目标进程创建时间: {error}"))?;
    Ok(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
}

#[cfg(target_os = "windows")]
fn executable_path(process_id: u32) -> Result<String, String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| format!("无法打开目标进程: {error}"))?;
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    unsafe {
        let _ = CloseHandle(process);
    }
    result.map_err(|error| format!("无法读取目标程序路径: {error}"))?;
    Ok(String::from_utf16_lossy(&buffer[..length as usize]))
}

fn executable_name_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&path)
        .to_string()
}

#[cfg(target_os = "windows")]
fn window_title(hwnd: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    let length = unsafe { GetWindowTextLengthW(hwnd) };
    if length <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    (copied > 0).then(|| String::from_utf16_lossy(&buffer[..copied as usize]))
}

#[cfg(target_os = "windows")]
pub fn capture_foreground_target() -> Result<TargetWindowIdentity, String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
    };

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() || !unsafe { IsWindow(hwnd).as_bool() } {
        return Err("当前没有可用的前台窗口".to_string());
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id == 0 {
        return Err("无法识别目标窗口所属进程".to_string());
    }

    let executable_path = executable_path(process_id)?;
    Ok(TargetWindowIdentity {
        hwnd: hwnd.0 as isize,
        process_id,
        process_started_at: process_started_at(process_id)?,
        executable_name: executable_name_from_path(&executable_path),
        executable_path,
        window_title: window_title(hwnd),
    })
}

#[cfg(target_os = "windows")]
pub fn validate_target_exists(target: &TargetWindowIdentity) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, IsWindow};

    let hwnd = HWND(target.hwnd as *mut _);
    if !unsafe { IsWindow(hwnd).as_bool() } {
        return Err("原目标窗口已经关闭".to_string());
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    if process_id != target.process_id {
        return Err("原目标窗口的进程身份已经变化".to_string());
    }
    if process_started_at(process_id)? != target.process_started_at {
        return Err("原目标进程已经被替换".to_string());
    }
    let current_path = executable_path(process_id)?;
    if !current_path.eq_ignore_ascii_case(&target.executable_path)
        || !executable_name_from_path(&current_path).eq_ignore_ascii_case(&target.executable_name)
    {
        return Err("原目标程序身份已经变化".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn validate_foreground_target(target: &TargetWindowIdentity) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    validate_target_exists(target)?;
    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0 as isize != target.hwnd {
        return Err("识别期间前台窗口已经变化".to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn activate_target(target: &TargetWindowIdentity) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    validate_target_exists(target)?;
    let hwnd = HWND(target.hwnd as *mut _);
    if !unsafe { SetForegroundWindow(hwnd).as_bool() } {
        return Err("Windows 拒绝激活原目标窗口".to_string());
    }
    validate_foreground_target(target)
}

#[cfg(not(target_os = "windows"))]
pub fn capture_foreground_target() -> Result<TargetWindowIdentity, String> {
    Err("目标窗口身份仅支持 Windows".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn validate_target_exists(_target: &TargetWindowIdentity) -> Result<(), String> {
    Err("目标窗口身份仅支持 Windows".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn validate_foreground_target(_target: &TargetWindowIdentity) -> Result<(), String> {
    Err("目标窗口身份仅支持 Windows".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn activate_target(_target: &TargetWindowIdentity) -> Result<(), String> {
    Err("目标窗口身份仅支持 Windows".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_safe_text() {
        assert_eq!(validate_output_text("你好，world 🙂"), Ok(()));
    }

    #[test]
    fn rejects_control_and_bidi_characters() {
        assert!(matches!(
            validate_output_text("line\nnext"),
            Err(OutputValidationError::ForbiddenCharacter { .. })
        ));
        assert!(matches!(
            validate_output_text("safe\u{202e}unsafe"),
            Err(OutputValidationError::ForbiddenCharacter { .. })
        ));
    }

    #[test]
    fn rejects_more_than_character_limit() {
        let text = "字".repeat(MAX_OUTPUT_CHARACTERS + 1);
        assert_eq!(
            validate_output_text(&text),
            Err(OutputValidationError::TooLong)
        );
    }

    #[test]
    fn process_creation_time_is_part_of_window_identity() {
        let first = TargetWindowIdentity {
            hwnd: 10,
            process_id: 20,
            process_started_at: 30,
            executable_name: "app.exe".to_string(),
            executable_path: r"C:\\Apps\\app.exe".to_string(),
            window_title: None,
        };
        let mut reused = first.clone();
        reused.process_started_at = 31;
        assert_ne!(first, reused);
    }

    #[test]
    fn pending_store_never_overwrites_the_sixth_result() {
        let target = TargetWindowIdentity {
            hwnd: 0,
            process_id: 0,
            process_started_at: 0,
            executable_name: "app.exe".to_string(),
            executable_path: r"C:\\Apps\\app.exe".to_string(),
            window_title: None,
        };
        let mut store = PendingOutputStore::default();
        for index in 0..MAX_PENDING_OUTPUTS {
            store
                .push(
                    index as u64,
                    format!("result {index}"),
                    target.clone(),
                    "test",
                    "test",
                )
                .unwrap();
        }
        assert!(matches!(
            store.push(9, "sixth".to_string(), target, "test", "test"),
            Err(PendingOutputError::Full)
        ));
        assert_eq!(store.list().len(), MAX_PENDING_OUTPUTS);
        assert!(store.list().iter().all(|entry| entry.text != "sixth"));
    }

    #[test]
    fn pending_store_purges_expired_results() {
        let target = TargetWindowIdentity {
            hwnd: 0,
            process_id: 0,
            process_started_at: 0,
            executable_name: "app.exe".to_string(),
            executable_path: r"C:\\Apps\\app.exe".to_string(),
            window_title: None,
        };
        let mut store = PendingOutputStore::default();
        store
            .push_with_certainty_and_ttl(
                1,
                "expired".to_string(),
                target,
                "test",
                "test",
                DeliveryCertainty::Retryable,
                Duration::ZERO,
            )
            .unwrap();
        assert!(store.list().is_empty());
    }

    #[test]
    fn smart_output_normalizes_crlf_and_bare_cr_to_lf() {
        assert_eq!(
            normalize_smart_output_text("first\r\nsecond\rthird").unwrap(),
            "first\nsecond\nthird"
        );
    }

    #[test]
    fn smart_output_allows_only_lf_among_control_characters() {
        assert_eq!(
            normalize_smart_output_text("line\nnext").unwrap(),
            "line\nnext"
        );
        for text in ["column\tnext", "safe\0unsafe", "safe\u{2066}unsafe"] {
            assert!(matches!(
                normalize_smart_output_text(text),
                Err(OutputValidationError::ForbiddenCharacter { .. })
            ));
        }
    }

    #[test]
    fn smart_output_counts_characters_after_normalization() {
        let exactly_limit = format!("{}\r\n", "x".repeat(MAX_OUTPUT_CHARACTERS - 1));
        assert_eq!(
            normalize_smart_output_text(&exactly_limit)
                .unwrap()
                .chars()
                .count(),
            MAX_OUTPUT_CHARACTERS
        );
        let over_limit = format!("{}\r\n", "x".repeat(MAX_OUTPUT_CHARACTERS));
        assert_eq!(
            normalize_smart_output_text(&over_limit),
            Err(OutputValidationError::TooLong)
        );
    }

    #[test]
    fn multiline_unsafe_targets_are_case_insensitive() {
        for executable in [
            "cmd.exe",
            "PowerShell.exe",
            "pwsh.exe",
            "WindowsTerminal.exe",
            "OpenConsole.exe",
            "conhost.exe",
        ] {
            assert!(is_multiline_unsafe_target(executable), "{executable}");
        }
        assert!(!is_multiline_unsafe_target("notepad.exe"));
        assert!(!is_multiline_unsafe_target("Code.exe"));
        assert!(!is_multiline_unsafe_target("Cursor.exe"));
    }
}
