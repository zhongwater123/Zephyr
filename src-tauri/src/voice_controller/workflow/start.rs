use super::super::audio_actor::AudioSessionHandle;
use super::super::contract::{
    StartOutcome, StartedSession, VoiceInternalEvent, VoiceInternalEventSink,
};
use super::super::incident::IncidentAttemptGuard;
use super::super::resources::{PreparedSession, SessionCancellation};
use crate::audio::{AudioQueueMonitor, IncidentAudioTap};
use crate::incident::model::{
    AttemptPolicy as IncidentAttemptPolicy, IncidentEvent, Recoverability, Stage as IncidentStage,
    StageOutcome as IncidentStageOutcome, TerminalOutcome,
};
use crate::preview::TranscriptPreviewState;
use crate::services::AppServices;
use crate::{history, hotwords};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch};

const STREAM_CHUNK_MS: u16 = 200;
const AUDIO_QUEUE_CAPACITY: usize = 32;

pub(crate) fn spawn_start(
    audio: AudioSessionHandle,
    services: AppServices,
    config: crate::config::AppConfig,
    session_id: u64,
    cancellation: Arc<SessionCancellation>,
    activation_intent: crate::text_processing::ActivationIntent,
    events: VoiceInternalEventSink,
) {
    tauri::async_runtime::spawn(async move {
        let outcome = prepare_start(
            audio,
            services,
            config,
            session_id,
            cancellation,
            activation_intent,
        )
        .await;
        let _ = events
            .send(VoiceInternalEvent::StartFinished(outcome))
            .await;
    });
}

async fn prepare_start(
    audio: AudioSessionHandle,
    services: AppServices,
    config: crate::config::AppConfig,
    session_id: u64,
    cancellation: Arc<SessionCancellation>,
    activation_intent: crate::text_processing::ActivationIntent,
) -> StartOutcome {
    if cancellation.is_cancelled() {
        return StartOutcome::Cancelled { session_id };
    }

    let target = match crate::target::capture_foreground_target() {
        Ok(target) => target,
        Err(message) => {
            return StartOutcome::Failed {
                session_id,
                message,
            }
        }
    };
    let app_context = history::AppContext {
        app_name: Some(target.executable_name.clone()),
        app_title: target.window_title.clone(),
    };
    let attempt_id = uuid::Uuid::new_v4().to_string();
    let content_enabled = config.incident_recovery_enabled && config.incident_consent_version > 0;
    let policy = IncidentAttemptPolicy {
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
        policy,
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

    let (chunk_tx, chunk_rx) = mpsc::channel(AUDIO_QUEUE_CAPACITY);
    let (transcript_tx, transcript_events) = watch::channel(None);
    let audio_queue = Arc::new(AudioQueueMonitor::default());
    let preview_state = Arc::new(tokio::sync::Mutex::new(TranscriptPreviewState::default()));
    let incident_audio_tap =
        content_enabled.then(|| IncidentAudioTap::new(incident_sink.clone(), attempt_id.clone()));
    let asr_hints = match hotwords::compose_asr_hints(&config, &app_context) {
        Ok(hints) => hints,
        Err(error) => {
            log::warn!("failed to compose ASR hotword hints: {error}");
            None
        }
    };
    let stream_info = match audio
        .start(
            session_id,
            STREAM_CHUNK_MS,
            chunk_tx,
            audio_queue.clone(),
            incident_audio_tap,
            cancellation.clone(),
        )
        .await
    {
        Ok(stream_info) => stream_info,
        Err(message) => {
            let mut incident = IncidentAttemptGuard::new(incident_sink, attempt_id);
            incident.record_failure(
                IncidentStage::Capture,
                "capture_start_failed",
                &message,
                Recoverability::None,
            );
            incident.finish(TerminalOutcome::Failed, false);
            return StartOutcome::Failed {
                session_id,
                message,
            };
        }
    };

    if cancellation.is_cancelled() {
        audio.cancel(session_id).await;
        let mut incident = IncidentAttemptGuard::new(incident_sink, attempt_id);
        incident.cancel(IncidentStage::Capture, "session_cancelled");
        return StartOutcome::Cancelled { session_id };
    }

    let provider = services.provider.build(&config);
    let deadline_cancellation = Arc::new(SessionCancellation::default());
    StartOutcome::Started(StartedSession {
        prepared: PreparedSession {
            session_id,
            attempt_id,
            provider,
            stream_info,
            chunk_rx,
            transcript_tx,
            transcript_events,
            preview_state,
            app_context,
            target,
            cancellation,
            deadline_cancellation,
            audio_queue,
            started_at: Instant::now(),
            config,
            activation_intent,
            asr_hints,
        },
    })
}
