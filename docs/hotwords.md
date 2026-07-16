# Hotwords And Context

Zephyr can inject hotwords and short context into the ASR request.

## Manual Hotwords

Manual hotwords are edited in the hotword library UI.

They are stored locally in `history.db` and are prioritized before agent-generated hotwords.

## Agent Hotwords

The optional hotword agent uses a DeepSeek-compatible chat completion API.

Default settings:

```text
base_url: https://api.deepseek.com
model:    deepseek-v4-flash
```

The DeepSeek API key is stored in the OS keyring.

## Automatic Organization

Automatic organization is disabled by default.

When enabled, the app can organize hotwords after every 20 new successful history records.

Users can also trigger organization manually with the "整理热词" button.

## What Is Sent To DeepSeek

When organization runs, the app may send:

- recent history text that has not yet been organized
- current manual hotwords
- current agent hotwords
- app names and contexts

The app should not send raw audio to DeepSeek.

## ASR Injection

At voice session start, the app composes ASR hints from:

1. manual hotwords
2. agent hotwords
3. current foreground app context
4. profile context

The resulting hint is placed into `request.corpus.context` as a JSON string compatible with Volcengine ASR.

The app keeps the hint conservative to avoid exceeding streaming ASR context limits.

## Local Data

Hotwords, profile context, app contexts, organization metadata, and recent errors are stored locally in SQLite.

See [PRIVACY.md](../PRIVACY.md) for data handling details.
