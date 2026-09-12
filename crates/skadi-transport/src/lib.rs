//! Транспортный слой: TCP, TLS, REALITY.

pub mod outbound;
pub mod reality;
pub mod relay;
pub mod tcp;
pub mod tls;
pub mod tls_client;
pub mod udp;

pub use reality::{RealityError, RealityServerConfig, RealityTransport};
pub use relay::{
    copy_bidirectional_with_idle_timeout, copy_bidirectional_with_limits, RelayLimits,
    IDLE_TIMEOUT_MSG, SESSION_LIFETIME_MSG,
};
pub use outbound::{OutboundTcpTransport, TcpUpstream};
pub use tcp::TcpTransport;
pub use tls::{TlsCertPaths, TlsError, TlsServerConfig, TlsSniCert, TlsTransport};
pub use tls_client::{TlsClientConfig, TlsOutboundTransport};
pub use udp::{relay_vless_udp_with_limits, UdpTransport};
