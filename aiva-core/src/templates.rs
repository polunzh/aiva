use crate::{
    AivaError, CacheStrategy, NetworkConfig, PortMapping, Protocol, Result, StorageConfig, VMConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMTemplate {
    pub name: String,
    pub description: String,
    pub runtime: RuntimeType,
    pub base_config: VMConfig,
    pub setup_scripts: Vec<String>,
    pub runtime_commands: HashMap<String, String>,
    pub mcp_support: MCPSupport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuntimeType {
    Python3 {
        version: String,
        package_manager: String,
    },
    NodeJS {
        version: String,
        package_manager: String,
    },
    Custom {
        name: String,
        version: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPSupport {
    pub sse_enabled: bool,
    pub stdio_enabled: bool,
    pub default_port: Option<u16>,
    pub supported_transports: Vec<String>,
}

impl VMTemplate {
    pub fn python3_uv() -> Self {
        Self {
            name: "python3-uv".to_string(),
            description: "Python 3.12 with uv package manager for MCP servers".to_string(),
            runtime: RuntimeType::Python3 {
                version: "3.12".to_string(),
                package_manager: "uv".to_string(),
            },
            base_config: Self::default_vm_config_with_port(3000),
            setup_scripts: vec![
                "#!/bin/bash".to_string(),
                "set -e".to_string(),
                "echo 'Setting up Python 3.12 with uv...'".to_string(),
                "# Install Python 3.12".to_string(),
                "apt-get update && apt-get install -y software-properties-common".to_string(),
                "add-apt-repository ppa:deadsnakes/ppa -y".to_string(),
                "apt-get update && apt-get install -y python3.12 python3.12-venv python3.12-dev"
                    .to_string(),
                "# Install uv".to_string(),
                "curl -LsSf https://astral.sh/uv/install.sh | sh".to_string(),
                "echo 'export PATH=\"$HOME/.cargo/bin:$PATH\"' >> ~/.bashrc".to_string(),
                "# Create MCP working directory".to_string(),
                "mkdir -p /opt/mcp".to_string(),
                "chown $(whoami):$(whoami) /opt/mcp".to_string(),
                "echo 'Python 3.12 with uv setup complete'".to_string(),
            ],
            runtime_commands: {
                let mut commands = HashMap::new();
                commands.insert("python".to_string(), "python3.12".to_string());
                commands.insert("pip".to_string(), "uv pip".to_string());
                commands.insert("venv".to_string(), "uv venv".to_string());
                commands.insert("run".to_string(), "uv run".to_string());
                commands
            },
            mcp_support: MCPSupport {
                sse_enabled: true,
                stdio_enabled: true,
                default_port: Some(3000),
                supported_transports: vec!["sse".to_string(), "stdio".to_string()],
            },
        }
    }

    pub fn nodejs22_npx() -> Self {
        Self {
            name: "nodejs22-npx".to_string(),
            description: "Node.js 22 with npx for MCP servers".to_string(),
            runtime: RuntimeType::NodeJS {
                version: "22".to_string(),
                package_manager: "npm".to_string(),
            },
            base_config: Self::default_vm_config_with_port(3001),
            setup_scripts: vec![
                "#!/bin/bash".to_string(),
                "set -e".to_string(),
                "echo 'Setting up Node.js 22 with npx...'".to_string(),
                "# Install Node.js 22 via NodeSource repository".to_string(),
                "curl -fsSL https://deb.nodesource.com/setup_22.x | bash -".to_string(),
                "apt-get install -y nodejs".to_string(),
                "# Verify installation".to_string(),
                "node --version".to_string(),
                "npm --version".to_string(),
                "npx --version".to_string(),
                "# Create MCP working directory".to_string(),
                "mkdir -p /opt/mcp".to_string(),
                "chown $(whoami):$(whoami) /opt/mcp".to_string(),
                "# Set npm global prefix to avoid permission issues".to_string(),
                "npm config set prefix /opt/mcp/.npm-global".to_string(),
                "echo 'export PATH=/opt/mcp/.npm-global/bin:$PATH' >> ~/.bashrc".to_string(),
                "echo 'Node.js 22 with npx setup complete'".to_string(),
            ],
            runtime_commands: {
                let mut commands = HashMap::new();
                commands.insert("node".to_string(), "node".to_string());
                commands.insert("npm".to_string(), "npm".to_string());
                commands.insert("npx".to_string(), "npx".to_string());
                commands.insert("run".to_string(), "npx".to_string());
                commands
            },
            mcp_support: MCPSupport {
                sse_enabled: true,
                stdio_enabled: true,
                default_port: Some(3001),
                supported_transports: vec!["sse".to_string(), "stdio".to_string()],
            },
        }
    }

    fn default_vm_config_with_port(default_port: u16) -> VMConfig {
        VMConfig {
            cpus: 2,
            memory_mb: 4096,
            disk_gb: 20,
            kernel_path: PathBuf::from("/opt/aiva/images/vmlinux"),
            rootfs_path: PathBuf::from("/opt/aiva/images/rootfs.ext4"),
            network: NetworkConfig {
                guest_ip: "172.16.0.2".to_string(),
                host_ip: "172.16.0.1".to_string(),
                subnet: "172.16.0.0/24".to_string(),
                gateway: "172.16.0.1".to_string(),
                dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
                dhcp_enabled: false,
                port_mappings: vec![PortMapping {
                    host_port: default_port,
                    guest_port: default_port,
                    protocol: Protocol::Tcp,
                }],
            },
            storage: StorageConfig {
                cache_strategy: CacheStrategy::Writeback,
                additional_drives: vec![],
            },
        }
    }

    pub fn get_all_templates() -> Vec<VMTemplate> {
        vec![Self::python3_uv(), Self::nodejs22_npx()]
    }

    pub fn get_template_by_name(name: &str) -> Result<VMTemplate> {
        match name {
            "python3-uv" | "python3" | "python" => Ok(Self::python3_uv()),
            "nodejs22-npx" | "nodejs22" | "nodejs" | "node" => Ok(Self::nodejs22_npx()),
            _ => Err(AivaError::ConfigError(format!(
                "Unknown template: {name}. Available templates: python3-uv, nodejs22-npx"
            ))),
        }
    }

    pub fn list_available_templates() -> Vec<(String, String)> {
        vec![
            (
                "python3-uv".to_string(),
                "Python 3.12 with uv package manager for MCP servers".to_string(),
            ),
            (
                "nodejs22-npx".to_string(),
                "Node.js 22 with npx for MCP servers".to_string(),
            ),
        ]
    }

    /// Generate the VM configuration from this template
    pub fn generate_vm_config(&self, customizations: Option<VMConfigCustomizations>) -> VMConfig {
        let mut config = self.base_config.clone();

        if let Some(custom) = customizations {
            if let Some(cpus) = custom.cpus {
                config.cpus = cpus;
            }
            if let Some(memory_mb) = custom.memory_mb {
                config.memory_mb = memory_mb;
            }
            if let Some(disk_gb) = custom.disk_gb {
                config.disk_gb = disk_gb;
            }
            if let Some(additional_ports) = custom.additional_ports {
                for port in additional_ports {
                    config.network.port_mappings.push(PortMapping {
                        host_port: port,
                        guest_port: port,
                        protocol: Protocol::Tcp,
                    });
                }
            }
        }

        config
    }

    /// Get the setup script as a single string
    pub fn get_setup_script(&self) -> String {
        self.setup_scripts.join("\n")
    }

    /// Get the command to run a specific MCP server
    pub fn get_run_command(&self, mcp_command: &str, transport: &str) -> Result<String> {
        if !self
            .mcp_support
            .supported_transports
            .contains(&transport.to_string())
        {
            return Err(AivaError::ConfigError(format!(
                "Transport '{}' not supported by template '{}'. Supported: {}",
                transport,
                self.name,
                self.mcp_support.supported_transports.join(", ")
            )));
        }

        let base_cmd = match &self.runtime {
            RuntimeType::Python3 { .. } => {
                if mcp_command.starts_with("python") || mcp_command.starts_with("uv") {
                    mcp_command.to_string()
                } else {
                    format!("uv run {mcp_command}")
                }
            }
            RuntimeType::NodeJS { .. } => {
                if mcp_command.starts_with("node")
                    || mcp_command.starts_with("npm")
                    || mcp_command.starts_with("npx")
                {
                    mcp_command.to_string()
                } else {
                    format!("npx {mcp_command}")
                }
            }
            RuntimeType::Custom { .. } => mcp_command.to_string(),
        };

        let full_command = match transport {
            "sse" => {
                // Check if the command already includes --port
                if base_cmd.contains("--port") {
                    // Command already has port specified, use as-is
                    format!("cd /opt/mcp && {base_cmd}")
                } else {
                    // Add default port
                    let port = self.mcp_support.default_port.unwrap_or(3000);
                    // Check if the command already includes the transport mode
                    if mcp_command.contains(" sse") || mcp_command.contains(" stdio") {
                        format!("cd /opt/mcp && {base_cmd} --port {port}")
                    } else {
                        format!("cd /opt/mcp && {base_cmd} {transport} --port {port}")
                    }
                }
            }
            "stdio" => {
                // stdio doesn't use ports, just check for transport mode
                if mcp_command.contains(" sse") || mcp_command.contains(" stdio") {
                    format!("cd /opt/mcp && {base_cmd}")
                } else {
                    format!("cd /opt/mcp && {base_cmd} {transport}")
                }
            }
            _ => {
                format!("cd /opt/mcp && {base_cmd}")
            }
        };

        Ok(full_command)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMConfigCustomizations {
    pub cpus: Option<u32>,
    pub memory_mb: Option<u64>,
    pub disk_gb: Option<u64>,
    pub additional_ports: Option<Vec<u16>>,
}

pub struct TemplateManager;

impl TemplateManager {
    pub fn list_templates() -> Vec<VMTemplate> {
        VMTemplate::get_all_templates()
    }

    pub fn get_template(name: &str) -> Result<VMTemplate> {
        VMTemplate::get_template_by_name(name)
    }

    pub fn validate_template_name(name: &str) -> bool {
        matches!(
            name,
            "python3-uv" | "python3" | "python" | "nodejs22-npx" | "nodejs22" | "nodejs" | "node"
        )
    }
}
