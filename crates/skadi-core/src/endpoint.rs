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
        || ip.octets()[0] == 0
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
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
    fn validate_literal_endpoint() {
        let endpoint = Endpoint::Ip("127.0.0.1:22".parse().unwrap());
        assert!(validate_outbound_literal(&endpoint, false).is_err());
        assert!(validate_outbound_literal(&endpoint, true).is_ok());
    }
}
