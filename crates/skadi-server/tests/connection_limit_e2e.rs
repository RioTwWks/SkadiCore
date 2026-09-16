//! Интеграционный тест: лимит одновременных соединений (backpressure).

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

/// Upstream, который держит соединение открытым.
async fn spawn_hold_server() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 64];
                while stream.read(&mut buf).await.unwrap_or(0) > 0 {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            });
        }
    });

    addr
}

fn vless_request_to(addr: std::net::SocketAddr) -> Vec<u8> {
    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    build_tcp_request(&uuid, addr.ip().to_string().parse().unwrap(), addr.port())
}

async fn vless_handshake(stream: &mut TcpStream, target: std::net::SocketAddr) {
    stream.write_all(&vless_request_to(target)).await.unwrap();
    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));
}

#[tokio::test]
async fn rejects_connection_when_limit_reached() {
    let hold_addr = spawn_hold_server().await;
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig {
            listen: proxy_addr.into(),
            max_connections: Some(1),
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
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client1 = TcpStream::connect(proxy_addr).await.unwrap();
    vless_handshake(&mut client1, hold_addr).await;

    let mut client2 = TcpStream::connect(proxy_addr).await.unwrap();
    let write_result = client2.write_all(&vless_request_to(hold_addr)).await;
    let read_result = client2.read(&mut [0u8; 1]).await;
    assert!(
        write_result.is_err() || matches!(read_result, Ok(0) | Err(_)),
        "second connection should be rejected, write={:?} read={:?}",
        write_result,
        read_result
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn rejects_zero_max_connections_in_config() {
    let config = Config {
        server: ServerConfig {
            listen: "127.0.0.1:0".into(),
            max_connections: Some(0),
            ..Default::default()
        },
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: true,
                auth: skadi_protocol::AuthMethod::NoAuth,
                users: vec![],
            },
            vless: VlessConfig {
                enabled: false,
                users: vec![],
            },
        },
        transport: TransportConfig::default(),
        api: Default::default(),
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };

    let err = config.validate().unwrap_err().to_string();
    assert!(err.contains("max_connections"));
}
