mod audio;
mod command_error;
mod commands;
mod config;
mod delivery;
mod history;
mod hotwords;
mod incident;
mod inject;
mod overlay;
mod pending_output_service;
mod physical_shortcut;
mod platform;
mod preview;
mod provider;
mod provider_model;
mod repositories;
mod runtime_metrics;
mod services;
mod session;
mod shortcut_manager;
mod state;
mod streaming_pipeline;
mod target;
mod voice_controller;
mod voice_input_service;
mod voice_trigger;
mod windows_keyboard;

use config::{AppConfig, ConfigRecovery};
use preview::TranscriptPreviewState;
use provider::ProviderError;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tauri::Manager;
use tokio::sync::Notify;

#[derive(Debug, Default)]
pub struct SessionCancellation {
    cancelled: AtomicBool,
    notify: Notify,
}

impl SessionCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub async fn cancelled(&self) {
        let notified = self.notify.notified();
        if self.is_cancelled() {
            return;
        }
        notified.await;
    }
}

pub struct ActiveSession {
    pub session_id: u64,
    pub attempt_id: String,
    pub provider_task: tauri::async_runtime::JoinHandle<()>,
    pub provider_result: tokio::sync::oneshot::Receiver<Result<String, ProviderError>>,
    pub preview_state: Arc<tokio::sync::Mutex<TranscriptPreviewState>>,
    pub app_context: history::AppContext,
    pub target: target::TargetWindowIdentity,
    pub cancellation: Arc<SessionCancellation>,
    pub deadline_cancellation: Arc<SessionCancellation>,
    pub audio_queue: Arc<audio::AudioQueueMonitor>,
    pub started_at: Instant,
    pub config: AppConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetrics {
    pub session_id: u64,
    pub audio_packets: u64,
    pub queue_high_watermark: usize,
    pub overflow: bool,
    pub recording_duration_ms: u64,
    pub cancel_reason: Option<String>,
    pub final_state: String,
}

pub fn run() {
    install_tls_provider();
    runtime_metrics::install();

    let loaded = config::load_config_with_status().unwrap_or_else(|error| {
        log::error!("failed to load configuration safely: {error}");
        let config = AppConfig {
            enabled: false,
            ..AppConfig::default()
        };
        config::LoadedConfig {
            config,
            recovery: ConfigRecovery::DisabledDefaults,
        }
    });
    let app_services = services::AppServices::production(loaded.clone())
        .expect("failed to initialize application services");
    let voice_services = app_services.clone();
    let shutdown_incidents = app_services.incidents.clone();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .level_for("shortcut_edit_trace", log::LevelFilter::Debug)
                .level_for("rustls", log::LevelFilter::Warn)
                .level_for("tungstenite", log::LevelFilter::Warn)
                .level_for("tao", log::LevelFilter::Warn)
                .max_file_size(2_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .build(),
        )
        .manage(app_services)
        .invoke_handler(tauri::generate_handler![
            commands::config::get_config,
            commands::asr::get_asr_option_pool,
            commands::asr::set_asr_option,
            commands::config::get_config_status,
            commands::session::get_preinput_payload,
            commands::session::get_voice_state,
            commands::session::list_pending_outputs,
            commands::session::get_session_metrics,
            commands::session::deliver_pending_output,
            commands::session::copy_pending_output,
            commands::session::discard_pending_output,
            commands::config::authorize_endpoint,
            commands::config::revoke_endpoint,
            commands::config::set_clipboard_compatibility,
            commands::history::list_history,
            commands::history::update_history,
            commands::history::delete_history,
            commands::history::clear_history,
            commands::history::copy_history_text,
            commands::incident::list_incidents,
            commands::incident::get_incident_health,
            commands::incident::copy_incident_text,
            commands::incident::get_incident_audio,
            commands::incident::export_incident_report,
            commands::incident::save_incident_audio,
            commands::incident::save_incident_report,
            commands::incident::delete_incident,
            commands::incident::set_incident_pinned,
            commands::incident::record_frontend_incident,
            commands::shortcut::begin_shortcut_edit,
            commands::shortcut::commit_shortcut_edit,
            commands::shortcut::cancel_shortcut_edit,
            commands::shortcut::record_shortcut_edit_trace,
            commands::hotwords::get_hotword_state,
            commands::hotwords::save_hotword_settings,
            commands::hotwords::save_manual_hotwords,
            commands::hotwords::add_hotword,
            commands::hotwords::update_hotword,
            commands::hotwords::delete_hotword,
            commands::hotwords::organize_hotwords_now,
            commands::hotwords::test_hotword_agent,
            commands::hotwords::delete_agent_hotword,
            commands::hotwords::promote_agent_hotword,
            commands::hotwords::update_profile_context,
            commands::hotwords::update_app_context,
            commands::hotwords::delete_app_context,
            commands::config::save_config,
            commands::config::set_enabled,
            commands::config::set_history_enabled,
            commands::config::set_incident_recovery_enabled,
            commands::provider::test_provider
        ])
        .setup(move |app| {
            let pending = Arc::new(pending_output_service::PendingOutputService::default());
            let enabled = voice_services.config.snapshot().enabled;
            let voice = voice_controller::VoiceSessionHandle::spawn(
                app.handle().clone(),
                enabled,
                voice_services.clone(),
                pending.clone(),
            );
            let shortcut_manager = shortcut_manager::ShortcutManager::initialize(
                app,
                voice_services.clone(),
                voice.clone(),
            )?;
            let voice_control = voice_input_service::VoiceControlService::new(
                voice_services.config.clone(),
                voice.clone(),
                shortcut_manager.clone(),
            );
            app.manage(voice);
            app.manage(pending);
            app.manage(shortcut_manager.clone());
            app.manage(voice_control.clone());
            overlay::setup_preinput_window(app.handle())?;
            platform::tray::setup(app.handle(), voice_control)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(move |app_handle, event| match event {
        tauri::RunEvent::Resumed => {
            if let Some(manager) = app_handle.try_state::<Arc<shortcut_manager::ShortcutManager>>()
            {
                let manager = manager.inner().clone();
                tauri::async_runtime::spawn_blocking(move || manager.resume());
            }
        }
        tauri::RunEvent::Exit => {
            if let Some(manager) = app_handle.try_state::<Arc<shortcut_manager::ShortcutManager>>()
            {
                manager.shutdown();
            }
            shutdown_incidents.shutdown();
        }
        _ => {}
    });
}

fn install_tls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn session_cancellation_wakes_waiters_and_is_sticky() {
        let cancellation = Arc::new(SessionCancellation::default());
        let waiter_cancellation = cancellation.clone();
        let waiter = tokio::spawn(async move {
            waiter_cancellation.cancelled().await;
        });

        tokio::task::yield_now().await;
        cancellation.cancel();

        timeout(Duration::from_millis(100), waiter)
            .await
            .expect("cancellation waiter should wake")
            .expect("cancellation waiter should not panic");
        timeout(Duration::from_millis(100), cancellation.cancelled())
            .await
            .expect("late waiter should observe prior cancellation");
    }

    #[test]
    fn external_voice_layers_cannot_reach_mutable_runtime() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let files = [
            "src/commands/asr.rs",
            "src/commands/config.rs",
            "src/commands/session.rs",
            "src/platform/tray.rs",
            "src/shortcut_manager/mod.rs",
            "src/streaming_pipeline.rs",
            "src/delivery.rs",
        ];
        let forbidden = [
            "SharedRuntime",
            "runtime.lock",
            "runtime.provider",
            "sessions.pending_outputs",
            "SessionEvent",
        ];

        for relative in files {
            let source = std::fs::read_to_string(manifest.join(relative)).unwrap();
            for token in forbidden {
                assert!(
                    !source.contains(token),
                    "{relative} must not depend on mutable voice runtime token {token}"
                );
            }
        }
    }
}
