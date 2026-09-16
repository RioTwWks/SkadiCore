//! E2E: локальный SOCKS5 (skadi-client) → VLESS+TLS → skadi-server → echo.

mod common;

use rcgen::generate_simple_self_signed;
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, ProtocolConfig, ServerConfig, TlsConfig, TransportConfig};
use skadi_server::run_server;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

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
            email: None,
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
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_tx
}

async fn socks5_connect(target_ip: Ipv4Addr, target_port: u16, proxy: std::net::SocketAddr) {
    let mut stream = TcpStream::connect(proxy).await.unwrap();

    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method = [0u8; 2];
    stream.read_exact(&mut method).await.unwrap();
    assert_eq!(method, [0x05, 0x00]);

    let mut req = vec![0x05, 0x01, 0x00, 0x01];
    req.extend_from_slice(&target_ip.octets());
    req.extend_from_slice(&target_port.to_be_bytes());
    stream.write_all(&req).await.unwrap();

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.unwrap();
    assert_eq!(head[0], 0x05);
    assert_eq!(head[1], 0x00);

    match head[3] {
        0x01 => {
            let mut rest = [0u8; 6];
            stream.read_exact(&mut rest).await.unwrap();
        }
        _ => panic!("unexpected SOCKS5 bound address type"),
    }

    let payload = b"hello via skadi-client";
    stream.write_all(payload).await.unwrap();
    let mut received = vec![0u8; payload.len()];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);
}

#[tokio::test]
async fn client_socks5_to_vless_tls_relay() {
    let echo_addr = spawn_echo_server().await;

    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    let ca_path = dir.path().join("ca.pem");
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();
    std::fs::write(&ca_path, &cert_pem).unwrap();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let server_shutdown = spawn_vless_tls_server(proxy_addr, &cert_path, &key_path).await;

    let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client_listen = client_listener.local_addr().unwrap();
    drop(client_listener);

    let client_config_path = dir.path().join("client.toml");
    let client_toml = format!(
        r#"
[client]
listen = "{listen}"

[remote]
server = "{server}"
uuid = "{uuid}"

[remote.tls]
enabled = true
ca_file = "{ca}"
server_name = "localhost"
"#,
        listen = client_listen,
        server = proxy_addr,
        uuid = TEST_USER_ID,
        ca = ca_path.display(),
    );
    std::fs::write(&client_config_path, client_toml).unwrap();

    let config = skadi_client::ClientConfig::load(&client_config_path).unwrap();
    let (client_shutdown_tx, client_shutdown_rx) = watch::channel(false);
    let client_task = tokio::spawn(skadi_client::run(config, client_shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    socks5_connect(
        echo_addr.ip().to_string().parse().unwrap(),
        echo_addr.port(),
        client_listen,
    )
    .await;

    let _ = client_shutdown_tx.send(true);
    let _ = client_task.await;
    let _ = server_shutdown.send(true);
}
