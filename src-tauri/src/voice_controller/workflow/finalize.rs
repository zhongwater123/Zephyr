use super::super::contract::{
    FinalizationJob, FinalizeOutcome, PendingDraft, VoiceInternalEvent, VoiceInternalEventSink,
};
use super::super::incident::IncidentAttemptGuard;
use super::super::resources::SessionResources;
use crate::delivery::{DeliveryIntent, DeliveryService};
use crate::history::HistoryProvenance;
use crate::incident::model::{
    Recoverability, Stage as IncidentStage, StageOutcome as IncidentStageOutcome, TerminalOutcome,
};
use crate::inject::{RestorationState, SubmissionState};
use crate::provider::ProviderError;
use crate::text_processing::{
    ActivationIntent, FrozenTranscript, PolishLevel, ProcessingPlan, ProcessingRequest,
};
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

const FINAL_TRANSCRIPT_TIMEOUT_SECS: u64 = 25;
const EMPTY_TRANSCRIPT_TIMEOUT_MS: u64 = 800;

fn should_start_text_processing(polish_level: PolishLevel) -> bool {
    !polish_level.is_fast()
}

fn asr_direct_delivery(transcript: &FrozenTranscript) -> (String, HistoryProvenance) {
    (
        transcript.as_str().to_string(),
        HistoryProvenance::asr_direct(),
    )
}

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
        executor,
        delivery_mode,
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
        activation_intent,
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

    crate::provider::diagnostics::log_text(crate::provider::diagnostics::AsrTextTrace {
        stage: "aggregate_final",
        session_id,
        request_id: None,
        sequence: 0,
        kind: "final_transcript",
        is_final: Some(true),
        text: &transcript,
    });

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
    let frozen_transcript = match FrozenTranscript::new(transcript.clone()) {
        Ok(value) => value,
        Err(_) => {
            incident.cancel(IncidentStage::Asr, "empty_final_transcript");
            return FinalizeOutcome::Cancelled {
                session_id,
                reason: "empty_final_transcript".to_string(),
            };
        }
    };
    match activation_intent {
        ActivationIntent::SmartDictation => {}
    }
    let polish_level = PolishLevel::try_from(config.polish_level).unwrap_or_default();
    let plan = ProcessingPlan::new(
        config.revision,
        polish_level,
        target.executable_name.clone(),
        app_context.app_name.clone(),
    );
    let processing_attempt = if !should_start_text_processing(polish_level) {
        incident.stage(
            IncidentStage::Processing,
            IncidentStageOutcome::SkippedByPolicy,
            None,
        );
        None
    } else {
        incident.stage(
            IncidentStage::Processing,
            IncidentStageOutcome::Running,
            None,
        );
        let processing_started = std::time::Instant::now();
        let processing_result = match services.prompt_repository.load() {
            Ok(prompt) => {
                let request = ProcessingRequest {
                    plan: plan.clone(),
                    prompt,
                    transcript: frozen_transcript.clone(),
                };
                tokio::select! {
                    biased;
                    _ = cancellation.cancelled() => {
                        incident.cancel(IncidentStage::Processing, "session_cancelled");
                        return FinalizeOutcome::Cancelled {
                            session_id,
                            reason: "session_cancelled".to_string(),
                        };
                    }
                    result = services.text_processor.process(request) => {
                        result.map_err(|error| (error.reason_code().to_string(), error.to_string()))
                    }
                }
            }
            Err(error) => Err((
                "processing_prompt_unavailable".to_string(),
                error.to_string(),
            )),
        };
        let processing_elapsed = processing_started.elapsed();
        incident.metric(
            "text_processing_latency_ms",
            processing_elapsed.as_secs_f64() * 1_000.0,
            "milliseconds",
        );
        Some((processing_result, processing_elapsed))
    };
    let (delivery_text, provenance) = match processing_attempt {
        None => asr_direct_delivery(&frozen_transcript),
        Some((Ok(output), _)) => {
            incident.stage(
                IncidentStage::Processing,
                IncidentStageOutcome::Succeeded,
                None,
            );
            let provenance = HistoryProvenance::smart_processed(
                format!("smart_polish_l{}", output.polish_level.as_u8()),
                output.prompt_version.clone(),
            );
            (output.text, provenance)
        }
        Some((Err((reason_code, message)), processing_elapsed)) => {
            let diagnostic = format!(
                "{message}; polish_level={}; target_executable={}; elapsed_ms={}; deadline_ms={}",
                plan.polish_level.as_u8(),
                plan.target_executable,
                processing_elapsed.as_millis(),
                plan.deadline.as_millis(),
            );
            incident.record_failure(
                IncidentStage::Processing,
                &reason_code,
                &diagnostic,
                Recoverability::TextAndAudio,
            );
            log::warn!(
                "smart dictation processing failed; using frozen ASR fallback: {diagnostic}"
            );
            (
                frozen_transcript.as_str().to_string(),
                HistoryProvenance::asr_fallback(),
            )
        }
    };

    let delivery = DeliveryService::new(services.clone());
    let delivery_intent = DeliveryIntent::SmartDictation;
    incident.stage(IncidentStage::Delivery, IncidentStageOutcome::Running, None);
    let delivery_text =
        match delivery.validate_with_intent(&delivery_text, &target, false, delivery_intent) {
            Ok(prepared) => prepared,
            Err(error) => {
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
                        text: delivery_text,
                        target,
                        reason_code: error.code.to_string(),
                        reason_message: error.message,
                        delivery_intent,
                        provenance,
                        certainty: crate::target::DeliveryCertainty::Retryable,
                    },
                };
            }
        };

    let (authorization, authorized) = oneshot::channel();
    if !events
        .send(VoiceInternalEvent::ReadyToInject {
            session_id,
            text: delivery_text.clone(),
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

    crate::provider::diagnostics::log_text(crate::provider::diagnostics::AsrTextTrace {
        stage: "delivery_inject",
        session_id,
        request_id: None,
        sequence: 0,
        kind: "inject_payload",
        is_final: Some(true),
        text: &delivery_text,
    });

    let receipt = match delivery
        .inject_with_intent(
            delivery_text.clone(),
            target.clone(),
            executor,
            delivery_mode,
            delivery_intent,
        )
        .await
    {
        Ok(receipt) => receipt,
        Err(error) => {
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
                    text: delivery_text,
                    target,
                    reason_code: error.code.to_string(),
                    reason_message: error.message,
                    delivery_intent,
                    provenance,
                    certainty: crate::target::DeliveryCertainty::Retryable,
                },
            };
        }
    };
    match receipt.submission {
        SubmissionState::NotSubmitted => {
            let message = "未向目标提交任何输入事件".to_string();
            incident.record_failure(
                IncidentStage::Delivery,
                "injection_not_submitted",
                &message,
                Recoverability::TextAndAudio,
            );
            incident.finish(TerminalOutcome::Failed, false);
            return FinalizeOutcome::Pending {
                session_id,
                draft: PendingDraft {
                    text: delivery_text,
                    target,
                    reason_code: "injection_not_submitted".to_string(),
                    reason_message: message,
                    delivery_intent,
                    provenance,
                    certainty: crate::target::DeliveryCertainty::Retryable,
                },
            };
        }
        SubmissionState::Unknown => {
            let message = "文本可能已经输入，请先检查目标窗口；系统不会自动重试".to_string();
            incident.record_failure(
                IncidentStage::Delivery,
                "delivery_submission_unknown",
                &message,
                Recoverability::TextAndAudio,
            );
            incident.finish(TerminalOutcome::Failed, false);
            return FinalizeOutcome::Pending {
                session_id,
                draft: PendingDraft {
                    text: delivery_text,
                    target,
                    reason_code: "delivery_submission_unknown".to_string(),
                    reason_message: message,
                    delivery_intent,
                    provenance,
                    certainty: crate::target::DeliveryCertainty::MayHaveBeenSubmitted,
                },
            };
        }
        SubmissionState::Submitted => {}
    }
    match receipt.restoration {
        RestorationState::NotNeeded | RestorationState::Restored => {}
        RestorationState::SkippedConcurrentChange => incident.finding(
            IncidentStage::Delivery,
            "clipboard_restore_skipped",
            "paste submitted; clipboard changed concurrently, so restoration was skipped",
            Recoverability::None,
        ),
        RestorationState::Failed => incident.finding(
            IncidentStage::Delivery,
            "clipboard_restore_failed_after_submit",
            "paste submitted; clipboard restoration failed",
            Recoverability::None,
        ),
    }
    incident.stage(
        IncidentStage::Delivery,
        IncidentStageOutcome::Succeeded,
        None,
    );

    let history_enabled = config.history_enabled;
    let history_committed = if history_enabled {
        let committed = delivery
            .commit_with_provenance(delivery_text, app_context, config, provenance)
            .await;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_is_the_only_mode_that_bypasses_text_processing() {
        assert!(!should_start_text_processing(PolishLevel::Fast));
        assert!(should_start_text_processing(PolishLevel::Light));
        assert!(should_start_text_processing(PolishLevel::Standard));
        assert!(should_start_text_processing(PolishLevel::Deep));
    }

    #[test]
    fn fast_direct_delivery_preserves_frozen_asr_text_and_origin() {
        let transcript = FrozenTranscript::new("  原话\r\n第二行  ".to_string()).unwrap();
        let (text, provenance) = asr_direct_delivery(&transcript);
        assert_eq!(text, "  原话\r\n第二行  ");
        assert_eq!(
            provenance.text_origin,
            crate::history::HISTORY_ORIGIN_ASR_DIRECT
        );
        assert!(provenance.processor_profile.is_none());
        assert!(provenance.processor_version.is_none());
    }
}
