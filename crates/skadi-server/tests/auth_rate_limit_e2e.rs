//! Per-IP rate limiting на неудачные аутентификации.

mod common;
use skadi_protocol::vless::{build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{
    AuthRateLimitConfig, Config, ProtocolConfig, ServerConfig, TransportConfig,
};
use skadi_server::run_server;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;

const VALID_USER: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

fn vless_request(uuid: &str, target: std::net::SocketAddr) -> Vec<u8> {
    let uuid = *Uuid::parse(uuid).unwrap().as_bytes();
    build_tcp_request(
        &uuid,
        target.ip().to_string().parse().unwrap(),
        target.port(),
    )
}

async fn try_vless_handshake(proxy: std::net::SocketAddr, uuid: &str) -> bool {
    let mut stream = TcpStream::connect(proxy).await.unwrap();
    let target = "1.1.1.1:443".parse().unwrap();
    stream
        .write_all(&vless_request(uuid, target))
        .await
        .unwrap();
    let mut response = [0u8; 2];
    match stream.read(&mut response).await {
        Ok(n) if n >= 1 => response[0] == VLESS_VERSION,
        _ => false,
    }
}

#[tokio::test]
async fn blocks_ip_after_repeated_auth_failures() {
    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig {
            listen: proxy_addr.into(),
            auth_rate_limit: AuthRateLimitConfig {
                enabled: true,
                max_failures: Some(2),
                window_secs: 600,
                ban_base_secs: 60,
                ban_max_secs: 3600,
            },
            ..Default::default()
        },
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: false,
                auth: skadi_protocol::AuthMethod::NoAuth,
                users: vec![],
                bind: false,
                udp_associate: false,
            },
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: VALID_USER.to_string(),
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

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let bad = "00000000-0000-0000-0000-000000000099";
    assert!(!try_vless_handshake(proxy_addr, bad).await);
    assert!(!try_vless_handshake(proxy_addr, bad).await);
    assert!(!try_vless_handshake(proxy_addr, VALID_USER).await);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(3), server).await;
}
