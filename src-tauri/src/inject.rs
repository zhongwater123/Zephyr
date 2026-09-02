use crate::target_port::CapturedTarget;
use async_trait::async_trait;
pub use paste_protocol::{DeliveryMode, DeliveryReceipt, RestorationState, SubmissionState};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug)]
// Automatic delivery is intentionally unsupported by the macOS bootstrap. The
// shared request remains intact for Windows without manufacturing macOS reads.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub struct DeliveryRequest {
    pub transaction_id: Uuid,
    pub text: String,
    pub target: CapturedTarget,
    pub mode: DeliveryMode,
    pub allow_unicode_fallback: bool,
}

#[derive(Debug, Error)]
pub enum InjectError {
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("paste helper is unavailable: {0}")]
    HelperUnavailable(String),
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("clipboard content cannot be safely snapshotted: {0}")]
    ClipboardSnapshotUnsupported(String),
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("delivery target changed: {0}")]
    TargetChanged(String),
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    #[error("unsupported desktop capability {capability}: {message}")]
    UnsupportedCapability {
        capability: &'static str,
        message: String,
    },
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    #[error("delivery worker failed: {0}")]
    Worker(String),
}

#[async_trait]
pub trait DeliveryExecutor: Send + Sync {
    async fn deliver(&self, request: DeliveryRequest) -> Result<DeliveryReceipt, InjectError>;
}
