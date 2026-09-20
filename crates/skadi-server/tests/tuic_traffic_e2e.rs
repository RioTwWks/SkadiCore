//! E2E: skadi-server (TUIC) ↔ skadi-client ([tuic]) ↔ SOCKS5 echo.
//!
//! Пропускается, если нет `tuic-server`/`tuic-client` (или env binaries).

mod common;

use common::test_outbound;
use rcgen::generate_simple_self_signed;
use skadi_protocol::{Socks5Config, VlessConfig};
use skadi_server::config::{Config, ProtocolConfig, ServerConfig, TransportConfig, TuicFileConfig};
use skadi_server::run_server;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const TEST_UUID: &str = "00000000-0000-0000-0000-0000000000e2";

fn which(name: &str) -> Option<PathBuf> {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

fn bin_available(env_name: &str, default: &str) -> bool {
    std::env::var(env_name)
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| which(default))
        .is_some()
}

fn tuic_binaries_available() -> bool {
    bin_available("TUIC_SERVER_BINARY", "tuic-server")
        && bin_available("TUIC_CLIENT_BINARY", "tuic-client")
}

async fn spawn_echo() -> std::net::SocketAddr {
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

async fn socks5_echo(proxy: std::net::SocketAddr, target: std::net::SocketAddr) {
    let mut stream = TcpStream::connect(proxy).await.unwrap();
    stream.write_all(&[0x05, 0x01, 0x00]).await.unwrap();
    let mut method = [0u8; 2];
    stream.read_exact(&mut method).await.unwrap();
    assert_eq!(method, [0x05, 0x00]);

    let ip: Ipv4Addr = target.ip().to_string().parse().unwrap();
    let mut req = vec![0x05, 0x01, 0x00, 0x01];
    req.extend_from_slice(&ip.octets());
    req.extend_from_slice(&target.port().to_be_bytes());
    stream.write_all(&req).await.unwrap();

    let mut head = [0u8; 4];
    stream.read_exact(&mut head).await.unwrap();
    assert_eq!(head[0], 0x05);
    assert_eq!(head[1], 0x00, "SOCKS5 CONNECT failed REP={:#x}", head[1]);
    match head[3] {
        0x01 => {
            let mut rest = [0u8; 6];
            stream.read_exact(&mut rest).await.unwrap();
        }
        _ => panic!("unexpected ATYP"),
    }

    let payload = b"hello tuic traffic e2e";
    stream.write_all(payload).await.unwrap();
    let mut got = vec![0u8; payload.len()];
    stream.read_exact(&mut got).await.unwrap();
    assert_eq!(got, payload);
}

#[tokio::test]
async fn tuic_skadi_client_socks5_echo() {
    if !tuic_binaries_available() {
        eprintln!("SKIP tuic_traffic_e2e: tuic-server/tuic-client (or TUIC_*_BINARY) not found");
        return;
    }

    let echo_addr = spawn_echo().await;

    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();

    let tcp_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tcp_addr = tcp_listener.local_addr().unwrap();
    drop(tcp_listener);

    let tuic_listener = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let tuic_port = tuic_listener.local_addr().unwrap().port();
    drop(tuic_listener);

    let password = "test-only-tuic-e2e";
    let config = Config {
        server: ServerConfig::with_listen(tcp_addr.to_string()),
        protocol: ProtocolConfig {
            socks5: Socks5Config {
                enabled: false,
                auth: skadi_protocol::AuthMethod::NoAuth,
                users: vec![],
                bind: false,
                udp_associate: false,
            },
            vless: VlessConfig {
                enabled: false,
                users: vec![],
            },
        },
        transport: TransportConfig {
            tuic: TuicFileConfig {
                enabled: true,
                listen: format!("127.0.0.1:{tuic_port}"),
                uuid: Some(TEST_UUID.into()),
                password: Some(password.into()),
                certificate: Some(cert_path.to_string_lossy().into_owned()),
                private_key: Some(key_path.to_string_lossy().into_owned()),
                congestion_control: "bbr".into(),
                alpn: vec!["h3".into()],
            },
            ..Default::default()
        },
        api: Default::default(),
        metrics: Default::default(),
        outbound: test_outbound(),
    };
    config.validate().unwrap();

    let (server_tx, server_rx) = watch::channel(false);
    tokio::spawn(run_server(config, server_rx));
    tokio::time::sleep(Duration::from_millis(1000)).await;

    let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socks_addr = client_listener.local_addr().unwrap();
    drop(client_listener);

    let client_toml = format!(
        r#"
[client]
listen = "{socks}"

[tuic]
server = "127.0.0.1:{tuic_port}"
uuid = "{uuid}"
password = "{password}"
allow_insecure = true
"#,
        socks = socks_addr,
        tuic_port = tuic_port,
        uuid = TEST_UUID,
        password = password,
    );
    let client_path = dir.path().join("client.toml");
    std::fs::write(&client_path, client_toml).unwrap();

    let client_cfg = skadi_client::ClientConfig::load(&client_path).unwrap();
    let (client_tx, client_rx) = watch::channel(false);
    let client_task = tokio::spawn(skadi_client::run(client_cfg, client_rx));
    tokio::time::sleep(Duration::from_millis(1500)).await;

    socks5_echo(socks_addr, echo_addr).await;

    let _ = client_tx.send(true);
    let _ = client_task.await;
    let _ = server_tx.send(true);
}
