# Configuration

Most configuration is edited in the app settings drawer.

## App Config

Non-secret settings are stored in:

```text
<os config dir>/gy-typing/config.json
```

On Windows this usually resolves under `%APPDATA%`.

## Secrets

Secrets are stored through the OS keyring, not in `config.json`.

Stored secret entries:

- `transcription-api-key`
- `transcription-app-key`
- `transcription-access-key`
- `hotword-agent-api-key`

Keyring service name:

```text
gy-typing
```

## Default ASR Provider

```text
base_url:    wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async
resource_id: volc.bigasr.sauc.duration
model:       bigmodel
language:    zh-CN
auth_mode:   app_access
```

`api_key` auth mode is also supported.

## Recognition Behavior

The app exposes these ASR request switches:

- Punctuation: enabled by default.
- ITN: enabled by default.
- Semantic smoothing: disabled by default.
- First-character acceleration: enabled by default.

Some core Preview Mode settings are intentionally fixed in code:

- `enable_nonstream: true`
- `show_utterances: true`
- `result_type: full`
- `end_window_size: 800`
- `force_to_speech_time: 1000`

## Shortcut

Default shortcut:

```text
Ctrl+Alt+Space
```

The shortcut can be changed in settings. The app validates obvious reserved combinations and attempts a real global shortcut registration before saving.

## History

History is enabled by default.

Each successful final paste can store:

- transcript text
- local timestamp to seconds
- best-effort foreground app name
- best-effort window title
- character count

History can be searched, edited, copied, deleted, or cleared from the UI.

## Hotwords

Hotword injection is enabled by default, but only sends hints when local hotwords or context exist.

Automatic DeepSeek organization is disabled by default. It requires a DeepSeek API key and user opt-in.
