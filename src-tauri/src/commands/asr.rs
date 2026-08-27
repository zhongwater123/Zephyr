use crate::command_error::{self, CommandError, CommandResult};
use crate::config::ConfigValue;
use crate::provider_model::AsrOptionPool;
use crate::services::{AppServices, ConfigServiceError};
use tauri::{State, WebviewWindow};

fn conflict(pool: AsrOptionPool) -> CommandError {
    CommandError::with_details(
        "config_conflict",
        "识别选项已被其他操作修改，已重新加载",
        serde_json::json!({
            "currentRevision": pool.revision,
            "currentPool": pool,
        }),
    )
}

#[tauri::command]
pub fn get_asr_option_pool(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AsrOptionPool> {
    command_error::require_window(&window, "main")?;
    services
        .provider
        .option_pool(&services.config.snapshot())
        .map_err(|error| CommandError::new("invalid_asr_configuration", error.to_string()))
}

#[tauri::command]
pub fn set_asr_option(
    option_id: String,
    value: ConfigValue,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AsrOptionPool> {
    command_error::require_window(&window, "main")?;
    let mut next = services.config.snapshot();
    if next.asr.revision != expected_revision {
        return Err(conflict(services.provider.option_pool(&next).map_err(
            |error| CommandError::new("invalid_asr_configuration", error.to_string()),
        )?));
    }
    services
        .provider
        .model()
        .set_option(&mut next.asr, &option_id, value)
        .map_err(|error| CommandError::new("invalid_asr_option", error.to_string()))?;
    next.asr.revision = next.asr.revision.saturating_add(1);
    next.revision = next.revision.saturating_add(1);
    let app_revision = next.revision.saturating_sub(1);
    let saved = match services.config.commit_config(app_revision, next) {
        Ok(saved) => saved,
        Err(ConfigServiceError::Conflict(current)) => {
            return Err(conflict(services.provider.option_pool(&current).map_err(
                |error| CommandError::new("invalid_asr_configuration", error.to_string()),
            )?))
        }
        Err(ConfigServiceError::Storage(error)) => {
            return Err(CommandError::new("config_write_failed", error.to_string()))
        }
    };
    services
        .provider
        .option_pool(&saved)
        .map_err(|error| CommandError::new("invalid_asr_configuration", error.to_string()))
}
