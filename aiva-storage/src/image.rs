use crate::{ImageFormat, ImageInfo, ImageSource};
use aiva_core::{AivaError, Result};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

#[async_trait]
pub trait ImageBackend: Send + Sync {
    async fn pull(&self, source: &ImageSource, path: &std::path::Path) -> Result<()>;
    async fn push(&self, path: &std::path::Path, destination: &ImageSource) -> Result<()>;
    async fn convert(
        &self,
        input: &std::path::Path,
        output: &std::path::Path,
        format: ImageFormat,
    ) -> Result<()>;
}

pub struct ImageManager {
    storage_path: PathBuf,
    images: Arc<RwLock<HashMap<String, ImageInfo>>>,
    backend: Box<dyn ImageBackend>,
}

impl ImageManager {
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        let backend = Box::new(LocalImageBackend::new());
        Ok(Self {
            storage_path,
            images: Arc::new(RwLock::new(HashMap::new())),
            backend,
        })
    }

    pub async fn init(&self) -> Result<()> {
        fs::create_dir_all(&self.storage_path).await?;
        fs::create_dir_all(self.storage_path.join("images")).await?;
        fs::create_dir_all(self.storage_path.join("cache")).await?;
        self.load_images().await?;
        Ok(())
    }

    pub async fn pull_image(&self, name: &str, source: ImageSource) -> Result<ImageInfo> {
        info!("Pulling image {} from {:?}", name, source);

        let image_id = Uuid::new_v4().to_string();
        let image_path = self.storage_path.join("images").join(&image_id);

        self.backend.pull(&source, &image_path).await?;

        let size_mb = fs::metadata(&image_path).await?.len() / (1024 * 1024);
        let format = Self::detect_format(&image_path)?;

        let info = ImageInfo {
            id: image_id.clone(),
            name: name.to_string(),
            size_mb,
            format,
            source,
            created_at: Utc::now(),
        };

        self.images.write().await.insert(image_id, info.clone());
        self.save_metadata().await?;

        Ok(info)
    }

    pub async fn create_from_rootfs(&self, name: &str, rootfs_path: &PathBuf) -> Result<ImageInfo> {
        info!("Creating image {} from rootfs {:?}", name, rootfs_path);

        let image_id = Uuid::new_v4().to_string();
        let image_path = self.storage_path.join("images").join(&image_id);

        self.create_ext4_image(rootfs_path, &image_path).await?;

        let size_mb = fs::metadata(&image_path).await?.len() / (1024 * 1024);

        let info = ImageInfo {
            id: image_id.clone(),
            name: name.to_string(),
            size_mb,
            format: ImageFormat::Raw,
            source: ImageSource::Local(rootfs_path.clone()),
            created_at: Utc::now(),
        };

        self.images.write().await.insert(image_id, info.clone());
        self.save_metadata().await?;

        Ok(info)
    }

    pub async fn delete_image(&self, image_id: &str) -> Result<()> {
        let mut images = self.images.write().await;

        if let Some(info) = images.remove(image_id) {
            let image_path = self.storage_path.join("images").join(image_id);
            if image_path.exists() {
                fs::remove_file(&image_path).await?;
            }
            info!("Deleted image {}", info.name);
            self.save_metadata().await?;
            Ok(())
        } else {
            Err(AivaError::StorageError(format!(
                "Image {image_id} not found"
            )))
        }
    }

    pub async fn list_images(&self) -> Result<Vec<ImageInfo>> {
        let images = self.images.read().await;
        Ok(images.values().cloned().collect())
    }

    pub async fn get_image(&self, image_id: &str) -> Result<ImageInfo> {
        let images = self.images.read().await;
        images
            .get(image_id)
            .cloned()
            .ok_or_else(|| AivaError::StorageError(format!("Image {image_id} not found")))
    }

    pub async fn get_image_path(&self, image_id: &str) -> Result<PathBuf> {
        let images = self.images.read().await;
        if images.contains_key(image_id) {
            Ok(self.storage_path.join("images").join(image_id))
        } else {
            Err(AivaError::StorageError(format!(
                "Image {image_id} not found"
            )))
        }
    }

    async fn create_ext4_image(
        &self,
        _rootfs_path: &std::path::Path,
        output_path: &std::path::Path,
    ) -> Result<()> {
        use tokio::process::Command;

        // Create sparse file
        let output = Command::new("dd")
            .args([
                "if=/dev/zero",
                &format!("of={}", output_path.display()),
                "bs=1M",
                "count=0",
                "seek=4096", // 4GB sparse file
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(AivaError::StorageError(
                "Failed to create sparse file".to_string(),
            ));
        }

        // Create ext4 filesystem
        let output = Command::new("mkfs.ext4")
            .args(["-F", &output_path.to_string_lossy()])
            .output()
            .await?;

        if !output.status.success() {
            return Err(AivaError::StorageError(
                "Failed to create ext4 filesystem".to_string(),
            ));
        }

        // Mount and copy rootfs
        let mount_point = self.storage_path.join("tmp_mount");
        fs::create_dir_all(&mount_point).await?;

        // This would require elevated privileges
        warn!("Rootfs copy requires elevated privileges, skipping for now");

        Ok(())
    }

    fn detect_format(path: &std::path::Path) -> Result<ImageFormat> {
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        match extension.to_lowercase().as_str() {
            "qcow2" => Ok(ImageFormat::Qcow2),
            "vmdk" => Ok(ImageFormat::Vmdk),
            "vhd" | "vhdx" => Ok(ImageFormat::Vhd),
            _ => Ok(ImageFormat::Raw),
        }
    }

    async fn load_images(&self) -> Result<()> {
        let metadata_path = self.storage_path.join("images.json");

        if metadata_path.exists() {
            let content = fs::read_to_string(&metadata_path).await?;
            let images: HashMap<String, ImageInfo> = serde_json::from_str(&content)?;
            *self.images.write().await = images;
            debug!(
                "Loaded {} images from metadata",
                self.images.read().await.len()
            );
        }

        Ok(())
    }

    async fn save_metadata(&self) -> Result<()> {
        let metadata_path = self.storage_path.join("images.json");
        let images = self.images.read().await;
        let content = serde_json::to_string_pretty(&*images)?;
        fs::write(&metadata_path, content).await?;
        Ok(())
    }
}

struct LocalImageBackend {
    client: reqwest::Client,
}

impl LocalImageBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl ImageBackend for LocalImageBackend {
    async fn pull(&self, source: &ImageSource, path: &std::path::Path) -> Result<()> {
        match source {
            ImageSource::Url(url) => {
                info!("Downloading image from {}", url);

                let response =
                    self.client
                        .get(url)
                        .send()
                        .await
                        .map_err(|e| AivaError::NetworkError {
                            operation: "image download".to_string(),
                            cause: e.to_string(),
                        })?;

                if !response.status().is_success() {
                    return Err(AivaError::NetworkError {
                        operation: "image download".to_string(),
                        cause: format!("HTTP {}", response.status()),
                    });
                }

                let mut file = fs::File::create(path).await?;
                let content = response
                    .bytes()
                    .await
                    .map_err(|e| AivaError::NetworkError {
                        operation: "image download".to_string(),
                        cause: e.to_string(),
                    })?;

                file.write_all(&content).await?;
                Ok(())
            }
            ImageSource::Local(local_path) => {
                fs::copy(local_path, path).await?;
                Ok(())
            }
            ImageSource::Registry { repo: _, tag: _ } => {
                // TODO: Implement OCI registry support
                Err(AivaError::NotImplemented(
                    "Registry pull not yet implemented".to_string(),
                ))
            }
        }
    }

    async fn push(&self, path: &std::path::Path, destination: &ImageSource) -> Result<()> {
        match destination {
            ImageSource::Local(dest_path) => {
                fs::copy(path, dest_path).await?;
                Ok(())
            }
            _ => Err(AivaError::NotImplemented(
                "Push to URL/Registry not implemented".to_string(),
            )),
        }
    }

    async fn convert(
        &self,
        input: &std::path::Path,
        output: &std::path::Path,
        format: ImageFormat,
    ) -> Result<()> {
        use tokio::process::Command;

        let format_str = match format {
            ImageFormat::Raw => "raw",
            ImageFormat::Qcow2 => "qcow2",
            ImageFormat::Vmdk => "vmdk",
            ImageFormat::Vhd => "vpc",
        };

        let output = Command::new("qemu-img")
            .args([
                "convert",
                "-f",
                "raw", // Assume input is raw
                "-O",
                format_str,
                &input.to_string_lossy(),
                &output.to_string_lossy(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(AivaError::StorageError(format!(
                "Failed to convert image: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }
}
