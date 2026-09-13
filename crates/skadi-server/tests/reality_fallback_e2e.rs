//! Интеграционный тест: REALITY fallback на dest при невалидном клиенте.

use base64::Engine;
use skadi_protocol::{VlessConfig, VlessUser};
use skadi_server::config::{Config, ProtocolConfig, RealityConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use skadi_transport::RealityServerConfig;
use skadi_transport::RealityTransport;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

#[tokio::test]
async fn reality_fallback_to_dest() {
    let dest_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dest_addr = dest_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut stream, _) = dest_listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"Hello non-TLS world");
        stream.write_all(b"I am fallback").await.unwrap();
    });

    let private_key = [0x42u8; 32];
    let reality = RealityTransport::new(&RealityServerConfig {
        private_key,
        dest: dest_addr.to_string(),
        server_names: vec!["example.com".into()],
        short_ids: vec![vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]],
        connect_timeout: Duration::from_secs(10),
        idle_timeout: None,
        max_session_lifetime: None,
    })
    .unwrap();

    let server_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_addr = server_listener.local_addr().unwrap();
    let reality = Arc::new(reality);

    tokio::spawn(async move {
        loop {
            if let Ok((stream, _)) = server_listener.accept().await {
                let r = Arc::clone(&reality);
                tokio::spawn(async move {
                    let _ = r.accept(stream).await;
                });
            }
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = TcpStream::connect(server_addr).await.unwrap();
    client.write_all(b"Hello non-TLS world").await.unwrap();

    let mut resp = [0u8; 64];
    let n = client.read(&mut resp).await.unwrap();
    assert_eq!(&resp[..n], b"I am fallback");
}

#[tokio::test]
async fn reality_server_config_via_run_server() {
    let dest_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dest_addr = dest_listener.local_addr().unwrap();

    tokio::spawn(async move {
        if let Ok((mut stream, _)) = dest_listener.accept().await {
            let _ = stream.write_all(b"ok").await;
        }
    });

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
                    id: "b831381d-6324-4d53-ad4f-8cda48b30811".into(),
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
                server_names: vec!["example.com".into()],
                private_key: Some(base64::engine::general_purpose::STANDARD.encode([0x42u8; 32])),
                short_ids: vec!["0102030405060708".into()],
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
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = TcpStream::connect(proxy_addr).await.unwrap();
    client.write_all(b"probe").await.unwrap();
    let mut buf = [0u8; 8];
    let n = client.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"ok");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
