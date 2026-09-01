use crate::delivery::DeliveryIntent;
use crate::history::HistoryProvenance;
use crate::target::{
    DeliveryCertainty, PendingOutput, PendingOutputDraft, PendingOutputError, PendingOutputRecord,
    PendingOutputStore,
};
use crate::target_port::{CapturedTarget, TargetPort};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOutputServiceError {
    Full,
    NotFound,
    Busy,
}

#[derive(Clone, Debug)]
pub struct PendingDeliveryMetadata {
    pub intent: DeliveryIntent,
    pub provenance: HistoryProvenance,
    pub certainty: DeliveryCertainty,
}

impl Default for PendingDeliveryMetadata {
    fn default() -> Self {
        Self {
            intent: DeliveryIntent::Legacy,
            provenance: HistoryProvenance::default(),
            certainty: DeliveryCertainty::Retryable,
        }
    }
}

#[derive(Default)]
struct PendingState {
    store: PendingOutputStore,
    reserved: HashSet<String>,
    metadata: HashMap<String, PendingDeliveryMetadata>,
}

pub struct PendingOutputService {
    targets: std::sync::Arc<dyn TargetPort>,
    state: Mutex<PendingState>,
}

pub struct PendingOutputLease {
    service: std::sync::Arc<PendingOutputService>,
    id: String,
    record: PendingOutputRecord,
    terminal: bool,
    metadata: PendingDeliveryMetadata,
}

impl PendingOutputLease {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn record(&self) -> &PendingOutputRecord {
        &self.record
    }

    pub fn metadata(&self) -> &PendingDeliveryMetadata {
        &self.metadata
    }

    pub fn complete(mut self) -> Result<PendingOutputRecord, PendingOutputServiceError> {
        let completed = self.service.complete(&self.id)?;
        self.terminal = true;
        Ok(completed)
    }

    pub fn retain(
        mut self,
        certainty: DeliveryCertainty,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
    ) -> Result<(), PendingOutputServiceError> {
        self.service
            .update_delivery_failure(&self.id, certainty, reason_code, reason_message)?;
        self.service.release(&self.id);
        self.terminal = true;
        Ok(())
    }
}

impl Drop for PendingOutputLease {
    fn drop(&mut self) {
        if !self.terminal {
            self.service.release(&self.id);
        }
    }
}

impl PendingOutputService {
    pub fn new(targets: std::sync::Arc<dyn TargetPort>) -> Self {
        Self {
            targets,
            state: Mutex::new(PendingState::default()),
        }
    }

    pub fn is_full(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .store
            .is_full()
    }

    #[allow(dead_code)]
    pub fn push(
        &self,
        session_id: u64,
        text: String,
        target: CapturedTarget,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
    ) -> Result<PendingOutput, PendingOutputServiceError> {
        self.push_with_metadata(
            session_id,
            text,
            target,
            reason_code,
            reason_message,
            PendingDeliveryMetadata::default(),
        )
    }

    pub fn push_with_metadata(
        &self,
        session_id: u64,
        text: String,
        target: CapturedTarget,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
        metadata: PendingDeliveryMetadata,
    ) -> Result<PendingOutput, PendingOutputServiceError> {
        let target_available = self.targets.exists(&target).is_ok();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let output = state
            .store
            .push(PendingOutputDraft {
                session_id,
                text,
                target,
                target_available,
                reason_code: reason_code.into(),
                reason_message: reason_message.into(),
                certainty: metadata.certainty,
            })
            .map_err(|error| match error {
                PendingOutputError::Full => PendingOutputServiceError::Full,
            })?;
        state.metadata.insert(output.id.clone(), metadata);
        Ok(output)
    }

    pub fn list(&self) -> Vec<PendingOutput> {
        let targets = self.targets.clone();
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .store
            .list(|target| targets.exists(target).is_ok())
    }

    pub fn reserve(&self, id: &str) -> Result<PendingOutputRecord, PendingOutputServiceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.reserved.contains(id) {
            return Err(PendingOutputServiceError::Busy);
        }
        let record = state
            .store
            .get(id)
            .ok_or(PendingOutputServiceError::NotFound)?;
        state.reserved.insert(id.to_string());
        Ok(record)
    }

    pub fn reserve_lease(
        self: &std::sync::Arc<Self>,
        id: &str,
    ) -> Result<PendingOutputLease, PendingOutputServiceError> {
        let record = self.reserve(id)?;
        let metadata = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .metadata
            .get(id)
            .cloned()
            .unwrap_or_default();
        Ok(PendingOutputLease {
            service: self.clone(),
            id: id.to_string(),
            record,
            metadata,
            terminal: false,
        })
    }

    pub fn release(&self, id: &str) {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reserved
            .remove(id);
    }

    fn update_delivery_failure(
        &self,
        id: &str,
        certainty: DeliveryCertainty,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
    ) -> Result<(), PendingOutputServiceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.reserved.contains(id) {
            return Err(PendingOutputServiceError::NotFound);
        }
        state
            .store
            .update_delivery_failure(id, certainty, reason_code, reason_message)
            .then_some(())
            .ok_or(PendingOutputServiceError::NotFound)
    }

    pub fn complete(&self, id: &str) -> Result<PendingOutputRecord, PendingOutputServiceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.reserved.remove(id) {
            return Err(PendingOutputServiceError::NotFound);
        }
        let record = state
            .store
            .remove(id)
            .ok_or(PendingOutputServiceError::NotFound)?;
        state.metadata.remove(id);
        Ok(record)
    }

    pub fn discard(&self, id: &str) -> Result<PendingOutputRecord, PendingOutputServiceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.reserved.contains(id) {
            return Err(PendingOutputServiceError::Busy);
        }
        let record = state
            .store
            .remove(id)
            .ok_or(PendingOutputServiceError::NotFound)?;
        state.metadata.remove(id);
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_port::tests::{fake_target, FakeTargetPort};

    fn target() -> CapturedTarget {
        fake_target("editor.exe")
    }

    fn service() -> PendingOutputService {
        PendingOutputService::new(std::sync::Arc::new(FakeTargetPort::available()))
    }

    #[test]
    fn reservation_prevents_duplicate_delivery() {
        let service = service();
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();

        assert!(service.reserve(&output.id).is_ok());
        assert_eq!(
            service.reserve(&output.id).unwrap_err(),
            PendingOutputServiceError::Busy
        );
        assert_eq!(
            service.discard(&output.id).unwrap_err(),
            PendingOutputServiceError::Busy
        );
    }

    #[test]
    fn failed_delivery_release_keeps_output() {
        let service = service();
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        service.reserve(&output.id).unwrap();
        service.release(&output.id);

        assert!(service.reserve(&output.id).is_ok());
    }

    #[test]
    fn committed_delivery_removes_output() {
        let service = service();
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        service.reserve(&output.id).unwrap();
        service.complete(&output.id).unwrap();

        assert_eq!(
            service.reserve(&output.id).unwrap_err(),
            PendingOutputServiceError::NotFound
        );
    }

    #[test]
    fn dropped_lease_releases_reservation() {
        let service = std::sync::Arc::new(service());
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        drop(service.reserve_lease(&output.id).unwrap());

        assert!(service.reserve(&output.id).is_ok());
    }

    #[test]
    fn smart_metadata_survives_pending_reservation() {
        let service = std::sync::Arc::new(service());
        let provenance = HistoryProvenance::smart_processed("office", "office-v1");
        let output = service
            .push_with_metadata(
                7,
                "first\nsecond".to_string(),
                target(),
                "target_changed",
                "target changed",
                PendingDeliveryMetadata {
                    intent: DeliveryIntent::SmartDictation,
                    provenance: provenance.clone(),
                    certainty: DeliveryCertainty::Retryable,
                },
            )
            .unwrap();

        let lease = service.reserve_lease(&output.id).unwrap();
        assert_eq!(lease.metadata().intent, DeliveryIntent::SmartDictation);
        assert_eq!(lease.metadata().provenance, provenance);
    }

    #[test]
    fn uncertain_retention_updates_the_public_retry_boundary() {
        let service = std::sync::Arc::new(service());
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        service
            .reserve_lease(&output.id)
            .unwrap()
            .retain(
                DeliveryCertainty::MayHaveBeenSubmitted,
                "delivery_submission_unknown",
                "可能已经输入",
            )
            .unwrap();

        let retained = service.list().into_iter().next().unwrap();
        assert_eq!(
            retained.delivery_certainty,
            DeliveryCertainty::MayHaveBeenSubmitted
        );
        assert_eq!(retained.reason_code, "delivery_submission_unknown");
        assert!(service.reserve(&output.id).is_ok());
    }

    #[test]
    fn pending_availability_is_always_resolved_through_the_target_port() {
        let targets = std::sync::Arc::new(FakeTargetPort::failing_exists("closed"));
        let service = PendingOutputService::new(targets.clone());
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        assert!(!output.target_available);
        assert!(!service.list()[0].target_available);
        assert_eq!(targets.calls(), vec!["exists", "exists"]);
    }
}
