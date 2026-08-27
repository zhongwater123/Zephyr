use crate::config::{self, AppConfig};
use crate::history::{self, AppContext};
use crate::provider::AsrSessionHints;
use chrono::Local;
use reqwest::StatusCode;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use url::Url;

const HOTWORD_BATCH_SIZE: i64 = 20;
const MAX_HOTWORDS: usize = 30;
const MAX_CONTEXT_CHARS: usize = 120;
const AGENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const AGENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
const DATABASE_BUSY_TIMEOUT: Duration = Duration::from_secs(3);

static ORGANIZE_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Error)]
pub enum HotwordError {
    #[error("无法打开热词数据库: {0}")]
    Open(String),
    #[error("无法读写热词数据库: {0}")]
    Database(String),
    #[error("热词整理正在进行")]
    AlreadyRunning,
    #[error("热词自动整理未开启")]
    AgentDisabled,
    #[error("DeepSeek API Key 尚未配置")]
    MissingApiKey,
    #[error("DeepSeek 请求失败: {0}")]
    Request(String),
    #[error("DeepSeek 响应无法解析: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct HotwordState {
    pub hotwords_enabled: bool,
    pub hotword_agent_enabled: bool,
    pub hotword_agent_base_url: String,
    pub hotword_agent_model: String,
    pub has_hotword_agent_api_key: bool,
    pub manual_hotwords: Vec<String>,
    pub agent_hotwords: Vec<String>,
    pub profile_context: String,
    pub app_contexts: Vec<AppHotwordContext>,
    pub pending_count: i64,
    pub updated_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppHotwordContext {
    pub app_name: String,
    pub context: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HotwordSettingsInput {
    pub hotwords_enabled: bool,
    pub hotword_agent_enabled: bool,
    pub hotword_agent_base_url: String,
    pub hotword_agent_model: String,
}

#[derive(Debug, Clone)]
struct StoredHotwordState {
    manual_hotwords: Vec<String>,
    agent_hotwords: Vec<String>,
    profile_context: String,
    app_contexts: Vec<AppHotwordContext>,
    last_processed_rowid: i64,
    updated_at: Option<String>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PendingHistoryItem {
    rowid: i64,
    text: String,
    created_at: String,
    app_name: Option<String>,
    app_title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentOutput {
    #[serde(default)]
    agent_hotwords: Vec<String>,
    #[serde(default)]
    profile_context: String,
    #[serde(default)]
    app_contexts: Vec<AppHotwordContext>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    content: String,
}

pub fn get_state(config: &AppConfig, has_api_key: bool) -> Result<HotwordState, HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    state_from_connection(config, &connection, has_api_key)
}

pub fn save_manual_hotwords(words: Vec<String>) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    update_manual_hotwords(&connection, sanitize_words(words, true))
}

pub fn add_hotword(word: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    add_hotword_to_state(&mut state, word);
    save_stored_state(&connection, &state)
}

pub fn update_hotword(old_word: &str, new_word: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    update_hotword_in_state(&mut state, old_word, new_word);
    save_stored_state(&connection, &state)
}

pub fn delete_hotword(word: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    delete_hotword_from_state(&mut state, word);
    save_stored_state(&connection, &state)
}

pub fn delete_agent_hotword(word: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    state.agent_hotwords.retain(|item| item != word);
    save_stored_state(&connection, &state)
}

pub fn promote_agent_hotword(word: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    if !word.trim().is_empty() && !state.manual_hotwords.iter().any(|item| item == word) {
        state.manual_hotwords.push(word.trim().to_string());
    }
    state.agent_hotwords.retain(|item| item != word);
    state.manual_hotwords = sanitize_words(state.manual_hotwords, true);
    save_stored_state(&connection, &state)
}

pub fn update_profile_context(text: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    state.profile_context = truncate_chars(text.trim(), MAX_CONTEXT_CHARS);
    save_stored_state(&connection, &state)
}

pub fn update_app_context(app_name: &str, context: &str) -> Result<(), HotwordError> {
    let app_name = app_name.trim();
    if app_name.is_empty() {
        return Ok(());
    }
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    let context = truncate_chars(context.trim(), MAX_CONTEXT_CHARS);
    if let Some(item) = state
        .app_contexts
        .iter_mut()
        .find(|item| item.app_name.eq_ignore_ascii_case(app_name))
    {
        item.context = context;
    } else {
        state.app_contexts.push(AppHotwordContext {
            app_name: app_name.to_string(),
            context,
        });
    }
    state.app_contexts = normalize_app_contexts(state.app_contexts);
    save_stored_state(&connection, &state)
}

pub fn delete_app_context(app_name: &str) -> Result<(), HotwordError> {
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let mut state = load_stored_state(&connection)?;
    state
        .app_contexts
        .retain(|item| !item.app_name.eq_ignore_ascii_case(app_name));
    save_stored_state(&connection, &state)
}

pub fn compose_asr_hints(
    config: &AppConfig,
    app_context: &AppContext,
) -> Result<Option<AsrSessionHints>, HotwordError> {
    if !config.hotwords_enabled {
        return Ok(None);
    }

    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;
    let state = load_stored_state(&connection)?;
    let mut hotwords = state.manual_hotwords;
    hotwords.extend(state.agent_hotwords);
    hotwords = sanitize_words(hotwords, true);
    hotwords.truncate(MAX_HOTWORDS);

    let app_context_text = app_context
        .app_name
        .as_deref()
        .and_then(|app_name| {
            state
                .app_contexts
                .iter()
                .find(|item| item.app_name.eq_ignore_ascii_case(app_name))
                .map(|item| item.context.clone())
        })
        .filter(|text| !text.trim().is_empty());
    let profile_context =
        (!state.profile_context.trim().is_empty()).then(|| state.profile_context.clone());

    if hotwords.is_empty() && app_context_text.is_none() && profile_context.is_none() {
        return Ok(None);
    }

    Ok(Some(AsrSessionHints {
        hotwords,
        profile_context,
        app_context: app_context_text,
    }))
}

pub fn should_auto_organize(config: &AppConfig) -> bool {
    if !config.hotword_agent_enabled {
        return false;
    }
    get_state(config, false)
        .map(|state| state.pending_count >= HOTWORD_BATCH_SIZE)
        .unwrap_or(false)
}

pub async fn test_agent_connection(
    config: AppConfig,
    api_key: String,
) -> Result<String, HotwordError> {
    if !config.is_endpoint_trusted(
        &config.hotword_agent_base_url,
        config::EndpointPurpose::HotwordAgent,
    ) {
        return Err(HotwordError::Request(
            "热词 Agent endpoint 尚未通过 Windows 原生授权".to_string(),
        ));
    }
    let url = chat_completions_url(&config.hotword_agent_base_url)?;
    let payload = json!({
        "model": config.hotword_agent_model,
        "messages": [
            {
                "role": "system",
                "content": "你是 Zephyr 的热词 Agent 连通性测试。只需简短回复 OK。"
            },
            {
                "role": "user",
                "content": "请回复 OK，用于验证接口连通性。"
            }
        ],
        "stream": false
    });

    let response = agent_http_client()?
        .post(url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|error| HotwordError::Request(error.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| HotwordError::Request(error.to_string()))?;
    if status != StatusCode::OK {
        return Err(HotwordError::Request(format!(
            "HTTP {}: {}",
            status.as_u16(),
            truncate_chars(&body, 240)
        )));
    }

    let response: ChatCompletionResponse =
        serde_json::from_str(&body).map_err(|error| HotwordError::Parse(error.to_string()))?;
    response
        .choices
        .first()
        .map(|choice| choice.message.content.trim())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| HotwordError::Parse("choices[0].message.content 为空".to_string()))?;
    Ok("DeepSeek 热词 Agent 已就绪。".to_string())
}

pub async fn organize_hotwords(
    config: AppConfig,
    force: bool,
    api_key: String,
) -> Result<HotwordState, HotwordError> {
    if !force && !config.hotword_agent_enabled {
        return Err(HotwordError::AgentDisabled);
    }
    if ORGANIZE_RUNNING.swap(true, Ordering::SeqCst) {
        return Err(HotwordError::AlreadyRunning);
    }
    let _guard = OrganizeGuard;

    if !config.is_endpoint_trusted(
        &config.hotword_agent_base_url,
        config::EndpointPurpose::HotwordAgent,
    ) {
        return Err(HotwordError::Request(
            "热词 Agent endpoint 尚未通过 Windows 原生授权".to_string(),
        ));
    }
    let (stored_state, items) = {
        let connection = open_database(
            &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
        )?;
        let state = load_stored_state(&connection)?;
        let pending_count = pending_count(&connection, state.last_processed_rowid)?;
        if !force && pending_count < HOTWORD_BATCH_SIZE {
            return state_from_connection(&config, &connection, true);
        }
        let limit = if force {
            pending_count.max(1)
        } else {
            HOTWORD_BATCH_SIZE
        };
        let items = pending_history_items(&connection, state.last_processed_rowid, limit)?;
        (state, items)
    };

    if items.is_empty() {
        return get_state(&config, true);
    }

    let max_rowid = items.iter().map(|item| item.rowid).max().unwrap_or(0);
    let result = request_agent_output(&config, &api_key, &stored_state, &items).await;
    let connection = open_database(
        &history::history_path().map_err(|error| HotwordError::Open(error.to_string()))?,
    )?;

    match result {
        Ok(agent_output) => {
            let latest_state = load_stored_state(&connection)?;
            let next_state =
                merge_agent_output(&stored_state, latest_state, agent_output, max_rowid);
            save_stored_state(&connection, &next_state)?;
            state_from_connection(&config, &connection, true)
        }
        Err(error) => {
            set_last_error(&connection, &error.to_string())?;
            Err(error)
        }
    }
}

struct OrganizeGuard;

impl Drop for OrganizeGuard {
    fn drop(&mut self) {
        ORGANIZE_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn open_database(path: &Path) -> Result<Connection, HotwordError> {
    let connection =
        Connection::open(path).map_err(|error| HotwordError::Open(error.to_string()))?;
    connection
        .busy_timeout(DATABASE_BUSY_TIMEOUT)
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    connection
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    initialize_database(&connection)?;
    Ok(connection)
}

fn initialize_database(connection: &Connection) -> Result<(), HotwordError> {
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS history_items (
                id TEXT PRIMARY KEY,
                text TEXT NOT NULL,
                created_at TEXT NOT NULL,
                app_name TEXT,
                app_title TEXT,
                char_count INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS hotword_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                manual_hotwords_json TEXT NOT NULL DEFAULT '[]',
                agent_hotwords_json TEXT NOT NULL DEFAULT '[]',
                profile_context TEXT NOT NULL DEFAULT '',
                app_contexts_json TEXT NOT NULL DEFAULT '[]',
                last_processed_rowid INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT,
                last_error TEXT
            )",
            [],
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    connection
        .execute(
            "INSERT OR IGNORE INTO hotword_state
             (id, manual_hotwords_json, agent_hotwords_json, profile_context, app_contexts_json, last_processed_rowid)
             VALUES (1, '[]', '[]', '', '[]', 0)",
            [],
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    Ok(())
}

fn state_from_connection(
    config: &AppConfig,
    connection: &Connection,
    has_api_key: bool,
) -> Result<HotwordState, HotwordError> {
    let stored = load_stored_state(connection)?;
    let pending_count = pending_count(connection, stored.last_processed_rowid)?;
    let endpoint_authorized = config.is_endpoint_trusted(
        &config.hotword_agent_base_url,
        config::EndpointPurpose::HotwordAgent,
    );
    Ok(HotwordState {
        hotwords_enabled: config.hotwords_enabled,
        hotword_agent_enabled: config.hotword_agent_enabled,
        hotword_agent_base_url: config.hotword_agent_base_url.clone(),
        hotword_agent_model: config.hotword_agent_model.clone(),
        has_hotword_agent_api_key: endpoint_authorized && has_api_key,
        manual_hotwords: stored.manual_hotwords,
        agent_hotwords: stored.agent_hotwords,
        profile_context: stored.profile_context,
        app_contexts: stored.app_contexts,
        pending_count,
        updated_at: stored.updated_at,
        last_error: stored.last_error,
    })
}

fn load_stored_state(connection: &Connection) -> Result<StoredHotwordState, HotwordError> {
    connection
        .query_row(
            "SELECT manual_hotwords_json, agent_hotwords_json, profile_context,
                    app_contexts_json, last_processed_rowid, updated_at, last_error
             FROM hotword_state WHERE id = 1",
            [],
            |row| {
                let manual_json: String = row.get(0)?;
                let agent_json: String = row.get(1)?;
                let app_contexts_json: String = row.get(3)?;
                Ok(StoredHotwordState {
                    manual_hotwords: parse_json_or_default(&manual_json),
                    agent_hotwords: parse_json_or_default(&agent_json),
                    profile_context: row.get(2)?,
                    app_contexts: parse_json_or_default(&app_contexts_json),
                    last_processed_rowid: row.get(4)?,
                    updated_at: row.get(5)?,
                    last_error: row.get(6)?,
                })
            },
        )
        .map_err(|error| HotwordError::Database(error.to_string()))
}

fn save_stored_state(
    connection: &Connection,
    state: &StoredHotwordState,
) -> Result<(), HotwordError> {
    connection
        .execute(
            "UPDATE hotword_state
             SET manual_hotwords_json = ?1,
                 agent_hotwords_json = ?2,
                 profile_context = ?3,
                 app_contexts_json = ?4,
                 last_processed_rowid = ?5,
                 updated_at = ?6,
                 last_error = ?7
             WHERE id = 1",
            params![
                to_json(&state.manual_hotwords)?,
                to_json(&state.agent_hotwords)?,
                &state.profile_context,
                to_json(&state.app_contexts)?,
                state.last_processed_rowid,
                &state.updated_at,
                &state.last_error
            ],
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    Ok(())
}

fn update_manual_hotwords(connection: &Connection, words: Vec<String>) -> Result<(), HotwordError> {
    connection
        .execute(
            "UPDATE hotword_state
             SET manual_hotwords_json = ?1,
                 updated_at = ?2
             WHERE id = 1",
            params![
                to_json(&words)?,
                Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
            ],
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    Ok(())
}

fn set_last_error(connection: &Connection, message: &str) -> Result<(), HotwordError> {
    connection
        .execute(
            "UPDATE hotword_state SET last_error = ?1 WHERE id = 1",
            params![truncate_chars(message, 240)],
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    Ok(())
}

fn pending_count(connection: &Connection, last_processed_rowid: i64) -> Result<i64, HotwordError> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM history_items WHERE rowid > ?1",
            params![last_processed_rowid],
            |row| row.get(0),
        )
        .map_err(|error| HotwordError::Database(error.to_string()))
}

fn pending_history_items(
    connection: &Connection,
    last_processed_rowid: i64,
    limit: i64,
) -> Result<Vec<PendingHistoryItem>, HotwordError> {
    let mut statement = connection
        .prepare(
            "SELECT rowid, text, created_at, app_name, app_title
             FROM history_items
             WHERE rowid > ?1
             ORDER BY rowid ASC
             LIMIT ?2",
        )
        .map_err(|error| HotwordError::Database(error.to_string()))?;
    let rows = statement
        .query_map(params![last_processed_rowid, limit], |row| {
            Ok(PendingHistoryItem {
                rowid: row.get(0)?,
                text: row.get(1)?,
                created_at: row.get(2)?,
                app_name: row.get(3)?,
                app_title: row.get(4)?,
            })
        })
        .map_err(|error| HotwordError::Database(error.to_string()))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| HotwordError::Database(error.to_string()))
}

async fn request_agent_output(
    config: &AppConfig,
    api_key: &str,
    state: &StoredHotwordState,
    items: &[PendingHistoryItem],
) -> Result<AgentOutput, HotwordError> {
    let url = chat_completions_url(&config.hotword_agent_base_url)?;
    let payload = json!({
        "model": config.hotword_agent_model,
        "messages": [
            {
                "role": "system",
                "content": "你是语音输入热词整理 Agent。只返回严格 JSON，不要 Markdown。根据历史输入提炼专有名词、产品名、人名、技术词、用户长期偏好和应用场景。不要泄露完整原文，不要编造。"
            },
            {
                "role": "user",
                "content": build_agent_prompt(state, items)?
            }
        ],
        "stream": false
    });

    let response = agent_http_client()?
        .post(url)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
        .map_err(|error| HotwordError::Request(error.to_string()))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| HotwordError::Request(error.to_string()))?;
    if status != StatusCode::OK {
        return Err(HotwordError::Request(format!(
            "HTTP {}: {}",
            status.as_u16(),
            truncate_chars(&body, 240)
        )));
    }

    let response: ChatCompletionResponse =
        serde_json::from_str(&body).map_err(|error| HotwordError::Parse(error.to_string()))?;
    let content = response
        .choices
        .first()
        .map(|choice| choice.message.content.trim())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| HotwordError::Parse("choices[0].message.content 为空".to_string()))?;
    parse_agent_output(content)
}

fn build_agent_prompt(
    state: &StoredHotwordState,
    items: &[PendingHistoryItem],
) -> Result<String, HotwordError> {
    let input = json!({
        "requirements": {
            "agent_hotwords": "最多30个，优先专有名词、产品名、人名、技术词；避免通用虚词。",
            "profile_context": "最多120个中文字符，描述稳定偏好、口音、常见主题。",
            "app_contexts": "最多8个应用，每个context最多120个中文字符。",
            "output": "只返回 JSON: {\"agent_hotwords\":[],\"profile_context\":\"\",\"app_contexts\":[{\"app_name\":\"\",\"context\":\"\"}]}"
        },
        "current_knowledge": {
            "manual_hotwords": state.manual_hotwords,
            "agent_hotwords": state.agent_hotwords,
            "profile_context": state.profile_context,
            "app_contexts": state.app_contexts
        },
        "new_history": items
    });
    serde_json::to_string(&input).map_err(|error| HotwordError::Parse(error.to_string()))
}

fn parse_agent_output(content: &str) -> Result<AgentOutput, HotwordError> {
    let content = strip_json_fence(content);
    let value: Value =
        serde_json::from_str(&content).map_err(|error| HotwordError::Parse(error.to_string()))?;
    serde_json::from_value(value).map_err(|error| HotwordError::Parse(error.to_string()))
}

fn strip_json_fence(content: &str) -> String {
    let trimmed = content.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }
    trimmed
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

fn chat_completions_url(base_url: &str) -> Result<String, HotwordError> {
    let base_url = base_url.trim().trim_end_matches('/');
    let endpoint = if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else {
        format!("{base_url}/chat/completions")
    };
    let parsed = Url::parse(&endpoint)
        .map_err(|error| HotwordError::Request(format!("无效的热词 Agent 地址: {error}")))?;
    let is_loopback_http = parsed.scheme() == "http"
        && parsed
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"));
    if parsed.scheme() != "https" && !is_loopback_http {
        return Err(HotwordError::Request(
            "热词 Agent 地址必须使用 HTTPS；仅本机回环地址允许 HTTP。".to_string(),
        ));
    }
    Ok(endpoint)
}

fn agent_http_client() -> Result<reqwest::Client, HotwordError> {
    reqwest::Client::builder()
        .connect_timeout(AGENT_CONNECT_TIMEOUT)
        .timeout(AGENT_REQUEST_TIMEOUT)
        .build()
        .map_err(|error| HotwordError::Request(error.to_string()))
}

fn merge_agent_output(
    requested_state: &StoredHotwordState,
    mut latest_state: StoredHotwordState,
    agent_output: AgentOutput,
    max_rowid: i64,
) -> StoredHotwordState {
    latest_state.agent_hotwords = sanitize_words(agent_output.agent_hotwords, false);
    latest_state.agent_hotwords.truncate(MAX_HOTWORDS);

    if latest_state.profile_context == requested_state.profile_context {
        latest_state.profile_context =
            truncate_chars(agent_output.profile_context.trim(), MAX_CONTEXT_CHARS);
    }
    if latest_state.app_contexts == requested_state.app_contexts {
        latest_state.app_contexts = normalize_app_contexts(agent_output.app_contexts);
    }

    latest_state.last_processed_rowid = max_rowid;
    latest_state.updated_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
    latest_state.last_error = None;
    latest_state
}

fn normalize_app_contexts(contexts: Vec<AppHotwordContext>) -> Vec<AppHotwordContext> {
    let mut normalized: Vec<AppHotwordContext> = Vec::new();
    for item in contexts {
        let app_name = item.app_name.trim();
        let context = item.context.trim();
        if app_name.is_empty() || context.is_empty() {
            continue;
        }
        if let Some(existing) = normalized
            .iter_mut()
            .find(|existing| existing.app_name.eq_ignore_ascii_case(app_name))
        {
            existing.context = truncate_chars(context, MAX_CONTEXT_CHARS);
        } else if normalized.len() < 8 {
            normalized.push(AppHotwordContext {
                app_name: app_name.to_string(),
                context: truncate_chars(context, MAX_CONTEXT_CHARS),
            });
        }
    }
    normalized
}

fn add_hotword_to_state(state: &mut StoredHotwordState, word: &str) {
    let word = truncate_chars(word.trim(), 40);
    if word.is_empty() {
        return;
    }
    delete_hotword_from_state(state, &word);
    state.manual_hotwords.push(word);
    state.manual_hotwords = sanitize_words(state.manual_hotwords.clone(), true);
    state.updated_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
}

fn update_hotword_in_state(state: &mut StoredHotwordState, old_word: &str, new_word: &str) {
    delete_hotword_from_state(state, old_word);
    add_hotword_to_state(state, new_word);
}

fn delete_hotword_from_state(state: &mut StoredHotwordState, word: &str) {
    let word = word.trim();
    if word.is_empty() {
        return;
    }
    state.manual_hotwords.retain(|item| item != word);
    state.agent_hotwords.retain(|item| item != word);
    state.updated_at = Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
}

fn sanitize_words(words: Vec<String>, allow_short: bool) -> Vec<String> {
    let mut sanitized = Vec::new();
    for word in words {
        let word = truncate_chars(word.trim(), 40);
        if word.is_empty() {
            continue;
        }
        if !allow_short && word.chars().count() < 2 {
            continue;
        }
        if !sanitized.iter().any(|item| item == &word) {
            sanitized.push(word);
        }
        if sanitized.len() >= MAX_HOTWORDS {
            break;
        }
    }
    sanitized
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn parse_json_or_default<T>(json: &str) -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    serde_json::from_str(json).unwrap_or_default()
}

fn to_json<T>(value: &T) -> Result<String, HotwordError>
where
    T: Serialize,
{
    serde_json::to_string(value).map_err(|error| HotwordError::Database(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AppConfig {
        AppConfig {
            hotwords_enabled: true,
            hotword_agent_enabled: true,
            hotword_agent_base_url: "https://api.deepseek.com".to_string(),
            hotword_agent_model: "deepseek-v4-flash".to_string(),
            ..AppConfig::default()
        }
    }

    #[test]
    fn manual_hotwords_are_sanitized_and_deduplicated() {
        let words = sanitize_words(
            vec![
                " Zephyr ".to_string(),
                "Zephyr".to_string(),
                "".to_string(),
                "火山引擎".to_string(),
            ],
            true,
        );

        assert_eq!(words, vec!["Zephyr", "火山引擎"]);
    }

    #[test]
    fn unified_hotword_edit_promotes_agent_word_to_manual_word() {
        let mut state = StoredHotwordState {
            manual_hotwords: vec!["Zephyr".to_string()],
            agent_hotwords: vec!["火山".to_string()],
            profile_context: String::new(),
            app_contexts: Vec::new(),
            last_processed_rowid: 0,
            updated_at: None,
            last_error: None,
        };

        update_hotword_in_state(&mut state, "火山", "火山引擎");

        assert_eq!(state.manual_hotwords, vec!["Zephyr", "火山引擎"]);
        assert!(state.agent_hotwords.is_empty());
    }

    #[test]
    fn unified_hotword_delete_removes_from_manual_and_agent_sources() {
        let mut state = StoredHotwordState {
            manual_hotwords: vec!["Zephyr".to_string(), "火山引擎".to_string()],
            agent_hotwords: vec!["火山引擎".to_string(), "DeepSeek".to_string()],
            profile_context: String::new(),
            app_contexts: Vec::new(),
            last_processed_rowid: 0,
            updated_at: None,
            last_error: None,
        };

        delete_hotword_from_state(&mut state, "火山引擎");

        assert_eq!(state.manual_hotwords, vec!["Zephyr"]);
        assert_eq!(state.agent_hotwords, vec!["DeepSeek"]);
    }

    #[test]
    fn pending_count_uses_history_rowid_after_last_processed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let connection = open_database(&path).unwrap();
        connection
            .execute(
                "INSERT INTO history_items (id, text, created_at, char_count)
                 VALUES ('1', '第一条', '2026-07-16 10:00:00', 3),
                        ('2', '第二条', '2026-07-16 10:00:01', 3)",
                [],
            )
            .unwrap();

        assert_eq!(pending_count(&connection, 0).unwrap(), 2);
        assert_eq!(pending_count(&connection, 1).unwrap(), 1);
    }

    #[test]
    fn parse_agent_output_accepts_plain_json_and_fenced_json() {
        let plain = parse_agent_output(
            r#"{"agent_hotwords":["Zephyr"],"profile_context":"语音输入","app_contexts":[]}"#,
        )
        .unwrap();
        let fenced = parse_agent_output(
            "```json\n{\"agent_hotwords\":[\"火山引擎\"],\"profile_context\":\"\",\"app_contexts\":[]}\n```",
        )
        .unwrap();

        assert_eq!(plain.agent_hotwords, vec!["Zephyr"]);
        assert_eq!(fenced.agent_hotwords, vec!["火山引擎"]);
    }

    #[test]
    fn agent_endpoint_requires_https_except_for_loopback_hosts() {
        assert!(chat_completions_url("https://api.deepseek.com").is_ok());
        assert!(chat_completions_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(chat_completions_url("http://example.com/v1").is_err());
    }

    #[test]
    fn agent_result_preserves_user_edits_made_while_request_was_running() {
        let requested = StoredHotwordState {
            manual_hotwords: vec!["Zephyr".to_string()],
            agent_hotwords: Vec::new(),
            profile_context: "旧的个人上下文".to_string(),
            app_contexts: vec![AppHotwordContext {
                app_name: "code.exe".to_string(),
                context: "旧的应用上下文".to_string(),
            }],
            last_processed_rowid: 10,
            updated_at: None,
            last_error: None,
        };
        let mut latest = requested.clone();
        latest.manual_hotwords.push("用户新词".to_string());
        latest.profile_context = "用户刚刚修改的上下文".to_string();

        let merged = merge_agent_output(
            &requested,
            latest,
            AgentOutput {
                agent_hotwords: vec!["Agent 新词".to_string()],
                profile_context: "Agent 生成的上下文".to_string(),
                app_contexts: vec![AppHotwordContext {
                    app_name: "code.exe".to_string(),
                    context: "Agent 生成的应用上下文".to_string(),
                }],
            },
            20,
        );

        assert_eq!(merged.manual_hotwords, vec!["Zephyr", "用户新词"]);
        assert_eq!(merged.profile_context, "用户刚刚修改的上下文");
        assert_eq!(merged.app_contexts[0].context, "Agent 生成的应用上下文");
        assert_eq!(merged.agent_hotwords, vec!["Agent 新词"]);
        assert_eq!(merged.last_processed_rowid, 20);
    }

    #[test]
    fn state_reports_pending_count() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        let connection = open_database(&path).unwrap();
        connection
            .execute(
                "INSERT INTO history_items (id, text, created_at, char_count)
                 VALUES ('1', '第一条', '2026-07-16 10:00:00', 3)",
                [],
            )
            .unwrap();

        let state = state_from_connection(&test_config(), &connection, false).unwrap();

        assert!(state.hotwords_enabled);
        assert_eq!(state.pending_count, 1);
    }
}
