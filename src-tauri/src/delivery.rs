use crate::history::{AppContext, HistoryProvenance};
use crate::hotwords;
use crate::inject::{
    DeliveryExecutor, DeliveryMode, DeliveryReceipt, DeliveryRequest, InjectError,
};
use crate::services::AppServices;
use crate::target;
use crate::target_port::{CapturedTarget, TargetPort};
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
    target_window: &CapturedTarget,
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

    if intent == DeliveryIntent::SmartDictation
        && prepared.contains('\n')
        && target_window.context().multiline_may_execute
    {
        return Err(DeliveryFailure {
            code: "multiline_delivery_requires_user_action",
            message: "目标可能执行粘贴的换行，结果已进入待处理区，请由你主动复制".to_string(),
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
    targets: Arc<dyn TargetPort>,
}

impl DeliveryService {
    pub fn new(services: AppServices, targets: Arc<dyn TargetPort>) -> Self {
        Self { services, targets }
    }

    #[allow(dead_code)]
    pub fn validate(
        &self,
        text: &str,
        target: &CapturedTarget,
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
        validate_delivery_target(self.targets.as_ref(), target, activate)
    }

    pub fn validate_with_intent(
        &self,
        text: &str,
        target_window: &CapturedTarget,
        activate: bool,
        intent: DeliveryIntent,
    ) -> Result<String, DeliveryFailure> {
        let prepared = prepare_delivery_text(text, target_window, intent)?;
        validate_delivery_target(self.targets.as_ref(), target_window, activate)?;
        Ok(prepared)
    }

    pub async fn inject_with_intent(
        &self,
        text: String,
        target_window: CapturedTarget,
        executor: Arc<dyn DeliveryExecutor>,
        requested_mode: DeliveryMode,
        intent: DeliveryIntent,
    ) -> Result<DeliveryReceipt, DeliveryFailure> {
        let transaction_id = Uuid::new_v4();
        let mode = match intent {
            DeliveryIntent::Legacy => requested_mode,
            DeliveryIntent::SmartDictation => DeliveryMode::ClipboardPaste,
        };
        let started = Instant::now();
        let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
        log::info!(
            "delivery_inject_started transaction_id={transaction_id} mode={mode:?} chars={} bytes={} sha256={digest} target_exe={} target_pid={}",
            text.chars().count(),
            text.len(),
            target_window.context().application_key,
            target_window.context().process_id
        );
        let result = executor
            .deliver(DeliveryRequest {
                transaction_id,
                text,
                target: target_window,
                mode,
                allow_unicode_fallback: intent == DeliveryIntent::SmartDictation,
            })
            .await
            .map_err(|error| map_inject_error(error, intent, mode));
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

fn validate_delivery_target(
    targets: &dyn TargetPort,
    target: &CapturedTarget,
    activate: bool,
) -> Result<(), DeliveryFailure> {
    if activate {
        targets
            .activate(target)
            .map_err(|message| DeliveryFailure {
                code: "target_changed",
                message,
            })?;
    }
    targets
        .validate_foreground(target)
        .map_err(|message| DeliveryFailure {
            code: "target_changed",
            message,
        })
}

fn map_inject_error(
    error: InjectError,
    intent: DeliveryIntent,
    mode: DeliveryMode,
) -> DeliveryFailure {
    match error {
        InjectError::HelperUnavailable(message) => DeliveryFailure {
            code: if intent == DeliveryIntent::SmartDictation {
                "atomic_paste_temporarily_unavailable"
            } else if mode == DeliveryMode::ClipboardPaste {
                "clipboard_compatibility_temporarily_unavailable"
            } else {
                "delivery_helper_unavailable"
            },
            message: format!("安全交付辅助进程不可用，结果已进入待处理区：{message}"),
        },
        InjectError::ClipboardSnapshotUnsupported(message) => DeliveryFailure {
            code: "clipboard_snapshot_unsupported",
            message: format!("当前剪贴板无法安全保存，尚未覆盖，结果已进入待处理区：{message}"),
        },
        InjectError::TargetChanged(message) => DeliveryFailure {
            code: "target_changed",
            message,
        },
        InjectError::UnsupportedCapability {
            capability,
            message,
        } => DeliveryFailure {
            code: "unsupported_platform",
            message: format!("{message} ({capability})"),
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
    use crate::target_port::tests::{fake_target_with_multiline, FakeTargetPort};

    fn target(executable_name: &str) -> CapturedTarget {
        let multiline_may_execute = matches!(
            executable_name.to_ascii_lowercase().as_str(),
            "cmd.exe"
                | "powershell.exe"
                | "pwsh.exe"
                | "windowsterminal.exe"
                | "openconsole.exe"
                | "conhost.exe"
        );
        fake_target_with_multiline(executable_name, multiline_may_execute)
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
    fn smart_multiline_is_atomic_for_editors_but_fails_closed_for_terminals() {
        for executable in ["notepad.exe", "Code.exe"] {
            assert_eq!(
                prepare_delivery_text(
                    "first\nsecond",
                    &target(executable),
                    DeliveryIntent::SmartDictation,
                )
                .unwrap(),
                "first\nsecond"
            );
        }
        let error = prepare_delivery_text(
            "first\nsecond",
            &target("WindowsTerminal.exe"),
            DeliveryIntent::SmartDictation,
        )
        .unwrap_err();
        assert_eq!(error.code, "multiline_delivery_requires_user_action");
    }

    #[test]
    fn helper_unavailable_uses_mode_specific_pending_reasons() {
        let smart = map_inject_error(
            InjectError::HelperUnavailable("missing".to_string()),
            DeliveryIntent::SmartDictation,
            DeliveryMode::ClipboardPaste,
        );
        assert_eq!(smart.code, "atomic_paste_temporarily_unavailable");

        let compatibility = map_inject_error(
            InjectError::HelperUnavailable("missing".to_string()),
            DeliveryIntent::Legacy,
            DeliveryMode::ClipboardPaste,
        );
        assert_eq!(
            compatibility.code,
            "clipboard_compatibility_temporarily_unavailable"
        );

        let unicode = map_inject_error(
            InjectError::HelperUnavailable("missing".to_string()),
            DeliveryIntent::Legacy,
            DeliveryMode::Unicode,
        );
        assert_eq!(unicode.code, "delivery_helper_unavailable");
    }

    #[test]
    fn unsupported_delivery_remains_a_stable_platform_failure() {
        let failure = map_inject_error(
            InjectError::UnsupportedCapability {
                capability: "automaticTextDelivery",
                message: "automatic text delivery is unsupported on macOS".to_string(),
            },
            DeliveryIntent::SmartDictation,
            DeliveryMode::Unicode,
        );

        assert_eq!(failure.code, "unsupported_platform");
        assert!(failure.message.contains("automaticTextDelivery"));
    }

    #[test]
    fn initial_delivery_validates_without_activating() {
        let targets = FakeTargetPort::available();
        validate_delivery_target(&targets, &target("notepad.exe"), false).unwrap();
        assert_eq!(targets.calls(), vec!["validate_foreground"]);
    }

    #[test]
    fn pending_delivery_activates_then_revalidates() {
        let targets = FakeTargetPort::available();
        validate_delivery_target(&targets, &target("notepad.exe"), true).unwrap();
        assert_eq!(targets.calls(), vec!["activate", "validate_foreground"]);
    }

    #[test]
    fn target_validation_failure_preserves_the_stable_delivery_error() {
        let targets = FakeTargetPort::failing_validation("识别期间前台窗口已经变化");
        let error = validate_delivery_target(&targets, &target("notepad.exe"), false).unwrap_err();
        assert_eq!(error.code, "target_changed");
        assert_eq!(error.message, "识别期间前台窗口已经变化");
    }
}
