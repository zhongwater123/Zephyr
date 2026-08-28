use crate::incident::model::TerminalOutcome;
use crate::incident::model::{IncidentEvent, Recoverability};
use crate::incident::model::{Stage as IncidentStage, StageOutcome as IncidentStageOutcome};
use std::sync::Arc;

pub(super) struct IncidentAttemptGuard {
    sink: Arc<dyn crate::incident::IncidentSink>,
    attempt_id: String,
    finished: bool,
    finding_recorded: bool,
}

impl IncidentAttemptGuard {
    pub(super) fn new(sink: Arc<dyn crate::incident::IncidentSink>, attempt_id: String) -> Self {
        Self {
            sink,
            attempt_id,
            finished: false,
            finding_recorded: false,
        }
    }

    pub(super) fn stage(
        &self,
        stage: IncidentStage,
        outcome: IncidentStageOutcome,
        reason_code: Option<String>,
    ) {
        let _ = self.sink.try_emit(IncidentEvent::StageChanged {
            attempt_id: self.attempt_id.clone(),
            stage,
            outcome,
            reason_code,
            monotonic_us: 0,
        });
    }

    pub(super) fn finding(
        &mut self,
        stage: IncidentStage,
        code: &str,
        message: &str,
        recoverability: Recoverability,
    ) {
        self.finding_recorded = true;
        let _ = self.sink.try_emit(IncidentEvent::Finding {
            attempt_id: self.attempt_id.clone(),
            stage,
            code: code.to_string(),
            message: message.to_string(),
            severity: "error",
            recoverability,
        });
    }

    pub(super) fn record_failure(
        &mut self,
        stage: IncidentStage,
        code: &str,
        message: &str,
        recoverability: Recoverability,
    ) {
        self.stage(stage, IncidentStageOutcome::Failed, Some(code.to_string()));
        self.finding(stage, code, message, recoverability);
    }

    pub(super) fn cancel(&mut self, stage: IncidentStage, code: &str) {
        self.stage(
            stage,
            IncidentStageOutcome::Cancelled,
            Some(code.to_string()),
        );
        self.finish(TerminalOutcome::Cancelled, false);
    }

    pub(super) fn final_transcript(&self, text: &str, monotonic_us: u64) {
        let _ = self.sink.try_emit(IncidentEvent::FinalTranscript {
            attempt_id: self.attempt_id.clone(),
            text: text.to_string(),
            monotonic_us,
        });
    }

    pub(super) fn finish(&mut self, outcome: TerminalOutcome, history_committed: bool) {
        let discard_recovery_material = outcome == TerminalOutcome::Succeeded && history_committed;
        self.finish_with_recovery_policy(outcome, history_committed, discard_recovery_material);
    }

    pub(super) fn finish_delivered(
        &mut self,
        history_committed: bool,
        discard_recovery_material: bool,
    ) {
        self.finish_with_recovery_policy(
            TerminalOutcome::Succeeded,
            history_committed,
            discard_recovery_material,
        );
    }

    fn finish_with_recovery_policy(
        &mut self,
        outcome: TerminalOutcome,
        history_committed: bool,
        discard_recovery_material: bool,
    ) {
        metrics::counter!("voice.sessions.completed").increment(1);
        let _ = self.sink.try_emit(IncidentEvent::AttemptEnded {
            attempt_id: self.attempt_id.clone(),
            outcome,
            history_committed,
            discard_recovery_material,
            ended_at_utc_ms: chrono::Utc::now().timestamp_millis(),
        });
        self.finished = true;
    }
}

impl Drop for IncidentAttemptGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        if !self.finding_recorded {
            self.finding(
                IncidentStage::Runtime,
                "pipeline_incomplete",
                "会话在完成正常提交前结束",
                Recoverability::TextAndAudio,
            );
        }
        metrics::counter!("voice.sessions.completed").increment(1);
        let _ = self.sink.try_emit(IncidentEvent::AttemptEnded {
            attempt_id: self.attempt_id.clone(),
            outcome: TerminalOutcome::Failed,
            history_committed: false,
            discard_recovery_material: false,
            ended_at_utc_ms: chrono::Utc::now().timestamp_millis(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct CollectingIncidentSink {
        events: std::sync::Mutex<Vec<String>>,
    }

    impl crate::incident::IncidentSink for CollectingIncidentSink {
        fn try_emit(&self, event: IncidentEvent) -> crate::incident::model::EmitOutcome {
            let label = match event {
                IncidentEvent::StageChanged {
                    stage,
                    outcome,
                    reason_code,
                    ..
                } => {
                    format!(
                        "stage:{}:{}:{}",
                        stage.as_str(),
                        outcome.as_str(),
                        reason_code.unwrap_or_default()
                    )
                }
                IncidentEvent::Finding { code, .. } => format!("finding:{code}"),
                IncidentEvent::FinalTranscript {
                    text, monotonic_us, ..
                } => {
                    format!("final:{text}:{monotonic_us}")
                }
                IncidentEvent::AttemptEnded {
                    outcome,
                    history_committed,
                    discard_recovery_material,
                    ..
                } => {
                    format!(
                        "end:{}:{history_committed}:{discard_recovery_material}",
                        outcome.as_str()
                    )
                }
                _ => "other".to_string(),
            };
            self.events.lock().unwrap().push(label);
            crate::incident::model::EmitOutcome::Accepted
        }

        fn health_snapshot(&self) -> crate::incident::model::IncidentHealth {
            crate::incident::model::IncidentHealth::default()
        }
    }

    #[test]
    fn guard_emits_failure_before_one_terminal_event() {
        let sink = Arc::new(CollectingIncidentSink::default());
        {
            let mut guard = IncidentAttemptGuard::new(sink.clone(), "attempt".to_string());
            guard.record_failure(
                IncidentStage::Asr,
                "asr_timeout",
                "timeout",
                Recoverability::Audio,
            );
            guard.finish(TerminalOutcome::Failed, false);
        }
        assert_eq!(
            sink.events.lock().unwrap().as_slice(),
            [
                "stage:asr:failed:asr_timeout",
                "finding:asr_timeout",
                "end:failed:false:false",
            ]
        );
    }

    #[test]
    fn unfinished_guard_records_pipeline_incomplete_once() {
        let sink = Arc::new(CollectingIncidentSink::default());
        {
            let _guard = IncidentAttemptGuard::new(sink.clone(), "attempt".to_string());
        }
        assert_eq!(
            sink.events.lock().unwrap().as_slice(),
            ["finding:pipeline_incomplete", "end:failed:false:false"]
        );
    }
}
