use crate::command_error::{self, CommandError, CommandResult};
use crate::shortcut_lifecycle::ShortcutLifecycleSnapshot;
use crate::shortcut_manager::ShortcutManager;
use std::sync::Arc;
use tauri::{State, WebviewWindow};

#[tauri::command]
pub fn start_shortcut_capture(
    expected_revision: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutLifecycleSnapshot> {
    command_error::require_window(&window, "main")?;
    manager
        .start_capture(expected_revision)
        .map_err(|error| CommandError::new("shortcut_lifecycle_failed", error))
}

#[tauri::command]
pub fn cancel_shortcut_operation(
    operation_id: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutLifecycleSnapshot> {
    command_error::require_window(&window, "main")?;
    manager
        .cancel_operation(operation_id)
        .map_err(|error| CommandError::new("shortcut_lifecycle_failed", error))
}

#[tauri::command]
pub fn undo_last_shortcut_change(
    change_id: u64,
    expected_revision: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutLifecycleSnapshot> {
    command_error::require_window(&window, "main")?;
    manager
        .undo(change_id, expected_revision)
        .map_err(|error| CommandError::new("shortcut_lifecycle_failed", error))
}

#[tauri::command]
pub fn get_shortcut_lifecycle(
    operation_id: Option<u64>,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutLifecycleSnapshot> {
    command_error::require_window(&window, "main")?;
    manager
        .lifecycle(operation_id)
        .map_err(|error| CommandError::new("shortcut_lifecycle_failed", error))
}

#[tauri::command]
pub fn restore_default_shortcut(
    expected_revision: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<ShortcutLifecycleSnapshot> {
    command_error::require_window(&window, "main")?;
    manager
        .restore_default(expected_revision)
        .map_err(|error| CommandError::new("shortcut_lifecycle_failed", error))
}
