use crate::error::*;
use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use uuid::Uuid;

#[async_trait]
pub trait VMManager: Send + Sync {
    async fn create_vm(&self, name: String, config: VMConfig) -> Result<VMInstance>;
    async fn start_vm(&self, id: &Uuid) -> Result<()>;
    async fn stop_vm(&self, id: &Uuid, force: bool) -> Result<()>;
    async fn delete_vm(&self, id: &Uuid) -> Result<()>;
    async fn get_vm(&self, id: &Uuid) -> Result<Option<VMInstance>>;
    async fn get_vm_by_name(&self, name: &str) -> Result<Option<VMInstance>>;
    async fn list_vms(&self) -> Result<Vec<VMInstance>>;
    async fn update_vm_state(&self, id: &Uuid, state: VMState) -> Result<()>;
    async fn get_vm_metrics(&self, id: &Uuid) -> Result<VMMetrics>;
    async fn execute_command(&self, id: &Uuid, command: &str) -> Result<String>;
    async fn force_reset_vm_state(&self, id: &Uuid, state: VMState) -> Result<()>;
    async fn reset_stuck_vms(&self) -> Result<Vec<(Uuid, VMState)>>;
}

pub struct VMOrchestrator {
    vms: Arc<RwLock<HashMap<Uuid, VMInstance>>>,
    platform: Arc<dyn Platform>,
    state_file: PathBuf,
}

impl VMOrchestrator {
    pub fn new(platform: Arc<dyn Platform>) -> Self {
        let state_file = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".aiva")
            .join("vm_state.json");

        Self {
            vms: Arc::new(RwLock::new(HashMap::new())),
            platform,
            state_file,
        }
    }

    pub async fn load_state(&self) -> Result<()> {
        if !self.state_file.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&self.state_file).await?;
        let vms: HashMap<Uuid, VMInstance> = serde_json::from_str(&content)?;

        *self.vms.write().await = vms;
        Ok(())
    }

    async fn save_state(&self) -> Result<()> {
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).await?;
        }

        let vms = self.vms.read().await;
        let content = serde_json::to_string_pretty(&*vms)?;
        fs::write(&self.state_file, content).await?;
        Ok(())
    }
}

#[async_trait]
impl VMManager for VMOrchestrator {
    async fn create_vm(&self, name: String, config: VMConfig) -> Result<VMInstance> {
        let id = Uuid::new_v4();
        let now = Utc::now();

        let instance = VMInstance {
            id,
            name: name.clone(),
            state: VMState::Creating,
            config,
            runtime: RuntimeInfo {
                pid: None,
                api_socket: None,
                vsock_cid: None,
                tap_device: None,
            },
            created_at: now,
            updated_at: now,
        };

        // Store the instance
        {
            let mut vms = self.vms.write().await;
            vms.insert(id, instance.clone());
        }

        // Create the VM through platform
        match self.platform.create_vm(&instance).await {
            Ok(updated_instance) => {
                {
                    let mut vms = self.vms.write().await;
                    vms.insert(id, updated_instance.clone());
                }
                self.save_state().await?;
                Ok(updated_instance)
            }
            Err(e) => {
                // Clean up on error
                {
                    let mut vms = self.vms.write().await;
                    vms.remove(&id);
                }
                self.save_state().await?;
                Err(e)
            }
        }
    }

    async fn start_vm(&self, id: &Uuid) -> Result<()> {
        let vm = {
            let vms = self.vms.read().await;
            vms.get(id).cloned()
        };

        let vm = vm.ok_or_else(|| AivaError::VMError {
            vm_name: id.to_string(),
            state: VMState::Stopped,
            message: "VM not found".to_string(),
        })?;

        if vm.state != VMState::Stopped {
            return Err(AivaError::InvalidStateTransition(format!(
                "Cannot start VM in state {:?}",
                vm.state
            )));
        }

        self.platform.start_vm(&vm).await?;
        self.update_vm_state(id, VMState::Running).await?;

        Ok(())
    }

    async fn stop_vm(&self, id: &Uuid, force: bool) -> Result<()> {
        let vm = {
            let vms = self.vms.read().await;
            vms.get(id).cloned()
        };

        let vm = vm.ok_or_else(|| AivaError::VMError {
            vm_name: id.to_string(),
            state: VMState::Stopped,
            message: "VM not found".to_string(),
        })?;

        if vm.state != VMState::Running && vm.state != VMState::Paused {
            return Err(AivaError::InvalidStateTransition(format!(
                "Cannot stop VM in state {:?}",
                vm.state
            )));
        }

        self.update_vm_state(id, VMState::Stopping).await?;

        // Use timeout to prevent hanging indefinitely
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.platform.stop_vm(&vm, force),
        )
        .await;

        match result {
            Ok(Ok(())) => {
                self.update_vm_state(id, VMState::Stopped).await?;
                Ok(())
            }
            Ok(Err(e)) => {
                // If platform stop fails, still mark as stopped to prevent being stuck
                self.update_vm_state(id, VMState::Stopped).await?;
                Err(e)
            }
            Err(_) => {
                // Timeout occurred, mark as stopped to prevent being stuck
                self.update_vm_state(id, VMState::Stopped).await?;
                Err(AivaError::PlatformError {
                    platform: "timeout".to_string(),
                    message: "VM stop operation timed out after 30 seconds".to_string(),
                    recoverable: true,
                })
            }
        }
    }

    async fn delete_vm(&self, id: &Uuid) -> Result<()> {
        let vm = {
            let vms = self.vms.read().await;
            vms.get(id).cloned()
        };

        let vm = vm.ok_or_else(|| AivaError::VMError {
            vm_name: id.to_string(),
            state: VMState::Stopped,
            message: "VM not found".to_string(),
        })?;

        if vm.state != VMState::Stopped {
            return Err(AivaError::InvalidStateTransition(format!(
                "Cannot delete VM in state {:?}",
                vm.state
            )));
        }

        self.platform.delete_vm(&vm).await?;

        {
            let mut vms = self.vms.write().await;
            vms.remove(id);
        }

        self.save_state().await?;

        Ok(())
    }

    async fn get_vm(&self, id: &Uuid) -> Result<Option<VMInstance>> {
        let vms = self.vms.read().await;
        Ok(vms.get(id).cloned())
    }

    async fn get_vm_by_name(&self, name: &str) -> Result<Option<VMInstance>> {
        let vms = self.vms.read().await;
        Ok(vms.values().find(|vm| vm.name == name).cloned())
    }

    async fn list_vms(&self) -> Result<Vec<VMInstance>> {
        let vms = self.vms.read().await;
        Ok(vms.values().cloned().collect())
    }

    async fn update_vm_state(&self, id: &Uuid, state: VMState) -> Result<()> {
        {
            let mut vms = self.vms.write().await;

            if let Some(vm) = vms.get_mut(id) {
                vm.state = state;
                vm.updated_at = Utc::now();
            } else {
                return Err(AivaError::VMError {
                    vm_name: id.to_string(),
                    state: VMState::Stopped,
                    message: "VM not found".to_string(),
                });
            }
        }
        self.save_state().await?;
        Ok(())
    }

    async fn get_vm_metrics(&self, id: &Uuid) -> Result<VMMetrics> {
        let vm = {
            let vms = self.vms.read().await;
            vms.get(id).cloned()
        };

        let vm = vm.ok_or_else(|| AivaError::VMError {
            vm_name: id.to_string(),
            state: VMState::Stopped,
            message: "VM not found".to_string(),
        })?;

        self.platform.get_vm_metrics(&vm).await
    }

    async fn execute_command(&self, id: &Uuid, command: &str) -> Result<String> {
        let vm = {
            let vms = self.vms.read().await;
            vms.get(id).cloned()
        };

        let vm = vm.ok_or_else(|| AivaError::VMError {
            vm_name: id.to_string(),
            state: VMState::Stopped,
            message: "VM not found".to_string(),
        })?;

        if vm.state != VMState::Running {
            return Err(AivaError::VMError {
                vm_name: vm.name,
                state: vm.state,
                message: "VM must be running to execute commands".to_string(),
            });
        }

        self.platform.execute_command(&vm, command).await
    }

    /// Force reset a VM's state - use with caution
    async fn force_reset_vm_state(&self, id: &Uuid, state: VMState) -> Result<()> {
        {
            let mut vms = self.vms.write().await;
            if let Some(vm) = vms.get_mut(id) {
                vm.state = state;
                vm.updated_at = Utc::now();
            } else {
                return Err(AivaError::VMError {
                    vm_name: id.to_string(),
                    state: VMState::Stopped,
                    message: "VM not found".to_string(),
                });
            }
        }
        self.save_state().await?;
        Ok(())
    }

    /// Reset VMs that are stuck in transitional states
    async fn reset_stuck_vms(&self) -> Result<Vec<(Uuid, VMState)>> {
        let mut reset_vms = Vec::new();
        let now = Utc::now();

        {
            let mut vms = self.vms.write().await;
            for (id, vm) in vms.iter_mut() {
                // If VM has been in a transitional state for more than 2 minutes, reset it
                if matches!(vm.state, VMState::Creating | VMState::Stopping) {
                    let duration = now.signed_duration_since(vm.updated_at);
                    if duration.num_seconds() > 120 {
                        let old_state = vm.state;
                        vm.state = match old_state {
                            VMState::Creating => VMState::Stopped,
                            VMState::Stopping => VMState::Stopped,
                            _ => old_state,
                        };
                        vm.updated_at = now;
                        reset_vms.push((*id, old_state));
                    }
                }
            }
        }

        if !reset_vms.is_empty() {
            self.save_state().await?;
        }

        Ok(reset_vms)
    }
}

#[async_trait]
pub trait Platform: Send + Sync {
    async fn create_vm(&self, instance: &VMInstance) -> Result<VMInstance>;
    async fn start_vm(&self, instance: &VMInstance) -> Result<()>;
    async fn stop_vm(&self, instance: &VMInstance, force: bool) -> Result<()>;
    async fn delete_vm(&self, instance: &VMInstance) -> Result<()>;
    async fn get_vm_metrics(&self, instance: &VMInstance) -> Result<VMMetrics>;
    async fn execute_command(&self, instance: &VMInstance, command: &str) -> Result<String>;
    async fn check_requirements(&self) -> Result<()>;
    fn name(&self) -> &str;
}
