use crate::commands::ConfigAction;
use crate::output::{OutputFormat, print_error, print_info, print_success};
use aiva_core::{Config, Result};
use std::fs;
use std::path::PathBuf;

pub async fn execute(action: ConfigAction, _config: Config, _format: OutputFormat) -> Result<()> {
    match action {
        ConfigAction::Get { name, key } => {
            print_info(&format!("Getting config value '{key}' for VM '{name}'"));

            // Get VM-specific configuration
            let vm_config = get_vm_config(&name)?;

            // Use dot notation to access nested config values
            let value = get_config_value(&vm_config, &key)?;

            match value {
                Some(val) => {
                    println!("{key}: {val}");
                }
                None => {
                    print_error(&format!(
                        "Configuration key '{key}' not found for VM '{name}'"
                    ));
                }
            }
        }
        ConfigAction::Set { name, key, value } => {
            print_info(&format!(
                "Setting config '{key}' = '{value}' for VM '{name}'"
            ));

            // Load existing VM configuration
            let mut vm_config = get_vm_config(&name)?;

            // Set the configuration value
            set_config_value(&mut vm_config, &key, &value)?;

            // Save the updated configuration
            save_vm_config(&name, &vm_config)?;

            print_success(&format!("Config '{key}' set to '{value}' for VM '{name}'"));
        }
        ConfigAction::List { name } => {
            print_info(&format!("Listing configuration for VM '{name}'"));

            let vm_config = get_vm_config(&name)?;

            // Display configuration in a readable format
            println!("VM Configuration for '{name}':");
            println!("  CPUs: {}", vm_config.cpus);
            println!("  Memory: {} MB", vm_config.memory_mb);
            println!("  Disk: {} GB", vm_config.disk_gb);
            println!("  Kernel: {}", vm_config.kernel_path.display());
            println!("  Rootfs: {}", vm_config.rootfs_path.display());

            println!("  Network:");
            println!("    Guest IP: {}", vm_config.network.guest_ip);
            println!("    Host IP: {}", vm_config.network.host_ip);
            println!("    Subnet: {}", vm_config.network.subnet);
            println!("    Gateway: {}", vm_config.network.gateway);
            println!("    DNS Servers: {:?}", vm_config.network.dns_servers);
            println!("    DHCP Enabled: {}", vm_config.network.dhcp_enabled);

            println!("  Storage:");
            println!("    Cache Strategy: {}", vm_config.storage.cache_strategy);
            println!(
                "    Additional Drives: {}",
                vm_config.storage.additional_drives.len()
            );
        }
    }

    Ok(())
}

fn get_vm_config_path(name: &str) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        aiva_core::AivaError::ConfigError("Cannot determine home directory".to_string())
    })?;

    let vm_dir = home.join(".aiva").join("data").join("vms").join(name);
    Ok(vm_dir.join("config.json"))
}

fn get_vm_config(name: &str) -> Result<aiva_core::VMConfig> {
    let config_path = get_vm_config_path(name)?;

    if !config_path.exists() {
        return Err(aiva_core::AivaError::ConfigError(format!(
            "VM '{name}' configuration not found. Run 'aiva init {name}' first."
        )));
    }

    let content = fs::read_to_string(&config_path)?;
    let config: aiva_core::VMConfig = serde_json::from_str(&content)
        .map_err(|e| aiva_core::AivaError::ConfigError(format!("Failed to parse config: {e}")))?;

    Ok(config)
}

fn save_vm_config(name: &str, config: &aiva_core::VMConfig) -> Result<()> {
    let config_path = get_vm_config_path(name)?;

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(config).map_err(|e| {
        aiva_core::AivaError::ConfigError(format!("Failed to serialize config: {e}"))
    })?;

    fs::write(&config_path, content)?;

    Ok(())
}

fn get_config_value(config: &aiva_core::VMConfig, key: &str) -> Result<Option<String>> {
    match key {
        "cpus" => Ok(Some(config.cpus.to_string())),
        "memory" | "memory_mb" => Ok(Some(config.memory_mb.to_string())),
        "disk" | "disk_gb" => Ok(Some(config.disk_gb.to_string())),
        "kernel_path" => Ok(Some(config.kernel_path.display().to_string())),
        "rootfs_path" => Ok(Some(config.rootfs_path.display().to_string())),
        "network.guest_ip" => Ok(Some(config.network.guest_ip.clone())),
        "network.host_ip" => Ok(Some(config.network.host_ip.clone())),
        "network.subnet" => Ok(Some(config.network.subnet.clone())),
        "network.gateway" => Ok(Some(config.network.gateway.clone())),
        "network.dns_servers" => Ok(Some(config.network.dns_servers.join(","))),
        "network.dhcp_enabled" => Ok(Some(config.network.dhcp_enabled.to_string())),
        "storage.cache_strategy" => Ok(Some(config.storage.cache_strategy.to_string())),
        _ => Ok(None),
    }
}

fn set_config_value(config: &mut aiva_core::VMConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "cpus" => {
            config.cpus = value
                .parse()
                .map_err(|_| aiva_core::AivaError::ConfigError("Invalid CPU count".to_string()))?;
        }
        "memory_mb" => {
            config.memory_mb = value.parse().map_err(|_| {
                aiva_core::AivaError::ConfigError("Invalid memory size".to_string())
            })?;
        }
        "disk_gb" => {
            config.disk_gb = value
                .parse()
                .map_err(|_| aiva_core::AivaError::ConfigError("Invalid disk size".to_string()))?;
        }
        "kernel_path" => {
            config.kernel_path = PathBuf::from(value);
        }
        "rootfs_path" => {
            config.rootfs_path = PathBuf::from(value);
        }
        "network.guest_ip" => {
            config.network.guest_ip = value.to_string();
        }
        "network.host_ip" => {
            config.network.host_ip = value.to_string();
        }
        "network.subnet" => {
            config.network.subnet = value.to_string();
        }
        "network.gateway" => {
            config.network.gateway = value.to_string();
        }
        "network.dns_servers" => {
            config.network.dns_servers = value.split(',').map(|s| s.trim().to_string()).collect();
        }
        "network.dhcp_enabled" => {
            config.network.dhcp_enabled = value.parse().map_err(|_| {
                aiva_core::AivaError::ConfigError("Invalid boolean value".to_string())
            })?;
        }
        "storage.cache_strategy" => {
            config.storage.cache_strategy = match value.to_lowercase().as_str() {
                "writeback" => aiva_core::CacheStrategy::Writeback,
                "unsafe" => aiva_core::CacheStrategy::Unsafe,
                _ => {
                    return Err(aiva_core::AivaError::ConfigError(
                        "Invalid cache strategy".to_string(),
                    ));
                }
            };
        }
        _ => {
            return Err(aiva_core::AivaError::ConfigError(format!(
                "Unknown configuration key: {key}"
            )));
        }
    }

    Ok(())
}
