//! Интеграционный тест: VLESS UDP over plain TCP.

mod common;
use skadi_protocol::vless::{build_response_header, build_udp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, ProtocolConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use std::net::Ipv4Addr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";

async fn spawn_udp_echo_server() -> std::net::SocketAddr {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();

    tokio::spawn(async move {
        let mut buf = [0u8; 1024];
        loop {
            match socket.recv_from(&mut buf).await {
                Ok((n, peer)) => {
                    if socket.send_to(&buf[..n], peer).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
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

async fn spawn_vless_server(proxy_addr: std::net::SocketAddr) -> watch::Sender<bool> {
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
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_tx
}

#[tokio::test]
async fn vless_udp_ipv4_connect_and_relay() {
    let echo_addr = spawn_udp_echo_server().await;

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown_tx = spawn_vless_server(proxy_addr).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();

    let request = build_udp_request(
        &uuid,
        echo_addr.ip().to_string().parse::<Ipv4Addr>().unwrap(),
        echo_addr.port(),
    );
    stream.write_all(&request).await.unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    let payload = b"dns-query-udp";
    let len = (payload.len() as u16).to_be_bytes();
    stream.write_all(&len).await.unwrap();
    stream.write_all(payload).await.unwrap();

    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await.unwrap();
    let resp_len = u16::from_be_bytes(len_buf) as usize;
    let mut received = vec![0u8; resp_len];
    stream.read_exact(&mut received).await.unwrap();
    assert_eq!(received, payload);

    let _ = shutdown_tx.send(true);
}

#[tokio::test]
async fn vless_vision_flow_rejected_without_response() {
    use skadi_protocol::vless::{build_addons_with_flow, build_tcp_request, FLOW_XTLS_VISION};

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown_tx = spawn_vless_server(proxy_addr).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();

    let mut request = build_tcp_request(&uuid, Ipv4Addr::new(127, 0, 0, 1), 80);
    let addons = build_addons_with_flow(FLOW_XTLS_VISION);
    request[17] = addons.len() as u8;
    request.splice(18..18, addons.iter().cloned());

    stream.write_all(&request).await.unwrap();

    let mut response = [0u8; 2];
    let read_result = stream.read_exact(&mut response).await;
    assert!(
        read_result.is_err(),
        "xtls-rprx-vision must close connection without VLESS response"
    );

    let _ = shutdown_tx.send(true);
}
