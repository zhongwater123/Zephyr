use crate::delivery::DeliveryIntent;
use crate::history::HistoryProvenance;
use crate::target::{
    PendingOutput, PendingOutputError, PendingOutputRecord, PendingOutputStore,
    TargetWindowIdentity,
};
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
}

impl Default for PendingDeliveryMetadata {
    fn default() -> Self {
        Self {
            intent: DeliveryIntent::Legacy,
            provenance: HistoryProvenance::default(),
        }
    }
}

#[derive(Default)]
struct PendingState {
    store: PendingOutputStore,
    reserved: HashSet<String>,
    metadata: HashMap<String, PendingDeliveryMetadata>,
}

#[derive(Default)]
pub struct PendingOutputService {
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
}

impl Drop for PendingOutputLease {
    fn drop(&mut self) {
        if !self.terminal {
            self.service.release(&self.id);
        }
    }
}

impl PendingOutputService {
    pub fn is_full(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .store
            .is_full()
    }

    pub fn push(
        &self,
        session_id: u64,
        text: String,
        target: TargetWindowIdentity,
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
        target: TargetWindowIdentity,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
        metadata: PendingDeliveryMetadata,
    ) -> Result<PendingOutput, PendingOutputServiceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let output = state
            .store
            .push(session_id, text, target, reason_code, reason_message)
            .map_err(|error| match error {
                PendingOutputError::Full => PendingOutputServiceError::Full,
            })?;
        state.metadata.insert(output.id.clone(), metadata);
        Ok(output)
    }

    pub fn list(&self) -> Vec<PendingOutput> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .store
            .list()
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

    fn target() -> TargetWindowIdentity {
        TargetWindowIdentity {
            hwnd: 1,
            process_id: 2,
            process_started_at: 3,
            executable_name: "editor.exe".to_string(),
            window_title: Some("Editor".to_string()),
        }
    }

    #[test]
    fn reservation_prevents_duplicate_delivery() {
        let service = PendingOutputService::default();
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
        let service = PendingOutputService::default();
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        service.reserve(&output.id).unwrap();
        service.release(&output.id);

        assert!(service.reserve(&output.id).is_ok());
    }

    #[test]
    fn committed_delivery_removes_output() {
        let service = PendingOutputService::default();
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
        let service = std::sync::Arc::new(PendingOutputService::default());
        let output = service
            .push(1, "hello".to_string(), target(), "test", "test")
            .unwrap();
        drop(service.reserve_lease(&output.id).unwrap());

        assert!(service.reserve(&output.id).is_ok());
    }

    #[test]
    fn smart_metadata_survives_pending_reservation() {
        let service = std::sync::Arc::new(PendingOutputService::default());
        let provenance = HistoryProvenance::smart_processed("office", "office-v1");
        let output = service
            .push_with_metadata(
                7,
                "first\nsecond".to_string(),
                target(),
                "target_changed",
                "target changed",
                PendingDeliveryMetadata {
                    intent: DeliveryIntent::SmartDictationAtomicPaste,
                    provenance: provenance.clone(),
                },
            )
            .unwrap();

        let lease = service.reserve_lease(&output.id).unwrap();
        assert_eq!(
            lease.metadata().intent,
            DeliveryIntent::SmartDictationAtomicPaste
        );
        assert_eq!(lease.metadata().provenance, provenance);
    }
}
