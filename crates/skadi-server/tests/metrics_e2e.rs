//! E2E: Prometheus `/metrics` и `/healthz`.

use skadi_protocol::vless::{build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{VlessConfig, VlessUser};
use skadi_server::config::{Config, MetricsConfig, ProtocolConfig, ServerConfig, TransportConfig};
use skadi_server::run_server;
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

fn pick_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn http_get(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("connect metrics");
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        path
    );
    stream.write_all(req.as_bytes()).await.unwrap();

    let mut body = String::new();
    stream.read_to_string(&mut body).await.unwrap();
    body
}

#[tokio::test]
async fn healthz_and_metrics_endpoints() {
    let echo_addr = spawn_echo_server().await;
    let proxy_port = pick_free_port();
    let metrics_port = pick_free_port();

    let config = Config {
        server: ServerConfig::with_listen(format!("127.0.0.1:{}", proxy_port)),
        protocol: ProtocolConfig {
            socks5: Default::default(),
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: TEST_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: Default::default(),
        metrics: MetricsConfig {
            enabled: true,
            listen: format!("127.0.0.1:{}", metrics_port),
        },
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let health = http_get(metrics_port, "/healthz").await;
    assert!(health.contains("200 OK"));
    assert!(health.contains("ok"));

    let before = http_get(metrics_port, "/metrics").await;
    assert!(before.contains("200 OK"));

    // VLESS-сессия увеличивает счётчики.
    let uuid = Uuid::parse(TEST_USER_ID).unwrap();
    let ip = match echo_addr.ip() {
        std::net::IpAddr::V4(v4) => v4,
        _ => panic!("ipv4 echo"),
    };
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
        .await
        .unwrap();
    stream
        .write_all(&build_tcp_request(uuid.as_bytes(), ip, echo_addr.port()))
        .await
        .unwrap();
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await.unwrap();
    assert_eq!(header[0], VLESS_VERSION);
    stream.write_all(b"ping").await.unwrap();
    let mut buf = [0u8; 4];
    stream.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");
    stream.shutdown().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let after = http_get(metrics_port, "/metrics").await;
    assert!(
        after.contains("skadicore_connections_total"),
        "expected connection counter in metrics"
    );
    assert!(
        after.contains("skadicore_transfer_bytes_total"),
        "expected transfer counter after relay"
    );

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
