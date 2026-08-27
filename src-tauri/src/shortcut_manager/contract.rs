use crate::physical_shortcut::ShortcutBinding;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutRuntimeState {
    Active,
    Suspended,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEditSession {
    pub edit_id: u64,
    pub trace_id: String,
    pub config_revision: u64,
    pub active_label: String,
    pub active_binding: Option<ShortcutBinding>,
    pub runtime_state: ShortcutRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEditOutcome {
    pub success: bool,
    pub edit_id: u64,
    pub trace_id: String,
    pub config_revision: u64,
    pub active_label: String,
    pub active_binding: Option<ShortcutBinding>,
    pub runtime_state: ShortcutRuntimeState,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ShortcutEditInterrupted {
    pub(super) outcome: ShortcutEditOutcome,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutTraceEvent {
    UiCaptureStarted,
    DomKeydown,
    DomKeyup,
    CandidateRejected,
    CandidateFinalized,
    BeginAcknowledged,
    CommitDispatched,
    CommitCompleted,
    OptimisticRollback,
    CancelRequested,
    FocusLost,
    EditInterrupted,
}

impl ShortcutTraceEvent {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::UiCaptureStarted => "ui_capture_started",
            Self::DomKeydown => "dom_keydown",
            Self::DomKeyup => "dom_keyup",
            Self::CandidateRejected => "candidate_rejected",
            Self::CandidateFinalized => "candidate_finalized",
            Self::BeginAcknowledged => "begin_acknowledged",
            Self::CommitDispatched => "commit_dispatched",
            Self::CommitCompleted => "commit_completed",
            Self::OptimisticRollback => "optimistic_rollback",
            Self::CancelRequested => "cancel_requested",
            Self::FocusLost => "focus_lost",
            Self::EditInterrupted => "edit_interrupted",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEditTraceInput {
    pub trace_id: String,
    pub edit_id: Option<u64>,
    pub event_seq: u32,
    pub elapsed_ms: u64,
    pub event: ShortcutTraceEvent,
    pub code: Option<String>,
    pub key: Option<String>,
    pub location: Option<u8>,
    pub repeat: Option<bool>,
    pub ctrl: Option<bool>,
    pub alt: Option<bool>,
    pub shift: Option<bool>,
    pub meta: Option<bool>,
    pub alt_graph: Option<bool>,
    #[serde(default)]
    pub held_codes: Vec<String>,
    pub candidate_label: Option<String>,
    pub candidate_binding: Option<ShortcutBinding>,
    pub reason_code: Option<String>,
}

impl ShortcutEditTraceInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.trace_id.is_empty()
            || self.trace_id.len() > 64
            || !self
                .trace_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err("快捷键 traceId 无效。".into());
        }
        for value in self
            .code
            .iter()
            .chain(self.key.iter())
            .chain(self.held_codes.iter())
        {
            if value.chars().count() > 64 {
                return Err("快捷键诊断字段过长。".into());
            }
        }
        if self.held_codes.len() > 8
            || self
                .candidate_binding
                .as_ref()
                .is_some_and(|binding| binding.modifiers.len() > 8)
            || self
                .candidate_label
                .as_ref()
                .is_some_and(|value| value.chars().count() > 128)
            || self
                .reason_code
                .as_ref()
                .is_some_and(|value| value.chars().count() > 64)
        {
            return Err("快捷键诊断载荷过大。".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_trace_is_bounded_but_preserves_raw_key_content() {
        let input = ShortcutEditTraceInput {
            trace_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            edit_id: None,
            event_seq: 2,
            elapsed_ms: 14,
            event: ShortcutTraceEvent::DomKeydown,
            code: Some("KeyK".into()),
            key: Some("k".into()),
            location: Some(0),
            repeat: Some(false),
            ctrl: Some(true),
            alt: Some(false),
            shift: Some(false),
            meta: Some(false),
            alt_graph: Some(false),
            held_codes: vec!["ControlLeft".into()],
            candidate_label: Some("左 Ctrl+K".into()),
            candidate_binding: None,
            reason_code: None,
        };
        assert!(input.validate().is_ok());
        let mut oversized = input;
        oversized.key = Some("x".repeat(65));
        assert!(oversized.validate().is_err());
    }
}
