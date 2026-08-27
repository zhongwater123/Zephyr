use super::super::contract::{ShortcutEditOutcome, ShortcutEditSession, ShortcutRuntimeState};
use super::super::ports::{
    ShortcutConfigPort, ShortcutObserverPort, ShortcutRuntimePort, ShortcutStoreFailure,
};
use super::{
    EditCoordinator, HOOK_UNAVAILABLE, PERSISTENCE_FAILED, REVISION_CONFLICT,
    RUNTIME_ROLLBACK_FAILED,
};
use crate::config::AppConfig;
use crate::physical_shortcut::ShortcutBinding;
use crate::physical_shortcut::{ModifierBinding, ModifierKind, ModifierSide, PhysicalKeyId};
use crate::windows_keyboard::KeyboardEngineDiagnostics;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct FakeConfig {
    current: Mutex<AppConfig>,
    failure: Mutex<Option<ShortcutStoreFailure>>,
    commits: AtomicUsize,
}

impl FakeConfig {
    fn new(current: AppConfig) -> Self {
        Self {
            current: Mutex::new(current),
            failure: Mutex::new(None),
            commits: AtomicUsize::new(0),
        }
    }

    fn fail_storage(&self) {
        *self.failure.lock().unwrap() = Some(ShortcutStoreFailure::Storage(
            "simulated storage failure".into(),
        ));
    }

    fn replace(&self, next: AppConfig) {
        *self.current.lock().unwrap() = next;
    }
}

impl ShortcutConfigPort for FakeConfig {
    fn snapshot(&self) -> AppConfig {
        self.current.lock().unwrap().clone()
    }

    fn commit_shortcut(
        &self,
        expected_revision: u64,
        next: AppConfig,
    ) -> Result<AppConfig, ShortcutStoreFailure> {
        self.commits.fetch_add(1, Ordering::Relaxed);
        if let Some(failure) = self.failure.lock().unwrap().take() {
            return Err(failure);
        }
        let mut current = self.current.lock().unwrap();
        if current.revision != expected_revision {
            return Err(ShortcutStoreFailure::Conflict);
        }
        *current = next.clone();
        Ok(next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeOperation {
    SetBinding(Option<ShortcutBinding>),
    SetEnabled(bool),
    EnsureReady { force_reinstall: bool },
    Shutdown,
}

struct FakeRuntime {
    binding: Mutex<Option<ShortcutBinding>>,
    enabled: AtomicBool,
    operations: Mutex<Vec<RuntimeOperation>>,
    fail_bindings: Mutex<Vec<ShortcutBinding>>,
}

impl FakeRuntime {
    fn new(binding: Option<ShortcutBinding>, enabled: bool) -> Self {
        Self {
            binding: Mutex::new(binding),
            enabled: AtomicBool::new(enabled),
            operations: Mutex::new(Vec::new()),
            fail_bindings: Mutex::new(Vec::new()),
        }
    }

    fn fail_binding(&self, binding: ShortcutBinding) {
        self.fail_bindings.lock().unwrap().push(binding);
    }

    fn replace_runtime_binding(&self, binding: Option<ShortcutBinding>) {
        *self.binding.lock().unwrap() = binding;
    }

    fn operations(&self) -> Vec<RuntimeOperation> {
        self.operations.lock().unwrap().clone()
    }
}

impl ShortcutRuntimePort for FakeRuntime {
    fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
        let binding = binding.cloned();
        self.operations
            .lock()
            .unwrap()
            .push(RuntimeOperation::SetBinding(binding.clone()));
        if let Some(candidate) = binding.as_ref() {
            let mut failures = self.fail_bindings.lock().unwrap();
            if let Some(index) = failures.iter().position(|failure| failure == candidate) {
                failures.remove(index);
                return Err(format!(
                    "simulated binding failure for {}",
                    candidate.display_label()
                ));
            }
        }
        *self.binding.lock().unwrap() = binding;
        Ok(())
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        self.operations
            .lock()
            .unwrap()
            .push(RuntimeOperation::SetEnabled(enabled));
    }

    fn ensure_runtime_ready(
        &self,
        force_reinstall: bool,
    ) -> Result<u64, crate::windows_keyboard::KeyboardEngineError> {
        self.operations
            .lock()
            .unwrap()
            .push(RuntimeOperation::EnsureReady { force_reinstall });
        Ok(1)
    }

    fn is_healthy(&self) -> bool {
        true
    }

    fn diagnostics(&self) -> KeyboardEngineDiagnostics {
        KeyboardEngineDiagnostics {
            hook_generation: 1,
            observed_events: 0,
            emitted_events: 0,
            dropped_events: 0,
            hook_healthy: true,
            hook_worker_alive: true,
            dispatch_alive: true,
            enabled: self.enabled.load(Ordering::Relaxed),
        }
    }

    fn shutdown(&self) {
        self.operations
            .lock()
            .unwrap()
            .push(RuntimeOperation::Shutdown);
    }
}

#[derive(Default)]
struct FakeObserver {
    runtime_errors: Mutex<Vec<Option<String>>>,
    interruptions: Mutex<Vec<ShortcutEditOutcome>>,
}

impl ShortcutObserverPort for FakeObserver {
    fn publish_runtime_error(&self, message: Option<String>) {
        self.runtime_errors.lock().unwrap().push(message);
    }

    fn emit_interrupted(&self, outcome: ShortcutEditOutcome) {
        self.interruptions.lock().unwrap().push(outcome);
    }
}

fn binding(modifier: ModifierKind, trigger: u16) -> ShortcutBinding {
    ShortcutBinding {
        modifiers: vec![ModifierBinding {
            kind: modifier,
            side: ModifierSide::Left,
        }],
        trigger: PhysicalKeyId::new(trigger, false),
    }
}

fn configured(enabled: bool) -> AppConfig {
    AppConfig {
        revision: 7,
        enabled,
        ..AppConfig::default()
    }
}

fn harness(
    config: AppConfig,
) -> (
    EditCoordinator,
    Arc<FakeConfig>,
    Arc<FakeRuntime>,
    Arc<FakeObserver>,
) {
    let store = Arc::new(FakeConfig::new(config.clone()));
    let runtime = Arc::new(FakeRuntime::new(
        config.shortcut_binding.clone(),
        config.enabled,
    ));
    let observer = Arc::new(FakeObserver::default());
    let coordinator =
        EditCoordinator::new(store.clone(), Some(runtime.clone()), None, observer.clone());
    (coordinator, store, runtime, observer)
}

fn begin(coordinator: &EditCoordinator) -> ShortcutEditSession {
    coordinator.begin_edit("trace-1".into(), 7).unwrap()
}

#[test]
fn runtime_apply_failure_never_commits_configuration() {
    let initial = configured(true);
    let old_binding = initial.shortcut_binding.clone().unwrap();
    let (coordinator, store, runtime, _) = harness(initial);
    let candidate = binding(ModifierKind::Control, 0x2e);
    runtime.fail_binding(candidate.clone());
    runtime.replace_runtime_binding(None);

    let edit = begin(&coordinator);
    let outcome = coordinator
        .commit_edit("trace-1".into(), edit.edit_id, 7, candidate.clone())
        .unwrap();

    assert!(!outcome.success);
    assert_eq!(outcome.error_code.as_deref(), Some(HOOK_UNAVAILABLE));
    assert_eq!(store.commits.load(Ordering::Relaxed), 0);
    assert_eq!(*runtime.binding.lock().unwrap(), Some(old_binding.clone()));
    assert_eq!(
        runtime.operations(),
        vec![
            RuntimeOperation::SetEnabled(false),
            RuntimeOperation::EnsureReady {
                force_reinstall: true,
            },
            RuntimeOperation::SetEnabled(false),
            RuntimeOperation::SetBinding(Some(candidate)),
            RuntimeOperation::SetEnabled(false),
            RuntimeOperation::SetBinding(Some(old_binding)),
            RuntimeOperation::EnsureReady {
                force_reinstall: false,
            },
            RuntimeOperation::SetEnabled(true),
        ]
    );
    assert!(runtime.enabled.load(Ordering::Relaxed));
}

#[test]
fn persistence_failure_restores_authoritative_runtime_binding() {
    let initial = configured(true);
    let old_binding = initial.shortcut_binding.clone().unwrap();
    let (coordinator, store, runtime, _) = harness(initial);
    store.fail_storage();
    let candidate = binding(ModifierKind::Control, 0x2e);

    let edit = begin(&coordinator);
    let outcome = coordinator
        .commit_edit("trace-1".into(), edit.edit_id, 7, candidate.clone())
        .unwrap();

    assert!(!outcome.success);
    assert_eq!(outcome.error_code.as_deref(), Some(PERSISTENCE_FAILED));
    assert_eq!(*runtime.binding.lock().unwrap(), Some(old_binding.clone()));
    assert!(runtime.enabled.load(Ordering::Relaxed));
    let operations = runtime.operations();
    let candidate_apply = operations
        .iter()
        .position(|operation| operation == &RuntimeOperation::SetBinding(Some(candidate.clone())))
        .unwrap();
    let authoritative_restore = operations
        .iter()
        .rposition(|operation| {
            operation == &RuntimeOperation::SetBinding(Some(old_binding.clone()))
        })
        .unwrap();
    assert!(candidate_apply < authoritative_restore);
}

#[test]
fn rollback_failure_reports_runtime_rollback_error() {
    let initial = configured(true);
    let old_binding = initial.shortcut_binding.clone().unwrap();
    let (coordinator, store, runtime, observer) = harness(initial);
    store.fail_storage();
    runtime.fail_binding(old_binding.clone());
    let candidate = binding(ModifierKind::Control, 0x2e);

    let edit = begin(&coordinator);
    let outcome = coordinator
        .commit_edit("trace-1".into(), edit.edit_id, 7, candidate.clone())
        .unwrap();

    assert!(!outcome.success);
    assert_eq!(outcome.error_code.as_deref(), Some(RUNTIME_ROLLBACK_FAILED));
    assert!(observer
        .runtime_errors
        .lock()
        .unwrap()
        .last()
        .is_some_and(Option::is_some));
    assert_eq!(*runtime.binding.lock().unwrap(), Some(candidate.clone()));
    assert!(!runtime.enabled.load(Ordering::Relaxed));
    assert!(runtime
        .operations()
        .contains(&RuntimeOperation::SetBinding(Some(old_binding))));
}

#[test]
fn revision_conflict_restores_the_new_authoritative_binding() {
    let initial = configured(true);
    let (coordinator, store, runtime, _) = harness(initial.clone());
    let edit = begin(&coordinator);
    let authoritative = binding(ModifierKind::Alt, 0x1e);
    store.replace(AppConfig {
        revision: 8,
        shortcut: authoritative.display_label(),
        shortcut_binding: Some(authoritative.clone()),
        ..initial
    });

    let outcome = coordinator
        .commit_edit(
            "trace-1".into(),
            edit.edit_id,
            7,
            binding(ModifierKind::Control, 0x2e),
        )
        .unwrap();

    assert_eq!(outcome.error_code.as_deref(), Some(REVISION_CONFLICT));
    assert_eq!(*runtime.binding.lock().unwrap(), Some(authoritative));
    assert_eq!(store.commits.load(Ordering::Relaxed), 0);
}

#[test]
fn successful_commit_keeps_runtime_and_configuration_consistent() {
    let initial = configured(true);
    let (coordinator, store, runtime, _) = harness(initial);
    let candidate = binding(ModifierKind::Control, 0x2e);
    let edit = begin(&coordinator);

    let outcome = coordinator
        .commit_edit("trace-1".into(), edit.edit_id, 7, candidate.clone())
        .unwrap();

    assert!(outcome.success);
    assert!(outcome.changed);
    assert_eq!(outcome.config_revision, 8);
    assert_eq!(store.snapshot().shortcut_binding, Some(candidate.clone()));
    assert_eq!(*runtime.binding.lock().unwrap(), Some(candidate));
    assert!(runtime.enabled.load(Ordering::Relaxed));
}

#[test]
fn disabled_commit_persists_without_enabling_or_reinstalling_hook() {
    let initial = configured(false);
    let (coordinator, store, runtime, _) = harness(initial);
    let edit = begin(&coordinator);

    let outcome = coordinator
        .commit_edit(
            "trace-1".into(),
            edit.edit_id,
            7,
            binding(ModifierKind::Control, 0x2e),
        )
        .unwrap();

    assert!(outcome.success);
    assert_eq!(outcome.runtime_state, ShortcutRuntimeState::Disabled);
    assert_eq!(outcome.message, "快捷键已保存，开启后生效。");
    assert_eq!(store.commits.load(Ordering::Relaxed), 1);
    assert!(!runtime.enabled.load(Ordering::Relaxed));
    assert!(runtime
        .operations()
        .iter()
        .all(|operation| !matches!(operation, RuntimeOperation::EnsureReady { .. })));
    assert!(runtime
        .operations()
        .iter()
        .all(|operation| { !matches!(operation, RuntimeOperation::SetEnabled(true)) }));
}

#[test]
fn cancel_restores_old_binding_and_ends_edit() {
    let initial = configured(true);
    let old_binding = initial.shortcut_binding.clone();
    let (coordinator, _, runtime, _) = harness(initial);
    let edit = begin(&coordinator);

    let outcome = coordinator
        .cancel_edit("trace-1".into(), edit.edit_id)
        .unwrap();

    assert!(outcome.success);
    assert!(!outcome.changed);
    assert_eq!(*runtime.binding.lock().unwrap(), old_binding);
    assert!(runtime.enabled.load(Ordering::Relaxed));
    assert!(coordinator.current_edit().unwrap().is_none());
}

#[test]
fn unchanged_binding_has_no_persistence_side_effect() {
    let initial = configured(true);
    let candidate = initial.shortcut_binding.clone().unwrap();
    let (coordinator, store, runtime, _) = harness(initial);
    let edit = begin(&coordinator);

    let outcome = coordinator
        .commit_edit("trace-1".into(), edit.edit_id, 7, candidate.clone())
        .unwrap();

    assert!(outcome.success);
    assert!(!outcome.changed);
    assert_eq!(store.commits.load(Ordering::Relaxed), 0);
    assert_eq!(*runtime.binding.lock().unwrap(), Some(candidate));
    assert_eq!(
        runtime.operations(),
        vec![
            RuntimeOperation::SetEnabled(false),
            RuntimeOperation::EnsureReady {
                force_reinstall: false,
            },
            RuntimeOperation::SetEnabled(true),
        ]
    );
}
