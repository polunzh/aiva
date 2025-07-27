mod bridge;
mod iptables;
mod tap;

pub use bridge::{configure_bridge, create_bridge, delete_bridge};
pub use iptables::{cleanup_nat_rules, setup_nat_rules};
pub use tap::{configure_tap_device, create_tap_device, delete_tap_device};

use aiva_core::{NetworkConfig, NetworkInfo, Result, VMInstance};

pub async fn setup_network(instance: &VMInstance) -> Result<NetworkInfo> {
    // 1. Create TAP device
    let tap_device = create_tap_device(&instance.name)?;

    // 2. Configure bridge
    configure_bridge(&tap_device)?;

    // 3. Set up iptables rules
    setup_nat_rules(&instance.config.network)?;

    // 4. Configure DHCP (if enabled)
    if instance.config.network.dhcp_enabled {
        setup_dhcp_server(&instance.config.network)?;
    }

    Ok(NetworkInfo {
        tap_device,
        guest_ip: instance.config.network.guest_ip.clone(),
        host_ip: instance.config.network.host_ip.clone(),
    })
}

pub async fn cleanup_network(instance: &VMInstance) -> Result<()> {
    if let Some(tap_device) = &instance.runtime.tap_device {
        // Clean up iptables rules
        cleanup_nat_rules(&instance.config.network)?;

        // Delete TAP device
        delete_tap_device(tap_device)?;
    }

    Ok(())
}

fn setup_dhcp_server(_config: &NetworkConfig) -> Result<()> {
    // TODO: Implement DHCP server setup
    Ok(())
}
