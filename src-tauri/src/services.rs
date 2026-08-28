use crate::config::{AppConfig, ConfigError, ConfigRecovery, CredentialUpdates, LoadedConfig};
use crate::platform::{NativeConfirmation, WindowsNativeConfirmation};
use crate::provider::{
    StreamingTranscriptionProvider, UnavailableProvider, VolcengineAdapter, VolcengineAuth,
    VolcengineAuthMode, VolcengineRuntimeProfile,
};
use crate::provider_model::{
    AsrOptionPool, ProviderModel, ProviderModelError, VolcengineProviderModel,
};
use crate::repositories::{
    ConfigRepository, CredentialStore, DeepSeekHotwordAgentClient, HistoryRepository,
    HotwordAgentClient, HotwordRepository, JsonConfigRepository, SqliteStore,
    WindowsCredentialStore,
};
use crate::text_processing::{DeepSeekTextProcessor, PromptRepository, TextProcessor};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug)]
pub enum ConfigServiceError {
    Conflict(Box<AppConfig>),
    Storage(ConfigError),
}

impl From<ConfigError> for ConfigServiceError {
    fn from(error: ConfigError) -> Self {
        Self::Storage(error)
    }
}

pub struct ConfigService {
    repository: Arc<dyn ConfigRepository>,
    credentials: Arc<dyn CredentialStore>,
    current: RwLock<AppConfig>,
    recovery: RwLock<ConfigRecovery>,
    mutation: Mutex<()>,
}

impl ConfigService {
    pub fn new(
        loaded: LoadedConfig,
        repository: Arc<dyn ConfigRepository>,
        credentials: Arc<dyn CredentialStore>,
    ) -> Self {
        Self {
            repository,
            credentials,
            current: RwLock::new(loaded.config),
            recovery: RwLock::new(loaded.recovery),
            mutation: Mutex::new(()),
        }
    }

    pub fn snapshot(&self) -> AppConfig {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn recovery(&self) -> ConfigRecovery {
        *self
            .recovery
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn commit(
        &self,
        expected_revision: u64,
        next: AppConfig,
        credential_updates: &CredentialUpdates,
    ) -> Result<AppConfig, ConfigServiceError> {
        let _guard = self
            .mutation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current = self.snapshot();
        if current.revision != expected_revision {
            return Err(ConfigServiceError::Conflict(Box::new(current)));
        }
        let credential_snapshot = self
            .credentials
            .update_transactionally(credential_updates)?;
        if let Err(error) = self.repository.save(&next) {
            if let Err(rollback_error) = self.credentials.restore(&credential_snapshot) {
                log::error!(
                    "failed to roll back credentials after config save failure: {rollback_error}"
                );
            }
            return Err(ConfigServiceError::Storage(error));
        }
        *self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = next.clone();
        *self
            .recovery
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = ConfigRecovery::None;
        Ok(next)
    }

    pub fn commit_config(
        &self,
        expected_revision: u64,
        next: AppConfig,
    ) -> Result<AppConfig, ConfigServiceError> {
        self.commit(expected_revision, next, &CredentialUpdates::default())
    }
}

#[derive(Clone)]
pub struct AppServices {
    pub config: Arc<ConfigService>,
    pub credentials: Arc<dyn CredentialStore>,
    pub history: Arc<dyn HistoryRepository>,
    pub hotwords: Arc<dyn HotwordRepository>,
    pub hotword_agent: Arc<dyn HotwordAgentClient>,
    pub provider: Arc<ProviderService>,
    pub confirmations: Arc<dyn NativeConfirmation>,
    pub incidents: Arc<crate::incident::IncidentService>,
    pub text_processor: Arc<dyn TextProcessor>,
    pub prompt_repository: Arc<PromptRepository>,
}

pub struct ProviderService {
    credentials: Arc<dyn CredentialStore>,
    model: Arc<VolcengineProviderModel>,
    profile: VolcengineRuntimeProfile,
}

impl ProviderService {
    pub fn new(credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            credentials,
            model: Arc::new(VolcengineProviderModel),
            profile: VolcengineRuntimeProfile::from_deployment(),
        }
    }

    pub fn model(&self) -> &dyn ProviderModel {
        self.model.as_ref()
    }

    pub fn option_pool(&self, config: &AppConfig) -> Result<AsrOptionPool, ProviderModelError> {
        self.model.option_pool(&config.asr)
    }

    pub fn auth(&self) -> Result<VolcengineAuth, ConfigError> {
        Ok(VolcengineAuth {
            api_key: self.credentials.load_api_key()?,
            app_key: self.credentials.load_app_key()?,
            access_key: self.credentials.load_access_key()?,
        })
    }

    pub fn build_adapter(&self, config: &AppConfig) -> Result<VolcengineAdapter, String> {
        if !self.profile.endpoint.starts_with("wss://") {
            return Err("部署的识别服务地址无效".to_string());
        }
        let auth = self.auth().map_err(|error| error.to_string())?;
        match self.profile.auth_mode {
            VolcengineAuthMode::ApiKey if auth.api_key.is_none() => {
                return Err("部署环境尚未提供识别服务凭据".to_string())
            }
            VolcengineAuthMode::AppAccess
                if auth.app_key.is_none() || auth.access_key.is_none() =>
            {
                return Err("部署环境尚未提供识别服务凭据".to_string())
            }
            _ => {}
        }
        let options = self
            .model
            .request_options(&config.asr)
            .map_err(|error| error.to_string())?;
        Ok(VolcengineAdapter::new(self.profile.clone(), options, auth))
    }

    pub fn build(&self, config: &AppConfig) -> Arc<dyn StreamingTranscriptionProvider> {
        match self.build_adapter(config) {
            Ok(adapter) => Arc::new(adapter),
            Err(message) => Arc::new(UnavailableProvider::new(message)),
        }
    }
}
impl AppServices {
    pub fn hotword_state(
        &self,
    ) -> Result<crate::hotwords::HotwordState, crate::hotwords::HotwordError> {
        let config = self.config.snapshot();
        let has_api_key = if config.is_endpoint_trusted(
            &config.hotword_agent_base_url,
            crate::config::EndpointPurpose::HotwordAgent,
        ) {
            self.credentials
                .load_deepseek_api_key()
                .map_err(|error| crate::hotwords::HotwordError::Request(error.to_string()))?
                .is_some()
        } else {
            false
        };
        self.hotwords.state(&config, has_api_key)
    }

    pub fn production(loaded: LoadedConfig) -> Result<Self, ConfigError> {
        let credentials: Arc<dyn CredentialStore> = Arc::new(WindowsCredentialStore);
        let provider = Arc::new(ProviderService::new(credentials.clone()));
        let repository: Arc<dyn ConfigRepository> = Arc::new(JsonConfigRepository::production()?);
        let loaded = repository.load().unwrap_or(loaded);
        let config = Arc::new(ConfigService::new(loaded, repository, credentials.clone()));
        let sqlite = Arc::new(SqliteStore);
        let prompt_repository = Arc::new(
            PromptRepository::bundled()
                .map_err(|error| ConfigError::Read(format!("prompt bundle invalid: {error}")))?,
        );
        let text_processor: Arc<dyn TextProcessor> = Arc::new(
            DeepSeekTextProcessor::production(config.clone(), credentials.clone()).map_err(
                |error| ConfigError::Read(format!("text processor unavailable: {error}")),
            )?,
        );
        Ok(Self {
            config,
            credentials: credentials.clone(),
            history: sqlite.clone(),
            hotwords: sqlite,
            hotword_agent: Arc::new(DeepSeekHotwordAgentClient::new(credentials)),
            provider,
            confirmations: Arc::new(WindowsNativeConfirmation),
            incidents: Arc::new(crate::incident::IncidentService::production()),
            text_processor,
            prompt_repository,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CredentialSnapshot, CredentialUpdates};

    struct MemoryConfigRepository {
        value: Mutex<LoadedConfig>,
        fail_save: bool,
    }

    impl ConfigRepository for MemoryConfigRepository {
        fn load(&self) -> Result<LoadedConfig, ConfigError> {
            Ok(self.value.lock().unwrap().clone())
        }

        fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
            if self.fail_save {
                return Err(ConfigError::Write("injected failure".to_string()));
            }
            self.value.lock().unwrap().config = config.clone();
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryCredentialStore;

    impl CredentialStore for MemoryCredentialStore {
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
            Ok(None)
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

    fn service(fail_save: bool) -> ConfigService {
        let loaded = LoadedConfig {
            config: AppConfig::default(),
            recovery: ConfigRecovery::None,
        };
        ConfigService::new(
            loaded.clone(),
            Arc::new(MemoryConfigRepository {
                value: Mutex::new(loaded),
                fail_save,
            }),
            Arc::new(MemoryCredentialStore),
        )
    }

    #[test]
    fn failed_save_does_not_replace_in_memory_config() {
        let service = service(true);
        let mut next = service.snapshot();
        next.revision = 1;
        assert!(service.commit_config(0, next).is_err());
        assert_eq!(service.snapshot().revision, 0);
    }

    #[test]
    fn stale_revision_is_rejected() {
        let service = service(false);
        let mut next = service.snapshot();
        next.revision = 1;
        service.commit_config(0, next).unwrap();
        let mut stale = service.snapshot();
        stale.revision = 2;
        assert!(service.commit_config(0, stale).is_err());
    }
}
