use aiva_core::{AivaError, Result};
use std::time::Duration;
use tracing::debug;

/// Vsock port used for command execution
pub const VSOCK_COMMAND_PORT: u32 = 5555;

/// Command execution through vsock or network connection
pub struct VsockExecutor {
    _vm_name: String,
    connection_type: ConnectionType,
}

#[derive(Debug, Clone)]
pub enum ConnectionType {
    /// Direct vsock connection (Linux with vsock support)
    Vsock { cid: u32 },
    /// Network connection through port forwarding
    Network { host: String, port: u16 },
    /// SSH connection for fallback
    Ssh {
        host: String,
        port: u16,
        key_path: Option<String>,
    },
}

impl VsockExecutor {
    pub fn new(vm_name: String, connection_type: ConnectionType) -> Self {
        Self {
            _vm_name: vm_name,
            connection_type,
        }
    }

    /// Execute a command in the VM and return the output
    pub async fn execute_command(&self, command: &str) -> Result<String> {
        match &self.connection_type {
            ConnectionType::Vsock { cid } => self.execute_vsock(*cid, command).await,
            ConnectionType::Network { host, port } => {
                self.execute_network(host, *port, command).await
            }
            ConnectionType::Ssh {
                host,
                port,
                key_path,
            } => {
                self.execute_ssh(host, *port, key_path.as_deref(), command)
                    .await
            }
        }
    }

    /// Execute command through vsock (Linux only)
    async fn execute_vsock(&self, _cid: u32, _command: &str) -> Result<String> {
        #[cfg(target_os = "linux")]
        {
            debug!("Executing command via vsock CID {}: {}", _cid, _command);

            // For now, we'll use a network connection as vsock support requires special setup
            // In production, this would use actual vsock sockets
            return self
                .execute_network("localhost", VSOCK_COMMAND_PORT as u16, _command)
                .await;
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(AivaError::PlatformError {
                platform: "vsock".to_string(),
                message: "Vsock is only supported on Linux".to_string(),
                recoverable: false,
            })
        }
    }

    /// Execute command through network connection
    async fn execute_network(&self, host: &str, port: u16, command: &str) -> Result<String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        debug!(
            "Executing command via network {}:{}: {}",
            host, port, command
        );

        // Try to connect with timeout
        let addr = format!("{host}:{port}");
        let stream = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(&addr))
            .await
            .map_err(|_| AivaError::NetworkError {
                operation: "connect".to_string(),
                cause: format!("Connection to {addr} timed out"),
            })?
            .map_err(|e| AivaError::NetworkError {
                operation: "connect".to_string(),
                cause: format!("Failed to connect to {addr}: {e}"),
            })?;

        // Send command
        let mut stream = stream;
        stream
            .write_all(command.as_bytes())
            .await
            .map_err(|e| AivaError::NetworkError {
                operation: "send_command".to_string(),
                cause: format!("Failed to send command: {e}"),
            })?;
        stream
            .write_all(b"\n")
            .await
            .map_err(|e| AivaError::NetworkError {
                operation: "send_newline".to_string(),
                cause: format!("Failed to send newline: {e}"),
            })?;

        // Read response
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .map_err(|e| AivaError::NetworkError {
                operation: "read_response".to_string(),
                cause: format!("Failed to read response: {e}"),
            })?;

        Ok(response)
    }

    /// Execute command through SSH
    async fn execute_ssh(
        &self,
        host: &str,
        port: u16,
        key_path: Option<&str>,
        command: &str,
    ) -> Result<String> {
        debug!("Executing command via SSH {}:{}: {}", host, port, command);

        let mut ssh_cmd = tokio::process::Command::new("ssh");
        ssh_cmd
            .arg("-o")
            .arg("StrictHostKeyChecking=no")
            .arg("-o")
            .arg("UserKnownHostsFile=/dev/null")
            .arg("-o")
            .arg("LogLevel=ERROR")
            .arg("-p")
            .arg(port.to_string());

        if let Some(key) = key_path {
            ssh_cmd.arg("-i").arg(key);
        }

        ssh_cmd.arg(format!("root@{host}")).arg(command);

        let output = ssh_cmd
            .output()
            .await
            .map_err(|e| AivaError::PlatformError {
                platform: "ssh".to_string(),
                message: format!("Failed to execute SSH command: {e}"),
                recoverable: true,
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AivaError::PlatformError {
                platform: "ssh".to_string(),
                message: format!("SSH command failed: {stderr}"),
                recoverable: true,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Check if the executor can connect to the VM
    pub async fn check_connection(&self) -> Result<bool> {
        match self.execute_command("echo 'connection_test'").await {
            Ok(output) => Ok(output.contains("connection_test")),
            Err(_) => Ok(false),
        }
    }
}
