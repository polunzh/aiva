use aiva_core::{AivaError, Result};
use std::path::PathBuf;

pub fn parse_memory_size(memory: &str) -> Result<u64> {
    let memory = memory.to_uppercase();
    let (value, unit) = if memory.ends_with("GB") {
        let value = memory
            .trim_end_matches("GB")
            .parse::<u64>()
            .map_err(|_| AivaError::ConfigError("Invalid memory size".to_string()))?;
        (value, 1024)
    } else if memory.ends_with("MB") {
        let value = memory
            .trim_end_matches("MB")
            .parse::<u64>()
            .map_err(|_| AivaError::ConfigError("Invalid memory size".to_string()))?;
        (value, 1)
    } else {
        return Err(AivaError::ConfigError(
            "Memory size must end with MB or GB".to_string(),
        ));
    };

    Ok(value * unit)
}

pub fn parse_disk_size(disk: &str) -> Result<u64> {
    let disk = disk.to_uppercase();
    if disk.ends_with("GB") {
        let value = disk
            .trim_end_matches("GB")
            .parse::<u64>()
            .map_err(|_| AivaError::ConfigError("Invalid disk size".to_string()))?;
        Ok(value)
    } else {
        Err(AivaError::ConfigError(
            "Disk size must end with GB".to_string(),
        ))
    }
}

pub fn parse_port_mapping(port: &str) -> Result<(u16, u16)> {
    let parts: Vec<&str> = port.split(':').collect();
    if parts.len() != 2 {
        return Err(AivaError::ConfigError(
            "Port mapping must be in format host:guest".to_string(),
        ));
    }

    let host_port = parts[0]
        .parse::<u16>()
        .map_err(|_| AivaError::ConfigError("Invalid host port".to_string()))?;
    let guest_port = parts[1]
        .parse::<u16>()
        .map_err(|_| AivaError::ConfigError("Invalid guest port".to_string()))?;

    Ok((host_port, guest_port))
}

pub fn get_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AivaError::ConfigError("Cannot determine home directory".to_string()))?;
    Ok(home.join(".aiva").join("data"))
}

pub fn get_images_dir() -> Result<PathBuf> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join("images"))
}

pub fn get_vm_dir(vm_name: &str) -> Result<PathBuf> {
    let data_dir = get_data_dir()?;
    Ok(data_dir.join("vms").join(vm_name))
}
