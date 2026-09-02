use super::{PreInputPayload, PREINPUT_LABEL};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const PREINPUT_SHOW_EVENT: &str = "preinput_show";
const PREINPUT_UPDATE_EVENT: &str = "preinput_update";
const PREINPUT_HIDE_EVENT: &str = "preinput_hide";
const PREINPUT_WIDTH: f64 = 360.0;
const PREINPUT_HEIGHT: f64 = 88.0;

pub fn setup_preinput_window(app: &AppHandle) -> tauri::Result<()> {
    let window = ensure_preinput_window(app)?;
    window.hide()?;
    Ok(())
}

pub(super) fn show(app: &AppHandle, payload: &PreInputPayload) -> tauri::Result<()> {
    let window = ensure_preinput_window(app)?;
    if let Some(position) = overlay_position(app) {
        if let Err(error) = window.set_position(position) {
            log::warn!("failed to position preinput overlay: {error}");
        }
    }
    if let Err(error) = window.show() {
        log::warn!("failed to show preinput overlay: {error}");
    }
    emit(app, PREINPUT_SHOW_EVENT, payload);
    emit(app, PREINPUT_UPDATE_EVENT, payload);
    Ok(())
}

pub(super) fn emit_update(app: &AppHandle, payload: &PreInputPayload) {
    emit(app, PREINPUT_UPDATE_EVENT, payload);
}

pub(super) fn hide(app: &AppHandle, payload: &PreInputPayload) {
    if let Some(window) = app.get_webview_window(PREINPUT_LABEL) {
        emit(app, PREINPUT_HIDE_EVENT, payload);
        if let Err(error) = window.hide() {
            log::warn!("failed to hide preinput overlay: {error}");
        }
    }
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
    .title("Zephyr Preview")
    .inner_size(PREINPUT_WIDTH, PREINPUT_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()
}

fn emit<T: serde::Serialize + Clone>(app: &AppHandle, event: &str, payload: &T) {
    if let Err(error) = app.emit_to(PREINPUT_LABEL, event, payload.clone()) {
        log::warn!("failed to emit {event} to preinput overlay: {error}");
    }
}

fn overlay_position(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    bottom_center_position(app)
}

fn bottom_center_position(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    let (x, y, width, height) = target_screen_rect().or_else(|| primary_screen_rect(app))?;

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

fn target_screen_rect() -> Option<(i32, i32, u32, u32)> {
    foreground_monitor_rect().or_else(cursor_monitor_rect)
}

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
