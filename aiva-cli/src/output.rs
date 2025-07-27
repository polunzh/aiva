use clap::ValueEnum;
use colored::*;
use serde::Serialize;
use tabled::{Table, Tabled};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

pub trait OutputFormatter {
    fn format<T: Serialize>(&self, data: T) -> String;
    fn format_table<T: Tabled + serde::Serialize>(&self, data: Vec<T>) -> String;
}

impl OutputFormatter for OutputFormat {
    fn format<T: Serialize>(&self, data: T) -> String {
        match self {
            OutputFormat::Table => {
                // For non-tabular data, use pretty JSON
                serde_json::to_string_pretty(&data).unwrap_or_else(|e| e.to_string())
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(&data).unwrap_or_else(|e| e.to_string())
            }
            OutputFormat::Yaml => serde_yaml::to_string(&data).unwrap_or_else(|e| e.to_string()),
        }
    }

    fn format_table<T: Tabled + serde::Serialize>(&self, data: Vec<T>) -> String {
        match self {
            OutputFormat::Table => {
                if data.is_empty() {
                    "No data to display".to_string()
                } else {
                    Table::new(data).to_string()
                }
            }
            OutputFormat::Json => {
                serde_json::to_string_pretty(&data).unwrap_or_else(|e| e.to_string())
            }
            OutputFormat::Yaml => serde_yaml::to_string(&data).unwrap_or_else(|e| e.to_string()),
        }
    }
}

pub fn print_success(message: &str) {
    println!("{} {}", "✓".green(), message);
}

pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red(), message);
}

pub fn print_warning(message: &str) {
    println!("{} {}", "⚠".yellow(), message);
}

pub fn print_info(message: &str) {
    println!("{} {}", "ℹ".blue(), message);
}

pub fn print_progress(message: &str) {
    println!("{} {}", "⟳".cyan(), message);
}
