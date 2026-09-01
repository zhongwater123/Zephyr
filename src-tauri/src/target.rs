use crate::target_port::CapturedTarget;
use serde::Serialize;
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const MAX_OUTPUT_CHARACTERS: usize = 8_000;
pub const MAX_PENDING_OUTPUTS: usize = 5;
pub const PENDING_OUTPUT_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingOutput {
    pub id: String,
    pub session_id: u64,
    pub text: String,
    pub executable_name: String,
    pub window_title: Option<String>,
    pub created_at_unix_ms: u64,
    pub expires_at_unix_ms: u64,
    pub target_available: bool,
    pub reason_code: String,
    pub reason_message: String,
    pub delivery_certainty: DeliveryCertainty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeliveryCertainty {
    Retryable,
    MayHaveBeenSubmitted,
}

#[derive(Clone, Debug)]
pub struct PendingOutputRecord {
    pub dto: PendingOutput,
    pub target: CapturedTarget,
    expires_at: Instant,
}

pub struct PendingOutputDraft {
    pub session_id: u64,
    pub text: String,
    pub target: CapturedTarget,
    pub target_available: bool,
    pub reason_code: String,
    pub reason_message: String,
    pub certainty: DeliveryCertainty,
}

#[derive(Debug, Default)]
pub struct PendingOutputStore {
    entries: VecDeque<PendingOutputRecord>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PendingOutputError {
    Full,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputValidationError {
    Empty,
    TooLong,
    ForbiddenCharacter { index: usize, codepoint: u32 },
}

/// Normalizes and validates text for SmartDictation's atomic-paste path.
///
/// Legacy delivery intentionally keeps using validate_output_text, whose
/// historical contract rejects every control character, including newlines.
/// SmartDictation permits LF as the sole control character after normalizing
/// CRLF and bare CR to LF.
pub fn normalize_smart_output_text(text: &str) -> Result<String, OutputValidationError> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    validate_output_text_inner(&normalized, true)?;
    Ok(normalized)
}

impl PendingOutputStore {
    pub fn is_full(&mut self) -> bool {
        self.purge_expired();
        self.entries.len() >= MAX_PENDING_OUTPUTS
    }

    pub fn push(&mut self, draft: PendingOutputDraft) -> Result<PendingOutput, PendingOutputError> {
        self.push_with_ttl(draft, PENDING_OUTPUT_TTL)
    }

    fn push_with_ttl(
        &mut self,
        draft: PendingOutputDraft,
        ttl: Duration,
    ) -> Result<PendingOutput, PendingOutputError> {
        self.purge_expired();
        if self.entries.len() >= MAX_PENDING_OUTPUTS {
            return Err(PendingOutputError::Full);
        }

        let now = Instant::now();
        let created_at_unix_ms = unix_time_ms();
        let ttl_ms = ttl.as_millis() as u64;
        let PendingOutputDraft {
            session_id,
            text,
            target,
            target_available,
            reason_code,
            reason_message,
            certainty,
        } = draft;
        let dto = PendingOutput {
            id: uuid::Uuid::new_v4().to_string(),
            session_id,
            text,
            executable_name: target.context().application_key.clone(),
            window_title: target.context().window_title.clone(),
            created_at_unix_ms,
            expires_at_unix_ms: created_at_unix_ms.saturating_add(ttl_ms),
            target_available,
            reason_code,
            reason_message,
            delivery_certainty: certainty,
        };

        self.entries.push_back(PendingOutputRecord {
            dto: dto.clone(),
            target,
            expires_at: now + ttl,
        });
        Ok(dto)
    }

    pub fn list<F>(&mut self, mut target_available: F) -> Vec<PendingOutput>
    where
        F: FnMut(&CapturedTarget) -> bool,
    {
        self.purge_expired();
        self.entries
            .iter()
            .map(|record| {
                let mut dto = record.dto.clone();
                dto.target_available = target_available(&record.target);
                dto
            })
            .collect()
    }

    pub fn get(&mut self, id: &str) -> Option<PendingOutputRecord> {
        self.purge_expired();
        self.entries
            .iter()
            .find(|record| record.dto.id == id)
            .cloned()
    }

    pub fn remove(&mut self, id: &str) -> Option<PendingOutputRecord> {
        self.purge_expired();
        let index = self.entries.iter().position(|record| record.dto.id == id)?;
        self.entries.remove(index)
    }

    pub fn update_delivery_failure(
        &mut self,
        id: &str,
        certainty: DeliveryCertainty,
        reason_code: impl Into<String>,
        reason_message: impl Into<String>,
    ) -> bool {
        self.purge_expired();
        let Some(record) = self.entries.iter_mut().find(|record| record.dto.id == id) else {
            return false;
        };
        record.dto.delivery_certainty = certainty;
        record.dto.reason_code = reason_code.into();
        record.dto.reason_message = reason_message.into();
        true
    }

    fn purge_expired(&mut self) {
        let now = Instant::now();
        self.entries.retain(|record| record.expires_at > now);
    }
}

pub fn validate_output_text(text: &str) -> Result<(), OutputValidationError> {
    validate_output_text_inner(text, false)
}

fn validate_output_text_inner(text: &str, allow_lf: bool) -> Result<(), OutputValidationError> {
    if text.is_empty() {
        return Err(OutputValidationError::Empty);
    }

    let mut count = 0usize;
    for (index, character) in text.chars().enumerate() {
        count += 1;
        if count > MAX_OUTPUT_CHARACTERS {
            return Err(OutputValidationError::TooLong);
        }

        let codepoint = character as u32;
        let is_bidi_override_or_isolate = matches!(codepoint, 0x202A..=0x202E | 0x2066..=0x2069);
        if (character.is_control() && !(allow_lf && character == '\n'))
            || is_bidi_override_or_isolate
        {
            return Err(OutputValidationError::ForbiddenCharacter { index, codepoint });
        }
    }

    Ok(())
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_port::tests::fake_target;

    fn pending_draft(session_id: u64, text: impl Into<String>) -> PendingOutputDraft {
        PendingOutputDraft {
            session_id,
            text: text.into(),
            target: fake_target("app.exe"),
            target_available: true,
            reason_code: "test".to_string(),
            reason_message: "test".to_string(),
            certainty: DeliveryCertainty::Retryable,
        }
    }

    #[test]
    fn validates_safe_text() {
        assert_eq!(validate_output_text("你好，world 🙂"), Ok(()));
    }

    #[test]
    fn rejects_control_and_bidi_characters() {
        assert!(matches!(
            validate_output_text("line\nnext"),
            Err(OutputValidationError::ForbiddenCharacter { .. })
        ));
        assert!(matches!(
            validate_output_text("safe\u{202e}unsafe"),
            Err(OutputValidationError::ForbiddenCharacter { .. })
        ));
    }

    #[test]
    fn rejects_more_than_character_limit() {
        let text = "字".repeat(MAX_OUTPUT_CHARACTERS + 1);
        assert_eq!(
            validate_output_text(&text),
            Err(OutputValidationError::TooLong)
        );
    }

    #[test]
    fn pending_store_never_overwrites_the_sixth_result() {
        let mut store = PendingOutputStore::default();
        for index in 0..MAX_PENDING_OUTPUTS {
            store
                .push(pending_draft(index as u64, format!("result {index}")))
                .unwrap();
        }
        assert!(matches!(
            store.push(pending_draft(9, "sixth")),
            Err(PendingOutputError::Full)
        ));
        assert_eq!(store.list(|_| true).len(), MAX_PENDING_OUTPUTS);
        assert!(store
            .list(|_| true)
            .iter()
            .all(|entry| entry.text != "sixth"));
    }

    #[test]
    fn pending_store_purges_expired_results() {
        let mut store = PendingOutputStore::default();
        store
            .push_with_ttl(pending_draft(1, "expired"), Duration::ZERO)
            .unwrap();
        assert!(store.list(|_| true).is_empty());
    }

    #[test]
    fn smart_output_normalizes_crlf_and_bare_cr_to_lf() {
        assert_eq!(
            normalize_smart_output_text("first\r\nsecond\rthird").unwrap(),
            "first\nsecond\nthird"
        );
    }

    #[test]
    fn smart_output_allows_only_lf_among_control_characters() {
        assert_eq!(
            normalize_smart_output_text("line\nnext").unwrap(),
            "line\nnext"
        );
        for text in ["column\tnext", "safe\0unsafe", "safe\u{2066}unsafe"] {
            assert!(matches!(
                normalize_smart_output_text(text),
                Err(OutputValidationError::ForbiddenCharacter { .. })
            ));
        }
    }

    #[test]
    fn smart_output_counts_characters_after_normalization() {
        let exactly_limit = format!("{}\r\n", "x".repeat(MAX_OUTPUT_CHARACTERS - 1));
        assert_eq!(
            normalize_smart_output_text(&exactly_limit)
                .unwrap()
                .chars()
                .count(),
            MAX_OUTPUT_CHARACTERS
        );
        let over_limit = format!("{}\r\n", "x".repeat(MAX_OUTPUT_CHARACTERS));
        assert_eq!(
            normalize_smart_output_text(&over_limit),
            Err(OutputValidationError::TooLong)
        );
    }
}
