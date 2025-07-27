use aiva_core::{AivaError, Result};
use std::process::Command;
use tracing::{debug, info};

const BRIDGE_NAME: &str = "aiva-br0";

pub fn create_bridge() -> Result<()> {
    info!("Creating bridge: {}", BRIDGE_NAME);

    let output = Command::new("ip")
        .args(["link", "add", "name", BRIDGE_NAME, "type", "bridge"])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "create bridge".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("File exists") {
            debug!("Bridge {} already exists", BRIDGE_NAME);
        } else {
            return Err(AivaError::NetworkError {
                operation: "create bridge".to_string(),
                cause: stderr.to_string(),
            });
        }
    }

    // Bring up the bridge
    Command::new("ip")
        .args(["link", "set", "dev", BRIDGE_NAME, "up"])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "bring up bridge".to_string(),
            cause: e.to_string(),
        })?;

    // Set bridge IP
    Command::new("ip")
        .args(["addr", "add", "172.16.0.1/24", "dev", BRIDGE_NAME])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "configure bridge IP".to_string(),
            cause: e.to_string(),
        })?;

    Ok(())
}

pub fn configure_bridge(tap_device: &str) -> Result<()> {
    debug!("Adding TAP device {} to bridge {}", tap_device, BRIDGE_NAME);

    // Ensure bridge exists
    create_bridge()?;

    // Add TAP device to bridge
    let output = Command::new("ip")
        .args(["link", "set", tap_device, "master", BRIDGE_NAME])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "add TAP to bridge".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(AivaError::NetworkError {
            operation: "add TAP to bridge".to_string(),
            cause: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(())
}

pub fn delete_bridge() -> Result<()> {
    info!("Deleting bridge: {}", BRIDGE_NAME);

    let output = Command::new("ip")
        .args(["link", "delete", BRIDGE_NAME])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "delete bridge".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("Cannot find device") {
            return Err(AivaError::NetworkError {
                operation: "delete bridge".to_string(),
                cause: stderr.to_string(),
            });
        }
    }

    Ok(())
}
