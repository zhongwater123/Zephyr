use super::model::{AttemptPolicy, IncidentEvent};
use super::redact_sensitive;
use super::schema::{artifact_path, bounded, utc_ms, IncidentPaths};
use bytes::Bytes;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::sync::Arc;

struct OpenAudio {
    file: File,
    part_name: String,
    bytes: u64,
    has_gap: bool,
    last_checkpoint_ms: i64,
}

pub struct VaultWriter {
    connection: Connection,
    paths: IncidentPaths,
    audio: HashMap<String, OpenAudio>,
    pending_audio_gaps: HashSet<Arc<str>>,
    policies: HashMap<String, AttemptPolicy>,
    event_sequences: HashMap<String, u64>,
    artifact_capture_paused: bool,
}

impl VaultWriter {
    pub fn new(connection: Connection, paths: IncidentPaths) -> Self {
        Self {
            connection,
            paths,
            audio: HashMap::new(),
            pending_audio_gaps: HashSet::new(),
            policies: HashMap::new(),
            event_sequences: HashMap::new(),
            artifact_capture_paused: false,
        }
    }

    pub fn flush_open_artifacts(&mut self) -> Result<(), String> {
        for audio in self.audio.values_mut() {
            audio.file.flush().map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn recover_interrupted(&mut self) -> Result<(), String> {
        let now = utc_ms();
        self.connection
            .execute(
                "UPDATE voice_attempts SET terminal_outcome='interrupted', ended_at_utc_ms=?1,
                 expires_at_utc_ms=?1 + retention_days * 86400000, updated_at_utc_ms=?1
                 WHERE terminal_outcome IS NULL",
                [now],
            )
            .map_err(|error| error.to_string())?;
        for entry in fs::read_dir(&self.paths.artifacts).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.extension().and_then(|value| value.to_str()) == Some("part") {
                let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                let Some(attempt_id) = file_name.strip_suffix(".pcm.part") else {
                    continue;
                };
                let attempt: Option<(i64, i64, i64)> = self
                    .connection
                    .query_row(
                        "SELECT content_enabled,audio_capture_authorized,
                         COALESCE(expires_at_utc_ms, ?2)
                         FROM voice_attempts
                         WHERE attempt_id=?1 AND terminal_outcome='interrupted'",
                        params![attempt_id, now.saturating_add(7 * 86_400_000)],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if let Some((1, 1, expires)) = attempt {
                    let sealed_name = format!("{attempt_id}.pcm");
                    let sealed = self.paths.artifacts.join(&sealed_name);
                    fs::rename(&path, &sealed).map_err(|error| error.to_string())?;
                    let byte_size = fs::metadata(&sealed)
                        .map_err(|error| error.to_string())?
                        .len();
                    let digest = sha256_file(&sealed)?;
                    self.connection.execute(
                        "INSERT INTO recovery_artifacts(attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size,sha256_hex,expires_at_utc_ms,sealed_at_utc_ms)
                         VALUES(?1,'audio_pcm',?2,'truncated',?3,?4,?5,?6)
                         ON CONFLICT(attempt_id,artifact_kind) DO UPDATE SET
                         relative_path=excluded.relative_path,artifact_completeness='truncated',
                         byte_size=excluded.byte_size,sha256_hex=excluded.sha256_hex,
                         expires_at_utc_ms=excluded.expires_at_utc_ms,sealed_at_utc_ms=excluded.sealed_at_utc_ms",
                        params![attempt_id, sealed_name, byte_size, digest, expires, now],
                    ).map_err(|error| error.to_string())?;
                } else {
                    remove_file_if_present(&path)?;
                }
            }
        }
        if let Ok(content) = fs::read_to_string(&self.paths.emergency) {
            let mut retained = Vec::new();
            for line in content.lines().filter(|line| !line.trim().is_empty()) {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                    retained.push(line);
                    continue;
                };
                if self
                    .process(IncidentEvent::FrontendFailure {
                        attempt_id: uuid::Uuid::new_v4().to_string(),
                        source: "panic_hook".to_string(),
                        code: "runtime_panic".to_string(),
                        message: value
                            .get("message")
                            .and_then(|value| value.as_str())
                            .unwrap_or("runtime panic")
                            .to_string(),
                        stack: value
                            .get("backtrace")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        occurred_at_utc_ms: value
                            .get("occurredAtUtcMs")
                            .and_then(|value| value.as_i64())
                            .unwrap_or(now),
                    })
                    .is_err()
                {
                    retained.push(line);
                }
            }
            let remaining = if retained.is_empty() {
                String::new()
            } else {
                format!("{}\n", retained.join("\n"))
            };
            fs::write(&self.paths.emergency, remaining).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn process(&mut self, event: IncidentEvent) -> Result<(), String> {
        match event {
            IncidentEvent::AttemptStarted {
                attempt_id,
                runtime_session_id,
                started_at_utc_ms,
                app_name,
                app_title,
                policy,
                ..
            } => self.start_attempt(
                &attempt_id,
                runtime_session_id,
                started_at_utc_ms,
                app_name,
                app_title,
                policy,
            ),
            IncidentEvent::StageChanged {
                attempt_id,
                stage,
                outcome,
                reason_code,
                monotonic_us,
            } => {
                self.connection.execute(
                    "INSERT INTO attempt_stages(attempt_id,stage_name,stage_outcome,reason_code,first_monotonic_us,last_monotonic_us)
                     VALUES(?1,?2,?3,?4,?5,?5)
                     ON CONFLICT(attempt_id,stage_name) DO UPDATE SET
                     stage_outcome=excluded.stage_outcome, reason_code=excluded.reason_code,
                     last_monotonic_us=excluded.last_monotonic_us",
                    params![attempt_id, stage.as_str(), outcome.as_str(), reason_code, monotonic_us],
                ).map_err(|error| error.to_string())?;
                self.event(&attempt_id, "stage_changed", Some(monotonic_us), serde_json::json!({
                    "stage": stage.as_str(), "stageOutcome": outcome.as_str(), "reasonCode": reason_code
                }))
            }
            IncidentEvent::AudioChunk {
                attempt_id, bytes, ..
            } => self.audio_chunk(&attempt_id, bytes),
            IncidentEvent::AudioGap { attempt_id } => {
                if let Some(audio) = self.audio.get_mut(attempt_id.as_ref()) {
                    audio.has_gap = true;
                } else {
                    self.pending_audio_gaps.insert(attempt_id);
                }
                Ok(())
            }
            IncidentEvent::PartialCheckpoint {
                attempt_id,
                text,
                confirmed_chars,
                monotonic_us,
            } => {
                if self
                    .policy(&attempt_id)
                    .map(|p| p.content_enabled && p.save_text)
                    .unwrap_or(false)
                {
                    self.write_text(&attempt_id, "partial_text", &text)?;
                }
                self.event(
                    &attempt_id,
                    "partial_checkpoint",
                    Some(monotonic_us),
                    serde_json::json!({
                        "confirmedChars": confirmed_chars, "textLength": text.chars().count()
                    }),
                )
            }
            IncidentEvent::FinalTranscript {
                attempt_id,
                text,
                monotonic_us,
            } => {
                if self
                    .policy(&attempt_id)
                    .map(|p| p.content_enabled && p.save_text)
                    .unwrap_or(false)
                {
                    self.write_text(&attempt_id, "final_text", &text)?;
                }
                self.event(
                    &attempt_id,
                    "final_transcript",
                    Some(monotonic_us),
                    serde_json::json!({
                        "textLength": text.chars().count()
                    }),
                )
            }
            IncidentEvent::Finding {
                attempt_id,
                stage,
                code,
                message,
                severity,
                recoverability,
            } => {
                self.connection.execute(
                    "INSERT INTO incident_findings(attempt_id,stage_name,reason_code,severity,recoverability,localized_message,created_at_utc_ms)
                     VALUES(?1,?2,?3,?4,?5,?6,?7)",
                    params![attempt_id, stage.as_str(), bounded(&code, 96), severity,
                        recoverability.as_str(), bounded(&redact_sensitive(&message), 1024), utc_ms()],
                ).map_err(|error| error.to_string())?;
                Ok(())
            }
            IncidentEvent::Metric {
                attempt_id,
                name,
                value,
                unit,
            } => {
                self.connection.execute(
                    "INSERT INTO diagnostic_metrics(attempt_id,metric_name,metric_value,metric_unit,created_at_utc_ms)
                     VALUES(?1,?2,?3,?4,?5)",
                    params![attempt_id, name, value, unit, utc_ms()],
                ).map_err(|error| error.to_string())?;
                Ok(())
            }
            IncidentEvent::AttemptEnded {
                attempt_id,
                outcome,
                history_committed: _,
                discard_recovery_material,
                ended_at_utc_ms,
            } => {
                self.seal_audio(&attempt_id)?;
                if outcome == super::model::TerminalOutcome::Succeeded && discard_recovery_material
                {
                    self.delete_artifacts(&attempt_id)?;
                }
                let retention = self
                    .policy(&attempt_id)
                    .map(|p| p.retention_days)
                    .unwrap_or(7);
                let expires = ended_at_utc_ms.saturating_add(i64::from(retention) * 86_400_000);
                self.connection
                    .execute(
                        "UPDATE voice_attempts SET ended_at_utc_ms=?2,terminal_outcome=?3,
                     expires_at_utc_ms=?4,updated_at_utc_ms=?2 WHERE attempt_id=?1",
                        params![attempt_id, ended_at_utc_ms, outcome.as_str(), expires],
                    )
                    .map_err(|error| error.to_string())?;
                self.rollup(&attempt_id, outcome.as_str(), ended_at_utc_ms)?;
                self.policies.remove(&attempt_id);
                Ok(())
            }
            IncidentEvent::FrontendFailure {
                attempt_id,
                source,
                code,
                message,
                stack,
                occurred_at_utc_ms,
            } => {
                let policy = AttemptPolicy {
                    content_enabled: false,
                    save_audio: false,
                    save_text: false,
                    retention_days: 7,
                    storage_limit_mb: 512,
                    success_rollup_days: 30,
                };
                self.start_attempt(
                    &attempt_id,
                    0,
                    occurred_at_utc_ms,
                    Some(source),
                    None,
                    policy,
                )?;
                self.connection.execute(
                    "INSERT INTO incident_findings(attempt_id,stage_name,reason_code,severity,recoverability,localized_message,created_at_utc_ms)
                     VALUES(?1,'frontend',?2,'error','none',?3,?4)",
                    params![
                        attempt_id,
                        bounded(&code, 96),
                        bounded(&redact_sensitive(&message), 1024),
                        occurred_at_utc_ms
                    ],
                ).map_err(|error| error.to_string())?;
                self.event(
                    &attempt_id,
                    "frontend_failure",
                    None,
                    serde_json::json!({
                        "stack": stack.map(|value| bounded(&redact_sensitive(&value), 4096))
                    }),
                )?;
                self.process(IncidentEvent::AttemptEnded {
                    attempt_id,
                    outcome: super::model::TerminalOutcome::Failed,
                    history_committed: false,
                    discard_recovery_material: false,
                    ended_at_utc_ms: occurred_at_utc_ms,
                })
            }
        }
    }

    pub fn maintenance(&mut self) -> Result<bool, String> {
        let now = utc_ms();
        let mut statement = self.connection.prepare(
            "SELECT attempt_id FROM voice_attempts WHERE pinned=0 AND expires_at_utc_ms IS NOT NULL AND expires_at_utc_ms < ?1"
        ).map_err(|error| error.to_string())?;
        let ids = statement
            .query_map([now], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        drop(statement);
        for id in ids {
            self.delete_artifacts(&id)?;
            self.delete_attempt_rows(&id)?;
        }
        self.connection
            .execute(
                "DELETE FROM daily_metric_rollups WHERE expires_at_utc_ms < ?1",
                [now],
            )
            .map_err(|error| error.to_string())?;
        let degraded = self.directory_size()? > self.storage_limit_bytes()?;
        self.artifact_capture_paused = degraded;
        Ok(degraded)
    }

    fn start_attempt(
        &mut self,
        id: &str,
        session: u64,
        started: i64,
        app: Option<String>,
        title: Option<String>,
        policy: AttemptPolicy,
    ) -> Result<(), String> {
        let content = policy.content_enabled;
        let audio_capture_authorized = content && policy.save_audio;
        let text_capture_authorized = content && policy.save_text;
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO voice_attempts(attempt_id,runtime_session_id,started_at_utc_ms,content_enabled,audio_capture_authorized,text_capture_authorized,target_app_name,target_window_title,retention_days,storage_limit_mb,success_rollup_days,created_at_utc_ms,updated_at_utc_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?3,?3)",
            params![id, session, started, content, audio_capture_authorized, text_capture_authorized,
                if content { app } else { None }, if content { title } else { None },
                policy.retention_days, policy.storage_limit_mb, policy.success_rollup_days],
        ).map_err(|error| error.to_string())?;
        if inserted > 0 {
            self.policies.insert(id.to_string(), policy);
        } else if !self.policies.contains_key(id) {
            let (content_enabled, audio_authorized, text_authorized) = self
                .connection
                .query_row(
                    "SELECT content_enabled,audio_capture_authorized,text_capture_authorized
                     FROM voice_attempts WHERE attempt_id=?1",
                    [id],
                    |row| {
                        Ok((
                            row.get::<_, bool>(0)?,
                            row.get::<_, bool>(1)?,
                            row.get::<_, bool>(2)?,
                        ))
                    },
                )
                .map_err(|error| error.to_string())?;
            let mut persisted_policy = policy;
            persisted_policy.content_enabled &= content_enabled;
            persisted_policy.save_audio &= persisted_policy.content_enabled && audio_authorized;
            persisted_policy.save_text &= persisted_policy.content_enabled && text_authorized;
            self.policies.insert(id.to_string(), persisted_policy);
        }
        Ok(())
    }

    fn policy(&self, id: &str) -> Option<&AttemptPolicy> {
        self.policies.get(id)
    }

    fn audio_chunk(&mut self, id: &str, bytes: Bytes) -> Result<(), String> {
        if !self
            .policy(id)
            .map(|p| p.content_enabled && p.save_audio)
            .unwrap_or(false)
        {
            return Ok(());
        }
        if self.artifact_capture_paused {
            return Ok(());
        }
        if !self.audio.contains_key(id) {
            let name = format!("{id}.pcm.part");
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.paths.artifacts.join(&name))
                .map_err(|error| error.to_string())?;
            let has_gap = self.pending_audio_gaps.remove(id);
            self.audio.insert(
                id.to_string(),
                OpenAudio {
                    file,
                    part_name: name,
                    bytes: 0,
                    has_gap,
                    last_checkpoint_ms: utc_ms(),
                },
            );
        }
        let audio = self.audio.get_mut(id).expect("audio entry inserted");
        audio
            .file
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        audio.bytes += bytes.len() as u64;
        if utc_ms() - audio.last_checkpoint_ms >= 500 {
            audio.file.flush().map_err(|error| error.to_string())?;
            audio.last_checkpoint_ms = utc_ms();
        }
        Ok(())
    }
    fn write_text(&mut self, id: &str, kind: &str, text: &str) -> Result<(), String> {
        let name = format!("{id}-{kind}.txt");
        if self.artifact_capture_paused {
            return Ok(());
        }
        let path = self.paths.artifacts.join(&name);
        fs::write(&path, text.as_bytes()).map_err(|error| error.to_string())?;
        let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
        let expires = utc_ms().saturating_add(
            i64::from(self.policy(id).map(|p| p.retention_days).unwrap_or(7)) * 86_400_000,
        );
        self.connection.execute(
            "INSERT INTO recovery_artifacts(attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size,sha256_hex,expires_at_utc_ms,sealed_at_utc_ms)
             VALUES(?1,?2,?3,'complete',?4,?5,?6,?7)
             ON CONFLICT(attempt_id,artifact_kind) DO UPDATE SET
             relative_path=excluded.relative_path,byte_size=excluded.byte_size,
             sha256_hex=excluded.sha256_hex,sealed_at_utc_ms=excluded.sealed_at_utc_ms",
            params![id, kind, name, text.len() as u64, digest, expires, utc_ms()],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn seal_audio(&mut self, id: &str) -> Result<(), String> {
        let Some(mut audio) = self.audio.remove(id) else {
            return Ok(());
        };
        audio.file.flush().map_err(|error| error.to_string())?;
        audio.file.sync_data().map_err(|error| error.to_string())?;
        drop(audio.file);
        let sealed_name = format!("{id}.pcm");
        let part = self.paths.artifacts.join(&audio.part_name);
        let sealed = self.paths.artifacts.join(&sealed_name);
        fs::rename(&part, &sealed).map_err(|error| error.to_string())?;
        let digest = sha256_file(&sealed)?;
        let completeness = if audio.has_gap { "gapped" } else { "complete" };
        let expires = utc_ms().saturating_add(
            i64::from(self.policy(id).map(|p| p.retention_days).unwrap_or(7)) * 86_400_000,
        );
        self.connection.execute(
            "INSERT INTO recovery_artifacts(attempt_id,artifact_kind,relative_path,artifact_completeness,byte_size,sha256_hex,expires_at_utc_ms,sealed_at_utc_ms)
             VALUES(?1,'audio_pcm',?2,?3,?4,?5,?6,?7)
             ON CONFLICT(attempt_id,artifact_kind) DO UPDATE SET
             relative_path=excluded.relative_path,artifact_completeness=excluded.artifact_completeness,
             byte_size=excluded.byte_size,sha256_hex=excluded.sha256_hex,sealed_at_utc_ms=excluded.sealed_at_utc_ms",
            params![id, sealed_name, completeness, audio.bytes, digest, expires, utc_ms()],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn delete_artifacts(&mut self, id: &str) -> Result<(), String> {
        self.audio.remove(id);
        self.pending_audio_gaps.remove(id);
        let mut statement = self
            .connection
            .prepare("SELECT relative_path FROM recovery_artifacts WHERE attempt_id=?1")
            .map_err(|error| error.to_string())?;
        let names = statement
            .query_map([id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();
        drop(statement);
        for name in names {
            let path = artifact_path(&self.paths, &name)?;
            remove_file_if_present(&path)?;
        }
        for suffix in [".pcm.part", ".pcm", "-partial_text.txt", "-final_text.txt"] {
            let name = format!("{id}{suffix}");
            let path = artifact_path(&self.paths, &name)?;
            remove_file_if_present(&path)?;
        }
        self.connection
            .execute("DELETE FROM recovery_artifacts WHERE attempt_id=?1", [id])
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn delete_attempt_rows(&mut self, id: &str) -> Result<(), String> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for table in [
            "diagnostic_metrics",
            "incident_events",
            "incident_findings",
            "attempt_stages",
        ] {
            transaction
                .execute(&format!("DELETE FROM {table} WHERE attempt_id=?1"), [id])
                .map_err(|error| error.to_string())?;
        }
        transaction
            .execute("DELETE FROM voice_attempts WHERE attempt_id=?1", [id])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    }

    fn event(
        &mut self,
        id: &str,
        name: &str,
        monotonic_us: Option<u64>,
        attributes: serde_json::Value,
    ) -> Result<(), String> {
        let sequence = self.event_sequences.entry(id.to_string()).or_insert(0);
        *sequence = sequence.saturating_add(1);
        self.connection.execute(
            "INSERT OR IGNORE INTO incident_events(attempt_id,sequence_number,event_name,monotonic_us,attributes_json,created_at_utc_ms)
             VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, *sequence, name, monotonic_us, attributes.to_string(), utc_ms()],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn rollup(&self, id: &str, outcome: &str, ended: i64) -> Result<(), String> {
        let days = self.policy(id).map(|p| p.success_rollup_days).unwrap_or(30);
        let date = chrono::DateTime::from_timestamp_millis(ended)
            .unwrap_or_default()
            .format("%Y-%m-%d")
            .to_string();
        self.connection.execute(
            "INSERT INTO daily_metric_rollups(metric_date,metric_name,dimension_key,metric_count,metric_sum,expires_at_utc_ms)
             VALUES(?1,'attempt_terminal',?2,1,1,?3)
             ON CONFLICT(metric_date,metric_name,dimension_key) DO UPDATE SET
             metric_count=metric_count+1,metric_sum=metric_sum+1",
            params![date, outcome, ended.saturating_add(i64::from(days) * 86_400_000)],
        ).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn directory_size(&self) -> Result<u64, String> {
        let mut total = 0u64;
        for entry in fs::read_dir(&self.paths.root).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            if entry.path().is_file() {
                total = total
                    .saturating_add(entry.metadata().map_err(|error| error.to_string())?.len());
            }
        }
        for entry in fs::read_dir(&self.paths.artifacts).map_err(|error| error.to_string())? {
            total = total.saturating_add(
                entry
                    .map_err(|error| error.to_string())?
                    .metadata()
                    .map_err(|error| error.to_string())?
                    .len(),
            );
        }
        Ok(total)
    }

    fn storage_limit_bytes(&self) -> Result<u64, String> {
        let limit: Option<u32> = self
            .connection
            .query_row(
                "SELECT MAX(storage_limit_mb) FROM voice_attempts",
                [],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        Ok(u64::from(limit.unwrap_or(512)) * 1024 * 1024)
    }
}

fn remove_file_if_present(path: &std::path::Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
