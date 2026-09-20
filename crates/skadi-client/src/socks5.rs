//! Локальный SOCKS5 inbound → VLESS outbound.

use crate::config::ClientConfig;
use crate::outbound::Outbound;
use anyhow::{Context, Result};
use skadi_core::Endpoint;
use skadi_protocol::socks5::{AuthMethod, Socks5Handler, REP_SUCCEEDED};
use skadi_transport::{copy_bidirectional_with_limits, RelayLimits};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tracing::{debug, info, warn};

pub async fn run(
    config: &ClientConfig,
    outbound: Outbound,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let listen = config.listen_addr()?;
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind client SOCKS5 on {}", listen))?;

    let socks5 = config.client.socks5.inbound_config();
    let auth_label = match socks5.auth {
        AuthMethod::NoAuth => "no-auth",
        AuthMethod::UserPass => "user-pass",
    };
    info!(
        listen = %listen,
        auth = auth_label,
        users = socks5.users.len(),
        "client SOCKS5 listening"
    );

    if !listen.ip().is_loopback() && socks5.auth == AuthMethod::NoAuth {
        warn!(
            listen = %listen,
            "client SOCKS5 binds a non-loopback address without auth; \
             set [client.socks5] auth = \"user-pass\" or bind 127.0.0.1 / ::1"
        );
    }

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
        let socks5 = socks5.clone();

        tokio::spawn(async move {
            if let Err(err) = handle_session(&mut local, peer, &outbound, &socks5).await {
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
    socks5: &skadi_protocol::socks5::Socks5Config,
) -> Result<()> {
    let target = match Socks5Handler::negotiate(local, socks5).await {
        Ok(req) => req.target,
        Err(err) => {
            let _ = Socks5Handler::send_error(local, skadi_protocol::REP_GENERAL_FAILURE).await;
            return Err(err).context("SOCKS5 negotiation failed");
        }
    };

    crate::warnings::debug_socks5_target_resolution(&target);
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
