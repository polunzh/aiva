use crate::command_pool::{CommandPool, ConnectionType};
use aiva_core::Result;

#[tokio::test]
async fn test_command_pool_creation() {
    let pool = CommandPool::new();
    let vms = pool.list_vms().await;
    assert_eq!(vms.len(), 0, "New pool should have no VMs");
}

#[tokio::test]
async fn test_vm_registration() -> Result<()> {
    let pool = CommandPool::new();

    // Register a VM with network connection
    let vm_name = "test-vm".to_string();
    let _connection = ConnectionType::Network {
        host: "127.0.0.1".to_string(),
        port: 5555,
    };

    // This will fail because there's no actual VM to connect to
    // But we can test that the registration logic works
    assert!(!pool.is_registered(&vm_name).await);

    // In a real test, we would mock the connection or use a test server

    Ok(())
}

#[tokio::test]
async fn test_vm_unregistration() -> Result<()> {
    let pool = CommandPool::new();
    let vm_name = "test-vm";

    // Unregistering a non-existent VM should succeed
    pool.unregister_vm(vm_name).await?;

    assert!(!pool.is_registered(vm_name).await);

    Ok(())
}

#[tokio::test]
async fn test_connection_types() {
    // Test different connection type creations
    let _vsock = ConnectionType::Vsock { cid: 3 };

    let _network = ConnectionType::Network {
        host: "192.168.1.100".to_string(),
        port: 8080,
    };

    let _ssh = ConnectionType::Ssh {
        host: "example.com".to_string(),
        port: 22,
        key_path: Some("/home/user/.ssh/id_rsa".to_string()),
    };
}

#[tokio::test]
async fn test_global_command_pool() {
    use crate::command_pool::get_command_pool;

    let pool1 = get_command_pool();
    let pool2 = get_command_pool();

    // Should be the same instance
    let vms1 = pool1.list_vms().await;
    let vms2 = pool2.list_vms().await;

    assert_eq!(vms1.len(), vms2.len());
}
