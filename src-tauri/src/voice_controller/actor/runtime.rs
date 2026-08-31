use super::super::contract::{VoiceAvailability, VoiceStatusSnapshot};
use super::super::resources::SessionMetrics;
use crate::state::{AppStateMachine, VoiceState, VoiceStatePayload};
use crate::voice_trigger::{ActivationId, VoiceActivation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum VoicePhase {
    Idle,
    Starting,
    Recording,
    Stopping,
    Transcribing,
    Pasting,
    Error,
    ShuttingDown,
}

pub(super) struct CurrentSession {
    pub(super) session_id: u64,
    pub(super) activation: VoiceActivation,
    pub(super) config_revision: u64,
}

pub(super) struct VoiceRuntime {
    pub(super) machine: AppStateMachine,
    payload: VoiceStatePayload,
    pub(super) desired_enabled: bool,
    pub(super) desired_revision: u64,
    pub(super) availability: VoiceAvailability,
    pub(super) phase: VoicePhase,
    pub(super) current: Option<CurrentSession>,
    pub(super) last_metrics: Option<SessionMetrics>,
    pub(super) shortcut_registration_error: Option<String>,
}

impl VoiceRuntime {
    pub(super) fn new(enabled: bool, revision: u64) -> Self {
        let mut machine = AppStateMachine::new();
        let payload = machine.set_enabled(enabled);
        Self {
            machine,
            payload,
            desired_enabled: enabled,
            desired_revision: revision,
            availability: if enabled {
                VoiceAvailability::Available
            } else {
                VoiceAvailability::Disabled
            },
            phase: VoicePhase::Idle,
            current: None,
            last_metrics: None,
            shortcut_registration_error: None,
        }
    }

    pub(super) fn current_id(&self) -> Option<u64> {
        self.current.as_ref().map(|session| session.session_id)
    }

    pub(super) fn owns_activation(&self, activation_id: &ActivationId) -> bool {
        self.current.as_ref().map(|session| &session.activation.id) == Some(activation_id)
    }

    pub(super) fn accepts_begin(&self) -> bool {
        self.desired_enabled
            && self.availability == VoiceAvailability::Available
            && self.phase == VoicePhase::Idle
            && self.current.is_none()
    }

    pub(super) fn set_payload(&mut self, payload: VoiceStatePayload) -> VoiceStatePayload {
        self.payload = payload.clone();
        payload
    }

    pub(super) fn snapshot(&self) -> VoiceStatusSnapshot {
        let mut payload = self.payload.clone();
        if payload.state == VoiceState::Idle {
            if let Some(error) = &self.shortcut_registration_error {
                payload.message = error.clone();
            }
        }
        VoiceStatusSnapshot {
            payload,
            session_active: self.current.is_some(),
            desired_enabled: self.desired_enabled,
            desired_revision: self.desired_revision,
            availability: self.availability.clone(),
            shortcut_error: self.shortcut_registration_error.clone(),
        }
    }

    pub(super) fn replace_payload(&mut self, payload: VoiceStatePayload) {
        self.payload = payload;
    }

    pub(super) fn begin(
        &mut self,
        session_id: u64,
        activation: VoiceActivation,
        config_revision: u64,
    ) -> Option<VoiceStatePayload> {
        if !self.accepts_begin() {
            return None;
        }
        let payload = self.machine.activation_started()?;
        self.current = Some(CurrentSession {
            session_id,
            activation,
            config_revision,
        });
        self.phase = VoicePhase::Starting;
        let mut payload = payload;
        payload.message = "正在启动".to_string();
        Some(self.set_payload(payload))
    }

    pub(super) fn mark_recording(&mut self, session_id: u64) -> Option<()> {
        if self.current_id() != Some(session_id) || self.phase != VoicePhase::Starting {
            return None;
        }
        let message = match self.current.as_ref()?.activation.behavior {
            crate::voice_trigger::TriggerBehavior::PushToTalk => "正在聆听，松开结束",
            crate::voice_trigger::TriggerBehavior::PressToToggle => "正在聆听，再按一次结束",
        };
        self.phase = VoicePhase::Recording;
        let payload = self.machine.activation_ready(message)?;
        self.set_payload(payload);
        Some(())
    }

    pub(super) fn set_desired(&mut self, enabled: bool, revision: u64) -> VoiceStatePayload {
        self.desired_enabled = enabled;
        self.desired_revision = revision;
        self.availability = if enabled {
            VoiceAvailability::Available
        } else {
            VoiceAvailability::Disabled
        };
        let machine_payload = self.machine.set_enabled(enabled);
        let payload = if enabled && self.current.is_some() && self.phase == VoicePhase::Pasting {
            VoiceStatePayload {
                state: VoiceState::Pasting,
                message: "正在写入".to_string(),
                elapsed_ms: None,
            }
        } else {
            machine_payload
        };
        if self.current.is_none() {
            self.phase = VoicePhase::Idle;
        }
        self.set_payload(payload)
    }

    pub(super) fn mark_shutting_down(&mut self) -> VoiceStatePayload {
        self.availability = VoiceAvailability::ShuttingDown;
        self.phase = VoicePhase::ShuttingDown;
        let payload = self.machine.set_enabled(false);
        self.set_payload(payload)
    }

    pub(super) fn clear_current(&mut self, session_id: u64) {
        if self.current_id() == Some(session_id) {
            self.current = None;
            if self.phase != VoicePhase::ShuttingDown {
                self.phase = VoicePhase::Idle;
            }
        }
    }

    pub(super) fn record_outcome(
        &mut self,
        session_id: u64,
        final_state: &str,
        reason: Option<&str>,
    ) {
        if let Some(metrics) = &mut self.last_metrics {
            if metrics.session_id == session_id {
                metrics.final_state = final_state.to_string();
                metrics.cancel_reason = reason.map(str::to_string);
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
