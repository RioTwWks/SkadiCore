//! Soak-тесты: многократные connect/relay/close без роста RSS и зависших сессий.

use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, MetricsConfig, ProtocolConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
/// Допустимый рост VmRSS (KiB) после серии итераций (с запасом на allocator noise).
const MAX_RSS_GROWTH_KB: u64 = 12_288;

async fn spawn_echo_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 256];
                loop {
                    match socket.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if socket.write_all(&buf[..n]).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });

    addr
}

fn pick_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn spawn_soak_proxy(proxy_port: u16, metrics_port: u16) -> watch::Sender<bool> {
    let config = Config {
        server: ServerConfig::with_listen(format!("127.0.0.1:{}", proxy_port)),
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: false,
                auth: skadi_protocol::AuthMethod::NoAuth,
                users: vec![],
            },
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: TEST_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: Default::default(),
        metrics: MetricsConfig {
            enabled: true,
            listen: format!("127.0.0.1:{}", metrics_port),
        },
        outbound: Default::default(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;
    shutdown_tx
}

async fn vless_ping(proxy_addr: std::net::SocketAddr, echo_addr: std::net::SocketAddr) {
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();
    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let ip = match echo_addr.ip() {
        std::net::IpAddr::V4(v4) => v4,
        _ => panic!("echo must be ipv4"),
    };
    stream
        .write_all(&build_tcp_request(&uuid, ip, echo_addr.port()))
        .await
        .unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    stream.write_all(b"soak").await.unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"soak");
    stream.shutdown().await.unwrap();
}

fn current_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            return line.split_whitespace().nth(1)?.parse().ok();
        }
    }
    None
}

async fn http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("connect metrics");
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        path
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).await.unwrap();
    body
}

fn parse_active_connections(metrics: &str) -> Option<f64> {
    for line in metrics.lines() {
        if line.starts_with("skadicore_active_connections ") {
            return line.split_whitespace().nth(1)?.parse().ok();
        }
    }
    None
}

async fn run_soak_iterations(
    proxy_addr: std::net::SocketAddr,
    echo_addr: std::net::SocketAddr,
    metrics_port: u16,
    warmup: u32,
    iterations: u32,
) {
    for _ in 0..warmup {
        vless_ping(proxy_addr, echo_addr).await;
    }

    let baseline_rss = current_rss_kb();

    for _ in 0..iterations {
        vless_ping(proxy_addr, echo_addr).await;
    }

    tokio::time::sleep(Duration::from_millis(150)).await;

    let metrics = http_get(metrics_port, "/metrics").await;
    let active = parse_active_connections(&metrics).expect("active_connections gauge");
    assert_eq!(
        active, 0.0,
        "leaked active connections after soak: {}",
        active
    );

    if let (Some(before), Some(after)) = (baseline_rss, current_rss_kb()) {
        let growth = after.saturating_sub(before);
        assert!(
            growth <= MAX_RSS_GROWTH_KB,
            "RSS grew too much: {} KiB -> {} KiB (delta {} KiB, max {} KiB)",
            before,
            after,
            growth,
            MAX_RSS_GROWTH_KB
        );
    }
}

#[tokio::test]
async fn sequential_connections_no_leak() {
    let echo_addr = spawn_echo_server().await;
    let proxy_port = pick_free_port();
    let metrics_port = pick_free_port();
    let shutdown_tx = spawn_soak_proxy(proxy_port, metrics_port).await;
    let proxy_addr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();

    run_soak_iterations(proxy_addr, echo_addr, metrics_port, 10, 120).await;

    let _ = shutdown_tx.send(true);
}

/// Ручной/ночной прогон: `cargo test -p skadi-server --test connection_soak_e2e long -- --ignored`
#[tokio::test]
#[ignore = "long soak test; run manually or in nightly CI"]
async fn long_sequential_connections_no_leak() {
    let echo_addr = spawn_echo_server().await;
    let proxy_port = pick_free_port();
    let metrics_port = pick_free_port();
    let shutdown_tx = spawn_soak_proxy(proxy_port, metrics_port).await;
    let proxy_addr = format!("127.0.0.1:{}", proxy_port).parse().unwrap();

    run_soak_iterations(proxy_addr, echo_addr, metrics_port, 50, 2000).await;

    let _ = shutdown_tx.send(true);
}
