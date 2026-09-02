use crate::command_error::CommandResult;
use crate::config::EndpointPurpose;
use crate::desktop_support::DesktopSupportPolicy;
use crate::inject::DeliveryExecutor;
use crate::shortcut_runtime::ShortcutRuntimeFactory;
use crate::target_port::TargetPort;
use std::sync::Arc;
use tauri::{AppHandle, WebviewWindow};

pub mod tray;
pub mod window_lifecycle;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub trait NativeConfirmation: Send + Sync {
    fn authorize_endpoint(
        &self,
        window: &WebviewWindow,
        origin: &str,
        purpose: &EndpointPurpose,
    ) -> CommandResult<bool>;

    fn enable_clipboard_compatibility(
        &self,
        window: &WebviewWindow,
        executable_name: &str,
    ) -> CommandResult<bool>;
}

pub struct PlatformServiceAdapters {
    pub support: DesktopSupportPolicy,
    pub confirmations: Arc<dyn NativeConfirmation>,
}

pub struct PlatformRuntimeAdapters {
    pub targets: Arc<dyn TargetPort>,
    pub delivery: Arc<dyn DeliveryExecutor>,
    pub shortcut: Arc<dyn ShortcutRuntimeFactory>,
}

pub fn service_adapters() -> PlatformServiceAdapters {
    #[cfg(target_os = "windows")]
    {
        windows::service_adapters()
    }
    #[cfg(target_os = "macos")]
    {
        macos::service_adapters()
    }
}

pub fn runtime_adapters(app: &AppHandle) -> PlatformRuntimeAdapters {
    #[cfg(target_os = "windows")]
    {
        windows::runtime_adapters(app)
    }
    #[cfg(target_os = "macos")]
    {
        macos::runtime_adapters(app)
    }
}
