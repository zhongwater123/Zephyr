use crate::history::{AppContext, HistoryProvenance};
use crate::hotwords;
use crate::inject::{
    DeliveryExecutor, DeliveryMode, DeliveryReceipt, DeliveryRequest, InjectError,
};
use crate::services::AppServices;
use crate::target;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DeliveryFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryIntent {
    Legacy,
    SmartDictation,
}

pub fn prepare_delivery_text(
    text: &str,
    _target_window: &target::TargetWindowIdentity,
    intent: DeliveryIntent,
) -> Result<String, DeliveryFailure> {
    let prepared = match intent {
        DeliveryIntent::Legacy => {
            target::validate_output_text(text).map_err(map_output_validation_error)?;
            text.to_string()
        }
        DeliveryIntent::SmartDictation => {
            target::normalize_smart_output_text(text).map_err(map_output_validation_error)?
        }
    };

    if intent == DeliveryIntent::SmartDictation && prepared.contains('\n') {
        return Err(DeliveryFailure {
            code: "atomic_paste_temporarily_unavailable",
            message: "多行整体粘贴正在安全升级，结果已进入待处理区".to_string(),
        });
    }

    Ok(prepared)
}

fn map_output_validation_error(error: target::OutputValidationError) -> DeliveryFailure {
    use target::OutputValidationError;
    let (code, message) = match error {
        OutputValidationError::Empty => ("output_empty", "output is empty".to_string()),
        OutputValidationError::TooLong => (
            "output_too_long",
            "output exceeds 8000 Unicode characters".to_string(),
        ),
        OutputValidationError::ForbiddenCharacter { index, codepoint } => (
            "output_forbidden_character",
            format!("output contains a forbidden character at {index}: U+{codepoint:04X}"),
        ),
    };
    DeliveryFailure { code, message }
}

#[derive(Clone)]
pub struct DeliveryService {
    services: AppServices,
}

impl DeliveryService {
    pub fn new(services: AppServices) -> Self {
        Self { services }
    }

    #[allow(dead_code)]
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

    pub fn validate_with_intent(
        &self,
        text: &str,
        target_window: &target::TargetWindowIdentity,
        activate: bool,
        intent: DeliveryIntent,
    ) -> Result<String, DeliveryFailure> {
        let prepared = prepare_delivery_text(text, target_window, intent)?;
        if activate {
            target::activate_target(target_window).map_err(|message| DeliveryFailure {
                code: "target_changed",
                message,
            })?;
        }
        target::validate_foreground_target(target_window).map_err(|message| DeliveryFailure {
            code: "target_changed",
            message,
        })?;
        Ok(prepared)
    }

    pub async fn inject_with_intent(
        &self,
        text: String,
        target_window: target::TargetWindowIdentity,
        executor: Arc<dyn DeliveryExecutor>,
        requested_mode: DeliveryMode,
        intent: DeliveryIntent,
    ) -> Result<DeliveryReceipt, DeliveryFailure> {
        let transaction_id = Uuid::new_v4();
        let mode = match intent {
            DeliveryIntent::Legacy => requested_mode,
            DeliveryIntent::SmartDictation => DeliveryMode::Unicode,
        };
        let started = Instant::now();
        let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
        log::info!(
            "delivery_inject_started transaction_id={transaction_id} mode={mode:?} chars={} bytes={} sha256={digest} target_exe={} target_pid={}",
            text.chars().count(),
            text.len(),
            target_window.executable_name,
            target_window.process_id
        );
        let result = executor
            .deliver(DeliveryRequest {
                transaction_id,
                text,
                target: target_window,
                mode,
            })
            .await
            .map_err(map_inject_error);
        match &result {
            Ok(receipt) => log::info!(
                "delivery_inject_finished transaction_id={transaction_id} submission={:?} restoration={:?} elapsed_ms={}",
                receipt.submission,
                receipt.restoration,
                started.elapsed().as_millis()
            ),
            Err(error) => log::warn!(
                "delivery_inject_finished transaction_id={transaction_id} code={} elapsed_ms={}",
                error.code,
                started.elapsed().as_millis()
            ),
        }
        result
    }

    #[allow(dead_code)]
    pub async fn commit(
        &self,
        text: String,
        context: AppContext,
        config: crate::config::AppConfig,
    ) -> bool {
        self.commit_with_provenance(text, context, config, HistoryProvenance::default())
            .await
    }

    pub async fn commit_with_provenance(
        &self,
        text: String,
        context: AppContext,
        config: crate::config::AppConfig,
        provenance: HistoryProvenance,
    ) -> bool {
        if !config.history_enabled {
            return false;
        }
        let history = self.services.history.clone();
        let wrote_history = match tauri::async_runtime::spawn_blocking(move || {
            history.insert_with_provenance(&text, &context, &provenance)
        })
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

fn map_inject_error(error: InjectError) -> DeliveryFailure {
    match error {
        InjectError::ClipboardTemporarilyUnavailable => DeliveryFailure {
            code: "clipboard_compatibility_temporarily_unavailable",
            message: "剪贴板兼容模式正在安全升级，结果已进入待处理区".to_string(),
        },
        other => DeliveryFailure {
            code: "injection_task_failed",
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(executable_name: &str) -> target::TargetWindowIdentity {
        target::TargetWindowIdentity {
            hwnd: 1,
            process_id: 2,
            process_started_at: 3,
            executable_name: executable_name.to_string(),
            executable_path: format!(r"C:\\Apps\\{executable_name}"),
            window_title: None,
        }
    }

    #[test]
    fn legacy_delivery_still_rejects_multiline_text() {
        assert_eq!(
            prepare_delivery_text(
                "first\nsecond",
                &target("notepad.exe"),
                DeliveryIntent::Legacy,
            )
            .unwrap_err()
            .code,
            "output_forbidden_character"
        );
    }

    #[test]
    fn smart_delivery_keeps_single_line_text() {
        assert_eq!(
            prepare_delivery_text(
                "first second",
                &target("notepad.exe"),
                DeliveryIntent::SmartDictation,
            )
            .unwrap(),
            "first second"
        );
    }

    #[test]
    fn smart_multiline_fails_closed_until_atomic_paste_is_available() {
        for executable in ["notepad.exe", "Code.exe", "WindowsTerminal.exe"] {
            let error = prepare_delivery_text(
                "first\nsecond",
                &target(executable),
                DeliveryIntent::SmartDictation,
            )
            .unwrap_err();
            assert_eq!(error.code, "atomic_paste_temporarily_unavailable");
        }
    }
}
