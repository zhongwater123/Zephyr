use super::super::contract::{
    FinalizationJob, FinalizeOutcome, PendingDraft, VoiceInternalEvent, VoiceInternalEventSink,
};
use super::super::incident::IncidentAttemptGuard;
use super::super::resources::SessionResources;
use crate::delivery::DeliveryService;
use crate::incident::model::{
    Recoverability, Stage as IncidentStage, StageOutcome as IncidentStageOutcome, TerminalOutcome,
};
use crate::provider::ProviderError;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

const FINAL_TRANSCRIPT_TIMEOUT_SECS: u64 = 25;
const EMPTY_TRANSCRIPT_TIMEOUT_MS: u64 = 800;

pub(crate) fn spawn_finalization(job: FinalizationJob, events: VoiceInternalEventSink) {
    tauri::async_runtime::spawn(async move {
        let outcome = finalize(job, &events).await;
        let _ = events
            .send(VoiceInternalEvent::FinalizationFinished(outcome))
            .await;
    });
}

async fn finalize(job: FinalizationJob, events: &VoiceInternalEventSink) -> FinalizeOutcome {
    let FinalizationJob {
        session,
        injector,
        injection_method,
        services,
    } = job;
    let SessionResources {
        session_id,
        attempt_id,
        provider_task,
        mut provider_result,
        preview_state,
        app_context,
        target,
        cancellation,
        deadline_cancellation: _,
        audio_queue: _,
        started_at,
        config,
        state_tx: _,
    } = session;
    let mut incident = IncidentAttemptGuard::new(services.incidents.sink(), attempt_id);
    let has_preview = !preview_state.lock().await.rendered_text().trim().is_empty();
    if has_preview {
        let text = preview_state.lock().await.rendered_text();
        let _ = events
            .send(VoiceInternalEvent::FinalizationProgress { session_id, text })
            .await;
    }

    let initial_wait = if has_preview {
        Duration::from_secs(FINAL_TRANSCRIPT_TIMEOUT_SECS)
    } else {
        Duration::from_millis(EMPTY_TRANSCRIPT_TIMEOUT_MS)
    };
    let first = tokio::select! {
        _ = cancellation.cancelled() => None,
        result = timeout(initial_wait, &mut provider_result) => Some(result),
    };
    let transcript = match first {
        None => {
            provider_task.abort();
            incident.cancel(IncidentStage::Asr, "session_cancelled");
            return FinalizeOutcome::Cancelled {
                session_id,
                reason: "session_cancelled".to_string(),
            };
        }
        Some(Ok(Ok(Ok(text)))) => text,
        Some(Ok(Ok(Err(error)))) => {
            return provider_failure_outcome(session_id, error, has_preview, &mut incident);
        }
        Some(Ok(Err(error))) => {
            let message = error.to_string();
            incident.record_failure(
                IncidentStage::Asr,
                "asr_result_channel_closed",
                &message,
                Recoverability::TextAndAudio,
            );
            incident.finish(TerminalOutcome::Failed, false);
            return FinalizeOutcome::Failed {
                session_id,
                reason_code: "asr_result_channel_closed".to_string(),
                message,
            };
        }
        Some(Err(_)) if !has_preview => {
            let late_preview = preview_state.lock().await.rendered_text();
            if late_preview.trim().is_empty() {
                provider_task.abort();
                incident.cancel(IncidentStage::Asr, "no_speech");
                return FinalizeOutcome::Cancelled {
                    session_id,
                    reason: "no_speech".to_string(),
                };
            }
            let _ = events
                .send(VoiceInternalEvent::FinalizationProgress {
                    session_id,
                    text: late_preview,
                })
                .await;
            let second = tokio::select! {
                _ = cancellation.cancelled() => None,
                result = timeout(Duration::from_secs(FINAL_TRANSCRIPT_TIMEOUT_SECS), &mut provider_result) => Some(result),
            };
            match second {
                None => {
                    provider_task.abort();
                    incident.cancel(IncidentStage::Asr, "session_cancelled");
                    return FinalizeOutcome::Cancelled {
                        session_id,
                        reason: "session_cancelled".to_string(),
                    };
                }
                Some(Ok(Ok(Ok(text)))) => text,
                Some(Ok(Ok(Err(error)))) => {
                    return provider_failure_outcome(session_id, error, true, &mut incident);
                }
                Some(Ok(Err(error))) => {
                    let message = error.to_string();
                    incident.record_failure(
                        IncidentStage::Asr,
                        "asr_result_channel_closed",
                        &message,
                        Recoverability::TextAndAudio,
                    );
                    incident.finish(TerminalOutcome::Failed, false);
                    return FinalizeOutcome::Failed {
                        session_id,
                        reason_code: "asr_result_channel_closed".to_string(),
                        message,
                    };
                }
                Some(Err(_)) => {
                    return final_timeout(session_id, &provider_task, &mut incident);
                }
            }
        }
        Some(Err(_)) => return final_timeout(session_id, &provider_task, &mut incident),
    };

    incident.stage(
        IncidentStage::Capture,
        IncidentStageOutcome::Succeeded,
        None,
    );
    if cancellation.is_cancelled() {
        incident.cancel(IncidentStage::Delivery, "session_cancelled");
        return FinalizeOutcome::Cancelled {
            session_id,
            reason: "session_cancelled".to_string(),
        };
    }
    if transcript.trim().is_empty() {
        incident.cancel(IncidentStage::Asr, "empty_final_transcript");
        return FinalizeOutcome::Cancelled {
            session_id,
            reason: "empty_final_transcript".to_string(),
        };
    }
    incident.final_transcript(
        &transcript,
        started_at.elapsed().as_micros().min(u64::MAX as u128) as u64,
    );
    incident.stage(IncidentStage::Asr, IncidentStageOutcome::Succeeded, None);

    let delivery = DeliveryService::new(services.clone());
    incident.stage(IncidentStage::Delivery, IncidentStageOutcome::Running, None);
    if let Err(error) = delivery.validate(&transcript, &target, false) {
        incident.record_failure(
            IncidentStage::Delivery,
            error.code,
            &error.message,
            Recoverability::TextAndAudio,
        );
        incident.finish(TerminalOutcome::Failed, false);
        return FinalizeOutcome::Pending {
            session_id,
            draft: PendingDraft {
                text: transcript,
                target,
                reason_code: error.code.to_string(),
                reason_message: error.message,
            },
        };
    }

    let (authorization, authorized) = oneshot::channel();
    if !events
        .send(VoiceInternalEvent::ReadyToInject {
            session_id,
            text: transcript.clone(),
            response: authorization,
        })
        .await
        || !authorized.await.unwrap_or(false)
        || cancellation.is_cancelled()
    {
        incident.cancel(IncidentStage::Delivery, "session_cancelled");
        return FinalizeOutcome::Cancelled {
            session_id,
            reason: "session_cancelled".to_string(),
        };
    }

    if let Err(error) = delivery
        .inject(transcript.clone(), injector, injection_method)
        .await
    {
        incident.record_failure(
            IncidentStage::Delivery,
            error.code,
            &error.message,
            Recoverability::TextAndAudio,
        );
        incident.finish(TerminalOutcome::Failed, false);
        return FinalizeOutcome::Pending {
            session_id,
            draft: PendingDraft {
                text: transcript,
                target,
                reason_code: error.code.to_string(),
                reason_message: error.message,
            },
        };
    }
    incident.stage(
        IncidentStage::Delivery,
        IncidentStageOutcome::Succeeded,
        None,
    );

    let history_enabled = config.history_enabled;
    let history_committed = if history_enabled {
        let committed = delivery.commit(transcript, app_context, config).await;
        if committed {
            incident.stage(
                IncidentStage::History,
                IncidentStageOutcome::Succeeded,
                None,
            );
        } else {
            incident.record_failure(
                IncidentStage::History,
                "history_write_failed",
                "文字已输入，但正式历史写入失败",
                Recoverability::TextAndAudio,
            );
        }
        committed
    } else {
        incident.stage(
            IncidentStage::History,
            IncidentStageOutcome::SkippedByPolicy,
            None,
        );
        false
    };
    incident.finish_delivered(history_committed, history_committed || !history_enabled);
    FinalizeOutcome::Delivered {
        session_id,
        history_committed,
    }
}

fn provider_failure_outcome(
    session_id: u64,
    error: ProviderError,
    had_preview: bool,
    incident: &mut IncidentAttemptGuard,
) -> FinalizeOutcome {
    if !had_preview && matches!(error, ProviderError::NoSpeech) {
        incident.cancel(IncidentStage::Asr, error.cancel_reason());
        return FinalizeOutcome::Cancelled {
            session_id,
            reason: error.cancel_reason().to_string(),
        };
    }
    let reason_code = error.cancel_reason().to_string();
    let message = error.to_string();
    incident.record_failure(
        IncidentStage::Asr,
        &reason_code,
        &message,
        Recoverability::TextAndAudio,
    );
    incident.finish(TerminalOutcome::Failed, false);
    FinalizeOutcome::Failed {
        session_id,
        reason_code,
        message,
    }
}

fn final_timeout(
    session_id: u64,
    provider_task: &tauri::async_runtime::JoinHandle<()>,
    incident: &mut IncidentAttemptGuard,
) -> FinalizeOutcome {
    let message = format!("流式识别在 {FINAL_TRANSCRIPT_TIMEOUT_SECS} 秒内没有返回最终文本");
    incident.record_failure(
        IncidentStage::Asr,
        "asr_final_timeout",
        &message,
        Recoverability::TextAndAudio,
    );
    incident.finish(TerminalOutcome::Failed, false);
    provider_task.abort();
    FinalizeOutcome::Failed {
        session_id,
        reason_code: "asr_final_timeout".to_string(),
        message,
    }
}
