//! Hysteria2 — QUIC/UDP прокси через внешний `hysteria` binary.

mod config;
mod manager;
mod render;

pub use config::Hysteria2ServerConfig;
pub use manager::{Hysteria2Error, Hysteria2Manager};
pub use render::render_server_yaml;
