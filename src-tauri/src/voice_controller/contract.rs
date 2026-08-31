use super::resources::{PreparedSession, SessionCancellation, SessionMetrics, SessionResources};
use crate::command_error::CommandResult;
use crate::delivery::DeliveryIntent;
use crate::history::HistoryProvenance;
use crate::inject::{InjectionMethod, TextInjector};
use crate::pending_output_service::PendingOutputLease;
use crate::provider::ProviderError;
use crate::services::AppServices;
use crate::state::{VoiceState, VoiceStatePayload};
use crate::target::TargetWindowIdentity;
use crate::voice_trigger::{ActivationId, BeginDecision, VoiceActivation, VoiceCancelReason};
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, watch};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VoiceAvailability {
    Disabled,
    Available,
    ShuttingDown,
    Faulted,
}

#[derive(Clone, Debug)]
pub struct VoiceStatusSnapshot {
    pub payload: VoiceStatePayload,
    pub session_active: bool,
    #[allow(dead_code)]
    pub desired_enabled: bool,
    pub desired_revision: u64,
    pub availability: VoiceAvailability,
    #[allow(dead_code)]
    pub shortcut_error: Option<String>,
}

pub(super) enum VoiceCommand {
    Begin {
        activation: VoiceActivation,
        response: oneshot::Sender<BeginDecision>,
    },
    Finish(ActivationId),
    Cancel {
        activation_id: ActivationId,
        reason: VoiceCancelReason,
    },
    SetAvailability {
        desired_enabled: bool,
        revision: u64,
        response: oneshot::Sender<CommandResult<VoiceStatusSnapshot>>,
    },
    DeliverPending {
        id: String,
        response: oneshot::Sender<CommandResult<()>>,
    },
    QueryMetrics {
        response: oneshot::Sender<Option<SessionMetrics>>,
    },
    SetShortcutHealth(Option<String>),
    Shutdown {
        response: oneshot::Sender<()>,
    },
}

pub(super) enum VoiceInternalEvent {
    StartFinished(StartOutcome),
    AudioStopped {
        session_id: u64,
        result: Result<Duration, String>,
    },
    DeadlineReached {
        session_id: u64,
    },
    AudioOverflow {
        session_id: u64,
    },
    ProviderFinished {
        session_id: u64,
        result: Result<(), ProviderError>,
    },
    FinalizationProgress {
        session_id: u64,
        text: String,
    },
    ReadyToInject {
        session_id: u64,
        text: String,
        response: oneshot::Sender<bool>,
    },
    FinalizationFinished(FinalizeOutcome),
    PendingDeliveryFinished(PendingDeliveryOutcome),
    ResetAfterError {
        session_id: u64,
    },
}

#[allow(clippy::large_enum_variant)]
pub(super) enum ActorMessage {
    Command(VoiceCommand),
    Internal(VoiceInternalEvent),
}

#[derive(Clone)]
pub(crate) struct VoiceInternalEventSink {
    tx: mpsc::WeakSender<ActorMessage>,
}

impl VoiceInternalEventSink {
    pub(super) fn new(tx: mpsc::Sender<ActorMessage>) -> Self {
        Self { tx: tx.downgrade() }
    }

    pub(super) async fn send(&self, event: VoiceInternalEvent) -> bool {
        let Some(tx) = self.tx.upgrade() else {
            return false;
        };
        tx.send(ActorMessage::Internal(event)).await.is_ok()
    }

    pub(crate) async fn report_audio_overflow(&self, session_id: u64) {
        let _ = self
            .send(VoiceInternalEvent::AudioOverflow { session_id })
            .await;
    }

    pub(super) async fn report_provider_finished(
        &self,
        session_id: u64,
        result: Result<(), ProviderError>,
    ) {
        let _ = self
            .send(VoiceInternalEvent::ProviderFinished { session_id, result })
            .await;
    }

    pub(super) async fn report_deadline(&self, session_id: u64) {
        let _ = self
            .send(VoiceInternalEvent::DeadlineReached { session_id })
            .await;
    }
}

#[derive(Clone)]
pub(crate) struct VoiceSessionObserver {
    session_id: u64,
    attempt_id: String,
    started_at: Instant,
    state_rx: watch::Receiver<VoiceState>,
    cancellation: Arc<SessionCancellation>,
}

#[derive(Clone)]
pub(crate) struct SessionObservation {
    pub state: VoiceState,
    pub attempt_id: String,
    pub monotonic_us: u64,
}

impl VoiceSessionObserver {
    pub(super) fn new(
        session_id: u64,
        attempt_id: String,
        started_at: Instant,
        state_rx: watch::Receiver<VoiceState>,
        cancellation: Arc<SessionCancellation>,
    ) -> Self {
        Self {
            session_id,
            attempt_id,
            started_at,
            state_rx,
            cancellation,
        }
    }

    pub(crate) fn observe(&self, session_id: u64) -> Option<SessionObservation> {
        if self.session_id != session_id || self.cancellation.is_cancelled() {
            return None;
        }
        Some(SessionObservation {
            state: self.state_rx.borrow().clone(),
            attempt_id: self.attempt_id.clone(),
            monotonic_us: self.started_at.elapsed().as_micros().min(u64::MAX as u128) as u64,
        })
    }
}

pub(super) struct StartedSession {
    pub prepared: PreparedSession,
}

#[allow(clippy::large_enum_variant)]
pub(super) enum StartOutcome {
    Started(StartedSession),
    Cancelled { session_id: u64 },
    Failed { session_id: u64, message: String },
}

impl StartOutcome {
    pub(super) fn session_id(&self) -> u64 {
        match self {
            Self::Started(started) => started.prepared.session_id,
            Self::Cancelled { session_id } | Self::Failed { session_id, .. } => *session_id,
        }
    }
}

pub(super) struct FinalizationJob {
    pub session: SessionResources,
    pub injector: Arc<dyn TextInjector>,
    pub injection_method: InjectionMethod,
    pub services: AppServices,
}

pub(super) struct PendingDraft {
    pub text: String,
    pub target: TargetWindowIdentity,
    pub reason_code: String,
    pub reason_message: String,
    pub delivery_intent: DeliveryIntent,
    pub provenance: HistoryProvenance,
}

pub(super) enum FinalizeOutcome {
    Delivered {
        session_id: u64,
        history_committed: bool,
    },
    Pending {
        session_id: u64,
        draft: PendingDraft,
    },
    Cancelled {
        session_id: u64,
        reason: String,
    },
    Failed {
        session_id: u64,
        reason_code: String,
        message: String,
    },
}

impl FinalizeOutcome {
    pub(super) fn session_id(&self) -> u64 {
        match self {
            Self::Delivered { session_id, .. }
            | Self::Pending { session_id, .. }
            | Self::Cancelled { session_id, .. }
            | Self::Failed { session_id, .. } => *session_id,
        }
    }
}

pub(super) struct PendingDeliveryJob {
    pub lease: PendingOutputLease,
    pub injector: Arc<dyn TextInjector>,
    pub injection_method: InjectionMethod,
    pub services: AppServices,
    pub config: crate::config::AppConfig,
}

pub(super) enum PendingDeliveryOutcome {
    Delivered {
        lease: PendingOutputLease,
    },
    Retained {
        lease: PendingOutputLease,
        code: &'static str,
        message: String,
    },
}

impl PendingDeliveryOutcome {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Delivered { lease } | Self::Retained { lease, .. } => lease.id(),
        }
    }
}

pub(super) async fn wait_for_recording_deadline(
    cancellation: Arc<SessionCancellation>,
    duration: Duration,
) -> bool {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => false,
        _ = tokio::time::sleep(duration) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn weak_internal_sink_does_not_keep_mailbox_open() {
        let (tx, mut rx) = mpsc::channel(1);
        let sink = VoiceInternalEventSink::new(tx.clone());
        drop(tx);
        assert!(rx.recv().await.is_none());
        assert!(
            !sink
                .send(VoiceInternalEvent::DeadlineReached { session_id: 1 })
                .await
        );
    }
}
