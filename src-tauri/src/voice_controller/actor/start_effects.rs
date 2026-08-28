use super::*;

impl VoiceSessionActor {
    pub(super) fn execute_effects(&mut self, effects: Vec<Effect>, presenter: &VoicePresenter) {
        for effect in effects {
            match effect {
                Effect::StartSession { session_id } => {
                    let Some(starting) = &self.starting else {
                        continue;
                    };
                    if starting.session_id != session_id {
                        continue;
                    }
                    workflow::spawn_start(
                        self.audio.clone(),
                        self.services.clone(),
                        starting.config.clone(),
                        session_id,
                        starting.cancellation.clone(),
                        self.events.clone(),
                    );
                }
                Effect::StopAudio { session_id } => {
                    let audio = self.audio.clone();
                    let events = self.events.clone();
                    tauri::async_runtime::spawn(async move {
                        let result = audio.stop(session_id).await;
                        let _ = events
                            .send(VoiceInternalEvent::AudioStopped { session_id, result })
                            .await;
                    });
                }
                Effect::CancelSession { session_id } => {
                    if let Err(error) = self.cancel_resources(session_id, "session_cancelled") {
                        if self.runtime.phase != VoicePhase::ShuttingDown {
                            let fault = reducer::fault(&mut self.runtime, error);
                            self.execute_effects(fault, presenter);
                        }
                    }
                    if self.runtime.phase != VoicePhase::Pasting {
                        presenter.hide(session_id);
                    }
                }
                Effect::Publish => presenter.emit_state(self.runtime.snapshot().payload),
            }
        }
    }

    pub(super) fn finish_start(&mut self, outcome: StartOutcome, presenter: &VoicePresenter) {
        let session_id = outcome.session_id();
        let Some(starting) = self.starting.take() else {
            self.discard_start_outcome(outcome);
            return;
        };
        if starting.session_id != session_id || self.runtime.current_id() != Some(session_id) {
            self.starting = Some(starting);
            self.discard_start_outcome(outcome);
            return;
        }
        match outcome {
            StartOutcome::Started(started) => {
                let prepared = started.prepared;
                let attempt_id = prepared.attempt_id.clone();
                let started_at = prepared.started_at;
                let cancellation = prepared.cancellation.clone();
                let deadline = prepared.deadline_cancellation.clone();
                let preview = prepared.preview_state.clone();
                let audio_queue = prepared.audio_queue.clone();
                let transcript_events = prepared.transcript_events.clone();
                let (state_tx, state_rx) = watch::channel(VoiceState::Recording);
                let resources = self.activate_prepared(prepared, state_tx, self.events.clone());
                let observer = VoiceSessionObserver::new(
                    session_id,
                    attempt_id,
                    started_at,
                    state_rx,
                    cancellation,
                );
                self.resources = Some(resources);
                crate::streaming_pipeline::spawn_transcript_event_relay(
                    presenter.presentation_sink(),
                    observer,
                    transcript_events,
                    preview,
                    session_id,
                    self.services.incidents.sink(),
                );
                crate::streaming_pipeline::spawn_audio_overflow_watcher(
                    self.events.clone(),
                    audio_queue,
                    session_id,
                );
                self.schedule_deadline(session_id, deadline);
                let effects = reducer::start_succeeded(&mut self.runtime, session_id);
                self.execute_effects(effects, presenter);
            }
            StartOutcome::Cancelled { .. } => {
                let _ = reducer::complete(&mut self.runtime, session_id);
                presenter.hide(session_id);
                presenter.emit_state(self.runtime.snapshot().payload);
            }
            StartOutcome::Failed { message, .. } => {
                if let Some(payload) = reducer::fail(&mut self.runtime, session_id, message.clone())
                {
                    presenter.show_error(session_id, message);
                    presenter.emit_state(payload);
                    self.schedule_error_reset(session_id);
                }
            }
        }
    }

    pub(super) fn activate_prepared(
        &self,
        prepared: PreparedSession,
        state_tx: watch::Sender<VoiceState>,
        events: VoiceInternalEventSink,
    ) -> SessionResources {
        let PreparedSession {
            session_id,
            attempt_id,
            provider,
            stream_info,
            chunk_rx,
            transcript_tx,
            transcript_events: _,
            preview_state,
            app_context,
            target,
            cancellation,
            deadline_cancellation,
            audio_queue,
            started_at,
            config,
            asr_hints,
        } = prepared;
        let (provider_result_tx, provider_result) = oneshot::channel();
        let provider_task = tauri::async_runtime::spawn(async move {
            let result = provider
                .transcribe_stream(stream_info, chunk_rx, transcript_tx, asr_hints)
                .await;
            let event_result = result.as_ref().map(|_| ()).map_err(Clone::clone);
            let _ = provider_result_tx.send(result);
            events
                .report_provider_finished(session_id, event_result)
                .await;
        });
        SessionResources {
            session_id,
            attempt_id,
            provider_task,
            provider_result,
            preview_state,
            app_context,
            target,
            cancellation,
            deadline_cancellation,
            audio_queue,
            started_at,
            config,
            state_tx,
        }
    }

    pub(super) fn discard_start_outcome(&self, outcome: StartOutcome) {
        if let StartOutcome::Started(started) = outcome {
            let session_id = started.prepared.session_id;
            started.prepared.cancellation.cancel();
            if let Err(error) = self.audio.request_cancel(session_id) {
                log::warn!("failed to request stale audio cancellation: {error}");
            }
        }
    }
}
