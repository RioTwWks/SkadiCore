use anyhow::{bail, Context, Result};
use serde::de::{self, Deserializer};
use serde::Deserialize;
use skadi_core::SecretString;
use skadi_protocol::{Socks5Config, VlessConfig};
use skadi_transport::{
    AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig, OutboundTcpTransport, PaddingRange,
    RealityServerConfig, TlsCertPaths, TlsClientConfig, TlsKexMode, TlsServerConfig, TlsSniCert,
    XhttpConfig, XhttpMode,
};
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub protocol: ProtocolConfig,
    #[serde(default)]
    pub transport: TransportConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
    #[serde(default)]
    pub outbound: OutboundConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_metrics_listen")]
    pub listen: String,
}

fn default_metrics_listen() -> String {
    "127.0.0.1:9090".to_string()
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: default_metrics_listen(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_api_listen")]
    pub listen: String,
    pub token: Option<SecretString>,
    #[serde(default)]
    pub tls: ApiTlsConfig,
    /// Макс. RPC в секунду (глобально). Не задано или `0` — без лимита.
    #[serde(default)]
    pub rate_limit_per_sec: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ApiTlsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
}

fn default_api_listen() -> String {
    "127.0.0.1:10085".to_string()
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: default_api_listen(),
            token: None,
            tls: ApiTlsConfig::default(),
            rate_limit_per_sec: None,
        }
    }
}

/// Один или несколько адресов для inbound TCP (`IP:PORT` или массив для dual-stack).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenAddrs {
    addrs: Vec<String>,
}

impl ListenAddrs {
    pub fn single(addr: impl Into<String>) -> Self {
        Self {
            addrs: vec![addr.into()],
        }
    }

    pub fn as_strings(&self) -> &[String] {
        &self.addrs
    }

    pub fn socket_addrs(&self) -> Result<Vec<SocketAddr>> {
        self.addrs
            .iter()
            .map(|s| {
                s.parse()
                    .with_context(|| format!("invalid server.listen: {}", s))
            })
            .collect()
    }

    pub fn validate(&self) -> Result<()> {
        if self.addrs.is_empty() {
            bail!("server.listen must specify at least one address");
        }
        let mut seen = HashSet::new();
        for addr_str in &self.addrs {
            let addr: SocketAddr = addr_str
                .parse()
                .with_context(|| format!("invalid server.listen: {}", addr_str))?;
            if !seen.insert(addr) {
                bail!("duplicate server.listen address: {}", addr);
            }
        }
        Ok(())
    }
}

impl Default for ListenAddrs {
    fn default() -> Self {
        Self::single("127.0.0.1:0")
    }
}

impl<'de> Deserialize<'de> for ListenAddrs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Many(Vec<String>),
        }

        match Raw::deserialize(deserializer)? {
            Raw::One(addr) => Ok(Self::single(addr)),
            Raw::Many(addrs) => {
                if addrs.is_empty() {
                    return Err(de::Error::custom("server.listen array must not be empty"));
                }
                Ok(Self { addrs })
            }
        }
    }
}

impl std::fmt::Display for ListenAddrs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.addrs.join(", "))
    }
}

impl From<String> for ListenAddrs {
    fn from(addr: String) -> Self {
        Self::single(addr)
    }
}

impl From<&str> for ListenAddrs {
    fn from(addr: &str) -> Self {
        Self::single(addr)
    }
}

impl From<SocketAddr> for ListenAddrs {
    fn from(addr: SocketAddr) -> Self {
        Self::single(addr.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub listen: ListenAddrs,
    #[serde(default)]
    pub timeouts: ServerTimeoutsConfig,
    /// Максимум одновременных inbound-сессий. Не задано — без лимита.
    #[serde(default)]
    pub max_connections: Option<u32>,
    #[serde(default)]
    pub auth_rate_limit: AuthRateLimitConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthRateLimitConfig {
    #[serde(default = "default_auth_rate_limit_enabled")]
    pub enabled: bool,
    #[serde(default = "default_auth_max_failures")]
    pub max_failures: Option<u32>,
    #[serde(default = "default_auth_window_secs")]
    pub window_secs: u64,
    #[serde(default = "default_auth_ban_base_secs")]
    pub ban_base_secs: u64,
    #[serde(default = "default_auth_ban_max_secs")]
    pub ban_max_secs: u64,
}

fn default_auth_rate_limit_enabled() -> bool {
    true
}

fn default_auth_max_failures() -> Option<u32> {
    Some(10)
}

fn default_auth_window_secs() -> u64 {
    600
}

fn default_auth_ban_base_secs() -> u64 {
    60
}

fn default_auth_ban_max_secs() -> u64 {
    3600
}

impl Default for AuthRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_auth_rate_limit_enabled(),
            max_failures: default_auth_max_failures(),
            window_secs: default_auth_window_secs(),
            ban_base_secs: default_auth_ban_base_secs(),
            ban_max_secs: default_auth_ban_max_secs(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerTimeoutsConfig {
    /// Таймаут установки исходящего TCP-соединения (секунды).
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    /// Закрыть сессию при отсутствии трафика в обе стороны (секунды). `0` или отсутствие — выключено.
    #[serde(default)]
    pub idle_timeout_secs: Option<u64>,
    /// Максимальная длительность relay-сессии (секунды), независимо от активности.
    #[serde(default)]
    pub max_session_lifetime_secs: Option<u64>,
}

fn default_connect_timeout_secs() -> u64 {
    10
}

impl Default for ServerTimeoutsConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: default_connect_timeout_secs(),
            idle_timeout_secs: None,
            max_session_lifetime_secs: None,
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: ListenAddrs::default(),
            timeouts: ServerTimeoutsConfig::default(),
            max_connections: None,
            auth_rate_limit: AuthRateLimitConfig::default(),
        }
    }
}

impl ServerConfig {
    pub fn with_listen(listen: impl Into<ListenAddrs>) -> Self {
        Self {
            listen: listen.into(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct ProtocolConfig {
    #[serde(default)]
    pub socks5: Socks5Config,
    #[serde(default)]
    pub vless: VlessConfig,
}

#[derive(Debug, Deserialize, Default)]
pub struct OutboundConfig {
    #[serde(default)]
    pub tls: OutboundTlsConfig,
    /// Разрешить подключения к loopback/private/link-local адресам.
    #[serde(default)]
    pub allow_private: bool,
}

#[derive(Debug, Deserialize)]
pub struct OutboundTlsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// PEM с доверенными CA. Если не задан — системное хранилище.
    pub ca_file: Option<String>,
    /// Клиентский сертификат (mTLS).
    pub cert: Option<String>,
    pub key: Option<String>,
    /// `classic` или `hybrid_pq` (X25519MLKEM768).
    #[serde(default = "default_tls_kex_mode")]
    pub kex_mode: String,
}

impl Default for OutboundTlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ca_file: None,
            cert: None,
            key: None,
            kex_mode: default_tls_kex_mode(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct TransportConfig {
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub reality: RealityConfig,
    #[serde(default)]
    pub xhttp: XhttpFileConfig,
    #[serde(default)]
    pub awg: AwgFileConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct XhttpFileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_xhttp_path")]
    pub path: String,
    pub host: Option<String>,
    #[serde(default = "default_xhttp_mode")]
    pub mode: String,
    #[serde(default)]
    pub no_sse_header: bool,
    /// `[min, max]` длина X-Padding в ответе (байты).
    pub x_padding_bytes: Option<[u32; 2]>,
}

fn default_xhttp_path() -> String {
    "/xhttp".to_string()
}

fn default_xhttp_mode() -> String {
    "auto".to_string()
}

impl Default for XhttpFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: default_xhttp_path(),
            host: None,
            mode: default_xhttp_mode(),
            no_sse_header: false,
            x_padding_bytes: None,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AwgFileConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_awg_listen")]
    pub listen: String,
    #[serde(default = "default_awg_interface")]
    pub interface: String,
    pub private_key: Option<String>,
    #[serde(default = "default_awg_address")]
    pub address: String,
    pub mtu: Option<u16>,
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
    #[serde(default)]
    pub peers: Vec<AwgPeerFileConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AwgPeerFileConfig {
    pub public_key: String,
    #[serde(default)]
    pub allowed_ips: Vec<String>,
    pub preshared_key: Option<String>,
    pub endpoint: Option<String>,
    pub persistent_keepalive: Option<u16>,
}

fn default_awg_listen() -> String {
    "0.0.0.0:51820".to_string()
}

fn default_awg_interface() -> String {
    "skadiwg0".to_string()
}

fn default_awg_address() -> String {
    "10.8.0.1/24".to_string()
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
    "1-10000000".to_string()
}

fn default_awg_h2() -> String {
    "10000001-20000000".to_string()
}

fn default_awg_h3() -> String {
    "20000001-30000000".to_string()
}

fn default_awg_h4() -> String {
    "30000001-40000000".to_string()
}

impl Default for AwgFileConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            listen: default_awg_listen(),
            interface: default_awg_interface(),
            private_key: None,
            address: default_awg_address(),
            mtu: Some(1420),
            jc: default_awg_jc(),
            jmin: default_awg_jmin(),
            jmax: default_awg_jmax(),
            s1: default_awg_s1(),
            s2: default_awg_s2(),
            s3: default_awg_s3(),
            s4: default_awg_s4(),
            h1: default_awg_h1(),
            h2: default_awg_h2(),
            h3: default_awg_h3(),
            h4: default_awg_h4(),
            peers: Vec::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Сертификат по умолчанию (fallback при неизвестном SNI).
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(default)]
    pub alpn: Vec<String>,
    /// SNI-специфичные сертификаты.
    #[serde(default)]
    pub certificates: Vec<TlsSniCertConfig>,
    /// `classic` (по умолчанию) или `hybrid_pq` (X25519MLKEM768, RFC 10024).
    #[serde(default = "default_tls_kex_mode")]
    pub kex_mode: String,
}

fn default_tls_kex_mode() -> String {
    "classic".to_string()
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cert: None,
            key: None,
            alpn: Vec::new(),
            certificates: Vec::new(),
            kex_mode: default_tls_kex_mode(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RealityConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Fallback destination `host:port` (Xray: `dest`).
    pub dest: Option<String>,
    /// Разрешённые SNI (Xray: `serverNames`).
    #[serde(default)]
    pub server_names: Vec<String>,
    /// X25519 private key, base64 (32 bytes).
    pub private_key: Option<String>,
    /// Short IDs, hex (1..8 bytes each).
    #[serde(default)]
    pub short_ids: Vec<String>,
    /// Путь к PEM/DER leaf-сертификата dest для ImpersonateCert (rkn-fix).
    pub impersonate_cert: Option<String>,
    /// Получить leaf-сертификат с `dest` при старте (если `impersonate_cert` не задан).
    #[serde(default = "default_fetch_impersonate_cert")]
    pub fetch_impersonate_cert: bool,
    /// `classic` (по умолчанию) или `hybrid_pq` (X25519MLKEM768, RFC 10024).
    #[serde(default = "default_tls_kex_mode")]
    pub kex_mode: String,
}

impl Default for RealityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dest: None,
            server_names: Vec::new(),
            private_key: None,
            short_ids: Vec::new(),
            impersonate_cert: None,
            fetch_impersonate_cert: default_fetch_impersonate_cert(),
            kex_mode: default_tls_kex_mode(),
        }
    }
}

fn default_fetch_impersonate_cert() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
pub struct TlsSniCertConfig {
    pub server_names: Vec<String>,
    pub cert: String,
    pub key: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("cannot read {:?}", path))?;
        let config: Config =
            toml::from_str(&raw).with_context(|| format!("invalid TOML in {:?}", path))?;
        config.validate()?;
        Ok(config)
    }

    /// Проверка конфигурации после десериализации.
    pub fn validate(&self) -> Result<()> {
        self.server.listen.validate()?;

        if self.server.timeouts.connect_timeout_secs == 0 {
            bail!("server.timeouts.connect_timeout_secs must be greater than 0");
        }
        if self
            .server
            .timeouts
            .idle_timeout_secs
            .is_some_and(|secs| secs == 0)
        {
            bail!("server.timeouts.idle_timeout_secs must be greater than 0 when set");
        }
        if self.server.max_connections.is_some_and(|n| n == 0) {
            bail!("server.max_connections must be greater than 0 when set");
        }
        if self.server.auth_rate_limit.enabled {
            if self
                .server
                .auth_rate_limit
                .max_failures
                .is_some_and(|n| n == 0)
            {
                bail!("server.auth_rate_limit.max_failures must be greater than 0 when enabled");
            }
            if self.server.auth_rate_limit.window_secs == 0 {
                bail!("server.auth_rate_limit.window_secs must be greater than 0");
            }
            if self.server.auth_rate_limit.ban_base_secs == 0 {
                bail!("server.auth_rate_limit.ban_base_secs must be greater than 0");
            }
            if self.server.auth_rate_limit.ban_max_secs == 0 {
                bail!("server.auth_rate_limit.ban_max_secs must be greater than 0");
            }
        }
        if self
            .server
            .timeouts
            .max_session_lifetime_secs
            .is_some_and(|secs| secs == 0)
        {
            bail!("server.timeouts.max_session_lifetime_secs must be greater than 0 when set");
        }

        let socks_on = self.protocol.socks5.enabled;
        let vless_on = self.protocol.vless.enabled;

        if !socks_on && !vless_on && !self.transport.awg.enabled {
            bail!("at least one of protocol (socks5/vless) or transport.awg must be enabled");
        }

        if socks_on
            && self.protocol.socks5.auth == skadi_protocol::AuthMethod::UserPass
            && self.protocol.socks5.users.is_empty()
        {
            bail!("protocol.socks5.auth is user-pass but no users configured");
        }

        if vless_on {
            if self.protocol.vless.users.is_empty() {
                bail!("protocol.vless is enabled but no users configured");
            }
            for user in &self.protocol.vless.users {
                skadi_protocol::vless::Uuid::parse(&user.id)
                    .with_context(|| format!("invalid VLESS user id: {}", user.id))?;
            }
        }

        if self.transport.tls.enabled && self.transport.reality.enabled {
            bail!("transport.tls and transport.reality cannot both be enabled");
        }

        self.validate_xhttp()?;
        self.validate_awg()?;
        self.validate_outbound_tls()?;

        if self.transport.tls.enabled {
            self.validate_tls()?;
            let _ = self.tls_server_config()?;
        }

        if self.transport.reality.enabled {
            self.validate_reality()?;
            let _ = self.reality_server_config()?;
        }

        if self.api.enabled {
            self.validate_api()?;
        }

        if self.metrics.enabled {
            self.validate_metrics()?;
        }

        Ok(())
    }

    fn validate_metrics(&self) -> Result<()> {
        let addr: SocketAddr = self
            .metrics
            .listen
            .parse()
            .with_context(|| format!("invalid metrics.listen: {}", self.metrics.listen))?;

        if !is_loopback(&addr) {
            bail!(
                "metrics.listen must bind to loopback (127.0.0.1 or ::1), got {}",
                self.metrics.listen
            );
        }

        Ok(())
    }

    fn validate_api(&self) -> Result<()> {
        let addr: SocketAddr = self
            .api
            .listen
            .parse()
            .with_context(|| format!("invalid api.listen: {}", self.api.listen))?;

        if !is_loopback(&addr) {
            bail!(
                "api.listen must bind to loopback (127.0.0.1 or ::1), got {}",
                self.api.listen
            );
        }

        let token = self
            .api
            .token
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("api.token is required when api.enabled = true"))?;
        if token.is_blank() {
            bail!("api.token must not be empty");
        }

        if self.api.tls.enabled {
            let cert =
                self.api.tls.cert.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("api.tls.cert is required when api.tls.enabled")
                })?;
            let key =
                self.api.tls.key.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("api.tls.key is required when api.tls.enabled")
                })?;
            ensure_readable_file(cert, "api.tls.cert")?;
            ensure_readable_file(key, "api.tls.key")?;
        } else if self.api.tls.cert.is_some() || self.api.tls.key.is_some() {
            bail!("api.tls.cert/key set but api.tls.enabled = false");
        }

        if let Some(limit) = self.api.rate_limit_per_sec {
            if limit == 0 {
                bail!("api.rate_limit_per_sec must be > 0 when set");
            }
        }

        Ok(())
    }

    fn validate_tls(&self) -> Result<()> {
        let tls = &self.transport.tls;

        let has_default = tls.cert.is_some() || tls.key.is_some();
        if has_default && (tls.cert.is_none() || tls.key.is_none()) {
            bail!("transport.tls.cert and transport.tls.key must both be set");
        }

        if tls.cert.is_none() && tls.certificates.is_empty() {
            bail!(
                "transport.tls.enabled but no certificates configured: \
                 set cert/key or [[transport.tls.certificates]]"
            );
        }

        if let Some(cert) = &tls.cert {
            ensure_readable_file(cert, "transport.tls.cert")?;
        }
        if let Some(key) = &tls.key {
            ensure_readable_file(key, "transport.tls.key")?;
        }
        parse_tls_kex_mode(&tls.kex_mode, "transport.tls.kex_mode")?;

        let mut seen_names = HashSet::new();
        for (idx, entry) in tls.certificates.iter().enumerate() {
            if entry.server_names.is_empty() {
                bail!(
                    "transport.tls.certificates[{}]: server_names must not be empty",
                    idx
                );
            }
            ensure_readable_file(
                &entry.cert,
                &format!("transport.tls.certificates[{}].cert", idx),
            )?;
            ensure_readable_file(
                &entry.key,
                &format!("transport.tls.certificates[{}].key", idx),
            )?;

            for name in &entry.server_names {
                let normalized = name.trim().to_ascii_lowercase();
                if normalized.is_empty() {
                    bail!("transport.tls.certificates[{}]: empty server name", idx);
                }
                if !seen_names.insert(normalized) {
                    bail!(
                        "transport.tls.certificates: duplicate server name: {}",
                        name
                    );
                }
            }
        }

        Ok(())
    }

    /// Таймаут установки исходящего TCP-соединения.
    pub fn connect_timeout(&self) -> Duration {
        Duration::from_secs(self.server.timeouts.connect_timeout_secs)
    }

    /// Таймаут неактивности relay-сессии. `None` — без ограничения.
    pub fn idle_timeout(&self) -> Option<Duration> {
        self.server
            .timeouts
            .idle_timeout_secs
            .filter(|&secs| secs > 0)
            .map(Duration::from_secs)
    }

    /// Максимальная длительность relay-сессии. `None` — без ограничения.
    pub fn max_session_lifetime(&self) -> Option<Duration> {
        self.server
            .timeouts
            .max_session_lifetime_secs
            .filter(|&secs| secs > 0)
            .map(Duration::from_secs)
    }

    /// Лимит одновременных inbound-сессий. `None` — без ограничения.
    pub fn max_connections(&self) -> Option<u32> {
        self.server.max_connections.filter(|&n| n > 0)
    }

    /// Сколько протоколов включено.
    pub fn enabled_protocol_count(&self) -> usize {
        self.enabled_protocols().count()
    }

    /// Включённые inbound-протоколы (для sniffing / dispatch).
    pub fn enabled_protocols(&self) -> skadi_core::EnabledProtocols {
        skadi_core::EnabledProtocols {
            socks5: self.protocol.socks5.enabled,
            vless: self.protocol.vless.enabled,
        }
    }

    fn validate_outbound_tls(&self) -> Result<()> {
        let tls = &self.outbound.tls;
        if !tls.enabled {
            return Ok(());
        }

        match (&tls.cert, &tls.key) {
            (Some(cert), Some(key)) => {
                ensure_readable_file(cert, "outbound.tls.cert")?;
                ensure_readable_file(key, "outbound.tls.key")?;
            }
            (None, None) => {}
            _ => bail!("outbound.tls.cert and outbound.tls.key must both be set"),
        }

        if let Some(ca) = &tls.ca_file {
            ensure_readable_file(ca, "outbound.tls.ca_file")?;
        }
        parse_tls_kex_mode(&tls.kex_mode, "outbound.tls.kex_mode")?;

        Ok(())
    }

    pub fn allow_private_outbound(&self) -> bool {
        self.outbound.allow_private
    }

    /// Собрать исходящий TCP-транспорт (plain или TLS).
    pub fn outbound_tcp_transport(&self) -> Result<OutboundTcpTransport> {
        let timeout = self.connect_timeout();
        let allow_private = self.allow_private_outbound();
        if self.outbound.tls.enabled {
            OutboundTcpTransport::tls_with_policy(
                timeout,
                &self.outbound_tls_client_config(),
                allow_private,
            )
        } else {
            Ok(OutboundTcpTransport::plain_with_policy(
                timeout,
                allow_private,
            ))
        }
    }

    fn outbound_tls_client_config(&self) -> TlsClientConfig {
        let tls = &self.outbound.tls;
        TlsClientConfig {
            ca_file: tls.ca_file.clone(),
            client_cert: tls.cert.clone(),
            client_key: tls.key.clone(),
            kex_mode: parse_tls_kex_mode(&tls.kex_mode, "outbound.tls.kex_mode")
                .expect("outbound TLS KEX mode validated in validate()"),
        }
    }

    /// TLS включён на inbound.
    pub fn tls_enabled(&self) -> bool {
        self.transport.tls.enabled
    }

    /// TLS включён на outbound.
    pub fn outbound_tls_enabled(&self) -> bool {
        self.outbound.tls.enabled
    }

    /// REALITY включён на inbound.
    pub fn reality_enabled(&self) -> bool {
        self.transport.reality.enabled
    }

    /// XHTTP stream-one включён на inbound (поверх TLS/REALITY/plain TCP).
    pub fn xhttp_enabled(&self) -> bool {
        self.transport.xhttp.enabled
    }

    /// Runtime-конфиг XHTTP inbound.
    pub fn xhttp_config(&self) -> Result<XhttpConfig> {
        let xhttp = &self.transport.xhttp;
        let mode = XhttpMode::parse(&xhttp.mode)
            .ok_or_else(|| anyhow::anyhow!("transport.xhttp.mode invalid: {}", xhttp.mode))?;
        let padding = match xhttp.x_padding_bytes {
            Some([min, max]) if min <= max => PaddingRange::new(min, max),
            Some([min, max]) => bail!(
                "transport.xhttp.x_padding_bytes: min ({}) must be <= max ({})",
                min,
                max
            ),
            None => PaddingRange::default(),
        };
        Ok(XhttpConfig {
            path: xhttp.path.clone(),
            host: xhttp.host.clone(),
            mode,
            padding,
            no_sse_header: xhttp.no_sse_header,
        })
    }

    fn validate_xhttp(&self) -> Result<()> {
        if !self.transport.xhttp.enabled {
            return Ok(());
        }
        if self.transport.xhttp.path.trim().is_empty() {
            bail!("transport.xhttp.path must not be empty when xhttp is enabled");
        }
        if XhttpMode::parse(&self.transport.xhttp.mode).is_none() {
            bail!("transport.xhttp.mode must be auto, packet-up, stream-up, or stream-one");
        }
        if let Some([min, max]) = self.transport.xhttp.x_padding_bytes {
            if min > max {
                bail!("transport.xhttp.x_padding_bytes: min must be <= max");
            }
        }
        Ok(())
    }

    pub fn awg_enabled(&self) -> bool {
        self.transport.awg.enabled
    }

    /// Runtime-конфиг AmneziaWG.
    pub fn awg_server_config(&self) -> Result<AwgServerConfig> {
        let awg = &self.transport.awg;
        let listen: SocketAddr = awg
            .listen
            .parse()
            .with_context(|| format!("invalid transport.awg.listen: {}", awg.listen))?;

        let private_key = awg
            .private_key
            .clone()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("transport.awg.private_key is required when awg is enabled"))?;

        if awg.interface.trim().is_empty() {
            bail!("transport.awg.interface must not be empty");
        }

        if awg.peers.is_empty() {
            bail!("transport.awg.peers must contain at least one peer when awg is enabled");
        }

        let peers = awg
            .peers
            .iter()
            .map(|peer| {
                if peer.public_key.trim().is_empty() {
                    bail!("transport.awg.peers[].public_key must not be empty");
                }
                if peer.allowed_ips.is_empty() {
                    bail!("transport.awg.peers[].allowed_ips must not be empty");
                }
                Ok(AwgPeerConfig {
                    public_key: peer.public_key.clone(),
                    allowed_ips: peer.allowed_ips.clone(),
                    preshared_key: peer.preshared_key.clone(),
                    endpoint: peer.endpoint.clone(),
                    persistent_keepalive: peer.persistent_keepalive,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(AwgServerConfig {
            listen,
            interface_name: awg.interface.clone(),
            private_key,
            address: awg.address.clone(),
            mtu: awg.mtu,
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
            peers,
        })
    }

    fn validate_awg(&self) -> Result<()> {
        if !self.transport.awg.enabled {
            return Ok(());
        }
        let runtime = self.awg_server_config()?;
        skadi_transport::render_server_conf(&runtime)
            .map_err(|e| anyhow::anyhow!("transport.awg: {}", e))?;
        Ok(())
    }

    /// Собрать runtime-конфиг TLS для `TlsTransport`.
    pub fn tls_server_config(&self) -> Result<TlsServerConfig> {
        let tls = &self.transport.tls;

        let default = match (&tls.cert, &tls.key) {
            (Some(cert), Some(key)) => Some(TlsCertPaths {
                cert_path: cert.clone(),
                key_path: key.clone(),
            }),
            (None, None) => None,
            _ => bail!("transport.tls.cert and transport.tls.key must both be set"),
        };

        let sni_certs = tls
            .certificates
            .iter()
            .map(|entry| {
                Ok(TlsSniCert {
                    server_names: entry.server_names.clone(),
                    cert_path: entry.cert.clone(),
                    key_path: entry.key.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        if default.is_none() && sni_certs.is_empty() {
            bail!("no TLS certificates configured");
        }

        Ok(TlsServerConfig {
            default,
            sni_certs,
            alpn: tls.alpn.clone(),
            kex_mode: parse_tls_kex_mode(&tls.kex_mode, "transport.tls.kex_mode")?,
        })
    }

    fn validate_reality(&self) -> Result<()> {
        let reality = &self.transport.reality;

        let dest = reality
            .dest
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("transport.reality.dest is required"))?;
        if dest.trim().is_empty() {
            bail!("transport.reality.dest must not be empty");
        }
        if !dest.contains(':') {
            bail!("transport.reality.dest must be host:port");
        }

        if reality.server_names.is_empty() {
            bail!("transport.reality.server_names must not be empty");
        }

        let key_b64 = reality
            .private_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("transport.reality.private_key is required"))?;
        let key_bytes = decode_base64_32(key_b64, "transport.reality.private_key")?;

        if reality.short_ids.is_empty() {
            bail!("transport.reality.short_ids must not be empty");
        }
        for (idx, sid) in reality.short_ids.iter().enumerate() {
            let bytes = hex::decode(sid)
                .with_context(|| format!("transport.reality.short_ids[{}]: invalid hex", idx))?;
            if bytes.is_empty() || bytes.len() > 8 {
                bail!(
                    "transport.reality.short_ids[{}]: must be 1..8 bytes, got {}",
                    idx,
                    bytes.len()
                );
            }
        }

        // Проверяем, что ключ парсится (значение используется при сборке runtime-конфига).
        let _ = key_bytes;
        parse_tls_kex_mode(&reality.kex_mode, "transport.reality.kex_mode")?;
        Ok(())
    }

    /// Собрать runtime-конфиг REALITY для `RealityTransport`.
    pub fn reality_server_config(&self) -> Result<RealityServerConfig> {
        let reality = &self.transport.reality;
        let private_key = decode_base64_32(
            reality.private_key.as_deref().unwrap(),
            "transport.reality.private_key",
        )?;

        let short_ids = reality
            .short_ids
            .iter()
            .map(|sid| hex::decode(sid).with_context(|| format!("invalid short_id hex: {}", sid)))
            .collect::<Result<Vec<_>>>()?;

        let impersonate_cert = reality
            .impersonate_cert
            .as_deref()
            .map(skadi_transport::load_impersonate_cert_file)
            .transpose()
            .context("transport.reality.impersonate_cert")?;

        Ok(RealityServerConfig {
            private_key,
            dest: reality.dest.clone().unwrap(),
            server_names: reality.server_names.clone(),
            short_ids,
            impersonate_cert,
            connect_timeout: self.connect_timeout(),
            idle_timeout: self.idle_timeout(),
            max_session_lifetime: self.max_session_lifetime(),
            kex_mode: parse_tls_kex_mode(&reality.kex_mode, "transport.reality.kex_mode")?,
        })
    }

    /// Краткая сводка для `skadicore check-config`.
    pub fn print_check_summary(&self, path: &Path) {
        println!("Configuration OK: {}", path.display());
        println!("  listen:  {}", self.server.listen);
        println!(
            "  timeouts: connect={}s, idle={}, max_lifetime={}",
            self.server.timeouts.connect_timeout_secs,
            match self.idle_timeout() {
                Some(d) => format!("{}s", d.as_secs()),
                None => "disabled".to_string(),
            },
            match self.max_session_lifetime() {
                Some(d) => format!("{}s", d.as_secs()),
                None => "disabled".to_string(),
            }
        );
        println!(
            "  max_connections: {}",
            match self.max_connections() {
                Some(n) => n.to_string(),
                None => "unlimited".to_string(),
            }
        );
        println!(
            "  vless:   {} ({} users)",
            on_off(self.protocol.vless.enabled),
            self.protocol.vless.users.len()
        );
        println!(
            "  socks5:  {} ({} users, auth={})",
            on_off(self.protocol.socks5.enabled),
            self.protocol.socks5.users.len(),
            socks_auth_label(self.protocol.socks5.auth)
        );
        println!("  tls:     {}", on_off(self.transport.tls.enabled));
        println!("  reality: {}", on_off(self.transport.reality.enabled));
        println!(
            "  xhttp:   {}",
            if self.transport.xhttp.enabled {
                format!(
                    "enabled (path={}, mode={})",
                    self.transport.xhttp.path, self.transport.xhttp.mode
                )
            } else {
                "disabled".to_string()
            }
        );
        println!(
            "  awg:     {}",
            if self.transport.awg.enabled {
                format!(
                    "enabled (listen={}, iface={}, {} peers)",
                    self.transport.awg.listen,
                    self.transport.awg.interface,
                    self.transport.awg.peers.len()
                )
            } else {
                "disabled".to_string()
            }
        );
        println!("  outbound.tls: {}", on_off(self.outbound.tls.enabled));
        println!(
            "  api:     {}",
            if self.api.enabled {
                let tls = if self.api.tls.enabled { ", tls" } else { "" };
                let rate = match self.api.rate_limit_per_sec {
                    Some(n) => format!(", rate_limit={}/s", n),
                    None => String::new(),
                };
                format!("enabled ({}{}{})", self.api.listen, tls, rate)
            } else {
                "disabled".to_string()
            }
        );
        println!(
            "  metrics: {}",
            if self.metrics.enabled {
                format!("enabled ({})", self.metrics.listen)
            } else {
                "disabled".to_string()
            }
        );
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

fn socks_auth_label(auth: skadi_protocol::AuthMethod) -> &'static str {
    match auth {
        skadi_protocol::AuthMethod::NoAuth => "no-auth",
        skadi_protocol::AuthMethod::UserPass => "user-pass",
    }
}

fn decode_base64_32(value: &str, field: &str) -> Result<[u8; 32]> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .with_context(|| format!("{}: invalid base64", field))?;
    if bytes.len() != 32 {
        bail!("{}: must decode to 32 bytes, got {}", field, bytes.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn is_loopback(addr: &SocketAddr) -> bool {
    match addr.ip() {
        IpAddr::V4(v4) => v4.octets()[0] == 127,
        IpAddr::V6(v6) => v6.is_loopback(),
    }
}

fn parse_tls_kex_mode(value: &str, field: &str) -> Result<TlsKexMode> {
    TlsKexMode::parse(value).with_context(|| format!("invalid {}", field))
}

fn ensure_readable_file(path: &str, field: &str) -> Result<()> {
    let p = PathBuf::from(path);
    if !p.is_file() {
        bail!("{}: file not found: {}", field, path);
    }
    std::fs::File::open(&p).with_context(|| format!("{}: cannot read {}", field, path))?;
    Ok(())
}

#[cfg(test)]
mod reality_kex_tests {
    use super::*;

    const REALITY_STUB: &str = r#"
[server]
listen = "127.0.0.1:443"

[protocol.vless]
enabled = true

[[protocol.vless.users]]
id = "b831381d-6324-4d53-ad4f-8cda48b30811"

[transport.reality]
enabled = true
dest = "www.example.com:443"
server_names = ["www.example.com"]
private_key = "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI="
short_ids = ["0123456789abcdef"]
fetch_impersonate_cert = false
"#;

    #[test]
    fn reality_hybrid_pq_kex_mode() {
        let raw = format!("{REALITY_STUB}kex_mode = \"hybrid_pq\"\n");
        let config: Config = toml::from_str(&raw).unwrap();
        config.validate().unwrap();
        let runtime = config.reality_server_config().unwrap();
        assert_eq!(runtime.kex_mode, TlsKexMode::HybridPq);
    }

    #[test]
    fn reality_defaults_to_classic_kex() {
        let config: Config = toml::from_str(REALITY_STUB).unwrap();
        config.validate().unwrap();
        let runtime = config.reality_server_config().unwrap();
        assert_eq!(runtime.kex_mode, TlsKexMode::Classic);
    }
}

#[cfg(test)]
mod secret_tests {
    use super::{ApiConfig, SecretString};

    #[test]
    fn api_config_debug_masks_token() {
        let api = ApiConfig {
            enabled: true,
            listen: "127.0.0.1:10085".to_string(),
            token: Some(SecretString::new("super-secret-grpc-token")),
            tls: Default::default(),
            rate_limit_per_sec: None,
        };
        let debug = format!("{api:?}");
        assert!(!debug.contains("super-secret-grpc-token"));
        assert!(debug.contains("[REDACTED]"));
    }
}

#[cfg(test)]
mod listen_tests {
    use super::ListenAddrs;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct ListenOnly {
        listen: ListenAddrs,
    }

    #[test]
    fn deserializes_single_address() {
        let cfg: ListenOnly = toml::from_str("listen = \"127.0.0.1:443\"").unwrap();
        assert_eq!(cfg.listen.as_strings(), &["127.0.0.1:443".to_string()]);
        cfg.listen.validate().unwrap();
    }

    #[test]
    fn deserializes_dual_stack_array() {
        let cfg: ListenOnly = toml::from_str("listen = [\"0.0.0.0:443\", \"[::]:443\"]").unwrap();
        assert_eq!(cfg.listen.as_strings().len(), 2);
        cfg.listen.validate().unwrap();
    }

    #[test]
    fn rejects_duplicate_addresses() {
        let cfg: ListenOnly =
            toml::from_str("listen = [\"127.0.0.1:443\", \"127.0.0.1:443\"]").unwrap();
        assert!(cfg.listen.validate().is_err());
    }
}
