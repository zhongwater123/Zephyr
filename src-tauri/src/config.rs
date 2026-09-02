use crate::physical_shortcut::{ShortcutBinding, DEFAULT_SHORTCUT_LABEL};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use url::Url;

const KEYRING_SERVICE: &str = "gy-typing";
const KEYRING_API_KEY_USER: &str = "transcription-api-key";
const KEYRING_APP_KEY_USER: &str = "transcription-app-key";
const KEYRING_ACCESS_KEY_USER: &str = "transcription-access-key";
// Preserve the existing Credential Manager account name while treating the value
// as the shared DeepSeek credential for separately authorized feature purposes.
const KEYRING_DEEPSEEK_SHARED_API_KEY_USER: &str = "hotword-agent-api-key";
const DEFAULT_HOTWORD_AGENT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_HOTWORD_AGENT_MODEL: &str = "deepseek-v4-flash";
pub const CURRENT_SCHEMA_VERSION: u32 = 10;
pub const DEFAULT_POLISH_LEVEL: u8 = 2;
const OFFICIAL_HOTWORD_ORIGIN: &str = "https://api.deepseek.com:443";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("无法创建配置目录: {0}")]
    CreateDir(String),
    #[error("无法读取配置: {0}")]
    Read(String),
    #[error("无法写入配置: {0}")]
    Write(String),
    #[error("无法解析配置: {0}")]
    Parse(String),
    #[error("无法保存 API Key: {0}")]
    Keyring(String),
    #[error("无法原子替换配置: {0}")]
    AtomicReplace(String),
    #[error("endpoint 无效: {0}")]
    InvalidEndpoint(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EndpointPurpose {
    Asr,
    HotwordAgent,
    TextProcessing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedEndpoint {
    pub origin: String,
    pub purpose: EndpointPurpose,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InjectionStrategy {
    Unicode,
    ClipboardCompatibility,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutTriggerMode {
    #[default]
    Hold,
    Toggle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InjectionOverride {
    pub executable_name: String,
    pub strategy: InjectionStrategy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigRecovery {
    None,
    Backup,
    DisabledDefaults,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub recovery: ConfigRecovery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ConfigValue {
    Boolean(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigEnvelope {
    pub provider_id: String,
    pub schema_version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default)]
    pub values: BTreeMap<String, ConfigValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub revision: u64,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default)]
    pub shortcut_binding: Option<ShortcutBinding>,
    #[serde(default)]
    pub shortcut_trigger_mode: ShortcutTriggerMode,
    #[serde(default = "default_asr_config")]
    pub asr: ProviderConfigEnvelope,
    #[serde(default = "default_history_enabled")]
    pub history_enabled: bool,
    #[serde(default)]
    pub incident_recovery_enabled: bool,
    #[serde(default)]
    pub incident_consent_version: u32,
    #[serde(default = "default_true")]
    pub incident_save_failed_audio: bool,
    #[serde(default = "default_true")]
    pub incident_save_failed_text: bool,
    #[serde(default = "default_incident_retention_days")]
    pub incident_retention_days: u16,
    #[serde(default = "default_incident_storage_limit_mb")]
    pub incident_storage_limit_mb: u32,
    #[serde(default = "default_incident_success_rollup_days")]
    pub incident_success_rollup_days: u16,
    #[serde(default = "default_hotwords_enabled")]
    pub hotwords_enabled: bool,
    #[serde(default = "default_hotword_agent_enabled")]
    pub hotword_agent_enabled: bool,
    #[serde(default = "default_hotword_agent_base_url")]
    pub hotword_agent_base_url: String,
    #[serde(default = "default_hotword_agent_model")]
    pub hotword_agent_model: String,
    #[serde(default = "default_polish_level")]
    pub polish_level: u8,
    #[serde(default)]
    pub trusted_endpoints: Vec<TrustedEndpoint>,
    #[serde(default)]
    pub injection_overrides: Vec<InjectionOverride>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            revision: 0,
            enabled: true,
            shortcut: default_shortcut(),
            shortcut_binding: Some(ShortcutBinding::default_physical()),
            shortcut_trigger_mode: ShortcutTriggerMode::Hold,
            asr: default_asr_config(),
            history_enabled: true,
            incident_recovery_enabled: false,
            incident_consent_version: 0,
            incident_save_failed_audio: true,
            incident_save_failed_text: true,
            incident_retention_days: default_incident_retention_days(),
            incident_storage_limit_mb: default_incident_storage_limit_mb(),
            incident_success_rollup_days: default_incident_success_rollup_days(),
            hotwords_enabled: true,
            hotword_agent_enabled: false,
            hotword_agent_base_url: default_hotword_agent_base_url(),
            hotword_agent_model: default_hotword_agent_model(),
            polish_level: DEFAULT_POLISH_LEVEL,
            trusted_endpoints: official_endpoints(),
            injection_overrides: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn is_endpoint_trusted(&self, endpoint: &str, purpose: EndpointPurpose) -> bool {
        normalize_origin(endpoint)
            .map(|origin| {
                is_official_origin(&origin, &purpose)
                    || self
                        .trusted_endpoints
                        .iter()
                        .any(|entry| entry.origin == origin && entry.purpose == purpose)
            })
            .unwrap_or(false)
    }

    pub fn injection_strategy_for(&self, executable_name: &str) -> InjectionStrategy {
        self.injection_overrides
            .iter()
            .find(|entry| entry.executable_name.eq_ignore_ascii_case(executable_name))
            .map(|entry| entry.strategy.clone())
            .unwrap_or(InjectionStrategy::Unicode)
    }
}

fn default_enabled() -> bool {
    true
}

fn default_shortcut() -> String {
    DEFAULT_SHORTCUT_LABEL.to_string()
}

fn default_history_enabled() -> bool {
    true
}

fn default_true() -> bool {
    true
}

fn default_incident_retention_days() -> u16 {
    7
}

fn default_incident_storage_limit_mb() -> u32 {
    512
}

fn default_incident_success_rollup_days() -> u16 {
    30
}

fn default_hotwords_enabled() -> bool {
    true
}

fn default_hotword_agent_enabled() -> bool {
    false
}

fn default_hotword_agent_base_url() -> String {
    DEFAULT_HOTWORD_AGENT_BASE_URL.to_string()
}

fn default_hotword_agent_model() -> String {
    DEFAULT_HOTWORD_AGENT_MODEL.to_string()
}

fn default_polish_level() -> u8 {
    DEFAULT_POLISH_LEVEL
}

fn default_asr_config() -> ProviderConfigEnvelope {
    crate::provider_model::VolcengineProviderModel::default_envelope()
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gy-typing");
    fs::create_dir_all(&dir).map_err(|error| ConfigError::CreateDir(error.to_string()))?;
    Ok(dir.join("config.json"))
}

pub fn load_config_with_status() -> Result<LoadedConfig, ConfigError> {
    load_config_with_status_from_path(&config_path()?)
}

#[cfg(test)]
pub fn load_config_from_path(path: &Path) -> Result<AppConfig, ConfigError> {
    Ok(load_config_with_status_from_path(path)?.config)
}

pub fn load_config_with_status_from_path(path: &Path) -> Result<LoadedConfig, ConfigError> {
    if !path.exists() {
        return Ok(LoadedConfig {
            config: AppConfig::default(),
            recovery: ConfigRecovery::None,
        });
    }

    if let Ok(config) = read_and_migrate_config(path) {
        return Ok(LoadedConfig {
            config,
            recovery: ConfigRecovery::None,
        });
    }

    let backup = backup_path(path);
    if let Ok(config) = read_and_migrate_config(&backup) {
        return Ok(LoadedConfig {
            config,
            recovery: ConfigRecovery::Backup,
        });
    }

    let config = AppConfig {
        enabled: false,
        ..AppConfig::default()
    };
    Ok(LoadedConfig {
        config,
        recovery: ConfigRecovery::DisabledDefaults,
    })
}

pub fn save_config_to_path(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ConfigError::CreateDir(error.to_string()))?;
    }
    let mut normalized = config.clone();
    normalized.schema_version = CURRENT_SCHEMA_VERSION;
    normalize_trusted_endpoints(&mut normalized)?;
    normalize_polish_level(&mut normalized);
    let json = serde_json::to_vec_pretty(&normalized)
        .map_err(|error| ConfigError::Parse(error.to_string()))?;

    if read_and_migrate_config(path).is_ok() {
        atomic_write(
            &backup_path(path),
            &fs::read(path)
                .map_err(|error| ConfigError::Read(format!("读取旧配置以创建备份失败: {error}")))?,
        )?;
    }
    atomic_write(path, &json)
}

fn read_and_migrate_config(path: &Path) -> Result<AppConfig, ConfigError> {
    let json = fs::read_to_string(path).map_err(|error| ConfigError::Read(error.to_string()))?;
    let raw: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| ConfigError::Parse(error.to_string()))?;
    let source_schema = raw
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    let mut config: AppConfig = serde_json::from_value(raw.clone())
        .map_err(|error| ConfigError::Parse(error.to_string()))?;
    if source_schema < 3 {
        crate::provider_model::VolcengineProviderModel::migrate_legacy_envelope(
            &raw,
            &mut config.asr,
        );
    }
    #[cfg(target_os = "windows")]
    if config.shortcut_binding.is_none() {
        config.shortcut_binding = ShortcutBinding::from_legacy_label(&config.shortcut).ok();
    }
    config.schema_version = CURRENT_SCHEMA_VERSION;
    normalize_trusted_endpoints(&mut config)?;
    normalize_polish_level(&mut config);
    Ok(config)
}

fn backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    path.with_file_name(format!("{file_name}.bak"))
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), ConfigError> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.json");
    let temp_path = path.with_file_name(format!(".{file_name}.tmp-{}", uuid::Uuid::new_v4()));
    let mut temp = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .map_err(|error| ConfigError::Write(error.to_string()))?;
    temp.write_all(contents)
        .and_then(|_| temp.flush())
        .and_then(|_| temp.sync_all())
        .map_err(|error| ConfigError::Write(error.to_string()))?;
    drop(temp);

    let replace_result = atomic_replace(&temp_path, path);
    if replace_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    replace_result
}

#[cfg(target_os = "windows")]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), ConfigError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(destination_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| ConfigError::AtomicReplace(error.to_string()))
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), ConfigError> {
    fs::rename(source, destination).map_err(|error| ConfigError::AtomicReplace(error.to_string()))
}

pub fn normalize_origin(endpoint: &str) -> Result<String, ConfigError> {
    let url =
        Url::parse(endpoint).map_err(|error| ConfigError::InvalidEndpoint(error.to_string()))?;
    let scheme = url.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "https" | "wss") {
        return Err(ConfigError::InvalidEndpoint(
            "只允许 https 或 wss endpoint".to_string(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ConfigError::InvalidEndpoint("endpoint 缺少主机名".to_string()))?
        .to_ascii_lowercase();
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ConfigError::InvalidEndpoint("endpoint 无法确定有效端口".to_string()))?;
    Ok(format!("{scheme}://{host}:{port}"))
}

fn official_endpoints() -> Vec<TrustedEndpoint> {
    vec![
        TrustedEndpoint {
            origin: OFFICIAL_HOTWORD_ORIGIN.to_string(),
            purpose: EndpointPurpose::HotwordAgent,
        },
        TrustedEndpoint {
            origin: OFFICIAL_HOTWORD_ORIGIN.to_string(),
            purpose: EndpointPurpose::TextProcessing,
        },
    ]
}

fn is_official_origin(origin: &str, purpose: &EndpointPurpose) -> bool {
    matches!(
        (origin, purpose),
        (OFFICIAL_HOTWORD_ORIGIN, EndpointPurpose::HotwordAgent)
            | (OFFICIAL_HOTWORD_ORIGIN, EndpointPurpose::TextProcessing)
    )
}

fn normalize_polish_level(config: &mut AppConfig) {
    if !(0..=3).contains(&config.polish_level) {
        config.polish_level = DEFAULT_POLISH_LEVEL;
    }
}

fn normalize_trusted_endpoints(config: &mut AppConfig) -> Result<(), ConfigError> {
    let mut normalized = official_endpoints();
    for endpoint in std::mem::take(&mut config.trusted_endpoints) {
        if endpoint.purpose == EndpointPurpose::Asr {
            continue;
        }
        let origin = normalize_origin(&endpoint.origin)?;
        if !normalized
            .iter()
            .any(|entry| entry.origin == origin && entry.purpose == endpoint.purpose)
        {
            normalized.push(TrustedEndpoint {
                origin,
                purpose: endpoint.purpose,
            });
        }
    }
    config.trusted_endpoints = normalized;
    Ok(())
}

pub fn save_api_key(api_key: &str) -> Result<(), ConfigError> {
    save_secret(KEYRING_API_KEY_USER, api_key)
}

pub fn load_api_key() -> Result<Option<String>, ConfigError> {
    load_secret(KEYRING_API_KEY_USER)
}

pub fn save_app_key(app_key: &str) -> Result<(), ConfigError> {
    save_secret(KEYRING_APP_KEY_USER, app_key)
}

pub fn load_app_key() -> Result<Option<String>, ConfigError> {
    load_secret(KEYRING_APP_KEY_USER)
}

pub fn save_access_key(access_key: &str) -> Result<(), ConfigError> {
    save_secret(KEYRING_ACCESS_KEY_USER, access_key)
}

pub fn load_access_key() -> Result<Option<String>, ConfigError> {
    load_secret(KEYRING_ACCESS_KEY_USER)
}

pub fn save_hotword_agent_api_key(api_key: &str) -> Result<(), ConfigError> {
    save_deepseek_api_key(api_key)
}

pub fn save_deepseek_api_key(api_key: &str) -> Result<(), ConfigError> {
    save_secret(KEYRING_DEEPSEEK_SHARED_API_KEY_USER, api_key)
}

#[derive(Debug, Clone, Default)]
pub struct CredentialUpdates {
    pub api_key: Option<String>,
    pub app_key: Option<String>,
    pub access_key: Option<String>,
    pub hotword_agent_api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CredentialSnapshot {
    pub(crate) api_key: Option<String>,
    pub(crate) app_key: Option<String>,
    pub(crate) access_key: Option<String>,
    pub(crate) hotword_agent_api_key: Option<String>,
}

pub fn snapshot_credentials() -> Result<CredentialSnapshot, ConfigError> {
    Ok(CredentialSnapshot {
        api_key: load_api_key()?,
        app_key: load_app_key()?,
        access_key: load_access_key()?,
        hotword_agent_api_key: load_hotword_agent_api_key()?,
    })
}

pub fn update_credentials_transactionally(
    updates: &CredentialUpdates,
) -> Result<CredentialSnapshot, ConfigError> {
    let snapshot = snapshot_credentials()?;

    let result = (|| {
        if let Some(value) = updates.api_key.as_deref() {
            save_api_key(value)?;
        }
        if let Some(value) = updates.app_key.as_deref() {
            save_app_key(value)?;
        }
        if let Some(value) = updates.access_key.as_deref() {
            save_access_key(value)?;
        }
        if let Some(value) = updates.hotword_agent_api_key.as_deref() {
            save_hotword_agent_api_key(value)?;
        }
        Ok(())
    })();

    if let Err(error) = result {
        let _ = restore_credentials(&snapshot);
        return Err(error);
    }
    Ok(snapshot)
}

pub fn restore_credentials(snapshot: &CredentialSnapshot) -> Result<(), ConfigError> {
    restore_secret(KEYRING_API_KEY_USER, snapshot.api_key.as_deref())?;
    restore_secret(KEYRING_APP_KEY_USER, snapshot.app_key.as_deref())?;
    restore_secret(KEYRING_ACCESS_KEY_USER, snapshot.access_key.as_deref())?;
    restore_secret(
        KEYRING_DEEPSEEK_SHARED_API_KEY_USER,
        snapshot.hotword_agent_api_key.as_deref(),
    )
}

pub fn load_hotword_agent_api_key() -> Result<Option<String>, ConfigError> {
    load_deepseek_api_key()
}

pub fn load_deepseek_api_key() -> Result<Option<String>, ConfigError> {
    load_secret(KEYRING_DEEPSEEK_SHARED_API_KEY_USER)
}

fn save_secret(user: &str, value: &str) -> Result<(), ConfigError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|error| ConfigError::Keyring(error.to_string()))?;
    entry
        .set_password(value)
        .map_err(|error| ConfigError::Keyring(error.to_string()))
}

fn load_secret(user: &str) -> Result<Option<String>, ConfigError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|error| ConfigError::Keyring(error.to_string()))?;
    match entry.get_password() {
        Ok(api_key) if !api_key.trim().is_empty() => Ok(Some(api_key)),
        Ok(_) => Ok(None),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(ConfigError::Keyring(error.to_string())),
    }
}

fn restore_secret(user: &str, value: Option<&str>) -> Result<(), ConfigError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, user)
        .map_err(|error| ConfigError::Keyring(error.to_string()))?;
    match value {
        Some(value) => entry
            .set_password(value)
            .map_err(|error| ConfigError::Keyring(error.to_string())),
        None => match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(ConfigError::Keyring(error.to_string())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_key_is_not_serialized_to_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = AppConfig::default();

        save_config_to_path(&path, &config).unwrap();
        let json = fs::read_to_string(path).unwrap();

        assert!(json.contains("左 Ctrl+左 Shift+Space"));
        assert!(json.contains("\"shortcut_binding\""));
        assert!(!json.contains("shortcut_mode"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("access_key"));
        assert!(!json.contains("app_key"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("\"provider\""));
        assert!(!json.contains("recognition_behavior"));
    }

    #[test]
    fn missing_config_loads_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = load_config_from_path(&dir.path().join("missing.json")).unwrap();

        assert_eq!(config, AppConfig::default());
    }

    #[test]
    fn old_config_without_history_enabled_uses_default_true() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(
            &path,
            r#"{
              "enabled": true,
              "shortcut": "Ctrl+Alt+Space"
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&path).unwrap();

        assert!(config.history_enabled);
        assert_eq!(config.asr, default_asr_config());
        assert!(config.hotwords_enabled);
        assert!(!config.hotword_agent_enabled);
        assert_eq!(
            config.hotword_agent_base_url,
            DEFAULT_HOTWORD_AGENT_BASE_URL
        );
        assert_eq!(config.hotword_agent_model, DEFAULT_HOTWORD_AGENT_MODEL);
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(config.revision, 0);
        assert_eq!(config.shortcut, "Ctrl+Alt+Space");
        assert_eq!(config.polish_level, DEFAULT_POLISH_LEVEL);
        #[cfg(target_os = "windows")]
        assert!(config.shortcut_binding.is_some());
        #[cfg(target_os = "macos")]
        assert!(config.shortcut_binding.is_none());
    }

    #[test]
    fn endpoint_trust_is_bound_to_origin_port_and_purpose() {
        let mut config = AppConfig::default();
        config.trusted_endpoints.push(TrustedEndpoint {
            origin: "https://custom.example:8443".to_string(),
            purpose: EndpointPurpose::HotwordAgent,
        });

        assert!(config.is_endpoint_trusted(
            "https://custom.example:8443/v1/chat/completions",
            EndpointPurpose::HotwordAgent
        ));
        assert!(!config.is_endpoint_trusted(
            "https://custom.example/v1/chat/completions",
            EndpointPurpose::HotwordAgent
        ));
        assert!(!config.is_endpoint_trusted(
            "https://custom.example:8443/v1/chat/completions",
            EndpointPurpose::Asr
        ));
    }

    #[test]
    fn corrupt_primary_recovers_last_valid_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let first = AppConfig {
            revision: 7,
            ..AppConfig::default()
        };
        save_config_to_path(&path, &first).unwrap();
        let mut second = first.clone();
        second.revision = 8;
        save_config_to_path(&path, &second).unwrap();
        fs::write(&path, "{broken").unwrap();

        let loaded = load_config_with_status_from_path(&path).unwrap();

        assert_eq!(loaded.recovery, ConfigRecovery::Backup);
        assert_eq!(loaded.config.revision, 7);
    }

    #[test]
    fn corrupt_primary_and_backup_start_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, "{broken").unwrap();
        fs::write(backup_path(&path), "also broken").unwrap();

        let loaded = load_config_with_status_from_path(&path).unwrap();

        assert_eq!(loaded.recovery, ConfigRecovery::DisabledDefaults);
        assert!(!loaded.config.enabled);
    }
    #[test]
    fn schema_three_config_migrates_with_incident_recovery_unconsented() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let legacy = serde_json::json!({
            "schema_version": 3,
            "enabled": true,
            "shortcut": "Ctrl+Alt+Space",
            "history_enabled": true,
            "asr": default_asr_config()
        });
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let config = read_and_migrate_config(&path).unwrap();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(!config.incident_recovery_enabled);
        assert_eq!(config.incident_consent_version, 0);
        assert!(config.incident_save_failed_audio);
        assert!(config.incident_save_failed_text);
        assert_eq!(config.shortcut, "Ctrl+Alt+Space");
        #[cfg(target_os = "windows")]
        assert!(config.shortcut_binding.is_some());
        #[cfg(target_os = "macos")]
        assert!(config.shortcut_binding.is_none());
    }

    #[test]
    fn legacy_invalid_and_fast_polish_levels_migrate_or_persist() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut legacy = serde_json::to_value(AppConfig::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("polish_level");
        legacy["schema_version"] = 7.into();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(read_and_migrate_config(&path).unwrap().polish_level, 2);

        legacy["polish_level"] = 9.into();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(read_and_migrate_config(&path).unwrap().polish_level, 2);

        legacy["polish_level"] = 0.into();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(read_and_migrate_config(&path).unwrap().polish_level, 0);
    }

    #[test]
    fn legacy_config_without_shortcut_trigger_mode_defaults_to_hold() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let mut legacy = serde_json::to_value(AppConfig::default()).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("shortcut_trigger_mode");
        legacy["schema_version"] = 8.into();
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let config = read_and_migrate_config(&path).unwrap();

        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(config.shortcut_trigger_mode, ShortcutTriggerMode::Hold);
    }

    #[test]
    fn toggle_shortcut_trigger_mode_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = AppConfig {
            shortcut_trigger_mode: ShortcutTriggerMode::Toggle,
            ..AppConfig::default()
        };

        save_config_to_path(&path, &config).unwrap();
        let loaded = load_config_from_path(&path).unwrap();

        assert_eq!(loaded.shortcut_trigger_mode, ShortcutTriggerMode::Toggle);
    }

    #[test]
    fn unmappable_legacy_shortcut_is_preserved_without_silent_default() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let legacy = serde_json::json!({
            "schema_version": 5,
            "enabled": true,
            "shortcut": "Ctrl+DefinitelyUnknown",
            "history_enabled": true,
            "asr": default_asr_config()
        });
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let config = read_and_migrate_config(&path).unwrap();

        assert_eq!(config.shortcut, "Ctrl+DefinitelyUnknown");
        assert!(config.shortcut_binding.is_none());
    }
    #[test]
    fn new_install_uses_exact_left_physical_default_shortcut() {
        let config = AppConfig::default();
        assert_eq!(config.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(config.shortcut, DEFAULT_SHORTCUT_LABEL);
        assert_eq!(
            config.shortcut_binding,
            Some(ShortcutBinding::default_physical())
        );
    }
}
