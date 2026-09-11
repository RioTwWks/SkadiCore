//! Интеграционный тест: SOCKS5 CONNECT поверх TLS.

use rcgen::generate_simple_self_signed;
use rustls::pki_types::ServerName;
use rustls::RootCertStore;
use rustls_pemfile::certs;
use skadi_protocol::AuthMethod;
use skadi_protocol::Socks5Config;
use skadi_server::config::{Config, ProtocolConfig, ServerConfig, TlsConfig, TransportConfig};
use skadi_server::run_server;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsConnector;

/// Поднять echo-сервер и вернуть его адрес.
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

fn write_socks5_connect_request(host: &str, port: u16) -> Vec<u8> {
    let host_bytes = host.as_bytes();
    let mut req = Vec::with_capacity(7 + host_bytes.len());
    req.push(0x05); // VER
    req.push(0x01); // CMD CONNECT
    req.push(0x00); // RSV
    req.push(0x03); // ATYP domain
    req.push(host_bytes.len() as u8);
    req.extend_from_slice(host_bytes);
    req.extend_from_slice(&port.to_be_bytes());
    req
}

#[tokio::test]
async fn socks5_over_tls_connect_and_relay() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    let echo_addr = spawn_echo_server().await;
    let echo_host = echo_addr.ip().to_string();
    let echo_port = echo_addr.port();

    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();

    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, key_pem).unwrap();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    // Освобождаем порт — сервер сам забиндится на тот же адрес.
    drop(proxy_listener);

    let config = Config {
        server: ServerConfig {
            listen: proxy_addr.to_string(),
        },
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: true,
                auth: AuthMethod::NoAuth,
                users: vec![],
            },
            vless: Default::default(),
        },
        transport: TransportConfig {
            tls: TlsConfig {
                enabled: true,
                cert: Some(cert_path.to_string_lossy().into_owned()),
                key: Some(key_path.to_string_lossy().into_owned()),
                alpn: vec!["http/1.1".into()],
                certificates: vec![],
            },
        },
    };

    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));

    // Даём серверу время подняться.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // TLS-клиент с доверенным self-signed сертификатом.
    let mut root_store = RootCertStore::empty();
    let mut cert_reader = Cursor::new(cert_pem.as_bytes());
    let cert_der = certs(&mut cert_reader).next().unwrap().unwrap();
    root_store.add(cert_der).unwrap();

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(client_config));

    let tcp = TcpStream::connect(proxy_addr).await.unwrap();
    let server_name = ServerName::try_from("localhost").unwrap();
    let mut tls = connector.connect(server_name, tcp).await.unwrap();

    // SOCKS5 greeting + no-auth.
    tls.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method_reply = [0u8; 2];
    tls.read_exact(&mut method_reply).await.unwrap();
    assert_eq!(method_reply, [0x05, 0x00]);

    // CONNECT к echo-серверу.
    let connect = write_socks5_connect_request(&echo_host, echo_port);
    tls.write_all(&connect).await.unwrap();

    let mut reply_header = [0u8; 4];
    tls.read_exact(&mut reply_header).await.unwrap();
    assert_eq!(reply_header[0], 0x05);
    assert_eq!(reply_header[1], 0x00); // success

    // Дочитываем BND.ADDR по ATYP из reply_header[3].
    match reply_header[3] {
        0x01 => {
            let mut rest = [0u8; 6];
            tls.read_exact(&mut rest).await.unwrap();
        }
        0x03 => {
            let mut dlen = [0u8; 1];
            tls.read_exact(&mut dlen).await.unwrap();
            let mut rest = vec![0u8; dlen[0] as usize + 2];
            tls.read_exact(&mut rest).await.unwrap();
        }
        0x04 => {
            let mut rest = [0u8; 18];
            tls.read_exact(&mut rest).await.unwrap();
        }
        other => panic!("unexpected SOCKS5 ATYP: 0x{:02x}", other),
    }

    let payload = b"hello over tls+socks5";
    tls.write_all(payload).await.unwrap();

    let mut received = vec![0u8; payload.len()];
    tls.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
