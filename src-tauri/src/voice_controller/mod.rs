mod actor;
mod audio_actor;
mod contract;
mod incident;
pub(crate) mod presenter;
mod resources;
mod workflow;

#[allow(unused_imports)]
pub use contract::{VoiceAvailability, VoiceStatusSnapshot};
pub(crate) use contract::{VoiceInternalEventSink, VoiceSessionObserver};
pub use resources::SessionMetrics;

use crate::pending_output_service::PendingOutputService;
use crate::services::AppServices;
use crate::voice_trigger::{
    ActivationId, BeginReceipt, VoiceActivation, VoiceCancelReason, VoiceTriggerError,
    VoiceTriggerPort,
};
use actor::build_actor;
use contract::{ActorMessage, VoiceCommand};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot, watch};

const CONTROL_QUEUE_CAPACITY: usize = 16;

#[derive(Clone)]
pub struct VoiceSessionHandle {
    tx: mpsc::Sender<ActorMessage>,
    status_rx: watch::Receiver<VoiceStatusSnapshot>,
    fail_closed: Arc<AtomicBool>,
    fail_closed_notify: Arc<tokio::sync::Notify>,
}

impl VoiceSessionHandle {
    pub fn spawn(
        app: AppHandle,
        enabled: bool,
        revision: u64,
        services: AppServices,
        pending: Arc<PendingOutputService>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(CONTROL_QUEUE_CAPACITY);
        let fail_closed = Arc::new(AtomicBool::new(false));
        let fail_closed_notify = Arc::new(tokio::sync::Notify::new());
        let (actor, status_rx) = build_actor(
            app,
            enabled,
            revision,
            services,
            pending,
            rx,
            VoiceInternalEventSink::new(tx.clone()),
            fail_closed.clone(),
            fail_closed_notify.clone(),
        );
        let handle = Self {
            tx,
            status_rx,
            fail_closed: fail_closed.clone(),
            fail_closed_notify: fail_closed_notify.clone(),
        };
        tauri::async_runtime::spawn(async move {
            actor.run().await;
        });
        handle
    }

    fn submit(&self, command: VoiceCommand) -> Result<(), VoiceTriggerError> {
        match self.tx.try_send(ActorMessage::Command(command)) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.fail_closed.store(true, Ordering::Release);
                self.fail_closed_notify.notify_one();
                Err(VoiceTriggerError::Busy)
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                Err(VoiceTriggerError::ControlPlaneUnavailable)
            }
        }
    }

    pub fn status_snapshot(&self) -> VoiceStatusSnapshot {
        self.status_rx.borrow().clone()
    }

    pub(crate) async fn set_availability(
        &self,
        enabled: bool,
        revision: u64,
    ) -> crate::command_error::CommandResult<VoiceStatusSnapshot> {
        let (response, result) = oneshot::channel();
        self.submit(VoiceCommand::SetAvailability {
            desired_enabled: enabled,
            revision,
            response,
        })
        .map_err(map_trigger_error)?;
        result.await.map_err(|_| {
            crate::command_error::CommandError::new("voice_control_unavailable", "语音控制面不可用")
        })?
    }

    pub(crate) fn set_shortcut_health(
        &self,
        error: Option<String>,
    ) -> Result<(), VoiceTriggerError> {
        self.submit(VoiceCommand::SetShortcutHealth(error))
    }

    pub async fn metrics(&self) -> Result<Option<SessionMetrics>, VoiceTriggerError> {
        let (response, result) = oneshot::channel();
        self.submit(VoiceCommand::QueryMetrics { response })?;
        result
            .await
            .map_err(|_| VoiceTriggerError::ControlPlaneUnavailable)
    }

    pub async fn deliver_pending(&self, id: String) -> crate::command_error::CommandResult<()> {
        let (response, result) = oneshot::channel();
        self.submit(VoiceCommand::DeliverPending { id, response })
            .map_err(map_trigger_error)?;
        result.await.map_err(|_| {
            crate::command_error::CommandError::new("voice_control_unavailable", "语音控制面不可用")
        })?
    }

    pub async fn shutdown(&self) {
        let (response, result) = oneshot::channel();
        if self
            .tx
            .send(ActorMessage::Command(VoiceCommand::Shutdown { response }))
            .await
            .is_ok()
        {
            let _ = result.await;
        }
    }
}

impl VoiceTriggerPort for VoiceSessionHandle {
    fn begin(&self, activation: VoiceActivation) -> Result<BeginReceipt, VoiceTriggerError> {
        let (response, result) = oneshot::channel();
        self.submit(VoiceCommand::Begin {
            activation,
            response,
        })?;
        Ok(BeginReceipt::new(result))
    }

    fn finish(&self, activation_id: ActivationId) -> Result<(), VoiceTriggerError> {
        self.submit(VoiceCommand::Finish(activation_id))
    }

    fn cancel(
        &self,
        activation_id: ActivationId,
        reason: VoiceCancelReason,
    ) -> Result<(), VoiceTriggerError> {
        self.submit(VoiceCommand::Cancel {
            activation_id,
            reason,
        })
    }
}

fn map_trigger_error(error: VoiceTriggerError) -> crate::command_error::CommandError {
    match error {
        VoiceTriggerError::Busy => crate::command_error::CommandError::new(
            "voice_control_busy",
            "语音控制面繁忙，当前操作已安全拒绝",
        ),
        VoiceTriggerError::ControlPlaneUnavailable => {
            crate::command_error::CommandError::new("voice_control_unavailable", "语音控制面不可用")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{VoiceState, VoiceStatePayload};

    fn snapshot() -> VoiceStatusSnapshot {
        VoiceStatusSnapshot {
            payload: VoiceStatePayload {
                state: VoiceState::Idle,
                message: "ready".to_string(),
                elapsed_ms: None,
            },
            session_active: false,
            desired_enabled: true,
            desired_revision: 1,
            availability: VoiceAvailability::Available,
            shortcut_error: None,
        }
    }

    fn handle(tx: mpsc::Sender<ActorMessage>) -> VoiceSessionHandle {
        let (_, status_rx) = watch::channel(snapshot());
        VoiceSessionHandle {
            tx,
            status_rx,
            fail_closed: Arc::new(AtomicBool::new(false)),
            fail_closed_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    #[test]
    fn full_public_mailbox_fails_closed() {
        let (tx, _rx) = mpsc::channel(1);
        let voice = handle(tx.clone());
        tx.try_send(ActorMessage::Command(VoiceCommand::SetShortcutHealth(None)))
            .unwrap();

        assert_eq!(
            voice.finish(ActivationId::new()).unwrap_err(),
            VoiceTriggerError::Busy
        );
        assert!(voice.fail_closed.load(Ordering::Acquire));
    }

    #[test]
    fn closed_public_mailbox_is_reported() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let voice = handle(tx);
        assert_eq!(
            voice.finish(ActivationId::new()).unwrap_err(),
            VoiceTriggerError::ControlPlaneUnavailable
        );
    }

    #[tokio::test]
    async fn dropping_last_handle_closes_mailbox() {
        let (tx, mut rx) = mpsc::channel(1);
        let voice = handle(tx);
        drop(voice);
        assert!(rx.recv().await.is_none());
    }
}
