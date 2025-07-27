use crate::output::{OutputFormat, print_error, print_info, print_progress, print_success};
use aiva_core::{Config, Result, VMLogger, VMManager, VMTemplate};
use std::fs;
use std::sync::Arc;

pub async fn execute(
    name: String,
    command: String,
    transport: Option<String>,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    let transport = transport.unwrap_or_else(|| "sse".to_string());
    print_progress(&format!("Running MCP command in VM '{name}': {command}"));

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Find VM by name
    let vm = vm_manager.get_vm_by_name(&name).await?;

    if let Some(vm) = vm {
        let logger = VMLogger::new(vm.name.clone());
        logger.init().await?;

        // Check if VM is running
        if vm.state != aiva_core::VMState::Running {
            print_error(&format!(
                "VM '{name}' is not running. Current state: {:?}",
                vm.state
            ));
            print_info(&format!("Start the VM first: aiva start {name}"));
            return Err(aiva_core::AivaError::VMError {
                vm_name: name,
                state: vm.state,
                message: "VM is not running".to_string(),
            });
        }

        // Load template information to get runtime context
        let vm_dir = dirs::home_dir()
            .ok_or_else(|| {
                aiva_core::AivaError::ConfigError("Cannot determine home directory".to_string())
            })?
            .join(".aiva")
            .join("data")
            .join("vms")
            .join(&name);

        let template_file = vm_dir.join("config").join("template.json");
        let template = if template_file.exists() {
            let template_content = fs::read_to_string(&template_file)?;
            serde_json::from_str::<VMTemplate>(&template_content).map_err(|e| {
                aiva_core::AivaError::ConfigError(format!("Failed to parse template: {e}"))
            })?
        } else {
            print_info("No template information found, using default command execution");
            return execute_raw_command(&name, &command, &logger).await;
        };

        logger
            .info(&format!(
                "Executing MCP command with transport: {transport}"
            ))
            .await?;
        print_info(&format!(
            "Template: {} - {}",
            template.name, template.description
        ));
        print_info(&format!("Runtime: {:?}", template.runtime));

        // Generate the runtime-specific command
        let full_command = match template.get_run_command(&command, &transport) {
            Ok(cmd) => cmd,
            Err(e) => {
                print_error(&format!("Failed to generate command: {e}"));
                logger
                    .error(&format!("Command generation failed: {e}"))
                    .await?;
                return Err(e);
            }
        };

        print_info(&format!("Executing: {full_command}"));
        logger
            .info(&format!("Full command: {full_command}"))
            .await?;

        print_progress("Executing command in VM...");

        // Execute the command in the VM using the platform integration
        let execution_result = vm_manager.execute_command(&vm.id, &full_command).await;

        match execution_result {
            Ok(output) => {
                logger
                    .info(&format!("Command execution successful: {}", output.trim()))
                    .await?;
                print_info(&format!("Command output: {}", output.trim()));
            }
            Err(e) => {
                logger
                    .error(&format!("Command execution failed: {e}"))
                    .await?;
                print_error(&format!("Command execution failed: {e}"));
                return Err(e);
            }
        }

        // Check if this is an SSE mode command and provide connection info
        if transport == "sse" {
            // Extract port from command if it contains --port, otherwise use template default
            let port = if let Some(port_pos) = command.find("--port ") {
                command[port_pos + 7..]
                    .split_whitespace()
                    .next()
                    .and_then(|p| p.parse::<u16>().ok())
                    .unwrap_or_else(|| template.mcp_support.default_port.unwrap_or(3000))
            } else {
                template.mcp_support.default_port.unwrap_or(3000)
            };

            print_success("MCP server started successfully!");
            print_info("MCP Server Details:");
            print_info("  Transport: SSE");
            print_info("  Host: 172.16.0.2 (VM internal)");
            print_info(&format!("  Port: {port}"));
            print_info(&format!("  URL: http://172.16.0.2:{port}"));

            // Show the host URL (Lima forwards ports directly)
            print_info(&format!("  Host URL: http://localhost:{port}"));
            print_info(&format!("  Connect from host: localhost:{port}"));
            print_info("Use the above URL to connect your MCP client to this server.");
            print_info(&format!("Monitor logs: aiva logs {name} --follow"));
        } else if transport == "stdio" {
            print_success("MCP server ready for stdio communication!");
            print_info("The server is running in stdio mode.");
            print_info("Connect your MCP client using stdio transport.");
            print_info(&format!("Monitor logs: aiva logs {name} --follow"));
        }

        logger
            .info(&format!(
                "MCP command execution completed with transport: {transport}"
            ))
            .await?;
    } else {
        print_error(&format!("VM '{name}' not found"));
        return Err(aiva_core::AivaError::VMError {
            vm_name: name,
            state: aiva_core::VMState::Stopped,
            message: "VM not found".to_string(),
        });
    }

    Ok(())
}

async fn execute_raw_command(name: &str, command: &str, logger: &VMLogger) -> Result<()> {
    print_info(&format!("Executing raw command: {command}"));
    logger
        .info(&format!("Raw command execution: {command}"))
        .await?;

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = std::sync::Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Find VM by name
    let vm = vm_manager.get_vm_by_name(name).await?;

    if let Some(vm) = vm {
        if vm.state != aiva_core::VMState::Running {
            print_error(&format!(
                "VM '{name}' is not running. Current state: {:?}",
                vm.state
            ));
            return Err(aiva_core::AivaError::VMError {
                vm_name: name.to_string(),
                state: vm.state,
                message: "VM is not running".to_string(),
            });
        }

        print_progress("Executing raw command in VM...");

        // Execute the command in the VM
        let execution_result = vm_manager.execute_command(&vm.id, command).await;

        match execution_result {
            Ok(output) => {
                logger
                    .info(&format!(
                        "Raw command execution successful: {}",
                        output.trim()
                    ))
                    .await?;
                print_info(&format!("Command output: {}", output.trim()));
                print_success("Command executed successfully");
            }
            Err(e) => {
                logger
                    .error(&format!("Raw command execution failed: {e}"))
                    .await?;
                print_error(&format!("Command execution failed: {e}"));
                return Err(e);
            }
        }
    } else {
        return Err(aiva_core::AivaError::VMError {
            vm_name: name.to_string(),
            state: aiva_core::VMState::Stopped,
            message: "VM not found".to_string(),
        });
    }

    logger.info("Raw command execution completed").await?;

    Ok(())
}
