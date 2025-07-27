use crate::firecracker::FirecrackerApiClient;
use aiva_core::{AivaError, Result, VMState, VMTemplate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirecrackerVMConfig {
    pub vm_id: String,
    pub socket_path: PathBuf,
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub vcpu_count: u32,
    pub mem_size_mib: u64,
    pub tap_device: String,
    pub guest_ip: String,
    pub network_interface: String,
}

pub struct FirecrackerVM {
    config: FirecrackerVMConfig,
    #[allow(dead_code)] // Used for direct API access when not running through Lima
    api_client: Option<FirecrackerApiClient>,
    #[allow(dead_code)] // Used for process management when not running through Lima
    firecracker_process: Option<Child>,
    pub state: VMState,
}

impl FirecrackerVM {
    #[allow(dead_code)]
    pub fn new(config: FirecrackerVMConfig) -> Self {
        Self {
            config,
            api_client: None,
            firecracker_process: None,
            state: VMState::Stopped,
        }
    }

    #[allow(dead_code)] // Used for direct rootfs creation when not using Lima
    pub async fn create_rootfs_from_template(
        &self,
        template: &VMTemplate,
        base_rootfs_path: &Path,
    ) -> Result<PathBuf> {
        let rootfs_path = self.config.rootfs_path.clone();

        info!(
            "Creating rootfs for VM {} from template {}",
            self.config.vm_id, template.name
        );

        // Copy base rootfs
        tokio::fs::copy(base_rootfs_path, &rootfs_path)
            .await
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to copy base rootfs: {e}"),
                recoverable: false,
            })?;

        // Resize rootfs to template size
        let disk_size_gb = template.base_config.disk_gb;
        self.resize_rootfs(&rootfs_path, disk_size_gb).await?;

        // Mount and customize rootfs
        self.customize_rootfs(&rootfs_path, template).await?;

        Ok(rootfs_path)
    }

    #[allow(dead_code)] // Helper method for rootfs creation
    async fn resize_rootfs(&self, rootfs_path: &Path, size_gb: u64) -> Result<()> {
        debug!("Resizing rootfs to {}GB", size_gb);

        let output = Command::new("truncate")
            .args(["-s", &format!("{size_gb}G")])
            .arg(rootfs_path)
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to resize rootfs: {e}"),
                recoverable: false,
            })?;

        if !output.status.success() {
            return Err(AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!(
                    "Failed to resize rootfs: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                recoverable: false,
            });
        }

        // Run e2fsck and resize2fs
        let commands = [
            vec!["e2fsck", "-f", "-y", rootfs_path.to_str().unwrap()],
            vec!["resize2fs", rootfs_path.to_str().unwrap()],
        ];

        for cmd in &commands {
            let output = Command::new(cmd[0]).args(&cmd[1..]).output().map_err(|e| {
                AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Failed to run {}: {}", cmd[0], e),
                    recoverable: false,
                }
            })?;

            if !output.status.success() {
                warn!(
                    "Command {} returned non-zero status: {}",
                    cmd[0],
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }

        Ok(())
    }

    #[allow(dead_code)] // Helper method for rootfs customization
    async fn customize_rootfs(&self, rootfs_path: &Path, template: &VMTemplate) -> Result<()> {
        debug!("Customizing rootfs with template: {}", template.name);

        let mount_dir = format!("/tmp/aiva-mount-{}", self.config.vm_id);

        // Create mount point
        tokio::fs::create_dir_all(&mount_dir)
            .await
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to create mount directory: {e}"),
                recoverable: false,
            })?;

        // Mount the rootfs
        let mount_output = Command::new("sudo")
            .args([
                "mount",
                "-o",
                "loop",
                rootfs_path.to_str().unwrap(),
                &mount_dir,
            ])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to mount rootfs: {e}"),
                recoverable: false,
            })?;

        if !mount_output.status.success() {
            return Err(AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!(
                    "Failed to mount rootfs: {}",
                    String::from_utf8_lossy(&mount_output.stderr)
                ),
                recoverable: false,
            });
        }

        // Run setup script inside chroot
        let setup_script = template.get_setup_script();
        let script_path = format!("{mount_dir}/tmp/setup.sh");

        tokio::fs::write(&script_path, setup_script)
            .await
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to write setup script: {e}"),
                recoverable: false,
            })?;

        // Make script executable and run it
        let chmod_output = Command::new("sudo")
            .args(["chmod", "+x", &script_path])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to make script executable: {e}"),
                recoverable: false,
            })?;

        if !chmod_output.status.success() {
            warn!(
                "Failed to chmod setup script: {}",
                String::from_utf8_lossy(&chmod_output.stderr)
            );
        }

        // Run the setup script in chroot
        let chroot_output = Command::new("sudo")
            .args(["chroot", &mount_dir, "/tmp/setup.sh"])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to run setup script: {e}"),
                recoverable: false,
            })?;

        if !chroot_output.status.success() {
            warn!(
                "Setup script returned non-zero status: {}",
                String::from_utf8_lossy(&chroot_output.stderr)
            );
        } else {
            debug!("Setup script completed successfully");
        }

        // Unmount
        let umount_output = Command::new("sudo")
            .args(["umount", &mount_dir])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to unmount rootfs: {e}"),
                recoverable: false,
            })?;

        if !umount_output.status.success() {
            warn!(
                "Failed to unmount rootfs: {}",
                String::from_utf8_lossy(&umount_output.stderr)
            );
        }

        // Remove mount directory
        let _ = tokio::fs::remove_dir(&mount_dir).await;

        Ok(())
    }

    #[allow(dead_code)] // Used for direct TAP setup when not using Lima
    pub async fn setup_tap_device(&self) -> Result<()> {
        debug!("Setting up TAP device: {}", self.config.tap_device);

        // Create TAP device
        let tap_commands = [
            vec![
                "sudo",
                "ip",
                "tuntap",
                "add",
                &self.config.tap_device,
                "mode",
                "tap",
            ],
            vec![
                "sudo",
                "ip",
                "addr",
                "add",
                "172.16.0.1/24",
                "dev",
                &self.config.tap_device,
            ],
            vec![
                "sudo",
                "ip",
                "link",
                "set",
                "dev",
                &self.config.tap_device,
                "up",
            ],
        ];

        for cmd in &tap_commands {
            let output = Command::new(cmd[0]).args(&cmd[1..]).output().map_err(|e| {
                AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Failed to setup TAP device: {e}"),
                    recoverable: false,
                }
            })?;

            if !output.status.success() {
                error!(
                    "TAP setup command failed: {} - {}",
                    cmd.join(" "),
                    String::from_utf8_lossy(&output.stderr)
                );
                return Err(AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!(
                        "Failed to setup TAP device: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    recoverable: false,
                });
            }
        }

        Ok(())
    }

    #[allow(dead_code)] // Used for direct Firecracker management when not using Lima
    pub async fn start_firecracker(&mut self) -> Result<()> {
        info!("Starting Firecracker process for VM: {}", self.config.vm_id);

        // Remove socket if it exists
        if self.config.socket_path.exists() {
            tokio::fs::remove_file(&self.config.socket_path)
                .await
                .map_err(|e| AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Failed to remove existing socket: {e}"),
                    recoverable: false,
                })?;
        }

        // Start Firecracker process
        let mut child = Command::new("firecracker")
            .arg("--api-sock")
            .arg(&self.config.socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to start Firecracker: {e}"),
                recoverable: false,
            })?;

        // Wait for socket to be created
        let mut attempts = 0;
        while !self.config.socket_path.exists() && attempts < 50 {
            sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }

        if !self.config.socket_path.exists() {
            let _ = child.kill();
            return Err(AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: "Firecracker socket was not created within timeout".to_string(),
                recoverable: false,
            });
        }

        self.firecracker_process = Some(child);
        self.api_client = Some(FirecrackerApiClient::new(self.config.socket_path.clone())?);
        self.state = VMState::Creating;

        debug!("Firecracker process started, socket available");
        Ok(())
    }

    #[allow(dead_code)] // Used for direct VM configuration when not using Lima
    pub async fn configure_vm(&mut self) -> Result<()> {
        let client = self
            .api_client
            .as_ref()
            .ok_or_else(|| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: "Firecracker API client not initialized".to_string(),
                recoverable: false,
            })?;

        info!("Configuring Firecracker VM: {}", self.config.vm_id);

        // Configure machine
        client
            .configure_machine(self.config.vcpu_count, self.config.mem_size_mib)
            .await?;

        // Configure boot source
        let boot_args = format!(
            "console=ttyS0 reboot=k panic=1 pci=off ip={}::172.16.0.1:255.255.255.0::eth0:off",
            self.config.guest_ip
        );
        client
            .configure_boot_source(&self.config.kernel_path, &boot_args)
            .await?;

        // Configure root drive
        client
            .configure_drive("rootfs", &self.config.rootfs_path, false, "Unsafe")
            .await?;

        // Configure network interface
        client
            .configure_network("eth0", &self.config.tap_device, Some(&self.config.guest_ip))
            .await?;

        debug!("Firecracker VM configuration completed");
        Ok(())
    }

    #[allow(dead_code)] // Used for direct VM start when not using Lima
    pub async fn start_vm(&mut self) -> Result<()> {
        let client = self
            .api_client
            .as_ref()
            .ok_or_else(|| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: "Firecracker API client not initialized".to_string(),
                recoverable: false,
            })?;

        info!("Starting Firecracker VM: {}", self.config.vm_id);

        client.start_instance().await?;
        self.state = VMState::Running;

        // Wait a bit for the VM to boot
        sleep(Duration::from_secs(5)).await;

        debug!("Firecracker VM started successfully");
        Ok(())
    }

    #[allow(dead_code)] // Used for direct VM stop when not using Lima
    pub async fn stop_vm(&mut self) -> Result<()> {
        info!("Stopping Firecracker VM: {}", self.config.vm_id);

        // Try graceful shutdown first
        if let Some(client) = &self.api_client {
            let _ = client.shutdown_vm().await;
            sleep(Duration::from_secs(5)).await;
        }

        // Kill the Firecracker process
        if let Some(mut child) = self.firecracker_process.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        // Clean up TAP device
        self.cleanup_tap_device().await;

        // Remove socket
        if self.config.socket_path.exists() {
            let _ = tokio::fs::remove_file(&self.config.socket_path).await;
        }

        self.state = VMState::Stopped;
        self.api_client = None;

        debug!("Firecracker VM stopped");
        Ok(())
    }

    #[allow(dead_code)] // Helper method for TAP cleanup
    async fn cleanup_tap_device(&self) {
        debug!("Cleaning up TAP device: {}", self.config.tap_device);

        let _ = Command::new("sudo")
            .args(["ip", "link", "delete", &self.config.tap_device])
            .output();
    }

    #[allow(dead_code)] // Used for direct command execution when not using Lima
    pub async fn execute_command(&self, command: &str) -> Result<String> {
        if self.state != VMState::Running {
            return Err(AivaError::VMError {
                vm_name: self.config.vm_id.clone(),
                state: self.state,
                message: "VM is not running".to_string(),
            });
        }

        debug!("Executing command in VM {}: {}", self.config.vm_id, command);

        // For now, use SSH to execute commands in the VM
        // In a production environment, you might use the Firecracker agent or other mechanisms
        let ssh_output = Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=10",
                &format!("root@{}", self.config.guest_ip),
                command,
            ])
            .output()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to execute SSH command: {e}"),
                recoverable: true,
            })?;

        if !ssh_output.status.success() {
            let stderr = String::from_utf8_lossy(&ssh_output.stderr);
            return Err(AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("SSH command failed: {stderr}"),
                recoverable: true,
            });
        }

        Ok(String::from_utf8_lossy(&ssh_output.stdout).to_string())
    }

    #[allow(dead_code)]
    pub fn get_config(&self) -> &FirecrackerVMConfig {
        &self.config
    }

    #[allow(dead_code)] // Used for direct state access when not using Lima
    pub fn get_state(&self) -> VMState {
        self.state
    }
}
