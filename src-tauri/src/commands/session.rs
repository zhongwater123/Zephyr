use crate::command_error::{self, CommandError, CommandResult};
use crate::overlay;
use crate::pending_output_service::{PendingOutputService, PendingOutputServiceError};
use crate::state::VoiceStatePayload;
use crate::voice_controller::VoiceSessionHandle;
use crate::SessionMetrics;
use std::sync::Arc;
use tauri::{State, WebviewWindow};

fn map_pending_error(error: PendingOutputServiceError) -> CommandError {
    match error {
        PendingOutputServiceError::Full => {
            CommandError::new("pending_output_full", "待处理结果已满")
        }
        PendingOutputServiceError::NotFound => {
            CommandError::new("pending_output_not_found", "待处理结果不存在或已过期")
        }
        PendingOutputServiceError::Busy => {
            CommandError::new("pending_output_busy", "待处理结果正在执行其他操作")
        }
    }
}

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
    voice: State<'_, VoiceSessionHandle>,
) -> CommandResult<VoiceStatePayload> {
    command_error::require_window(&window, "main")?;
    Ok(voice.status_snapshot().payload)
}

#[tauri::command]
pub fn list_pending_outputs(
    window: WebviewWindow,
    pending: State<'_, Arc<PendingOutputService>>,
) -> CommandResult<Vec<crate::target::PendingOutput>> {
    command_error::require_window(&window, "main")?;
    Ok(pending.list())
}

#[tauri::command]
pub async fn get_session_metrics(
    window: WebviewWindow,
    voice: State<'_, VoiceSessionHandle>,
) -> CommandResult<Option<SessionMetrics>> {
    command_error::require_window(&window, "main")?;
    voice
        .metrics()
        .await
        .map_err(|error| CommandError::new("voice_control_unavailable", format!("{error:?}")))
}

#[tauri::command]
pub fn discard_pending_output(
    id: String,
    window: WebviewWindow,
    pending: State<'_, Arc<PendingOutputService>>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    pending.discard(&id).map_err(map_pending_error)?;
    Ok(())
}

#[tauri::command]
pub fn copy_pending_output(
    id: String,
    window: WebviewWindow,
    pending: State<'_, Arc<PendingOutputService>>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    let record = pending.reserve(&id).map_err(map_pending_error)?;
    let mut clipboard = arboard::Clipboard::new().map_err(|error| {
        pending.release(&id);
        CommandError::new("clipboard_unavailable", error.to_string())
    })?;
    if let Err(error) = clipboard.set_text(record.dto.text) {
        pending.release(&id);
        return Err(CommandError::new(
            "clipboard_write_failed",
            error.to_string(),
        ));
    }
    pending.complete(&id).map_err(map_pending_error)?;
    Ok(())
}

#[tauri::command]
pub async fn deliver_pending_output(
    id: String,
    window: WebviewWindow,
    voice: State<'_, VoiceSessionHandle>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    voice.deliver_pending(id).await
}
