use crate::target::PendingOutputStore;
use crate::{ActiveSession, SessionCancellation, SessionMetrics};
use std::sync::Arc;

#[derive(Default)]
pub struct SessionCoordinator {
    pub active: Option<ActiveSession>,
    pub current_id: Option<u64>,
    pub current_cancellation: Option<Arc<SessionCancellation>>,
    pub pending_outputs: PendingOutputStore,
    pub last_metrics: Option<SessionMetrics>,
}
