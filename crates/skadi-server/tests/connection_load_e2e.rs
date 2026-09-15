//! Нагрузочные интеграционные тесты: множество одновременных VLESS-сессий.

mod common;
use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{
    Config, ProtocolConfig, ServerConfig, ServerTimeoutsConfig, TransportConfig,
};
use skadi_server::run_server;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

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

async fn spawn_load_proxy(
    echo_addr: std::net::SocketAddr,
    max_connections: u32,
) -> (std::net::SocketAddr, watch::Sender<bool>) {
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig {
            listen: proxy_addr.into(),
            max_connections: Some(max_connections),
            ..Default::default()
        },
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
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    let _ = echo_addr;
    (proxy_addr, shutdown_tx)
}

async fn vless_ping(proxy_addr: std::net::SocketAddr, echo_addr: std::net::SocketAddr) {
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();
    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let request = build_tcp_request(
        &uuid,
        echo_addr.ip().to_string().parse().unwrap(),
        echo_addr.port(),
    );
    stream.write_all(&request).await.unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    stream.write_all(b"load").await.unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"load");
}

#[tokio::test]
async fn concurrent_vless_connections() {
    const CONNECTIONS: u32 = 64;

    let echo_addr = spawn_echo_server().await;
    let (proxy_addr, shutdown_tx) = spawn_load_proxy(echo_addr, CONNECTIONS + 16).await;

    let mut tasks = Vec::with_capacity(CONNECTIONS as usize);
    for _ in 0..CONNECTIONS {
        tasks.push(tokio::spawn(vless_ping(proxy_addr, echo_addr)));
    }

    for task in tasks {
        task.await.unwrap();
    }

    let _ = shutdown_tx.send(true);
}

/// Ручной/ночной прогон: `cargo test -p skadi-server --test connection_load_e2e massive -- --ignored`
#[tokio::test]
#[ignore = "heavy load test; run manually"]
async fn massive_concurrent_vless_connections() {
    const CONNECTIONS: u32 = 512;

    let echo_addr = spawn_echo_server().await;
    let (proxy_addr, shutdown_tx) = spawn_load_proxy(echo_addr, CONNECTIONS + 64).await;

    let mut tasks = Vec::with_capacity(CONNECTIONS as usize);
    for _ in 0..CONNECTIONS {
        tasks.push(tokio::spawn(vless_ping(proxy_addr, echo_addr)));
    }

    for task in tasks {
        task.await.unwrap();
    }

    let _ = shutdown_tx.send(true);
}
