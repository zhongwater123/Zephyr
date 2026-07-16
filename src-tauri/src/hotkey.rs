use crate::overlay::{self, PreInputPayload, PreInputState};
use crate::preview::TranscriptPreviewState;
use crate::provider::{ProviderError, TranscriptEvent};
use crate::shortcut::{self, ShortcutValidation};
use crate::state::{ReleaseDecision, VoiceState, VoiceStatePayload};
use crate::{history, hotwords, ActiveSession, SharedRuntime};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time::{sleep, timeout, Duration};

const VOICE_STATE_EVENT: &str = "voice_state_changed";
const STREAM_CHUNK_MS: u16 = 200;
const FINAL_TRANSCRIPT_TIMEOUT_SECS: u64 = 25;
const EMPTY_TRANSCRIPT_TIMEOUT_MS: u64 = 800;

pub fn register_voice_shortcut(app: &mut tauri::App, runtime: SharedRuntime) -> tauri::Result<()> {
    let handler_runtime = runtime.clone();

    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, incoming_shortcut, event| {
                let is_current_shortcut = handler_runtime
                    .lock()
                    .map(|runtime| runtime.registered_shortcut == Some(*incoming_shortcut))
                    .unwrap_or(false);
                if !is_current_shortcut {
                    return;
                }

                match event.state() {
                    ShortcutState::Pressed => handle_pressed(app, handler_runtime.clone()),
                    ShortcutState::Released => handle_released(app, handler_runtime.clone()),
                }
            })
            .build(),
    )?;

    let configured_shortcut = runtime
        .lock()
        .map(|runtime| runtime.config.shortcut.clone())
        .unwrap_or_else(|_| shortcut::DEFAULT_SHORTCUT.to_string());
    let parsed = shortcut::parse_shortcut(&configured_shortcut)
        .unwrap_or_else(|_| shortcut::default_shortcut());
    app.handle()
        .global_shortcut()
        .register(parsed.shortcut)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error.to_string()))?;
    if let Ok(mut runtime) = runtime.lock() {
        runtime.registered_shortcut = Some(parsed.shortcut);
        runtime.registered_shortcut_label = parsed.normalized.clone();
        runtime.config.shortcut = parsed.normalized;
    }
    Ok(())
}

pub fn validate_shortcut(
    app: &AppHandle,
    runtime: SharedRuntime,
    shortcut: &str,
) -> Result<ShortcutValidation, String> {
    let parsed = match shortcut::parse_shortcut(shortcut) {
        Ok(parsed) => parsed,
        Err(error) => return Ok(shortcut::validation_error(shortcut, error)),
    };

    let current = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .registered_shortcut;
    if current == Some(parsed.shortcut) {
        return Ok(shortcut::validation_success(
            shortcut,
            parsed.normalized,
            true,
            Some("当前快捷键已在使用。".to_string()),
        ));
    }

    match app.global_shortcut().register(parsed.shortcut) {
        Ok(()) => {
            if let Err(error) = app.global_shortcut().unregister(parsed.shortcut) {
                log::warn!("failed to unregister temporary shortcut validation: {error}");
            }
            Ok(shortcut::validation_success(
                shortcut,
                parsed.normalized,
                true,
                Some("快捷键可用。".to_string()),
            ))
        }
        Err(error) => Ok(shortcut::validation_success(
            shortcut,
            parsed.normalized,
            false,
            Some(format!(
                "该快捷键已被系统或其他应用占用，Zephyr 无法注册。({error})"
            )),
        )),
    }
}

pub fn save_shortcut(
    app: &AppHandle,
    runtime: SharedRuntime,
    shortcut: &str,
) -> Result<crate::config::AppConfig, String> {
    let parsed = shortcut::parse_shortcut(shortcut).map_err(|error| error.message())?;
    let current = runtime
        .lock()
        .map_err(|error| error.to_string())?
        .registered_shortcut;

    if current != Some(parsed.shortcut) {
        app.global_shortcut()
            .register(parsed.shortcut)
            .map_err(|error| {
                format!("该快捷键已被系统或其他应用占用，Zephyr 无法注册。({error})")
            })?;

        if let Some(current) = current {
            if let Err(error) = app.global_shortcut().unregister(current) {
                log::warn!("failed to unregister previous shortcut: {error}");
            }
        }
    }

    let config = {
        let mut runtime = runtime.lock().map_err(|error| error.to_string())?;
        runtime.registered_shortcut = Some(parsed.shortcut);
        runtime.registered_shortcut_label = parsed.normalized.clone();
        runtime.config.shortcut = parsed.normalized;
        crate::config::save_config(&runtime.config).map_err(|error| error.to_string())?;
        runtime.config.clone()
    };

    Ok(config)
}

fn handle_pressed(app: &AppHandle, runtime: SharedRuntime) {
    let (payload, event_rx, preview_state, session_id) = {
        let (chunk_tx, chunk_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let mut voice = runtime.lock().expect("voice runtime lock poisoned");
        let preview_state = Arc::new(tokio::sync::Mutex::new(TranscriptPreviewState::default()));

        let Some(payload) = voice.machine.hotkey_pressed() else {
            return;
        };
        let session_id = overlay::begin_preinput_session();

        let app_context = history::capture_foreground_app();
        let asr_hints = match hotwords::compose_asr_hints(&voice.config, &app_context) {
            Ok(hints) => hints,
            Err(error) => {
                log::warn!("failed to compose ASR hotword hints: {error}");
                None
            }
        };

        let stream_info = match voice.recorder.start_streaming(STREAM_CHUNK_MS, chunk_tx) {
            Ok(stream_info) => stream_info,
            Err(error) => {
                let payload = voice.machine.fail(error.to_string());
                emit_state(app, payload);
                schedule_idle_reset(app.clone(), runtime.clone(), session_id);
                return;
            }
        };

        let provider = voice.provider.clone();
        let provider_task = tauri::async_runtime::spawn(async move {
            provider
                .transcribe_stream(stream_info, chunk_rx, event_tx, asr_hints)
                .await
        });
        voice.active_session = Some(ActiveSession {
            session_id,
            provider_task,
            preview_state: preview_state.clone(),
            app_context,
        });
        (payload, event_rx, preview_state, session_id)
    };

    overlay::show_preinput(
        app,
        PreInputPayload {
            session_id,
            seq: 0,
            text: String::new(),
            state: PreInputState::Recording,
            confirmed_chars: Some(0),
            message: Some("正在聆听".to_string()),
        },
    );
    spawn_transcript_event_relay(
        app.clone(),
        runtime.clone(),
        event_rx,
        preview_state,
        session_id,
    );
    emit_state(app, payload);
}

fn handle_released(app: &AppHandle, runtime: SharedRuntime) {
    let (provider_task, preview_state, injector, history_enabled, app_context, session_id) = {
        let mut voice = runtime.lock().expect("voice runtime lock poisoned");
        if voice.machine.state() != &VoiceState::Recording {
            return;
        }

        let duration = match voice.recorder.stop_streaming() {
            Ok(duration) => duration,
            Err(error) => {
                let payload = voice.machine.fail(error.to_string());
                emit_state(app, payload);
                let session_id = voice
                    .active_session
                    .as_ref()
                    .map(|session| session.session_id)
                    .unwrap_or_else(overlay::current_preinput_session_id);
                schedule_idle_reset(app.clone(), runtime.clone(), session_id);
                return;
            }
        };

        let decision = voice.machine.hotkey_released(duration);
        let Some(session) = voice.active_session.take() else {
            let payload = voice.machine.fail("识别会话不存在");
            emit_state(app, payload);
            schedule_idle_reset(
                app.clone(),
                runtime.clone(),
                overlay::current_preinput_session_id(),
            );
            return;
        };

        match decision {
            ReleaseDecision::Cancelled { payload, .. } => {
                session.provider_task.abort();
                emit_state(app, payload);
                overlay::hide_preinput_for_session(app, session.session_id);
                return;
            }
            ReleaseDecision::Transcribe { payload, .. } => {
                emit_state(app, payload);
            }
        }

        (
            session.provider_task,
            session.preview_state,
            voice.injector.clone(),
            voice.config.history_enabled,
            session.app_context,
            session.session_id,
        )
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut provider_task = provider_task;
        let has_preview_text = !preview_state
            .lock()
            .await
            .rendered_text()
            .trim()
            .is_empty();

        if has_preview_text {
            overlay::update_preinput(
                &app,
                PreInputPayload {
                    session_id,
                    seq: 0,
                    text: preview_state.lock().await.rendered_text(),
                    state: PreInputState::Finalizing,
                    confirmed_chars: None,
                    message: Some("正在收束".to_string()),
                },
            );
        }

        let wait_duration = if has_preview_text {
            Duration::from_secs(FINAL_TRANSCRIPT_TIMEOUT_SECS)
        } else {
            Duration::from_millis(EMPTY_TRANSCRIPT_TIMEOUT_MS)
        };

        let transcript = match timeout(wait_duration, &mut provider_task).await
        {
            Ok(Ok(Ok(transcript))) => transcript,
            Ok(Ok(Err(error))) => {
                if !has_preview_text && is_empty_input_error(&error) {
                    cancel_session_quietly(&app, runtime.clone(), session_id).await;
                    return;
                }
                fail_and_reset(&app, runtime.clone(), error.to_string(), session_id).await;
                return;
            }
            Ok(Err(error)) => {
                fail_and_reset(&app, runtime.clone(), error.to_string(), session_id).await;
                return;
            }
            Err(_) => {
                if has_preview_text {
                    provider_task.abort();
                    fail_and_reset(
                        &app,
                        runtime.clone(),
                        format!(
                            "流式识别在 {FINAL_TRANSCRIPT_TIMEOUT_SECS} 秒内没有返回最终文本"
                        ),
                        session_id,
                    )
                    .await;
                    return;
                } else {
                    let late_preview_text = preview_state.lock().await.rendered_text();
                    if late_preview_text.trim().is_empty() {
                        provider_task.abort();
                        cancel_session_quietly(&app, runtime.clone(), session_id).await;
                        return;
                    } else {
                        overlay::update_preinput(
                            &app,
                            PreInputPayload {
                                session_id,
                                seq: 0,
                                text: late_preview_text,
                                state: PreInputState::Finalizing,
                                confirmed_chars: None,
                                message: Some("正在收束".to_string()),
                            },
                        );

                        match timeout(
                            Duration::from_secs(FINAL_TRANSCRIPT_TIMEOUT_SECS),
                            &mut provider_task,
                        )
                        .await
                        {
                            Ok(Ok(Ok(transcript))) => transcript,
                            Ok(Ok(Err(error))) => {
                                fail_and_reset(
                                    &app,
                                    runtime.clone(),
                                    error.to_string(),
                                    session_id,
                                )
                                .await;
                                return;
                            }
                            Ok(Err(error)) => {
                                fail_and_reset(
                                    &app,
                                    runtime.clone(),
                                    error.to_string(),
                                    session_id,
                                )
                                .await;
                                return;
                            }
                            Err(_) => {
                                provider_task.abort();
                                fail_and_reset(
                                    &app,
                                    runtime.clone(),
                                    format!(
                                        "流式识别在 {FINAL_TRANSCRIPT_TIMEOUT_SECS} 秒内没有返回最终文本"
                                    ),
                                    session_id,
                                )
                                .await;
                                return;
                            }
                        }
                    }
                }
            }
        };

        let final_text = transcript;

        if final_text.trim().is_empty() {
            cancel_session_quietly(&app, runtime.clone(), session_id).await;
            return;
        }

        overlay::update_preinput(
            &app,
            PreInputPayload {
                session_id,
                seq: 0,
                text: final_text.clone(),
                state: PreInputState::Finalizing,
                confirmed_chars: None,
                message: Some("正在写入".to_string()),
            },
        );

        let paste_payload = {
            let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
            runtime.machine.paste_started()
        };
        emit_state(&app, paste_payload);

        let text_to_paste = final_text.clone();
        let paste_result = tauri::async_runtime::spawn_blocking(move || {
            injector.paste_text(&text_to_paste)
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result.map_err(|error| error.to_string()));

        if let Err(error) = paste_result {
            fail_and_reset(&app, runtime.clone(), error, session_id).await;
            return;
        }

        let wrote_history = if history_enabled {
            let text_to_record = final_text.clone();
            let context_to_record = app_context.clone();
            match tauri::async_runtime::spawn_blocking(move || {
                history::insert_transcript(&text_to_record, &context_to_record)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| error.to_string()))
            {
                Ok(_) => true,
                Err(error) => {
                    log::warn!("failed to write voice input history: {error}");
                    false
                }
            }
        } else {
            false
        };

        if wrote_history {
            let config = {
                let runtime = runtime.lock().expect("voice runtime lock poisoned");
                runtime.config.clone()
            };
            if hotwords::should_auto_organize(&config) {
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = hotwords::organize_hotwords(config, false).await {
                        log::warn!("failed to auto organize hotwords: {error}");
                    }
                });
            }
        }

        let complete_payload = {
            let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
            runtime.machine.complete()
        };
        emit_state(&app, complete_payload);
        overlay::hide_preinput_for_session(&app, session_id);
    });
}

fn spawn_transcript_event_relay(
    app: AppHandle,
    runtime: SharedRuntime,
    mut event_rx: UnboundedReceiver<TranscriptEvent>,
    preview_state: Arc<tokio::sync::Mutex<TranscriptPreviewState>>,
    session_id: u64,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(mut event) = event_rx.recv().await {
            while let Ok(next_event) = event_rx.try_recv() {
                if !next_event.text.trim().is_empty() {
                    event = next_event;
                }
            }

            if event.text.trim().is_empty() {
                continue;
            }

            let state = {
                let runtime = runtime.lock().expect("voice runtime lock poisoned");
                runtime.machine.state().clone()
            };

            if !matches!(state, VoiceState::Recording | VoiceState::Transcribing) {
                continue;
            }

            let (text, confirmed_chars) = {
                let mut preview_state = preview_state.lock().await;
                let text = preview_state.apply_event(&event);
                let confirmed_chars = preview_state.confirmed_chars();
                (text, confirmed_chars)
            };
            emit_state(
                &app,
                VoiceStatePayload {
                    state: state.clone(),
                    message: format!("正在识别 {} 个字", text.chars().count()),
                    elapsed_ms: None,
                },
            );

            overlay::update_preinput(
                &app,
                PreInputPayload {
                    session_id,
                    seq: 0,
                    text,
                    state: if matches!(state, VoiceState::Recording) {
                        PreInputState::Recording
                    } else {
                        PreInputState::Transcribing
                    },
                    confirmed_chars: Some(confirmed_chars),
                    message: None,
                },
            );
        }
    });
}

async fn fail_and_reset(
    app: &AppHandle,
    runtime: SharedRuntime,
    message: String,
    session_id: u64,
) {
    log::warn!("voice input failed: {message}");
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        runtime.machine.fail(message)
    };
    emit_state(app, payload);
    overlay::update_preinput(
        app,
        PreInputPayload {
            session_id,
            seq: 0,
            text: String::new(),
            state: PreInputState::Error,
            confirmed_chars: Some(0),
            message: Some("失败".to_string()),
        },
    );
    sleep(Duration::from_millis(1200)).await;
    overlay::hide_preinput_for_session(app, session_id);
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        runtime.machine.complete()
    };
    emit_state(app, payload);
}

async fn cancel_session_quietly(app: &AppHandle, runtime: SharedRuntime, session_id: u64) {
    overlay::hide_preinput_for_session(app, session_id);
    let payload = {
        let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
        runtime.machine.complete()
    };
    emit_state(app, payload);
}

fn schedule_idle_reset(app: AppHandle, runtime: SharedRuntime, session_id: u64) {
    tauri::async_runtime::spawn(async move {
        sleep(Duration::from_millis(1200)).await;
        let payload = {
            let mut runtime = runtime.lock().expect("voice runtime lock poisoned");
            runtime.machine.complete()
        };
        overlay::hide_preinput_for_session(&app, session_id);
        emit_state(&app, payload);
    });
}

fn emit_state(app: &AppHandle, payload: VoiceStatePayload) {
    if let Err(error) = app.emit(VOICE_STATE_EVENT, payload) {
        log::warn!("failed to emit voice state: {error}");
    }
}

fn is_empty_input_error(error: &ProviderError) -> bool {
    match error {
        ProviderError::EmptyTranscript | ProviderError::MissingFinalTranscript => true,
        ProviderError::Request(message) | ProviderError::Protocol(message) => {
            let normalized = message.to_ascii_lowercase();
            normalized.contains("45000002")
                || normalized.contains("empty audio")
                || message.contains("空音频")
        }
    }
}
