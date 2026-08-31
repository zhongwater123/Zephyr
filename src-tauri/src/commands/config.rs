use crate::command_error::{self, CommandError, CommandResult};
use crate::config::{
    self, AppConfig, ConfigRecovery, EndpointPurpose, InjectionOverride, InjectionStrategy,
    ShortcutTriggerMode, TrustedEndpoint,
};
use crate::services::{AppServices, ConfigServiceError};
use crate::shortcut_manager::ShortcutManager;
use crate::voice_controller::VoiceSessionHandle;
use crate::voice_input_service::{VoiceControlService, VoiceControlServiceError};
use serde::Serialize;
use std::sync::Arc;
use tauri::{State, WebviewWindow};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConfigStatus {
    pub(crate) provider_ready: bool,
    pub(crate) provider_message: String,
    pub(crate) recovery_warning: Option<String>,
}

pub(crate) fn require_revision(config: &AppConfig, expected_revision: u64) -> CommandResult<()> {
    if config.revision == expected_revision {
        return Ok(());
    }
    Err(CommandError::with_details(
        "config_conflict",
        "配置已被其他操作更新，请刷新后重试",
        serde_json::json!({
            "expectedRevision": expected_revision,
            "currentRevision": config.revision,
            "currentConfig": config,
        }),
    ))
}

fn history_enabled_config(mut current: AppConfig, enabled: bool) -> AppConfig {
    current.history_enabled = enabled;
    current.revision = current.revision.saturating_add(1);
    current
}

fn shortcut_trigger_mode_config(mut current: AppConfig, mode: ShortcutTriggerMode) -> AppConfig {
    current.shortcut_trigger_mode = mode;
    current.schema_version = config::CURRENT_SCHEMA_VERSION;
    current.revision = current.revision.saturating_add(1);
    current
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

fn map_voice_input_error(error: VoiceControlServiceError) -> CommandError {
    match error {
        VoiceControlServiceError::Config(error) => map_config_error(error),
        VoiceControlServiceError::NativeConfirmationRequired => CommandError::new(
            "native_confirmation_required",
            "新增剪贴板兼容应用必须通过专用的 Windows 原生确认流程",
        ),
        VoiceControlServiceError::Reconciliation {
            committed_revision,
            message,
        } => CommandError::with_details(
            "voice_reconciliation_failed",
            message,
            serde_json::json!({ "committedRevision": committed_revision }),
        ),
    }
}

pub(crate) fn status(
    config: &AppConfig,
    recovery: ConfigRecovery,
    provider: &crate::services::ProviderService,
) -> ConfigStatus {
    let recovery_warning = match recovery {
        ConfigRecovery::None => None,
        ConfigRecovery::Backup => Some("主配置损坏，已从最后一份有效备份恢复。".to_string()),
        ConfigRecovery::DisabledDefaults => {
            Some("主配置和备份均无效，已以禁用状态的安全默认配置启动。".to_string())
        }
    };
    match provider.build_adapter(config) {
        Ok(_) => ConfigStatus {
            provider_ready: true,
            provider_message: "火山引擎识别服务已由部署环境配置。".to_string(),
            recovery_warning,
        },
        Err(message) => ConfigStatus {
            provider_ready: false,
            provider_message: message,
            recovery_warning,
        },
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_conflict_returns_current_config() {
        let config = AppConfig {
            revision: 5,
            ..AppConfig::default()
        };
        let error = require_revision(&config, 4).unwrap_err();
        assert_eq!(error.code, "config_conflict");
        assert_eq!(error.details.unwrap()["currentRevision"], 5);
    }

    #[test]
    fn history_toggle_changes_only_the_flag_and_revision() {
        let current = AppConfig {
            revision: 8,
            history_enabled: true,
            ..AppConfig::default()
        };
        let next = history_enabled_config(current.clone(), false);
        assert!(!next.history_enabled);
        assert_eq!(next.revision, 9);
        assert_eq!(next.shortcut, current.shortcut);
        assert_eq!(next.asr, current.asr);
    }

    #[test]
    fn trigger_mode_change_preserves_disabled_state_and_advances_revision() {
        let current = AppConfig {
            revision: 8,
            enabled: false,
            shortcut_trigger_mode: ShortcutTriggerMode::Hold,
            ..AppConfig::default()
        };

        let next = shortcut_trigger_mode_config(current, ShortcutTriggerMode::Toggle);

        assert_eq!(next.revision, 9);
        assert!(!next.enabled);
        assert_eq!(next.shortcut_trigger_mode, ShortcutTriggerMode::Toggle);
        assert_eq!(next.schema_version, config::CURRENT_SCHEMA_VERSION);
    }
}

#[tauri::command]
pub fn get_config(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    Ok(services.config.snapshot())
}

#[tauri::command]
pub fn get_config_status(
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<ConfigStatus> {
    command_error::require_window(&window, "main")?;
    Ok(status(
        &services.config.snapshot(),
        services.config.recovery(),
        services.provider.as_ref(),
    ))
}

#[tauri::command]
pub fn authorize_endpoint(
    endpoint: String,
    purpose: EndpointPurpose,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    if purpose == EndpointPurpose::Asr {
        return Err(CommandError::new(
            "deployment_managed",
            "识别服务地址由部署环境管理",
        ));
    }
    let origin = config::normalize_origin(&endpoint).map_err(CommandError::from)?;
    let current = services.config.snapshot();
    require_revision(&current, expected_revision)?;
    if current.is_endpoint_trusted(&origin, purpose.clone()) {
        return Ok(current);
    }
    if !services
        .confirmations
        .authorize_endpoint(&window, &origin, &purpose)?
    {
        return Err(CommandError::new(
            "endpoint_authorization_denied",
            "未授权向该主机发送凭据",
        ));
    }
    let mut next = current;
    next.trusted_endpoints
        .push(TrustedEndpoint { origin, purpose });
    next.revision = next.revision.saturating_add(1);
    let next = services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)?;
    Ok(next)
}

#[tauri::command]
pub fn revoke_endpoint(
    endpoint: String,
    purpose: EndpointPurpose,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    if purpose == EndpointPurpose::Asr {
        return Err(CommandError::new(
            "deployment_managed",
            "识别服务地址由部署环境管理",
        ));
    }
    let origin = config::normalize_origin(&endpoint).map_err(CommandError::from)?;
    let mut next = services.config.snapshot();
    require_revision(&next, expected_revision)?;
    let before = next.trusted_endpoints.len();
    next.trusted_endpoints
        .retain(|entry| !(entry.origin == origin && entry.purpose == purpose));
    if next.trusted_endpoints.len() == before || next.is_endpoint_trusted(&origin, purpose.clone())
    {
        return Err(CommandError::new(
            "endpoint_not_revocable",
            "官方 endpoint 无需也不能撤销，或该授权不存在",
        ));
    }
    next.revision = next.revision.saturating_add(1);
    let next = services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)?;
    Ok(next)
}

#[tauri::command]
pub fn set_clipboard_compatibility(
    executable_name: String,
    enabled: bool,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    let executable_name = executable_name.trim();
    if executable_name.is_empty()
        || executable_name.contains(['/', '\\'])
        || !executable_name.to_ascii_lowercase().ends_with(".exe")
    {
        return Err(CommandError::new(
            "invalid_executable_name",
            "请输入不含路径的 Windows 可执行文件名，例如 legacy.exe",
        ));
    }
    if enabled {
        return Err(CommandError::new(
            "clipboard_compatibility_temporarily_unavailable",
            "剪贴板兼容模式正在安全升级，暂时不能新增兼容应用",
        ));
    }
    if enabled
        && !services
            .confirmations
            .enable_clipboard_compatibility(&window, executable_name)?
    {
        return Err(CommandError::new(
            "clipboard_compatibility_denied",
            "未启用剪贴板兼容模式",
        ));
    }
    let mut next = services.config.snapshot();
    require_revision(&next, expected_revision)?;
    next.injection_overrides
        .retain(|entry| !entry.executable_name.eq_ignore_ascii_case(executable_name));
    if enabled {
        next.injection_overrides.push(InjectionOverride {
            executable_name: executable_name.to_string(),
            strategy: InjectionStrategy::ClipboardCompatibility,
        });
    }
    next.revision = next.revision.saturating_add(1);
    services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)
}

#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    expected_revision: u64,
    window: WebviewWindow,
    voice_input: State<'_, VoiceControlService>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    voice_input
        .save_config(config, expected_revision)
        .await
        .map_err(map_voice_input_error)
}

#[tauri::command]
pub async fn set_enabled(
    enabled: bool,
    expected_revision: u64,
    window: WebviewWindow,
    voice_input: State<'_, VoiceControlService>,
) -> CommandResult<u64> {
    command_error::require_window(&window, "main")?;
    voice_input
        .set_enabled(enabled, expected_revision)
        .await
        .map_err(map_voice_input_error)
}

#[tauri::command]
pub fn set_history_enabled(
    enabled: bool,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    let current = services.config.snapshot();
    require_revision(&current, expected_revision)?;
    let next = history_enabled_config(current, enabled);
    services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)
}

#[tauri::command]
pub fn set_shortcut_trigger_mode(
    mode: ShortcutTriggerMode,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
    voice: State<'_, VoiceSessionHandle>,
    shortcut: State<'_, Arc<ShortcutManager>>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    if voice.status_snapshot().session_active || shortcut.is_trigger_active() {
        return Err(CommandError::new(
            "voice_session_active",
            "本次语音结束后才可以切换快捷键触发方式",
        ));
    }
    let mut next = services.config.snapshot();
    require_revision(&next, expected_revision)?;
    if next.shortcut_trigger_mode == mode {
        return Ok(next);
    }
    next = shortcut_trigger_mode_config(next, mode);
    services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)
}

#[tauri::command]
pub fn set_incident_recovery_enabled(
    enabled: bool,
    expected_revision: u64,
    window: WebviewWindow,
    services: State<'_, AppServices>,
) -> CommandResult<AppConfig> {
    command_error::require_window(&window, "main")?;
    let mut next = services.config.snapshot();
    require_revision(&next, expected_revision)?;
    next.incident_recovery_enabled = enabled;
    if enabled {
        next.incident_consent_version = next.incident_consent_version.max(1);
    }
    next.revision = next.revision.saturating_add(1);
    services
        .config
        .commit_config(expected_revision, next)
        .map_err(map_config_error)
}
