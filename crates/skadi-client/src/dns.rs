//! Перехват DNS (UDP/53): форвард UDP, DoH или DoT через VLESS-туннель.

use crate::config::TunDnsConfig;
use crate::dot::DotConfig;
use anyhow::{bail, Context, Result};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::doh::DohConfig;

#[derive(Clone)]
pub enum DnsHandler {
    UdpForward(SocketAddr),
    Doh(DohConfig),
    Dot(DotConfig),
}

/// Настройки перехвата DNS в TUN: upstream + блокировка системного DoT/DoH.
#[derive(Clone)]
pub struct DnsIntercept {
    handler: Option<DnsHandler>,
    block_system_dot: bool,
    block_system_doh: bool,
}

impl DnsIntercept {
    pub fn from_config(dns: &TunDnsConfig) -> Result<Self> {
        let handler = if dns.hijack {
            Some(DnsHandler::from_config(dns)?)
        } else {
            None
        };
        Ok(Self {
            handler,
            block_system_dot: dns.block_system_dot,
            block_system_doh: dns.block_system_doh,
        })
    }

    pub fn is_active(&self) -> bool {
        self.handler.is_some()
    }

    pub fn handler(&self) -> Option<&DnsHandler> {
        self.handler.as_ref()
    }

    pub fn should_block_tcp(&self, remote: SocketAddr) -> bool {
        if self.handler.is_none() {
            return false;
        }
        if self.block_system_dot && remote.port() == 853 {
            return true;
        }
        if self.block_system_doh && remote.port() == 443 && is_known_doh_ip(remote.ip()) {
            return true;
        }
        false
    }
}

impl DnsHandler {
    pub fn from_config(dns: &TunDnsConfig) -> Result<Self> {
        let server = dns
            .server
            .as_deref()
            .context("client.tun.dns.hijack requires client.tun.dns.server")?;

        match dns.mode.as_str() {
            "udp" => Ok(Self::UdpForward(parse_dns_upstream(server)?)),
            "doh" => Ok(Self::Doh(DohConfig::from_server_url(server)?)),
            "dot" => Ok(Self::Dot(DotConfig::from_server_url(server)?)),
            other => bail!(
                "client.tun.dns.mode must be \"udp\", \"doh\", or \"dot\", got {}",
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
    if value.starts_with("tls://") {
        bail!("client.tun.dns.server is a DoT URL; set client.tun.dns.mode = \"dot\"");
    }
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

/// Известные IP публичных DoH-резолверов (Cloudflare, Google, Quad9, OpenDNS).
pub fn is_known_doh_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => KNOWN_DOH_V4.contains(&ip),
        IpAddr::V6(ip) => KNOWN_DOH_V6.contains(&ip),
    }
}

const KNOWN_DOH_V4: [Ipv4Addr; 8] = [
    Ipv4Addr::new(1, 1, 1, 1),
    Ipv4Addr::new(1, 0, 0, 1),
    Ipv4Addr::new(8, 8, 8, 8),
    Ipv4Addr::new(8, 8, 4, 4),
    Ipv4Addr::new(9, 9, 9, 9),
    Ipv4Addr::new(149, 112, 112, 112),
    Ipv4Addr::new(208, 67, 222, 222),
    Ipv4Addr::new(208, 67, 220, 220),
];

const KNOWN_DOH_V6: [Ipv6Addr; 6] = [
    Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111),
    Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1001),
    Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888),
    Ipv6Addr::new(0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8844),
    Ipv6Addr::new(0x2620, 0xfe, 0xfe, 0, 0, 0, 0, 0),
    Ipv6Addr::new(0x2620, 0xfe, 0, 0, 0, 0, 0, 0xfe),
];

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
            block_system_dot: true,
            block_system_doh: true,
        };
        let intercept = DnsIntercept::from_config(&dns).unwrap();
        assert!(matches!(intercept.handler(), Some(DnsHandler::Doh(_))));
    }

    #[test]
    fn dot_mode_from_config() {
        let dns = TunDnsConfig {
            hijack: true,
            mode: "dot".to_string(),
            server: Some("tls://one.one.one.one".to_string()),
            block_system_dot: true,
            block_system_doh: true,
        };
        let intercept = DnsIntercept::from_config(&dns).unwrap();
        assert!(matches!(intercept.handler(), Some(DnsHandler::Dot(_))));
    }

    #[test]
    fn rejects_doh_url_in_udp_mode() {
        let dns = TunDnsConfig {
            hijack: true,
            mode: "udp".to_string(),
            server: Some("https://cloudflare-dns.com/dns-query".to_string()),
            block_system_dot: true,
            block_system_doh: true,
        };
        assert!(DnsIntercept::from_config(&dns).is_err());
    }

    #[test]
    fn blocks_dot_port_when_enabled() {
        let dns = TunDnsConfig {
            hijack: true,
            mode: "udp".to_string(),
            server: Some("8.8.8.8".to_string()),
            block_system_dot: true,
            block_system_doh: false,
        };
        let intercept = DnsIntercept::from_config(&dns).unwrap();
        let dot = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9)), 853);
        assert!(intercept.should_block_tcp(dot));
    }

    #[test]
    fn blocks_known_doh_on_443() {
        let dns = TunDnsConfig {
            hijack: true,
            mode: "udp".to_string(),
            server: Some("8.8.8.8".to_string()),
            block_system_dot: false,
            block_system_doh: true,
        };
        let intercept = DnsIntercept::from_config(&dns).unwrap();
        let doh = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 443);
        let web = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 443);
        assert!(intercept.should_block_tcp(doh));
        assert!(!intercept.should_block_tcp(web));
    }

    #[test]
    fn no_block_when_hijack_disabled() {
        let dns = TunDnsConfig {
            hijack: false,
            mode: "udp".to_string(),
            server: Some("8.8.8.8".to_string()),
            block_system_dot: true,
            block_system_doh: true,
        };
        let intercept = DnsIntercept::from_config(&dns).unwrap();
        let dot = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 853);
        assert!(!intercept.should_block_tcp(dot));
    }
}
