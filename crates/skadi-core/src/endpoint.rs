use std::fmt;
use std::net::SocketAddr;

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
