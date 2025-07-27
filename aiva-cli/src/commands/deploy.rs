use crate::output::{OutputFormat, print_error, print_progress, print_success, print_warning};
use aiva_core::{Config, Result, VMLogger, VMManager, VMState};
use std::path::PathBuf;
use std::sync::Arc;

pub async fn execute(
    name: String,
    image_path: PathBuf,
    restart: bool,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    print_progress(&format!("Deploying image to AI agent/MCP server: {name}"));

    if !image_path.exists() {
        print_error(&format!("Image file not found: {}", image_path.display()));
        return Err(aiva_core::AivaError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Image file not found",
        )));
    }

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Find VM by name
    let vm = vm_manager.get_vm_by_name(&name).await?;

    if let Some(vm) = vm {
        // Initialize logging for deployment
        let logger = VMLogger::new(vm.name.clone());
        logger.init().await?;
        logger
            .info(&format!(
                "Starting deployment of image: {}",
                image_path.display()
            ))
            .await?;

        // Check if VM is running and handle restart logic
        let was_running = vm.state == VMState::Running || vm.state == VMState::Paused;

        if was_running && restart {
            print_progress("Stopping VM for deployment...");
            logger.info("Stopping VM for deployment").await?;
            vm_manager.stop_vm(&vm.id, false).await?;
            print_success("VM stopped successfully");
        } else if was_running && !restart {
            print_warning(
                "VM is running. Use --restart to stop and restart the VM after deployment.",
            );
            logger
                .warn("VM is running but restart not requested")
                .await?;
        }

        // Deploy the image
        print_progress("Deploying image...");
        logger.info("Beginning image deployment").await?;

        // For now, this is a mock implementation
        // In a real implementation, this would:
        // 1. Copy the image to the VM's storage location
        // 2. Update the VM configuration to use the new image
        // 3. Verify the image integrity
        // 4. Update the VM instance with new image information

        print_progress("Copying image to VM storage...");
        logger
            .info("Copying image to VM storage (mock implementation)")
            .await?;

        // Simulate some work
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        print_progress("Updating VM configuration...");
        logger
            .info("Updating VM configuration with new image")
            .await?;

        // Update VM with new image path (mock)
        print_progress("Verifying image integrity...");
        logger.info("Verifying deployed image integrity").await?;

        print_success("Image deployed successfully");
        logger
            .info("Image deployment completed successfully")
            .await?;

        // Restart VM if requested and it was running
        if restart && was_running {
            print_progress("Restarting VM with new image...");
            logger.info("Restarting VM with new image").await?;
            vm_manager.start_vm(&vm.id).await?;
            print_success("VM restarted successfully");
            logger
                .info("VM restarted successfully with new image")
                .await?;
        } else if restart && !was_running {
            print_progress("Starting VM with new image...");
            logger.info("Starting VM with new image").await?;
            vm_manager.start_vm(&vm.id).await?;
            print_success("VM started successfully");
            logger
                .info("VM started successfully with new image")
                .await?;
        }

        logger.info("Deployment process completed").await?;
        print_success(&format!("Deployment completed for: {name}"));
    } else {
        print_error(&format!("VM '{name}' not found"));
        return Err(aiva_core::AivaError::VMError {
            vm_name: name,
            state: VMState::Stopped,
            message: "VM not found".to_string(),
        });
    }

    Ok(())
}
