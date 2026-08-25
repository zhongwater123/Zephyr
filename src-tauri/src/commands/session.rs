use crate::command_error::{self, CommandError, CommandResult};
use crate::delivery;
use crate::overlay;
use crate::services::AppServices;
use crate::state::VoiceStatePayload;
use crate::{SessionMetrics, SharedRuntime};
use tauri::{State, WebviewWindow};

#[tauri::command]
pub fn get_preinput_payload(
    window: WebviewWindow,
) -> CommandResult<Option<overlay::PreInputPayload>> {
    command_error::require_window(&window, overlay::PREINPUT_LABEL)?;
    Ok(overlay::current_preinput_payload())
}

#[tauri::command]
pub fn get_voice_state(
    window: WebviewWindow,
    runtime: State<'_, SharedRuntime>,
) -> CommandResult<VoiceStatePayload> {
    command_error::require_window(&window, "main")?;
    let runtime = runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?;
    Ok(runtime.voice_state_payload())
}

#[tauri::command]
pub fn list_pending_outputs(
    window: WebviewWindow,
    runtime: State<'_, SharedRuntime>,
) -> CommandResult<Vec<crate::target::PendingOutput>> {
    command_error::require_window(&window, "main")?;
    let mut runtime = runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?;
    Ok(runtime.sessions.pending_outputs.list())
}

#[tauri::command]
pub fn get_session_metrics(
    window: WebviewWindow,
    runtime: State<'_, SharedRuntime>,
) -> CommandResult<Option<SessionMetrics>> {
    command_error::require_window(&window, "main")?;
    let runtime = runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?;
    if let Some(session) = &runtime.sessions.active {
        let queue = session.audio_queue.snapshot();
        return Ok(Some(SessionMetrics {
            session_id: session.session_id,
            audio_packets: queue.packets,
            queue_high_watermark: queue.high_watermark,
            overflow: queue.overflow,
            recording_duration_ms: session.started_at.elapsed().as_millis() as u64,
            cancel_reason: None,
            final_state: "recording".to_string(),
        }));
    }
    Ok(runtime.sessions.last_metrics.clone())
}

#[tauri::command]
pub fn discard_pending_output(
    id: String,
    window: WebviewWindow,
    runtime: State<'_, SharedRuntime>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?
        .sessions
        .pending_outputs
        .remove(&id)
        .ok_or_else(|| CommandError::new("pending_output_not_found", "待处理结果不存在或已过期"))?;
    Ok(())
}

#[tauri::command]
pub fn copy_pending_output(
    id: String,
    window: WebviewWindow,
    runtime: State<'_, SharedRuntime>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    let text = runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?
        .sessions
        .pending_outputs
        .get(&id)
        .map(|record| record.dto.text)
        .ok_or_else(|| CommandError::new("pending_output_not_found", "待处理结果不存在或已过期"))?;
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| CommandError::new("clipboard_unavailable", error.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|error| CommandError::new("clipboard_write_failed", error.to_string()))?;
    runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?
        .sessions
        .pending_outputs
        .remove(&id);
    Ok(())
}

#[tauri::command]
pub async fn deliver_pending_output(
    id: String,
    window: WebviewWindow,
    runtime: State<'_, SharedRuntime>,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    delivery::deliver_pending(id, runtime.inner().clone(), services.inner().clone()).await
}
