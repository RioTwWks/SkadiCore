use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::Error;

/// Целевой адрес: домен или IP с портом.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Ip(SocketAddr),
    Domain(String, u16),
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Endpoint::Ip(addr) => write!(f, "{}", addr),
            Endpoint::Domain(host, port) => write!(f, "{}:{}", host, port),
        }
    }
}

/// Заблокированные для outbound-релея адреса (SSRF-защита).
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_forbidden_ipv4(v4),
        IpAddr::V6(v6) => is_forbidden_ipv6(v6),
    }
}

fn is_forbidden_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || is_carrier_grade_nat(ip)
        || is_benchmarking_ipv4(ip)
        || ip.octets()[0] == 0
}

/// RFC 6598 Shared Address Space (`100.64.0.0/10`) — CGNAT / cloud metadata ranges.
fn is_carrier_grade_nat(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 100 && (64..128).contains(&octets[1])
}

/// RFC 2544 benchmarking (`198.18.0.0/15`).
fn is_benchmarking_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    octets[0] == 198 && (18..20).contains(&octets[1])
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
    // IPv4-mapped (`::ffff:x.x.x.x`) — проверяем вложенный IPv4, иначе loopback/private
    // обходят v6-only правила.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_forbidden_ipv4(v4);
    }

    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_unique_local()
        || ip.is_unicast_link_local()
        || ip.is_multicast()
}

/// Проверка literal IP до DNS-резолва.
pub fn validate_outbound_literal(endpoint: &Endpoint, allow_private: bool) -> Result<(), Error> {
    if allow_private {
        return Ok(());
    }

    if let Endpoint::Ip(addr) = endpoint {
        if is_forbidden_ip(addr.ip()) {
            return Err(Error::ForbiddenDestination(addr.to_string()));
        }
    }

    Ok(())
}

/// Проверка всех адресов после DNS-резолва.
pub fn validate_resolved_addrs(addrs: &[SocketAddr], allow_private: bool) -> Result<(), Error> {
    if allow_private {
        return Ok(());
    }

    for addr in addrs {
        if is_forbidden_ip(addr.ip()) {
            return Err(Error::ForbiddenDestination(addr.to_string()));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_loopback_and_private_ipv4() {
        assert!(is_forbidden_ip("127.0.0.1".parse().unwrap()));
        assert!(is_forbidden_ip("10.0.0.1".parse().unwrap()));
        assert!(is_forbidden_ip("192.168.1.1".parse().unwrap()));
        assert!(is_forbidden_ip("169.254.1.1".parse().unwrap()));
        assert!(is_forbidden_ip("100.64.0.1".parse().unwrap()));
        assert!(is_forbidden_ip("100.127.255.255".parse().unwrap()));
        assert!(is_forbidden_ip("198.18.0.1".parse().unwrap()));
        assert!(is_forbidden_ip("224.0.0.1".parse().unwrap()));
        assert!(!is_forbidden_ip("100.63.255.255".parse().unwrap()));
        assert!(!is_forbidden_ip("100.128.0.1".parse().unwrap()));
        assert!(!is_forbidden_ip("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn blocks_loopback_and_ula_ipv6() {
        assert!(is_forbidden_ip("::1".parse().unwrap()));
        assert!(is_forbidden_ip("fd00::1".parse().unwrap()));
        assert!(is_forbidden_ip("fe80::1".parse().unwrap()));
        assert!(!is_forbidden_ip("2001:4860:4860::8888".parse().unwrap()));
    }

    #[test]
    fn blocks_ipv4_mapped_loopback_and_private() {
        assert!(is_forbidden_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(is_forbidden_ip("::ffff:10.0.0.1".parse().unwrap()));
        assert!(is_forbidden_ip("::ffff:100.64.1.2".parse().unwrap()));
        assert!(!is_forbidden_ip("::ffff:8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn validate_literal_endpoint() {
        let endpoint = Endpoint::Ip("127.0.0.1:22".parse().unwrap());
        assert!(validate_outbound_literal(&endpoint, false).is_err());
        assert!(validate_outbound_literal(&endpoint, true).is_ok());
    }
}
