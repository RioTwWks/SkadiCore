//! Локальный SOCKS5 inbound → VLESS outbound.

use crate::config::ClientConfig;
use crate::outbound::Outbound;
use anyhow::{Context, Result};
use skadi_core::Endpoint;
use skadi_protocol::socks5::{Socks5Config, Socks5Handler, REP_SUCCEEDED};
use skadi_transport::{copy_bidirectional_with_limits, RelayLimits};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tracing::{debug, info, warn};

fn local_socks5_config() -> Socks5Config {
    Socks5Config {
        enabled: true,
        auth: skadi_protocol::AuthMethod::NoAuth,
        users: vec![],
    }
}

pub async fn run(
    config: &ClientConfig,
    outbound: Outbound,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let listen = config.listen_addr()?;
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind client SOCKS5 on {}", listen))?;

    info!(listen = %listen, "client SOCKS5 listening");

    loop {
        if *shutdown.borrow() {
            break;
        }

        let accept = tokio::select! {
            res = listener.accept() => res,
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
                continue;
            }
        };

        let (mut local, peer) = accept.context("client SOCKS5 accept failed")?;
        let outbound = outbound.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_session(&mut local, peer, &outbound).await {
                debug!(peer = %peer, error = %err, "client SOCKS5 session ended");
            }
        });
    }

    Ok(())
}

async fn handle_session(
    local: &mut TcpStream,
    peer: SocketAddr,
    outbound: &Outbound,
) -> Result<()> {
    let socks5 = local_socks5_config();
    let target = match Socks5Handler::negotiate(local, &socks5).await {
        Ok(endpoint) => endpoint,
        Err(err) => {
            let _ = Socks5Handler::send_error(local, skadi_protocol::REP_GENERAL_FAILURE).await;
            return Err(err).context("SOCKS5 negotiation failed");
        }
    };

    debug!(peer = %peer, target = %display_endpoint(&target), "client SOCKS5 request");

    let mut remote = match outbound.open_tcp(&target).await {
        Ok(stream) => stream,
        Err(err) => {
            warn!(peer = %peer, error = %err, "failed to connect to remote proxy");
            let _ = Socks5Handler::send_error(local, skadi_protocol::REP_CONNECTION_REFUSED).await;
            return Err(err);
        }
    };

    let bound = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    Socks5Handler::send_reply(local, REP_SUCCEEDED, bound)
        .await
        .context("failed to send SOCKS5 success reply")?;

    copy_bidirectional_with_limits(local, &mut remote, RelayLimits::default())
        .await
        .context("client relay failed")?;

    Ok(())
}

fn display_endpoint(endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Ip(addr) => addr.to_string(),
        Endpoint::Domain(host, port) => format!("{}:{}", host, port),
    }
}
