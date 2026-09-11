use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_protocol::{Socks5Config, VlessConfig};
use skadi_transport::{TlsCertPaths, TlsServerConfig, TlsSniCert};
use std::collections::HashSet;
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
    /// Сертификат по умолчанию (fallback при неизвестном SNI).
    pub cert: Option<String>,
    pub key: Option<String>,
    #[serde(default)]
    pub alpn: Vec<String>,
    /// SNI-специфичные сертификаты.
    #[serde(default)]
    pub certificates: Vec<TlsSniCertConfig>,
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
            self.validate_tls()?;
            let _ = self.tls_server_config()?;
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
}

fn ensure_readable_file(path: &str, field: &str) -> Result<()> {
    let p = PathBuf::from(path);
    if !p.is_file() {
        bail!("{}: file not found: {}", field, path);
    }
    std::fs::File::open(&p).with_context(|| format!("{}: cannot read {}", field, path))?;
    Ok(())
}
