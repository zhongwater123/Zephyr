# Architecture

> 本页保留为稳定的旧链接入口。可增量维护的架构文档、C4 图、arc42-Lean、代码地图和 ADR 已迁移至 [Architecture Knowledge Base](architecture/README.md)。

下面内容是当前实现的概览；涉及变更影响与决策依据时，请以新架构知识库为导航入口。

Zephyr is a Windows helper input tool. It keeps the realtime voice path in Rust and uses the Tauri WebView for settings, status, and visual feedback.

## Runtime Flow

```text
Tauri/WebView
-> typed IPC client
-> thin domain commands
-> AppServices
   |-> ConfigService
   |-> HistoryRepository / HotwordRepository
   |-> ProviderService / CredentialStore
   |-> NativeConfirmation
   `-> VoiceSessionController
       |-> StreamingPipeline
       `-> DeliveryService
-> JSON / SQLite / Windows Credential Manager / Win32 adapters
```

The global shortcut adapter converts native press/release callbacks into bounded controller events.
The controller serializes session state, starts the microphone/provider pipeline, and delegates all
text side effects to `DeliveryService`.

## Backend Boundaries

- `lib.rs`: startup composition, managed state, handler registration, and application launch.
- `commands/`: window-label validation, DTO translation, service calls, and structured error mapping.
- `services.rs`: `AppServices`, the single-owner `ConfigService`, and credential-safe provider construction.
- `repositories.rs`: config, credential, history, hotword, and Agent interfaces plus production adapters.
- `platform.rs` and `platform/tray.rs`: native confirmations and tray/window startup behavior.
- `voice_controller.rs`: single-owner session loop and capacity-16 fail-closed control channel.
- `streaming_pipeline.rs`: provider task, latest-value preview relay, overflow watcher, and lifecycle.
- `delivery.rs`: target/text validation, injection, Pending fallback, and commit ordering.

- `state.rs`: voice state machine for idle, recording, transcribing, pasting, disabled, and error states.
- `session.rs`: single-owner session coordinator, cancellation ownership, pending output queue, and metrics.
- `hotkey.rs` and `low_level_hook.rs`: unified shortcut ownership, persistent `RegisterHotKey`
  preview reservations, best-effort `WH_KEYBOARD_LL` exclusive matching, transactional replacement,
  self-injected input filtering, and explicit shutdown cleanup.
- `audio.rs`: microphone capture and the bounded 32-slot PCM queue with explicit overflow signalling.
- `provider.rs`: provider-neutral session types, normalized errors, and streaming trait.
- `provider_model.rs`: active supplier option specs, validation, defaults, and option-to-request mapping.
- `provider/volcengine.rs`: Volcengine runtime profile, authentication headers, WebSocket protocol, and wire codec.
- `preview.rs`: latest transcript preview text and confirmed character tracking.
- `overlay.rs`: preinput overlay window, session IDs, payload sequencing, positioning, and event coalescing.
- `inject.rs`: UTF-16 `SendInput` by default; explicit per-application OLE clipboard compatibility.
- `target.rs`: HWND/PID/process-creation identity checks, text validation, and the TTL-bound pending queue.
- `config.rs`: config models, atomic JSON primitives, endpoint trust rules, and credential primitives.
- `history.rs`: local SQLite transcript history.
- `hotwords.rs`: hotword domain rules and SQLite operations exposed through repository interfaces.
- `shortcut.rs`: shortcut parsing, normalization, and reserved-key validation.

Each voice session owns an immutable ID, target identity, cancellation signal, bounded audio queue,
and a monotonically increasing session ID. Pausing
the app cancels capture and provider work before the runtime enters the disabled state. Async
completion paths verify that they still own the current session before injecting text or changing
the UI state.

Automatic delivery requires the original HWND to remain foreground and its PID, process creation
time, and executable name to remain unchanged. A mismatch, invalid output, or rejected injection
creates an in-memory pending result (maximum 5, TTL 10 minutes) without history or hotword side
effects. Successful injection is the commit point.

## Frontend Boundaries

`src/main.tsx` selects the window entry and dynamically imports either the main application or the
preinput overlay. This keeps settings, history, and Three.js code out of the preinput bundle.

- `src/ipc/client.ts`: typed clients grouped by config, history, hotwords, pending, shortcut, provider, and preinput.
- `src/app/AppShell.tsx`: layout, global voice state, config snapshot, and global notices.
- `src/features/*`: feature UI and feature-owned form, loading, error, and editing state controllers.
- `src/app/useRevisionedConfigMutation.ts`: revision sequencing, late-response rejection, conflict reload, and error normalization.
- `src/domain.ts`: stable Tauri DTO types and defaults.

The Three.js ASCII field remains asynchronously loaded, so the lightweight preinput window does not
load the visualization bundle.

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

Non-secret config is stored in `config.json` under the OS app config directory. Mutations use
revision compare-and-swap and an atomic sibling-file replacement. The last validated config is kept
as `config.json.bak`; invalid primary and backup files start the runtime disabled.

History and hotword state share a local SQLite database named `history.db`.

Incident recovery is isolated in `incident.db` plus `LocalAppData/gy-typing/incidents/artifacts/`. The voice path only performs bounded lock-free `IncidentSink::try_emit` calls; formal history APIs and schema remain unchanged. The History Dialog aggregates recovery through a separate typed `incidentApi`.

SQLite connections use WAL mode and a bounded busy timeout. Hotword-agent results merge against
the latest stored state so edits made while a network request is running are not overwritten.

Secrets are stored in the OS keyring with service name `gy-typing`.

Credentials are loaded only after the normalized `scheme + host + effective port + purpose` origin
is trusted. Built-in service origins are trusted by default; custom origins require a parented native
Windows confirmation and can be revoked without deleting the keyring secret.

The `main` and `preinput` WebViews use separate capabilities. Sensitive commands validate the caller
window label in Rust, overlay events are targeted to `preinput`, and production builds use a strict
CSP without remote scripts, frames, objects, or base redirects.

## External Services

- Volcengine handles streaming ASR.
- DeepSeek or a compatible chat completion endpoint can organize hotwords and contexts when enabled.

The app should never send raw audio to DeepSeek.
