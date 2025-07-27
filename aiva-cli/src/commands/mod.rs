mod config;
mod data;
mod delete;
mod deploy;
mod init;
mod logs;
mod run;
mod start;
mod status;
mod stop;

use aiva_core::{Config as AivaConfig, Result};
use clap::Subcommand;
use std::path::PathBuf;

use crate::output::OutputFormat;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Initialize a new AI agent/MCP server environment
    Init {
        /// Name of the agent
        name: String,

        /// Template to use
        #[arg(short, long)]
        template: Option<String>,
    },

    /// Start an AI agent/MCP server instance
    Start {
        /// Name of the agent
        name: String,

        /// Number of vCPUs
        #[arg(long)]
        cpus: Option<u32>,

        /// Memory size (e.g., 8GB)
        #[arg(long)]
        memory: Option<String>,

        /// Disk size (e.g., 50GB)
        #[arg(long)]
        disk: Option<String>,

        /// Port mappings (format: host:guest)
        #[arg(short, long)]
        port: Vec<String>,
    },

    /// Stop an AI agent/MCP server instance
    Stop {
        /// Name of the agent
        name: String,

        /// Force stop
        #[arg(short, long)]
        force: bool,
    },

    /// Delete an AI agent/MCP server instance
    Delete {
        /// Name of the agent
        name: String,

        /// Force delete (delete running VMs)
        #[arg(short, long)]
        force: bool,
    },

    /// Show status of AI agent/MCP server instances
    Status {
        /// Name of the agent (optional, shows all if not specified)
        name: Option<String>,
    },

    /// Deploy a new image to an AI agent/MCP server
    Deploy {
        /// Name of the agent
        name: String,

        /// Path to the image
        #[arg(long)]
        image_path: PathBuf,

        /// Restart after deployment
        #[arg(long)]
        restart: bool,
    },

    /// Show logs from an AI agent/MCP server
    Logs {
        /// Name of the agent
        name: String,

        /// Follow log output
        #[arg(short, long)]
        follow: bool,

        /// Number of lines to show from the end
        #[arg(short, long)]
        tail: Option<usize>,
    },

    /// Run an MCP server command in a VM
    Run {
        /// Name of the agent
        name: String,

        /// Command to execute
        command: String,

        /// Transport mode (sse, stdio)
        #[arg(short, long, default_value = "sse")]
        transport: Option<String>,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Data management operations
    Data {
        #[command(subcommand)]
        operation: DataOperation,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Get a configuration value
    Get {
        /// Name of the agent
        name: String,
        /// Configuration key
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Name of the agent
        name: String,
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },

    /// List all configuration values
    List {
        /// Name of the agent
        name: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum DataOperation {
    /// Sync data between host and VM
    Sync {
        /// Name of the agent
        name: String,

        /// Source path
        #[arg(long)]
        source: PathBuf,

        /// Destination path
        #[arg(long)]
        dest: PathBuf,
    },

    /// List data volumes
    List {
        /// Name of the agent
        name: String,
    },
}

pub async fn execute(command: Command, config: AivaConfig, format: OutputFormat) -> Result<()> {
    match command {
        Command::Init { name, template } => init::execute(name, template, config, format).await,
        Command::Start {
            name,
            cpus,
            memory,
            disk,
            port,
        } => start::execute(name, cpus, memory, disk, port, config, format).await,
        Command::Stop { name, force } => stop::execute(name, force, config, format).await,
        Command::Delete { name, force } => delete::execute(name, force, config, format).await,
        Command::Status { name } => status::execute(name, config, format).await,
        Command::Deploy {
            name,
            image_path,
            restart,
        } => deploy::execute(name, image_path, restart, config, format).await,
        Command::Logs { name, follow, tail } => {
            logs::execute(name, follow, tail, config, format).await
        }
        Command::Run {
            name,
            command,
            transport,
        } => run::execute(name, command, transport, config, format).await,
        Command::Config { action } => config::execute(action, config, format).await,
        Command::Data { operation } => data::execute(operation, config, format).await,
    }
}
