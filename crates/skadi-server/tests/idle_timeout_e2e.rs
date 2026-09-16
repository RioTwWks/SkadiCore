//! Интеграционный тест: idle timeout закрывает неактивную сессию.

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
                let mut buf = [0u8; 1024];
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

#[tokio::test]
async fn idle_timeout_closes_inactive_vless_session() {
    let echo_addr = spawn_echo_server().await;
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig {
            listen: proxy_addr.into(),
            timeouts: ServerTimeoutsConfig {
                connect_timeout_secs: 10,
                idle_timeout_secs: Some(1),
                max_session_lifetime_secs: None,
            },
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

    stream.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");

    tokio::time::sleep(Duration::from_millis(1500)).await;

    let read_result = stream.read(&mut buf).await;
    assert!(
        matches!(read_result, Ok(0) | Err(_)),
        "expected idle-closed connection, got {:?}",
        read_result
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn rejects_zero_idle_timeout_in_config() {
    let config = Config {
        server: ServerConfig {
            listen: "127.0.0.1:0".into(),
            timeouts: ServerTimeoutsConfig {
                connect_timeout_secs: 10,
                idle_timeout_secs: Some(0),
                max_session_lifetime_secs: None,
            },
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
    assert!(err.contains("idle_timeout_secs"));
}
