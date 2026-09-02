use crate::config::{self, AppConfig, InjectionStrategy};
use crate::desktop_support::{DesktopCapability, DesktopSupportPolicy};
use crate::services::{ConfigService, ConfigServiceError};
use crate::shortcut_manager::ShortcutManager;
use crate::voice_controller::{VoiceAvailability, VoiceSessionHandle};
use std::sync::Arc;

#[derive(Debug)]
pub enum VoiceControlServiceError {
    Config(ConfigServiceError),
    NativeConfirmationRequired,
    Unsupported(DesktopCapability),
    Reconciliation {
        committed_revision: u64,
        message: String,
    },
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
    support: DesktopSupportPolicy,
}

impl VoiceControlService {
    pub fn new(
        config: Arc<ConfigService>,
        voice: VoiceSessionHandle,
        shortcut: Arc<ShortcutManager>,
        support: DesktopSupportPolicy,
    ) -> Self {
        Self {
            config,
            voice,
            shortcut,
            support,
        }
    }

    pub async fn save_config(
        &self,
        mut next: AppConfig,
        expected_revision: u64,
    ) -> Result<AppConfig, VoiceControlServiceError> {
        let current = self.config.snapshot();
        if current.revision != expected_revision {
            return Err(ConfigServiceError::Conflict(Box::new(current)).into());
        }
        if let Some(capability) = required_capability_for_config_change(&current, &next) {
            self.require(capability)?;
        }
        if introduces_clipboard_compatibility(&current, &next) {
            return Err(VoiceControlServiceError::NativeConfirmationRequired);
        }

        next.schema_version = config::CURRENT_SCHEMA_VERSION;
        next.asr = current.asr.clone();
        next.shortcut = current.shortcut.clone();
        next.shortcut_binding = current.shortcut_binding.clone();
        next.shortcut_trigger_mode = current.shortcut_trigger_mode;
        next.revision = current.revision.saturating_add(1);
        let committed = self.config.commit_config(expected_revision, next)?;

        // Every committed revision is a reconciliation opportunity. This also
        // repairs a previous partial commit whose Actor acknowledgement failed.
        self.apply_enabled(committed.enabled, committed.revision)
            .await?;
        Ok(committed)
    }

    pub async fn set_enabled(
        &self,
        enabled: bool,
        expected_revision: u64,
    ) -> Result<u64, VoiceControlServiceError> {
        if enabled {
            self.require(DesktopCapability::GlobalShortcut)?;
        }
        let mut next = self.config.snapshot();
        if next.revision != expected_revision {
            return Err(ConfigServiceError::Conflict(Box::new(next)).into());
        }
        next.enabled = enabled;
        next.revision = next.revision.saturating_add(1);
        let revision = next.revision;
        self.config.commit_config(expected_revision, next)?;

        self.apply_enabled(enabled, revision).await?;
        Ok(revision)
    }

    fn require(&self, capability: DesktopCapability) -> Result<(), VoiceControlServiceError> {
        if self.support.supports(capability) {
            Ok(())
        } else {
            Err(VoiceControlServiceError::Unsupported(capability))
        }
    }

    pub async fn toggle_from_current(&self) -> Result<u64, VoiceControlServiceError> {
        let current = self.config.snapshot();
        self.set_enabled(!current.enabled, current.revision).await
    }

    async fn apply_enabled(
        &self,
        enabled: bool,
        committed_revision: u64,
    ) -> Result<(), VoiceControlServiceError> {
        let effective_enabled = effective_voice_enabled(&self.support, enabled);
        let snapshot = self
            .voice
            .set_availability(effective_enabled, committed_revision)
            .await
            .map_err(|error| VoiceControlServiceError::Reconciliation {
                committed_revision,
                message: format!("{error:?}"),
            })?;
        let expected = if effective_enabled {
            VoiceAvailability::Available
        } else {
            VoiceAvailability::Disabled
        };
        if snapshot.availability != expected || snapshot.desired_revision != committed_revision {
            return Err(VoiceControlServiceError::Reconciliation {
                committed_revision,
                message: format!(
                    "语音运行态协调未完成：availability={:?}, desiredRevision={}",
                    snapshot.availability, snapshot.desired_revision
                ),
            });
        }
        if let Err(message) = self.shortcut.set_enabled(effective_enabled) {
            log::warn!(
                "voice desired state committed at revision {committed_revision}, but shortcut health is degraded: {message}"
            );
        }
        Ok(())
    }
}

fn required_capability_for_config_change(
    current: &AppConfig,
    next: &AppConfig,
) -> Option<DesktopCapability> {
    if !current.enabled && next.enabled {
        Some(DesktopCapability::GlobalShortcut)
    } else if introduces_clipboard_compatibility(current, next) {
        Some(DesktopCapability::AutomaticTextDelivery)
    } else {
        None
    }
}

fn effective_voice_enabled(policy: &DesktopSupportPolicy, persisted_enabled: bool) -> bool {
    persisted_enabled && policy.supports(DesktopCapability::GlobalShortcut)
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

    #[test]
    fn config_preflight_allows_unrelated_saves_of_legacy_enabled_state() {
        let current = AppConfig::default();
        let mut next = current.clone();
        next.history_enabled = !current.history_enabled;

        assert_eq!(required_capability_for_config_change(&current, &next), None);
        assert!(!effective_voice_enabled(
            &DesktopSupportPolicy::macos_bootstrap(),
            next.enabled
        ));
    }

    #[test]
    fn config_preflight_identifies_new_unsupported_capabilities() {
        let current = AppConfig {
            enabled: false,
            ..AppConfig::default()
        };
        let mut enabling = current.clone();
        enabling.enabled = true;
        assert_eq!(
            required_capability_for_config_change(&current, &enabling),
            Some(DesktopCapability::GlobalShortcut)
        );

        let mut clipboard = current.clone();
        clipboard.injection_overrides.push(InjectionOverride {
            executable_name: "legacy.exe".to_string(),
            strategy: InjectionStrategy::ClipboardCompatibility,
        });
        assert_eq!(
            required_capability_for_config_change(&current, &clipboard),
            Some(DesktopCapability::AutomaticTextDelivery)
        );
    }
}
