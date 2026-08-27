use crate::incident::model::IncidentEvent;
use crate::incident::IncidentSink;
use crate::overlay::{self, PreInputPayload, PreInputState};
use crate::preview::TranscriptPreviewState;
use crate::provider::TranscriptEvent;
use crate::state::{VoiceState, VoiceStatePayload};
use crate::voice_controller::{VoiceSessionHandle, VoiceSessionObserver};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;

const VOICE_STATE_EVENT: &str = "voice_state_changed";
const PARTIAL_CHECKPOINT_INTERVAL: Duration = Duration::from_millis(500);

fn emit_state(app: &AppHandle, payload: VoiceStatePayload) {
    if let Err(error) = app.emit(VOICE_STATE_EVENT, payload) {
        log::warn!("failed to emit voice state: {error}");
    }
}

pub fn spawn_transcript_event_relay(
    app: AppHandle,
    observer: VoiceSessionObserver,
    mut event_rx: watch::Receiver<Option<TranscriptEvent>>,
    preview_state: Arc<tokio::sync::Mutex<TranscriptPreviewState>>,
    session_id: u64,
    incident_sink: Arc<dyn IncidentSink>,
) {
    tauri::async_runtime::spawn(async move {
        let mut last_checkpoint = Instant::now() - PARTIAL_CHECKPOINT_INTERVAL;
        while event_rx.changed().await.is_ok() {
            let Some(event) = event_rx.borrow_and_update().clone() else {
                continue;
            };

            if event.text.trim().is_empty() {
                continue;
            }

            let Some(observation) = observer.observe(session_id) else {
                continue;
            };
            let state = observation.state;
            let attempt_id = observation.attempt_id;
            let monotonic_us = observation.monotonic_us;

            if !matches!(state, VoiceState::Recording | VoiceState::Transcribing) {
                continue;
            }

            let (text, confirmed_chars) = {
                let mut preview_state = preview_state.lock().await;
                let text = preview_state.apply_event(&event);
                let confirmed_chars = preview_state.confirmed_chars();
                (text, confirmed_chars)
            };
            if !event.is_final && last_checkpoint.elapsed() >= PARTIAL_CHECKPOINT_INTERVAL {
                last_checkpoint = Instant::now();
                let _ = incident_sink.try_emit(IncidentEvent::PartialCheckpoint {
                    attempt_id,
                    text: text.clone(),
                    confirmed_chars,
                    monotonic_us,
                });
            }
            emit_state(
                &app,
                VoiceStatePayload {
                    state: state.clone(),
                    message: format!("正在识别 {} 个字", text.chars().count()),
                    elapsed_ms: None,
                },
            );

            overlay::update_preinput(
                &app,
                PreInputPayload {
                    session_id,
                    seq: 0,
                    text,
                    state: if matches!(state, VoiceState::Recording) {
                        PreInputState::Recording
                    } else {
                        PreInputState::Transcribing
                    },
                    confirmed_chars: Some(confirmed_chars),
                    message: None,
                },
            );
        }
    });
}

pub fn spawn_audio_overflow_watcher(
    app: AppHandle,
    controller: VoiceSessionHandle,
    monitor: Arc<crate::audio::AudioQueueMonitor>,
    session_id: u64,
) {
    tauri::async_runtime::spawn(async move {
        monitor.overflowed().await;
        let _ = app;
        controller.report_audio_overflow(session_id);
    });
}
