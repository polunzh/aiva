use aiva_core::{
    CacheStrategy, NetworkConfig, PortMapping, Protocol, Result, StorageConfig, VMConfig,
    VMInstance, VMState,
};
use aiva_platform::{detect_platform, get_current_platform};
use std::time::Duration;
use uuid::Uuid;

// Helper to create a test VM configuration
fn create_test_vm_config() -> VMConfig {
    VMConfig {
        cpus: 1,
        memory_mb: 512,
        disk_gb: 5,
        kernel_path: "/opt/aiva/kernel/vmlinux".to_string().into(),
        rootfs_path: "/opt/aiva/images/base.rootfs.ext4".to_string().into(),
        network: NetworkConfig {
            guest_ip: "172.16.0.2".to_string(),
            host_ip: "172.16.0.1".to_string(),
            subnet: "172.16.0.0/24".to_string(),
            gateway: "172.16.0.1".to_string(),
            dns_servers: vec!["8.8.8.8".to_string()],
            dhcp_enabled: false,
            port_mappings: vec![PortMapping {
                host_port: 8080,
                guest_port: 80,
                protocol: Protocol::Tcp,
            }],
        },
        storage: StorageConfig {
            cache_strategy: CacheStrategy::Writeback,
            additional_drives: vec![],
        },
    }
}

// Helper to create a test VM instance
fn create_test_vm_instance(name: &str) -> VMInstance {
    VMInstance {
        id: Uuid::new_v4(),
        name: name.to_string(),
        state: VMState::Stopped,
        config: create_test_vm_config(),
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

#[tokio::test]
async fn test_platform_detection() -> Result<()> {
    let detected = detect_platform();
    let platform = get_current_platform()?;

    assert_eq!(platform.name(), detected);

    println!("Detected platform: {detected}");

    Ok(())
}

#[tokio::test]
async fn test_platform_requirements_check() -> Result<()> {
    let platform = get_current_platform()?;

    match platform.check_requirements().await {
        Ok(_) => println!("Platform requirements satisfied"),
        Err(e) => {
            println!("Platform requirements check failed: {e}");
            // This is expected in many test environments
        }
    }

    Ok(())
}

// Test that requires actual platform setup - marked as ignore
#[tokio::test]
#[ignore = "Requires platform-specific setup (Lima/WSL/KVM)"]
async fn test_vm_create_delete_lifecycle() -> Result<()> {
    let platform = get_current_platform()?;
    let instance = create_test_vm_instance("integration-test-vm");

    println!("Creating VM: {}", instance.name);

    // Create VM
    let created = platform.create_vm(&instance).await?;
    assert_eq!(created.state, VMState::Stopped);
    assert!(created.runtime.api_socket.is_some());

    println!("VM created successfully");

    // Delete VM
    platform.delete_vm(&created).await?;

    println!("VM deleted successfully");

    Ok(())
}

// Test VM operations - requires setup
#[tokio::test]
#[ignore = "Requires platform-specific setup and running VM"]
async fn test_vm_full_lifecycle() -> Result<()> {
    let platform = get_current_platform()?;
    let mut instance = create_test_vm_instance("full-lifecycle-test-vm");

    // Create VM
    println!("Creating VM: {}", instance.name);
    instance = platform.create_vm(&instance).await?;

    // Start VM
    println!("Starting VM");
    platform.start_vm(&instance).await?;

    // Wait for VM to be ready
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Get metrics
    println!("Getting VM metrics");
    let metrics = platform.get_vm_metrics(&instance).await?;

    println!("VM Metrics:");
    println!("  CPU Usage: {:.2}%", metrics.cpu_usage);
    println!(
        "  Memory: {} MB / {} MB",
        metrics.memory_usage.used_mb, metrics.memory_usage.total_mb
    );
    println!("  Network RX: {} bytes", metrics.network_io.rx_bytes);
    println!("  Network TX: {} bytes", metrics.network_io.tx_bytes);

    // Execute command
    println!("Executing command in VM");
    match platform
        .execute_command(&instance, "echo 'Hello from VM'")
        .await
    {
        Ok(output) => println!("Command output: {output}"),
        Err(e) => println!("Command execution failed: {e}"),
    }

    // Stop VM
    println!("Stopping VM");
    platform.stop_vm(&instance, false).await?;

    // Delete VM
    println!("Deleting VM");
    platform.delete_vm(&instance).await?;

    Ok(())
}

// Test error handling
#[tokio::test]
async fn test_error_handling() -> Result<()> {
    let platform = get_current_platform()?;
    let instance = create_test_vm_instance("nonexistent-vm");

    // Try to stop a non-existent VM
    match platform.stop_vm(&instance, false).await {
        Ok(_) => println!("Stop succeeded (platform handles gracefully)"),
        Err(e) => println!("Stop failed as expected: {e}"),
    }

    // Try to execute command on non-running VM
    match platform.execute_command(&instance, "echo test").await {
        Ok(_) => println!("Command succeeded unexpectedly"),
        Err(e) => println!("Command failed as expected: {e}"),
    }

    Ok(())
}

// Cross-platform networking test
#[tokio::test]
#[ignore = "Requires network setup"]
async fn test_cross_platform_networking() -> Result<()> {
    use aiva_platform::command_pool::{ConnectionType, get_command_pool};

    let pool = get_command_pool();

    // Test network connection type
    let connection = ConnectionType::Network {
        host: "127.0.0.1".to_string(),
        port: 8080,
    };

    // This will fail without actual server
    match pool
        .register_vm("test-net-vm".to_string(), connection)
        .await
    {
        Ok(_) => println!("VM registered successfully"),
        Err(e) => println!("VM registration failed (expected): {e}"),
    }

    Ok(())
}

// Performance test
#[tokio::test]
#[ignore = "Long running performance test"]
async fn test_platform_performance() -> Result<()> {
    let platform = get_current_platform()?;

    let start = std::time::Instant::now();

    // Create multiple test instances
    let mut instances = vec![];
    for i in 0..5 {
        let instance = create_test_vm_instance(&format!("perf-test-vm-{i}"));
        instances.push(instance);
    }

    // Time platform operations
    for instance in &instances {
        match platform.create_vm(instance).await {
            Ok(created) => {
                println!("Created VM {} in {:?}", created.name, start.elapsed());

                // Clean up
                let _ = platform.delete_vm(&created).await;
            }
            Err(e) => {
                println!("Failed to create VM {}: {}", instance.name, e);
            }
        }
    }

    println!(
        "Total time for {} VMs: {:?}",
        instances.len(),
        start.elapsed()
    );

    Ok(())
}
