use aiva_core::{AivaError, Result};
use std::process::Command;
use tracing::{debug, info};

pub fn create_tap_device(name: &str) -> Result<String> {
    let tap_name = format!("aiva-tap-{}", &name[..8.min(name.len())]);

    info!("Creating TAP device: {}", tap_name);

    // Create TAP device
    let output = Command::new("ip")
        .args(["tuntap", "add", "dev", &tap_name, "mode", "tap"])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "create TAP device".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("File exists") {
            // TAP device already exists, that's okay
            debug!("TAP device {} already exists", tap_name);
        } else {
            return Err(AivaError::NetworkError {
                operation: "create TAP device".to_string(),
                cause: stderr.to_string(),
            });
        }
    }

    // Bring up the TAP device
    Command::new("ip")
        .args(["link", "set", "dev", &tap_name, "up"])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "bring up TAP device".to_string(),
            cause: e.to_string(),
        })?;

    Ok(tap_name)
}

pub fn delete_tap_device(tap_name: &str) -> Result<()> {
    info!("Deleting TAP device: {}", tap_name);

    let output = Command::new("ip")
        .args(["link", "delete", tap_name])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "delete TAP device".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("Cannot find device") {
            return Err(AivaError::NetworkError {
                operation: "delete TAP device".to_string(),
                cause: stderr.to_string(),
            });
        }
    }

    Ok(())
}

pub fn configure_tap_device(tap_name: &str, ip_addr: &str) -> Result<()> {
    debug!("Configuring TAP device {} with IP {}", tap_name, ip_addr);

    let output = Command::new("ip")
        .args(["addr", "add", ip_addr, "dev", tap_name])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "configure TAP device".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("File exists") {
            // IP already assigned
            return Err(AivaError::NetworkError {
                operation: "configure TAP device".to_string(),
                cause: stderr.to_string(),
            });
        }
    }

    Ok(())
}
