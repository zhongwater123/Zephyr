use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Capture,
    Asr,
    Delivery,
    History,
    Runtime,
    Frontend,
    Vault,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Asr => "asr",
            Self::Delivery => "delivery",
            Self::History => "history",
            Self::Runtime => "runtime",
            Self::Frontend => "frontend",
            Self::Vault => "vault",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageOutcome {
    NotStarted,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    SkippedByPolicy,
    Unknown,
}

impl StageOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::SkippedByPolicy => "skipped_by_policy",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalOutcome {
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

impl TerminalOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recoverability {
    None,
    PartialText,
    FinalText,
    Audio,
    TextAndAudio,
}

impl Recoverability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::PartialText => "partial_text",
            Self::FinalText => "final_text",
            Self::Audio => "audio",
            Self::TextAndAudio => "text_and_audio",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AttemptPolicy {
    pub content_enabled: bool,
    pub save_audio: bool,
    pub save_text: bool,
    pub retention_days: u16,
    pub storage_limit_mb: u32,
    pub success_rollup_days: u16,
}

#[derive(Debug, Clone)]
pub enum IncidentEvent {
    AttemptStarted {
        attempt_id: String,
        runtime_session_id: u64,
        started_at_utc_ms: i64,
        app_version: String,
        app_name: Option<String>,
        app_title: Option<String>,
        policy: AttemptPolicy,
    },
    StageChanged {
        attempt_id: String,
        stage: Stage,
        outcome: StageOutcome,
        reason_code: Option<String>,
        monotonic_us: u64,
    },
    AudioChunk {
        attempt_id: Arc<str>,
        sequence: u64,
        bytes: Bytes,
        duration_ms: u16,
        is_final: bool,
    },
    AudioGap {
        attempt_id: Arc<str>,
    },
    PartialCheckpoint {
        attempt_id: String,
        text: String,
        confirmed_chars: usize,
        monotonic_us: u64,
    },
    FinalTranscript {
        attempt_id: String,
        text: String,
        monotonic_us: u64,
    },
    Finding {
        attempt_id: String,
        stage: Stage,
        code: String,
        message: String,
        severity: &'static str,
        recoverability: Recoverability,
    },
    Metric {
        attempt_id: String,
        name: &'static str,
        value: f64,
        unit: &'static str,
    },
    AttemptEnded {
        attempt_id: String,
        outcome: TerminalOutcome,
        history_committed: bool,
        discard_recovery_material: bool,
        ended_at_utc_ms: i64,
    },
    FrontendFailure {
        attempt_id: String,
        source: String,
        code: String,
        message: String,
        stack: Option<String>,
        occurred_at_utc_ms: i64,
    },
}

impl IncidentEvent {
    pub fn attempt_id(&self) -> &str {
        match self {
            Self::AudioChunk { attempt_id, .. } | Self::AudioGap { attempt_id } => {
                attempt_id.as_ref()
            }
            Self::AttemptStarted { attempt_id, .. }
            | Self::StageChanged { attempt_id, .. }
            | Self::PartialCheckpoint { attempt_id, .. }
            | Self::FinalTranscript { attempt_id, .. }
            | Self::Finding { attempt_id, .. }
            | Self::Metric { attempt_id, .. }
            | Self::AttemptEnded { attempt_id, .. }
            | Self::FrontendFailure { attempt_id, .. } => attempt_id,
        }
    }

    pub fn is_audio(&self) -> bool {
        matches!(self, Self::AudioChunk { .. })
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    QueueFull,
    WriterUnavailable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmitOutcome {
    Accepted,
    Disabled,
    Dropped(DropReason),
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncidentHealth {
    pub available: bool,
    pub degraded: bool,
    pub control_events_dropped: u64,
    pub audio_chunks_dropped: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncidentItem {
    pub id: String,
    pub created_at_utc_ms: i64,
    pub terminal_outcome: String,
    pub failure_stage: String,
    pub failure_code: String,
    pub failure_message: String,
    pub recoverability: String,
    pub partial_text: Option<String>,
    pub final_text: Option<String>,
    pub audio_available: bool,
    pub audio_completeness: Option<String>,
    pub pinned: bool,
    pub expires_at_utc_ms: Option<i64>,
    pub target_app: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendIncidentInput {
    pub source: String,
    pub code: String,
    pub message: String,
    pub stack: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportOptions {
    #[serde(default)]
    pub include_text: bool,
    #[serde(default)]
    pub include_audio: bool,
    #[serde(default)]
    pub include_log_excerpt: bool,
}
