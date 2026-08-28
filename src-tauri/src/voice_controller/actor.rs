use super::audio_actor::AudioSessionHandle;
use super::contract::{
    wait_for_recording_deadline, ActorMessage, FinalizationJob, FinalizeOutcome,
    PendingDeliveryJob, PendingDeliveryOutcome, StartOutcome, VoiceCommand, VoiceInternalEvent,
    VoiceInternalEventSink, VoiceSessionObserver, VoiceStatusSnapshot,
};
use super::incident::IncidentAttemptGuard;
use super::presenter::VoicePresenter;
use super::resources::{PreparedSession, SessionCancellation, SessionMetrics, SessionResources};
use super::workflow;
mod finalization_effects;
mod lifecycle_effects;
mod pending_effects;
mod reducer;
mod resource_effects;
mod runtime;
mod start_effects;

use crate::command_error::{CommandError, CommandResult};
use crate::config::{AppConfig, InjectionStrategy};
use crate::incident::model::{
    IncidentEvent, Recoverability, Stage as IncidentStage, StageOutcome, TerminalOutcome,
};
use crate::inject::{InjectionMethod, TextInjector, UnicodeTextInjector};
use crate::pending_output_service::{PendingOutputService, PendingOutputServiceError};
use crate::services::AppServices;
use crate::state::{ReleaseDecision, VoiceState};
use crate::voice_trigger::{BeginDecision, VoiceActivation};
use reducer::Effect;
use runtime::{VoicePhase, VoiceRuntime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::{mpsc, oneshot, watch};

const MAX_RECORDING_SECS: u64 = 120;
const ERROR_DISPLAY_MS: u64 = 1200;

struct StartingResources {
    session_id: u64,
    config: AppConfig,
    cancellation: Arc<SessionCancellation>,
    intent: crate::text_processing::ActivationIntent,
}

struct PendingOperation {
    id: String,
    response: oneshot::Sender<CommandResult<()>>,
}

pub(super) struct VoiceSessionActor {
    app: AppHandle,
    runtime: VoiceRuntime,
    services: AppServices,
    pending: Arc<PendingOutputService>,
    rx: mpsc::Receiver<ActorMessage>,
    events: VoiceInternalEventSink,
    status_tx: watch::Sender<VoiceStatusSnapshot>,
    fail_closed: Arc<AtomicBool>,
    fail_closed_notify: Arc<tokio::sync::Notify>,
    audio: AudioSessionHandle,
    injector: Arc<dyn TextInjector>,
    starting: Option<StartingResources>,
    resources: Option<SessionResources>,
    finalizing_cancellation: Option<(u64, Arc<SessionCancellation>)>,
    pending_operation: Option<PendingOperation>,
}

pub(super) fn build_actor(
    app: AppHandle,
    enabled: bool,
    revision: u64,
    services: AppServices,
    pending: Arc<PendingOutputService>,
    rx: mpsc::Receiver<ActorMessage>,
    events: VoiceInternalEventSink,
    fail_closed: Arc<AtomicBool>,
    fail_closed_notify: Arc<tokio::sync::Notify>,
) -> (VoiceSessionActor, watch::Receiver<VoiceStatusSnapshot>) {
    let runtime = VoiceRuntime::new(enabled, revision);
    let (status_tx, status_rx) = watch::channel(runtime.snapshot());
    (
        VoiceSessionActor {
            app,
            runtime,
            services,
            pending,
            rx,
            events,
            status_tx,
            fail_closed,
            fail_closed_notify,
            audio: AudioSessionHandle::spawn(),
            injector: Arc::new(UnicodeTextInjector),
            starting: None,
            resources: None,
            finalizing_cancellation: None,
            pending_operation: None,
        },
        status_rx,
    )
}

impl VoiceSessionActor {
    pub(super) async fn run(mut self) {
        let presenter = VoicePresenter::new(self.app.clone());
        loop {
            let keep_running = tokio::select! {
                biased;
                _ = self.fail_closed_notify.notified() => {
                    if self.fail_closed.swap(false, Ordering::AcqRel) {
                        let effects = reducer::fail_close(
                            &mut self.runtime,
                            "会话控制队列不可用，录音已安全取消",
                        );
                        self.execute_effects(effects, &presenter);
                    }
                    true
                }
                message = self.rx.recv() => {
                    match message {
                        Some(ActorMessage::Command(command)) => {
                            self.handle_command(command, &presenter).await
                        }
                        Some(ActorMessage::Internal(event)) => {
                            self.handle_internal(event, &presenter);
                            true
                        }
                        None => {
                            self.shutdown_resources(&presenter).await;
                            false
                        }
                    }
                }
            };
            self.status_tx.send_replace(self.runtime.snapshot());
            if !keep_running {
                break;
            }
        }
    }

    async fn handle_command(&mut self, command: VoiceCommand, presenter: &VoicePresenter) -> bool {
        match command {
            VoiceCommand::Begin {
                activation,
                response,
            } => self.begin(activation, response, presenter),
            VoiceCommand::Finish(activation_id) => {
                let effects = reducer::finish(&mut self.runtime, &activation_id);
                self.execute_effects(effects, presenter);
            }
            VoiceCommand::Cancel {
                activation_id,
                reason,
            } => {
                if self.runtime.owns_activation(&activation_id) {
                    log::info!(
                        "voice activation cancelled: activation_id={}, reason={reason:?}",
                        activation_id
                    );
                }
                let effects = reducer::cancel(&mut self.runtime, &activation_id);
                self.execute_effects(effects, presenter);
            }
            VoiceCommand::SetAvailability {
                desired_enabled,
                revision,
                response,
            } => {
                let effects =
                    reducer::set_availability(&mut self.runtime, desired_enabled, revision);
                self.execute_effects(effects, presenter);
                let _ = response.send(Ok(self.runtime.snapshot()));
            }
            VoiceCommand::DeliverPending { id, response } => {
                self.begin_pending_delivery(id, response)
            }
            VoiceCommand::QueryMetrics { response } => {
                let _ = response.send(self.metrics_snapshot());
            }
            VoiceCommand::SetShortcutHealth(error) => {
                let effects = reducer::set_shortcut_health(&mut self.runtime, error);
                self.execute_effects(effects, presenter);
            }
            VoiceCommand::Shutdown { response } => {
                self.rx.close();
                self.shutdown_resources(presenter).await;
                self.reject_queued_after_shutdown();
                let _ = response.send(());
                return false;
            }
        }
        true
    }

    fn handle_internal(&mut self, event: VoiceInternalEvent, presenter: &VoicePresenter) {
        match event {
            VoiceInternalEvent::StartFinished(outcome) => self.finish_start(outcome, presenter),
            VoiceInternalEvent::AudioStopped { session_id, result } => {
                self.finish_audio_stop(session_id, result, presenter)
            }
            VoiceInternalEvent::DeadlineReached { session_id } => {
                let effects = reducer::recording_deadline(&mut self.runtime, session_id);
                self.execute_effects(effects, presenter);
            }
            VoiceInternalEvent::AudioOverflow { session_id } => {
                self.audio_overflow(session_id, presenter)
            }
            VoiceInternalEvent::ProviderFinished { session_id, result } => match result {
                Ok(()) => log::debug!("provider finished for session_id={session_id}"),
                Err(error) => self.provider_failed(session_id, error, presenter),
            },
            VoiceInternalEvent::FinalizationProgress { session_id, text } => {
                if self.runtime.current_id() == Some(session_id) {
                    presenter.show_finalizing(session_id, text, "正在收束");
                }
            }
            VoiceInternalEvent::ReadyToInject {
                session_id,
                text,
                response,
            } => {
                let cancelled = self
                    .finalizing_cancellation
                    .as_ref()
                    .is_none_or(|(current, token)| *current != session_id || token.is_cancelled());
                let authorized = if cancelled {
                    None
                } else {
                    reducer::authorize_injection(&mut self.runtime, session_id)
                };
                if let Some(payload) = authorized {
                    presenter.show_finalizing(session_id, text, "正在写入");
                    presenter.emit_state(payload);
                    let _ = response.send(true);
                } else {
                    let _ = response.send(false);
                }
            }
            VoiceInternalEvent::FinalizationFinished(outcome) => {
                self.finish_finalization(outcome, presenter)
            }
            VoiceInternalEvent::PendingDeliveryFinished(outcome) => {
                self.finish_pending_delivery(outcome, presenter)
            }
            VoiceInternalEvent::ResetAfterError { session_id } => {
                let effects = reducer::reset_after_error(&mut self.runtime, session_id);
                if !effects.is_empty() {
                    presenter.hide(session_id);
                }
                self.execute_effects(effects, presenter);
            }
        }
    }

    fn begin(
        &mut self,
        activation: VoiceActivation,
        response: oneshot::Sender<BeginDecision>,
        presenter: &VoicePresenter,
    ) {
        if self.pending_operation.is_some() {
            let _ = response.send(BeginDecision::Rejected {
                reason: crate::voice_trigger::BeginRejection::Busy,
            });
            return;
        }
        let config = self.services.config.snapshot();
        let session_id = presenter.begin_session();
        let intent = activation.intent;
        let effects = match reducer::begin(
            &mut self.runtime,
            session_id,
            activation,
            config.revision,
            self.pending.is_full(),
        ) {
            Ok(effects) => effects,
            Err(reason) => {
                let _ = response.send(BeginDecision::Rejected { reason });
                return;
            }
        };
        let cancellation = Arc::new(SessionCancellation::default());
        self.starting = Some(StartingResources {
            session_id,
            config,
            cancellation,
            intent,
        });
        let config_revision = self
            .runtime
            .current
            .as_ref()
            .map(|current| current.config_revision)
            .unwrap_or(self.runtime.desired_revision);
        let _ = response.send(BeginDecision::Accepted {
            session_id,
            config_revision,
        });
        presenter.show_recording(session_id);
        self.execute_effects(effects, presenter);
    }
}

fn map_pending_error(error: PendingOutputServiceError) -> CommandError {
    match error {
        PendingOutputServiceError::Full => {
            CommandError::new("pending_output_full", "待处理结果已满")
        }
        PendingOutputServiceError::NotFound => {
            CommandError::new("pending_output_not_found", "待处理结果不存在或已过期")
        }
        PendingOutputServiceError::Busy => {
            CommandError::new("pending_output_busy", "待处理结果正在执行其他操作")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_trigger::VoiceActivation;

    #[test]
    fn stale_activation_does_not_own_current_session() {
        let mut runtime = VoiceRuntime::new(true, 1);
        let active = VoiceActivation::shortcut();
        runtime.begin(1, active, 1).unwrap();
        assert!(!runtime.owns_activation(&VoiceActivation::shortcut().id));
    }

    #[test]
    fn desired_state_and_availability_are_distinct_from_shortcut_health() {
        let mut runtime = VoiceRuntime::new(true, 9);
        runtime.shortcut_registration_error = Some("hook failed".to_string());
        let snapshot = runtime.snapshot();
        assert!(snapshot.desired_enabled);
        assert_eq!(snapshot.desired_revision, 9);
        assert_eq!(
            snapshot.availability,
            super::super::contract::VoiceAvailability::Available
        );
        assert_eq!(snapshot.shortcut_error.as_deref(), Some("hook failed"));
    }
}
