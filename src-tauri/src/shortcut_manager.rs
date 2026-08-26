//! Shortcut command orchestration, runtime binding transactions and Tauri lifecycle events.

use crate::config::{AppConfig, CURRENT_SCHEMA_VERSION};
use crate::physical_shortcut::{ModifierKind, ShortcutBinding, DEFAULT_SHORTCUT_LABEL};
use crate::services::{AppServices, ConfigService, ConfigServiceError};
use crate::shortcut_lifecycle::{
    ShortcutLifecycleCoordinator, ShortcutLifecycleSnapshot, ShortcutOperationKind,
    ShortcutOperationPhase, ShortcutRuntimeState,
};
use crate::voice_controller::{SessionEvent, VoiceSessionController};
use crate::windows_keyboard::{
    CaptureArmReceipt, CapturedShortcut, KeyboardEngineError, KeyboardEngineEvent,
    WindowsKeyboardEngine,
};
use crate::SharedRuntime;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const LIFECYCLE_EVENT: &str = "shortcut_lifecycle_changed";
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(30);
const RELEASE_TIMEOUT: Duration = Duration::from_secs(10);
const INVALID_BINDING: &str = "invalid_binding";
const RESERVED_BINDING: &str = "reserved_binding";
const REVISION_CONFLICT: &str = "revision_conflict";
const HOOK_UNAVAILABLE: &str = "hook_unavailable";
const PERSISTENCE_FAILED: &str = "persistence_failed";
const CAPTURE_TIMEOUT_CODE: &str = "capture_timeout";
const RELEASE_TIMEOUT_CODE: &str = "release_timeout";
const HOOK_INTERRUPTED: &str = "hook_interrupted";
const RUNTIME_ROLLBACK_FAILED: &str = "runtime_rollback_failed";

#[derive(Clone)]
struct CaptureTransaction {
    operation_id: u64,
    hook_generation: u64,
    expected_revision: u64,
    capture_attempt: u64,
    release_timeout_started: bool,
}

#[derive(Clone)]
struct UndoTransaction {
    change_id: u64,
    binding: Option<ShortcutBinding>,
    label: String,
    committed_revision: u64,
}

struct ManagerState {
    lifecycle: ShortcutLifecycleCoordinator,
    capture: Option<CaptureTransaction>,
    undo: Option<UndoTransaction>,
}

#[derive(Debug)]
struct ApplyFailure {
    code: &'static str,
    message: String,
    retryable: bool,
    runtime_error: Option<String>,
}

#[derive(Debug)]
enum ShortcutStoreFailure {
    Conflict,
    Storage(String),
}

trait ShortcutConfigStore {
    fn shortcut_snapshot(&self) -> AppConfig;
    fn commit_shortcut(
        &self,
        expected_revision: u64,
        next: AppConfig,
    ) -> Result<AppConfig, ShortcutStoreFailure>;
}

impl ShortcutConfigStore for ConfigService {
    fn shortcut_snapshot(&self) -> AppConfig {
        ConfigService::snapshot(self)
    }

    fn commit_shortcut(
        &self,
        expected_revision: u64,
        next: AppConfig,
    ) -> Result<AppConfig, ShortcutStoreFailure> {
        self.commit_config(expected_revision, next)
            .map_err(|error| match error {
                ConfigServiceError::Conflict(_) => ShortcutStoreFailure::Conflict,
                ConfigServiceError::Storage(error) => {
                    ShortcutStoreFailure::Storage(error.to_string())
                }
            })
    }
}

trait ShortcutBindingRuntime {
    fn replace_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String>;
    fn enable_binding(&self, enabled: bool);
}

impl ShortcutBindingRuntime for WindowsKeyboardEngine {
    fn replace_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
        self.set_binding(binding)
    }

    fn enable_binding(&self, enabled: bool) {
        self.set_enabled(enabled);
    }
}

#[derive(Debug)]
struct BindingCommit {
    config: AppConfig,
    old_binding: Option<ShortcutBinding>,
    old_label: String,
}

pub struct ShortcutManager {
    app: AppHandle,
    runtime: SharedRuntime,
    config: Arc<ConfigService>,
    controller: VoiceSessionController,
    operation_gate: Mutex<()>,
    state: Mutex<ManagerState>,
    engine: Mutex<Option<Arc<WindowsKeyboardEngine>>>,
    last_logged_sequence: AtomicU64,
}

impl ShortcutManager {
    pub fn initialize(
        app: &mut tauri::App,
        runtime: SharedRuntime,
        services: AppServices,
    ) -> tauri::Result<(VoiceSessionController, Arc<Self>)> {
        let controller = VoiceSessionController::new(runtime.clone(), services.clone());
        let config = services.config.snapshot();
        let binding = config.shortcut_binding.clone();
        let weak_slot: Arc<OnceLock<Weak<ShortcutManager>>> = Arc::new(OnceLock::new());
        let callback_slot = weak_slot.clone();
        let engine_result = WindowsKeyboardEngine::start(move |event| {
            if let Some(manager) = callback_slot.get().and_then(Weak::upgrade) {
                manager.handle_engine_event(event);
            }
        });
        let (engine, initial_error) = match engine_result {
            Ok(engine) => {
                let engine_error = engine.startup_error();
                let binding_error = binding
                    .is_none()
                    .then(|| "旧快捷键无法映射为物理键，请重新设置。".to_string());
                (Some(Arc::new(engine)), engine_error.or(binding_error))
            }
            Err(error) => (None, Some(error)),
        };
        let manager = Arc::new(Self {
            app: app.handle().clone(),
            runtime: runtime.clone(),
            config: services.config.clone(),
            controller: controller.clone(),
            operation_gate: Mutex::new(()),
            state: Mutex::new(ManagerState {
                lifecycle: ShortcutLifecycleCoordinator::new(
                    config.revision,
                    config.enabled,
                    config.shortcut.clone(),
                    binding.clone(),
                    initial_error.clone(),
                ),
                capture: None,
                undo: None,
            }),
            engine: Mutex::new(engine),
            last_logged_sequence: AtomicU64::new(0),
        });
        let _ = weak_slot.set(Arc::downgrade(&manager));
        if let Ok(engine) = manager.engine_handle() {
            if let Err(error) = engine.set_binding(binding.as_ref()) {
                manager.set_runtime_error(error);
            } else {
                engine.set_enabled(config.enabled && binding.is_some());
            }
        }
        if let Ok(mut voice) = runtime.lock() {
            voice.shortcut_registration_error = initial_error;
        }
        manager.publish_snapshot();
        Ok((controller, manager))
    }

    pub fn lifecycle(
        &self,
        operation_id: Option<u64>,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        self.state
            .lock()
            .map(|state| state.lifecycle.query_snapshot(operation_id))
            .map_err(|error| error.to_string())
    }

    pub fn start_capture(
        &self,
        expected_revision: u64,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        self.start_capture_locked(expected_revision)
    }

    fn start_capture_locked(
        &self,
        expected_revision: u64,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        let (operation_id, created) = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state
                .lifecycle
                .begin(ShortcutOperationKind::Capture, "正在准备快捷键录制。")
        };
        if !created {
            return self.lifecycle(Some(operation_id));
        }
        self.suspend_engine();
        self.publish_snapshot();
        let current = self.config.snapshot();
        if current.revision != expected_revision {
            return self.fail_operation_locked(
                operation_id,
                REVISION_CONFLICT,
                "配置已被其他操作更新，请刷新后重试。",
                true,
                None,
            );
        }
        let engine = match self.engine_handle() {
            Ok(engine) => engine,
            Err(error) => {
                return self.fail_operation_locked(
                    operation_id,
                    HOOK_UNAVAILABLE,
                    error.clone(),
                    true,
                    Some(error),
                )
            }
        };
        let prepared = match engine.prepare_capture() {
            Ok(prepared) => prepared,
            Err(error) => return self.fail_keyboard_engine_locked(operation_id, error),
        };
        {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if state.lifecycle.operation_phase(operation_id)
                != Some(ShortcutOperationPhase::Starting)
            {
                engine.cancel_capture(Some(operation_id));
                return Ok(state.lifecycle.query_snapshot(Some(operation_id)));
            }
            state.capture = Some(CaptureTransaction {
                operation_id,
                hook_generation: prepared.hook_generation,
                expected_revision,
                capture_attempt: 1,
                release_timeout_started: false,
            });
        }
        let receipt = match engine.arm_capture(prepared, operation_id) {
            Ok(receipt) => receipt,
            Err(error) => return self.fail_keyboard_engine_locked(operation_id, error),
        };
        let entered_capture = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            commit_armed_capture(&mut state, receipt)
        };
        if !entered_capture {
            engine.cancel_capture(Some(operation_id));
            return self.fail_operation_locked(
                operation_id,
                HOOK_UNAVAILABLE,
                "快捷键捕获状态在握手提交前发生变化。",
                true,
                None,
            );
        }
        metrics::counter!("shortcut.operation.started", "kind" => "capture").increment(1);
        self.publish_snapshot();
        self.schedule_capture_timeout(operation_id, receipt.hook_generation);
        self.lifecycle(Some(operation_id))
    }

    pub fn cancel_operation(&self, operation_id: u64) -> Result<ShortcutLifecycleSnapshot, String> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        self.cancel_operation_locked(operation_id)
    }

    fn cancel_operation_locked(
        &self,
        operation_id: u64,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        let cancellable = {
            let state = self.state.lock().map_err(|error| error.to_string())?;
            matches!(
                state.lifecycle.operation_phase(operation_id),
                Some(ShortcutOperationPhase::Starting | ShortcutOperationPhase::Capturing)
            )
        };
        if !cancellable {
            return self.lifecycle(Some(operation_id));
        }
        self.log_capture_diagnostics("cancel", operation_id);
        if let Ok(engine) = self.engine_handle() {
            engine.cancel_capture(Some(operation_id));
        }
        let restore_result = self.restore_authoritative_runtime_locked(Some(operation_id));
        let cancelled = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.capture = None;
            match restore_result {
                Err(error) => {
                    state.lifecycle.fail(
                        operation_id,
                        HOOK_UNAVAILABLE,
                        "取消录制后无法恢复快捷键 Hook。",
                        true,
                        Some(error),
                    );
                    false
                }
                Ok(current) => {
                    state.lifecycle.sync_authoritative_config(
                        current.revision,
                        current.enabled,
                        current.shortcut,
                        current.shortcut_binding,
                    );
                    state
                        .lifecycle
                        .cancel(operation_id, "已取消，原快捷键保持不变。");
                    true
                }
            }
        };
        if cancelled {
            metrics::counter!("shortcut.operation.cancelled", "kind" => "capture").increment(1);
        } else {
            metrics::counter!("shortcut.operation.failed", "error_code" => HOOK_UNAVAILABLE)
                .increment(1);
        }
        self.sync_voice_runtime_error();
        self.publish_snapshot();
        self.lifecycle(Some(operation_id))
    }

    pub fn restore_default(
        &self,
        expected_revision: u64,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        let (operation_id, created) = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.lifecycle.begin(
                ShortcutOperationKind::RestoreDefault,
                "正在准备恢复默认快捷键。",
            )
        };
        if !created {
            return self.lifecycle(Some(operation_id));
        }
        self.suspend_engine();
        self.publish_snapshot();
        self.run_binding_operation(
            operation_id,
            expected_revision,
            Some(ShortcutBinding::default_physical()),
            DEFAULT_SHORTCUT_LABEL.into(),
            true,
        )?;
        self.lifecycle(Some(operation_id))
    }

    pub fn undo(
        &self,
        change_id: u64,
        expected_revision: u64,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        let (operation_id, created) = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state
                .lifecycle
                .begin(ShortcutOperationKind::Undo, "正在准备撤销快捷键变更。")
        };
        if !created {
            return self.lifecycle(Some(operation_id));
        }
        self.suspend_engine();
        self.publish_snapshot();
        let transaction = self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .undo
            .clone()
            .filter(|undo| {
                undo.change_id == change_id && undo.committed_revision == expected_revision
            });
        let Some(transaction) = transaction else {
            return self.fail_operation_locked(
                operation_id,
                REVISION_CONFLICT,
                "该快捷键变更已无法撤销。",
                false,
                None,
            );
        };
        let succeeded = self.run_binding_operation(
            operation_id,
            expected_revision,
            transaction.binding,
            transaction.label,
            false,
        )?;
        if succeeded {
            if let Ok(mut state) = self.state.lock() {
                state.undo = None;
            }
        }
        self.lifecycle(Some(operation_id))
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let _operation = self
            .operation_gate
            .lock()
            .map_err(|error| error.to_string())?;
        let active_operation = self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .lifecycle
            .active_operation_id();
        if let Some(operation_id) = active_operation {
            let _ = self.cancel_operation_locked(operation_id);
        }
        let current = self.config.snapshot();
        let binding = current.shortcut_binding.clone();
        let result = self.engine_handle().and_then(|engine| {
            if enabled {
                engine
                    .ensure_runtime_ready(true)
                    .map_err(|error| error.to_string())?;
            }
            engine.cancel_capture(None);
            engine.set_enabled(enabled && binding.is_some());
            Ok::<(), String>(())
        });
        if let Err(error) = result {
            if let Ok(mut state) = self.state.lock() {
                state.lifecycle.sync_authoritative_config(
                    current.revision,
                    current.enabled,
                    current.shortcut,
                    current.shortcut_binding,
                );
                state.lifecycle.set_runtime_error(error.clone());
            }
            self.sync_voice_runtime_error();
            self.publish_snapshot();
            return Err(error);
        }
        {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.lifecycle.sync_authoritative_config(
                current.revision,
                current.enabled,
                current.shortcut,
                current.shortcut_binding,
            );
            state.lifecycle.set_enabled(enabled);
            state.capture = None;
        }
        self.sync_voice_runtime_error();
        self.publish_snapshot();
        Ok(())
    }

    pub fn resume(&self) {
        let Ok(_operation) = self.operation_gate.lock() else {
            self.set_runtime_error("快捷键操作门闩已损坏。".into());
            return;
        };
        let active_capture = self.state.lock().ok().and_then(|state| {
            state
                .lifecycle
                .active_operation_id()
                .filter(|operation_id| {
                    state.lifecycle.operation_kind(*operation_id)
                        == Some(ShortcutOperationKind::Capture)
                })
        });
        if let Some(operation_id) = active_capture {
            let _ = self.fail_operation_with_source_locked(
                "resume",
                operation_id,
                HOOK_INTERRUPTED,
                "系统状态变化中断了快捷键录制。",
                true,
                None,
            );
        }
        let current = self.config.snapshot();
        let binding = current.shortcut_binding.clone();
        let enabled = current.enabled;
        let result = self.engine_handle().and_then(|engine| {
            engine
                .ensure_runtime_ready(true)
                .map_err(|error| error.to_string())?;
            engine.set_binding(binding.as_ref())?;
            engine.set_enabled(enabled && binding.is_some());
            Ok(())
        });
        match result {
            Ok(()) => {
                if let Ok(mut state) = self.state.lock() {
                    state.lifecycle.sync_authoritative_config(
                        current.revision,
                        current.enabled,
                        current.shortcut,
                        current.shortcut_binding,
                    );
                    state.lifecycle.restore_runtime_health();
                }
                self.sync_voice_runtime_error();
                self.publish_snapshot();
            }
            Err(error) => self.set_runtime_error(error),
        }
    }

    pub fn shutdown(&self) {
        let engine = {
            let Ok(_operation) = self.operation_gate.lock() else {
                return;
            };
            self.engine.lock().ok().and_then(|mut engine| engine.take())
        };
        if let Some(engine) = engine {
            engine.shutdown();
        }
    }

    fn handle_engine_event(&self, event: KeyboardEngineEvent) {
        match event {
            KeyboardEngineEvent::Pressed => {
                self.controller.submit(&self.app, SessionEvent::Pressed)
            }
            KeyboardEngineEvent::Released => {
                self.controller.submit(&self.app, SessionEvent::Released)
            }
            KeyboardEngineEvent::CaptureProgress {
                capture_id,
                hook_generation,
                label,
                binding,
            } => self.publish_capture_progress(capture_id, hook_generation, label, binding),
            KeyboardEngineEvent::CaptureCancelled {
                capture_id,
                hook_generation,
            } => {
                self.cancel_capture_from_engine(capture_id, hook_generation);
            }
            KeyboardEngineEvent::Captured(captured) => self.commit_capture(captured),
        }
    }

    fn publish_capture_progress(
        &self,
        operation_id: u64,
        hook_generation: u64,
        label: String,
        binding: Option<ShortcutBinding>,
    ) {
        let Ok(_operation) = self.operation_gate.lock() else {
            return;
        };
        let release_timeout_attempt = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if !state.capture.as_ref().is_some_and(|capture| {
                capture.operation_id == operation_id && capture.hook_generation == hook_generation
            }) {
                return;
            }
            let changed =
                state
                    .lifecycle
                    .update_candidate(operation_id, label.clone(), binding.clone());
            if !changed {
                return;
            }
            if binding.is_none() && label.is_empty() {
                if let Some(capture) = state.capture.as_mut() {
                    if capture.release_timeout_started {
                        capture.capture_attempt = capture.capture_attempt.saturating_add(1);
                        capture.release_timeout_started = false;
                    }
                }
                None
            } else {
                let should_start = binding.is_some()
                    && state.capture.as_ref().is_some_and(|capture| {
                        capture.operation_id == operation_id
                            && capture.hook_generation == hook_generation
                            && !capture.release_timeout_started
                    });
                if should_start {
                    state.capture.as_mut().map(|capture| {
                        capture.release_timeout_started = true;
                        capture.capture_attempt
                    })
                } else {
                    None
                }
            }
        };
        log::debug!(
            "shortcut operation candidate operationId={} label={}",
            operation_id,
            label
        );
        self.publish_snapshot();
        if let Some(capture_attempt) = release_timeout_attempt {
            self.schedule_release_timeout(operation_id, hook_generation, capture_attempt);
        }
    }

    fn commit_capture(&self, captured: CapturedShortcut) {
        let Ok(_operation) = self.operation_gate.lock() else {
            return;
        };
        let capture = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.capture.clone())
            .filter(|capture| {
                capture.operation_id == captured.capture_id
                    && capture.hook_generation == captured.hook_generation
            });
        let Some(capture) = capture else {
            return;
        };
        {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            if !state.lifecycle.transition(
                captured.capture_id,
                ShortcutOperationPhase::Validating,
                "正在验证新的快捷键。",
            ) {
                return;
            }
        }
        self.publish_snapshot();
        if let Err((code, message)) = validate_captured(&captured) {
            let engine = match self.engine_handle() {
                Ok(engine) => engine,
                Err(error) => {
                    let _ = self.fail_operation_locked(
                        captured.capture_id,
                        HOOK_UNAVAILABLE,
                        error.clone(),
                        true,
                        Some(error),
                    );
                    return;
                }
            };
            if let Err(error) = engine.retry_capture(
                captured.capture_id,
                captured.hook_generation,
            ) {
                let _ = self.fail_keyboard_engine_locked(captured.capture_id, error);
                return;
            }
            let resumed = {
                let Ok(mut state) = self.state.lock() else {
                    engine.cancel_capture(Some(captured.capture_id));
                    return;
                };
                let transaction_matches = state.capture.as_ref().is_some_and(|capture| {
                    capture.operation_id == captured.capture_id
                        && capture.hook_generation == captured.hook_generation
                });
                if transaction_matches {
                    if let Some(capture) = state.capture.as_mut() {
                        capture.capture_attempt = capture.capture_attempt.saturating_add(1);
                        capture.release_timeout_started = false;
                    }
                    state
                        .lifecycle
                        .reject_candidate(captured.capture_id, code, message)
                } else {
                    false
                }
            };
            if !resumed {
                engine.cancel_capture(Some(captured.capture_id));
                let _ = self.fail_operation_locked(
                    captured.capture_id,
                    HOOK_UNAVAILABLE,
                    "快捷键验证后无法恢复录入状态。",
                    true,
                    None,
                );
                return;
            }
            metrics::counter!("shortcut.capture.validation_rejected", "error_code" => code)
                .increment(1);
            self.publish_snapshot();
            return;
        }
        let _ = self.run_binding_operation(
            captured.capture_id,
            capture.expected_revision,
            Some(captured.binding),
            captured.label,
            true,
        );
    }

    fn run_binding_operation(
        &self,
        operation_id: u64,
        expected_revision: u64,
        binding: Option<ShortcutBinding>,
        label: String,
        record_undo: bool,
    ) -> Result<bool, String> {
        let phase = self
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .lifecycle
            .operation_phase(operation_id);
        if phase == Some(ShortcutOperationPhase::Starting) {
            {
                let mut state = self.state.lock().map_err(|error| error.to_string())?;
                state.lifecycle.transition(
                    operation_id,
                    ShortcutOperationPhase::Validating,
                    "正在验证目标快捷键。",
                );
            }
            self.publish_snapshot();
            if let Some(binding) = binding.as_ref() {
                if let Err(message) = binding.validate() {
                    self.fail_operation_locked(operation_id, INVALID_BINDING, message, true, None)?;
                    return Ok(false);
                }
            }
        }
        let current = self.config.snapshot();
        match binding_is_unchanged(&current, expected_revision, &binding) {
            Ok(true) => {
                let restore_runtime = self.restore_authoritative_runtime_locked(Some(operation_id));
                if let Err(error) = restore_runtime {
                    self.fail_operation_locked(
                        operation_id,
                        HOOK_UNAVAILABLE,
                        "快捷键未变化，但运行时 Hook 恢复失败。",
                        true,
                        Some(error),
                    )?;
                    return Ok(false);
                }
                let message = if current.enabled {
                    "快捷键未发生变化。"
                } else {
                    "快捷键未发生变化；当前语音输入已关闭。"
                };
                {
                    let mut state = self.state.lock().map_err(|error| error.to_string())?;
                    state.capture = None;
                    state.lifecycle.succeed_unchanged(operation_id, message);
                }
                metrics::counter!("shortcut.operation.unchanged").increment(1);
                self.sync_voice_runtime_error();
                self.publish_snapshot();
                return Ok(true);
            }
            Ok(false) => {}
            Err(error) => {
                self.fail_operation_locked(
                    operation_id,
                    error.code,
                    error.message,
                    error.retryable,
                    error.runtime_error,
                )?;
                return Ok(false);
            }
        }
        {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if !state.lifecycle.transition(
                operation_id,
                ShortcutOperationPhase::Applying,
                "正在应用新的快捷键。",
            ) {
                return Ok(false);
            }
        }
        self.publish_snapshot();
        match self.apply_binding_transaction(
            operation_id,
            binding,
            label,
            expected_revision,
            record_undo,
        ) {
            Ok(()) => {
                self.sync_voice_runtime_error();
                self.publish_snapshot();
                Ok(true)
            }
            Err(error) => {
                self.fail_operation_locked(
                    operation_id,
                    error.code,
                    error.message,
                    error.retryable,
                    error.runtime_error,
                )?;
                Ok(false)
            }
        }
    }

    fn apply_binding_transaction(
        &self,
        operation_id: u64,
        binding: Option<ShortcutBinding>,
        label: String,
        expected_revision: u64,
        record_undo: bool,
    ) -> Result<(), ApplyFailure> {
        let engine = self.engine_handle().map_err(|error| ApplyFailure {
            code: HOOK_UNAVAILABLE,
            message: error,
            retryable: true,
            runtime_error: None,
        })?;
        engine.cancel_capture(Some(operation_id));
        if self.config.snapshot().enabled {
            engine
                .ensure_runtime_ready(false)
                .map_err(|error| ApplyFailure {
                    code: HOOK_UNAVAILABLE,
                    message: error.to_string(),
                    retryable: true,
                    runtime_error: None,
                })?;
        }
        let transaction = execute_binding_transaction(
            self.config.as_ref(),
            engine.as_ref(),
            binding.clone(),
            label.clone(),
            expected_revision,
        )?;
        let BindingCommit {
            config: committed,
            old_binding,
            old_label,
        } = transaction;
        let operation_kind = {
            let mut state = self.state.lock().map_err(|error| ApplyFailure {
                code: RUNTIME_ROLLBACK_FAILED,
                message: error.to_string(),
                retryable: false,
                runtime_error: Some("快捷键状态锁已损坏。".into()),
            })?;
            state.capture = None;
            if record_undo {
                state.undo = Some(UndoTransaction {
                    change_id: operation_id,
                    binding: old_binding,
                    label: old_label,
                    committed_revision: committed.revision,
                });
            }
            let message = if committed.enabled {
                format!("快捷键 {label} 已启用。")
            } else {
                format!("快捷键 {label} 已保存；开启语音输入后生效。")
            };
            let operation_kind = state.lifecycle.operation_kind(operation_id);
            state
                .lifecycle
                .succeed(operation_id, committed.revision, label, binding, message);
            operation_kind
        };
        let kind = match operation_kind {
            Some(ShortcutOperationKind::Capture) => "capture",
            Some(ShortcutOperationKind::RestoreDefault) => "restore_default",
            Some(ShortcutOperationKind::Undo) => "undo",
            None => "unknown",
        };
        metrics::counter!("shortcut.operation.succeeded", "kind" => kind).increment(1);
        Ok(())
    }

    fn fail_keyboard_engine_locked(
        &self,
        operation_id: u64,
        error: KeyboardEngineError,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        log::warn!(
            "shortcut capture handshake failed operationId={} kind={:?} runtimeUnavailable={} message={}",
            operation_id,
            error.kind,
            error.runtime_unavailable(),
            error.message
        );
        self.fail_operation_with_source_locked(
            "handshake",
            operation_id,
            HOOK_UNAVAILABLE,
            error.message,
            true,
            None,
        )
    }

    fn fail_operation_locked(
        &self,
        operation_id: u64,
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        runtime_error: Option<String>,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        self.fail_operation_with_source_locked(
            "failed",
            operation_id,
            code,
            message,
            retryable,
            runtime_error,
        )
    }

    fn fail_operation_with_source_locked(
        &self,
        source: &str,
        operation_id: u64,
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        runtime_error: Option<String>,
    ) -> Result<ShortcutLifecycleSnapshot, String> {
        let message = message.into();
        self.log_capture_diagnostics(source, operation_id);
        let restore_result = self.restore_authoritative_runtime_locked(Some(operation_id));
        let restore_error = restore_result.as_ref().err().cloned();
        let runtime_error = runtime_error.or(restore_error);
        {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if let Ok(current) = restore_result {
                state.lifecycle.sync_authoritative_config(
                    current.revision,
                    current.enabled,
                    current.shortcut,
                    current.shortcut_binding,
                );
            }
            if state
                .capture
                .as_ref()
                .is_some_and(|capture| capture.operation_id == operation_id)
            {
                state.capture = None;
            }
            state
                .lifecycle
                .fail(operation_id, code, message, retryable, runtime_error);
        }
        metrics::counter!("shortcut.operation.failed", "error_code" => code).increment(1);
        self.sync_voice_runtime_error();
        self.publish_snapshot();
        self.lifecycle(Some(operation_id))
    }

    fn cancel_capture_from_engine(&self, operation_id: u64, hook_generation: u64) {
        let Ok(_operation) = self.operation_gate.lock() else {
            return;
        };
        let matches = self.state.lock().ok().is_some_and(|state| {
            state.capture.as_ref().is_some_and(|capture| {
                capture.operation_id == operation_id && capture.hook_generation == hook_generation
            })
        });
        if matches {
            let _ = self.cancel_operation_locked(operation_id);
        }
    }

    fn capture_timeout(
        &self,
        operation_id: u64,
        hook_generation: u64,
        capture_attempt: Option<u64>,
    ) {
        let Ok(_operation) = self.operation_gate.lock() else {
            return;
        };
        let active = self.state.lock().ok().is_some_and(|state| {
            state.capture.as_ref().is_some_and(|capture| {
                capture_timeout_matches(
                    capture,
                    operation_id,
                    hook_generation,
                    state.lifecycle.operation_phase(operation_id),
                    capture_attempt,
                )
            })
        });
        if !active {
            return;
        }
        let (code, message) = if capture_attempt.is_some() {
            (
                RELEASE_TIMEOUT_CODE,
                "等待按键全部释放超时，快捷键配置未更改。",
            )
        } else {
            (
                CAPTURE_TIMEOUT_CODE,
                "等待快捷键输入超时，快捷键配置未更改。",
            )
        };
        let _ = self.fail_operation_with_source_locked(
            "timeout",
            operation_id,
            code,
            message,
            true,
            None,
        );
    }

    fn schedule_capture_timeout(&self, operation_id: u64, hook_generation: u64) {
        self.schedule_timeout(operation_id, hook_generation, CAPTURE_TIMEOUT, None);
    }

    fn schedule_release_timeout(
        &self,
        operation_id: u64,
        hook_generation: u64,
        capture_attempt: u64,
    ) {
        self.schedule_timeout(
            operation_id,
            hook_generation,
            RELEASE_TIMEOUT,
            Some(capture_attempt),
        );
    }

    fn schedule_timeout(
        &self,
        operation_id: u64,
        hook_generation: u64,
        delay: Duration,
        capture_attempt: Option<u64>,
    ) {
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            if let Some(manager) = app.try_state::<Arc<ShortcutManager>>() {
                manager.capture_timeout(operation_id, hook_generation, capture_attempt);
            }
        });
    }

    fn suspend_engine(&self) {
        if let Ok(engine) = self.engine_handle() {
            engine.set_enabled(false);
        }
    }

    fn engine_handle(&self) -> Result<Arc<WindowsKeyboardEngine>, String> {
        self.engine
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
            .cloned()
            .ok_or_else(|| "物理快捷键引擎未运行。".to_string())
    }

    fn restore_authoritative_runtime_locked(
        &self,
        capture_id: Option<u64>,
    ) -> Result<AppConfig, String> {
        let current = self.config.snapshot();
        let binding = current.shortcut_binding.as_ref();
        let engine = self.engine_handle()?;
        engine.cancel_capture(capture_id);
        engine.set_binding(binding)?;
        if current.enabled && binding.is_some() {
            engine
                .ensure_runtime_ready(false)
                .map_err(|error| error.to_string())?;
            engine.set_enabled(true);
        } else {
            engine.set_enabled(false);
        }
        Ok(current)
    }

    fn log_capture_diagnostics(&self, source: &str, operation_id: u64) {
        let (phase, transaction_generation) = self
            .state
            .lock()
            .map(|state| {
                (
                    state.lifecycle.operation_phase(operation_id),
                    state.capture.as_ref().and_then(|capture| {
                        (capture.operation_id == operation_id).then_some(capture.hook_generation)
                    }),
                )
            })
            .unwrap_or((None, None));
        let Ok(engine) = self.engine_handle() else {
            log::warn!(
                "shortcut capture diagnostics operationId={} source={} phase={:?} engine=missing",
                operation_id,
                source,
                phase
            );
            return;
        };
        let diagnostics = engine.capture_diagnostics();
        log::info!(
            "shortcut capture diagnostics operationId={} source={} phase={:?} hookGeneration={} captureId={} observed={} emitted={} dropped={} hookAlive={} hookWorkerAlive={} dispatchAlive={}",
            operation_id,
            source,
            phase,
            transaction_generation.unwrap_or(diagnostics.hook_generation),
            diagnostics.capture_id,
            diagnostics.observed_events,
            diagnostics.emitted_events,
            diagnostics.dropped_events,
            diagnostics.hook_alive,
            diagnostics.hook_worker_alive,
            diagnostics.dispatch_alive
        );
    }

    fn set_runtime_error(&self, message: String) {
        metrics::counter!("shortcut.runtime.error").increment(1);
        if let Ok(mut state) = self.state.lock() {
            state.lifecycle.set_runtime_error(message);
        }
        self.sync_voice_runtime_error();
        self.publish_snapshot();
    }

    fn sync_voice_runtime_error(&self) {
        let runtime_error = self.state.lock().ok().and_then(|state| {
            let snapshot = state.lifecycle.snapshot();
            (snapshot.runtime.state == ShortcutRuntimeState::Error)
                .then_some(snapshot.runtime.message)
        });
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.shortcut_registration_error = runtime_error;
        }
    }

    fn publish_snapshot(&self) {
        let snapshot = match self.state.lock() {
            Ok(state) => state.lifecycle.snapshot(),
            Err(error) => {
                log::error!("failed to publish shortcut lifecycle: {error}");
                return;
            }
        };
        let should_log = self
            .last_logged_sequence
            .swap(snapshot.sequence, Ordering::AcqRel)
            != snapshot.sequence;
        if should_log {
            if let Some(operation) = snapshot.operation.as_ref() {
                if operation.phase.is_active() {
                    log::info!(
                        "shortcut lifecycle operationId={} kind={:?} phase={:?}",
                        operation.operation_id,
                        operation.kind,
                        operation.phase
                    );
                } else {
                    log::info!(
                        "shortcut lifecycle operationId={} kind={:?} phase={:?} errorCode={} changed={} candidate={}",
                        operation.operation_id,
                        operation.kind,
                        operation.phase,
                        operation.error_code.as_deref().unwrap_or("none"),
                        match operation.changed {
                            Some(true) => "true",
                            Some(false) => "false",
                            None => "none",
                        },
                        operation.candidate_label.as_deref().unwrap_or("none")
                    );
                }
            }
        }
        if let Err(error) = self.app.emit(LIFECYCLE_EVENT, snapshot) {
            log::warn!("failed to emit shortcut lifecycle: {error}");
        }
    }
}

fn binding_is_unchanged(
    current: &AppConfig,
    expected_revision: u64,
    binding: &Option<ShortcutBinding>,
) -> Result<bool, ApplyFailure> {
    if current.revision != expected_revision {
        return Err(ApplyFailure {
            code: REVISION_CONFLICT,
            message: "配置已被其他操作更新，请刷新后重试。".into(),
            retryable: true,
            runtime_error: None,
        });
    }
    Ok(
        match (current.shortcut_binding.as_ref(), binding.as_ref()) {
            (Some(current), Some(candidate)) => current.physically_equivalent(candidate),
            (None, None) => true,
            _ => false,
        },
    )
}

fn commit_armed_capture(state: &mut ManagerState, receipt: CaptureArmReceipt) -> bool {
    let transaction_matches = state.capture.as_ref().is_some_and(|capture| {
        capture.operation_id == receipt.operation_id
            && capture.hook_generation == receipt.hook_generation
    });
    transaction_matches
        && state.lifecycle.transition(
            receipt.operation_id,
            ShortcutOperationPhase::Capturing,
            "请按下新的物理快捷键，松开后自动保存。",
        )
}

fn validate_captured(captured: &CapturedShortcut) -> Result<(), (&'static str, String)> {
    captured
        .binding
        .validate()
        .map_err(|message| (INVALID_BINDING, message))?;
    if captured.label.ends_with("F12") {
        return Err((
            RESERVED_BINDING,
            "F12 由 Windows 调试器保留，不能作为语音快捷键。".into(),
        ));
    }
    let has_alt = captured
        .binding
        .modifiers
        .iter()
        .any(|modifier| modifier.kind == ModifierKind::Alt);
    let has_ctrl = captured
        .binding
        .modifiers
        .iter()
        .any(|modifier| modifier.kind == ModifierKind::Control);
    let has_shift = captured
        .binding
        .modifiers
        .iter()
        .any(|modifier| modifier.kind == ModifierKind::Shift);
    if has_alt && !has_ctrl && !has_shift && captured.label.ends_with("Tab") {
        return Err((RESERVED_BINDING, "Alt+Tab 是系统切换窗口快捷键。".into()));
    }
    if has_ctrl && has_alt && captured.label.ends_with("Delete") {
        return Err((
            RESERVED_BINDING,
            "Ctrl+Alt+Delete 是系统安全快捷键。".into(),
        ));
    }
    Ok(())
}

fn execute_binding_transaction(
    store: &impl ShortcutConfigStore,
    runtime: &impl ShortcutBindingRuntime,
    binding: Option<ShortcutBinding>,
    label: String,
    expected_revision: u64,
) -> Result<BindingCommit, ApplyFailure> {
    let current = store.shortcut_snapshot();
    if current.revision != expected_revision {
        return Err(ApplyFailure {
            code: REVISION_CONFLICT,
            message: "配置已被其他操作更新，请刷新后重试。".into(),
            retryable: true,
            runtime_error: None,
        });
    }
    let old_binding = current.shortcut_binding.clone();
    let old_label = current.shortcut.clone();
    let enabled = current.enabled;
    runtime
        .replace_binding(binding.as_ref())
        .map_err(|message| ApplyFailure {
            code: HOOK_UNAVAILABLE,
            message,
            retryable: true,
            runtime_error: None,
        })?;
    runtime.enable_binding(enabled && binding.is_some());

    let mut next = current;
    next.shortcut = label;
    next.shortcut_binding = binding;
    next.schema_version = CURRENT_SCHEMA_VERSION;
    next.revision = next.revision.saturating_add(1);
    match store.commit_shortcut(expected_revision, next) {
        Ok(config) => Ok(BindingCommit {
            config,
            old_binding,
            old_label,
        }),
        Err(error) => {
            let (code, message) = match error {
                ShortcutStoreFailure::Conflict => (
                    REVISION_CONFLICT,
                    "配置已被其他操作更新，请刷新后重试。".into(),
                ),
                ShortcutStoreFailure::Storage(message) => (PERSISTENCE_FAILED, message),
            };
            let rollback = runtime.replace_binding(old_binding.as_ref()).map(|()| {
                runtime.enable_binding(enabled && old_binding.is_some());
            });
            match rollback {
                Ok(()) => Err(ApplyFailure {
                    code,
                    message,
                    retryable: true,
                    runtime_error: None,
                }),
                Err(rollback_error) => Err(ApplyFailure {
                    code: RUNTIME_ROLLBACK_FAILED,
                    message: format!("{message}；恢复原快捷键失败：{rollback_error}"),
                    retryable: false,
                    runtime_error: Some(format!(
                        "快捷键配置保存失败，且原运行时绑定恢复失败：{rollback_error}"
                    )),
                }),
            }
        }
    }
}

fn capture_timeout_matches(
    capture: &CaptureTransaction,
    operation_id: u64,
    hook_generation: u64,
    phase: Option<ShortcutOperationPhase>,
    capture_attempt: Option<u64>,
) -> bool {
    capture.operation_id == operation_id
        && capture.hook_generation == hook_generation
        && phase == Some(ShortcutOperationPhase::Capturing)
        && match capture_attempt {
            Some(attempt) => {
                capture.release_timeout_started && capture.capture_attempt == attempt
            }
            None => !capture.release_timeout_started,
        }
}

#[cfg(test)]
mod transaction_tests {
    use super::*;
    use crate::physical_shortcut::PhysicalKeyId;
    use std::cell::{Cell, RefCell};

    #[derive(Clone, Copy)]
    enum CommitMode {
        Success,
        Conflict,
        Storage,
    }

    struct FakeStore {
        current: RefCell<AppConfig>,
        mode: CommitMode,
    }

    impl ShortcutConfigStore for FakeStore {
        fn shortcut_snapshot(&self) -> AppConfig {
            self.current.borrow().clone()
        }

        fn commit_shortcut(
            &self,
            expected_revision: u64,
            next: AppConfig,
        ) -> Result<AppConfig, ShortcutStoreFailure> {
            match self.mode {
                CommitMode::Conflict => Err(ShortcutStoreFailure::Conflict),
                CommitMode::Storage => Err(ShortcutStoreFailure::Storage("disk full".into())),
                CommitMode::Success => {
                    if self.current.borrow().revision != expected_revision {
                        return Err(ShortcutStoreFailure::Conflict);
                    }
                    *self.current.borrow_mut() = next.clone();
                    Ok(next)
                }
            }
        }
    }

    struct FakeRuntime {
        binding: RefCell<Option<ShortcutBinding>>,
        enabled: Cell<bool>,
        replace_calls: Cell<usize>,
        fail_on_call: Option<usize>,
    }

    impl ShortcutBindingRuntime for FakeRuntime {
        fn replace_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
            let call = self.replace_calls.get() + 1;
            self.replace_calls.set(call);
            if self.fail_on_call == Some(call) {
                return Err(format!("runtime replace {call} failed"));
            }
            *self.binding.borrow_mut() = binding.cloned();
            Ok(())
        }

        fn enable_binding(&self, enabled: bool) {
            self.enabled.set(enabled);
        }
    }

    fn fixture(
        mode: CommitMode,
        fail_on_call: Option<usize>,
    ) -> (FakeStore, FakeRuntime, ShortcutBinding, ShortcutBinding) {
        let mut current = AppConfig::default();
        current.revision = 3;
        current.enabled = true;
        let old = ShortcutBinding::default_physical();
        current.shortcut = DEFAULT_SHORTCUT_LABEL.into();
        current.shortcut_binding = Some(old.clone());
        let mut next = old.clone();
        next.trigger = PhysicalKeyId::new(0x2f, false);
        (
            FakeStore {
                current: RefCell::new(current),
                mode,
            },
            FakeRuntime {
                binding: RefCell::new(Some(old.clone())),
                enabled: Cell::new(true),
                replace_calls: Cell::new(0),
                fail_on_call,
            },
            old,
            next,
        )
    }

    #[test]
    fn runtime_replace_failure_does_not_touch_config() {
        let (store, runtime, old, next) = fixture(CommitMode::Success, Some(1));
        let failure = execute_binding_transaction(&store, &runtime, Some(next), "next".into(), 3)
            .unwrap_err();
        assert_eq!(failure.code, HOOK_UNAVAILABLE);
        assert_eq!(store.shortcut_snapshot().revision, 3);
        assert_eq!(runtime.binding.borrow().clone(), Some(old));
    }

    #[test]
    fn stale_revision_is_rejected_before_runtime_replacement() {
        let (store, runtime, old, next) = fixture(CommitMode::Success, None);
        let failure = execute_binding_transaction(&store, &runtime, Some(next), "next".into(), 2)
            .unwrap_err();
        assert_eq!(failure.code, REVISION_CONFLICT);
        assert_eq!(runtime.replace_calls.get(), 0);
        assert_eq!(runtime.binding.borrow().clone(), Some(old));
    }

    #[test]
    fn persistence_failure_restores_old_runtime_binding() {
        let (store, runtime, old, next) = fixture(CommitMode::Storage, None);
        let failure = execute_binding_transaction(&store, &runtime, Some(next), "next".into(), 3)
            .unwrap_err();
        assert_eq!(failure.code, PERSISTENCE_FAILED);
        assert_eq!(runtime.replace_calls.get(), 2);
        assert_eq!(runtime.binding.borrow().clone(), Some(old));
        assert!(runtime.enabled.get());
    }

    #[test]
    fn rollback_failure_is_a_runtime_error() {
        let (store, runtime, _old, next) = fixture(CommitMode::Storage, Some(2));
        let failure =
            execute_binding_transaction(&store, &runtime, Some(next.clone()), "next".into(), 3)
                .unwrap_err();
        assert_eq!(failure.code, RUNTIME_ROLLBACK_FAILED);
        assert!(failure.runtime_error.is_some());
        assert_eq!(runtime.binding.borrow().clone(), Some(next));
    }

    #[test]
    fn commit_time_conflict_rolls_runtime_back() {
        let (store, runtime, old, next) = fixture(CommitMode::Conflict, None);
        let failure = execute_binding_transaction(&store, &runtime, Some(next), "next".into(), 3)
            .unwrap_err();
        assert_eq!(failure.code, REVISION_CONFLICT);
        assert_eq!(runtime.binding.borrow().clone(), Some(old));
    }

    #[test]
    fn successful_transaction_updates_runtime_and_config_together() {
        let (store, runtime, _old, next) = fixture(CommitMode::Success, None);
        let committed =
            execute_binding_transaction(&store, &runtime, Some(next.clone()), "next".into(), 3)
                .unwrap();
        assert_eq!(committed.config.revision, 4);
        assert_eq!(committed.config.shortcut_binding, Some(next.clone()));
        assert_eq!(runtime.binding.borrow().clone(), Some(next));
        assert!(runtime.enabled.get());
    }

    #[test]
    fn unchanged_binding_is_detected_before_runtime_or_config_mutation() {
        let (store, runtime, old, _next) = fixture(CommitMode::Success, None);
        let current = store.shortcut_snapshot();
        assert!(binding_is_unchanged(&current, 3, &Some(old)).unwrap());
        assert_eq!(runtime.replace_calls.get(), 0);
        assert_eq!(store.shortcut_snapshot().revision, 3);
    }

    #[test]
    fn unchanged_binding_still_honors_expected_revision() {
        let (store, _runtime, old, _next) = fixture(CommitMode::Success, None);
        let failure = binding_is_unchanged(&store.shortcut_snapshot(), 2, &Some(old)).unwrap_err();
        assert_eq!(failure.code, REVISION_CONFLICT);
    }

    #[test]
    fn capturing_is_not_public_until_the_matching_arm_receipt_is_committed() {
        let binding = ShortcutBinding::default_physical();
        let mut state = ManagerState {
            lifecycle: ShortcutLifecycleCoordinator::new(
                3,
                true,
                DEFAULT_SHORTCUT_LABEL.into(),
                Some(binding),
                None,
            ),
            capture: None,
            undo: None,
        };
        let (operation_id, created) = state
            .lifecycle
            .begin(ShortcutOperationKind::Capture, "正在准备快捷键录制。");
        assert!(created);
        state.capture = Some(CaptureTransaction {
            operation_id,
            hook_generation: 7,
            expected_revision: 3,
            capture_attempt: 1,
            release_timeout_started: false,
        });
        assert_eq!(
            state
                .lifecycle
                .query_snapshot(Some(operation_id))
                .operation
                .unwrap()
                .phase,
            ShortcutOperationPhase::Starting
        );
        assert!(!commit_armed_capture(
            &mut state,
            CaptureArmReceipt {
                operation_id,
                hook_generation: 6,
            }
        ));
        assert_eq!(
            state
                .lifecycle
                .query_snapshot(Some(operation_id))
                .operation
                .unwrap()
                .phase,
            ShortcutOperationPhase::Starting
        );
        assert!(commit_armed_capture(
            &mut state,
            CaptureArmReceipt {
                operation_id,
                hook_generation: 7,
            }
        ));
        assert_eq!(
            state
                .lifecycle
                .query_snapshot(Some(operation_id))
                .operation
                .unwrap()
                .phase,
            ShortcutOperationPhase::Capturing
        );
    }

    #[test]
    fn undo_to_an_unmapped_legacy_binding_disables_runtime_consistently() {
        let (store, runtime, _old, _next) = fixture(CommitMode::Success, None);
        let committed =
            execute_binding_transaction(&store, &runtime, None, "legacy".into(), 3).unwrap();
        assert_eq!(committed.config.shortcut_binding, None);
        assert_eq!(runtime.binding.borrow().clone(), None);
        assert!(!runtime.enabled.get());
    }

    #[test]
    fn capture_timeout_stops_after_main_selection_and_release_timeout_takes_over() {
        let waiting = CaptureTransaction {
            operation_id: 9,
            hook_generation: 4,
            expected_revision: 3,
            capture_attempt: 1,
            release_timeout_started: false,
        };
        assert!(capture_timeout_matches(
            &waiting,
            9,
            4,
            Some(ShortcutOperationPhase::Capturing),
            None,
        ));
        let releasing = CaptureTransaction {
            release_timeout_started: true,
            ..waiting
        };
        assert!(!capture_timeout_matches(
            &releasing,
            9,
            4,
            Some(ShortcutOperationPhase::Capturing),
            None,
        ));
        assert!(capture_timeout_matches(
            &releasing,
            9,
            4,
            Some(ShortcutOperationPhase::Capturing),
            Some(1),
        ));
        assert!(!capture_timeout_matches(
            &releasing,
            8,
            4,
            Some(ShortcutOperationPhase::Capturing),
            Some(1),
        ));
        assert!(!capture_timeout_matches(
            &releasing,
            9,
            3,
            Some(ShortcutOperationPhase::Capturing),
            Some(1),
        ));
    }

    #[test]
    fn capture_validation_accepts_right_modifier_single_key_and_modifier_chord() {
        let right_control = CapturedShortcut {
            capture_id: 1,
            hook_generation: 1,
            binding: ShortcutBinding {
                modifiers: Vec::new(),
                trigger: PhysicalKeyId::new(0x1d, true),
            },
            label: "右 Ctrl".into(),
        };
        assert!(validate_captured(&right_control).is_ok());

        let chord = CapturedShortcut {
            capture_id: 2,
            hook_generation: 1,
            binding: crate::physical_shortcut::modifier_only_binding(
                crate::physical_shortcut::RIGHT_CTRL | crate::physical_shortcut::RIGHT_SHIFT,
            )
            .unwrap(),
            label: "右 Ctrl+右 Shift".into(),
        };
        assert!(validate_captured(&chord).is_ok());
    }

    #[test]
    fn capture_validation_rejects_left_modifier_single_key_but_accepts_modified_escape() {
        let left_control = CapturedShortcut {
            capture_id: 3,
            hook_generation: 1,
            binding: ShortcutBinding {
                modifiers: Vec::new(),
                trigger: PhysicalKeyId::new(0x1d, false),
            },
            label: "左 Ctrl".into(),
        };
        assert_eq!(
            validate_captured(&left_control).unwrap_err().0,
            INVALID_BINDING
        );

        let control_escape = CapturedShortcut {
            capture_id: 4,
            hook_generation: 1,
            binding: ShortcutBinding {
                modifiers: vec![crate::physical_shortcut::ModifierBinding {
                    kind: ModifierKind::Control,
                    side: crate::physical_shortcut::ModifierSide::Left,
                }],
                trigger: PhysicalKeyId::new(0x01, false),
            },
            label: "左 Ctrl+Escape".into(),
        };
        assert!(validate_captured(&control_escape).is_ok());
    }
}
