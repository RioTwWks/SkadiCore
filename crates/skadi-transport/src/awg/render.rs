//! Рендер WireGuard/AmneziaWG `.conf` через `wireguard-conf`.

use super::config::{AwgObfuscationConfig, AwgPeerConfig, AwgServerConfig};
use anyhow::{bail, Context, Result};
use ipnet::IpNet;
use wireguard_conf::prelude::*;
use wireguard_conf::{AmneziaWG, AmneziaWG2, HRange};

/// Параметры клиентского `.conf` для экспорта.
#[derive(Debug, Clone)]
pub struct AwgClientExport {
    pub private_key: String,
    pub address: String,
    pub dns: Option<String>,
    /// Маршруты через туннель (по умолчанию full-tunnel).
    pub allowed_ips: Vec<String>,
    pub persistent_keepalive: Option<u16>,
}

impl Default for AwgClientExport {
    fn default() -> Self {
        Self {
            private_key: String::new(),
            address: String::new(),
            dns: Some("1.1.1.1".into()),
            allowed_ips: vec!["0.0.0.0/0".into(), "::/0".into()],
            persistent_keepalive: Some(25),
        }
    }
}

/// Собрать клиентский `.conf` для peer (AmneziaVPN / awg-quick).
pub fn render_client_conf(
    server_public_key: &str,
    endpoint: &str,
    client: &AwgClientExport,
    obfuscation: &AwgObfuscationConfig,
    mtu: Option<u16>,
) -> Result<String> {
    let private_key = PrivateKey::try_from(client.private_key.as_str())
        .map_err(|e| anyhow::anyhow!("invalid client private_key: {}", e))?;
    let address = parse_ipnet(&client.address, "client address")?;
    let awg = build_amnezia_settings(obfuscation)?;

    let allowed_ips = client
        .allowed_ips
        .iter()
        .map(|ip| parse_ipnet(ip, "allowed_ips"))
        .collect::<Result<Vec<_>>>()?;

    if endpoint.trim().is_empty() {
        bail!("AWG endpoint must not be empty");
    }

    let server_public_key = PublicKey::try_from(server_public_key)
        .map_err(|e| anyhow::anyhow!("invalid server public_key: {}", e))?;

    let mut peer_builder = Peer::builder();
    let mut peer = peer_builder
        .public_key(server_public_key)
        .endpoint(endpoint.trim())
        .allowed_ips(allowed_ips);

    if let Some(keepalive) = client.persistent_keepalive {
        peer = peer.persistent_keepalive(keepalive);
    }

    let mut iface_builder = Interface::builder();
    let mut builder = iface_builder
        .private_key(private_key)
        .address([address])
        .amnezia_settings(awg)
        .peers([peer.build()]);

    if let Some(mtu) = mtu {
        builder = builder.mtu(mtu as usize);
    }
    if let Some(dns) = &client.dns {
        if !dns.trim().is_empty() {
            builder = builder.dns(vec![dns.trim().to_string()]);
        }
    }

    Ok(builder.build().to_string())
}

/// Собрать `.conf` для `awg setconf` / экспорта клиенту.
pub fn render_server_conf(config: &AwgServerConfig) -> Result<String> {
    let private_key = PrivateKey::try_from(config.private_key.as_str())
        .map_err(|e| anyhow::anyhow!("invalid AWG private_key: {}", e))?;

    let address = parse_ipnet(&config.address, "address")?;

    let awg = build_amnezia_settings(&config.obfuscation)?;

    let mut peer_builders = Vec::new();
    for peer in &config.peers {
        peer_builders.push(build_peer(peer)?);
    }

    let mut iface_builder = Interface::builder();
    let mut builder = iface_builder
        .private_key(private_key)
        .address([address])
        .listen_port(config.listen.port())
        .amnezia_settings(awg);

    if let Some(mtu) = config.mtu {
        builder = builder.mtu(mtu as usize);
    }

    let iface = builder.peers(peer_builders).build();
    Ok(iface.to_string())
}

/// Убрать поля `wg-quick` (`Address`, `MTU`, `DNS`), которые `awg setconf` не принимает.
///
/// Export/`.conf` для AmneziaVPN оставляют полный файл; для `setconf` нужен только UAPI-набор.
pub fn strip_wgquick_fields(conf: &str) -> String {
    conf.lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return true;
            }
            let key = trimmed.split_once('=').map(|(k, _)| k.trim()).unwrap_or("");
            !matches!(
                key.to_ascii_lowercase().as_str(),
                "address"
                    | "mtu"
                    | "dns"
                    | "table"
                    | "preup"
                    | "postup"
                    | "predown"
                    | "postdown"
                    | "saveconfig"
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn build_peer(peer: &AwgPeerConfig) -> Result<Peer> {
    let public_key = PublicKey::try_from(peer.public_key.as_str())
        .map_err(|e| anyhow::anyhow!("invalid AWG peer public_key: {}", e))?;

    let allowed_ips = peer
        .allowed_ips
        .iter()
        .map(|ip| parse_ipnet(ip, "allowed_ips"))
        .collect::<Result<Vec<_>>>()?;

    let mut peer_builder = Peer::builder();
    let mut builder = peer_builder.public_key(public_key).allowed_ips(allowed_ips);

    if let Some(psk) = &peer.preshared_key {
        let key = PresharedKey::try_from(psk.as_str())
            .map_err(|e| anyhow::anyhow!("invalid AWG preshared_key: {}", e))?;
        builder = builder.preshared_key(key);
    }

    if let Some(endpoint) = &peer.endpoint {
        let addr: SocketAddr = endpoint
            .parse()
            .with_context(|| format!("invalid peer endpoint: {}", endpoint))?;
        builder = builder.endpoint(addr.to_string());
    }

    if let Some(keepalive) = peer.persistent_keepalive {
        builder = builder.persistent_keepalive(keepalive);
    }

    Ok(builder.build())
}

fn build_amnezia_settings(obf: &super::config::AwgObfuscationConfig) -> Result<AmneziaWG> {
    let settings = AmneziaWG2::builder()
        .jc(obf.jc)
        .jmin(obf.jmin)
        .jmax(obf.jmax)
        .s1(obf.s1)
        .s2(obf.s2)
        .s3(obf.s3)
        .s4(obf.s4)
        .h1(parse_h_range(&obf.h1, "h1")?)
        .h2(parse_h_range(&obf.h2, "h2")?)
        .h3(parse_h_range(&obf.h3, "h3")?)
        .h4(parse_h_range(&obf.h4, "h4")?)
        .build()
        .map_err(|e| anyhow::anyhow!("AWG obfuscation build failed: {}", e))?;

    settings
        .validate()
        .map_err(|e| anyhow::anyhow!("AWG obfuscation validation failed: {}", e))?;

    Ok(settings)
}

fn parse_ipnet(value: &str, field: &str) -> Result<IpNet> {
    value
        .parse()
        .with_context(|| format!("invalid AWG {}: {}", field, value))
}

fn parse_h_range(value: &str, field: &str) -> Result<HRange> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("AWG {} must not be empty", field);
    }

    if let Some((lo, hi)) = trimmed.split_once('-') {
        let min = lo
            .trim()
            .parse::<u32>()
            .with_context(|| format!("invalid {}", field))?;
        let max = hi
            .trim()
            .parse::<u32>()
            .with_context(|| format!("invalid {}", field))?;
        if min > max {
            bail!("AWG {}: min ({}) must be <= max ({})", field, min, max);
        }
        return Ok(HRange::new(min, max));
    }

    let single = trimmed
        .parse::<u32>()
        .with_context(|| format!("invalid {}", field))?;
    Ok(HRange::new(single, single))
}

use std::net::SocketAddr;

#[cfg(test)]
mod tests {
    use super::strip_wgquick_fields;

    #[test]
    fn strip_removes_address_mtu_dns() {
        let raw = "\
[Interface]
Address = 10.8.0.1/24
ListenPort = 51820
PrivateKey = abc
MTU = 1420
DNS = 1.1.1.1
Jc = 8

[Peer]
AllowedIPs = 10.8.0.2/32
";
        let stripped = strip_wgquick_fields(raw);
        let lower = stripped.to_ascii_lowercase();
        assert!(!lower.contains("address"));
        assert!(!lower.contains("mtu"));
        assert!(!lower.contains("dns ="));
        assert!(stripped.contains("ListenPort"));
        assert!(stripped.contains("PrivateKey"));
        assert!(stripped.contains("Jc = 8"));
    }
}
