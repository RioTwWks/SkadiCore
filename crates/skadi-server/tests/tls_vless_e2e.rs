//! Интеграционный тест: VLESS TCP CONNECT поверх TLS.

use rcgen::generate_simple_self_signed;
use rustls::RootCertStore;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName};
use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, ProtocolConfig, ServerConfig, TlsConfig, TransportConfig};
use skadi_server::run_server;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsConnector;

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

fn test_vless_config() -> VlessConfig {
    VlessConfig {
        enabled: true,
        users: vec![VlessUser {
            id: TEST_USER_ID.to_string(),
            email: Some("alice@example.com".into()),
            flow: None,
        }],
    }
}

async fn spawn_vless_tls_server(
    proxy_addr: std::net::SocketAddr,
    cert_path: &std::path::Path,
    key_path: &std::path::Path,
) -> watch::Sender<bool> {
    let config = Config {
        server: ServerConfig::with_listen(proxy_addr.to_string()),
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: false,
                auth: skadi_protocol::AuthMethod::NoAuth,
                users: vec![],
            },
            vless: test_vless_config(),
        },
        transport: TransportConfig {
            reality: Default::default(),
            tls: TlsConfig {
                enabled: true,
                cert: Some(cert_path.to_string_lossy().into_owned()),
                key: Some(key_path.to_string_lossy().into_owned()),
                alpn: vec![],
                certificates: vec![],
            },
        },
        api: Default::default(),
        metrics: Default::default(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_tx
}

async fn tls_connect(
    proxy_addr: std::net::SocketAddr,
    cert_pem: &str,
) -> tokio_rustls::client::TlsStream<TcpStream> {
    let mut root_store = RootCertStore::empty();
    let cert_der = CertificateDer::from_pem_slice(cert_pem.as_bytes()).unwrap();
    root_store.add(cert_der).unwrap();

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));

    let tcp = TcpStream::connect(proxy_addr).await.unwrap();
    let name = ServerName::try_from("localhost".to_owned()).unwrap();
    connector.connect(name, tcp).await.unwrap()
}

#[tokio::test]
async fn vless_over_tls_ipv4_connect_and_relay() {
    let echo_addr = spawn_echo_server().await;

    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown_tx = spawn_vless_tls_server(proxy_addr, &cert_path, &key_path).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let mut tls = tls_connect(proxy_addr, &cert_pem).await;

    let request = build_tcp_request(
        &uuid,
        echo_addr.ip().to_string().parse::<Ipv4Addr>().unwrap(),
        echo_addr.port(),
    );
    tls.write_all(&request).await.unwrap();

    let mut response = [0u8; 2];
    tls.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    let payload = b"hello over tls+vless";
    tls.write_all(payload).await.unwrap();

    let mut received = vec![0u8; payload.len()];
    tls.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);

    let _ = shutdown_tx.send(true);
}

#[tokio::test]
async fn vless_over_tls_invalid_uuid_closes_without_response() {
    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown_tx = spawn_vless_tls_server(proxy_addr, &cert_path, &key_path).await;

    let bad_uuid = [0u8; 16];
    let mut tls = tls_connect(proxy_addr, &cert_pem).await;

    let request = build_tcp_request(&bad_uuid, Ipv4Addr::new(127, 0, 0, 1), 80);
    tls.write_all(&request).await.unwrap();

    let mut response = [0u8; 2];
    let read_result = tls.read_exact(&mut response).await;
    assert!(
        read_result.is_err(),
        "invalid UUID must close connection without VLESS response"
    );

    let _ = shutdown_tx.send(true);
}
