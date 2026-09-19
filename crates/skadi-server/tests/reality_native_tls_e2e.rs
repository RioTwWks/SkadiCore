//! Native rustls REALITY client TLS handshake (без VLESS/SOCKS5).

mod common;

use common::xray::RealityTestKeys;
use skadi_core::Endpoint;
use skadi_protocol::{Socks5Config, VlessConfig};
use skadi_server::config::{Config, ProtocolConfig, RealityConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use skadi_transport::{RealityTlsClientConfig, RealityTlsOutboundTransport, TlsKexMode};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::watch;

const SERVER_NAME: &str = "reality.test";

#[tokio::test]
async fn native_reality_tls_handshake_only() {
    let keys = RealityTestKeys::fixed();
    let dest_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let dest_addr = dest_listener.local_addr().unwrap();
    tokio::spawn(async move { while dest_listener.accept().await.is_ok() {} });

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig::with_listen(proxy_addr.to_string()),
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
                users: vec![skadi_protocol::VlessUser {
                    id: "00000000-0000-0000-0000-000000000001".to_string(),
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
                impersonate_cert: None,
                fetch_impersonate_cert: false,
                kex_mode: "classic".into(),
            },
            ..Default::default()
        },
        api: Default::default(),
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let mut server_public_key = [0u8; 32];
    use base64::Engine;
    let pk = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(&keys.password)
        .unwrap();
    server_public_key.copy_from_slice(&pk);

    let transport = RealityTlsOutboundTransport::new(
        Duration::from_secs(10),
        &RealityTlsClientConfig {
            server_public_key,
            short_id: hex::decode(&keys.short_id_hex).unwrap(),
            server_name: SERVER_NAME.to_string(),
            kex_mode: TlsKexMode::Classic,
        },
    )
    .unwrap();

    let endpoint = Endpoint::Ip(proxy_addr);
    transport
        .connect(&endpoint)
        .await
        .expect("native REALITY TLS handshake should succeed");

    let _ = shutdown_tx.send(true);
}
