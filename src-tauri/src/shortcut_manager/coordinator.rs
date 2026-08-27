//! Shortcut edit state, persistence sequencing, runtime application, and rollback.

use super::contract::{ShortcutEditOutcome, ShortcutEditSession, ShortcutEditTraceInput};
use super::ports::{ShortcutConfigPort, ShortcutObserverPort, ShortcutRuntimePort};
use super::state::{ManagerState, ShortcutEditTransaction};
use super::validation::validate_trace_id;

use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(super) const TRACE_TARGET: &str = "shortcut_edit_trace";
pub(super) const REVISION_CONFLICT: &str = "revision_conflict";
pub(super) const HOOK_UNAVAILABLE: &str = "hook_unavailable";
pub(super) const PERSISTENCE_FAILED: &str = "persistence_failed";
pub(super) const HOOK_INTERRUPTED: &str = "hook_interrupted";
pub(super) const RUNTIME_ROLLBACK_FAILED: &str = "runtime_rollback_failed";

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

pub(super) struct EditCoordinator {
    pub(super) config: Arc<dyn ShortcutConfigPort>,
    pub(super) operation_gate: Mutex<()>,
    pub(super) state: Mutex<ManagerState>,
    pub(super) engine: Mutex<Option<Arc<dyn ShortcutRuntimePort>>>,
    pub(super) observer: Arc<dyn ShortcutObserverPort>,
}

impl EditCoordinator {
    pub(super) fn new(
        config: Arc<dyn ShortcutConfigPort>,
        engine: Option<Arc<dyn ShortcutRuntimePort>>,
        initial_error: Option<String>,
        observer: Arc<dyn ShortcutObserverPort>,
    ) -> Self {
        Self {
            config,
            operation_gate: Mutex::new(()),
            state: Mutex::new(ManagerState {
                next_edit_id: 0,
                edit: None,
                runtime_error: initial_error.clone(),
            }),
            engine: Mutex::new(engine),
            observer,
        }
    }

    pub(super) fn begin_edit(
        &self,
        trace_id: String,
        expected_revision: u64,
    ) -> Result<ShortcutEditSession, String> {
        validate_trace_id(&trace_id)?;
        let _gate = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let current = self.config.snapshot();
        log::info!(
            target: TRACE_TARGET,
            "event=edit_begin_requested traceId={} editId=none expectedRevision={} currentRevision={} phase=begin enabled={}",
            trace_id,
            expected_revision,
            current.revision,
            current.enabled
        );

        let existing = self.current_edit()?;
        if let Some(existing) = existing.as_ref() {
            if existing.trace_id == trace_id
                && existing.expected_revision == expected_revision
                && current.revision == existing.expected_revision
            {
                let mut session = self.session_for(
                    &current,
                    existing.edit_id,
                    trace_id,
                    None,
                    "正在录入新的快捷键。",
                );
                session.config_revision = existing.expected_revision;
                return Ok(session);
            }
        }

        if current.revision != expected_revision {
            metrics::counter!("shortcut.operation.failed", "error_code" => REVISION_CONFLICT)
                .increment(1);
            log::warn!(
                target: TRACE_TARGET,
                "event=edit_begin_failed traceId={} editId=0 expectedRevision={} currentRevision={} phase=revision durationMs={} result=failed errorCode={}",
                trace_id,
                expected_revision,
                current.revision,
                started.elapsed().as_millis(),
                REVISION_CONFLICT
            );
            return Ok(self.session_for(
                &current,
                0,
                trace_id,
                Some(REVISION_CONFLICT),
                "配置已被其他操作更新，请刷新后重试。",
            ));
        }

        if existing.is_some() {
            self.interrupt_active_edit_locked("superseded", "新的换绑会话中断了上一轮录入。");
        }
        let engine = match self.engine_handle() {
            Ok(engine) => engine,
            Err(message) => {
                self.set_runtime_error(Some(message.clone()));
                self.log_engine(
                    "edit_begin_failed",
                    &trace_id,
                    0,
                    expected_revision,
                    current.revision,
                    "begin",
                    started.elapsed().as_millis(),
                    "failed",
                    HOOK_UNAVAILABLE,
                    "failed",
                    &current.shortcut,
                );
                return Ok(self.session_for(
                    &current,
                    0,
                    trace_id,
                    Some(HOOK_UNAVAILABLE),
                    &message,
                ));
            }
        };
        engine.set_enabled(false);
        let edit_id = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.next_edit_id = state.next_edit_id.saturating_add(1).max(1);
            let edit_id = state.next_edit_id;
            state.edit = Some(ShortcutEditTransaction {
                edit_id,
                trace_id: trace_id.clone(),
                expected_revision,
                started_at: Instant::now(),
            });
            edit_id
        };
        metrics::counter!("shortcut.operation.started", "kind" => "edit").increment(1);
        self.log_engine(
            "runtime_suspended",
            &trace_id,
            edit_id,
            expected_revision,
            current.revision,
            "begin",
            started.elapsed().as_millis(),
            "success",
            "none",
            "none",
            &current.shortcut,
        );
        self.log_engine(
            "edit_begin_completed",
            &trace_id,
            edit_id,
            expected_revision,
            current.revision,
            "begin",
            started.elapsed().as_millis(),
            "success",
            "none",
            "none",
            &current.shortcut,
        );
        Ok(self.session_for(&current, edit_id, trace_id, None, "正在录入新的快捷键。"))
    }

    pub(super) fn cancel_edit(
        &self,
        trace_id: String,
        edit_id: u64,
    ) -> Result<ShortcutEditOutcome, String> {
        validate_trace_id(&trace_id)?;
        let _gate = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        let started = Instant::now();
        let current = self.config.snapshot();
        log::info!(
            target: TRACE_TARGET,
            "event=cancel_requested traceId={} editId={} expectedRevision=none currentRevision={} phase=cancel",
            trace_id,
            edit_id,
            current.revision
        );
        let Some(transaction) = self.current_edit()? else {
            return Ok(self.outcome_for(
                &current,
                false,
                edit_id,
                trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        };
        if transaction.trace_id != trace_id || (edit_id != 0 && transaction.edit_id != edit_id) {
            return Ok(self.outcome_for(
                &current,
                false,
                edit_id,
                trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        }
        let edit_id = transaction.edit_id;
        self.take_edit(edit_id, &trace_id)?;
        match self.restore_authoritative_runtime(false) {
            Ok(restored) => {
                self.set_runtime_error(None);
                let outcome = self.outcome_for(
                    &restored,
                    true,
                    edit_id,
                    trace_id,
                    false,
                    None,
                    "已取消，原快捷键保持不变。",
                );
                self.log_terminal(
                    &outcome,
                    "edit_cancelled",
                    started,
                    transaction.expected_revision,
                    "success",
                );
                metrics::counter!("shortcut.operation.cancelled", "kind" => "edit").increment(1);
                Ok(outcome)
            }
            Err(message) => {
                self.set_runtime_error(Some(message.clone()));
                let outcome = self.outcome_for(
                    &self.config.snapshot(),
                    false,
                    edit_id,
                    trace_id,
                    false,
                    Some(RUNTIME_ROLLBACK_FAILED),
                    &format!("取消换绑后无法恢复原快捷键：{message}"),
                );
                self.log_terminal(
                    &outcome,
                    "rollback_failed",
                    started,
                    transaction.expected_revision,
                    "failed",
                );
                Ok(outcome)
            }
        }
    }

    pub(super) fn record_trace(&self, input: ShortcutEditTraceInput) -> Result<(), String> {
        input.validate()?;
        log::debug!(
            target: TRACE_TARGET,
            "event=frontend_trace traceId={} editId={} eventSeq={} clientElapsedMs={} phase={} code={:?} key={:?} location={:?} repeat={:?} ctrl={:?} alt={:?} shift={:?} meta={:?} altGraph={:?} heldCodes={:?} candidateLabel={:?} candidateBinding={:?} reasonCode={:?}",
            input.trace_id,
            input.edit_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into()),
            input.event_seq,
            input.elapsed_ms,
            input.event.as_str(),
            input.code,
            input.key,
            input.location,
            input.repeat,
            input.ctrl,
            input.alt,
            input.shift,
            input.meta,
            input.alt_graph,
            input.held_codes,
            input.candidate_label,
            input.candidate_binding,
            input.reason_code,
        );
        Ok(())
    }

    pub(super) fn engine_handle(&self) -> Result<Arc<dyn ShortcutRuntimePort>, String> {
        self.engine
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
            .cloned()
            .ok_or_else(|| "物理快捷键引擎未运行。".to_string())
    }
}
