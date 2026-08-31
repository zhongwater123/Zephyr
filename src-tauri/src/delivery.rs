use crate::history::{AppContext, HistoryProvenance};
use crate::hotwords;
use crate::inject::{AtomicPasteReceipt, InjectionMethod, TextInjector};
use crate::services::AppServices;
use crate::target;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DeliveryFailure {
    pub code: &'static str,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryIntent {
    Legacy,
    SmartDictationAtomicPaste,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryReceipt {
    LegacyInjected,
    AtomicPaste(AtomicPasteReceipt),
}

pub fn prepare_delivery_text(
    text: &str,
    target_window: &target::TargetWindowIdentity,
    intent: DeliveryIntent,
) -> Result<String, DeliveryFailure> {
    let prepared = match intent {
        DeliveryIntent::Legacy => {
            target::validate_output_text(text).map_err(map_output_validation_error)?;
            text.to_string()
        }
        DeliveryIntent::SmartDictationAtomicPaste => {
            target::normalize_smart_output_text(text).map_err(map_output_validation_error)?
        }
    };

    if intent == DeliveryIntent::SmartDictationAtomicPaste
        && prepared.contains('\n')
        && target::is_multiline_unsafe_target(&target_window.executable_name)
    {
        return Err(DeliveryFailure {
            code: "multiline_target_unsafe",
            message: format!(
                "multiline automatic paste is disabled for {}",
                target_window.executable_name
            ),
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
        injector: Arc<dyn TextInjector>,
        method: InjectionMethod,
        intent: DeliveryIntent,
    ) -> Result<DeliveryReceipt, DeliveryFailure> {
        match intent {
            DeliveryIntent::Legacy => {
                self.inject(text, injector, method).await?;
                Ok(DeliveryReceipt::LegacyInjected)
            }
            DeliveryIntent::SmartDictationAtomicPaste => {
                tauri::async_runtime::spawn_blocking(move || {
                    injector.inject_atomic_paste(&text, &target_window)
                })
                .await
                .map_err(|error| DeliveryFailure {
                    code: "injection_task_failed",
                    message: error.to_string(),
                })?
                .map(DeliveryReceipt::AtomicPaste)
                .map_err(|error| DeliveryFailure {
                    code: "injection_rejected_before_submit",
                    message: error.to_string(),
                })
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn target(executable_name: &str) -> target::TargetWindowIdentity {
        target::TargetWindowIdentity {
            hwnd: 1,
            process_id: 2,
            process_started_at: 3,
            executable_name: executable_name.to_string(),
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
    fn smart_delivery_normalizes_multiline_text_for_ordinary_targets() {
        assert_eq!(
            prepare_delivery_text(
                "first\r\nsecond\rthird",
                &target("notepad.exe"),
                DeliveryIntent::SmartDictationAtomicPaste,
            )
            .unwrap(),
            "first\nsecond\nthird"
        );
    }

    #[test]
    fn smart_multiline_fails_closed_only_for_known_command_targets() {
        for executable in ["cmd.exe", "PowerShell.exe", "WindowsTerminal.exe"] {
            let error = prepare_delivery_text(
                "first\nsecond",
                &target(executable),
                DeliveryIntent::SmartDictationAtomicPaste,
            )
            .unwrap_err();
            assert_eq!(error.code, "multiline_target_unsafe");
        }
        assert!(prepare_delivery_text(
            "first\nsecond",
            &target("Code.exe"),
            DeliveryIntent::SmartDictationAtomicPaste,
        )
        .is_ok());
        assert!(prepare_delivery_text(
            "first\nsecond",
            &target("Cursor.exe"),
            DeliveryIntent::SmartDictationAtomicPaste,
        )
        .is_ok());
    }
}
