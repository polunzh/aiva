use crate::output::{OutputFormat, OutputFormatter, print_error, print_info};
use aiva_core::{Config, Result, VMInstance, VMManager};
use colored::*;
use serde::Serialize;
use std::sync::Arc;
use tabled::Tabled;

#[derive(Serialize, Tabled)]
struct VMStatus {
    name: String,
    state: String,
    cpus: u32,
    memory: String,
    uptime: String,
    ip: String,
}

impl From<VMInstance> for VMStatus {
    fn from(vm: VMInstance) -> Self {
        let state = match vm.state {
            aiva_core::VMState::Running => "Running".green().to_string(),
            aiva_core::VMState::Stopped => "Stopped".red().to_string(),
            aiva_core::VMState::Paused => "Paused".yellow().to_string(),
            aiva_core::VMState::Creating => "Creating".cyan().to_string(),
            aiva_core::VMState::Stopping => "Stopping".yellow().to_string(),
            aiva_core::VMState::Error => "Error".red().bold().to_string(),
        };

        let uptime = if vm.state == aiva_core::VMState::Running {
            let duration = chrono::Utc::now() - vm.created_at;
            format_duration(duration.to_std().unwrap_or_default())
        } else {
            "-".to_string()
        };

        VMStatus {
            name: vm.name,
            state,
            cpus: vm.config.cpus,
            memory: format!("{}MB", vm.config.memory_mb),
            uptime,
            ip: vm.config.network.guest_ip,
        }
    }
}

fn format_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d {}h", secs / 86400, (secs % 86400) / 3600)
    }
}

pub async fn execute(name: Option<String>, _config: Config, format: OutputFormat) -> Result<()> {
    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Auto-reset any stuck VMs
    let reset_vms = vm_manager.reset_stuck_vms().await?;
    if !reset_vms.is_empty() {
        for (id, old_state) in reset_vms {
            if let Some(vm) = vm_manager.get_vm(&id).await? {
                print_info(&format!(
                    "Reset VM '{}' from {:?} to Stopped (was stuck for >2 minutes)",
                    vm.name, old_state
                ));
            }
        }
    }

    if let Some(name) = name {
        // Show specific VM
        if let Some(vm) = vm_manager.get_vm_by_name(&name).await? {
            let status = VMStatus::from(vm.clone());

            match format {
                OutputFormat::Table => {
                    println!("\n{}", format.format_table(vec![status]));

                    // Show additional details for table format
                    if vm.state == aiva_core::VMState::Running {
                        println!("\nNetwork Configuration:");
                        println!("  Gateway: {}", vm.config.network.gateway);
                        println!("  DNS: {}", vm.config.network.dns_servers.join(", "));

                        if !vm.config.network.port_mappings.is_empty() {
                            println!("\nPort Mappings:");
                            for mapping in &vm.config.network.port_mappings {
                                println!(
                                    "  {} -> {} ({})",
                                    mapping.host_port,
                                    mapping.guest_port,
                                    match mapping.protocol {
                                        aiva_core::Protocol::Tcp => "TCP",
                                        aiva_core::Protocol::Udp => "UDP",
                                    }
                                );
                            }
                        }

                        // Try to get metrics
                        match vm_manager.get_vm_metrics(&vm.id).await {
                            Ok(metrics) => {
                                println!("\nResource Usage:");
                                println!("  CPU: {:.1}%", metrics.cpu_usage);
                                println!(
                                    "  Memory: {}MB / {}MB ({:.1}%)",
                                    metrics.memory_usage.used_mb,
                                    metrics.memory_usage.total_mb,
                                    (metrics.memory_usage.used_mb as f64
                                        / metrics.memory_usage.total_mb as f64)
                                        * 100.0
                                );
                            }
                            Err(_) => {
                                print_info("Metrics not available");
                            }
                        }
                    }
                }
                _ => {
                    println!("{}", format.format(status));
                }
            }
        } else {
            print_error(&format!("VM '{name}' not found"));
            return Err(aiva_core::AivaError::VMError {
                vm_name: name,
                state: aiva_core::VMState::Stopped,
                message: "VM not found".to_string(),
            });
        }
    } else {
        // Show all VMs
        let vms = vm_manager.list_vms().await?;

        if vms.is_empty() {
            print_info("No VMs found. Run 'aiva init <name>' to create a new VM.");
        } else {
            let statuses: Vec<VMStatus> = vms.into_iter().map(VMStatus::from).collect();
            println!("{}", format.format_table(statuses));
        }
    }

    Ok(())
}
