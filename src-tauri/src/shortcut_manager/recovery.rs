use super::contract::ShortcutEditOutcome;
use super::coordinator::{
    EditCoordinator, HOOK_INTERRUPTED, RUNTIME_ROLLBACK_FAILED, TRACE_TARGET,
};
use crate::config::AppConfig;
use std::time::Instant;

impl EditCoordinator {
    pub(super) fn fail_active_edit_locked(
        &self,
        trace_id: &str,
        edit_id: u64,
        code: &'static str,
        message: &str,
        started: Instant,
        candidate_label: &str,
    ) -> ShortcutEditOutcome {
        let expected_revision = self
            .current_edit()
            .ok()
            .flatten()
            .filter(|transaction| {
                transaction.edit_id == edit_id && transaction.trace_id == trace_id
            })
            .map(|transaction| transaction.expected_revision)
            .unwrap_or_else(|| self.config.snapshot().revision);
        let _ = self.take_edit(edit_id, trace_id);
        log::warn!(
            target: TRACE_TARGET,
            "event=rollback_started traceId={} editId={} phase=rollback candidateLabel={:?} errorCode={} message={:?}",
            trace_id,
            edit_id,
            candidate_label,
            code,
            message
        );
        match self.restore_authoritative_runtime(false) {
            Ok(current) => {
                self.set_runtime_error(None);
                let outcome = self.outcome_for(
                    &current,
                    false,
                    edit_id,
                    trace_id.to_string(),
                    false,
                    Some(code),
                    message,
                );
                self.log_terminal(
                    &outcome,
                    "rollback_completed",
                    started,
                    expected_revision,
                    "success",
                );
                metrics::counter!("shortcut.operation.failed", "error_code" => code).increment(1);
                outcome
            }
            Err(rollback_error) => {
                let runtime_message = format!("{message}；恢复原快捷键失败：{rollback_error}");
                self.set_runtime_error(Some(runtime_message.clone()));
                let outcome = self.outcome_for(
                    &self.config.snapshot(),
                    false,
                    edit_id,
                    trace_id.to_string(),
                    false,
                    Some(RUNTIME_ROLLBACK_FAILED),
                    &runtime_message,
                );
                self.log_terminal(
                    &outcome,
                    "rollback_failed",
                    started,
                    expected_revision,
                    "failed",
                );
                metrics::counter!(
                    "shortcut.operation.failed",
                    "error_code" => RUNTIME_ROLLBACK_FAILED
                )
                .increment(1);
                outcome
            }
        }
    }

    pub(super) fn interrupt_active_edit_locked(&self, source: &str, message: &str) {
        let transaction = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.edit.take());
        let Some(transaction) = transaction else {
            return;
        };
        let restore = self.restore_authoritative_runtime(false);
        let (code, final_message) = match restore {
            Ok(_) => {
                self.set_runtime_error(None);
                (HOOK_INTERRUPTED, message.to_string())
            }
            Err(error) => {
                let final_message = format!("{message}；恢复快捷键失败：{error}");
                self.set_runtime_error(Some(final_message.clone()));
                (RUNTIME_ROLLBACK_FAILED, final_message)
            }
        };
        let outcome = self.outcome_for(
            &self.config.snapshot(),
            false,
            transaction.edit_id,
            transaction.trace_id,
            false,
            Some(code),
            &final_message,
        );
        log::warn!(
            target: TRACE_TARGET,
            "event=edit_interrupted traceId={} editId={} expectedRevision={} currentRevision={} source={} phase=interrupt totalDurationMs={} result=failed errorCode={} message={:?}",
            outcome.trace_id,
            outcome.edit_id,
            transaction.expected_revision,
            outcome.config_revision,
            source,
            transaction.started_at.elapsed().as_millis(),
            code,
            final_message
        );
        self.observer.emit_interrupted(outcome);
    }

    pub(super) fn restore_authoritative_runtime(
        &self,
        force_reinstall: bool,
    ) -> Result<AppConfig, String> {
        let current = self.config.snapshot();
        let engine = self.engine_handle()?;
        engine.set_enabled(false);
        engine.set_binding(current.shortcut_binding.as_ref())?;
        if current.enabled {
            if current.shortcut_binding.is_none() {
                return Err("当前快捷键无法映射为物理按键，运行时未恢复。".to_string());
            }
            engine
                .ensure_runtime_ready(force_reinstall)
                .map_err(|error| error.message)?;
            engine.set_enabled(true);
        }
        Ok(current)
    }
}
