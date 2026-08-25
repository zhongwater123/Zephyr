use crate::physical_shortcut::{
    modifier_bit, modifiers_from_bits, CompiledBinding, ModifierKind, PhysicalKeyId,
    ShortcutBinding, LEFT_ALT, LEFT_CTRL, RIGHT_ALT,
};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const SELF_INJECTED_MARKER: usize = 0x4759_5459_5049_4E47;
const EVENT_QUEUE_CAPACITY: usize = 32;
const REINSTALL_MESSAGE: u32 = WM_APP + 0x481;
const QUIT_MESSAGE: u32 = WM_APP + 0x482;
const BINDING_VALID: u64 = 1 << 63;

#[derive(Debug, Clone)]
pub struct CapturedShortcut {
    pub capture_id: u64,
    pub binding: ShortcutBinding,
    pub label: String,
}

#[derive(Debug, Clone)]
pub enum KeyboardEngineEvent {
    Pressed,
    Released,
    Captured(CapturedShortcut),
    ReinstallFailed(String),
}

#[derive(Debug, Clone, Copy)]
enum Signal {
    Pressed,
    Released,
    Captured { capture_id: u64, modifiers: u8, key: PhysicalKeyId, vk: u32 },
    ReinstallFailed,
    Shutdown,
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
    capture_key: AtomicU32,
    capture_modifiers: AtomicU8,
    capture_vk: AtomicU32,
    healthy: AtomicBool,
    last_install_error: Mutex<Option<String>>,
    dropped_events: AtomicU64,
    events: SyncSender<Signal>,
}

impl HookGlobals {
    fn new(events: SyncSender<Signal>) -> Self {
        Self {
            enabled: AtomicBool::new(false), binding: AtomicU64::new(0),
            held_modifiers: AtomicU8::new(0), active_down: AtomicBool::new(false),
            consume_until_up: AtomicBool::new(false), desired_active: AtomicBool::new(false),
            capture_id: AtomicU64::new(0), capture_down: AtomicBool::new(false),
            capture_key: AtomicU32::new(0), capture_modifiers: AtomicU8::new(0),
            capture_vk: AtomicU32::new(0), healthy: AtomicBool::new(false),
            last_install_error: Mutex::new(None), dropped_events: AtomicU64::new(0), events,
        }
    }

    fn emit(&self, event: Signal) {
        if self.events.try_send(event).is_err() {
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn release_active(&self) {
        self.consume_until_up.store(false, Ordering::Release);
        if self.active_down.swap(false, Ordering::AcqRel) {
            self.desired_active.store(false, Ordering::Release);
            self.emit(Signal::Released);
        }
    }

    fn record_install_result(&self, result: Result<(), String>) {
        self.healthy.store(result.is_ok(), Ordering::Release);
        if let Ok(mut error) = self.last_install_error.lock() { *error = result.err(); }
    }

    fn install_error(&self) -> String {
        self.last_install_error.lock().ok().and_then(|value| value.clone())
            .unwrap_or_else(|| "物理快捷键引擎当前不可用。".into())
    }
}

static GLOBALS: OnceLock<Arc<HookGlobals>> = OnceLock::new();

pub struct WindowsKeyboardEngine {
    globals: Arc<HookGlobals>,
    thread_id: u32,
    dispatch: Option<JoinHandle<()>>,
    hook_thread: Option<JoinHandle<()>>,
}

impl WindowsKeyboardEngine {
    pub fn start(on_event: impl Fn(KeyboardEngineEvent) + Send + 'static) -> Result<Self, String> {
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let globals = Arc::new(HookGlobals::new(event_tx));
        GLOBALS.set(globals.clone()).map_err(|_| "物理快捷键引擎已初始化".to_string())?;
        let dispatch_globals = globals.clone();
        let dispatch = thread::Builder::new().name("gy-shortcut-dispatch".into()).spawn(move || {
            let mut delivered_active = false;
            while let Ok(signal) = event_rx.recv() {
                match signal {
                    Signal::Pressed if !delivered_active => { on_event(KeyboardEngineEvent::Pressed); delivered_active = true; }
                    Signal::Released if delivered_active => { on_event(KeyboardEngineEvent::Released); delivered_active = false; }
                    Signal::Captured { capture_id, modifiers, key, vk } => {
                        let binding = ShortcutBinding { modifiers: modifiers_from_bits(modifiers), trigger: key };
                        let label = binding.label_with_trigger(&key_label(vk));
                        on_event(KeyboardEngineEvent::Captured(CapturedShortcut { capture_id, binding, label }));
                    }
                    Signal::ReinstallFailed => on_event(KeyboardEngineEvent::ReinstallFailed(dispatch_globals.install_error())),
                    Signal::Shutdown => break,
                    _ => {}
                }
                let desired = dispatch_globals.desired_active.load(Ordering::Acquire);
                if desired != delivered_active {
                    on_event(if desired { KeyboardEngineEvent::Pressed } else { KeyboardEngineEvent::Released });
                    delivered_active = desired;
                }
            }
            if delivered_active { on_event(KeyboardEngineEvent::Released); }
        }).map_err(|error| format!("无法启动快捷键分发线程：{error}"))?;

        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let hook_thread = thread::Builder::new().name("gy-physical-keyboard".into())
            .spawn(move || hook_thread_main(ready_tx))
            .map_err(|error| format!("无法启动键盘钩子线程：{error}"))?;
        let thread_id = ready_rx.recv_timeout(Duration::from_secs(2))
            .map_err(|_| "键盘钩子线程启动超时".to_string())??;
        if !globals.healthy.load(Ordering::Acquire) {
            let error = globals.install_error();
            let _ = globals.events.send(Signal::Shutdown);
            let _ = hook_thread.join();
            let _ = dispatch.join();
            return Err(error);
        }
        metrics::counter!("shortcut.hook.installed").increment(1);
        Ok(Self { globals, thread_id, dispatch: Some(dispatch), hook_thread: Some(hook_thread) })
    }

    pub fn set_binding(&self, binding: Option<&ShortcutBinding>) -> Result<(), String> {
        self.globals.release_active();
        let packed = match binding { Some(value) => pack_binding(value.compile()?), None => 0 };
        self.globals.binding.store(packed, Ordering::Release);
        Ok(())
    }

    pub fn set_enabled(&self, enabled: bool) {
        if !enabled { self.globals.release_active(); }
        self.globals.enabled.store(enabled, Ordering::Release);
    }

    pub fn start_capture(&self, capture_id: u64) {
        self.globals.release_active();
        self.globals.capture_down.store(false, Ordering::Release);
        self.globals.capture_key.store(0, Ordering::Release);
        self.globals.capture_id.store(capture_id, Ordering::Release);
    }

    pub fn cancel_capture(&self, capture_id: Option<u64>) {
        let current = self.globals.capture_id.load(Ordering::Acquire);
        if capture_id.is_none() || capture_id == Some(current) {
            self.globals.capture_id.store(0, Ordering::Release);
            self.globals.capture_down.store(false, Ordering::Release);
            self.globals.capture_key.store(0, Ordering::Release);
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.globals.healthy.load(Ordering::Acquire)
            && self.hook_thread.as_ref().is_some_and(|thread| !thread.is_finished())
    }

    pub fn ensure_healthy(&self) -> Result<(), String> {
        if self.is_healthy() { return Ok(()); }
        if self.hook_thread.as_ref().is_none_or(|thread| thread.is_finished()) {
            return Err("物理快捷键线程已停止，请重启应用。".into());
        }
        unsafe { PostThreadMessageW(self.thread_id, REINSTALL_MESSAGE, WPARAM(0), LPARAM(0)) }
            .map_err(|error| format!("无法请求重新安装键盘钩子：{error}"))?;
        for _ in 0..40 {
            if self.is_healthy() { return Ok(()); }
            thread::sleep(Duration::from_millis(25));
        }
        Err(self.globals.install_error())
    }

    pub fn shutdown(&mut self) {
        if self.hook_thread.is_none() && self.dispatch.is_none() { return; }
        self.globals.set_enabled(false);
        self.globals.binding.store(0, Ordering::Release);
        self.cancel_capture(None);
        self.globals.healthy.store(false, Ordering::Release);
        let _ = unsafe { PostThreadMessageW(self.thread_id, QUIT_MESSAGE, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.hook_thread.take() { let _ = thread.join(); }
        let _ = self.globals.events.send(Signal::Shutdown);
        if let Some(thread) = self.dispatch.take() { let _ = thread.join(); }
        let dropped = self.globals.dropped_events.load(Ordering::Relaxed);
        if dropped > 0 { metrics::counter!("shortcut.hook.events_dropped").increment(dropped); }
        metrics::counter!("shortcut.hook.uninstalled").increment(1);
    }
}

impl Drop for WindowsKeyboardEngine { fn drop(&mut self) { self.shutdown(); } }

fn pack_binding(binding: CompiledBinding) -> u64 {
    BINDING_VALID | binding.trigger.packed() as u64
        | ((binding.sided_modifiers as u64) << 17) | ((binding.any_modifiers as u64) << 25)
}

fn unpack_binding(value: u64) -> Option<CompiledBinding> {
    (value & BINDING_VALID != 0).then(|| CompiledBinding {
        trigger: PhysicalKeyId::from_packed(value as u32 & 0x1ffff),
        sided_modifiers: ((value >> 17) & 0xff) as u8,
        any_modifiers: ((value >> 25) & 0x0f) as u8,
    })
}

fn hook_thread_main(ready: SyncSender<Result<u32, String>>) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
    let mut hook = match install_hook() {
        Ok(value) => { if let Some(g) = GLOBALS.get() { g.record_install_result(Ok(())); } let _ = ready.send(Ok(thread_id)); Some(value) }
        Err(error) => { if let Some(g) = GLOBALS.get() { g.record_install_result(Err(error.clone())); } let _ = ready.send(Ok(thread_id)); None }
    };
    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 || message.message == QUIT_MESSAGE { break; }
        if message.message == REINSTALL_MESSAGE {
            if let Some(current) = hook.take() { let _ = unsafe { UnhookWindowsHookEx(current) }; }
            match install_hook() {
                Ok(next) => { hook = Some(next); if let Some(g) = GLOBALS.get() { g.record_install_result(Ok(())); } metrics::counter!("shortcut.hook.reinstalled").increment(1); }
                Err(error) => { if let Some(g) = GLOBALS.get() { g.record_install_result(Err(error)); g.emit(Signal::ReinstallFailed); } }
            }
        }
    }
    if let Some(value) = hook { let _ = unsafe { UnhookWindowsHookEx(value) }; }
    if let Some(g) = GLOBALS.get() { g.healthy.store(false, Ordering::Release); }
}

fn install_hook() -> Result<HHOOK, String> {
    let module = unsafe { GetModuleHandleW(None) }.map_err(|error| format!("无法取得应用模块句柄：{error}"))?;
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), HINSTANCE(module.0), 0) }
        .map_err(|error| format!("无法安装物理键盘钩子：{error}"))
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 { return CallNextHookEx(None, code, wparam, lparam); }
    let Some(globals) = GLOBALS.get() else { return CallNextHookEx(None, code, wparam, lparam); };
    let input = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    let message = wparam.0 as u32;
    let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if !down && !up { return CallNextHookEx(None, code, wparam, lparam); }
    let key = PhysicalKeyId::new(input.scanCode as u16, input.flags.contains(LLKHF_EXTENDED));
    if process_keyboard_event(globals, key, input.vkCode, down, up, input.time, input.dwExtraInfo) { LRESULT(1) }
    else { CallNextHookEx(None, code, wparam, lparam) }
}

fn process_keyboard_event(globals: &HookGlobals, key: PhysicalKeyId, vk: u32, down: bool, up: bool, _time: u32, extra: usize) -> bool {
    if extra == SELF_INJECTED_MARKER { return false; }
    if let Some(bit) = modifier_bit(key) {
        let held = if down { globals.held_modifiers.fetch_or(bit, Ordering::AcqRel) | bit }
            else { globals.held_modifiers.fetch_and(!bit, Ordering::AcqRel) & !bit };
        if up && globals.active_down.load(Ordering::Acquire) {
            if let Some(binding) = unpack_binding(globals.binding.load(Ordering::Acquire)) {
                if !binding.required_modifiers_still_held(held) {
                    globals.active_down.store(false, Ordering::Release);
                    globals.desired_active.store(false, Ordering::Release);
                    globals.emit(Signal::Released);
                }
            }
        }
        return false;
    }

    let capture_id = globals.capture_id.load(Ordering::Acquire);
    if capture_id != 0 {
        let held = globals.held_modifiers.load(Ordering::Acquire);
        if down && held != 0 {
            if !globals.capture_down.swap(true, Ordering::AcqRel) {
                globals.capture_key.store(key.packed(), Ordering::Release);
                globals.capture_modifiers.store(held, Ordering::Release);
                globals.capture_vk.store(vk, Ordering::Release);
            }
            if globals.capture_key.load(Ordering::Acquire) == key.packed() { return true; }
        }
        if up && globals.capture_down.load(Ordering::Acquire) && globals.capture_key.load(Ordering::Acquire) == key.packed() {
            globals.capture_down.store(false, Ordering::Release);
            globals.capture_id.store(0, Ordering::Release);
            globals.emit(Signal::Captured { capture_id, modifiers: globals.capture_modifiers.load(Ordering::Acquire), key, vk: globals.capture_vk.load(Ordering::Acquire) });
            return true;
        }
        return false;
    }

    if !globals.enabled.load(Ordering::Acquire) { return false; }
    let Some(binding) = unpack_binding(globals.binding.load(Ordering::Acquire)) else { return false; };
    if key != binding.trigger { return false; }
    if down {
        if globals.consume_until_up.load(Ordering::Acquire) { return true; }
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

fn key_label(vk: u32) -> String {
    if (b'A' as u32..=b'Z' as u32).contains(&vk) || (b'0' as u32..=b'9' as u32).contains(&vk) {
        return char::from_u32(vk).unwrap_or('?').to_string();
    }
    match vk as u16 {
        value if value == VK_SPACE.0 => "Space".into(), value if value == VK_TAB.0 => "Tab".into(),
        value if value == VK_RETURN.0 => "Enter".into(), value if value == VK_ESCAPE.0 => "Escape".into(),
        value if value == VK_BACK.0 => "Backspace".into(), value if value == VK_DELETE.0 => "Delete".into(),
        value if value == VK_INSERT.0 => "Insert".into(), value if value == VK_HOME.0 => "Home".into(),
        value if value == VK_END.0 => "End".into(), value if value == VK_PRIOR.0 => "PageUp".into(),
        value if value == VK_NEXT.0 => "PageDown".into(), value if value == VK_UP.0 => "ArrowUp".into(),
        value if value == VK_DOWN.0 => "ArrowDown".into(), value if value == VK_LEFT.0 => "ArrowLeft".into(),
        value if value == VK_RIGHT.0 => "ArrowRight".into(),
        value if (VK_F1.0..=VK_F24.0).contains(&value) => format!("F{}", value - VK_F1.0 + 1),
        _ => format!("ScanCode {:02X}", vk),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn globals() -> (HookGlobals, mpsc::Receiver<Signal>) { let (tx, rx) = mpsc::sync_channel(16); (HookGlobals::new(tx), rx) }

    #[test]
    fn exact_left_binding_consumes_only_matching_main_key() {
        let (g, rx) = globals();
        g.binding.store(pack_binding(ShortcutBinding::default_physical().compile().unwrap()), Ordering::Release);
        g.enabled.store(true, Ordering::Release);
        assert!(!process_keyboard_event(&g, PhysicalKeyId::new(0x1d, false), VK_LCONTROL.0 as u32, true, false, 1, 0));
        assert!(!process_keyboard_event(&g, PhysicalKeyId::new(0x2a, false), VK_LSHIFT.0 as u32, true, false, 2, 0));
        assert!(process_keyboard_event(&g, PhysicalKeyId::new(0x39, false), VK_SPACE.0 as u32, true, false, 3, 0));
        assert!(matches!(rx.try_recv(), Ok(Signal::Pressed)));
        assert!(process_keyboard_event(&g, PhysicalKeyId::new(0x39, false), VK_SPACE.0 as u32, true, false, 4, 0));
        assert!(rx.try_recv().is_err());
        assert!(process_keyboard_event(&g, PhysicalKeyId::new(0x39, false), VK_SPACE.0 as u32, false, true, 5, 0));
        assert!(matches!(rx.try_recv(), Ok(Signal::Released)));
    }

    #[test]
    fn own_injection_is_passed_and_ignored() {
        let (g, _) = globals();
        assert!(!process_keyboard_event(&g, PhysicalKeyId::new(0x39, false), VK_SPACE.0 as u32, true, false, 1, SELF_INJECTED_MARKER));
    }
}
