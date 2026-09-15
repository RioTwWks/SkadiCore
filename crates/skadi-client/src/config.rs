//! Конфигурация клиентского режима (локальный SOCKS5 / TUN → удалённый VLESS).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_core::Endpoint;
use skadi_protocol::vless::Uuid;
use skadi_transport::TlsClientConfig;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    pub client: ClientListenConfig,
    pub remote: RemoteConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientListenConfig {
    /// Локальный SOCKS5. Опционально, если включён TUN.
    pub listen: Option<String>,
    #[serde(default)]
    pub tun: TunConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TunConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_tun_name")]
    pub name: String,
    #[serde(default = "default_tun_address")]
    pub address: String,
    #[serde(default = "default_tun_gateway")]
    pub gateway: String,
    #[serde(default = "default_tun_netmask")]
    pub netmask: String,
    #[serde(default = "default_tun_mtu")]
    pub mtu: u16,
    #[serde(default)]
    pub routing: TunRoutingConfig,
    #[serde(default)]
    pub dns: TunDnsConfig,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: default_tun_name(),
            address: default_tun_address(),
            gateway: default_tun_gateway(),
            netmask: default_tun_netmask(),
            mtu: default_tun_mtu(),
            routing: TunRoutingConfig::default(),
            dns: TunDnsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TunRoutingConfig {
    /// Автоматически настроить `ip rule` / `ip route` (требует root/CAP_NET_ADMIN).
    #[serde(default)]
    pub auto: bool,
    #[serde(default = "default_routing_table")]
    pub table: u32,
    /// Дополнительные IPv4, которые не должны идти в TUN (помимо IP прокси).
    #[serde(default)]
    pub bypass: Vec<String>,
}

impl Default for TunRoutingConfig {
    fn default() -> Self {
        Self {
            auto: false,
            table: default_routing_table(),
            bypass: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TunDnsConfig {
    /// Перенаправлять UDP/53 через `server` в VLESS-туннель.
    #[serde(default = "default_dns_hijack")]
    pub hijack: bool,
    #[serde(default = "default_dns_server")]
    pub server: Option<String>,
}

impl Default for TunDnsConfig {
    fn default() -> Self {
        Self {
            hijack: default_dns_hijack(),
            server: default_dns_server(),
        }
    }
}

fn default_routing_table() -> u32 {
    100
}

fn default_dns_hijack() -> bool {
    true
}

fn default_dns_server() -> Option<String> {
    Some("8.8.8.8".to_string())
}

fn default_tun_name() -> String {
    "skadi0".to_string()
}

fn default_tun_address() -> String {
    "10.0.0.2".to_string()
}

fn default_tun_gateway() -> String {
    "10.0.0.1".to_string()
}

fn default_tun_netmask() -> String {
    "255.255.255.0".to_string()
}

fn default_tun_mtu() -> u16 {
    1500
}

impl TunConfig {
    pub fn validate(&self) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }
        parse_ip(&self.address, "client.tun.address")?;
        parse_ip(&self.gateway, "client.tun.gateway")?;
        parse_ip(&self.netmask, "client.tun.netmask")?;
        if self.name.is_empty() {
            bail!("client.tun.name must not be empty");
        }
        if self.mtu < 576 {
            bail!("client.tun.mtu must be at least 576");
        }
        if self.routing.table == 0 || self.routing.table > 252 {
            bail!("client.tun.routing.table must be in 1..=252");
        }
        parse_ipv4_list(&self.routing.bypass, "client.tun.routing.bypass")?;
        if self.dns.hijack && self.dns.server.is_none() {
            bail!("client.tun.dns.hijack requires client.tun.dns.server");
        }
        if let Some(server) = &self.dns.server {
            parse_dns_upstream(server)?;
        }
        Ok(())
    }
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
        if self.client.listen.is_none() && !self.client.tun.enabled {
            bail!("client: set client.listen (SOCKS5) or client.tun.enabled = true");
        }
        if let Some(listen) = &self.client.listen {
            parse_socket_addr(listen, "client.listen")?;
        }
        parse_server_endpoint(&self.remote.server)?;
        Uuid::parse(&self.remote.uuid).map_err(|e| anyhow::anyhow!("remote.uuid: {}", e))?;
        self.client.tun.validate()?;
        Ok(())
    }

    pub fn socks5_enabled(&self) -> bool {
        self.client.listen.is_some()
    }

    pub fn tun_enabled(&self) -> bool {
        self.client.tun.enabled
    }

    pub fn listen_addr(&self) -> Result<SocketAddr> {
        let listen = self
            .client
            .listen
            .as_deref()
            .context("client.listen is not configured")?;
        parse_socket_addr(listen, "client.listen")
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

fn parse_ip(value: &str, field: &str) -> Result<IpAddr> {
    value
        .parse()
        .with_context(|| format!("invalid {}: {}", field, value))
}

fn parse_ipv4_list(values: &[String], field: &str) -> Result<Vec<std::net::Ipv4Addr>> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        let ip = parse_ip(value, field)?;
        match ip {
            IpAddr::V4(v4) => out.push(v4),
            IpAddr::V6(_) => bail!("{} must be IPv4: {}", field, value),
        }
    }
    Ok(out)
}

fn parse_dns_upstream(value: &str) -> Result<SocketAddr> {
    if value.contains(':') {
        return value
            .parse()
            .with_context(|| format!("invalid client.tun.dns.server: {}", value));
    }
    let ip = parse_ip(value, "client.tun.dns.server")?;
    Ok(SocketAddr::new(ip, 53))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tun_only_config_validates() {
        let raw = r#"
[client]
[client.tun]
enabled = true

[remote]
server = "127.0.0.1:443"
uuid = "00000000-0000-0000-0000-000000000001"
"#;
        let config: ClientConfig = toml::from_str(raw).unwrap();
        config.validate().unwrap();
        assert!(!config.socks5_enabled());
        assert!(config.tun_enabled());
    }

    #[test]
    fn tun_routing_dns_config_validates() {
        let raw = r#"
[client]
[client.tun]
enabled = true

[client.tun.routing]
auto = true
table = 100
bypass = ["192.168.0.1"]

[client.tun.dns]
hijack = true
server = "1.1.1.1:53"

[remote]
server = "proxy.example.com:443"
uuid = "00000000-0000-0000-0000-000000000001"
"#;
        let config: ClientConfig = toml::from_str(raw).unwrap();
        config.validate().unwrap();
        assert!(config.client.tun.routing.auto);
        assert_eq!(config.client.tun.routing.table, 100);
        assert_eq!(config.client.tun.dns.server.as_deref(), Some("1.1.1.1:53"));
    }

    #[test]
    fn tun_invalid_routing_table_fails() {
        let raw = r#"
[client]
[client.tun]
enabled = true

[client.tun.routing]
table = 0

[remote]
server = "127.0.0.1:443"
uuid = "00000000-0000-0000-0000-000000000001"
"#;
        let config: ClientConfig = toml::from_str(raw).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn missing_inbound_fails_validation() {
        let raw = r#"
[client]

[remote]
server = "127.0.0.1:443"
uuid = "00000000-0000-0000-0000-000000000001"
"#;
        let config: ClientConfig = toml::from_str(raw).unwrap();
        assert!(config.validate().is_err());
    }
}
