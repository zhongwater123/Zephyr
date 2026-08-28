use crate::audio::AudioQueueMonitor;
use crate::config::AppConfig;
use crate::history::AppContext;
use crate::preview::TranscriptPreviewState;
use crate::provider::{
    AsrSessionHints, AudioChunk, AudioStreamInfo, ProviderError, StreamingTranscriptionProvider,
    TranscriptEvent,
};
use crate::target::TargetWindowIdentity;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot, watch, Notify};

#[derive(Debug, Default)]
pub(super) struct SessionCancellation {
    cancelled: AtomicBool,
    notify: Notify,
}

impl SessionCancellation {
    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(super) async fn cancelled(&self) {
        let notified = self.notify.notified();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetrics {
    pub session_id: u64,
    pub audio_packets: u64,
    pub queue_high_watermark: usize,
    pub overflow: bool,
    pub recording_duration_ms: u64,
    pub cancel_reason: Option<String>,
    pub final_state: String,
}

pub(super) struct PreparedSession {
    pub session_id: u64,
    pub attempt_id: String,
    pub provider: Arc<dyn StreamingTranscriptionProvider>,
    pub stream_info: AudioStreamInfo,
    pub chunk_rx: mpsc::Receiver<AudioChunk>,
    pub transcript_tx: watch::Sender<Option<TranscriptEvent>>,
    pub transcript_events: watch::Receiver<Option<TranscriptEvent>>,
    pub preview_state: Arc<tokio::sync::Mutex<TranscriptPreviewState>>,
    pub app_context: AppContext,
    pub target: TargetWindowIdentity,
    pub cancellation: Arc<SessionCancellation>,
    pub deadline_cancellation: Arc<SessionCancellation>,
    pub audio_queue: Arc<AudioQueueMonitor>,
    pub started_at: Instant,
    pub config: AppConfig,
    pub asr_hints: Option<AsrSessionHints>,
}

pub(super) struct SessionResources {
    pub session_id: u64,
    pub attempt_id: String,
    pub provider_task: tauri::async_runtime::JoinHandle<()>,
    pub provider_result: oneshot::Receiver<Result<String, ProviderError>>,
    pub preview_state: Arc<tokio::sync::Mutex<TranscriptPreviewState>>,
    pub app_context: AppContext,
    pub target: TargetWindowIdentity,
    pub cancellation: Arc<SessionCancellation>,
    pub deadline_cancellation: Arc<SessionCancellation>,
    pub audio_queue: Arc<AudioQueueMonitor>,
    pub started_at: Instant,
    pub config: AppConfig,
    pub state_tx: watch::Sender<crate::state::VoiceState>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn session_cancellation_wakes_waiters_and_is_sticky() {
        let cancellation = Arc::new(SessionCancellation::default());
        let waiter_cancellation = cancellation.clone();
        let waiter = tokio::spawn(async move {
            waiter_cancellation.cancelled().await;
        });

        tokio::task::yield_now().await;
        cancellation.cancel();

        timeout(Duration::from_millis(100), waiter)
            .await
            .expect("cancellation waiter should wake")
            .expect("cancellation waiter should not panic");
        timeout(Duration::from_millis(100), cancellation.cancelled())
            .await
            .expect("late waiter should observe prior cancellation");
    }
}
