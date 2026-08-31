use crate::overlay::{self, PreInputPayload, PreInputState};
use crate::state::{VoiceState, VoiceStatePayload};
use crate::voice_trigger::{BeginRejection, TriggerBehavior};
use tauri::{AppHandle, Emitter, Manager};

const VOICE_STATE_EVENT: &str = "voice_state_changed";
const TRANSIENT_REJECTION_MS: u64 = 1200;

#[derive(Clone)]
pub(super) struct VoicePresenter {
    app: AppHandle,
}

impl VoicePresenter {
    pub(super) fn new(app: AppHandle) -> Self {
        Self { app }
    }

    pub(super) fn emit_state(&self, payload: VoiceStatePayload) {
        if let Err(error) = self.app.emit(VOICE_STATE_EVENT, payload) {
            log::warn!("failed to emit voice state: {error}");
        }
    }

    pub(super) fn begin_session(&self) -> u64 {
        overlay::begin_preinput_session()
    }

    pub(super) fn show_starting(&self, session_id: u64) {
        overlay::show_preinput(
            &self.app,
            PreInputPayload {
                session_id,
                seq: 0,
                text: String::new(),
                state: PreInputState::Starting,
                confirmed_chars: Some(0),
                message: Some("正在启动麦克风".to_string()),
            },
        );
    }

    pub(super) fn show_recording(&self, session_id: u64, behavior: TriggerBehavior) {
        let message = match behavior {
            TriggerBehavior::PushToTalk => "正在聆听，松开结束",
            TriggerBehavior::PressToToggle => "正在聆听，再按一次结束",
        };
        overlay::update_preinput(
            &self.app,
            PreInputPayload {
                session_id,
                seq: 0,
                text: String::new(),
                state: PreInputState::Recording,
                confirmed_chars: Some(0),
                message: Some(message.to_string()),
            },
        );
    }

    pub(super) fn show_begin_rejection(&self, reason: BeginRejection) {
        let message = match reason {
            BeginRejection::Disabled => "语音输入已暂停",
            BeginRejection::Busy => "上一轮语音仍在处理中",
            BeginRejection::PendingFull => "待处理结果已满，请先处理",
            BeginRejection::ShuttingDown => "语音输入正在关闭",
        }
        .to_string();
        let session_id = self.begin_session();
        overlay::show_preinput(
            &self.app,
            PreInputPayload {
                session_id,
                seq: 0,
                text: String::new(),
                state: PreInputState::Error,
                confirmed_chars: Some(0),
                message: Some(message),
            },
        );
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(TRANSIENT_REJECTION_MS)).await;
            overlay::hide_preinput_for_session(&app, session_id);
        });
    }

    pub(super) fn show_finalizing(&self, session_id: u64, text: String, message: &str) {
        overlay::update_preinput(
            &self.app,
            PreInputPayload {
                session_id,
                seq: 0,
                text,
                state: PreInputState::Finalizing,
                confirmed_chars: None,
                message: Some(message.to_string()),
            },
        );
    }

    pub(super) fn show_error(&self, session_id: u64, message: String) {
        overlay::update_preinput(
            &self.app,
            PreInputPayload {
                session_id,
                seq: 0,
                text: String::new(),
                state: PreInputState::Error,
                confirmed_chars: Some(0),
                message: Some(message),
            },
        );
    }

    pub(super) fn hide(&self, session_id: u64) {
        overlay::hide_preinput_for_session(&self.app, session_id);
    }

    pub(super) fn pending_changed(&self) {
        if let Some(main) = self.app.get_webview_window("main") {
            let _ = main.emit("pending_outputs_changed", ());
        }
    }

    pub(super) fn presentation_sink(&self) -> VoicePresentationSink {
        VoicePresentationSink {
            presenter: self.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct VoicePresentationSink {
    presenter: VoicePresenter,
}

impl VoicePresentationSink {
    pub(crate) fn progress(
        &self,
        session_id: u64,
        state: VoiceState,
        text: String,
        confirmed_chars: usize,
    ) {
        self.presenter.emit_state(VoiceStatePayload {
            state: state.clone(),
            message: format!("正在识别 {} 个字", text.chars().count()),
            elapsed_ms: None,
        });
        overlay::update_preinput(
            &self.presenter.app,
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
}
