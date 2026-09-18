//! NAT и IP forwarding для AWG VPN (Linux).

use super::manager::AwgError;
use std::process::Command;
use tracing::info;

/// Параметры NAT для VPN-подсети.
#[derive(Debug, Clone)]
pub struct AwgNatConfig {
    pub enabled: bool,
    /// Исходящий интерфейс (`eth0`). Если `None` — определяется через default route.
    pub egress_interface: Option<String>,
    /// Подсеть клиентов для MASQUERADE, напр. `10.8.0.0/24`.
    pub subnet: String,
}

/// Включить `net.ipv4.ip_forward` и правило MASQUERADE для подсети AWG.
pub fn apply_nat(config: &AwgNatConfig) -> Result<(), AwgError> {
    if !config.enabled {
        return Ok(());
    }

    if config.subnet.trim().is_empty() {
        return Err(AwgError::SetconfFailed(
            "NAT subnet must not be empty".into(),
        ));
    }

    enable_ip_forward()?;

    let iface = match &config.egress_interface {
        Some(name) if !name.trim().is_empty() => name.clone(),
        _ => detect_default_interface()?,
    };

    run_iptables(&[
        "-t",
        "nat",
        "-C",
        "POSTROUTING",
        "-s",
        &config.subnet,
        "-o",
        &iface,
        "-j",
        "MASQUERADE",
    ])
    .or_else(|_| {
        run_iptables(&[
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            &config.subnet,
            "-o",
            &iface,
            "-j",
            "MASQUERADE",
        ])
    })?;

    info!(
        subnet = %config.subnet,
        egress = %iface,
        "AWG NAT configured (MASQUERADE)"
    );
    Ok(())
}

fn enable_ip_forward() -> Result<(), AwgError> {
    let output = Command::new("sysctl")
        .args(["-w", "net.ipv4.ip_forward=1"])
        .output()
        .map_err(|e| AwgError::SetconfFailed(format!("sysctl failed: {}", e)))?;
    if !output.status.success() {
        return Err(AwgError::SetconfFailed(format!(
            "sysctl net.ipv4.ip_forward failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}

fn detect_default_interface() -> Result<String, AwgError> {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .map_err(|e| AwgError::SetconfFailed(format!("ip route failed: {}", e)))?;
    if !output.status.success() {
        return Err(AwgError::SetconfFailed(
            "failed to detect default route interface; set transport.awg.nat.egress_interface"
                .into(),
        ));
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.split_whitespace().collect();
    for i in 0..parts.len() {
        if parts[i] == "dev" && i + 1 < parts.len() {
            return Ok(parts[i + 1].to_string());
        }
    }
    Err(AwgError::SetconfFailed(
        "could not parse default interface from `ip route`".into(),
    ))
}

fn run_iptables(args: &[&str]) -> Result<(), AwgError> {
    let output = Command::new("iptables")
        .args(args)
        .output()
        .map_err(|e| AwgError::SetconfFailed(format!("iptables failed: {}", e)))?;
    if !output.status.success() {
        return Err(AwgError::SetconfFailed(format!(
            "iptables {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}
