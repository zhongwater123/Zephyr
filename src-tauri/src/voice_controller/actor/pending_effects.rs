use super::*;

impl VoiceSessionActor {
    pub(super) fn begin_pending_delivery(
        &mut self,
        id: String,
        response: oneshot::Sender<CommandResult<()>>,
    ) {
        if self.runtime.current.is_some() || self.pending_operation.is_some() {
            let _ = response.send(Err(CommandError::new(
                "session_active",
                "录音、识别或待处理交付进行中，暂时不能执行该操作",
            )));
            return;
        }
        let lease = match self.pending.reserve_lease(&id) {
            Ok(lease) => lease,
            Err(error) => {
                let _ = response.send(Err(map_pending_error(error)));
                return;
            }
        };
        let config = self.services.config.snapshot();
        let method = match config.injection_strategy_for(&lease.record().target.executable_name) {
            InjectionStrategy::Unicode => InjectionMethod::Unicode,
            InjectionStrategy::ClipboardCompatibility => InjectionMethod::ClipboardCompatibility,
        };
        self.pending_operation = Some(PendingOperation { id, response });
        workflow::spawn_pending_delivery(
            PendingDeliveryJob {
                lease,
                injector: self.injector.clone(),
                injection_method: method,
                services: self.services.clone(),
                config,
            },
            self.events.clone(),
        );
    }

    pub(super) fn finish_pending_delivery(
        &mut self,
        outcome: PendingDeliveryOutcome,
        presenter: &VoicePresenter,
    ) {
        let id = outcome.id().to_string();
        let Some(operation) = self.pending_operation.take() else {
            return;
        };
        if operation.id != id {
            self.pending_operation = Some(operation);
            return;
        }
        match outcome {
            PendingDeliveryOutcome::Delivered { lease } => {
                let result = lease.complete().map(|_| ()).map_err(map_pending_error);
                if result.is_ok() {
                    presenter.pending_changed();
                }
                let _ = operation.response.send(result);
            }
            PendingDeliveryOutcome::Retained {
                lease,
                code,
                message,
            } => {
                drop(lease);
                let _ = operation
                    .response
                    .send(Err(CommandError::new(code, message)));
            }
        }
    }

    pub(super) fn metrics_snapshot(&self) -> Option<SessionMetrics> {
        if let Some(session) = &self.resources {
            let queue = session.audio_queue.snapshot();
            return Some(SessionMetrics {
                session_id: session.session_id,
                audio_packets: queue.packets,
                queue_high_watermark: queue.high_watermark,
                overflow: queue.overflow,
                recording_duration_ms: session.started_at.elapsed().as_millis() as u64,
                cancel_reason: None,
                final_state: format!("{:?}", self.runtime.phase).to_lowercase(),
            });
        }
        self.runtime.last_metrics.clone()
    }
}
