use super::contract::{ShortcutEditOutcome, ShortcutEditSession, ShortcutRuntimeState};
use super::coordinator::EditCoordinator;
use crate::config::AppConfig;
use std::time::Instant;

#[derive(Clone)]
pub(super) struct ShortcutEditTransaction {
    pub(super) edit_id: u64,
    pub(super) trace_id: String,
    pub(super) expected_revision: u64,
    pub(super) started_at: Instant,
}

pub(super) struct ManagerState {
    pub(super) next_edit_id: u64,
    pub(super) edit: Option<ShortcutEditTransaction>,
    pub(super) runtime_error: Option<String>,
}

impl EditCoordinator {
    pub(super) fn current_edit(&self) -> Result<Option<ShortcutEditTransaction>, String> {
        self.state
            .lock()
            .map(|state| state.edit.clone())
            .map_err(|error| error.to_string())
    }

    pub(super) fn take_edit(&self, edit_id: u64, trace_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.edit.as_ref().is_some_and(|transaction| {
            transaction.edit_id == edit_id && transaction.trace_id == trace_id
        }) {
            state.edit = None;
        }
        Ok(())
    }

    pub(super) fn finish_edit_success(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.edit = None;
            state.runtime_error = None;
        }
        self.publish_current_runtime_error();
    }

    pub(super) fn runtime_state(&self, config: &AppConfig) -> ShortcutRuntimeState {
        let Ok(state) = self.state.lock() else {
            return ShortcutRuntimeState::Error;
        };
        if state.runtime_error.is_some() {
            ShortcutRuntimeState::Error
        } else if !config.enabled {
            ShortcutRuntimeState::Disabled
        } else if state.edit.is_some() {
            ShortcutRuntimeState::Suspended
        } else {
            ShortcutRuntimeState::Active
        }
    }

    pub(super) fn session_for(
        &self,
        config: &AppConfig,
        edit_id: u64,
        trace_id: String,
        error_code: Option<&str>,
        message: &str,
    ) -> ShortcutEditSession {
        ShortcutEditSession {
            edit_id,
            trace_id,
            config_revision: config.revision,
            active_label: config.shortcut.clone(),
            active_binding: config.shortcut_binding.clone(),
            runtime_state: self.runtime_state(config),
            error_code: error_code.map(str::to_string),
            message: message.to_string(),
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "maps one shortcut edit result into its IPC DTO"
    )]
    pub(super) fn outcome_for(
        &self,
        config: &AppConfig,
        success: bool,
        edit_id: u64,
        trace_id: String,
        changed: bool,
        error_code: Option<&str>,
        message: &str,
    ) -> ShortcutEditOutcome {
        ShortcutEditOutcome {
            success,
            edit_id,
            trace_id,
            config_revision: config.revision,
            active_label: config.shortcut.clone(),
            active_binding: config.shortcut_binding.clone(),
            runtime_state: self.runtime_state(config),
            changed,
            error_code: error_code.map(str::to_string),
            message: message.to_string(),
        }
    }

    pub(super) fn set_runtime_error(&self, message: Option<String>) {
        if message.is_some() {
            metrics::counter!("shortcut.runtime.error").increment(1);
        }
        if let Ok(mut state) = self.state.lock() {
            state.runtime_error = message;
        }
        self.publish_current_runtime_error();
    }

    pub(super) fn publish_current_runtime_error(&self) {
        let runtime_error = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.runtime_error.clone());
        self.observer.publish_runtime_error(runtime_error);
    }
}
