use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMInstance {
    pub id: Uuid,
    pub name: String,
    pub state: VMState,
    pub config: VMConfig,
    pub runtime: RuntimeInfo,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMConfig {
    pub cpus: u32,
    pub memory_mb: u64,
    pub disk_gb: u64,
    pub kernel_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub network: NetworkConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VMState {
    Creating,
    Running,
    Paused,
    Stopping,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub pid: Option<u32>,
    pub api_socket: Option<PathBuf>,
    pub vsock_cid: Option<u32>,
    pub tap_device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub guest_ip: String,
    pub host_ip: String,
    pub subnet: String,
    pub gateway: String,
    pub dns_servers: Vec<String>,
    pub dhcp_enabled: bool,
    pub port_mappings: Vec<PortMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub host_port: u16,
    pub guest_port: u16,
    pub protocol: Protocol,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "tcp"),
            Protocol::Udp => write!(f, "udp"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub cache_strategy: CacheStrategy,
    pub additional_drives: Vec<BlockDevice>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CacheStrategy {
    Writeback,
    Unsafe,
}

impl std::fmt::Display for CacheStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CacheStrategy::Writeback => write!(f, "writeback"),
            CacheStrategy::Unsafe => write!(f, "unsafe"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDevice {
    pub path: PathBuf,
    pub size_mb: u64,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub tap_device: String,
    pub guest_ip: String,
    pub host_ip: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VMMetrics {
    pub cpu_usage: f64,
    pub memory_usage: MemoryMetrics,
    pub disk_io: DiskIOMetrics,
    pub network_io: NetworkIOMetrics,
    pub uptime: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub cache_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIOMetrics {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkIOMetrics {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataTransferMethod {
    NetworkTransfer {
        protocol: TransferProtocol,
    },
    BlockDeviceMount {
        image_path: PathBuf,
        mount_point: String,
    },
    TemporaryVolume {
        size_mb: u64,
        format: FileSystem,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferProtocol {
    Ssh,
    Http,
    VSock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileSystem {
    Ext4,
    Xfs,
    Btrfs,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            guest_ip: "172.16.0.2".to_string(),
            host_ip: "172.16.0.1".to_string(),
            subnet: "172.16.0.0/24".to_string(),
            gateway: "172.16.0.1".to_string(),
            dns_servers: vec!["8.8.8.8".to_string(), "1.1.1.1".to_string()],
            dhcp_enabled: false,
            port_mappings: vec![],
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            cache_strategy: CacheStrategy::Writeback,
            additional_drives: vec![],
        }
    }
}
