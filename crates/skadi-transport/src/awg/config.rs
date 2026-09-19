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
#[derive(Debug, Clone)]
pub struct AwgPeerConfig {
    pub public_key: String,
    pub allowed_ips: Vec<String>,
    pub preshared_key: Option<String>,
    pub endpoint: Option<String>,
    pub persistent_keepalive: Option<u16>,
}

/// Полная конфигурация AWG-сервера.
#[derive(Debug, Clone)]
pub struct AwgServerConfig {
    pub listen: SocketAddr,
    pub interface_name: String,
    pub private_key: String,
    pub address: String,
    pub mtu: Option<u16>,
    pub obfuscation: AwgObfuscationConfig,
    pub peers: Vec<AwgPeerConfig>,
}

impl AwgServerConfig {
    /// Путь к UAPI-сокету `amneziawg-go` (Linux).
    pub fn uapi_socket_path(&self) -> String {
        format!("{}/{}.sock", awg_uapi_socket_dir(), self.interface_name)
    }
}

/// Клиентская конфигурация AWG (skadicore client).
#[derive(Debug, Clone)]
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

impl AwgClientConfig {
    pub fn uapi_socket_path(&self) -> String {
        format!("{}/{}.sock", awg_uapi_socket_dir(), self.interface_name)
    }
}
