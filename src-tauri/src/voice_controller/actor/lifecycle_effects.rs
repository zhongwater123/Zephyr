use super::*;

impl VoiceSessionActor {
    pub(super) fn reject_queued_after_shutdown(&mut self) {
        while let Ok(message) = self.rx.try_recv() {
            let ActorMessage::Command(command) = message else {
                continue;
            };
            match command {
                VoiceCommand::Begin { response, .. } => {
                    let _ = response.send(BeginDecision::Rejected {
                        reason: crate::voice_trigger::BeginRejection::ShuttingDown,
                    });
                }
                VoiceCommand::SetAvailability { response, .. } => {
                    let _ = response.send(Err(CommandError::new(
                        "voice_control_unavailable",
                        "语音控制面正在关闭",
                    )));
                }
                VoiceCommand::DeliverPending { response, .. } => {
                    let _ = response.send(Err(CommandError::new(
                        "voice_control_unavailable",
                        "语音控制面正在关闭",
                    )));
                }
                VoiceCommand::QueryMetrics { response } => {
                    let _ = response.send(self.metrics_snapshot());
                }
                VoiceCommand::Shutdown { response } => {
                    let _ = response.send(());
                }
                VoiceCommand::Finish(_)
                | VoiceCommand::Cancel { .. }
                | VoiceCommand::SetShortcutHealth(_) => {}
            }
        }
    }

    pub(super) fn schedule_deadline(
        &self,
        session_id: u64,
        cancellation: Arc<SessionCancellation>,
    ) {
        let events = self.events.clone();
        tauri::async_runtime::spawn(async move {
            if wait_for_recording_deadline(cancellation, Duration::from_secs(MAX_RECORDING_SECS))
                .await
            {
                events.report_deadline(session_id).await;
            }
        });
    }

    pub(super) fn schedule_error_reset(&self, session_id: u64) {
        let events = self.events.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ERROR_DISPLAY_MS)).await;
            let _ = events
                .send(VoiceInternalEvent::ResetAfterError { session_id })
                .await;
        });
    }

    pub(super) async fn shutdown_resources(&mut self, presenter: &VoicePresenter) {
        let session_id = self.runtime.current_id();
        let effects = reducer::shutdown(&mut self.runtime);
        self.execute_effects(effects, presenter);
        if let Some(session_id) = session_id {
            presenter.hide(session_id);
        }
        if let Some(operation) = self.pending_operation.take() {
            let _ = operation.response.send(Err(CommandError::new(
                "voice_control_unavailable",
                "语音控制面正在关闭",
            )));
        }
        self.audio.shutdown().await;
    }
}
