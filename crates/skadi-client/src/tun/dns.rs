//! Перехват DNS (UDP/53) и форвард через VLESS.

use crate::config::TunDnsConfig;
use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsHijack {
    upstream: SocketAddr,
}

impl DnsHijack {
    pub fn from_config(dns: &TunDnsConfig) -> Result<Option<Self>> {
        if !dns.hijack {
            return Ok(None);
        }
        let server = dns
            .server
            .as_deref()
            .context("client.tun.dns.hijack requires client.tun.dns.server")?;
        let upstream = parse_dns_upstream(server)?;
        Ok(Some(Self { upstream }))
    }

    pub fn rewrite(&self, remote: SocketAddr) -> SocketAddr {
        if remote.port() == 53 {
            self.upstream
        } else {
            remote
        }
    }
}

pub fn parse_dns_upstream(value: &str) -> Result<SocketAddr> {
    if value.contains(':') {
        return value
            .parse()
            .with_context(|| format!("invalid client.tun.dns.server: {}", value));
    }

    let ip: IpAddr = value
        .parse()
        .with_context(|| format!("invalid client.tun.dns.server: {}", value))?;
    Ok(SocketAddr::new(ip, 53))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn rewrite_dns_port_only() {
        let hijack = DnsHijack {
            upstream: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53),
        };
        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 5353);
        let router_dns = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 53);
        let https = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443);

        assert_eq!(hijack.rewrite(router_dns), hijack.upstream);
        assert_eq!(hijack.rewrite(https), https);
        assert_eq!(hijack.rewrite(local), local);
    }
}
