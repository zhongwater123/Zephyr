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

use crate::config::ShortcutTriggerMode;
use crate::services::{AppServices, ConfigService};
use crate::voice_controller::VoiceSessionHandle;
use crate::voice_trigger::{
    ActivationId, BeginDecision, BeginReceipt, TriggerBehavior, VoiceActivation, VoiceCancelReason,
    VoiceTriggerPort,
};
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
    voice: VoiceSessionHandle,
    config: Arc<ConfigService>,
    activation: Mutex<ShortcutActivationState>,
    coordinator: Arc<EditCoordinator>,
}

#[derive(Debug, Default)]
enum ShortcutActivationState {
    #[default]
    Idle,
    Engaged {
        activation_id: ActivationId,
        mode: ShortcutTriggerMode,
    },
}

enum ShortcutActivationAction {
    Begin(VoiceActivation),
    Finish(ActivationId),
    Cancel(ActivationId),
}

impl ShortcutActivationState {
    fn apply(
        &mut self,
        event: KeyboardEngineEvent,
        configured_mode: ShortcutTriggerMode,
    ) -> Option<ShortcutActivationAction> {
        match (&self, event) {
            (Self::Idle, KeyboardEngineEvent::Pressed) => {
                let behavior = match configured_mode {
                    ShortcutTriggerMode::Hold => TriggerBehavior::PushToTalk,
                    ShortcutTriggerMode::Toggle => TriggerBehavior::PressToToggle,
                };
                let activation = VoiceActivation::shortcut_for(behavior);
                *self = Self::Engaged {
                    activation_id: activation.id.clone(),
                    mode: configured_mode,
                };
                Some(ShortcutActivationAction::Begin(activation))
            }
            (
                Self::Engaged {
                    activation_id,
                    mode: ShortcutTriggerMode::Hold,
                },
                KeyboardEngineEvent::Released,
            )
            | (
                Self::Engaged {
                    activation_id,
                    mode: ShortcutTriggerMode::Toggle,
                },
                KeyboardEngineEvent::Pressed,
            ) => {
                let activation_id = activation_id.clone();
                *self = Self::Idle;
                Some(ShortcutActivationAction::Finish(activation_id))
            }
            (Self::Engaged { activation_id, .. }, KeyboardEngineEvent::Interrupted) => {
                let activation_id = activation_id.clone();
                *self = Self::Idle;
                Some(ShortcutActivationAction::Cancel(activation_id))
            }
            _ => None,
        }
    }

    fn clear_if_matching(&mut self, activation_id: &ActivationId) {
        if matches!(
            self,
            Self::Engaged {
                activation_id: current,
                ..
            } if current == activation_id
        ) {
            *self = Self::Idle;
        }
    }

    fn is_engaged(&self) -> bool {
        matches!(self, Self::Engaged { .. })
    }
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
            trigger: Arc::new(voice.clone()),
            voice,
            config: services.config.clone(),
            activation: Mutex::new(ShortcutActivationState::Idle),
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
        if self.is_trigger_active() || self.voice.status_snapshot().session_active {
            return Err("本次语音结束后才可以修改快捷键。".to_string());
        }
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
        if !enabled {
            self.cancel_active(VoiceCancelReason::UserRequested);
        }
        self.coordinator.set_enabled(enabled)
    }

    pub fn resume(&self) {
        self.coordinator.resume();
    }

    pub fn shutdown(&self) {
        self.cancel_active(VoiceCancelReason::TriggerInterrupted);
        self.coordinator.shutdown();
    }

    pub fn is_trigger_active(&self) -> bool {
        self.activation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_engaged()
    }

    fn handle_engine_event(self: &Arc<Self>, event: KeyboardEngineEvent) {
        let mode = self.config.snapshot().shortcut_trigger_mode;
        let action = self
            .activation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .apply(event, mode);
        match action {
            Some(ShortcutActivationAction::Begin(activation)) => {
                let activation_id = activation.id.clone();
                match self.trigger.begin(activation) {
                    Ok(receipt) => self.observe_begin(activation_id, receipt),
                    Err(error) => {
                        log::warn!("shortcut begin submission failed: {error:?}");
                        self.clear_activation(&activation_id);
                    }
                }
            }
            Some(ShortcutActivationAction::Finish(activation_id)) => {
                let _ = self.trigger.finish(activation_id);
            }
            Some(ShortcutActivationAction::Cancel(activation_id)) => {
                let _ = self
                    .trigger
                    .cancel(activation_id, VoiceCancelReason::TriggerInterrupted);
            }
            None => {}
        }
        if event == KeyboardEngineEvent::Interrupted {
            self.coordinator.handle_hook_interrupted();
        }
    }

    fn observe_begin(self: &Arc<Self>, activation_id: ActivationId, receipt: BeginReceipt) {
        let manager = Arc::downgrade(self);
        tauri::async_runtime::spawn(async move {
            match receipt.wait().await {
                Ok(BeginDecision::Accepted(accepted)) => accepted.completion.wait().await,
                Ok(BeginDecision::Rejected { reason }) => {
                    log::info!(
                        "shortcut activation rejected: activation_id={}, reason={reason:?}",
                        activation_id
                    );
                }
                Err(error) => {
                    log::warn!(
                        "shortcut begin receipt failed: activation_id={}, error={error:?}",
                        activation_id
                    );
                }
            }
            if let Some(manager) = manager.upgrade() {
                manager.clear_activation(&activation_id);
            }
        });
    }

    fn clear_activation(&self, activation_id: &ActivationId) {
        self.activation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear_if_matching(activation_id);
    }

    fn cancel_active(&self, reason: VoiceCancelReason) {
        let action = self
            .activation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .apply(KeyboardEngineEvent::Interrupted, ShortcutTriggerMode::Hold);
        if let Some(ShortcutActivationAction::Cancel(activation_id)) = action {
            let _ = self.trigger.cancel(activation_id, reason);
        }
    }
}

#[cfg(test)]
mod trigger_mode_tests {
    use super::*;

    fn id_from(action: ShortcutActivationAction) -> ActivationId {
        match action {
            ShortcutActivationAction::Begin(activation) => activation.id,
            ShortcutActivationAction::Finish(id) | ShortcutActivationAction::Cancel(id) => id,
        }
    }

    #[test]
    fn hold_finishes_on_release_and_ignores_repeated_press() {
        let mut state = ShortcutActivationState::Idle;
        let begin = state
            .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Hold)
            .unwrap();
        let activation_id = id_from(begin);

        assert!(state
            .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Hold)
            .is_none());
        let finish = state
            .apply(KeyboardEngineEvent::Released, ShortcutTriggerMode::Hold)
            .unwrap();

        assert_eq!(id_from(finish), activation_id);
        assert!(!state.is_engaged());
    }

    #[test]
    fn toggle_ignores_release_and_finishes_on_second_press() {
        let mut state = ShortcutActivationState::Idle;
        let begin = state
            .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Toggle)
            .unwrap();
        let activation_id = id_from(begin);

        assert!(state
            .apply(KeyboardEngineEvent::Released, ShortcutTriggerMode::Hold)
            .is_none());
        let finish = state
            .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Hold)
            .unwrap();

        assert_eq!(id_from(finish), activation_id);
        assert!(!state.is_engaged());
    }

    #[test]
    fn active_activation_keeps_its_original_mode() {
        let mut state = ShortcutActivationState::Idle;
        state
            .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Toggle)
            .unwrap();

        assert!(state
            .apply(KeyboardEngineEvent::Released, ShortcutTriggerMode::Hold)
            .is_none());
        assert!(state.is_engaged());
    }

    #[test]
    fn interruption_cancels_and_stale_completion_cannot_clear_new_activation() {
        let mut state = ShortcutActivationState::Idle;
        let first = id_from(
            state
                .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Toggle)
                .unwrap(),
        );
        let cancelled = id_from(
            state
                .apply(
                    KeyboardEngineEvent::Interrupted,
                    ShortcutTriggerMode::Toggle,
                )
                .unwrap(),
        );
        assert_eq!(cancelled, first);

        let second = id_from(
            state
                .apply(KeyboardEngineEvent::Pressed, ShortcutTriggerMode::Toggle)
                .unwrap(),
        );
        state.clear_if_matching(&first);
        assert!(state.is_engaged());
        state.clear_if_matching(&second);
        assert!(!state.is_engaged());
    }
}
