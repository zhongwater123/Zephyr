//! Authoritative lifecycle state for shortcut runtime and mutation operations.

use serde::Serialize;

use crate::physical_shortcut::ShortcutBinding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutRuntimeState {
    Active,
    Suspended,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutOperationKind {
    Capture,
    RestoreDefault,
    Undo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutOperationPhase {
    Starting,
    Capturing,
    Validating,
    Applying,
    Succeeded,
    Failed,
    Cancelled,
}

impl ShortcutOperationPhase {
    pub(crate) fn is_active(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Capturing | Self::Validating | Self::Applying
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRuntimeSnapshot {
    pub state: ShortcutRuntimeState,
    pub active_label: String,
    pub active_binding: Option<ShortcutBinding>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutOperationSnapshot {
    pub operation_id: u64,
    pub kind: ShortcutOperationKind,
    pub phase: ShortcutOperationPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_binding: Option<ShortcutBinding>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutLifecycleSnapshot {
    pub sequence: u64,
    pub config_revision: u64,
    pub runtime: ShortcutRuntimeSnapshot,
    pub operation: Option<ShortcutOperationSnapshot>,
}

#[derive(Debug)]
pub(crate) struct ShortcutLifecycleCoordinator {
    sequence: u64,
    config_revision: u64,
    enabled: bool,
    next_operation_id: u64,
    runtime: ShortcutRuntimeSnapshot,
    operation: Option<ShortcutOperationSnapshot>,
}

impl ShortcutLifecycleCoordinator {
    pub(crate) fn new(
        config_revision: u64,
        enabled: bool,
        active_label: String,
        active_binding: Option<ShortcutBinding>,
        runtime_error: Option<String>,
    ) -> Self {
        let runtime = runtime_snapshot(enabled, active_label, active_binding, runtime_error);
        Self {
            sequence: 1,
            config_revision,
            enabled,
            next_operation_id: 0,
            runtime,
            operation: None,
        }
    }

    pub(crate) fn snapshot(&self) -> ShortcutLifecycleSnapshot {
        ShortcutLifecycleSnapshot {
            sequence: self.sequence,
            config_revision: self.config_revision,
            runtime: self.runtime.clone(),
            operation: self.operation.clone(),
        }
    }

    pub(crate) fn query_snapshot(&self, operation_id: Option<u64>) -> ShortcutLifecycleSnapshot {
        let mut snapshot = self.snapshot();
        if snapshot
            .operation
            .as_ref()
            .is_some_and(|operation| !operation.phase.is_active())
            && operation_id
                != snapshot
                    .operation
                    .as_ref()
                    .map(|operation| operation.operation_id)
        {
            snapshot.operation = None;
        }
        snapshot
    }

    pub(crate) fn begin(
        &mut self,
        kind: ShortcutOperationKind,
        message: impl Into<String>,
    ) -> (u64, bool) {
        if let Some(operation) = self
            .operation
            .as_ref()
            .filter(|operation| operation.phase.is_active())
        {
            return (operation.operation_id, false);
        }
        self.next_operation_id = self.next_operation_id.saturating_add(1).max(1);
        let operation_id = self.next_operation_id;
        self.operation = Some(ShortcutOperationSnapshot {
            operation_id,
            kind,
            phase: ShortcutOperationPhase::Starting,
            candidate_label: None,
            candidate_binding: None,
            message: message.into(),
            error_code: None,
            retryable: false,
            changed: None,
        });
        self.suspend_runtime();
        self.bump();
        (operation_id, true)
    }

    pub(crate) fn transition(
        &mut self,
        operation_id: u64,
        phase: ShortcutOperationPhase,
        message: impl Into<String>,
    ) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id || !valid_transition(operation.phase, phase) {
            return false;
        }
        operation.phase = phase;
        operation.message = message.into();
        operation.error_code = None;
        operation.retryable = false;
        operation.changed = None;
        self.bump();
        true
    }

    pub(crate) fn update_candidate(
        &mut self,
        operation_id: u64,
        label: String,
        binding: Option<ShortcutBinding>,
    ) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id
            || operation.phase != ShortcutOperationPhase::Capturing
        {
            return false;
        }
        let candidate_unchanged = operation.candidate_label.as_deref() == Some(label.as_str())
            && operation.candidate_binding == binding;
        let warning_cleared = operation.error_code.is_some();
        if candidate_unchanged && !warning_cleared {
            return false;
        }
        operation.candidate_label = Some(label);
        operation.candidate_binding = binding;
        operation.message = "请按下新的物理快捷键，松开后自动保存。".into();
        operation.error_code = None;
        operation.retryable = false;
        self.bump();
        true
    }

    pub(crate) fn reject_candidate(
        &mut self,
        operation_id: u64,
        error_code: &'static str,
        message: impl Into<String>,
    ) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id
            || operation.phase != ShortcutOperationPhase::Validating
        {
            return false;
        }
        operation.phase = ShortcutOperationPhase::Capturing;
        operation.message = message.into();
        operation.error_code = Some(error_code.into());
        operation.retryable = true;
        operation.changed = None;
        self.bump();
        true
    }

    pub(crate) fn succeed(
        &mut self,
        operation_id: u64,
        config_revision: u64,
        active_label: String,
        active_binding: Option<ShortcutBinding>,
        message: impl Into<String>,
    ) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id
            || operation.phase != ShortcutOperationPhase::Applying
        {
            return false;
        }
        operation.phase = ShortcutOperationPhase::Succeeded;
        operation.message = message.into();
        operation.error_code = None;
        operation.retryable = false;
        operation.changed = Some(true);
        self.config_revision = config_revision;
        self.runtime = runtime_snapshot(self.enabled, active_label, active_binding, None);
        self.bump();
        true
    }

    pub(crate) fn succeed_unchanged(
        &mut self,
        operation_id: u64,
        message: impl Into<String>,
    ) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id
            || operation.phase != ShortcutOperationPhase::Validating
        {
            return false;
        }
        operation.phase = ShortcutOperationPhase::Succeeded;
        operation.message = message.into();
        operation.error_code = None;
        operation.retryable = false;
        operation.changed = Some(false);
        self.restore_runtime();
        self.bump();
        true
    }

    pub(crate) fn fail(
        &mut self,
        operation_id: u64,
        error_code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        runtime_error: Option<String>,
    ) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id || !operation.phase.is_active() {
            return false;
        }
        operation.phase = ShortcutOperationPhase::Failed;
        operation.message = message.into();
        operation.error_code = Some(error_code.into());
        operation.retryable = retryable;
        operation.changed = None;
        if let Some(runtime_error) = runtime_error {
            self.runtime.state = ShortcutRuntimeState::Error;
            self.runtime.message = runtime_error;
        } else {
            self.restore_runtime();
        }
        self.bump();
        true
    }

    pub(crate) fn cancel(&mut self, operation_id: u64, message: impl Into<String>) -> bool {
        let Some(operation) = self.operation.as_mut() else {
            return false;
        };
        if operation.operation_id != operation_id
            || !matches!(
                operation.phase,
                ShortcutOperationPhase::Starting | ShortcutOperationPhase::Capturing
            )
        {
            return false;
        }
        operation.phase = ShortcutOperationPhase::Cancelled;
        operation.message = message.into();
        operation.error_code = None;
        operation.retryable = false;
        operation.changed = None;
        self.restore_runtime();
        self.bump();
        true
    }

    pub(crate) fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled && self.runtime.state != ShortcutRuntimeState::Error {
            return;
        }
        self.enabled = enabled;
        if self
            .operation
            .as_ref()
            .is_some_and(|operation| operation.phase.is_active())
        {
            self.suspend_runtime();
        } else {
            self.restore_runtime();
        }
        self.bump();
    }

    pub(crate) fn sync_authoritative_config(
        &mut self,
        config_revision: u64,
        enabled: bool,
        active_label: String,
        active_binding: Option<ShortcutBinding>,
    ) {
        let changed = self.config_revision != config_revision
            || self.enabled != enabled
            || self.runtime.active_label != active_label
            || self.runtime.active_binding != active_binding;
        if !changed {
            return;
        }
        self.config_revision = config_revision;
        self.enabled = enabled;
        self.runtime.active_label = active_label;
        self.runtime.active_binding = active_binding;
        if !self
            .operation
            .as_ref()
            .is_some_and(|operation| operation.phase.is_active())
        {
            self.restore_runtime();
        }
        self.bump();
    }

    pub(crate) fn set_runtime_error(&mut self, message: String) {
        self.runtime.state = ShortcutRuntimeState::Error;
        self.runtime.message = message;
        self.bump();
    }

    pub(crate) fn restore_runtime_health(&mut self) {
        self.restore_runtime();
        self.bump();
    }

    pub(crate) fn active_operation_id(&self) -> Option<u64> {
        self.operation
            .as_ref()
            .filter(|operation| operation.phase.is_active())
            .map(|operation| operation.operation_id)
    }

    pub(crate) fn operation_kind(&self, operation_id: u64) -> Option<ShortcutOperationKind> {
        self.operation
            .as_ref()
            .filter(|operation| operation.operation_id == operation_id)
            .map(|operation| operation.kind)
    }

    pub(crate) fn operation_phase(&self, operation_id: u64) -> Option<ShortcutOperationPhase> {
        self.operation
            .as_ref()
            .filter(|operation| operation.operation_id == operation_id)
            .map(|operation| operation.phase)
    }

    fn suspend_runtime(&mut self) {
        self.runtime.state = ShortcutRuntimeState::Suspended;
        self.runtime.message = "快捷键换绑期间，原快捷键已暂时停用。".into();
    }

    fn restore_runtime(&mut self) {
        self.runtime = runtime_snapshot(
            self.enabled,
            self.runtime.active_label.clone(),
            self.runtime.active_binding.clone(),
            None,
        );
    }

    fn bump(&mut self) {
        self.sequence = self.sequence.saturating_add(1);
    }
}

fn runtime_snapshot(
    enabled: bool,
    active_label: String,
    active_binding: Option<ShortcutBinding>,
    runtime_error: Option<String>,
) -> ShortcutRuntimeSnapshot {
    if let Some(message) = runtime_error {
        ShortcutRuntimeSnapshot {
            state: ShortcutRuntimeState::Error,
            active_label,
            active_binding,
            message,
        }
    } else if active_binding.is_none() {
        ShortcutRuntimeSnapshot {
            state: ShortcutRuntimeState::Error,
            active_label,
            active_binding,
            message: "旧快捷键无法映射为物理键，请重新设置。".into(),
        }
    } else if enabled {
        ShortcutRuntimeSnapshot {
            state: ShortcutRuntimeState::Active,
            active_label,
            active_binding,
            message: "物理快捷键已启用。".into(),
        }
    } else {
        ShortcutRuntimeSnapshot {
            state: ShortcutRuntimeState::Disabled,
            active_label,
            active_binding,
            message: "语音输入已关闭，快捷键全部放行。".into(),
        }
    }
}

fn valid_transition(from: ShortcutOperationPhase, to: ShortcutOperationPhase) -> bool {
    matches!(
        (from, to),
        (
            ShortcutOperationPhase::Starting,
            ShortcutOperationPhase::Capturing
                | ShortcutOperationPhase::Validating
                | ShortcutOperationPhase::Applying
        ) | (
            ShortcutOperationPhase::Capturing,
            ShortcutOperationPhase::Validating
        ) | (
            ShortcutOperationPhase::Validating,
            ShortcutOperationPhase::Applying
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinator() -> ShortcutLifecycleCoordinator {
        ShortcutLifecycleCoordinator::new(
            7,
            true,
            "左 Ctrl+C".into(),
            Some(ShortcutBinding::default_physical()),
            None,
        )
    }

    #[test]
    fn sequence_is_monotonic_and_stale_operation_cannot_mutate_state() {
        let mut lifecycle = coordinator();
        let initial = lifecycle.snapshot().sequence;
        let (operation_id, created) = lifecycle.begin(ShortcutOperationKind::Capture, "starting");
        assert!(created);
        assert!(lifecycle.snapshot().sequence > initial);
        assert!(!lifecycle.transition(
            operation_id + 1,
            ShortcutOperationPhase::Capturing,
            "stale"
        ));
        assert!(lifecycle.transition(operation_id, ShortcutOperationPhase::Capturing, "capturing"));
        let sequence = lifecycle.snapshot().sequence;
        assert!(!lifecycle.update_candidate(operation_id + 1, "stale".into(), None));
        assert_eq!(lifecycle.snapshot().sequence, sequence);
    }

    #[test]
    fn rejected_candidate_returns_to_capture_and_next_progress_clears_warning() {
        let mut lifecycle = coordinator();
        let (operation_id, _) = lifecycle.begin(ShortcutOperationKind::Capture, "starting");
        assert!(lifecycle.transition(
            operation_id,
            ShortcutOperationPhase::Capturing,
            "capture"
        ));
        assert!(lifecycle.update_candidate(operation_id, "C".into(), None));
        assert!(lifecycle.transition(
            operation_id,
            ShortcutOperationPhase::Validating,
            "validate"
        ));
        assert!(lifecycle.reject_candidate(
            operation_id,
            "invalid_binding",
            "requires modifier"
        ));
        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.runtime.state, ShortcutRuntimeState::Suspended);
        let operation = snapshot.operation.unwrap();
        assert_eq!(operation.phase, ShortcutOperationPhase::Capturing);
        assert_eq!(operation.candidate_label.as_deref(), Some("C"));
        assert_eq!(operation.error_code.as_deref(), Some("invalid_binding"));
        assert!(operation.retryable);

        assert!(lifecycle.update_candidate(operation_id, "C".into(), None));
        let operation = lifecycle.snapshot().operation.unwrap();
        assert!(operation.error_code.is_none());
        assert_eq!(operation.message, "请按下新的物理快捷键，松开后自动保存。");
    }

    #[test]
    fn operation_failure_restores_still_valid_runtime_binding() {
        let mut lifecycle = coordinator();
        let (operation_id, _) = lifecycle.begin(ShortcutOperationKind::Capture, "starting");
        assert_eq!(
            lifecycle.snapshot().runtime.state,
            ShortcutRuntimeState::Suspended
        );
        assert!(lifecycle.transition(operation_id, ShortcutOperationPhase::Capturing, "capturing"));
        assert!(lifecycle.fail(operation_id, "persistence_failed", "failed", true, None));
        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.runtime.state, ShortcutRuntimeState::Active);
        assert_eq!(snapshot.runtime.active_label, "左 Ctrl+C");
        assert_eq!(
            snapshot.operation.unwrap().phase,
            ShortcutOperationPhase::Failed
        );
    }

    #[test]
    fn terminal_operation_cannot_regress_and_new_operation_gets_new_id() {
        let mut lifecycle = coordinator();
        let (first, _) = lifecycle.begin(ShortcutOperationKind::Capture, "starting");
        assert!(lifecycle.transition(first, ShortcutOperationPhase::Capturing, "capturing"));
        assert!(lifecycle.cancel(first, "cancelled"));
        assert!(!lifecycle.transition(first, ShortcutOperationPhase::Validating, "late"));
        let (second, created) = lifecycle.begin(ShortcutOperationKind::Capture, "retry");
        assert!(created);
        assert!(second > first);
    }

    #[test]
    fn repeated_start_returns_current_active_operation() {
        let mut lifecycle = coordinator();
        let (first, created) = lifecycle.begin(ShortcutOperationKind::Capture, "starting");
        assert!(created);
        let (second, created) = lifecycle.begin(ShortcutOperationKind::RestoreDefault, "ignored");
        assert!(!created);
        assert_eq!(first, second);
        assert_eq!(
            lifecycle.operation_kind(first),
            Some(ShortcutOperationKind::Capture)
        );
    }

    #[test]
    fn unchanged_success_restores_runtime_without_revision_change() {
        let mut lifecycle = coordinator();
        let (operation_id, _) = lifecycle.begin(ShortcutOperationKind::Capture, "starting");
        assert!(lifecycle.transition(operation_id, ShortcutOperationPhase::Capturing, "capture"));
        assert!(lifecycle.transition(operation_id, ShortcutOperationPhase::Validating, "validate"));
        assert!(lifecycle.succeed_unchanged(operation_id, "unchanged"));
        let snapshot = lifecycle.snapshot();
        assert_eq!(snapshot.config_revision, 7);
        assert_eq!(snapshot.runtime.state, ShortcutRuntimeState::Active);
        let operation = snapshot.operation.unwrap();
        assert_eq!(operation.phase, ShortcutOperationPhase::Succeeded);
        assert_eq!(operation.changed, Some(false));
    }

    #[test]
    fn validation_and_application_are_not_cancellable() {
        let mut validating = coordinator();
        let (validating_id, _) = validating.begin(ShortcutOperationKind::Capture, "starting");
        assert!(validating.transition(validating_id, ShortcutOperationPhase::Capturing, "capture"));
        assert!(validating.transition(
            validating_id,
            ShortcutOperationPhase::Validating,
            "validate"
        ));
        assert!(!validating.cancel(validating_id, "late cancel"));

        let mut applying = coordinator();
        let (applying_id, _) = applying.begin(ShortcutOperationKind::Capture, "starting");
        assert!(applying.transition(applying_id, ShortcutOperationPhase::Capturing, "capture"));
        assert!(applying.transition(applying_id, ShortcutOperationPhase::Validating, "validate"));
        assert!(applying.transition(applying_id, ShortcutOperationPhase::Applying, "apply"));
        assert!(!applying.cancel(applying_id, "late cancel"));
    }
}
