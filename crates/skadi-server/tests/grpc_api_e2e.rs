//! E2E: gRPC API hot reload пользователей VLESS, TLS и rate limiting.

mod common;
use rcgen::generate_simple_self_signed;
use skadi_api::skadi_api_client::SkadiApiClient;
use skadi_api::{
    AddVlessUserRequest, GetStatsRequest, ListVlessUsersRequest, RemoveVlessUserRequest,
    VlessUser as ProtoVlessUser,
};
use skadi_core::SecretString;
use skadi_protocol::vless::{build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{VlessConfig, VlessUser};
use skadi_server::config::{
    ApiConfig, ApiTlsConfig, Config, ProtocolConfig, ServerConfig, TransportConfig,
};
use skadi_server::run_server;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tonic::metadata::MetadataValue;
use tonic::transport::{Certificate, Channel, ClientTlsConfig};
use tonic::{Code, Request};

const INITIAL_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
const ADDED_USER_ID: &str = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
const API_TOKEN: &str = "test-grpc-token";

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

async fn vless_connect_and_echo(
    proxy_port: u16,
    user_id: &str,
    echo_addr: std::net::SocketAddr,
) -> Result<(), String> {
    let uuid = Uuid::parse(user_id).map_err(|e| e.to_string())?;
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", proxy_port))
        .await
        .map_err(|e| e.to_string())?;

    let ip = match echo_addr.ip() {
        std::net::IpAddr::V4(v4) => v4,
        _ => return Err("echo server must be IPv4".into()),
    };
    let req = build_tcp_request(uuid.as_bytes(), ip, echo_addr.port());
    stream.write_all(&req).await.map_err(|e| e.to_string())?;

    let mut header = [0u8; 2];
    let n = stream.read(&mut header).await.map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("connection closed during VLESS response (auth failed?)".into());
    }
    if header[0] != VLESS_VERSION {
        return Err(format!("unexpected VLESS version: 0x{:02x}", header[0]));
    }

    let payload = b"grpc-api-e2e";
    stream.write_all(payload).await.map_err(|e| e.to_string())?;
    let mut received = vec![0u8; payload.len()];
    stream
        .read_exact(&mut received)
        .await
        .map_err(|e| e.to_string())?;
    if received != payload {
        return Err("echo payload mismatch".into());
    }

    Ok(())
}

fn authed_request<T>(inner: T) -> Request<T> {
    let mut req = Request::new(inner);
    req.metadata_mut().insert(
        "authorization",
        MetadataValue::try_from(format!("Bearer {}", API_TOKEN)).unwrap(),
    );
    req
}

#[tokio::test]
async fn grpc_add_remove_vless_user_hot_reload() {
    let echo_addr = spawn_echo_server().await;
    let proxy_port = pick_free_port();
    let api_port = pick_free_port();

    let config = Config {
        server: ServerConfig::with_listen(format!("127.0.0.1:{}", proxy_port)),
        protocol: ProtocolConfig {
            socks5: Default::default(),
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: INITIAL_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: ApiConfig {
            enabled: true,
            listen: format!("127.0.0.1:{}", api_port),
            token: Some(SecretString::new(API_TOKEN)),
            tls: ApiTlsConfig::default(),
            rate_limit_per_sec: None,
            audit_log: true,
        },
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    vless_connect_and_echo(proxy_port, INITIAL_USER_ID, echo_addr)
        .await
        .expect("initial user should work");

    let channel = Channel::from_shared(format!("http://127.0.0.1:{}", api_port))
        .unwrap()
        .connect()
        .await
        .expect("grpc connect");
    let mut client = SkadiApiClient::new(channel);

    let add_resp = client
        .add_vless_user(authed_request(AddVlessUserRequest {
            user: Some(ProtoVlessUser {
                id: ADDED_USER_ID.to_string(),
                email: "added@example.com".to_string(),
            }),
        }))
        .await
        .expect("add_vless_user rpc")
        .into_inner();
    assert!(add_resp.ok, "add failed: {}", add_resp.message);

    vless_connect_and_echo(proxy_port, ADDED_USER_ID, echo_addr)
        .await
        .expect("added user should work");

    let list = client
        .list_vless_users(authed_request(ListVlessUsersRequest {}))
        .await
        .expect("list_vless_users")
        .into_inner();
    assert_eq!(list.users.len(), 2);

    let stats = client
        .get_stats(authed_request(GetStatsRequest {}))
        .await
        .expect("get_stats")
        .into_inner();
    assert_eq!(stats.vless_users, 2);

    let remove_resp = client
        .remove_vless_user(authed_request(RemoveVlessUserRequest {
            id: INITIAL_USER_ID.to_string(),
        }))
        .await
        .expect("remove_vless_user")
        .into_inner();
    assert!(remove_resp.ok, "remove failed: {}", remove_resp.message);

    assert!(
        vless_connect_and_echo(proxy_port, INITIAL_USER_ID, echo_addr)
            .await
            .is_err(),
        "removed user should fail auth"
    );

    vless_connect_and_echo(proxy_port, ADDED_USER_ID, echo_addr)
        .await
        .expect("remaining user should still work");

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn grpc_rejects_missing_token() {
    let api_port = pick_free_port();
    let proxy_port = pick_free_port();

    let config = Config {
        server: ServerConfig::with_listen(format!("127.0.0.1:{}", proxy_port)),
        protocol: ProtocolConfig {
            socks5: Default::default(),
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: INITIAL_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: ApiConfig {
            enabled: true,
            listen: format!("127.0.0.1:{}", api_port),
            token: Some(SecretString::new(API_TOKEN)),
            tls: ApiTlsConfig::default(),
            rate_limit_per_sec: None,
            audit_log: true,
        },
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let channel = Channel::from_shared(format!("http://127.0.0.1:{}", api_port))
        .unwrap()
        .connect()
        .await
        .expect("grpc connect");
    let mut client = SkadiApiClient::new(channel);

    let err = client
        .list_vless_users(Request::new(ListVlessUsersRequest {}))
        .await
        .expect_err("should reject unauthenticated request");
    assert_eq!(err.code(), tonic::Code::Unauthenticated);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn grpc_api_over_tls() {
    let proxy_port = pick_free_port();
    let api_port = pick_free_port();

    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let key_pem = cert.key_pair.serialize_pem();

    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("api-cert.pem");
    let key_path = dir.path().join("api-key.pem");
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, key_pem).unwrap();

    let config = Config {
        server: ServerConfig::with_listen(format!("127.0.0.1:{}", proxy_port)),
        protocol: ProtocolConfig {
            socks5: Default::default(),
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: INITIAL_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: ApiConfig {
            enabled: true,
            listen: format!("127.0.0.1:{}", api_port),
            token: Some(SecretString::new(API_TOKEN)),
            tls: ApiTlsConfig {
                enabled: true,
                cert: Some(cert_path.to_string_lossy().into_owned()),
                key: Some(key_path.to_string_lossy().into_owned()),
            },
            rate_limit_per_sec: None,
            audit_log: true,
        },
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(Certificate::from_pem(cert_pem.as_bytes()));
    let channel = Channel::from_shared(format!("https://127.0.0.1:{}", api_port))
        .unwrap()
        .tls_config(tls)
        .unwrap()
        .connect()
        .await
        .expect("grpc tls connect");
    let mut client = SkadiApiClient::new(channel);

    let stats = client
        .get_stats(authed_request(GetStatsRequest {}))
        .await
        .expect("get_stats over tls")
        .into_inner();
    assert_eq!(stats.vless_users, 1);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}

#[tokio::test]
async fn grpc_api_rate_limit() {
    let proxy_port = pick_free_port();
    let api_port = pick_free_port();

    let config = Config {
        server: ServerConfig::with_listen(format!("127.0.0.1:{}", proxy_port)),
        protocol: ProtocolConfig {
            socks5: Default::default(),
            vless: VlessConfig {
                enabled: true,
                users: vec![VlessUser {
                    id: INITIAL_USER_ID.to_string(),
                    email: None,
                    flow: None,
                }],
            },
        },
        transport: TransportConfig::default(),
        api: ApiConfig {
            enabled: true,
            listen: format!("127.0.0.1:{}", api_port),
            token: Some(SecretString::new(API_TOKEN)),
            tls: ApiTlsConfig::default(),
            rate_limit_per_sec: Some(2),
            audit_log: true,
        },
        metrics: Default::default(),
        outbound: common::test_outbound(),
    };
    config.validate().unwrap();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let server = tokio::spawn(run_server(config, shutdown_rx));
    tokio::time::sleep(Duration::from_millis(200)).await;

    let channel = Channel::from_shared(format!("http://127.0.0.1:{}", api_port))
        .unwrap()
        .connect()
        .await
        .expect("grpc connect");
    let mut client = SkadiApiClient::new(channel);

    let mut ok = 0u32;
    let mut limited = 0u32;
    for _ in 0..5 {
        match client.get_stats(authed_request(GetStatsRequest {})).await {
            Ok(_) => ok += 1,
            Err(e) if e.code() == Code::ResourceExhausted => limited += 1,
            Err(e) => panic!("unexpected error: {}", e),
        }
    }
    assert_eq!(ok, 2);
    assert_eq!(limited, 3);

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
