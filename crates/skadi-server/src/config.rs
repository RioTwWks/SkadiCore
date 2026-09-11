use anyhow::{Context, Result};
use serde::Deserialize;
use skadi_protocol::{Socks5Config, VlessConfig};
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
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {:?}", path))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("invalid TOML in {:?}", path))?;
        Ok(config)
    }
}
