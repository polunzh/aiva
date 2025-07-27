use crate::vsock_executor::{ConnectionType, VSOCK_COMMAND_PORT, VsockExecutor};

#[tokio::test]
async fn test_vsock_executor_creation() {
    let vm_name = "test-vm".to_string();

    // Test vsock connection type
    let vsock_conn = ConnectionType::Vsock { cid: 3 };
    let _vsock_executor = VsockExecutor::new(vm_name.clone(), vsock_conn);

    // Test network connection type
    let network_conn = ConnectionType::Network {
        host: "192.168.1.100".to_string(),
        port: 5555,
    };
    let _network_executor = VsockExecutor::new(vm_name.clone(), network_conn);

    // Test SSH connection type
    let ssh_conn = ConnectionType::Ssh {
        host: "example.com".to_string(),
        port: 22,
        key_path: Some("/home/user/.ssh/id_rsa".to_string()),
    };
    let _ssh_executor = VsockExecutor::new(vm_name, ssh_conn);
}

#[test]
fn test_vsock_port_constant() {
    assert_eq!(VSOCK_COMMAND_PORT, 5555);
}

#[tokio::test]
async fn test_create_executor_replacement() {
    let vm_name = "test-vm".to_string();
    let guest_ip = "192.168.1.100".to_string();
    let guest_port = 8080;

    // Create executor directly instead of using removed helper function
    let executor = VsockExecutor::new(
        vm_name,
        ConnectionType::Network {
            host: guest_ip,
            port: guest_port,
        },
    );

    // The executor should be created successfully
    // We can't test much more without a real connection
    let result = executor.check_connection().await;
    assert!(result.is_ok());
    assert!(!result.unwrap()); // Should fail to connect to non-existent service
}

#[tokio::test]
async fn test_connection_check_failure() {
    // Create an executor that will fail to connect
    let vm_name = "nonexistent-vm".to_string();
    let conn = ConnectionType::Network {
        host: "127.0.0.1".to_string(),
        port: 9999, // Unlikely to have anything listening here
    };

    let executor = VsockExecutor::new(vm_name, conn);

    // Connection check should fail
    let result = executor.check_connection().await;
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn test_vsock_on_linux() {
    // This test only runs on Linux
    let vm_name = "linux-vm".to_string();
    let conn = ConnectionType::Vsock { cid: 3 };

    let executor = VsockExecutor::new(vm_name, conn);

    // Try to execute a command (will fail without actual vsock setup)
    let result = executor.execute_command("echo test").await;

    // We expect this to fail in test environment
    assert!(result.is_err());
}

#[tokio::test]
async fn test_ssh_connection_type() {
    let vm_name = "ssh-vm".to_string();

    // Test SSH with key
    let ssh_with_key = ConnectionType::Ssh {
        host: "test.example.com".to_string(),
        port: 2222,
        key_path: Some("/path/to/key".to_string()),
    };

    let executor_with_key = VsockExecutor::new(vm_name.clone(), ssh_with_key);

    // Test SSH without key
    let ssh_without_key = ConnectionType::Ssh {
        host: "test.example.com".to_string(),
        port: 22,
        key_path: None,
    };

    let executor_without_key = VsockExecutor::new(vm_name, ssh_without_key);

    // Both should be created successfully
    assert!(matches!(executor_with_key, VsockExecutor { .. }));
    assert!(matches!(executor_without_key, VsockExecutor { .. }));
}
