//! Конфигурация клиентского режима (локальный SOCKS5 → удалённый VLESS).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_core::Endpoint;
use skadi_protocol::vless::Uuid;
use skadi_transport::TlsClientConfig;
use std::net::SocketAddr;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    pub client: ClientListenConfig,
    pub remote: RemoteConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientListenConfig {
    pub listen: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteConfig {
    /// Адрес удалённого прокси: `host:port`.
    pub server: String,
    pub uuid: String,
    #[serde(default)]
    pub tls: RemoteTlsConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RemoteTlsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub ca_file: Option<String>,
    /// SNI для TLS handshake. По умолчанию — hostname из `server`.
    pub server_name: Option<String>,
}

impl ClientConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read client config {:?}", path))?;
        let config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse client config {:?}", path))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        parse_socket_addr(&self.client.listen, "client.listen")?;
        parse_server_endpoint(&self.remote.server)?;
        Uuid::parse(&self.remote.uuid)
            .map_err(|e| anyhow::anyhow!("remote.uuid: {}", e))?;
        if self.remote.tls.enabled && self.remote.tls.ca_file.is_none() {
            // Системные CA допустимы, если ca_file не задан.
        }
        Ok(())
    }

    pub fn listen_addr(&self) -> Result<SocketAddr> {
        parse_socket_addr(&self.client.listen, "client.listen")
    }

    pub fn proxy_endpoint(&self) -> Result<Endpoint> {
        parse_server_endpoint(&self.remote.server)
    }

    pub fn tls_sni_endpoint(&self) -> Result<Endpoint> {
        if let Some(name) = &self.remote.tls.server_name {
            let (_, port) = split_host_port(&self.remote.server)?;
            return Ok(Endpoint::Domain(name.clone(), port));
        }
        self.proxy_endpoint()
    }

    pub fn uuid_bytes(&self) -> Result<[u8; 16]> {
        Ok(*Uuid::parse(&self.remote.uuid)?.as_bytes())
    }

    pub fn tls_client_config(&self) -> TlsClientConfig {
        TlsClientConfig {
            ca_file: self.remote.tls.ca_file.clone(),
            client_cert: None,
            client_key: None,
        }
    }

    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

fn parse_socket_addr(value: &str, field: &str) -> Result<SocketAddr> {
    value
        .parse()
        .with_context(|| format!("invalid {}: {}", field, value))
}

fn parse_server_endpoint(value: &str) -> Result<Endpoint> {
    let (host, port) = split_host_port(value)?;
    if let Ok(ip) = host.parse() {
        Ok(Endpoint::Ip(SocketAddr::new(ip, port)))
    } else {
        Ok(Endpoint::Domain(host, port))
    }
}

fn split_host_port(value: &str) -> Result<(String, u16)> {
    let (host, port_str) = value
        .rsplit_once(':')
        .with_context(|| format!("invalid server address (expected host:port): {}", value))?;
    if host.is_empty() {
        bail!("empty server host in {}", value);
    }
    let port = port_str
        .parse()
        .with_context(|| format!("invalid server port in {}", value))?;
    Ok((host.to_string(), port))
}
