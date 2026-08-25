use crate::command_error::{self, CommandError, CommandResult};
use crate::history::HistoryItem;
use crate::services::AppServices;
use tauri::{State, WebviewWindow};

#[tauri::command]
pub fn list_history(
    query: Option<String>,
    limit: i64,
    offset: i64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<Vec<HistoryItem>> {
    command_error::require_window(&window, "main")?;
    services
        .history
        .list(query, limit, offset)
        .map_err(|error| CommandError::new("history_read_failed", error.to_string()))
}

#[tauri::command]
pub fn update_history(
    id: String,
    text: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    services
        .history
        .update(&id, &text)
        .map_err(|error| CommandError::new("history_update_failed", error.to_string()))
}

#[tauri::command]
pub fn delete_history(
    id: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    services
        .history
        .delete(&id)
        .map_err(|error| CommandError::new("history_delete_failed", error.to_string()))
}

#[tauri::command]
pub fn clear_history(window: WebviewWindow, services: State<'_, AppServices>) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    services
        .history
        .clear()
        .map_err(|error| CommandError::new("history_clear_failed", error.to_string()))
}

#[tauri::command]
pub fn copy_history_text(
    id: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    let text = services
        .history
        .get_text(&id)
        .map_err(|error| CommandError::new("history_read_failed", error.to_string()))?;
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|error| CommandError::new("clipboard_unavailable", error.to_string()))?;
    clipboard
        .set_text(text)
        .map_err(|error| CommandError::new("clipboard_write_failed", error.to_string()))
}
