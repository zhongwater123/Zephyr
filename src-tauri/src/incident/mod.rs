pub mod model;
mod query;
mod redaction;
mod schema;
mod writer;

pub(crate) use redaction::redact_sensitive;

use crossbeam_queue::ArrayQueue;
use model::{DropReason, EmitOutcome, IncidentEvent, IncidentHealth, IncidentItem, ReportOptions};
use schema::{open_database, IncidentPaths};
use std::fs::OpenOptions;
use std::io::Write;
use std::panic::PanicHookInfo;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

const CONTROL_QUEUE_CAPACITY: usize = 64;
const AUDIO_QUEUE_CAPACITY: usize = 64;
const AUDIO_GAP_QUEUE_CAPACITY: usize = 64;

pub trait IncidentSink: Send + Sync {
    fn try_emit(&self, event: IncidentEvent) -> EmitOutcome;
    fn health_snapshot(&self) -> IncidentHealth;
    fn shutdown(&self, _timeout: Duration) {}
}

#[derive(Default)]
pub struct NoopIncidentSink;

impl IncidentSink for NoopIncidentSink {
    fn try_emit(&self, _event: IncidentEvent) -> EmitOutcome {
        EmitOutcome::Disabled
    }
    fn health_snapshot(&self) -> IncidentHealth {
        IncidentHealth::default()
    }
}

struct HealthState {
    available: AtomicBool,
    degraded: AtomicBool,
    control_dropped: AtomicU64,
    audio_dropped: AtomicU64,
    last_error: Mutex<Option<String>>,
}

impl HealthState {
    fn snapshot(&self) -> IncidentHealth {
        IncidentHealth {
            available: self.available.load(Ordering::Relaxed),
            degraded: self.degraded.load(Ordering::Relaxed),
            control_events_dropped: self.control_dropped.load(Ordering::Relaxed),
            audio_chunks_dropped: self.audio_dropped.load(Ordering::Relaxed),
            last_error: self.last_error.lock().ok().and_then(|value| value.clone()),
        }
    }

    fn fail(&self, message: String) {
        self.degraded.store(true, Ordering::Relaxed);
        if let Ok(mut last) = self.last_error.lock() {
            *last = Some(message);
        }
    }
}

fn process_writer_event(
    writer: &mut writer::VaultWriter,
    health: &HealthState,
    event: IncidentEvent,
) {
    if let Err(error) = writer.process(event) {
        health.fail(error);
    }
}

fn drain_audio_gap_queue(
    writer: &mut writer::VaultWriter,
    health: &HealthState,
    audio_gap_queue: &ArrayQueue<Arc<str>>,
) -> bool {
    let mut drained = false;
    while let Some(attempt_id) = audio_gap_queue.pop() {
        drained = true;
        process_writer_event(writer, health, IncidentEvent::AudioGap { attempt_id });
    }
    drained
}

fn drain_audio_queue(
    writer: &mut writer::VaultWriter,
    health: &HealthState,
    audio_queue: &ArrayQueue<IncidentEvent>,
) -> bool {
    let mut drained = false;
    while let Some(event) = audio_queue.pop() {
        drained = true;
        process_writer_event(writer, health, event);
    }
    drained
}

fn run_writer(
    connection: rusqlite::Connection,
    paths: IncidentPaths,
    control_queue: Arc<ArrayQueue<IncidentEvent>>,
    audio_queue: Arc<ArrayQueue<IncidentEvent>>,
    audio_gap_queue: Arc<ArrayQueue<Arc<str>>>,
    shutdown_rx: mpsc::Receiver<mpsc::SyncSender<()>>,
    health: &HealthState,
) {
    let mut writer = writer::VaultWriter::new(connection, paths);
    if let Err(error) = writer.recover_interrupted() {
        health.fail(error);
    }
    let mut last_maintenance = Instant::now();
    loop {
        if let Ok(done) = shutdown_rx.try_recv() {
            while let Some(event) = control_queue.pop() {
                if matches!(&event, IncidentEvent::AttemptEnded { .. }) {
                    drain_audio_gap_queue(&mut writer, health, &audio_gap_queue);
                    drain_audio_queue(&mut writer, health, &audio_queue);
                }
                process_writer_event(&mut writer, health, event);
            }
            drain_audio_gap_queue(&mut writer, health, &audio_gap_queue);
            drain_audio_queue(&mut writer, health, &audio_queue);
            if let Err(error) = writer.flush_open_artifacts() {
                health.fail(error);
            }
            let _ = done.try_send(());
            break;
        }

        let mut processed = false;
        while let Some(event) = control_queue.pop() {
            processed = true;
            if matches!(&event, IncidentEvent::AttemptEnded { .. }) {
                drain_audio_gap_queue(&mut writer, health, &audio_gap_queue);
                drain_audio_queue(&mut writer, health, &audio_queue);
            }
            process_writer_event(&mut writer, health, event);
        }
        processed |= drain_audio_gap_queue(&mut writer, health, &audio_gap_queue);
        processed |= drain_audio_queue(&mut writer, health, &audio_queue);

        if last_maintenance.elapsed() >= Duration::from_secs(1) {
            match writer.maintenance() {
                Ok(degraded) => health.degraded.store(degraded, Ordering::Relaxed),
                Err(error) => health.fail(error),
            }
            last_maintenance = Instant::now();
        }
        if !processed {
            std::thread::park_timeout(Duration::from_millis(50));
        }
    }
}

pub struct AsyncIncidentVault {
    control_queue: Arc<ArrayQueue<IncidentEvent>>,
    audio_queue: Arc<ArrayQueue<IncidentEvent>>,
    audio_gap_queue: Arc<ArrayQueue<Arc<str>>>,
    health: Arc<HealthState>,
    shutdown_tx: mpsc::SyncSender<mpsc::SyncSender<()>>,
    worker_thread: std::thread::Thread,
}

impl AsyncIncidentVault {
    fn start(paths: IncidentPaths) -> Result<Arc<Self>, String> {
        let connection = open_database(&paths).map_err(|error| error.to_string())?;
        let control_queue = Arc::new(ArrayQueue::new(CONTROL_QUEUE_CAPACITY));
        let audio_queue = Arc::new(ArrayQueue::new(AUDIO_QUEUE_CAPACITY));
        let audio_gap_queue = Arc::new(ArrayQueue::new(AUDIO_GAP_QUEUE_CAPACITY));
        let (shutdown_tx, shutdown_rx) = mpsc::sync_channel::<mpsc::SyncSender<()>>(1);
        let health = Arc::new(HealthState {
            available: AtomicBool::new(true),
            degraded: AtomicBool::new(false),
            control_dropped: AtomicU64::new(0),
            audio_dropped: AtomicU64::new(0),
            last_error: Mutex::new(None),
        });
        let worker_health = health.clone();
        let worker_control_queue = control_queue.clone();
        let worker_audio_queue = audio_queue.clone();
        let worker_audio_gap_queue = audio_gap_queue.clone();
        let worker = std::thread::Builder::new()
            .name("incident-vault".to_string())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_writer(
                        connection,
                        paths,
                        worker_control_queue,
                        worker_audio_queue,
                        worker_audio_gap_queue,
                        shutdown_rx,
                        &worker_health,
                    );
                }));
                if result.is_err() {
                    worker_health.fail("incident_writer_panicked".to_string());
                }
                worker_health.available.store(false, Ordering::Release);
            })
            .map_err(|error| error.to_string())?;
        let worker_thread = worker.thread().clone();
        drop(worker);
        Ok(Arc::new(Self {
            control_queue,
            audio_queue,
            audio_gap_queue,
            shutdown_tx,
            health,
            worker_thread,
        }))
    }
}

impl IncidentSink for AsyncIncidentVault {
    fn try_emit(&self, event: IncidentEvent) -> EmitOutcome {
        if !self.health.available.load(Ordering::Acquire) {
            return EmitOutcome::Dropped(DropReason::WriterUnavailable);
        }
        let is_audio = event.is_audio();
        let result = if is_audio {
            self.audio_queue.push(event)
        } else {
            self.control_queue.push(event)
        };
        match result {
            Ok(()) => {
                self.worker_thread.unpark();
                EmitOutcome::Accepted
            }
            Err(event) => {
                metrics::counter!("incident.vault.events_dropped").increment(1);
                if is_audio {
                    self.health.audio_dropped.fetch_add(1, Ordering::Relaxed);
                    let attempt_id = match event {
                        IncidentEvent::AudioChunk { attempt_id, .. } => attempt_id,
                        _ => unreachable!("only audio chunks use the audio queue"),
                    };
                    if let Err(attempt_id) = self.audio_gap_queue.push(attempt_id) {
                        let _ = self
                            .control_queue
                            .push(IncidentEvent::AudioGap { attempt_id });
                    }
                    self.worker_thread.unpark();
                } else {
                    self.health.control_dropped.fetch_add(1, Ordering::Relaxed);
                }
                EmitOutcome::Dropped(DropReason::QueueFull)
            }
        }
    }

    fn health_snapshot(&self) -> IncidentHealth {
        self.health.snapshot()
    }

    fn shutdown(&self, timeout: Duration) {
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        if self.shutdown_tx.try_send(done_tx).is_ok() {
            self.worker_thread.unpark();
            let _ = done_rx.recv_timeout(timeout);
        }
    }
}

#[derive(Clone)]
pub struct IncidentService {
    sink: Arc<dyn IncidentSink>,
    paths: Option<IncidentPaths>,
}

impl IncidentService {
    pub fn production() -> Self {
        match IncidentPaths::discover()
            .and_then(|paths| AsyncIncidentVault::start(paths.clone()).map(|sink| (paths, sink)))
        {
            Ok((paths, sink)) => {
                install_emergency_panic_hook(Some(paths.clone()));
                Self {
                    sink,
                    paths: Some(paths),
                }
            }
            Err(error) => {
                log::error!("IncidentVault unavailable; continuing with Noop sink: {error}");
                Self {
                    sink: Arc::new(NoopIncidentSink),
                    paths: None,
                }
            }
        }
    }

    pub fn sink(&self) -> Arc<dyn IncidentSink> {
        self.sink.clone()
    }
    pub fn health(&self) -> IncidentHealth {
        self.sink.health_snapshot()
    }

    pub fn shutdown(&self) {
        self.sink.shutdown(Duration::from_millis(500));
    }

    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<IncidentItem>, String> {
        self.with_database(|connection, paths| query::list(connection, paths, limit, offset))
    }

    pub fn get(&self, id: &str) -> Result<IncidentItem, String> {
        self.with_database(|connection, paths| query::get(connection, paths, id))
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<(), String> {
        self.with_database(|connection, _| query::set_pinned(connection, id, pinned))
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        self.with_database(|connection, paths| query::delete(connection, paths, id))
    }

    pub fn audio_wav(&self, id: &str) -> Result<Vec<u8>, String> {
        self.with_database(|connection, paths| query::audio_wav(connection, paths, id))
    }

    pub fn report(
        &self,
        id: &str,
        options: &ReportOptions,
        log_dir: Option<&std::path::Path>,
    ) -> Result<Vec<u8>, String> {
        self.with_database(|connection, paths| {
            query::report(connection, paths, id, options, log_dir)
        })
    }

    fn with_database<T>(
        &self,
        operation: impl FnOnce(&rusqlite::Connection, &IncidentPaths) -> Result<T, String>,
    ) -> Result<T, String> {
        let paths = self
            .paths
            .as_ref()
            .ok_or_else(|| "incident_vault_unavailable".to_string())?;
        let connection = open_database(paths).map_err(|error| error.to_string())?;
        operation(&connection, paths)
    }
}

pub fn install_emergency_panic_hook(paths: Option<IncidentPaths>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(paths) = &paths {
            append_emergency(paths, info);
        }
        previous(info);
    }));
}

fn append_emergency(paths: &IncidentPaths, info: &PanicHookInfo<'_>) {
    let message = if let Some(value) = info.payload().downcast_ref::<&str>() {
        (*value).to_string()
    } else if let Some(value) = info.payload().downcast_ref::<String>() {
        value.clone()
    } else {
        "non-string panic".to_string()
    };
    let location = info
        .location()
        .map(|value| redact_sensitive(&format!("{}:{}", value.file(), value.line())));
    let backtrace = redact_sensitive(&std::backtrace::Backtrace::force_capture().to_string());
    let line = serde_json::json!({
        "occurredAtUtcMs": schema::utc_ms(),
        "message": schema::bounded(&redact_sensitive(&message), 1024),
        "location": location,
        "backtrace": schema::bounded(&backtrace, 8192)
    })
    .to_string();
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.emergency)
    {
        let _ = file.write_all(line.as_bytes());
        let _ = file.write_all(b"\n");
    }
}
#[cfg(test)]
mod tests {
    use super::model::{
        AttemptPolicy, IncidentEvent, Recoverability, Stage, StageOutcome, TerminalOutcome,
    };
    use super::schema::{open_database, IncidentPaths};
    use super::writer::VaultWriter;
    use super::{AsyncIncidentVault, HealthState, IncidentSink, AUDIO_GAP_QUEUE_CAPACITY};
    use bytes::Bytes;
    use crossbeam_queue::ArrayQueue;
    use std::sync::atomic::{AtomicBool, AtomicU64};
    use std::sync::{Arc, Mutex};
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

    fn policy(content_enabled: bool) -> AttemptPolicy {
        AttemptPolicy {
            content_enabled,
            save_audio: true,
            save_text: true,
            retention_days: 7,
            storage_limit_mb: 512,
            success_rollup_days: 30,
        }
    }

    fn start(writer: &mut VaultWriter, id: &str, content_enabled: bool) {
        writer
            .process(IncidentEvent::AttemptStarted {
                attempt_id: id.to_string(),
                runtime_session_id: 1,
                started_at_utc_ms: 1000,
                app_version: "test".to_string(),
                app_name: Some("secret.exe".to_string()),
                app_title: Some("secret title".to_string()),
                policy: policy(content_enabled),
            })
            .unwrap();
    }

    #[test]
    fn writer_redacts_finding_messages_even_when_a_caller_forgets() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "redacted-finding", false);
        writer
            .process(IncidentEvent::Finding {
                attempt_id: "redacted-finding".to_string(),
                stage: Stage::Asr,
                code: "provider_failed".to_string(),
                message: "Authorization: Bearer must-not-persist".to_string(),
                severity: "error",
                recoverability: Recoverability::None,
            })
            .unwrap();

        let stored: String = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT localized_message FROM incident_findings
                 WHERE attempt_id='redacted-finding'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, "[REDACTED]");
    }

    #[test]
    fn unconsented_attempt_never_creates_content_artifacts_or_target_context() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "no-consent", false);
        writer
            .process(IncidentEvent::AudioChunk {
                attempt_id: "no-consent".to_string().into(),
                sequence: 0,
                bytes: Bytes::from_static(&[1, 2, 3, 4]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        writer
            .process(IncidentEvent::PartialCheckpoint {
                attempt_id: "no-consent".to_string(),
                text: "private text".to_string(),
                confirmed_chars: 3,
                monotonic_us: 1,
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "no-consent".to_string(),
                outcome: TerminalOutcome::Failed,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        assert_eq!(std::fs::read_dir(&paths.artifacts).unwrap().count(), 0);
        let read = open_database(&paths).unwrap();
        let target: (Option<String>, Option<String>) = read.query_row(
            "SELECT target_app_name,target_window_title FROM voice_attempts WHERE attempt_id='no-consent'",
            [], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap();
        assert_eq!(target, (None, None));
    }

    #[test]
    fn delivered_attempt_keeps_recovery_material_when_history_write_fails() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "history-failed", true);
        writer
            .process(IncidentEvent::AudioChunk {
                attempt_id: "history-failed".to_string().into(),
                sequence: 0,
                bytes: Bytes::from_static(&[1, 2, 3, 4]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        writer
            .process(IncidentEvent::FinalTranscript {
                attempt_id: "history-failed".to_string(),
                text: "recover me".to_string(),
                monotonic_us: 1,
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "history-failed".to_string(),
                outcome: TerminalOutcome::Succeeded,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        assert!(paths.artifacts.join("history-failed.pcm").exists());
        assert!(paths
            .artifacts
            .join("history-failed-final_text.txt")
            .exists());
        let artifact_count: i64 = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM recovery_artifacts WHERE attempt_id='history-failed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifact_count, 2);
    }

    #[test]
    fn delivered_attempt_discards_material_when_history_is_skipped_by_policy() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "history-disabled", true);
        writer
            .process(IncidentEvent::AudioChunk {
                attempt_id: "history-disabled".to_string().into(),
                sequence: 0,
                bytes: Bytes::from_static(&[1, 2, 3, 4]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        writer
            .process(IncidentEvent::FinalTranscript {
                attempt_id: "history-disabled".to_string(),
                text: "delivered".to_string(),
                monotonic_us: 1,
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "history-disabled".to_string(),
                outcome: TerminalOutcome::Succeeded,
                history_committed: false,
                discard_recovery_material: true,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        assert_eq!(std::fs::read_dir(&paths.artifacts).unwrap().count(), 0);
        let artifact_count: i64 = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM recovery_artifacts WHERE attempt_id='history-disabled'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifact_count, 0);
    }

    #[test]
    fn successful_committed_attempt_removes_raw_recovery_material() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "success", true);
        writer
            .process(IncidentEvent::AudioChunk {
                attempt_id: "success".to_string().into(),
                sequence: 0,
                bytes: Bytes::from_static(&[1, 2, 3, 4]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        writer
            .process(IncidentEvent::FinalTranscript {
                attempt_id: "success".to_string(),
                text: "delivered".to_string(),
                monotonic_us: 1,
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "success".to_string(),
                outcome: TerminalOutcome::Succeeded,
                history_committed: true,
                discard_recovery_material: true,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        assert_eq!(std::fs::read_dir(&paths.artifacts).unwrap().count(), 0);
        let read = open_database(&paths).unwrap();
        let count: i64 = read
            .query_row(
                "SELECT COUNT(*) FROM recovery_artifacts WHERE attempt_id='success'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn dual_queues_preserve_attempt_start_and_end_around_audio() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let vault = super::AsyncIncidentVault::start(paths.clone()).unwrap();
        assert!(matches!(
            vault.try_emit(IncidentEvent::AttemptStarted {
                attempt_id: "ordered".to_string(),
                runtime_session_id: 2,
                started_at_utc_ms: 1000,
                app_version: "test".to_string(),
                app_name: None,
                app_title: None,
                policy: policy(true),
            }),
            super::model::EmitOutcome::Accepted
        ));
        let _ = vault.try_emit(IncidentEvent::AudioChunk {
            attempt_id: "ordered".to_string().into(),
            sequence: 0,
            bytes: Bytes::from_static(&[1, 2, 3, 4]),
            duration_ms: 1,
            is_final: true,
        });
        let _ = vault.try_emit(IncidentEvent::AttemptEnded {
            attempt_id: "ordered".to_string(),
            outcome: TerminalOutcome::Failed,
            history_committed: false,
            discard_recovery_material: false,
            ended_at_utc_ms: 2000,
        });

        for _ in 0..50 {
            if paths.artifacts.join("ordered.pcm").is_file() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            std::fs::read(paths.artifacts.join("ordered.pcm")).unwrap(),
            [1, 2, 3, 4]
        );
    }
    #[test]
    fn audio_drop_keeps_a_gap_marker_when_the_control_queue_is_full() {
        let control_queue = Arc::new(ArrayQueue::new(1));
        control_queue
            .push(IncidentEvent::Finding {
                attempt_id: "control-full".to_string(),
                stage: Stage::Vault,
                code: "marker".to_string(),
                message: "marker".to_string(),
                severity: "warning",
                recoverability: Recoverability::None,
            })
            .unwrap();
        let audio_queue = Arc::new(ArrayQueue::new(1));
        audio_queue
            .push(IncidentEvent::AudioChunk {
                attempt_id: "audio-full".into(),
                sequence: 0,
                bytes: Bytes::from_static(&[1, 2]),
                duration_ms: 1,
                is_final: false,
            })
            .unwrap();
        let audio_gap_queue = Arc::new(ArrayQueue::new(AUDIO_GAP_QUEUE_CAPACITY));
        let (shutdown_tx, _shutdown_rx) = std::sync::mpsc::sync_channel(1);
        let vault = AsyncIncidentVault {
            control_queue,
            audio_queue,
            audio_gap_queue: audio_gap_queue.clone(),
            health: Arc::new(HealthState {
                available: AtomicBool::new(true),
                degraded: AtomicBool::new(false),
                control_dropped: AtomicU64::new(0),
                audio_dropped: AtomicU64::new(0),
                last_error: Mutex::new(None),
            }),
            shutdown_tx,
            worker_thread: std::thread::current(),
        };

        assert!(matches!(
            vault.try_emit(IncidentEvent::AudioChunk {
                attempt_id: "must-be-gapped".into(),
                sequence: 1,
                bytes: Bytes::from_static(&[3, 4]),
                duration_ms: 1,
                is_final: true,
            }),
            super::model::EmitOutcome::Dropped(super::model::DropReason::QueueFull)
        ));
        assert_eq!(
            audio_gap_queue.pop().as_deref(),
            Some("must-be-gapped"),
            "gap completeness must not depend on free control-queue capacity"
        );
    }

    #[test]
    fn audio_gap_before_first_written_chunk_marks_artifact_gapped() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "gap-before-open", true);
        writer
            .process(IncidentEvent::AudioGap {
                attempt_id: "gap-before-open".to_string().into(),
            })
            .unwrap();
        writer
            .process(IncidentEvent::AudioChunk {
                attempt_id: "gap-before-open".to_string().into(),
                sequence: 1,
                bytes: Bytes::from_static(&[3, 4]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "gap-before-open".to_string(),
                outcome: TerminalOutcome::Failed,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        let completeness: String = open_database(&paths).unwrap().query_row(
            "SELECT artifact_completeness FROM recovery_artifacts WHERE attempt_id='gap-before-open' AND artifact_kind='audio_pcm'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(completeness, "gapped");
    }

    #[test]
    fn restart_indexes_unsealed_audio_as_truncated_recovery_material() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        {
            let connection = open_database(&paths).unwrap();
            let mut writer = VaultWriter::new(connection, paths.clone());
            start(&mut writer, "crash-audio", true);
            writer
                .process(IncidentEvent::AudioChunk {
                    attempt_id: "crash-audio".to_string().into(),
                    sequence: 0,
                    bytes: Bytes::from_static(&[1, 2, 3, 4]),
                    duration_ms: 1,
                    is_final: false,
                })
                .unwrap();
            writer.flush_open_artifacts().unwrap();
        }
        let connection = open_database(&paths).unwrap();
        let mut recovered = VaultWriter::new(connection, paths.clone());
        recovered.recover_interrupted().unwrap();

        let read = open_database(&paths).unwrap();
        let outcome: String = read
            .query_row(
                "SELECT terminal_outcome FROM voice_attempts WHERE attempt_id='crash-audio'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(outcome, "interrupted");
        let indexed: i64 = read.query_row(
            "SELECT COUNT(*) FROM recovery_artifacts WHERE attempt_id='crash-audio' AND artifact_kind='audio_pcm' AND artifact_completeness='truncated'",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(indexed, 1);
        assert_eq!(
            std::fs::read(paths.artifacts.join("crash-audio.pcm")).unwrap(),
            [1, 2, 3, 4]
        );
    }

    #[test]
    fn restart_discards_audio_without_persisted_audio_authorization() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        let mut text_only = policy(true);
        text_only.save_audio = false;
        writer
            .process(IncidentEvent::AttemptStarted {
                attempt_id: "text-only".to_string(),
                runtime_session_id: 1,
                started_at_utc_ms: 1000,
                app_version: "test".to_string(),
                app_name: None,
                app_title: None,
                policy: text_only,
            })
            .unwrap();
        start(&mut writer, "already-succeeded", true);
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "already-succeeded".to_string(),
                outcome: TerminalOutcome::Succeeded,
                history_committed: true,
                discard_recovery_material: true,
                ended_at_utc_ms: 2000,
            })
            .unwrap();
        std::fs::write(paths.artifacts.join("text-only.pcm.part"), [1, 2]).unwrap();
        std::fs::write(paths.artifacts.join("orphan.pcm.part"), [3, 4]).unwrap();
        std::fs::write(paths.artifacts.join("already-succeeded.pcm.part"), [5, 6]).unwrap();

        writer.recover_interrupted().unwrap();

        for id in ["text-only", "orphan", "already-succeeded"] {
            assert!(!paths.artifacts.join(format!("{id}.pcm.part")).exists());
            assert!(!paths.artifacts.join(format!("{id}.pcm")).exists());
        }
        let count: i64 = open_database(&paths)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM recovery_artifacts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn emergency_import_retains_malformed_lines_after_importing_valid_records() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        std::fs::write(
            &paths.emergency,
            "not-json\n{\"occurredAtUtcMs\":1000,\"message\":\"panic marker\"}\n",
        )
        .unwrap();

        writer.recover_interrupted().unwrap();

        assert_eq!(
            std::fs::read_to_string(&paths.emergency).unwrap(),
            "not-json\n"
        );
        let imported: i64 = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM incident_findings
                 WHERE reason_code='runtime_panic' AND localized_message='panic marker'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(imported, 1);
    }

    #[test]
    fn duplicate_attempt_start_cannot_escalate_an_unconsented_policy() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        start(&mut writer, "policy-escalation", false);
        start(&mut writer, "policy-escalation", true);
        writer
            .process(IncidentEvent::AudioChunk {
                attempt_id: "policy-escalation".to_string().into(),
                sequence: 0,
                bytes: Bytes::from_static(&[9, 9]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        writer
            .process(IncidentEvent::FinalTranscript {
                attempt_id: "policy-escalation".to_string(),
                text: "must not persist".to_string(),
                monotonic_us: 1,
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "policy-escalation".to_string(),
                outcome: TerminalOutcome::Failed,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        assert_eq!(std::fs::read_dir(&paths.artifacts).unwrap().count(), 0);
        let content_enabled: i64 = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT content_enabled FROM voice_attempts WHERE attempt_id='policy-escalation'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(content_enabled, 0);
    }

    #[test]
    fn duplicate_start_after_writer_restart_cannot_upgrade_content_subpermissions() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        let mut restricted = policy(true);
        restricted.save_audio = false;
        restricted.save_text = false;
        writer
            .process(IncidentEvent::AttemptStarted {
                attempt_id: "subpermission-restart".to_string(),
                runtime_session_id: 1,
                started_at_utc_ms: 1000,
                app_version: "test".to_string(),
                app_name: None,
                app_title: None,
                policy: restricted,
            })
            .unwrap();
        drop(writer);

        let connection = open_database(&paths).unwrap();
        let mut restarted = VaultWriter::new(connection, paths.clone());
        start(&mut restarted, "subpermission-restart", true);
        restarted
            .process(IncidentEvent::AudioChunk {
                attempt_id: "subpermission-restart".into(),
                sequence: 0,
                bytes: Bytes::from_static(&[7, 8]),
                duration_ms: 1,
                is_final: true,
            })
            .unwrap();
        restarted
            .process(IncidentEvent::FinalTranscript {
                attempt_id: "subpermission-restart".to_string(),
                text: "must remain metadata only".to_string(),
                monotonic_us: 1,
            })
            .unwrap();
        restarted
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "subpermission-restart".to_string(),
                outcome: TerminalOutcome::Failed,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 2000,
            })
            .unwrap();

        assert_eq!(std::fs::read_dir(&paths.artifacts).unwrap().count(), 0);
        let persisted: (i64, i64) = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT audio_capture_authorized,text_capture_authorized
                 FROM voice_attempts WHERE attempt_id='subpermission-restart'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(persisted, (0, 0));
    }
    #[test]
    fn retention_cleanup_removes_every_attempt_owned_row() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let connection = open_database(&paths).unwrap();
        let mut writer = VaultWriter::new(connection, paths.clone());
        let mut expiring_policy = policy(true);
        expiring_policy.retention_days = 0;
        writer
            .process(IncidentEvent::AttemptStarted {
                attempt_id: "expired".to_string(),
                runtime_session_id: 7,
                started_at_utc_ms: 1,
                app_version: "test".to_string(),
                app_name: None,
                app_title: None,
                policy: expiring_policy,
            })
            .unwrap();
        writer
            .process(IncidentEvent::StageChanged {
                attempt_id: "expired".to_string(),
                stage: Stage::Asr,
                outcome: StageOutcome::Failed,
                reason_code: Some("asr_timeout".to_string()),
                monotonic_us: 10,
            })
            .unwrap();
        writer
            .process(IncidentEvent::Finding {
                attempt_id: "expired".to_string(),
                stage: Stage::Asr,
                code: "asr_timeout".to_string(),
                message: "timeout".to_string(),
                severity: "error",
                recoverability: Recoverability::Audio,
            })
            .unwrap();
        writer
            .process(IncidentEvent::Metric {
                attempt_id: "expired".to_string(),
                name: "asr_latency_ms",
                value: 10.0,
                unit: "milliseconds",
            })
            .unwrap();
        writer
            .process(IncidentEvent::AttemptEnded {
                attempt_id: "expired".to_string(),
                outcome: TerminalOutcome::Failed,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 1,
            })
            .unwrap();
        writer.maintenance().unwrap();

        let read = open_database(&paths).unwrap();
        for table in [
            "voice_attempts",
            "attempt_stages",
            "incident_findings",
            "incident_events",
            "diagnostic_metrics",
            "recovery_artifacts",
        ] {
            let sql = format!("SELECT COUNT(*) FROM {table} WHERE attempt_id='expired'");
            let count: i64 = read.query_row(&sql, [], |row| row.get(0)).unwrap();
            assert_eq!(count, 0, "expired rows remain in {table}");
        }
    }

    #[test]
    fn audio_try_emit_p99_stays_below_fifty_microseconds() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let vault = super::AsyncIncidentVault::start(paths).unwrap();
        let attempt_id = std::sync::Arc::<str>::from("audio-p99");
        let _ = vault.try_emit(IncidentEvent::AttemptStarted {
            attempt_id: attempt_id.to_string(),
            runtime_session_id: 10,
            started_at_utc_ms: 1000,
            app_version: "test".to_string(),
            app_name: None,
            app_title: None,
            policy: policy(false),
        });

        for sequence in 0..128 {
            let _ = vault.try_emit(IncidentEvent::AudioChunk {
                attempt_id: attempt_id.clone(),
                sequence,
                bytes: Bytes::from_static(&[1, 2, 3, 4]),
                duration_ms: 1,
                is_final: false,
            });
        }

        let mut elapsed = Vec::with_capacity(4096);
        for sequence in 128..4224 {
            let started = std::time::Instant::now();
            let outcome = vault.try_emit(IncidentEvent::AudioChunk {
                attempt_id: attempt_id.clone(),
                sequence,
                bytes: Bytes::from_static(&[1, 2, 3, 4]),
                duration_ms: 1,
                is_final: false,
            });
            elapsed.push(started.elapsed());
            assert!(matches!(
                outcome,
                super::model::EmitOutcome::Accepted
                    | super::model::EmitOutcome::Dropped(super::model::DropReason::QueueFull)
            ));
        }
        elapsed.sort_unstable();
        let p99 = elapsed[(elapsed.len() * 99) / 100];
        assert!(
            p99 < std::time::Duration::from_micros(50),
            "AudioChunk try_emit P99 was {p99:?}"
        );
        vault.shutdown(std::time::Duration::from_millis(500));
    }

    #[test]
    fn bounded_shutdown_drains_accepted_control_and_audio_events() {
        let temp = TempDir::new().unwrap();
        let paths = test_paths(&temp);
        let vault = super::AsyncIncidentVault::start(paths.clone()).unwrap();
        assert!(matches!(
            vault.try_emit(IncidentEvent::AttemptStarted {
                attempt_id: "shutdown-drain".to_string(),
                runtime_session_id: 9,
                started_at_utc_ms: 1000,
                app_version: "test".to_string(),
                app_name: None,
                app_title: None,
                policy: policy(true),
            }),
            super::model::EmitOutcome::Accepted
        ));
        assert!(matches!(
            vault.try_emit(IncidentEvent::AudioChunk {
                attempt_id: "shutdown-drain".to_string().into(),
                sequence: 0,
                bytes: Bytes::from_static(&[5, 6]),
                duration_ms: 1,
                is_final: true,
            }),
            super::model::EmitOutcome::Accepted
        ));
        assert!(matches!(
            vault.try_emit(IncidentEvent::AttemptEnded {
                attempt_id: "shutdown-drain".to_string(),
                outcome: TerminalOutcome::Failed,
                history_committed: false,
                discard_recovery_material: false,
                ended_at_utc_ms: 2000,
            }),
            super::model::EmitOutcome::Accepted
        ));
        vault.shutdown(std::time::Duration::from_millis(500));

        assert_eq!(
            std::fs::read(paths.artifacts.join("shutdown-drain.pcm")).unwrap(),
            [5, 6]
        );
        let outcome: String = open_database(&paths)
            .unwrap()
            .query_row(
                "SELECT terminal_outcome FROM voice_attempts WHERE attempt_id='shutdown-drain'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(outcome, "failed");
    }
}
