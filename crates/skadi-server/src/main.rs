use anyhow::{Context, Result};
use clap::Parser;
use skadi_core::Session;
use skadi_protocol::{
    Socks5Config, Socks5Handler, REP_CONNECTION_REFUSED, REP_GENERAL_FAILURE,
    REP_HOST_UNREACHABLE, REP_SUCCEEDED,
};
use skadi_transport::TcpTransport;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::signal;
use tokio::sync::watch;
use tracing::{error, info, info_span, Instrument};

mod config;

use config::Config;

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
    let listener = TcpListener::bind(&config.server.listen)
        .await
        .with_context(|| format!("failed to bind {}", config.server.listen))?;

    info!(addr = %config.server.listen, "listening");

    let transport = TcpTransport::new(Duration::from_secs(10));
    let socks_config = Arc::new(config.protocol.socks5);

    if !socks_config.enabled {
        anyhow::bail!("no protocol enabled");
    }

    // Канал для оповещения задач о shutdown.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let accept_handle = tokio::spawn({
        let shutdown_rx = shutdown_rx.clone();
        async move {
            accept_loop(listener, transport, socks_config, shutdown_rx).await
        }
    });

    // Ждём Ctrl+C или SIGTERM.
    wait_for_shutdown().await;
    info!("shutdown signal received");

    // Оповещаем все задачи.
    let _ = shutdown_tx.send(true);

    // Даём accept_loop'у завершиться.
    let _ = tokio::time::timeout(Duration::from_secs(5), accept_handle).await;

    info!("SkadiCore stopped");
    Ok(())
}


async fn accept_loop(
    listener: TcpListener,
    transport: TcpTransport,
    socks_config: Arc<Socks5Config>,
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
                        tokio::spawn(async move {
                            let session = Session::new(peer);
                            if let Err(e) = handle_client(
                                client, transport, session, socks_config,
                            ).await {
                                error!(error = %e, "session failed");
                            }
                        }.instrument(info_span!("session", peer = %peer)));
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
        let mut sigterm = signal::unix::signal(
            signal::unix::SignalKind::terminate()
        ).expect("failed to install SIGTERM handler");

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
) -> Result<()> {
    // Фаза 1: переговоры SOCKS5. Получаем целевой endpoint.
    let target = Socks5Handler::negotiate(&mut client, &socks_config).await?;

    // Фаза 2: подключаемся к upstream.
    let mut upstream = match transport.connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            // Сообщаем клиенту причину отказа.
            let code = classify_connect_error(&e);
            let _ = Socks5Handler::send_error(&mut client, code).await;
            return Err(e);
        }
    };

    let bound = upstream.local_addr().unwrap_or_else(|_| {
        "0.0.0.0:0".parse().expect("valid dummy addr")
    });

    // Фаза 3: success-reply с реальным bound-адресом.
    Socks5Handler::send_reply(&mut client, REP_SUCCEEDED, bound).await?;

    info!(
        session = ?session.id,
        target = %target,
        bound = %bound,
        "connected"
    );

    // Фаза 4: релей.
    let (up, down) = copy_bidirectional(&mut client, &mut upstream).await?;
    info!(session = ?session.id, up, down, "closed");

    Ok(())
}

/// Грубая классификация ошибок подключения по тексту.
/// TODO: заменить на типизированные ошибки в TcpTransport.
fn classify_connect_error(e: &anyhow::Error) -> u8 {
    let msg = e.to_string();
    if msg.contains("refused") {
        REP_CONNECTION_REFUSED
    } else if msg.contains("unreachable") {
        REP_HOST_UNREACHABLE
    } else if msg.contains("timeout") {
        REP_HOST_UNREACHABLE
    } else {
        REP_GENERAL_FAILURE
    }
}
