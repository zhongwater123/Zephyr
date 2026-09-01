use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
pub const SELF_INJECTED_MARKER: usize = 0x4759_5459_5049_4E47u64 as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HelperOperation {
    SelfCheck,
    Deliver,
    Recover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryMode {
    Unicode,
    ClipboardPaste,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubmissionState {
    NotSubmitted,
    Submitted,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RestorationState {
    NotNeeded,
    Restored,
    SkippedConcurrentChange,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelperStage {
    SnapshotComplete,
    PayloadWriteStarted,
    PayloadWritten,
    TargetVerified,
    PasteSubmitting,
    PasteSubmitted,
    RestoreStarted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetIdentity {
    pub hwnd: i64,
    pub process_id: u32,
    pub process_started_at: u64,
    pub executable_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperRequest {
    pub protocol_version: u16,
    pub operation: HelperOperation,
    pub transaction_id: Uuid,
    pub mode: Option<DeliveryMode>,
    pub text: Option<String>,
    pub target: Option<TargetIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fault_at: Option<HelperStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_input_count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReceipt {
    pub transaction_id: Uuid,
    pub submission: SubmissionState,
    pub restoration: RestorationState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HelperEventKind {
    SelfCheck {
        #[serde(rename = "helperVersion")]
        helper_version: String,
    },
    Stage {
        stage: HelperStage,
    },
    Terminal {
        receipt: DeliveryReceipt,
        code: Option<String>,
        message: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelperEvent {
    pub protocol_version: u16,
    pub transaction_id: Uuid,
    pub sequence: u32,
    #[serde(flatten)]
    pub event: HelperEventKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_and_stage_names_are_stable() {
        let request = HelperRequest {
            protocol_version: PROTOCOL_VERSION,
            operation: HelperOperation::Deliver,
            transaction_id: Uuid::nil(),
            mode: Some(DeliveryMode::ClipboardPaste),
            text: Some("hello".to_string()),
            target: Some(TargetIdentity {
                hwnd: 1,
                process_id: 2,
                process_started_at: 3,
                executable_path: r"C:\\Apps\\editor.exe".to_string(),
            }),
            fault_at: None,
            send_input_count: None,
        };
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(value["mode"], "clipboardPaste");
    }
}
