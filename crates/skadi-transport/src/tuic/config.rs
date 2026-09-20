//! Runtime-конфигурация TUIC.

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

/// Клиентский TUIC: локальный SOCKS5 через `tuic-client`.
#[derive(Debug, Clone)]
pub struct TuicClientConfig {
    /// Адрес сервера `host:port` (HOST обычно CN сертификата).
    pub server: String,
    pub uuid: String,
    pub password: String,
    /// Локальный SOCKS5.
    pub socks5_listen: String,
    /// Опциональный IP для dial (обход DNS / SNI mismatch).
    pub ip: Option<String>,
    pub congestion_control: String,
    pub alpn: Vec<String>,
    pub udp_relay_mode: String,
    /// Пропустить проверку сертификата (lab / self-signed).
    pub allow_insecure: bool,
}
