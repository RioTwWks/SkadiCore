//! Рендер WireGuard/AmneziaWG `.conf` через `wireguard-conf`.

use super::config::{AwgPeerConfig, AwgServerConfig};
use anyhow::{bail, Context, Result};
use ipnet::IpNet;
use wireguard_conf::prelude::*;
use wireguard_conf::{AmneziaWG, AmneziaWG2, HRange};

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
