use super::*;

impl VoiceSessionActor {
    pub(super) fn finish_audio_stop(
        &mut self,
        session_id: u64,
        result: Result<Duration, String>,
        presenter: &VoicePresenter,
    ) {
        let duration = match result {
            Ok(duration) => duration,
            Err(message) => {
                self.fail_active_session(
                    session_id,
                    IncidentStage::Capture,
                    "capture_stop_failed",
                    message,
                    presenter,
                );
                return;
            }
        };
        let Some(session) = self.resources.take() else {
            return;
        };
        if session.session_id != session_id {
            self.resources = Some(session);
            return;
        }
        session.deadline_cancellation.cancel();
        let queue = session.audio_queue.snapshot();
        self.record_metrics(session_id, &session.attempt_id, duration, queue);
        match reducer::audio_stopped(&mut self.runtime, session_id, duration) {
            Some(ReleaseDecision::Cancelled { .. }) => {
                let mut incident = IncidentAttemptGuard::new(
                    self.services.incidents.sink(),
                    session.attempt_id.clone(),
                );
                incident.stage(
                    IncidentStage::Capture,
                    StageOutcome::Cancelled,
                    Some("recording_too_short".to_string()),
                );
                incident.finish(TerminalOutcome::Cancelled, false);
                session.cancellation.cancel();
                session.provider_task.abort();
                reducer::record_outcome(
                    &mut self.runtime,
                    session_id,
                    "cancelled",
                    Some("recording_too_short"),
                );
                presenter.hide(session_id);
                presenter.emit_state(self.runtime.snapshot().payload);
            }
            Some(ReleaseDecision::Transcribe { payload, .. }) => {
                session.state_tx.send_replace(VoiceState::Transcribing);
                let method = match session
                    .config
                    .injection_strategy_for(&session.target.executable_name)
                {
                    InjectionStrategy::Unicode => InjectionMethod::Unicode,
                    InjectionStrategy::ClipboardCompatibility => {
                        InjectionMethod::ClipboardCompatibility
                    }
                };
                self.finalizing_cancellation = Some((session_id, session.cancellation.clone()));
                presenter.emit_state(payload);
                workflow::spawn_finalization(
                    FinalizationJob {
                        session,
                        injector: self.injector.clone(),
                        injection_method: method,
                        services: self.services.clone(),
                    },
                    self.events.clone(),
                );
            }
            None => {
                session.cancellation.cancel();
                session.provider_task.abort();
            }
        }
    }

    pub(super) fn finish_finalization(
        &mut self,
        outcome: FinalizeOutcome,
        presenter: &VoicePresenter,
    ) {
        let session_id = outcome.session_id();
        if self.runtime.current_id() != Some(session_id) {
            return;
        }
        self.finalizing_cancellation = None;
        match outcome {
            FinalizeOutcome::Delivered {
                history_committed, ..
            } => {
                if !history_committed {
                    log::warn!("voice text delivered but history was not committed");
                }
                reducer::record_outcome(&mut self.runtime, session_id, "delivered", None);
                self.complete_session(session_id, presenter, None);
            }
            FinalizeOutcome::Pending { draft, .. } => {
                let reason_code = draft.reason_code.clone();
                match self.pending.push(
                    session_id,
                    draft.text,
                    draft.target,
                    &reason_code,
                    draft.reason_message,
                ) {
                    Ok(_) => {
                        reducer::record_outcome(
                            &mut self.runtime,
                            session_id,
                            "pending",
                            Some(&reason_code),
                        );
                        self.complete_session(
                            session_id,
                            presenter,
                            Some("结果已进入待处理区".to_string()),
                        );
                        presenter.pending_changed();
                    }
                    Err(_) => self.fail_without_resources(
                        session_id,
                        "待处理结果已满，无法保留本次结果".to_string(),
                        presenter,
                    ),
                }
            }
            FinalizeOutcome::Cancelled { reason, .. } => {
                reducer::record_outcome(&mut self.runtime, session_id, "cancelled", Some(&reason));
                self.complete_session(session_id, presenter, None);
            }
            FinalizeOutcome::Failed {
                reason_code,
                message,
                ..
            } => {
                reducer::record_outcome(
                    &mut self.runtime,
                    session_id,
                    "failed",
                    Some(&reason_code),
                );
                self.fail_without_resources(session_id, message, presenter);
            }
        }
    }
}
