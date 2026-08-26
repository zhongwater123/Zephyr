use crate::physical_shortcut::{
    modifier_bit, modifier_bits_label, modifier_only_binding, modifiers_from_bits, CompiledBinding,
    PhysicalKeyId, ShortcutBinding, LEFT_ALT, LEFT_CTRL, LEFT_SHIFT, LEFT_WIN, RIGHT_ALT,
    RIGHT_CTRL, RIGHT_SHIFT, RIGHT_WIN,
};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const SELF_INJECTED_MARKER: usize = 0x4759_5459_5049_4E47u64 as usize;
const EVENT_QUEUE_CAPACITY: usize = 32;
const REINSTALL_MESSAGE: u32 = WM_APP + 0x481;
const QUIT_MESSAGE: u32 = WM_APP + 0x482;
const BINDING_VALID: u64 = 1 << 63;
const HOOK_GENERATION_TIMEOUT: Duration = Duration::from_secs(2);
const MODIFIER_ONLY_HOLD_MS: u32 = 200;

#[derive(Debug, Clone)]
pub struct CapturedShortcut {
    pub capture_id: u64,
    pub hook_generation: u64,
    pub binding: ShortcutBinding,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum KeyboardEngineEvent {
    Pressed,
    Released,
    CaptureProgress {
        capture_id: u64,
        hook_generation: u64,
        label: String,
        binding: Option<ShortcutBinding>,
    },
    CaptureCancelled {
        capture_id: u64,
        hook_generation: u64,
    },
    Captured(CapturedShortcut),
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Pressed,
    Released,
    CaptureProgress {
        capture_id: u64,
        hook_generation: u64,
        sequence: u64,
        modifiers: u8,
        key: Option<PhysicalKeyId>,
        vk: u32,
    },
    Captured {
        capture_id: u64,
        hook_generation: u64,
        modifiers: u8,
        key: PhysicalKeyId,
        vk: u32,
        modifier_only: bool,
    },
    CaptureCancelled {
        capture_id: u64,
        hook_generation: u64,
    },
    Shutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyboardEngineErrorKind {
    DispatchUnavailable,
    HookWorkerUnavailable,
    ReinstallRequestFailed,
    ReinstallTimeout,
    ReinstallFailed,
    StalePreparation,
    CaptureArmFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KeyboardEngineError {
    pub(crate) kind: KeyboardEngineErrorKind,
    pub(crate) message: String,
}

impl KeyboardEngineError {
    fn new(kind: KeyboardEngineErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub(crate) fn runtime_unavailable(&self) -> bool {
        !matches!(
            self.kind,
            KeyboardEngineErrorKind::StalePreparation | KeyboardEngineErrorKind::CaptureArmFailed
        )
    }
}

impl fmt::Display for KeyboardEngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for KeyboardEngineError {}

#[derive(Debug)]
pub(crate) struct PreparedCapture {
    pub(crate) hook_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureArmReceipt {
    pub(crate) operation_id: u64,
    pub(crate) hook_generation: u64,
}

#[derive(Debug, Clone, Default)]
struct HookInstallReceipt {
    generation: u64,
    error: Option<String>,
}

struct HookGlobals {
    enabled: AtomicBool,
    binding: AtomicU64,
    held_modifiers: AtomicU8,
    active_down: AtomicBool,
    consume_until_up: AtomicBool,
    desired_active: AtomicBool,
    capture_id: AtomicU64,
    capture_down: AtomicBool,
    capture_main_released: AtomicBool,
    capture_complete: AtomicBool,
    pending_capture_progress: AtomicBool,
    pending_capture_delivery: AtomicBool,
    pending_capture_cancel: AtomicU64,
    pending_capture_cancel_generation: AtomicU64,
    capture_key: AtomicU32,
    capture_generation: AtomicU64,
    cancelled_capture_key: AtomicU32,
    capture_modifiers: AtomicU8,
    capture_seen_modifiers: AtomicU8,
    capture_modifier_started_at: AtomicU32,
    capture_vk: AtomicU32,
    capture_modifier_only: AtomicBool,
    progress_candidate: AtomicU64,
    next_progress_sequence: AtomicU64,
    progress_sequence: AtomicU64,
    last_left_ctrl_time: AtomicU32,
    altgr_synthetic_ctrl: AtomicBool,
    abort_startup: AtomicBool,
    hook_thread_id: AtomicU32,
    healthy: AtomicBool,
    dispatch_alive: AtomicBool,
    next_hook_generation: AtomicU64,
    armed_hook_generation: AtomicU64,
    install_receipt: Mutex<HookInstallReceipt>,
    install_changed: Condvar,
    last_install_error: Mutex<Option<String>>,
    dropped_events: AtomicU64,
    capture_observed_events: AtomicU64,
    capture_emitted_events: AtomicU64,
    capture_dropped_events: AtomicU64,
    events: SyncSender<Signal>,
}

impl HookGlobals {
    fn new(events: SyncSender<Signal>) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            binding: AtomicU64::new(0),
            held_modifiers: AtomicU8::new(0),
            active_down: AtomicBool::new(false),
            consume_until_up: AtomicBool::new(false),
            desired_active: AtomicBool::new(false),
            capture_id: AtomicU64::new(0),
            capture_down: AtomicBool::new(false),
            capture_main_released: AtomicBool::new(false),
            capture_complete: AtomicBool::new(false),
            pending_capture_progress: AtomicBool::new(false),
            pending_capture_delivery: AtomicBool::new(false),
            pending_capture_cancel: AtomicU64::new(0),
            pending_capture_cancel_generation: AtomicU64::new(0),
            capture_key: AtomicU32::new(0),
            capture_generation: AtomicU64::new(0),
            cancelled_capture_key: AtomicU32::new(0),
            capture_modifiers: AtomicU8::new(0),
            capture_seen_modifiers: AtomicU8::new(0),
            capture_modifier_started_at: AtomicU32::new(0),
            capture_vk: AtomicU32::new(0),
            capture_modifier_only: AtomicBool::new(false),
            progress_candidate: AtomicU64::new(0),
            next_progress_sequence: AtomicU64::new(0),
            progress_sequence: AtomicU64::new(0),
            last_left_ctrl_time: AtomicU32::new(0),
            altgr_synthetic_ctrl: AtomicBool::new(false),
            abort_startup: AtomicBool::new(false),
            hook_thread_id: AtomicU32::new(0),
            healthy: AtomicBool::new(false),
            dispatch_alive: AtomicBool::new(false),
            next_hook_generation: AtomicU64::new(0),
            armed_hook_generation: AtomicU64::new(0),
            install_receipt: Mutex::new(HookInstallReceipt::default()),
            install_changed: Condvar::new(),
            last_install_error: Mutex::new(None),
            dropped_events: AtomicU64::new(0),
            capture_observed_events: AtomicU64::new(0),
            capture_emitted_events: AtomicU64::new(0),
            capture_dropped_events: AtomicU64::new(0),
            events,
        }
    }

    fn emit(&self, event: Signal) {
        let capture_progress = matches!(event, Signal::CaptureProgress { .. });
        let capture_event = matches!(
            event,
            Signal::CaptureProgress { .. }
                | Signal::Captured { .. }
                | Signal::CaptureCancelled { .. }
        );
        if capture_event {
            self.capture_emitted_events.fetch_add(1, Ordering::Relaxed);
        }
        if self.events.try_send(event).is_err() {
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
            if capture_progress {
                self.pending_capture_progress.store(true, Ordering::Release);
            }
            if capture_event {
                self.capture_dropped_events.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn release_active(&self) {
        self.consume_until_up.store(false, Ordering::Release);
        if self.active_down.swap(false, Ordering::AcqRel) {
            self.desired_active.store(false, Ordering::Release);
            self.emit(Signal::Released);
        }
    }

    fn next_install_generation(&self) -> u64 {
        self.next_hook_generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1)
            .max(1)
    }

    fn record_install_result(&self, generation: u64, result: Result<(), String>) {
        self.healthy.store(result.is_ok(), Ordering::Release);
        let error = result.err();
        if let Ok(mut error) = self.last_install_error.lock() {
            *error = error.clone();
        }
        if let Ok(mut receipt) = self.install_receipt.lock() {
            *receipt = HookInstallReceipt { generation, error };
            self.install_changed.notify_all();
        }
    }

    fn install_error(&self) -> String {
        self.last_install_error
            .lock()
            .ok()
            .and_then(|value| value.clone())
            .unwrap_or_else(|| "物理快捷键引擎当前不可用。".into())
    }

    fn consume_prepared_generation(&self, hook_generation: u64) -> Result<(), KeyboardEngineError> {
        let receipt = self
            .install_receipt
            .lock()
            .map_err(|error| {
                KeyboardEngineError::new(
                    KeyboardEngineErrorKind::CaptureArmFailed,
                    error.to_string(),
                )
            })?
            .clone();
        if receipt.generation != hook_generation || receipt.error.is_some() {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::StalePreparation,
                "快捷键捕获凭据已经过期，请重试。",
            ));
        }
        let previous_generation = self.armed_hook_generation.load(Ordering::Acquire);
        if hook_generation <= previous_generation
            || self
                .armed_hook_generation
                .compare_exchange(
                    previous_generation,
                    hook_generation,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_err()
        {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::StalePreparation,
                "快捷键捕获凭据已被使用。",
            ));
        }
        Ok(())
    }
}

static GLOBALS: OnceLock<Arc<HookGlobals>> = OnceLock::new();

pub struct WindowsKeyboardEngine {
    globals: Arc<HookGlobals>,
    dispatch: Mutex<Option<JoinHandle<()>>>,
    hook_thread: Mutex<Option<JoinHandle<()>>>,
}

pub struct CaptureDiagnostics {
    pub capture_id: u64,
    pub hook_generation: u64,
    pub observed_events: u64,
    pub emitted_events: u64,
    pub dropped_events: u64,
    pub hook_alive: bool,
    pub hook_worker_alive: bool,
    pub dispatch_alive: bool,
}

impl WindowsKeyboardEngine {
    pub fn start(on_event: impl Fn(KeyboardEngineEvent) + Send + 'static) -> Result<Self, String> {
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let globals = Arc::new(HookGlobals::new(event_tx));
        let dispatch_globals = globals.clone();
        globals.dispatch_alive.store(true, Ordering::Release);
        let dispatch = thread::Builder::new()
            .name("gy-shortcut-dispatch".into())
            .spawn(move || {
                let mut delivered_active = false;
                let mut delivered_progress_sequence = 0;
                while let Ok(signal) = event_rx.recv() {
                    match signal {
                        Signal::Pressed if !delivered_active => {
                            dispatch_engine_event(&on_event, KeyboardEngineEvent::Pressed);
                            delivered_active = true;
                        }
                        Signal::Released if delivered_active => {
                            dispatch_engine_event(&on_event, KeyboardEngineEvent::Released);
                            delivered_active = false;
                        }
                        Signal::CaptureProgress {
                            capture_id,
                            hook_generation,
                            sequence,
                            modifiers,
                            key,
                            vk,
                        } if sequence > delivered_progress_sequence => {
                            deliver_capture_progress(
                                &on_event,
                                capture_id,
                                hook_generation,
                                modifiers,
                                key,
                                vk,
                            );
                            delivered_progress_sequence = sequence;
                        }
                        Signal::Captured {
                            capture_id,
                            hook_generation,
                            modifiers,
                            key,
                            vk,
                            modifier_only,
                        } if dispatch_globals
                            .pending_capture_delivery
                            .swap(false, Ordering::AcqRel) =>
                        {
                            deliver_captured(
                                &on_event,
                                capture_id,
                                hook_generation,
                                modifiers,
                                key,
                                vk,
                                modifier_only,
                            );
                        }
                        Signal::CaptureCancelled {
                            capture_id,
                            hook_generation,
                        } if dispatch_globals
                            .pending_capture_cancel
                            .compare_exchange(capture_id, 0, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok() =>
                        {
                            dispatch_engine_event(
                                &on_event,
                                KeyboardEngineEvent::CaptureCancelled {
                                    capture_id,
                                    hook_generation,
                                },
                            );
                        }
                        Signal::Shutdown => break,
                        _ => {}
                    }
                    let desired = dispatch_globals.desired_active.load(Ordering::Acquire);
                    if desired != delivered_active {
                        metrics::counter!("shortcut.hook.state_reconciled").increment(1);
                        dispatch_engine_event(
                            &on_event,
                            if desired {
                                KeyboardEngineEvent::Pressed
                            } else {
                                KeyboardEngineEvent::Released
                            },
                        );
                        delivered_active = desired;
                    }
                    if dispatch_globals
                        .pending_capture_progress
                        .swap(false, Ordering::AcqRel)
                    {
                        let capture_id = dispatch_globals.capture_id.load(Ordering::Acquire);
                        if capture_id != 0 {
                            let sequence =
                                dispatch_globals.progress_sequence.load(Ordering::Acquire);
                            let (modifiers, key, vk) = unpack_capture_progress(
                                dispatch_globals.progress_candidate.load(Ordering::Acquire),
                            );
                            if sequence > delivered_progress_sequence {
                                metrics::counter!("shortcut.hook.capture_progress_reconciled")
                                    .increment(1);
                                deliver_capture_progress(
                                    &on_event,
                                    capture_id,
                                    dispatch_globals.capture_generation.load(Ordering::Acquire),
                                    modifiers,
                                    key,
                                    vk,
                                );
                                delivered_progress_sequence = sequence;
                            }
                        }
                    }
                    if dispatch_globals
                        .pending_capture_delivery
                        .swap(false, Ordering::AcqRel)
                    {
                        let capture_id = dispatch_globals.capture_id.load(Ordering::Acquire);
                        if capture_id != 0 {
                            metrics::counter!("shortcut.hook.capture_reconciled").increment(1);
                            deliver_captured(
                                &on_event,
                                capture_id,
                                dispatch_globals.capture_generation.load(Ordering::Acquire),
                                dispatch_globals.capture_modifiers.load(Ordering::Acquire),
                                PhysicalKeyId::from_packed(
                                    dispatch_globals.capture_key.load(Ordering::Acquire),
                                ),
                                dispatch_globals.capture_vk.load(Ordering::Acquire),
                                dispatch_globals
                                    .capture_modifier_only
                                    .load(Ordering::Acquire),
                            );
                        }
                    }
                    let cancelled_capture_id = dispatch_globals
                        .pending_capture_cancel
                        .swap(0, Ordering::AcqRel);
                    if cancelled_capture_id != 0 {
                        metrics::counter!("shortcut.hook.cancel_reconciled").increment(1);
                        dispatch_engine_event(
                            &on_event,
                            KeyboardEngineEvent::CaptureCancelled {
                                capture_id: cancelled_capture_id,
                                hook_generation: dispatch_globals
                                    .pending_capture_cancel_generation
                                    .load(Ordering::Acquire),
                            },
                        );
                    }
                }
                if delivered_active {
                    dispatch_engine_event(&on_event, KeyboardEngineEvent::Released);
                }
                dispatch_globals
                    .dispatch_alive
                    .store(false, Ordering::Release);
            })
            .map_err(|error| {
                globals.dispatch_alive.store(false, Ordering::Release);
                format!("无法启动快捷键分发线程：{error}")
            })?;
        if GLOBALS.set(globals.clone()).is_err() {
            let _ = globals.events.send(Signal::Shutdown);
            let _ = dispatch.join();
            return Err("物理快捷键引擎已初始化".into());
        }
        let engine = Self {
            globals,
            dispatch: Mutex::new(Some(dispatch)),
            hook_thread: Mutex::new(None),
        };
        if let Err(error) = engine.ensure_hook_worker() {
            log::error!("physical keyboard worker startup failed: {error}");
        }
        if engine.is_healthy() {
            metrics::counter!("shortcut.hook.installed").increment(1);
        } else {
            metrics::counter!("shortcut.hook.install_failed").increment(1);
        }
        Ok(engine)
    }

    pub fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
        self.globals.release_active();
        let packed = match binding {
            Some(value) => pack_binding(value.compile()?),
            None => 0,
        };
        self.globals.binding.store(packed, Ordering::Release);
        Ok(())
    }

    pub fn set_enabled(&self, enabled: bool) {
        if !enabled {
            self.globals.release_active();
        }
        self.globals.enabled.store(enabled, Ordering::Release);
    }

    pub(crate) fn startup_error(&self) -> Option<String> {
        (!self.is_healthy()).then(|| {
            if !self.is_dispatch_alive() {
                "快捷键事件分发线程未运行。".into()
            } else {
                self.globals.install_error()
            }
        })
    }

    pub(crate) fn prepare_capture(&self) -> Result<PreparedCapture, KeyboardEngineError> {
        let hook_generation = self.reinstall_generation()?;
        Ok(PreparedCapture { hook_generation })
    }

    pub(crate) fn arm_capture(
        &self,
        prepared: PreparedCapture,
        operation_id: u64,
    ) -> Result<CaptureArmReceipt, KeyboardEngineError> {
        self.ensure_dispatch_alive()?;
        if !self.is_hook_worker_alive() || !self.globals.healthy.load(Ordering::Acquire) {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::HookWorkerUnavailable,
                "物理快捷键 Hook 在捕获准备后已停止。",
            ));
        }
        self.globals
            .consume_prepared_generation(prepared.hook_generation)?;

        self.globals.release_active();
        let previous_capture = self.globals.capture_id.load(Ordering::Acquire);
        if previous_capture != 0 {
            self.cancel_capture(Some(previous_capture));
        }
        let held_modifiers = synchronize_modifier_bits(self.globals.as_ref());
        self.globals.capture_down.store(false, Ordering::Release);
        self.globals
            .capture_main_released
            .store(false, Ordering::Release);
        self.globals
            .capture_complete
            .store(false, Ordering::Release);
        self.globals
            .pending_capture_progress
            .store(false, Ordering::Release);
        self.globals
            .pending_capture_delivery
            .store(false, Ordering::Release);
        self.globals
            .pending_capture_cancel
            .store(0, Ordering::Release);
        self.globals
            .pending_capture_cancel_generation
            .store(0, Ordering::Release);
        self.globals.capture_key.store(0, Ordering::Release);
        self.globals
            .capture_generation
            .store(prepared.hook_generation, Ordering::Release);
        self.globals.capture_modifiers.store(0, Ordering::Release);
        self.globals
            .capture_seen_modifiers
            .store(held_modifiers, Ordering::Release);
        self.globals.capture_vk.store(0, Ordering::Release);
        self.globals
            .capture_modifier_started_at
            .store(0, Ordering::Release);
        self.globals
            .capture_modifier_only
            .store(false, Ordering::Release);
        self.globals.progress_candidate.store(
            pack_capture_progress(held_modifiers, None, 0),
            Ordering::Release,
        );
        self.globals
            .capture_observed_events
            .store(0, Ordering::Release);
        self.globals
            .capture_emitted_events
            .store(0, Ordering::Release);
        self.globals
            .capture_dropped_events
            .store(0, Ordering::Release);
        self.globals
            .capture_id
            .store(operation_id, Ordering::Release);
        if self.globals.capture_id.load(Ordering::Acquire) != operation_id
            || self.globals.capture_generation.load(Ordering::Acquire) != prepared.hook_generation
        {
            self.globals.capture_id.store(0, Ordering::Release);
            self.globals.capture_generation.store(0, Ordering::Release);
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::CaptureArmFailed,
                "无法进入物理快捷键捕获状态。",
            ));
        }
        if held_modifiers != 0 {
            emit_capture_progress(
                self.globals.as_ref(),
                operation_id,
                prepared.hook_generation,
                held_modifiers,
                None,
                0,
            );
        }
        Ok(CaptureArmReceipt {
            operation_id,
            hook_generation: prepared.hook_generation,
        })
    }

    pub(crate) fn retry_capture(
        &self,
        capture_id: u64,
        hook_generation: u64,
    ) -> Result<(), KeyboardEngineError> {
        self.ensure_dispatch_alive()?;
        if !self.is_hook_worker_alive() || !self.globals.healthy.load(Ordering::Acquire) {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::HookWorkerUnavailable,
                "物理快捷键 Hook 在重新录入前已停止。",
            ));
        }
        if self.globals.capture_id.load(Ordering::Acquire) != capture_id
            || self.globals.capture_generation.load(Ordering::Acquire) != hook_generation
            || !self.globals.capture_complete.load(Ordering::Acquire)
        {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::CaptureArmFailed,
                "快捷键候选已过期，无法继续当前录入。",
            ));
        }

        self.globals.release_active();
        let held_modifiers = synchronize_modifier_bits(self.globals.as_ref());
        self.globals.capture_down.store(false, Ordering::Release);
        self.globals
            .capture_main_released
            .store(false, Ordering::Release);
        self.globals
            .pending_capture_progress
            .store(false, Ordering::Release);
        self.globals
            .pending_capture_delivery
            .store(false, Ordering::Release);
        self.globals
            .pending_capture_cancel
            .store(0, Ordering::Release);
        self.globals
            .pending_capture_cancel_generation
            .store(0, Ordering::Release);
        self.globals.capture_key.store(0, Ordering::Release);
        self.globals.capture_modifiers.store(0, Ordering::Release);
        self.globals
            .capture_seen_modifiers
            .store(held_modifiers, Ordering::Release);
        self.globals.capture_vk.store(0, Ordering::Release);
        self.globals
            .capture_modifier_started_at
            .store(0, Ordering::Release);
        self.globals
            .capture_modifier_only
            .store(false, Ordering::Release);
        self.globals.progress_candidate.store(
            pack_capture_progress(held_modifiers, None, 0),
            Ordering::Release,
        );
        self.globals.capture_complete.store(false, Ordering::Release);

        if self.globals.capture_id.load(Ordering::Acquire) != capture_id
            || self.globals.capture_generation.load(Ordering::Acquire) != hook_generation
        {
            self.globals.capture_complete.store(true, Ordering::Release);
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::CaptureArmFailed,
                "快捷键捕获状态在重新录入时发生变化。",
            ));
        }
        if held_modifiers != 0 {
            emit_capture_progress(
                self.globals.as_ref(),
                capture_id,
                hook_generation,
                held_modifiers,
                None,
                0,
            );
        }
        Ok(())
    }

    pub fn cancel_capture(&self, capture_id: Option<u64>) {
        let current = self.globals.capture_id.load(Ordering::Acquire);
        let pending_cancel = self.globals.pending_capture_cancel.load(Ordering::Acquire);
        if capture_id.is_none()
            || capture_id == Some(current)
            || (current == 0 && capture_id == Some(pending_cancel))
        {
            if self.globals.capture_down.load(Ordering::Acquire) {
                self.globals.cancelled_capture_key.store(
                    self.globals.capture_key.load(Ordering::Acquire),
                    Ordering::Release,
                );
            }
            self.globals.capture_id.store(0, Ordering::Release);
            self.globals.capture_generation.store(0, Ordering::Release);
            self.globals.capture_down.store(false, Ordering::Release);
            self.globals
                .capture_main_released
                .store(false, Ordering::Release);
            self.globals
                .capture_complete
                .store(false, Ordering::Release);
            self.globals
                .pending_capture_progress
                .store(false, Ordering::Release);
            self.globals
                .pending_capture_delivery
                .store(false, Ordering::Release);
            self.globals
                .pending_capture_cancel
                .store(0, Ordering::Release);
            self.globals
                .pending_capture_cancel_generation
                .store(0, Ordering::Release);
            self.globals.capture_key.store(0, Ordering::Release);
            self.globals.capture_modifiers.store(0, Ordering::Release);
            self.globals
                .capture_seen_modifiers
                .store(0, Ordering::Release);
            self.globals.capture_vk.store(0, Ordering::Release);
            self.globals
                .capture_modifier_started_at
                .store(0, Ordering::Release);
            self.globals
                .capture_modifier_only
                .store(false, Ordering::Release);
            self.globals.progress_candidate.store(0, Ordering::Release);
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.globals.healthy.load(Ordering::Acquire)
            && self.is_hook_worker_alive()
            && self.is_dispatch_alive()
    }

    pub(crate) fn ensure_runtime_ready(
        &self,
        force_reinstall: bool,
    ) -> Result<u64, KeyboardEngineError> {
        if !force_reinstall && self.is_healthy() {
            return self
                .globals
                .install_receipt
                .lock()
                .map(|receipt| receipt.generation)
                .map_err(|error| {
                    KeyboardEngineError::new(
                        KeyboardEngineErrorKind::HookWorkerUnavailable,
                        error.to_string(),
                    )
                });
        }
        self.reinstall_generation()
    }

    pub fn capture_diagnostics(&self) -> CaptureDiagnostics {
        CaptureDiagnostics {
            capture_id: self.globals.capture_id.load(Ordering::Acquire),
            hook_generation: self.globals.capture_generation.load(Ordering::Acquire),
            observed_events: self.globals.capture_observed_events.load(Ordering::Relaxed),
            emitted_events: self.globals.capture_emitted_events.load(Ordering::Relaxed),
            dropped_events: self.globals.capture_dropped_events.load(Ordering::Relaxed),
            hook_alive: self.globals.healthy.load(Ordering::Acquire),
            hook_worker_alive: self.is_hook_worker_alive(),
            dispatch_alive: self.is_dispatch_alive(),
        }
    }

    fn reinstall_generation(&self) -> Result<u64, KeyboardEngineError> {
        self.ensure_dispatch_alive()?;
        self.ensure_hook_worker()?;
        let generation = self.globals.next_install_generation();
        self.globals.release_active();
        self.globals.healthy.store(false, Ordering::Release);
        let thread_id = self.globals.hook_thread_id.load(Ordering::Acquire);
        if thread_id == 0 {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::HookWorkerUnavailable,
                "物理快捷键 Hook 线程未就绪。",
            ));
        }
        unsafe {
            PostThreadMessageW(
                thread_id,
                REINSTALL_MESSAGE,
                WPARAM(generation as usize),
                LPARAM(0),
            )
        }
        .map_err(|error| {
            KeyboardEngineError::new(
                KeyboardEngineErrorKind::ReinstallRequestFailed,
                format!("无法请求重新安装键盘 Hook：{error}"),
            )
        })?;
        self.wait_for_install(generation)?;
        Ok(generation)
    }

    fn wait_for_install(&self, generation: u64) -> Result<(), KeyboardEngineError> {
        self.wait_for_install_timeout(generation, HOOK_GENERATION_TIMEOUT)
    }

    fn wait_for_install_timeout(
        &self,
        generation: u64,
        timeout_duration: Duration,
    ) -> Result<(), KeyboardEngineError> {
        let receipt = self.globals.install_receipt.lock().map_err(|error| {
            KeyboardEngineError::new(KeyboardEngineErrorKind::ReinstallFailed, error.to_string())
        })?;
        let (receipt, timeout) = self
            .globals
            .install_changed
            .wait_timeout_while(receipt, timeout_duration, |receipt| {
                receipt.generation < generation
            })
            .map_err(|error| {
                KeyboardEngineError::new(
                    KeyboardEngineErrorKind::ReinstallFailed,
                    error.to_string(),
                )
            })?;
        if timeout.timed_out() && receipt.generation < generation {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::ReinstallTimeout,
                "等待键盘 Hook 重装回执超时。",
            ));
        }
        if receipt.generation != generation {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::StalePreparation,
                "键盘 Hook 重装回执已被更新的请求替代。",
            ));
        }
        if let Some(error) = receipt.error.as_ref() {
            return Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::ReinstallFailed,
                error.clone(),
            ));
        }
        Ok(())
    }

    fn ensure_dispatch_alive(&self) -> Result<(), KeyboardEngineError> {
        if self.is_dispatch_alive() {
            Ok(())
        } else {
            Err(KeyboardEngineError::new(
                KeyboardEngineErrorKind::DispatchUnavailable,
                "快捷键事件分发线程已停止，请重启应用。",
            ))
        }
    }

    fn is_dispatch_alive(&self) -> bool {
        self.globals.dispatch_alive.load(Ordering::Acquire)
            && self
                .dispatch
                .lock()
                .ok()
                .and_then(|thread| thread.as_ref().map(|thread| !thread.is_finished()))
                .unwrap_or(false)
    }

    fn is_hook_worker_alive(&self) -> bool {
        self.globals.hook_thread_id.load(Ordering::Acquire) != 0
            && self
                .hook_thread
                .lock()
                .ok()
                .and_then(|thread| thread.as_ref().map(|thread| !thread.is_finished()))
                .unwrap_or(false)
    }

    fn ensure_hook_worker(&self) -> Result<(), KeyboardEngineError> {
        self.ensure_dispatch_alive()?;
        let mut worker = self.hook_thread.lock().map_err(|error| {
            KeyboardEngineError::new(
                KeyboardEngineErrorKind::HookWorkerUnavailable,
                error.to_string(),
            )
        })?;
        if worker.as_ref().is_some_and(|thread| thread.is_finished()) {
            if let Some(thread) = worker.take() {
                let _ = thread.join();
            }
            self.globals.hook_thread_id.store(0, Ordering::Release);
            self.globals.healthy.store(false, Ordering::Release);
        }
        if worker.is_some() {
            return (self.globals.hook_thread_id.load(Ordering::Acquire) != 0)
                .then_some(())
                .ok_or_else(|| {
                    KeyboardEngineError::new(
                        KeyboardEngineErrorKind::HookWorkerUnavailable,
                        "键盘 Hook 线程仍在启动，暂时无法捕获。",
                    )
                });
        }

        let startup_generation = self.globals.next_install_generation();
        self.globals.abort_startup.store(false, Ordering::Release);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("gy-physical-keyboard".into())
            .spawn(move || hook_thread_main(ready_tx, startup_generation))
            .map_err(|error| {
                let message = format!("无法启动键盘 Hook 线程：{error}");
                self.globals
                    .record_install_result(startup_generation, Err(message.clone()));
                KeyboardEngineError::new(KeyboardEngineErrorKind::HookWorkerUnavailable, message)
            })?;
        *worker = Some(thread);
        drop(worker);

        match ready_rx.recv_timeout(HOOK_GENERATION_TIMEOUT) {
            Ok(thread_id) if thread_id != 0 => Ok(()),
            _ => {
                self.globals.abort_startup.store(true, Ordering::Release);
                let thread_id = self.globals.hook_thread_id.load(Ordering::Acquire);
                if thread_id != 0 {
                    let _ = unsafe {
                        PostThreadMessageW(thread_id, QUIT_MESSAGE, WPARAM(0), LPARAM(0))
                    };
                }
                let error = "键盘 Hook 线程启动超时。".to_string();
                self.globals
                    .record_install_result(startup_generation, Err(error.clone()));
                Err(KeyboardEngineError::new(
                    KeyboardEngineErrorKind::HookWorkerUnavailable,
                    error,
                ))
            }
        }
    }

    pub fn shutdown(&self) {
        let hook_empty = self
            .hook_thread
            .lock()
            .map(|thread| thread.is_none())
            .unwrap_or(true);
        let dispatch_empty = self
            .dispatch
            .lock()
            .map(|thread| thread.is_none())
            .unwrap_or(true);
        if hook_empty && dispatch_empty {
            return;
        }
        self.globals.release_active();
        self.globals.enabled.store(false, Ordering::Release);
        self.globals.binding.store(0, Ordering::Release);
        self.cancel_capture(None);
        self.globals.healthy.store(false, Ordering::Release);
        let thread_id = self.globals.hook_thread_id.load(Ordering::Acquire);
        if thread_id != 0 {
            let _ = unsafe { PostThreadMessageW(thread_id, QUIT_MESSAGE, WPARAM(0), LPARAM(0)) };
        }
        let hook_thread = self
            .hook_thread
            .lock()
            .ok()
            .and_then(|mut thread| thread.take());
        if let Some(thread) = hook_thread {
            let _ = thread.join();
        }
        let _ = self.globals.events.send(Signal::Shutdown);
        let dispatch = self
            .dispatch
            .lock()
            .ok()
            .and_then(|mut thread| thread.take());
        if let Some(thread) = dispatch {
            let _ = thread.join();
        }
        self.globals.dispatch_alive.store(false, Ordering::Release);
        let dropped = self.globals.dropped_events.load(Ordering::Relaxed);
        if dropped > 0 {
            metrics::counter!("shortcut.hook.events_dropped").increment(dropped);
        }
        metrics::counter!("shortcut.hook.uninstalled").increment(1);
    }
}

impl Drop for WindowsKeyboardEngine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn deliver_capture_progress(
    on_event: &impl Fn(KeyboardEngineEvent),
    capture_id: u64,
    hook_generation: u64,
    modifiers: u8,
    key: Option<PhysicalKeyId>,
    vk: u32,
) {
    let modifier_bits = modifiers;
    let modifiers = modifiers_from_bits(modifier_bits);
    let (label, binding) = if let Some(key) = key {
        let binding = ShortcutBinding {
            modifiers,
            trigger: key,
        };
        (binding.label_with_trigger(&key_label(vk)), Some(binding))
    } else {
        (
            modifier_bits_label(modifier_bits),
            modifier_only_binding(modifier_bits),
        )
    };
    dispatch_engine_event(
        on_event,
        KeyboardEngineEvent::CaptureProgress {
            capture_id,
            hook_generation,
            label,
            binding,
        },
    );
}

fn deliver_captured(
    on_event: &impl Fn(KeyboardEngineEvent),
    capture_id: u64,
    hook_generation: u64,
    modifiers: u8,
    key: PhysicalKeyId,
    vk: u32,
    modifier_only: bool,
) {
    let binding = if modifier_only {
        let Some(binding) = modifier_only_binding(modifiers) else {
            return;
        };
        binding
    } else {
        ShortcutBinding {
            modifiers: modifiers_from_bits(modifiers),
            trigger: key,
        }
    };
    let label = if modifier_only {
        modifier_bits_label(modifiers)
    } else {
        binding.label_with_trigger(&key_label(vk))
    };
    dispatch_engine_event(
        on_event,
        KeyboardEngineEvent::Captured(CapturedShortcut {
            capture_id,
            hook_generation,
            binding,
            label,
        }),
    );
}

fn dispatch_engine_event(on_event: &impl Fn(KeyboardEngineEvent), event: KeyboardEngineEvent) {
    if catch_unwind(AssertUnwindSafe(|| on_event(event))).is_err() {
        metrics::counter!("shortcut.dispatch.callback_panicked").increment(1);
        log::error!("shortcut dispatch callback panicked; worker remains alive");
    }
}

fn pack_binding(binding: CompiledBinding) -> u64 {
    BINDING_VALID
        | binding.trigger.packed() as u64
        | ((binding.sided_modifiers as u64) << 17)
        | ((binding.any_modifiers as u64) << 25)
}

fn unpack_binding(value: u64) -> Option<CompiledBinding> {
    (value & BINDING_VALID != 0).then(|| CompiledBinding {
        trigger: PhysicalKeyId::from_packed(value as u32 & 0x1ffff),
        sided_modifiers: ((value >> 17) & 0xff) as u8,
        any_modifiers: ((value >> 25) & 0x0f) as u8,
        trigger_modifier: modifier_bit(PhysicalKeyId::from_packed(value as u32 & 0x1ffff))
            .unwrap_or(0),
    })
}

fn hook_thread_main(ready: SyncSender<u32>, startup_generation: u64) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
    if let Some(g) = GLOBALS.get() {
        g.hook_thread_id.store(thread_id, Ordering::Release);
    }
    let mut hook = match install_hook() {
        Ok(value) => {
            if let Some(g) = GLOBALS.get() {
                g.record_install_result(startup_generation, Ok(()));
            }
            let _ = ready.send(thread_id);
            Some(value)
        }
        Err(error) => {
            if let Some(g) = GLOBALS.get() {
                g.record_install_result(startup_generation, Err(error));
            }
            let _ = ready.send(thread_id);
            None
        }
    };
    if GLOBALS
        .get()
        .is_some_and(|globals| globals.abort_startup.load(Ordering::Acquire))
    {
        if let Some(value) = hook {
            let _ = unsafe { UnhookWindowsHookEx(value) };
        }
        if let Some(g) = GLOBALS.get() {
            g.healthy.store(false, Ordering::Release);
        }
        return;
    }
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 || message.message == QUIT_MESSAGE {
            break;
        }
        if message.message == REINSTALL_MESSAGE {
            let generation = message.wParam.0 as u64;
            if let Some(g) = GLOBALS.get() {
                g.release_active();
            }
            if let Some(current) = hook.take() {
                let _ = unsafe { UnhookWindowsHookEx(current) };
            }
            match install_hook() {
                Ok(next) => {
                    hook = Some(next);
                    if let Some(g) = GLOBALS.get() {
                        g.record_install_result(generation, Ok(()));
                    }
                    metrics::counter!("shortcut.hook.reinstalled").increment(1);
                }
                Err(error) => {
                    metrics::counter!("shortcut.hook.reinstall_failed").increment(1);
                    if let Some(g) = GLOBALS.get() {
                        g.record_install_result(generation, Err(error));
                    }
                }
            }
        }
    }
    if let Some(value) = hook {
        let _ = unsafe { UnhookWindowsHookEx(value) };
    }
    if let Some(g) = GLOBALS.get() {
        g.healthy.store(false, Ordering::Release);
        g.hook_thread_id.store(0, Ordering::Release);
    }
}

fn install_hook() -> Result<HHOOK, String> {
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|error| format!("无法取得应用模块句柄：{error}"))?;
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), HINSTANCE(module.0), 0) }
        .map_err(|error| format!("无法安装物理键盘钩子：{error}"))
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let Some(globals) = GLOBALS.get() else {
        return CallNextHookEx(None, code, wparam, lparam);
    };
    let input = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let message = wparam.0 as u32;
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if !down && !up {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let key = PhysicalKeyId::new(input.scanCode as u16, input.flags.contains(LLKHF_EXTENDED));
    if process_keyboard_event(
        globals,
        key,
        input.vkCode,
        down,
        up,
        input.time,
        input.dwExtraInfo,
    ) {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn process_keyboard_event(
    globals: &HookGlobals,
    key: PhysicalKeyId,
    vk: u32,
    down: bool,
    up: bool,
    time: u32,
    extra: usize,
) -> bool {
    if extra == SELF_INJECTED_MARKER {
        return false;
    }
    if globals.capture_id.load(Ordering::Acquire) != 0 {
        globals
            .capture_observed_events
            .fetch_add(1, Ordering::Relaxed);
    }
    if let Some(bit) = modifier_bit(key) {
        if down && bit == LEFT_CTRL {
            globals.last_left_ctrl_time.store(time, Ordering::Release);
        }
        let mut held = if down {
            globals.held_modifiers.fetch_or(bit, Ordering::AcqRel) | bit
        } else {
            globals.held_modifiers.fetch_and(!bit, Ordering::AcqRel) & !bit
        };
        if down
            && bit == RIGHT_ALT
            && held & LEFT_CTRL != 0
            && time.wrapping_sub(globals.last_left_ctrl_time.load(Ordering::Acquire)) <= 2
        {
            globals.altgr_synthetic_ctrl.store(true, Ordering::Release);
            held = globals
                .held_modifiers
                .fetch_and(!LEFT_CTRL, Ordering::AcqRel)
                & !LEFT_CTRL;
            globals
                .capture_seen_modifiers
                .fetch_and(!LEFT_CTRL, Ordering::AcqRel);
        }
        if up && bit == LEFT_CTRL && globals.altgr_synthetic_ctrl.swap(false, Ordering::AcqRel) {
            held = globals
                .held_modifiers
                .fetch_and(!LEFT_CTRL, Ordering::AcqRel)
                & !LEFT_CTRL;
        }
        let capture_id = globals.capture_id.load(Ordering::Acquire);
        let hook_generation = globals.capture_generation.load(Ordering::Acquire);
        if capture_id != 0 && !globals.capture_complete.load(Ordering::Acquire) {
            let main_selected = globals.capture_key.load(Ordering::Acquire) != 0;
            if down && !main_selected {
                let previous = globals
                    .capture_seen_modifiers
                    .fetch_or(bit, Ordering::AcqRel);
                let seen = previous | bit;
                if previous == 0 {
                    globals.capture_modifier_started_at.store(time, Ordering::Release);
                }
                emit_capture_progress(globals, capture_id, hook_generation, seen, None, 0);
            }
            if held == 0 {
                if globals.capture_main_released.load(Ordering::Acquire) {
                    complete_capture(globals, capture_id);
                } else if !main_selected {
                    let seen = globals.capture_seen_modifiers.load(Ordering::Acquire);
                    let started = globals.capture_modifier_started_at.load(Ordering::Acquire);
                    if seen != 0 && time.wrapping_sub(started) >= MODIFIER_ONLY_HOLD_MS {
                        complete_modifier_capture(globals, capture_id, seen);
                    } else if seen != 0 {
                        globals.capture_seen_modifiers.store(0, Ordering::Release);
                        globals.capture_modifier_started_at.store(0, Ordering::Release);
                        emit_capture_progress(globals, capture_id, hook_generation, 0, None, 0);
                    }
                }
            }
        }
        if up && globals.active_down.load(Ordering::Acquire) {
            if let Some(binding) = unpack_binding(globals.binding.load(Ordering::Acquire)) {
                if !binding.required_modifiers_still_held(held) {
                    globals.active_down.store(false, Ordering::Release);
                    globals.desired_active.store(false, Ordering::Release);
                    globals.emit(Signal::Released);
                }
            }
        }
        if capture_id == 0 && down && globals.enabled.load(Ordering::Acquire) {
            if let Some(binding) = unpack_binding(globals.binding.load(Ordering::Acquire)) {
                if binding.is_modifier_only()
                    && binding.matches_modifiers(held)
                    && !globals.active_down.swap(true, Ordering::AcqRel)
                {
                    globals.desired_active.store(true, Ordering::Release);
                    globals.emit(Signal::Pressed);
                }
            }
        }
        if capture_id != 0 {
            return true;
        }
        if globals.enabled.load(Ordering::Acquire)
            && bit & (LEFT_WIN | RIGHT_WIN) != 0
            && unpack_binding(globals.binding.load(Ordering::Acquire))
                .is_some_and(|binding| binding.includes_modifier_bit(bit))
        {
            return true;
        }
        return false;
    }

    let cancelled_capture_key = globals.cancelled_capture_key.load(Ordering::Acquire);
    if cancelled_capture_key != 0 && cancelled_capture_key == key.packed() {
        if up {
            let _ = globals.cancelled_capture_key.compare_exchange(
                cancelled_capture_key,
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
        return true;
    }

    let capture_id = globals.capture_id.load(Ordering::Acquire);
    if capture_id != 0 {
        let hook_generation = globals.capture_generation.load(Ordering::Acquire);
        if globals.capture_complete.load(Ordering::Acquire) {
            return false;
        }
        let held = globals.held_modifiers.load(Ordering::Acquire);
        if down
            && vk == VK_ESCAPE.0 as u32
            && held == 0
            && !globals.capture_down.load(Ordering::Acquire)
        {
            globals
                .cancelled_capture_key
                .store(key.packed(), Ordering::Release);
            globals.capture_id.store(0, Ordering::Release);
            globals
                .pending_capture_cancel
                .store(capture_id, Ordering::Release);
            globals
                .pending_capture_cancel_generation
                .store(hook_generation, Ordering::Release);
            globals.emit(Signal::CaptureCancelled {
                capture_id,
                hook_generation,
            });
            return true;
        }
        if down {
            if !globals.capture_down.swap(true, Ordering::AcqRel) {
                globals.capture_key.store(key.packed(), Ordering::Release);
                globals.capture_modifiers.store(held, Ordering::Release);
                globals.capture_vk.store(vk, Ordering::Release);
                globals
                    .capture_modifier_only
                    .store(false, Ordering::Release);
                emit_capture_progress(globals, capture_id, hook_generation, held, Some(key), vk);
            }
            if globals.capture_key.load(Ordering::Acquire) == key.packed() {
                return true;
            }
        }
        if up
            && globals.capture_down.load(Ordering::Acquire)
            && globals.capture_key.load(Ordering::Acquire) == key.packed()
        {
            globals.capture_down.store(false, Ordering::Release);
            globals.capture_main_released.store(true, Ordering::Release);
            if held == 0 {
                complete_capture(globals, capture_id);
            }
            return true;
        }
        return false;
    }

    if !globals.enabled.load(Ordering::Acquire) {
        return false;
    }
    let Some(binding) = unpack_binding(globals.binding.load(Ordering::Acquire)) else {
        return false;
    };
    if key != binding.trigger {
        return false;
    }
    if down {
        if globals.consume_until_up.load(Ordering::Acquire) {
            return true;
        }
        let held = globals.held_modifiers.load(Ordering::Acquire);
        if binding.matches_modifiers(held) {
            globals.consume_until_up.store(true, Ordering::Release);
            if !globals.active_down.swap(true, Ordering::AcqRel) {
                globals.desired_active.store(true, Ordering::Release);
                globals.emit(Signal::Pressed);
            }
            return true;
        }
    }
    if up && globals.consume_until_up.swap(false, Ordering::AcqRel) {
        if globals.active_down.swap(false, Ordering::AcqRel) {
            globals.desired_active.store(false, Ordering::Release);
            globals.emit(Signal::Released);
        }
        return true;
    }
    false
}

fn complete_capture(globals: &HookGlobals, capture_id: u64) {
    if globals.capture_complete.swap(true, Ordering::AcqRel) {
        return;
    }
    globals
        .pending_capture_delivery
        .store(true, Ordering::Release);
    globals.emit(Signal::Captured {
        capture_id,
        hook_generation: globals.capture_generation.load(Ordering::Acquire),
        modifiers: globals.capture_modifiers.load(Ordering::Acquire),
        key: PhysicalKeyId::from_packed(globals.capture_key.load(Ordering::Acquire)),
        vk: globals.capture_vk.load(Ordering::Acquire),
        modifier_only: false,
    });
}

fn complete_modifier_capture(globals: &HookGlobals, capture_id: u64, modifiers: u8) {
    let Some(binding) = modifier_only_binding(modifiers) else {
        return;
    };
    if globals.capture_complete.swap(true, Ordering::AcqRel) {
        return;
    }
    globals
        .capture_modifiers
        .store(modifiers, Ordering::Release);
    globals
        .capture_key
        .store(binding.trigger.packed(), Ordering::Release);
    globals.capture_vk.store(0, Ordering::Release);
    globals.capture_modifier_only.store(true, Ordering::Release);
    globals
        .pending_capture_delivery
        .store(true, Ordering::Release);
    globals.emit(Signal::Captured {
        capture_id,
        hook_generation: globals.capture_generation.load(Ordering::Acquire),
        modifiers,
        key: binding.trigger,
        vk: 0,
        modifier_only: true,
    });
}

fn emit_capture_progress(
    globals: &HookGlobals,
    capture_id: u64,
    hook_generation: u64,
    modifiers: u8,
    key: Option<PhysicalKeyId>,
    vk: u32,
) {
    let sequence = globals
        .next_progress_sequence
        .fetch_add(1, Ordering::AcqRel)
        .saturating_add(1)
        .max(1);
    globals
        .progress_candidate
        .store(pack_capture_progress(modifiers, key, vk), Ordering::Relaxed);
    globals.progress_sequence.store(sequence, Ordering::Release);
    globals.emit(Signal::CaptureProgress {
        capture_id,
        hook_generation,
        sequence,
        modifiers,
        key,
        vk,
    });
}

fn pack_capture_progress(modifiers: u8, key: Option<PhysicalKeyId>, vk: u32) -> u64 {
    modifiers as u64
        | ((key.map(PhysicalKeyId::packed).unwrap_or(0) as u64) << 8)
        | ((vk as u64) << 25)
}

fn unpack_capture_progress(value: u64) -> (u8, Option<PhysicalKeyId>, u32) {
    let modifiers = value as u8;
    let packed_key = ((value >> 8) & 0x1ffff) as u32;
    let key = (packed_key != 0).then(|| PhysicalKeyId::from_packed(packed_key));
    (modifiers, key, (value >> 25) as u32)
}

fn current_modifier_bits() -> u8 {
    modifier_bits_from_pressed(|key| {
        (unsafe { GetAsyncKeyState(key.0 as i32) }) as u16 & 0x8000 != 0
    })
}

fn synchronize_modifier_bits(globals: &HookGlobals) -> u8 {
    loop {
        let observed = globals.held_modifiers.load(Ordering::Acquire);
        let mut sampled = current_modifier_bits();
        if sampled & RIGHT_ALT == 0 {
            globals.altgr_synthetic_ctrl.store(false, Ordering::Release);
        } else if globals.altgr_synthetic_ctrl.load(Ordering::Acquire) {
            sampled &= !LEFT_CTRL;
        }
        if globals
            .held_modifiers
            .compare_exchange(observed, sampled, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            return sampled;
        }
    }
}

fn modifier_bits_from_pressed(mut pressed: impl FnMut(VIRTUAL_KEY) -> bool) -> u8 {
    [
        (VK_LCONTROL, LEFT_CTRL),
        (VK_RCONTROL, RIGHT_CTRL),
        (VK_LMENU, LEFT_ALT),
        (VK_RMENU, RIGHT_ALT),
        (VK_LSHIFT, LEFT_SHIFT),
        (VK_RSHIFT, RIGHT_SHIFT),
        (VK_LWIN, LEFT_WIN),
        (VK_RWIN, RIGHT_WIN),
    ]
    .into_iter()
    .fold(
        0,
        |bits, (key, bit)| {
            if pressed(key) {
                bits | bit
            } else {
                bits
            }
        },
    )
}

fn key_label(vk: u32) -> String {
    if (b'A' as u32..=b'Z' as u32).contains(&vk) || (b'0' as u32..=b'9' as u32).contains(&vk) {
        return char::from_u32(vk).unwrap_or('?').to_string();
    }
    match vk as u16 {
        value if value == VK_SPACE.0 => "Space".into(),
        value if value == VK_TAB.0 => "Tab".into(),
        value if value == VK_RETURN.0 => "Enter".into(),
        value if value == VK_ESCAPE.0 => "Escape".into(),
        value if value == VK_BACK.0 => "Backspace".into(),
        value if value == VK_DELETE.0 => "Delete".into(),
        value if value == VK_INSERT.0 => "Insert".into(),
        value if value == VK_HOME.0 => "Home".into(),
        value if value == VK_END.0 => "End".into(),
        value if value == VK_PRIOR.0 => "PageUp".into(),
        value if value == VK_NEXT.0 => "PageDown".into(),
        value if value == VK_UP.0 => "ArrowUp".into(),
        value if value == VK_DOWN.0 => "ArrowDown".into(),
        value if value == VK_LEFT.0 => "ArrowLeft".into(),
        value if value == VK_RIGHT.0 => "ArrowRight".into(),
        value if (VK_F1.0..=VK_F24.0).contains(&value) => format!("F{}", value - VK_F1.0 + 1),
        _ => format!("ScanCode {:02X}", vk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn globals() -> (HookGlobals, mpsc::Receiver<Signal>) {
        let (tx, rx) = mpsc::sync_channel(16);
        (HookGlobals::new(tx), rx)
    }

    impl HookGlobals {
        fn start_capture_for_test(&self, capture_id: u64) {
            self.capture_generation.store(1, Ordering::Release);
            self.armed_hook_generation.store(1, Ordering::Release);
            self.capture_id.store(capture_id, Ordering::Release);
            self.capture_complete.store(false, Ordering::Release);
            self.capture_main_released.store(false, Ordering::Release);
            self.capture_down.store(false, Ordering::Release);
            self.capture_key.store(0, Ordering::Release);
            self.capture_modifiers.store(0, Ordering::Release);
            self.capture_seen_modifiers.store(0, Ordering::Release);
            self.capture_vk.store(0, Ordering::Release);
            self.capture_modifier_started_at
                .store(0, Ordering::Release);
            self.capture_modifier_only.store(false, Ordering::Release);
        }
    }

    #[test]
    fn prepared_generation_is_current_and_single_use() {
        let (g, _) = globals();
        g.record_install_result(1, Err("initial install failed".into()));
        assert_eq!(
            g.consume_prepared_generation(1).unwrap_err().kind,
            KeyboardEngineErrorKind::StalePreparation
        );
        g.record_install_result(2, Ok(()));
        assert_eq!(
            g.consume_prepared_generation(1).unwrap_err().kind,
            KeyboardEngineErrorKind::StalePreparation
        );
        assert!(g.consume_prepared_generation(2).is_ok());
        assert_eq!(
            g.consume_prepared_generation(2).unwrap_err().kind,
            KeyboardEngineErrorKind::StalePreparation
        );
    }

    #[test]
    fn install_receipt_distinguishes_timeout_and_install_failure() {
        let (g, _) = globals();
        let engine = WindowsKeyboardEngine {
            globals: Arc::new(g),
            dispatch: Mutex::new(None),
            hook_thread: Mutex::new(None),
        };
        assert_eq!(
            engine
                .wait_for_install_timeout(1, Duration::from_millis(1))
                .unwrap_err()
                .kind,
            KeyboardEngineErrorKind::ReinstallTimeout
        );
        engine
            .globals
            .record_install_result(2, Err("install failed".into()));
        assert_eq!(
            engine
                .wait_for_install_timeout(2, Duration::from_millis(1))
                .unwrap_err()
                .kind,
            KeyboardEngineErrorKind::ReinstallFailed
        );
    }

    #[test]
    fn full_queue_coalesces_the_latest_capture_progress_snapshot() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let g = HookGlobals::new(tx);
        g.emit(Signal::Pressed);
        emit_capture_progress(&g, 7, 3, LEFT_CTRL, None, 0);
        let key = PhysicalKeyId::new(0x39, false);
        emit_capture_progress(&g, 7, 3, LEFT_CTRL | RIGHT_SHIFT, Some(key), 0x20);
        assert!(g.pending_capture_progress.load(Ordering::Acquire));
        assert_eq!(
            unpack_capture_progress(g.progress_candidate.load(Ordering::Acquire)),
            (LEFT_CTRL | RIGHT_SHIFT, Some(key), 0x20)
        );
        assert_eq!(g.capture_dropped_events.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn modifier_state_sync_preserves_left_and_right_sides() {
        let bits = modifier_bits_from_pressed(|key| {
            key == VK_LCONTROL || key == VK_RSHIFT || key == VK_RMENU
        });
        assert_eq!(bits, LEFT_CTRL | RIGHT_SHIFT | RIGHT_ALT);
    }

    #[test]
    fn exact_left_binding_consumes_only_matching_main_key() {
        let (g, rx) = globals();
        g.binding.store(
            pack_binding(ShortcutBinding::default_physical().compile().unwrap()),
            Ordering::Release,
        );
        g.enabled.store(true, Ordering::Release);
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, false),
            VK_LCONTROL.0 as u32,
            true,
            false,
            1,
            0
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x2a, false),
            VK_LSHIFT.0 as u32,
            true,
            false,
            2,
            0
        ));
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            3,
            0
        ));
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            4,
            0
        ));
        assert!(rx.try_recv().is_err());
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            false,
            true,
            5,
            0
        ));
        assert!(matches!(rx.try_recv(), Ok(Signal::Released)));
    }

    #[test]
    fn right_side_and_extra_modifiers_do_not_match_left_binding() {
        let (g, rx) = globals();
        g.binding.store(
            pack_binding(ShortcutBinding::default_physical().compile().unwrap()),
            Ordering::Release,
        );
        g.enabled.store(true, Ordering::Release);
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, true),
            VK_RCONTROL.0 as u32,
            true,
            false,
            1,
            0,
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x2a, false),
            VK_LSHIFT.0 as u32,
            true,
            false,
            2,
            0,
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            3,
            0,
        ));
        assert!(rx.try_recv().is_err());

        g.held_modifiers.store(0, Ordering::Release);
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, false),
            VK_LCONTROL.0 as u32,
            true,
            false,
            4,
            0,
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x2a, false),
            VK_LSHIFT.0 as u32,
            true,
            false,
            5,
            0,
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x38, false),
            VK_LMENU.0 as u32,
            true,
            false,
            6,
            0,
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            7,
            0,
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn early_modifier_release_releases_once_but_main_key_up_stays_consumed() {
        let (g, rx) = globals();
        g.binding.store(
            pack_binding(ShortcutBinding::default_physical().compile().unwrap()),
            Ordering::Release,
        );
        g.enabled.store(true, Ordering::Release);
        process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, false),
            VK_LCONTROL.0 as u32,
            true,
            false,
            1,
            0,
        );
        process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x2a, false),
            VK_LSHIFT.0 as u32,
            true,
            false,
            2,
            0,
        );
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            3,
            0
        ));
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x2a, false),
            VK_LSHIFT.0 as u32,
            false,
            true,
            4,
            0
        ));
        assert!(matches!(rx.try_recv(), Ok(Signal::Released)));
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            false,
            true,
            5,
            0
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn disabled_engine_passes_every_key() {
        let (g, rx) = globals();
        g.binding.store(
            pack_binding(ShortcutBinding::default_physical().compile().unwrap()),
            Ordering::Release,
        );
        g.held_modifiers.store(
            LEFT_CTRL | crate::physical_shortcut::LEFT_SHIFT,
            Ordering::Release,
        );
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            1,
            0
        ));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn cancelled_capture_consumes_repeat_and_key_up_without_triggering_voice() {
        let (g, rx) = globals();
        let key = PhysicalKeyId::new(0x39, false);
        g.cancelled_capture_key
            .store(key.packed(), Ordering::Release);
        assert!(process_keyboard_event(
            &g,
            key,
            VK_SPACE.0 as u32,
            true,
            false,
            1,
            0
        ));
        assert!(process_keyboard_event(
            &g,
            key,
            VK_SPACE.0 as u32,
            false,
            true,
            2,
            0
        ));
        assert_eq!(g.cancelled_capture_key.load(Ordering::Acquire), 0);
        assert!(rx.try_recv().is_err());
        assert!(!process_keyboard_event(
            &g,
            key,
            VK_SPACE.0 as u32,
            false,
            true,
            3,
            0
        ));
    }
    #[test]
    fn capture_waits_until_every_involved_key_is_released() {
        let (g, rx) = globals();
        g.start_capture_for_test(7);
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, false),
            VK_LCONTROL.0 as u32,
            true,
            false,
            1,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 7,
                key: None,
                ..
            })
        ));
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            2,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 7,
                key: Some(_),
                ..
            })
        ));
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            false,
            true,
            3,
            0
        ));
        assert!(rx.try_recv().is_err());
        assert!(process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, false),
            VK_LCONTROL.0 as u32,
            false,
            true,
            4,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::Captured { capture_id: 7, .. })
        ));
    }

    #[test]
    fn unmodified_main_key_is_reported_as_candidate_then_completed() {
        let (g, rx) = globals();
        let key = PhysicalKeyId::new(0x2e, false);
        g.start_capture_for_test(8);
        assert!(process_keyboard_event(
            &g,
            key,
            b'C' as u32,
            true,
            false,
            1,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 8,
                modifiers: 0,
                key: Some(_),
                ..
            })
        ));
        assert!(process_keyboard_event(
            &g,
            key,
            b'C' as u32,
            false,
            true,
            2,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::Captured {
                capture_id: 8,
                modifiers: 0,
                ..
            })
        ));
    }

    #[test]
    fn unmodified_escape_requests_authoritative_cancellation() {
        let (g, rx) = globals();
        let escape = PhysicalKeyId::new(0x01, false);
        g.start_capture_for_test(9);
        assert!(process_keyboard_event(
            &g,
            escape,
            VK_ESCAPE.0 as u32,
            true,
            false,
            1,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureCancelled { capture_id: 9, .. })
        ));
        assert_eq!(g.capture_id.load(Ordering::Acquire), 0);
        assert!(process_keyboard_event(
            &g,
            escape,
            VK_ESCAPE.0 as u32,
            false,
            true,
            2,
            0
        ));
    }

    #[test]
    fn right_control_alone_is_reported_then_completed_as_modifier_only() {
        let (g, rx) = globals();
        let right_control = PhysicalKeyId::new(0x1d, true);
        g.start_capture_for_test(12);
        assert!(process_keyboard_event(
            &g,
            right_control,
            VK_RCONTROL.0 as u32,
            true,
            false,
            1,
            0,
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 12,
                modifiers: crate::physical_shortcut::RIGHT_CTRL,
                key: None,
                ..
            })
        ));
        assert!(process_keyboard_event(
            &g,
            right_control,
            VK_RCONTROL.0 as u32,
            false,
            true,
            201,
            0,
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::Captured {
                capture_id: 12,
                modifiers: crate::physical_shortcut::RIGHT_CTRL,
                modifier_only: true,
                ..
            })
        ));
    }

    #[test]
    fn short_modifier_tap_clears_candidate_and_keeps_capture_armed() {
        let (g, rx) = globals();
        let right_control = PhysicalKeyId::new(0x1d, true);
        g.start_capture_for_test(14);
        assert!(process_keyboard_event(
            &g,
            right_control,
            VK_RCONTROL.0 as u32,
            true,
            false,
            1,
            0,
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 14,
                modifiers: crate::physical_shortcut::RIGHT_CTRL,
                key: None,
                ..
            })
        ));
        assert!(process_keyboard_event(
            &g,
            right_control,
            VK_RCONTROL.0 as u32,
            false,
            true,
            100,
            0,
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 14,
                modifiers: 0,
                key: None,
                ..
            })
        ));
        assert!(!g.capture_complete.load(Ordering::Acquire));
        assert_eq!(g.capture_id.load(Ordering::Acquire), 14);
    }

    #[test]
    fn modifier_only_candidate_accumulates_and_completes_after_every_release() {
        let (g, rx) = globals();
        let right_control = PhysicalKeyId::new(0x1d, true);
        let right_shift = PhysicalKeyId::new(0x36, false);
        let expected = crate::physical_shortcut::RIGHT_CTRL | crate::physical_shortcut::RIGHT_SHIFT;
        g.start_capture_for_test(13);
        process_keyboard_event(&g, right_control, VK_RCONTROL.0 as u32, true, false, 1, 0);
        let _ = rx.try_recv();
        process_keyboard_event(&g, right_shift, VK_RSHIFT.0 as u32, true, false, 2, 0);
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 13,
                modifiers,
                key: None,
                ..
            }) if modifiers == expected
        ));
        process_keyboard_event(&g, right_control, VK_RCONTROL.0 as u32, false, true, 3, 0);
        assert!(rx.try_recv().is_err());
        process_keyboard_event(&g, right_shift, VK_RSHIFT.0 as u32, false, true, 204, 0);
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::Captured {
                capture_id: 13,
                modifiers,
                modifier_only: true,
                ..
            }) if modifiers == expected
        ));
    }

    #[test]
    fn installed_modifier_only_chord_triggers_independent_of_press_order() {
        let (g, rx) = globals();
        let binding = modifier_only_binding(
            crate::physical_shortcut::RIGHT_CTRL | crate::physical_shortcut::RIGHT_SHIFT,
        )
        .unwrap()
        .compile()
        .unwrap();
        g.binding.store(pack_binding(binding), Ordering::Release);
        g.enabled.store(true, Ordering::Release);
        let right_shift = PhysicalKeyId::new(0x36, false);
        let right_control = PhysicalKeyId::new(0x1d, true);
        process_keyboard_event(&g, right_shift, VK_RSHIFT.0 as u32, true, false, 1, 0);
        assert!(rx.try_recv().is_err());
        process_keyboard_event(&g, right_control, VK_RCONTROL.0 as u32, true, false, 2, 0);
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));
        process_keyboard_event(&g, right_shift, VK_RSHIFT.0 as u32, false, true, 3, 0);
        assert!(matches!(rx.try_recv(), Ok(Signal::Released)));
    }

    #[test]
    fn installed_ctrl_win_chord_triggers_and_consumes_the_win_key() {
        let (g, rx) = globals();
        let binding = modifier_only_binding(
            crate::physical_shortcut::RIGHT_CTRL | crate::physical_shortcut::LEFT_WIN,
        )
        .unwrap()
        .compile()
        .unwrap();
        g.binding.store(pack_binding(binding), Ordering::Release);
        g.enabled.store(true, Ordering::Release);
        let right_control = PhysicalKeyId::new(0x1d, true);
        let left_win = PhysicalKeyId::new(0x5b, true);
        assert!(!process_keyboard_event(&g, right_control, VK_RCONTROL.0 as u32, true, false, 1, 0));
        assert!(rx.try_recv().is_err());
        assert!(process_keyboard_event(&g, left_win, VK_LWIN.0 as u32, true, false, 2, 0));
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));
        assert!(process_keyboard_event(&g, left_win, VK_LWIN.0 as u32, false, true, 3, 0));
        assert!(matches!(rx.try_recv(), Ok(Signal::Released)));
    }

    #[test]
    fn a_second_capture_replaces_candidate_state_and_repeat_is_deduplicated() {
        let (g, rx) = globals();
        let control = PhysicalKeyId::new(0x1d, false);
        let shift = PhysicalKeyId::new(0x36, false);
        let first_key = PhysicalKeyId::new(0x2f, false);
        let second_key = PhysicalKeyId::new(0x2e, false);

        g.start_capture_for_test(10);
        process_keyboard_event(&g, control, VK_LCONTROL.0 as u32, true, false, 1, 0);
        let _ = rx.try_recv();
        assert!(process_keyboard_event(
            &g,
            first_key,
            b'V' as u32,
            true,
            false,
            2,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 10,
                key: Some(key),
                ..
            }) if key == first_key
        ));
        assert!(process_keyboard_event(
            &g,
            first_key,
            b'V' as u32,
            true,
            false,
            3,
            0
        ));
        assert!(rx.try_recv().is_err());
        process_keyboard_event(&g, first_key, b'V' as u32, false, true, 4, 0);
        process_keyboard_event(&g, control, VK_LCONTROL.0 as u32, false, true, 5, 0);
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::Captured { capture_id: 10, key, .. }) if key == first_key
        ));

        g.start_capture_for_test(11);
        process_keyboard_event(&g, shift, VK_RSHIFT.0 as u32, true, false, 6, 0);
        let _ = rx.try_recv();
        assert!(process_keyboard_event(
            &g,
            second_key,
            b'C' as u32,
            true,
            false,
            7,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::CaptureProgress {
                capture_id: 11,
                key: Some(key),
                ..
            }) if key == second_key
        ));
        process_keyboard_event(&g, shift, VK_RSHIFT.0 as u32, false, true, 8, 0);
        assert!(rx.try_recv().is_err());
        assert!(process_keyboard_event(
            &g,
            second_key,
            b'C' as u32,
            false,
            true,
            9,
            0
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(Signal::Captured { capture_id: 11, key, .. }) if key == second_key
        ));
    }

    #[test]
    fn altgr_does_not_record_the_synthetic_left_control() {
        let (g, _) = globals();
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x1d, false),
            VK_LCONTROL.0 as u32,
            true,
            false,
            10,
            0
        ));
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x38, true),
            VK_RMENU.0 as u32,
            true,
            false,
            10,
            0
        ));
        assert_eq!(g.held_modifiers.load(Ordering::Acquire), RIGHT_ALT);
    }

    #[test]
    fn own_injection_is_passed_and_ignored() {
        let (g, _) = globals();
        assert!(!process_keyboard_event(
            &g,
            PhysicalKeyId::new(0x39, false),
            VK_SPACE.0 as u32,
            true,
            false,
            1,
            SELF_INJECTED_MARKER
        ));
    }
}
