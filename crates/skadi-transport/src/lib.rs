//! Транспортный слой: TCP, TLS, REALITY, XHTTP, AmneziaWG, Hysteria2, TUIC.

pub mod awg;
pub mod crypto;
mod hybrid_kx;
pub mod hysteria2;
pub mod mux;
pub mod outbound;
mod outbound_policy;
pub mod reality;
pub mod relay;
pub mod tcp;
pub mod tls;
pub mod tls_client;
pub mod tuic;
pub mod udp;
pub mod xhttp;
mod xudp;

pub use awg::{
    apply_nat, generate_keypair, public_key_from_private, render_client_conf, render_server_conf,
    AwgClientConfig, AwgClientExport, AwgClientManager, AwgError, AwgManager, AwgNatConfig,
    AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig,
};
pub use crypto::TlsKexMode;
pub use hysteria2::{
    render_server_yaml as render_hysteria2_yaml, Hysteria2Error, Hysteria2Manager,
    Hysteria2ServerConfig,
};
pub use mux::relay_vless_mux_with_limits;
pub use outbound::{OutboundTcpTransport, TcpUpstream};
pub use reality::{
    fetch_impersonate_cert_from_dest, load_impersonate_cert_file, parse_client_hello,
    BufferedPrefixStream, RealityError, RealityServerConfig, RealityTransport,
};
pub use relay::{
    copy_bidirectional_with_idle_timeout, copy_bidirectional_with_limits, RelayLimits,
    IDLE_TIMEOUT_MSG, SESSION_LIFETIME_MSG,
};
pub use tcp::TcpTransport;
pub use tls::{TlsCertPaths, TlsError, TlsServerConfig, TlsSniCert, TlsTransport};
pub use tls_client::{TlsClientConfig, TlsOutboundTransport};
pub use tuic::{render_server_toml as render_tuic_toml, TuicError, TuicManager, TuicServerConfig};
pub use udp::{
    read_vless_udp_frame, relay_vless_udp_with_limits, write_vless_udp_frame, UdpTransport,
};
pub use xhttp::{
    accept_stream_one, accept_xhttp, PaddingRange, XhttpAcceptResult, XhttpConfig, XhttpError,
    XhttpIo, XhttpMode, XhttpSessionManager,
};
