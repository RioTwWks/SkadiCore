//! VLESS over TLS + XHTTP packet-up (sequenced POST uplink + GET downlink).

mod common;
use rcgen::generate_simple_self_signed;
use rustls::RootCertStore;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, ServerName};
use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use skadi_protocol::{Socks5Config, VlessConfig, VlessUser};
use skadi_server::config::{
    Config, ProtocolConfig, ServerConfig, TlsConfig, TransportConfig, XhttpFileConfig,
};
use skadi_server::run_server;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_rustls::TlsConnector;

const TEST_USER_ID: &str = "b831381d-6324-4d53-ad4f-8cda48b30811";
const SESSION_ID: &str = "e2e-packet-up-session";

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

async fn spawn_xhttp_server(
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
            xhttp: XhttpFileConfig {
                enabled: true,
                path: "/xhttp".to_string(),
                host: None,
                mode: "packet-up".to_string(),
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
    tokio::time::sleep(Duration::from_millis(150)).await;
    shutdown_tx
}

async fn tls_connect(
    proxy_addr: std::net::SocketAddr,
    cert_pem: &str,
) -> tokio_rustls::client::TlsStream<TcpStream> {
    let mut roots = RootCertStore::empty();
    let certs = CertificateDer::pem_slice_iter(cert_pem.as_bytes())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for cert in certs {
        roots.add(cert).unwrap();
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let stream = TcpStream::connect(proxy_addr).await.unwrap();
    let server_name = ServerName::try_from("localhost".to_string()).unwrap();
    connector.connect(server_name, stream).await.unwrap()
}

async fn read_http_headers(stream: &mut tokio_rustls::client::TlsStream<TcpStream>) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while buf.len() < 64 * 1024 {
        stream.read_exact(&mut byte).await.unwrap();
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    buf
}

async fn read_line(stream: &mut tokio_rustls::client::TlsStream<TcpStream>) -> String {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while buf.len() < 4096 {
        stream.read_exact(&mut byte).await.unwrap();
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n") {
            break;
        }
    }
    String::from_utf8(buf).unwrap()
}

async fn read_chunked_exact(
    stream: &mut tokio_rustls::client::TlsStream<TcpStream>,
    out: &mut [u8],
) {
    let line = read_line(stream).await;
    let size = usize::from_str_radix(line.trim(), 16).unwrap();
    assert_eq!(size, out.len());
    stream.read_exact(out).await.unwrap();
    let mut crlf = [0u8; 2];
    stream.read_exact(&mut crlf).await.unwrap();
    assert_eq!(crlf, [b'\r', b'\n']);
}

#[tokio::test]
async fn vless_over_xhttp_packet_up_handshake() {
    let echo_addr = spawn_echo_server().await;

    let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_pem = cert.cert.pem();
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    std::fs::write(&cert_path, &cert_pem).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();

    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();
    drop(proxy_listener);

    let shutdown = spawn_xhttp_server(proxy_addr, &cert_path, &key_path).await;

    let get_path = format!("/xhttp/{}", SESSION_ID);
    let get_task = tokio::spawn({
        let cert_pem = cert_pem.clone();
        async move {
            let mut tls = tls_connect(proxy_addr, &cert_pem).await;
            let req = format!(
                "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n",
                path = get_path
            );
            tls.write_all(req.as_bytes()).await.unwrap();
            let headers = read_http_headers(&mut tls).await;
            assert!(headers.starts_with(b"HTTP/1.1 200"));
            tls
        }
    });

    tokio::time::sleep(Duration::from_millis(30)).await;

    let uuid = *Uuid::parse(TEST_USER_ID).unwrap().as_bytes();
    let vless_req = build_tcp_request(
        &uuid,
        echo_addr.ip().to_string().parse::<Ipv4Addr>().unwrap(),
        echo_addr.port(),
    );

    let post_path = format!("/xhttp/{}/0", SESSION_ID);
    let mut post_tls = tls_connect(proxy_addr, &cert_pem).await;
    let post_req = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {len}\r\n\r\n",
        path = post_path,
        len = vless_req.len()
    );
    post_tls.write_all(post_req.as_bytes()).await.unwrap();
    post_tls.write_all(&vless_req).await.unwrap();
    let post_headers = read_http_headers(&mut post_tls).await;
    assert!(post_headers.starts_with(b"HTTP/1.1 200"));

    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut get_tls = get_task.await.unwrap();
    let mut resp_hdr = [0u8; 2];
    read_chunked_exact(&mut get_tls, &mut resp_hdr).await;
    assert_eq!(resp_hdr, build_response_header(VLESS_VERSION));

    let _ = shutdown.send(true);
}
