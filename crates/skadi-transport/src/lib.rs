//! Транспортный слой: TCP, TLS, REALITY, XHTTP.

pub mod crypto;
mod hybrid_kx;
pub mod mux;
pub mod outbound;
mod outbound_policy;
pub mod reality;
pub mod relay;
pub mod tcp;
pub mod tls;
pub mod tls_client;
pub mod udp;
pub mod xhttp;
mod xudp;

pub use crypto::TlsKexMode;
pub use mux::relay_vless_mux_with_limits;
pub use outbound::{OutboundTcpTransport, TcpUpstream};
pub use reality::{
    fetch_impersonate_cert_from_dest, load_impersonate_cert_file, RealityError,
    RealityServerConfig, RealityTransport,
};
pub use relay::{
    copy_bidirectional_with_idle_timeout, copy_bidirectional_with_limits, RelayLimits,
    IDLE_TIMEOUT_MSG, SESSION_LIFETIME_MSG,
};
pub use tcp::TcpTransport;
pub use tls::{TlsCertPaths, TlsError, TlsServerConfig, TlsSniCert, TlsTransport};
pub use tls_client::{TlsClientConfig, TlsOutboundTransport};
pub use udp::{
    read_vless_udp_frame, relay_vless_udp_with_limits, write_vless_udp_frame, UdpTransport,
};
pub use xhttp::{
    accept_stream_one, accept_xhttp, PaddingRange, XhttpAcceptResult, XhttpConfig, XhttpError,
    XhttpIo, XhttpMode, XhttpSessionManager,
};
