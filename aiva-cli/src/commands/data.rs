use crate::commands::DataOperation;
use crate::output::{OutputFormat, print_error, print_info, print_progress, print_success};
use aiva_core::{Config, Result, VMLogger, VMManager};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

pub async fn execute(
    operation: DataOperation,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    match operation {
        DataOperation::Sync { name, source, dest } => {
            print_progress(&format!("Syncing data for VM '{name}'"));
            print_progress(&format!("Source: {}", source.display()));
            print_progress(&format!("Destination: {}", dest.display()));

            // Get platform and VM manager
            let platform = aiva_platform::get_current_platform()?;
            let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
            vm_manager.load_state().await?;

            // Find VM by name
            let vm = vm_manager.get_vm_by_name(&name).await?;

            if let Some(vm) = vm {
                let logger = VMLogger::new(vm.name.clone());
                logger.init().await?;
                logger
                    .info(&format!(
                        "Starting data sync: {} -> {}",
                        source.display(),
                        dest.display()
                    ))
                    .await?;

                // Validate source path
                if !source.exists() {
                    print_error(&format!("Source path does not exist: {}", source.display()));
                    return Err(aiva_core::AivaError::IoError(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Source path not found",
                    )));
                }

                // Create destination directory if it doesn't exist
                if let Some(parent) = dest.parent() {
                    if !parent.exists() {
                        print_progress("Creating destination directory...");
                        fs::create_dir_all(parent)?;
                        logger
                            .info(&format!(
                                "Created destination directory: {}",
                                parent.display()
                            ))
                            .await?;
                    }
                }

                // Perform the sync operation
                print_progress("Copying files...");

                if source.is_file() {
                    // Copy single file
                    fs::copy(&source, &dest)?;
                    logger
                        .info(&format!(
                            "Copied file: {} -> {}",
                            source.display(),
                            dest.display()
                        ))
                        .await?;
                } else if source.is_dir() {
                    // Copy directory recursively
                    copy_dir_recursive(&source, &dest)?;
                    logger
                        .info(&format!(
                            "Copied directory: {} -> {}",
                            source.display(),
                            dest.display()
                        ))
                        .await?;
                }

                logger.info("Data sync completed successfully").await?;
                print_success(&format!("Data sync completed for VM '{name}'"));
            } else {
                print_error(&format!("VM '{name}' not found"));
                return Err(aiva_core::AivaError::VMError {
                    vm_name: name,
                    state: aiva_core::VMState::Stopped,
                    message: "VM not found".to_string(),
                });
            }
        }
        DataOperation::List { name } => {
            print_progress(&format!("Listing data volumes for VM '{name}'"));

            // Get platform and VM manager
            let platform = aiva_platform::get_current_platform()?;
            let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
            vm_manager.load_state().await?;

            // Find VM by name
            let vm = vm_manager.get_vm_by_name(&name).await?;

            if let Some(_vm) = vm {
                // Get VM data directory
                let home = dirs::home_dir().ok_or_else(|| {
                    aiva_core::AivaError::ConfigError("Cannot determine home directory".to_string())
                })?;
                let vm_data_dir = home
                    .join(".aiva")
                    .join("data")
                    .join("vms")
                    .join(&name)
                    .join("data");

                print_info(&format!("Data volumes for VM '{name}':"));

                if vm_data_dir.exists() {
                    list_directory_contents(&vm_data_dir, 0)?;
                } else {
                    print_info("No data volumes found");
                    print_info(&format!("Data directory: {}", vm_data_dir.display()));
                }
            } else {
                print_error(&format!("VM '{name}' not found"));
                return Err(aiva_core::AivaError::VMError {
                    vm_name: name,
                    state: aiva_core::VMState::Stopped,
                    message: "VM not found".to_string(),
                });
            }
        }
    }

    Ok(())
}

fn copy_dir_recursive(source: &PathBuf, dest: &PathBuf) -> Result<()> {
    if !dest.exists() {
        fs::create_dir_all(dest)?;
    }

    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let dest_path = dest.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &dest_path)?;
        } else {
            fs::copy(&source_path, &dest_path)?;
        }
    }

    Ok(())
}

fn list_directory_contents(dir: &PathBuf, indent: usize) -> Result<()> {
    let entries = fs::read_dir(dir)?;
    let prefix = "  ".repeat(indent);

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();

        if path.is_dir() {
            println!("{}📁 {}/", prefix, name.to_string_lossy());
            list_directory_contents(&path, indent + 1)?;
        } else {
            let metadata = fs::metadata(&path)?;
            let size = metadata.len();
            println!("{}📄 {} ({} bytes)", prefix, name.to_string_lossy(), size);
        }
    }

    Ok(())
}
