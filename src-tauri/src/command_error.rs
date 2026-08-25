use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

pub type CommandResult<T> = Result<T, CommandError>;

impl CommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(
        code: impl Into<String>,
        message: impl Into<String>,
        details: serde_json::Value,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: Some(details),
        }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::new("internal_error", message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::new("internal_error", message)
    }
}

impl From<crate::config::ConfigError> for CommandError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self::new("config_error", error.to_string())
    }
}

pub fn require_window(window: &tauri::WebviewWindow, expected: &str) -> CommandResult<()> {
    if window.label() == expected {
        Ok(())
    } else {
        Err(CommandError::with_details(
            "permission_denied",
            "当前窗口无权调用此命令",
            serde_json::json!({
                "expectedWindow": expected,
                "actualWindow": window.label(),
            }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_error_serialization_keeps_the_ipc_contract() {
        let plain = serde_json::to_value(CommandError::new("permission_denied", "denied"))
            .expect("serialize command error");
        assert_eq!(
            plain,
            serde_json::json!({ "code": "permission_denied", "message": "denied" })
        );

        let detailed = serde_json::to_value(CommandError::with_details(
            "config_conflict",
            "conflict",
            serde_json::json!({ "currentRevision": 4 }),
        ))
        .expect("serialize detailed command error");
        assert_eq!(detailed["details"]["currentRevision"], 4);
    }
}
