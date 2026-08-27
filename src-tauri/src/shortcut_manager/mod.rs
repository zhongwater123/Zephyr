//! Tauri façade and production assembly for shortcut editing.

mod commit;
mod contract;
mod coordinator;
mod ports;
mod recovery;
mod runtime_lifecycle;
mod state;
mod telemetry;
mod validation;

use contract::ShortcutEditInterrupted;
pub use contract::{ShortcutEditOutcome, ShortcutEditSession, ShortcutEditTraceInput};
use coordinator::EditCoordinator;
use ports::{ShortcutObserverPort, ShortcutRuntimePort};

use crate::services::AppServices;
use crate::voice_controller::{SessionEvent, VoiceSessionController};
use crate::windows_keyboard::{KeyboardEngineEvent, WindowsKeyboardEngine};
use crate::SharedRuntime;
use std::sync::{Arc, OnceLock, Weak};
use tauri::{AppHandle, Emitter};

const INTERRUPTED_EVENT: &str = "shortcut_edit_interrupted";

struct TauriShortcutObserver {
    app: AppHandle,
    runtime: SharedRuntime,
}

impl ShortcutObserverPort for TauriShortcutObserver {
    fn publish_runtime_error(&self, message: Option<String>) {
        let payload = if let Ok(mut runtime) = self.runtime.lock() {
            runtime.shortcut_registration_error = message;
            Some(runtime.voice_state_payload())
        } else {
            None
        };
        if let Some(payload) = payload {
            let _ = self.app.emit("voice_state_changed", payload);
        }
    }

    fn emit_interrupted(&self, outcome: ShortcutEditOutcome) {
        let _ = self
            .app
            .emit(INTERRUPTED_EVENT, ShortcutEditInterrupted { outcome });
    }
}

pub struct ShortcutManager {
    app: AppHandle,
    controller: VoiceSessionController,
    coordinator: Arc<EditCoordinator>,
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
        let (engine, initial_error): (Option<Arc<dyn ShortcutRuntimePort>>, Option<String>) =
            match engine_result {
                Ok(engine) => {
                    let hook_error = engine.startup_error();
                    let binding_error = binding
                        .is_none()
                        .then(|| "旧快捷键无法映射为物理键，请重新设置。".to_string());
                    (Some(Arc::new(engine)), hook_error.or(binding_error))
                }
                Err(error) => (None, Some(error)),
            };
        let observer: Arc<dyn ShortcutObserverPort> = Arc::new(TauriShortcutObserver {
            app: app.handle().clone(),
            runtime,
        });
        let coordinator = Arc::new(EditCoordinator::new(
            services.config.clone(),
            engine,
            initial_error,
            observer,
        ));
        let manager = Arc::new(Self {
            app: app.handle().clone(),
            controller: controller.clone(),
            coordinator,
        });
        let _ = weak_slot.set(Arc::downgrade(&manager));
        manager.coordinator.initialize_runtime();
        Ok((controller, manager))
    }

    pub fn begin_edit(
        &self,
        trace_id: String,
        expected_revision: u64,
    ) -> Result<ShortcutEditSession, String> {
        self.coordinator.begin_edit(trace_id, expected_revision)
    }

    pub fn commit_edit(
        &self,
        trace_id: String,
        edit_id: u64,
        expected_revision: u64,
        binding: crate::physical_shortcut::ShortcutBinding,
    ) -> Result<ShortcutEditOutcome, String> {
        self.coordinator
            .commit_edit(trace_id, edit_id, expected_revision, binding)
    }

    pub fn cancel_edit(
        &self,
        trace_id: String,
        edit_id: u64,
    ) -> Result<ShortcutEditOutcome, String> {
        self.coordinator.cancel_edit(trace_id, edit_id)
    }

    pub fn record_trace(&self, input: ShortcutEditTraceInput) -> Result<(), String> {
        self.coordinator.record_trace(input)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        self.coordinator.set_enabled(enabled)
    }

    pub fn resume(&self) {
        self.coordinator.resume();
    }

    pub fn shutdown(&self) {
        self.coordinator.shutdown();
    }

    fn handle_engine_event(&self, event: KeyboardEngineEvent) {
        match event {
            KeyboardEngineEvent::Pressed => {
                self.controller.submit(&self.app, SessionEvent::Pressed)
            }
            KeyboardEngineEvent::Released => {
                self.controller.submit(&self.app, SessionEvent::Released)
            }
            KeyboardEngineEvent::Interrupted => self.coordinator.handle_hook_interrupted(),
        }
    }
}
