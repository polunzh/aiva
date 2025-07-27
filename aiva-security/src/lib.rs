pub mod isolation;
pub mod policy;

use aiva_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[async_trait]
pub trait SecurityManager: Send + Sync {
    async fn apply_isolation(&self, vm_id: &str, policy: &SecurityPolicy) -> Result<()>;
    async fn validate_policy(&self, policy: &SecurityPolicy) -> Result<()>;
    async fn get_effective_policy(&self, vm_id: &str) -> Result<SecurityPolicy>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityPolicy {
    pub name: String,
    pub isolation_level: IsolationLevel,
    pub capabilities: CapabilitySet,
    pub syscall_filter: Option<SyscallFilter>,
    pub resource_limits: ResourceLimits,
    pub network_policy: NetworkPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum IsolationLevel {
    None,
    Basic,
    Enhanced,
    Maximum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySet {
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallFilter {
    pub default_action: FilterAction,
    pub rules: Vec<SyscallRule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FilterAction {
    Allow,
    Kill,
    Trap,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallRule {
    pub syscall: String,
    pub action: FilterAction,
    pub conditions: Option<Vec<Condition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub arg_index: u32,
    pub operation: CompareOp,
    pub value: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum CompareOp {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    MaskedEqual(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpu_quota: Option<u32>,
    pub memory_limit: Option<u64>,
    pub pids_limit: Option<u32>,
    pub open_files: Option<u32>,
    pub io_bandwidth: Option<IOLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IOLimit {
    pub read_bps: Option<u64>,
    pub write_bps: Option<u64>,
    pub read_iops: Option<u64>,
    pub write_iops: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    pub allow_outbound: bool,
    pub allowed_ports: Vec<PortRule>,
    pub blocked_ips: Vec<String>,
    pub rate_limit: Option<NetworkRateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRule {
    pub port: u16,
    pub protocol: String,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Direction {
    Inbound,
    Outbound,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkRateLimit {
    pub bandwidth_mbps: u32,
    pub connections_per_second: u32,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            isolation_level: IsolationLevel::Basic,
            capabilities: CapabilitySet {
                allowed: vec![],
                denied: vec!["CAP_SYS_ADMIN".to_string()],
            },
            syscall_filter: None,
            resource_limits: ResourceLimits::default(),
            network_policy: NetworkPolicy::default(),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            cpu_quota: Some(100),
            memory_limit: Some(8 * 1024 * 1024 * 1024), // 8GB
            pids_limit: Some(1024),
            open_files: Some(1024),
            io_bandwidth: None,
        }
    }
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_outbound: true,
            allowed_ports: vec![],
            blocked_ips: vec![],
            rate_limit: None,
        }
    }
}

pub fn load_preset_policies() -> HashMap<String, SecurityPolicy> {
    let mut policies = HashMap::new();

    // Minimal isolation for trusted workloads
    policies.insert(
        "trusted".to_string(),
        SecurityPolicy {
            name: "trusted".to_string(),
            isolation_level: IsolationLevel::None,
            capabilities: CapabilitySet {
                allowed: vec![],
                denied: vec![],
            },
            syscall_filter: None,
            resource_limits: ResourceLimits {
                cpu_quota: None,
                memory_limit: None,
                pids_limit: None,
                open_files: None,
                io_bandwidth: None,
            },
            network_policy: NetworkPolicy {
                allow_outbound: true,
                allowed_ports: vec![],
                blocked_ips: vec![],
                rate_limit: None,
            },
        },
    );

    // Standard isolation for general workloads
    policies.insert("standard".to_string(), SecurityPolicy::default());

    // Enhanced security for sensitive workloads
    policies.insert(
        "restricted".to_string(),
        SecurityPolicy {
            name: "restricted".to_string(),
            isolation_level: IsolationLevel::Enhanced,
            capabilities: CapabilitySet {
                allowed: vec![],
                denied: vec![
                    "CAP_SYS_ADMIN".to_string(),
                    "CAP_NET_ADMIN".to_string(),
                    "CAP_SYS_PTRACE".to_string(),
                ],
            },
            syscall_filter: Some(SyscallFilter {
                default_action: FilterAction::Allow,
                rules: vec![
                    SyscallRule {
                        syscall: "ptrace".to_string(),
                        action: FilterAction::Kill,
                        conditions: None,
                    },
                    SyscallRule {
                        syscall: "mount".to_string(),
                        action: FilterAction::Kill,
                        conditions: None,
                    },
                ],
            }),
            resource_limits: ResourceLimits {
                cpu_quota: Some(50),
                memory_limit: Some(4 * 1024 * 1024 * 1024), // 4GB
                pids_limit: Some(512),
                open_files: Some(512),
                io_bandwidth: Some(IOLimit {
                    read_bps: Some(100 * 1024 * 1024), // 100MB/s
                    write_bps: Some(100 * 1024 * 1024),
                    read_iops: Some(1000),
                    write_iops: Some(1000),
                }),
            },
            network_policy: NetworkPolicy {
                allow_outbound: false,
                allowed_ports: vec![
                    PortRule {
                        port: 443,
                        protocol: "tcp".to_string(),
                        direction: Direction::Outbound,
                    },
                    PortRule {
                        port: 80,
                        protocol: "tcp".to_string(),
                        direction: Direction::Outbound,
                    },
                ],
                blocked_ips: vec!["10.0.0.0/8".to_string(), "192.168.0.0/16".to_string()],
                rate_limit: Some(NetworkRateLimit {
                    bandwidth_mbps: 100,
                    connections_per_second: 10,
                }),
            },
        },
    );

    // Maximum isolation for untrusted workloads
    policies.insert(
        "isolated".to_string(),
        SecurityPolicy {
            name: "isolated".to_string(),
            isolation_level: IsolationLevel::Maximum,
            capabilities: CapabilitySet {
                allowed: vec![],
                denied: vec![
                    "ALL".to_string(), // Deny all capabilities
                ],
            },
            syscall_filter: Some(SyscallFilter {
                default_action: FilterAction::Kill,
                rules: vec![
                    // Only allow essential syscalls
                    SyscallRule {
                        syscall: "read".to_string(),
                        action: FilterAction::Allow,
                        conditions: None,
                    },
                    SyscallRule {
                        syscall: "write".to_string(),
                        action: FilterAction::Allow,
                        conditions: None,
                    },
                    SyscallRule {
                        syscall: "open".to_string(),
                        action: FilterAction::Allow,
                        conditions: None,
                    },
                    SyscallRule {
                        syscall: "close".to_string(),
                        action: FilterAction::Allow,
                        conditions: None,
                    },
                    // Add more essential syscalls as needed
                ],
            }),
            resource_limits: ResourceLimits {
                cpu_quota: Some(25),
                memory_limit: Some(2 * 1024 * 1024 * 1024), // 2GB
                pids_limit: Some(256),
                open_files: Some(256),
                io_bandwidth: Some(IOLimit {
                    read_bps: Some(50 * 1024 * 1024), // 50MB/s
                    write_bps: Some(50 * 1024 * 1024),
                    read_iops: Some(500),
                    write_iops: Some(500),
                }),
            },
            network_policy: NetworkPolicy {
                allow_outbound: false,
                allowed_ports: vec![],
                blocked_ips: vec!["0.0.0.0/0".to_string()], // Block all
                rate_limit: Some(NetworkRateLimit {
                    bandwidth_mbps: 10,
                    connections_per_second: 1,
                }),
            },
        },
    );

    policies
}

pub use isolation::IsolationManager;
pub use policy::PolicyManager;
