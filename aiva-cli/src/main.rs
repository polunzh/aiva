mod commands;
mod output;
mod utils;

use aiva_core::Config;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "aiva")]
#[command(about = "AIVA - Secure microVM environment for AI agents and MCP servers", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,

    #[arg(short, long, global = true, help = "Verbose output")]
    verbose: bool,

    #[arg(short, long, global = true, help = "Quiet output")]
    quiet: bool,

    #[arg(
        long,
        global = true,
        help = "Output format",
        value_enum,
        default_value = "table"
    )]
    format: output::OutputFormat,

    #[arg(
        long,
        global = true,
        help = "Path to Lima configuration file (default: ./lima.yml if exists, otherwise built-in config)"
    )]
    lima_config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "error"
    } else {
        "info"
    };

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(log_level))
        .init();

    // Load configuration
    let config = Config::load()?;

    // Set Lima config environment variable if provided
    if let Some(ref lima_config) = cli.lima_config {
        unsafe {
            std::env::set_var("AIVA_LIMA_CONFIG", lima_config);
        }
    }

    // Execute command
    match commands::execute(cli.command, config, cli.format).await {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}
