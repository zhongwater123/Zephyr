use crate::incident::model::IncidentEvent;
use crate::incident::IncidentSink;
use crate::preview::TranscriptPreviewState;
use crate::provider::TranscriptEvent;
use crate::state::VoiceState;
use crate::voice_controller::presenter::VoicePresentationSink;
use crate::voice_controller::{VoiceInternalEventSink, VoiceSessionObserver};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

const PARTIAL_CHECKPOINT_INTERVAL: Duration = Duration::from_millis(500);

pub fn spawn_transcript_event_relay(
    presenter: VoicePresentationSink,
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
            presenter.progress(session_id, state, text, confirmed_chars);
        }
    });
}

pub fn spawn_audio_overflow_watcher(
    events: VoiceInternalEventSink,
    monitor: Arc<crate::audio::AudioQueueMonitor>,
    session_id: u64,
) {
    tauri::async_runtime::spawn(async move {
        monitor.overflowed().await;
        events.report_audio_overflow(session_id).await;
    });
}
