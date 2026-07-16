use crate::config::{ProviderConfig, RecognitionBehaviorConfig};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::io::{Read, Write};
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::{timeout, Duration};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use url::Url;
use uuid::Uuid;

const WEBSOCKET_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const WS_OPCODE_TEXT: u8 = 0x1;
const WS_OPCODE_BINARY: u8 = 0x2;
const WS_OPCODE_CLOSE: u8 = 0x8;
const WS_OPCODE_PING: u8 = 0x9;
const WS_OPCODE_PONG: u8 = 0xa;
const WEBSOCKET_ACCEPT_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_HANDSHAKE_BYTES: usize = 64 * 1024;
const MAX_WS_FRAME_PAYLOAD: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("transcription result is empty")]
    EmptyTranscript,
    #[error("streaming transcription ended before a final result")]
    MissingFinalTranscript,
    #[error("provider request failed: {0}")]
    Request(String),
    #[error("provider protocol error: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioChunk {
    pub bytes: Vec<u8>,
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
        chunks: UnboundedReceiver<AudioChunk>,
        events: UnboundedSender<TranscriptEvent>,
        hints: Option<AsrSessionHints>,
    ) -> Result<String, ProviderError>;
}

#[derive(Debug, Default)]
pub struct MockProvider;

#[derive(Debug, Clone)]
pub struct VolcengineAsrProvider {
    config: ProviderConfig,
    behavior: RecognitionBehaviorConfig,
    auth: VolcengineAuth,
}

#[derive(Debug, Clone, Default)]
pub struct VolcengineAuth {
    pub api_key: Option<String>,
    pub app_key: Option<String>,
    pub access_key: Option<String>,
}

impl VolcengineAsrProvider {
    pub fn new(
        config: ProviderConfig,
        behavior: RecognitionBehaviorConfig,
        auth: VolcengineAuth,
    ) -> Self {
        Self {
            config,
            behavior,
            auth,
        }
    }

    pub async fn probe_connection(&self) -> Result<String, ProviderError> {
        let request_id = Uuid::new_v4().to_string();

        log::info!(
            "probing volcengine asr websocket: endpoint={}, resource_id={}, request_id={}",
            self.config.base_url,
            self.config.resource_id,
            request_id
        );
        let mut connection = connect_raw_websocket(&self.config, &self.auth, &request_id).await?;
        let logid = connection
            .response
            .header("x-tt-logid")
            .unwrap_or("missing");
        log::info!("volcengine asr websocket probe connected, logid={logid}");

        if let Err(error) = write_ws_frame(&mut connection.stream, WS_OPCODE_CLOSE, &[]).await {
            log::warn!("failed to close volcengine asr websocket probe: {error}");
        }

        Ok(format!(
            "WebSocket 已连接。X-Tt-Logid: {logid}。按住 Ctrl+Alt+Space 可测试真实双向流式识别。"
        ))
    }
}

#[async_trait]
impl StreamingTranscriptionProvider for MockProvider {
    async fn transcribe_stream(
        &self,
        _info: AudioStreamInfo,
        mut chunks: UnboundedReceiver<AudioChunk>,
        events: UnboundedSender<TranscriptEvent>,
        _hints: Option<AsrSessionHints>,
    ) -> Result<String, ProviderError> {
        let mut non_empty_chunks = 0usize;
        let mut saw_final = false;

        while let Some(chunk) = chunks.recv().await {
            if !chunk.bytes.is_empty() {
                non_empty_chunks += 1;
                let _ = events.send(TranscriptEvent {
                    text: format!("模拟增量结果（{non_empty_chunks}）"),
                    is_final: false,
                    utterances: Vec::new(),
                });
            }

            if chunk.is_final {
                saw_final = true;
                break;
            }
        }

        if non_empty_chunks == 0 {
            return Err(ProviderError::EmptyTranscript);
        }

        if !saw_final {
            return Err(ProviderError::MissingFinalTranscript);
        }

        let final_text = format!("模拟识别结果（{non_empty_chunks} 个音频包）");
        let _ = events.send(TranscriptEvent {
            text: final_text.clone(),
            is_final: true,
            utterances: Vec::new(),
        });
        Ok(final_text)
    }
}

#[async_trait]
impl StreamingTranscriptionProvider for VolcengineAsrProvider {
    async fn transcribe_stream(
        &self,
        info: AudioStreamInfo,
        mut chunks: UnboundedReceiver<AudioChunk>,
        events: UnboundedSender<TranscriptEvent>,
        hints: Option<AsrSessionHints>,
    ) -> Result<String, ProviderError> {
        let request_id = Uuid::new_v4().to_string();

        log::info!(
            "connecting volcengine asr websocket: endpoint={}, resource_id={}, request_id={}",
            self.config.base_url,
            self.config.resource_id,
            request_id
        );
        let mut connection = connect_raw_websocket(&self.config, &self.auth, &request_id).await?;
        log_websocket_connected(&connection.response);

        let (mut reader, mut writer) = tokio::io::split(&mut connection.stream);
        let (raw_frame_tx, mut raw_frame_rx) = tokio::sync::mpsc::unbounded_channel();
        let read_frames = async {
            loop {
                let frame = read_ws_frame(&mut reader).await?;
                if raw_frame_tx.send(frame).is_err() {
                    return Ok::<(), ProviderError>(());
                }
            }
        };

        let mut sequence: i32 = 1;
        let full_request = build_full_client_request(
            &self.config,
            &self.behavior,
            &info,
            sequence,
            hints.as_ref(),
        )?;
        log::info!(
            "volcengine asr sending full client request: seq={}, sample_rate={}, channels={}, chunk_ms={}, async_mode={}",
            sequence,
            info.sample_rate,
            info.channels,
            info.chunk_duration_ms,
            is_async_bidirectional(&self.config)
        );
        write_ws_frame(&mut writer, WS_OPCODE_BINARY, &full_request).await?;
        sequence += 1;

        let drive_stream = async {
            let mut sent_final_audio = false;
            let mut sent_audio_chunks = 0usize;
            let mut last_text = String::new();
            let mut final_text = String::new();

            loop {
                tokio::select! {
                    chunk = chunks.recv(), if !sent_final_audio => {
                        match chunk {
                            Some(chunk) => {
                                let current_sequence = if chunk.is_final { -sequence } else { sequence };
                                let frame = build_audio_request(&chunk.bytes, current_sequence)?;
                                write_ws_frame(&mut writer, WS_OPCODE_BINARY, &frame).await?;
                                if !chunk.bytes.is_empty() {
                                    sent_audio_chunks += 1;
                                }
                                sequence += 1;
                                if chunk.is_final {
                                    log::info!(
                                        "volcengine asr sent final audio chunk: seq={}, chunks={}, final_ms={}",
                                        current_sequence,
                                        sent_audio_chunks,
                                        chunk.duration_ms
                                    );
                                    sent_final_audio = true;
                                }
                            }
                            None => {
                                let current_sequence = -sequence;
                                let frame = build_audio_request(&[], current_sequence)?;
                                write_ws_frame(&mut writer, WS_OPCODE_BINARY, &frame).await?;
                                log::info!(
                                    "volcengine asr audio channel closed; sent final empty chunk: seq={}, chunks={}",
                                    current_sequence,
                                    sent_audio_chunks
                                );
                                sent_final_audio = true;
                            }
                        }
                    }
                    frame = raw_frame_rx.recv() => {
                        let Some(frame) = frame else {
                            break;
                        };
                        match frame.opcode {
                            WS_OPCODE_CLOSE => {
                                log::info!("volcengine asr websocket raw close frame: {}", hex_preview(&frame.payload));
                                break;
                            }
                            WS_OPCODE_PING => {
                                log::debug!("volcengine asr websocket raw ping: {}", hex_preview(&frame.payload));
                                write_ws_frame(&mut writer, WS_OPCODE_PONG, &frame.payload).await?;
                                continue;
                            }
                            WS_OPCODE_PONG => {
                                log::debug!("volcengine asr websocket raw pong: {}", hex_preview(&frame.payload));
                                continue;
                            }
                            WS_OPCODE_TEXT | WS_OPCODE_BINARY => {}
                            opcode => {
                                log::debug!(
                                    "volcengine asr websocket raw ignored opcode={opcode}, payload={}",
                                    hex_preview(&frame.payload)
                                );
                                continue;
                            }
                        }

                        log_raw_server_frame(frame.opcode, &frame.payload);
                        match parse_server_message(&frame.payload)? {
                            ServerMessage::Transcript {
                                text,
                                is_final,
                                is_last_package,
                                utterances,
                            } => {
                                if !text.trim().is_empty() && (text != last_text || is_final) {
                                    last_text = text.clone();
                                    log::debug!(
                                        "volcengine asr transcript update: final={}, utterances={}, chars={}",
                                        is_final,
                                        utterances.len(),
                                        text.chars().count()
                                    );
                                    let _ = events.send(TranscriptEvent {
                                        text: text.clone(),
                                        is_final,
                                        utterances,
                                    });
                                }
                                if is_last_package {
                                    final_text = text;
                                    break;
                                }
                            }
                            ServerMessage::Empty => {}
                        }
                    }
                }

                if sent_final_audio && !final_text.is_empty() {
                    break;
                }
            }

            if final_text.trim().is_empty() {
                Err(ProviderError::MissingFinalTranscript)
            } else {
                Ok(final_text)
            }
        };

        tokio::pin!(read_frames);
        tokio::pin!(drive_stream);
        tokio::select! {
            result = &mut drive_stream => result,
            result = &mut read_frames => match result {
                Ok(()) => Err(ProviderError::MissingFinalTranscript),
                Err(error) => Err(error),
            },
        }
    }
}

enum ServerMessage {
    Transcript {
        text: String,
        is_final: bool,
        is_last_package: bool,
        utterances: Vec<TranscriptUtterance>,
    },
    Empty,
}

fn log_websocket_connected(response: &HandshakeResponse) {
    if let Some(logid) = response.header("x-tt-logid") {
        log::info!("volcengine asr connected, logid={logid}");
    } else {
        log::info!("volcengine asr connected, X-Tt-Logid header missing");
    }
}

fn map_io_error(context: &str, error: std::io::Error) -> ProviderError {
    log::warn!("volcengine asr websocket raw io error during {context}: {error}");
    ProviderError::Request(format!("{context}: {error}"))
}

struct RawWebSocketConnection {
    stream: TlsStream<TcpStream>,
    response: HandshakeResponse,
}

struct HandshakeResponse {
    status_code: u16,
    status_line: String,
    headers: Vec<(String, String)>,
}

impl HandshakeResponse {
    fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

struct RawWsFrame {
    opcode: u8,
    payload: Vec<u8>,
}

async fn connect_raw_websocket(
    config: &ProviderConfig,
    auth: &VolcengineAuth,
    request_id: &str,
) -> Result<RawWebSocketConnection, ProviderError> {
    timeout(
        WEBSOCKET_CONNECT_TIMEOUT,
        connect_raw_websocket_inner(config, auth, request_id),
    )
    .await
    .map_err(|_| {
        ProviderError::Request(format!(
            "websocket connect timed out after {}s",
            WEBSOCKET_CONNECT_TIMEOUT.as_secs()
        ))
    })?
}

async fn connect_raw_websocket_inner(
    config: &ProviderConfig,
    auth: &VolcengineAuth,
    request_id: &str,
) -> Result<RawWebSocketConnection, ProviderError> {
    let url = Url::parse(&config.base_url)
        .map_err(|error| ProviderError::Request(format!("invalid websocket URL: {error}")))?;
    if url.scheme() != "wss" {
        return Err(ProviderError::Request(format!(
            "unsupported websocket URL scheme: {}",
            url.scheme()
        )));
    }

    let host = url
        .host_str()
        .ok_or_else(|| ProviderError::Request("websocket URL host is missing".to_string()))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ProviderError::Request("websocket URL port is missing".to_string()))?;
    let path = websocket_request_path(&url);

    let tcp = TcpStream::connect((host, port))
        .await
        .map_err(|error| map_io_error("connect tcp socket", error))?;
    let mut stream = TlsConnector::from(Arc::new(tls_client_config()?))
        .connect(
            ServerName::try_from(host.to_string()).map_err(|error| {
                ProviderError::Request(format!("invalid TLS server name: {error}"))
            })?,
            tcp,
        )
        .await
        .map_err(|error| map_io_error("connect tls socket", error))?;

    let websocket_key = websocket_key();
    let request =
        build_handshake_request(host, port, &path, &websocket_key, config, auth, request_id)?;
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| map_io_error("write websocket handshake", error))?;
    stream
        .flush()
        .await
        .map_err(|error| map_io_error("flush websocket handshake", error))?;

    let mut response_bytes = read_handshake_response(&mut stream).await?;
    let response = parse_handshake_response(&response_bytes)?;
    if response.status_code != 101 {
        if let Some(content_length) = response
            .header("content-length")
            .and_then(|value| value.parse::<usize>().ok())
        {
            let mut body = vec![0u8; content_length];
            stream
                .read_exact(&mut body)
                .await
                .map_err(|error| map_io_error("read websocket handshake body", error))?;
            response_bytes.extend_from_slice(&body);
        }
    }
    log::debug!(
        "volcengine asr raw handshake response: {}",
        String::from_utf8_lossy(&response_bytes)
    );
    log::info!(
        "volcengine asr handshake parsed: status={}, line={}",
        response.status_code,
        response.status_line
    );

    if response.status_code != 101 {
        return Err(ProviderError::Request(format!(
            "websocket handshake failed: {}",
            response.status_line
        )));
    }

    let expected_accept = websocket_accept(&websocket_key);
    let actual_accept = response.header("sec-websocket-accept").unwrap_or("");
    if actual_accept.trim() != expected_accept {
        return Err(ProviderError::Protocol(format!(
            "websocket accept mismatch: expected {expected_accept}, got {actual_accept}"
        )));
    }

    Ok(RawWebSocketConnection { stream, response })
}

fn tls_client_config() -> Result<ClientConfig, ProviderError> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut roots = RootCertStore::empty();
    let certs = rustls_native_certs::load_native_certs();
    let cert_count = certs.certs.len();
    let error_count = certs.errors.len();
    let (valid_count, invalid_count) = roots.add_parsable_certificates(certs.certs);
    log::debug!(
        "loaded native TLS certificates: raw={}, valid={}, invalid={}, load_errors={}",
        cert_count,
        valid_count,
        invalid_count,
        error_count
    );

    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

fn websocket_request_path(url: &Url) -> String {
    let mut path = if url.path().is_empty() {
        "/".to_string()
    } else {
        url.path().to_string()
    };
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    path
}

fn build_handshake_request(
    host: &str,
    port: u16,
    path: &str,
    websocket_key: &str,
    config: &ProviderConfig,
    auth: &VolcengineAuth,
    request_id: &str,
) -> Result<String, ProviderError> {
    let host_header = if port == 443 {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };

    let mut lines = vec![
        format!("GET {path} HTTP/1.1"),
        format!("Host: {host_header}"),
        "Upgrade: websocket".to_string(),
        "Connection: Upgrade".to_string(),
        "Sec-WebSocket-Version: 13".to_string(),
        format!("Sec-WebSocket-Key: {websocket_key}"),
        format!("X-Api-Resource-Id: {}", config.resource_id),
        format!("X-Api-Request-Id: {request_id}"),
    ];

    if config.auth_mode.trim().eq_ignore_ascii_case("api_key") {
        let api_key = auth
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ProviderError::Request("X-Api-Key is not configured.".to_string()))?;
        lines.push(format!("X-Api-Key: {api_key}"));
    } else {
        let app_key = auth
            .app_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ProviderError::Request("X-Api-App-Key is not configured.".to_string())
            })?;
        let access_key = auth
            .access_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ProviderError::Request("X-Api-Access-Key is not configured.".to_string())
            })?;
        lines.push(format!("X-Api-App-Key: {app_key}"));
        lines.push(format!("X-Api-Access-Key: {access_key}"));
    }

    lines.push("\r\n".to_string());
    Ok(lines.join("\r\n"))
}

async fn read_handshake_response<R>(reader: &mut R) -> Result<Vec<u8>, ProviderError>
where
    R: AsyncRead + Unpin,
{
    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        reader
            .read_exact(&mut byte)
            .await
            .map_err(|error| map_io_error("read websocket handshake", error))?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            return Ok(response);
        }
        if response.len() > MAX_HANDSHAKE_BYTES {
            return Err(ProviderError::Protocol(format!(
                "websocket handshake response too large: {} bytes",
                response.len()
            )));
        }
    }
}

fn parse_handshake_response(bytes: &[u8]) -> Result<HandshakeResponse, ProviderError> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| ProviderError::Protocol("empty websocket handshake response".to_string()))?
        .to_string();
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| {
            ProviderError::Protocol(format!("invalid websocket status line: {status_line}"))
        })?
        .parse::<u16>()
        .map_err(|error| {
            ProviderError::Protocol(format!("invalid websocket status code: {error}"))
        })?;

    let headers = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();

    Ok(HandshakeResponse {
        status_code,
        status_line,
        headers,
    })
}

fn websocket_key() -> String {
    BASE64.encode(Uuid::new_v4().as_bytes())
}

fn websocket_accept(key: &str) -> String {
    let mut sha1 = Sha1::new();
    sha1.update(key.as_bytes());
    sha1.update(WEBSOCKET_ACCEPT_GUID.as_bytes());
    BASE64.encode(sha1.finalize())
}

async fn read_ws_frame<R>(reader: &mut R) -> Result<RawWsFrame, ProviderError>
where
    R: AsyncRead + Unpin,
{
    let mut header = [0u8; 2];
    reader
        .read_exact(&mut header)
        .await
        .map_err(|error| map_io_error("read websocket frame header", error))?;

    let opcode = header[0] & 0x0f;
    let masked = header[1] & 0x80 != 0;
    let mut payload_len = (header[1] & 0x7f) as u64;

    if payload_len == 126 {
        let mut len = [0u8; 2];
        reader
            .read_exact(&mut len)
            .await
            .map_err(|error| map_io_error("read websocket frame extended length", error))?;
        payload_len = u16::from_be_bytes(len) as u64;
    } else if payload_len == 127 {
        let mut len = [0u8; 8];
        reader
            .read_exact(&mut len)
            .await
            .map_err(|error| map_io_error("read websocket frame extended length", error))?;
        payload_len = u64::from_be_bytes(len);
    }

    if payload_len as usize > MAX_WS_FRAME_PAYLOAD {
        return Err(ProviderError::Protocol(format!(
            "websocket frame payload too large: {payload_len}"
        )));
    }

    let mask = if masked {
        let mut mask = [0u8; 4];
        reader
            .read_exact(&mut mask)
            .await
            .map_err(|error| map_io_error("read websocket frame mask", error))?;
        Some(mask)
    } else {
        None
    };

    let mut payload = vec![0u8; payload_len as usize];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|error| map_io_error("read websocket frame payload", error))?;

    if let Some(mask) = mask {
        apply_websocket_mask(&mut payload, mask);
    }

    Ok(RawWsFrame { opcode, payload })
}

async fn write_ws_frame<W>(writer: &mut W, opcode: u8, payload: &[u8]) -> Result<(), ProviderError>
where
    W: AsyncWrite + Unpin,
{
    let mask = websocket_mask();
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.push(0x80 | (opcode & 0x0f));

    if payload.len() < 126 {
        frame.push(0x80 | payload.len() as u8);
    } else if payload.len() <= u16::MAX as usize {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(payload.len() as u64).to_be_bytes());
    }

    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| *byte ^ mask[index % 4]),
    );

    writer
        .write_all(&frame)
        .await
        .map_err(|error| map_io_error("write websocket frame", error))?;
    writer
        .flush()
        .await
        .map_err(|error| map_io_error("flush websocket frame", error))
}

fn websocket_mask() -> [u8; 4] {
    let uuid = Uuid::new_v4();
    let bytes = uuid.as_bytes();
    [bytes[0], bytes[1], bytes[2], bytes[3]]
}

fn apply_websocket_mask(payload: &mut [u8], mask: [u8; 4]) {
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % 4];
    }
}

fn log_raw_server_frame(opcode: u8, bytes: &[u8]) {
    let header = bytes
        .get(0..4)
        .map(hex_preview)
        .unwrap_or_else(|| "<short>".to_string());
    log::debug!(
        "volcengine asr raw websocket frame: opcode={}, len={}, protocol_header={}, preview={}",
        opcode,
        bytes.len(),
        header,
        hex_preview(bytes)
    );
}

fn hex_preview(bytes: &[u8]) -> String {
    const MAX_BYTES: usize = 96;
    let mut out = bytes
        .iter()
        .take(MAX_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    if bytes.len() > MAX_BYTES {
        out.push_str(" ...");
    }
    out
}

fn build_full_client_request(
    config: &ProviderConfig,
    behavior: &RecognitionBehaviorConfig,
    info: &AudioStreamInfo,
    sequence: i32,
    hints: Option<&AsrSessionHints>,
) -> Result<Vec<u8>, ProviderError> {
    let mut audio = json!({
        "format": "pcm",
        "codec": "raw",
        "rate": info.sample_rate,
        "bits": 16,
        "channel": info.channels,
    });

    if config.base_url.contains("nostream") && !config.language.trim().is_empty() {
        audio["language"] = Value::String(config.language.clone());
    }

    let mut request = json!({
        "model_name": config.model,
        "enable_nonstream": true,
        "enable_itn": behavior.enable_itn,
        "enable_punc": behavior.enable_punc,
        "enable_ddc": behavior.enable_ddc,
        "enable_accelerate_text": behavior.enable_accelerate_text,
        "show_utterances": true,
        "result_type": "full",
        "end_window_size": 800,
        "force_to_speech_time": 1000,
    });

    if let Some(context) = hints.and_then(build_corpus_context) {
        request["corpus"] = json!({ "context": context });
    }

    let payload = json!({
        "user": {
            "uid": "gy-typing",
            "platform": "Windows",
            "sdk_version": "gy-typing-0.1.0",
        },
        "audio": audio,
        "request": request
    });

    let bytes =
        serde_json::to_vec(&payload).map_err(|error| ProviderError::Request(error.to_string()))?;
    build_protocol_frame(0x1, 0x1, 0x1, 0x1, Some(sequence), &bytes)
}

fn build_corpus_context(hints: &AsrSessionHints) -> Option<String> {
    const TOKEN_BUDGET: usize = 100;
    let mut used = 0usize;
    let mut hotwords = Vec::new();
    for word in &hints.hotwords {
        let word = word.trim();
        if word.is_empty() {
            continue;
        }
        let cost = estimate_context_tokens(word);
        if used + cost > TOKEN_BUDGET {
            break;
        }
        used += cost;
        hotwords.push(json!({ "word": word }));
    }

    let mut context_data = Vec::new();
    for text in [hints.app_context.as_deref(), hints.profile_context.as_deref()]
        .into_iter()
        .flatten()
    {
        let remaining = TOKEN_BUDGET.saturating_sub(used);
        if remaining == 0 {
            break;
        }
        let text = truncate_context_text(text.trim(), remaining);
        if text.is_empty() {
            continue;
        }
        used += estimate_context_tokens(&text);
        context_data.push(json!({ "text": text }));
    }

    if hotwords.is_empty() && context_data.is_empty() {
        return None;
    }

    let mut context = json!({});
    if !hotwords.is_empty() {
        context["hotwords"] = Value::Array(hotwords);
    }
    if !context_data.is_empty() {
        context["context_type"] = Value::String("dialog_ctx".to_string());
        context["context_data"] = Value::Array(context_data);
    }
    serde_json::to_string(&context).ok()
}

fn estimate_context_tokens(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count().max(1)
}

fn truncate_context_text(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn is_async_bidirectional(config: &ProviderConfig) -> bool {
    config
        .base_url
        .trim()
        .trim_end_matches('/')
        .ends_with("/bigmodel_async")
}

fn build_audio_request(audio: &[u8], sequence: i32) -> Result<Vec<u8>, ProviderError> {
    let flags = if sequence < 0 { 0x3 } else { 0x1 };
    build_protocol_frame(0x2, flags, 0x0, 0x1, Some(sequence), audio)
}

fn build_protocol_frame(
    message_type: u8,
    flags: u8,
    serialization: u8,
    compression: u8,
    sequence: Option<i32>,
    payload: &[u8],
) -> Result<Vec<u8>, ProviderError> {
    let payload = if compression == 0x1 {
        gzip(payload)?
    } else {
        payload.to_vec()
    };
    let sequence_len = if sequence.is_some() { 4 } else { 0 };
    let mut frame = Vec::with_capacity(8 + sequence_len + payload.len());
    frame.push(0x11);
    frame.push((message_type << 4) | flags);
    frame.push((serialization << 4) | compression);
    frame.push(0x00);
    if let Some(sequence) = sequence {
        frame.extend_from_slice(&sequence.to_be_bytes());
    }
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

fn parse_server_message(bytes: &[u8]) -> Result<ServerMessage, ProviderError> {
    if bytes.len() < 4 {
        return Err(ProviderError::Protocol(
            "server frame is too short".to_string(),
        ));
    }

    let header_size = ((bytes[0] & 0x0f) as usize) * 4;
    let message_type = bytes[1] >> 4;
    let flags = bytes[1] & 0x0f;
    let serialization = bytes[2] >> 4;
    let compression = bytes[2] & 0x0f;

    if bytes.len() < header_size {
        return Err(ProviderError::Protocol(
            "invalid server header size".to_string(),
        ));
    }

    match message_type {
        0x9 => parse_transcript_response(bytes, header_size, flags, serialization, compression),
        0xf => parse_error_response(bytes, header_size),
        _ => Ok(ServerMessage::Empty),
    }
}

fn parse_transcript_response(
    bytes: &[u8],
    header_size: usize,
    flags: u8,
    serialization: u8,
    compression: u8,
) -> Result<ServerMessage, ProviderError> {
    let mut offset = header_size;
    let has_sequence = flags & 0x01 != 0;
    let is_last_package = flags & 0x02 != 0;
    let has_event = flags & 0x04 != 0;

    if has_sequence {
        offset += 4;
    }
    if has_event {
        offset += 4;
    }
    if bytes.len() < offset + 4 {
        return Ok(ServerMessage::Empty);
    }

    let payload_size = u32::from_be_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .map_err(|_| ProviderError::Protocol("invalid payload size".to_string()))?,
    ) as usize;
    offset += 4;
    if bytes.len() < offset + payload_size {
        return Err(ProviderError::Protocol(
            "truncated server payload".to_string(),
        ));
    }

    let payload = decode_payload(&bytes[offset..offset + payload_size], compression)?;
    log::debug!(
        "volcengine asr raw response payload: {}",
        String::from_utf8_lossy(&payload)
    );
    if serialization != 0x1 {
        return Ok(ServerMessage::Empty);
    }

    let json: Value = serde_json::from_slice(&payload)
        .map_err(|error| ProviderError::Protocol(error.to_string()))?;
    let text = extract_text(&json);
    let utterances = extract_utterances(&json);
    let is_final =
        is_last_package || utterances.iter().any(|utterance| utterance.definite) || extract_definite(&json);
    Ok(if text.trim().is_empty() {
        ServerMessage::Empty
    } else {
        ServerMessage::Transcript {
            text,
            is_final,
            is_last_package,
            utterances,
        }
    })
}

fn parse_error_response(bytes: &[u8], header_size: usize) -> Result<ServerMessage, ProviderError> {
    if bytes.len() < header_size + 8 {
        return Err(ProviderError::Protocol(
            "truncated error response".to_string(),
        ));
    }
    let code = u32::from_be_bytes(
        bytes[header_size..header_size + 4]
            .try_into()
            .map_err(|_| ProviderError::Protocol("invalid error code".to_string()))?,
    );
    let size = u32::from_be_bytes(
        bytes[header_size + 4..header_size + 8]
            .try_into()
            .map_err(|_| ProviderError::Protocol("invalid error size".to_string()))?,
    ) as usize;
    let start = header_size + 8;
    let end = (start + size).min(bytes.len());
    let message = String::from_utf8_lossy(&bytes[start..end]).to_string();
    log::warn!("volcengine asr raw error payload: {message}");
    Err(ProviderError::Request(format!(
        "server error {code}: {message}"
    )))
}

fn decode_payload(payload: &[u8], compression: u8) -> Result<Vec<u8>, ProviderError> {
    if compression == 0x1 {
        gunzip(payload)
    } else {
        Ok(payload.to_vec())
    }
}

fn gzip(payload: &[u8]) -> Result<Vec<u8>, ProviderError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(payload)
        .map_err(|error| ProviderError::Request(error.to_string()))?;
    encoder
        .finish()
        .map_err(|error| ProviderError::Request(error.to_string()))
}

fn gunzip(payload: &[u8]) -> Result<Vec<u8>, ProviderError> {
    let mut decoder = GzDecoder::new(payload);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|error| ProviderError::Protocol(error.to_string()))?;
    Ok(out)
}

fn extract_text(value: &Value) -> String {
    if let Some(text) = value.pointer("/result/text").and_then(Value::as_str) {
        return text.to_string();
    }

    if let Some(results) = value.get("result").and_then(Value::as_array) {
        return results
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("");
    }

    String::new()
}

fn extract_definite(value: &Value) -> bool {
    value
        .pointer("/result/utterances")
        .and_then(Value::as_array)
        .map(|utterances| {
            utterances
                .iter()
                .any(|utterance| utterance.get("definite").and_then(Value::as_bool) == Some(true))
        })
        .unwrap_or(false)
}

fn extract_utterances(value: &Value) -> Vec<TranscriptUtterance> {
    value
        .pointer("/result/utterances")
        .and_then(Value::as_array)
        .map(|utterances| {
            utterances
                .iter()
                .filter_map(|utterance| {
                    let text = utterance.get("text")?.as_str()?.to_string();
                    if text.trim().is_empty() {
                        return None;
                    }

                    Some(TranscriptUtterance {
                        text,
                        start_time: json_u32(utterance.get("start_time")),
                        end_time: json_u32(utterance.get("end_time")),
                        definite: utterance
                            .get("definite")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn json_u32(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|number| u32::try_from(number).ok())
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

    fn provider_config() -> ProviderConfig {
        ProviderConfig {
            base_url: "wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async".to_string(),
            auth_mode: "app_access".to_string(),
            resource_id: "volc.bigasr.sauc.duration".to_string(),
            model: "bigmodel".to_string(),
            language: "zh-CN".to_string(),
        }
    }

    fn behavior_config() -> RecognitionBehaviorConfig {
        RecognitionBehaviorConfig::default()
    }

    #[test]
    fn full_client_request_matches_sauc_demo_framing() {
        let frame =
            build_full_client_request(&provider_config(), &behavior_config(), &stream_info(), 1, None)
                .unwrap();

        assert_eq!(&frame[0..4], &[0x11, 0x11, 0x11, 0x00]);
        assert_eq!(i32::from_be_bytes(frame[4..8].try_into().unwrap()), 1);

        let payload_size = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
        assert_eq!(frame.len(), 12 + payload_size);
        let payload: Value = serde_json::from_slice(&gunzip(&frame[12..]).unwrap()).unwrap();

        assert_eq!(
            payload.pointer("/audio/format").and_then(Value::as_str),
            Some("pcm")
        );
        assert_eq!(
            payload
                .pointer("/request/model_name")
                .and_then(Value::as_str),
            Some("bigmodel")
        );
        assert_eq!(
            payload
                .pointer("/request/enable_ddc")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload
                .pointer("/request/enable_nonstream")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/request/enable_accelerate_text")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/request/end_window_size")
                .and_then(Value::as_u64),
            Some(800)
        );
        assert_eq!(
            payload
                .pointer("/request/force_to_speech_time")
                .and_then(Value::as_u64),
            Some(1000)
        );
    }

    #[test]
    fn full_client_request_uses_recognition_behavior_config() {
        let behavior = RecognitionBehaviorConfig {
            enable_itn: false,
            enable_punc: false,
            enable_ddc: true,
            enable_accelerate_text: false,
        };
        let frame =
            build_full_client_request(&provider_config(), &behavior, &stream_info(), 1, None)
                .unwrap();
        let payload_size = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
        let payload: Value =
            serde_json::from_slice(&gunzip(&frame[12..12 + payload_size]).unwrap()).unwrap();

        assert_eq!(
            payload.pointer("/request/enable_itn").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload.pointer("/request/enable_punc").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload.pointer("/request/enable_ddc").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            payload
                .pointer("/request/enable_accelerate_text")
                .and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            payload
                .pointer("/request/enable_nonstream")
                .and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn full_client_request_includes_hotword_context_when_hints_present() {
        let hints = AsrSessionHints {
            hotwords: vec!["Zephyr".to_string(), "火山引擎".to_string()],
            profile_context: Some("用户经常讨论云端语音输入。".to_string()),
            app_context: Some("当前在代码编辑器中输入技术方案。".to_string()),
        };
        let frame =
            build_full_client_request(
                &provider_config(),
                &behavior_config(),
                &stream_info(),
                1,
                Some(&hints),
            )
            .unwrap();
        let payload_size = u32::from_be_bytes(frame[8..12].try_into().unwrap()) as usize;
        let payload: Value = serde_json::from_slice(&gunzip(&frame[12..12 + payload_size]).unwrap()).unwrap();
        let context = payload
            .pointer("/request/corpus/context")
            .and_then(Value::as_str)
            .expect("corpus context should be present");
        let context_json: Value = serde_json::from_str(context).unwrap();

        assert_eq!(
            context_json
                .pointer("/hotwords/0/word")
                .and_then(Value::as_str),
            Some("Zephyr")
        );
        assert!(context_json
            .pointer("/context_data/0/text")
            .and_then(Value::as_str)
            .unwrap()
            .contains("代码编辑器"));
    }

    #[test]
    fn audio_request_uses_positive_and_negative_sequences() {
        let frame = build_audio_request(&[1, 2, 3], 2).unwrap();

        assert_eq!(&frame[0..4], &[0x11, 0x21, 0x01, 0x00]);
        assert_eq!(i32::from_be_bytes(frame[4..8].try_into().unwrap()), 2);
        assert_eq!(gunzip(&frame[12..]).unwrap(), vec![1, 2, 3]);

        let final_frame = build_audio_request(&[], -3).unwrap();

        assert_eq!(&final_frame[0..4], &[0x11, 0x23, 0x01, 0x00]);
        assert_eq!(
            i32::from_be_bytes(final_frame[4..8].try_into().unwrap()),
            -3
        );
        assert!(gunzip(&final_frame[12..]).unwrap().is_empty());
    }

    #[test]
    fn server_response_parser_skips_sequence_and_event_fields() {
        let payload = gzip(r#"{"result":{"text":"hello"}}"#.as_bytes()).unwrap();
        let mut frame = vec![0x11, 0x95, 0x11, 0x00];
        frame.extend_from_slice(&2_i32.to_be_bytes());
        frame.extend_from_slice(&7_i32.to_be_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);

        match parse_server_message(&frame).unwrap() {
            ServerMessage::Transcript {
                text,
                is_final,
                utterances,
                ..
            } => {
                assert_eq!(text, "hello");
                assert!(!is_final);
                assert!(utterances.is_empty());
            }
            ServerMessage::Empty => panic!("expected transcript"),
        }
    }

    #[test]
    fn server_response_parser_extracts_definite_utterances() {
        let payload = gzip(
            r#"{"result":{"text":"hello.","utterances":[{"text":"hello.","start_time":0,"end_time":860,"definite":true}]}}"#
                .as_bytes(),
        )
        .unwrap();
        let mut frame = vec![0x11, 0x91, 0x11, 0x00];
        frame.extend_from_slice(&2_i32.to_be_bytes());
        frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        frame.extend_from_slice(&payload);

        match parse_server_message(&frame).unwrap() {
            ServerMessage::Transcript {
                text,
                is_final,
                is_last_package,
                utterances,
            } => {
                assert_eq!(text, "hello.");
                assert!(is_final);
                assert!(!is_last_package);
                assert_eq!(utterances.len(), 1);
                assert_eq!(utterances[0].text, "hello.");
                assert_eq!(utterances[0].start_time, Some(0));
                assert_eq!(utterances[0].end_time, Some(860));
                assert!(utterances[0].definite);
            }
            ServerMessage::Empty => panic!("expected transcript"),
        }
    }

    #[tokio::test]
    async fn mock_streaming_provider_returns_partial_and_final_events() {
        let provider = MockProvider;
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

        chunk_tx
            .send(AudioChunk {
                bytes: vec![1, 2, 3],
                duration_ms: 200,
                is_final: false,
            })
            .unwrap();
        chunk_tx
            .send(AudioChunk {
                bytes: Vec::new(),
                duration_ms: 0,
                is_final: true,
            })
            .unwrap();
        drop(chunk_tx);

        let final_text = provider
            .transcribe_stream(stream_info(), chunk_rx, event_tx, None)
            .await
            .unwrap();
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        assert_eq!(events[0].text, "模拟增量结果（1）");
        assert!(!events[0].is_final);
        assert!(events[0].utterances.is_empty());
        assert_eq!(events[1].text, "模拟识别结果（1 个音频包）");
        assert!(events[1].is_final);
        assert!(events[1].utterances.is_empty());
        assert_eq!(final_text, "模拟识别结果（1 个音频包）");
    }

    #[tokio::test]
    async fn mock_streaming_provider_rejects_empty_audio() {
        let provider = MockProvider;
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();

        chunk_tx
            .send(AudioChunk {
                bytes: Vec::new(),
                duration_ms: 0,
                is_final: true,
            })
            .unwrap();
        drop(chunk_tx);

        let error = provider
            .transcribe_stream(stream_info(), chunk_rx, event_tx, None)
            .await
            .unwrap_err();

        assert!(matches!(error, ProviderError::EmptyTranscript));
    }

    #[tokio::test]
    async fn mock_streaming_provider_requires_final_chunk() {
        let provider = MockProvider;
        let (chunk_tx, chunk_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();

        chunk_tx
            .send(AudioChunk {
                bytes: vec![1, 2, 3],
                duration_ms: 200,
                is_final: false,
            })
            .unwrap();
        drop(chunk_tx);

        let error = provider
            .transcribe_stream(stream_info(), chunk_rx, event_tx, None)
            .await
            .unwrap_err();

        assert!(matches!(error, ProviderError::MissingFinalTranscript));
    }

    #[tokio::test]
    async fn websocket_audio_frames_use_binary_opcode() {
        let payload = build_audio_request(&[1, 2, 3], 2).unwrap();
        let (mut client, mut server) = tokio::io::duplex(1024);

        write_ws_frame(&mut client, WS_OPCODE_BINARY, &payload)
            .await
            .unwrap();

        let mut header = [0u8; 2];
        server.read_exact(&mut header).await.unwrap();

        assert_eq!(header[0] & 0x0f, WS_OPCODE_BINARY);
    }
}
