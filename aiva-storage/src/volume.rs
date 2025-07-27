use crate::{BlockDeviceInfo, StorageBackend, Volume, VolumeConfig, VolumeFormat};
use aiva_core::{AivaError, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

pub struct VolumeManager {
    storage_path: PathBuf,
    volumes: Arc<RwLock<HashMap<String, Volume>>>,
    backend: Box<dyn StorageBackend>,
}

impl VolumeManager {
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        let backend = Box::new(LocalStorageBackend::new(storage_path.clone()));
        Ok(Self {
            storage_path,
            volumes: Arc::new(RwLock::new(HashMap::new())),
            backend,
        })
    }

    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.storage_path).await?;
        fs::create_dir_all(self.storage_path.join("volumes")).await?;
        self.load_volumes().await?;
        Ok(())
    }

    pub async fn create_volume(&self, config: VolumeConfig) -> Result<Volume> {
        info!(
            "Creating volume {} with size {}MB",
            config.name, config.size_mb
        );

        let volume = self.backend.create_volume(&config).await?;
        self.volumes
            .write()
            .await
            .insert(volume.id.clone(), volume.clone());
        self.save_metadata().await?;

        Ok(volume)
    }

    pub async fn delete_volume(&self, volume_id: &str) -> Result<()> {
        info!("Deleting volume {}", volume_id);

        // Check if volume is attached
        {
            let volumes = self.volumes.read().await;
            if let Some(volume) = volumes.get(volume_id) {
                if volume.attached_to.is_some() {
                    return Err(AivaError::StorageError(
                        "Cannot delete attached volume".to_string(),
                    ));
                }
            }
        }

        self.backend.delete_volume(volume_id).await?;
        self.volumes.write().await.remove(volume_id);
        self.save_metadata().await?;

        Ok(())
    }

    pub async fn attach_volume(&self, volume_id: &str, vm_id: &str) -> Result<BlockDeviceInfo> {
        info!("Attaching volume {} to VM {}", volume_id, vm_id);

        // Check if volume exists and is not already attached
        {
            let volumes = self.volumes.read().await;
            if let Some(volume) = volumes.get(volume_id) {
                if let Some(attached_to) = &volume.attached_to {
                    return Err(AivaError::StorageError(format!(
                        "Volume already attached to {attached_to}"
                    )));
                }
            } else {
                return Err(AivaError::StorageError(format!(
                    "Volume {volume_id} not found"
                )));
            }
        }

        let info = self.backend.attach_volume(volume_id, vm_id).await?;

        // Update volume state
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(volume_id) {
            volume.attached_to = Some(vm_id.to_string());
        }

        self.save_metadata().await?;
        Ok(info)
    }

    pub async fn detach_volume(&self, volume_id: &str) -> Result<()> {
        info!("Detaching volume {}", volume_id);

        self.backend.detach_volume(volume_id).await?;

        // Update volume state
        let mut volumes = self.volumes.write().await;
        if let Some(volume) = volumes.get_mut(volume_id) {
            volume.attached_to = None;
        }

        self.save_metadata().await?;
        Ok(())
    }

    pub async fn list_volumes(&self) -> Result<Vec<Volume>> {
        let volumes = self.volumes.read().await;
        Ok(volumes.values().cloned().collect())
    }

    pub async fn get_volume(&self, volume_id: &str) -> Result<Volume> {
        let volumes = self.volumes.read().await;
        volumes
            .get(volume_id)
            .cloned()
            .ok_or_else(|| AivaError::StorageError(format!("Volume {volume_id} not found")))
    }

    async fn load_volumes(&self) -> Result<()> {
        let metadata_path = self.storage_path.join("volumes.json");

        if metadata_path.exists() {
            let content = fs::read_to_string(&metadata_path).await?;
            let volumes: HashMap<String, Volume> = serde_json::from_str(&content)?;
            *self.volumes.write().await = volumes;
            debug!(
                "Loaded {} volumes from metadata",
                self.volumes.read().await.len()
            );
        }

        Ok(())
    }

    async fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.storage_path.join("volumes.json");
        let volumes = self.volumes.read().await;
        let content = serde_json::to_string_pretty(&*volumes)?;
        fs::write(&metadata_path, content).await?;
        Ok(())
    }
}

struct LocalStorageBackend {
    storage_path: PathBuf,
}

impl LocalStorageBackend {
    fn new(storage_path: PathBuf) -> Self {
        Self { storage_path }
    }

    async fn create_sparse_file(&self, path: &std::path::Path, size_mb: u64) -> Result<()> {
        use tokio::process::Command;

        let output = Command::new("dd")
            .args([
                "if=/dev/zero",
                &format!("of={}", path.display()),
                "bs=1M",
                "count=0",
                &format!("seek={size_mb}"),
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(AivaError::StorageError(format!(
                "Failed to create sparse file: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    async fn format_volume(&self, path: &std::path::Path, format: VolumeFormat) -> Result<()> {
        use tokio::process::Command;

        match format {
            VolumeFormat::Raw => {
                // Raw format doesn't need formatting
                Ok(())
            }
            VolumeFormat::Ext4 => {
                let output = Command::new("mkfs.ext4")
                    .args(["-F", &path.to_string_lossy()])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(AivaError::StorageError(format!(
                        "Failed to format as ext4: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
                Ok(())
            }
            VolumeFormat::Qcow2 => {
                // For qcow2, we need to use qemu-img instead
                let output = Command::new("qemu-img")
                    .args([
                        "create",
                        "-f",
                        "qcow2",
                        &path.to_string_lossy(),
                        &format!("{}M", path.metadata().unwrap().len() / (1024 * 1024)),
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(AivaError::StorageError(format!(
                        "Failed to create qcow2: {}",
                        String::from_utf8_lossy(&output.stderr)
                    )));
                }
                Ok(())
            }
        }
    }
}

#[async_trait]
impl StorageBackend for LocalStorageBackend {
    async fn create_volume(&self, config: &VolumeConfig) -> Result<Volume> {
        let volume_id = Uuid::new_v4().to_string();
        let volume_path = self.storage_path.join("volumes").join(&volume_id);

        // Create volume file
        if config.sparse {
            self.create_sparse_file(&volume_path, config.size_mb)
                .await?;
        } else {
            // Create full file
            let file = fs::File::create(&volume_path).await?;
            file.set_len(config.size_mb * 1024 * 1024).await?;
        }

        // Format if needed
        if config.format != VolumeFormat::Raw {
            self.format_volume(&volume_path, config.format).await?;
        }

        Ok(Volume {
            id: volume_id,
            name: config.name.clone(),
            size_mb: config.size_mb,
            path: volume_path,
            format: config.format,
            attached_to: None,
            created_at: Utc::now(),
        })
    }

    async fn delete_volume(&self, volume_id: &str) -> Result<()> {
        let volume_path = self.storage_path.join("volumes").join(volume_id);
        if volume_path.exists() {
            fs::remove_file(&volume_path).await?;
        }
        Ok(())
    }

    async fn attach_volume(&self, volume_id: &str, _vm_id: &str) -> Result<BlockDeviceInfo> {
        let volume_path = self.storage_path.join("volumes").join(volume_id);

        if !volume_path.exists() {
            return Err(AivaError::StorageError(format!(
                "Volume file not found: {volume_path:?}"
            )));
        }

        // In a real implementation, this would create a block device mapping
        // For now, we just return the path
        Ok(BlockDeviceInfo {
            path: volume_path,
            device: format!("/dev/vd{}", volume_id.chars().next().unwrap_or('a')),
            read_only: false,
        })
    }

    async fn detach_volume(&self, _volume_id: &str) -> Result<()> {
        // In a real implementation, this would remove the block device mapping
        Ok(())
    }

    async fn list_volumes(&self) -> Result<Vec<Volume>> {
        // This is handled by VolumeManager
        Ok(vec![])
    }

    async fn get_volume(&self, _volume_id: &str) -> Result<Volume> {
        // This is handled by VolumeManager
        Err(AivaError::NotImplemented(
            "get_volume should be called on VolumeManager".to_string(),
        ))
    }
}
