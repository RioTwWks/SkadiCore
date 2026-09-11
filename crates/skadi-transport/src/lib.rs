//! Транспортный слой: TCP, TLS, REALITY.

pub mod reality;
pub mod tcp;
pub mod tls;

pub use reality::{RealityError, RealityServerConfig, RealityTransport};
pub use tcp::TcpTransport;
pub use tls::{TlsCertPaths, TlsError, TlsServerConfig, TlsSniCert, TlsTransport};
