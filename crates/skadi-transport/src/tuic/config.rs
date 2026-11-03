//! Runtime-конфигурация TUIC.

use std::fmt;
use std::net::SocketAddr;

/// Конфигурация TUIC backend (внешний `tuic-server` binary).
///
/// `Debug` маскирует uuid и пароль — конфиг не должен утекать в логи.
#[derive(Clone)]
pub struct TuicServerConfig {
    pub listen: SocketAddr,
    pub uuid: String,
    pub password: String,
    pub cert_path: String,
    pub key_path: String,
    pub congestion_control: String,
    pub alpn: Vec<String>,
}

impl fmt::Debug for TuicServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TuicServerConfig")
            .field("listen", &self.listen)
            .field("uuid", &skadi_core::REDACTED)
            .field("password", &skadi_core::REDACTED)
            .field("cert_path", &self.cert_path)
            .field("key_path", &self.key_path)
            .field("congestion_control", &self.congestion_control)
            .field("alpn", &self.alpn)
            .finish()
    }
}

/// Клиентский TUIC: локальный SOCKS5 через `tuic-client`.
///
/// `Debug` маскирует uuid и пароль — конфиг не должен утекать в логи.
#[derive(Clone)]
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

impl fmt::Debug for TuicClientConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TuicClientConfig")
            .field("server", &self.server)
            .field("uuid", &skadi_core::REDACTED)
            .field("password", &skadi_core::REDACTED)
            .field("socks5_listen", &self.socks5_listen)
            .field("ip", &self.ip)
            .field("congestion_control", &self.congestion_control)
            .field("alpn", &self.alpn)
            .field("udp_relay_mode", &self.udp_relay_mode)
            .field("allow_insecure", &self.allow_insecure)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_masks_uuid_and_password() {
        let config = TuicServerConfig {
            listen: "127.0.0.1:8443".parse().unwrap(),
            uuid: "00000000-0000-0000-0000-0000000000aa".into(),
            password: "super-secret".into(),
            cert_path: "cert.pem".into(),
            key_path: "key.pem".into(),
            congestion_control: "bbr".into(),
            alpn: vec!["h3".into()],
        };
        let rendered = format!("{:?}", config);
        assert!(!rendered.contains("super-secret"));
        assert!(!rendered.contains("00000000-0000-0000-0000-0000000000aa"));
    }
}
