//! SkadiCore server — сборка транспорта и протоколов в работающее ядро.

pub mod api;
mod auth_rate_limit;
pub mod config;
mod connection_gate;
pub mod genkey;
pub mod observability;
mod prefixed;
pub mod store;

use auth_rate_limit::{is_auth_failure, AuthRateLimiter};
use connection_gate::ConnectionGate;

use anyhow::{Context, Result};
use config::{Config, ListenAddrs};
use prefixed::PrefixedStream;
use skadi_core::{validate_outbound_literal, EnabledProtocols, Protocol, Session};
use skadi_protocol::{
    Socks5Handler, VlessHandler, CMD_MUX, CMD_TCP, CMD_UDP, REP_CONNECTION_REFUSED,
    REP_GENERAL_FAILURE, REP_HOST_UNREACHABLE, REP_NOT_ALLOWED, REP_SUCCEEDED,
};
use skadi_transport::{
    accept_xhttp, copy_bidirectional_with_limits, relay_vless_mux_with_limits,
    relay_vless_udp_with_limits, AwgManager, Hysteria2Manager, OutboundTcpTransport, RealityError,
    RealityTransport, RelayLimits, TlsTransport, TuicManager, UdpTransport, XhttpAcceptResult,
    XhttpConfig, XhttpSessionManager, IDLE_TIMEOUT_MSG, SESSION_LIFETIME_MSG,
};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use store::UserStore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::TcpListener;
use tokio::signal;
use tokio::sync::watch;
use tracing::{debug, error, info, info_span, warn, Instrument};

/// Проверить конфиг без запуска сервера.
pub fn check_config(path: &Path) -> Result<()> {
    let config = Config::load(path)?;
    config.print_check_summary(path);
    Ok(())
}

/// Запуск сервера с обработкой Ctrl+C / SIGTERM и SIGHUP reload.
pub async fn run(config: Config, config_path: PathBuf) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let user_store = Arc::new(UserStore::from_protocol(&config.protocol));

    spawn_sighup_reload(config_path, Arc::clone(&user_store));

    let accept_handle = tokio::spawn(run_server_with_store(config, shutdown_rx, user_store));

    wait_for_shutdown().await;
    info!("shutdown signal received");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), accept_handle).await;

    info!("SkadiCore stopped");
    Ok(())
}

/// Запуск accept-loop до получения сигнала shutdown (для тестов).
pub async fn run_server(config: Config, shutdown_rx: watch::Receiver<bool>) -> Result<()> {
    let user_store = Arc::new(UserStore::from_protocol(&config.protocol));
    run_server_with_store(config, shutdown_rx, user_store).await
}

/// Запуск accept-loop с внешним `UserStore` (для SIGHUP reload).
pub async fn run_server_with_store(
    config: Config,
    shutdown_rx: watch::Receiver<bool>,
    user_store: Arc<UserStore>,
) -> Result<()> {
    warn_missing_ipv6_dual_stack(&config.server.listen);
    let listeners = bind_listeners(&config.server.listen).await?;

    info!(
        addrs = %config.server.listen,
        tls = config.tls_enabled(),
        reality = config.reality_enabled(),
        awg = config.awg_enabled(),
        hysteria2 = config.hysteria2_enabled(),
        tuic = config.tuic_enabled(),
        api = config.api.enabled,
        metrics = config.metrics.enabled,
        idle_timeout_secs = ?config.server.timeouts.idle_timeout_secs,
        max_session_lifetime_secs = ?config.server.timeouts.max_session_lifetime_secs,
        max_connections = ?config.max_connections(),
        outbound_tls = config.outbound_tls_enabled(),
        "listening"
    );

    let outbound_tcp = config.outbound_tcp_transport()?;
    let outbound_udp =
        UdpTransport::with_policy(config.connect_timeout(), config.allow_private_outbound());
    let auth_rate_limiter = AuthRateLimiter::from_config(&config.server.auth_rate_limit);
    let relay_limits = RelayLimits {
        idle: config.idle_timeout(),
        max_lifetime: config.max_session_lifetime(),
    };
    let connection_gate = ConnectionGate::new(config.max_connections());
    let inbound = if config.reality_enabled() {
        let mut reality_cfg = config.reality_server_config()?;
        if reality_cfg.impersonate_cert.is_none() && config.transport.reality.fetch_impersonate_cert
        {
            match skadi_transport::fetch_impersonate_cert_from_dest(&reality_cfg.dest).await {
                Ok(cert) => {
                    info!(
                        dest = %reality_cfg.dest,
                        cert_len = cert.len(),
                        "fetched REALITY impersonate cert from dest"
                    );
                    reality_cfg.impersonate_cert = Some(cert);
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        dest = %reality_cfg.dest,
                        "failed to fetch REALITY impersonate cert; using randomized per-connection certs"
                    );
                }
            }
        }
        InboundTransport::Reality(RealityTransport::new(&reality_cfg)?)
    } else if config.tls_enabled() {
        InboundTransport::Tls(TlsTransport::new(&config.tls_server_config()?)?)
    } else {
        InboundTransport::Plain
    };

    let enabled_protocols = config.enabled_protocols();
    let xhttp_inbound = if config.xhttp_enabled() {
        Some((config.xhttp_config()?, Arc::new(XhttpSessionManager::new())))
    } else {
        None
    };

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

    let metrics_handle = if config.metrics.enabled {
        let metrics_config = config.metrics.clone();
        let prom = observability::install_recorder()?;
        let metrics_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            if let Err(e) =
                observability::run_metrics_server(&metrics_config, prom, metrics_shutdown).await
            {
                error!(error = %e, "metrics HTTP failed");
            }
        }))
    } else {
        None
    };

    let awg_handle = if config.awg_enabled() {
        let awg_config = config.awg_server_config()?;
        let awg_nat = config.awg_nat_config();
        let awg_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            match AwgManager::start(&awg_config, Some(&awg_nat)).await {
                Ok(manager) => {
                    if let Err(e) = manager.run_until_shutdown(awg_shutdown).await {
                        error!(error = %e, "AmneziaWG stopped with error");
                    }
                }
                Err(e) => error!(error = %e, "failed to start AmneziaWG"),
            }
        }))
    } else {
        None
    };

    let hysteria2_handle = if config.hysteria2_enabled() {
        let hy2_config = config.hysteria2_server_config()?;
        let hy2_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            match Hysteria2Manager::start(&hy2_config).await {
                Ok(manager) => {
                    if let Err(e) = manager.run_until_shutdown(hy2_shutdown).await {
                        error!(error = %e, "Hysteria2 stopped with error");
                    }
                }
                Err(e) => error!(error = %e, "failed to start Hysteria2"),
            }
        }))
    } else {
        None
    };

    let tuic_handle = if config.tuic_enabled() {
        let tuic_config = config.tuic_server_config()?;
        let tuic_shutdown = shutdown_rx.clone();
        Some(tokio::spawn(async move {
            match TuicManager::start(&tuic_config).await {
                Ok(manager) => {
                    if let Err(e) = manager.run_until_shutdown(tuic_shutdown).await {
                        error!(error = %e, "TUIC stopped with error");
                    }
                }
                Err(e) => error!(error = %e, "failed to start TUIC"),
            }
        }))
    } else {
        None
    };

    let proxy_enabled = enabled_protocols.socks5 || enabled_protocols.vless;
    let mut accept_handles = Vec::with_capacity(listeners.len());
    if !proxy_enabled {
        info!("proxy protocols disabled; TCP accept loop skipped (UDP transport-only mode)");
    }
    for listener in listeners {
        if !proxy_enabled {
            break;
        }
        let shutdown = shutdown_rx.clone();
        let outbound_tcp = outbound_tcp.clone();
        let outbound_udp = outbound_udp.clone();
        let inbound = inbound.clone();
        let user_store = Arc::clone(&user_store);
        let xhttp_inbound = xhttp_inbound.clone();
        let relay_limits = relay_limits;
        let connection_gate = connection_gate.clone();
        let auth_rate_limiter = auth_rate_limiter.clone();
        let allow_private_outbound = config.allow_private_outbound();
        accept_handles.push(tokio::spawn(accept_loop(
            listener,
            outbound_tcp,
            outbound_udp,
            inbound,
            user_store,
            enabled_protocols,
            xhttp_inbound,
            relay_limits,
            connection_gate,
            auth_rate_limiter,
            allow_private_outbound,
            shutdown,
        )));
    }

    for handle in accept_handles {
        let _ = handle.await;
    }

    if let Some(handle) = api_handle {
        let _ = handle.await;
    }

    if let Some(handle) = metrics_handle {
        let _ = handle.await;
    }

    if let Some(handle) = awg_handle {
        let _ = handle.await;
    }

    if let Some(handle) = hysteria2_handle {
        let _ = handle.await;
    }

    if let Some(handle) = tuic_handle {
        let _ = handle.await;
    }

    Ok(())
}

async fn bind_listeners(addrs: &ListenAddrs) -> Result<Vec<TcpListener>> {
    let mut listeners = Vec::new();
    for addr_str in addrs.as_strings() {
        let addr: SocketAddr = addr_str
            .parse()
            .with_context(|| format!("invalid listen address: {}", addr_str))?;
        let listener = TcpListener::bind(addr)
            .await
            .with_context(|| format!("failed to bind {}", addr_str))?;
        listeners.push(listener);
    }
    Ok(listeners)
}

fn warn_missing_ipv6_dual_stack(addrs: &ListenAddrs) {
    for addr_str in addrs.as_strings() {
        let Ok(addr) = addr_str.parse::<SocketAddr>() else {
            continue;
        };
        if !addr.ip().is_unspecified() || !addr.is_ipv4() {
            continue;
        }
        let has_v6 = addrs.as_strings().iter().any(|other| {
            other
                .parse::<SocketAddr>()
                .map(|s| s.is_ipv6() && s.ip().is_unspecified() && s.port() == addr.port())
                .unwrap_or(false)
        });
        if !has_v6 {
            warn!(
                port = addr.port(),
                "server.listen includes 0.0.0.0 without [::] on the same port — IPv6 clients may be unreachable; add \"[::]:{}\" for dual-stack",
                addr.port()
            );
        }
    }
}

#[derive(Clone)]
enum InboundTransport {
    Plain,
    Tls(TlsTransport),
    Reality(RealityTransport),
}

async fn accept_loop(
    listener: TcpListener,
    outbound_tcp: OutboundTcpTransport,
    outbound_udp: UdpTransport,
    inbound: InboundTransport,
    user_store: Arc<UserStore>,
    enabled_protocols: EnabledProtocols,
    xhttp_inbound: Option<(XhttpConfig, Arc<XhttpSessionManager>)>,
    relay_limits: RelayLimits,
    connection_gate: ConnectionGate,
    auth_rate_limiter: Option<AuthRateLimiter>,
    allow_private_outbound: bool,
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
                        if auth_rate_limiter
                            .as_ref()
                            .is_some_and(|limiter| limiter.is_blocked(peer.ip()))
                        {
                            observability::auth_blocked();
                            debug!(peer = %peer, "connection dropped (auth rate limit)");
                            continue;
                        }

                        let permit = match connection_gate.try_acquire() {
                            Some(permit) => permit,
                            None => {
                                observability::connection_rejected();
                                debug!(peer = %peer, "connection rejected (limit reached)");
                                continue;
                            }
                        };

                        let outbound_tcp = outbound_tcp.clone();
                        let outbound_udp = outbound_udp.clone();
                        let inbound = inbound.clone();
                        let user_store = Arc::clone(&user_store);
                        let xhttp_inbound = xhttp_inbound.clone();
                        let auth_rate_limiter = auth_rate_limiter.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            let session = Session::new(peer);
                            let result = match inbound {
                                InboundTransport::Plain => serve_inbound(
                                    client,
                                    xhttp_inbound.clone(),
                                    outbound_tcp,
                                    outbound_udp,
                                    session,
                                    user_store,
                                    enabled_protocols,
                                    relay_limits,
                                    auth_rate_limiter,
                                    allow_private_outbound,
                                )
                                .await,
                                InboundTransport::Tls(tls) => match tls.accept(client).await {
                                    Ok(tls_stream) => serve_inbound(
                                        tls_stream,
                                        xhttp_inbound.clone(),
                                        outbound_tcp,
                                        outbound_udp,
                                        session,
                                        user_store,
                                        enabled_protocols,
                                        relay_limits,
                                        auth_rate_limiter,
                                        allow_private_outbound,
                                    )
                                    .await,
                                    Err(e) => {
                                        error!(error = %e, "TLS handshake failed");
                                        Err(e)
                                    }
                                },
                                InboundTransport::Reality(reality) => {
                                    match reality.accept(client).await {
                                        Ok(tls_stream) => serve_inbound(
                                            tls_stream,
                                            xhttp_inbound.clone(),
                                            outbound_tcp,
                                            outbound_udp,
                                            session,
                                            user_store,
                                            enabled_protocols,
                                            relay_limits,
                                            auth_rate_limiter,
                                            allow_private_outbound,
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

#[cfg(unix)]
fn spawn_sighup_reload(config_path: PathBuf, user_store: Arc<UserStore>) {
    tokio::spawn(async move {
        let mut sighup =
            signal::unix::signal(signal::unix::SignalKind::hangup()).expect("SIGHUP handler");

        while sighup.recv().await.is_some() {
            match Config::load(&config_path) {
                Ok(config) => {
                    if let Err(e) = user_store.reload_from_protocol(&config.protocol) {
                        error!(error = %e, "SIGHUP reload failed");
                        continue;
                    }
                    info!(
                        path = %config_path.display(),
                        vless_users = config.protocol.vless.users.len(),
                        socks5_users = config.protocol.socks5.users.len(),
                        "config reloaded (protocol users)"
                    );
                }
                Err(e) => error!(
                    error = %e,
                    path = %config_path.display(),
                    "SIGHUP config invalid"
                ),
            }
        }
    });
}

#[cfg(not(unix))]
fn spawn_sighup_reload(_config_path: PathBuf, _user_store: Arc<UserStore>) {}

async fn serve_inbound<S>(
    stream: S,
    xhttp_inbound: Option<(XhttpConfig, Arc<XhttpSessionManager>)>,
    outbound_tcp: OutboundTcpTransport,
    outbound_udp: UdpTransport,
    session: Session,
    user_store: Arc<UserStore>,
    enabled_protocols: EnabledProtocols,
    relay_limits: RelayLimits,
    auth_rate_limiter: Option<AuthRateLimiter>,
    allow_private_outbound: bool,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    if let Some((cfg, sessions)) = xhttp_inbound {
        return match accept_xhttp(stream, cfg, sessions)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
        {
            XhttpAcceptResult::Upgraded(upgraded) => {
                handle_connection(
                    upgraded,
                    outbound_tcp,
                    outbound_udp,
                    session,
                    user_store,
                    enabled_protocols,
                    relay_limits,
                    auth_rate_limiter,
                    allow_private_outbound,
                )
                .await
            }
            XhttpAcceptResult::PostHandled => Ok(()),
        };
    }

    handle_connection(
        stream,
        outbound_tcp,
        outbound_udp,
        session,
        user_store,
        enabled_protocols,
        relay_limits,
        auth_rate_limiter,
        allow_private_outbound,
    )
    .await
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
    outbound_tcp: OutboundTcpTransport,
    outbound_udp: UdpTransport,
    session: Session,
    user_store: Arc<UserStore>,
    enabled_protocols: EnabledProtocols,
    relay_limits: RelayLimits,
    auth_rate_limiter: Option<AuthRateLimiter>,
    allow_private_outbound: bool,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let socks_config = user_store.socks5_config();
    let vless_config = user_store.vless_config();

    let prefix_byte = if enabled_protocols.needs_sniffing() {
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

    let protocol = if enabled_protocols.needs_sniffing() {
        let byte = prefix_byte.expect("sniffing requires first byte");
        enabled_protocols
            .detect(byte)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
    } else {
        enabled_protocols
            .sole()
            .expect("at least one protocol enabled (validated at config load)")
    };

    let (target, vless_command) = match protocol {
        Protocol::Socks5 => match Socks5Handler::negotiate(&mut stream, &socks_config).await {
            Ok(endpoint) => (endpoint, CMD_TCP),
            Err(e) => {
                if is_auth_failure(&e) {
                    record_auth_failure(
                        auth_rate_limiter.as_ref(),
                        session.peer.ip(),
                        Protocol::Socks5.metric_label(),
                    );
                }
                return Err(e);
            }
        },
        Protocol::Vless => match VlessHandler::handshake(&mut stream, &vless_config).await {
            Ok(handshake) => (handshake.target, handshake.command),
            Err(e) => {
                if is_auth_failure(&e) {
                    record_auth_failure(
                        auth_rate_limiter.as_ref(),
                        session.peer.ip(),
                        Protocol::Vless.metric_label(),
                    );
                }
                return Err(e);
            }
        },
    };

    if let Err(err) = validate_outbound_literal(&target, allow_private_outbound) {
        if protocol == Protocol::Socks5 {
            let _ = Socks5Handler::send_error(&mut stream, REP_NOT_ALLOWED).await;
        }
        return Err(anyhow::anyhow!(err.to_string()));
    }

    let protocol_label = vless_metric_label(protocol, vless_command);

    observability::connection_opened(protocol_label);

    if protocol == Protocol::Vless && vless_command == CMD_MUX {
        info!(
            session = ?session.id,
            protocol = "vless-mux",
            "mux session started"
        );

        let relay =
            relay_vless_mux_with_limits(stream, outbound_tcp, outbound_udp, relay_limits).await;
        return finish_relay(session.id, protocol_label, relay);
    }

    if protocol == Protocol::Vless && vless_command == CMD_UDP {
        let mut upstream = match outbound_udp.connect(&target).await {
            Ok(s) => s,
            Err(e) => {
                observability::connection_failed(protocol_label);
                return Err(e);
            }
        };

        info!(
            session = ?session.id,
            target = %target,
            protocol = "vless-udp",
            "connected"
        );

        let relay = relay_vless_udp_with_limits(&mut stream, &mut upstream, relay_limits).await;
        return finish_relay(session.id, protocol_label, relay);
    }

    let mut upstream = match outbound_tcp.connect(&target).await {
        Ok(s) => s,
        Err(e) => {
            if protocol == Protocol::Socks5 {
                let code = classify_connect_error(&e);
                let _ = Socks5Handler::send_error(&mut stream, code).await;
            }
            observability::connection_failed(protocol_label);
            return Err(e.into());
        }
    };

    if protocol == Protocol::Socks5 {
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

    let relay = copy_bidirectional_with_limits(&mut stream, &mut upstream, relay_limits).await;
    finish_relay(session.id, protocol_label, relay)
}

fn finish_relay(
    session_id: skadi_core::SessionId,
    protocol_label: &'static str,
    relay: std::io::Result<(u64, u64)>,
) -> Result<()> {
    match relay {
        Ok((up, down)) => {
            observability::connection_closed(protocol_label, up, down);
            info!(session = ?session_id, up, down, "closed");
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut && e.to_string() == IDLE_TIMEOUT_MSG => {
            observability::connection_closed(protocol_label, 0, 0);
            info!(session = ?session_id, "closed (idle timeout)");
            Ok(())
        }
        Err(e)
            if e.kind() == std::io::ErrorKind::TimedOut
                && e.to_string() == SESSION_LIFETIME_MSG =>
        {
            observability::connection_closed(protocol_label, 0, 0);
            info!(session = ?session_id, "closed (max session lifetime)");
            Ok(())
        }
        Err(e) => {
            observability::connection_failed(protocol_label);
            Err(e.into())
        }
    }
}

fn vless_metric_label(protocol: Protocol, vless_command: u8) -> &'static str {
    if protocol == Protocol::Socks5 {
        return protocol.metric_label();
    }
    match vless_command {
        CMD_UDP => "vless-udp",
        CMD_MUX => "vless-mux",
        _ => protocol.metric_label(),
    }
}

fn record_auth_failure(
    limiter: Option<&AuthRateLimiter>,
    ip: std::net::IpAddr,
    protocol: &'static str,
) {
    if let Some(limiter) = limiter {
        limiter.record_failure(ip);
    }
    observability::auth_failure(protocol);
}

fn classify_connect_error(e: &anyhow::Error) -> u8 {
    let msg = e.to_string();
    if msg.contains("forbidden destination") {
        REP_NOT_ALLOWED
    } else if msg.contains("refused") {
        REP_CONNECTION_REFUSED
    } else if msg.contains("unreachable") || msg.contains("timeout") {
        REP_HOST_UNREACHABLE
    } else {
        REP_GENERAL_FAILURE
    }
}
