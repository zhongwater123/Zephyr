use crate::command_error::{CommandError, CommandResult};
use crate::config::InjectionStrategy;
use crate::history::AppContext;
use crate::hotwords;
use crate::inject::InjectionMethod;
use crate::inject::TextInjector;
use crate::services::AppServices;
use crate::{target, SharedRuntime};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DeliveryFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone)]
pub struct DeliveryService {
    services: AppServices,
}

impl DeliveryService {
    pub fn new(services: AppServices) -> Self {
        Self { services }
    }

    pub fn validate(
        &self,
        text: &str,
        target: &target::TargetWindowIdentity,
        activate: bool,
    ) -> Result<(), DeliveryFailure> {
        target::validate_output_text(text).map_err(|error| {
            use target::OutputValidationError;
            let (code, message) = match error {
                OutputValidationError::Empty => {
                    ("output_empty", "识别结果为空，未执行输入".to_string())
                }
                OutputValidationError::TooLong => (
                    "output_too_long",
                    "识别结果超过 8000 个字符，已转入待处理区".to_string(),
                ),
                OutputValidationError::ForbiddenCharacter { index, codepoint } => (
                    "output_forbidden_character",
                    format!(
                        "识别结果包含不允许的控制或双向字符（位置 {index}，U+{codepoint:04X}）"
                    ),
                ),
            };
            DeliveryFailure { code, message }
        })?;
        if activate {
            target::activate_target(target).map_err(|message| DeliveryFailure {
                code: "target_changed",
                message,
            })?;
        }
        target::validate_foreground_target(target).map_err(|message| DeliveryFailure {
            code: "target_changed",
            message,
        })
    }

    pub async fn inject(
        &self,
        text: String,
        injector: Arc<dyn TextInjector>,
        method: InjectionMethod,
    ) -> Result<(), DeliveryFailure> {
        tauri::async_runtime::spawn_blocking(move || injector.inject_text(&text, method))
            .await
            .map_err(|error| DeliveryFailure {
                code: "injection_task_failed",
                message: error.to_string(),
            })?
            .map_err(|error| DeliveryFailure {
                code: "injection_rejected",
                message: error.to_string(),
            })
    }

    pub async fn commit(
        &self,
        text: String,
        context: AppContext,
        config: crate::config::AppConfig,
    ) -> bool {
        if !config.history_enabled {
            return false;
        }
        let history = self.services.history.clone();
        let wrote_history =
            match tauri::async_runtime::spawn_blocking(move || history.insert(&text, &context))
                .await
            {
                Ok(Ok(_)) => true,
                Ok(Err(error)) => {
                    log::warn!("failed to write delivered output history: {error}");
                    false
                }
                Err(error) => {
                    log::warn!("failed to join delivered output history write: {error}");
                    false
                }
            };
        if wrote_history && hotwords::should_auto_organize(&config) {
            let agent = self.services.hotword_agent.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = agent.organize(config, false).await {
                    log::warn!("failed to auto organize hotwords after delivery: {error}");
                }
            });
        }
        wrote_history
    }
}

pub async fn deliver_pending(
    id: String,
    runtime: SharedRuntime,
    services: AppServices,
) -> CommandResult<()> {
    let config = services.config.snapshot();
    let delivery = DeliveryService::new(services.clone());
    let (record, injector, method) = {
        let mut runtime = runtime
            .lock()
            .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?;
        if runtime.sessions.current_id.is_some() {
            return Err(CommandError::new(
                "session_active",
                "录音或识别进行中，暂时不能发送待处理结果",
            ));
        }
        let record = runtime.sessions.pending_outputs.get(&id).ok_or_else(|| {
            CommandError::new("pending_output_not_found", "待处理结果不存在或已过期")
        })?;
        let method = match config.injection_strategy_for(&record.target.executable_name) {
            InjectionStrategy::Unicode => InjectionMethod::Unicode,
            InjectionStrategy::ClipboardCompatibility => InjectionMethod::ClipboardCompatibility,
        };
        (record, runtime.injector.clone(), method)
    };

    delivery
        .validate(&record.dto.text, &record.target, true)
        .map_err(|error| CommandError::new(error.code, error.message))?;
    delivery
        .inject(record.dto.text.clone(), injector, method)
        .await
        .map_err(|error| CommandError::new(error.code, error.message))?;

    runtime
        .lock()
        .map_err(|error| CommandError::new("runtime_lock_failed", error.to_string()))?
        .sessions
        .pending_outputs
        .remove(&id);

    delivery
        .commit(
            record.dto.text,
            AppContext {
                app_name: Some(record.target.executable_name),
                app_title: record.target.window_title,
            },
            config,
        )
        .await;
    Ok(())
}
