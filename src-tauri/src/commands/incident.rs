use crate::command_error::{self, CommandError, CommandResult};
use crate::incident::model::{
    FrontendIncidentInput, IncidentEvent, IncidentHealth, IncidentItem, ReportOptions,
};
use crate::services::AppServices;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{ipc::Response, Manager, State, WebviewWindow};

fn map_error(code: &'static str, error: String) -> CommandError {
    CommandError::new(code, error)
}

#[tauri::command]
pub fn list_incidents(
    limit: u32,
    offset: u32,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<Vec<IncidentItem>> {
    command_error::require_window(&window, "main")?;
    services
        .incidents
        .list(limit, offset)
        .map_err(|error| map_error("incident_read_failed", error))
}

#[tauri::command]
pub fn get_incident_health(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<IncidentHealth> {
    command_error::require_window(&window, "main")?;
    Ok(services.incidents.health())
}

#[tauri::command]
pub fn copy_incident_text(
    id: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    let item = services
        .incidents
        .get(&id)
        .map_err(|error| map_error("incident_read_failed", error))?;
    let text = item
        .final_text
        .or(item.partial_text)
        .ok_or_else(|| CommandError::new("incident_text_unavailable", "没有可恢复的文本"))?;
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| CommandError::new("clipboard_unavailable", error.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|error| CommandError::new("clipboard_write_failed", error.to_string()))
}

#[tauri::command]
pub fn get_incident_audio(
    id: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<Response> {
    command_error::require_window(&window, "main")?;
    services
        .incidents
        .audio_wav(&id)
        .map(Response::new)
        .map_err(|error| map_error("incident_audio_failed", error))
}

#[tauri::command]
pub fn export_incident_report(
    id: String,
    options: ReportOptions,
    app: tauri::AppHandle,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<Response> {
    command_error::require_window(&window, "main")?;
    let log_dir = options
        .include_log_excerpt
        .then(|| app.path().app_log_dir().ok())
        .flatten();
    services
        .incidents
        .report(&id, &options, log_dir.as_deref())
        .map(Response::new)
        .map_err(|error| map_error("incident_report_failed", error))
}

#[tauri::command]
pub fn delete_incident(
    id: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    services
        .incidents
        .delete(&id)
        .map_err(|error| map_error("incident_delete_failed", error))
}

#[tauri::command]
pub fn set_incident_pinned(
    id: String,
    pinned: bool,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    services
        .incidents
        .set_pinned(&id, pinned)
        .map_err(|error| map_error("incident_update_failed", error))
}

#[tauri::command]
pub fn record_frontend_incident(
    input: FrontendIncidentInput,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    if window.label() != "main" && window.label() != "preinput" {
        return Err(CommandError::new(
            "window_not_allowed",
            "窗口无权记录前端异常",
        ));
    }
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    let mut last = LAST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| CommandError::new("frontend_incident_limiter_failed", "异常限流器不可用"))?;
    if last
        .map(|value| value.elapsed() < Duration::from_secs(2))
        .unwrap_or(false)
    {
        return Ok(());
    }
    *last = Some(Instant::now());
    let id = uuid::Uuid::new_v4().to_string();
    let event = IncidentEvent::FrontendFailure {
        attempt_id: id,
        source: input.source.chars().take(64).collect(),
        code: input.code.chars().take(96).collect(),
        message: redact(&input.message).chars().take(1024).collect(),
        stack: input
            .stack
            .map(|value| redact(&value).chars().take(4096).collect()),
        occurred_at_utc_ms: chrono::Utc::now().timestamp_millis(),
    };
    let _ = services.incidents.sink().try_emit(event);
    Ok(())
}

fn redact(value: &str) -> String {
    crate::incident::redact_sensitive(value)
}
#[tauri::command]
pub fn save_incident_audio(
    id: String,
    path: std::path::PathBuf,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    validate_export_path(&path, "wav")?;
    let bytes = services
        .incidents
        .audio_wav(&id)
        .map_err(|error| map_error("incident_audio_failed", error))?;
    std::fs::write(&path, bytes)
        .map_err(|error| CommandError::new("incident_export_write_failed", error.to_string()))
}

#[tauri::command]
pub fn save_incident_report(
    id: String,
    options: ReportOptions,
    path: std::path::PathBuf,
    app: tauri::AppHandle,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    validate_export_path(&path, "zip")?;
    let log_dir = options
        .include_log_excerpt
        .then(|| app.path().app_log_dir().ok())
        .flatten();
    let bytes = services
        .incidents
        .report(&id, &options, log_dir.as_deref())
        .map_err(|error| map_error("incident_report_failed", error))?;
    std::fs::write(&path, bytes)
        .map_err(|error| CommandError::new("incident_export_write_failed", error.to_string()))
}

fn validate_export_path(path: &std::path::Path, extension: &str) -> CommandResult<()> {
    if !path.is_absolute()
        || path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case(extension))
            != Some(true)
        || !path.parent().map(|parent| parent.is_dir()).unwrap_or(false)
    {
        return Err(CommandError::new(
            "incident_export_path_invalid",
            "导出路径必须是已存在目录下的 WAV 或 ZIP 文件",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod current_contract_tests {
    use super::*;

    #[test]
    fn frontend_error_redaction_removes_credential_values() {
        let redacted = redact(concat!(
            "Authorization: Bearer browser-secret ",
            "api_key = second-secret ",
            "C:\\Users\\Alice\\app.tsx",
        ));
        for secret in ["browser-secret", "second-secret", "Alice"] {
            assert!(
                !redacted.contains(secret),
                "frontend exception redaction leaked {secret}: {redacted}"
            );
        }
    }

    #[test]
    fn export_path_rejects_relative_and_mismatched_extensions() {
        assert!(validate_export_path(std::path::Path::new("incident.zip"), "zip").is_err());
        let temp = tempfile::tempdir().unwrap();
        let wrong_extension = temp.path().join("incident.txt");
        assert!(validate_export_path(&wrong_extension, "zip").is_err());
        let valid = temp.path().join("incident.zip");
        assert!(validate_export_path(&valid, "zip").is_ok());
    }
}
