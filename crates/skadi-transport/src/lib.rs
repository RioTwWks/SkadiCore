//! Транспортный слой: TCP, TLS, REALITY.

pub mod reality;
pub mod relay;
pub mod tcp;
pub mod tls;

pub use reality::{RealityError, RealityServerConfig, RealityTransport};
pub use relay::copy_bidirectional_with_idle_timeout;
pub use tcp::TcpTransport;
pub use tls::{TlsCertPaths, TlsError, TlsServerConfig, TlsSniCert, TlsTransport};
