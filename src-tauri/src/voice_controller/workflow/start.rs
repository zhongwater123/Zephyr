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
use crate::target_port::TargetPort;
use crate::{history, hotwords};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch};

const STREAM_CHUNK_MS: u16 = 200;
const AUDIO_QUEUE_CAPACITY: usize = 32;

pub(crate) struct StartJob {
    pub(crate) audio: AudioSessionHandle,
    pub(crate) services: AppServices,
    pub(crate) targets: Arc<dyn TargetPort>,
    pub(crate) config: crate::config::AppConfig,
    pub(crate) session_id: u64,
    pub(crate) cancellation: Arc<SessionCancellation>,
    pub(crate) activation_intent: crate::text_processing::ActivationIntent,
}

pub(crate) fn spawn_start(job: StartJob, events: VoiceInternalEventSink) {
    tauri::async_runtime::spawn(async move {
        let outcome = prepare_start(job).await;
        let _ = events
            .send(VoiceInternalEvent::StartFinished(outcome))
            .await;
    });
}

async fn prepare_start(job: StartJob) -> StartOutcome {
    let StartJob {
        audio,
        services,
        targets,
        config,
        session_id,
        cancellation,
        activation_intent,
    } = job;
    if cancellation.is_cancelled() {
        return StartOutcome::Cancelled { session_id };
    }

    let (target, app_context) = match capture_target_context(targets.as_ref()) {
        Ok(captured) => captured,
        Err(message) => {
            return StartOutcome::Failed {
                session_id,
                message,
            }
        }
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

fn capture_target_context(
    targets: &dyn TargetPort,
) -> Result<(crate::target_port::CapturedTarget, history::AppContext), String> {
    let target = targets.capture()?;
    let app_context = history::AppContext {
        app_name: Some(target.context().application_key.clone()),
        app_title: target.context().window_title.clone(),
    };
    Ok((target, app_context))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_port::tests::FakeTargetPort;

    #[test]
    fn capture_failure_is_returned_before_start_resources_are_created() {
        let targets = FakeTargetPort::failing_capture("capture failed");
        assert_eq!(
            capture_target_context(&targets).unwrap_err(),
            "capture failed"
        );
        assert_eq!(targets.calls(), vec!["capture"]);
    }

    #[test]
    fn captured_windows_application_context_is_preserved() {
        let targets = FakeTargetPort::available();
        let (target, context) = capture_target_context(&targets).unwrap();
        assert_eq!(target.context().application_key, "notepad.exe");
        assert_eq!(context.app_name.as_deref(), Some("notepad.exe"));
        assert_eq!(context.app_title.as_deref(), Some("Target"));
        assert_eq!(targets.calls(), vec!["capture"]);
    }
}
