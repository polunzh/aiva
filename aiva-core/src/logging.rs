use crate::Result;
use chrono::Utc;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub struct VMLogger {
    vm_name: String,
    log_file: PathBuf,
}

impl VMLogger {
    pub fn new(vm_name: String) -> Self {
        let log_file = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".aiva")
            .join("logs")
            .join(format!("{vm_name}.log"));

        Self { vm_name, log_file }
    }

    pub async fn init(&self) -> Result<()> {
        if let Some(parent) = self.log_file.parent() {
            fs::create_dir_all(parent).await?;
        }
        Ok(())
    }

    pub async fn log(&self, level: LogLevel, message: &str) -> Result<()> {
        let timestamp = Utc::now();
        let log_entry = format!(
            "{} [{}] [{}] {}\n",
            timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            level.as_str(),
            self.vm_name,
            message
        );

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
            .await?;

        file.write_all(log_entry.as_bytes()).await?;
        file.flush().await?;
        Ok(())
    }

    pub async fn info(&self, message: &str) -> Result<()> {
        self.log(LogLevel::Info, message).await
    }

    pub async fn warn(&self, message: &str) -> Result<()> {
        self.log(LogLevel::Warn, message).await
    }

    pub async fn error(&self, message: &str) -> Result<()> {
        self.log(LogLevel::Error, message).await
    }

    pub async fn debug(&self, message: &str) -> Result<()> {
        self.log(LogLevel::Debug, message).await
    }
}

#[derive(Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Debug => "DEBUG",
        }
    }
}
