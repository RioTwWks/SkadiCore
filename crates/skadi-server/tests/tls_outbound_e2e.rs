//! Интеграционный тест: VLESS через прокси с TLS outbound к upstream.

mod common;

use rcgen::generate_simple_self_signed;
use skadi_protocol::vless::{build_response_header, build_tcp_domain_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{
    Config, OutboundConfig, OutboundTlsConfig, ProtocolConfig, ServerConfig, TransportConfig,
};
use skadi_server::run_server;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsAcceptor;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

async fn spawn_tls_echo_server(cert_pem: &str, key_pem: &str) -> std::net::SocketAddr {
    use rustls::ServerConfig;
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, PrivateKeyDer};
    use std::sync::Arc;

    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).unwrap();

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_config));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                break;
            };
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = [0u8; 1024];
                loop {
                    match tls.read(&mut buf).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tls.write_all(&buf[..n]).await.is_err() {
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
            email: None,
            flow: None,
        }],
    }
}

#[tokio::test]
async fn vless_relay_over_tls_outbound() {
    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();

    let dir = tempfile::tempdir().unwrap();
    let ca_path = dir.path().join("upstream-ca.pem");
    std::fs::write(&ca_path, &cert_pem).unwrap();

    let echo_addr = spawn_tls_echo_server(&cert_pem, &key_pem).await;

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
            },
            vless: test_vless_config(),
        },
        transport: TransportConfig::default(),
        api: Default::default(),
        metrics: Default::default(),
        outbound: OutboundConfig {
            tls: OutboundTlsConfig {
                enabled: true,
                ca_file: Some(ca_path.to_string_lossy().into_owned()),
                cert: None,
                key: None,
            },
            allow_private: true,
            ..Default::default()
        },
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();

    let request = build_tcp_domain_request(&uuid, "localhost", echo_addr.port());
    stream.write_all(&request).await.unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    let payload = b"hello via tls-outbound";
    stream.write_all(payload).await.unwrap();

    let mut received = vec![0u8; payload.len()];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);

    let _ = shutdown_tx.send(true);
}
