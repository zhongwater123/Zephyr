mod audio;
mod config;
mod history;
mod hotwords;
mod hotkey;
mod inject;
mod overlay;
mod preview;
mod provider;
mod shortcut;
mod state;

use audio::Recorder;
use config::{AppConfig, ConfigError};
use hotwords::{HotwordSettingsInput, HotwordState};
use inject::{ClipboardTextInjector, TextInjector};
use preview::TranscriptPreviewState;
use provider::{
    AudioChunk, AudioStreamInfo, MockProvider, ProviderError, StreamingTranscriptionProvider,
    VolcengineAsrProvider, VolcengineAuth,
};
use history::HistoryItem;
use serde::Serialize;
use shortcut::ShortcutValidation;
use state::AppStateMachine;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

pub type SharedRuntime = Arc<Mutex<VoiceRuntime>>;

pub struct ActiveSession {
    pub session_id: u64,
    pub provider_task: tauri::async_runtime::JoinHandle<Result<String, ProviderError>>,
    pub preview_state: Arc<tokio::sync::Mutex<TranscriptPreviewState>>,
    pub app_context: history::AppContext,
}

#[derive(Debug, Clone, Serialize)]
struct ConfigStatus {
    has_api_key: bool,
    has_app_key: bool,
    has_access_key: bool,
    provider_ready: bool,
    provider_message: String,
}

pub struct VoiceRuntime {
    pub machine: AppStateMachine,
    pub recorder: Recorder,
    pub provider: Arc<dyn StreamingTranscriptionProvider>,
    pub injector: Arc<dyn TextInjector>,
    pub config: AppConfig,
    pub active_session: Option<ActiveSession>,
    pub registered_shortcut: Option<tauri_plugin_global_shortcut::Shortcut>,
    pub registered_shortcut_label: String,
}

impl VoiceRuntime {
    fn new(config: AppConfig) -> Self {
        let mut machine = AppStateMachine::new();
        machine.set_enabled(config.enabled);
        let mut recorder = Recorder::new();
        if let Err(error) = recorder.warm_up() {
            log::warn!("failed to warm up microphone input; will retry on first recording: {error}");
        }
        Self {
            machine,
            recorder,
            provider: provider_from_config(&config),
            injector: Arc::new(ClipboardTextInjector),
            config,
            active_session: None,
            registered_shortcut: None,
            registered_shortcut_label: String::new(),
        }
    }
}

#[tauri::command]
fn get_config(runtime: State<'_, SharedRuntime>) -> Result<AppConfig, String> {
    Ok(runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone())
}

#[tauri::command]
fn get_config_status(runtime: State<'_, SharedRuntime>) -> Result<ConfigStatus, String> {
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    Ok(config_status(&config))
}

#[tauri::command]
fn get_preinput_payload() -> Option<overlay::PreInputPayload> {
    overlay::current_preinput_payload()
}

#[tauri::command]
fn list_history(
    query: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<Vec<HistoryItem>, String> {
    history::list_history(query, limit, offset).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_history(id: String, text: String) -> Result<(), String> {
    history::update_history(&id, &text).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_history(id: String) -> Result<(), String> {
    history::delete_history(&id).map_err(|error| error.to_string())
}

#[tauri::command]
fn clear_history() -> Result<(), String> {
    history::clear_history().map_err(|error| error.to_string())
}

#[tauri::command]
fn copy_history_text(id: String) -> Result<(), String> {
    let text = history::get_history_text(&id).map_err(|error| error.to_string())?;
    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_text(text)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn validate_shortcut(
    shortcut: String,
    app: AppHandle,
    runtime: State<'_, SharedRuntime>,
) -> Result<ShortcutValidation, String> {
    hotkey::validate_shortcut(&app, runtime.inner().clone(), &shortcut).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_shortcut(
    shortcut: String,
    app: AppHandle,
    runtime: State<'_, SharedRuntime>,
) -> Result<AppConfig, String> {
    hotkey::save_shortcut(&app, runtime.inner().clone(), &shortcut)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_shortcut(app: AppHandle, runtime: State<'_, SharedRuntime>) -> Result<AppConfig, String> {
    hotkey::save_shortcut(&app, runtime.inner().clone(), shortcut::DEFAULT_SHORTCUT)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_hotword_state(runtime: State<'_, SharedRuntime>) -> Result<HotwordState, String> {
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_hotword_settings(
    settings: HotwordSettingsInput,
    api_key: Option<String>,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    if let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) {
        config::save_hotword_agent_api_key(&api_key).map_err(display_config_error)?;
    }

    let config = {
        let mut runtime = runtime.lock().map_err(|error| error.to_string())?;
        runtime.config.hotwords_enabled = settings.hotwords_enabled;
        runtime.config.hotword_agent_enabled = settings.hotword_agent_enabled;
        runtime.config.hotword_agent_base_url = settings.hotword_agent_base_url;
        runtime.config.hotword_agent_model = settings.hotword_agent_model;
        config::save_config(&runtime.config).map_err(display_config_error)?;
        runtime.config.clone()
    };

    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_manual_hotwords(
    words: Vec<String>,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::save_manual_hotwords(words).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn add_hotword(word: String, runtime: State<'_, SharedRuntime>) -> Result<HotwordState, String> {
    hotwords::add_hotword(&word).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_hotword(
    old_word: String,
    new_word: String,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::update_hotword(&old_word, &new_word).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_hotword(word: String, runtime: State<'_, SharedRuntime>) -> Result<HotwordState, String> {
    hotwords::delete_hotword(&word).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
async fn organize_hotwords_now(runtime: State<'_, SharedRuntime>) -> Result<HotwordState, String> {
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::organize_hotwords(config, true)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn test_hotword_agent(runtime: State<'_, SharedRuntime>) -> Result<String, String> {
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::test_agent_connection(config)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_agent_hotword(
    word: String,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::delete_agent_hotword(&word).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn promote_agent_hotword(
    word: String,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::promote_agent_hotword(&word).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_profile_context(
    text: String,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::update_profile_context(&text).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_app_context(
    app_name: String,
    context: String,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::update_app_context(&app_name, &context).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_app_context(
    app_name: String,
    runtime: State<'_, SharedRuntime>,
) -> Result<HotwordState, String> {
    hotwords::delete_app_context(&app_name).map_err(|error| error.to_string())?;
    let config = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .config
        .clone();
    hotwords::get_state(&config).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_config(
    config: AppConfig,
    api_key: Option<String>,
    app_key: Option<String>,
    access_key: Option<String>,
    hotword_agent_api_key: Option<String>,
    runtime: State<'_, SharedRuntime>,
) -> Result<AppConfig, String> {
    config::save_config(&config).map_err(display_config_error)?;
    if let Some(api_key) = api_key.filter(|key| !key.trim().is_empty()) {
        config::save_api_key(&api_key).map_err(display_config_error)?;
    }
    if let Some(app_key) = app_key.filter(|key| !key.trim().is_empty()) {
        config::save_app_key(&app_key).map_err(display_config_error)?;
    }
    if let Some(access_key) = access_key.filter(|key| !key.trim().is_empty()) {
        config::save_access_key(&access_key).map_err(display_config_error)?;
    }
    if let Some(api_key) = hotword_agent_api_key.filter(|key| !key.trim().is_empty()) {
        config::save_hotword_agent_api_key(&api_key).map_err(display_config_error)?;
    }

    let mut runtime = runtime.lock().map_err(|error| error.to_string())?;
    runtime.machine.set_enabled(config.enabled);
    runtime.provider = provider_from_config(&config);
    runtime.config = config.clone();
    Ok(config)
}

#[tauri::command]
fn set_enabled(
    enabled: bool,
    app: AppHandle,
    runtime: State<'_, SharedRuntime>,
) -> Result<(), String> {
    let payload = {
        let mut runtime = runtime.lock().map_err(|error| error.to_string())?;
        runtime.config.enabled = enabled;
        config::save_config(&runtime.config).map_err(display_config_error)?;
        runtime.machine.set_enabled(enabled)
    };
    app.emit("voice_state_changed", payload)
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
async fn test_provider(runtime: State<'_, SharedRuntime>) -> Result<String, String> {
    let config = {
        let runtime = runtime.lock().map_err(|error| error.to_string())?;
        runtime.config.clone()
    };

    let status = config_status(&config);
    if !status.provider_ready {
        return Ok(status.provider_message);
    }

    if config.provider.base_url.starts_with("wss://") {
        let auth = load_provider_auth().map_err(display_config_error)?;
        let provider = VolcengineAsrProvider::new(
            config.provider.clone(),
            config.recognition_behavior.clone(),
            auth,
        );
        return provider
            .probe_connection()
            .await
            .map_err(|error| error.to_string());
    }

    let provider = MockProvider;
    let (chunk_tx, chunk_rx) = mpsc::unbounded_channel();
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    let stream_info = AudioStreamInfo {
        sample_rate: 16_000,
        channels: 1,
        encoding: "pcm_s16le",
        chunk_duration_ms: 200,
    };

    chunk_tx
        .send(AudioChunk {
            bytes: vec![1, 2, 3],
            duration_ms: 200,
            is_final: false,
        })
        .map_err(|error| error.to_string())?;
    chunk_tx
        .send(AudioChunk {
            bytes: Vec::new(),
            duration_ms: 0,
            is_final: true,
        })
        .map_err(|error| error.to_string())?;
    drop(chunk_tx);

    provider
        .transcribe_stream(stream_info, chunk_rx, event_tx, None)
        .await
        .map_err(|error| error.to_string())
}

pub fn run() {
    install_tls_provider();

    let config = config::load_config().unwrap_or_default();
    let runtime: SharedRuntime = Arc::new(Mutex::new(VoiceRuntime::new(config)));

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("rustls", log::LevelFilter::Warn)
                .level_for("tungstenite", log::LevelFilter::Warn)
                .level_for("tao", log::LevelFilter::Warn)
                .build(),
        )
        .manage(runtime.clone())
        .invoke_handler(tauri::generate_handler![
            get_config,
            get_config_status,
            get_preinput_payload,
            list_history,
            update_history,
            delete_history,
            clear_history,
            copy_history_text,
            validate_shortcut,
            save_shortcut,
            reset_shortcut,
            get_hotword_state,
            save_hotword_settings,
            save_manual_hotwords,
            add_hotword,
            update_hotword,
            delete_hotword,
            organize_hotwords_now,
            test_hotword_agent,
            delete_agent_hotword,
            promote_agent_hotword,
            update_profile_context,
            update_app_context,
            delete_app_context,
            save_config,
            set_enabled,
            test_provider
        ])
        .setup(move |app| {
            overlay::setup_preinput_window(app.handle())?;
            hotkey::register_voice_shortcut(app, runtime.clone())?;
            setup_tray(app.handle(), runtime.clone())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn install_tls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn display_config_error(error: ConfigError) -> String {
    error.to_string()
}

fn config_status(config: &AppConfig) -> ConfigStatus {
    let has_api_key = config::has_api_key();
    let has_app_key = config::has_app_key();
    let has_access_key = config::has_access_key();
    let base_url = config.provider.base_url.trim();
    let resource_id = config.provider.resource_id.trim();
    let model = config.provider.model.trim();
    let uses_api_key = config
        .provider
        .auth_mode
        .trim()
        .eq_ignore_ascii_case("api_key");

    if base_url.is_empty() {
        return ConfigStatus {
            has_api_key,
            has_app_key,
            has_access_key,
            provider_ready: false,
            provider_message: "服务地址不能为空。".to_string(),
        };
    }

    if base_url != "mock" && !base_url.starts_with("wss://") {
        return ConfigStatus {
            has_api_key,
            has_app_key,
            has_access_key,
            provider_ready: false,
            provider_message: "服务地址应为 mock 或 wss:// WebSocket 地址。".to_string(),
        };
    }

    if base_url.starts_with("wss://") && uses_api_key && !has_api_key {
        return ConfigStatus {
            has_api_key,
            has_app_key,
            has_access_key,
            provider_ready: false,
            provider_message: "接口密钥尚未保存到 Windows 凭据管理器。".to_string(),
        };
    }

    if base_url.starts_with("wss://") && !uses_api_key && (!has_app_key || !has_access_key) {
        return ConfigStatus {
            has_api_key,
            has_app_key,
            has_access_key,
            provider_ready: false,
            provider_message: "官方 SAUC 鉴权需要应用密钥和访问密钥。".to_string(),
        };
    }

    if base_url.starts_with("wss://") && resource_id.is_empty() {
        return ConfigStatus {
            has_api_key,
            has_app_key,
            has_access_key,
            provider_ready: false,
            provider_message: "火山引擎 ASR 需要资源标识。".to_string(),
        };
    }

    if model.is_empty() {
        return ConfigStatus {
            has_api_key,
            has_app_key,
            has_access_key,
            provider_ready: false,
            provider_message: "模型名称不能为空。".to_string(),
        };
    }

    ConfigStatus {
        has_api_key,
        has_app_key,
        has_access_key,
        provider_ready: true,
        provider_message: if base_url == "mock" {
            "模拟识别服务已就绪。".to_string()
        } else if !base_url.trim_end_matches('/').ends_with("/bigmodel_async") {
            "云端识别已配置。建议使用 bigmodel_async 地址以获得双向流式体验。".to_string()
        } else {
            "双向流式识别已配置。按住快捷键即可测试实时增量结果。".to_string()
        },
    }
}

fn provider_from_config(config: &AppConfig) -> Arc<dyn StreamingTranscriptionProvider> {
    let base_url = config.provider.base_url.trim();
    if base_url == "mock" || !base_url.starts_with("wss://") {
        return Arc::new(MockProvider);
    }

    match load_provider_auth() {
        Ok(auth) => Arc::new(VolcengineAsrProvider::new(
            config.provider.clone(),
            config.recognition_behavior.clone(),
            auth,
        )),
        Err(error) => {
            log::warn!("failed to load provider credentials; using MockProvider: {error}");
            Arc::new(MockProvider)
        }
    }
}

fn load_provider_auth() -> Result<VolcengineAuth, ConfigError> {
    Ok(VolcengineAuth {
        api_key: config::load_api_key()?,
        app_key: config::load_app_key()?,
        access_key: config::load_access_key()?,
    })
}

fn setup_tray(app: &AppHandle, runtime: SharedRuntime) -> tauri::Result<()> {
    let open_settings =
        MenuItem::with_id(app, "open_settings", "打开设置", true, None::<&str>)?;
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
                let payload = {
                    let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
                    let next_enabled = !runtime.machine.is_enabled();
                    runtime.config.enabled = next_enabled;
                    if let Err(error) = config::save_config(&runtime.config) {
                        log::warn!("failed to persist tray toggle: {error}");
                    }
                    runtime.machine.set_enabled(next_enabled)
                };
                if let Err(error) = app.emit("voice_state_changed", payload) {
                    log::warn!("failed to emit tray toggle state: {error}");
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
