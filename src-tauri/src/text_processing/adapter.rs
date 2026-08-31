use super::{
    FrozenTranscript, PolishLevel, ProcessingPlan, PromptDocument, DEFAULT_TEXT_PROCESSING_MODEL,
};
use crate::config::EndpointPurpose;
use crate::repositories::CredentialStore;
use crate::services::ConfigService;
use async_trait::async_trait;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use url::Url;
const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
const BASE_URL_ENV: &str = "ZEPHYR_TEXT_PROCESSING_BASE_URL";
const MODEL_ENV: &str = "ZEPHYR_TEXT_PROCESSING_MODEL";
const MAX_TOKENS: u32 = 16_384;
const MAX_RESPONSE_BYTES: usize = 128 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub struct TextProcessingDeployment {
    pub base_url: String,
    pub model: String,
}

impl TextProcessingDeployment {
    pub fn from_deployment() -> Self {
        Self {
            base_url: std::env::var(BASE_URL_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: std::env::var(MODEL_ENV)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_TEXT_PROCESSING_MODEL.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcessingRequest {
    pub plan: ProcessingPlan,
    pub prompt: PromptDocument,
    pub transcript: FrozenTranscript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessedText {
    pub text: String,
    pub polish_level: PolishLevel,
    pub model: String,
    pub prompt_version: String,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ProcessingFailure {
    #[error("text processing was cancelled")]
    Cancelled,
    #[error("text processing exceeded its deadline")]
    Timeout,
    #[error("text processing endpoint is not authorized")]
    Unauthorized,
    #[error("shared DeepSeek credential is missing")]
    MissingCredential,
    #[error("shared DeepSeek credential is unavailable")]
    CredentialUnavailable,
    #[error("DeepSeek request failed")]
    HttpFailed,
    #[error("DeepSeek returned empty content")]
    EmptyContent,
    #[error("DeepSeek returned invalid JSON")]
    InvalidJson,
    #[error("DeepSeek returned an invalid response schema")]
    InvalidSchema,
    #[error("DeepSeek stopped at the token limit")]
    FinishLength,
    #[error("processed text exceeds the character limit")]
    OutputTooLong,
    #[error("processed text contains a forbidden character")]
    ForbiddenCharacter,
}

impl ProcessingFailure {
    pub const fn reason_code(&self) -> &'static str {
        match self {
            Self::Cancelled => "processing_cancelled",
            Self::Timeout => "processing_timeout",
            Self::Unauthorized => "processing_unauthorized",
            Self::MissingCredential => "processing_missing_key",
            Self::CredentialUnavailable => "processing_credential_unavailable",
            Self::HttpFailed => "processing_http_failed",
            Self::EmptyContent => "processing_empty_content",
            Self::InvalidJson => "processing_invalid_json",
            Self::InvalidSchema => "processing_invalid_schema",
            Self::FinishLength => "processing_finish_length",
            Self::OutputTooLong => "processing_output_too_long",
            Self::ForbiddenCharacter => "processing_forbidden_character",
        }
    }
}

#[async_trait]
pub trait TextProcessor: Send + Sync {
    async fn process(&self, request: ProcessingRequest)
        -> Result<ProcessedText, ProcessingFailure>;
}

pub struct DeepSeekTextProcessor {
    config: Arc<ConfigService>,
    credentials: Arc<dyn CredentialStore>,
    deployment: TextProcessingDeployment,
    transport: Arc<dyn DeepSeekTransport>,
}

impl DeepSeekTextProcessor {
    pub fn production(
        config: Arc<ConfigService>,
        credentials: Arc<dyn CredentialStore>,
    ) -> Result<Self, ProcessingFailure> {
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(super::TEXT_PROCESSING_DEADLINE)
            .build()
            .map_err(|_| ProcessingFailure::HttpFailed)?;
        Ok(Self {
            config,
            credentials,
            deployment: TextProcessingDeployment::from_deployment(),
            transport: Arc::new(ReqwestDeepSeekTransport { client }),
        })
    }

    #[cfg(test)]
    fn with_transport(
        config: Arc<ConfigService>,
        credentials: Arc<dyn CredentialStore>,
        deployment: TextProcessingDeployment,
        transport: Arc<dyn DeepSeekTransport>,
    ) -> Self {
        Self {
            config,
            credentials,
            deployment,
            transport,
        }
    }

    async fn process_once(
        &self,
        request: ProcessingRequest,
    ) -> Result<ProcessedText, ProcessingFailure> {
        let endpoint = chat_completions_url(&self.deployment.base_url)?;
        if !self
            .config
            .snapshot()
            .is_endpoint_trusted(&endpoint, EndpointPurpose::TextProcessing)
        {
            return Err(ProcessingFailure::Unauthorized);
        }
        let key = self
            .credentials
            .load_deepseek_api_key()
            .map_err(|_| ProcessingFailure::CredentialUnavailable)?
            .filter(|value| !value.trim().is_empty())
            .ok_or(ProcessingFailure::MissingCredential)?;
        let payload = build_request_payload(&request, &self.deployment.model)?;
        let response = self.transport.execute(&endpoint, &key, payload).await?;
        parse_response(
            &response.body,
            response.status,
            &request,
            &self.deployment.model,
        )
    }
}

#[async_trait]
impl TextProcessor for DeepSeekTextProcessor {
    async fn process(
        &self,
        request: ProcessingRequest,
    ) -> Result<ProcessedText, ProcessingFailure> {
        let deadline = request.plan.deadline;
        let started = Instant::now();
        match tokio::time::timeout(deadline, self.process_once(request)).await {
            Err(_) => Err(ProcessingFailure::Timeout),
            Ok(_) if started.elapsed() >= deadline => Err(ProcessingFailure::Timeout),
            Ok(result) => result,
        }
    }
}

struct TransportResponse {
    status: StatusCode,
    body: Vec<u8>,
}

#[async_trait]
trait DeepSeekTransport: Send + Sync {
    async fn execute(
        &self,
        endpoint: &str,
        api_key: &str,
        payload: Value,
    ) -> Result<TransportResponse, ProcessingFailure>;
}

struct ReqwestDeepSeekTransport {
    client: reqwest::Client,
}

#[async_trait]
impl DeepSeekTransport for ReqwestDeepSeekTransport {
    async fn execute(
        &self,
        endpoint: &str,
        api_key: &str,
        payload: Value,
    ) -> Result<TransportResponse, ProcessingFailure> {
        let mut response = self
            .client
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|_| ProcessingFailure::HttpFailed)?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ProcessingFailure::InvalidSchema);
        }
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| ProcessingFailure::HttpFailed)?
        {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(ProcessingFailure::InvalidSchema);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(TransportResponse { status, body })
    }
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Deserialize)]
struct ChatCompletionChoice {
    finish_reason: String,
    message: ChatCompletionMessage,
}

#[derive(Deserialize)]
struct ChatCompletionMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OutputDocument {
    text: String,
}

fn build_request_payload(
    request: &ProcessingRequest,
    model: &str,
) -> Result<Value, ProcessingFailure> {
    let user_data = serde_json::to_string(&json!({
        "raw_text": request.transcript.as_str(),
        "target_application": {
            "executable": request.plan.target_executable,
            "name": request.plan.application_name,
        },
        "polish_level": request.plan.polish_level.as_u8(),
    }))
    .map_err(|_| ProcessingFailure::InvalidSchema)?;
    let system = format!(
        "{}\n\n输出协议：只输出合法 json 对象，不要 Markdown、解释或额外字段。\nEXAMPLE JSON OUTPUT:\n{{\"text\":\"可直接交付的最终文本\"}}",
        request.prompt.content.trim_end()
    );
    Ok(json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user_data}
        ],
        "response_format": {"type": "json_object"},
        "thinking": {"type": "disabled"},
        "stream": false,
        "max_tokens": MAX_TOKENS
    }))
}

fn parse_response(
    body: &[u8],
    status: StatusCode,
    request: &ProcessingRequest,
    model: &str,
) -> Result<ProcessedText, ProcessingFailure> {
    if status != StatusCode::OK {
        return Err(ProcessingFailure::HttpFailed);
    }
    let value: Value = serde_json::from_slice(body).map_err(|_| ProcessingFailure::InvalidJson)?;
    let response: ChatCompletionResponse =
        serde_json::from_value(value).map_err(|_| ProcessingFailure::InvalidSchema)?;
    if response.choices.len() != 1 {
        return Err(ProcessingFailure::InvalidSchema);
    }
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or(ProcessingFailure::InvalidSchema)?;
    if choice.finish_reason == "length" {
        return Err(ProcessingFailure::FinishLength);
    }
    if choice.finish_reason != "stop" {
        return Err(ProcessingFailure::InvalidSchema);
    }
    let content = choice
        .message
        .content
        .filter(|value| !value.trim().is_empty())
        .ok_or(ProcessingFailure::EmptyContent)?;
    let value: Value =
        serde_json::from_str(&content).map_err(|_| ProcessingFailure::InvalidJson)?;
    let output: OutputDocument =
        serde_json::from_value(value).map_err(|_| ProcessingFailure::InvalidSchema)?;
    let text = normalize_and_validate_output(&output.text, request.plan.max_characters)?;
    Ok(ProcessedText {
        text,
        polish_level: request.plan.polish_level,
        model: model.to_string(),
        prompt_version: request.prompt.version.clone(),
    })
}

fn normalize_and_validate_output(
    value: &str,
    max_characters: usize,
) -> Result<String, ProcessingFailure> {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    if normalized.trim().is_empty() {
        return Err(ProcessingFailure::EmptyContent);
    }
    if normalized.chars().count() > max_characters {
        return Err(ProcessingFailure::OutputTooLong);
    }
    for character in normalized.chars() {
        let codepoint = character as u32;
        let bidi = matches!(codepoint, 0x202A..=0x202E | 0x2066..=0x2069);
        if (character.is_control() && character != '\n') || bidi {
            return Err(ProcessingFailure::ForbiddenCharacter);
        }
    }
    Ok(normalized)
}

fn chat_completions_url(base_url: &str) -> Result<String, ProcessingFailure> {
    let base = base_url.trim().trim_end_matches('/');
    let endpoint = if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    };
    let parsed = Url::parse(&endpoint).map_err(|_| ProcessingFailure::Unauthorized)?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err(ProcessingFailure::Unauthorized);
    }
    Ok(endpoint)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AppConfig, ConfigError, ConfigRecovery, CredentialSnapshot, CredentialUpdates, LoadedConfig,
    };
    use crate::repositories::ConfigRepository;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct MemoryConfigRepository;

    impl ConfigRepository for MemoryConfigRepository {
        fn load(&self) -> Result<LoadedConfig, ConfigError> {
            unreachable!("initialized from a supplied snapshot")
        }

        fn save(&self, _config: &AppConfig) -> Result<(), ConfigError> {
            Ok(())
        }
    }

    struct CountingCredentialStore {
        reads: AtomicUsize,
    }

    impl CredentialStore for CountingCredentialStore {
        fn load_api_key(&self) -> Result<Option<String>, ConfigError> {
            Ok(None)
        }

        fn load_app_key(&self) -> Result<Option<String>, ConfigError> {
            Ok(None)
        }

        fn load_access_key(&self) -> Result<Option<String>, ConfigError> {
            Ok(None)
        }

        fn load_deepseek_api_key(&self) -> Result<Option<String>, ConfigError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(Some("secret".to_string()))
        }

        fn update_transactionally(
            &self,
            _updates: &CredentialUpdates,
        ) -> Result<CredentialSnapshot, ConfigError> {
            Ok(CredentialSnapshot {
                api_key: None,
                app_key: None,
                access_key: None,
                hotword_agent_api_key: None,
            })
        }

        fn restore(&self, _snapshot: &CredentialSnapshot) -> Result<(), ConfigError> {
            Ok(())
        }
    }

    struct PanicTransport;

    #[async_trait]
    impl DeepSeekTransport for PanicTransport {
        async fn execute(
            &self,
            _endpoint: &str,
            _api_key: &str,
            _payload: Value,
        ) -> Result<TransportResponse, ProcessingFailure> {
            panic!("transport must not run before trust and credential checks")
        }
    }

    fn request(max_characters: usize) -> ProcessingRequest {
        ProcessingRequest {
            plan: ProcessingPlan {
                config_revision: 8,
                polish_level: PolishLevel::Standard,
                target_executable: "WINWORD.EXE".to_string(),
                application_name: Some("Microsoft Word".to_string()),
                deadline: Duration::from_secs(20),
                max_characters,
            },
            prompt: PromptDocument {
                version: "smart-polish-v2".to_string(),
                sha256: "0".repeat(64),
                content: "Unified smart polishing prompt".to_string(),
            },
            transcript: FrozenTranscript::new("嗯，测试一下".to_string()).unwrap(),
        }
    }

    fn text_content(text: &str) -> String {
        serde_json::to_string(&json!({"text": text})).unwrap()
    }

    fn completion(content: Option<String>, finish_reason: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1,
            "model": "deepseek-v4-flash",
            "system_fingerprint": "test",
            "choices": [{
                "index": 0,
                "finish_reason": finish_reason,
                "logprobs": null,
                "message": {
                    "role": "assistant",
                    "content": content,
                    "reasoning_content": null
                }
            }],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 1,
                "total_tokens": 2
            }
        }))
        .unwrap()
    }

    #[test]
    fn payload_uses_flash_json_output_without_thinking_or_streaming() {
        let payload =
            build_request_payload(&request(8_000), DEFAULT_TEXT_PROCESSING_MODEL).unwrap();

        assert_eq!(payload["model"], "deepseek-v4-flash");
        assert_eq!(payload["response_format"]["type"], "json_object");
        assert_eq!(payload["thinking"]["type"], "disabled");
        assert_eq!(payload["stream"], false);
        assert_eq!(payload["max_tokens"], MAX_TOKENS);
        assert!(payload["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("json"));
        let data: Value =
            serde_json::from_str(payload["messages"][1]["content"].as_str().unwrap()).unwrap();
        assert_eq!(
            data,
            json!({
                "raw_text": "嗯，测试一下",
                "target_application": {
                    "executable": "WINWORD.EXE",
                    "name": "Microsoft Word"
                },
                "polish_level": 2
            })
        );
    }

    #[test]
    fn official_response_metadata_is_allowed_but_content_has_one_strict_field() {
        let request = request(8_000);
        let parsed = parse_response(
            &completion(Some(text_content("整理后的文本")), "stop"),
            StatusCode::OK,
            &request,
            DEFAULT_TEXT_PROCESSING_MODEL,
        )
        .unwrap();
        assert_eq!(parsed.polish_level, PolishLevel::Standard);
        assert_eq!(parsed.text, "整理后的文本");
        assert_eq!(parsed.model, "deepseek-v4-flash");

        let extra = serde_json::to_string(&json!({"text": "ok", "extra": true})).unwrap();
        assert_eq!(
            parse_response(
                &completion(Some(extra), "stop"),
                StatusCode::OK,
                &request,
                DEFAULT_TEXT_PROCESSING_MODEL,
            ),
            Err(ProcessingFailure::InvalidSchema)
        );
    }

    #[test]
    fn response_rejects_empty_length_8001_and_control_but_accepts_8000() {
        let request = request(8_000);
        let parse = |content: Option<String>, finish: &str| {
            parse_response(
                &completion(content, finish),
                StatusCode::OK,
                &request,
                DEFAULT_TEXT_PROCESSING_MODEL,
            )
        };

        assert_eq!(parse(None, "stop"), Err(ProcessingFailure::EmptyContent));
        assert_eq!(
            parse(Some(String::new()), "stop"),
            Err(ProcessingFailure::EmptyContent)
        );
        assert_eq!(
            parse(Some(text_content("partial")), "length"),
            Err(ProcessingFailure::FinishLength)
        );
        assert!(parse(Some(text_content(&"字".repeat(8_000))), "stop").is_ok());
        assert_eq!(
            parse(Some(text_content(&"字".repeat(8_001))), "stop"),
            Err(ProcessingFailure::OutputTooLong)
        );
        assert_eq!(
            parse(Some(text_content("unsafe\ttext")), "stop"),
            Err(ProcessingFailure::ForbiddenCharacter)
        );
    }

    #[tokio::test]
    async fn untrusted_endpoint_fails_before_shared_credential_read() {
        let credentials = Arc::new(CountingCredentialStore {
            reads: AtomicUsize::new(0),
        });
        let config = Arc::new(ConfigService::new(
            LoadedConfig {
                config: AppConfig::default(),
                recovery: ConfigRecovery::None,
            },
            Arc::new(MemoryConfigRepository),
            credentials.clone(),
        ));
        let processor = DeepSeekTextProcessor::with_transport(
            config,
            credentials.clone(),
            TextProcessingDeployment {
                base_url: "https://untrusted.example".to_string(),
                model: DEFAULT_TEXT_PROCESSING_MODEL.to_string(),
            },
            Arc::new(PanicTransport),
        );

        assert_eq!(
            processor.process(request(8_000)).await,
            Err(ProcessingFailure::Unauthorized)
        );
        assert_eq!(credentials.reads.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    #[ignore = "requires live DeepSeek credentials and network access"]
    async fn live_deployment_credential_completes_text_processing_request() {
        let credentials: Arc<dyn CredentialStore> =
            Arc::new(crate::repositories::WindowsCredentialStore);
        let config = Arc::new(ConfigService::new(
            LoadedConfig {
                config: AppConfig::default(),
                recovery: ConfigRecovery::None,
            },
            Arc::new(MemoryConfigRepository),
            credentials.clone(),
        ));
        let processor = DeepSeekTextProcessor::production(config, credentials).unwrap();

        let result = processor.process(request(8_000)).await.unwrap();

        assert!(!result.text.trim().is_empty());
        assert_eq!(result.model, DEFAULT_TEXT_PROCESSING_MODEL);
    }
}
