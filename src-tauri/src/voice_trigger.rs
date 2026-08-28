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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeginRejection {
    Disabled,
    Busy,
    PendingFull,
    ShuttingDown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeginDecision {
    Accepted {
        session_id: u64,
        config_revision: u64,
    },
    Rejected {
        reason: BeginRejection,
    },
}

pub struct BeginReceipt {
    response: tokio::sync::oneshot::Receiver<BeginDecision>,
}

impl BeginReceipt {
    pub(crate) fn new(response: tokio::sync::oneshot::Receiver<BeginDecision>) -> Self {
        Self { response }
    }

    pub async fn wait(self) -> Result<BeginDecision, VoiceTriggerError> {
        self.response
            .await
            .map_err(|_| VoiceTriggerError::ControlPlaneUnavailable)
    }
}

pub trait VoiceTriggerPort: Send + Sync {
    fn begin(&self, activation: VoiceActivation) -> Result<BeginReceipt, VoiceTriggerError>;
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

    #[tokio::test]
    async fn begin_receipt_reports_actor_decision_without_blocking_submission() {
        let (response, result) = tokio::sync::oneshot::channel();
        let receipt = BeginReceipt::new(result);
        response
            .send(BeginDecision::Accepted {
                session_id: 4,
                config_revision: 9,
            })
            .unwrap();
        assert_eq!(
            receipt.wait().await.unwrap(),
            BeginDecision::Accepted {
                session_id: 4,
                config_revision: 9,
            }
        );
    }

    #[tokio::test]
    async fn begin_receipt_reports_closed_actor() {
        let (response, result) = tokio::sync::oneshot::channel();
        drop(response);
        assert_eq!(
            BeginReceipt::new(result).wait().await.unwrap_err(),
            VoiceTriggerError::ControlPlaneUnavailable
        );
    }
}
