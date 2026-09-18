//! Runtime-конфигурация TUIC-сервера.

use std::net::SocketAddr;

/// Конфигурация TUIC backend (внешний `tuic-server` binary).
#[derive(Debug, Clone)]
pub struct TuicServerConfig {
    pub listen: SocketAddr,
    pub uuid: String,
    pub password: String,
    pub cert_path: String,
    pub key_path: String,
    pub congestion_control: String,
    pub alpn: Vec<String>,
}
