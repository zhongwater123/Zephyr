use crate::services::ConfigService;
use crate::shortcut_manager::ShortcutManager;
use crate::voice_controller::VoiceSessionController;
use crate::SharedRuntime;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

pub fn setup(
    app: &AppHandle,
    runtime: SharedRuntime,
    config_service: Arc<ConfigService>,
    controller: VoiceSessionController,
    shortcut_manager: Arc<ShortcutManager>,
) -> tauri::Result<()> {
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
                let next_enabled = runtime
                    .lock()
                    .map(|runtime| !runtime.machine.is_enabled())
                    .unwrap_or(false);
                if !next_enabled {
                    controller.request_cancel(app);
                }
                let current = config_service.snapshot();
                let mut next_config = current.clone();
                next_config.enabled = next_enabled;
                next_config.revision = next_config.revision.saturating_add(1);
                if let Err(error) = config_service.commit_config(current.revision, next_config) {
                    log::warn!("failed to persist tray toggle: {error:?}");
                    return;
                }
                let payload = {
                    let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
                    runtime.machine.set_enabled(next_enabled);
                    runtime.voice_state_payload()
                };
                if let Err(error) = app.emit("voice_state_changed", payload) {
                    log::warn!("failed to emit tray toggle state: {error}");
                }
                if next_enabled {
                    let shortcut_manager = shortcut_manager.clone();
                    tauri::async_runtime::spawn_blocking(move || {
                        if let Err(error) = shortcut_manager.set_enabled(true) {
                            log::error!(
                                target: "shortcut_edit_trace",
                                "event=tray_enable_failed phase=enable enabled=true result=failed errorCode=hook_unavailable message={:?}",
                                error
                            );
                        }
                    });
                } else if let Err(error) = shortcut_manager.set_enabled(false) {
                    log::error!(
                        target: "shortcut_edit_trace",
                        "event=tray_enable_failed phase=enable enabled=false result=failed errorCode=hook_unavailable message={:?}",
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
