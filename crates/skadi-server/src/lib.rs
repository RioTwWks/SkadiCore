//! SkadiCore server — сборка транспорта и протоколов в работающее ядро.

pub mod api;
pub mod config;
pub mod genkey;
mod prefixed;
pub mod store;

use anyhow::{bail, Context, Result};
use config::Config;
use prefixed::PrefixedStream;
use skadi_core::Session;
use skadi_protocol::{
    Socks5Handler, VlessHandler, REP_CONNECTION_REFUSED, REP_GENERAL_FAILURE, REP_HOST_UNREACHABLE,
    REP_SUCCEEDED,
};
use skadi_transport::{RealityError, RealityTransport, TcpTransport, TlsTransport};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use store::UserStore;
use tokio::io::{copy_bidirectional, AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;
use tracing::{error, info, info_span, Instrument};

/// Первый байт SOCKS5 greeting.
const SOCKS5_VERSION_BYTE: u8 = 0x05;
/// Первый байт VLESS v0.
const VLESS_VERSION_BYTE: u8 = 0x00;

/// Запуск сервера с обработкой Ctrl+C / SIGTERM.
pub async fn run(config: Config) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let accept_handle = tokio::spawn(run_server(config, shutdown_rx));

    wait_for_shutdown().await;
    info!("shutdown signal received");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), accept_handle).await;

    info!("SkadiCore stopped");
    Ok(())
}

/// Запуск accept-loop до получения сигнала shutdown (для тестов).
pub async fn run_server(config: Config, shutdown_rx: watch::Receiver<bool>) -> Result<()> {
    let listen: SocketAddr = config
        .server
        .listen
        .parse()
        .with_context(|| format!("invalid listen address: {}", config.server.listen))?;

    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind {}", config.server.listen))?;

    info!(
        addr = %config.server.listen,
        tls = config.tls_enabled(),
        reality = config.reality_enabled(),
        api = config.api.enabled,
        "listening"
    );

    let outbound = TcpTransport::new(Duration::from_secs(10));
    let inbound = if config.reality_enabled() {
        InboundTransport::Reality(RealityTransport::new(&config.reality_server_config()?)?)
    } else if config.tls_enabled() {
        InboundTransport::Tls(TlsTransport::new(&config.tls_server_config()?)?)
    } else {
        InboundTransport::Plain
    };

    let sniff_protocols = config.enabled_protocol_count() > 1;
    let user_store = Arc::new(UserStore::from_protocol(&config.protocol));

    let api_handle = if config.api.enabled {
        let api_config = config.api.clone();
        let store = Arc::clone(&user_store);
        let api_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            if let Err(e) = api::run_api_server(&api_config, store, api_shutdown).await {
                error!(error = %e, "gRPC API failed");
            }
        }))
    } else {
        None
    };

    accept_loop(
        listener,
        outbound,
        inbound,
        user_store,
        sniff_protocols,
        shutdown_rx,
    )
    .await;

    if let Some(handle) = api_handle {
        let _ = handle.await;
    }

    Ok(())
}

#[derive(Clone)]
enum InboundTransport {
    Plain,
    Tls(TlsTransport),
    Reality(RealityTransport),
}

async fn accept_loop(
    listener: TcpListener,
    outbound: TcpTransport,
    inbound: InboundTransport,
    user_store: Arc<UserStore>,
    sniff_protocols: bool,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            biased;

            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("accept loop stopping");
                    return;
                }
            }

            result = listener.accept() => {
                match result {
                    Ok((client, peer)) => {
                        let outbound = outbound.clone();
                        let inbound = inbound.clone();
                        let user_store = Arc::clone(&user_store);
                        tokio::spawn(async move {
                            let session = Session::new(peer);
                            let result = match inbound {
                                InboundTransport::Plain => handle_connection(
                                    client,
                                    outbound,
                                    session,
                                    user_store,
                                    sniff_protocols,
                                )
                                .await,
                                InboundTransport::Tls(tls) => match tls.accept(client).await {
                                    Ok(tls_stream) => handle_connection(
                                        tls_stream,
                                        outbound,
                                        session,
                                        user_store,
                                        sniff_protocols,
                                    )
                                    .await,
                                    Err(e) => {
                                        error!(error = %e, "TLS handshake failed");
                                        Err(e)
                                    }
                                },
                                InboundTransport::Reality(reality) => {
                                    match reality.accept(client).await {
                                        Ok(tls_stream) => handle_connection(
                                            tls_stream,
                                            outbound,
                                            session,
                                            user_store,
                                            sniff_protocols,
                                        )
                                        .await,
                                        Err(e) => {
                                            if e.downcast_ref::<RealityError>()
                                                == Some(&RealityError::FallbackHandled)
                                            {
                                                Ok(())
                                            } else {
                                                error!(error = %e, "REALITY handshake failed");
                                                Err(e)
                                            }
                                        }
                                    }
                                }
                            };

                            if let Err(e) = result {
                                error!(error = %e, "session failed");
                            }
                        }
                        .instrument(info_span!("session", peer = %peer)));
                    }
                    Err(e) => error!(error = %e, "accept failed"),
                }
            }
        }
    }
}

async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");

        tokio::select! {
            _ = signal::ctrl_c() => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        let _ = signal::ctrl_c().await;
    }
}

async fn handle_connection<S>(
    mut client: S,
    outbound: TcpTransport,
    session: Session,
    user_store: Arc<UserStore>,
    sniff_protocols: bool,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let socks_config = user_store.socks5_config();
    let vless_config = user_store.vless_config();

    let prefix_byte = if sniff_protocols {
        let mut first = [0u8; 1];
        client
            .read_exact(&mut first)
            .await
            .context("failed to read protocol byte")?;
        Some(first[0])
    } else {
        None
    };

    let mut stream = PrefixedStream::new(client, prefix_byte);

    let (target, protocol) = if sniff_protocols {
        let byte = prefix_byte.expect("sniffing requires first byte");
        match byte {
            SOCKS5_VERSION_BYTE if socks_config.enabled => {
                let endpoint = Socks5Handler::negotiate(&mut stream, &socks_config).await?;
                (endpoint, ProtocolKind::Socks5)
            }
            VLESS_VERSION_BYTE if vless_config.enabled => {
                let endpoint = VlessHandler::handshake(&mut stream, &vless_config).await?;
                (endpoint, ProtocolKind::Vless)
            }
            _ => bail!("unknown or disabled protocol byte: 0x{:02x}", byte),
        }
    } else if vless_config.enabled {
        let endpoint = VlessHandler::handshake(&mut stream, &vless_config).await?;
        (endpoint, ProtocolKind::Vless)
    } else if socks_config.enabled {
        let endpoint = Socks5Handler::negotiate(&mut stream, &socks_config).await?;
        (endpoint, ProtocolKind::Socks5)
    } else {
        bail!("no protocol enabled");
    };

    let mut upstream = match outbound.connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            if protocol == ProtocolKind::Socks5 {
                let code = classify_connect_error(&e);
                let _ = Socks5Handler::send_error(&mut stream, code).await;
            }
            return Err(e);
        }
    };

    if protocol == ProtocolKind::Socks5 {
        let bound = upstream
            .local_addr()
            .unwrap_or_else(|_| "0.0.0.0:0".parse().expect("valid dummy addr"));
        Socks5Handler::send_reply(&mut stream, REP_SUCCEEDED, bound).await?;
        info!(
            session = ?session.id,
            target = %target,
            bound = %bound,
            protocol = "socks5",
            "connected"
        );
    } else {
        info!(
            session = ?session.id,
            target = %target,
            protocol = "vless",
            "connected"
        );
    }

    let (up, down) = copy_bidirectional(&mut stream, &mut upstream).await?;
    info!(session = ?session.id, up, down, "closed");

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtocolKind {
    Socks5,
    Vless,
}

fn classify_connect_error(e: &anyhow::Error) -> u8 {
    let msg = e.to_string();
    if msg.contains("refused") {
        REP_CONNECTION_REFUSED
    } else if msg.contains("unreachable") || msg.contains("timeout") {
        REP_HOST_UNREACHABLE
    } else {
        REP_GENERAL_FAILURE
    }
}
