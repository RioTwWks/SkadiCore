//! Конфигурация клиентского режима (локальный SOCKS5 / TUN → удалённый VLESS).

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_core::Endpoint;
use skadi_protocol::vless::Uuid;
use skadi_transport::{
    AwgClientConfig, AwgObfuscationConfig, RealityTlsClientConfig, TlsClientConfig, TlsKexMode,
};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    pub client: ClientListenConfig,
    #[serde(default)]
    pub remote: Option<RemoteConfig>,
    #[serde(default)]
    pub awg: Option<AwgClientFileConfig>,
}

/// Клиентский режим AmneziaWG (`skadicore client` + amneziawg-go).
#[derive(Debug, Clone, Deserialize)]
pub struct AwgClientFileConfig {
    #[serde(default = "default_awg_client_interface")]
    pub interface: String,
    pub private_key: String,
    pub address: String,
    pub server_public_key: String,
    pub endpoint: String,
    pub mtu: Option<u16>,
    pub dns: Option<String>,
    #[serde(default = "default_awg_client_allowed_ips")]
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
    #[serde(default = "default_awg_jc")]
    pub jc: u8,
    #[serde(default = "default_awg_jmin")]
    pub jmin: u16,
    #[serde(default = "default_awg_jmax")]
    pub jmax: u16,
    #[serde(default = "default_awg_s1")]
    pub s1: u8,
    #[serde(default = "default_awg_s2")]
    pub s2: u8,
    #[serde(default = "default_awg_s3")]
    pub s3: u8,
    #[serde(default = "default_awg_s4")]
    pub s4: u8,
    #[serde(default = "default_awg_h1")]
    pub h1: String,
    #[serde(default = "default_awg_h2")]
    pub h2: String,
    #[serde(default = "default_awg_h3")]
    pub h3: String,
    #[serde(default = "default_awg_h4")]
    pub h4: String,
}

fn default_awg_client_interface() -> String {
    "skadiawg0".to_string()
}

fn default_awg_client_allowed_ips() -> Vec<String> {
    vec!["0.0.0.0/0".into(), "::/0".into()]
}

fn default_awg_jc() -> u8 {
    8
}
fn default_awg_jmin() -> u16 {
    64
}
fn default_awg_jmax() -> u16 {
    1024
}
fn default_awg_s1() -> u8 {
    32
}
fn default_awg_s2() -> u8 {
    32
}
fn default_awg_s3() -> u8 {
    16
}
fn default_awg_s4() -> u8 {
    16
}
fn default_awg_h1() -> String {
    "1-10000000".into()
}
fn default_awg_h2() -> String {
    "10000001-20000000".into()
}
fn default_awg_h3() -> String {
    "20000001-30000000".into()
}
fn default_awg_h4() -> String {
    "30000001-40000000".into()
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
    /// `off` | `static` | `probe` — см. `docs/CONFIGURATION.md` (PMTUD).
    #[serde(default = "default_tun_pmtud")]
    pub pmtud: String,
    /// Запас на VLESS+TLS оверхед при `pmtud = "probe"`.
    #[serde(default = "default_tun_mtu_overhead")]
    pub mtu_overhead: u16,
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
            pmtud: default_tun_pmtud(),
            mtu_overhead: default_tun_mtu_overhead(),
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
    /// `udp` — форвард DNS UDP; `doh` — DoH; `dot` — DoT (все через VLESS).
    #[serde(default = "default_dns_mode")]
    pub mode: String,
    #[serde(default = "default_dns_server")]
    pub server: Option<String>,
    /// Блокировать TCP/853 (системный DoT), чтобы приложения использовали UDP/53.
    #[serde(default = "default_block_system_dot")]
    pub block_system_dot: bool,
    /// Блокировать TCP/443 к известным DoH-резолверам (Cloudflare, Google, …).
    #[serde(default = "default_block_system_doh")]
    pub block_system_doh: bool,
}

impl Default for TunDnsConfig {
    fn default() -> Self {
        Self {
            hijack: default_dns_hijack(),
            mode: default_dns_mode(),
            server: default_dns_server(),
            block_system_dot: default_block_system_dot(),
            block_system_doh: default_block_system_doh(),
        }
    }
}

fn default_routing_table() -> u32 {
    100
}

fn default_dns_hijack() -> bool {
    true
}

fn default_dns_mode() -> String {
    "udp".to_string()
}

fn default_dns_server() -> Option<String> {
    Some("8.8.8.8".to_string())
}

fn default_block_system_dot() -> bool {
    true
}

fn default_block_system_doh() -> bool {
    true
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
    crate::warnings::RECOMMENDED_TUN_MTU
}

fn default_tun_pmtud() -> String {
    "static".to_string()
}

fn default_tun_mtu_overhead() -> u16 {
    crate::pmtud::DEFAULT_OVERLAY_OVERHEAD
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
        crate::pmtud::parse_pmtud_mode(&self.pmtud)?;
        if self.mtu_overhead > 500 {
            bail!("client.tun.mtu_overhead must be at most 500");
        }
        if self.routing.table == 0 || self.routing.table > 252 {
            bail!("client.tun.routing.table must be in 1..=252");
        }
        parse_ipv4_list(&self.routing.bypass, "client.tun.routing.bypass")?;
        if self.dns.hijack && self.dns.server.is_none() {
            bail!("client.tun.dns.hijack requires client.tun.dns.server");
        }
        if self.dns.hijack {
            let _ = crate::dns::DnsIntercept::from_config(&self.dns)?;
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
    #[serde(default)]
    pub reality: RemoteRealityConfig,
}

/// REALITY outbound (Xray: `password`, `shortId`, `serverName`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RemoteRealityConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Публичный X25519 ключ сервера (Xray `password`, URL-safe base64).
    pub password: Option<String>,
    /// Short ID, hex (1..8 байт).
    pub short_id: Option<String>,
    /// SNI / `serverName` для ClientHello (маскировка).
    pub server_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RemoteTlsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub ca_file: Option<String>,
    /// SNI для TLS handshake. По умолчанию — hostname из `server`.
    pub server_name: Option<String>,
    /// `classic` (по умолчанию) или `hybrid_pq` (X25519MLKEM768, RFC 10024).
    #[serde(default = "default_remote_tls_kex_mode")]
    pub kex_mode: String,
}

fn default_remote_tls_kex_mode() -> String {
    "classic".to_string()
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
        let has_awg = self.awg.is_some();
        let has_remote = self.remote.is_some();
        if has_awg == has_remote {
            bail!("client config: set exactly one of [remote] (VLESS) or [awg] (AmneziaWG)");
        }
        if self.client.listen.is_none() && !self.client.tun.enabled && !has_awg {
            bail!("client: set client.listen (SOCKS5), client.tun.enabled = true, or [awg] mode");
        }
        if let Some(listen) = &self.client.listen {
            parse_socket_addr(listen, "client.listen")?;
        }
        if let Some(remote) = &self.remote {
            parse_server_endpoint(&remote.server)?;
            Uuid::parse(&remote.uuid).map_err(|e| anyhow::anyhow!("remote.uuid: {}", e))?;
            if remote.tls.enabled && remote.reality.enabled {
                bail!("remote.tls and remote.reality cannot both be enabled");
            }
            if remote.tls.enabled {
                TlsKexMode::parse(&remote.tls.kex_mode)
                    .with_context(|| "invalid remote.tls.kex_mode")?;
            }
            if remote.reality.enabled {
                parse_reality_client(&remote.reality)?;
            }
        }
        if let Some(awg) = &self.awg {
            if awg.private_key.trim().is_empty() {
                bail!("awg.private_key must not be empty");
            }
            if awg.server_public_key.trim().is_empty() {
                bail!("awg.server_public_key must not be empty");
            }
            parse_socket_addr(&awg.endpoint, "awg.endpoint")?;
            if awg.allowed_ips.is_empty() {
                bail!("awg.allowed_ips must not be empty");
            }
        }
        self.client.tun.validate()?;
        Ok(())
    }

    pub fn awg_enabled(&self) -> bool {
        self.awg.is_some()
    }

    pub fn awg_runtime_config(&self) -> Result<AwgClientConfig> {
        let awg = self.awg.as_ref().context("awg section not configured")?;
        Ok(AwgClientConfig {
            interface_name: awg.interface.clone(),
            private_key: awg.private_key.clone(),
            address: awg.address.clone(),
            server_public_key: awg.server_public_key.clone(),
            endpoint: awg.endpoint.clone(),
            mtu: awg.mtu,
            dns: awg.dns.clone(),
            allowed_ips: awg.allowed_ips.clone(),
            persistent_keepalive: awg.persistent_keepalive,
            obfuscation: AwgObfuscationConfig {
                jc: awg.jc,
                jmin: awg.jmin,
                jmax: awg.jmax,
                s1: awg.s1,
                s2: awg.s2,
                s3: awg.s3,
                s4: awg.s4,
                h1: awg.h1.clone(),
                h2: awg.h2.clone(),
                h3: awg.h3.clone(),
                h4: awg.h4.clone(),
            },
        })
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
        let remote = self
            .remote
            .as_ref()
            .context("remote section not configured")?;
        parse_server_endpoint(&remote.server)
    }

    pub fn tls_sni_endpoint(&self) -> Result<Endpoint> {
        let remote = self
            .remote
            .as_ref()
            .context("remote section not configured")?;
        if let Some(name) = &remote.tls.server_name {
            let (_, port) = split_host_port(&remote.server)?;
            return Ok(Endpoint::Domain(name.clone(), port));
        }
        self.proxy_endpoint()
    }

    pub fn uuid_bytes(&self) -> Result<[u8; 16]> {
        let remote = self
            .remote
            .as_ref()
            .context("remote section not configured")?;
        Ok(*Uuid::parse(&remote.uuid)?.as_bytes())
    }

    pub fn reality_enabled(&self) -> bool {
        self.remote.as_ref().is_some_and(|r| r.reality.enabled)
    }

    pub fn tls_client_config(&self) -> Result<TlsClientConfig> {
        let remote = self
            .remote
            .as_ref()
            .context("remote section not configured")?;
        if remote.reality.enabled {
            bail!("remote.tls is disabled when remote.reality is enabled");
        }
        Ok(TlsClientConfig {
            ca_file: remote.tls.ca_file.clone(),
            client_cert: None,
            client_key: None,
            kex_mode: TlsKexMode::parse(&remote.tls.kex_mode)
                .with_context(|| "invalid remote.tls.kex_mode")?,
        })
    }

    pub fn reality_tls_client_config(&self) -> Result<RealityTlsClientConfig> {
        use base64::Engine;
        let remote = self
            .remote
            .as_ref()
            .context("remote section not configured")?;
        let reality = &remote.reality;
        if !reality.enabled {
            bail!("remote.reality is not enabled");
        }
        let password = reality
            .password
            .as_deref()
            .context("remote.reality.password is required")?;
        let short_id = reality
            .short_id
            .as_deref()
            .context("remote.reality.short_id is required")?;
        let server_name = reality
            .server_name
            .as_deref()
            .context("remote.reality.server_name is required")?;
        let pk = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(password)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(password))
            .context("remote.reality.password: invalid base64 public key")?;
        if pk.len() != 32 {
            bail!("remote.reality.password must decode to 32 bytes");
        }
        let sid = hex::decode(short_id).context("remote.reality.short_id: invalid hex")?;
        if sid.is_empty() || sid.len() > 8 {
            bail!("remote.reality.short_id must be 1..8 bytes when decoded");
        }
        let mut server_public_key = [0u8; 32];
        server_public_key.copy_from_slice(&pk);
        Ok(RealityTlsClientConfig {
            server_public_key,
            short_id: sid,
            server_name: server_name.to_string(),
            kex_mode: TlsKexMode::Classic,
        })
    }

    pub fn reality_sni_endpoint(&self) -> Result<Endpoint> {
        let remote = self
            .remote
            .as_ref()
            .context("remote section not configured")?;
        let name = remote
            .reality
            .server_name
            .as_deref()
            .context("remote.reality.server_name is required")?;
        let (_, port) = split_host_port(&remote.server)?;
        Ok(Endpoint::Domain(name.to_string(), port))
    }

    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

fn parse_reality_client(cfg: &RemoteRealityConfig) -> Result<()> {
    use base64::Engine;
    let password = cfg
        .password
        .as_deref()
        .context("remote.reality.password is required")?;
    let short_id = cfg
        .short_id
        .as_deref()
        .context("remote.reality.short_id is required")?;
    let server_name = cfg
        .server_name
        .as_deref()
        .context("remote.reality.server_name is required")?;
    if server_name.is_empty() {
        bail!("remote.reality.server_name must not be empty");
    }
    let pk = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(password)
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(password))
        .with_context(|| "remote.reality.password: invalid base64 public key")?;
    if pk.len() != 32 {
        bail!("remote.reality.password must decode to 32 bytes");
    }
    let sid = hex::decode(short_id).context("remote.reality.short_id: invalid hex")?;
    if sid.is_empty() || sid.len() > 8 {
        bail!("remote.reality.short_id must be 1..8 bytes when decoded");
    }
    Ok(())
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
    fn tun_dot_dns_config_validates() {
        let raw = r#"
[client]
[client.tun]
enabled = true

[client.tun.dns]
hijack = true
mode = "dot"
server = "tls://one.one.one.one"

[remote]
server = "proxy.example.com:443"
uuid = "00000000-0000-0000-0000-000000000001"
"#;
        let config: ClientConfig = toml::from_str(raw).unwrap();
        config.validate().unwrap();
        assert_eq!(config.client.tun.dns.mode, "dot");
        assert!(config.client.tun.dns.block_system_dot);
    }

    #[test]
    fn tun_doh_dns_config_validates() {
        let raw = r#"
[client]
[client.tun]
enabled = true

[client.tun.dns]
hijack = true
mode = "doh"
server = "https://cloudflare-dns.com/dns-query"

[remote]
server = "proxy.example.com:443"
uuid = "00000000-0000-0000-0000-000000000001"
"#;
        let config: ClientConfig = toml::from_str(raw).unwrap();
        config.validate().unwrap();
        assert_eq!(config.client.tun.dns.mode, "doh");
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
