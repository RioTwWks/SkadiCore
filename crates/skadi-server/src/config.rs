use anyhow::{bail, Context, Result};
use serde::Deserialize;
use skadi_protocol::{Socks5Config, VlessConfig};
use std::net::SocketAddr;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub protocol: ProtocolConfig,
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
}
