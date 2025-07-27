pub mod command_pool;
mod firecracker;
mod firecracker_vm;
mod linux;
mod macos;
mod vsock_executor;
mod windows;

#[cfg(test)]
mod tests;

use aiva_core::{Platform, Result};
use std::sync::Arc;

pub use linux::LinuxPlatform;
pub use macos::MacOSPlatform;
pub use windows::WindowsPlatform;

pub fn get_current_platform() -> Result<Arc<dyn Platform>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(LinuxPlatform::new()?))
    }

    #[cfg(target_os = "macos")]
    {
        Ok(Arc::new(MacOSPlatform::new()?))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Arc::new(WindowsPlatform::new()?))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(aiva_core::AivaError::PlatformError {
            platform: std::env::consts::OS.to_string(),
            message: "Unsupported platform".to_string(),
            recoverable: false,
        })
    }
}

pub fn get_platform_with_config(lima_config: Option<String>) -> Result<Arc<dyn Platform>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Arc::new(LinuxPlatform::new()?))
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(config_path) = lima_config {
            Ok(Arc::new(MacOSPlatform::with_config(config_path)?))
        } else {
            Ok(Arc::new(MacOSPlatform::new()?))
        }
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Arc::new(WindowsPlatform::new()?))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err(aiva_core::AivaError::PlatformError {
            platform: std::env::consts::OS.to_string(),
            message: "Unsupported platform".to_string(),
            recoverable: false,
        })
    }
}

pub fn detect_platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}
