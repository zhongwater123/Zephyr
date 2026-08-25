# Privacy

Zephyr is a local desktop application, but it can send data to cloud services that you configure.

## What The App Processes

- Microphone audio while the voice shortcut is held.
- Transcription results returned by the ASR provider.
- Local history records when history is enabled.
- Hotwords, profile context, and app-specific context when hotword features are used.
- Foreground app name and window title when history capture succeeds.

## What Is Stored Locally

Non-secret settings are stored in the app config directory under `gy-typing/config.json`.

History and hotword state are stored in a local SQLite database named `history.db` in the same app config area.

When the user explicitly enables “异常恢复”, abnormal voice attempts are indexed in a separate local `incident.db`. Failed-session PCM audio and partial/final text may be stored for 7 days under `LocalAppData/gy-typing/incidents/artifacts/`. Recovery is independent of the formal-history switch. After successful delivery, recovery material is deleted when formal history committed or history was disabled; if formal-history writing fails, material is retained for recovery.

Diagnostic success/failure rollups contain no user content and are retained for up to 30 days. Recovery artifacts use the current Windows user's directory permissions; artifact encryption is not implemented in this version (`encryption_version=0`).

Each attempt persists separate content, audio, and text authorization bits. Upgraded databases default new audio/text subpermissions to denied. On restart, only a matching audio-authorized unfinished attempt may seal `.pcm.part`; unauthorized and orphan files are removed. Artifact paths are restricted to the incident artifact directory, reads verify SHA-256, and failed deletion keeps the local index so deletion can be retried.

API keys are stored with the operating system keyring service name `gy-typing`. The JSON config file should not contain API keys, app keys, access keys, or DeepSeek keys.

## What May Be Sent To External Services

Audio is streamed to the configured ASR provider during a voice input session.

When hotword organization is enabled or manually triggered, selected history text and existing hotword/context data may be sent to the configured DeepSeek-compatible chat completion endpoint.

The app should not send audio to DeepSeek. DeepSeek is used only for text-based hotword and context organization.

## What Is Not Stored

Before explicit recovery consent, the app does not store microphone audio, partial/final transcripts, foreground app names, or window titles in IncidentVault.

Ordinary logs should not contain API keys, audio content, or recognized text. Debug logs may contain provider status, request IDs, timing, protocol metadata, and service error payloads. Ordinary logs are not automatically imported into IncidentVault.

Finding messages, frontend exceptions, panic messages/backtraces, URL queries, credential-like lines, and local paths are passed through a shared bounded redactor before IncidentVault persistence or diagnostic export.

On Windows, exclusive shortcut mode observes keyboard events through a user-session low-level hook only to compare the configured modifier set and main key. Ordinary keys and typed content are not stored or logged. Diagnostics record only backend lifecycle, registration failures, mode transitions, and aggregate dropped-control-event counts.

IncidentVault does not upload data. Diagnostic ZIP files are generated locally only after user action; text, audio, and log excerpts are excluded by default and require separate choices. Selecting transcript text adds only partial/final transcript fields and does not implicitly add target application or window context. Optional ordinary-log excerpts are limited to 256KB of recent local log tails and are redacted.

## User Controls

- History can be turned on or off in settings.
- Existing history can be edited, copied, deleted, or cleared.
- Hotword injection can be turned on or off.
- Exception recovery can be enabled or disabled independently of history. Upgraded users start unconsented.
- Recovery records can be copied, played, exported, deleted, pinned, or returned to automatic expiry.
- Automatic hotword organization is off by default and requires a DeepSeek API key.

## Third-Party Services

This project does not control the privacy practices of Volcengine, DeepSeek, or any compatible service you configure. Review the provider's own terms and privacy policy before sending personal or sensitive data.
