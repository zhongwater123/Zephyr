# Trigger Latency Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve perceived and real trigger latency for the Windows voice input flow, especially press-to-overlay and press-to-first-partial.

**Architecture:** Keep Preview Mode unchanged. Move visible feedback to the earliest possible point, remove synchronous database work from the hotkey press path, add latency telemetry, and only then evaluate WebSocket preconnection. ASR request parameters are tuned conservatively by enabling first-character acceleration while keeping final recognition behavior intact.

**Tech Stack:** Tauri 2, Rust, cpal, custom WebSocket ASR provider, Preact overlay UI, SQLite hotword/history store.

---

### Task 1: Enable First-Character ASR Acceleration

**Files:**
- Modify: `src-tauri/src/provider.rs`

- [x] **Step 1: Add request parameter test coverage**

Ensure `full_client_request_matches_sauc_demo_framing` asserts:

```rust
assert_eq!(
    payload
        .pointer("/request/enable_accelerate_text")
        .and_then(Value::as_bool),
    Some(true)
);
```

- [x] **Step 2: Enable acceleration in full client request**

In `build_full_client_request`, include:

```rust
"enable_accelerate_text": true,
```

Do not set `accelerate_score` in this task. Keep the first version simple and compare real behavior before tuning aggressiveness.

- [ ] **Step 3: Manual verification**

Run:

```powershell
cd D:\gy-TYPING
npm run tauri dev
```

Expected: app starts, ASR request still succeeds, first partial appears no slower than before.

---

### Task 2: Move Overlay Show To The Beginning Of Hotkey Press

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1: Reorder `handle_pressed`**

Show the preinput overlay immediately after `voice.machine.hotkey_pressed()` succeeds.

Target flow:

```text
hotkey_pressed
→ overlay::show_preinput
→ capture foreground app
→ compose hotword hints
→ start recorder
→ spawn provider
→ spawn transcript relay
→ emit voice state
```

The visible overlay should no longer wait for `capture_foreground_app`, SQLite hotword reads, recorder start, or provider spawn.

- [ ] **Step 2: Preserve error behavior**

If recorder startup fails after the overlay is already visible, update overlay to error state and hide it through the existing reset path.

- [ ] **Step 3: Manual verification**

Press the shortcut once without speaking.

Expected: the overlay appears immediately, even before any ASR text is available.

---

### Task 3: Remove SQLite Reads From The Hotkey Press Path

**Files:**
- Modify: `src-tauri/src/hotwords.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1: Add an in-memory hotword cache**

Create a runtime-owned cache that stores:

```rust
pub struct HotwordCache {
    pub manual_hotwords: Vec<String>,
    pub agent_hotwords: Vec<String>,
    pub profile_context: String,
    pub app_contexts: Vec<AppHotwordContext>,
}
```

Populate it at app startup from SQLite.

- [ ] **Step 2: Update cache after mutations**

Every command that changes hotwords or contexts must update the cache after the database write:

```text
save_manual_hotwords
delete_agent_hotword
promote_agent_hotword
update_profile_context
update_app_context
delete_app_context
organize_hotwords_now
auto organize success
```

- [ ] **Step 3: Compose hints from memory on hotkey press**

Replace hotkey-time SQLite reads with a pure in-memory function:

```rust
hotwords::compose_asr_hints_from_cache(&config, &cache, &app_context)
```

Expected: pressing the shortcut never opens SQLite before recording starts.

- [ ] **Step 4: Manual verification**

Add a manual hotword, save it, then speak a sentence containing it.

Expected: no functional regression; hotword still appears in ASR `corpus.context`.

---

### Task 4: Add Latency Telemetry Without Logging Text

**Files:**
- Create: `src-tauri/src/telemetry.rs`
- Modify: `src-tauri/src/hotkey.rs`
- Modify: `src-tauri/src/provider.rs`

- [ ] **Step 1: Add session timing struct**

Create:

```rust
pub struct VoiceLatencyTrace {
    pressed_at: Instant,
    overlay_shown_at: Option<Instant>,
    recording_started_at: Option<Instant>,
    websocket_connected_at: Option<Instant>,
    full_request_sent_at: Option<Instant>,
    first_audio_sent_at: Option<Instant>,
    first_partial_at: Option<Instant>,
    released_at: Option<Instant>,
    final_at: Option<Instant>,
    pasted_at: Option<Instant>,
}
```

- [ ] **Step 2: Log timing summary only**

On completion or failure, log only millisecond durations:

```text
latency press_to_overlay=12ms press_to_recording=24ms press_to_ws=410ms press_to_first_partial=880ms release_to_paste=320ms
```

Never log recognized text, audio, or API keys.

- [ ] **Step 3: Manual verification**

Run a short voice input.

Expected: terminal prints one timing summary per session.

---

### Task 5: Evaluate WebSocket Preconnection After Telemetry

**Files:**
- Modify only after Task 4 shows WebSocket connect dominates first partial latency.

- [ ] **Step 1: Inspect latency logs**

If `press_to_ws` is consistently high, proceed. If not, do not implement preconnection.

- [ ] **Step 2: Prototype provider warm connection**

Only prototype if Volcengine accepts short idle windows before the full client request.

Candidate behavior:

```text
app idle
→ keep one authenticated websocket ready for a short TTL
→ hotkey press claims connection
→ send full client request immediately
```

- [ ] **Step 3: Fallback safely**

If the warm connection is closed, expired, or errors, fall back to current connect-on-press behavior.

- [ ] **Step 4: Manual verification**

Compare latency telemetry before and after.

Expected: `press_to_first_partial` improves; no increase in ASR errors.

---

### Task 6: Optional ASR Parameter Tuning

**Files:**
- Modify: `src-tauri/src/provider.rs`
- Modify: `src/main.tsx` only if exposing controls

- [ ] **Step 1: Add `accelerate_score` only after baseline testing**

If `enable_accelerate_text: true` helps but first partial still feels slow, add:

```json
"accelerate_score": 5
```

Do not start above 5. Higher values may degrade first-character accuracy.

- [ ] **Step 2: Consider finalization parameters separately**

Only tune these if release-to-final is slow:

```json
"end_window_size": 600
"force_to_speech_time": 800
```

Do not mix these changes with WebSocket preconnection in the same test round.

- [ ] **Step 3: Manual verification**

Use the same spoken phrase across test runs.

Expected: partial text appears faster without obviously worse recognition quality.
