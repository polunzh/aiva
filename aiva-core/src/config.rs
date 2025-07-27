use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub defaults: DefaultConfig,
    pub platform: PlatformConfig,
    pub networking: NetworkingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultConfig {
    pub cpus: u32,
    pub memory: String,
    pub disk: String,
    pub cache_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    pub linux: LinuxConfig,
    pub macos: MacOSConfig,
    pub windows: WindowsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinuxConfig {
    pub firecracker_binary: PathBuf,
    pub jailer_binary: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacOSConfig {
    pub lima_instance: String,
    pub lima_cpus: u32,
    pub lima_memory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsConfig {
    pub wsl_distro: String,
    pub nested_virtualization: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkingConfig {
    pub bridge_name: String,
    pub subnet: String,
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProfile {
    pub cpus: u32,
    pub memory: String,
    pub disk: String,
    pub description: String,
}

impl Config {
    pub fn load() -> crate::Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = serde_yaml::from_str(&content)
                .map_err(|e| crate::AivaError::ConfigError(e.to_string()))?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> crate::Result<()> {
        let config_path = Self::config_path()?;
        let config_dir = config_path.parent().unwrap();

        std::fs::create_dir_all(config_dir)?;

        let content = serde_yaml::to_string(self)
            .map_err(|e| crate::AivaError::ConfigError(e.to_string()))?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    fn config_path() -> crate::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| {
            crate::AivaError::ConfigError("Cannot determine home directory".to_string())
        })?;
        Ok(home.join(".aiva").join("config.yaml"))
    }

    pub fn resource_profiles() -> HashMap<String, ResourceProfile> {
        let mut profiles = HashMap::new();

        profiles.insert(
            "minimal".to_string(),
            ResourceProfile {
                cpus: 2,
                memory: "4GB".to_string(),
                disk: "20GB".to_string(),
                description: "Lightweight MCP server".to_string(),
            },
        );

        profiles.insert(
            "standard".to_string(),
            ResourceProfile {
                cpus: 4,
                memory: "8GB".to_string(),
                disk: "50GB".to_string(),
                description: "Standard AI agent".to_string(),
            },
        );

        profiles.insert(
            "performance".to_string(),
            ResourceProfile {
                cpus: 8,
                memory: "16GB".to_string(),
                disk: "100GB".to_string(),
                description: "Large model inference".to_string(),
            },
        );

        profiles
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            defaults: DefaultConfig {
                cpus: 4,
                memory: "8GB".to_string(),
                disk: "50GB".to_string(),
                cache_strategy: "writeback".to_string(),
            },
            platform: PlatformConfig {
                linux: LinuxConfig {
                    firecracker_binary: PathBuf::from("/usr/bin/firecracker"),
                    jailer_binary: PathBuf::from("/usr/bin/jailer"),
                },
                macos: MacOSConfig {
                    lima_instance: "aiva-host".to_string(),
                    lima_cpus: 8,
                    lima_memory: "16GB".to_string(),
                },
                windows: WindowsConfig {
                    wsl_distro: "aiva-wsl".to_string(),
                    nested_virtualization: true,
                },
            },
            networking: NetworkingConfig {
                bridge_name: "aiva-br0".to_string(),
                subnet: "172.16.0.0/24".to_string(),
                dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
            },
        }
    }
}

use dirs; // Add this to dependencies
