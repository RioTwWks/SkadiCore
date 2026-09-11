use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_protocol::{Socks5Config, VlessConfig};
use skadi_transport::{RealityServerConfig, TlsCertPaths, TlsServerConfig, TlsSniCert};
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
    pub token: Option<String>,
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
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub listen: String,
    #[serde(default)]
    pub timeouts: ServerTimeoutsConfig,
    /// Максимум одновременных inbound-сессий. Не задано — без лимита.
    #[serde(default)]
    pub max_connections: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerTimeoutsConfig {
    /// Таймаут установки исходящего TCP-соединения (секунды).
    #[serde(default = "default_connect_timeout_secs")]
    pub connect_timeout_secs: u64,
    /// Закрыть сессию при отсутствии трафика в обе стороны (секунды). `0` или отсутствие — выключено.
    #[serde(default)]
    pub idle_timeout_secs: Option<u64>,
}

fn default_connect_timeout_secs() -> u64 {
    10
}

impl Default for ServerTimeoutsConfig {
    fn default() -> Self {
        Self {
            connect_timeout_secs: default_connect_timeout_secs(),
            idle_timeout_secs: None,
        }
    }
}

impl ServerConfig {
    pub fn with_listen(listen: impl Into<String>) -> Self {
        Self {
            listen: listen.into(),
            timeouts: ServerTimeoutsConfig::default(),
            max_connections: None,
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
pub struct TransportConfig {
    #[serde(default)]
    pub tls: TlsConfig,
    #[serde(default)]
    pub reality: RealityConfig,
}

#[derive(Debug, Deserialize, Default)]
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
}

#[derive(Debug, Deserialize, Default)]
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
        self.server
            .listen
            .parse::<SocketAddr>()
            .with_context(|| format!("invalid server.listen: {}", self.server.listen))?;

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

        let socks_on = self.protocol.socks5.enabled;
        let vless_on = self.protocol.vless.enabled;

        if !socks_on && !vless_on {
            bail!("at least one protocol must be enabled (socks5 or vless)");
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
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("api.token is required when api.enabled = true"))?;
        if token.trim().is_empty() {
            bail!("api.token must not be empty");
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

    /// Лимит одновременных inbound-сессий. `None` — без ограничения.
    pub fn max_connections(&self) -> Option<u32> {
        self.server.max_connections.filter(|&n| n > 0)
    }

    /// Сколько протоколов включено.
    pub fn enabled_protocol_count(&self) -> usize {
        let mut n = 0;
        if self.protocol.socks5.enabled {
            n += 1;
        }
        if self.protocol.vless.enabled {
            n += 1;
        }
        n
    }

    /// TLS включён на inbound.
    pub fn tls_enabled(&self) -> bool {
        self.transport.tls.enabled
    }

    /// REALITY включён на inbound.
    pub fn reality_enabled(&self) -> bool {
        self.transport.reality.enabled
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

        Ok(RealityServerConfig {
            private_key,
            dest: reality.dest.clone().unwrap(),
            server_names: reality.server_names.clone(),
            short_ids,
            connect_timeout: self.connect_timeout(),
            idle_timeout: self.idle_timeout(),
        })
    }

    /// Краткая сводка для `skadicore check-config`.
    pub fn print_check_summary(&self, path: &Path) {
        println!("Configuration OK: {}", path.display());
        println!("  listen:  {}", self.server.listen);
        println!(
            "  timeouts: connect={}s, idle={}",
            self.server.timeouts.connect_timeout_secs,
            match self.idle_timeout() {
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
            "  api:     {}",
            if self.api.enabled {
                format!("enabled ({})", self.api.listen)
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

fn ensure_readable_file(path: &str, field: &str) -> Result<()> {
    let p = PathBuf::from(path);
    if !p.is_file() {
        bail!("{}: file not found: {}", field, path);
    }
    std::fs::File::open(&p).with_context(|| format!("{}: cannot read {}", field, path))?;
    Ok(())
}
