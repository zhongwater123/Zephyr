use crate::config::{self, AppConfig, CredentialUpdates, InjectionStrategy};
use crate::services::{ConfigService, ConfigServiceError};
use crate::shortcut_manager::ShortcutManager;
use crate::voice_controller::VoiceSessionHandle;
use std::sync::Arc;

#[derive(Debug)]
pub enum VoiceControlServiceError {
    Config(ConfigServiceError),
    NativeConfirmationRequired,
    VoiceControl(String),
    ShortcutState(String),
}

impl From<ConfigServiceError> for VoiceControlServiceError {
    fn from(error: ConfigServiceError) -> Self {
        Self::Config(error)
    }
}

#[derive(Clone)]
pub struct VoiceControlService {
    config: Arc<ConfigService>,
    voice: VoiceSessionHandle,
    shortcut: Arc<ShortcutManager>,
}

impl VoiceControlService {
    pub fn new(
        config: Arc<ConfigService>,
        voice: VoiceSessionHandle,
        shortcut: Arc<ShortcutManager>,
    ) -> Self {
        Self {
            config,
            voice,
            shortcut,
        }
    }

    pub fn save_config(
        &self,
        mut next: AppConfig,
        expected_revision: u64,
        hotword_agent_api_key: Option<String>,
    ) -> Result<AppConfig, VoiceControlServiceError> {
        let current = self.config.snapshot();
        if current.revision != expected_revision {
            return Err(ConfigServiceError::Conflict(Box::new(current)).into());
        }
        if introduces_clipboard_compatibility(&current, &next) {
            return Err(VoiceControlServiceError::NativeConfirmationRequired);
        }

        next.schema_version = config::CURRENT_SCHEMA_VERSION;
        next.asr = current.asr.clone();
        next.shortcut = current.shortcut.clone();
        next.shortcut_binding = current.shortcut_binding.clone();
        next.revision = current.revision.saturating_add(1);
        let updates = CredentialUpdates {
            hotword_agent_api_key: hotword_agent_api_key.filter(|key| !key.trim().is_empty()),
            ..CredentialUpdates::default()
        };
        let committed = self.config.commit(expected_revision, next, &updates)?;

        if current.enabled != committed.enabled {
            self.apply_enabled(committed.enabled)?;
        }
        Ok(committed)
    }

    pub fn set_enabled(
        &self,
        enabled: bool,
        expected_revision: u64,
    ) -> Result<u64, VoiceControlServiceError> {
        let mut next = self.config.snapshot();
        if next.revision != expected_revision {
            return Err(ConfigServiceError::Conflict(Box::new(next)).into());
        }
        next.enabled = enabled;
        next.revision = next.revision.saturating_add(1);
        let revision = next.revision;
        self.config.commit_config(expected_revision, next)?;

        self.apply_enabled(enabled)?;
        Ok(revision)
    }

    pub fn toggle_from_current(&self) -> Result<u64, VoiceControlServiceError> {
        let current = self.config.snapshot();
        self.set_enabled(!current.enabled, current.revision)
    }

    fn apply_enabled(&self, enabled: bool) -> Result<(), VoiceControlServiceError> {
        self.voice
            .set_availability(enabled)
            .map_err(|error| VoiceControlServiceError::VoiceControl(format!("{error:?}")))?;
        self.shortcut
            .set_enabled(enabled)
            .map_err(VoiceControlServiceError::ShortcutState)
    }
}

fn introduces_clipboard_compatibility(current: &AppConfig, next: &AppConfig) -> bool {
    next.injection_overrides.iter().any(|candidate| {
        candidate.strategy == InjectionStrategy::ClipboardCompatibility
            && !current.injection_overrides.iter().any(|existing| {
                existing.strategy == InjectionStrategy::ClipboardCompatibility
                    && existing
                        .executable_name
                        .eq_ignore_ascii_case(&candidate.executable_name)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::InjectionOverride;

    #[test]
    fn detects_only_new_clipboard_compatibility_grants() {
        let current = AppConfig::default();
        let mut next = current.clone();
        next.injection_overrides.push(InjectionOverride {
            executable_name: "legacy.exe".to_string(),
            strategy: InjectionStrategy::ClipboardCompatibility,
        });
        assert!(introduces_clipboard_compatibility(&current, &next));

        let unchanged = next.clone();
        assert!(!introduces_clipboard_compatibility(&next, &unchanged));
    }
}
