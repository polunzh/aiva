use aiva_core::{AivaError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

pub use crate::vsock_executor::{ConnectionType, VsockExecutor};

/// Manages command executors for VMs
pub struct CommandPool {
    executors: Arc<RwLock<HashMap<String, Arc<VsockExecutor>>>>,
}

impl CommandPool {
    pub fn new() -> Self {
        Self {
            executors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a VM with its executor
    pub async fn register_vm(
        &self,
        vm_name: String,
        connection_type: ConnectionType,
    ) -> Result<()> {
        let executor = Arc::new(VsockExecutor::new(vm_name.clone(), connection_type));

        // Test connection before registering
        if !executor.check_connection().await? {
            return Err(AivaError::NetworkError {
                operation: "register_vm".to_string(),
                cause: format!("Failed to establish connection to VM {vm_name}"),
            });
        }

        let mut executors = self.executors.write().await;
        executors.insert(vm_name.clone(), executor);

        info!("Registered VM {} in command pool", vm_name);
        Ok(())
    }

    /// Execute a command on a specific VM
    pub async fn execute_command(&self, vm_name: &str, command: &str) -> Result<String> {
        let executors = self.executors.read().await;

        let executor = executors.get(vm_name).ok_or_else(|| AivaError::VMError {
            vm_name: vm_name.to_string(),
            state: aiva_core::VMState::Stopped,
            message: "VM not registered in command pool".to_string(),
        })?;

        debug!("Executing command on VM {}: {}", vm_name, command);
        executor.execute_command(command).await
    }

    /// Remove a VM from the pool
    pub async fn unregister_vm(&self, vm_name: &str) -> Result<()> {
        let mut executors = self.executors.write().await;
        executors.remove(vm_name);

        info!("Unregistered VM {} from command pool", vm_name);
        Ok(())
    }

    /// Check if a VM is registered
    pub async fn is_registered(&self, vm_name: &str) -> bool {
        let executors = self.executors.read().await;
        executors.contains_key(vm_name)
    }

    /// Get all registered VMs
    pub async fn list_vms(&self) -> Vec<String> {
        let executors = self.executors.read().await;
        executors.keys().cloned().collect()
    }
}

impl Default for CommandPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Global command pool instance
static COMMAND_POOL: once_cell::sync::Lazy<CommandPool> =
    once_cell::sync::Lazy::new(CommandPool::new);

/// Get the global command pool
pub fn get_command_pool() -> &'static CommandPool {
    &COMMAND_POOL
}
