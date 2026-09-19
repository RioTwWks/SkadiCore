//! Интеграционный тест: SNI-роутинг сертификатов.

mod common;
use rcgen::generate_simple_self_signed;
use rustls::RootCertStore;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName};
use skadi_protocol::AuthMethod;
use skadi_protocol::Socks5Config;
use skadi_server::config::{
    Config, ProtocolConfig, ServerConfig, TlsConfig, TlsSniCertConfig, TransportConfig,
};
use skadi_server::run_server;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio_rustls::TlsConnector;

fn trust_store_for_pem(cert_pem: &str) -> RootCertStore {
    let mut store = RootCertStore::empty();
    let der = CertificateDer::from_pem_slice(cert_pem.as_bytes()).unwrap();
    store.add(der).unwrap();
    store
}

async fn tls_handshake(
    addr: std::net::SocketAddr,
    sni: &str,
    trust: RootCertStore,
) -> tokio_rustls::client::TlsStream<TcpStream> {
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(trust)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let tcp = TcpStream::connect(addr).await.unwrap();
    let name = ServerName::try_from(sni.to_owned()).unwrap();
    connector.connect(name, tcp).await.unwrap()
}

#[tokio::test]
async fn sni_routes_to_matching_certificate() {
    let cert_alpha = generate_simple_self_signed(vec!["alpha.local".into()]).unwrap();
    let cert_beta = generate_simple_self_signed(vec!["beta.local".into()]).unwrap();
    let pem_alpha = cert_alpha.cert.pem();
    let pem_beta = cert_beta.cert.pem();

    let dir = tempfile::tempdir().unwrap();
    let alpha_cert = dir.path().join("alpha.pem");
    let alpha_key = dir.path().join("alpha.key");
    let beta_cert = dir.path().join("beta.pem");
    let beta_key = dir.path().join("beta.key");
    std::fs::write(&alpha_cert, &pem_alpha).unwrap();
    std::fs::write(&alpha_key, cert_alpha.key_pair.serialize_pem()).unwrap();
    std::fs::write(&beta_cert, &pem_beta).unwrap();
    std::fs::write(&beta_key, cert_beta.key_pair.serialize_pem()).unwrap();

    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig::with_listen(proxy_addr.to_string()),
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: true,
                auth: AuthMethod::NoAuth,
                users: vec![],
                bind: false,
                udp_associate: false,
            },
            vless: Default::default(),
        },
        transport: TransportConfig {
            reality: Default::default(),
            tls: TlsConfig {
                enabled: true,
                cert: None,
                key: None,
                alpn: vec![],
                certificates: vec![
                    TlsSniCertConfig {
                        server_names: vec!["alpha.local".into()],
                        cert: alpha_cert.to_string_lossy().into_owned(),
                        key: alpha_key.to_string_lossy().into_owned(),
                    },
                    TlsSniCertConfig {
                        server_names: vec!["beta.local".into()],
                        cert: beta_cert.to_string_lossy().into_owned(),
                        key: beta_key.to_string_lossy().into_owned(),
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
        api: Default::default(),
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // alpha.local → сертификат alpha (доверяем только alpha).
    let mut alpha_tls =
        tls_handshake(proxy_addr, "alpha.local", trust_store_for_pem(&pem_alpha)).await;
    alpha_tls.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut reply = [0u8; 2];
    alpha_tls.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [0x05, 0x00]);

    // beta.local → сертификат beta.
    let mut beta_tls =
        tls_handshake(proxy_addr, "beta.local", trust_store_for_pem(&pem_beta)).await;
    beta_tls.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    beta_tls.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [0x05, 0x00]);

    // Неизвестный SNI без default → TLS handshake должен упасть.
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(trust_store_for_pem(&pem_alpha))
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));
    let tcp = TcpStream::connect(proxy_addr).await.unwrap();
    let name = ServerName::try_from("gamma.local".to_owned()).unwrap();
    let err = connector.connect(name, tcp).await;
    assert!(
        err.is_err(),
        "unknown SNI must fail without default certificate"
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn sni_unknown_name_falls_back_to_default_cert() {
    let cert_default = generate_simple_self_signed(vec!["default.local".into()]).unwrap();
    let cert_named = generate_simple_self_signed(vec!["named.local".into()]).unwrap();
    let pem_default = cert_default.cert.pem();
    let pem_named = cert_named.cert.pem();

    let dir = tempfile::tempdir().unwrap();
    let default_cert = dir.path().join("default.pem");
    let default_key = dir.path().join("default.key");
    let named_cert = dir.path().join("named.pem");
    let named_key = dir.path().join("named.key");
    std::fs::write(&default_cert, &pem_default).unwrap();
    std::fs::write(&default_key, cert_default.key_pair.serialize_pem()).unwrap();
    std::fs::write(&named_cert, &pem_named).unwrap();
    std::fs::write(&named_key, cert_named.key_pair.serialize_pem()).unwrap();

    let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig::with_listen(proxy_addr.to_string()),
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: true,
                auth: AuthMethod::NoAuth,
                users: vec![],
                bind: false,
                udp_associate: false,
            },
            vless: Default::default(),
        },
        transport: TransportConfig {
            reality: Default::default(),
            tls: TlsConfig {
                enabled: true,
                cert: Some(default_cert.to_string_lossy().into_owned()),
                key: Some(default_key.to_string_lossy().into_owned()),
                alpn: vec![],
                certificates: vec![TlsSniCertConfig {
                    server_names: vec!["named.local".into()],
                    cert: named_cert.to_string_lossy().into_owned(),
                    key: named_key.to_string_lossy().into_owned(),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
        api: Default::default(),
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    // named.local → named cert
    let mut named_tls =
        tls_handshake(proxy_addr, "named.local", trust_store_for_pem(&pem_named)).await;
    named_tls.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut reply = [0u8; 2];
    named_tls.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [0x05, 0x00]);

    // Имя вне SNI-таблицы → сервер отдаёт default cert.
    // Клиент запрашивает default.local, чтобы проверка имени прошла.
    let mut fallback_tls = tls_handshake(
        proxy_addr,
        "default.local",
        trust_store_for_pem(&pem_default),
    )
    .await;
    fallback_tls.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    fallback_tls.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, [0x05, 0x00]);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
