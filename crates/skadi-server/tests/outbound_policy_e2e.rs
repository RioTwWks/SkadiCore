//! Блокировка подключений к loopback/private адресам (SSRF-защита).

use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, OutboundConfig, ProtocolConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;

const TEST_USER: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

fn vless_request_to(target: std::net::SocketAddr) -> Vec<u8> {
    let uuid = *Uuid::parse(TEST_USER).unwrap().as_bytes();
    build_tcp_request(
        &uuid,
        target.ip().to_string().parse().unwrap(),
        target.port(),
    )
}

#[tokio::test]
async fn rejects_loopback_target_by_default() {
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig {
            listen: proxy_addr.to_string(),
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
                    id: TEST_USER.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: Default::default(),
        metrics: Default::default(),
        outbound: OutboundConfig::default(),
    };

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();
    let target = "127.0.0.1:9".parse().unwrap();
    stream.write_all(&vless_request_to(target)).await.unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    let mut buf = [0u8; 16];
    let n = stream.read(&mut buf).await.unwrap_or(0);
    assert_eq!(n, 0, "connection should close without relaying to loopback");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
}
