use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_protocol::{Socks5Config, VlessConfig};
use skadi_transport::TlsServerConfig;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub protocol: ProtocolConfig,
    #[serde(default)]
    pub transport: TransportConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub listen: String,
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
}

#[derive(Debug, Deserialize, Default)]
pub struct TlsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(default)]
    pub alpn: Vec<String>,
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

        if self.transport.tls.enabled {
            let cert = self
                .transport
                .tls
                .cert
                .as_deref()
                .context("transport.tls.enabled but cert is not set")?;
            let key = self
                .transport
                .tls
                .key
                .as_deref()
                .context("transport.tls.enabled but key is not set")?;

            ensure_readable_file(cert, "transport.tls.cert")?;
            ensure_readable_file(key, "transport.tls.key")?;

            // Проверяем, что PEM парсится, до старта сервера.
            let _ = self.tls_server_config()?;
        }

        Ok(())
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

    /// Собрать runtime-конфиг TLS для `TlsTransport`.
    pub fn tls_server_config(&self) -> Result<TlsServerConfig> {
        let tls = &self.transport.tls;
        Ok(TlsServerConfig {
            cert_path: tls
                .cert
                .clone()
                .context("transport.tls.cert is required when TLS is enabled")?,
            key_path: tls
                .key
                .clone()
                .context("transport.tls.key is required when TLS is enabled")?,
            alpn: tls.alpn.clone(),
        })
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
