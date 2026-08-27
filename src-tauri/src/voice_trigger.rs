use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ActivationId(String);

impl ActivationId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for ActivationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ActivationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerSource {
    Shortcut,
    UserInterface,
    ExternalAdapter(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerBehavior {
    PushToTalk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceActivation {
    pub id: ActivationId,
    pub source: TriggerSource,
    pub behavior: TriggerBehavior,
}

impl VoiceActivation {
    pub fn shortcut() -> Self {
        Self {
            id: ActivationId::new(),
            source: TriggerSource::Shortcut,
            behavior: TriggerBehavior::PushToTalk,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceCancelReason {
    TriggerInterrupted,
    UserRequested,
    Adapter(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceTriggerError {
    Busy,
    ControlPlaneUnavailable,
}

pub trait VoiceTriggerPort: Send + Sync {
    fn begin(&self, activation: VoiceActivation) -> Result<(), VoiceTriggerError>;
    fn finish(&self, activation_id: ActivationId) -> Result<(), VoiceTriggerError>;
    fn cancel(
        &self,
        activation_id: ActivationId,
        reason: VoiceCancelReason,
    ) -> Result<(), VoiceTriggerError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_ids_are_unique() {
        assert_ne!(ActivationId::new(), ActivationId::new());
    }

    #[test]
    fn shortcut_activation_has_push_to_talk_semantics() {
        let activation = VoiceActivation::shortcut();
        assert_eq!(activation.source, TriggerSource::Shortcut);
        assert_eq!(activation.behavior, TriggerBehavior::PushToTalk);
    }
}
