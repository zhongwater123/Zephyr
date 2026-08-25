# IncidentVault event dictionary

Core fields are orthogonal. Stable `reason_code` values are separate from localized messages. Generic `status`, `data`, `content`, and `error` columns are not used.

## Outcomes

- terminal: `succeeded | failed | cancelled | interrupted`
- stage: `not_started | running | succeeded | failed | cancelled | skipped_by_policy | unknown`
- artifact completeness: `complete | truncated | gapped`
- recoverability: `none | partial_text | final_text | audio | text_and_audio`

## Typed events

| Event | Required attributes | Content policy |
| --- | --- | --- |
| attempt_started | attempt_id, runtime_session_id, UTC start, policy snapshot | app identity stored only with consent |
| stage_changed | stage, stage_outcome, monotonic_us, optional reason_code | no user content |
| audio_chunk | sequence, duration_ms, final marker, Bytes | artifact only with consent |
| audio_gap | attempt_id | no user content |
| partial_checkpoint | confirmed_chars, monotonic_us | text artifact only with consent; max every 500ms |
| final_transcript | monotonic_us | text artifact only with consent |
| finding | stage, reason_code, severity, recoverability, localized_message | message is bounded |
| metric | fixed name, numeric value, unit | no user content |
| attempt_ended | terminal_outcome, history_committed, discard_recovery_material, UTC end | `history_committed` records formal-history result; the orthogonal discard flag controls artifact deletion |
| frontend_failure | source, reason_code, bounded/redacted message and stack | rate limited and deduplicated |

JSON is limited to versioned event attributes produced by typed encoders. Artifact bytes and transcript text never enter event JSON.
## Persistence and validation rules

- `AttemptPolicy` is snapshotted once. Duplicate starts and restarts may only narrow persisted `content_enabled`, `audio_capture_authorized`, and `text_capture_authorized`; they cannot upgrade consent.
- `AudioChunk` carries `Bytes` plus `Arc<str>` attempt identity. A dropped chunk increments health counters and enqueues a bounded `audio_gap` marker independent of control-queue capacity.
- Partial and final text are artifact files, not event attributes. The provider's canonical final is emitted once before terminal success.
- Artifact filenames must be a single normal path component. Text/audio reads verify the recorded SHA-256 before returning bytes.
- Finding messages, frontend failures, panic messages, stacks, URL query strings, credential-like lines, and local paths pass through the shared bounded redactor before persistence/export.
