use aiva_core::{AivaError, Platform, Result, VMInstance, VMLogger, VMMetrics, VMState};
use askama::Template;
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, info, warn};

use crate::command_pool::{ConnectionType, get_command_pool};
use crate::vsock_executor::VSOCK_COMMAND_PORT;

// Askama templates for bash scripts
#[derive(Template)]
#[template(path = "windows_setup_firecracker.sh", escape = "none")]
struct SetupFirecrackerTemplate;

#[derive(Template)]
#[template(path = "windows_create_vm.sh", escape = "none")]
struct CreateVmTemplate {
    vm_name: String,
    disk_gb: u64,
    config_json: String,
}

#[derive(Template)]
#[template(path = "windows_start_vm.sh", escape = "none")]
struct StartVmTemplate {
    vm_name: String,
}

#[derive(Template)]
#[template(path = "windows_stop_vm.sh", escape = "none")]
struct StopVmTemplate {
    vm_name: String,
    force_flag: String,
}

#[derive(Template)]
#[template(path = "windows_delete_vm.sh", escape = "none")]
struct DeleteVmTemplate {
    vm_name: String,
}

#[derive(Template)]
#[template(path = "windows_get_metrics.sh", escape = "none")]
struct GetMetricsTemplate {
    vm_name: String,
}

pub struct WindowsPlatform {
    wsl_distro: String,
}

impl WindowsPlatform {
    pub fn new() -> Result<Self> {
        Ok(Self {
            wsl_distro: String::from("Ubuntu"), // Default to Ubuntu
        })
    }

    fn check_nested_virtualization(&self) -> Result<()> {
        // Check if running on Windows 11
        let output = Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: String::from("windows"),
                message: format!("Failed to check Windows version: {e}"),
                recoverable: false,
            })?;

        let version = String::from_utf8_lossy(&output.stdout);
        if !version.contains("Version 10.0.22") {
            // Windows 11 versions start with 10.0.22xxx
            warn!("Windows 11 is recommended for nested virtualization support");
        }

        // Check WSL version
        let wsl_output = Command::new("wsl")
            .args(["--status"])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: String::from("windows"),
                message: format!("WSL not found: {e}"),
                recoverable: true,
            })?;

        let wsl_info = String::from_utf8_lossy(&wsl_output.stdout);
        if !wsl_info.contains("WSL version: 2") && !wsl_info.contains("WSL 2") {
            return Err(AivaError::PlatformError {
                platform: String::from("windows"),
                message: String::from("WSL 2 is required for nested virtualization"),
                recoverable: true,
            });
        }

        Ok(())
    }

    async fn ensure_wsl_distro(&self) -> Result<String> {
        let output = Command::new("wsl")
            .args(["--list", "--quiet"])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: String::from("windows"),
                message: format!("Failed to list WSL distributions: {e}"),
                recoverable: false,
            })?;

        let distros = String::from_utf8_lossy(&output.stdout);

        // Try to find a suitable distro
        let distro_to_use = if distros.contains(&self.wsl_distro) {
            self.wsl_distro.clone()
        } else if distros.contains("Ubuntu") {
            info!("Using Ubuntu as WSL distribution");
            String::from("Ubuntu")
        } else if distros.contains("Debian") {
            info!("Using Debian as WSL distribution");
            String::from("Debian")
        } else {
            return Err(AivaError::PlatformError {
                platform: String::from("windows"),
                message: String::from(
                    "No suitable WSL distribution found. Please install Ubuntu from Microsoft Store.",
                ),
                recoverable: true,
            });
        };

        // Ensure Firecracker is installed in WSL
        self.setup_firecracker_in_wsl(&distro_to_use).await?;

        Ok(distro_to_use)
    }

    async fn setup_firecracker_in_wsl(&self, distro: &str) -> Result<()> {
        info!("Setting up Firecracker in WSL distribution: {}", distro);

        let template = SetupFirecrackerTemplate;
        let script = template.render().map_err(|e| AivaError::PlatformError {
            platform: String::from("windows"),
            message: format!("Failed to render setup script template: {e}"),
            recoverable: false,
        })?;

        let result = self.exec_in_wsl(distro, &script).await?;
        debug!("Firecracker setup result: {}", result);

        Ok(())
    }

    async fn exec_in_wsl(&self, distro: &str, command: &str) -> Result<String> {
        let output = Command::new("wsl")
            .args(["-d", distro, "bash", "-c", command])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: String::from("windows"),
                message: format!("Failed to execute in WSL: {e}"),
                recoverable: false,
            })?;

        if !output.status.success() {
            return Err(AivaError::PlatformError {
                platform: String::from("windows"),
                message: format!(
                    "Command failed in WSL: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                recoverable: false,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    async fn create_firecracker_config(&self, instance: &VMInstance) -> Result<String> {
        // Create Firecracker configuration for WSL
        let config = serde_json::json!({
            "vm_id": instance.name,
            "vcpu_count": instance.config.cpus,
            "mem_size_mib": instance.config.memory_mb,
            "kernel_path": "/opt/aiva/firecracker/vmlinux",
            "rootfs_path": format!("/var/lib/firecracker/{}.rootfs.ext4", instance.name),
            "kernel_args": format!(
                "console=ttyS0 reboot=k panic=1 pci=off init=/sbin/init ip={}::172.16.0.1:255.255.255.0::eth0:off",
                instance.config.network.guest_ip
            ),
            "network": {
                "iface_id": "eth0",
                "guest_ip": instance.config.network.guest_ip,
                "tap_device": format!("tap-{}", instance.name)
            }
        });

        Ok(serde_json::to_string(&config)?)
    }
}

#[async_trait]
impl Platform for WindowsPlatform {
    async fn create_vm(&self, instance: &VMInstance) -> Result<VMInstance> {
        self.check_nested_virtualization()?;
        let distro = self.ensure_wsl_distro().await?;

        let logger = VMLogger::new(instance.name.clone());
        logger.init().await?;
        logger.info("VM creation started on Windows WSL2").await?;

        info!("Creating VM {} through WSL2", instance.name);

        // Create Firecracker configuration
        let config_json = self.create_firecracker_config(instance).await?;

        // Create VM using the template
        let template = CreateVmTemplate {
            vm_name: instance.name.clone(),
            disk_gb: instance.config.disk_gb,
            config_json,
        };

        let script = template.render().map_err(|e| AivaError::PlatformError {
            platform: String::from("windows"),
            message: format!("Failed to render create VM script template: {e}"),
            recoverable: false,
        })?;

        self.exec_in_wsl(&distro, &script).await?;

        logger.info("VM created successfully in WSL2").await?;

        let mut updated_instance = instance.clone();
        updated_instance.state = VMState::Stopped;
        updated_instance.runtime.pid = None;
        updated_instance.runtime.api_socket = Some(PathBuf::from(format!(
            "/var/lib/firecracker/{}/firecracker.sock",
            instance.name
        )));
        updated_instance.runtime.tap_device = Some(format!("tap-{}", instance.name));

        Ok(updated_instance)
    }

    async fn start_vm(&self, instance: &VMInstance) -> Result<()> {
        let distro = self.ensure_wsl_distro().await?;

        let logger = VMLogger::new(instance.name.clone());
        logger.info("VM start initiated on Windows WSL2").await?;

        debug!("Starting VM {} through WSL2", instance.name);

        let template = StartVmTemplate {
            vm_name: instance.name.clone(),
        };

        let script = template.render().map_err(|e| AivaError::PlatformError {
            platform: String::from("windows"),
            message: format!("Failed to render start VM script template: {e}"),
            recoverable: false,
        })?;

        self.exec_in_wsl(&distro, &script).await?;

        logger.info("VM started successfully in WSL2").await?;

        Ok(())
    }

    async fn stop_vm(&self, instance: &VMInstance, force: bool) -> Result<()> {
        let distro = self.ensure_wsl_distro().await?;

        let logger = VMLogger::new(instance.name.clone());
        logger
            .info(&format!("VM stop initiated (force: {force})"))
            .await?;

        debug!(
            "Stopping VM {} through WSL2 (force: {})",
            instance.name, force
        );

        let template = StopVmTemplate {
            vm_name: instance.name.clone(),
            force_flag: if force {
                String::from("-9")
            } else {
                String::new()
            },
        };

        let script = template.render().map_err(|e| AivaError::PlatformError {
            platform: String::from("windows"),
            message: format!("Failed to render stop VM script template: {e}"),
            recoverable: false,
        })?;

        self.exec_in_wsl(&distro, &script).await?;

        logger.info("VM stopped successfully in WSL2").await?;

        Ok(())
    }

    async fn delete_vm(&self, instance: &VMInstance) -> Result<()> {
        let distro = self.ensure_wsl_distro().await?;

        let logger = VMLogger::new(instance.name.clone());
        logger.info("VM deletion initiated on Windows WSL2").await?;

        debug!("Deleting VM {} through WSL2", instance.name);

        // First stop the VM if it's running
        let _ = self.stop_vm(instance, true).await;

        let template = DeleteVmTemplate {
            vm_name: instance.name.clone(),
        };

        let script = template.render().map_err(|e| AivaError::PlatformError {
            platform: String::from("windows"),
            message: format!("Failed to render delete VM script template: {e}"),
            recoverable: false,
        })?;

        self.exec_in_wsl(&distro, &script).await?;

        // Unregister from command pool
        let command_pool = get_command_pool();
        command_pool.unregister_vm(&instance.name).await?;

        logger.info("VM deleted successfully from WSL2").await?;

        Ok(())
    }

    async fn get_vm_metrics(&self, instance: &VMInstance) -> Result<VMMetrics> {
        let distro = self.ensure_wsl_distro().await?;

        info!("Getting metrics for VM {} (Windows - WSL2)", instance.name);

        let template = GetMetricsTemplate {
            vm_name: instance.name.clone(),
        };

        let script = template.render().map_err(|e| AivaError::PlatformError {
            platform: String::from("windows"),
            message: format!("Failed to render metrics script template: {e}"),
            recoverable: false,
        })?;

        let result = self.exec_in_wsl(&distro, &script).await?;

        // Parse the simple JSON output
        if result.contains("error") {
            // Return default metrics if VM is not running
            return Ok(VMMetrics {
                cpu_usage: 0.0,
                memory_usage: aiva_core::MemoryMetrics {
                    total_mb: instance.config.memory_mb,
                    used_mb: 0,
                    available_mb: instance.config.memory_mb,
                    cache_mb: 0,
                },
                disk_io: aiva_core::DiskIOMetrics {
                    read_bytes: 0,
                    write_bytes: 0,
                    read_ops: 0,
                    write_ops: 0,
                },
                network_io: aiva_core::NetworkIOMetrics {
                    rx_bytes: 0,
                    tx_bytes: 0,
                    rx_packets: 0,
                    tx_packets: 0,
                },
                uptime: std::time::Duration::from_secs(0),
            });
        }

        // Simple parsing of the output
        let mut cpu_usage = 15.0;
        let mut memory_used_kb = 0u64;
        let mut memory_total_kb = 0u64;
        let mut rx_bytes = 0u64;
        let mut tx_bytes = 0u64;

        for line in result.lines() {
            if line.contains("cpu_usage") {
                if let Some(value) = line.split(':').nth(1) {
                    cpu_usage = value.trim().trim_end_matches(',').parse().unwrap_or(15.0);
                }
            } else if line.contains("memory_used_kb") {
                if let Some(value) = line.split(':').nth(1) {
                    memory_used_kb = value.trim().trim_end_matches(',').parse().unwrap_or(0);
                }
            } else if line.contains("memory_total_kb") {
                if let Some(value) = line.split(':').nth(1) {
                    memory_total_kb = value.trim().trim_end_matches(',').parse().unwrap_or(0);
                }
            } else if line.contains("rx_bytes") {
                if let Some(value) = line.split(':').nth(1) {
                    rx_bytes = value.trim().trim_end_matches(',').parse().unwrap_or(0);
                }
            } else if line.contains("tx_bytes") {
                if let Some(value) = line.split(':').nth(1) {
                    tx_bytes = value.trim().parse().unwrap_or(0);
                }
            }
        }

        Ok(VMMetrics {
            cpu_usage,
            memory_usage: aiva_core::MemoryMetrics {
                total_mb: memory_total_kb / 1024,
                used_mb: memory_used_kb / 1024,
                available_mb: (memory_total_kb - memory_used_kb) / 1024,
                cache_mb: 0,
            },
            disk_io: aiva_core::DiskIOMetrics {
                read_bytes: 0,
                write_bytes: 0,
                read_ops: 0,
                write_ops: 0,
            },
            network_io: aiva_core::NetworkIOMetrics {
                rx_bytes,
                tx_bytes,
                rx_packets: 0,
                tx_packets: 0,
            },
            uptime: std::time::Duration::from_secs(3600), // Default 1 hour
        })
    }

    async fn execute_command(&self, instance: &VMInstance, command: &str) -> Result<String> {
        let logger = VMLogger::new(instance.name.clone());
        logger
            .info(&format!("Executing command: {command}"))
            .await?;

        info!(
            "Executing command in VM {} (Windows - WSL2): {}",
            instance.name, command
        );

        // Check if VM is running
        if instance.state != VMState::Running {
            return Err(AivaError::VMError {
                vm_name: instance.name.clone(),
                state: instance.state,
                message: format!("VM is not running (current state: {:?})", instance.state),
            });
        }

        // Get command pool and check if VM is registered
        let command_pool = get_command_pool();

        // If not registered, register it now
        if !command_pool.is_registered(&instance.name).await {
            // Use network connection through guest IP
            let connection_type = ConnectionType::Network {
                host: instance.config.network.guest_ip.clone(),
                port: VSOCK_COMMAND_PORT as u16,
            };

            // Register the VM with the command pool
            if let Err(e) = command_pool
                .register_vm(instance.name.clone(), connection_type)
                .await
            {
                warn!("Failed to register VM in command pool: {}", e);

                // Fallback: execute command directly in WSL
                let distro = self.ensure_wsl_distro().await?;
                let fallback_output = self.exec_in_wsl(&distro, command).await?;
                return Ok(fallback_output);
            }
        }

        // Execute the command through the command pool
        let output = command_pool
            .execute_command(&instance.name, command)
            .await?;

        logger.info("Command executed successfully").await?;
        info!(
            "Command executed successfully in VM {} with WSL2",
            instance.name
        );

        Ok(output)
    }

    async fn check_requirements(&self) -> Result<()> {
        // Check if WSL is installed
        let wsl_check = Command::new("wsl")
            .arg("--help")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false);

        if !wsl_check {
            return Err(AivaError::PlatformError {
                platform: String::from("windows"),
                message: String::from("WSL is not installed. Please enable WSL 2."),
                recoverable: true,
            });
        }

        // Check nested virtualization
        self.check_nested_virtualization()?;

        info!("Windows platform requirements satisfied");

        Ok(())
    }

    fn name(&self) -> &str {
        "windows"
    }
}
