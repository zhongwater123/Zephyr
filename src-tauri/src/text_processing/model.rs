use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub const DEFAULT_TEXT_PROCESSING_MODEL: &str = "deepseek-v4-flash";
pub const TEXT_PROCESSING_DEADLINE: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrozenTranscript(String);

impl FrozenTranscript {
    pub fn new(text: String) -> Result<Self, TranscriptError> {
        if text.trim().is_empty() {
            return Err(TranscriptError::Empty);
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum TranscriptError {
    #[error("ASR final transcript is empty")]
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivationIntent {
    SmartDictation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum PolishLevel {
    Fast = 0,
    Light = 1,
    Standard = 2,
    Deep = 3,
}

impl PolishLevel {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn is_fast(self) -> bool {
        matches!(self, Self::Fast)
    }
}

impl Default for PolishLevel {
    fn default() -> Self {
        Self::Standard
    }
}

impl TryFrom<u8> for PolishLevel {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Light),
            0 => Ok(Self::Fast),
            2 => Ok(Self::Standard),
            3 => Ok(Self::Deep),
            _ => Err("polish level must be 0, 1, 2, or 3"),
        }
    }
}

impl From<PolishLevel> for u8 {
    fn from(value: PolishLevel) -> Self {
        value.as_u8()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessingPlan {
    pub config_revision: u64,
    pub polish_level: PolishLevel,
    pub target_executable: String,
    pub application_name: Option<String>,
    pub deadline: Duration,
    pub max_characters: usize,
}

impl ProcessingPlan {
    pub fn new(
        config_revision: u64,
        polish_level: PolishLevel,
        target_executable: impl Into<String>,
        application_name: Option<String>,
    ) -> Self {
        Self {
            config_revision,
            polish_level,
            target_executable: target_executable.into(),
            application_name: application_name
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            deadline: TEXT_PROCESSING_DEADLINE,
            max_characters: crate::target::MAX_OUTPUT_CHARACTERS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_transcript_preserves_the_authoritative_text_exactly() {
        let original = "  第一段\r\n第二段  ".to_string();
        let frozen = FrozenTranscript::new(original.clone()).unwrap();

        assert_eq!(frozen.as_str(), original);
        assert_eq!(frozen.into_inner(), original);
    }

    #[test]
    fn frozen_transcript_rejects_empty_or_whitespace_only_finals() {
        assert_eq!(
            FrozenTranscript::new(String::new()),
            Err(TranscriptError::Empty)
        );
        assert_eq!(
            FrozenTranscript::new(" \r\n\t".to_string()),
            Err(TranscriptError::Empty)
        );
    }

    #[test]
    fn polish_level_is_numeric_strict_and_defaults_to_standard() {
        for (raw, level) in [
            (1, PolishLevel::Light),
            (2, PolishLevel::Standard),
            (0, PolishLevel::Fast),
            (3, PolishLevel::Deep),
        ] {
            assert_eq!(PolishLevel::try_from(raw), Ok(level));
            assert_eq!(level.as_u8(), raw);
        }
        assert!(PolishLevel::Fast.is_fast());
        assert!(PolishLevel::try_from(4).is_err());
        assert_eq!(PolishLevel::default(), PolishLevel::Standard);
    }

    #[test]
    fn processing_plan_freezes_only_allowed_app_context() {
        let plan = ProcessingPlan::new(
            8,
            PolishLevel::Deep,
            "WINWORD.EXE",
            Some("  Microsoft Word  ".to_string()),
        );
        assert_eq!(plan.target_executable, "WINWORD.EXE");
        assert_eq!(plan.application_name.as_deref(), Some("Microsoft Word"));
        assert_eq!(plan.config_revision, 8);
    }
}
