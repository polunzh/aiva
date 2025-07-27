use crate::{CapabilitySet, IsolationLevel, NetworkPolicy, ResourceLimits, SecurityPolicy};
use aiva_core::{AivaError, Result};
use serde_json;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info, warn};

pub struct PolicyManager {
    policies_dir: PathBuf,
    policies: HashMap<String, SecurityPolicy>,
}

impl PolicyManager {
    pub fn new(policies_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            policies_dir,
            policies: HashMap::new(),
        })
    }

    pub async fn init(&mut self) -> Result<()> {
        fs::create_dir_all(&self.policies_dir).await?;
        self.load_policies().await?;

        // Create default policies if none exist
        if self.policies.is_empty() {
            self.create_default_policies().await?;
        }

        Ok(())
    }

    pub async fn load_policies(&mut self) -> Result<()> {
        let mut entries = fs::read_dir(&self.policies_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                match self.load_policy_from_file(&path).await {
                    Ok(policy) => {
                        self.policies.insert(policy.name.clone(), policy);
                    }
                    Err(e) => {
                        warn!("Failed to load policy from {:?}: {}", path, e);
                    }
                }
            }
        }

        info!("Loaded {} security policies", self.policies.len());
        Ok(())
    }

    pub async fn save_policy(&self, policy: &SecurityPolicy) -> Result<()> {
        let filename = format!("{}.json", policy.name);
        let path = self.policies_dir.join(filename);

        let content = serde_json::to_string_pretty(policy)?;
        fs::write(&path, content).await?;

        info!("Saved policy {} to {:?}", policy.name, path);
        Ok(())
    }

    pub async fn delete_policy(&mut self, name: &str) -> Result<()> {
        if !self.policies.contains_key(name) {
            return Err(AivaError::SecurityError(format!("Policy {name} not found")));
        }

        let filename = format!("{name}.json");
        let path = self.policies_dir.join(filename);

        if path.exists() {
            fs::remove_file(&path).await?;
        }

        self.policies.remove(name);
        info!("Deleted policy {}", name);
        Ok(())
    }

    pub fn get_policy(&self, name: &str) -> Result<&SecurityPolicy> {
        self.policies
            .get(name)
            .ok_or_else(|| AivaError::SecurityError(format!("Policy {name} not found")))
    }

    pub fn list_policies(&self) -> Vec<String> {
        self.policies.keys().cloned().collect()
    }

    pub async fn create_policy(&mut self, policy: SecurityPolicy) -> Result<()> {
        self.validate_policy(&policy)?;
        self.save_policy(&policy).await?;
        self.policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    pub async fn update_policy(&mut self, policy: SecurityPolicy) -> Result<()> {
        if !self.policies.contains_key(&policy.name) {
            return Err(AivaError::SecurityError(format!(
                "Policy {} not found",
                policy.name
            )));
        }

        self.validate_policy(&policy)?;
        self.save_policy(&policy).await?;
        self.policies.insert(policy.name.clone(), policy);
        Ok(())
    }

    pub fn validate_policy(&self, policy: &SecurityPolicy) -> Result<()> {
        // Validate policy name
        if policy.name.is_empty() {
            return Err(AivaError::SecurityError(
                "Policy name cannot be empty".to_string(),
            ));
        }

        // Validate resource limits
        if let Some(cpu_quota) = policy.resource_limits.cpu_quota {
            if cpu_quota == 0 || cpu_quota > 100 {
                return Err(AivaError::SecurityError(
                    "CPU quota must be between 1 and 100".to_string(),
                ));
            }
        }

        if let Some(memory_limit) = policy.resource_limits.memory_limit {
            if memory_limit == 0 {
                return Err(AivaError::SecurityError(
                    "Memory limit must be greater than 0".to_string(),
                ));
            }
        }

        // Validate capabilities
        for cap in &policy.capabilities.denied {
            if cap == "ALL" && !policy.capabilities.allowed.is_empty() {
                return Err(AivaError::SecurityError(
                    "Cannot allow capabilities when ALL is denied".to_string(),
                ));
            }
        }

        // Validate syscall filter
        if let Some(filter) = &policy.syscall_filter {
            if filter.rules.is_empty() && matches!(filter.default_action, crate::FilterAction::Kill)
            {
                warn!("Policy {} denies all syscalls by default", policy.name);
            }
        }

        Ok(())
    }

    pub fn merge_policies(&self, base: &str, overlay: &str) -> Result<SecurityPolicy> {
        let base_policy = self.get_policy(base)?;
        let overlay_policy = self.get_policy(overlay)?;

        let mut merged = base_policy.clone();
        merged.name = format!("{base}-{overlay}");

        // Merge isolation levels (take the higher/more restrictive one)
        if overlay_policy.isolation_level as u8 > base_policy.isolation_level as u8 {
            merged.isolation_level = overlay_policy.isolation_level;
        }

        // Merge capabilities (combine denied, intersect allowed)
        for cap in &overlay_policy.capabilities.denied {
            if !merged.capabilities.denied.contains(cap) {
                merged.capabilities.denied.push(cap.clone());
            }
        }

        // Keep only capabilities allowed by both policies
        merged
            .capabilities
            .allowed
            .retain(|cap| overlay_policy.capabilities.allowed.contains(cap));

        // Merge resource limits (take the more restrictive one)
        if let Some(overlay_cpu) = overlay_policy.resource_limits.cpu_quota {
            if let Some(base_cpu) = merged.resource_limits.cpu_quota {
                merged.resource_limits.cpu_quota = Some(overlay_cpu.min(base_cpu));
            } else {
                merged.resource_limits.cpu_quota = Some(overlay_cpu);
            }
        }

        if let Some(overlay_mem) = overlay_policy.resource_limits.memory_limit {
            if let Some(base_mem) = merged.resource_limits.memory_limit {
                merged.resource_limits.memory_limit = Some(overlay_mem.min(base_mem));
            } else {
                merged.resource_limits.memory_limit = Some(overlay_mem);
            }
        }

        // Merge network policies (more restrictive)
        if !overlay_policy.network_policy.allow_outbound {
            merged.network_policy.allow_outbound = false;
        }

        // Combine blocked IPs
        for ip in &overlay_policy.network_policy.blocked_ips {
            if !merged.network_policy.blocked_ips.contains(ip) {
                merged.network_policy.blocked_ips.push(ip.clone());
            }
        }

        // Take the more restrictive rate limit
        if let Some(overlay_rate) = &overlay_policy.network_policy.rate_limit {
            if let Some(base_rate) = &merged.network_policy.rate_limit {
                merged.network_policy.rate_limit = Some(crate::NetworkRateLimit {
                    bandwidth_mbps: overlay_rate.bandwidth_mbps.min(base_rate.bandwidth_mbps),
                    connections_per_second: overlay_rate
                        .connections_per_second
                        .min(base_rate.connections_per_second),
                });
            } else {
                merged.network_policy.rate_limit = Some(overlay_rate.clone());
            }
        }

        Ok(merged)
    }

    async fn load_policy_from_file(&self, path: &PathBuf) -> Result<SecurityPolicy> {
        let content = fs::read_to_string(path).await?;
        let policy: SecurityPolicy = serde_json::from_str(&content)?;
        debug!("Loaded policy {} from {:?}", policy.name, path);
        Ok(policy)
    }

    async fn create_default_policies(&mut self) -> Result<()> {
        info!("Creating default security policies");

        for (name, policy) in crate::load_preset_policies() {
            self.save_policy(&policy).await?;
            self.policies.insert(name, policy);
        }

        Ok(())
    }
}

impl std::str::FromStr for IsolationLevel {
    type Err = AivaError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "none" => Ok(IsolationLevel::None),
            "basic" => Ok(IsolationLevel::Basic),
            "enhanced" => Ok(IsolationLevel::Enhanced),
            "maximum" => Ok(IsolationLevel::Maximum),
            _ => Err(AivaError::SecurityError(format!(
                "Invalid isolation level: {s}"
            ))),
        }
    }
}

pub fn create_mcp_policy() -> SecurityPolicy {
    SecurityPolicy {
        name: "mcp-server".to_string(),
        isolation_level: IsolationLevel::Enhanced,
        capabilities: CapabilitySet {
            allowed: vec!["CAP_NET_BIND_SERVICE".to_string()],
            denied: vec![
                "CAP_SYS_ADMIN".to_string(),
                "CAP_SYS_PTRACE".to_string(),
                "CAP_SYS_MODULE".to_string(),
            ],
        },
        syscall_filter: Some(crate::SyscallFilter {
            default_action: crate::FilterAction::Allow,
            rules: vec![
                crate::SyscallRule {
                    syscall: "mount".to_string(),
                    action: crate::FilterAction::Kill,
                    conditions: None,
                },
                crate::SyscallRule {
                    syscall: "umount".to_string(),
                    action: crate::FilterAction::Kill,
                    conditions: None,
                },
                crate::SyscallRule {
                    syscall: "ptrace".to_string(),
                    action: crate::FilterAction::Kill,
                    conditions: None,
                },
            ],
        }),
        resource_limits: ResourceLimits {
            cpu_quota: Some(75),
            memory_limit: Some(4 * 1024 * 1024 * 1024), // 4GB
            pids_limit: Some(512),
            open_files: Some(1024),
            io_bandwidth: Some(crate::IOLimit {
                read_bps: Some(200 * 1024 * 1024), // 200MB/s
                write_bps: Some(200 * 1024 * 1024),
                read_iops: Some(2000),
                write_iops: Some(2000),
            }),
        },
        network_policy: NetworkPolicy {
            allow_outbound: true,
            allowed_ports: vec![
                crate::PortRule {
                    port: 443,
                    protocol: "tcp".to_string(),
                    direction: crate::Direction::Outbound,
                },
                crate::PortRule {
                    port: 80,
                    protocol: "tcp".to_string(),
                    direction: crate::Direction::Outbound,
                },
                crate::PortRule {
                    port: 8080,
                    protocol: "tcp".to_string(),
                    direction: crate::Direction::Inbound,
                },
            ],
            blocked_ips: vec!["127.0.0.1/32".to_string()],
            rate_limit: Some(crate::NetworkRateLimit {
                bandwidth_mbps: 500,
                connections_per_second: 50,
            }),
        },
    }
}

pub fn create_ai_agent_policy() -> SecurityPolicy {
    SecurityPolicy {
        name: "ai-agent".to_string(),
        isolation_level: IsolationLevel::Basic,
        capabilities: CapabilitySet {
            allowed: vec![],
            denied: vec!["CAP_SYS_ADMIN".to_string(), "CAP_NET_ADMIN".to_string()],
        },
        syscall_filter: None, // Allow most syscalls for AI workloads
        resource_limits: ResourceLimits {
            cpu_quota: Some(80),
            memory_limit: Some(16 * 1024 * 1024 * 1024), // 16GB for AI workloads
            pids_limit: Some(1024),
            open_files: Some(2048),
            io_bandwidth: Some(crate::IOLimit {
                read_bps: Some(1024 * 1024 * 1024), // 1GB/s
                write_bps: Some(1024 * 1024 * 1024),
                read_iops: Some(10000),
                write_iops: Some(10000),
            }),
        },
        network_policy: NetworkPolicy {
            allow_outbound: true,
            allowed_ports: vec![
                crate::PortRule {
                    port: 443,
                    protocol: "tcp".to_string(),
                    direction: crate::Direction::Outbound,
                },
                crate::PortRule {
                    port: 80,
                    protocol: "tcp".to_string(),
                    direction: crate::Direction::Outbound,
                },
            ],
            blocked_ips: vec![],
            rate_limit: Some(crate::NetworkRateLimit {
                bandwidth_mbps: 1000,
                connections_per_second: 100,
            }),
        },
    }
}
