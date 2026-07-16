# Windows AI Voice Input Assistant Design

## Goal

Build a Windows helper input tool that stays lightweight, starts quickly, and lets the user hold a global shortcut to speak. While the shortcut is held, the app streams small audio chunks to a bidirectional ASR provider, receives partial transcript updates, shows those updates in a non-focus-stealing preinput overlay, and pastes the final server transcript into the active application when the shortcut is released.

## Product Shape

The first version is an auxiliary input tool, not a Windows TSF input method. It runs as a Tauri desktop app with a Rust backend and a small WebView settings UI.

Confirmed MVP behavior:

- Hold `Ctrl+Alt+Space` to record.
- Release to stop recording.
- Reject recordings shorter than 300ms.
- Transcribe through ByteDance bidirectional streaming ASR, preferring `bigmodel_async`.
- Split audio into about 200ms packets. This matches the preferred packet size for ByteDance's bidirectional streaming ASR mode.
- Show partial and second-pass text in a floating preinput overlay.
- Paste only the final server transcript through the clipboard and restore the previous text clipboard when possible.
- Show lightweight state messages: idle, recording, transcribing, pasting, disabled, and error.
- Let users edit the global shortcut from the settings UI.
- Store optional local input history after successful final paste.
- Support manual hotwords, app/profile context, and optional DeepSeek-based hotword organization.

## Architecture

The realtime path stays in Rust:

`global shortcut -> recorder -> 200ms PCM chunks -> streaming provider -> transcript events -> preview state -> overlay event -> final paste -> state event`

The frontend is intentionally non-critical for audio and provider flow. It displays settings, status, and the overlay text pushed from Rust. WebView performance should not affect recording, provider transport, or final paste timing.

## Preview Mode

Preview Mode is the only active input mode in the current code path.

- The provider treats `result.text` from the server as the single text authority.
- `TranscriptPreviewState` stores only the latest text, confirmed character count, and last event time.
- The client does not merge utterances into final text and does not append partial hypotheses.
- `utterances` and `definite` are used only to calculate confirmed progress for the overlay.
- The final committed text is the provider's final server packet text.
- If the provider stream closes without a last package, the request fails instead of silently committing a partial result.

Live direct input into the target Windows text field is deferred. It should be treated as a future experiment, likely requiring TSF or a more native composition path. The current MVP intentionally avoids partial-level Backspace or Unicode injection into the target application.

## Modules

- `state.rs`: finite state machine for `Idle`, `Recording`, `Transcribing`, `Pasting`, `Disabled`, and `Error`.
- `hotkey.rs`: registers the global shortcut and coordinates the press/release flow.
- `audio.rs`: records microphone samples with `cpal`, splits PCM into 200ms chunks, and keeps WAV encoding for tests/debug use.
- `provider.rs`: implements ByteDance WebSocket ASR binary protocol and keeps the provider boundary isolated.
- `preview.rs`: tracks the latest authoritative preview text and confirmed progress.
- `overlay.rs`: owns the preinput window, positioning, payload sequence numbers, and coalesced overlay events.
- `inject.rs`: pastes final text using clipboard replacement plus simulated `Ctrl+V`.
- `config.rs`: stores non-secret settings in JSON and stores the API key through the OS keyring.
- `history.rs`: stores successful final transcripts, timestamps, and best-effort foreground app metadata in SQLite.
- `hotwords.rs`: stores manual/agent hotwords and context, optionally organizes history with DeepSeek, and builds ASR hints.
- `shortcut.rs`: parses, normalizes, validates, and dynamically registers user-selected shortcuts.

## Deferred Work

- Rewrite, translation, and completion modes.
- Full multi-format clipboard snapshot and restore.
- TSF input method integration.
- Offline or hybrid local recognition.
- Live direct composition in the target input field.
