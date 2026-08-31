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
mod shortcut_manager;
mod state;
mod streaming_pipeline;
mod target;
mod text_processing;
mod voice_controller;
mod voice_input_service;
mod voice_trigger;
mod windows_keyboard;

use config::{AppConfig, ConfigRecovery};
use std::sync::Arc;
use tauri::Manager;

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
                .level_for("asr_trace", log::LevelFilter::Debug)
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
            commands::config::set_shortcut_trigger_mode,
            commands::config::set_incident_recovery_enabled,
            commands::provider::test_provider
        ])
        .setup(move |app| {
            let pending = Arc::new(pending_output_service::PendingOutputService::default());
            let initial_config = voice_services.config.snapshot();
            let voice = voice_controller::VoiceSessionHandle::spawn(
                app.handle().clone(),
                initial_config.enabled,
                initial_config.revision,
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
            if let Some(voice) = app_handle.try_state::<voice_controller::VoiceSessionHandle>() {
                tauri::async_runtime::block_on(voice.shutdown());
            }
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
    use std::path::Path;
    use std::path::PathBuf;

    fn rust_sources(root: &Path) -> Vec<PathBuf> {
        let mut sources = Vec::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources.extend(rust_sources(&path));
            } else if path.extension().and_then(|value| value.to_str()) == Some("rs") {
                sources.push(path);
            }
        }
        sources
    }

    #[test]
    fn external_voice_layers_cannot_reach_mutable_runtime() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let forbidden = [
            "SharedRuntime",
            "runtime.lock",
            "runtime.provider",
            "sessions.pending_outputs",
            "SessionEvent",
            "VoiceRuntime",
            "SessionResources",
        ];

        let mut files = Vec::new();
        for root in ["src/commands", "src/platform", "src/shortcut_manager"] {
            files.extend(rust_sources(&manifest.join(root)));
        }
        files.extend([
            manifest.join("src/streaming_pipeline.rs"),
            manifest.join("src/delivery.rs"),
        ]);
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            for token in forbidden {
                assert!(
                    !source.contains(token),
                    "{} must not depend on voice runtime token {token}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn automatic_delivery_cannot_restore_live_ole_clipboard_objects() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut files = vec![
            manifest.join("src/delivery.rs"),
            manifest.join("src/inject.rs"),
        ];
        files.extend(rust_sources(&manifest.join("src/voice_controller")));
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            for forbidden in [
                "OleGetClipboard",
                "OleSetClipboard",
                "OleFlushClipboard",
                "IDataObject",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} must not use forbidden automatic clipboard token {forbidden}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn voice_runtime_has_exactly_one_source_writer() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let voice_root = manifest.join("src/voice_controller");
        let actor = std::fs::read_to_string(voice_root.join("actor.rs")).unwrap();
        let runtime = std::fs::read_to_string(voice_root.join("actor").join("runtime.rs")).unwrap();

        assert!(runtime.contains("struct VoiceRuntime"));
        assert!(actor.contains("runtime: VoiceRuntime"));
        assert!(!actor.contains("runtime.lock"));
        assert!(!actor.contains("Arc<Mutex<VoiceRuntime>>"));
        assert!(!runtime.contains("Arc<Mutex<VoiceRuntime>>"));

        for path in rust_sources(&voice_root.join("workflow"))
            .into_iter()
            .chain([voice_root.join("workflow.rs")])
        {
            let source = std::fs::read_to_string(&path).unwrap();
            for forbidden in [
                "VoiceRuntime",
                "AppStateMachine",
                "SharedRuntime",
                "runtime.lock",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} must not depend on actor-owned state token {forbidden}",
                    path.display()
                );
            }
        }

        for forbidden in [
            "Recorder",
            "TextInjector",
            "PreparedSession",
            "SessionResources",
            "provider_task",
            "PendingOutputLease",
            "AppHandle",
        ] {
            assert!(
                !runtime.contains(forbidden),
                "VoiceRuntime must not contain execution resource token {forbidden}"
            );
        }

        let all_voice_source = rust_sources(&voice_root)
            .into_iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect::<String>();
        assert!(!all_voice_source.contains("SharedRuntime"));
        assert!(!all_voice_source.contains("Arc<Mutex<VoiceRuntime>>"));

        let mut actor_implementation = rust_sources(&voice_root.join("actor"));
        actor_implementation.push(voice_root.join("actor.rs"));
        for path in actor_implementation {
            let file_name = path.file_name().and_then(|value| value.to_str());
            if matches!(file_name, Some("runtime.rs" | "reducer.rs")) {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            for forbidden in [
                "runtime.set_payload(",
                "runtime.replace_payload(",
                "runtime.set_desired(",
                "runtime.mark_shutting_down(",
                "runtime.clear_current(",
                "runtime.record_outcome(",
                "runtime.phase = VoicePhase",
                "runtime.availability = VoiceAvailability",
                "runtime.last_metrics = Some",
                "runtime.shortcut_registration_error = error",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} bypasses the private reducer with {forbidden}",
                    path.display()
                );
            }
        }

        let lib_source = std::fs::read_to_string(manifest.join("src/lib.rs")).unwrap();
        assert!(!lib_source.contains(&["struct", "ActiveSession"].join(" ")));
        assert!(!lib_source.contains(&["struct", "SessionCancellation"].join(" ")));
    }

    #[test]
    fn spawned_voice_workers_do_not_capture_runtime() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let voice_root = manifest.join("src/voice_controller");
        let mut workers = rust_sources(&voice_root.join("workflow"));
        workers.push(voice_root.join("workflow.rs"));
        workers.push(manifest.join("src/streaming_pipeline.rs"));
        for path in workers {
            let source = std::fs::read_to_string(&path).unwrap();
            assert!(!source.contains("VoiceRuntime"));
            assert!(!source.contains("AppStateMachine"));
            assert!(!source.contains("runtime.lock"));
        }
    }

    #[test]
    fn presenter_is_the_only_voice_ui_gateway() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let voice_root = manifest.join("src/voice_controller");
        for path in rust_sources(&voice_root) {
            if path.file_name().and_then(|value| value.to_str()) == Some("presenter.rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).unwrap();
            assert!(
                !source.contains(".emit(") && !source.contains("overlay::"),
                "{} bypasses VoicePresenter",
                path.display()
            );
        }
        let streaming =
            std::fs::read_to_string(manifest.join("src/streaming_pipeline.rs")).unwrap();
        assert!(!streaming.contains("AppHandle"));
        assert!(!streaming.contains(".emit("));
        assert!(!streaming.contains("overlay::"));
    }
}
