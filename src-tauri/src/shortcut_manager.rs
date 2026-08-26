//! Shortcut editing transactions and the runtime keyboard binding boundary.

use crate::config::{AppConfig, CURRENT_SCHEMA_VERSION};
use crate::physical_shortcut::{ModifierKind, PhysicalKeyId, ShortcutBinding};
use crate::services::{AppServices, ConfigService, ConfigServiceError};
use crate::voice_controller::{SessionEvent, VoiceSessionController};
use crate::windows_keyboard::{KeyboardEngineEvent, WindowsKeyboardEngine};
use crate::SharedRuntime;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

const INTERRUPTED_EVENT: &str = "shortcut_edit_interrupted";
const TRACE_TARGET: &str = "shortcut_edit_trace";
const INVALID_BINDING: &str = "invalid_binding";
const RESERVED_BINDING: &str = "reserved_binding";
const REVISION_CONFLICT: &str = "revision_conflict";
const HOOK_UNAVAILABLE: &str = "hook_unavailable";
const PERSISTENCE_FAILED: &str = "persistence_failed";
const HOOK_INTERRUPTED: &str = "hook_interrupted";
const RUNTIME_ROLLBACK_FAILED: &str = "runtime_rollback_failed";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutRuntimeState {
    Active,
    Suspended,
    Disabled,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEditSession {
    pub edit_id: u64,
    pub trace_id: String,
    pub config_revision: u64,
    pub active_label: String,
    pub active_binding: Option<ShortcutBinding>,
    pub runtime_state: ShortcutRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEditOutcome {
    pub success: bool,
    pub edit_id: u64,
    pub trace_id: String,
    pub config_revision: u64,
    pub active_label: String,
    pub active_binding: Option<ShortcutBinding>,
    pub runtime_state: ShortcutRuntimeState,
    pub changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutEditInterrupted {
    outcome: ShortcutEditOutcome,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutTraceEvent {
    UiCaptureStarted,
    DomKeydown,
    DomKeyup,
    CandidateRejected,
    CandidateFinalized,
    BeginAcknowledged,
    CommitDispatched,
    CommitCompleted,
    OptimisticRollback,
    CancelRequested,
    FocusLost,
    EditInterrupted,
}

impl ShortcutTraceEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::UiCaptureStarted => "ui_capture_started",
            Self::DomKeydown => "dom_keydown",
            Self::DomKeyup => "dom_keyup",
            Self::CandidateRejected => "candidate_rejected",
            Self::CandidateFinalized => "candidate_finalized",
            Self::BeginAcknowledged => "begin_acknowledged",
            Self::CommitDispatched => "commit_dispatched",
            Self::CommitCompleted => "commit_completed",
            Self::OptimisticRollback => "optimistic_rollback",
            Self::CancelRequested => "cancel_requested",
            Self::FocusLost => "focus_lost",
            Self::EditInterrupted => "edit_interrupted",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutEditTraceInput {
    pub trace_id: String,
    pub edit_id: Option<u64>,
    pub event_seq: u32,
    pub elapsed_ms: u64,
    pub event: ShortcutTraceEvent,
    pub code: Option<String>,
    pub key: Option<String>,
    pub location: Option<u8>,
    pub repeat: Option<bool>,
    pub ctrl: Option<bool>,
    pub alt: Option<bool>,
    pub shift: Option<bool>,
    pub meta: Option<bool>,
    pub alt_graph: Option<bool>,
    #[serde(default)]
    pub held_codes: Vec<String>,
    pub candidate_label: Option<String>,
    pub candidate_binding: Option<ShortcutBinding>,
    pub reason_code: Option<String>,
}

impl ShortcutEditTraceInput {
    pub fn validate(&self) -> Result<(), String> {
        if self.trace_id.is_empty()
            || self.trace_id.len() > 64
            || !self
                .trace_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err("快捷键 traceId 无效。".into());
        }
        for value in self
            .code
            .iter()
            .chain(self.key.iter())
            .chain(self.held_codes.iter())
        {
            if value.chars().count() > 64 {
                return Err("快捷键诊断字段过长。".into());
            }
        }
        if self.held_codes.len() > 8
            || self
                .candidate_binding
                .as_ref()
                .is_some_and(|binding| binding.modifiers.len() > 8)
            || self
                .candidate_label
                .as_ref()
                .is_some_and(|value| value.chars().count() > 128)
            || self
                .reason_code
                .as_ref()
                .is_some_and(|value| value.chars().count() > 64)
        {
            return Err("快捷键诊断载荷过大。".into());
        }
        Ok(())
    }
}

#[derive(Clone)]
struct ShortcutEditTransaction {
    edit_id: u64,
    trace_id: String,
    expected_revision: u64,
    started_at: Instant,
}

struct ManagerState {
    next_edit_id: u64,
    edit: Option<ShortcutEditTransaction>,
    runtime_error: Option<String>,
}

#[derive(Debug)]
struct EditFailure {
    code: &'static str,
    message: String,
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
        self.snapshot()
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

pub struct ShortcutManager {
    app: AppHandle,
    runtime: SharedRuntime,
    config: Arc<ConfigService>,
    controller: VoiceSessionController,
    operation_gate: Mutex<()>,
    state: Mutex<ManagerState>,
    engine: Mutex<Option<Arc<WindowsKeyboardEngine>>>,
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
                let hook_error = engine.startup_error();
                let binding_error = binding
                    .is_none()
                    .then(|| "旧快捷键无法映射为物理键，请重新设置。".to_string());
                (Some(Arc::new(engine)), hook_error.or(binding_error))
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
                next_edit_id: 0,
                edit: None,
                runtime_error: initial_error.clone(),
            }),
            engine: Mutex::new(engine),
        });
        let _ = weak_slot.set(Arc::downgrade(&manager));
        if let Ok(engine) = manager.engine_handle() {
            if let Err(error) = engine.set_binding(binding.as_ref()) {
                manager.set_runtime_error(Some(error));
            } else {
                engine.set_enabled(config.enabled && binding.is_some() && initial_error.is_none());
            }
        }
        manager.sync_voice_runtime_error();
        Ok((controller, manager))
    }

    pub fn begin_edit(
        &self,
        trace_id: String,
        expected_revision: u64,
    ) -> Result<ShortcutEditSession, String> {
        validate_trace_id(&trace_id)?;
        let _gate = self.operation_gate.lock().map_err(|error| error.to_string())?;
        let started = Instant::now();
        let current = self.config.snapshot();
        log::info!(
            target: TRACE_TARGET,
            "event=edit_begin_requested traceId={} editId=none expectedRevision={} currentRevision={} phase=begin enabled={}",
            trace_id,
            expected_revision,
            current.revision,
            current.enabled
        );

        let existing = self.current_edit()?;
        if let Some(existing) = existing.as_ref() {
            if existing.trace_id == trace_id
                && existing.expected_revision == expected_revision
                && current.revision == existing.expected_revision
            {
                let mut session = self.session_for(
                    &current,
                    existing.edit_id,
                    trace_id,
                    None,
                    "正在录入新的快捷键。",
                );
                session.config_revision = existing.expected_revision;
                return Ok(session);
            }
        }

        if current.revision != expected_revision {
            metrics::counter!("shortcut.operation.failed", "error_code" => REVISION_CONFLICT)
                .increment(1);
            log::warn!(
                target: TRACE_TARGET,
                "event=edit_begin_failed traceId={} editId=0 expectedRevision={} currentRevision={} phase=revision durationMs={} result=failed errorCode={}",
                trace_id,
                expected_revision,
                current.revision,
                started.elapsed().as_millis(),
                REVISION_CONFLICT
            );
            return Ok(self.session_for(
                &current,
                0,
                trace_id,
                Some(REVISION_CONFLICT),
                "配置已被其他操作更新，请刷新后重试。",
            ));
        }

        if existing.is_some() {
            self.interrupt_active_edit_locked(
                "superseded",
                "新的换绑会话中断了上一轮录入。",
            );
        }
        let engine = match self.engine_handle() {
            Ok(engine) => engine,
            Err(message) => {
                self.set_runtime_error(Some(message.clone()));
                self.log_engine(
                    "edit_begin_failed",
                    &trace_id,
                    0,
                    expected_revision,
                    current.revision,
                    "begin",
                    started.elapsed().as_millis(),
                    "failed",
                    HOOK_UNAVAILABLE,
                    "failed",
                    &current.shortcut,
                );
                return Ok(self.session_for(
                    &current,
                    0,
                    trace_id,
                    Some(HOOK_UNAVAILABLE),
                    &message,
                ));
            }
        };
        engine.set_enabled(false);
        let edit_id = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.next_edit_id = state.next_edit_id.saturating_add(1).max(1);
            let edit_id = state.next_edit_id;
            state.edit = Some(ShortcutEditTransaction {
                edit_id,
                trace_id: trace_id.clone(),
                expected_revision,
                started_at: Instant::now(),
            });
            edit_id
        };
        metrics::counter!("shortcut.operation.started", "kind" => "edit").increment(1);
        self.log_engine(
            "runtime_suspended",
            &trace_id,
            edit_id,
            expected_revision,
            current.revision,
            "begin",
            started.elapsed().as_millis(),
            "success",
            "none",
            "none",
            &current.shortcut,
        );
        self.log_engine(
            "edit_begin_completed",
            &trace_id,
            edit_id,
            expected_revision,
            current.revision,
            "begin",
            started.elapsed().as_millis(),
            "success",
            "none",
            "none",
            &current.shortcut,
        );
        Ok(self.session_for(
            &current,
            edit_id,
            trace_id,
            None,
            "正在录入新的快捷键。",
        ))
    }

    pub fn commit_edit(
        &self,
        trace_id: String,
        edit_id: u64,
        expected_revision: u64,
        binding: ShortcutBinding,
    ) -> Result<ShortcutEditOutcome, String> {
        validate_trace_id(&trace_id)?;
        let _gate = self.operation_gate.lock().map_err(|error| error.to_string())?;
        let started = Instant::now();
        let candidate_label = binding.display_label();
        let current = self.config.snapshot();
        log::info!(
            target: TRACE_TARGET,
            "event=commit_requested traceId={} editId={} expectedRevision={} currentRevision={} phase=commit candidateLabel={:?} enabled={}",
            trace_id,
            edit_id,
            expected_revision,
            current.revision,
            candidate_label,
            current.enabled
        );

        let Some(transaction) = self.current_edit()? else {
            return Ok(self.outcome_for(
                &current,
                false,
                edit_id,
                trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        };
        if transaction.edit_id != edit_id || transaction.trace_id != trace_id {
            return Ok(self.outcome_for(
                &current,
                false,
                edit_id,
                trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        }
        if transaction.expected_revision != expected_revision {
            return Ok(self.fail_active_edit_locked(
                &trace_id,
                edit_id,
                REVISION_CONFLICT,
                "换绑会话的配置版本不一致，请重新录入。",
                started,
                &candidate_label,
            ));
        }
        if current.revision != expected_revision {
            return Ok(self.fail_active_edit_locked(
                &trace_id,
                edit_id,
                REVISION_CONFLICT,
                "配置已被其他操作更新，请重新录入。",
                started,
                &candidate_label,
            ));
        }
        if let Err(failure) = validate_candidate(&binding) {
            log::warn!(
                target: TRACE_TARGET,
                "event=validation_failed traceId={} editId={} expectedRevision={} currentRevision={} phase=validation durationMs={} candidateLabel={:?} result=failed errorCode={} message={:?}",
                trace_id,
                edit_id,
                expected_revision,
                current.revision,
                started.elapsed().as_millis(),
                candidate_label,
                failure.code,
                failure.message
            );
            return Ok(self.fail_active_edit_locked(
                &trace_id,
                edit_id,
                failure.code,
                &failure.message,
                started,
                &candidate_label,
            ));
        }
        log::debug!(
            target: TRACE_TARGET,
            "event=validation_completed traceId={} editId={} phase=validation durationMs={} candidateLabel={:?}",
            trace_id,
            edit_id,
            started.elapsed().as_millis(),
            candidate_label
        );

        let unchanged = current
            .shortcut_binding
            .as_ref()
            .is_some_and(|active| active.physically_equivalent(&binding));
        let engine = match self.engine_handle() {
            Ok(engine) => engine,
            Err(message) => {
                return Ok(self.fail_active_edit_locked(
                    &trace_id,
                    edit_id,
                    HOOK_UNAVAILABLE,
                    &message,
                    started,
                    &candidate_label,
                ));
            }
        };

        if current.enabled {
            let hook_started = Instant::now();
            self.log_engine(
                "hook_reinstall_requested",
                &trace_id,
                edit_id,
                expected_revision,
                current.revision,
                "hook",
                hook_started.elapsed().as_millis(),
                "started",
                "none",
                "none",
                &candidate_label,
            );
            match engine.ensure_runtime_ready(true) {
                Ok(_) => self.log_engine(
                    "hook_reinstall_completed",
                    &trace_id,
                    edit_id,
                    expected_revision,
                    current.revision,
                    "hook",
                    hook_started.elapsed().as_millis(),
                    "success",
                    "none",
                    "none",
                    &candidate_label,
                ),
                Err(error) => {
                    log::error!(
                        target: TRACE_TARGET,
                        "event=hook_reinstall_failed_detail traceId={} editId={} phase=hook durationMs={} errorKind={:?} error={:?}",
                        trace_id,
                        edit_id,
                        hook_started.elapsed().as_millis(),
                        error.kind,
                        error.message
                    );
                    self.log_engine(
                        "hook_reinstall_failed",
                        &trace_id,
                        edit_id,
                        expected_revision,
                        current.revision,
                        "hook",
                        hook_started.elapsed().as_millis(),
                        "failed",
                        HOOK_UNAVAILABLE,
                        "pending",
                        &candidate_label,
                    );
                    return Ok(self.fail_active_edit_locked(
                        &trace_id,
                        edit_id,
                        HOOK_UNAVAILABLE,
                        &error.message,
                        started,
                        &candidate_label,
                    ));
                }
            }
        }

        engine.set_enabled(false);
        if let Err(message) = engine.set_binding(Some(&binding)) {
            return Ok(self.fail_active_edit_locked(
                &trace_id,
                edit_id,
                HOOK_UNAVAILABLE,
                &message,
                started,
                &candidate_label,
            ));
        }
        engine.set_enabled(current.enabled);
        self.log_engine(
            "runtime_binding_applied",
            &trace_id,
            edit_id,
            expected_revision,
            current.revision,
            "runtime_apply",
            started.elapsed().as_millis(),
            "success",
            "none",
            "none",
            &candidate_label,
        );

        if unchanged {
            self.finish_edit_success();
            let outcome = self.outcome_for(
                &current,
                true,
                edit_id,
                trace_id.clone(),
                false,
                None,
                "快捷键未变化。",
            );
            self.log_terminal(&outcome, "commit_completed", started, expected_revision, "none");
            return Ok(outcome);
        }

        let persistence_started = Instant::now();
        log::info!(
            target: TRACE_TARGET,
            "event=persistence_started traceId={} editId={} phase=persistence candidateLabel={:?}",
            trace_id,
            edit_id,
            candidate_label
        );
        let mut next = current.clone();
        next.shortcut = candidate_label.clone();
        next.shortcut_binding = Some(binding);
        next.schema_version = CURRENT_SCHEMA_VERSION;
        next.revision = next.revision.saturating_add(1);
        match self.config.commit_shortcut(expected_revision, next) {
            Ok(committed) => {
                self.finish_edit_success();
                log::info!(
                    target: TRACE_TARGET,
                    "event=persistence_completed traceId={} editId={} phase=persistence durationMs={} currentRevision={} result=success",
                    trace_id,
                    edit_id,
                    persistence_started.elapsed().as_millis(),
                    committed.revision
                );
                let outcome = self.outcome_for(
                    &committed,
                    true,
                    edit_id,
                    trace_id,
                    true,
                    None,
                    "快捷键已更新。",
                );
                self.log_terminal(&outcome, "commit_completed", started, expected_revision, "none");
                metrics::counter!("shortcut.operation.completed", "kind" => "edit").increment(1);
                Ok(outcome)
            }
            Err(error) => {
                let (code, message) = match error {
                    ShortcutStoreFailure::Conflict => (
                        REVISION_CONFLICT,
                        "配置已被其他操作更新，请重新录入。".to_string(),
                    ),
                    ShortcutStoreFailure::Storage(message) => (PERSISTENCE_FAILED, message),
                };
                log::warn!(
                    target: TRACE_TARGET,
                    "event=persistence_failed traceId={} editId={} phase=persistence durationMs={} result=failed errorCode={} message={:?}",
                    trace_id,
                    edit_id,
                    persistence_started.elapsed().as_millis(),
                    code,
                    message
                );
                Ok(self.fail_active_edit_locked(
                    &trace_id,
                    edit_id,
                    code,
                    &message,
                    started,
                    &candidate_label,
                ))
            }
        }
    }
    pub fn cancel_edit(
        &self,
        trace_id: String,
        edit_id: u64,
    ) -> Result<ShortcutEditOutcome, String> {
        validate_trace_id(&trace_id)?;
        let _gate = self.operation_gate.lock().map_err(|error| error.to_string())?;
        let started = Instant::now();
        let current = self.config.snapshot();
        log::info!(
            target: TRACE_TARGET,
            "event=cancel_requested traceId={} editId={} expectedRevision=none currentRevision={} phase=cancel",
            trace_id,
            edit_id,
            current.revision
        );
        let Some(transaction) = self.current_edit()? else {
            return Ok(self.outcome_for(
                &current,
                false,
                edit_id,
                trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        };
        if transaction.trace_id != trace_id
            || (edit_id != 0 && transaction.edit_id != edit_id)
        {
            return Ok(self.outcome_for(
                &current,
                false,
                edit_id,
                trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                "本轮快捷键录入已经结束。",
            ));
        }
        let edit_id = transaction.edit_id;
        self.take_edit(edit_id, &trace_id)?;
        match self.restore_authoritative_runtime(false) {
            Ok(restored) => {
                self.set_runtime_error(None);
                let outcome = self.outcome_for(
                    &restored,
                    true,
                    edit_id,
                    trace_id,
                    false,
                    None,
                    "已取消，原快捷键保持不变。",
                );
                self.log_terminal(
                    &outcome,
                    "edit_cancelled",
                    started,
                    transaction.expected_revision,
                    "success",
                );
                metrics::counter!("shortcut.operation.cancelled", "kind" => "edit").increment(1);
                Ok(outcome)
            }
            Err(message) => {
                self.set_runtime_error(Some(message.clone()));
                let outcome = self.outcome_for(
                    &self.config.snapshot(),
                    false,
                    edit_id,
                    trace_id,
                    false,
                    Some(RUNTIME_ROLLBACK_FAILED),
                    &format!("取消换绑后无法恢复原快捷键：{message}"),
                );
                self.log_terminal(
                    &outcome,
                    "rollback_failed",
                    started,
                    transaction.expected_revision,
                    "failed",
                );
                Ok(outcome)
            }
        }
    }

    pub fn record_trace(&self, input: ShortcutEditTraceInput) -> Result<(), String> {
        input.validate()?;
        log::debug!(
            target: TRACE_TARGET,
            "event=frontend_trace traceId={} editId={} eventSeq={} clientElapsedMs={} phase={} code={:?} key={:?} location={:?} repeat={:?} ctrl={:?} alt={:?} shift={:?} meta={:?} altGraph={:?} heldCodes={:?} candidateLabel={:?} candidateBinding={:?} reasonCode={:?}",
            input.trace_id,
            input.edit_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into()),
            input.event_seq,
            input.elapsed_ms,
            input.event.as_str(),
            input.code,
            input.key,
            input.location,
            input.repeat,
            input.ctrl,
            input.alt,
            input.shift,
            input.meta,
            input.alt_graph,
            input.held_codes,
            input.candidate_label,
            input.candidate_binding,
            input.reason_code,
        );
        Ok(())
    }

    pub fn set_enabled(&self, _enabled: bool) -> Result<(), String> {
        let _gate = self.operation_gate.lock().map_err(|error| error.to_string())?;
        self.interrupt_active_edit_locked(
            "enable_changed",
            "启用状态变化中断了快捷键录入。",
        );
        let current = self.config.snapshot();
        match self.restore_authoritative_runtime(current.enabled) {
            Ok(_) => {
                self.set_runtime_error(None);
                Ok(())
            }
            Err(message) => {
                self.set_runtime_error(Some(message.clone()));
                Err(message)
            }
        }
    }

    pub fn resume(&self) {
        let Ok(_gate) = self.operation_gate.lock() else {
            self.set_runtime_error(Some("快捷键操作门闩已损坏。".into()));
            return;
        };
        self.interrupt_active_edit_locked(
            "system_resume",
            "系统恢复中断了快捷键录入，请重新设置。",
        );
        match self.restore_authoritative_runtime(true) {
            Ok(_) => self.set_runtime_error(None),
            Err(message) => self.set_runtime_error(Some(message)),
        }
    }

    pub fn shutdown(&self) {
        let engine = {
            let Ok(_gate) = self.operation_gate.lock() else {
                return;
            };
            if let Ok(mut state) = self.state.lock() {
                state.edit = None;
            }
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
            KeyboardEngineEvent::Interrupted => self.handle_hook_interrupted(),
        }
    }

    fn handle_hook_interrupted(&self) {
        let Ok(_gate) = self.operation_gate.lock() else {
            self.set_runtime_error(Some("快捷键操作门闩已损坏。".into()));
            return;
        };
        if self
            .engine_handle()
            .is_ok_and(|engine| engine.is_healthy())
        {
            log::debug!(
                target: TRACE_TARGET,
                "event=hook_interruption_ignored reason=already_recovered"
            );
            return;
        }

        let message = "键盘 Hook 工作线程已退出；旧快捷键当前不可用，请重新设置或重新启用。";
        let transaction = self
            .state
            .lock()
            .ok()
            .and_then(|mut state| state.edit.take());
        self.set_runtime_error(Some(message.to_string()));
        log::error!(
            target: TRACE_TARGET,
            "event=hook_interruption_confirmed phase=runtime result=failed errorCode={} message={:?}",
            HOOK_INTERRUPTED,
            message
        );
        if let Some(transaction) = transaction {
            let outcome = self.outcome_for(
                &self.config.snapshot(),
                false,
                transaction.edit_id,
                transaction.trace_id,
                false,
                Some(HOOK_INTERRUPTED),
                message,
            );
            self.log_engine(
                "edit_interrupted",
                &outcome.trace_id,
                outcome.edit_id,
                transaction.expected_revision,
                outcome.config_revision,
                "interrupt",
                transaction.started_at.elapsed().as_millis(),
                "failed",
                HOOK_INTERRUPTED,
                "failed",
                &outcome.active_label,
            );
            let _ = self.app.emit(
                INTERRUPTED_EVENT,
                ShortcutEditInterrupted { outcome },
            );
        }
    }
    fn current_edit(&self) -> Result<Option<ShortcutEditTransaction>, String> {
        self.state
            .lock()
            .map(|state| state.edit.clone())
            .map_err(|error| error.to_string())
    }

    fn take_edit(&self, edit_id: u64, trace_id: &str) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|error| error.to_string())?;
        if state.edit.as_ref().is_some_and(|transaction| {
            transaction.edit_id == edit_id && transaction.trace_id == trace_id
        }) {
            state.edit = None;
        }
        Ok(())
    }

    fn finish_edit_success(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.edit = None;
            state.runtime_error = None;
        }
        self.sync_voice_runtime_error();
    }

    fn fail_active_edit_locked(
        &self,
        trace_id: &str,
        edit_id: u64,
        code: &'static str,
        message: &str,
        started: Instant,
        candidate_label: &str,
    ) -> ShortcutEditOutcome {
        let expected_revision = self
            .current_edit()
            .ok()
            .flatten()
            .filter(|transaction| {
                transaction.edit_id == edit_id && transaction.trace_id == trace_id
            })
            .map(|transaction| transaction.expected_revision)
            .unwrap_or_else(|| self.config.snapshot().revision);
        let _ = self.take_edit(edit_id, trace_id);
        log::warn!(
            target: TRACE_TARGET,
            "event=rollback_started traceId={} editId={} phase=rollback candidateLabel={:?} errorCode={} message={:?}",
            trace_id,
            edit_id,
            candidate_label,
            code,
            message
        );
        match self.restore_authoritative_runtime(false) {
            Ok(current) => {
                self.set_runtime_error(None);
                let outcome = self.outcome_for(
                    &current,
                    false,
                    edit_id,
                    trace_id.to_string(),
                    false,
                    Some(code),
                    message,
                );
                self.log_terminal(
                    &outcome,
                    "rollback_completed",
                    started,
                    expected_revision,
                    "success",
                );
                metrics::counter!("shortcut.operation.failed", "error_code" => code).increment(1);
                outcome
            }
            Err(rollback_error) => {
                let runtime_message =
                    format!("{message}；恢复原快捷键失败：{rollback_error}");
                self.set_runtime_error(Some(runtime_message.clone()));
                let outcome = self.outcome_for(
                    &self.config.snapshot(),
                    false,
                    edit_id,
                    trace_id.to_string(),
                    false,
                    Some(RUNTIME_ROLLBACK_FAILED),
                    &runtime_message,
                );
                self.log_terminal(
                    &outcome,
                    "rollback_failed",
                    started,
                    expected_revision,
                    "failed",
                );
                metrics::counter!(
                    "shortcut.operation.failed",
                    "error_code" => RUNTIME_ROLLBACK_FAILED
                )
                .increment(1);
                outcome
            }
        }
    }

    fn interrupt_active_edit_locked(&self, source: &str, message: &str) {
        let transaction = self.state.lock().ok().and_then(|mut state| state.edit.take());
        let Some(transaction) = transaction else {
            return;
        };
        let restore = self.restore_authoritative_runtime(false);
        let (code, final_message) = match restore {
            Ok(_) => {
                self.set_runtime_error(None);
                (HOOK_INTERRUPTED, message.to_string())
            }
            Err(error) => {
                let final_message = format!("{message}；恢复快捷键失败：{error}");
                self.set_runtime_error(Some(final_message.clone()));
                (RUNTIME_ROLLBACK_FAILED, final_message)
            }
        };
        let outcome = self.outcome_for(
            &self.config.snapshot(),
            false,
            transaction.edit_id,
            transaction.trace_id,
            false,
            Some(code),
            &final_message,
        );
        log::warn!(
            target: TRACE_TARGET,
            "event=edit_interrupted traceId={} editId={} expectedRevision={} currentRevision={} source={} phase=interrupt totalDurationMs={} result=failed errorCode={} message={:?}",
            outcome.trace_id,
            outcome.edit_id,
            transaction.expected_revision,
            outcome.config_revision,
            source,
            transaction.started_at.elapsed().as_millis(),
            code,
            final_message
        );
        let _ = self.app.emit(
            INTERRUPTED_EVENT,
            ShortcutEditInterrupted {
                outcome: outcome.clone(),
            },
        );
    }

    fn restore_authoritative_runtime(&self, force_reinstall: bool) -> Result<AppConfig, String> {
        let current = self.config.snapshot();
        let engine = self.engine_handle()?;
        engine.set_enabled(false);
        engine.set_binding(current.shortcut_binding.as_ref())?;
        if current.enabled {
            if current.shortcut_binding.is_none() {
                return Err("当前快捷键无法映射为物理按键，运行时未恢复。".to_string());
            }
            engine
                .ensure_runtime_ready(force_reinstall)
                .map_err(|error| error.message)?;
            engine.set_enabled(true);
        }
        Ok(current)
    }

    fn engine_handle(&self) -> Result<Arc<WindowsKeyboardEngine>, String> {
        self.engine
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
            .cloned()
            .ok_or_else(|| "物理快捷键引擎未运行。".to_string())
    }

    fn runtime_state(&self, config: &AppConfig) -> ShortcutRuntimeState {
        let Ok(state) = self.state.lock() else {
            return ShortcutRuntimeState::Error;
        };
        if state.runtime_error.is_some() {
            ShortcutRuntimeState::Error
        } else if !config.enabled {
            ShortcutRuntimeState::Disabled
        } else if state.edit.is_some() {
            ShortcutRuntimeState::Suspended
        } else {
            ShortcutRuntimeState::Active
        }
    }

    fn session_for(
        &self,
        config: &AppConfig,
        edit_id: u64,
        trace_id: String,
        error_code: Option<&str>,
        message: &str,
    ) -> ShortcutEditSession {
        ShortcutEditSession {
            edit_id,
            trace_id,
            config_revision: config.revision,
            active_label: config.shortcut.clone(),
            active_binding: config.shortcut_binding.clone(),
            runtime_state: self.runtime_state(config),
            error_code: error_code.map(str::to_string),
            message: message.to_string(),
        }
    }

    fn outcome_for(
        &self,
        config: &AppConfig,
        success: bool,
        edit_id: u64,
        trace_id: String,
        changed: bool,
        error_code: Option<&str>,
        message: &str,
    ) -> ShortcutEditOutcome {
        ShortcutEditOutcome {
            success,
            edit_id,
            trace_id,
            config_revision: config.revision,
            active_label: config.shortcut.clone(),
            active_binding: config.shortcut_binding.clone(),
            runtime_state: self.runtime_state(config),
            changed,
            error_code: error_code.map(str::to_string),
            message: message.to_string(),
        }
    }

    fn set_runtime_error(&self, message: Option<String>) {
        if message.is_some() {
            metrics::counter!("shortcut.runtime.error").increment(1);
        }
        if let Ok(mut state) = self.state.lock() {
            state.runtime_error = message;
        }
        self.sync_voice_runtime_error();
    }

    fn sync_voice_runtime_error(&self) {
        let runtime_error = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.runtime_error.clone());
        let payload = if let Ok(mut runtime) = self.runtime.lock() {
            runtime.shortcut_registration_error = runtime_error;
            Some(runtime.voice_state_payload())
        } else {
            None
        };
        if let Some(payload) = payload {
            let _ = self.app.emit("voice_state_changed", payload);
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn log_engine(
        &self,
        event: &str,
        trace_id: &str,
        edit_id: u64,
        expected_revision: u64,
        current_revision: u64,
        phase: &str,
        duration_ms: u128,
        result: &str,
        error_code: &str,
        rollback_result: &str,
        candidate_label: &str,
    ) {
        match self.engine_handle() {
            Ok(engine) => {
                let diagnostics = engine.diagnostics();
                let level = if error_code == RUNTIME_ROLLBACK_FAILED
                    || event == "hook_reinstall_failed"
                {
                    log::Level::Error
                } else if result == "failed" {
                    log::Level::Warn
                } else {
                    log::Level::Info
                };
                log::log!(
                    target: TRACE_TARGET,
                    level,
                    "event={} traceId={} editId={} expectedRevision={} currentRevision={} phase={} durationMs={} totalDurationMs={} hookGeneration={} observed={} emitted={} dropped={} hookHealthy={} hookWorkerAlive={} dispatchAlive={} enabled={} candidateLabel={:?} result={} errorCode={} rollbackResult={}",
                    event,
                    trace_id,
                    edit_id,
                    expected_revision,
                    current_revision,
                    phase,
                    duration_ms,
                    duration_ms,
                    diagnostics.hook_generation,
                    diagnostics.observed_events,
                    diagnostics.emitted_events,
                    diagnostics.dropped_events,
                    diagnostics.hook_healthy,
                    diagnostics.hook_worker_alive,
                    diagnostics.dispatch_alive,
                    diagnostics.enabled,
                    candidate_label,
                    result,
                    error_code,
                    rollback_result,
                );
            }
            Err(_) => {
                let level = if error_code == RUNTIME_ROLLBACK_FAILED || result == "failed" {
                    log::Level::Error
                } else {
                    log::Level::Info
                };
                log::log!(
                    target: TRACE_TARGET,
                    level,
                    "event={} traceId={} editId={} expectedRevision={} currentRevision={} phase={} durationMs={} totalDurationMs={} engine=missing candidateLabel={:?} result={} errorCode={} rollbackResult={}",
                    event,
                    trace_id,
                    edit_id,
                    expected_revision,
                    current_revision,
                    phase,
                    duration_ms,
                    duration_ms,
                    candidate_label,
                    result,
                    error_code,
                    rollback_result,
                );
            }
        }
    }

    fn log_terminal(
        &self,
        outcome: &ShortcutEditOutcome,
        event: &str,
        started: Instant,
        expected_revision: u64,
        rollback_result: &str,
    ) {
        self.log_engine(
            event,
            &outcome.trace_id,
            outcome.edit_id,
            expected_revision,
            outcome.config_revision,
            "terminal",
            started.elapsed().as_millis(),
            if outcome.success { "success" } else { "failed" },
            outcome.error_code.as_deref().unwrap_or("none"),
            rollback_result,
            &outcome.active_label,
        );
    }
}
fn validate_trace_id(trace_id: &str) -> Result<(), String> {
    if trace_id.is_empty()
        || trace_id.len() > 64
        || !trace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("快捷键 traceId 无效。".into());
    }
    Ok(())
}

fn validate_candidate(binding: &ShortcutBinding) -> Result<(), EditFailure> {
    binding.validate().map_err(|message| EditFailure {
        code: INVALID_BINDING,
        message,
    })?;
    let has = |kind| binding.modifiers.iter().any(|modifier| modifier.kind == kind);
    let trigger = binding.trigger;
    let reserved = if trigger == PhysicalKeyId::new(0x58, false) {
        Some("F12 由 Windows 调试器保留，不能作为语音快捷键。")
    } else if trigger == PhysicalKeyId::new(0x53, true)
        && has(ModifierKind::Control)
        && has(ModifierKind::Alt)
    {
        Some("Ctrl+Alt+Delete 是系统安全快捷键。")
    } else if trigger == PhysicalKeyId::new(0x01, false)
        && has(ModifierKind::Control)
        && has(ModifierKind::Shift)
    {
        Some("Ctrl+Shift+Escape 是系统任务管理器快捷键。")
    } else if trigger == PhysicalKeyId::new(0x0f, false) && has(ModifierKind::Alt) {
        Some("Alt+Tab 是系统切换窗口快捷键。")
    } else if trigger == PhysicalKeyId::new(0x3e, false) && has(ModifierKind::Alt) {
        Some("Alt+F4 是系统关闭窗口快捷键。")
    } else if trigger == PhysicalKeyId::new(0x26, false) && has(ModifierKind::Win) {
        Some("Win+L 是系统锁屏快捷键。")
    } else {
        None
    };
    if let Some(message) = reserved {
        return Err(EditFailure {
            code: RESERVED_BINDING,
            message: message.into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical_shortcut::{ModifierBinding, ModifierSide};

    fn binding(
        modifier: Option<ModifierKind>,
        trigger: PhysicalKeyId,
    ) -> ShortcutBinding {
        ShortcutBinding {
            modifiers: modifier
                .map(|kind| {
                    vec![ModifierBinding {
                        kind,
                        side: ModifierSide::Left,
                    }]
                })
                .unwrap_or_default(),
            trigger,
        }
    }

    #[test]
    fn reserved_windows_combinations_are_rejected() {
        for candidate in [
            binding(Some(ModifierKind::Alt), PhysicalKeyId::new(0x0f, false)),
            binding(Some(ModifierKind::Alt), PhysicalKeyId::new(0x3e, false)),
            binding(Some(ModifierKind::Win), PhysicalKeyId::new(0x26, false)),
            binding(None, PhysicalKeyId::new(0x58, false)),
        ] {
            assert_eq!(
                validate_candidate(&candidate).unwrap_err().code,
                RESERVED_BINDING
            );
        }
    }

    #[test]
    fn ordinary_copy_and_standalone_space_are_allowed() {
        assert!(validate_candidate(&binding(
            Some(ModifierKind::Control),
            PhysicalKeyId::new(0x2e, false),
        ))
        .is_ok());
        assert!(validate_candidate(&binding(
            None,
            PhysicalKeyId::new(0x39, false),
        ))
        .is_ok());
    }

    #[test]
    fn frontend_trace_is_bounded_but_preserves_raw_key_content() {
        let input = ShortcutEditTraceInput {
            trace_id: "123e4567-e89b-12d3-a456-426614174000".into(),
            edit_id: None,
            event_seq: 2,
            elapsed_ms: 14,
            event: ShortcutTraceEvent::DomKeydown,
            code: Some("KeyK".into()),
            key: Some("k".into()),
            location: Some(0),
            repeat: Some(false),
            ctrl: Some(true),
            alt: Some(false),
            shift: Some(false),
            meta: Some(false),
            alt_graph: Some(false),
            held_codes: vec!["ControlLeft".into()],
            candidate_label: Some("左 Ctrl+K".into()),
            candidate_binding: None,
            reason_code: None,
        };
        assert!(input.validate().is_ok());
        let mut oversized = input;
        oversized.key = Some("x".repeat(65));
        assert!(oversized.validate().is_err());
    }
}
