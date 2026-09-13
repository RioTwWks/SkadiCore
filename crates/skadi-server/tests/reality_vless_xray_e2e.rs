//! E2E: VLESS over REALITY с реальным клиентом Xray-core (совместим с v2rayNG/Nekoray).

mod common;

use common::xray::{ensure_xray_binary, socks5_connect, RealityTestKeys, XrayClient};
use skadi_protocol::{VlessConfig, VlessUser};
use skadi_server::config::{Config, ProtocolConfig, RealityConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
const SERVER_NAME: &str = "reality.test";

#[test]
fn reality_password_matches_xray() {
    let xray_bin = match ensure_xray_binary() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping password check: {}", e);
            return;
        }
    };
    let keys = RealityTestKeys::fixed();
    let from_xray =
        RealityTestKeys::password_from_xray(&xray_bin, &keys.private_key).expect("xray x25519");
    assert_eq!(from_xray, keys.password);
}

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

async fn spawn_dest_mock() -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            if let Ok((_, _)) = listener.accept().await {
                // REALITY fallback target — в успешном тесте не используется.
            }
        }
    });
    addr
}

#[tokio::test]
async fn vless_over_reality_with_xray_client() {
    let xray_bin = match ensure_xray_binary() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping xray e2e: {}", e);
            return;
        }
    };

    let keys = RealityTestKeys::fixed();
    let echo_addr = spawn_echo_server().await;
    let dest_addr = spawn_dest_mock().await;

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig::with_listen(proxy_addr.to_string()),
        protocol: ProtocolConfig {
            socks5: Default::default(),
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: TEST_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig {
            tls: Default::default(),
            reality: RealityConfig {
                enabled: true,
                dest: Some(dest_addr.to_string()),
                server_names: vec![SERVER_NAME.to_string()],
                private_key: Some(keys.private_key_b64()),
                short_ids: vec![keys.short_id_hex.clone()],
            },
            ..Default::default()
        },
        api: Default::default(),
        metrics: Default::default(),
        outbound: Default::default(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut xray = XrayClient::start(
        &xray_bin,
        "127.0.0.1",
        proxy_addr.port(),
        TEST_USER_ID,
        &keys,
        SERVER_NAME,
    )
    .expect("failed to start xray client");

    let echo_host = echo_addr.ip().to_string();
    let mut stream = socks5_connect(xray.socks_port, &echo_host, echo_addr.port())
        .await
        .expect("SOCKS5 connect through xray failed");

    let payload = b"hello vless+reality via xray";
    stream.write_all(payload).await.unwrap();

    let mut received = vec![0u8; payload.len()];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);

    xray.stop();
    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
