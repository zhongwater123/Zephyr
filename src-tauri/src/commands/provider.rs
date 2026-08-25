use crate::command_error::{self, CommandError, CommandResult};
use crate::services::AppServices;
use tauri::{State, WebviewWindow};

#[tauri::command]
pub async fn test_provider(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<String> {
    command_error::require_window(&window, "main")?;
    let config = services.config.snapshot();
    let adapter = services
        .provider
        .build_adapter(&config)
        .map_err(|message| CommandError::new("provider_not_ready", message))?;
    adapter
        .probe_connection()
        .await
        .map_err(|error| CommandError::new(error.cancel_reason(), error.user_message()))
}
