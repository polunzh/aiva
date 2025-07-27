use crate::output::{OutputFormat, print_error, print_info};
use aiva_core::{Config, Result, VMManager};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::{Duration, sleep};

pub async fn execute(
    name: String,
    follow: bool,
    tail: Option<usize>,
    _config: Config,
    _format: OutputFormat,
) -> Result<()> {
    print_info(&format!("Showing logs for AI agent/MCP server: {name}"));

    // Get platform and VM manager
    let platform = aiva_platform::get_current_platform()?;
    let vm_manager = Arc::new(aiva_core::VMOrchestrator::new(platform));
    vm_manager.load_state().await?;

    // Find VM by name
    let vm = vm_manager.get_vm_by_name(&name).await?;

    if let Some(vm) = vm {
        let log_file = get_log_file(&vm.name);

        if !log_file.exists() {
            print_error(&format!("Log file not found for VM: {name}"));
            return Ok(());
        }

        if follow {
            print_info("Following log output (press Ctrl+C to stop)...");
            follow_logs(&log_file, tail).await?;
        } else {
            show_logs(&log_file, tail).await?;
        }
    } else {
        print_error(&format!("VM '{name}' not found"));
    }

    Ok(())
}

fn get_log_file(vm_name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".aiva")
        .join("logs")
        .join(format!("{vm_name}.log"))
}

async fn show_logs(log_file: &PathBuf, tail: Option<usize>) -> Result<()> {
    let content = fs::read_to_string(log_file).await?;
    let lines: Vec<&str> = content.lines().collect();

    let lines_to_show = if let Some(tail_lines) = tail {
        let start = if lines.len() > tail_lines {
            lines.len() - tail_lines
        } else {
            0
        };
        &lines[start..]
    } else {
        &lines
    };

    for line in lines_to_show {
        println!("{line}");
    }

    Ok(())
}

async fn follow_logs(log_file: &PathBuf, tail: Option<usize>) -> Result<()> {
    // Show existing logs first
    if log_file.exists() {
        show_logs(log_file, tail).await?;
    }

    // Follow new logs
    loop {
        if log_file.exists() {
            let file = fs::File::open(log_file).await?;
            let reader = BufReader::new(file);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await? {
                println!("{line}");
            }
        }

        sleep(Duration::from_millis(100)).await;
    }
}
