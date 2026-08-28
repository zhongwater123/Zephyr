mod finalize;
mod pending;
mod start;

pub(super) use finalize::spawn_finalization;
pub(super) use pending::spawn_pending_delivery;
pub(super) use start::spawn_start;

#[cfg(test)]
mod tests {
    use super::super::contract::wait_for_recording_deadline;
    use super::super::resources::SessionCancellation;
    use std::sync::Arc;
    use tokio::time::Duration;

    #[tokio::test]
    async fn cancelled_recording_deadline_does_not_fire() {
        let cancellation = Arc::new(SessionCancellation::default());
        cancellation.cancel();
        assert!(!wait_for_recording_deadline(cancellation, Duration::from_secs(120)).await);
    }

    #[tokio::test]
    async fn active_recording_deadline_fires_when_elapsed() {
        let cancellation = Arc::new(SessionCancellation::default());
        assert!(wait_for_recording_deadline(cancellation, Duration::ZERO).await);
    }
}
