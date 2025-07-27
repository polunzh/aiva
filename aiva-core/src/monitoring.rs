use crate::{Result, VMInstance, VMMetrics, VMState};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[async_trait]
pub trait MetricsCollector: Send + Sync {
    async fn collect_metrics(&self, vm_id: &str) -> Result<VMMetrics>;
    async fn collect_system_metrics(&self) -> Result<SystemMetrics>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: f64,
    pub memory_usage: MemoryUsage,
    pub disk_usage: DiskUsage,
    pub network_stats: NetworkStats,
    pub active_vms: u32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsage {
    pub total_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskUsage {
    pub total_gb: f64,
    pub used_gb: f64,
    pub available_gb: f64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub errors: u64,
    pub drops: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: Uuid,
    pub vm_id: Option<String>,
    pub level: LogLevel,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub vm_id: Option<String>,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertType {
    ResourceExhaustion,
    HighCpuUsage,
    HighMemoryUsage,
    DiskSpaceLow,
    NetworkConnectivity,
    VMCrash,
    SecurityViolation,
    PerformanceDegradation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical,
    High,
    Medium,
    Low,
}

pub struct MonitoringService {
    metrics_collector: Box<dyn MetricsCollector>,
    alerts: Arc<RwLock<Vec<Alert>>>,
    logs: Arc<RwLock<Vec<LogEntry>>>,
    vm_instances: Arc<RwLock<HashMap<String, VMInstance>>>,
    alert_thresholds: AlertThresholds,
}

#[derive(Debug, Clone)]
pub struct AlertThresholds {
    pub cpu_usage_warning: f64,
    pub cpu_usage_critical: f64,
    pub memory_usage_warning: f64,
    pub memory_usage_critical: f64,
    pub disk_usage_warning: f64,
    pub disk_usage_critical: f64,
    pub network_error_rate_warning: f64,
    pub network_error_rate_critical: f64,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cpu_usage_warning: 80.0,
            cpu_usage_critical: 95.0,
            memory_usage_warning: 80.0,
            memory_usage_critical: 95.0,
            disk_usage_warning: 80.0,
            disk_usage_critical: 95.0,
            network_error_rate_warning: 5.0,
            network_error_rate_critical: 10.0,
        }
    }
}

impl MonitoringService {
    pub fn new(metrics_collector: Box<dyn MetricsCollector>) -> Self {
        Self {
            metrics_collector,
            alerts: Arc::new(RwLock::new(Vec::new())),
            logs: Arc::new(RwLock::new(Vec::new())),
            vm_instances: Arc::new(RwLock::new(HashMap::new())),
            alert_thresholds: AlertThresholds::default(),
        }
    }

    pub async fn start_monitoring(&self, interval: Duration) -> Result<()> {
        info!("Starting monitoring service with interval {:?}", interval);

        let mut interval_timer = tokio::time::interval(interval);
        loop {
            interval_timer.tick().await;

            if let Err(e) = self.collect_and_analyze_metrics().await {
                error!("Error collecting metrics: {}", e);
            }
        }
    }

    async fn collect_and_analyze_metrics(&self) -> Result<()> {
        // Collect system metrics
        let system_metrics = self.metrics_collector.collect_system_metrics().await?;
        self.analyze_system_metrics(&system_metrics).await?;

        // Collect VM metrics
        let vm_instances = self.vm_instances.read().await;
        for (vm_id, vm_instance) in vm_instances.iter() {
            if vm_instance.state == VMState::Running {
                match self.metrics_collector.collect_metrics(vm_id).await {
                    Ok(metrics) => {
                        self.analyze_vm_metrics(vm_id, &metrics).await?;
                    }
                    Err(e) => {
                        warn!("Failed to collect metrics for VM {}: {}", vm_id, e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn analyze_system_metrics(&self, metrics: &SystemMetrics) -> Result<()> {
        // Check CPU usage
        if metrics.cpu_usage > self.alert_thresholds.cpu_usage_critical {
            self.create_alert(
                None,
                AlertType::HighCpuUsage,
                AlertSeverity::Critical,
                format!("System CPU usage is at {:.1}%", metrics.cpu_usage),
            )
            .await?;
        } else if metrics.cpu_usage > self.alert_thresholds.cpu_usage_warning {
            self.create_alert(
                None,
                AlertType::HighCpuUsage,
                AlertSeverity::Medium,
                format!("System CPU usage is at {:.1}%", metrics.cpu_usage),
            )
            .await?;
        }

        // Check memory usage
        if metrics.memory_usage.usage_percent > self.alert_thresholds.memory_usage_critical {
            self.create_alert(
                None,
                AlertType::HighMemoryUsage,
                AlertSeverity::Critical,
                format!(
                    "System memory usage is at {:.1}%",
                    metrics.memory_usage.usage_percent
                ),
            )
            .await?;
        } else if metrics.memory_usage.usage_percent > self.alert_thresholds.memory_usage_warning {
            self.create_alert(
                None,
                AlertType::HighMemoryUsage,
                AlertSeverity::Medium,
                format!(
                    "System memory usage is at {:.1}%",
                    metrics.memory_usage.usage_percent
                ),
            )
            .await?;
        }

        // Check disk usage
        if metrics.disk_usage.usage_percent > self.alert_thresholds.disk_usage_critical {
            self.create_alert(
                None,
                AlertType::DiskSpaceLow,
                AlertSeverity::Critical,
                format!(
                    "System disk usage is at {:.1}%",
                    metrics.disk_usage.usage_percent
                ),
            )
            .await?;
        } else if metrics.disk_usage.usage_percent > self.alert_thresholds.disk_usage_warning {
            self.create_alert(
                None,
                AlertType::DiskSpaceLow,
                AlertSeverity::Medium,
                format!(
                    "System disk usage is at {:.1}%",
                    metrics.disk_usage.usage_percent
                ),
            )
            .await?;
        }

        Ok(())
    }

    async fn analyze_vm_metrics(&self, vm_id: &str, metrics: &VMMetrics) -> Result<()> {
        let memory_usage_percent =
            (metrics.memory_usage.used_mb as f64 / metrics.memory_usage.total_mb as f64) * 100.0;

        // Check VM CPU usage
        if metrics.cpu_usage > self.alert_thresholds.cpu_usage_critical {
            self.create_alert(
                Some(vm_id.to_string()),
                AlertType::HighCpuUsage,
                AlertSeverity::Critical,
                format!("VM {} CPU usage is at {:.1}%", vm_id, metrics.cpu_usage),
            )
            .await?;
        }

        // Check VM memory usage
        if memory_usage_percent > self.alert_thresholds.memory_usage_critical {
            self.create_alert(
                Some(vm_id.to_string()),
                AlertType::HighMemoryUsage,
                AlertSeverity::Critical,
                format!("VM {vm_id} memory usage is at {memory_usage_percent:.1}%"),
            )
            .await?;
        }

        // Check for performance degradation
        if metrics.cpu_usage > 90.0 && memory_usage_percent > 90.0 {
            self.create_alert(
                Some(vm_id.to_string()),
                AlertType::PerformanceDegradation,
                AlertSeverity::High,
                format!("VM {vm_id} is experiencing performance degradation"),
            )
            .await?;
        }

        Ok(())
    }

    async fn create_alert(
        &self,
        vm_id: Option<String>,
        alert_type: AlertType,
        severity: AlertSeverity,
        message: String,
    ) -> Result<()> {
        let alert = Alert {
            id: Uuid::new_v4(),
            vm_id,
            alert_type,
            severity,
            message,
            timestamp: Utc::now(),
            resolved: false,
            resolved_at: None,
        };

        self.alerts.write().await.push(alert.clone());

        // Log the alert
        match severity {
            AlertSeverity::Critical => error!("CRITICAL ALERT: {}", alert.message),
            AlertSeverity::High => warn!("HIGH ALERT: {}", alert.message),
            AlertSeverity::Medium => warn!("MEDIUM ALERT: {}", alert.message),
            AlertSeverity::Low => info!("LOW ALERT: {}", alert.message),
        }

        Ok(())
    }

    pub async fn get_alerts(&self, vm_id: Option<&str>) -> Result<Vec<Alert>> {
        let alerts = self.alerts.read().await;
        let filtered_alerts: Vec<Alert> = alerts
            .iter()
            .filter(|alert| {
                if let Some(vm_id) = vm_id {
                    alert.vm_id.as_ref() == Some(&vm_id.to_string())
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        Ok(filtered_alerts)
    }

    pub async fn resolve_alert(&self, alert_id: &Uuid) -> Result<()> {
        let mut alerts = self.alerts.write().await;
        if let Some(alert) = alerts.iter_mut().find(|a| a.id == *alert_id) {
            alert.resolved = true;
            alert.resolved_at = Some(Utc::now());
            info!("Resolved alert {}: {}", alert_id, alert.message);
        }
        Ok(())
    }

    pub async fn add_log_entry(&self, entry: LogEntry) -> Result<()> {
        self.logs.write().await.push(entry);
        Ok(())
    }

    pub async fn get_logs(
        &self,
        vm_id: Option<&str>,
        level: Option<LogLevel>,
    ) -> Result<Vec<LogEntry>> {
        let logs = self.logs.read().await;
        let filtered_logs: Vec<LogEntry> = logs
            .iter()
            .filter(|log| {
                let vm_match = if let Some(vm_id) = vm_id {
                    log.vm_id.as_ref() == Some(&vm_id.to_string())
                } else {
                    true
                };

                let level_match = if let Some(level) = level {
                    log.level as u8 <= level as u8
                } else {
                    true
                };

                vm_match && level_match
            })
            .cloned()
            .collect();

        Ok(filtered_logs)
    }

    pub async fn register_vm(&self, vm_instance: VMInstance) -> Result<()> {
        let vm_id = vm_instance.id.to_string();
        self.vm_instances
            .write()
            .await
            .insert(vm_id.clone(), vm_instance);

        self.add_log_entry(LogEntry {
            id: Uuid::new_v4(),
            vm_id: Some(vm_id),
            level: LogLevel::Info,
            message: "VM registered for monitoring".to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        })
        .await?;

        Ok(())
    }

    pub async fn unregister_vm(&self, vm_id: &str) -> Result<()> {
        self.vm_instances.write().await.remove(vm_id);

        self.add_log_entry(LogEntry {
            id: Uuid::new_v4(),
            vm_id: Some(vm_id.to_string()),
            level: LogLevel::Info,
            message: "VM unregistered from monitoring".to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        })
        .await?;

        Ok(())
    }

    pub async fn get_vm_metrics(&self, vm_id: &str) -> Result<VMMetrics> {
        self.metrics_collector.collect_metrics(vm_id).await
    }

    pub async fn get_system_metrics(&self) -> Result<SystemMetrics> {
        self.metrics_collector.collect_system_metrics().await
    }

    pub fn set_alert_thresholds(&mut self, thresholds: AlertThresholds) {
        self.alert_thresholds = thresholds;
    }
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Trace => write!(f, "TRACE"),
        }
    }
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Critical => write!(f, "CRITICAL"),
            AlertSeverity::High => write!(f, "HIGH"),
            AlertSeverity::Medium => write!(f, "MEDIUM"),
            AlertSeverity::Low => write!(f, "LOW"),
        }
    }
}

impl std::fmt::Display for AlertType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertType::ResourceExhaustion => write!(f, "Resource Exhaustion"),
            AlertType::HighCpuUsage => write!(f, "High CPU Usage"),
            AlertType::HighMemoryUsage => write!(f, "High Memory Usage"),
            AlertType::DiskSpaceLow => write!(f, "Disk Space Low"),
            AlertType::NetworkConnectivity => write!(f, "Network Connectivity"),
            AlertType::VMCrash => write!(f, "VM Crash"),
            AlertType::SecurityViolation => write!(f, "Security Violation"),
            AlertType::PerformanceDegradation => write!(f, "Performance Degradation"),
        }
    }
}

// Default implementation for metrics collection
pub struct DefaultMetricsCollector;

#[async_trait]
impl MetricsCollector for DefaultMetricsCollector {
    async fn collect_metrics(&self, vm_id: &str) -> Result<VMMetrics> {
        debug!("Collecting metrics for VM {}", vm_id);

        // This is a placeholder implementation
        // In a real implementation, this would query the Firecracker API
        Ok(VMMetrics {
            cpu_usage: 0.0,
            memory_usage: crate::MemoryMetrics {
                total_mb: 8192,
                used_mb: 0,
                available_mb: 8192,
                cache_mb: 0,
            },
            disk_io: crate::DiskIOMetrics {
                read_bytes: 0,
                write_bytes: 0,
                read_ops: 0,
                write_ops: 0,
            },
            network_io: crate::NetworkIOMetrics {
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
            },
            uptime: Duration::from_secs(0),
        })
    }

    async fn collect_system_metrics(&self) -> Result<SystemMetrics> {
        debug!("Collecting system metrics");

        // This is a placeholder implementation
        // In a real implementation, this would query system resources
        Ok(SystemMetrics {
            cpu_usage: 0.0,
            memory_usage: MemoryUsage {
                total_gb: 16.0,
                used_gb: 0.0,
                available_gb: 16.0,
                usage_percent: 0.0,
            },
            disk_usage: DiskUsage {
                total_gb: 500.0,
                used_gb: 0.0,
                available_gb: 500.0,
                usage_percent: 0.0,
            },
            network_stats: NetworkStats {
                rx_bytes: 0,
                tx_bytes: 0,
                rx_packets: 0,
                tx_packets: 0,
                errors: 0,
                drops: 0,
            },
            active_vms: 0,
            timestamp: Utc::now(),
        })
    }
}
