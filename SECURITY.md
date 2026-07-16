# Security

## Supported Versions

This project is pre-1.0. Security fixes are expected to land on the main development line unless a release branch is created later.

## Reporting A Vulnerability

Please report security issues privately to the project maintainer instead of opening a public issue with exploit details.

If no private contact has been published yet, open a minimal public issue that says a security report is available, without including secrets, keys, transcripts, or exploit details.

## Secrets

Do not commit API keys, app keys, access keys, `.env` files, local databases, or logs.

Zephyr stores service credentials in the operating system keyring. The app config file should not contain:

- Volcengine API Key
- Volcengine App Key
- Volcengine Access Key
- DeepSeek API Key

## Local Data

The local SQLite database can contain recognized text, foreground app names, window titles, hotwords, and context summaries. Treat it as sensitive user data.

## Logging

Logs should not include raw audio, final transcript text, history body text, or secrets. Provider request IDs and service log IDs may be logged for debugging.

Before sharing logs publicly, review them manually and redact any sensitive content.
