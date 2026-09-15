//! Проверка outbound-назначений (SSRF-защита).

use anyhow::{Context, Result};
use skadi_core::{validate_outbound_literal, validate_resolved_addrs, Endpoint};
use std::net::SocketAddr;

pub(crate) async fn resolve_endpoint_addrs(
    endpoint: &Endpoint,
    allow_private: bool,
) -> Result<Vec<SocketAddr>> {
    validate_outbound_literal(endpoint, allow_private)?;

    let addrs = match endpoint {
        Endpoint::Ip(addr) => vec![*addr],
        Endpoint::Domain(host, port) => {
            let mut list = Vec::new();
            let lookup = tokio::net::lookup_host(format!("{}:{}", host, port))
                .await
                .with_context(|| format!("failed to resolve {}:{}", host, port))?;
            for addr in lookup {
                list.push(addr);
            }
            if list.is_empty() {
                anyhow::bail!("no addresses for {}:{}", host, port);
            }
            list
        }
    };

    validate_resolved_addrs(&addrs, allow_private).map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(addrs)
}

pub(crate) async fn resolve_endpoint(
    endpoint: &Endpoint,
    allow_private: bool,
) -> Result<SocketAddr> {
    let addrs = resolve_endpoint_addrs(endpoint, allow_private).await?;
    Ok(prefer_ipv4(addrs))
}

fn prefer_ipv4(addrs: Vec<SocketAddr>) -> SocketAddr {
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .copied()
        .unwrap_or(addrs[0])
}
