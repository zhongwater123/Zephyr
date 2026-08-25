use crate::command_error::{self, CommandError, CommandResult};
use crate::config::AppConfig;
use crate::shortcut_manager::{ShortcutCaptureSession, ShortcutManager, ShortcutRuntimeStatus};
use std::sync::Arc;
use tauri::{State, WebviewWindow};

#[tauri::command]
pub fn start_shortcut_capture(
    expected_revision: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutCaptureSession> {
    command_error::require_window(&window, "main")?;
    manager
        .start_capture(expected_revision)
        .map_err(|error| CommandError::new("shortcut_capture_failed", error))
}

#[tauri::command]
pub fn cancel_shortcut_capture(
    capture_id: Option<u64>,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    manager
        .cancel_capture(capture_id)
        .map_err(|error| CommandError::new("shortcut_cancel_failed", error))
}

#[tauri::command]
pub fn undo_last_shortcut_change(
    change_id: u64,
    expected_revision: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    manager
        .undo(change_id, expected_revision)
        .map_err(|error| CommandError::new("shortcut_undo_failed", error))
}

#[tauri::command]
pub fn get_shortcut_status(
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutRuntimeStatus> {
    command_error::require_window(&window, "main")?;
    Ok(manager.status())
}
