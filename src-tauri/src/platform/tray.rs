use crate::voice_input_service::VoiceControlService;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn setup(app: &AppHandle, voice_control: VoiceControlService) -> tauri::Result<()> {
    let open_settings = MenuItem::with_id(app, "open_settings", "打开设置", true, None::<&str>)?;
    let toggle_enabled =
        MenuItem::with_id(app, "toggle_enabled", "暂停 / 继续", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_settings, &toggle_enabled, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("GY Typing")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open_settings" => show_main_window(app),
            "toggle_enabled" => {
                if let Err(error) = voice_control.toggle_from_current() {
                    log::error!(
                        target: "shortcut_edit_trace",
                        "event=tray_toggle_failed phase=enable result=failed errorCode=voice_control_failed message={:?}",
                        error
                    );
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
