mod platform;
mod snapshot;

use paste_protocol::{
    DeliveryMode, DeliveryReceipt, HelperEvent, HelperEventKind, HelperOperation, HelperRequest,
    HelperStage, RestorationState, SubmissionState, PROTOCOL_VERSION,
};
use std::io::{BufRead, Read, Write};
use std::thread;
use std::time::Duration;
use uuid::Uuid;

struct Emitter {
    transaction_id: Uuid,
    sequence: u32,
    last_stage: Option<HelperStage>,
    fault_at: Option<HelperStage>,
}

impl Emitter {
    fn new(request: &HelperRequest) -> Self {
        Self {
            transaction_id: request.transaction_id,
            sequence: 0,
            last_stage: None,
            fault_at: request.fault_at,
        }
    }

    fn self_check(&mut self) -> Result<(), String> {
        snapshot::self_check()?;
        let _owner = platform::ClipboardOwner::create()?;
        self.emit(HelperEventKind::SelfCheck {
            helper_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    fn stage(&mut self, stage: HelperStage) -> Result<(), String> {
        self.last_stage = Some(stage);
        self.emit(HelperEventKind::Stage { stage })?;
        if self.fault_at == Some(stage) {
            if cfg!(debug_assertions) {
                std::process::abort();
            }
            return Err("fault injection is disabled in release builds".to_string());
        }
        Ok(())
    }

    fn terminal(
        &mut self,
        receipt: DeliveryReceipt,
        code: Option<String>,
        message: Option<String>,
    ) -> Result<(), String> {
        self.emit(HelperEventKind::Terminal {
            receipt,
            code,
            message,
        })
    }

    fn emit(&mut self, event: HelperEventKind) -> Result<(), String> {
        self.sequence = self.sequence.saturating_add(1);
        let line = serde_json::to_string(&HelperEvent {
            protocol_version: PROTOCOL_VERSION,
            transaction_id: self.transaction_id,
            sequence: self.sequence,
            event,
        })
        .map_err(|error| error.to_string())?;
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|_| stdout.flush())
            .map_err(|error| error.to_string())
    }
}

fn main() {
    let mut input = String::new();
    if let Err(error) = std::io::stdin()
        .lock()
        .take(2 * 1024 * 1024)
        .read_line(&mut input)
    {
        eprintln!("request read failed: {error}");
        std::process::exit(2);
    }
    let request: HelperRequest = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("request parse failed: {error}");
            std::process::exit(2);
        }
    };
    let mut emitter = Emitter::new(&request);
    if request.protocol_version != PROTOCOL_VERSION {
        let _ = emitter.terminal(
            DeliveryReceipt {
                transaction_id: request.transaction_id,
                submission: SubmissionState::NotSubmitted,
                restoration: RestorationState::NotNeeded,
            },
            Some("protocol_version_mismatch".to_string()),
            Some("helper protocol version mismatch".to_string()),
        );
        std::process::exit(3);
    }
    if !cfg!(debug_assertions) && (request.fault_at.is_some() || request.send_input_count.is_some())
    {
        let _ = emitter.terminal(
            DeliveryReceipt {
                transaction_id: request.transaction_id,
                submission: SubmissionState::NotSubmitted,
                restoration: RestorationState::NotNeeded,
            },
            Some("fault_injection_disabled".to_string()),
            Some("fault injection fields are rejected by release helpers".to_string()),
        );
        std::process::exit(3);
    }
    let _ = snapshot::cleanup_expired();
    let result = match request.operation {
        HelperOperation::SelfCheck => emitter
            .self_check()
            .map(|_| None)
            .map_err(|message| failure("event_write_failed", message)),
        HelperOperation::Deliver => deliver(&request, &mut emitter).map(Some),
        HelperOperation::Recover => recover(&request, &mut emitter).map(Some),
    };
    match result {
        Ok(Some(receipt)) => {
            let _ = emitter.terminal(receipt, None, None);
        }
        Ok(None) => {}
        Err((code, message)) => {
            let submission = if request.operation == HelperOperation::Recover {
                SubmissionState::NotSubmitted
            } else {
                submission_from_stage(emitter.last_stage)
            };
            let restoration = if matches!(
                emitter.last_stage,
                Some(HelperStage::PayloadWriteStarted)
                    | Some(HelperStage::PayloadWritten)
                    | Some(HelperStage::TargetVerified)
                    | Some(HelperStage::PasteSubmitting)
                    | Some(HelperStage::PasteSubmitted)
                    | Some(HelperStage::RestoreStarted)
            ) {
                RestorationState::Failed
            } else {
                RestorationState::NotNeeded
            };
            let _ = emitter.terminal(
                DeliveryReceipt {
                    transaction_id: request.transaction_id,
                    submission,
                    restoration,
                },
                Some(code),
                Some(message),
            );
            std::process::exit(4);
        }
    }
}

fn deliver(
    request: &HelperRequest,
    emitter: &mut Emitter,
) -> Result<DeliveryReceipt, (String, String)> {
    let mode = request
        .mode
        .ok_or_else(|| failure("invalid_request", "delivery mode is required"))?;
    let text = request
        .text
        .as_deref()
        .ok_or_else(|| failure("invalid_request", "delivery text is required"))?;
    let target = request
        .target
        .as_ref()
        .ok_or_else(|| failure("invalid_request", "target identity is required"))?;
    if text.is_empty() || text.chars().count() > 8_000 {
        return Err(failure(
            "invalid_request",
            "delivery text length is invalid",
        ));
    }
    validate_text_and_target(text, &target.executable_path)?;
    match mode {
        DeliveryMode::Unicode => {
            platform::verify_target(target)
                .map_err(|message| failure("target_changed", message))?;
            emitter
                .stage(HelperStage::TargetVerified)
                .map_err(|message| failure("event_write_failed", message))?;
            emitter
                .stage(HelperStage::PasteSubmitting)
                .map_err(|message| failure("event_write_failed", message))?;
            let submission = platform::send_unicode(text, request.send_input_count);
            if submission == SubmissionState::Submitted {
                emitter
                    .stage(HelperStage::PasteSubmitted)
                    .map_err(|message| failure("event_write_failed", message))?;
            }
            Ok(DeliveryReceipt {
                transaction_id: request.transaction_id,
                submission,
                restoration: RestorationState::NotNeeded,
            })
        }
        DeliveryMode::ClipboardPaste => deliver_clipboard(request, emitter, text, target),
    }
}

fn deliver_clipboard(
    request: &HelperRequest,
    emitter: &mut Emitter,
    text: &str,
    target: &paste_protocol::TargetIdentity,
) -> Result<DeliveryReceipt, (String, String)> {
    let owner = platform::ClipboardOwner::create()
        .map_err(|message| failure("clipboard_owner_failed", message))?;
    let mut snapshot = platform::capture_clipboard(&owner, request.transaction_id)
        .map_err(|message| failure("clipboard_snapshot_unsupported", message))?;
    snapshot.phase = Some(HelperStage::SnapshotComplete);
    snapshot::write(&snapshot).map_err(|message| failure("snapshot_write_failed", message))?;
    emitter
        .stage(HelperStage::SnapshotComplete)
        .map_err(|message| failure("event_write_failed", message))?;

    snapshot.phase = Some(HelperStage::PayloadWriteStarted);
    snapshot::write(&snapshot).map_err(|message| failure("snapshot_write_failed", message))?;
    emitter
        .stage(HelperStage::PayloadWriteStarted)
        .map_err(|message| failure("event_write_failed", message))?;
    let (payload_sequence, payload_sha256) =
        platform::write_payload(&owner, request.transaction_id, text)
            .map_err(|message| failure("payload_write_failed", message))?;
    snapshot.phase = Some(HelperStage::PayloadWritten);
    snapshot.payload_sequence = Some(payload_sequence);
    snapshot.payload_sha256 = Some(payload_sha256);
    snapshot::write(&snapshot).map_err(|message| failure("snapshot_write_failed", message))?;
    emitter
        .stage(HelperStage::PayloadWritten)
        .map_err(|message| failure("event_write_failed", message))?;

    if let Err(message) = platform::verify_target(target) {
        let restoration = restore_if_current(&owner, &mut snapshot, emitter, false);
        return match restoration {
            Ok(_) => Err(failure("target_changed", message)),
            Err((_, restore_message)) => Err(failure(
                "target_changed_restore_failed",
                format!("{message}; {restore_message}"),
            )),
        };
    }
    emitter
        .stage(HelperStage::TargetVerified)
        .map_err(|message| failure("event_write_failed", message))?;
    emitter
        .stage(HelperStage::PasteSubmitting)
        .map_err(|message| failure("event_write_failed", message))?;
    let submission = platform::send_ctrl_v(request.send_input_count);
    if submission == SubmissionState::Submitted {
        snapshot.phase = Some(HelperStage::PasteSubmitted);
        snapshot::write(&snapshot).map_err(|message| failure("snapshot_write_failed", message))?;
        emitter
            .stage(HelperStage::PasteSubmitted)
            .map_err(|message| failure("event_write_failed", message))?;
    }
    thread::sleep(Duration::from_millis(500));
    let restoration = restore_if_current(
        &owner,
        &mut snapshot,
        emitter,
        submission == SubmissionState::Submitted,
    )?;
    Ok(DeliveryReceipt {
        transaction_id: request.transaction_id,
        submission,
        restoration,
    })
}

fn recover(
    request: &HelperRequest,
    emitter: &mut Emitter,
) -> Result<DeliveryReceipt, (String, String)> {
    if request.text.is_some() || request.target.is_some() {
        return Err(failure(
            "invalid_recover_request",
            "recover must not contain text or target",
        ));
    }
    let owner = platform::ClipboardOwner::create()
        .map_err(|message| failure("clipboard_owner_failed", message))?;
    let mut snapshot = snapshot::read(request.transaction_id)
        .map_err(|message| failure("snapshot_read_failed", message))?;
    let restoration = restore_if_current(&owner, &mut snapshot, emitter, true)?;
    Ok(DeliveryReceipt {
        transaction_id: request.transaction_id,
        submission: SubmissionState::NotSubmitted,
        restoration,
    })
}

fn restore_if_current(
    owner: &platform::ClipboardOwner,
    snapshot: &mut snapshot::Snapshot,
    emitter: &mut Emitter,
    emit_stage: bool,
) -> Result<RestorationState, (String, String)> {
    let (sequence, digest) = match (snapshot.payload_sequence, snapshot.payload_sha256) {
        (Some(sequence), Some(digest)) => (sequence, digest),
        _ => {
            return Err(failure(
                "restore_identity_missing",
                "snapshot has no complete payload identity",
            ))
        }
    };
    snapshot.phase = Some(HelperStage::RestoreStarted);
    snapshot::write(snapshot).map_err(|message| failure("snapshot_write_failed", message))?;
    if emit_stage {
        emitter
            .stage(HelperStage::RestoreStarted)
            .map_err(|message| failure("event_write_failed", message))?;
    }
    let restoration =
        match platform::restore_clipboard_if_current(owner, snapshot, sequence, &digest)
            .map_err(|message| failure("clipboard_restore_failed", message))?
        {
            platform::RestoreOutcome::Restored => RestorationState::Restored,
            platform::RestoreOutcome::SkippedConcurrentChange => {
                RestorationState::SkippedConcurrentChange
            }
        };
    snapshot::remove(snapshot.transaction_id);
    Ok(restoration)
}

fn submission_from_stage(stage: Option<HelperStage>) -> SubmissionState {
    match stage {
        Some(HelperStage::PasteSubmitted) | Some(HelperStage::RestoreStarted) => {
            SubmissionState::Submitted
        }
        Some(HelperStage::PasteSubmitting) => SubmissionState::Unknown,
        _ => SubmissionState::NotSubmitted,
    }
}

fn failure(code: impl Into<String>, message: impl Into<String>) -> (String, String) {
    (code.into(), message.into())
}

fn validate_text_and_target(text: &str, executable_path: &str) -> Result<(), (String, String)> {
    for character in text.chars() {
        let codepoint = character as u32;
        let forbidden_control =
            (codepoint <= 0x1F && character != '\n') || (0x7F..=0x9F).contains(&codepoint);
        let forbidden_bidi = matches!(
            codepoint,
            0x061C
                | 0x200E
                | 0x200F
                | 0x202A..=0x202E
                | 0x2066..=0x2069
        );
        if forbidden_control || forbidden_bidi {
            return Err(failure(
                "invalid_request",
                format!("delivery text contains forbidden U+{codepoint:04X}"),
            ));
        }
    }
    let executable = std::path::Path::new(executable_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    const COMMAND_SURFACES: &[&str] = &[
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
        "windowsterminal.exe",
        "openconsole.exe",
        "conhost.exe",
    ];
    if text.contains('\n')
        && COMMAND_SURFACES
            .iter()
            .any(|candidate| executable.eq_ignore_ascii_case(candidate))
    {
        return Err(failure(
            "multiline_delivery_requires_user_action",
            "multiline text is not automatically delivered to command surfaces",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_classification_never_retries_near_submission() {
        assert_eq!(submission_from_stage(None), SubmissionState::NotSubmitted);
        assert_eq!(
            submission_from_stage(Some(HelperStage::PayloadWritten)),
            SubmissionState::NotSubmitted
        );
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
    fn helper_revalidates_text_and_terminal_multiline_policy() {
        assert!(validate_text_and_target("hello", r"C:\\Apps\\editor.exe").is_ok());
        assert!(validate_text_and_target("bad\u{202e}", r"C:\\Apps\\editor.exe").is_err());
        assert!(validate_text_and_target("one\ntwo", r"C:\\Windows\\pwsh.exe").is_err());
    }
}
