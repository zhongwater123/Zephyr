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

API keys are stored with the operating system keyring service name `gy-typing`. The JSON config file should not contain API keys, app keys, access keys, or DeepSeek keys.

## What May Be Sent To External Services

Audio is streamed to the configured ASR provider during a voice input session.

When hotword organization is enabled or manually triggered, selected history text and existing hotword/context data may be sent to the configured DeepSeek-compatible chat completion endpoint.

The app should not send audio to DeepSeek. DeepSeek is used only for text-based hotword and context organization.

## What Is Not Stored

The app does not intentionally store raw microphone audio.

The app should not log API keys, audio content, or recognized text. Debug logs may contain provider status, request IDs, timing, protocol metadata, and service error payloads.

## User Controls

- History can be turned on or off in settings.
- Existing history can be edited, copied, deleted, or cleared.
- Hotword injection can be turned on or off.
- Automatic hotword organization is off by default and requires a DeepSeek API key.

## Third-Party Services

This project does not control the privacy practices of Volcengine, DeepSeek, or any compatible service you configure. Review the provider's own terms and privacy policy before sending personal or sensitive data.
