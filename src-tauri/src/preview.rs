use crate::provider::{TranscriptEvent, TranscriptUtterance};
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct TranscriptPreviewState {
    latest_text: String,
    confirmed_chars: usize,
    last_event_at: Option<Instant>,
}

impl TranscriptPreviewState {
    pub fn apply_event(&mut self, event: &TranscriptEvent) -> String {
        if !event.text.trim().is_empty() {
            self.latest_text = event.text.clone();
        }
        if let Some(confirmed_chars) = confirmed_chars_from_utterances(&event.utterances) {
            self.confirmed_chars = confirmed_chars;
        }
        self.last_event_at = Some(Instant::now());
        self.latest_text.clone()
    }

    pub fn rendered_text(&self) -> String {
        self.latest_text.clone()
    }

    pub fn confirmed_chars(&self) -> usize {
        self.confirmed_chars
    }

    pub fn last_event_at(&self) -> Option<Instant> {
        self.last_event_at
    }
}

fn confirmed_chars_from_utterances(utterances: &[TranscriptUtterance]) -> Option<usize> {
    if utterances.is_empty() {
        return None;
    }

    Some(
        utterances
            .iter()
            .filter(|utterance| utterance.definite)
            .map(|utterance| utterance.text.chars().count())
            .sum(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(text: &str, utterances: Vec<TranscriptUtterance>) -> TranscriptEvent {
        TranscriptEvent {
            text: text.to_string(),
            is_final: utterances.iter().any(|utterance| utterance.definite),
            utterances,
        }
    }

    fn utterance(text: &str, definite: bool) -> TranscriptUtterance {
        TranscriptUtterance {
            text: text.to_string(),
            start_time: None,
            end_time: None,
            definite,
        }
    }

    #[test]
    fn consecutive_partials_replace_latest_text_without_appending() {
        let mut state = TranscriptPreviewState::default();

        assert_eq!(state.apply_event(&event("one two", Vec::new())), "one two");
        assert_eq!(
            state.apply_event(&event("one two three", Vec::new())),
            "one two three"
        );

        assert_eq!(state.rendered_text(), "one two three");
    }

    #[test]
    fn long_counting_text_does_not_duplicate_previous_hypotheses() {
        let mut state = TranscriptPreviewState::default();

        state.apply_event(&event("1 2 3 4 5", Vec::new()));
        state.apply_event(&event("1 2 3 4 5 6 7 8 9 10", Vec::new()));
        state.apply_event(&event("1 2 3 4 5 6 7 8 9 10 11 12", Vec::new()));

        assert_eq!(state.rendered_text(), "1 2 3 4 5 6 7 8 9 10 11 12");
    }

    #[test]
    fn definite_utterances_update_confirmed_chars_without_changing_text() {
        let mut state = TranscriptPreviewState::default();

        let text = state.apply_event(&event(
            "hello world again",
            vec![
                utterance("hello ", true),
                utterance("world", true),
                utterance(" again", false),
            ],
        ));

        assert_eq!(text, "hello world again");
        assert_eq!(state.confirmed_chars(), "hello world".chars().count());
        assert!(state.last_event_at().is_some());
    }
}
