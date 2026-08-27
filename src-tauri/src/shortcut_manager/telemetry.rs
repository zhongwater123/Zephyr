use super::contract::ShortcutEditOutcome;
use super::coordinator::{EditCoordinator, RUNTIME_ROLLBACK_FAILED, TRACE_TARGET};
use std::time::Instant;

impl EditCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn log_engine(
        &self,
        event: &str,
        trace_id: &str,
        edit_id: u64,
        expected_revision: u64,
        current_revision: u64,
        phase: &str,
        duration_ms: u128,
        result: &str,
        error_code: &str,
        rollback_result: &str,
        candidate_label: &str,
    ) {
        match self.engine_handle() {
            Ok(engine) => {
                let diagnostics = engine.diagnostics();
                let level =
                    if error_code == RUNTIME_ROLLBACK_FAILED || event == "hook_reinstall_failed" {
                        log::Level::Error
                    } else if result == "failed" {
                        log::Level::Warn
                    } else {
                        log::Level::Info
                    };
                log::log!(
                    target: TRACE_TARGET,
                    level,
                    "event={} traceId={} editId={} expectedRevision={} currentRevision={} phase={} durationMs={} totalDurationMs={} hookGeneration={} observed={} emitted={} dropped={} hookHealthy={} hookWorkerAlive={} dispatchAlive={} enabled={} candidateLabel={:?} result={} errorCode={} rollbackResult={}",
                    event,
                    trace_id,
                    edit_id,
                    expected_revision,
                    current_revision,
                    phase,
                    duration_ms,
                    duration_ms,
                    diagnostics.hook_generation,
                    diagnostics.observed_events,
                    diagnostics.emitted_events,
                    diagnostics.dropped_events,
                    diagnostics.hook_healthy,
                    diagnostics.hook_worker_alive,
                    diagnostics.dispatch_alive,
                    diagnostics.enabled,
                    candidate_label,
                    result,
                    error_code,
                    rollback_result,
                );
            }
            Err(_) => {
                let level = if error_code == RUNTIME_ROLLBACK_FAILED || result == "failed" {
                    log::Level::Error
                } else {
                    log::Level::Info
                };
                log::log!(
                    target: TRACE_TARGET,
                    level,
                    "event={} traceId={} editId={} expectedRevision={} currentRevision={} phase={} durationMs={} totalDurationMs={} engine=missing candidateLabel={:?} result={} errorCode={} rollbackResult={}",
                    event,
                    trace_id,
                    edit_id,
                    expected_revision,
                    current_revision,
                    phase,
                    duration_ms,
                    duration_ms,
                    candidate_label,
                    result,
                    error_code,
                    rollback_result,
                );
            }
        }
    }

    pub(super) fn log_terminal(
        &self,
        outcome: &ShortcutEditOutcome,
        event: &str,
        started: Instant,
        expected_revision: u64,
        rollback_result: &str,
    ) {
        self.log_engine(
            event,
            &outcome.trace_id,
            outcome.edit_id,
            expected_revision,
            outcome.config_revision,
            "terminal",
            started.elapsed().as_millis(),
            if outcome.success { "success" } else { "failed" },
            outcome.error_code.as_deref().unwrap_or("none"),
            rollback_result,
            &outcome.active_label,
        );
    }
}
