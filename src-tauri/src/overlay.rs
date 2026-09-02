use serde::Serialize;
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "windows")]
use std::time::Duration;
use std::time::Instant;
use tauri::AppHandle;

#[cfg(target_os = "windows")]
mod window;

#[cfg(target_os = "windows")]
pub use window::setup_preinput_window;

pub const PREINPUT_LABEL: &str = "preinput";
#[cfg(target_os = "windows")]
const PREINPUT_EMIT_COALESCE_MS: u64 = 30;
static PREINPUT_STORE: OnceLock<Mutex<PreInputStore>> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreInputPayload {
    pub session_id: u64,
    pub seq: u64,
    pub text: String,
    pub state: PreInputState,
    pub confirmed_chars: Option<usize>,
    pub message: Option<String>,
}

#[derive(Debug, Default)]
struct PreInputStore {
    current: Option<PreInputPayload>,
    current_session_id: u64,
    closed_session_id: u64,
    next_seq: u64,
    last_emit_at: Option<Instant>,
    delayed_emit_scheduled: bool,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PreInputState {
    Starting,
    Recording,
    Transcribing,
    Finalizing,
    Dismissing,
    Error,
}

#[cfg(target_os = "windows")]
pub fn show_preinput(app: &AppHandle, payload: PreInputPayload) {
    let Some(payload) = store_preinput_payload(payload) else {
        return;
    };
    if let Err(error) = window::show(app, &payload) {
        log::warn!("failed to create preinput overlay window: {error}");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn show_preinput(_app: &AppHandle, _payload: PreInputPayload) {
    log::warn!("preinput overlay presentation is unsupported on this platform");
}

#[cfg(target_os = "windows")]
pub fn update_preinput(app: &AppHandle, payload: PreInputPayload) {
    if let Some(payload) = store_preinput_payload(payload) {
        emit_update_coalesced(app, payload);
    }
}

#[cfg(not(target_os = "windows"))]
pub fn update_preinput(_app: &AppHandle, _payload: PreInputPayload) {
    log::warn!("preinput overlay presentation is unsupported on this platform");
}

pub fn begin_preinput_session() -> u64 {
    if let Ok(mut store) = preinput_store().lock() {
        store.current_session_id = store.current_session_id.saturating_add(1);
        store.next_seq = 0;
        store.current = None;
        store.last_emit_at = None;
        store.delayed_emit_scheduled = false;
        return store.current_session_id;
    }
    0
}

#[allow(dead_code)]
pub fn current_preinput_session_id() -> u64 {
    preinput_store()
        .lock()
        .map(|store| store.current_session_id)
        .unwrap_or(0)
}

pub fn hide_preinput_for_session(app: &AppHandle, session_id: u64) {
    let Some(payload) = clear_current_preinput_payload(session_id) else {
        return;
    };
    #[cfg(target_os = "windows")]
    {
        window::hide(app, &payload);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, payload);
        log::warn!("preinput overlay presentation is unsupported on this platform");
    }
}

pub fn current_preinput_payload() -> Option<PreInputPayload> {
    preinput_store()
        .lock()
        .ok()
        .and_then(|store| store.current.clone())
}

#[cfg(target_os = "windows")]
fn store_preinput_payload(mut payload: PreInputPayload) -> Option<PreInputPayload> {
    if let Ok(mut store) = preinput_store().lock() {
        if payload.session_id <= store.closed_session_id {
            return None;
        }
        if payload.session_id < store.current_session_id {
            return None;
        }
        if payload.session_id > store.current_session_id {
            store.current_session_id = payload.session_id;
            store.next_seq = 0;
            store.last_emit_at = None;
            store.delayed_emit_scheduled = false;
        }
        store.next_seq = store.next_seq.saturating_add(1);
        payload.seq = store.next_seq;
        store.current = Some(payload.clone());
        return Some(payload);
    }
    None
}

fn clear_current_preinput_payload(session_id: u64) -> Option<PreInputPayload> {
    if let Ok(mut store) = preinput_store().lock() {
        if session_id == store.current_session_id {
            store.closed_session_id = store.closed_session_id.max(session_id);
            store.current = None;
            store.delayed_emit_scheduled = false;
            store.next_seq = store.next_seq.saturating_add(1);
            return Some(PreInputPayload {
                session_id,
                seq: store.next_seq,
                text: String::new(),
                state: PreInputState::Dismissing,
                confirmed_chars: Some(0),
                message: None,
            });
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn emit_update_coalesced(app: &AppHandle, payload: PreInputPayload) {
    let delay = {
        let Ok(mut store) = preinput_store().lock() else {
            window::emit_update(app, &payload);
            return;
        };

        match store.last_emit_at {
            None => {
                store.last_emit_at = Some(Instant::now());
                drop(store);
                window::emit_update(app, &payload);
                return;
            }
            Some(last_emit_at)
                if last_emit_at.elapsed() >= Duration::from_millis(PREINPUT_EMIT_COALESCE_MS) =>
            {
                store.last_emit_at = Some(Instant::now());
                store.delayed_emit_scheduled = false;
                drop(store);
                window::emit_update(app, &payload);
                return;
            }
            Some(last_emit_at) if !store.delayed_emit_scheduled => {
                store.delayed_emit_scheduled = true;
                Duration::from_millis(PREINPUT_EMIT_COALESCE_MS)
                    .saturating_sub(last_emit_at.elapsed())
            }
            Some(_) => return,
        }
    };

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(delay).await;
        let payload = {
            let Ok(mut store) = preinput_store().lock() else {
                return;
            };
            store.delayed_emit_scheduled = false;
            store.last_emit_at = Some(Instant::now());
            store.current.clone()
        };
        if let Some(payload) = payload {
            window::emit_update(&app, &payload);
        }
    });
}

fn preinput_store() -> &'static Mutex<PreInputStore> {
    PREINPUT_STORE.get_or_init(|| Mutex::new(PreInputStore::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_hide_cannot_close_a_newer_preinput_session() {
        let stale_session = begin_preinput_session();
        let current_session = begin_preinput_session();

        assert!(clear_current_preinput_payload(stale_session).is_none());
        assert!(clear_current_preinput_payload(current_session).is_some());
    }
}
