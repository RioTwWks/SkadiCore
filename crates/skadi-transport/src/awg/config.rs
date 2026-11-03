//! Runtime-конфигурация AmneziaWG-сервера.

use std::net::SocketAddr;

/// Базовая директория UAPI-сокетов `amneziawg-go` (Linux).
///
/// По умолчанию `/var/run/amneziawg`; для тестов и нестандартных установок
/// можно задать `AWG_UAPI_DIR`.
pub fn awg_uapi_socket_dir() -> String {
    std::env::var("AWG_UAPI_DIR").unwrap_or_else(|_| "/var/run/amneziawg".to_string())
}

/// Параметры обфускации AmneziaWG 2.0 (должны совпадать с клиентом).
#[derive(Debug, Clone)]
pub struct AwgObfuscationConfig {
    pub jc: u8,
    pub jmin: u16,
    pub jmax: u16,
    pub s1: u8,
    pub s2: u8,
    pub s3: u8,
    pub s4: u8,
    pub h1: String,
    pub h2: String,
    pub h3: String,
    pub h4: String,
}

/// Peer AmneziaWG (клиент).
///
/// `Debug` маскирует preshared_key — ключи не должны утекать в логи.
#[derive(Clone)]
pub struct AwgPeerConfig {
    pub public_key: String,
    pub allowed_ips: Vec<String>,
    pub preshared_key: Option<String>,
    pub endpoint: Option<String>,
    pub persistent_keepalive: Option<u16>,
}

impl std::fmt::Debug for AwgPeerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwgPeerConfig")
            .field("public_key", &self.public_key)
            .field("allowed_ips", &self.allowed_ips)
            .field(
                "preshared_key",
                &self.preshared_key.as_ref().map(|_| skadi_core::REDACTED),
            )
            .field("endpoint", &self.endpoint)
            .field("persistent_keepalive", &self.persistent_keepalive)
            .finish()
    }
}

/// Полная конфигурация AWG-сервера.
///
/// `Debug` маскирует private_key — ключи не должны утекать в логи.
#[derive(Clone)]
pub struct AwgServerConfig {
    pub listen: SocketAddr,
    pub interface_name: String,
    pub private_key: String,
    pub address: String,
    pub mtu: Option<u16>,
    pub obfuscation: AwgObfuscationConfig,
    pub peers: Vec<AwgPeerConfig>,
}

impl std::fmt::Debug for AwgServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwgServerConfig")
            .field("listen", &self.listen)
            .field("interface_name", &self.interface_name)
            .field("private_key", &skadi_core::REDACTED)
            .field("address", &self.address)
            .field("mtu", &self.mtu)
            .field("obfuscation", &self.obfuscation)
            .field("peers", &self.peers)
            .finish()
    }
}

impl AwgServerConfig {
    /// Путь к UAPI-сокету `amneziawg-go` (Linux).
    pub fn uapi_socket_path(&self) -> String {
        format!("{}/{}.sock", awg_uapi_socket_dir(), self.interface_name)
    }
}

/// Клиентская конфигурация AWG (skadicore client).
///
/// `Debug` маскирует private_key — ключи не должны утекать в логи.
#[derive(Clone)]
pub struct AwgClientConfig {
    pub interface_name: String,
    pub private_key: String,
    pub address: String,
    pub server_public_key: String,
    pub endpoint: String,
    pub mtu: Option<u16>,
    pub dns: Option<String>,
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    pub obfuscation: AwgObfuscationConfig,
}

impl std::fmt::Debug for AwgClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwgClientConfig")
            .field("interface_name", &self.interface_name)
            .field("private_key", &skadi_core::REDACTED)
            .field("address", &self.address)
            .field("server_public_key", &self.server_public_key)
            .field("endpoint", &self.endpoint)
            .field("mtu", &self.mtu)
            .field("dns", &self.dns)
            .field("allowed_ips", &self.allowed_ips)
            .field("persistent_keepalive", &self.persistent_keepalive)
            .field("obfuscation", &self.obfuscation)
            .finish()
    }
}

impl AwgClientConfig {
    pub fn uapi_socket_path(&self) -> String {
        format!("{}/{}.sock", awg_uapi_socket_dir(), self.interface_name)
    }
}
