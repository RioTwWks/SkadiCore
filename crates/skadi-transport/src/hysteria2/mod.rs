//! Hysteria2 — QUIC/UDP прокси через внешний `hysteria` binary.

mod config;
mod manager;
mod render;

pub use config::{Hysteria2ClientConfig, Hysteria2ServerConfig};
pub use manager::{Hysteria2ClientManager, Hysteria2Error, Hysteria2Manager};
pub use render::{render_client_yaml, render_server_yaml, render_share_uri};
