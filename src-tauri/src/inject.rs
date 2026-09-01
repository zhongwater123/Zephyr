use crate::target::TargetWindowIdentity;
use async_trait::async_trait;
pub use paste_protocol::{DeliveryMode, DeliveryReceipt, RestorationState, SubmissionState};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DeliveryRequest {
    pub transaction_id: Uuid,
    pub text: String,
    pub target: TargetWindowIdentity,
    pub mode: DeliveryMode,
    pub allow_unicode_fallback: bool,
}

#[derive(Debug, Error)]
pub enum InjectError {
    #[error("paste helper is unavailable: {0}")]
    HelperUnavailable(String),
    #[error("clipboard content cannot be safely snapshotted: {0}")]
    ClipboardSnapshotUnsupported(String),
    #[error("delivery target changed: {0}")]
    TargetChanged(String),
    #[error("delivery worker failed: {0}")]
    Worker(String),
}

#[async_trait]
pub trait DeliveryExecutor: Send + Sync {
    async fn deliver(&self, request: DeliveryRequest) -> Result<DeliveryReceipt, InjectError>;
}
