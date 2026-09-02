use super::{NativeConfirmation, PlatformRuntimeAdapters, PlatformServiceAdapters};
use crate::command_error::CommandResult;
use crate::config::EndpointPurpose;
use crate::desktop_support::{unsupported_platform, DesktopCapability, DesktopSupportPolicy};
use crate::inject::{DeliveryExecutor, DeliveryReceipt, DeliveryRequest, InjectError};
use crate::physical_shortcut::ShortcutBinding;
use crate::shortcut_runtime::{
    KeyboardEngineDiagnostics, KeyboardEngineError, ShortcutEventHandler, ShortcutRuntimeFactory,
    ShortcutRuntimePort,
};
use crate::target_port::{CapturedTarget, TargetPort};
use async_trait::async_trait;
use std::sync::Arc;
use tauri::WebviewWindow;

#[derive(Debug, Default)]
struct UnsupportedNativeConfirmation;

impl NativeConfirmation for UnsupportedNativeConfirmation {
    fn authorize_endpoint(
        &self,
        _window: &WebviewWindow,
        _origin: &str,
        _purpose: &EndpointPurpose,
    ) -> CommandResult<bool> {
        Err(unsupported_platform(DesktopCapability::NativeConfirmation))
    }

    fn enable_clipboard_compatibility(
        &self,
        _window: &WebviewWindow,
        _executable_name: &str,
    ) -> CommandResult<bool> {
        Err(unsupported_platform(
            DesktopCapability::AutomaticTextDelivery,
        ))
    }
}

pub(super) fn service_adapters() -> PlatformServiceAdapters {
    PlatformServiceAdapters {
        support: DesktopSupportPolicy::macos_bootstrap(),
        confirmations: Arc::new(UnsupportedNativeConfirmation),
    }
}

#[derive(Debug, Default)]
struct UnsupportedDeliveryExecutor;

#[derive(Debug, Default)]
struct UnsupportedTargetPort;

#[derive(Debug, Default)]
struct UnsupportedShortcutRuntime;

#[derive(Debug, Default)]
struct UnsupportedShortcutRuntimeFactory;

const SHORTCUT_UNSUPPORTED: &str = "global shortcut is unsupported on macOS";

impl ShortcutRuntimeFactory for UnsupportedShortcutRuntimeFactory {
    fn start(
        &self,
        _on_event: ShortcutEventHandler,
    ) -> Result<Arc<dyn ShortcutRuntimePort>, String> {
        Ok(Arc::new(UnsupportedShortcutRuntime))
    }
}

impl ShortcutRuntimePort for UnsupportedShortcutRuntime {
    fn startup_error(&self) -> Option<String> {
        Some(SHORTCUT_UNSUPPORTED.to_string())
    }

    fn set_binding(&self, _binding: Option<&ShortcutBinding>) -> Result<(), String> {
        Err(SHORTCUT_UNSUPPORTED.to_string())
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        if enabled {
            Err(SHORTCUT_UNSUPPORTED.to_string())
        } else {
            Ok(())
        }
    }

    fn ensure_runtime_ready(&self, _force_reinstall: bool) -> Result<u64, KeyboardEngineError> {
        Err(KeyboardEngineError::unsupported(SHORTCUT_UNSUPPORTED))
    }

    fn is_healthy(&self) -> bool {
        false
    }

    fn diagnostics(&self) -> KeyboardEngineDiagnostics {
        KeyboardEngineDiagnostics::default()
    }

    fn shutdown(&self) {}
}

impl TargetPort for UnsupportedTargetPort {
    fn capture(&self) -> Result<CapturedTarget, String> {
        Err("target capture is unsupported on macOS".to_string())
    }

    fn exists(&self, _target: &CapturedTarget) -> Result<(), String> {
        Err("target identity is unsupported on macOS".to_string())
    }

    fn validate_foreground(&self, _target: &CapturedTarget) -> Result<(), String> {
        Err("target validation is unsupported on macOS".to_string())
    }

    fn activate(&self, _target: &CapturedTarget) -> Result<(), String> {
        Err("target activation is unsupported on macOS".to_string())
    }
}

#[async_trait]
impl DeliveryExecutor for UnsupportedDeliveryExecutor {
    async fn deliver(&self, _request: DeliveryRequest) -> Result<DeliveryReceipt, InjectError> {
        Err(InjectError::UnsupportedCapability {
            capability: DesktopCapability::AutomaticTextDelivery.as_str(),
            message: "automatic text delivery is unsupported on macOS".to_string(),
        })
    }
}

pub(super) fn runtime_adapters(_app: &tauri::AppHandle) -> PlatformRuntimeAdapters {
    PlatformRuntimeAdapters {
        targets: Arc::new(UnsupportedTargetPort),
        delivery: Arc::new(UnsupportedDeliveryExecutor),
        shortcut: Arc::new(UnsupportedShortcutRuntimeFactory),
    }
}
