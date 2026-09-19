//! REALITY transport (Xray-compatible TLS camouflage).

mod auth;
mod cert;
mod client_auth;
mod hello_parser;
mod impersonate;
mod prefixed;
mod server;

pub use cert::generate_reality_cert;
pub use impersonate::{fetch_impersonate_cert_from_dest, load_impersonate_cert_file};
pub use rustls::reality::{verify_server_cert_hmac, RealityServerCertVerifier, AUTH_HMAC_TAIL_LEN};

pub use client_auth::{RealityClientAuth, RealityClientAuthError};
pub use hello_parser::{parse_client_hello, ClientHelloInfo};
pub use prefixed::BufferedPrefixStream;
pub use server::{RealityError, RealityServerConfig, RealityTransport};
