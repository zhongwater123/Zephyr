# ADR-0008：隔离的本地 IncidentVault

- Status: Accepted
- Date: 2026-08-25
- Deciders: Project maintainers
- Supersedes: None
- Superseded by: None

## Context

Voice input can fail during capture, ASR, delivery, or formal-history commit. Recovery must preserve useful local material without adding disk, SQLite, JSON, mutex, or waiting work to the audio/ASR hot path.

## Decision

Keep formal history in `history.db` and introduce `incident.db` plus an artifact directory. The frontend may aggregate both in the history surface, but backend APIs and storage remain separate.

The main flow emits typed events through lock-free bounded queues: control events and audio chunks each use a capacity-64 `ArrayQueue`; audio-drop completeness markers use a separate capacity-64 bounded queue. `try_emit` only performs bounded in-memory pushes and thread wakeup. A dedicated OS thread owns the IncidentVault write-path SQLite connection and artifact handles, catches its own panic, and publishes health without entering the voice business branch. User-initiated read/export/delete operations use short-lived isolated connections.

Content capture requires explicit consent and is independent of `history_enabled`. The per-attempt snapshot persists orthogonal total-content, audio, and text authorizations; legacy rows migrate with both content subpermissions denied. Without consent, app/window identity, audio, partial text, and final text are discarded before storage. Successful delivery deletes recovery material when formal history committed or history was skipped by policy; a formal-history write failure preserves material. Failed material expires after 7 days; content-free rollups expire after 30 days.

V1 is local-only. Diagnostic ZIP generation is user initiated and excludes text, audio, and ordinary logs by default. No retry, reinjection, upload outbox, Sentry, or OpenTelemetry endpoint is introduced.

## Consequences

- Formal-history and hotword semantics remain unchanged.
- Recovery can survive history write failures and application restart.
- Storage is protected by the current Windows user ACL; metadata records `encryption_version=0` for a future migration.
- Binary audio crosses IPC as bytes, never JSON/base64.
- Artifact paths are constrained to one filename, reads verify SHA-256, and a failed deletion keeps its database index retryable.
- Crash recovery indexes only persisted audio-authorized `.pcm.part` files; unauthorized/orphan files are removed. Panic emergency lines are redacted, and malformed or failed imports remain for a later retry.
- Diagnostic ZIP fields are allowlisted. Text, audio, logs, and target-application context are never implicitly coupled.
