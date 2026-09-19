//! TUIC — QUIC прокси через внешний `tuic-server` binary.

mod config;
mod manager;
mod render;

pub use config::TuicServerConfig;
pub use manager::{TuicError, TuicManager};
pub use render::render_server_toml;
