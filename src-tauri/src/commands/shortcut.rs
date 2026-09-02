use crate::command_error::{self, CommandError, CommandResult};
use crate::desktop_support::DesktopCapability;
use crate::physical_shortcut::ShortcutBinding;
use crate::services::AppServices;
use crate::shortcut_manager::{
    ShortcutEditOutcome, ShortcutEditSession, ShortcutEditTraceInput, ShortcutManager,
};
use std::sync::Arc;
use tauri::{State, WebviewWindow};

fn manager_error(error: String) -> CommandError {
    CommandError::new("shortcut_edit_failed", error)
}

fn task_error(error: impl ToString) -> CommandError {
    CommandError::new("shortcut_edit_task_failed", error.to_string())
}

#[tauri::command]
pub async fn begin_shortcut_edit(
    trace_id: String,
    expected_revision: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
    services: State<'_, AppServices>,
) -> CommandResult<ShortcutEditSession> {
    command_error::require_window(&window, "main")?;
    services
        .support
        .require(DesktopCapability::GlobalShortcut)?;
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.begin_edit(trace_id, expected_revision))
        .await
        .map_err(task_error)?
        .map_err(manager_error)
}

#[tauri::command]
pub async fn commit_shortcut_edit(
    trace_id: String,
    edit_id: u64,
    expected_revision: u64,
    binding: ShortcutBinding,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
    services: State<'_, AppServices>,
) -> CommandResult<ShortcutEditOutcome> {
    command_error::require_window(&window, "main")?;
    services
        .support
        .require(DesktopCapability::GlobalShortcut)?;
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        manager.commit_edit(trace_id, edit_id, expected_revision, binding)
    })
    .await
    .map_err(task_error)?
    .map_err(manager_error)
}

#[tauri::command]
pub async fn cancel_shortcut_edit(
    trace_id: String,
    edit_id: u64,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
    services: State<'_, AppServices>,
) -> CommandResult<ShortcutEditOutcome> {
    command_error::require_window(&window, "main")?;
    services
        .support
        .require(DesktopCapability::GlobalShortcut)?;
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.cancel_edit(trace_id, edit_id))
        .await
        .map_err(task_error)?
        .map_err(manager_error)
}

#[tauri::command]
pub fn record_shortcut_edit_trace(
    input: ShortcutEditTraceInput,
    window: WebviewWindow,
    manager: State<'_, Arc<ShortcutManager>>,
    services: State<'_, AppServices>,
) -> CommandResult<()> {
    command_error::require_window(&window, "main")?;
    services
        .support
        .require(DesktopCapability::GlobalShortcut)?;
    manager
        .record_trace(input)
        .map_err(|error| CommandError::new("shortcut_trace_invalid", error))
}
