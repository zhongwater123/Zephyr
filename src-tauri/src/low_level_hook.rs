use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const SELF_INJECTED_MARKER: usize = 0x4759_5459_5049_4E47;
const EVENT_QUEUE_CAPACITY: usize = 32;
const REINSTALL_MESSAGE: u32 = WM_APP + 0x471;
const QUIT_MESSAGE: u32 = WM_APP + 0x472;

const MOD_CTRL: u8 = 1;
const MOD_ALT: u8 = 2;
const MOD_SHIFT: u8 = 4;
const MOD_WIN: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookChord {
    pub modifiers: u8,
    pub vk: u32,
}

impl HookChord {
    pub fn from_shortcut(shortcut: Shortcut) -> Result<Self, String> {
        let mut modifiers = 0;
        if shortcut.mods.contains(Modifiers::CONTROL) {
            modifiers |= MOD_CTRL;
        }
        if shortcut.mods.contains(Modifiers::ALT) {
            modifiers |= MOD_ALT;
        }
        if shortcut.mods.contains(Modifiers::SHIFT) {
            modifiers |= MOD_SHIFT;
        }
        if shortcut.mods.intersects(Modifiers::SUPER | Modifiers::META) {
            modifiers |= MOD_WIN;
        }
        let vk = code_to_vk(shortcut.key)
            .ok_or_else(|| format!("快捷键主键不支持低级钩子：{}", shortcut.key))?;
        Ok(Self { modifiers, vk })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    ActivePressed,
    ActiveReleased,
    PreviewVerified(u64),
    ReinstallFailed,
    Shutdown,
}

struct HookGlobals {
    voice_enabled: AtomicBool,
    active_modifiers: AtomicU8,
    active_vk: AtomicU32,
    active_down: AtomicBool,
    preview_modifiers: AtomicU8,
    preview_vk: AtomicU32,
    preview_down: AtomicBool,
    preview_token: AtomicU64,
    verified_preview_token: AtomicU64,
    held_modifiers: AtomicU8,
    observed_down_vk: AtomicU32,
    observed_down_modifiers: AtomicU8,
    healthy: AtomicBool,
    last_install_error: Mutex<Option<String>>,
    dropped_events: AtomicU64,
    events: SyncSender<HookEvent>,
}

impl HookGlobals {
    fn new(events: SyncSender<HookEvent>) -> Self {
        Self {
            voice_enabled: AtomicBool::new(false),
            active_modifiers: AtomicU8::new(0),
            active_vk: AtomicU32::new(0),
            active_down: AtomicBool::new(false),
            preview_modifiers: AtomicU8::new(0),
            preview_vk: AtomicU32::new(0),
            preview_down: AtomicBool::new(false),
            preview_token: AtomicU64::new(0),
            verified_preview_token: AtomicU64::new(0),
            held_modifiers: AtomicU8::new(0),
            observed_down_vk: AtomicU32::new(0),
            observed_down_modifiers: AtomicU8::new(0),
            healthy: AtomicBool::new(false),
            last_install_error: Mutex::new(None),
            dropped_events: AtomicU64::new(0),
            events,
        }
    }

    fn emit(&self, event: HookEvent) {
        if self.events.try_send(event).is_err() {
            self.dropped_events.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn set_active(&self, chord: Option<HookChord>) {
        self.active_down.store(false, Ordering::Release);
        if let Some(chord) = chord {
            self.active_modifiers
                .store(chord.modifiers, Ordering::Release);
            self.active_vk.store(chord.vk, Ordering::Release);
        } else {
            self.active_vk.store(0, Ordering::Release);
            self.active_modifiers.store(0, Ordering::Release);
        }
    }

    fn set_preview(&self, chord: Option<HookChord>, preview_token: u64) {
        self.preview_down.store(false, Ordering::Release);
        self.verified_preview_token.store(0, Ordering::Release);
        if let Some(chord) = chord {
            self.preview_modifiers
                .store(chord.modifiers, Ordering::Release);
            self.preview_vk.store(chord.vk, Ordering::Release);
            self.preview_token.store(preview_token, Ordering::Release);
            let already_down = self.observed_down_vk.load(Ordering::Acquire) == chord.vk
                && self.observed_down_modifiers.load(Ordering::Acquire) == chord.modifiers;
            self.preview_down.store(already_down, Ordering::Release);
        } else {
            self.preview_vk.store(0, Ordering::Release);
            self.preview_modifiers.store(0, Ordering::Release);
            self.preview_token.store(0, Ordering::Release);
        }
    }

    fn record_install_result(&self, result: Result<(), String>) {
        self.healthy.store(result.is_ok(), Ordering::Release);
        if let Ok(mut last_error) = self.last_install_error.lock() {
            *last_error = result.err();
        }
    }

    fn install_error(&self) -> String {
        self.last_install_error
            .lock()
            .ok()
            .and_then(|error| error.clone())
            .unwrap_or_else(|| "低级键盘钩子当前不可用。".to_string())
    }
}

static GLOBALS: OnceLock<Arc<HookGlobals>> = OnceLock::new();

pub struct LowLevelHookService {
    globals: Arc<HookGlobals>,
    thread_id: u32,
    dispatch: Option<JoinHandle<()>>,
    hook_thread: Option<JoinHandle<()>>,
}

impl LowLevelHookService {
    pub fn start(on_event: impl Fn(HookEvent) + Send + 'static) -> Result<Self, String> {
        let (event_tx, event_rx) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let globals = Arc::new(HookGlobals::new(event_tx));
        GLOBALS
            .set(globals.clone())
            .map_err(|_| "低级键盘钩子已初始化".to_string())?;

        let dispatch = thread::Builder::new()
            .name("gy-shortcut-dispatch".to_string())
            .spawn(move || {
                while let Ok(event) = event_rx.recv() {
                    if event == HookEvent::Shutdown {
                        break;
                    }
                    on_event(event);
                }
            })
            .map_err(|error| format!("无法启动快捷键分发线程：{error}"))?;

        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let hook_thread = thread::Builder::new()
            .name("gy-keyboard-hook".to_string())
            .spawn(move || hook_thread_main(ready_tx))
            .map_err(|error| format!("无法启动键盘钩子线程：{error}"))?;
        let ready = ready_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "键盘钩子线程启动超时".to_string())
            .and_then(|result| result);
        let thread_id = match ready {
            Ok(thread_id) => thread_id,
            Err(error) => {
                metrics::counter!("shortcut.hook.install_errors").increment(1);
                let _ = globals.events.send(HookEvent::Shutdown);
                let _ = hook_thread.join();
                let _ = dispatch.join();
                return Err(error);
            }
        };
        if globals.healthy.load(Ordering::Acquire) {
            log::info!("low-level shortcut hook installed");
            metrics::counter!("shortcut.hook.installed").increment(1);
        } else {
            log::warn!("low-level shortcut hook thread started without an active hook");
        }

        Ok(Self {
            globals,
            thread_id,
            dispatch: Some(dispatch),
            hook_thread: Some(hook_thread),
        })
    }

    pub fn set_voice_enabled(&self, enabled: bool) {
        self.globals.voice_enabled.store(enabled, Ordering::Release);
        if !enabled {
            self.globals.active_down.store(false, Ordering::Release);
        }
    }

    pub fn set_active(&self, chord: Option<HookChord>) {
        self.globals.set_active(chord);
    }

    pub fn set_preview(&self, chord: Option<HookChord>, preview_token: u64) {
        self.globals.set_preview(chord, preview_token);
    }

    pub fn preview_verified(&self, preview_token: u64) -> bool {
        preview_token != 0
            && self.globals.verified_preview_token.load(Ordering::Acquire) == preview_token
    }

    pub fn is_healthy(&self) -> bool {
        self.globals.healthy.load(Ordering::Acquire)
            && self
                .hook_thread
                .as_ref()
                .is_some_and(|thread| !thread.is_finished())
    }

    pub fn ensure_healthy(&self) -> Result<(), String> {
        if self.is_healthy() {
            return Ok(());
        }
        if self
            .hook_thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
        {
            return Err("低级键盘钩子线程已停止，请重启应用。".to_string());
        }
        self.reinstall()?;
        for _ in 0..40 {
            if self.is_healthy() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
        Err(self.globals.install_error())
    }

    pub fn reinstall(&self) -> Result<(), String> {
        unsafe { PostThreadMessageW(self.thread_id, REINSTALL_MESSAGE, WPARAM(0), LPARAM(0)) }
            .map_err(|error| format!("无法请求重新安装键盘钩子：{error}"))
    }

    pub fn shutdown(&mut self) {
        if self.hook_thread.is_none() && self.dispatch.is_none() {
            return;
        }
        self.globals.set_active(None);
        self.globals.set_preview(None, 0);
        self.globals.voice_enabled.store(false, Ordering::Release);
        self.globals.healthy.store(false, Ordering::Release);
        let _ = unsafe { PostThreadMessageW(self.thread_id, QUIT_MESSAGE, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.hook_thread.take() {
            let _ = thread.join();
        }
        let dropped = self.globals.dropped_events.load(Ordering::Relaxed);
        if dropped > 0 {
            metrics::counter!("shortcut.hook.events_dropped").increment(dropped);
            log::warn!("low-level shortcut event queue dropped {dropped} control events");
        }
        let _ = self.globals.events.send(HookEvent::Shutdown);
        if let Some(dispatch) = self.dispatch.take() {
            let _ = dispatch.join();
        }
        log::info!("low-level shortcut hook uninstalled");
        metrics::counter!("shortcut.hook.uninstalled").increment(1);
    }
}

impl Drop for LowLevelHookService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn hook_thread_main(ready: SyncSender<Result<u32, String>>) {
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE) };
    let mut hook = match install_hook() {
        Ok(hook) => {
            if let Some(globals) = GLOBALS.get() {
                globals.record_install_result(Ok(()));
            }
            let _ = ready.send(Ok(thread_id));
            Some(hook)
        }
        Err(error) => {
            if let Some(globals) = GLOBALS.get() {
                globals.record_install_result(Err(error.clone()));
            }
            log::error!("failed to install low-level keyboard hook: {error}");
            metrics::counter!("shortcut.hook.install_errors").increment(1);
            let _ = ready.send(Ok(thread_id));
            None
        }
    };

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
        if result.0 <= 0 || message.message == QUIT_MESSAGE {
            break;
        }
        if message.message == REINSTALL_MESSAGE {
            if let Some(current) = hook.take() {
                let _ = unsafe { UnhookWindowsHookEx(current) };
            }
            match install_hook() {
                Ok(next) => {
                    hook = Some(next);
                    if let Some(globals) = GLOBALS.get() {
                        globals.record_install_result(Ok(()));
                    }
                    log::info!("low-level shortcut hook reinstalled after system resume");
                    metrics::counter!("shortcut.hook.reinstalled").increment(1);
                }
                Err(error) => {
                    if let Some(globals) = GLOBALS.get() {
                        globals.record_install_result(Err(error.clone()));
                    }
                    log::error!("failed to reinstall low-level keyboard hook: {error}");
                    metrics::counter!("shortcut.hook.install_errors").increment(1);
                    if let Some(globals) = GLOBALS.get() {
                        globals.emit(HookEvent::ReinstallFailed);
                    }
                }
            }
        }
    }
    if let Some(hook) = hook {
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }
    if let Some(globals) = GLOBALS.get() {
        globals.healthy.store(false, Ordering::Release);
    }
}

fn install_hook() -> Result<HHOOK, String> {
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|error| format!("无法取得应用模块句柄：{error}"))?;
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), HINSTANCE(module.0), 0) }
        .map_err(|error| format!("无法安装低级键盘钩子：{error}"))
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
    let key_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
    let key_up = message == WM_KEYUP || message == WM_SYSKEYUP;
    if !key_down && !key_up {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if process_keyboard_event(globals, input.vkCode, key_down, key_up, input.dwExtraInfo) {
        return LRESULT(1);
    }
    CallNextHookEx(None, code, wparam, lparam)
}

fn process_keyboard_event(
    globals: &HookGlobals,
    vk_code: u32,
    key_down: bool,
    key_up: bool,
    extra_info: usize,
) -> bool {
    if extra_info == SELF_INJECTED_MARKER {
        return false;
    }

    if let Some(bit) = modifier_bit(vk_code) {
        if key_down {
            globals.held_modifiers.fetch_or(bit, Ordering::AcqRel);
        } else {
            globals.held_modifiers.fetch_and(!bit, Ordering::AcqRel);
        }
        return false;
    }

    let held = globals.held_modifiers.load(Ordering::Acquire);
    if key_down {
        globals
            .observed_down_modifiers
            .store(held, Ordering::Release);
        globals.observed_down_vk.store(vk_code, Ordering::Release);
    }
    let preview_vk = globals.preview_vk.load(Ordering::Acquire);
    if preview_vk != 0 && vk_code == preview_vk {
        if key_down && held == globals.preview_modifiers.load(Ordering::Acquire) {
            globals.preview_down.store(true, Ordering::Release);
            return true;
        }
        if key_up && globals.preview_down.swap(false, Ordering::AcqRel) {
            let preview_token = globals.preview_token.load(Ordering::Acquire);
            if preview_token != 0 {
                globals
                    .verified_preview_token
                    .store(preview_token, Ordering::Release);
                globals.emit(HookEvent::PreviewVerified(preview_token));
            }
            globals.observed_down_vk.store(0, Ordering::Release);
            return true;
        }
    }

    let active_vk = globals.active_vk.load(Ordering::Acquire);
    if globals.voice_enabled.load(Ordering::Acquire) && active_vk != 0 && vk_code == active_vk {
        if key_down && held == globals.active_modifiers.load(Ordering::Acquire) {
            if !globals.active_down.swap(true, Ordering::AcqRel) {
                globals.emit(HookEvent::ActivePressed);
            }
            return true;
        }
        if key_up && globals.active_down.swap(false, Ordering::AcqRel) {
            globals.emit(HookEvent::ActiveReleased);
            globals.observed_down_vk.store(0, Ordering::Release);
            return true;
        }
    }
    if key_up && globals.observed_down_vk.load(Ordering::Acquire) == vk_code {
        globals.observed_down_vk.store(0, Ordering::Release);
    }
    false
}

fn modifier_bit(vk: u32) -> Option<u8> {
    match vk as u16 {
        value if value == VK_CONTROL.0 || value == VK_LCONTROL.0 || value == VK_RCONTROL.0 => {
            Some(MOD_CTRL)
        }
        value if value == VK_MENU.0 || value == VK_LMENU.0 || value == VK_RMENU.0 => Some(MOD_ALT),
        value if value == VK_SHIFT.0 || value == VK_LSHIFT.0 || value == VK_RSHIFT.0 => {
            Some(MOD_SHIFT)
        }
        value if value == VK_LWIN.0 || value == VK_RWIN.0 => Some(MOD_WIN),
        _ => None,
    }
}

fn code_to_vk(code: Code) -> Option<u32> {
    Some(match code {
        Code::KeyA => VK_A.0,
        Code::KeyB => VK_B.0,
        Code::KeyC => VK_C.0,
        Code::KeyD => VK_D.0,
        Code::KeyE => VK_E.0,
        Code::KeyF => VK_F.0,
        Code::KeyG => VK_G.0,
        Code::KeyH => VK_H.0,
        Code::KeyI => VK_I.0,
        Code::KeyJ => VK_J.0,
        Code::KeyK => VK_K.0,
        Code::KeyL => VK_L.0,
        Code::KeyM => VK_M.0,
        Code::KeyN => VK_N.0,
        Code::KeyO => VK_O.0,
        Code::KeyP => VK_P.0,
        Code::KeyQ => VK_Q.0,
        Code::KeyR => VK_R.0,
        Code::KeyS => VK_S.0,
        Code::KeyT => VK_T.0,
        Code::KeyU => VK_U.0,
        Code::KeyV => VK_V.0,
        Code::KeyW => VK_W.0,
        Code::KeyX => VK_X.0,
        Code::KeyY => VK_Y.0,
        Code::KeyZ => VK_Z.0,
        Code::Digit0 => VK_0.0,
        Code::Digit1 => VK_1.0,
        Code::Digit2 => VK_2.0,
        Code::Digit3 => VK_3.0,
        Code::Digit4 => VK_4.0,
        Code::Digit5 => VK_5.0,
        Code::Digit6 => VK_6.0,
        Code::Digit7 => VK_7.0,
        Code::Digit8 => VK_8.0,
        Code::Digit9 => VK_9.0,
        Code::Space => VK_SPACE.0,
        Code::Tab => VK_TAB.0,
        Code::Enter => VK_RETURN.0,
        Code::Escape => VK_ESCAPE.0,
        Code::Backspace => VK_BACK.0,
        Code::Delete => VK_DELETE.0,
        Code::Insert => VK_INSERT.0,
        Code::Home => VK_HOME.0,
        Code::End => VK_END.0,
        Code::PageUp => VK_PRIOR.0,
        Code::PageDown => VK_NEXT.0,
        Code::ArrowUp => VK_UP.0,
        Code::ArrowDown => VK_DOWN.0,
        Code::ArrowLeft => VK_LEFT.0,
        Code::ArrowRight => VK_RIGHT.0,
        Code::F1 => VK_F1.0,
        Code::F2 => VK_F2.0,
        Code::F3 => VK_F3.0,
        Code::F4 => VK_F4.0,
        Code::F5 => VK_F5.0,
        Code::F6 => VK_F6.0,
        Code::F7 => VK_F7.0,
        Code::F8 => VK_F8.0,
        Code::F9 => VK_F9.0,
        Code::F10 => VK_F10.0,
        Code::F11 => VK_F11.0,
        Code::F12 => VK_F12.0,
        Code::F13 => VK_F13.0,
        Code::F14 => VK_F14.0,
        Code::F15 => VK_F15.0,
        Code::F16 => VK_F16.0,
        Code::F17 => VK_F17.0,
        Code::F18 => VK_F18.0,
        Code::F19 => VK_F19.0,
        Code::F20 => VK_F20.0,
        Code::F21 => VK_F21.0,
        Code::F22 => VK_F22.0,
        Code::F23 => VK_F23.0,
        Code::F24 => VK_F24.0,
        _ => return None,
    } as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_globals() -> (HookGlobals, mpsc::Receiver<HookEvent>) {
        let (events, received) = mpsc::sync_channel(16);
        (HookGlobals::new(events), received)
    }

    fn key(globals: &HookGlobals, vk: u32, down: bool, extra: usize) -> bool {
        process_keyboard_event(globals, vk, down, !down, extra)
    }

    #[test]
    fn maps_exact_modifier_set_and_virtual_key() {
        let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
        let chord = HookChord::from_shortcut(shortcut).unwrap();
        assert_eq!(chord.modifiers, MOD_CTRL | MOD_ALT);
        assert_eq!(chord.vk, VK_SPACE.0 as u32);
    }

    #[test]
    fn left_and_right_modifiers_share_one_logical_bit() {
        assert_eq!(modifier_bit(VK_LCONTROL.0 as u32), Some(MOD_CTRL));
        assert_eq!(modifier_bit(VK_RCONTROL.0 as u32), Some(MOD_CTRL));
        assert_eq!(modifier_bit(VK_LMENU.0 as u32), Some(MOD_ALT));
        assert_eq!(modifier_bit(VK_RMENU.0 as u32), Some(MOD_ALT));
    }

    #[test]
    fn preview_swallows_only_the_exact_main_key_and_verifies_on_release() {
        let (globals, received) = test_globals();
        globals.set_preview(
            Some(HookChord {
                modifiers: MOD_CTRL | MOD_SHIFT,
                vk: VK_SPACE.0 as u32,
            }),
            7,
        );

        assert!(!key(&globals, VK_LCONTROL.0 as u32, true, 0));
        assert!(!key(&globals, VK_RSHIFT.0 as u32, true, 0));
        assert!(!key(&globals, VK_A.0 as u32, true, 0));
        assert!(key(&globals, VK_SPACE.0 as u32, true, 0));
        assert!(key(&globals, VK_SPACE.0 as u32, true, 0));
        assert!(received.try_recv().is_err());
        assert!(key(&globals, VK_SPACE.0 as u32, false, 0));
        assert_eq!(received.try_recv(), Ok(HookEvent::PreviewVerified(7)));
        assert_eq!(globals.verified_preview_token.load(Ordering::Acquire), 7);
        assert!(!key(&globals, VK_LCONTROL.0 as u32, false, 0));
        assert!(!key(&globals, VK_RSHIFT.0 as u32, false, 0));
    }

    #[test]
    fn preview_can_verify_the_same_physical_press_that_selected_the_candidate() {
        let (globals, received) = test_globals();
        assert!(!key(&globals, VK_LCONTROL.0 as u32, true, 0));
        assert!(!key(&globals, VK_LSHIFT.0 as u32, true, 0));
        assert!(!key(&globals, VK_SPACE.0 as u32, true, 0));

        globals.set_preview(
            Some(HookChord {
                modifiers: MOD_CTRL | MOD_SHIFT,
                vk: VK_SPACE.0 as u32,
            }),
            23,
        );

        assert!(key(&globals, VK_SPACE.0 as u32, false, 0));
        assert_eq!(received.try_recv(), Ok(HookEvent::PreviewVerified(23)));
    }

    #[test]
    fn active_shortcut_ignores_repeats_and_releases_after_modifier_first() {
        let (globals, received) = test_globals();
        globals.set_active(Some(HookChord {
            modifiers: MOD_CTRL | MOD_ALT,
            vk: VK_SPACE.0 as u32,
        }));
        globals.voice_enabled.store(true, Ordering::Release);

        assert!(!key(&globals, VK_RCONTROL.0 as u32, true, 0));
        assert!(!key(&globals, VK_LMENU.0 as u32, true, 0));
        assert!(key(&globals, VK_SPACE.0 as u32, true, 0));
        assert_eq!(received.try_recv(), Ok(HookEvent::ActivePressed));
        assert!(key(&globals, VK_SPACE.0 as u32, true, 0));
        assert!(received.try_recv().is_err());

        assert!(!key(&globals, VK_RCONTROL.0 as u32, false, 0));
        assert!(key(&globals, VK_SPACE.0 as u32, false, 0));
        assert_eq!(received.try_recv(), Ok(HookEvent::ActiveReleased));
    }

    #[test]
    fn extra_modifier_and_disabled_voice_are_never_swallowed() {
        let (globals, received) = test_globals();
        globals.set_active(Some(HookChord {
            modifiers: MOD_CTRL | MOD_SHIFT,
            vk: VK_SPACE.0 as u32,
        }));
        globals.voice_enabled.store(true, Ordering::Release);

        assert!(!key(&globals, VK_LCONTROL.0 as u32, true, 0));
        assert!(!key(&globals, VK_LSHIFT.0 as u32, true, 0));
        assert!(!key(&globals, VK_LMENU.0 as u32, true, 0));
        assert!(!key(&globals, VK_SPACE.0 as u32, true, 0));
        assert!(received.try_recv().is_err());

        globals
            .held_modifiers
            .store(MOD_CTRL | MOD_SHIFT, Ordering::Release);
        globals.voice_enabled.store(false, Ordering::Release);
        assert!(!key(&globals, VK_SPACE.0 as u32, true, 0));
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn external_injected_input_is_allowed_but_our_marker_is_filtered() {
        let (globals, received) = test_globals();
        globals.set_active(Some(HookChord {
            modifiers: MOD_CTRL,
            vk: VK_A.0 as u32,
        }));
        globals.voice_enabled.store(true, Ordering::Release);
        globals.held_modifiers.store(MOD_CTRL, Ordering::Release);

        assert!(!key(&globals, VK_A.0 as u32, true, SELF_INJECTED_MARKER));
        assert!(received.try_recv().is_err());
        assert!(key(&globals, VK_A.0 as u32, true, 0x1234));
        assert_eq!(received.try_recv(), Ok(HookEvent::ActivePressed));
    }
}
