use super::*;

impl VoiceSessionActor {
    pub(super) fn complete_session(
        &mut self,
        session_id: u64,
        presenter: &VoicePresenter,
        message: Option<String>,
    ) {
        let Some(mut payload) = reducer::complete(&mut self.runtime, session_id) else {
            return;
        };
        if let Some(message) = message {
            payload = reducer::override_message(&mut self.runtime, payload, message);
        }
        presenter.hide(session_id);
        presenter.emit_state(payload);
    }

    pub(super) fn cancel_resources(&mut self, session_id: u64, reason: &str) -> Result<(), String> {
        if self
            .starting
            .as_ref()
            .is_some_and(|starting| starting.session_id == session_id)
        {
            if let Some(starting) = self.starting.take() {
                starting.cancellation.cancel();
            }
        }
        if self
            .resources
            .as_ref()
            .is_some_and(|resources| resources.session_id == session_id)
        {
            if let Some(session) = self.resources.take() {
                let stage = if self.runtime.phase == VoicePhase::Recording {
                    IncidentStage::Capture
                } else {
                    IncidentStage::Runtime
                };
                let mut incident = IncidentAttemptGuard::new(
                    self.services.incidents.sink(),
                    session.attempt_id.clone(),
                );
                incident.cancel(stage, reason);
                session.cancellation.cancel();
                session.deadline_cancellation.cancel();
                session.provider_task.abort();
            }
        }
        if let Some((current, cancellation)) = &self.finalizing_cancellation {
            if *current == session_id {
                cancellation.cancel();
                if self.runtime.phase != VoicePhase::Pasting {
                    self.finalizing_cancellation = None;
                }
            }
        }
        self.audio.request_cancel(session_id)
    }

    pub(super) fn provider_failed(
        &mut self,
        session_id: u64,
        error: crate::provider::ProviderError,
        presenter: &VoicePresenter,
    ) {
        if self.runtime.current_id() != Some(session_id) || self.resources.is_none() {
            return;
        }
        let reason = error.cancel_reason().to_string();
        let message = error.user_message();
        self.fail_active_session(session_id, IncidentStage::Asr, &reason, message, presenter);
    }

    pub(super) fn audio_overflow(&mut self, session_id: u64, presenter: &VoicePresenter) {
        if self.runtime.current_id() != Some(session_id) {
            return;
        }
        self.fail_active_session(
            session_id,
            IncidentStage::Capture,
            "audio_queue_overflow",
            "网络处理过慢，录音已取消".to_string(),
            presenter,
        );
    }

    pub(super) fn fail_active_session(
        &mut self,
        session_id: u64,
        stage: IncidentStage,
        reason_code: &str,
        message: String,
        presenter: &VoicePresenter,
    ) {
        let Some(session) = self.resources.take() else {
            self.fail_without_resources(session_id, message, presenter);
            return;
        };
        if session.session_id != session_id {
            self.resources = Some(session);
            return;
        }
        let queue = session.audio_queue.snapshot();
        let duration = session.started_at.elapsed();
        let mut incident =
            IncidentAttemptGuard::new(self.services.incidents.sink(), session.attempt_id.clone());
        incident.record_failure(stage, reason_code, &message, Recoverability::TextAndAudio);
        incident.finish(TerminalOutcome::Failed, false);
        session.cancellation.cancel();
        session.deadline_cancellation.cancel();
        session.provider_task.abort();
        self.record_metrics(session_id, &session.attempt_id, duration, queue);
        reducer::record_outcome(&mut self.runtime, session_id, "failed", Some(reason_code));
        if let Err(error) = self.audio.request_cancel(session_id) {
            log::warn!("failed to request failed-session audio cancellation: {error}");
        }
        self.fail_without_resources(session_id, message, presenter);
    }

    pub(super) fn fail_without_resources(
        &mut self,
        session_id: u64,
        message: String,
        presenter: &VoicePresenter,
    ) {
        if let Some(payload) = reducer::fail(&mut self.runtime, session_id, message.clone()) {
            presenter.show_error(session_id, message);
            presenter.emit_state(payload);
            self.schedule_error_reset(session_id);
        }
    }

    pub(super) fn record_metrics(
        &mut self,
        session_id: u64,
        attempt_id: &str,
        duration: Duration,
        queue: crate::audio::AudioQueueSnapshot,
    ) {
        metrics::histogram!("voice.recording.duration_ms").record(duration.as_millis() as f64);
        metrics::gauge!("voice.audio_queue.high_watermark").set(queue.high_watermark as f64);
        let sink = self.services.incidents.sink();
        let _ = sink.try_emit(IncidentEvent::Metric {
            attempt_id: attempt_id.to_string(),
            name: "recording_duration_ms",
            value: duration.as_millis() as f64,
            unit: "milliseconds",
        });
        let _ = sink.try_emit(IncidentEvent::Metric {
            attempt_id: attempt_id.to_string(),
            name: "audio_queue_high_watermark",
            value: queue.high_watermark as f64,
            unit: "packets",
        });
        reducer::record_metrics(
            &mut self.runtime,
            SessionMetrics {
                session_id,
                audio_packets: queue.packets,
                queue_high_watermark: queue.high_watermark,
                overflow: queue.overflow,
                recording_duration_ms: duration.as_millis() as u64,
                cancel_reason: None,
                final_state: "transcribing".to_string(),
            },
        );
    }
}
