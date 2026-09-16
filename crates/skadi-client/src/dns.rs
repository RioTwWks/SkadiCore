//! Перехват DNS (UDP/53): форвард UDP или резолв через DoH в VLESS-туннеле.

use crate::config::TunDnsConfig;
use anyhow::{bail, Context, Result};
use std::net::SocketAddr;

use crate::doh::DohConfig;

#[derive(Clone)]
pub enum DnsHandler {
    UdpForward(SocketAddr),
    Doh(DohConfig),
}

impl DnsHandler {
    pub fn from_config(dns: &TunDnsConfig) -> Result<Option<Self>> {
        if !dns.hijack {
            return Ok(None);
        }
        let server = dns
            .server
            .as_deref()
            .context("client.tun.dns.hijack requires client.tun.dns.server")?;

        match dns.mode.as_str() {
            "udp" => Ok(Some(Self::UdpForward(parse_dns_upstream(server)?))),
            "doh" => Ok(Some(Self::Doh(DohConfig::from_server_url(server)?))),
            other => bail!(
                "client.tun.dns.mode must be \"udp\" or \"doh\", got {}",
                other
            ),
        }
    }

    pub fn rewrite_udp_upstream(&self, remote: SocketAddr) -> SocketAddr {
        match self {
            Self::UdpForward(upstream) if remote.port() == 53 => *upstream,
            _ => remote,
        }
    }
}

pub fn parse_dns_upstream(value: &str) -> Result<SocketAddr> {
    if value.starts_with("https://") {
        bail!("client.tun.dns.server is a DoH URL; set client.tun.dns.mode = \"doh\"");
    }
    if value.contains(':') {
        return value
            .parse()
            .with_context(|| format!("invalid client.tun.dns.server: {}", value));
    }

    let ip: std::net::IpAddr = value
        .parse()
        .with_context(|| format!("invalid client.tun.dns.server: {}", value))?;
    Ok(SocketAddr::new(ip, 53))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TunDnsConfig;
    use std::net::Ipv4Addr;

    #[test]
    fn rewrite_dns_port_only() {
        let handler = DnsHandler::UdpForward(SocketAddr::new(
            std::net::IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)),
            53,
        ));
        let local = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)), 5353);
        let router_dns = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 53);
        let https = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443);

        assert_eq!(handler.rewrite_udp_upstream(router_dns).port(), 53);
        assert_eq!(
            handler.rewrite_udp_upstream(router_dns),
            SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8)), 53)
        );
        assert_eq!(handler.rewrite_udp_upstream(https), https);
        assert_eq!(handler.rewrite_udp_upstream(local), local);
    }

    #[test]
    fn doh_mode_from_config() {
        let dns = TunDnsConfig {
            hijack: true,
            mode: "doh".to_string(),
            server: Some("https://cloudflare-dns.com/dns-query".to_string()),
        };
        let handler = DnsHandler::from_config(&dns).unwrap();
        assert!(matches!(handler, Some(DnsHandler::Doh(_))));
    }

    #[test]
    fn rejects_doh_url_in_udp_mode() {
        let dns = TunDnsConfig {
            hijack: true,
            mode: "udp".to_string(),
            server: Some("https://cloudflare-dns.com/dns-query".to_string()),
        };
        assert!(DnsHandler::from_config(&dns).is_err());
    }
}
