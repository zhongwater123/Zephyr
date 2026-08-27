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
use crate::voice_controller::VoiceSessionHandle;
use crate::voice_trigger::{ActivationId, VoiceActivation, VoiceCancelReason, VoiceTriggerPort};
use crate::windows_keyboard::{KeyboardEngineEvent, WindowsKeyboardEngine};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use tauri::{AppHandle, Emitter};

const INTERRUPTED_EVENT: &str = "shortcut_edit_interrupted";

struct TauriShortcutObserver {
    app: AppHandle,
    voice: VoiceSessionHandle,
}

impl ShortcutObserverPort for TauriShortcutObserver {
    fn publish_runtime_error(&self, message: Option<String>) {
        if let Err(error) = self.voice.set_shortcut_health(message) {
            log::warn!("failed to publish shortcut health: {error:?}");
        }
    }

    fn emit_interrupted(&self, outcome: ShortcutEditOutcome) {
        let _ = self
            .app
            .emit(INTERRUPTED_EVENT, ShortcutEditInterrupted { outcome });
    }
}

pub struct ShortcutManager {
    trigger: Arc<dyn VoiceTriggerPort>,
    active_activation: Mutex<Option<ActivationId>>,
    coordinator: Arc<EditCoordinator>,
}

impl ShortcutManager {
    pub fn initialize(
        app: &mut tauri::App,
        services: AppServices,
        voice: VoiceSessionHandle,
    ) -> tauri::Result<Arc<Self>> {
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
            voice: voice.clone(),
        });
        let coordinator = Arc::new(EditCoordinator::new(
            services.config.clone(),
            engine,
            initial_error,
            observer,
        ));
        let manager = Arc::new(Self {
            trigger: Arc::new(voice),
            active_activation: Mutex::new(None),
            coordinator,
        });
        let _ = weak_slot.set(Arc::downgrade(&manager));
        manager.coordinator.initialize_runtime();
        Ok(manager)
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
                let mut active = self
                    .active_activation
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if active.is_some() {
                    return;
                }
                let activation = VoiceActivation::shortcut();
                if self.trigger.begin(activation.clone()).is_ok() {
                    *active = Some(activation.id);
                }
            }
            KeyboardEngineEvent::Released => {
                let activation = self
                    .active_activation
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take();
                if let Some(activation_id) = activation {
                    let _ = self.trigger.finish(activation_id);
                }
            }
            KeyboardEngineEvent::Interrupted => {
                let activation = self
                    .active_activation
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .take();
                if let Some(activation_id) = activation {
                    let _ = self
                        .trigger
                        .cancel(activation_id, VoiceCancelReason::TriggerInterrupted);
                }
                self.coordinator.handle_hook_interrupted();
            }
        }
    }
}
