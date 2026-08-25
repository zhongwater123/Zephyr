use crate::config::{AppConfig, ShortcutMode};
#[cfg(target_os = "windows")]
use crate::low_level_hook::{HookChord, HookEvent, LowLevelHookService};
use crate::services::{AppServices, ConfigService};
use crate::shortcut;
use crate::voice_controller::{SessionEvent, VoiceSessionController};
use crate::SharedRuntime;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const PRIMARY_HOTKEY_ID: u32 = 1;
const SECONDARY_HOTKEY_ID: u32 = 2;
const MAX_APPLICATION_HOTKEY_ID: u32 = 0xBFFF;
const PREVIEW_EVENT: &str = "shortcut_preview_changed";
const STATUS_EVENT: &str = "shortcut_status_changed";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutBackend {
    RegisterHotkey,
    LowLevelHook,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutRuntimeState {
    Active,
    Inactive,
    Occupied,
    Verifying,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutRuntimeStatus {
    pub shortcut: String,
    pub mode: ShortcutMode,
    pub backend: ShortcutBackend,
    pub state: ShortcutRuntimeState,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutPreviewState {
    ReservedStandard,
    Occupied,
    AwaitingHookTest,
    HookVerified,
    Invalid,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutPreview {
    pub preview_id: u64,
    pub shortcut: String,
    pub normalized: String,
    pub mode: ShortcutMode,
    pub state: ShortcutPreviewState,
    pub reason: String,
}

#[derive(Clone)]
struct PreviewRegistration {
    dto: ShortcutPreview,
    parsed: Shortcut,
    reserved: Option<Shortcut>,
}

struct ManagerState {
    active_shortcut: Option<Shortcut>,
    active_label: String,
    active_mode: ShortcutMode,
    enabled: bool,
    preview: Option<PreviewRegistration>,
    next_preview_id: u64,
    status: ShortcutRuntimeStatus,
}

fn mark_preview_verified(
    state: &Arc<Mutex<ManagerState>>,
    expected_preview_id: u64,
) -> Option<(ShortcutPreview, ShortcutRuntimeStatus)> {
    state.lock().ok().and_then(|mut state| {
        let preview = state.preview.as_mut()?;
        if preview.dto.preview_id != expected_preview_id
            || preview.dto.state != ShortcutPreviewState::AwaitingHookTest
        {
            return None;
        }
        preview.dto.state = ShortcutPreviewState::HookVerified;
        preview.dto.reason = "独占快捷键实测成功，正在保存。".to_string();
        let dto = preview.dto.clone();
        state.status = ShortcutRuntimeStatus {
            shortcut: dto.normalized.clone(),
            mode: ShortcutMode::ExclusiveHook,
            backend: ShortcutBackend::LowLevelHook,
            state: ShortcutRuntimeState::Verifying,
            message: dto.reason.clone(),
        };
        Some((dto, state.status.clone()))
    })
}

pub struct ShortcutManager {
    app: AppHandle,
    runtime: SharedRuntime,
    config_service: Arc<ConfigService>,
    state: Arc<Mutex<ManagerState>>,
    #[cfg(target_os = "windows")]
    hook: Mutex<Option<LowLevelHookService>>,
    #[cfg(target_os = "windows")]
    hook_error: Option<String>,
}

fn with_registration_id(mut shortcut: Shortcut, id: u32) -> Shortcut {
    debug_assert!((1..=MAX_APPLICATION_HOTKEY_ID).contains(&id));
    shortcut.id = id;
    shortcut
}

fn next_registration_id(current: Option<Shortcut>) -> u32 {
    match current.map(|shortcut| shortcut.id()) {
        Some(PRIMARY_HOTKEY_ID) => SECONDARY_HOTKEY_ID,
        _ => PRIMARY_HOTKEY_ID,
    }
}

fn same_chord(left: Shortcut, right: Shortcut) -> bool {
    left.mods == right.mods && left.key == right.key
}

fn reuses_active_standard(
    old: Option<Shortcut>,
    old_mode: &ShortcutMode,
    preview: &PreviewRegistration,
) -> bool {
    *old_mode == ShortcutMode::Standard
        && preview.dto.mode == ShortcutMode::Standard
        && preview.reserved.is_none()
        && old.is_some_and(|old| same_chord(old, preview.parsed))
}

impl ShortcutManager {
    pub fn initialize(
        app: &mut tauri::App,
        runtime: SharedRuntime,
        services: AppServices,
    ) -> tauri::Result<(VoiceSessionController, Arc<Self>)> {
        let controller = VoiceSessionController::new(runtime.clone(), services.clone());
        let handler_runtime = runtime.clone();
        let handler_controller = controller.clone();
        app.handle().plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, incoming, event| {
                    let current = handler_runtime
                        .lock()
                        .map(|runtime| runtime.registered_shortcut == Some(*incoming))
                        .unwrap_or(false);
                    if !current {
                        return;
                    }
                    match event.state() {
                        ShortcutState::Pressed => {
                            handler_controller.submit(app, SessionEvent::Pressed)
                        }
                        ShortcutState::Released => {
                            handler_controller.submit(app, SessionEvent::Released)
                        }
                    }
                })
                .build(),
        )?;

        let config = services.config.snapshot();
        let state = Arc::new(Mutex::new(ManagerState {
            active_shortcut: None,
            active_label: config.shortcut.clone(),
            active_mode: config.shortcut_mode.clone(),
            enabled: config.enabled,
            preview: None,
            next_preview_id: 0,
            status: ShortcutRuntimeStatus {
                shortcut: config.shortcut.clone(),
                mode: config.shortcut_mode.clone(),
                backend: ShortcutBackend::None,
                state: ShortcutRuntimeState::Inactive,
                message: "快捷键尚未初始化。".to_string(),
            },
        }));

        #[cfg(target_os = "windows")]
        let (hook, hook_error) = {
            let event_state = state.clone();
            let event_app = app.handle().clone();
            let event_controller = controller.clone();
            let event_runtime = runtime.clone();
            match LowLevelHookService::start(move |event| match event {
                HookEvent::ActivePressed => {
                    event_controller.submit(&event_app, SessionEvent::Pressed)
                }
                HookEvent::ActiveReleased => {
                    event_controller.submit(&event_app, SessionEvent::Released)
                }
                HookEvent::PreviewVerified(preview_id) => {
                    let update = mark_preview_verified(&event_state, preview_id);
                    if let Some((dto, status)) = update {
                        let _ = event_app.emit(PREVIEW_EVENT, dto);
                        let _ = event_app.emit(STATUS_EVENT, status);
                    }
                }
                HookEvent::ReinstallFailed => {
                    let (preview, status, active_failed) = event_state
                        .lock()
                        .ok()
                        .map(|mut state| {
                            let preview = state.preview.as_mut().and_then(|preview| {
                                (preview.dto.mode == ShortcutMode::ExclusiveHook).then(|| {
                                    preview.dto.state = ShortcutPreviewState::Invalid;
                                    preview.dto.reason =
                                        "系统恢复后无法重新安装独占钩子，请重试或切回标准模式。"
                                            .to_string();
                                    preview.dto.clone()
                                })
                            });
                            let active_failed =
                                state.active_mode == ShortcutMode::ExclusiveHook && state.enabled;
                            state.status = if let Some(preview) = preview.as_ref() {
                                ShortcutRuntimeStatus {
                                    shortcut: preview.normalized.clone(),
                                    mode: ShortcutMode::ExclusiveHook,
                                    backend: ShortcutBackend::None,
                                    state: ShortcutRuntimeState::Error,
                                    message: preview.reason.clone(),
                                }
                            } else {
                                ShortcutRuntimeStatus {
                                    shortcut: state.active_label.clone(),
                                    mode: state.active_mode.clone(),
                                    backend: ShortcutBackend::None,
                                    state: ShortcutRuntimeState::Error,
                                    message: "系统恢复后无法重新安装独占钩子。".to_string(),
                                }
                            };
                            if active_failed {
                                state.active_shortcut = None;
                            }
                            (preview, state.status.clone(), active_failed)
                        })
                        .unwrap_or_else(|| {
                            (
                                None,
                                ShortcutRuntimeStatus {
                                    shortcut: String::new(),
                                    mode: ShortcutMode::ExclusiveHook,
                                    backend: ShortcutBackend::None,
                                    state: ShortcutRuntimeState::Error,
                                    message: "系统恢复后无法重新安装独占钩子。".to_string(),
                                },
                                false,
                            )
                        });
                    if active_failed {
                        if let Ok(mut runtime) = event_runtime.lock() {
                            runtime.shortcut_registration_error = Some(status.message.clone());
                        }
                    }
                    if let Some(preview) = preview {
                        let _ = event_app.emit(PREVIEW_EVENT, preview);
                    }
                    let _ = event_app.emit(STATUS_EVENT, status);
                }
                HookEvent::Shutdown => {}
            }) {
                Ok(service) => (Some(service), None),
                Err(error) => (None, Some(error)),
            }
        };

        let manager = Arc::new(Self {
            app: app.handle().clone(),
            runtime,
            config_service: services.config.clone(),
            state,
            #[cfg(target_os = "windows")]
            hook: Mutex::new(hook),
            #[cfg(target_os = "windows")]
            hook_error,
        });
        manager.activate_configured(&config);
        Ok((controller, manager))
    }

    pub fn status(&self) -> ShortcutRuntimeStatus {
        self.state
            .lock()
            .map(|state| state.status.clone())
            .unwrap_or_else(|_| ShortcutRuntimeStatus {
                shortcut: String::new(),
                mode: ShortcutMode::Standard,
                backend: ShortcutBackend::None,
                state: ShortcutRuntimeState::Error,
                message: "快捷键状态锁已损坏。".to_string(),
            })
    }

    pub fn preview(&self, preview_id: u64) -> Result<ShortcutPreview, String> {
        self.refresh_preview_verification();
        let state = self.state.lock().map_err(|error| error.to_string())?;
        let preview = state
            .preview
            .as_ref()
            .ok_or_else(|| "快捷键预览已失效，请重新设置。".to_string())?;
        if preview.dto.preview_id != preview_id {
            return Err("快捷键预览已过期，请重新设置。".to_string());
        }
        Ok(preview.dto.clone())
    }

    fn refresh_preview_verification(&self) {
        #[cfg(target_os = "windows")]
        {
            let preview_id = self
                .state
                .lock()
                .ok()
                .and_then(|state| state.preview.as_ref().map(|preview| preview.dto.preview_id));
            let Some(preview_id) = preview_id else {
                return;
            };
            let verified = self
                .hook
                .lock()
                .ok()
                .and_then(|hook| hook.as_ref().map(|hook| hook.preview_verified(preview_id)))
                .unwrap_or(false);
            if verified {
                if let Some((preview, status)) = mark_preview_verified(&self.state, preview_id) {
                    let _ = self.app.emit(PREVIEW_EVENT, preview);
                    let _ = self.app.emit(STATUS_EVENT, status);
                }
            }
        }
    }

    fn activate_configured(&self, config: &AppConfig) {
        let parsed = match shortcut::parse_shortcut(&config.shortcut) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.set_error(
                    &config.shortcut,
                    config.shortcut_mode.clone(),
                    error.message(),
                );
                return;
            }
        };
        if !config.enabled {
            self.update_inactive(&parsed.normalized, config.shortcut_mode.clone());
            return;
        }
        match config.shortcut_mode {
            ShortcutMode::Standard => {
                let registered = with_registration_id(parsed.shortcut, PRIMARY_HOTKEY_ID);
                match self.app.global_shortcut().register(registered) {
                    Ok(()) => self.promote_standard(registered, parsed.normalized),
                    Err(error) => self.set_occupied(
                        &config.shortcut,
                        ShortcutMode::Standard,
                        format!("快捷键已被系统或其他应用占用。({error})"),
                    ),
                }
            }
            ShortcutMode::ExclusiveHook => self.activate_hook(parsed.shortcut, parsed.normalized),
        }
    }

    fn activate_hook(&self, parsed: Shortcut, label: String) {
        #[cfg(target_os = "windows")]
        {
            let chord = match HookChord::from_shortcut(parsed) {
                Ok(chord) => chord,
                Err(error) => {
                    self.set_error(&label, ShortcutMode::ExclusiveHook, error);
                    return;
                }
            };
            match self.hook.lock() {
                Ok(hook) if hook.is_some() => {
                    let hook = hook.as_ref().unwrap();
                    if let Err(error) = hook.ensure_healthy() {
                        self.set_error(&label, ShortcutMode::ExclusiveHook, error);
                        return;
                    }
                    hook.set_active(Some(chord));
                    hook.set_voice_enabled(true);
                    self.promote_hook(parsed, label);
                }
                _ => self.set_error(
                    &label,
                    ShortcutMode::ExclusiveHook,
                    self.hook_error
                        .clone()
                        .unwrap_or_else(|| "低级键盘钩子不可用。".to_string()),
                ),
            }
        }
        #[cfg(not(target_os = "windows"))]
        self.set_error(
            &label,
            ShortcutMode::ExclusiveHook,
            "独占模式仅支持 Windows。".to_string(),
        );
    }

    pub fn prepare(&self, input: &str, mode: ShortcutMode) -> Result<ShortcutPreview, String> {
        self.cancel_preview(None)?;
        let parsed = match shortcut::parse_shortcut(input) {
            Ok(parsed) => parsed,
            Err(error) => return self.invalid_preview(input, mode, error.message()),
        };
        let (preview_id, active, active_mode) = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.next_preview_id = state.next_preview_id.saturating_add(1);
            (
                state.next_preview_id,
                state.active_shortcut,
                state.active_mode.clone(),
            )
        };
        match mode {
            ShortcutMode::Standard => {
                let same_active = active_mode == ShortcutMode::Standard
                    && active.is_some_and(|active| same_chord(active, parsed.shortcut));
                let reserved = if same_active {
                    None
                } else {
                    let candidate =
                        with_registration_id(parsed.shortcut, next_registration_id(active));
                    if let Err(error) = self.app.global_shortcut().register(candidate) {
                        let dto = ShortcutPreview {
                            preview_id,
                            shortcut: input.to_string(),
                            normalized: parsed.normalized.clone(),
                            mode,
                            state: ShortcutPreviewState::Occupied,
                            reason: format!(
                                "该快捷键已被系统或其他应用占用；可以选择独占接管。({error})"
                            ),
                        };
                        self.store_preview(dto.clone(), parsed.shortcut, None)?;
                        self.update_preview_status(&dto);
                        return Ok(dto);
                    }
                    Some(candidate)
                };
                let dto = ShortcutPreview {
                    preview_id,
                    shortcut: input.to_string(),
                    normalized: parsed.normalized.clone(),
                    mode,
                    state: ShortcutPreviewState::ReservedStandard,
                    reason: "快捷键已预占，保存后生效。".to_string(),
                };
                self.store_preview(dto.clone(), parsed.shortcut, reserved)?;
                self.update_preview_status(&dto);
                Ok(dto)
            }
            ShortcutMode::ExclusiveHook => {
                #[cfg(target_os = "windows")]
                {
                    HookChord::from_shortcut(parsed.shortcut)?;
                    let hook = self.hook.lock().map_err(|error| error.to_string())?;
                    let Some(hook) = hook.as_ref() else {
                        return self.invalid_preview(
                            input,
                            mode,
                            self.hook_error
                                .clone()
                                .unwrap_or_else(|| "低级键盘钩子不可用。".to_string()),
                        );
                    };
                    if let Err(error) = hook.ensure_healthy() {
                        return self.invalid_preview(input, mode, error);
                    }
                    let dto = ShortcutPreview {
                        preview_id,
                        shortcut: input.to_string(),
                        normalized: parsed.normalized.clone(),
                        mode,
                        state: ShortcutPreviewState::HookVerified,
                        reason: "独占接管可用，正在保存。".to_string(),
                    };
                    self.store_preview(dto.clone(), parsed.shortcut, None)?;
                    self.update_preview_status(&dto);
                    Ok(dto)
                }
                #[cfg(not(target_os = "windows"))]
                self.invalid_preview(input, mode, "独占模式仅支持 Windows。".to_string())
            }
        }
    }

    pub fn cancel_preview(&self, expected_id: Option<u64>) -> Result<(), String> {
        let preview = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if expected_id
                .is_some_and(|id| state.preview.as_ref().map(|p| p.dto.preview_id) != Some(id))
            {
                return Ok(());
            }
            state.preview.take()
        };
        if let Some(preview) = preview {
            self.release_preview(&preview);
        }
        #[cfg(target_os = "windows")]
        if let Ok(hook) = self.hook.lock() {
            if let Some(hook) = hook.as_ref() {
                hook.set_preview(None, 0);
            }
        }
        self.restore_active_status();
        Ok(())
    }

    fn store_preview(
        &self,
        dto: ShortcutPreview,
        parsed: Shortcut,
        reserved: Option<Shortcut>,
    ) -> Result<(), String> {
        self.state
            .lock()
            .map_err(|error| error.to_string())?
            .preview = Some(PreviewRegistration {
            dto,
            parsed,
            reserved,
        });
        Ok(())
    }

    fn invalid_preview(
        &self,
        input: &str,
        mode: ShortcutMode,
        reason: String,
    ) -> Result<ShortcutPreview, String> {
        let preview_id = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            state.next_preview_id = state.next_preview_id.saturating_add(1);
            state.next_preview_id
        };
        let dto = ShortcutPreview {
            preview_id,
            shortcut: input.to_string(),
            normalized: String::new(),
            mode,
            state: ShortcutPreviewState::Invalid,
            reason,
        };
        self.store_preview(dto.clone(), shortcut::default_shortcut().shortcut, None)?;
        self.update_preview_status(&dto);
        Ok(dto)
    }

    fn release_preview(&self, preview: &PreviewRegistration) {
        if let Some(reserved) = preview.reserved {
            let active = self
                .state
                .lock()
                .ok()
                .and_then(|state| state.active_shortcut);
            if active != Some(reserved) {
                let _ = self.app.global_shortcut().unregister(reserved);
            }
        }
    }

    pub fn commit(&self, preview_id: u64, expected_revision: u64) -> Result<AppConfig, String> {
        self.refresh_preview_verification();
        let preview = {
            let state = self.state.lock().map_err(|error| error.to_string())?;
            let preview = state
                .preview
                .clone()
                .ok_or_else(|| "快捷键预览已失效，请重新设置。".to_string())?;
            if preview.dto.preview_id != preview_id {
                return Err("快捷键预览已过期，请重新设置。".to_string());
            }
            if !matches!(
                preview.dto.state,
                ShortcutPreviewState::ReservedStandard | ShortcutPreviewState::HookVerified
            ) {
                return Err("快捷键尚未完成预占或独占实测。".to_string());
            }
            preview
        };
        if self
            .runtime
            .lock()
            .map_err(|error| error.to_string())?
            .sessions
            .active
            .is_some()
        {
            return Err("录音或识别进行中，结束当前会话后才能修改快捷键。".to_string());
        }
        #[cfg(target_os = "windows")]
        if preview.dto.mode == ShortcutMode::ExclusiveHook {
            let hook = self.hook.lock().map_err(|error| error.to_string())?;
            let hook = hook
                .as_ref()
                .ok_or_else(|| "低级键盘钩子不可用。".to_string())?;
            hook.ensure_healthy()?;
        }
        let current = self.config_service.snapshot();
        if current.revision != expected_revision {
            return Err(format!(
                "config_conflict: expected revision {expected_revision}, current revision {}",
                current.revision
            ));
        }
        if current.shortcut == preview.dto.normalized && current.shortcut_mode == preview.dto.mode {
            match preview.dto.mode {
                ShortcutMode::Standard => {
                    let active = self
                        .state
                        .lock()
                        .ok()
                        .and_then(|state| state.active_shortcut);
                    let registered = preview.reserved.or(active).unwrap_or(preview.parsed);
                    self.promote_standard(registered, preview.dto.normalized.clone());
                }
                ShortcutMode::ExclusiveHook => {
                    #[cfg(target_os = "windows")]
                    {
                        let chord = HookChord::from_shortcut(preview.parsed)?;
                        let hook = self.hook.lock().map_err(|error| error.to_string())?;
                        let hook = hook
                            .as_ref()
                            .ok_or_else(|| "低级键盘钩子不可用。".to_string())?;
                        hook.set_preview(None, 0);
                        hook.set_active(Some(chord));
                        hook.set_voice_enabled(current.enabled);
                    }
                    self.promote_hook(preview.parsed, preview.dto.normalized.clone());
                }
            }
            if let Ok(mut state) = self.state.lock() {
                state.preview = None;
            }
            self.emit_status();
            return Ok(current);
        }

        let mut next = current.clone();
        next.shortcut = preview.dto.normalized.clone();
        next.shortcut_mode = preview.dto.mode.clone();
        next.revision = next.revision.saturating_add(1);
        self.config_service
            .commit_config(expected_revision, next.clone())
            .map_err(|error| format!("{error:?}"))?;

        let (old, old_mode) = self
            .state
            .lock()
            .map_err(|error| error.to_string())
            .map(|state| (state.active_shortcut, state.active_mode.clone()))?;
        let reuse_old_standard = reuses_active_standard(old, &old_mode, &preview);
        if old_mode == ShortcutMode::Standard && !reuse_old_standard {
            if let Some(old) = old.filter(|old| preview.reserved != Some(*old)) {
                if let Err(error) = self.app.global_shortcut().unregister(old) {
                    self.rollback_config(&current, &next);
                    self.release_preview(&preview);
                    if let Ok(mut state) = self.state.lock() {
                        state.preview = None;
                    }
                    self.restore_active_status();
                    return Err(format!("无法注销旧快捷键，配置已回滚：{error}"));
                }
            }
        }

        if !next.enabled {
            self.release_preview(&preview);
            #[cfg(target_os = "windows")]
            if let Ok(hook) = self.hook.lock() {
                if let Some(hook) = hook.as_ref() {
                    hook.set_preview(None, 0);
                    hook.set_active(None);
                    hook.set_voice_enabled(false);
                }
            }
            if let Ok(mut state) = self.state.lock() {
                state.preview = None;
            }
            self.update_inactive(&preview.dto.normalized, preview.dto.mode.clone());
            return Ok(next);
        }

        match preview.dto.mode {
            ShortcutMode::Standard => {
                #[cfg(target_os = "windows")]
                if let Ok(hook) = self.hook.lock() {
                    if let Some(hook) = hook.as_ref() {
                        hook.set_active(None);
                        hook.set_preview(None, 0);
                    }
                }
                let registered = if reuse_old_standard {
                    old.expect("reused standard shortcut must exist")
                } else {
                    preview.reserved.unwrap_or(preview.parsed)
                };
                self.promote_standard(registered, preview.dto.normalized.clone());
            }
            ShortcutMode::ExclusiveHook => {
                #[cfg(target_os = "windows")]
                {
                    let chord = HookChord::from_shortcut(preview.parsed)?;
                    let hook = self.hook.lock().map_err(|error| error.to_string())?;
                    let hook = hook
                        .as_ref()
                        .ok_or_else(|| "低级键盘钩子不可用。".to_string())?;
                    hook.set_preview(None, 0);
                    hook.set_active(Some(chord));
                    hook.set_voice_enabled(next.enabled);
                }
                self.promote_hook(preview.parsed, preview.dto.normalized.clone());
            }
        }
        if let Ok(mut state) = self.state.lock() {
            state.preview = None;
        }
        self.emit_status();
        Ok(next)
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let (mode, label, active) = {
            let mut state = self.state.lock().map_err(|error| error.to_string())?;
            if state.enabled == enabled {
                return Ok(());
            }
            state.enabled = enabled;
            (
                state.active_mode.clone(),
                state.active_label.clone(),
                state.active_shortcut,
            )
        };
        if !enabled {
            let mut unregister_error = None;
            if mode == ShortcutMode::Standard {
                if let Some(active) = active {
                    if let Err(error) = self.app.global_shortcut().unregister(active) {
                        unregister_error = Some(error.to_string());
                    }
                }
                self.clear_runtime_registration();
            }
            #[cfg(target_os = "windows")]
            if let Ok(hook) = self.hook.lock() {
                if let Some(hook) = hook.as_ref() {
                    hook.set_voice_enabled(false);
                    hook.set_active(None);
                }
            }
            self.update_inactive(&label, mode);
            return unregister_error.map_or(Ok(()), Err);
        }

        let parsed = shortcut::parse_shortcut(&label).map_err(|error| error.message())?;
        match mode {
            ShortcutMode::Standard => {
                let registered = with_registration_id(parsed.shortcut, PRIMARY_HOTKEY_ID);
                match self.app.global_shortcut().register(registered) {
                    Ok(()) => self.promote_standard(registered, parsed.normalized),
                    Err(error) => self.set_occupied(
                        &label,
                        ShortcutMode::Standard,
                        format!("重新启用时快捷键已被占用。({error})"),
                    ),
                }
            }
            ShortcutMode::ExclusiveHook => self.activate_hook(parsed.shortcut, parsed.normalized),
        }
        self.emit_status();
        Ok(())
    }

    pub fn resume(&self) {
        let needs_hook = self
            .state
            .lock()
            .map(|state| {
                (state.enabled && state.active_mode == ShortcutMode::ExclusiveHook)
                    || state
                        .preview
                        .as_ref()
                        .is_some_and(|preview| preview.dto.mode == ShortcutMode::ExclusiveHook)
            })
            .unwrap_or(false);
        #[cfg(target_os = "windows")]
        if needs_hook {
            if let Ok(hook) = self.hook.lock() {
                if let Some(hook) = hook.as_ref() {
                    let _ = hook.reinstall();
                }
            }
        }
    }

    pub fn shutdown(&self) {
        let _ = self.cancel_preview(None);
        let standard = self
            .state
            .lock()
            .ok()
            .and_then(|state| {
                (state.active_mode == ShortcutMode::Standard).then_some(state.active_shortcut)
            })
            .flatten();
        if let Some(active) = standard {
            let _ = self.app.global_shortcut().unregister(active);
        }
        #[cfg(target_os = "windows")]
        if let Ok(mut hook) = self.hook.lock() {
            if let Some(mut hook) = hook.take() {
                hook.shutdown();
            }
        }
        self.clear_runtime_registration();
    }

    fn rollback_config(&self, current: &AppConfig, committed: &AppConfig) {
        let mut rollback = current.clone();
        rollback.revision = committed.revision.saturating_add(1);
        if let Err(error) = self
            .config_service
            .commit_config(committed.revision, rollback)
        {
            log::error!("shortcut config rollback failed: {error:?}");
        }
    }

    fn promote_standard(&self, registered: Shortcut, label: String) {
        log::info!("shortcut backend active: register_hotkey");
        metrics::counter!("shortcut.backend.transitions").increment(1);
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.registered_shortcut = Some(registered);
            runtime.registered_shortcut_label = label.clone();
            runtime.shortcut_registration_error = None;
        }
        if let Ok(mut state) = self.state.lock() {
            state.active_shortcut = Some(registered);
            state.active_label = label.clone();
            state.active_mode = ShortcutMode::Standard;
            state.status = ShortcutRuntimeStatus {
                shortcut: label,
                mode: ShortcutMode::Standard,
                backend: ShortcutBackend::RegisterHotkey,
                state: ShortcutRuntimeState::Active,
                message: "标准全局快捷键已生效。".to_string(),
            };
        }
    }

    fn promote_hook(&self, parsed: Shortcut, label: String) {
        log::info!("shortcut backend active: low_level_hook");
        metrics::counter!("shortcut.backend.transitions").increment(1);
        self.clear_runtime_registration();
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.registered_shortcut_label = label.clone();
            runtime.shortcut_registration_error = None;
        }
        if let Ok(mut state) = self.state.lock() {
            state.active_shortcut = Some(parsed);
            state.active_label = label.clone();
            state.active_mode = ShortcutMode::ExclusiveHook;
            state.status = ShortcutRuntimeStatus {
                shortcut: label,
                mode: ShortcutMode::ExclusiveHook,
                backend: ShortcutBackend::LowLevelHook,
                state: ShortcutRuntimeState::Active,
                message: "独占快捷键已生效（用户态最佳努力）。".to_string(),
            };
        }
    }

    fn clear_runtime_registration(&self) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.registered_shortcut = None;
        }
    }

    fn set_error(&self, label: &str, mode: ShortcutMode, message: String) {
        log::error!("shortcut backend error in {mode:?}: {message}");
        metrics::counter!("shortcut.registration.errors").increment(1);
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.registered_shortcut = None;
            runtime.registered_shortcut_label = label.to_string();
            runtime.shortcut_registration_error = Some(message.clone());
        }
        if let Ok(mut state) = self.state.lock() {
            state.status = ShortcutRuntimeStatus {
                shortcut: label.to_string(),
                mode,
                backend: ShortcutBackend::None,
                state: ShortcutRuntimeState::Error,
                message,
            };
        }
        self.emit_status();
    }

    fn set_occupied(&self, label: &str, mode: ShortcutMode, message: String) {
        log::warn!("shortcut registration occupied in {mode:?}: {message}");
        metrics::counter!("shortcut.registration.errors").increment(1);
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.registered_shortcut = None;
            runtime.registered_shortcut_label = label.to_string();
            runtime.shortcut_registration_error = Some(message.clone());
        }
        if let Ok(mut state) = self.state.lock() {
            state.status = ShortcutRuntimeStatus {
                shortcut: label.to_string(),
                mode,
                backend: ShortcutBackend::None,
                state: ShortcutRuntimeState::Occupied,
                message,
            };
        }
        self.emit_status();
    }

    fn update_inactive(&self, label: &str, mode: ShortcutMode) {
        if let Ok(mut runtime) = self.runtime.lock() {
            runtime.registered_shortcut = None;
            runtime.registered_shortcut_label = label.to_string();
            runtime.shortcut_registration_error = None;
        }
        if let Ok(mut state) = self.state.lock() {
            state.active_shortcut = None;
            state.active_label = label.to_string();
            state.active_mode = mode.clone();
            state.status = ShortcutRuntimeStatus {
                shortcut: label.to_string(),
                mode,
                backend: ShortcutBackend::None,
                state: ShortcutRuntimeState::Inactive,
                message: "语音输入已关闭，快捷键已放行。".to_string(),
            };
        }
        self.emit_status();
    }

    fn update_preview_status(&self, preview: &ShortcutPreview) {
        if let Ok(mut state) = self.state.lock() {
            state.status = ShortcutRuntimeStatus {
                shortcut: preview.normalized.clone(),
                mode: preview.mode.clone(),
                backend: match preview.state {
                    ShortcutPreviewState::ReservedStandard => ShortcutBackend::RegisterHotkey,
                    ShortcutPreviewState::AwaitingHookTest | ShortcutPreviewState::HookVerified => {
                        ShortcutBackend::LowLevelHook
                    }
                    _ => ShortcutBackend::None,
                },
                state: match preview.state {
                    ShortcutPreviewState::Occupied => ShortcutRuntimeState::Occupied,
                    ShortcutPreviewState::Invalid => ShortcutRuntimeState::Error,
                    _ => ShortcutRuntimeState::Verifying,
                },
                message: preview.reason.clone(),
            };
        }
        self.emit_status();
    }

    fn restore_active_status(&self) {
        let config = self.config_service.snapshot();
        if !config.enabled {
            self.update_inactive(&config.shortcut, config.shortcut_mode);
            return;
        }
        let runtime_error = self
            .runtime
            .lock()
            .ok()
            .and_then(|runtime| runtime.shortcut_registration_error.clone());
        if let Ok(mut state) = self.state.lock() {
            let mode = state.active_mode.clone();
            if state.active_shortcut.is_none() {
                let message = runtime_error.unwrap_or_else(|| "快捷键当前未生效。".to_string());
                state.status = ShortcutRuntimeStatus {
                    shortcut: state.active_label.clone(),
                    mode: mode.clone(),
                    backend: ShortcutBackend::None,
                    state: if mode == ShortcutMode::Standard {
                        ShortcutRuntimeState::Occupied
                    } else {
                        ShortcutRuntimeState::Error
                    },
                    message,
                };
                drop(state);
                self.emit_status();
                return;
            }
            state.status = ShortcutRuntimeStatus {
                shortcut: state.active_label.clone(),
                mode: mode.clone(),
                backend: match mode {
                    ShortcutMode::Standard => ShortcutBackend::RegisterHotkey,
                    ShortcutMode::ExclusiveHook => ShortcutBackend::LowLevelHook,
                },
                state: ShortcutRuntimeState::Active,
                message: match mode {
                    ShortcutMode::Standard => "标准全局快捷键已生效。".to_string(),
                    ShortcutMode::ExclusiveHook => {
                        "独占快捷键已生效（用户态最佳努力）。".to_string()
                    }
                },
            };
        }
        self.emit_status();
    }

    fn emit_status(&self) {
        let _ = self.app.emit(STATUS_EVENT, self.status());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::{Code, Modifiers};

    #[test]
    fn registration_ids_stay_in_the_microsoft_application_range() {
        let base = Shortcut::new(Some(Modifiers::CONTROL), Code::Space);
        let primary = with_registration_id(base, PRIMARY_HOTKEY_ID);
        let secondary = with_registration_id(base, SECONDARY_HOTKEY_ID);
        assert!((1..=MAX_APPLICATION_HOTKEY_ID).contains(&primary.id()));
        assert!((1..=MAX_APPLICATION_HOTKEY_ID).contains(&secondary.id()));
        assert_eq!(next_registration_id(Some(primary)), SECONDARY_HOTKEY_ID);
        assert_eq!(next_registration_id(Some(secondary)), PRIMARY_HOTKEY_ID);
        assert!(same_chord(primary, secondary));
    }

    #[test]
    fn normalized_recommit_reuses_the_already_registered_shortcut() {
        let parsed = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::Space);
        let registered = with_registration_id(parsed, PRIMARY_HOTKEY_ID);
        let preview = PreviewRegistration {
            dto: ShortcutPreview {
                preview_id: 1,
                shortcut: "ctrl + shift + space".to_string(),
                normalized: "Ctrl+Shift+Space".to_string(),
                mode: ShortcutMode::Standard,
                state: ShortcutPreviewState::ReservedStandard,
                reason: String::new(),
            },
            parsed,
            reserved: None,
        };
        assert!(reuses_active_standard(
            Some(registered),
            &ShortcutMode::Standard,
            &preview,
        ));
    }
}
