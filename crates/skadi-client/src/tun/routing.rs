//! Настройка policy routing для TUN (Linux, через `ip`).

use crate::config::TunRoutingConfig;
use anyhow::{bail, Context, Result};
use std::net::{IpAddr, Ipv4Addr};
use std::process::Command;
use tracing::{debug, info};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultRoute {
    pub gateway: Ipv4Addr,
    pub device: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BypassRoute {
    destination: Ipv4Addr,
    gateway: Ipv4Addr,
    device: String,
}

/// Снимок применённых правил для отката при shutdown.
pub struct RoutingGuard {
    table: u32,
    tun_address: Ipv4Addr,
    bypass_routes: Vec<BypassRoute>,
    applied: bool,
}

impl RoutingGuard {
    pub fn apply(
        tun: &crate::config::TunConfig,
        routing: &TunRoutingConfig,
        proxy_ips: &[Ipv4Addr],
    ) -> Result<Option<Self>> {
        if !routing.auto {
            return Ok(None);
        }

        let tun_address = parse_ipv4(&tun.address, "client.tun.address")?;
        let default_route = discover_default_route()?;
        let table = routing.table;

        let mut bypass_routes = Vec::new();
        let mut bypass_ips = parse_bypass_list(&routing.bypass)?;
        for ip in proxy_ips {
            if !bypass_ips.contains(ip) {
                bypass_ips.push(*ip);
            }
        }

        let bypass_count = bypass_ips.len();
        for dest in bypass_ips {
            let route = BypassRoute {
                destination: dest,
                gateway: default_route.gateway,
                device: default_route.device.clone(),
            };
            run_ip(&[
                "-4",
                "route",
                "replace",
                &format!("{}/32", route.destination),
                "via",
                &route.gateway.to_string(),
                "dev",
                &route.device,
            ])?;
            bypass_routes.push(route);
            debug!(
                destination = %dest,
                gateway = %default_route.gateway,
                device = %default_route.device,
                "TUN bypass route added"
            );
        }

        run_ip(&[
            "-4",
            "route",
            "replace",
            "default",
            "dev",
            &tun.name,
            "table",
            &table.to_string(),
        ])?;
        run_ip(&[
            "-4",
            "rule",
            "add",
            "from",
            &format!("{}/32", tun_address),
            "table",
            &table.to_string(),
        ])?;

        info!(
            tun = %tun.name,
            address = %tun_address,
            table = table,
            bypass = bypass_count,
            "TUN routing applied"
        );

        Ok(Some(Self {
            table,
            tun_address,
            bypass_routes,
            applied: true,
        }))
    }

    pub fn revert(&mut self) {
        if !self.applied {
            return;
        }

        let _ = run_ip(&[
            "-4",
            "rule",
            "del",
            "from",
            &format!("{}/32", self.tun_address),
            "table",
            &self.table.to_string(),
        ]);
        let _ = run_ip(&["-4", "route", "flush", "table", &self.table.to_string()]);

        for route in &self.bypass_routes {
            let _ = run_ip(&[
                "-4",
                "route",
                "del",
                &format!("{}/32", route.destination),
                "via",
                &route.gateway.to_string(),
                "dev",
                &route.device,
            ]);
        }

        self.applied = false;
        info!(table = self.table, "TUN routing reverted");
    }
}

impl Drop for RoutingGuard {
    fn drop(&mut self) {
        self.revert();
    }
}

pub fn discover_default_route() -> Result<DefaultRoute> {
    let output = Command::new("ip")
        .args(["-4", "route", "show", "default"])
        .output()
        .context("failed to run `ip route show default`")?;

    if !output.status.success() {
        bail!(
            "`ip route show default` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .context("no default IPv4 route found")?;

    parse_default_route_line(line)
}

pub fn parse_default_route_line(line: &str) -> Result<DefaultRoute> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let via_idx = parts
        .iter()
        .position(|p| *p == "via")
        .context("default route line missing `via`")?;
    let dev_idx = parts
        .iter()
        .position(|p| *p == "dev")
        .context("default route line missing `dev`")?;

    let gateway = parts
        .get(via_idx + 1)
        .context("default route missing gateway")?
        .parse::<Ipv4Addr>()
        .context("invalid default gateway")?;
    let device = parts
        .get(dev_idx + 1)
        .context("default route missing device")?
        .to_string();

    Ok(DefaultRoute { gateway, device })
}

pub async fn resolve_proxy_ipv4(server: &str) -> Result<Vec<Ipv4Addr>> {
    let host = server.rsplit_once(':').map(|(h, _)| h).unwrap_or(server);
    let mut ips = Vec::new();
    if let Ok(ip) = host.parse::<IpAddr>() {
        if let IpAddr::V4(v4) = ip {
            ips.push(v4);
        }
        return Ok(ips);
    }

    let addrs = tokio::net::lookup_host(format!("{}:0", host))
        .await
        .with_context(|| format!("failed to resolve proxy host {}", host))?;
    for addr in addrs {
        if let IpAddr::V4(v4) = addr.ip() {
            ips.push(v4);
        }
    }
    ips.sort();
    ips.dedup();
    Ok(ips)
}

fn parse_bypass_list(values: &[String]) -> Result<Vec<Ipv4Addr>> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        out.push(parse_ipv4(value, "client.tun.routing.bypass")?);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn parse_ipv4(value: &str, field: &str) -> Result<Ipv4Addr> {
    let ip = value
        .parse::<IpAddr>()
        .with_context(|| format!("invalid {}", field))?;
    match ip {
        IpAddr::V4(v4) => Ok(v4),
        IpAddr::V6(_) => bail!("{} must be IPv4", field),
    }
}

fn run_ip(args: &[&str]) -> Result<()> {
    let output = Command::new("ip")
        .args(args)
        .output()
        .with_context(|| format!("failed to run `ip {}`", args.join(" ")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("File exists") || stderr.contains("RTNETLINK answers: File exists") {
        return Ok(());
    }

    bail!("`ip {}` failed: {}", args.join(" "), stderr.trim());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_route_line_ok() {
        let route =
            parse_default_route_line("default via 192.168.1.1 dev eth0 proto dhcp").unwrap();
        assert_eq!(route.gateway, Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(route.device, "eth0");
    }
}
