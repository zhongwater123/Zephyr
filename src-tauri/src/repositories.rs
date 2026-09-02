use crate::config::{
    self, AppConfig, ConfigError, CredentialSnapshot, CredentialUpdates, LoadedConfig,
};
use crate::history::{self, AppContext, HistoryError, HistoryItem, HistoryProvenance};
use crate::hotwords::{self, HotwordError, HotwordState};
use async_trait::async_trait;
use std::path::PathBuf;

pub trait ConfigRepository: Send + Sync {
    fn load(&self) -> Result<LoadedConfig, ConfigError>;
    fn save(&self, config: &AppConfig) -> Result<(), ConfigError>;
}

#[derive(Debug, Clone)]
pub struct JsonConfigRepository {
    path: PathBuf,
}

impl JsonConfigRepository {
    pub fn production() -> Result<Self, ConfigError> {
        Ok(Self {
            path: config::config_path()?,
        })
    }

    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
}

impl ConfigRepository for JsonConfigRepository {
    fn load(&self) -> Result<LoadedConfig, ConfigError> {
        config::load_config_with_status_from_path(&self.path)
    }

    fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        config::save_config_to_path(&self.path, config)
    }
}

pub trait CredentialStore: Send + Sync {
    fn load_api_key(&self) -> Result<Option<String>, ConfigError>;
    fn load_app_key(&self) -> Result<Option<String>, ConfigError>;
    fn load_access_key(&self) -> Result<Option<String>, ConfigError>;
    fn load_deepseek_api_key(&self) -> Result<Option<String>, ConfigError>;
    fn update_transactionally(
        &self,
        updates: &CredentialUpdates,
    ) -> Result<CredentialSnapshot, ConfigError>;
    fn restore(&self, snapshot: &CredentialSnapshot) -> Result<(), ConfigError>;
}

#[derive(Debug, Default)]
pub struct SystemCredentialStore;

fn deployment_asr_api_key() -> Option<String> {
    option_env!("GY_TYPING_ASR_API_KEY")
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("GY_TYPING_ASR_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

fn deployment_deepseek_api_key() -> Option<String> {
    option_env!("GY_TYPING_DEEPSEEK_API_KEY")
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("GY_TYPING_DEEPSEEK_API_KEY")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}

impl CredentialStore for SystemCredentialStore {
    fn load_api_key(&self) -> Result<Option<String>, ConfigError> {
        match deployment_asr_api_key() {
            Some(api_key) => Ok(Some(api_key)),
            None => config::load_api_key(),
        }
    }

    fn load_app_key(&self) -> Result<Option<String>, ConfigError> {
        config::load_app_key()
    }

    fn load_access_key(&self) -> Result<Option<String>, ConfigError> {
        config::load_access_key()
    }

    fn load_deepseek_api_key(&self) -> Result<Option<String>, ConfigError> {
        match deployment_deepseek_api_key() {
            Some(api_key) => Ok(Some(api_key)),
            None => config::load_deepseek_api_key(),
        }
    }

    fn update_transactionally(
        &self,
        updates: &CredentialUpdates,
    ) -> Result<CredentialSnapshot, ConfigError> {
        config::update_credentials_transactionally(updates)
    }

    fn restore(&self, snapshot: &CredentialSnapshot) -> Result<(), ConfigError> {
        config::restore_credentials(snapshot)
    }
}

pub trait HistoryRepository: Send + Sync {
    #[allow(dead_code)]
    fn insert(&self, text: &str, context: &AppContext) -> Result<HistoryItem, HistoryError>;
    fn insert_with_provenance(
        &self,
        text: &str,
        context: &AppContext,
        provenance: &HistoryProvenance,
    ) -> Result<HistoryItem, HistoryError>;
    fn list(
        &self,
        query: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryItem>, HistoryError>;
    fn update(&self, id: &str, text: &str) -> Result<(), HistoryError>;
    fn delete(&self, id: &str) -> Result<(), HistoryError>;
    fn clear(&self) -> Result<(), HistoryError>;
    fn get_text(&self, id: &str) -> Result<String, HistoryError>;
}

#[derive(Debug, Default)]
pub struct SqliteStore;

impl HistoryRepository for SqliteStore {
    fn insert(&self, text: &str, context: &AppContext) -> Result<HistoryItem, HistoryError> {
        history::insert_transcript(text, context)
    }

    fn insert_with_provenance(
        &self,
        text: &str,
        context: &AppContext,
        provenance: &HistoryProvenance,
    ) -> Result<HistoryItem, HistoryError> {
        history::insert_transcript_with_provenance(text, context, provenance)
    }

    fn list(
        &self,
        query: Option<String>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<HistoryItem>, HistoryError> {
        history::list_history(query, limit, offset)
    }

    fn update(&self, id: &str, text: &str) -> Result<(), HistoryError> {
        history::update_history(id, text)
    }

    fn delete(&self, id: &str) -> Result<(), HistoryError> {
        history::delete_history(id)
    }

    fn clear(&self) -> Result<(), HistoryError> {
        history::clear_history()
    }

    fn get_text(&self, id: &str) -> Result<String, HistoryError> {
        history::get_history_text(id)
    }
}

pub trait HotwordRepository: Send + Sync {
    fn state(&self, config: &AppConfig, has_api_key: bool) -> Result<HotwordState, HotwordError>;
    fn save_manual(&self, words: Vec<String>) -> Result<(), HotwordError>;
    fn add(&self, word: &str) -> Result<(), HotwordError>;
    fn update(&self, old_word: &str, new_word: &str) -> Result<(), HotwordError>;
    fn delete(&self, word: &str) -> Result<(), HotwordError>;
    fn delete_agent(&self, word: &str) -> Result<(), HotwordError>;
    fn promote_agent(&self, word: &str) -> Result<(), HotwordError>;
    fn update_profile(&self, text: &str) -> Result<(), HotwordError>;
    fn update_app(&self, app_name: &str, context: &str) -> Result<(), HotwordError>;
    fn delete_app(&self, app_name: &str) -> Result<(), HotwordError>;
}

impl HotwordRepository for SqliteStore {
    fn state(&self, config: &AppConfig, has_api_key: bool) -> Result<HotwordState, HotwordError> {
        hotwords::get_state(config, has_api_key)
    }

    fn save_manual(&self, words: Vec<String>) -> Result<(), HotwordError> {
        hotwords::save_manual_hotwords(words)
    }

    fn add(&self, word: &str) -> Result<(), HotwordError> {
        hotwords::add_hotword(word)
    }

    fn update(&self, old_word: &str, new_word: &str) -> Result<(), HotwordError> {
        hotwords::update_hotword(old_word, new_word)
    }

    fn delete(&self, word: &str) -> Result<(), HotwordError> {
        hotwords::delete_hotword(word)
    }

    fn delete_agent(&self, word: &str) -> Result<(), HotwordError> {
        hotwords::delete_agent_hotword(word)
    }

    fn promote_agent(&self, word: &str) -> Result<(), HotwordError> {
        hotwords::promote_agent_hotword(word)
    }

    fn update_profile(&self, text: &str) -> Result<(), HotwordError> {
        hotwords::update_profile_context(text)
    }

    fn update_app(&self, app_name: &str, context: &str) -> Result<(), HotwordError> {
        hotwords::update_app_context(app_name, context)
    }

    fn delete_app(&self, app_name: &str) -> Result<(), HotwordError> {
        hotwords::delete_app_context(app_name)
    }
}

#[async_trait]
pub trait HotwordAgentClient: Send + Sync {
    async fn test_connection(&self, config: AppConfig) -> Result<String, HotwordError>;
    async fn organize(&self, config: AppConfig, force: bool) -> Result<HotwordState, HotwordError>;
}

pub struct DeepSeekHotwordAgentClient {
    credentials: std::sync::Arc<dyn CredentialStore>,
}

impl DeepSeekHotwordAgentClient {
    pub fn new(credentials: std::sync::Arc<dyn CredentialStore>) -> Self {
        Self { credentials }
    }

    fn require_authorized_key(&self, config: &AppConfig) -> Result<String, HotwordError> {
        if !config.is_endpoint_trusted(
            &config.hotword_agent_base_url,
            config::EndpointPurpose::HotwordAgent,
        ) {
            return Err(HotwordError::Request(
                "热词 Agent endpoint 尚未通过 Windows 原生授权".to_string(),
            ));
        }
        let key = self
            .credentials
            .load_deepseek_api_key()
            .map_err(|error| HotwordError::Request(error.to_string()))?
            .filter(|value| !value.trim().is_empty())
            .ok_or(HotwordError::MissingApiKey)?;
        Ok(key)
    }
}

#[async_trait]
impl HotwordAgentClient for DeepSeekHotwordAgentClient {
    async fn test_connection(&self, config: AppConfig) -> Result<String, HotwordError> {
        let key = self.require_authorized_key(&config)?;
        hotwords::test_agent_connection(config, key).await
    }

    async fn organize(&self, config: AppConfig, force: bool) -> Result<HotwordState, HotwordError> {
        let key = self.require_authorized_key(&config)?;
        hotwords::organize_hotwords(config, force, key).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingCredentialStore {
        hotword_reads: AtomicUsize,
    }

    impl CredentialStore for CountingCredentialStore {
        fn load_api_key(&self) -> Result<Option<String>, ConfigError> {
            Ok(None)
        }
        fn load_app_key(&self) -> Result<Option<String>, ConfigError> {
            Ok(None)
        }
        fn load_access_key(&self) -> Result<Option<String>, ConfigError> {
            Ok(None)
        }
        fn load_deepseek_api_key(&self) -> Result<Option<String>, ConfigError> {
            self.hotword_reads.fetch_add(1, Ordering::SeqCst);
            Ok(Some("secret".to_string()))
        }
        fn update_transactionally(
            &self,
            _updates: &CredentialUpdates,
        ) -> Result<CredentialSnapshot, ConfigError> {
            Ok(CredentialSnapshot {
                api_key: None,
                app_key: None,
                access_key: None,
                hotword_agent_api_key: None,
            })
        }
        fn restore(&self, _snapshot: &CredentialSnapshot) -> Result<(), ConfigError> {
            Ok(())
        }
    }

    #[test]
    fn json_repository_preserves_revision_and_backup_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let repository = JsonConfigRepository::at(path.clone());
        let first = AppConfig {
            revision: 7,
            ..AppConfig::default()
        };
        repository.save(&first).unwrap();
        let mut second = first.clone();
        second.revision = 8;
        repository.save(&second).unwrap();

        std::fs::write(&path, b"invalid").unwrap();
        let recovered = repository.load().unwrap();
        assert_eq!(recovered.config.revision, 7);
        assert_eq!(recovered.recovery, config::ConfigRecovery::Backup);
    }

    #[tokio::test]
    async fn untrusted_hotword_endpoint_is_rejected_before_credential_read() {
        let credentials = std::sync::Arc::new(CountingCredentialStore {
            hotword_reads: AtomicUsize::new(0),
        });
        let client = DeepSeekHotwordAgentClient::new(credentials.clone());
        let config = AppConfig {
            hotword_agent_base_url: "https://untrusted.example/v1".to_string(),
            ..AppConfig::default()
        };
        let error = client.test_connection(config).await.unwrap_err();
        assert!(error.to_string().contains("尚未通过"));
        assert_eq!(credentials.hotword_reads.load(Ordering::SeqCst), 0);
    }
}
