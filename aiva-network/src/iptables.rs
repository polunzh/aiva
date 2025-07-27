use aiva_core::{AivaError, NetworkConfig, Result};
use std::process::Command;
use tracing::{debug, info};

pub fn setup_nat_rules(config: &NetworkConfig) -> Result<()> {
    info!("Setting up NAT rules for subnet {}", config.subnet);

    // Enable IP forwarding
    std::fs::write("/proc/sys/net/ipv4/ip_forward", "1").map_err(|e| AivaError::NetworkError {
        operation: "enable IP forwarding".to_string(),
        cause: e.to_string(),
    })?;

    // Add MASQUERADE rule
    let output = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            &config.subnet,
            "-j",
            "MASQUERADE",
        ])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "add MASQUERADE rule".to_string(),
            cause: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Check if rule already exists
        let check = Command::new("iptables")
            .args([
                "-t",
                "nat",
                "-C",
                "POSTROUTING",
                "-s",
                &config.subnet,
                "-j",
                "MASQUERADE",
            ])
            .output();

        if let Ok(check_output) = check {
            if check_output.status.success() {
                debug!("NAT rule already exists");
                return Ok(());
            }
        }

        return Err(AivaError::NetworkError {
            operation: "add MASQUERADE rule".to_string(),
            cause: stderr.to_string(),
        });
    }

    // Add FORWARD rules
    add_forward_rule("ACCEPT", &config.subnet)?;

    // Add port forwarding rules
    for mapping in &config.port_mappings {
        add_port_forward_rule(
            &config.guest_ip,
            mapping.host_port,
            mapping.guest_port,
            &mapping.protocol.to_string().to_lowercase(),
        )?;
    }

    Ok(())
}

pub fn cleanup_nat_rules(config: &NetworkConfig) -> Result<()> {
    info!("Cleaning up NAT rules for subnet {}", config.subnet);

    // Remove MASQUERADE rule
    let _ = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-D",
            "POSTROUTING",
            "-s",
            &config.subnet,
            "-j",
            "MASQUERADE",
        ])
        .output();

    // Remove FORWARD rules
    let _ = Command::new("iptables")
        .args(["-D", "FORWARD", "-s", &config.subnet, "-j", "ACCEPT"])
        .output();

    let _ = Command::new("iptables")
        .args(["-D", "FORWARD", "-d", &config.subnet, "-j", "ACCEPT"])
        .output();

    // Remove port forwarding rules
    for mapping in &config.port_mappings {
        let _ = remove_port_forward_rule(
            &config.guest_ip,
            mapping.host_port,
            mapping.guest_port,
            &mapping.protocol.to_string().to_lowercase(),
        );
    }

    Ok(())
}

fn add_forward_rule(action: &str, subnet: &str) -> Result<()> {
    // Allow forwarding from subnet
    Command::new("iptables")
        .args(["-A", "FORWARD", "-s", subnet, "-j", action])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "add FORWARD rule".to_string(),
            cause: e.to_string(),
        })?;

    // Allow forwarding to subnet
    Command::new("iptables")
        .args(["-A", "FORWARD", "-d", subnet, "-j", action])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "add FORWARD rule".to_string(),
            cause: e.to_string(),
        })?;

    Ok(())
}

fn add_port_forward_rule(
    guest_ip: &str,
    host_port: u16,
    guest_port: u16,
    protocol: &str,
) -> Result<()> {
    debug!(
        "Adding port forward: {}:{} -> {}:{}",
        protocol, host_port, guest_ip, guest_port
    );

    Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "PREROUTING",
            "-p",
            protocol,
            "--dport",
            &host_port.to_string(),
            "-j",
            "DNAT",
            "--to-destination",
            &format!("{guest_ip}:{guest_port}"),
        ])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "add port forward rule".to_string(),
            cause: e.to_string(),
        })?;

    Ok(())
}

fn remove_port_forward_rule(
    guest_ip: &str,
    host_port: u16,
    guest_port: u16,
    protocol: &str,
) -> Result<()> {
    Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-D",
            "PREROUTING",
            "-p",
            protocol,
            "--dport",
            &host_port.to_string(),
            "-j",
            "DNAT",
            "--to-destination",
            &format!("{guest_ip}:{guest_port}"),
        ])
        .output()
        .map_err(|e| AivaError::NetworkError {
            operation: "remove port forward rule".to_string(),
            cause: e.to_string(),
        })?;

    Ok(())
}
