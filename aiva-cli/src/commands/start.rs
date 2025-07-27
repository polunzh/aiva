use crate::output::{OutputFormat, print_error, print_progress, print_success};
use crate::utils::{get_vm_dir, parse_disk_size, parse_memory_size, parse_port_mapping};
use aiva_core::{Config, PortMapping, Protocol, Result, VMConfig, VMManager};
use std::fs;
use std::sync::Arc;

pub async fn execute(
    name: String,
    cpus: Option<u32>,
    memory: Option<String>,
    disk: Option<String>,
    ports: Vec<String>,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    print_progress(&format!("Starting AI agent/MCP server: {name}"));

    // Load VM configuration
    let vm_dir = get_vm_dir(&name)?;
    let config_path = vm_dir.join("config").join("config.json");

    if !config_path.exists() {
        print_error(&format!(
            "VM '{name}' not found. Run 'aiva init {name}' first."
        ));
        return Err(aiva_core::AivaError::ConfigError(format!(
            "VM '{name}' not initialized"
        )));
    }

    let config_content = fs::read_to_string(&config_path)?;
    let mut vm_config: VMConfig = serde_json::from_str(&config_content)?;

    // Override with command line options
    if let Some(cpus) = cpus {
        vm_config.cpus = cpus;
    }
    if let Some(memory) = memory {
        vm_config.memory_mb = parse_memory_size(&memory)?;
    }
    if let Some(disk) = disk {
        vm_config.disk_gb = parse_disk_size(&disk)?;
    }

    // Parse port mappings
    for port in ports {
        let (host_port, guest_port) = parse_port_mapping(&port)?;
        vm_config.network.port_mappings.push(PortMapping {
            host_port,
            guest_port,
            protocol: Protocol::Tcp,
        });
    }

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Check if VM already exists
    if let Some(existing_vm) = vm_manager.get_vm_by_name(&name).await? {
        if existing_vm.state == aiva_core::VMState::Running {
            print_error(&format!("VM '{name}' is already running"));
            return Ok(());
        }

        // Start existing VM
        print_progress("Starting existing VM...");
        vm_manager.start_vm(&existing_vm.id).await?;
    } else {
        // Create and start new VM
        print_progress("Creating new VM...");
        let vm = vm_manager.create_vm(name.clone(), vm_config).await?;

        print_progress("Starting VM...");
        vm_manager.start_vm(&vm.id).await?;
    }

    print_success(&format!("Successfully started AI agent/MCP server: {name}"));
    print_success(&format!("To view logs, run: aiva logs {name}"));
    print_success(&format!("To check status, run: aiva status {name}"));

    Ok(())
}
