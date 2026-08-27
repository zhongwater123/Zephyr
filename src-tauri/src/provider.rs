pub mod volcengine;

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc::Receiver, watch};

pub use volcengine::{
    VolcengineAdapter, VolcengineAuth, VolcengineAuthMode, VolcengineRuntimeProfile,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderError {
    #[error("未检测到可识别的语音")]
    NoSpeech,
    #[error("识别服务拒绝鉴权: {0}")]
    AuthenticationRejected(String),
    #[error("识别服务配额已用尽: {0}")]
    QuotaExceeded(String),
    #[error("识别服务配置无效: {0}")]
    InvalidConfiguration(String),
    #[error("识别服务网络错误: {0}")]
    Network(String),
    #[error("识别服务协议错误: {0}")]
    Protocol(String),
    #[error("识别服务请求超时")]
    Timeout,
    #[error("识别已取消")]
    Cancelled,
}

impl ProviderError {
    pub fn user_message(&self) -> String {
        self.to_string()
    }

    pub fn cancel_reason(&self) -> &'static str {
        match self {
            Self::NoSpeech => "no_speech",
            Self::AuthenticationRejected(_) => "authentication_rejected",
            Self::QuotaExceeded(_) => "quota_exceeded",
            Self::InvalidConfiguration(_) => "invalid_configuration",
            Self::Network(_) => "network",
            Self::Protocol(_) => "protocol",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioChunk {
    pub bytes: Bytes,
    pub duration_ms: u16,
    pub is_final: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStreamInfo {
    pub sample_rate: u32,
    pub channels: u16,
    pub encoding: &'static str,
    pub chunk_duration_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptEvent {
    pub text: String,
    pub is_final: bool,
    pub utterances: Vec<TranscriptUtterance>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptUtterance {
    pub text: String,
    pub start_time: Option<u32>,
    pub end_time: Option<u32>,
    pub definite: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AsrSessionHints {
    pub hotwords: Vec<String>,
    pub profile_context: Option<String>,
    pub app_context: Option<String>,
}

#[async_trait]
pub trait StreamingTranscriptionProvider: Send + Sync {
    async fn transcribe_stream(
        &self,
        info: AudioStreamInfo,
        chunks: Receiver<AudioChunk>,
        events: watch::Sender<Option<TranscriptEvent>>,
        hints: Option<AsrSessionHints>,
    ) -> Result<String, ProviderError>;
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct MockProvider;

#[cfg(test)]
#[async_trait]
impl StreamingTranscriptionProvider for MockProvider {
    async fn transcribe_stream(
        &self,
        _info: AudioStreamInfo,
        mut chunks: Receiver<AudioChunk>,
        events: watch::Sender<Option<TranscriptEvent>>,
        _hints: Option<AsrSessionHints>,
    ) -> Result<String, ProviderError> {
        let mut non_empty_chunks = 0usize;
        let mut saw_final = false;
        while let Some(chunk) = chunks.recv().await {
            if !chunk.bytes.is_empty() {
                non_empty_chunks += 1;
                let _ = events.send(Some(TranscriptEvent {
                    text: format!("模拟增量结果（{non_empty_chunks}）"),
                    is_final: false,
                    utterances: Vec::new(),
                }));
            }
            if chunk.is_final {
                saw_final = true;
                break;
            }
        }
        if non_empty_chunks == 0 || !saw_final {
            return Err(ProviderError::NoSpeech);
        }
        let final_text = format!("模拟识别结果（{non_empty_chunks} 个音频包）");
        let _ = events.send(Some(TranscriptEvent {
            text: final_text.clone(),
            is_final: true,
            utterances: Vec::new(),
        }));
        Ok(final_text)
    }
}

#[derive(Debug, Clone)]
pub struct UnavailableProvider {
    message: String,
}

impl UnavailableProvider {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
impl StreamingTranscriptionProvider for UnavailableProvider {
    async fn transcribe_stream(
        &self,
        _info: AudioStreamInfo,
        _chunks: Receiver<AudioChunk>,
        _events: watch::Sender<Option<TranscriptEvent>>,
        _hints: Option<AsrSessionHints>,
    ) -> Result<String, ProviderError> {
        Err(ProviderError::InvalidConfiguration(self.message.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_info() -> AudioStreamInfo {
        AudioStreamInfo {
            sample_rate: 16_000,
            channels: 1,
            encoding: "pcm_s16le",
            chunk_duration_ms: 200,
        }
    }

    #[tokio::test]
    async fn mock_provider_normalizes_empty_and_unfinished_audio() {
        for chunk in [
            AudioChunk {
                bytes: Bytes::new(),
                duration_ms: 0,
                is_final: true,
            },
            AudioChunk {
                bytes: Bytes::from_static(&[1, 2, 3]),
                duration_ms: 200,
                is_final: false,
            },
        ] {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            let (event_tx, _) = tokio::sync::watch::channel(None);
            tx.send(chunk).await.unwrap();
            drop(tx);
            assert_eq!(
                MockProvider
                    .transcribe_stream(stream_info(), rx, event_tx, None)
                    .await
                    .unwrap_err(),
                ProviderError::NoSpeech
            );
        }
    }

    #[tokio::test]
    async fn mock_provider_emits_final_event_for_complete_audio() {
        let (tx, rx) = tokio::sync::mpsc::channel(2);
        let (event_tx, event_rx) = tokio::sync::watch::channel(None);
        tx.send(AudioChunk {
            bytes: Bytes::from_static(&[1, 2, 3]),
            duration_ms: 200,
            is_final: false,
        })
        .await
        .unwrap();
        tx.send(AudioChunk {
            bytes: Bytes::from_static(&[4, 5, 6]),
            duration_ms: 200,
            is_final: true,
        })
        .await
        .unwrap();
        drop(tx);

        let transcript = MockProvider
            .transcribe_stream(stream_info(), rx, event_tx, None)
            .await
            .unwrap();

        assert_eq!(transcript, "模拟识别结果（2 个音频包）");
        assert_eq!(
            event_rx.borrow().clone(),
            Some(TranscriptEvent {
                text: transcript,
                is_final: true,
                utterances: Vec::new(),
            })
        );
    }

    #[tokio::test]
    async fn unavailable_provider_preserves_configuration_failure() {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        let (event_tx, _) = tokio::sync::watch::channel(None);

        let error = UnavailableProvider::new("missing credential")
            .transcribe_stream(stream_info(), rx, event_tx, None)
            .await
            .unwrap_err();

        assert_eq!(
            error,
            ProviderError::InvalidConfiguration("missing credential".to_string())
        );
    }

    #[test]
    fn provider_errors_have_stable_cancel_reasons() {
        let cases = [
            (ProviderError::NoSpeech, "no_speech"),
            (
                ProviderError::AuthenticationRejected("rejected".to_string()),
                "authentication_rejected",
            ),
            (
                ProviderError::QuotaExceeded("exhausted".to_string()),
                "quota_exceeded",
            ),
            (
                ProviderError::InvalidConfiguration("invalid".to_string()),
                "invalid_configuration",
            ),
            (ProviderError::Network("offline".to_string()), "network"),
            (ProviderError::Protocol("bad frame".to_string()), "protocol"),
            (ProviderError::Timeout, "timeout"),
            (ProviderError::Cancelled, "cancelled"),
        ];

        for (error, expected) in cases {
            assert_eq!(error.cancel_reason(), expected);
        }
    }
}
