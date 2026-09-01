use crate::inject::{
    DeliveryExecutor, DeliveryMode, DeliveryReceipt, DeliveryRequest, InjectError,
    RestorationState, SubmissionState,
};
use async_trait::async_trait;
use paste_protocol::{
    HelperEvent, HelperEventKind, HelperOperation, HelperRequest, HelperStage, TargetIdentity,
    PROTOCOL_VERSION,
};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_shell::{process::CommandEvent, ShellExt};
use tokio::sync::Mutex;

const HELPER_NAME: &str = "zephyr-paste-helper";
const HELPER_BUDGET: Duration = Duration::from_secs(3);

#[derive(Default)]
struct TransactionState {
    helper_ready: Option<bool>,
}

pub struct ClipboardTransactionService {
    app: AppHandle,
    state: Mutex<TransactionState>,
}

impl ClipboardTransactionService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            state: Mutex::new(TransactionState::default()),
        }
    }

    async fn ensure_helper_ready(&self, state: &mut TransactionState) -> Result<(), InjectError> {
        if state.helper_ready == Some(true) {
            return Ok(());
        }
        if state.helper_ready == Some(false) {
            return Err(InjectError::HelperUnavailable(
                "self-check previously failed".to_string(),
            ));
        }
        let transaction_id = uuid::Uuid::new_v4();
        let request = HelperRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: HelperOperation::SelfCheck,
            transaction_id,
            mode: None,
            text: None,
            target: None,
            fault_at: None,
            send_input_count: None,
        };
        match self.run_helper(&request).await {
            Ok(report) if report.self_check && report.exit_code == Some(0) => {
                state.helper_ready = Some(true);
                Ok(())
            }
            Ok(_) => {
                state.helper_ready = Some(false);
                Err(InjectError::HelperUnavailable(
                    "self-check did not complete successfully".to_string(),
                ))
            }
            Err(failure) => {
                state.helper_ready = Some(false);
                Err(InjectError::HelperUnavailable(failure.message))
            }
        }
    }

    async fn deliver_locked(
        &self,
        request: DeliveryRequest,
    ) -> Result<DeliveryReceipt, InjectError> {
        let allow_unicode_fallback = request.allow_unicode_fallback;
        let helper_request = HelperRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: HelperOperation::Deliver,
            transaction_id: request.transaction_id,
            mode: Some(request.mode),
            text: Some(request.text),
            target: Some(TargetIdentity {
                hwnd: request.target.hwnd as i64,
                process_id: request.target.process_id,
                process_started_at: request.target.process_started_at,
                executable_path: request.target.executable_path,
            }),
            fault_at: None,
            send_input_count: None,
        };
        let result = self.execute_delivery_helper(&helper_request).await;
        if should_fallback_to_unicode(&helper_request, allow_unicode_fallback, &result) {
            log::warn!(
                "clipboard snapshot unavailable; retrying safe single-line Unicode delivery transaction_id={}",
                helper_request.transaction_id
            );
            let mut fallback_request = helper_request.clone();
            fallback_request.mode = Some(DeliveryMode::Unicode);
            return self.execute_delivery_helper(&fallback_request).await;
        }
        result
    }

    async fn execute_delivery_helper(
        &self,
        helper_request: &HelperRequest,
    ) -> Result<DeliveryReceipt, InjectError> {
        match self.run_helper(helper_request).await {
            Ok(report) => self.receipt_from_report(report, helper_request.mode).await,
            Err(failure) => {
                let mut receipt = DeliveryReceipt {
                    transaction_id: helper_request.transaction_id,
                    submission: submission_from_stage(failure.last_stage),
                    restoration: restoration_after_abnormal_exit(failure.last_stage),
                };
                if helper_request.mode == Some(DeliveryMode::ClipboardPaste)
                    && stage_at_least(failure.last_stage, HelperStage::PayloadWritten)
                {
                    receipt.restoration = self
                        .recover_once(helper_request.transaction_id)
                        .await
                        .unwrap_or(RestorationState::Failed);
                }
                if receipt.submission == SubmissionState::NotSubmitted {
                    Err(InjectError::Worker(failure.message))
                } else {
                    log::warn!(
                        "paste helper ended abnormally transaction_id={} stage={:?} submission={:?} restoration={:?}",
                        receipt.transaction_id,
                        failure.last_stage,
                        receipt.submission,
                        receipt.restoration
                    );
                    Ok(receipt)
                }
            }
        }
    }

    async fn receipt_from_report(
        &self,
        report: HelperReport,
        mode: Option<DeliveryMode>,
    ) -> Result<DeliveryReceipt, InjectError> {
        let mut terminal = report.terminal.ok_or_else(|| {
            InjectError::Worker("paste helper exited without a terminal receipt".to_string())
        })?;
        if mode == Some(DeliveryMode::ClipboardPaste)
            && terminal.receipt.restoration == RestorationState::Failed
            && stage_at_least(report.last_stage, HelperStage::PayloadWritten)
        {
            terminal.receipt.restoration = self
                .recover_once(terminal.receipt.transaction_id)
                .await
                .unwrap_or(RestorationState::Failed);
        }
        if terminal.receipt.submission != SubmissionState::NotSubmitted || terminal.code.is_none() {
            if let Some(code) = terminal.code.as_deref() {
                log::warn!(
                    "paste helper terminal incident transaction_id={} code={code} submission={:?} restoration={:?}",
                    terminal.receipt.transaction_id,
                    terminal.receipt.submission,
                    terminal.receipt.restoration
                );
            }
            return Ok(terminal.receipt);
        }
        let code = terminal.code.unwrap_or_default();
        let message = terminal
            .message
            .unwrap_or_else(|| "paste helper rejected delivery".to_string());
        match code.as_str() {
            "clipboard_snapshot_unsupported" => {
                Err(InjectError::ClipboardSnapshotUnsupported(message))
            }
            "target_changed" | "target_changed_restore_failed" => {
                Err(InjectError::TargetChanged(message))
            }
            "protocol_version_mismatch" => Err(InjectError::HelperUnavailable(message)),
            _ => Err(InjectError::Worker(format!("{code}: {message}"))),
        }
    }

    async fn recover_once(&self, transaction_id: uuid::Uuid) -> Option<RestorationState> {
        let request = HelperRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: HelperOperation::Recover,
            transaction_id,
            mode: None,
            text: None,
            target: None,
            fault_at: None,
            send_input_count: None,
        };
        let report = self.run_helper(&request).await.ok()?;
        let terminal = report.terminal?;
        log::warn!(
            "paste helper recovery transaction_id={transaction_id} restoration={:?}",
            terminal.receipt.restoration
        );
        Some(terminal.receipt.restoration)
    }

    async fn run_helper(&self, request: &HelperRequest) -> Result<HelperReport, HelperFailure> {
        let command =
            self.app.shell().sidecar(HELPER_NAME).map_err(|error| {
                HelperFailure::new(None, format!("sidecar unavailable: {error}"))
            })?;
        let (mut receiver, mut child) = command
            .spawn()
            .map_err(|error| HelperFailure::new(None, format!("sidecar start failed: {error}")))?;
        let mut payload = serde_json::to_vec(request)
            .map_err(|error| HelperFailure::new(None, error.to_string()))?;
        payload.push(b'\n');
        child
            .write(&payload)
            .map_err(|error| HelperFailure::new(None, format!("sidecar stdin failed: {error}")))?;

        let transaction_id = request.transaction_id;
        let mut report = HelperReport::default();
        let mut sequence = 0u32;
        let deadline = tokio::time::sleep(HELPER_BUDGET);
        tokio::pin!(deadline);
        loop {
            let event = tokio::select! {
                _ = &mut deadline => {
                    let last_stage = report.last_stage;
                    let _ = child.kill();
                    return Err(HelperFailure::new(
                        last_stage,
                        "paste helper exceeded the 3 second budget".to_string(),
                    ));
                }
                event = receiver.recv() => event,
            };
            let Some(event) = event else {
                return Err(HelperFailure::new(
                    report.last_stage,
                    "helper event stream closed".to_string(),
                ));
            };
            let event_result = match event {
                CommandEvent::Stdout(line) => {
                    apply_stdout_event(&mut report, &mut sequence, transaction_id, &line)
                }
                CommandEvent::Stderr(_) => {
                    report.stderr_seen = true;
                    Ok(None)
                }
                CommandEvent::Error(error) => Err(HelperFailure::new(report.last_stage, error)),
                CommandEvent::Terminated(payload) => {
                    report.exit_code = payload.code;
                    if report.terminal.is_some()
                        || (request.operation == HelperOperation::SelfCheck
                            && report.self_check
                            && payload.code == Some(0))
                    {
                        return Ok(report);
                    }
                    Err(HelperFailure::new(
                        report.last_stage,
                        format!("helper terminated without receipt: {:?}", payload.code),
                    ))
                }
                _ => Ok(None),
            };
            if let Err(failure) = event_result {
                let _ = child.kill();
                return Err(failure);
            }
        }
    }
}

fn apply_stdout_event(
    report: &mut HelperReport,
    sequence: &mut u32,
    transaction_id: uuid::Uuid,
    line: &[u8],
) -> Result<Option<()>, HelperFailure> {
    let event: HelperEvent = serde_json::from_slice(line).map_err(|error| {
        HelperFailure::new(report.last_stage, format!("invalid helper event: {error}"))
    })?;
    if event.protocol_version != PROTOCOL_VERSION
        || event.transaction_id != transaction_id
        || event.sequence != sequence.saturating_add(1)
    {
        return Err(HelperFailure::new(
            report.last_stage,
            "helper event identity or sequence mismatch".to_string(),
        ));
    }
    *sequence = event.sequence;
    match event.event {
        HelperEventKind::SelfCheck { .. } => report.self_check = true,
        HelperEventKind::Stage { stage } => {
            if !valid_stage_transition(report.last_stage, stage) {
                return Err(HelperFailure::new(
                    report.last_stage,
                    "helper stage moved backwards".to_string(),
                ));
            }
            report.last_stage = Some(stage);
        }
        HelperEventKind::Terminal {
            receipt,
            code,
            message,
        } => {
            if receipt.transaction_id != transaction_id || report.terminal.is_some() {
                return Err(HelperFailure::new(
                    report.last_stage,
                    "helper terminal receipt mismatch".to_string(),
                ));
            }
            report.terminal = Some(HelperTerminal {
                receipt,
                code,
                message,
            });
        }
    }
    Ok(None)
}

#[async_trait]
impl DeliveryExecutor for ClipboardTransactionService {
    async fn deliver(&self, request: DeliveryRequest) -> Result<DeliveryReceipt, InjectError> {
        let mut state = self.state.lock().await;
        self.ensure_helper_ready(&mut state).await?;
        self.deliver_locked(request).await
    }
}

#[derive(Default)]
struct HelperReport {
    self_check: bool,
    stderr_seen: bool,
    last_stage: Option<HelperStage>,
    terminal: Option<HelperTerminal>,
    exit_code: Option<i32>,
}

struct HelperTerminal {
    receipt: DeliveryReceipt,
    code: Option<String>,
    message: Option<String>,
}

struct HelperFailure {
    last_stage: Option<HelperStage>,
    message: String,
}

impl HelperFailure {
    fn new(last_stage: Option<HelperStage>, message: String) -> Self {
        Self {
            last_stage,
            message,
        }
    }
}

fn stage_rank(stage: HelperStage) -> u8 {
    match stage {
        HelperStage::SnapshotComplete => 1,
        HelperStage::PayloadWriteStarted => 2,
        HelperStage::PayloadWritten => 3,
        HelperStage::TargetVerified => 4,
        HelperStage::PasteSubmitting => 5,
        HelperStage::PasteSubmitted => 6,
        HelperStage::RestoreStarted => 7,
    }
}

fn valid_stage_transition(previous: Option<HelperStage>, next: HelperStage) -> bool {
    previous.is_none_or(|previous| stage_rank(next) > stage_rank(previous))
}

fn stage_at_least(stage: Option<HelperStage>, expected: HelperStage) -> bool {
    stage.is_some_and(|stage| stage_rank(stage) >= stage_rank(expected))
}

fn submission_from_stage(stage: Option<HelperStage>) -> SubmissionState {
    match stage {
        Some(HelperStage::PasteSubmitted | HelperStage::RestoreStarted) => {
            SubmissionState::Submitted
        }
        Some(HelperStage::PasteSubmitting) => SubmissionState::Unknown,
        _ => SubmissionState::NotSubmitted,
    }
}

fn restoration_after_abnormal_exit(stage: Option<HelperStage>) -> RestorationState {
    if stage_at_least(stage, HelperStage::PayloadWriteStarted) {
        RestorationState::Failed
    } else {
        RestorationState::NotNeeded
    }
}

fn should_fallback_to_unicode(
    request: &HelperRequest,
    allowed: bool,
    result: &Result<DeliveryReceipt, InjectError>,
) -> bool {
    allowed
        && request.mode == Some(DeliveryMode::ClipboardPaste)
        && request
            .text
            .as_deref()
            .is_some_and(|text| !text.contains('\n'))
        && matches!(result, Err(InjectError::ClipboardSnapshotUnsupported(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_stage_arbitration_is_monotonic_and_conservative() {
        assert!(valid_stage_transition(None, HelperStage::PasteSubmitting));
        assert!(valid_stage_transition(
            Some(HelperStage::PayloadWritten),
            HelperStage::PasteSubmitting
        ));
        assert!(!valid_stage_transition(
            Some(HelperStage::PasteSubmitting),
            HelperStage::PayloadWritten
        ));
        assert_eq!(submission_from_stage(None), SubmissionState::NotSubmitted);
        assert_eq!(
            submission_from_stage(Some(HelperStage::PasteSubmitting)),
            SubmissionState::Unknown
        );
        assert_eq!(
            submission_from_stage(Some(HelperStage::RestoreStarted)),
            SubmissionState::Submitted
        );
    }

    #[test]
    fn unsupported_clipboard_snapshot_only_falls_back_for_single_line_delivery() {
        let request = HelperRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: HelperOperation::Deliver,
            transaction_id: uuid::Uuid::nil(),
            mode: Some(DeliveryMode::ClipboardPaste),
            text: Some("single line".to_string()),
            target: Some(TargetIdentity {
                hwnd: 1,
                process_id: 2,
                process_started_at: 3,
                executable_path: r"C:\\Apps\\editor.exe".to_string(),
            }),
            fault_at: None,
            send_input_count: None,
        };
        let unsupported = Err(InjectError::ClipboardSnapshotUnsupported(
            "unmaterialized format".to_string(),
        ));
        assert!(should_fallback_to_unicode(&request, true, &unsupported));
        assert!(!should_fallback_to_unicode(&request, false, &unsupported));

        let mut multiline = request.clone();
        multiline.text = Some("first\nsecond".to_string());
        assert!(!should_fallback_to_unicode(&multiline, true, &unsupported));

        let worker_failure = Err(InjectError::Worker("failed".to_string()));
        assert!(!should_fallback_to_unicode(&request, true, &worker_failure));
    }
}
