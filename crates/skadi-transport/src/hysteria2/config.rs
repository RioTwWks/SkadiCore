//! Runtime-конфигурация Hysteria2.

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

/// Клиентский Hysteria2: локальный SOCKS5 через `hysteria client`.
#[derive(Debug, Clone)]
pub struct Hysteria2ClientConfig {
    /// Адрес сервера `host:port`.
    pub server: String,
    pub password: String,
    /// Локальный SOCKS5 (`127.0.0.1:10808`).
    pub socks5_listen: String,
    /// Опциональный HTTP proxy listen.
    pub http_listen: Option<String>,
    pub sni: Option<String>,
    pub ca_file: Option<String>,
    /// Отключить проверку TLS (только для self-signed / lab).
    pub insecure: bool,
    pub pin_sha256: Option<String>,
    pub bandwidth_up: Option<String>,
    pub bandwidth_down: Option<String>,
}
