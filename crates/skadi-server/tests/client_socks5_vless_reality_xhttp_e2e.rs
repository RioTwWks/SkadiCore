//! E2E: skadi-client (SOCKS5) → VLESS + REALITY + XHTTP stream-one → skadi-server.

mod common;

use common::xray::RealityTestKeys;
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{
    Config, ProtocolConfig, RealityConfig, ServerConfig, TransportConfig, XhttpFileConfig,
};
use skadi_server::run_server;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
const SERVER_NAME: &str = "reality.test";
const XHTTP_PATH: &str = "/xhttp";

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
            let _ = listener.accept().await;
        }
    });
    addr
}

async fn spawn_reality_xhttp_vless_server(
    proxy_addr: std::net::SocketAddr,
    keys: &RealityTestKeys,
    dest_addr: std::net::SocketAddr,
    mode: &str,
) -> watch::Sender<bool> {
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
                impersonate_cert: None,
                fetch_impersonate_cert: false,
                kex_mode: "classic".into(),
            },
            xhttp: XhttpFileConfig {
                enabled: true,
                path: XHTTP_PATH.to_string(),
                host: None,
                mode: mode.to_string(),
                no_sse_header: false,
                x_padding_bytes: None,
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
    tokio::time::sleep(Duration::from_millis(200)).await;
    shutdown_tx
}

async fn socks5_echo(target_ip: Ipv4Addr, target_port: u16, proxy: std::net::SocketAddr) {
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
    assert_eq!(head[1], 0x00, "SOCKS5 CONNECT failed: REP={:#x}", head[1]);

    match head[3] {
        0x01 => {
            let mut rest = [0u8; 6];
            stream.read_exact(&mut rest).await.unwrap();
        }
        _ => panic!("unexpected SOCKS5 bound address type"),
    }

    let payload = b"hello via skadi-client+reality+xhttp";
    stream.write_all(payload).await.unwrap();
    let mut received = vec![0u8; payload.len()];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);
}

async fn run_client_xhttp_e2e(mode: &str) {
    let keys = RealityTestKeys::fixed();
    let echo_addr = spawn_echo_server().await;
    let dest_addr = spawn_dest_mock().await;

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let server_shutdown =
        spawn_reality_xhttp_vless_server(proxy_addr, &keys, dest_addr, mode).await;

    let client_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client_listen = client_listener.local_addr().unwrap();
    drop(client_listener);

    let dir = tempfile::tempdir().unwrap();
    let client_config_path = dir.path().join("client.toml");
    let password = RealityTestKeys::password_from_private(&keys.private_key);
    let client_toml = format!(
        r#"
[client]
listen = "{listen}"

[remote]
server = "{server}"
uuid = "{uuid}"

[remote.tls]
enabled = false

[remote.reality]
enabled = true
password = "{password}"
short_id = "{short_id}"
server_name = "{server_name}"

[remote.xhttp]
enabled = true
path = "{path}"
mode = "{mode}"
"#,
        listen = client_listen,
        server = proxy_addr,
        uuid = TEST_USER_ID,
        password = password,
        short_id = keys.short_id_hex,
        server_name = SERVER_NAME,
        path = XHTTP_PATH,
        mode = mode,
    );
    std::fs::write(&client_config_path, client_toml).unwrap();

    let config = skadi_client::ClientConfig::load(&client_config_path).unwrap();
    let (client_shutdown_tx, client_shutdown_rx) = watch::channel(false);
    let client_task = tokio::spawn(skadi_client::run(config, client_shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    socks5_echo(
        echo_addr.ip().to_string().parse().unwrap(),
        echo_addr.port(),
        client_listen,
    )
    .await;

    let _ = client_shutdown_tx.send(true);
    let _ = client_task.await;
    let _ = server_shutdown.send(true);
}

#[tokio::test]
async fn client_socks5_to_vless_reality_xhttp_stream_one() {
    run_client_xhttp_e2e("stream-one").await;
}

#[tokio::test]
async fn client_socks5_to_vless_reality_xhttp_stream_up() {
    run_client_xhttp_e2e("stream-up").await;
}

#[tokio::test]
async fn client_socks5_to_vless_reality_xhttp_packet_up() {
    run_client_xhttp_e2e("packet-up").await;
}
