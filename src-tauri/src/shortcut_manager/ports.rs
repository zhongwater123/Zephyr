use crate::config::AppConfig;
use crate::physical_shortcut::ShortcutBinding;
use crate::services::{ConfigService, ConfigServiceError};
pub(super) use crate::shortcut_runtime::ShortcutRuntimePort;
use crate::shortcut_runtime::{KeyboardEngineDiagnostics, KeyboardEngineError};
#[cfg(target_os = "windows")]
use crate::windows_keyboard::WindowsKeyboardEngine;

use super::contract::ShortcutEditOutcome;

#[derive(Debug)]
pub(super) enum ShortcutStoreFailure {
    Conflict,
    Storage(String),
}

pub(super) trait ShortcutConfigPort: Send + Sync {
    fn snapshot(&self) -> AppConfig;

    fn commit_shortcut(
        &self,
        expected_revision: u64,
        next: AppConfig,
    ) -> Result<AppConfig, ShortcutStoreFailure>;
}

impl ShortcutConfigPort for ConfigService {
    fn snapshot(&self) -> AppConfig {
        ConfigService::snapshot(self)
    }

    fn commit_shortcut(
        &self,
        expected_revision: u64,
        next: AppConfig,
    ) -> Result<AppConfig, ShortcutStoreFailure> {
        self.commit_config(expected_revision, next)
            .map_err(|error| match error {
                ConfigServiceError::Conflict(_) => ShortcutStoreFailure::Conflict,
                ConfigServiceError::Storage(error) => {
                    ShortcutStoreFailure::Storage(error.to_string())
                }
            })
    }
}

#[cfg(target_os = "windows")]
impl ShortcutRuntimePort for WindowsKeyboardEngine {
    fn startup_error(&self) -> Option<String> {
        WindowsKeyboardEngine::startup_error(self)
    }

    fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
        WindowsKeyboardEngine::set_binding(self, binding)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        WindowsKeyboardEngine::set_enabled(self, enabled);
        Ok(())
    }

    fn ensure_runtime_ready(&self, force_reinstall: bool) -> Result<u64, KeyboardEngineError> {
        WindowsKeyboardEngine::ensure_runtime_ready(self, force_reinstall)
    }

    fn is_healthy(&self) -> bool {
        WindowsKeyboardEngine::is_healthy(self)
    }

    fn diagnostics(&self) -> KeyboardEngineDiagnostics {
        WindowsKeyboardEngine::diagnostics(self)
    }

    fn shutdown(&self) {
        WindowsKeyboardEngine::shutdown(self);
    }
}

pub(super) trait ShortcutObserverPort: Send + Sync {
    fn publish_runtime_error(&self, message: Option<String>);
    fn emit_interrupted(&self, outcome: ShortcutEditOutcome);
}
