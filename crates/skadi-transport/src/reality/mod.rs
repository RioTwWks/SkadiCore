//! REALITY transport (Xray-compatible TLS camouflage).

mod auth;
mod cert;
mod hello_parser;
mod impersonate;
mod prefixed;
mod server;

pub use cert::generate_reality_cert;
pub use impersonate::{fetch_impersonate_cert_from_dest, load_impersonate_cert_file};

pub use hello_parser::{parse_client_hello, ClientHelloInfo};
pub use server::{RealityError, RealityServerConfig, RealityTransport};
