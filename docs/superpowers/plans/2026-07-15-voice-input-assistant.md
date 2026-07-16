# Windows AI Voice Input Assistant Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows Tauri + Rust voice input assistant that records while a shortcut is held, streams audio chunks to cloud ASR, shows low-latency preview text, and pastes the final server transcription on release.

**Architecture:** Rust owns the realtime path: global shortcut, recording, 200ms audio chunking, streaming transcription provider, preview state, overlay events, and final paste injection. The Preact frontend owns settings, status display, and overlay rendering only.

**Tech Stack:** Tauri 2, Rust, Preact, TypeScript, Vite, `tauri-plugin-global-shortcut`, `cpal`, `hound`, `keyring`, `arboard`, `windows`.

---

### Task 1: Project Skeleton

**Files:**
- Create: `package.json`, `index.html`, `tsconfig.json`, `vite.config.ts`
- Create: `src/main.tsx`, `src/styles.css`
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`

- [x] Add Vite + Preact + TypeScript frontend scaffolding.
- [x] Add minimal Tauri 2 Rust crate scaffolding.
- [x] Add `.gitignore` for generated dependencies and build outputs.

### Task 2: Rust Core Modules

**Files:**
- Create: `src-tauri/src/state.rs`
- Create: `src-tauri/src/audio.rs`
- Create: `src-tauri/src/provider.rs`
- Create: `src-tauri/src/config.rs`
- Create: `src-tauri/src/inject.rs`

- [x] Add tests for repeated key press, release transitions, short recording rejection, and failure reset.
- [x] Add WAV encoding test that checks RIFF/WAVE headers.
- [x] Add 200ms chunking test that marks only the final chunk as final.
- [x] Add mock streaming provider tests for partial/final events and empty audio cases.
- [x] Add config tests to verify API key is not serialized.
- [x] Implement the minimal code for those tests.

### Task 3: Tauri Integration

**Files:**
- Create: `src-tauri/src/hotkey.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `src-tauri/capabilities/default.json`

- [x] Register `Ctrl+Alt+Space` through `tauri-plugin-global-shortcut`.
- [x] On press, enter recording and start `cpal`.
- [x] On release, split PCM into 200ms chunks, call `MockProvider`, emit partial transcript state, paste the final transcript, and emit `voice_state_changed`.
- [x] Add commands: `get_config`, `save_config`, `set_enabled`, `test_provider`.

### Task 4: Frontend UI

**Files:**
- Modify: `src/main.tsx`
- Modify: `src/styles.css`

- [x] Display current voice state from `voice_state_changed`.
- [x] Provide config fields for base URL, model, language, and API key.
- [x] Add enable/pause and provider test controls.

### Task 5: Verification

- [x] Run `npm install`.
- [x] Run `npm run build`.
- [ ] Run `cd src-tauri && cargo test`.
- [ ] Run `npm run tauri build`.

Cargo-based verification is blocked until Rust and Cargo are installed on the machine.

### ByteDance Streaming ASR Notes

- Preferred mode: bidirectional streaming optimized endpoint `wss://openspeech.bytedance.com/api/v3/sauc/bigmodel_async`.
- Default packet target: 200ms PCM chunks.
- Audio request WebSocket frames must use binary opcode and the official binary payload format.
- `enable_nonstream` is enabled by default so the overlay can show fast streaming text and second-pass definite progress.
- The implementation treats server `result.text` as the only authoritative preview/final text. Client-side utterance merging and partial appending are intentionally avoided because they caused repeated long-form transcripts.
- The provider should only complete successfully after receiving the protocol last package. A closed connection without a last package is an error, not a usable final transcript.
- Preview Mode is the current product path. Live direct input into the target Windows text field is deferred and should not remain as half-active Backspace/Unicode injection code.
