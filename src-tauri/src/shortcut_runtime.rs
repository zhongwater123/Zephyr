use crate::physical_shortcut::ShortcutBinding;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardEngineEvent {
    Pressed,
    Released,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyboardEngineErrorKind {
    DispatchUnavailable,
    HookWorkerUnavailable,
    ReinstallRequestFailed,
    ReinstallTimeout,
    ReinstallFailed,
    GenerationSuperseded,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyboardEngineError {
    pub(crate) kind: KeyboardEngineErrorKind,
    pub(crate) message: String,
}

impl KeyboardEngineError {
    pub(crate) fn new(kind: KeyboardEngineErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn unsupported(message: impl Into<String>) -> Self {
        Self::new(KeyboardEngineErrorKind::Unsupported, message)
    }
}

impl fmt::Display for KeyboardEngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for KeyboardEngineError {}

#[derive(Debug, Clone, Default)]
pub(crate) struct KeyboardEngineDiagnostics {
    pub hook_generation: u64,
    pub observed_events: u64,
    pub emitted_events: u64,
    pub dropped_events: u64,
    pub hook_healthy: bool,
    pub hook_worker_alive: bool,
    pub dispatch_alive: bool,
    pub enabled: bool,
}

pub(crate) type ShortcutEventHandler = Box<dyn Fn(KeyboardEngineEvent) + Send + 'static>;

pub(crate) trait ShortcutRuntimePort: Send + Sync {
    fn startup_error(&self) -> Option<String>;
    fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String>;
    fn set_enabled(&self, enabled: bool) -> Result<(), String>;
    fn ensure_runtime_ready(&self, force_reinstall: bool) -> Result<u64, KeyboardEngineError>;
    fn is_healthy(&self) -> bool;
    fn diagnostics(&self) -> KeyboardEngineDiagnostics;
    fn shutdown(&self);
}

pub(crate) trait ShortcutRuntimeFactory: Send + Sync {
    fn start(&self, on_event: ShortcutEventHandler)
        -> Result<Arc<dyn ShortcutRuntimePort>, String>;
}
