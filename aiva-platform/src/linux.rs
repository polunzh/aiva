use aiva_core::{AivaError, Platform, Result, VMInstance, VMLogger, VMMetrics, VMState};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

use crate::command_pool::{ConnectionType, get_command_pool};
use crate::vsock_executor::VSOCK_COMMAND_PORT;

pub struct LinuxPlatform {
    firecracker_path: PathBuf,
    jailer_path: PathBuf,
    kvm_device: PathBuf,
}

impl LinuxPlatform {
    pub fn new() -> Result<Self> {
        // Find firecracker binary
        let firecracker_path =
            which::which("firecracker").unwrap_or_else(|_| PathBuf::from("/usr/bin/firecracker"));

        // Find jailer binary
        let jailer_path =
            which::which("jailer").unwrap_or_else(|_| PathBuf::from("/usr/bin/jailer"));

        let kvm_device = PathBuf::from("/dev/kvm");

        Ok(Self {
            firecracker_path,
            jailer_path,
            kvm_device,
        })
    }

    fn check_vsock_support(&self) -> bool {
        // Check if vsock kernel module is loaded
        Path::new("/dev/vsock").exists() || Path::new("/dev/vhost-vsock").exists()
    }

    fn check_kvm_available(&self) -> Result<()> {
        if !self.kvm_device.exists() {
            return Err(AivaError::PlatformError {
                platform: "linux".to_string(),
                message: "KVM device not found. Ensure KVM modules are loaded.".to_string(),
                recoverable: false,
            });
        }

        // Check KVM permissions
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&self.kvm_device)?;
        let mode = metadata.permissions().mode();

        if mode & 0o666 != 0o666 {
            return Err(AivaError::PlatformError {
                platform: "linux".to_string(),
                message: format!(
                    "Insufficient permissions for {}. Run 'sudo chmod 666 /dev/kvm' or add user to kvm group.",
                    self.kvm_device.display()
                ),
                recoverable: true,
            });
        }

        Ok(())
    }

    async fn prepare_jailer_workspace(&self, vm: &VMInstance) -> Result<PathBuf> {
        let workspace = PathBuf::from("/tmp")
            .join("aiva-jailer")
            .join(vm.id.to_string());
        std::fs::create_dir_all(&workspace)?;

        // Create required directories
        let root_dir = workspace.join("root");
        std::fs::create_dir_all(&root_dir)?;

        // Copy kernel and rootfs
        let kernel_dest = root_dir.join("vmlinux");
        let rootfs_dest = root_dir.join("rootfs.ext4");

        std::fs::copy(&vm.config.kernel_path, &kernel_dest)?;
        std::fs::copy(&vm.config.rootfs_path, &rootfs_dest)?;

        Ok(workspace)
    }

    async fn spawn_firecracker(
        &self,
        workspace: &Path,
        vm: &VMInstance,
    ) -> Result<std::process::Child> {
        let socket_path = workspace.join("root").join("firecracker.socket");

        let mut cmd = Command::new(&self.jailer_path);
        cmd.arg("--id")
            .arg(vm.id.to_string())
            .arg("--exec-file")
            .arg(&self.firecracker_path)
            .arg("--uid")
            .arg("1000")
            .arg("--gid")
            .arg("1000")
            .arg("--chroot-base-dir")
            .arg(workspace)
            .arg("--")
            .arg("--api-sock")
            .arg(&socket_path);

        info!("Starting Firecracker with jailer: {:?}", cmd);

        let child = cmd.spawn().map_err(|e| AivaError::PlatformError {
            platform: "linux".to_string(),
            message: format!("Failed to spawn Firecracker: {e}"),
            recoverable: false,
        })?;

        // Wait for socket to be created
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        if !socket_path.exists() {
            return Err(AivaError::PlatformError {
                platform: "linux".to_string(),
                message: "Firecracker API socket not created".to_string(),
                recoverable: false,
            });
        }

        Ok(child)
    }

    async fn get_process_cpu_usage(&self, pid: u32) -> Result<f64> {
        // Read process stat
        let stat_path = format!("/proc/{pid}/stat");
        let stat_content = tokio::fs::read_to_string(&stat_path).await?;

        // Parse CPU time from stat file
        let fields: Vec<&str> = stat_content.split_whitespace().collect();
        if fields.len() > 14 {
            let utime: u64 = fields[13].parse().unwrap_or(0);
            let stime: u64 = fields[14].parse().unwrap_or(0);
            let total_time = utime + stime;

            // Get system uptime
            let uptime_content = tokio::fs::read_to_string("/proc/uptime").await?;
            let uptime: f64 = uptime_content
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0);

            // Calculate CPU usage percentage
            let hz = 100.0; // Typical USER_HZ value
            let cpu_usage = (total_time as f64 / hz / uptime) * 100.0;

            Ok(cpu_usage.min(100.0))
        } else {
            Ok(0.0)
        }
    }

    async fn get_process_memory_usage(&self, pid: u32) -> Result<aiva_core::MemoryMetrics> {
        // Read process status for memory info
        let status_path = format!("/proc/{pid}/status");
        let status_content = tokio::fs::read_to_string(&status_path).await?;

        let mut vm_size_kb = 0u64;
        let mut vm_rss_kb = 0u64;

        for line in status_content.lines() {
            if line.starts_with("VmSize:") {
                vm_size_kb = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if line.starts_with("VmRSS:") {
                vm_rss_kb = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
        }

        Ok(aiva_core::MemoryMetrics {
            total_mb: vm_size_kb / 1024,
            used_mb: vm_rss_kb / 1024,
            available_mb: (vm_size_kb - vm_rss_kb) / 1024,
            cache_mb: 0,
        })
    }

    async fn get_process_uptime(&self, pid: u32) -> Result<std::time::Duration> {
        // Get process start time
        let stat_path = format!("/proc/{pid}/stat");
        let stat_content = tokio::fs::read_to_string(&stat_path).await?;

        let fields: Vec<&str> = stat_content.split_whitespace().collect();
        if fields.len() > 21 {
            let starttime: u64 = fields[21].parse().unwrap_or(0);

            // Get system uptime
            let uptime_content = tokio::fs::read_to_string("/proc/uptime").await?;
            let system_uptime: f64 = uptime_content
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);

            let hz = 100.0; // Typical USER_HZ value
            let process_uptime = system_uptime - (starttime as f64 / hz);

            Ok(std::time::Duration::from_secs_f64(process_uptime.max(0.0)))
        } else {
            Ok(std::time::Duration::from_secs(0))
        }
    }

    async fn get_tap_device_metrics(
        &self,
        tap_device: &str,
    ) -> Result<aiva_core::NetworkIOMetrics> {
        // Read network statistics from /proc/net/dev
        let net_dev_content = tokio::fs::read_to_string("/proc/net/dev").await?;

        for line in net_dev_content.lines() {
            if line.contains(tap_device) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    // Format: interface: rx_bytes rx_packets ... tx_bytes tx_packets ...
                    let rx_bytes = parts[1].parse().unwrap_or(0);
                    let rx_packets = parts[2].parse().unwrap_or(0);
                    let tx_bytes = parts[9].parse().unwrap_or(0);
                    let tx_packets = parts[10].parse().unwrap_or(0);

                    return Ok(aiva_core::NetworkIOMetrics {
                        rx_bytes,
                        tx_bytes,
                        rx_packets,
                        tx_packets,
                    });
                }
            }
        }

        Ok(aiva_core::NetworkIOMetrics {
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
        })
    }
}

#[async_trait]
impl Platform for LinuxPlatform {
    async fn create_vm(&self, instance: &VMInstance) -> Result<VMInstance> {
        self.check_kvm_available()?;

        info!("Creating VM: {}", instance.name);

        // Prepare jailer workspace
        let workspace = self.prepare_jailer_workspace(instance).await?;

        // Spawn Firecracker process
        let child = self.spawn_firecracker(&workspace, instance).await?;
        let pid = child.id();

        // Configure VM through Firecracker API
        let api_client = crate::firecracker::FirecrackerApiClient::new(
            workspace.join("root").join("firecracker.socket"),
        )?;

        // Configure machine
        api_client
            .configure_machine(instance.config.cpus, instance.config.memory_mb)
            .await?;

        // Configure boot source
        api_client
            .configure_boot_source(
                &PathBuf::from("/vmlinux"),
                "console=ttyS0 reboot=k panic=1 pci=off",
            )
            .await?;

        // Configure root drive
        api_client
            .configure_drive("rootfs", &PathBuf::from("/rootfs.ext4"), false, "Writeback")
            .await?;

        // Configure network
        let tap_device = aiva_network::create_tap_device(&instance.name)?;
        api_client
            .configure_network("eth0", &tap_device, Some(&instance.config.network.guest_ip))
            .await?;

        // Start VM
        api_client.start_instance().await?;

        // Update instance with runtime info
        let mut updated_instance = instance.clone();
        updated_instance.runtime.pid = Some(pid);
        updated_instance.runtime.api_socket =
            Some(workspace.join("root").join("firecracker.socket"));
        updated_instance.runtime.tap_device = Some(tap_device);
        updated_instance.state = VMState::Running;

        info!("VM created successfully: {}", instance.name);

        Ok(updated_instance)
    }

    async fn start_vm(&self, instance: &VMInstance) -> Result<()> {
        debug!("Starting VM: {}", instance.name);

        // For Linux/Firecracker, VMs are created in running state
        // This would be used to resume a paused VM

        if let Some(socket_path) = &instance.runtime.api_socket {
            let api_client = crate::firecracker::FirecrackerApiClient::new(socket_path.clone())?;
            api_client.resume_vm().await?;
        }

        Ok(())
    }

    async fn stop_vm(&self, instance: &VMInstance, force: bool) -> Result<()> {
        debug!("Stopping VM: {} (force: {})", instance.name, force);

        if let Some(socket_path) = &instance.runtime.api_socket {
            let api_client = crate::firecracker::FirecrackerApiClient::new(socket_path.clone())?;

            if force {
                // Force shutdown
                if let Some(pid) = instance.runtime.pid {
                    use nix::sys::signal::{self, Signal};
                    use nix::unistd::Pid;

                    signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL).map_err(|e| {
                        AivaError::PlatformError {
                            platform: "linux".to_string(),
                            message: format!("Failed to kill process: {e}"),
                            recoverable: false,
                        }
                    })?;
                }
            } else {
                // Graceful shutdown
                api_client.shutdown_vm().await?;
            }
        }

        Ok(())
    }

    async fn delete_vm(&self, instance: &VMInstance) -> Result<()> {
        debug!("Deleting VM: {}", instance.name);

        // Remove jailer workspace
        let workspace = PathBuf::from("/tmp")
            .join("aiva-jailer")
            .join(instance.id.to_string());
        if workspace.exists() {
            std::fs::remove_dir_all(&workspace)?;
        }

        // Remove TAP device
        if let Some(tap_device) = &instance.runtime.tap_device {
            aiva_network::delete_tap_device(tap_device)?;
        }

        Ok(())
    }

    async fn get_vm_metrics(&self, instance: &VMInstance) -> Result<VMMetrics> {
        let logger = VMLogger::new(instance.name.clone());
        logger.info("Collecting VM metrics").await?;

        info!(
            "Getting metrics for VM {} (Linux - Firecracker)",
            instance.name
        );

        // Get metrics from Firecracker API if available
        if let Some(socket_path) = &instance.runtime.api_socket {
            if socket_path.exists() {
                let _api_client =
                    crate::firecracker::FirecrackerApiClient::new(socket_path.clone())?;

                // Try to get metrics from Firecracker
                // Note: This would need actual Firecracker metrics API implementation
                // For now, we'll collect host-level metrics for the VM process

                if let Some(pid) = instance.runtime.pid {
                    // Get process metrics
                    let cpu_usage = self.get_process_cpu_usage(pid).await?;
                    let memory_metrics = self.get_process_memory_usage(pid).await?;
                    let uptime = self.get_process_uptime(pid).await?;

                    // Get network metrics if TAP device exists
                    let network_metrics = if let Some(tap) = &instance.runtime.tap_device {
                        self.get_tap_device_metrics(tap).await?
                    } else {
                        aiva_core::NetworkIOMetrics {
                            rx_bytes: 0,
                            tx_bytes: 0,
                            rx_packets: 0,
                            tx_packets: 0,
                        }
                    };

                    logger
                        .info(&format!(
                            "Metrics collected: CPU {:.1}%, Memory {}/{} MB",
                            cpu_usage, memory_metrics.used_mb, memory_metrics.total_mb
                        ))
                        .await?;

                    return Ok(VMMetrics {
                        cpu_usage,
                        memory_usage: memory_metrics,
                        disk_io: aiva_core::DiskIOMetrics {
                            read_bytes: 0,
                            write_bytes: 0,
                            read_ops: 0,
                            write_ops: 0,
                        },
                        network_io: network_metrics,
                        uptime,
                    });
                }
            }
        }

        // Fallback to default metrics if we can't get real ones
        warn!(
            "Unable to collect real metrics for VM {}, using defaults",
            instance.name
        );
        Ok(VMMetrics {
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
        })
    }

    async fn execute_command(&self, instance: &VMInstance, command: &str) -> Result<String> {
        let logger = VMLogger::new(instance.name.clone());
        logger
            .info(&format!("Executing command: {command}"))
            .await?;

        info!(
            "Executing command in VM {} (Linux - Firecracker): {}",
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
            // Try to use vsock first, then fallback to network
            let connection_type = if self.check_vsock_support() {
                // For vsock, we need the VM's context ID (CID)
                // In a real implementation, this would be obtained from Firecracker
                let cid = 3; // Default guest CID, in production this would be dynamic
                ConnectionType::Vsock { cid }
            } else {
                // Use network connection through guest IP
                ConnectionType::Network {
                    host: instance.config.network.guest_ip.clone(),
                    port: VSOCK_COMMAND_PORT as u16,
                }
            };

            // Register the VM with the command pool
            if let Err(e) = command_pool
                .register_vm(instance.name.clone(), connection_type)
                .await
            {
                warn!("Failed to register VM in command pool: {}", e);

                // Fallback to SSH if available
                if let Some(port) = instance.config.network.port_mappings.first() {
                    let ssh_connection = ConnectionType::Ssh {
                        host: "localhost".to_string(),
                        port: port.host_port,
                        key_path: None,
                    };

                    command_pool
                        .register_vm(instance.name.clone(), ssh_connection)
                        .await?;
                } else {
                    return Err(e);
                }
            }
        }

        // Execute the command through the command pool
        let output = command_pool
            .execute_command(&instance.name, command)
            .await?;

        logger.info("Command executed successfully").await?;
        info!(
            "Command executed successfully in VM {} with Firecracker",
            instance.name
        );

        Ok(output)
    }

    async fn check_requirements(&self) -> Result<()> {
        // Check KVM
        self.check_kvm_available()?;

        // Check Firecracker binary
        if !self.firecracker_path.exists() {
            return Err(AivaError::PlatformError {
                platform: "linux".to_string(),
                message: format!(
                    "Firecracker not found at {}. Please install Firecracker.",
                    self.firecracker_path.display()
                ),
                recoverable: true,
            });
        }

        // Check jailer binary
        if !self.jailer_path.exists() {
            return Err(AivaError::PlatformError {
                platform: "linux".to_string(),
                message: format!(
                    "Jailer not found at {}. Please install Firecracker.",
                    self.jailer_path.display()
                ),
                recoverable: true,
            });
        }

        info!("Linux platform requirements satisfied");

        Ok(())
    }

    fn name(&self) -> &str {
        "linux"
    }
}
