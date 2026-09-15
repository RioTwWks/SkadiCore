//! Локальный SOCKS5 inbound и туннель через VLESS к удалённому прокси.

use crate::config::ClientConfig;
use anyhow::{Context, Result};
use skadi_core::Endpoint;
use skadi_protocol::socks5::{Socks5Config, Socks5Handler, REP_SUCCEEDED};
use skadi_protocol::vless::VlessClient;
use skadi_transport::{copy_bidirectional_with_limits, OutboundTcpTransport, RelayLimits};
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

pub async fn run(config: ClientConfig, mut shutdown: watch::Receiver<bool>) -> Result<()> {
    let listen = config.listen_addr()?;
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind client SOCKS5 on {}", listen))?;

    let transport = if config.remote.tls.enabled {
        OutboundTcpTransport::tls(config.connect_timeout(), &config.tls_client_config())?
    } else {
        OutboundTcpTransport::plain(config.connect_timeout())
    };

    let proxy_tls = config.tls_sni_endpoint()?;
    let proxy_tcp = config.proxy_endpoint()?;
    let uuid = config.uuid_bytes()?;

    info!(
        listen = %listen,
        remote = %config.remote.server,
        tls = config.remote.tls.enabled,
        "SkadiCore client starting"
    );

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
        let transport = transport.clone();
        let proxy_tls = proxy_tls.clone();
        let proxy_tcp = proxy_tcp.clone();

        tokio::spawn(async move {
            if let Err(err) =
                handle_session(&mut local, peer, &transport, &proxy_tls, &proxy_tcp, &uuid).await
            {
                debug!(peer = %peer, error = %err, "client session ended");
            }
        });
    }

    info!("SkadiCore client stopped");
    Ok(())
}

async fn handle_session(
    local: &mut TcpStream,
    peer: SocketAddr,
    transport: &OutboundTcpTransport,
    proxy_tls: &Endpoint,
    proxy_tcp: &Endpoint,
    uuid: &[u8; 16],
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

    let connect_endpoint = match transport {
        OutboundTcpTransport::Tls(_) => proxy_tls,
        OutboundTcpTransport::Plain(_) => proxy_tcp,
    };

    let mut remote = match transport.connect(connect_endpoint).await {
        Ok(stream) => stream,
        Err(err) => {
            warn!(peer = %peer, error = %err, "failed to connect to remote proxy");
            let _ = Socks5Handler::send_error(local, skadi_protocol::REP_CONNECTION_REFUSED).await;
            return Err(err).context("remote proxy connect failed");
        }
    };

    if let Err(err) = VlessClient::handshake_tcp(&mut remote, uuid, &target).await {
        let _ = Socks5Handler::send_error(local, skadi_protocol::REP_GENERAL_FAILURE).await;
        return Err(err).context("VLESS client handshake failed");
    }

    let bound = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    Socks5Handler::send_reply(local, REP_SUCCEEDED, bound)
        .await
        .context("failed to send SOCKS5 success reply")?;

    let _ = copy_bidirectional_with_limits(local, &mut remote, RelayLimits::default())
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
