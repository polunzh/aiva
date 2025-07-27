use crate::output::{OutputFormat, print_error, print_progress, print_success, print_warning};
use aiva_core::{Config, Result, VMManager};
use std::sync::Arc;

pub async fn execute(
    name: String,
    force: bool,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    print_progress(&format!("Stopping AI agent/MCP server: {name}"));

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Find VM by name
    let vm = vm_manager.get_vm_by_name(&name).await?;

    if let Some(vm) = vm {
        if vm.state != aiva_core::VMState::Running && vm.state != aiva_core::VMState::Paused {
            print_warning(&format!(
                "VM '{}' is not running (state: {:?})",
                name, vm.state
            ));
            return Ok(());
        }

        if force {
            print_warning("Force stopping VM...");
        } else {
            print_progress("Gracefully stopping VM...");
        }

        vm_manager.stop_vm(&vm.id, force).await?;

        print_success(&format!("Successfully stopped AI agent/MCP server: {name}"));
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
