use crate::physical_shortcut::{
    modifier_bit, CompiledBinding, PhysicalKeyId, ShortcutBinding, LEFT_CTRL, LEFT_WIN, RIGHT_ALT,
    RIGHT_WIN,
};
use crate::shortcut_runtime::{
    KeyboardEngineDiagnostics, KeyboardEngineError, KeyboardEngineErrorKind, KeyboardEngineEvent,
};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
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
const TRACE_TARGET: &str = "shortcut_edit_trace";

#[derive(Debug, Clone, Copy)]
enum Signal {
    Pressed,
    Released,
    Interrupted,
    Shutdown,
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
    last_left_ctrl_time: AtomicU32,
    altgr_synthetic_ctrl: AtomicBool,
    abort_startup: AtomicBool,
    hook_thread_id: AtomicU32,
    healthy: AtomicBool,
    dispatch_alive: AtomicBool,
    interruption_pending: AtomicBool,
    shutting_down: AtomicBool,
    next_hook_generation: AtomicU64,
    install_receipt: Mutex<HookInstallReceipt>,
    install_changed: Condvar,
    last_install_error: Mutex<Option<String>>,
    observed_events: AtomicU64,
    emitted_events: AtomicU64,
    dropped_events: AtomicU64,
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
            last_left_ctrl_time: AtomicU32::new(0),
            altgr_synthetic_ctrl: AtomicBool::new(false),
            abort_startup: AtomicBool::new(false),
            hook_thread_id: AtomicU32::new(0),
            healthy: AtomicBool::new(false),
            dispatch_alive: AtomicBool::new(false),
            interruption_pending: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            next_hook_generation: AtomicU64::new(0),
            install_receipt: Mutex::new(HookInstallReceipt::default()),
            install_changed: Condvar::new(),
            last_install_error: Mutex::new(None),
            observed_events: AtomicU64::new(0),
            emitted_events: AtomicU64::new(0),
            dropped_events: AtomicU64::new(0),
            events,
        }
    }

    fn emit(&self, event: Signal) {
        match self.events.try_send(event) {
            Ok(()) if !matches!(event, Signal::Shutdown) => {
                self.emitted_events.fetch_add(1, Ordering::Relaxed);
            }
            Ok(()) => {}
            Err(_) => {
                self.dropped_events.fetch_add(1, Ordering::Relaxed);
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
        if let Ok(mut last_error) = self.last_install_error.lock() {
            *last_error = error.clone();
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
}

static GLOBALS: OnceLock<Arc<HookGlobals>> = OnceLock::new();

pub struct WindowsKeyboardEngine {
    globals: Arc<HookGlobals>,
    dispatch: Mutex<Option<JoinHandle<()>>>,
    hook_thread: Mutex<Option<JoinHandle<()>>>,
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
                while let Ok(signal) = event_rx.recv() {
                    match signal {
                        Signal::Pressed if !delivered_active => {
                            log_runtime_signal(
                                dispatch_globals.as_ref(),
                                "runtime_binding_pressed",
                                "hook",
                            );
                            dispatch_engine_event(&on_event, KeyboardEngineEvent::Pressed);
                            delivered_active = true;
                        }
                        Signal::Released if delivered_active => {
                            log_runtime_signal(
                                dispatch_globals.as_ref(),
                                "runtime_binding_released",
                                "hook",
                            );
                            dispatch_engine_event(&on_event, KeyboardEngineEvent::Released);
                            delivered_active = false;
                        }
                        Signal::Interrupted => {}
                        Signal::Shutdown => break,
                        _ => {}
                    }
                    let desired = dispatch_globals.desired_active.load(Ordering::Acquire);
                    if desired != delivered_active {
                        metrics::counter!("shortcut.hook.state_reconciled").increment(1);
                        let event = if desired {
                            log_runtime_signal(
                                dispatch_globals.as_ref(),
                                "runtime_binding_pressed",
                                "reconcile",
                            );
                            KeyboardEngineEvent::Pressed
                        } else {
                            log_runtime_signal(
                                dispatch_globals.as_ref(),
                                "runtime_binding_released",
                                "reconcile",
                            );
                            KeyboardEngineEvent::Released
                        };
                        dispatch_engine_event(&on_event, event);
                        delivered_active = desired;
                    }
                    if dispatch_globals
                        .interruption_pending
                        .swap(false, Ordering::AcqRel)
                    {
                        log::error!(
                            target: TRACE_TARGET,
                            "event=hook_interrupted hookGeneration={} observed={} emitted={} dropped={} hookHealthy={} hookWorkerAlive={} dispatchAlive=true enabled={}",
                            dispatch_globals
                                .install_receipt
                                .lock()
                                .map(|receipt| receipt.generation)
                                .unwrap_or(0),
                            dispatch_globals.observed_events.load(Ordering::Relaxed),
                            dispatch_globals.emitted_events.load(Ordering::Relaxed),
                            dispatch_globals.dropped_events.load(Ordering::Relaxed),
                            dispatch_globals.healthy.load(Ordering::Acquire),
                            dispatch_globals.hook_thread_id.load(Ordering::Acquire) != 0,
                            dispatch_globals.enabled.load(Ordering::Acquire),
                        );
                        dispatch_engine_event(
                            &on_event,
                            KeyboardEngineEvent::Interrupted,
                        );
                    }
                }
                if delivered_active {
                    dispatch_engine_event(&on_event, KeyboardEngineEvent::Released);
                }
                dispatch_globals.dispatch_alive.store(false, Ordering::Release);
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
            log::error!(
                target: TRACE_TARGET,
                "event=hook_worker_start_failed error={:?}",
                error.message
            );
        }
        if engine.is_healthy() {
            metrics::counter!("shortcut.hook.installed").increment(1);
        } else {
            metrics::counter!("shortcut.hook.install_errors").increment(1);
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
        if enabled {
            synchronize_modifier_bits(self.globals.as_ref());
        } else {
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

    pub fn diagnostics(&self) -> KeyboardEngineDiagnostics {
        let hook_generation = self
            .globals
            .install_receipt
            .lock()
            .map(|receipt| receipt.generation)
            .unwrap_or(0);
        KeyboardEngineDiagnostics {
            hook_generation,
            observed_events: self.globals.observed_events.load(Ordering::Relaxed),
            emitted_events: self.globals.emitted_events.load(Ordering::Relaxed),
            dropped_events: self.globals.dropped_events.load(Ordering::Relaxed),
            hook_healthy: self.globals.healthy.load(Ordering::Acquire),
            hook_worker_alive: self.is_hook_worker_alive(),
            dispatch_alive: self.is_dispatch_alive(),
            enabled: self.globals.enabled.load(Ordering::Acquire),
        }
    }
    fn reinstall_generation(&self) -> Result<u64, KeyboardEngineError> {
        self.ensure_dispatch_alive()?;
        self.ensure_hook_worker()?;
        let generation = self.globals.next_install_generation();
        let started = Instant::now();
        log::info!(
            target: TRACE_TARGET,
            "event=hook_reinstall_requested hookGeneration={generation}"
        );
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
        match self.wait_for_install(generation) {
            Ok(()) => {
                log::info!(
                    target: TRACE_TARGET,
                    "event=hook_reinstall_completed hookGeneration={} durationMs={}",
                    generation,
                    started.elapsed().as_millis()
                );
                Ok(generation)
            }
            Err(error) => {
                log::error!(
                    target: TRACE_TARGET,
                    "event=hook_reinstall_failed hookGeneration={} durationMs={} errorKind={:?} error={:?}",
                    generation,
                    started.elapsed().as_millis(),
                    error.kind,
                    error.message
                );
                Err(error)
            }
        }
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
                KeyboardEngineErrorKind::GenerationSuperseded,
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
                .and_then(|worker| worker.as_ref().map(|worker| !worker.is_finished()))
                .unwrap_or(false)
    }

    fn is_hook_worker_alive(&self) -> bool {
        self.globals.hook_thread_id.load(Ordering::Acquire) != 0
            && self
                .hook_thread
                .lock()
                .ok()
                .and_then(|worker| worker.as_ref().map(|worker| !worker.is_finished()))
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
            log::warn!(target: TRACE_TARGET, "event=hook_worker_reaped");
        }
        if worker.is_some() {
            return (self.globals.hook_thread_id.load(Ordering::Acquire) != 0)
                .then_some(())
                .ok_or_else(|| {
                    KeyboardEngineError::new(
                        KeyboardEngineErrorKind::HookWorkerUnavailable,
                        "键盘 Hook 线程仍在启动。",
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
        self.globals.shutting_down.store(true, Ordering::Release);
        self.globals
            .interruption_pending
            .store(false, Ordering::Release);
        self.globals.release_active();
        self.globals.enabled.store(false, Ordering::Release);
        self.globals.binding.store(0, Ordering::Release);
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

fn log_runtime_signal(globals: &HookGlobals, event: &str, source: &str) {
    let hook_generation = globals
        .install_receipt
        .lock()
        .map(|receipt| receipt.generation)
        .unwrap_or(0);
    log::debug!(
        target: TRACE_TARGET,
        "event={} traceId=none editId=none phase=runtime source={} hookGeneration={} observed={} emitted={} dropped={} hookHealthy={} hookWorkerAlive={} dispatchAlive={} enabled={}",
        event,
        source,
        hook_generation,
        globals.observed_events.load(Ordering::Relaxed),
        globals.emitted_events.load(Ordering::Relaxed),
        globals.dropped_events.load(Ordering::Relaxed),
        globals.healthy.load(Ordering::Acquire),
        globals.hook_thread_id.load(Ordering::Acquire) != 0,
        globals.dispatch_alive.load(Ordering::Acquire),
        globals.enabled.load(Ordering::Acquire),
    );
}
fn dispatch_engine_event(on_event: &impl Fn(KeyboardEngineEvent), event: KeyboardEngineEvent) {
    if catch_unwind(AssertUnwindSafe(|| on_event(event))).is_err() {
        metrics::counter!("shortcut.dispatch.callback_panicked").increment(1);
        log::error!(
            target: TRACE_TARGET,
            "event=dispatch_callback_panicked workerAlive=true"
        );
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
    if let Some(globals) = GLOBALS.get() {
        globals.hook_thread_id.store(thread_id, Ordering::Release);
    }
    let mut hook = match install_hook() {
        Ok(value) => {
            if let Some(globals) = GLOBALS.get() {
                globals.record_install_result(startup_generation, Ok(()));
            }
            log::info!(
                target: TRACE_TARGET,
                "event=hook_install_completed hookGeneration={startup_generation}"
            );
            let _ = ready.send(thread_id);
            Some(value)
        }
        Err(error) => {
            if let Some(globals) = GLOBALS.get() {
                globals.record_install_result(startup_generation, Err(error.clone()));
            }
            log::error!(
                target: TRACE_TARGET,
                "event=hook_install_failed hookGeneration={} error={:?}",
                startup_generation,
                error
            );
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
        if let Some(globals) = GLOBALS.get() {
            globals.healthy.store(false, Ordering::Release);
        }
        return;
    }

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 || message.message == QUIT_MESSAGE {
            break;
        }
        if message.message != REINSTALL_MESSAGE {
            continue;
        }
        let generation = message.wParam.0 as u64;
        if let Some(globals) = GLOBALS.get() {
            globals.release_active();
        }
        if let Some(current) = hook.take() {
            let _ = unsafe { UnhookWindowsHookEx(current) };
        }
        match install_hook() {
            Ok(next) => {
                hook = Some(next);
                if let Some(globals) = GLOBALS.get() {
                    globals.record_install_result(generation, Ok(()));
                }
                metrics::counter!("shortcut.hook.reinstalled").increment(1);
            }
            Err(error) => {
                metrics::counter!("shortcut.hook.install_errors").increment(1);
                if let Some(globals) = GLOBALS.get() {
                    globals.record_install_result(generation, Err(error));
                }
            }
        }
    }
    if let Some(value) = hook {
        let _ = unsafe { UnhookWindowsHookEx(value) };
    }
    if let Some(globals) = GLOBALS.get() {
        globals.release_active();
        globals.healthy.store(false, Ordering::Release);
        globals.hook_thread_id.store(0, Ordering::Release);
        if !globals.abort_startup.load(Ordering::Acquire)
            && !globals.shutting_down.load(Ordering::Acquire)
        {
            globals.interruption_pending.store(true, Ordering::Release);
            globals.emit(Signal::Interrupted);
        }
    }
    log::warn!(target: TRACE_TARGET, "event=hook_worker_exited");
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
    if process_keyboard_event(globals, key, down, up, input.time, input.dwExtraInfo) {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, wparam, lparam)
    }
}

fn process_keyboard_event(
    globals: &HookGlobals,
    key: PhysicalKeyId,
    down: bool,
    up: bool,
    time: u32,
    extra: usize,
) -> bool {
    if extra == SELF_INJECTED_MARKER {
        return false;
    }
    globals.observed_events.fetch_add(1, Ordering::Relaxed);

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
        }
        if up && bit == LEFT_CTRL && globals.altgr_synthetic_ctrl.swap(false, Ordering::AcqRel) {
            held = globals
                .held_modifiers
                .fetch_and(!LEFT_CTRL, Ordering::AcqRel)
                & !LEFT_CTRL;
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
        if down && globals.enabled.load(Ordering::Acquire) {
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
        if globals.enabled.load(Ordering::Acquire)
            && bit & (LEFT_WIN | RIGHT_WIN) != 0
            && unpack_binding(globals.binding.load(Ordering::Acquire))
                .is_some_and(|binding| binding.includes_modifier_bit(bit))
        {
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

fn current_modifier_bits() -> u8 {
    [
        (VK_LCONTROL, crate::physical_shortcut::LEFT_CTRL),
        (VK_RCONTROL, crate::physical_shortcut::RIGHT_CTRL),
        (VK_LMENU, crate::physical_shortcut::LEFT_ALT),
        (VK_RMENU, crate::physical_shortcut::RIGHT_ALT),
        (VK_LSHIFT, crate::physical_shortcut::LEFT_SHIFT),
        (VK_RSHIFT, crate::physical_shortcut::RIGHT_SHIFT),
        (VK_LWIN, crate::physical_shortcut::LEFT_WIN),
        (VK_RWIN, crate::physical_shortcut::RIGHT_WIN),
    ]
    .into_iter()
    .fold(0, |bits, (key, bit)| {
        if (unsafe { GetAsyncKeyState(key.0 as i32) }) as u16 & 0x8000 != 0 {
            bits | bit
        } else {
            bits
        }
    })
}

fn synchronize_modifier_bits(globals: &HookGlobals) {
    let mut sampled = current_modifier_bits();
    if sampled & RIGHT_ALT == 0 {
        globals.altgr_synthetic_ctrl.store(false, Ordering::Release);
    } else if globals.altgr_synthetic_ctrl.load(Ordering::Acquire) {
        sampled &= !LEFT_CTRL;
    }
    globals.held_modifiers.store(sampled, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical_shortcut::{ModifierBinding, ModifierKind, ModifierSide};

    fn globals() -> (HookGlobals, mpsc::Receiver<Signal>) {
        let (tx, rx) = mpsc::sync_channel(32);
        (HookGlobals::new(tx), rx)
    }

    fn engine_for_receipt() -> (WindowsKeyboardEngine, mpsc::Receiver<Signal>) {
        let (globals, receiver) = globals();
        (
            WindowsKeyboardEngine {
                globals: Arc::new(globals),
                dispatch: Mutex::new(None),
                hook_thread: Mutex::new(None),
            },
            receiver,
        )
    }
    fn left_ctrl_space() -> ShortcutBinding {
        ShortcutBinding {
            modifiers: vec![ModifierBinding {
                kind: ModifierKind::Control,
                side: ModifierSide::Left,
            }],
            trigger: PhysicalKeyId::new(0x39, false),
        }
    }

    #[test]
    fn queue_counts_successful_and_dropped_signals_separately() {
        let (tx, rx) = mpsc::sync_channel(1);
        let globals = HookGlobals::new(tx);
        globals.emit(Signal::Pressed);
        globals.emit(Signal::Released);
        assert_eq!(globals.emitted_events.load(Ordering::Relaxed), 1);
        assert_eq!(globals.dropped_events.load(Ordering::Relaxed), 1);
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));
    }

    #[test]
    fn exact_left_binding_only_matches_the_configured_side() {
        let (globals, rx) = globals();
        globals.binding.store(
            pack_binding(left_ctrl_space().compile().unwrap()),
            Ordering::Release,
        );
        globals.enabled.store(true, Ordering::Release);

        assert!(!process_keyboard_event(
            &globals,
            PhysicalKeyId::new(0x1d, false),
            true,
            false,
            10,
            0,
        ));
        assert!(process_keyboard_event(
            &globals,
            PhysicalKeyId::new(0x39, false),
            true,
            false,
            11,
            0,
        ));
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));

        let (right_globals, right_rx) = self::globals();
        right_globals.binding.store(
            pack_binding(left_ctrl_space().compile().unwrap()),
            Ordering::Release,
        );
        right_globals.enabled.store(true, Ordering::Release);
        assert!(!process_keyboard_event(
            &right_globals,
            PhysicalKeyId::new(0x1d, true),
            true,
            false,
            20,
            0,
        ));
        assert!(!process_keyboard_event(
            &right_globals,
            PhysicalKeyId::new(0x39, false),
            true,
            false,
            21,
            0,
        ));
        assert!(right_rx.try_recv().is_err());
    }

    #[test]
    fn disabled_engine_never_consumes_or_emits() {
        let (globals, rx) = globals();
        globals.binding.store(
            pack_binding(left_ctrl_space().compile().unwrap()),
            Ordering::Release,
        );
        assert!(!process_keyboard_event(
            &globals,
            PhysicalKeyId::new(0x39, false),
            true,
            false,
            1,
            0,
        ));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn install_receipt_keeps_generation_and_failure() {
        let (globals, _) = globals();
        globals.record_install_result(2, Err("injected".into()));
        assert_eq!(globals.install_receipt.lock().unwrap().generation, 2);
        assert_eq!(globals.install_error(), "injected");
        assert!(!globals.healthy.load(Ordering::Acquire));
    }

    #[test]
    fn own_injection_is_ignored() {
        let (globals, rx) = globals();
        globals.binding.store(
            pack_binding(left_ctrl_space().compile().unwrap()),
            Ordering::Release,
        );
        globals.enabled.store(true, Ordering::Release);
        assert!(!process_keyboard_event(
            &globals,
            PhysicalKeyId::new(0x39, false),
            true,
            false,
            1,
            SELF_INJECTED_MARKER,
        ));
        assert!(rx.try_recv().is_err());
    }
    #[test]
    fn generation_receipt_succeeds_only_for_the_requested_generation() {
        let (engine, _receiver) = engine_for_receipt();
        engine.globals.record_install_result(4, Ok(()));
        assert!(engine
            .wait_for_install_timeout(4, Duration::from_millis(1))
            .is_ok());

        engine.globals.record_install_result(6, Ok(()));
        assert_eq!(
            engine
                .wait_for_install_timeout(5, Duration::from_millis(1))
                .unwrap_err()
                .kind,
            KeyboardEngineErrorKind::GenerationSuperseded
        );
    }

    #[test]
    fn generation_receipt_reports_install_failure_and_timeout() {
        let (failed, _receiver) = engine_for_receipt();
        failed
            .globals
            .record_install_result(3, Err("injected install failure".into()));
        assert_eq!(
            failed
                .wait_for_install_timeout(3, Duration::from_millis(1))
                .unwrap_err()
                .kind,
            KeyboardEngineErrorKind::ReinstallFailed
        );

        let (timed_out, _receiver) = engine_for_receipt();
        assert_eq!(
            timed_out
                .wait_for_install_timeout(1, Duration::from_millis(1))
                .unwrap_err()
                .kind,
            KeyboardEngineErrorKind::ReinstallTimeout
        );
    }

    #[test]
    fn dispatch_callback_panic_is_isolated() {
        dispatch_engine_event(
            &|_| panic!("injected callback panic"),
            KeyboardEngineEvent::Pressed,
        );
    }
}
