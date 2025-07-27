pub mod image;
pub mod volume;

use aiva_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn create_volume(&self, config: &VolumeConfig) -> Result<Volume>;
    async fn delete_volume(&self, volume_id: &str) -> Result<()>;
    async fn attach_volume(&self, volume_id: &str, vm_id: &str) -> Result<BlockDeviceInfo>;
    async fn detach_volume(&self, volume_id: &str) -> Result<()>;
    async fn list_volumes(&self) -> Result<Vec<Volume>>;
    async fn get_volume(&self, volume_id: &str) -> Result<Volume>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Volume {
    pub id: String,
    pub name: String,
    pub size_mb: u64,
    pub path: PathBuf,
    pub format: VolumeFormat,
    pub attached_to: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub name: String,
    pub size_mb: u64,
    pub format: VolumeFormat,
    pub sparse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum VolumeFormat {
    Raw,
    Qcow2,
    Ext4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDeviceInfo {
    pub path: PathBuf,
    pub device: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub name: String,
    pub size_mb: u64,
    pub format: ImageFormat,
    pub source: ImageSource,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageFormat {
    Raw,
    Qcow2,
    Vmdk,
    Vhd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageSource {
    Url(String),
    Local(PathBuf),
    Registry { repo: String, tag: String },
}

pub use image::ImageManager;
pub use volume::VolumeManager;
