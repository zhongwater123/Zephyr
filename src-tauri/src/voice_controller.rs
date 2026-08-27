use crate::config::InjectionStrategy;
use crate::delivery::DeliveryService;
use crate::incident::model::TerminalOutcome;
use crate::incident::model::{
    AttemptPolicy as IncidentAttemptPolicy, IncidentEvent, Recoverability,
};
use crate::incident::model::{Stage as IncidentStage, StageOutcome as IncidentStageOutcome};
use crate::inject::InjectionMethod;
use crate::inject::{TextInjector, UnicodeTextInjector};
use crate::overlay::{self, PreInputPayload, PreInputState};
use crate::pending_output_service::{PendingOutputService, PendingOutputServiceError};
use crate::preview::TranscriptPreviewState;
use crate::provider::ProviderError;
use crate::services::AppServices;
use crate::state::{ReleaseDecision, VoiceState, VoiceStatePayload};
use crate::target;
use crate::voice_trigger::{
    ActivationId, VoiceActivation, VoiceCancelReason, VoiceTriggerError, VoiceTriggerPort,
};
use crate::{history, hotwords, ActiveSession, SessionCancellation, SessionMetrics};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant as StdInstant;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{sleep, timeout, Duration};

const VOICE_STATE_EVENT: &str = "voice_state_changed";
const STREAM_CHUNK_MS: u16 = 200;
const FINAL_TRANSCRIPT_TIMEOUT_SECS: u64 = 25;
const EMPTY_TRANSCRIPT_TIMEOUT_MS: u64 = 800;
struct IncidentAttemptGuard {
    sink: Arc<dyn crate::incident::IncidentSink>,
    attempt_id: String,
    finished: bool,
    finding_recorded: bool,
}

impl IncidentAttemptGuard {
    fn new(sink: Arc<dyn crate::incident::IncidentSink>, attempt_id: String) -> Self {
        Self {
            sink,
            attempt_id,
            finished: false,
            finding_recorded: false,
        }
    }

    fn stage(
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

    fn finding(
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

    fn record_failure(
        &mut self,
        stage: IncidentStage,
        code: &str,
        message: &str,
        recoverability: Recoverability,
    ) {
        self.stage(stage, IncidentStageOutcome::Failed, Some(code.to_string()));
        self.finding(stage, code, message, recoverability);
    }

    fn cancel(&mut self, stage: IncidentStage, code: &str) {
        self.stage(
            stage,
            IncidentStageOutcome::Cancelled,
            Some(code.to_string()),
        );
        self.finish(TerminalOutcome::Cancelled, false);
    }

    fn final_transcript(&self, text: &str, monotonic_us: u64) {
        let _ = self.sink.try_emit(IncidentEvent::FinalTranscript {
            attempt_id: self.attempt_id.clone(),
            text: text.to_string(),
            monotonic_us,
        });
    }

    fn finish(&mut self, outcome: TerminalOutcome, history_committed: bool) {
        let discard_recovery_material = outcome == TerminalOutcome::Succeeded && history_committed;
        self.finish_with_recovery_policy(outcome, history_committed, discard_recovery_material);
    }

    fn finish_delivered(&mut self, history_committed: bool, discard_recovery_material: bool) {
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
const AUDIO_QUEUE_CAPACITY: usize = 32;
const MAX_RECORDING_SECS: u64 = 120;
const CONTROL_QUEUE_CAPACITY: usize = 16;

type SharedRuntime = Arc<Mutex<VoiceRuntime>>;

struct VoiceRuntime {
    machine: crate::state::AppStateMachine,
    recorder: crate::audio::Recorder,
    injector: Arc<dyn TextInjector>,
    sessions: crate::session::SessionCoordinator,
    active_activation: Option<VoiceActivation>,
    shortcut_registration_error: Option<String>,
}

impl VoiceRuntime {
    fn new(enabled: bool) -> Self {
        let mut machine = crate::state::AppStateMachine::new();
        machine.set_enabled(enabled);
        Self {
            machine,
            recorder: crate::audio::Recorder::new(),
            injector: Arc::new(UnicodeTextInjector),
            sessions: crate::session::SessionCoordinator::default(),
            active_activation: None,
            shortcut_registration_error: None,
        }
    }

    fn voice_state_payload(&self) -> VoiceStatePayload {
        let state = self.machine.state().clone();
        let default_message = match state {
            VoiceState::Idle => "准备就绪",
            VoiceState::Recording => "正在听",
            VoiceState::Transcribing => "识别中",
            VoiceState::Pasting => "正在输入",
            VoiceState::Disabled => "已暂停",
            VoiceState::Error => "语音输入发生错误",
        };
        VoiceStatePayload {
            state: state.clone(),
            message: if state == VoiceState::Idle {
                self.shortcut_registration_error
                    .clone()
                    .unwrap_or_else(|| default_message.to_string())
            } else {
                default_message.to_string()
            },
            elapsed_ms: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VoiceStatusSnapshot {
    pub payload: VoiceStatePayload,
    pub session_active: bool,
    pub desired_enabled: bool,
    pub shortcut_error: Option<String>,
}

#[derive(Clone)]
pub(crate) struct VoiceSessionObserver {
    runtime: SharedRuntime,
}

#[derive(Clone)]
pub(crate) struct SessionObservation {
    pub state: VoiceState,
    pub attempt_id: String,
    pub monotonic_us: u64,
}

impl VoiceSessionObserver {
    pub fn observe(&self, session_id: u64) -> Option<SessionObservation> {
        let runtime = self.runtime.lock().ok()?;
        if runtime.sessions.current_id != Some(session_id) {
            return None;
        }
        let active = runtime.sessions.active.as_ref()?;
        Some(SessionObservation {
            state: runtime.machine.state().clone(),
            attempt_id: active.attempt_id.clone(),
            monotonic_us: active
                .started_at
                .elapsed()
                .as_micros()
                .min(u64::MAX as u128) as u64,
        })
    }
}

impl VoiceStatusSnapshot {
    fn from_runtime(runtime: &VoiceRuntime) -> Self {
        Self {
            payload: runtime.voice_state_payload(),
            session_active: runtime.sessions.current_id.is_some(),
            desired_enabled: runtime.machine.is_enabled(),
            shortcut_error: runtime.shortcut_registration_error.clone(),
        }
    }
}

pub enum VoiceCommand {
    Begin(VoiceActivation),
    Finish(ActivationId),
    Cancel {
        activation_id: ActivationId,
        reason: VoiceCancelReason,
    },
    SetAvailability(bool),
    DeliverPending {
        id: String,
        response: oneshot::Sender<crate::command_error::CommandResult<()>>,
    },
    QueryMetrics {
        response: oneshot::Sender<Option<SessionMetrics>>,
    },
    SetShortcutHealth(Option<String>),
}

enum VoiceInternalEvent {
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
}

enum ControlMessage {
    Command(VoiceCommand),
    Internal(VoiceInternalEvent),
}

#[derive(Clone)]
pub struct VoiceSessionHandle {
    tx: mpsc::Sender<ControlMessage>,
    status_tx: watch::Sender<VoiceStatusSnapshot>,
    status_rx: watch::Receiver<VoiceStatusSnapshot>,
    fail_closed: Arc<AtomicBool>,
    fail_closed_notify: Arc<tokio::sync::Notify>,
}

struct VoiceSessionActor {
    app: AppHandle,
    runtime: SharedRuntime,
    services: AppServices,
    pending: Arc<PendingOutputService>,
    rx: mpsc::Receiver<ControlMessage>,
    handle: VoiceSessionHandle,
}

impl VoiceSessionHandle {
    pub fn spawn(
        app: AppHandle,
        enabled: bool,
        services: AppServices,
        pending: Arc<PendingOutputService>,
    ) -> Self {
        let runtime = Arc::new(Mutex::new(VoiceRuntime::new(enabled)));
        let (tx, rx) = mpsc::channel::<ControlMessage>(CONTROL_QUEUE_CAPACITY);
        let initial_status = VoiceStatusSnapshot::from_runtime(
            &runtime.lock().expect("voice runtime lock poisoned"),
        );
        let (status_tx, status_rx) = watch::channel(initial_status);
        let fail_closed = Arc::new(AtomicBool::new(false));
        let fail_closed_notify = Arc::new(tokio::sync::Notify::new());
        let handle = Self {
            tx,
            status_tx,
            status_rx,
            fail_closed,
            fail_closed_notify,
        };
        let actor = VoiceSessionActor {
            app,
            runtime,
            services,
            pending,
            rx,
            handle: handle.clone(),
        };
        tauri::async_runtime::spawn(async move {
            actor.run().await;
        });
        handle
    }

    fn submit(&self, message: ControlMessage) -> Result<(), VoiceTriggerError> {
        match self.tx.try_send(message) {
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

    pub(crate) fn set_availability(&self, enabled: bool) -> Result<(), VoiceTriggerError> {
        self.submit(ControlMessage::Command(VoiceCommand::SetAvailability(
            enabled,
        )))
    }

    pub(crate) fn set_shortcut_health(
        &self,
        error: Option<String>,
    ) -> Result<(), VoiceTriggerError> {
        self.submit(ControlMessage::Command(VoiceCommand::SetShortcutHealth(
            error,
        )))
    }

    pub async fn metrics(&self) -> Result<Option<SessionMetrics>, VoiceTriggerError> {
        let (response, result) = oneshot::channel();
        self.submit(ControlMessage::Command(VoiceCommand::QueryMetrics {
            response,
        }))?;
        result
            .await
            .map_err(|_| VoiceTriggerError::ControlPlaneUnavailable)
    }

    pub async fn deliver_pending(&self, id: String) -> crate::command_error::CommandResult<()> {
        let (response, result) = oneshot::channel();
        self.submit(ControlMessage::Command(VoiceCommand::DeliverPending {
            id,
            response,
        }))
        .map_err(map_trigger_error)?;
        result.await.map_err(|_| {
            crate::command_error::CommandError::new("voice_control_unavailable", "语音控制面不可用")
        })?
    }

    pub(crate) fn report_audio_overflow(&self, session_id: u64) {
        let _ = self.submit(ControlMessage::Internal(
            VoiceInternalEvent::AudioOverflow { session_id },
        ));
    }

    fn report_provider_finished(&self, session_id: u64, result: Result<(), ProviderError>) {
        let _ = self.submit(ControlMessage::Internal(
            VoiceInternalEvent::ProviderFinished { session_id, result },
        ));
    }

    fn report_deadline(&self, session_id: u64) {
        let _ = self.submit(ControlMessage::Internal(
            VoiceInternalEvent::DeadlineReached { session_id },
        ));
    }

    fn publish(&self, runtime: &VoiceRuntime) {
        self.status_tx
            .send_replace(VoiceStatusSnapshot::from_runtime(runtime));
    }

    fn publish_payload(&self, payload: VoiceStatePayload) {
        self.status_tx.send_if_modified(|snapshot| {
            if snapshot.payload == payload {
                return false;
            }
            snapshot.session_active = matches!(
                payload.state,
                VoiceState::Recording | VoiceState::Transcribing | VoiceState::Pasting
            );
            snapshot.payload = payload;
            true
        });
    }
}

impl VoiceTriggerPort for VoiceSessionHandle {
    fn begin(&self, activation: VoiceActivation) -> Result<(), VoiceTriggerError> {
        self.submit(ControlMessage::Command(VoiceCommand::Begin(activation)))
    }

    fn finish(&self, activation_id: ActivationId) -> Result<(), VoiceTriggerError> {
        self.submit(ControlMessage::Command(VoiceCommand::Finish(activation_id)))
    }

    fn cancel(
        &self,
        activation_id: ActivationId,
        reason: VoiceCancelReason,
    ) -> Result<(), VoiceTriggerError> {
        self.submit(ControlMessage::Command(VoiceCommand::Cancel {
            activation_id,
            reason,
        }))
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

impl VoiceSessionActor {
    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                _ = self.handle.fail_closed_notify.notified() => {
                    if self.handle.fail_closed.swap(false, Ordering::AcqRel) {
                        let _ = cancel_current_session(
                            &self.app,
                            self.runtime.clone(),
                            self.services.incidents.sink(),
                        );
                        let payload = {
                            let mut runtime = self.runtime.lock().expect("voice runtime lock poisoned");
                            runtime.machine.fail("会话控制队列不可用，录音已安全取消")
                        };
                        emit_state(&self.app, payload);
                        self.publish_status();
                    }
                }
                message = self.rx.recv() => {
                    let Some(message) = message else {
                        let _ = cancel_current_session(
                            &self.app,
                            self.runtime.clone(),
                            self.services.incidents.sink(),
                        );
                        break;
                    };
                    match message {
                        ControlMessage::Command(command) => self.handle_command(command).await,
                        ControlMessage::Internal(event) => self.handle_internal(event),
                    }
                    self.publish_status();
                }
            }
        }
    }

    async fn handle_command(&self, command: VoiceCommand) {
        match command {
            VoiceCommand::Begin(activation) => {
                let can_begin = {
                    let runtime = self.runtime.lock().expect("voice runtime lock poisoned");
                    runtime.sessions.current_id.is_none() && runtime.machine.is_enabled()
                };
                if !can_begin {
                    return;
                }
                handle_pressed(
                    &self.app,
                    self.runtime.clone(),
                    self.services.clone(),
                    self.pending.clone(),
                    self.handle.clone(),
                );
                let mut runtime = self.runtime.lock().expect("voice runtime lock poisoned");
                if runtime.sessions.current_id.is_some() {
                    runtime.active_activation = Some(activation);
                }
            }
            VoiceCommand::Finish(activation_id) => {
                if self.owns_activation(&activation_id) {
                    handle_released(
                        &self.app,
                        self.runtime.clone(),
                        self.services.clone(),
                        self.pending.clone(),
                    );
                }
            }
            VoiceCommand::Cancel {
                activation_id,
                reason,
            } => {
                if self.owns_activation(&activation_id) {
                    log::info!(
                        "voice activation cancelled: activation_id={}, reason={reason:?}",
                        activation_id
                    );
                    let _ = cancel_current_session(
                        &self.app,
                        self.runtime.clone(),
                        self.services.incidents.sink(),
                    );
                }
            }
            VoiceCommand::SetAvailability(enabled) => {
                if !enabled {
                    let _ = cancel_current_session(
                        &self.app,
                        self.runtime.clone(),
                        self.services.incidents.sink(),
                    );
                }
                let payload = {
                    let mut runtime = self.runtime.lock().expect("voice runtime lock poisoned");
                    runtime.machine.set_enabled(enabled)
                };
                emit_state(&self.app, payload);
            }
            VoiceCommand::DeliverPending { id, response } => {
                let result = deliver_pending(
                    &self.app,
                    id,
                    self.runtime.clone(),
                    self.pending.clone(),
                    self.services.clone(),
                )
                .await;
                let _ = response.send(result);
            }
            VoiceCommand::QueryMetrics { response } => {
                let _ = response.send(self.metrics_snapshot());
            }
            VoiceCommand::SetShortcutHealth(error) => {
                let payload = {
                    let mut runtime = self.runtime.lock().expect("voice runtime lock poisoned");
                    runtime.shortcut_registration_error = error;
                    runtime.voice_state_payload()
                };
                emit_state(&self.app, payload);
            }
        }
    }

    fn handle_internal(&self, event: VoiceInternalEvent) {
        match event {
            VoiceInternalEvent::DeadlineReached { session_id } => {
                let current = self
                    .runtime
                    .lock()
                    .map(|runtime| runtime.sessions.current_id)
                    .unwrap_or(None);
                if current == Some(session_id) {
                    log::info!(
                        "voice session reached recording deadline: session_id={}, duration_ms={}",
                        session_id,
                        MAX_RECORDING_SECS * 1000
                    );
                    handle_released(
                        &self.app,
                        self.runtime.clone(),
                        self.services.clone(),
                        self.pending.clone(),
                    );
                }
            }
            VoiceInternalEvent::AudioOverflow { session_id } => handle_audio_overflow(
                &self.app,
                self.runtime.clone(),
                self.services.clone(),
                session_id,
            ),
            VoiceInternalEvent::ProviderFinished { session_id, result } => match result {
                Ok(()) => log::debug!("provider finished for session_id={session_id}"),
                Err(error) => handle_provider_failure(
                    &self.app,
                    self.runtime.clone(),
                    session_id,
                    error,
                    self.services.clone(),
                ),
            },
        }
    }

    fn owns_activation(&self, activation_id: &ActivationId) -> bool {
        self.runtime
            .lock()
            .map(|runtime| activation_matches(&runtime.active_activation, activation_id))
            .unwrap_or(false)
    }

    fn metrics_snapshot(&self) -> Option<SessionMetrics> {
        let runtime = self.runtime.lock().expect("voice runtime lock poisoned");
        if let Some(session) = &runtime.sessions.active {
            let queue = session.audio_queue.snapshot();
            return Some(SessionMetrics {
                session_id: session.session_id,
                audio_packets: queue.packets,
                queue_high_watermark: queue.high_watermark,
                overflow: queue.overflow,
                recording_duration_ms: session.started_at.elapsed().as_millis() as u64,
                cancel_reason: None,
                final_state: "recording".to_string(),
            });
        }
        runtime.sessions.last_metrics.clone()
    }

    fn publish_status(&self) {
        if let Ok(runtime) = self.runtime.lock() {
            self.handle.publish(&runtime);
        }
    }
}

fn activation_matches(active: &Option<VoiceActivation>, activation_id: &ActivationId) -> bool {
    active.as_ref().map(|activation| &activation.id) == Some(activation_id)
}

fn handle_pressed(
    app: &AppHandle,
    runtime: SharedRuntime,
    services: AppServices,
    pending: Arc<PendingOutputService>,
    controller: VoiceSessionHandle,
) {
    let (payload, event_rx, preview_state, audio_queue, session_id, deadline_cancellation) = {
        let (chunk_tx, chunk_rx) = mpsc::channel(AUDIO_QUEUE_CAPACITY);
        let (event_tx, event_rx) = watch::channel(None);
        let audio_queue = Arc::new(crate::audio::AudioQueueMonitor::default());
        let mut voice = runtime.lock().expect("voice runtime lock poisoned");
        let preview_state = Arc::new(tokio::sync::Mutex::new(TranscriptPreviewState::default()));

        if voice.sessions.current_id.is_some() {
            return;
        }

        if pending.is_full() {
            let payload = voice
                .machine
                .fail("待处理结果已达到 5 条，请先发送、复制或丢弃后再开始录音".to_string());
            emit_state(app, payload);
            return;
        }

        let Some(payload) = voice.machine.activation_started() else {
            return;
        };
        let session_id = overlay::begin_preinput_session();
        let cancellation = Arc::new(SessionCancellation::default());
        let deadline_cancellation = Arc::new(SessionCancellation::default());
        voice.sessions.current_id = Some(session_id);
        voice.sessions.current_cancellation = Some(cancellation.clone());

        let target = match target::capture_foreground_target() {
            Ok(target) => target,
            Err(error) => {
                let payload = voice.machine.fail(error);
                emit_state(app, payload);
                schedule_idle_reset(app.clone(), runtime.clone(), session_id);
                return;
            }
        };
        let app_context = history::AppContext {
            app_name: Some(target.executable_name.clone()),
            app_title: target.window_title.clone(),
        };
        let config = services.config.snapshot();
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let content_enabled =
            config.incident_recovery_enabled && config.incident_consent_version > 0;
        let incident_policy = IncidentAttemptPolicy {
            content_enabled,
            save_audio: config.incident_save_failed_audio,
            save_text: config.incident_save_failed_text,
            retention_days: config.incident_retention_days,
            storage_limit_mb: config.incident_storage_limit_mb,
            success_rollup_days: config.incident_success_rollup_days,
        };
        let incident_sink = services.incidents.sink();
        let _ = incident_sink.try_emit(IncidentEvent::AttemptStarted {
            attempt_id: attempt_id.clone(),
            runtime_session_id: session_id,
            started_at_utc_ms: chrono::Utc::now().timestamp_millis(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            app_name: app_context.app_name.clone(),
            app_title: app_context.app_title.clone(),
            policy: incident_policy,
        });
        for stage in [IncidentStage::Capture, IncidentStage::Asr] {
            let _ = incident_sink.try_emit(IncidentEvent::StageChanged {
                attempt_id: attempt_id.clone(),
                stage,
                outcome: IncidentStageOutcome::Running,
                reason_code: None,
                monotonic_us: 0,
            });
        }
        let incident_audio_tap = content_enabled.then(|| {
            crate::audio::IncidentAudioTap::new(incident_sink.clone(), attempt_id.clone())
        });
        let asr_hints = match hotwords::compose_asr_hints(&config, &app_context) {
            Ok(hints) => hints,
            Err(error) => {
                log::warn!("failed to compose ASR hotword hints: {error}");
                None
            }
        };
        let stream_info = match voice.recorder.start_streaming(
            STREAM_CHUNK_MS,
            chunk_tx,
            audio_queue.clone(),
            incident_audio_tap,
        ) {
            Ok(stream_info) => stream_info,
            Err(error) => {
                let error_message = error.to_string();
                let mut incident =
                    IncidentAttemptGuard::new(incident_sink.clone(), attempt_id.clone());
                incident.record_failure(
                    IncidentStage::Capture,
                    "capture_start_failed",
                    &error_message,
                    Recoverability::None,
                );
                incident.finish(TerminalOutcome::Failed, false);
                let payload = voice.machine.fail(error_message);
                emit_state(app, payload);
                schedule_idle_reset(app.clone(), runtime.clone(), session_id);
                return;
            }
        };

        let provider = services.provider.build(&config);
        let (provider_result_tx, provider_result) = oneshot::channel();
        let provider_controller = controller.clone();
        let provider_task = tauri::async_runtime::spawn(async move {
            let result = provider
                .transcribe_stream(stream_info, chunk_rx, event_tx, asr_hints)
                .await;
            let event_result = result.as_ref().map(|_| ()).map_err(|error| error.clone());
            let _ = provider_result_tx.send(result);
            provider_controller.report_provider_finished(session_id, event_result);
        });
        voice.sessions.active = Some(ActiveSession {
            session_id,
            attempt_id,
            provider_task,
            provider_result,
            preview_state: preview_state.clone(),
            app_context,
            target,
            cancellation,
            deadline_cancellation: deadline_cancellation.clone(),
            audio_queue: audio_queue.clone(),
            started_at: StdInstant::now(),
            config,
        });
        (
            payload,
            event_rx,
            preview_state,
            audio_queue,
            session_id,
            deadline_cancellation,
        )
    };

    overlay::show_preinput(
        app,
        PreInputPayload {
            session_id,
            seq: 0,
            text: String::new(),
            state: PreInputState::Recording,
            confirmed_chars: Some(0),
            message: Some("正在聆听".to_string()),
        },
    );
    crate::streaming_pipeline::spawn_transcript_event_relay(
        app.clone(),
        VoiceSessionObserver {
            runtime: runtime.clone(),
        },
        event_rx,
        preview_state,
        session_id,
        services.incidents.sink(),
    );
    crate::streaming_pipeline::spawn_audio_overflow_watcher(
        app.clone(),
        controller.clone(),
        audio_queue,
        session_id,
    );
    schedule_recording_deadline(app.clone(), controller, session_id, deadline_cancellation);
    emit_state(app, payload);
}

fn handle_released(
    app: &AppHandle,
    runtime: SharedRuntime,
    services: AppServices,
    pending: Arc<PendingOutputService>,
) {
    let (
        provider_task,
        provider_result,
        preview_state,
        injector,
        history_enabled,
        session_config,
        app_context,
        target,
        injection_method,
        session_id,
        cancellation,
        attempt_id,
        attempt_started_at,
    ) = {
        let mut voice = runtime.lock().expect("voice runtime lock poisoned");
        if voice.machine.state() != &VoiceState::Recording {
            return;
        }

        let duration = match voice.recorder.stop_streaming() {
            Ok(duration) => duration,
            Err(error) => {
                let error_message = error.to_string();
                let payload = voice.machine.fail(error_message.clone());
                emit_state(app, payload);
                let session_id = voice
                    .sessions
                    .current_id
                    .unwrap_or_else(overlay::current_preinput_session_id);
                if let Some(session) = voice.sessions.active.take() {
                    let mut incident = IncidentAttemptGuard::new(
                        services.incidents.sink(),
                        session.attempt_id.clone(),
                    );
                    incident.record_failure(
                        IncidentStage::Capture,
                        "capture_stop_failed",
                        &error_message,
                        Recoverability::Audio,
                    );
                    incident.finish(TerminalOutcome::Failed, false);
                    session.cancellation.cancel();
                    session.deadline_cancellation.cancel();
                    session.provider_task.abort();
                }
                schedule_idle_reset(app.clone(), runtime.clone(), session_id);
                return;
            }
        };

        let decision = voice.machine.activation_finished(duration);
        let Some(session) = voice.sessions.active.take() else {
            let payload = voice.machine.fail("识别会话不存在");
            emit_state(app, payload);
            schedule_idle_reset(
                app.clone(),
                runtime.clone(),
                overlay::current_preinput_session_id(),
            );
            return;
        };
        session.deadline_cancellation.cancel();
        let queue_snapshot = session.audio_queue.snapshot();
        metrics::histogram!("voice.recording.duration_ms").record(duration.as_millis() as f64);
        metrics::gauge!("voice.audio_queue.high_watermark")
            .set(queue_snapshot.high_watermark as f64);
        let incident_sink = services.incidents.sink();
        let _ = incident_sink.try_emit(IncidentEvent::Metric {
            attempt_id: session.attempt_id.clone(),
            name: "recording_duration_ms",
            value: duration.as_millis() as f64,
            unit: "milliseconds",
        });
        let _ = incident_sink.try_emit(IncidentEvent::Metric {
            attempt_id: session.attempt_id.clone(),
            name: "audio_queue_high_watermark",
            value: queue_snapshot.high_watermark as f64,
            unit: "chunks",
        });
        voice.sessions.last_metrics = Some(crate::SessionMetrics {
            session_id: session.session_id,
            audio_packets: queue_snapshot.packets,
            queue_high_watermark: queue_snapshot.high_watermark,
            overflow: queue_snapshot.overflow,
            recording_duration_ms: duration.as_millis() as u64,
            cancel_reason: None,
            final_state: "transcribing".to_string(),
        });

        match decision {
            ReleaseDecision::Cancelled { payload, .. } => {
                if let Some(metrics) = &mut voice.sessions.last_metrics {
                    metrics.cancel_reason = Some("recording_too_short".to_string());
                    metrics.final_state = "cancelled".to_string();
                }
                let mut incident = IncidentAttemptGuard::new(
                    services.incidents.sink(),
                    session.attempt_id.clone(),
                );
                incident.stage(
                    IncidentStage::Capture,
                    IncidentStageOutcome::Cancelled,
                    Some("recording_too_short".to_string()),
                );
                incident.finish(TerminalOutcome::Cancelled, false);
                session.cancellation.cancel();
                session.provider_task.abort();
                clear_current_session(&mut voice, session.session_id);
                emit_state(app, payload);
                overlay::hide_preinput_for_session(app, session.session_id);
                return;
            }
            ReleaseDecision::Transcribe { payload, .. } => {
                emit_state(app, payload);
            }
        }

        (
            session.provider_task,
            session.provider_result,
            session.preview_state,
            voice.injector.clone(),
            session.config.history_enabled,
            session.config.clone(),
            session.app_context,
            session.target.clone(),
            match session
                .config
                .injection_strategy_for(&session.target.executable_name)
            {
                InjectionStrategy::Unicode => InjectionMethod::Unicode,
                InjectionStrategy::ClipboardCompatibility => {
                    InjectionMethod::ClipboardCompatibility
                }
            },
            session.session_id,
            session.cancellation,
            session.attempt_id,
            session.started_at,
        )
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let incident_sink = services.incidents.sink();
        let mut incident = IncidentAttemptGuard::new(incident_sink, attempt_id);
        let mut provider_result = provider_result;
        let has_preview_text = !preview_state.lock().await.rendered_text().trim().is_empty();

        if has_preview_text {
            overlay::update_preinput(
                &app,
                PreInputPayload {
                    session_id,
                    seq: 0,
                    text: preview_state.lock().await.rendered_text(),
                    state: PreInputState::Finalizing,
                    confirmed_chars: None,
                    message: Some("正在收束".to_string()),
                },
            );
        }

        let wait_duration = if has_preview_text {
            Duration::from_secs(FINAL_TRANSCRIPT_TIMEOUT_SECS)
        } else {
            Duration::from_millis(EMPTY_TRANSCRIPT_TIMEOUT_MS)
        };

        let wait_result = tokio::select! {
            _ = cancellation.cancelled() => {
                incident.cancel(IncidentStage::Asr, "session_cancelled");
                provider_task.abort();
                finish_cancelled_session(&app, runtime.clone(), session_id).await;
                return;
            }
            result = timeout(wait_duration, &mut provider_result) => result,
        };

        let transcript = match wait_result {
            Ok(Ok(Ok(transcript))) => transcript,
            Ok(Ok(Err(error))) => {
                if !has_preview_text && is_empty_input_error(&error) {
                    incident.cancel(IncidentStage::Asr, error.cancel_reason());
                    cancel_session_quietly(&app, runtime.clone(), session_id).await;
                    return;
                }
                incident.record_failure(
                    IncidentStage::Asr,
                    error.cancel_reason(),
                    &error.to_string(),
                    Recoverability::TextAndAudio,
                );
                fail_and_reset(&app, runtime.clone(), error.to_string(), session_id).await;
                return;
            }
            Ok(Err(error)) => {
                incident.record_failure(
                    IncidentStage::Asr,
                    "asr_result_channel_closed",
                    &error.to_string(),
                    Recoverability::TextAndAudio,
                );
                fail_and_reset(&app, runtime.clone(), error.to_string(), session_id).await;
                return;
            }
            Err(_) => {
                if has_preview_text {
                    incident.record_failure(
                        IncidentStage::Asr,
                        "asr_final_timeout",
                        "流式识别未在时限内返回最终文本",
                        Recoverability::TextAndAudio,
                    );
                    provider_task.abort();
                    fail_and_reset(
                        &app,
                        runtime.clone(),
                        format!("流式识别在 {FINAL_TRANSCRIPT_TIMEOUT_SECS} 秒内没有返回最终文本"),
                        session_id,
                    )
                    .await;
                    return;
                } else {
                    let late_preview_text = preview_state.lock().await.rendered_text();
                    if late_preview_text.trim().is_empty() {
                        incident.cancel(IncidentStage::Asr, "no_speech");
                        provider_task.abort();
                        cancel_session_quietly(&app, runtime.clone(), session_id).await;
                        return;
                    } else {
                        overlay::update_preinput(
                            &app,
                            PreInputPayload {
                                session_id,
                                seq: 0,
                                text: late_preview_text,
                                state: PreInputState::Finalizing,
                                confirmed_chars: None,
                                message: Some("正在收束".to_string()),
                            },
                        );

                        let wait_result = tokio::select! {
                            _ = cancellation.cancelled() => {
                                incident.cancel(IncidentStage::Asr, "session_cancelled");
                                provider_task.abort();
                                finish_cancelled_session(&app, runtime.clone(), session_id).await;
                                return;
                            }
                            result = timeout(
                                Duration::from_secs(FINAL_TRANSCRIPT_TIMEOUT_SECS),
                                &mut provider_result,
                            ) => result,
                        };
                        match wait_result {
                            Ok(Ok(Ok(transcript))) => transcript,
                            Ok(Ok(Err(error))) => {
                                incident.record_failure(
                                    IncidentStage::Asr,
                                    error.cancel_reason(),
                                    &error.to_string(),
                                    Recoverability::TextAndAudio,
                                );
                                fail_and_reset(
                                    &app,
                                    runtime.clone(),
                                    error.to_string(),
                                    session_id,
                                )
                                .await;
                                return;
                            }
                            Ok(Err(error)) => {
                                incident.record_failure(
                                    IncidentStage::Asr,
                                    "asr_result_channel_closed",
                                    &error.to_string(),
                                    Recoverability::TextAndAudio,
                                );
                                fail_and_reset(
                                    &app,
                                    runtime.clone(),
                                    error.to_string(),
                                    session_id,
                                )
                                .await;
                                return;
                            }
                            Err(_) => {
                                incident.record_failure(
                                    IncidentStage::Asr,
                                    "asr_final_timeout",
                                    "流式识别未在时限内返回最终文本",
                                    Recoverability::TextAndAudio,
                                );
                                provider_task.abort();
                                fail_and_reset(
                                    &app,
                                    runtime.clone(),
                                    format!(
                                        "流式识别在 {FINAL_TRANSCRIPT_TIMEOUT_SECS} 秒内没有返回最终文本"
                                    ),
                                    session_id,
                                )
                                .await;
                                return;
                            }
                        }
                    }
                }
            }
        };

        let final_text = transcript;
        incident.stage(
            IncidentStage::Capture,
            IncidentStageOutcome::Succeeded,
            None,
        );

        if cancellation.is_cancelled() {
            incident.cancel(IncidentStage::Delivery, "session_cancelled");
            finish_cancelled_session(&app, runtime.clone(), session_id).await;
            return;
        }

        if final_text.trim().is_empty() {
            incident.cancel(IncidentStage::Asr, "empty_final_transcript");
            cancel_session_quietly(&app, runtime.clone(), session_id).await;
            return;
        }
        let final_monotonic_us = attempt_started_at
            .elapsed()
            .as_micros()
            .min(u64::MAX as u128) as u64;
        incident.final_transcript(&final_text, final_monotonic_us);
        incident.stage(IncidentStage::Asr, IncidentStageOutcome::Succeeded, None);

        let delivery = DeliveryService::new(services.clone());
        incident.stage(IncidentStage::Delivery, IncidentStageOutcome::Running, None);
        if let Err(error) = delivery.validate(&final_text, &target, false) {
            incident.stage(
                IncidentStage::Delivery,
                IncidentStageOutcome::Failed,
                Some(error.code.to_string()),
            );
            incident.finding(
                IncidentStage::Delivery,
                error.code,
                &error.message,
                Recoverability::TextAndAudio,
            );
            queue_pending_output(
                &app,
                runtime.clone(),
                pending.clone(),
                session_id,
                final_text,
                target,
                error.code,
                error.message,
            );
            return;
        }

        overlay::update_preinput(
            &app,
            PreInputPayload {
                session_id,
                seq: 0,
                text: final_text.clone(),
                state: PreInputState::Finalizing,
                confirmed_chars: None,
                message: Some("正在写入".to_string()),
            },
        );

        let paste_payload = {
            let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
            runtime.machine.paste_started()
        };
        emit_state(&app, paste_payload);

        if let Err(error) = delivery
            .inject(final_text.clone(), injector, injection_method)
            .await
        {
            incident.stage(
                IncidentStage::Delivery,
                IncidentStageOutcome::Failed,
                Some(error.code.to_string()),
            );
            incident.finding(
                IncidentStage::Delivery,
                error.code,
                &error.message,
                Recoverability::TextAndAudio,
            );
            queue_pending_output(
                &app,
                runtime.clone(),
                pending.clone(),
                session_id,
                final_text,
                target,
                error.code,
                error.message,
            );
            return;
        }
        incident.stage(
            IncidentStage::Delivery,
            IncidentStageOutcome::Succeeded,
            None,
        );

        let (history_committed, discard_recovery_material) = if history_enabled {
            let committed = delivery
                .commit(final_text.clone(), app_context, session_config)
                .await;
            if committed {
                incident.stage(
                    IncidentStage::History,
                    IncidentStageOutcome::Succeeded,
                    None,
                );
            } else {
                incident.stage(
                    IncidentStage::History,
                    IncidentStageOutcome::Failed,
                    Some("history_write_failed".to_string()),
                );
                incident.finding(
                    IncidentStage::History,
                    "history_write_failed",
                    "文字已输入，但正式历史写入失败",
                    Recoverability::TextAndAudio,
                );
            }
            (committed, committed)
        } else {
            incident.stage(
                IncidentStage::History,
                IncidentStageOutcome::SkippedByPolicy,
                None,
            );
            (false, true)
        };
        incident.finish_delivered(history_committed, discard_recovery_material);

        record_session_outcome(&runtime, session_id, "delivered", None);

        if let Some(complete_payload) = complete_current_session(&runtime, session_id) {
            emit_state(&app, complete_payload);
        }
        overlay::hide_preinput_for_session(&app, session_id);
    });
}

fn handle_provider_failure(
    app: &AppHandle,
    runtime: SharedRuntime,
    session_id: u64,
    error: ProviderError,
    services: AppServices,
) {
    let cancel_reason = error.cancel_reason().to_string();
    let user_message = error.user_message();
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        if runtime.sessions.current_id != Some(session_id) {
            return;
        }
        let Some(session) = runtime.sessions.active.take() else {
            return;
        };
        let mut incident =
            IncidentAttemptGuard::new(services.incidents.sink(), session.attempt_id.clone());
        incident.stage(
            IncidentStage::Asr,
            IncidentStageOutcome::Failed,
            Some(cancel_reason.clone()),
        );
        incident.finding(
            IncidentStage::Asr,
            &cancel_reason,
            &user_message,
            Recoverability::TextAndAudio,
        );
        incident.finish(
            if matches!(error, ProviderError::Cancelled) {
                TerminalOutcome::Cancelled
            } else {
                TerminalOutcome::Failed
            },
            false,
        );
        let snapshot = session.audio_queue.snapshot();
        let recording_duration_ms = session.started_at.elapsed().as_millis() as u64;
        session.cancellation.cancel();
        session.deadline_cancellation.cancel();
        session.provider_task.abort();
        let _ = runtime.recorder.stop_streaming();
        clear_current_session(&mut runtime, session_id);
        runtime.sessions.last_metrics = Some(crate::SessionMetrics {
            session_id,
            audio_packets: snapshot.packets,
            queue_high_watermark: snapshot.high_watermark,
            overflow: snapshot.overflow,
            recording_duration_ms,
            cancel_reason: Some(cancel_reason.clone()),
            final_state: "cancelled".to_string(),
        });
        log::warn!(
            "voice session provider failed: session_id={}, cancel_reason={}, error={}",
            session_id,
            cancel_reason,
            error
        );
        runtime.machine.fail(user_message.clone())
    };
    emit_state(app, payload);
    overlay::update_preinput(
        app,
        PreInputPayload {
            session_id,
            seq: 0,
            text: String::new(),
            state: PreInputState::Error,
            confirmed_chars: Some(0),
            message: Some(user_message),
        },
    );
    schedule_idle_reset(app.clone(), runtime, session_id);
}

fn handle_audio_overflow(
    app: &AppHandle,
    runtime: SharedRuntime,
    services: AppServices,
    session_id: u64,
) {
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        if runtime.sessions.current_id != Some(session_id) {
            return;
        }
        let mut recording_duration_ms = 0;
        let mut snapshot = crate::audio::AudioQueueSnapshot {
            packets: 0,
            high_watermark: 0,
            overflow: true,
        };
        if let Some(session) = runtime.sessions.active.take() {
            recording_duration_ms = session.started_at.elapsed().as_millis() as u64;
            let mut incident =
                IncidentAttemptGuard::new(services.incidents.sink(), session.attempt_id.clone());
            incident.stage(
                IncidentStage::Capture,
                IncidentStageOutcome::Failed,
                Some("audio_queue_overflow".to_string()),
            );
            incident.finding(
                IncidentStage::Capture,
                "audio_queue_overflow",
                "ASR 音频队列已满，录音已取消",
                Recoverability::Audio,
            );
            incident.finish(TerminalOutcome::Failed, false);
            snapshot = session.audio_queue.snapshot();
            session.cancellation.cancel();
            session.deadline_cancellation.cancel();
            session.provider_task.abort();
        }
        let _ = runtime.recorder.stop_streaming();
        clear_current_session(&mut runtime, session_id);
        runtime.sessions.last_metrics = Some(crate::SessionMetrics {
            session_id,
            audio_packets: snapshot.packets,
            queue_high_watermark: snapshot.high_watermark,
            overflow: snapshot.overflow,
            recording_duration_ms,
            cancel_reason: Some("audio_queue_overflow".to_string()),
            final_state: "cancelled".to_string(),
        });
        log::warn!(
            "voice session audio overflow: session_id={}, packets={}, queue_high_watermark={}, overflow={}",
            session_id,
            snapshot.packets,
            snapshot.high_watermark,
            snapshot.overflow
        );
        runtime.machine.fail("网络处理过慢，录音已取消".to_string())
    };
    emit_state(app, payload);
    overlay::hide_preinput_for_session(app, session_id);
    schedule_idle_reset(app.clone(), runtime, session_id);
}

fn schedule_recording_deadline(
    app: AppHandle,
    controller: VoiceSessionHandle,
    session_id: u64,
    cancellation: Arc<SessionCancellation>,
) {
    tauri::async_runtime::spawn(async move {
        if wait_for_recording_deadline(cancellation, Duration::from_secs(MAX_RECORDING_SECS)).await
        {
            let _ = app;
            controller.report_deadline(session_id);
        }
    });
}

async fn wait_for_recording_deadline(
    cancellation: Arc<SessionCancellation>,
    duration: Duration,
) -> bool {
    tokio::select! {
        biased;
        _ = cancellation.cancelled() => false,
        _ = sleep(duration) => true,
    }
}

fn cancel_current_session(
    app: &AppHandle,
    runtime: SharedRuntime,
    incident_sink: Arc<dyn crate::incident::IncidentSink>,
) -> Result<(), String> {
    let session_id = {
        let mut runtime = runtime.lock().map_err(|error| error.to_string())?;
        let session_id = runtime.sessions.current_id;
        let incident_stage = if runtime.machine.state() == &VoiceState::Recording {
            IncidentStage::Capture
        } else {
            IncidentStage::Runtime
        };

        if let Some(cancellation) = &runtime.sessions.current_cancellation {
            cancellation.cancel();
        }

        if let Some(session) = runtime.sessions.active.take() {
            session.cancellation.cancel();
            let mut incident = IncidentAttemptGuard::new(incident_sink, session.attempt_id.clone());
            incident.cancel(incident_stage, "session_cancelled");
            session.deadline_cancellation.cancel();
            session.provider_task.abort();
            let _ = runtime.recorder.stop_streaming();
            clear_current_session(&mut runtime, session.session_id);
        }

        session_id
    };

    if let Some(session_id) = session_id {
        overlay::hide_preinput_for_session(app, session_id);
    }
    Ok(())
}

fn clear_current_session(runtime: &mut VoiceRuntime, session_id: u64) {
    if runtime.sessions.current_id == Some(session_id) {
        runtime.sessions.current_id = None;
        runtime.sessions.current_cancellation = None;
        runtime.active_activation = None;
    }
}

fn complete_current_session(runtime: &SharedRuntime, session_id: u64) -> Option<VoiceStatePayload> {
    let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
    if runtime.sessions.current_id != Some(session_id) {
        return None;
    }
    clear_current_session(&mut runtime, session_id);
    Some(runtime.machine.complete())
}

fn queue_pending_output(
    app: &AppHandle,
    runtime: SharedRuntime,
    pending: Arc<PendingOutputService>,
    session_id: u64,
    text: String,
    target: target::TargetWindowIdentity,
    reason_code: &str,
    reason_message: String,
) {
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        if runtime.sessions.current_id != Some(session_id) {
            return;
        }
        match pending.push(
            session_id,
            text,
            target,
            reason_code,
            reason_message.clone(),
        ) {
            Ok(_) => {
                if let Some(metrics) = &mut runtime.sessions.last_metrics {
                    if metrics.session_id == session_id {
                        metrics.final_state = "pending".to_string();
                        metrics.cancel_reason = Some(reason_code.to_string());
                    }
                }
                clear_current_session(&mut runtime, session_id);
                let mut payload = runtime.machine.complete();
                payload.message = "结果已进入待处理区".to_string();
                payload
            }
            Err(_) => {
                clear_current_session(&mut runtime, session_id);
                runtime.machine.fail("待处理结果已满，无法保留本次结果")
            }
        }
    };
    emit_state(app, payload);
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.emit("pending_outputs_changed", ());
    }
    overlay::hide_preinput_for_session(app, session_id);
}

async fn deliver_pending(
    app: &AppHandle,
    id: String,
    runtime: SharedRuntime,
    pending: Arc<PendingOutputService>,
    services: AppServices,
) -> crate::command_error::CommandResult<()> {
    use crate::command_error::CommandError;

    let config = services.config.snapshot();
    let (injector, record, method) = {
        let runtime = runtime
            .lock()
            .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?;
        if runtime.sessions.current_id.is_some() {
            return Err(CommandError::new(
                "session_active",
                "录音或识别进行中，暂时不能发送待处理结果",
            ));
        }
        let record = pending.reserve(&id).map_err(map_pending_error)?;
        let method = match config.injection_strategy_for(&record.target.executable_name) {
            InjectionStrategy::Unicode => InjectionMethod::Unicode,
            InjectionStrategy::ClipboardCompatibility => InjectionMethod::ClipboardCompatibility,
        };
        (runtime.injector.clone(), record, method)
    };

    let delivery = DeliveryService::new(services);
    if let Err(error) = delivery.validate(&record.dto.text, &record.target, true) {
        pending.release(&id);
        return Err(CommandError::new(error.code, error.message));
    }
    if let Err(error) = delivery
        .inject(record.dto.text.clone(), injector, method)
        .await
    {
        pending.release(&id);
        return Err(CommandError::new(error.code, error.message));
    }

    pending.complete(&id).map_err(map_pending_error)?;
    delivery
        .commit(
            record.dto.text,
            history::AppContext {
                app_name: Some(record.target.executable_name),
                app_title: record.target.window_title,
            },
            config,
        )
        .await;
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.emit("pending_outputs_changed", ());
    }
    Ok(())
}

fn map_pending_error(error: PendingOutputServiceError) -> crate::command_error::CommandError {
    use crate::command_error::CommandError;
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

fn record_session_outcome(
    runtime: &SharedRuntime,
    session_id: u64,
    final_state: &str,
    cancel_reason: Option<&str>,
) {
    if let Ok(mut runtime) = runtime.lock() {
        if let Some(metrics) = &mut runtime.sessions.last_metrics {
            if metrics.session_id == session_id {
                metrics.final_state = final_state.to_string();
                metrics.cancel_reason = cancel_reason.map(str::to_string);
                log::info!(
                    "voice session finished: session_id={}, audio_packets={}, queue_high_watermark={}, overflow={}, recording_duration_ms={}, cancel_reason={:?}, final_state={}",
                    metrics.session_id,
                    metrics.audio_packets,
                    metrics.queue_high_watermark,
                    metrics.overflow,
                    metrics.recording_duration_ms,
                    metrics.cancel_reason,
                    metrics.final_state
                );
            }
        }
    }
}

async fn finish_cancelled_session(app: &AppHandle, runtime: SharedRuntime, session_id: u64) {
    overlay::hide_preinput_for_session(app, session_id);
    if let Some(payload) = complete_current_session(&runtime, session_id) {
        emit_state(app, payload);
    }
}

async fn fail_and_reset(app: &AppHandle, runtime: SharedRuntime, message: String, session_id: u64) {
    log::warn!("voice input failed: {message}");
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        if runtime.sessions.current_id != Some(session_id) {
            return;
        }
        runtime.machine.fail(message)
    };
    emit_state(app, payload);
    overlay::update_preinput(
        app,
        PreInputPayload {
            session_id,
            seq: 0,
            text: String::new(),
            state: PreInputState::Error,
            confirmed_chars: Some(0),
            message: Some("失败".to_string()),
        },
    );
    sleep(Duration::from_millis(1200)).await;
    overlay::hide_preinput_for_session(app, session_id);
    if let Some(payload) = complete_current_session(&runtime, session_id) {
        emit_state(app, payload);
    }
}

async fn cancel_session_quietly(app: &AppHandle, runtime: SharedRuntime, session_id: u64) {
    overlay::hide_preinput_for_session(app, session_id);
    if let Some(payload) = complete_current_session(&runtime, session_id) {
        emit_state(app, payload);
    }
}

fn schedule_idle_reset(app: AppHandle, runtime: SharedRuntime, session_id: u64) {
    tauri::async_runtime::spawn(async move {
        sleep(Duration::from_millis(1200)).await;
        overlay::hide_preinput_for_session(&app, session_id);
        if let Some(payload) = complete_session_or_clear_error(&runtime, session_id) {
            emit_state(&app, payload);
        }
    });
}

fn complete_session_or_clear_error(
    runtime: &SharedRuntime,
    session_id: u64,
) -> Option<VoiceStatePayload> {
    let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
    match runtime.sessions.current_id {
        Some(current_id) if current_id == session_id => {
            clear_current_session(&mut runtime, session_id);
            Some(runtime.machine.complete())
        }
        None if runtime.machine.state() == &VoiceState::Error => Some(runtime.machine.complete()),
        _ => None,
    }
}

fn emit_state(app: &AppHandle, payload: VoiceStatePayload) {
    if let Some(handle) = app.try_state::<VoiceSessionHandle>() {
        handle.publish_payload(payload.clone());
    }
    if let Err(error) = app.emit(VOICE_STATE_EVENT, payload) {
        log::warn!("failed to emit voice state: {error}");
    }
}

fn is_empty_input_error(error: &ProviderError) -> bool {
    matches!(error, ProviderError::NoSpeech)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancelled_recording_deadline_does_not_fire() {
        let cancellation = Arc::new(SessionCancellation::default());
        cancellation.cancel();

        assert!(!wait_for_recording_deadline(cancellation, Duration::from_secs(120)).await);
    }

    #[tokio::test]
    async fn active_recording_deadline_fires_when_elapsed() {
        let cancellation = Arc::new(SessionCancellation::default());

        assert!(wait_for_recording_deadline(cancellation, Duration::ZERO).await);
    }

    #[test]
    fn only_the_active_activation_can_finish_a_session() {
        let active = VoiceActivation::shortcut();
        let stale = VoiceActivation::shortcut();

        assert!(activation_matches(&Some(active.clone()), &active.id));
        assert!(!activation_matches(&Some(active), &stale.id));
        assert!(!activation_matches(&None, &stale.id));
    }

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
                } => format!("final:{text}:{monotonic_us}"),
                IncidentEvent::AttemptEnded {
                    outcome,
                    history_committed,
                    discard_recovery_material,
                    ..
                } => format!(
                    "end:{}:{history_committed}:{discard_recovery_material}",
                    outcome.as_str()
                ),
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
    fn incident_guard_emits_failure_finding_before_one_terminal_event() {
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
    fn incident_guard_records_canonical_final_before_delivered_terminal_event() {
        let sink = Arc::new(CollectingIncidentSink::default());
        {
            let mut guard = IncidentAttemptGuard::new(sink.clone(), "attempt".to_string());
            guard.final_transcript("canonical final", 42);
            guard.finish_delivered(false, true);
        }

        assert_eq!(
            sink.events.lock().unwrap().as_slice(),
            ["final:canonical final:42", "end:succeeded:false:true"]
        );
    }

    #[test]
    fn dropping_an_unfinished_incident_guard_records_pipeline_incomplete_once() {
        let sink = Arc::new(CollectingIncidentSink::default());
        {
            let _guard = IncidentAttemptGuard::new(sink.clone(), "attempt".to_string());
        }

        assert_eq!(
            sink.events.lock().unwrap().as_slice(),
            ["finding:pipeline_incomplete", "end:failed:false:false",]
        );
    }
}
