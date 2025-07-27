use thiserror::Error;

#[derive(Error, Debug)]
pub enum AivaError {
    #[error("Platform error on {platform}: {message}")]
    PlatformError {
        platform: String,
        message: String,
        recoverable: bool,
    },

    #[error("Resource error for {resource_type}: {message}")]
    ResourceError {
        resource_type: ResourceType,
        message: String,
    },

    #[error("Network error during {operation}: {cause}")]
    NetworkError { operation: String, cause: String },

    #[error("VM error for {vm_name} in state {state:?}: {message}")]
    VMError {
        vm_name: String,
        state: crate::types::VMState,
        message: String,
    },

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Security error: {0}")]
    SecurityError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AivaError>;

#[derive(Debug, Clone)]
pub enum ResourceType {
    Cpu,
    Memory,
    Disk,
    Network,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Cpu => write!(f, "CPU"),
            ResourceType::Memory => write!(f, "Memory"),
            ResourceType::Disk => write!(f, "Disk"),
            ResourceType::Network => write!(f, "Network"),
        }
    }
}
