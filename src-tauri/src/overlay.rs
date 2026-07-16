use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const PREINPUT_LABEL: &str = "preinput";
const PREINPUT_SHOW_EVENT: &str = "preinput_show";
const PREINPUT_UPDATE_EVENT: &str = "preinput_update";
const PREINPUT_HIDE_EVENT: &str = "preinput_hide";
const PREINPUT_WIDTH: f64 = 520.0;
const PREINPUT_HEIGHT: f64 = 124.0;
const PREINPUT_EMIT_COALESCE_MS: u64 = 30;
static PREINPUT_STORE: OnceLock<Mutex<PreInputStore>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreInputPayload {
    pub session_id: u64,
    pub seq: u64,
    pub text: String,
    pub state: PreInputState,
    pub confirmed_chars: Option<usize>,
    pub message: Option<String>,
}

#[derive(Debug, Default)]
struct PreInputStore {
    current: Option<PreInputPayload>,
    current_session_id: u64,
    closed_session_id: u64,
    next_seq: u64,
    last_emit_at: Option<Instant>,
    delayed_emit_scheduled: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PreInputState {
    Recording,
    Transcribing,
    Finalizing,
    Dismissing,
    Error,
}

pub fn setup_preinput_window(app: &AppHandle) -> tauri::Result<()> {
    let window = ensure_preinput_window(app)?;
    window.hide()?;
    Ok(())
}

pub fn show_preinput(app: &AppHandle, payload: PreInputPayload) {
    let Ok(window) = ensure_preinput_window(app) else {
        log::warn!("failed to create preinput overlay window");
        return;
    };

    if let Some(position) = overlay_position(app) {
        if let Err(error) = window.set_position(position) {
            log::warn!("failed to position preinput overlay: {error}");
        }
    }

    let Some(payload) = store_preinput_payload(payload) else {
        return;
    };
    if let Err(error) = window.show() {
        log::warn!("failed to show preinput overlay: {error}");
    }
    emit_to_preinput(app, PREINPUT_SHOW_EVENT, &payload);
    emit_to_preinput(app, PREINPUT_UPDATE_EVENT, &payload);
}

pub fn update_preinput(app: &AppHandle, payload: PreInputPayload) {
    if let Some(payload) = store_preinput_payload(payload) {
        emit_update_coalesced(app, payload);
    }
}

pub fn hide_preinput(app: &AppHandle) {
    let session_id = current_preinput_session_id();
    hide_preinput_for_session(app, session_id);
}

pub fn begin_preinput_session() -> u64 {
    if let Ok(mut store) = preinput_store().lock() {
        store.current_session_id = store.current_session_id.saturating_add(1);
        store.next_seq = 0;
        store.current = None;
        store.last_emit_at = None;
        store.delayed_emit_scheduled = false;
        return store.current_session_id;
    }
    0
}

pub fn current_preinput_session_id() -> u64 {
    preinput_store()
        .lock()
        .map(|store| store.current_session_id)
        .unwrap_or(0)
}

pub fn hide_preinput_for_session(app: &AppHandle, session_id: u64) {
    let payload = clear_current_preinput_payload(session_id);
    if let Some(window) = app.get_webview_window(PREINPUT_LABEL) {
        emit_to_preinput(
            app,
            PREINPUT_HIDE_EVENT,
            &payload,
        );
        if let Err(error) = window.hide() {
            log::warn!("failed to hide preinput overlay: {error}");
        }
    }
}

pub fn current_preinput_payload() -> Option<PreInputPayload> {
    preinput_store()
        .lock()
        .ok()
        .and_then(|store| store.current.clone())
}

fn store_preinput_payload(mut payload: PreInputPayload) -> Option<PreInputPayload> {
    if let Ok(mut store) = preinput_store().lock() {
        if payload.session_id <= store.closed_session_id {
            return None;
        }
        if payload.session_id < store.current_session_id {
            return None;
        }
        if payload.session_id > store.current_session_id {
            store.current_session_id = payload.session_id;
            store.next_seq = 0;
            store.last_emit_at = None;
            store.delayed_emit_scheduled = false;
        }
        store.next_seq = store.next_seq.saturating_add(1);
        payload.seq = store.next_seq;
        store.current = Some(payload.clone());
        return Some(payload);
    }
    None
}

fn clear_current_preinput_payload(session_id: u64) -> PreInputPayload {
    if let Ok(mut store) = preinput_store().lock() {
        if session_id >= store.current_session_id {
            store.current_session_id = session_id;
            store.closed_session_id = store.closed_session_id.max(session_id);
            store.current = None;
            store.delayed_emit_scheduled = false;
            store.next_seq = store.next_seq.saturating_add(1);
            return PreInputPayload {
                session_id,
                seq: store.next_seq,
                text: String::new(),
                state: PreInputState::Dismissing,
                confirmed_chars: Some(0),
                message: None,
            };
        }
    }

    PreInputPayload {
        session_id,
        seq: 0,
        text: String::new(),
        state: PreInputState::Dismissing,
        confirmed_chars: Some(0),
        message: None,
    }
}

fn emit_update_coalesced(app: &AppHandle, payload: PreInputPayload) {
    let delay = {
        let Ok(mut store) = preinput_store().lock() else {
            emit_to_preinput(app, PREINPUT_UPDATE_EVENT, &payload);
            return;
        };

        match store.last_emit_at {
            None => {
                store.last_emit_at = Some(Instant::now());
                drop(store);
                emit_to_preinput(app, PREINPUT_UPDATE_EVENT, &payload);
                return;
            }
            Some(last_emit_at)
                if last_emit_at.elapsed() >= Duration::from_millis(PREINPUT_EMIT_COALESCE_MS) =>
            {
                store.last_emit_at = Some(Instant::now());
                store.delayed_emit_scheduled = false;
                drop(store);
                emit_to_preinput(app, PREINPUT_UPDATE_EVENT, &payload);
                return;
            }
            Some(last_emit_at) if !store.delayed_emit_scheduled => {
                store.delayed_emit_scheduled = true;
                Duration::from_millis(PREINPUT_EMIT_COALESCE_MS)
                    .saturating_sub(last_emit_at.elapsed())
            }
            Some(_) => return,
        }
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        let payload = {
            let Ok(mut store) = preinput_store().lock() else {
                return;
            };
            store.delayed_emit_scheduled = false;
            store.last_emit_at = Some(Instant::now());
            store.current.clone()
        };
        if let Some(payload) = payload {
            emit_to_preinput(&app, PREINPUT_UPDATE_EVENT, &payload);
        }
    });
}

fn preinput_store() -> &'static Mutex<PreInputStore> {
    PREINPUT_STORE.get_or_init(|| Mutex::new(PreInputStore::default()))
}

fn ensure_preinput_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window(PREINPUT_LABEL) {
        return Ok(window);
    }

    WebviewWindowBuilder::new(
        app,
        PREINPUT_LABEL,
        WebviewUrl::App("index.html?window=preinput".into()),
    )
    .title("GY Typing Preview")
    .inner_size(PREINPUT_WIDTH, PREINPUT_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()
}

fn emit_to_preinput<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(error) = app.emit(event, payload.clone()) {
        log::warn!("failed to emit {event} globally: {error}");
    }

    if let Some(window) = app.get_webview_window(PREINPUT_LABEL) {
        if let Err(error) = window.emit(event, payload.clone()) {
            log::warn!("failed to emit {event} to preinput overlay: {error}");
        }
    }
}

fn overlay_position(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    bottom_center_position(app)
}

fn bottom_center_position(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    let (x, y, width, height) =
        target_screen_rect(app).or_else(|| primary_screen_rect(app))?;

    Some(PhysicalPosition::new(
        x + ((width as f64 - PREINPUT_WIDTH) / 2.0).max(0.0) as i32,
        y + (height as f64 * 0.72 - PREINPUT_HEIGHT / 2.0).max(0.0) as i32,
    ))
}

fn primary_screen_rect(app: &AppHandle) -> Option<(i32, i32, u32, u32)> {
    let monitor = app.primary_monitor().ok().flatten()?;
    let PhysicalPosition { x, y } = monitor.position();
    let PhysicalSize { width, height } = monitor.size();
    Some((*x, *y, *width, *height))
}

#[cfg(target_os = "windows")]
fn target_screen_rect(_app: &AppHandle) -> Option<(i32, i32, u32, u32)> {
    foreground_monitor_rect().or_else(cursor_monitor_rect)
}

#[cfg(target_os = "windows")]
fn foreground_monitor_rect() -> Option<(i32, i32, u32, u32)> {
    use windows::Win32::Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let foreground = unsafe { GetForegroundWindow() };
    if foreground.0.is_null() {
        return None;
    }

    let monitor = unsafe { MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST) };
    monitor_rect(monitor)
}

#[cfg(target_os = "windows")]
fn cursor_monitor_rect() -> Option<(i32, i32, u32, u32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return None;
    }

    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    monitor_rect(monitor)
}

#[cfg(target_os = "windows")]
fn monitor_rect(monitor: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<(i32, i32, u32, u32)> {
    use std::mem::size_of;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};

    if monitor.0.is_null() {
        return None;
    }

    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };

    if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
        return None;
    }

    let rect = info.rcMonitor;
    let width = rect.right.saturating_sub(rect.left);
    let height = rect.bottom.saturating_sub(rect.top);
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((rect.left, rect.top, width as u32, height as u32))
}

#[cfg(not(target_os = "windows"))]
fn target_screen_rect(_app: &AppHandle) -> Option<(i32, i32, u32, u32)> {
    None
}
