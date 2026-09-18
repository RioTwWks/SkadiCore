//! TUN inbound (Linux): IP-пакеты → VLESS outbound.

mod routing;

use crate::config::TunConfig;
use crate::dns::{is_known_doh_hostname, DnsHandler, DnsIntercept};
use crate::outbound::Outbound;
use crate::pmtud::{
    build_icmp_frag_needed_v4, max_udp_payload_ipv4, parse_icmp_frag_needed_v4, parse_pmtud_mode,
    resolve_effective_mtu, MtuState,
};
use crate::tls_peek::{peek_tls_client_hello, tls_client_hello_sni};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use netstack_smoltcp::{StackBuilder, TcpListener, UdpSocket};
use routing::{resolve_proxy_ipv4, RoutingGuard};
use skadi_core::Endpoint;
use skadi_transport::{
    copy_bidirectional_with_limits, read_vless_udp_frame, write_vless_udp_frame,
    BufferedPrefixStream, RelayLimits,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::sync::watch;
use tracing::{debug, info, warn};

pub async fn run(
    tun: &TunConfig,
    proxy_host: &str,
    outbound: Outbound,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let dns_intercept = DnsIntercept::from_config(&tun.dns)?;
    let proxy_ips = resolve_proxy_ipv4(proxy_host).await?;
    let mut routing_guard = RoutingGuard::apply(tun, &tun.routing, &proxy_ips)?;

    let pmtud_mode = parse_pmtud_mode(&tun.pmtud)?;
    let effective_mtu =
        resolve_effective_mtu(tun.mtu, pmtud_mode, tun.mtu_overhead, proxy_host).await;

    let device = create_device(tun, effective_mtu)?;
    info!(
        name = %tun.name,
        address = %tun.address,
        gateway = %tun.gateway,
        configured_mtu = tun.mtu,
        effective_mtu,
        pmtud = %tun.pmtud,
        routing_auto = tun.routing.auto,
        dns_hijack = dns_intercept.is_active(),
        dns_mode = %tun.dns.mode,
        "client TUN starting"
    );

    let mtu_state = MtuState::shared(effective_mtu);
    let mtu = effective_mtu as usize;
    let (stack, runner, udp_socket, tcp_listener) = StackBuilder::default()
        .enable_tcp(true)
        .enable_udp(true)
        .enable_icmp(true)
        .mtu(mtu)
        .build()
        .context("failed to build userspace netstack")?;

    let udp_socket = udp_socket.context("UDP disabled in netstack")?;
    let tcp_listener = tcp_listener.context("TCP disabled in netstack")?;

    if let Some(runner) = runner {
        tokio::spawn(async move {
            if let Err(err) = runner.await {
                warn!(error = %err, "TUN netstack runner exited");
            }
        });
    }

    let framed = device.into_framed();
    let (mut tun_sink, mut tun_stream) = framed.split();
    let (mut stack_sink, mut stack_stream) = stack.split();
    let (stack_inject_tx, mut stack_inject_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    let stack_to_tun = tokio::spawn({
        let mtu_state = mtu_state.clone();
        async move {
            while let Some(pkt) = stack_stream.next().await {
                if let Ok(pkt) = pkt {
                    if let Some(next_mtu) = parse_icmp_frag_needed_v4(&pkt) {
                        mtu_state.try_lower(next_mtu);
                    }
                    if let Err(err) = tun_sink.send(pkt).await {
                        warn!(error = %err, "failed to write packet to TUN");
                        break;
                    }
                }
            }
        }
    });

    let tun_to_stack = tokio::spawn(async move {
        loop {
            tokio::select! {
                pkt = tun_stream.next() => {
                    match pkt {
                        Some(Ok(pkt)) => {
                            if let Err(err) = stack_sink.send(pkt).await {
                                warn!(error = %err, "failed to write packet to netstack");
                                break;
                            }
                        }
                        Some(Err(err)) => {
                            warn!(error = %err, "failed to read packet from TUN");
                            break;
                        }
                        None => break,
                    }
                }
                injected = stack_inject_rx.recv() => {
                    match injected {
                        Some(pkt) => {
                            if let Err(err) = stack_sink.send(pkt).await {
                                warn!(error = %err, "failed to inject packet into netstack");
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    let tcp_task = tokio::spawn({
        let outbound = outbound.clone();
        let dns_intercept = dns_intercept.clone();
        async move {
            handle_tcp_inbound(tcp_listener, outbound, dns_intercept).await;
        }
    });

    let udp_task = tokio::spawn({
        let outbound = outbound.clone();
        let dns_intercept = dns_intercept.clone();
        let mtu_state = mtu_state.clone();
        let stack_inject_tx = stack_inject_tx.clone();
        async move {
            handle_udp_inbound(
                udp_socket,
                outbound,
                dns_intercept,
                mtu_state,
                stack_inject_tx,
            )
            .await;
        }
    });

    loop {
        if *shutdown.borrow() {
            break;
        }
        if shutdown.changed().await.is_err() {
            break;
        }
    }

    stack_to_tun.abort();
    tun_to_stack.abort();
    tcp_task.abort();
    udp_task.abort();
    if let Some(guard) = routing_guard.as_mut() {
        guard.revert();
    }

    info!("client TUN stopped");
    Ok(())
}

fn create_device(tun: &TunConfig, mtu: u16) -> Result<tun::AsyncDevice> {
    let address: Ipv4Addr = tun.address.parse().context("invalid client.tun.address")?;
    let gateway: Ipv4Addr = tun.gateway.parse().context("invalid client.tun.gateway")?;
    let netmask: Ipv4Addr = tun.netmask.parse().context("invalid client.tun.netmask")?;

    let mut config = tun::Configuration::default();
    config
        .tun_name(&tun.name)
        .address(address)
        .destination(gateway)
        .netmask(netmask)
        .mtu(mtu)
        .up();

    tun::create_as_async(&config)
        .with_context(|| format!("failed to create TUN device {}", tun.name))
}

async fn handle_tcp_inbound(
    mut tcp_listener: TcpListener,
    outbound: Outbound,
    dns_intercept: DnsIntercept,
) {
    while let Some((mut stream, local, remote)) = tcp_listener.next().await {
        let outbound = outbound.clone();
        let dns_intercept = dns_intercept.clone();
        tokio::spawn(async move {
            if dns_intercept.should_block_tcp(remote) {
                debug!(%local, %remote, "blocked system DoT/DoH TCP connection");
                return;
            }

            let prefix = if dns_intercept.needs_doh_sni_inspection(remote) {
                match peek_tls_client_hello(&mut stream).await {
                    Ok(buf) => {
                        if let Some(sni) = tls_client_hello_sni(&buf) {
                            if is_known_doh_hostname(&sni) {
                                debug!(%local, %remote, sni, "blocked system DoH by SNI");
                                return;
                            }
                        }
                        buf
                    }
                    Err(err) => {
                        debug!(%local, %remote, error = %err, "TLS ClientHello peek failed");
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };

            debug!(%local, %remote, "TUN TCP connection");
            let mut relay_stream = BufferedPrefixStream::new(prefix, stream);
            match outbound.open_tcp(&socket_to_endpoint(remote)).await {
                Ok(mut remote_stream) => {
                    if let Err(err) = copy_bidirectional_with_limits(
                        &mut relay_stream,
                        &mut remote_stream,
                        RelayLimits::default(),
                    )
                    .await
                    {
                        debug!(%local, %remote, error = %err, "TUN TCP relay ended");
                    }
                }
                Err(err) => {
                    warn!(%local, %remote, error = %err, "TUN TCP connect failed");
                }
            }
        });
    }
}

async fn handle_udp_inbound(
    udp_socket: UdpSocket,
    outbound: Outbound,
    dns_intercept: DnsIntercept,
    mtu_state: std::sync::Arc<MtuState>,
    stack_inject_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
) {
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    let (reply_tx, mut reply_rx) = tokio::sync::mpsc::unbounded_channel();
    let (mut read_half, mut write_half) = udp_socket.split();
    let sessions: Arc<
        Mutex<HashMap<(SocketAddr, SocketAddr), tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
    > = Arc::new(Mutex::new(HashMap::new()));

    let reply_task = tokio::spawn(async move {
        while let Some((data, local, remote)) = reply_rx.recv().await {
            let _ = write_half.send((data, remote, local)).await;
        }
    });

    while let Some((data, local, remote)) = read_half.next().await {
        if remote.port() == 53 {
            match dns_intercept.handler() {
                Some(DnsHandler::Doh(config)) => {
                    let outbound = outbound.clone();
                    let config = config.clone();
                    let reply_tx = reply_tx.clone();
                    let query = data.clone();
                    tokio::spawn(async move {
                        match config.query(&outbound, &query).await {
                            Ok(response) => {
                                let _ = reply_tx.send((response, local, remote));
                            }
                            Err(err) => {
                                debug!(%local, %remote, error = %err, "DoH query failed");
                            }
                        }
                    });
                    continue;
                }
                Some(DnsHandler::Dot(config)) => {
                    let outbound = outbound.clone();
                    let config = config.clone();
                    let reply_tx = reply_tx.clone();
                    let query = data.clone();
                    tokio::spawn(async move {
                        match config.query(&outbound, &query).await {
                            Ok(response) => {
                                let _ = reply_tx.send((response, local, remote));
                            }
                            Err(err) => {
                                debug!(%local, %remote, error = %err, "DoT query failed");
                            }
                        }
                    });
                    continue;
                }
                _ => {}
            }
        }

        let tunnel_remote = match dns_intercept.handler() {
            Some(handler) => handler.rewrite_udp_upstream(remote),
            None => remote,
        };
        let key = (local, tunnel_remote);
        let sender = {
            let mut map = sessions.lock().await;
            if let Some(tx) = map.get(&key) {
                tx.clone()
            } else {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                map.insert(key, tx.clone());
                let outbound = outbound.clone();
                let reply_tx = reply_tx.clone();
                let mtu_state = mtu_state.clone();
                let stack_inject_tx = stack_inject_tx.clone();
                tokio::spawn(async move {
                    if let Err(err) = relay_udp_flow(
                        outbound,
                        tunnel_remote,
                        local,
                        rx,
                        reply_tx,
                        mtu_state,
                        stack_inject_tx,
                    )
                    .await
                    {
                        debug!(%local, %tunnel_remote, error = %err, "TUN UDP flow ended");
                    }
                });
                tx
            }
        };

        if sender.send(data).is_err() {
            sessions.lock().await.remove(&key);
        }
    }

    reply_task.abort();
}

async fn relay_udp_flow(
    outbound: Outbound,
    remote: SocketAddr,
    local: SocketAddr,
    mut inbound: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    reply_tx: tokio::sync::mpsc::UnboundedSender<(Vec<u8>, SocketAddr, SocketAddr)>,
    mtu_state: std::sync::Arc<MtuState>,
    stack_inject_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
) -> Result<()> {
    let mut stream = outbound
        .open_udp(&socket_to_endpoint(remote))
        .await
        .context("VLESS UDP tunnel open failed")?;

    loop {
        tokio::select! {
            payload = inbound.recv() => {
                match payload {
                    Some(data) => {
                        let max_payload = max_udp_payload_ipv4(mtu_state.get());
                        if data.len() > max_payload {
                            if let (IpAddr::V4(src), IpAddr::V4(dst)) = (local.ip(), remote.ip()) {
                                let icmp = build_icmp_frag_needed_v4(
                                    src,
                                    dst,
                                    local.port(),
                                    remote.port(),
                                    mtu_state.get(),
                                );
                                if stack_inject_tx.send(icmp).is_err() {
                                    break;
                                }
                                debug!(
                                    %local,
                                    %remote,
                                    payload_len = data.len(),
                                    max_payload,
                                    mtu = mtu_state.get(),
                                    "dropped oversized UDP datagram; sent ICMP Fragmentation Needed"
                                );
                            } else {
                                debug!(
                                    %local,
                                    %remote,
                                    payload_len = data.len(),
                                    max_payload,
                                    "dropped oversized UDP datagram (IPv6 ICMP PTB not implemented)"
                                );
                            }
                            continue;
                        }
                        write_vless_udp_frame(&mut stream, &data).await?
                    }
                    None => break,
                }
            }
            payload = read_vless_udp_frame(&mut stream) => {
                match payload? {
                    Some(data) => {
                        if reply_tx.send((data, local, remote)).is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }

    Ok(())
}

fn socket_to_endpoint(addr: SocketAddr) -> Endpoint {
    match addr.ip() {
        IpAddr::V4(_) => Endpoint::Ip(addr),
        IpAddr::V6(_) => Endpoint::Ip(addr),
    }
}
