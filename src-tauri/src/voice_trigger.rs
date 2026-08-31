use crate::text_processing::ActivationIntent;
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
    PressToToggle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoiceActivation {
    pub id: ActivationId,
    pub source: TriggerSource,
    pub behavior: TriggerBehavior,
    pub intent: ActivationIntent,
}

impl VoiceActivation {
    pub fn shortcut() -> Self {
        Self::shortcut_for(TriggerBehavior::PushToTalk)
    }

    pub fn shortcut_for(behavior: TriggerBehavior) -> Self {
        Self {
            id: ActivationId::new(),
            source: TriggerSource::Shortcut,
            behavior,
            intent: ActivationIntent::SmartDictation,
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

#[derive(Debug)]
pub struct ActivationCompletionReceipt {
    response: tokio::sync::oneshot::Receiver<()>,
}

impl ActivationCompletionReceipt {
    pub(crate) fn new(response: tokio::sync::oneshot::Receiver<()>) -> Self {
        Self { response }
    }

    pub async fn wait(self) {
        let _ = self.response.await;
    }
}

#[derive(Debug)]
pub struct AcceptedActivation {
    pub session_id: u64,
    pub config_revision: u64,
    pub completion: ActivationCompletionReceipt,
}

#[derive(Debug)]
pub enum BeginDecision {
    Accepted(AcceptedActivation),
    Rejected { reason: BeginRejection },
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
        assert_eq!(activation.intent, ActivationIntent::SmartDictation);
    }

    #[test]
    fn shortcut_activation_can_record_toggle_provenance() {
        let activation = VoiceActivation::shortcut_for(TriggerBehavior::PressToToggle);
        assert_eq!(activation.source, TriggerSource::Shortcut);
        assert_eq!(activation.behavior, TriggerBehavior::PressToToggle);
    }

    #[tokio::test]
    async fn begin_receipt_reports_actor_decision_without_blocking_submission() {
        let (response, result) = tokio::sync::oneshot::channel();
        let (completion, completion_result) = tokio::sync::oneshot::channel();
        let receipt = BeginReceipt::new(result);
        response
            .send(BeginDecision::Accepted(AcceptedActivation {
                session_id: 4,
                config_revision: 9,
                completion: ActivationCompletionReceipt::new(completion_result),
            }))
            .unwrap();
        let BeginDecision::Accepted(accepted) = receipt.wait().await.unwrap() else {
            panic!("expected accepted begin");
        };
        assert_eq!(accepted.session_id, 4);
        assert_eq!(accepted.config_revision, 9);
        completion.send(()).unwrap();
        accepted.completion.wait().await;
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
