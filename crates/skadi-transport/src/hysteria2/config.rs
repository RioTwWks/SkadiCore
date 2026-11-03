//! Runtime-конфигурация Hysteria2.

use std::fmt;
use std::net::SocketAddr;

/// Конфигурация Hysteria2 backend (внешний `hysteria` binary).
///
/// `Debug` маскирует пароль — конфиг не должен утекать в логи.
#[derive(Clone)]
pub struct Hysteria2ServerConfig {
    pub listen: SocketAddr,
    pub password: String,
    pub cert_path: String,
    pub key_path: String,
    pub masquerade_url: Option<String>,
}

impl fmt::Debug for Hysteria2ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hysteria2ServerConfig")
            .field("listen", &self.listen)
            .field("password", &skadi_core::REDACTED)
            .field("cert_path", &self.cert_path)
            .field("key_path", &self.key_path)
            .field("masquerade_url", &self.masquerade_url)
            .finish()
    }
}

/// Клиентский Hysteria2: локальный SOCKS5 через `hysteria client`.
///
/// `Debug` маскирует пароль — конфиг не должен утекать в логи.
#[derive(Clone)]
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

impl fmt::Debug for Hysteria2ClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hysteria2ClientConfig")
            .field("server", &self.server)
            .field("password", &skadi_core::REDACTED)
            .field("socks5_listen", &self.socks5_listen)
            .field("http_listen", &self.http_listen)
            .field("sni", &self.sni)
            .field("ca_file", &self.ca_file)
            .field("insecure", &self.insecure)
            .field("pin_sha256", &self.pin_sha256)
            .field("bandwidth_up", &self.bandwidth_up)
            .field("bandwidth_down", &self.bandwidth_down)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_password() {
        let config = Hysteria2ServerConfig {
            listen: "127.0.0.1:443".parse().unwrap(),
            password: "super-secret".into(),
            cert_path: "cert.pem".into(),
            key_path: "key.pem".into(),
            masquerade_url: None,
        };
        let rendered = format!("{:?}", config);
        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains("[REDACTED]"));
    }
}
