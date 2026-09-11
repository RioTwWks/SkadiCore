//! Транспортный слой: TCP, TLS, позже XHTTP.

pub mod tcp;
pub mod tls;

pub use tcp::TcpTransport;
pub use tls::{TlsError, TlsServerConfig, TlsTransport};
