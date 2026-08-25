use crate::config::{AppConfig, CURRENT_SCHEMA_VERSION};
use crate::physical_shortcut::{ModifierKind, ShortcutBinding};
use crate::services::{AppServices, ConfigService, ConfigServiceError};
use crate::voice_controller::{SessionEvent, VoiceSessionController};
use crate::windows_keyboard::{CapturedShortcut, KeyboardEngineEvent, WindowsKeyboardEngine};
use crate::SharedRuntime;
use serde::Serialize;
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tauri::{AppHandle, Emitter};

const CAPTURE_EVENT: &str = "shortcut_capture_changed";
const STATUS_EVENT: &str = "shortcut_status_changed";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutRuntimeState { Active, Disabled, Capturing, Error }

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRuntimeStatus {
    pub shortcut: String,
    pub state: ShortcutRuntimeState,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCaptureSession { pub capture_id: u64 }

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutCaptureState { Capturing, Saved, Cancelled, Error }

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutCaptureEvent {
    pub capture_id: u64,
    pub state: ShortcutCaptureState,
    pub message: String,
    pub config: Option<AppConfig>,
    pub change_id: Option<u64>,
}

#[derive(Clone)]
struct CaptureTransaction { id: u64, expected_revision: u64 }

#[derive(Clone)]
struct UndoTransaction { id: u64, binding: ShortcutBinding, label: String, committed_revision: u64 }

struct ManagerState {
    binding: ShortcutBinding,
    label: String,
    enabled: bool,
    capture: Option<CaptureTransaction>,
    undo: Option<UndoTransaction>,
    next_id: u64,
    status: ShortcutRuntimeStatus,
}

pub struct ShortcutManager {
    app: AppHandle,
    runtime: SharedRuntime,
    config: Arc<ConfigService>,
    controller: VoiceSessionController,
    state: Mutex<ManagerState>,
    engine: Mutex<Option<WindowsKeyboardEngine>>,
}

impl ShortcutManager {
    pub fn initialize(app: &mut tauri::App, runtime: SharedRuntime, services: AppServices)
        -> tauri::Result<(VoiceSessionController, Arc<Self>)>
    {
        let controller = VoiceSessionController::new(runtime.clone(), services.clone());
        let config = services.config.snapshot();
        let binding = config.shortcut_binding.clone().unwrap_or_else(|| ShortcutBinding::default_physical());
        let label = config.shortcut.clone();
        let weak_slot: Arc<OnceLock<Weak<ShortcutManager>>> = Arc::new(OnceLock::new());
        let callback_slot = weak_slot.clone();
        let engine = WindowsKeyboardEngine::start(move |event| {
            if let Some(manager) = callback_slot.get().and_then(Weak::upgrade) {
                manager.handle_engine_event(event);
            }
        });
        let (engine, initial_status, initial_error) = match engine {
            Ok(engine) => {
                let state = if config.enabled { ShortcutRuntimeState::Active } else { ShortcutRuntimeState::Disabled };
                let message = if config.enabled { "物理快捷键已启用。" } else { "语音输入已关闭，快捷键全部放行。" };
                (Some(engine), ShortcutRuntimeStatus { shortcut: label.clone(), state, message: message.into() }, None)
            }
            Err(error) => (None, ShortcutRuntimeStatus { shortcut: label.clone(), state: ShortcutRuntimeState::Error, message: error.clone() }, Some(error)),
        };
        let manager = Arc::new(Self {
            app: app.handle().clone(), runtime: runtime.clone(), config: services.config.clone(), controller: controller.clone(),
            state: Mutex::new(ManagerState { binding: binding.clone(), label, enabled: config.enabled, capture: None, undo: None, next_id: 0, status: initial_status }),
            engine: Mutex::new(engine),
        });
        let _ = weak_slot.set(Arc::downgrade(&manager));
        if let Ok(guard) = manager.engine.lock() {
            if let Some(engine) = guard.as_ref() {
                if let Err(error) = engine.set_binding(Some(&binding)) { manager.set_error(error); }
                engine.set_enabled(config.enabled);
            }
        }
        if let Ok(mut voice) = runtime.lock() { voice.shortcut_registration_error = initial_error; }
        manager.emit_status();
        Ok((controller, manager))
    }

    pub fn status(&self) -> ShortcutRuntimeStatus {
        self.state.lock().map(|state| state.status.clone()).unwrap_or_else(|_| ShortcutRuntimeStatus {
            shortcut: String::new(), state: ShortcutRuntimeState::Error, message: "快捷键状态锁已损坏。".into(),
        })
    }

    pub fn start_capture(&self, expected_revision: u64) -> Result<ShortcutCaptureSession, String> {
        let current = self.config.snapshot();
        if current.revision != expected_revision { return Err("配置已被其他操作更新，请刷新后重试。".into()); }
        let capture_id = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.next_id = state.next_id.saturating_add(1).max(1);
            let id = state.next_id;
            state.capture = Some(CaptureTransaction { id, expected_revision });
            state.status = ShortcutRuntimeStatus { shortcut: state.label.clone(), state: ShortcutRuntimeState::Capturing, message: "请按下新的物理快捷键，松开后自动保存。".into() };
            id
        };
        let guard = self.engine.lock().map_err(|error| error.to_string())?;
        let engine = guard.as_ref().ok_or_else(|| "物理快捷键引擎未运行。".to_string())?;
        engine.start_capture(capture_id);
        self.emit_status();
        self.emit_capture(ShortcutCaptureEvent { capture_id, state: ShortcutCaptureState::Capturing, message: "等待输入。".into(), config: None, change_id: None });
        Ok(ShortcutCaptureSession { capture_id })
    }

    pub fn cancel_capture(&self, capture_id: Option<u64>) -> Result<(), String> {
        let cancelled = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            let Some(capture) = state.capture.as_ref() else { return Ok(()); };
            if capture_id.is_some() && capture_id != Some(capture.id) { return Ok(()); }
            let id = capture.id;
            state.capture = None;
            state.status = normal_status(&state);
            id
        };
        if let Ok(guard) = self.engine.lock() {
            if let Some(engine) = guard.as_ref() { engine.cancel_capture(Some(cancelled)); }
        }
        self.emit_status();
        self.emit_capture(ShortcutCaptureEvent { capture_id: cancelled, state: ShortcutCaptureState::Cancelled, message: "已取消快捷键录制。".into(), config: None, change_id: None });
        Ok(())
    }

    pub fn undo(&self, change_id: u64, expected_revision: u64) -> Result<AppConfig, String> {
        let transaction = self.state.lock().map_err(|error| error.to_string())?.undo.clone()
            .filter(|undo| undo.id == change_id && undo.committed_revision == expected_revision)
            .ok_or_else(|| "该快捷键变更已无法撤销。".to_string())?;
        let current = self.config.snapshot();
        if current.revision != expected_revision { return Err("配置已被其他操作更新，无法撤销。".into()); }
        let mut next = current;
        next.shortcut = transaction.label.clone();
        next.shortcut_binding = Some(transaction.binding.clone());
        next.schema_version = CURRENT_SCHEMA_VERSION;
        next.revision = next.revision.saturating_add(1);
        let committed = self.config.commit_config(expected_revision, next).map_err(config_error)?;
        if let Some(engine) = self.engine.lock().map_err(|error| error.to_string())?.as_ref() {
            engine.set_binding(Some(&transaction.binding))?;
        }
        {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.binding = transaction.binding;
            state.label = transaction.label;
            state.undo = None;
            state.status = normal_status(&state);
        }
        self.emit_status();
        Ok(committed)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut guard = self.engine.lock().map_err(|error| error.to_string())?;
        let engine = guard.as_mut().ok_or_else(|| "物理快捷键引擎未运行。".to_string())?;
        if enabled { engine.ensure_healthy()?; }
        engine.set_enabled(enabled);
        {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.enabled = enabled;
            if !enabled { state.capture = None; engine.cancel_capture(None); }
            state.status = normal_status(&state);
        }
        if let Ok(mut runtime) = self.runtime.lock() { runtime.shortcut_registration_error = None; }
        drop(guard);
        self.emit_status();
        Ok(())
    }

    pub fn resume(&self) {
        let result = self.engine.lock().map_err(|error| error.to_string()).and_then(|guard| {
            guard.as_ref().ok_or_else(|| "物理快捷键引擎未运行。".to_string())?.ensure_healthy()
        });
        if let Err(error) = result { self.set_error(error); }
    }

    pub fn shutdown(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            if let Some(mut engine) = engine.take() { engine.shutdown(); }
        }
    }

    fn handle_engine_event(&self, event: KeyboardEngineEvent) {
        match event {
            KeyboardEngineEvent::Pressed => self.controller.submit(&self.app, SessionEvent::Pressed),
            KeyboardEngineEvent::Released => self.controller.submit(&self.app, SessionEvent::Released),
            KeyboardEngineEvent::Captured(captured) => self.commit_capture(captured),
            KeyboardEngineEvent::ReinstallFailed(error) => self.set_error(error),
        }
    }

    fn commit_capture(&self, captured: CapturedShortcut) {
        let capture = self.state.lock().ok().and_then(|state| state.capture.clone())
            .filter(|capture| capture.id == captured.capture_id);
        let Some(capture) = capture else { return; };
        if let Err(error) = validate_captured(&captured) {
            self.finish_capture_error(captured.capture_id, error);
            return;
        }
        let current = self.config.snapshot();
        if current.revision != capture.expected_revision {
            self.finish_capture_error(captured.capture_id, "配置已被其他操作更新，请重新录制。".into());
            return;
        }
        let old_binding = current.shortcut_binding.clone().unwrap_or_else(ShortcutBinding::default_physical);
        let old_label = current.shortcut.clone();
        let mut next = current;
        next.shortcut = captured.label.clone();
        next.shortcut_binding = Some(captured.binding.clone());
        next.schema_version = CURRENT_SCHEMA_VERSION;
        next.revision = next.revision.saturating_add(1);
        let committed = match self.config.commit_config(capture.expected_revision, next) {
            Ok(config) => config,
            Err(error) => { self.finish_capture_error(captured.capture_id, config_error(error)); return; }
        };
        if let Err(error) = self.engine.lock().map_err(|error| error.to_string())
            .and_then(|guard| guard.as_ref().ok_or_else(|| "物理快捷键引擎未运行。".to_string())?.set_binding(Some(&captured.binding)))
        {
            self.finish_capture_error(captured.capture_id, error);
            return;
        }
        let change_id = {
            let mut state = match self.state.lock() { Ok(value) => value, Err(_) => return };
            state.next_id = state.next_id.saturating_add(1);
            let change_id = state.next_id;
            state.binding = captured.binding;
            state.label = captured.label.clone();
            state.capture = None;
            state.undo = Some(UndoTransaction { id: change_id, binding: old_binding, label: old_label, committed_revision: committed.revision });
            state.status = normal_status(&state);
            change_id
        };
        self.emit_status();
        self.emit_capture(ShortcutCaptureEvent { capture_id: captured.capture_id, state: ShortcutCaptureState::Saved, message: format!("快捷键 {} 已启用。", captured.label), config: Some(committed), change_id: Some(change_id) });
    }

    fn finish_capture_error(&self, capture_id: u64, message: String) {
        if let Ok(mut state) = self.state.lock() {
            if state.capture.as_ref().is_some_and(|capture| capture.id == capture_id) { state.capture = None; }
            state.status = ShortcutRuntimeStatus { shortcut: state.label.clone(), state: ShortcutRuntimeState::Error, message: message.clone() };
        }
        self.emit_status();
        self.emit_capture(ShortcutCaptureEvent { capture_id, state: ShortcutCaptureState::Error, message, config: None, change_id: None });
    }

    fn set_error(&self, message: String) {
        if let Ok(mut state) = self.state.lock() {
            state.status = ShortcutRuntimeStatus { shortcut: state.label.clone(), state: ShortcutRuntimeState::Error, message: message.clone() };
        }
        if let Ok(mut runtime) = self.runtime.lock() { runtime.shortcut_registration_error = Some(message); }
        self.emit_status();
    }

    fn emit_status(&self) { let _ = self.app.emit(STATUS_EVENT, self.status()); }
    fn emit_capture(&self, event: ShortcutCaptureEvent) { let _ = self.app.emit(CAPTURE_EVENT, event); }
}

fn normal_status(state: &ManagerState) -> ShortcutRuntimeStatus {
    if state.enabled {
        ShortcutRuntimeStatus { shortcut: state.label.clone(), state: ShortcutRuntimeState::Active, message: "物理快捷键已启用。".into() }
    } else {
        ShortcutRuntimeStatus { shortcut: state.label.clone(), state: ShortcutRuntimeState::Disabled, message: "语音输入已关闭，快捷键全部放行。".into() }
    }
}

fn validate_captured(captured: &CapturedShortcut) -> Result<(), String> {
    captured.binding.validate()?;
    if captured.label.ends_with("F12") { return Err("F12 由 Windows 调试器保留，不能作为语音快捷键。".into()); }
    let has_alt = captured.binding.modifiers.iter().any(|m| m.kind == ModifierKind::Alt);
    let has_ctrl = captured.binding.modifiers.iter().any(|m| m.kind == ModifierKind::Control);
    let has_shift = captured.binding.modifiers.iter().any(|m| m.kind == ModifierKind::Shift);
    if has_alt && !has_ctrl && !has_shift && captured.label.ends_with("Tab") { return Err("Alt+Tab 是系统切换窗口快捷键。".into()); }
    if has_ctrl && has_alt && captured.label.ends_with("Delete") { return Err("Ctrl+Alt+Delete 是系统安全快捷键。".into()); }
    Ok(())
}

fn config_error(error: ConfigServiceError) -> String {
    match error { ConfigServiceError::Conflict(_) => "配置已被其他操作更新，请刷新后重试。".into(), ConfigServiceError::Storage(error) => error.to_string() }
}
