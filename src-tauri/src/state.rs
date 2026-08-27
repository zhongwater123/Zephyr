use serde::Serialize;
use std::time::Duration;

pub const MIN_RECORDING_MS: u128 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum VoiceState {
    Idle,
    Recording,
    Transcribing,
    Pasting,
    Disabled,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceStatePayload {
    pub state: VoiceState,
    pub message: String,
    pub elapsed_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseDecision {
    Cancelled {
        elapsed_ms: u128,
        payload: VoiceStatePayload,
    },
    Transcribe {
        elapsed_ms: u128,
        payload: VoiceStatePayload,
    },
}

#[derive(Debug, Clone)]
pub struct AppStateMachine {
    enabled: bool,
    state: VoiceState,
}

impl Default for AppStateMachine {
    fn default() -> Self {
        Self {
            enabled: true,
            state: VoiceState::Idle,
        }
    }
}

impl AppStateMachine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> &VoiceState {
        &self.state
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) -> VoiceStatePayload {
        self.enabled = enabled;
        self.state = if enabled {
            VoiceState::Idle
        } else {
            VoiceState::Disabled
        };
        self.payload(if enabled { "准备就绪" } else { "已暂停" }, None)
    }

    pub fn activation_started(&mut self) -> Option<VoiceStatePayload> {
        if !self.enabled || self.state == VoiceState::Recording {
            return None;
        }
        if self.state != VoiceState::Idle {
            return None;
        }
        self.state = VoiceState::Recording;
        Some(self.payload("正在听", None))
    }

    pub fn activation_finished(&mut self, duration: Duration) -> ReleaseDecision {
        let elapsed_ms = duration.as_millis();
        if elapsed_ms < MIN_RECORDING_MS {
            self.state = if self.enabled {
                VoiceState::Idle
            } else {
                VoiceState::Disabled
            };
            return ReleaseDecision::Cancelled {
                elapsed_ms,
                payload: self.payload("准备就绪", Some(elapsed_ms)),
            };
        }
        self.state = VoiceState::Transcribing;
        ReleaseDecision::Transcribe {
            elapsed_ms,
            payload: self.payload("识别中", Some(elapsed_ms)),
        }
    }

    pub fn paste_started(&mut self) -> VoiceStatePayload {
        self.state = VoiceState::Pasting;
        self.payload("正在输入", None)
    }

    pub fn complete(&mut self) -> VoiceStatePayload {
        self.state = if self.enabled {
            VoiceState::Idle
        } else {
            VoiceState::Disabled
        };
        self.payload(
            if self.enabled {
                "准备就绪"
            } else {
                "已暂停"
            },
            None,
        )
    }

    pub fn fail(&mut self, message: impl Into<String>) -> VoiceStatePayload {
        self.state = VoiceState::Error;
        self.payload(message, None)
    }

    fn payload(&self, message: impl Into<String>, elapsed_ms: Option<u128>) -> VoiceStatePayload {
        VoiceStatePayload {
            state: self.state.clone(),
            message: message.into(),
            elapsed_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_press_does_not_restart_recording() {
        let mut machine = AppStateMachine::new();

        let first = machine.activation_started();
        let second = machine.activation_started();

        assert_eq!(first.unwrap().state, VoiceState::Recording);
        assert!(second.is_none());
        assert_eq!(machine.state(), &VoiceState::Recording);
    }

    #[test]
    fn release_after_minimum_duration_enters_transcribing() {
        let mut machine = AppStateMachine::new();
        machine.activation_started();

        let decision = machine.activation_finished(Duration::from_millis(450));

        match decision {
            ReleaseDecision::Transcribe {
                elapsed_ms,
                payload,
            } => {
                assert_eq!(elapsed_ms, 450);
                assert_eq!(payload.state, VoiceState::Transcribing);
                assert_eq!(payload.elapsed_ms, Some(450));
            }
            ReleaseDecision::Cancelled { .. } => panic!("expected transcribe decision"),
        }
    }

    #[test]
    fn short_recording_is_cancelled_without_error() {
        let mut machine = AppStateMachine::new();
        machine.activation_started();

        let decision = machine.activation_finished(Duration::from_millis(120));

        match decision {
            ReleaseDecision::Cancelled {
                elapsed_ms,
                payload,
            } => {
                assert_eq!(elapsed_ms, 120);
                assert_eq!(payload.state, VoiceState::Idle);
                assert_eq!(payload.message, "准备就绪");
            }
            ReleaseDecision::Transcribe { .. } => panic!("expected cancelled decision"),
        }
    }

    #[test]
    fn failure_can_return_to_idle() {
        let mut machine = AppStateMachine::new();

        let error = machine.fail("网络错误");
        let complete = machine.complete();

        assert_eq!(error.state, VoiceState::Error);
        assert_eq!(complete.state, VoiceState::Idle);
    }
}
