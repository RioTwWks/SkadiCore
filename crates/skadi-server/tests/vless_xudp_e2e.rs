//! Интеграционный тест: VLESS Mux XUDP (session_id=0, GlobalID, per-packet addressing).

use skadi_protocol::vless::{
    build_mux_request, build_response_header, encode_data_frame, parse_meta_body, MuxMeta, Uuid,
    NETWORK_UDP, OPTION_DATA, SESSION_STATUS_KEEP, SESSION_STATUS_NEW, VLESS_VERSION,
    XUDP_SESSION_ID,
};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{Config, ProtocolConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::watch;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
const TEST_GLOBAL_ID: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
const TEST_GLOBAL_ID_RECONNECT: [u8; 8] = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];

async fn spawn_udp_echo_server() -> SocketAddr {
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
            email: None,
            flow: None,
        }],
    }
}

async fn spawn_vless_server(proxy_addr: SocketAddr) -> watch::Sender<bool> {
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
        outbound: Default::default(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;
    shutdown_tx
}

fn build_xudp_new_frame(global_id: [u8; 8], target: SocketAddr, payload: &[u8]) -> Vec<u8> {
    let meta = MuxMeta {
        session_id: XUDP_SESSION_ID,
        status: SESSION_STATUS_NEW,
        option: OPTION_DATA,
        network: Some(NETWORK_UDP),
        target: Some(skadi_core::Endpoint::Ip(target)),
        global_id: Some(global_id),
    };
    encode_data_frame(&meta, payload).unwrap()
}

fn build_xudp_keep_frame(target: SocketAddr, payload: &[u8]) -> Vec<u8> {
    let meta = MuxMeta {
        session_id: XUDP_SESSION_ID,
        status: SESSION_STATUS_KEEP,
        option: OPTION_DATA,
        network: Some(NETWORK_UDP),
        target: Some(skadi_core::Endpoint::Ip(target)),
        global_id: None,
    };
    encode_data_frame(&meta, payload).unwrap()
}

async fn read_mux_payload(stream: &mut TcpStream) -> (Option<skadi_core::Endpoint>, Vec<u8>) {
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await.unwrap();
    let meta_len = u16::from_be_bytes(len_buf) as usize;
    let mut meta_body = vec![0u8; meta_len];
    stream.read_exact(&mut meta_body).await.unwrap();

    let meta = parse_meta_body(&meta_body).unwrap();
    if !meta.has_data() {
        return (meta.target, vec![]);
    }

    let mut chunk_len_buf = [0u8; 2];
    stream.read_exact(&mut chunk_len_buf).await.unwrap();
    let chunk_len = u16::from_be_bytes(chunk_len_buf) as usize;
    let mut payload = vec![0u8; chunk_len];
    if chunk_len > 0 {
        stream.read_exact(&mut payload).await.unwrap();
    }
    (meta.target, payload)
}

// Tarpaulin mis-instruments XUDP relay paths; covered by `cargo test`.
#[cfg_attr(tarpaulin, ignore)]
#[tokio::test]
async fn vless_xudp_connect_and_relay() {
    let echo_addr = spawn_udp_echo_server().await;

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown_tx = spawn_vless_server(proxy_addr).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();

    stream.write_all(&build_mux_request(&uuid)).await.unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    let target = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), echo_addr.port());
    let payload = b"xudp-ping";
    let frame = build_xudp_new_frame(TEST_GLOBAL_ID, target, payload);
    stream.write_all(&frame).await.unwrap();

    let (source, received) = read_mux_payload(&mut stream).await;
    assert_eq!(received, payload);
    assert_eq!(
        source,
        Some(skadi_core::Endpoint::Ip(SocketAddr::new(
            Ipv4Addr::LOCALHOST.into(),
            echo_addr.port()
        )))
    );

    let payload2 = b"xudp-pong";
    let keep = build_xudp_keep_frame(target, payload2);
    stream.write_all(&keep).await.unwrap();

    let (_, received2) = read_mux_payload(&mut stream).await;
    assert_eq!(received2, payload2);

    let _ = shutdown_tx.send(true);
}

/// Два последовательных mux-соединения с одним GlobalID: второе должно hit'нуть cone.
#[cfg_attr(tarpaulin, ignore)]
#[tokio::test]
async fn vless_xudp_hit_reconnect() {
    let echo_addr = spawn_udp_echo_server().await;

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown_tx = spawn_vless_server(proxy_addr).await;
    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let target = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), echo_addr.port());

    async fn xudp_roundtrip(
        proxy_addr: SocketAddr,
        uuid: &[u8; 16],
        global_id: [u8; 8],
        target: SocketAddr,
    ) {
        let mut stream = TcpStream::connect(proxy_addr).await.unwrap();
        stream.write_all(&build_mux_request(uuid)).await.unwrap();

        let mut response = [0u8; 2];
        stream.read_exact(&mut response).await.unwrap();
        assert_eq!(response, build_response_header(VLESS_VERSION));

        let payload = b"xudp-reconnect";
        stream
            .write_all(&build_xudp_new_frame(global_id, target, payload))
            .await
            .unwrap();

        let (_, received) = read_mux_payload(&mut stream).await;
        assert_eq!(received, payload);
    }

    xudp_roundtrip(proxy_addr, &uuid, TEST_GLOBAL_ID_RECONNECT, target).await;
    xudp_roundtrip(proxy_addr, &uuid, TEST_GLOBAL_ID_RECONNECT, target).await;

    let _ = shutdown_tx.send(true);
}
