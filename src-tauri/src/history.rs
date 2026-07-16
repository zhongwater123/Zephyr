use chrono::Local;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("无法创建历史记录目录: {0}")]
    CreateDir(String),
    #[error("无法打开历史记录数据库: {0}")]
    Open(String),
    #[error("无法写入历史记录: {0}")]
    Database(String),
    #[error("历史记录不存在")]
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct AppContext {
    pub app_name: Option<String>,
    pub app_title: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryItem {
    pub id: String,
    pub text: String,
    pub created_at: String,
    pub app_name: Option<String>,
    pub app_title: Option<String>,
    pub char_count: i64,
}

pub fn history_path() -> Result<PathBuf, HistoryError> {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("gy-typing");
    std::fs::create_dir_all(&dir).map_err(|error| HistoryError::CreateDir(error.to_string()))?;
    Ok(dir.join("history.db"))
}

pub fn insert_transcript(text: &str, app_context: &AppContext) -> Result<HistoryItem, HistoryError> {
    let item = HistoryItem {
        id: Uuid::new_v4().to_string(),
        text: text.to_string(),
        created_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        app_name: app_context.app_name.clone(),
        app_title: app_context.app_title.clone(),
        char_count: text.chars().count() as i64,
    };
    insert_item(&history_path()?, &item)?;
    Ok(item)
}

pub fn list_history(
    query: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<Vec<HistoryItem>, HistoryError> {
    list_history_from_path(&history_path()?, query, limit, offset)
}

pub fn update_history(id: &str, text: &str) -> Result<(), HistoryError> {
    update_history_at_path(&history_path()?, id, text)
}

pub fn delete_history(id: &str) -> Result<(), HistoryError> {
    delete_history_at_path(&history_path()?, id)
}

pub fn clear_history() -> Result<(), HistoryError> {
    clear_history_at_path(&history_path()?)
}

pub fn get_history_text(id: &str) -> Result<String, HistoryError> {
    get_history_text_at_path(&history_path()?, id)
}

fn open_database(path: &Path) -> Result<Connection, HistoryError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| HistoryError::CreateDir(error.to_string()))?;
    }
    let connection = Connection::open(path).map_err(|error| HistoryError::Open(error.to_string()))?;
    initialize_database(&connection)?;
    Ok(connection)
}

fn initialize_database(connection: &Connection) -> Result<(), HistoryError> {
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
        .map_err(|error| HistoryError::Database(error.to_string()))?;
    connection
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_history_items_created_at
             ON history_items(created_at DESC)",
            [],
        )
        .map_err(|error| HistoryError::Database(error.to_string()))?;
    Ok(())
}

fn insert_item(path: &Path, item: &HistoryItem) -> Result<(), HistoryError> {
    let connection = open_database(path)?;
    connection
        .execute(
            "INSERT INTO history_items
             (id, text, created_at, app_name, app_title, char_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &item.id,
                &item.text,
                &item.created_at,
                &item.app_name,
                &item.app_title,
                item.char_count
            ],
        )
        .map_err(|error| HistoryError::Database(error.to_string()))?;
    Ok(())
}

fn list_history_from_path(
    path: &Path,
    query: Option<String>,
    limit: i64,
    offset: i64,
) -> Result<Vec<HistoryItem>, HistoryError> {
    let connection = open_database(path)?;
    let limit = limit.clamp(1, 100);
    let offset = offset.max(0);
    let query = query.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());

    let mut items = Vec::new();
    if let Some(query) = query {
        let pattern = format!("%{query}%");
        let mut statement = connection
            .prepare(
                "SELECT id, text, created_at, app_name, app_title, char_count
                 FROM history_items
                 WHERE text LIKE ?1 OR app_name LIKE ?1 OR app_title LIKE ?1
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?2 OFFSET ?3",
            )
            .map_err(|error| HistoryError::Database(error.to_string()))?;
        let rows = statement
            .query_map(params![pattern, limit, offset], row_to_history_item)
            .map_err(|error| HistoryError::Database(error.to_string()))?;
        for row in rows {
            items.push(row.map_err(|error| HistoryError::Database(error.to_string()))?);
        }
    } else {
        let mut statement = connection
            .prepare(
                "SELECT id, text, created_at, app_name, app_title, char_count
                 FROM history_items
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(|error| HistoryError::Database(error.to_string()))?;
        let rows = statement
            .query_map(params![limit, offset], row_to_history_item)
            .map_err(|error| HistoryError::Database(error.to_string()))?;
        for row in rows {
            items.push(row.map_err(|error| HistoryError::Database(error.to_string()))?);
        }
    }

    Ok(items)
}

fn update_history_at_path(path: &Path, id: &str, text: &str) -> Result<(), HistoryError> {
    let connection = open_database(path)?;
    let changed = connection
        .execute(
            "UPDATE history_items SET text = ?1, char_count = ?2 WHERE id = ?3",
            params![text, text.chars().count() as i64, id],
        )
        .map_err(|error| HistoryError::Database(error.to_string()))?;
    if changed == 0 {
        return Err(HistoryError::NotFound);
    }
    Ok(())
}

fn delete_history_at_path(path: &Path, id: &str) -> Result<(), HistoryError> {
    let connection = open_database(path)?;
    connection
        .execute("DELETE FROM history_items WHERE id = ?1", params![id])
        .map_err(|error| HistoryError::Database(error.to_string()))?;
    Ok(())
}

fn clear_history_at_path(path: &Path) -> Result<(), HistoryError> {
    let connection = open_database(path)?;
    connection
        .execute("DELETE FROM history_items", [])
        .map_err(|error| HistoryError::Database(error.to_string()))?;
    Ok(())
}

fn get_history_text_at_path(path: &Path, id: &str) -> Result<String, HistoryError> {
    let connection = open_database(path)?;
    connection
        .query_row(
            "SELECT text FROM history_items WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => HistoryError::NotFound,
            other => HistoryError::Database(other.to_string()),
        })
}

fn row_to_history_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryItem> {
    Ok(HistoryItem {
        id: row.get(0)?,
        text: row.get(1)?,
        created_at: row.get(2)?,
        app_name: row.get(3)?,
        app_title: row.get(4)?,
        char_count: row.get(5)?,
    })
}

#[cfg(target_os = "windows")]
pub fn capture_foreground_app() -> AppContext {
    use std::path::Path;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };
    use windows::core::PWSTR;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return AppContext::default();
        }

        let mut title_buffer = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, &mut title_buffer);
        let app_title = if title_len > 0 {
            Some(String::from_utf16_lossy(&title_buffer[..title_len as usize]))
        } else {
            None
        };

        let mut process_id = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id as *mut u32));
        let app_name = if process_id == 0 {
            None
        } else {
            foreground_process_name(process_id)
        };

        return AppContext {
            app_name,
            app_title,
        };
    }

    unsafe fn foreground_process_name(process_id: u32) -> Option<String> {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        if size == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..size as usize]);
        Path::new(&path)
            .file_name()
            .and_then(|value| value.to_str())
            .map(|value| value.to_string())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn capture_foreground_app() -> AppContext {
    AppContext::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.db");
        (dir, path)
    }

    #[test]
    fn database_initialization_creates_history_table() {
        let (_dir, path) = test_path();

        let connection = open_database(&path).unwrap();
        let exists: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='history_items'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(exists, 1);
    }

    #[test]
    fn history_crud_and_search_work() {
        let (_dir, path) = test_path();
        let first = HistoryItem {
            id: "first".to_string(),
            text: "今天写一段语音输入".to_string(),
            created_at: "2026-07-16 09:30:01".to_string(),
            app_name: Some("notepad.exe".to_string()),
            app_title: Some("记事本".to_string()),
            char_count: 9,
        };
        let second = HistoryItem {
            id: "second".to_string(),
            text: "浏览器里的测试".to_string(),
            created_at: "2026-07-16 09:31:01".to_string(),
            app_name: Some("chrome.exe".to_string()),
            app_title: Some("网页".to_string()),
            char_count: 7,
        };

        insert_item(&path, &first).unwrap();
        insert_item(&path, &second).unwrap();

        let all = list_history_from_path(&path, None, 20, 0).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "second");

        let searched = list_history_from_path(&path, Some("记事本".to_string()), 20, 0).unwrap();
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0].id, "first");

        update_history_at_path(&path, "first", "改过的文本").unwrap();
        assert_eq!(get_history_text_at_path(&path, "first").unwrap(), "改过的文本");

        delete_history_at_path(&path, "second").unwrap();
        assert_eq!(list_history_from_path(&path, None, 20, 0).unwrap().len(), 1);

        clear_history_at_path(&path).unwrap();
        assert!(list_history_from_path(&path, None, 20, 0).unwrap().is_empty());
    }
}
