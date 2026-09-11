//! REALITY transport (Xray-compatible TLS camouflage).

mod auth;
mod cert;
mod hello_parser;
mod prefixed;
mod server;

pub use hello_parser::{parse_client_hello, ClientHelloInfo};
pub use server::{RealityError, RealityServerConfig, RealityTransport};
