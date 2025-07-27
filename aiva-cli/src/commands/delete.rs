use crate::output::{OutputFormat, print_error, print_progress, print_success};
use aiva_core::{Config, Result, VMLogger, VMManager};
use std::sync::Arc;

pub async fn execute(
    name: String,
    force: bool,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    print_progress(&format!("Deleting VM '{name}'"));

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Check if VM exists
    let vm = vm_manager.get_vm_by_name(&name).await?;

    if let Some(vm) = vm {
        let logger = VMLogger::new(vm.name.clone());
        logger.init().await?;

        // Check if VM is running and force is not specified
        if vm.state == aiva_core::VMState::Running && !force {
            print_error(&format!(
                "VM '{name}' is running. Use --force to delete running VMs"
            ));
            logger
                .warn("Delete operation cancelled - VM is running and --force not specified")
                .await?;
            return Err(aiva_core::AivaError::VMError {
                vm_name: name,
                state: vm.state,
                message: "Cannot delete running VM without --force flag".to_string(),
            });
        }

        if force {
            logger.info("Force delete requested for running VM").await?;
        }

        logger
            .info(&format!("Deleting VM (force: {force})"))
            .await?;

        // Delete the VM
        vm_manager.delete_vm(&vm.id).await?;

        logger.info("VM deleted successfully").await?;
        print_success(&format!("VM '{name}' deleted successfully"));
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
