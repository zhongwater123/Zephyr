use crate::config::AppConfig;
use crate::physical_shortcut::ShortcutBinding;
use crate::services::{ConfigService, ConfigServiceError};
use crate::windows_keyboard::{
    KeyboardEngineDiagnostics, KeyboardEngineError, WindowsKeyboardEngine,
};

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

pub(super) trait ShortcutRuntimePort: Send + Sync {
    fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String>;
    fn set_enabled(&self, enabled: bool);
    fn ensure_runtime_ready(&self, force_reinstall: bool) -> Result<u64, KeyboardEngineError>;
    fn is_healthy(&self) -> bool;
    fn diagnostics(&self) -> KeyboardEngineDiagnostics;
    fn shutdown(&self);
}

impl ShortcutRuntimePort for WindowsKeyboardEngine {
    fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
        WindowsKeyboardEngine::set_binding(self, binding)
    }

    fn set_enabled(&self, enabled: bool) {
        WindowsKeyboardEngine::set_enabled(self, enabled);
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
