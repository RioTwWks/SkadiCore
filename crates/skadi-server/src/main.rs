use anyhow::{bail, Context, Result};
use clap::Parser;
use skadi_core::Session;
use skadi_protocol::{
    Socks5Config, Socks5Handler, VlessConfig, VlessHandler, REP_CONNECTION_REFUSED,
    REP_GENERAL_FAILURE, REP_HOST_UNREACHABLE, REP_SUCCEEDED,
};
use skadi_transport::TcpTransport;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{copy_bidirectional, AsyncReadExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal;
use tokio::sync::watch;
use tracing::{error, info, info_span, Instrument};

mod config;
mod prefixed;

use config::Config;
use prefixed::PrefixedTcpStream;

/// Первый байт SOCKS5 greeting.
const SOCKS5_VERSION_BYTE: u8 = 0x05;
/// Первый байт VLESS v0.
const VLESS_VERSION_BYTE: u8 = 0x00;

#[derive(Parser, Debug)]
#[command(name = "skadicore", version, about = "SkadiCore proxy kernel")]
struct Cli {
    #[arg(short, long, default_value = "config/skadi.toml")]
    config: PathBuf,

    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| cli.log_level.clone().into()),
        )
        .json()
        .init();

    let config = Config::load(&cli.config)
        .with_context(|| format!("failed to load config from {:?}", cli.config))?;

    info!(listen = %config.server.listen, "SkadiCore starting");

    run(config).await
}

async fn run(config: Config) -> Result<()> {
    let listen: SocketAddr = config
        .server
        .listen
        .parse()
        .with_context(|| format!("invalid listen address: {}", config.server.listen))?;

    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind {}", config.server.listen))?;

    info!(addr = %config.server.listen, "listening");

    let transport = TcpTransport::new(Duration::from_secs(10));
    let sniff_protocols = config.enabled_protocol_count() > 1;
    let socks_config = Arc::new(config.protocol.socks5);
    let vless_config = Arc::new(config.protocol.vless);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let accept_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            accept_loop(
                listener,
                transport,
                socks_config,
                vless_config,
                sniff_protocols,
                shutdown_rx,
            )
            .await
        }
    });

    wait_for_shutdown().await;
    info!("shutdown signal received");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), accept_handle).await;

    info!("SkadiCore stopped");
    Ok(())
}

async fn accept_loop(
    listener: TcpListener,
    transport: TcpTransport,
    socks_config: Arc<Socks5Config>,
    vless_config: Arc<VlessConfig>,
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
                        let transport = transport.clone();
                        let socks_config = Arc::clone(&socks_config);
                        let vless_config = Arc::clone(&vless_config);
                        tokio::spawn(async move {
                            let session = Session::new(peer);
                            if let Err(e) = handle_client(
                                client,
                                transport,
                                session,
                                socks_config,
                                vless_config,
                                sniff_protocols,
                            )
                            .await
                            {
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

async fn handle_client(
    mut client: TcpStream,
    transport: TcpTransport,
    session: Session,
    socks_config: Arc<Socks5Config>,
    vless_config: Arc<VlessConfig>,
    sniff_protocols: bool,
) -> Result<()> {
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

    let mut stream = PrefixedTcpStream::new(client, prefix_byte);

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

    let mut upstream = match transport.connect(&target).await {
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
