//! Runtime-конфигурация Hysteria2-сервера.

use std::net::SocketAddr;

/// Конфигурация Hysteria2 backend (внешний `hysteria` binary).
#[derive(Debug, Clone)]
pub struct Hysteria2ServerConfig {
    pub listen: SocketAddr,
    pub password: String,
    pub cert_path: String,
    pub key_path: String,
    pub masquerade_url: Option<String>,
}
