# Architecture

Zephyr is a Windows helper input tool. It keeps the realtime voice path in Rust and uses the Tauri WebView for settings, status, and visual feedback.

## Runtime Flow

```text
global shortcut
-> Rust hotkey coordinator
-> cpal microphone stream
-> 200ms PCM audio chunks
-> Volcengine WebSocket ASR provider
-> transcript events
-> preview state
-> preinput overlay events
-> final clipboard paste
-> optional history write
-> optional hotword organization
```

## Main Modules

- `state.rs`: voice state machine for idle, recording, transcribing, pasting, disabled, and error states.
- `hotkey.rs`: global shortcut registration and the press/release session lifecycle.
- `audio.rs`: microphone capture, PCM chunking, and WAV utilities used by tests/debug paths.
- `provider.rs`: Volcengine WebSocket ASR binary protocol implementation.
- `preview.rs`: latest transcript preview text and confirmed character tracking.
- `overlay.rs`: preinput overlay window, session IDs, payload sequencing, positioning, and event coalescing.
- `inject.rs`: final text paste through clipboard replacement and simulated `Ctrl+V`.
- `config.rs`: JSON config plus OS keyring secret storage.
- `history.rs`: local SQLite transcript history.
- `hotwords.rs`: manual hotwords, DeepSeek organization, profile/app context, and ASR hint composition.
- `shortcut.rs`: shortcut parsing, normalization, and reserved-key validation.

## Preview Mode

Preview Mode is the active input model.

ASR partial results update the floating preinput overlay. The target application is not edited until the user releases the shortcut and the final transcript is ready.

The client treats provider `result.text` as the text authority. It does not merge partial hypotheses, append segments, or edit the target text box incrementally.

## Preinput Overlay

The overlay is a separate Tauri webview window labelled `preinput`.

It is:

- always on top
- frameless
- transparent at the native window level
- non-focus-stealing
- positioned near the current foreground monitor

Payloads include `session_id` and `seq` so old ASR updates cannot revive or overwrite a new overlay session.

## Local Storage

Non-secret config is stored in `config.json` under the OS app config directory.

History and hotword state share a local SQLite database named `history.db`.

Secrets are stored in the OS keyring with service name `gy-typing`.

## External Services

- Volcengine handles streaming ASR.
- DeepSeek or a compatible chat completion endpoint can organize hotwords and contexts when enabled.

The app should never send raw audio to DeepSeek.
