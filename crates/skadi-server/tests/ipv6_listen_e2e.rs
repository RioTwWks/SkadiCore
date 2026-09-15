//! E2E: inbound на IPv6 loopback (`[::1]:port`) и dual-stack `listen`.

mod common;

use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, ListenAddrs, ProtocolConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;

const TEST_USER: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

fn vless_config() -> VlessConfig {
    VlessConfig {
        enabled: true,
        users: vec![VlessUser {
            id: TEST_USER.to_string(),
            email: None,
            flow: None,
        }],
    }
}

fn vless_request_to(target: std::net::SocketAddr) -> Vec<u8> {
    let uuid = *Uuid::parse(TEST_USER).unwrap().as_bytes();
    build_tcp_request(
        &uuid,
        target.ip().to_string().parse().unwrap(),
        target.port(),
    )
}

async fn spawn_echo_v4() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
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
async fn accepts_connections_on_ipv6_loopback() {
    let proxy_listener = tokio::net::TcpListener::bind("[::1]:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let echo_addr = spawn_echo_v4().await;

    let config = Config {
        server: ServerConfig::with_listen(proxy_addr.to_string()),
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: false,
                auth: skadi_protocol::AuthMethod::NoAuth,
                users: vec![],
            },
            vless: vless_config(),
        },
        transport: TransportConfig::default(),
        api: Default::default(),
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();
    stream
        .write_all(&vless_request_to(echo_addr))
        .await
        .unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    stream.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
}

#[test]
fn dual_stack_listen_deserializes() {
    #[derive(serde::Deserialize)]
    struct ListenOnly {
        listen: ListenAddrs,
    }
    let cfg: ListenOnly =
        toml::from_str("listen = [\"0.0.0.0:443\", \"[::]:443\"]").unwrap();
    cfg.listen.validate().unwrap();
    assert_eq!(cfg.listen.as_strings().len(), 2);
}
