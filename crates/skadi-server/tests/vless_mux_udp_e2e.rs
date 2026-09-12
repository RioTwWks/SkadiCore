//! Интеграционный тест: VLESS Mux UDP session over CMD_MUX.

use skadi_protocol::vless::{
    build_mux_request, build_response_header, encode_data_frame, parse_meta_body, MuxMeta, Uuid,
    NETWORK_UDP, OPTION_DATA, SESSION_STATUS_NEW, VLESS_VERSION,
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
const MUX_SESSION_ID: u16 = 2;

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

fn build_mux_udp_new_frame(target: SocketAddr, payload: &[u8]) -> Vec<u8> {
    let meta = MuxMeta {
        session_id: MUX_SESSION_ID,
        status: SESSION_STATUS_NEW,
        option: OPTION_DATA,
        network: Some(NETWORK_UDP),
        target: Some(skadi_core::Endpoint::Ip(target)),
    };
    encode_data_frame(&meta, payload).unwrap()
}

async fn read_mux_payload(stream: &mut TcpStream) -> Vec<u8> {
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await.unwrap();
    let meta_len = u16::from_be_bytes(len_buf) as usize;
    let mut meta_body = vec![0u8; meta_len];
    stream.read_exact(&mut meta_body).await.unwrap();

    let meta = parse_meta_body(&meta_body).unwrap();
    if !meta.has_data() {
        return vec![];
    }

    let mut chunk_len_buf = [0u8; 2];
    stream.read_exact(&mut chunk_len_buf).await.unwrap();
    let chunk_len = u16::from_be_bytes(chunk_len_buf) as usize;
    let mut payload = vec![0u8; chunk_len];
    if chunk_len > 0 {
        stream.read_exact(&mut payload).await.unwrap();
    }
    payload
}

#[tokio::test]
async fn vless_mux_udp_connect_and_relay() {
    let echo_addr = spawn_udp_echo_server().await;

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
        outbound: Default::default(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let mut stream = TcpStream::connect(proxy_addr).await.unwrap();

    stream.write_all(&build_mux_request(&uuid)).await.unwrap();

    let mut response = [0u8; 2];
    stream.read_exact(&mut response).await.unwrap();
    assert_eq!(response, build_response_header(VLESS_VERSION));

    let payload = b"mux-udp-ping";
    let frame = build_mux_udp_new_frame(
        SocketAddr::new(Ipv4Addr::LOCALHOST.into(), echo_addr.port()),
        payload,
    );
    stream.write_all(&frame).await.unwrap();

    let received = read_mux_payload(&mut stream).await;
    assert_eq!(received, payload);

    let _ = shutdown_tx.send(true);
}
