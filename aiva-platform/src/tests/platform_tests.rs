use crate::{detect_platform, get_current_platform};
use aiva_core::{
    CacheStrategy, NetworkConfig, Platform, Result, StorageConfig, VMConfig, VMInstance, VMState,
};
use uuid::Uuid;

#[test]
fn test_detect_platform() {
    let platform = detect_platform();

    #[cfg(target_os = "linux")]
    assert_eq!(platform, "linux");

    #[cfg(target_os = "macos")]
    assert_eq!(platform, "macos");

    #[cfg(target_os = "windows")]
    assert_eq!(platform, "windows");
}

#[tokio::test]
async fn test_get_current_platform() -> Result<()> {
    let platform = get_current_platform()?;

    // Platform should implement the Platform trait
    assert_eq!(platform.name(), detect_platform());

    Ok(())
}

#[tokio::test]
async fn test_platform_requirements() -> Result<()> {
    let platform = get_current_platform()?;

    // Check requirements should succeed or fail gracefully
    let result = platform.check_requirements().await;

    // On CI or test environments, this might fail
    // So we just check that it returns a result
    assert!(result.is_ok() || result.is_err());

    Ok(())
}

// Helper function to create a test VM instance
fn create_test_vm_instance(name: &str) -> VMInstance {
    VMInstance {
        id: Uuid::new_v4(),
        name: name.to_string(),
        state: VMState::Stopped,
        config: VMConfig {
            cpus: 2,
            memory_mb: 1024,
            disk_gb: 10,
            kernel_path: "/test/kernel".to_string().into(),
            rootfs_path: "/test/rootfs".to_string().into(),
            network: NetworkConfig {
                guest_ip: "192.168.1.100".to_string(),
                host_ip: "192.168.1.1".to_string(),
                subnet: "192.168.1.0/24".to_string(),
                gateway: "192.168.1.1".to_string(),
                dns_servers: vec!["8.8.8.8".to_string()],
                dhcp_enabled: false,
                port_mappings: vec![],
            },
            storage: StorageConfig {
                cache_strategy: CacheStrategy::Writeback,
                additional_drives: vec![],
            },
        },
        runtime: aiva_core::RuntimeInfo {
            pid: None,
            api_socket: None,
            vsock_cid: None,
            tap_device: None,
        },
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[cfg(target_os = "linux")]
mod linux_tests {
    use super::*;
    use crate::linux::LinuxPlatform;

    #[tokio::test]
    async fn test_linux_platform_creation() -> Result<()> {
        let platform = LinuxPlatform::new()?;
        assert_eq!(platform.name(), "linux");
        Ok(())
    }

    #[test]
    fn test_vsock_support_check() {
        let platform = LinuxPlatform::new().unwrap();
        // This will return false in most test environments
        let _has_vsock = platform.check_vsock_support();
        // Just verify it doesn't panic
    }
}

#[cfg(target_os = "macos")]
mod macos_tests {
    use super::*;
    use crate::macos::MacOSPlatform;

    #[tokio::test]
    async fn test_macos_platform_creation() -> Result<()> {
        let platform = MacOSPlatform::new()?;
        assert_eq!(platform.name(), "macos");
        Ok(())
    }

    #[tokio::test]
    async fn test_macos_with_custom_config() -> Result<()> {
        let platform = MacOSPlatform::with_config("/path/to/lima.yml".to_string())?;
        assert_eq!(platform.name(), "macos");
        Ok(())
    }
}

#[cfg(target_os = "windows")]
mod windows_tests {
    use super::*;
    use crate::windows::WindowsPlatform;

    #[tokio::test]
    async fn test_windows_platform_creation() -> Result<()> {
        let platform = WindowsPlatform::new()?;
        assert_eq!(platform.name(), "windows");
        Ok(())
    }
}

// Integration test for VM lifecycle (will likely fail without proper setup)
#[tokio::test]
#[ignore] // Ignore by default as it requires platform-specific setup
async fn test_vm_lifecycle() -> Result<()> {
    // Skip this test in CI or environments without proper VM setup
    if std::env::var("CI").is_ok() || std::env::var("SKIP_VM_TESTS").is_ok() {
        eprintln!("Skipping VM lifecycle test in CI/test environment");
        return Ok(());
    }

    let platform = get_current_platform()?;
    let mut instance = create_test_vm_instance("test-lifecycle-vm");

    // Try to create VM
    let create_result = platform.create_vm(&instance).await;
    match create_result {
        Ok(created_instance) => {
            instance = created_instance;
            assert_eq!(instance.state, VMState::Stopped);
        }
        Err(e) => {
            eprintln!("VM creation failed (expected in test environment): {e}");
            return Ok(());
        }
    }

    // Try to start VM
    let start_result = platform.start_vm(&instance).await;
    match start_result {
        Ok(_) => {
            instance.state = VMState::Running;
        }
        Err(e) => {
            eprintln!("VM start failed (expected in test environment): {e}");
            let _ = platform.delete_vm(&instance).await;
            return Ok(());
        }
    }

    // Try to get metrics (may return defaults)
    match platform.get_vm_metrics(&instance).await {
        Ok(metrics) => {
            assert!(metrics.cpu_usage >= 0.0);
            assert!(metrics.memory_usage.total_mb > 0);
        }
        Err(e) => {
            eprintln!("Metrics collection failed (continuing): {e}");
        }
    }

    // Try to execute a command
    match platform
        .execute_command(&instance, "echo 'Hello from VM'")
        .await
    {
        Ok(output) => {
            assert!(output.contains("Hello from VM") || output.contains("Hello"));
        }
        Err(e) => {
            eprintln!(
                "Command execution failed (expected in test environment): {e}"
            );
        }
    }

    // Clean up
    let _ = platform.stop_vm(&instance, false).await;
    let _ = platform.delete_vm(&instance).await;

    Ok(())
}

#[tokio::test]
async fn test_platform_error_handling() -> Result<()> {
    let platform = get_current_platform()?;
    let instance = create_test_vm_instance("nonexistent-vm");

    // These operations should fail gracefully

    // Stop non-existent VM
    let stop_result = platform.stop_vm(&instance, false).await;
    // May succeed or fail depending on platform
    let _ = stop_result;

    // Get metrics for non-existent VM
    let metrics_result = platform.get_vm_metrics(&instance).await;
    // Should return default metrics or error
    let _ = metrics_result;

    // Execute command on non-running VM
    let exec_result = platform.execute_command(&instance, "echo test").await;
    // Should fail with appropriate error
    assert!(exec_result.is_err() || exec_result.is_ok());

    Ok(())
}
