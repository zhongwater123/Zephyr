use crate::target::{
    PendingOutput, PendingOutputError, PendingOutputRecord, PendingOutputStore,
    TargetWindowIdentity,
};
use std::collections::HashSet;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingOutputServiceError {
    Full,
    NotFound,
    Busy,
}

#[derive(Default)]
struct PendingState {
    store: PendingOutputStore,
    reserved: HashSet<String>,
}

#[derive(Default)]
pub struct PendingOutputService {
    state: Mutex<PendingState>,
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
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .store
            .push(session_id, text, target, reason_code, reason_message)
            .map_err(|error| match error {
                PendingOutputError::Full => PendingOutputServiceError::Full,
            })
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
        state
            .store
            .remove(id)
            .ok_or(PendingOutputServiceError::NotFound)
    }

    pub fn discard(&self, id: &str) -> Result<PendingOutputRecord, PendingOutputServiceError> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.reserved.contains(id) {
            return Err(PendingOutputServiceError::Busy);
        }
        state
            .store
            .remove(id)
            .ok_or(PendingOutputServiceError::NotFound)
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
}
