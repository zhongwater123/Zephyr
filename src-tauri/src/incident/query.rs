use super::model::{IncidentItem, ReportOptions};
use super::redact_sensitive;
use super::schema::{artifact_path, IncidentPaths};
use crc32fast::Hasher as Crc32;
use rusqlite::{params, Connection};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;

pub fn list(
    connection: &Connection,
    paths: &IncidentPaths,
    limit: u32,
    offset: u32,
) -> Result<Vec<IncidentItem>, String> {
    let mut statement = connection.prepare(
        "SELECT a.attempt_id,a.started_at_utc_ms,COALESCE(a.terminal_outcome,'interrupted'),
          COALESCE(f.stage_name,'runtime'),COALESCE(f.reason_code,'interrupted'),
          COALESCE(f.localized_message,'上次运行未正常结束'),COALESCE(f.recoverability,'none'),
          a.pinned,a.expires_at_utc_ms,a.target_app_name,
          (SELECT relative_path FROM recovery_artifacts WHERE attempt_id=a.attempt_id AND artifact_kind='partial_text'),
          (SELECT sha256_hex FROM recovery_artifacts WHERE attempt_id=a.attempt_id AND artifact_kind='partial_text'),
          (SELECT relative_path FROM recovery_artifacts WHERE attempt_id=a.attempt_id AND artifact_kind='final_text'),
          (SELECT sha256_hex FROM recovery_artifacts WHERE attempt_id=a.attempt_id AND artifact_kind='final_text'),
          (SELECT relative_path FROM recovery_artifacts WHERE attempt_id=a.attempt_id AND artifact_kind='audio_pcm'),
          (SELECT artifact_completeness FROM recovery_artifacts WHERE attempt_id=a.attempt_id AND artifact_kind='audio_pcm')
         FROM voice_attempts a
         LEFT JOIN incident_findings f ON f.finding_id=(
           SELECT MAX(f2.finding_id) FROM incident_findings f2 WHERE f2.attempt_id=a.attempt_id
         )
         WHERE a.terminal_outcome IS NOT NULL AND (a.terminal_outcome <> 'succeeded' OR f.finding_id IS NOT NULL)
         ORDER BY a.pinned DESC,a.started_at_utc_ms DESC LIMIT ?1 OFFSET ?2"
    ).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![limit.min(200), offset], |row| {
            let partial_path: Option<String> = row.get(10)?;
            let partial_sha256: Option<String> = row.get(11)?;
            let final_path: Option<String> = row.get(12)?;
            let final_sha256: Option<String> = row.get(13)?;
            let audio_path: Option<String> = row.get(14)?;
            Ok(IncidentItem {
                id: row.get(0)?,
                created_at_utc_ms: row.get(1)?,
                terminal_outcome: row.get(2)?,
                failure_stage: row.get(3)?,
                failure_code: row.get(4)?,
                failure_message: row.get(5)?,
                recoverability: row.get(6)?,
                partial_text: verified_text(paths, partial_path, partial_sha256),
                final_text: verified_text(paths, final_path, final_sha256),
                audio_available: audio_path
                    .as_ref()
                    .and_then(|name| artifact_path(paths, name).ok())
                    .map(|path| path.is_file())
                    .unwrap_or(false),
                audio_completeness: row.get(15)?,
                pinned: row.get::<_, i64>(7)? != 0,
                expires_at_utc_ms: row.get(8)?,
                target_app: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn get(
    connection: &Connection,
    paths: &IncidentPaths,
    id: &str,
) -> Result<IncidentItem, String> {
    let mut offset = 0u32;
    loop {
        let page = list(connection, paths, 200, offset)?;
        if let Some(item) = page.iter().find(|item| item.id == id).cloned() {
            return Ok(item);
        }
        if page.len() < 200 {
            return Err("incident_not_found".to_string());
        }
        let next = offset.saturating_add(200);
        if next == offset {
            return Err("incident_not_found".to_string());
        }
        offset = next;
    }
}

fn read_verified_artifact(
    paths: &IncidentPaths,
    name: &str,
    expected_sha256: Option<&str>,
) -> Result<Vec<u8>, String> {
    let expected_sha256 =
        expected_sha256.ok_or_else(|| "artifact_integrity_unverified".to_string())?;
    let path = artifact_path(paths, name)?;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err("artifact_integrity_mismatch".to_string());
    }
    Ok(bytes)
}

fn verified_text(
    paths: &IncidentPaths,
    name: Option<String>,
    sha256_hex: Option<String>,
) -> Option<String> {
    let name = name?;
    let bytes = read_verified_artifact(paths, &name, sha256_hex.as_deref()).ok()?;
    String::from_utf8(bytes).ok()
}
pub fn set_pinned(connection: &Connection, id: &str, pinned: bool) -> Result<(), String> {
    connection
        .execute(
            "UPDATE voice_attempts SET pinned=?2 WHERE attempt_id=?1",
            params![id, pinned],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn remove_file_if_present(path: &std::path::Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn delete(connection: &Connection, paths: &IncidentPaths, id: &str) -> Result<(), String> {
    let mut statement = connection
        .prepare("SELECT relative_path FROM recovery_artifacts WHERE attempt_id=?1")
        .map_err(|error| error.to_string())?;
    let names = statement
        .query_map([id], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    drop(statement);
    for name in names {
        let path = artifact_path(paths, &name)?;
        remove_file_if_present(&path)?;
    }
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(|error| error.to_string())?;
    let result = (|| {
        connection.execute("DELETE FROM diagnostic_metrics WHERE attempt_id=?1", [id])?;
        connection.execute("DELETE FROM incident_events WHERE attempt_id=?1", [id])?;
        connection.execute("DELETE FROM incident_findings WHERE attempt_id=?1", [id])?;
        connection.execute("DELETE FROM attempt_stages WHERE attempt_id=?1", [id])?;
        connection.execute("DELETE FROM recovery_artifacts WHERE attempt_id=?1", [id])?;
        connection.execute("DELETE FROM voice_attempts WHERE attempt_id=?1", [id])?;
        Ok::<_, rusqlite::Error>(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error.to_string())
        }
    }
}

pub fn audio_wav(
    connection: &Connection,
    paths: &IncidentPaths,
    id: &str,
) -> Result<Vec<u8>, String> {
    let (name, sha256_hex): (String, Option<String>) = connection.query_row(
        "SELECT relative_path,sha256_hex FROM recovery_artifacts WHERE attempt_id=?1 AND artifact_kind='audio_pcm'",
        [id], |row| Ok((row.get(0)?, row.get(1)?)),
    ).map_err(|_| "audio_not_available".to_string())?;
    let pcm = read_verified_artifact(paths, &name, sha256_hex.as_deref())?;
    let data_len = u32::try_from(pcm.len()).map_err(|_| "audio_too_large".to_string())?;
    let mut wav = Vec::with_capacity(pcm.len() + 44);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36u32.saturating_add(data_len)).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&16_000u32.to_le_bytes());
    wav.extend_from_slice(&32_000u32.to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    Ok(wav)
}

pub fn report(
    connection: &Connection,
    paths: &IncidentPaths,
    id: &str,
    options: &ReportOptions,
    log_dir: Option<&std::path::Path>,
) -> Result<Vec<u8>, String> {
    let incident = get(connection, paths, id)?;
    let log_excerpt = if options.include_log_excerpt {
        log_dir.and_then(read_redacted_log_excerpt)
    } else {
        None
    };
    let manifest = json!({
        "format": "gy-typing-incident", "version": 1, "attemptId": id,
        "containsText": options.include_text, "containsAudio": options.include_audio,
        "containsLogExcerpt": log_excerpt.is_some()
    });
    let mut incident_payload = json!({
        "id": incident.id, "createdAtUtcMs": incident.created_at_utc_ms,
        "terminalOutcome": incident.terminal_outcome, "failureStage": incident.failure_stage,
        "failureCode": incident.failure_code, "failureMessage": incident.failure_message,
        "recoverability": incident.recoverability, "audioCompleteness": incident.audio_completeness
    });
    if options.include_text {
        incident_payload["partialText"] = json!(incident.partial_text);
        incident_payload["finalText"] = json!(incident.final_text);
    }
    let incident_json =
        serde_json::to_vec_pretty(&incident_payload).map_err(|error| error.to_string())?;
    let events = query_jsonl(connection, "SELECT event_name,attributes_json,created_at_utc_ms FROM incident_events WHERE attempt_id=?1 ORDER BY sequence_number", id)?;
    let metrics = query_jsonl(connection, "SELECT metric_name,json_object('value',metric_value,'unit',metric_unit),created_at_utc_ms FROM diagnostic_metrics WHERE attempt_id=?1 ORDER BY metric_id", id)?;
    let mut entries = vec![
        (
            "manifest.json".to_string(),
            serde_json::to_vec_pretty(&manifest).unwrap_or_default(),
        ),
        ("incident.json".to_string(), incident_json),
        ("events.jsonl".to_string(), events),
        ("metrics.json".to_string(), metrics),
    ];
    if options.include_audio && incident.audio_available {
        entries.push(("audio.wav".to_string(), audio_wav(connection, paths, id)?));
    }
    if let Some(logs) = log_excerpt {
        entries.push(("logs.txt".to_string(), logs));
    }
    let checksums = entries
        .iter()
        .map(|(name, bytes)| format!("{:x}  {name}\n", Sha256::digest(bytes)))
        .collect::<String>();
    entries.push(("checksums.sha256".to_string(), checksums.into_bytes()));
    stored_zip(entries)
}

fn query_jsonl(connection: &Connection, sql: &str, id: &str) -> Result<Vec<u8>, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    let values = statement.query_map([id], |row| Ok(json!({
        "name": row.get::<_, String>(0)?,
        "attributes": serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(1)?).unwrap_or(json!({})),
        "createdAtUtcMs": row.get::<_, i64>(2)?
    }).to_string())).map_err(|error| error.to_string())?;
    let mut output = String::new();
    for value in values {
        output.push_str(&value.map_err(|error| error.to_string())?);
        output.push('\n');
    }
    Ok(output.into_bytes())
}

fn read_redacted_log_excerpt(directory: &std::path::Path) -> Option<Vec<u8>> {
    const LIMIT: usize = 256 * 1024;
    let mut files = fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .collect::<Vec<_>>();
    files.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok(),
        )
    });
    let mut raw = Vec::new();
    for entry in files {
        if raw.len() >= LIMIT {
            break;
        }
        let bytes = fs::read(entry.path()).ok()?;
        let remaining = LIMIT - raw.len();
        let start = bytes.len().saturating_sub(remaining);
        raw.extend_from_slice(&bytes[start..]);
    }
    if raw.is_empty() {
        return None;
    }
    Some(redact_log(&String::from_utf8_lossy(&raw)).into_bytes())
}

fn redact_log(input: &str) -> String {
    redact_sensitive(input)
}
fn stored_zip(entries: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();

    let mut central = Vec::new();
    for (name, data) in entries {
        let offset = u32::try_from(out.len()).map_err(|_| "report_too_large".to_string())?;
        let mut crc = Crc32::new();
        crc.update(&data);
        let crc = crc.finalize();
        let name_bytes = name.as_bytes();
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&data);
        central.push((name, crc, data.len() as u32, offset));
    }
    let central_offset = out.len() as u32;
    for (name, crc, size, offset) in &central {
        let name = name.as_bytes();
        out.extend_from_slice(&0x02014b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        out.extend_from_slice(name);
    }
    let central_size = out.len() as u32 - central_offset;
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u16).to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod current_contract_tests {
    use super::*;
    use crate::incident::schema::{open_database, IncidentPaths};
    use rusqlite::params;
    use tempfile::TempDir;

    fn test_paths(temp: &TempDir) -> IncidentPaths {
        let root = temp.path().join("incidents");
        let artifacts = root.join("artifacts");
        let exports = root.join("exports");
        std::fs::create_dir_all(&artifacts).unwrap();
        std::fs::create_dir_all(&exports).unwrap();
        IncidentPaths {
            database: root.join("incident.db"),
            emergency: root.join("panic-emergency.jsonl"),
            root,
            artifacts,
            exports,
        }
    }

    #[test]
    fn log_redaction_removes_header_and_assignment_values() {
        let input = concat!(
            "Authorization: Bearer very-secret-token\n",
            "api_key = another-secret\n",
            "https://example.test/path?token=query-secret\n",
            "C:\\Users\\Alice\\private.log\n",
        );
        let redacted = redact_log(input);
        for secret in [
            "very-secret-token",
            "another-secret",
            "query-secret",
            "Alice",
        ] {
            assert!(
                !redacted.contains(secret),
                "redaction leaked {secret}: {redacted}"
            );
        }
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("[QUERY_REDACTED]"));
        assert!(redacted.contains("[LOCAL_PATH]"));
    }

    #[test]
    fn report_lookup_is_not_limited_to_the_latest_two_hundred_incidents() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        for index in 0..=200 {
            let id = format!("incident-{index:03}");
            connection
                .execute(
                    "INSERT INTO voice_attempts(
                    attempt_id,runtime_session_id,started_at_utc_ms,ended_at_utc_ms,
                    terminal_outcome,content_enabled,retention_days,storage_limit_mb,
                    success_rollup_days,created_at_utc_ms,updated_at_utc_ms
                 ) VALUES(?1,1,?2,?2,'failed',0,7,512,30,?2,?2)",
                    params![id, index as i64],
                )
                .unwrap();
            connection
                .execute(
                    "INSERT INTO incident_findings(
                    attempt_id,stage_name,reason_code,severity,recoverability,
                    localized_message,created_at_utc_ms
                 ) VALUES(?1,'asr','asr_timeout','error','none','timeout',?2)",
                    params![id, index as i64],
                )
                .unwrap();
        }

        let bytes = report(
            &connection,
            &paths,
            "incident-000",
            &ReportOptions {
                include_text: false,
                include_audio: false,
                include_log_excerpt: false,
            },
            None,
        )
        .expect("a report addressed by stable incident id must not depend on list pagination");
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn default_report_does_not_embed_recovery_text_or_audio() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        connection
            .execute(
                "INSERT INTO voice_attempts(
                attempt_id,runtime_session_id,started_at_utc_ms,ended_at_utc_ms,
                terminal_outcome,content_enabled,retention_days,storage_limit_mb,
                success_rollup_days,created_at_utc_ms,updated_at_utc_ms
             ) VALUES('private',1,1,2,'failed',1,7,512,30,1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO incident_findings(
                attempt_id,stage_name,reason_code,severity,recoverability,
                localized_message,created_at_utc_ms
             ) VALUES('private','asr','asr_timeout','error','text_and_audio','timeout',2)",
                [],
            )
            .unwrap();
        std::fs::write(
            paths.artifacts.join("private-partial.txt"),
            b"private transcript",
        )
        .unwrap();
        std::fs::write(paths.artifacts.join("private.pcm"), b"private audio marker").unwrap();
        connection
            .execute(
                "INSERT INTO recovery_artifacts(
                attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size
             ) VALUES('private','partial_text','private-partial.txt','complete',18)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO recovery_artifacts(
                attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size
             ) VALUES('private','audio_pcm','private.pcm','complete',20)",
                [],
            )
            .unwrap();

        let zip = report(
            &connection,
            &paths,
            "private",
            &ReportOptions {
                include_text: false,
                include_audio: false,
                include_log_excerpt: false,
            },
            None,
        )
        .unwrap();
        assert!(!zip
            .windows(b"private transcript".len())
            .any(|value| value == b"private transcript"));
        assert!(!zip
            .windows(b"private audio marker".len())
            .any(|value| value == b"private audio marker"));
    }
    #[test]
    fn audio_with_a_mismatched_checksum_is_never_returned() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        std::fs::write(paths.artifacts.join("corrupt.pcm"), [1, 2, 3, 4]).unwrap();
        let connection = open_database(&paths).unwrap();
        connection
            .execute(
                "INSERT INTO voice_attempts(
                attempt_id,runtime_session_id,started_at_utc_ms,ended_at_utc_ms,
                terminal_outcome,content_enabled,retention_days,storage_limit_mb,
                success_rollup_days,created_at_utc_ms,updated_at_utc_ms
             ) VALUES('corrupt',1,1,2,'failed',1,7,512,30,1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO recovery_artifacts(
                attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size,sha256_hex
             ) VALUES('corrupt','audio_pcm','corrupt.pcm','complete',4,?1)",
                ["00".repeat(32)],
            )
            .unwrap();

        assert_eq!(
            audio_wav(&connection, &paths, "corrupt"),
            Err("artifact_integrity_mismatch".to_string())
        );
    }
    #[test]
    fn text_attachment_does_not_implicitly_export_target_application_context() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        connection
            .execute(
                "INSERT INTO voice_attempts(
                    attempt_id,runtime_session_id,started_at_utc_ms,ended_at_utc_ms,
                    terminal_outcome,content_enabled,target_app_name,target_window_title,
                    retention_days,storage_limit_mb,success_rollup_days,created_at_utc_ms,updated_at_utc_ms
                 ) VALUES('scoped-export',1,1,2,'failed',1,'secret-app.exe','secret document',
                          7,512,30,1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO incident_findings(
                    attempt_id,stage_name,reason_code,severity,recoverability,
                    localized_message,created_at_utc_ms
                 ) VALUES('scoped-export','asr','asr_timeout','error','final_text','timeout',2)",
                [],
            )
            .unwrap();
        let transcript = b"allowed transcript";
        std::fs::write(
            paths.artifacts.join("scoped-export-final_text.txt"),
            transcript,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO recovery_artifacts(
                    attempt_id,artifact_kind,relative_path,artifact_completeness,
                    byte_size,sha256_hex
                 ) VALUES('scoped-export','final_text','scoped-export-final_text.txt',
                          'complete',?1,?2)",
                params![
                    transcript.len() as u64,
                    format!("{:x}", Sha256::digest(transcript))
                ],
            )
            .unwrap();

        let zip = report(
            &connection,
            &paths,
            "scoped-export",
            &ReportOptions {
                include_text: true,
                include_audio: false,
                include_log_excerpt: false,
            },
            None,
        )
        .unwrap();

        assert!(zip
            .windows(transcript.len())
            .any(|value| value == transcript));
        for private_context in [b"secret-app.exe".as_slice(), b"secret document".as_slice()] {
            assert!(!zip
                .windows(private_context.len())
                .any(|value| value == private_context));
        }
    }

    #[test]
    fn failed_artifact_removal_keeps_database_index_for_a_retry() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        std::fs::create_dir(paths.artifacts.join("not-a-file")).unwrap();
        connection
            .execute(
                "INSERT INTO voice_attempts(
                    attempt_id,runtime_session_id,started_at_utc_ms,ended_at_utc_ms,
                    terminal_outcome,content_enabled,retention_days,storage_limit_mb,
                    success_rollup_days,created_at_utc_ms,updated_at_utc_ms
                 ) VALUES('delete-retry',1,1,2,'failed',1,7,512,30,1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO recovery_artifacts(
                    attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size
                 ) VALUES('delete-retry','audio_pcm','not-a-file','complete',0)",
                [],
            )
            .unwrap();

        assert!(delete(&connection, &paths, "delete-retry").is_err());
        let remaining: (i64, i64) = connection
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM voice_attempts WHERE attempt_id='delete-retry'),
                   (SELECT COUNT(*) FROM recovery_artifacts WHERE attempt_id='delete-retry')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(remaining, (1, 1));
    }

    #[test]
    fn corrupted_artifact_path_cannot_delete_a_file_outside_artifact_root() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let outside = paths.root.join("must-survive.txt");
        std::fs::write(&outside, b"keep").unwrap();
        let connection = open_database(&paths).unwrap();
        connection
            .execute(
                "INSERT INTO voice_attempts(
                attempt_id,runtime_session_id,started_at_utc_ms,ended_at_utc_ms,
                terminal_outcome,content_enabled,retention_days,storage_limit_mb,
                success_rollup_days,created_at_utc_ms,updated_at_utc_ms
             ) VALUES('tampered',1,1,2,'failed',0,7,512,30,1,2)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO recovery_artifacts(
                attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size
             ) VALUES('tampered','partial_text','../must-survive.txt','complete',4)",
                [],
            )
            .unwrap();

        assert_eq!(
            delete(&connection, &paths, "tampered"),
            Err("artifact_path_invalid".to_string())
        );
        assert!(
            outside.is_file(),
            "database path corruption must not escape the artifacts directory"
        );
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM recovery_artifacts WHERE attempt_id='tampered'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 1, "a failed delete must remain retryable");
    }
}
