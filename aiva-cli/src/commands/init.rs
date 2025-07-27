use crate::output::{OutputFormat, print_error, print_info, print_progress, print_success};
use crate::utils::{get_images_dir, get_vm_dir};
use aiva_core::{Config, Result, TemplateManager, VMConfigCustomizations, VMManager, VMTemplate};
use std::fs;
use std::sync::Arc;

pub async fn execute(
    name: String,
    template: Option<String>,
    config: Config,
    _format: OutputFormat,
) -> Result<()> {
    // Handle template selection
    let selected_template = if let Some(template_name) = template {
        print_progress(&format!("Using template: {template_name}"));
        match TemplateManager::get_template(&template_name) {
            Ok(tmpl) => {
                print_info(&format!("Template: {} - {}", tmpl.name, tmpl.description));
                tmpl
            }
            Err(_) => {
                print_error(&format!("Unknown template: {template_name}"));
                print_info("Available templates:");
                for (name, desc) in VMTemplate::list_available_templates() {
                    print_info(&format!("  {name}: {desc}"));
                }
                return Err(aiva_core::AivaError::ConfigError(format!(
                    "Unknown template: {template_name}"
                )));
            }
        }
    } else {
        // Show available templates and ask user to specify
        print_info("No template specified. Available templates:");
        for (name, desc) in VMTemplate::list_available_templates() {
            print_info(&format!("  {name}: {desc}"));
        }
        print_info(&format!(
            "Usage: aiva init {name} --template <template-name>"
        ));
        print_info("Example: aiva init my-python-server --template python3-uv");
        return Ok(());
    };

    print_progress(&format!(
        "Initializing AI agent/MCP server: {name} with template {}",
        selected_template.name
    ));

    // Create directories
    let vm_dir = get_vm_dir(&name)?;
    let images_dir = get_images_dir()?;

    print_progress("Creating directories...");
    fs::create_dir_all(&vm_dir)?;
    fs::create_dir_all(&images_dir)?;
    fs::create_dir_all(vm_dir.join("data"))?;
    fs::create_dir_all(vm_dir.join("logs"))?;

    print_progress("Checking platform requirements...");

    // Check platform
    let platform = aiva_platform::get_current_platform()?;
    platform.check_requirements().await?;

    print_info(&format!("Detected platform: {}", platform.name()));

    // Download base images if needed
    let kernel_path = images_dir.join("vmlinux");
    let rootfs_path = images_dir.join("rootfs.ext4");

    if !kernel_path.exists() || !rootfs_path.exists() {
        print_progress("Downloading base images...");
        // TODO: Implement image download
        print_info("Using default images (download not implemented yet)");
    }

    // Generate VM configuration from template
    let vm_config = selected_template.generate_vm_config(Some(VMConfigCustomizations {
        cpus: Some(config.defaults.cpus),
        memory_mb: Some(crate::utils::parse_memory_size(&config.defaults.memory)?),
        disk_gb: Some(crate::utils::parse_disk_size(&config.defaults.disk)?),
        additional_ports: None,
    }));

    // Create configuration directory
    let vm_config_dir = vm_dir.join("config");
    fs::create_dir_all(&vm_config_dir)?;

    // Save VM configuration
    let config_file = vm_config_dir.join("config.json");
    let config_content = serde_json::to_string_pretty(&vm_config)?;
    fs::write(config_file, config_content)?;

    // Save template information
    let template_file = vm_config_dir.join("template.json");
    let template_content = serde_json::to_string_pretty(&selected_template)?;
    fs::write(template_file, template_content)?;

    // Save setup script
    let setup_script_file = vm_dir.join("setup.sh");
    fs::write(setup_script_file, selected_template.get_setup_script())?;

    // Create VM instance using the VMManager
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Check if VM already exists
    if vm_manager.get_vm_by_name(&name).await?.is_some() {
        print_error(&format!("VM '{name}' already exists"));
        return Err(aiva_core::AivaError::VMError {
            vm_name: name,
            state: aiva_core::VMState::Stopped,
            message: "VM already exists".to_string(),
        });
    }

    // Create VM instance
    let vm_instance = vm_manager.create_vm(name.clone(), vm_config).await?;
    print_progress(&format!("Created VM instance with ID: {}", vm_instance.id));

    print_success(&format!(
        "AI agent/MCP server '{name}' initialized successfully with template {}",
        selected_template.name
    ));

    // Display information about the template
    print_info(&format!("Runtime: {:?}", selected_template.runtime));
    if selected_template.mcp_support.sse_enabled {
        if let Some(port) = selected_template.mcp_support.default_port {
            print_info(&format!("Default MCP port: {port} (SSE mode)"));
        }
    }
    print_info(&format!(
        "Supported transports: {}",
        selected_template
            .mcp_support
            .supported_transports
            .join(", ")
    ));

    // Display next steps
    print_info("Next steps:");
    print_info(&format!("  1. aiva start {name}"));
    print_info(&format!("  2. aiva run {name} \"your-mcp-command sse\""));
    print_info(&format!("  3. aiva logs {name} --follow"));

    Ok(())
}
