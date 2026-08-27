use super::contract::ShortcutEditOutcome;
use super::coordinator::{
    EditCoordinator, HOOK_INTERRUPTED, HOOK_UNAVAILABLE, PERSISTENCE_FAILED, REVISION_CONFLICT,
    TRACE_TARGET,
};
use super::ports::{ShortcutRuntimePort, ShortcutStoreFailure};
use super::validation::{validate_candidate, validate_trace_id};
use crate::config::{AppConfig, CURRENT_SCHEMA_VERSION};
use crate::physical_shortcut::ShortcutBinding;
use std::sync::Arc;
use std::time::Instant;

struct CommitAttempt {
    trace_id: String,
    edit_id: u64,
    expected_revision: u64,
    started: Instant,
    candidate_label: String,
    current: AppConfig,
}

impl EditCoordinator {
    pub(super) fn commit_edit(
        &self,
        trace_id: String,
        edit_id: u64,
        expected_revision: u64,
        binding: ShortcutBinding,
    ) -> Result<ShortcutEditOutcome, String> {
        validate_trace_id(&trace_id)?;
        let _gate = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        let attempt = CommitAttempt {
            candidate_label: binding.display_label(),
            current: self.config.snapshot(),
            trace_id,
            edit_id,
            expected_revision,
            started: Instant::now(),
        };
        log::info!(
            target: TRACE_TARGET,
            "event=commit_requested traceId={} editId={} expectedRevision={} currentRevision={} phase=commit candidateLabel={:?} enabled={}",
            attempt.trace_id,
            attempt.edit_id,
            attempt.expected_revision,
            attempt.current.revision,
            attempt.candidate_label,
            attempt.current.enabled
        );

        let Some(transaction) = self.current_edit()? else {
            return Ok(self.outcome_for(
                &attempt.current,
                false,
                attempt.edit_id,
                attempt.trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        };
        if transaction.edit_id != attempt.edit_id || transaction.trace_id != attempt.trace_id {
            return Ok(self.outcome_for(
                &attempt.current,
                false,
                attempt.edit_id,
                attempt.trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        }
        if transaction.expected_revision != attempt.expected_revision {
            return Ok(self.fail_commit(
                &attempt,
                REVISION_CONFLICT,
                "换绑会话的配置版本不一致，请重新录入。",
            ));
        }
        if attempt.current.revision != attempt.expected_revision {
            return Ok(self.fail_commit(
                &attempt,
                REVISION_CONFLICT,
                "配置已被其他操作更新，请重新录入。",
            ));
        }
        if let Err(failure) = validate_candidate(&binding) {
            log::warn!(
                target: TRACE_TARGET,
                "event=validation_failed traceId={} editId={} expectedRevision={} currentRevision={} phase=validation durationMs={} candidateLabel={:?} result=failed errorCode={} message={:?}",
                attempt.trace_id,
                attempt.edit_id,
                attempt.expected_revision,
                attempt.current.revision,
                attempt.started.elapsed().as_millis(),
                attempt.candidate_label,
                failure.code,
                failure.message
            );
            return Ok(self.fail_commit(&attempt, failure.code, &failure.message));
        }
        log::debug!(
            target: TRACE_TARGET,
            "event=validation_completed traceId={} editId={} phase=validation durationMs={} candidateLabel={:?}",
            attempt.trace_id,
            attempt.edit_id,
            attempt.started.elapsed().as_millis(),
            attempt.candidate_label
        );

        let engine = match self.engine_handle() {
            Ok(engine) => engine,
            Err(message) => return Ok(self.fail_commit(&attempt, HOOK_UNAVAILABLE, &message)),
        };
        let unchanged = attempt
            .current
            .shortcut_binding
            .as_ref()
            .is_some_and(|active| active.physically_equivalent(&binding));
        if unchanged {
            return Ok(self.finish_unchanged(attempt, &engine));
        }
        if let Err(outcome) = self.prepare_runtime_for_commit(&attempt, &engine) {
            return Ok(*outcome);
        }
        if let Err(outcome) = self.apply_runtime_binding(&attempt, &engine, &binding) {
            return Ok(*outcome);
        }
        Ok(self.persist_binding(attempt, binding))
    }

    fn fail_commit(
        &self,
        attempt: &CommitAttempt,
        code: &'static str,
        message: &str,
    ) -> ShortcutEditOutcome {
        self.fail_active_edit_locked(
            &attempt.trace_id,
            attempt.edit_id,
            code,
            message,
            attempt.started,
            &attempt.candidate_label,
        )
    }

    fn finish_unchanged(
        &self,
        attempt: CommitAttempt,
        engine: &Arc<dyn ShortcutRuntimePort>,
    ) -> ShortcutEditOutcome {
        if attempt.current.enabled {
            if let Err(error) = engine.ensure_runtime_ready(false) {
                return self.fail_commit(&attempt, HOOK_UNAVAILABLE, &error.message);
            }
            engine.set_enabled(true);
        }
        self.finish_edit_success();
        let outcome = self.outcome_for(
            &attempt.current,
            true,
            attempt.edit_id,
            attempt.trace_id,
            false,
            None,
            "快捷键未变化。",
        );
        self.log_terminal(
            &outcome,
            "commit_completed",
            attempt.started,
            attempt.expected_revision,
            "none",
        );
        outcome
    }

    fn prepare_runtime_for_commit(
        &self,
        attempt: &CommitAttempt,
        engine: &Arc<dyn ShortcutRuntimePort>,
    ) -> Result<(), Box<ShortcutEditOutcome>> {
        if !attempt.current.enabled {
            return Ok(());
        }
        let hook_started = Instant::now();
        self.log_engine(
            "hook_reinstall_requested",
            &attempt.trace_id,
            attempt.edit_id,
            attempt.expected_revision,
            attempt.current.revision,
            "hook",
            hook_started.elapsed().as_millis(),
            "started",
            "none",
            "none",
            &attempt.candidate_label,
        );
        match engine.ensure_runtime_ready(true) {
            Ok(_) => {
                self.log_engine(
                    "hook_reinstall_completed",
                    &attempt.trace_id,
                    attempt.edit_id,
                    attempt.expected_revision,
                    attempt.current.revision,
                    "hook",
                    hook_started.elapsed().as_millis(),
                    "success",
                    "none",
                    "none",
                    &attempt.candidate_label,
                );
                Ok(())
            }
            Err(error) => {
                log::error!(
                    target: TRACE_TARGET,
                    "event=hook_reinstall_failed_detail traceId={} editId={} phase=hook durationMs={} errorKind={:?} error={:?}",
                    attempt.trace_id,
                    attempt.edit_id,
                    hook_started.elapsed().as_millis(),
                    error.kind,
                    error.message
                );
                self.log_engine(
                    "hook_reinstall_failed",
                    &attempt.trace_id,
                    attempt.edit_id,
                    attempt.expected_revision,
                    attempt.current.revision,
                    "hook",
                    hook_started.elapsed().as_millis(),
                    "failed",
                    HOOK_UNAVAILABLE,
                    "pending",
                    &attempt.candidate_label,
                );
                Err(Box::new(self.fail_commit(
                    attempt,
                    HOOK_UNAVAILABLE,
                    &error.message,
                )))
            }
        }
    }

    fn apply_runtime_binding(
        &self,
        attempt: &CommitAttempt,
        engine: &Arc<dyn ShortcutRuntimePort>,
        binding: &ShortcutBinding,
    ) -> Result<(), Box<ShortcutEditOutcome>> {
        engine.set_enabled(false);
        if let Err(message) = engine.set_binding(Some(binding)) {
            return Err(Box::new(self.fail_commit(
                attempt,
                HOOK_UNAVAILABLE,
                &message,
            )));
        }
        engine.set_enabled(attempt.current.enabled);
        self.log_engine(
            "runtime_binding_applied",
            &attempt.trace_id,
            attempt.edit_id,
            attempt.expected_revision,
            attempt.current.revision,
            "runtime_apply",
            attempt.started.elapsed().as_millis(),
            "success",
            "none",
            "none",
            &attempt.candidate_label,
        );
        Ok(())
    }

    fn persist_binding(
        &self,
        attempt: CommitAttempt,
        binding: ShortcutBinding,
    ) -> ShortcutEditOutcome {
        let persistence_started = Instant::now();
        log::info!(
            target: TRACE_TARGET,
            "event=persistence_started traceId={} editId={} phase=persistence candidateLabel={:?}",
            attempt.trace_id,
            attempt.edit_id,
            attempt.candidate_label
        );
        let mut next = attempt.current.clone();
        next.shortcut = attempt.candidate_label.clone();
        next.shortcut_binding = Some(binding);
        next.schema_version = CURRENT_SCHEMA_VERSION;
        next.revision = next.revision.saturating_add(1);
        match self.config.commit_shortcut(attempt.expected_revision, next) {
            Ok(committed) => {
                self.finish_edit_success();
                log::info!(
                    target: TRACE_TARGET,
                    "event=persistence_completed traceId={} editId={} phase=persistence durationMs={} currentRevision={} result=success",
                    attempt.trace_id,
                    attempt.edit_id,
                    persistence_started.elapsed().as_millis(),
                    committed.revision
                );
                let outcome = self.outcome_for(
                    &committed,
                    true,
                    attempt.edit_id,
                    attempt.trace_id,
                    true,
                    None,
                    if attempt.current.enabled {
                        "快捷键已更新。"
                    } else {
                        "快捷键已保存，开启后生效。"
                    },
                );
                self.log_terminal(
                    &outcome,
                    "commit_completed",
                    attempt.started,
                    attempt.expected_revision,
                    "none",
                );
                metrics::counter!("shortcut.operation.completed", "kind" => "edit").increment(1);
                outcome
            }
            Err(error) => {
                let (code, message) = match error {
                    ShortcutStoreFailure::Conflict => (
                        REVISION_CONFLICT,
                        "配置已被其他操作更新，请重新录入。".to_string(),
                    ),
                    ShortcutStoreFailure::Storage(message) => (PERSISTENCE_FAILED, message),
                };
                log::warn!(
                    target: TRACE_TARGET,
                    "event=persistence_failed traceId={} editId={} phase=persistence durationMs={} result=failed errorCode={} message={:?}",
                    attempt.trace_id,
                    attempt.edit_id,
                    persistence_started.elapsed().as_millis(),
                    code,
                    message
                );
                self.fail_commit(&attempt, code, &message)
            }
        }
    }
}
