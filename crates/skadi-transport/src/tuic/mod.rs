//! TUIC — QUIC прокси через внешние `tuic-server` / `tuic-client`.

mod config;
mod manager;
mod render;

pub use config::{TuicClientConfig, TuicServerConfig};
pub use manager::{TuicClientManager, TuicError, TuicManager};
pub use render::{render_client_json, render_server_toml};
