use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const KEYRING_SERVICE: &str = "gy-typing";
const KEYRING_API_KEY_USER: &str = "transcription-api-key";
const KEYRING_APP_KEY_USER: &str = "transcription-app-key";
const KEYRING_ACCESS_KEY_USER: &str = "transcription-access-key";
const KEYRING_HOTWORD_AGENT_API_KEY_USER: &str = "hotword-agent-api-key";
const DEFAULT_BASE_URL: &str = "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async";
const DEFAULT_RESOURCE_ID: &str = "volc.bigasr.sauc.duration";
const DEFAULT_MODEL: &str = "bigmodel";
const DEFAULT_LANGUAGE: &str = "zh-CN";
const DEFAULT_AUTH_MODE: &str = "app_access";
const DEFAULT_HOTWORD_AGENT_BASE_URL: &str = "https://api.deepseek.com";
const DEFAULT_HOTWORD_AGENT_MODEL: &str = "deepseek-v4-flash";

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,
    #[serde(default = "default_resource_id")]
    pub resource_id: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_language")]
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecognitionBehaviorConfig {
    #[serde(default = "default_enable_itn")]
    pub enable_itn: bool,
    #[serde(default = "default_enable_punc")]
    pub enable_punc: bool,
    #[serde(default = "default_enable_ddc")]
    pub enable_ddc: bool,
    #[serde(default = "default_enable_accelerate_text")]
    pub enable_accelerate_text: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default)]
    pub provider: ProviderConfig,
    #[serde(default)]
    pub recognition_behavior: RecognitionBehaviorConfig,
    #[serde(default = "default_history_enabled")]
    pub history_enabled: bool,
    #[serde(default = "default_hotwords_enabled")]
    pub hotwords_enabled: bool,
    #[serde(default = "default_hotword_agent_enabled")]
    pub hotword_agent_enabled: bool,
    #[serde(default = "default_hotword_agent_base_url")]
    pub hotword_agent_base_url: String,
    #[serde(default = "default_hotword_agent_model")]
    pub hotword_agent_model: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            shortcut: default_shortcut(),
            provider: ProviderConfig {
                base_url: default_base_url(),
                auth_mode: default_auth_mode(),
                resource_id: default_resource_id(),
                model: default_model(),
                language: default_language(),
            },
            recognition_behavior: RecognitionBehaviorConfig::default(),
            history_enabled: true,
            hotwords_enabled: true,
            hotword_agent_enabled: false,
            hotword_agent_base_url: default_hotword_agent_base_url(),
            hotword_agent_model: default_hotword_agent_model(),
        }
    }
}

impl Default for RecognitionBehaviorConfig {
    fn default() -> Self {
        Self {
            enable_itn: default_enable_itn(),
            enable_punc: default_enable_punc(),
            enable_ddc: default_enable_ddc(),
            enable_accelerate_text: default_enable_accelerate_text(),
        }
    }
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            auth_mode: default_auth_mode(),
            resource_id: default_resource_id(),
            model: default_model(),
            language: default_language(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_shortcut() -> String {
    "Ctrl+Alt+Space".to_string()
}

fn default_history_enabled() -> bool {
    true
}

fn default_enable_itn() -> bool {
    true
}

fn default_enable_punc() -> bool {
    true
}

fn default_enable_ddc() -> bool {
    false
}

fn default_enable_accelerate_text() -> bool {
    true
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

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

fn default_resource_id() -> String {
    DEFAULT_RESOURCE_ID.to_string()
}

fn default_model() -> String {
    DEFAULT_MODEL.to_string()
}

fn default_language() -> String {
    DEFAULT_LANGUAGE.to_string()
}

fn default_auth_mode() -> String {
    DEFAULT_AUTH_MODE.to_string()
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gy-typing");
    fs::create_dir_all(&dir).map_err(|error| ConfigError::CreateDir(error.to_string()))?;
    Ok(dir.join("config.json"))
}

pub fn load_config() -> Result<AppConfig, ConfigError> {
    load_config_from_path(&config_path()?)
}

pub fn save_config(config: &AppConfig) -> Result<(), ConfigError> {
    save_config_to_path(&config_path()?, config)
}

pub fn load_config_from_path(path: &Path) -> Result<AppConfig, ConfigError> {
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let json = fs::read_to_string(path).map_err(|error| ConfigError::Read(error.to_string()))?;
    serde_json::from_str(&json).map_err(|error| ConfigError::Parse(error.to_string()))
}

pub fn save_config_to_path(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| ConfigError::CreateDir(error.to_string()))?;
    }
    let json = serde_json::to_string_pretty(config)
        .map_err(|error| ConfigError::Parse(error.to_string()))?;
    fs::write(path, json).map_err(|error| ConfigError::Write(error.to_string()))
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
    save_secret(KEYRING_HOTWORD_AGENT_API_KEY_USER, api_key)
}

pub fn load_hotword_agent_api_key() -> Result<Option<String>, ConfigError> {
    load_secret(KEYRING_HOTWORD_AGENT_API_KEY_USER)
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

pub fn has_api_key() -> bool {
    load_api_key()
        .map(|api_key| api_key.is_some())
        .unwrap_or(false)
}

pub fn has_app_key() -> bool {
    load_app_key()
        .map(|app_key| app_key.is_some())
        .unwrap_or(false)
}

pub fn has_access_key() -> bool {
    load_access_key()
        .map(|access_key| access_key.is_some())
        .unwrap_or(false)
}

pub fn has_hotword_agent_api_key() -> bool {
    load_hotword_agent_api_key()
        .map(|api_key| api_key.is_some())
        .unwrap_or(false)
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

        assert!(json.contains("Ctrl+Alt+Space"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("access_key"));
        assert!(!json.contains("app_key"));
        assert!(!json.contains("secret"));
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
              "shortcut": "Ctrl+Alt+Space",
              "provider": {
                "base_url": "mock",
                "auth_mode": "app_access",
                "resource_id": "volc.bigasr.sauc.duration",
                "model": "bigmodel",
                "language": "zh-CN"
              }
            }"#,
        )
        .unwrap();

        let config = load_config_from_path(&path).unwrap();

        assert!(config.history_enabled);
        assert_eq!(config.recognition_behavior, RecognitionBehaviorConfig::default());
        assert!(config.hotwords_enabled);
        assert!(!config.hotword_agent_enabled);
        assert_eq!(config.hotword_agent_base_url, DEFAULT_HOTWORD_AGENT_BASE_URL);
        assert_eq!(config.hotword_agent_model, DEFAULT_HOTWORD_AGENT_MODEL);
    }
}
