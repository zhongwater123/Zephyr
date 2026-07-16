# Volcengine ASR Provider

Zephyr currently implements Volcengine streaming ASR over raw WebSocket.

## Default Endpoint

```text
wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async
```

This is the optimized bidirectional streaming endpoint.

## Request Shape

Audio is captured from the default microphone, chunked around 200ms, and sent as PCM.

The provider sends:

- a full client request frame with JSON metadata
- multiple audio-only request frames
- a final negative sequence audio frame

WebSocket frames use binary opcode.

## Default Request Parameters

```json
{
  "audio": {
    "format": "pcm",
    "codec": "raw",
    "rate": 16000,
    "bits": 16,
    "channel": 1
  },
  "request": {
    "model_name": "bigmodel",
    "enable_nonstream": true,
    "enable_itn": true,
    "enable_punc": true,
    "enable_ddc": false,
    "enable_accelerate_text": true,
    "show_utterances": true,
    "result_type": "full",
    "end_window_size": 800,
    "force_to_speech_time": 1000
  }
}
```

Some values can be changed in the recognition behavior settings.

## Auth Modes

Newer console mode:

```text
X-Api-Key
X-Api-Resource-Id
X-Api-Request-Id
```

Older console mode:

```text
X-Api-App-Key
X-Api-Access-Key
X-Api-Resource-Id
X-Api-Request-Id
```

The app stores all credentials in the OS keyring.

## Debugging

The provider logs request IDs and service log IDs when available.

It should not log API keys, raw audio, or recognized text.

Common failure classes:

- `401 Unauthorized`: wrong auth mode or invalid key.
- Empty audio / no transcript: often caused by short press, no microphone input, or silence.
- Timeout waiting for final text: provider did not return a final package within the app limit.

See [troubleshooting.md](troubleshooting.md).
