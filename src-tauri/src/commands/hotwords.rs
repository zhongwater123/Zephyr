use crate::command_error::{self, CommandError, CommandResult};
use crate::config::AppConfig;
use crate::hotwords::{HotwordSettingsInput, HotwordState};
use crate::services::{AppServices, ConfigServiceError};
use tauri::{State, WebviewWindow};

fn state(services: &State<'_, AppServices>) -> CommandResult<HotwordState> {
    services
        .hotword_state()
        .map_err(|error| CommandError::new("hotword_state_failed", error.to_string()))
}

fn require_revision(config: &AppConfig, expected_revision: u64) -> CommandResult<()> {
    if config.revision == expected_revision {
        return Ok(());
    }
    Err(CommandError::with_details(
        "config_conflict",
        "配置已被其他操作修改，请重新确认后再保存",
        serde_json::json!({
            "currentRevision": config.revision,
            "currentConfig": config,
        }),
    ))
}

fn map_config_error(error: ConfigServiceError) -> CommandError {
    match error {
        ConfigServiceError::Conflict(current) => CommandError::with_details(
            "config_conflict",
            "配置已被其他操作修改，请重新确认后再保存",
            serde_json::json!({
                "currentRevision": current.revision,
                "currentConfig": current,
            }),
        ),
        ConfigServiceError::Storage(error) => {
            CommandError::new("config_write_failed", error.to_string())
        }
    }
}

#[tauri::command]
pub fn get_hotword_state(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    state(&services)
}

#[tauri::command]
pub fn save_hotword_settings(
    settings: HotwordSettingsInput,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    let current = services.config.snapshot();
    require_revision(&current, expected_revision)?;
    let mut next = current;
    next.hotwords_enabled = settings.hotwords_enabled;
    next.hotword_agent_enabled = settings.hotword_agent_enabled;
    next.hotword_agent_base_url = settings.hotword_agent_base_url;
    next.hotword_agent_model = settings.hotword_agent_model;
    next.revision = next.revision.saturating_add(1);
    services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)?;
    state(&services)
}

#[tauri::command]
pub fn save_manual_hotwords(
    words: Vec<String>,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .save_manual(words)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn add_hotword(
    word: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .add(&word)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn update_hotword(
    old_word: String,
    new_word: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .update(&old_word, &new_word)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn delete_hotword(
    word: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .delete(&word)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub async fn organize_hotwords_now(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotword_agent
        .organize(services.config.snapshot(), true)
        .await
        .map_err(|error| CommandError::new("hotword_organize_failed", error.to_string()))
}

#[tauri::command]
pub async fn test_hotword_agent(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<String> {
    command_error::require_window(&window, "main")?;
    services
        .hotword_agent
        .test_connection(services.config.snapshot())
        .await
        .map_err(|error| CommandError::new("hotword_agent_request_failed", error.to_string()))
}

#[tauri::command]
pub fn delete_agent_hotword(
    word: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .delete_agent(&word)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn promote_agent_hotword(
    word: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .promote_agent(&word)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn update_profile_context(
    text: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .update_profile(&text)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn update_app_context(
    app_name: String,
    context: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .update_app(&app_name, &context)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}

#[tauri::command]
pub fn delete_app_context(
    app_name: String,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<HotwordState> {
    command_error::require_window(&window, "main")?;
    services
        .hotwords
        .delete_app(&app_name)
        .map_err(|error| CommandError::new("hotword_update_failed", error.to_string()))?;
    state(&services)
}
