use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct IncidentPaths {
    pub root: PathBuf,
    pub database: PathBuf,
    pub artifacts: PathBuf,
    pub exports: PathBuf,
    pub emergency: PathBuf,
}

impl IncidentPaths {
    pub fn discover() -> Result<Self, String> {
        let root = dirs::data_local_dir()
            .ok_or_else(|| "cannot resolve LocalAppData".to_string())?
            .join("gy-typing")
            .join("incidents");
        let paths = Self {
            database: root.join("incident.db"),
            artifacts: root.join("artifacts"),
            exports: root.join("exports"),
            emergency: root.join("panic-emergency.jsonl"),
            root,
        };
        fs::create_dir_all(&paths.artifacts).map_err(|error| error.to_string())?;
        fs::create_dir_all(&paths.exports).map_err(|error| error.to_string())?;
        Ok(paths)
    }
}

pub fn artifact_path(paths: &IncidentPaths, relative_name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative_name);
    let mut components = relative.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(paths.artifacts.join(relative)),
        _ => Err("artifact_path_invalid".to_string()),
    }
}

pub fn open_database(paths: &IncidentPaths) -> rusqlite::Result<Connection> {
    let connection = Connection::open_with_flags(
        &paths.database,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(std::time::Duration::from_secs(1))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    connection.execute_batch(SCHEMA)?;
    ensure_capture_authorization_columns(&connection)?;
    Ok(connection)
}

fn ensure_capture_authorization_columns(connection: &Connection) -> rusqlite::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(voice_attempts)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for column in ["audio_capture_authorized", "text_capture_authorized"] {
        if !columns.iter().any(|existing| existing == column) {
            connection.execute(
                &format!(
                    "ALTER TABLE voice_attempts ADD COLUMN {column} INTEGER NOT NULL DEFAULT 0"
                ),
                [],
            )?;
        }
    }
    Ok(())
}
const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS voice_attempts (
  attempt_id TEXT PRIMARY KEY,
  runtime_session_id INTEGER NOT NULL,
  started_at_utc_ms INTEGER NOT NULL,
  ended_at_utc_ms INTEGER,
  terminal_outcome TEXT,
  content_enabled INTEGER NOT NULL,
  audio_capture_authorized INTEGER NOT NULL DEFAULT 0,
  text_capture_authorized INTEGER NOT NULL DEFAULT 0,
  target_app_name TEXT,
  target_window_title TEXT,
  pinned INTEGER NOT NULL DEFAULT 0,
  expires_at_utc_ms INTEGER,
  retention_days INTEGER NOT NULL,
  storage_limit_mb INTEGER NOT NULL,
  success_rollup_days INTEGER NOT NULL,
  encryption_version INTEGER NOT NULL DEFAULT 0,
  created_at_utc_ms INTEGER NOT NULL,
  updated_at_utc_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_voice_attempts_started
  ON voice_attempts(started_at_utc_ms DESC);
CREATE TABLE IF NOT EXISTS attempt_stages (
  attempt_id TEXT NOT NULL,
  stage_name TEXT NOT NULL,
  stage_outcome TEXT NOT NULL,
  reason_code TEXT,
  first_monotonic_us INTEGER,
  last_monotonic_us INTEGER,
  PRIMARY KEY (attempt_id, stage_name)
);
CREATE TABLE IF NOT EXISTS incident_findings (
  finding_id INTEGER PRIMARY KEY AUTOINCREMENT,
  attempt_id TEXT NOT NULL,
  stage_name TEXT NOT NULL,
  reason_code TEXT NOT NULL,
  severity TEXT NOT NULL,
  recoverability TEXT NOT NULL,
  localized_message TEXT NOT NULL,
  resolved_at_utc_ms INTEGER,
  created_at_utc_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_findings_attempt
  ON incident_findings(attempt_id, finding_id);
CREATE TABLE IF NOT EXISTS incident_events (
  event_id INTEGER PRIMARY KEY AUTOINCREMENT,
  attempt_id TEXT NOT NULL,
  sequence_number INTEGER NOT NULL,
  event_name TEXT NOT NULL,
  event_schema_version INTEGER NOT NULL DEFAULT 1,
  monotonic_us INTEGER,
  attributes_json TEXT NOT NULL,
  created_at_utc_ms INTEGER NOT NULL,
  UNIQUE(attempt_id, sequence_number)
);
CREATE TABLE IF NOT EXISTS recovery_artifacts (
  artifact_id INTEGER PRIMARY KEY AUTOINCREMENT,
  attempt_id TEXT NOT NULL,
  artifact_kind TEXT NOT NULL,
  relative_path TEXT NOT NULL,
  artifact_completeness TEXT NOT NULL,
  byte_size INTEGER NOT NULL DEFAULT 0,
  sha256_hex TEXT,
  expires_at_utc_ms INTEGER,
  sealed_at_utc_ms INTEGER,
  encryption_version INTEGER NOT NULL DEFAULT 0,
  UNIQUE(attempt_id, artifact_kind)
);
CREATE TABLE IF NOT EXISTS diagnostic_metrics (
  metric_id INTEGER PRIMARY KEY AUTOINCREMENT,
  attempt_id TEXT NOT NULL,
  metric_name TEXT NOT NULL,
  metric_value REAL NOT NULL,
  metric_unit TEXT NOT NULL,
  created_at_utc_ms INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS daily_metric_rollups (
  metric_date TEXT NOT NULL,
  metric_name TEXT NOT NULL,
  dimension_key TEXT NOT NULL,
  metric_count INTEGER NOT NULL DEFAULT 0,
  metric_sum REAL NOT NULL DEFAULT 0,
  expires_at_utc_ms INTEGER NOT NULL,
  PRIMARY KEY(metric_date, metric_name, dimension_key)
);
"#;

pub fn utc_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn open_database_adds_capture_authorization_columns_to_an_existing_incident_db() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("incidents");
        let artifacts = root.join("artifacts");
        let exports = root.join("exports");
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::create_dir_all(&exports).unwrap();
        let paths = IncidentPaths {
            database: root.join("incident.db"),
            emergency: root.join("panic-emergency.jsonl"),
            root,
            artifacts,
            exports,
        };
        let legacy = Connection::open(&paths.database).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE voice_attempts (
                  attempt_id TEXT PRIMARY KEY,
                  runtime_session_id INTEGER NOT NULL,
                  started_at_utc_ms INTEGER NOT NULL,
                  ended_at_utc_ms INTEGER,
                  terminal_outcome TEXT,
                  content_enabled INTEGER NOT NULL,
                  target_app_name TEXT,
                  target_window_title TEXT,
                  pinned INTEGER NOT NULL DEFAULT 0,
                  expires_at_utc_ms INTEGER,
                  retention_days INTEGER NOT NULL,
                  storage_limit_mb INTEGER NOT NULL,
                  success_rollup_days INTEGER NOT NULL,
                  encryption_version INTEGER NOT NULL DEFAULT 0,
                  created_at_utc_ms INTEGER NOT NULL,
                  updated_at_utc_ms INTEGER NOT NULL
                );",
            )
            .unwrap();
        drop(legacy);

        let migrated = open_database(&paths).unwrap();
        let authorizations: (i64, i64) = migrated
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM pragma_table_info('voice_attempts') WHERE name='audio_capture_authorized'),
                   (SELECT COUNT(*) FROM pragma_table_info('voice_attempts') WHERE name='text_capture_authorized')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(authorizations, (1, 1));

        migrated
            .execute(
                "INSERT INTO voice_attempts(
                   attempt_id,runtime_session_id,started_at_utc_ms,content_enabled,
                   retention_days,storage_limit_mb,success_rollup_days,
                   created_at_utc_ms,updated_at_utc_ms
                 ) VALUES('legacy-defaults',1,1,1,7,512,30,1,1)",
                [],
            )
            .unwrap();
        let defaults: (i64, i64) = migrated
            .query_row(
                "SELECT audio_capture_authorized,text_capture_authorized
                 FROM voice_attempts WHERE attempt_id='legacy-defaults'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(defaults, (0, 0));
    }
}
