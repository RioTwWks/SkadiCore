//! TLS-клиент для исходящих соединений (outbound).

use anyhow::{bail, Context, Result};
use rustls::ClientConfig;
use rustls::RootCertStore;
use rustls_pki_types::ServerName;
use skadi_core::Endpoint;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::debug;

use crate::tcp::TcpTransport;
use crate::tls::{crypto_provider, ensure_crypto_provider, load_certs, load_private_key};

/// Конфигурация TLS outbound.
#[derive(Debug, Clone, Default)]
pub struct TlsClientConfig {
    /// PEM с доверенными CA. Если `None` — системное хранилище (`rustls-native-certs`).
    pub ca_file: Option<String>,
    /// Клиентский сертификат (mTLS), опционально.
    pub client_cert: Option<String>,
    pub client_key: Option<String>,
}

/// Исходящий транспорт: TCP + TLS handshake с проверкой сертификата.
#[derive(Clone)]
pub struct TlsOutboundTransport {
    tcp: TcpTransport,
    connector: TlsConnector,
    connect_timeout: Duration,
}

impl TlsOutboundTransport {
    pub fn new(connect_timeout: Duration, config: &TlsClientConfig) -> Result<Self> {
        ensure_crypto_provider()?;
        let roots = build_root_store(config.ca_file.as_deref())?;

        let provider = crypto_provider()?;
        let builder = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("unsupported TLS protocol versions")?
            .with_root_certificates(roots);

        let client_config = match (&config.client_cert, &config.client_key) {
            (Some(cert), Some(key)) => {
                let chain = load_certs(cert)?;
                let key = load_private_key(key)?;
                builder
                    .with_client_auth_cert(chain, key)
                    .context("invalid outbound TLS client certificate")?
            }
            (None, None) => builder.with_no_client_auth(),
            _ => bail!("outbound.tls.cert and outbound.tls.key must both be set"),
        };

        Ok(Self {
            tcp: TcpTransport::new(connect_timeout),
            connector: TlsConnector::from(Arc::new(client_config)),
            connect_timeout,
        })
    }

    /// TCP connect + TLS handshake к целевому endpoint.
    pub async fn connect(&self, endpoint: &Endpoint) -> Result<TlsStream<TcpStream>> {
        let server_name = endpoint_server_name(endpoint)?;
        let target = endpoint_display(endpoint);

        debug!(target = %target, "TLS outbound connecting");

        let tcp = self.tcp.connect(endpoint).await?;

        let tls = tokio::time::timeout(
            self.connect_timeout,
            self.connector.connect(server_name, tcp),
        )
        .await
        .map_err(|_| anyhow::anyhow!("TLS outbound handshake timeout to {}", target))?
        .with_context(|| format!("TLS outbound handshake failed to {}", target))?;

        Ok(tls)
    }
}

fn build_root_store(ca_file: Option<&str>) -> Result<RootCertStore> {
    let mut roots = RootCertStore::empty();

    if ca_file.is_none() {
        let native = rustls_native_certs::load_native_certs();
        for err in native.errors {
            tracing::warn!(error = %err, "failed to load system CA certificate");
        }
        for cert in native.certs {
            if let Err(e) = roots.add(cert) {
                tracing::warn!(error = %e, "skipping invalid system CA certificate");
            }
        }
    }

    if let Some(path) = ca_file {
        for cert in load_certs(path)? {
            roots
                .add(cert)
                .with_context(|| format!("invalid CA certificate in {}", path))?;
        }
    }

    if roots.is_empty() {
        bail!("no trust anchors for outbound TLS (set outbound.tls.ca_file or use system roots)");
    }

    Ok(roots)
}

fn endpoint_server_name(endpoint: &Endpoint) -> Result<ServerName<'static>> {
    match endpoint {
        Endpoint::Domain(host, _) => {
            let host = host.trim();
            if host.is_empty() {
                bail!("empty domain for TLS SNI");
            }
            ServerName::try_from(host.to_string())
                .map_err(|_| anyhow::anyhow!("invalid TLS server name: {}", host))
        }
        Endpoint::Ip(addr) => Ok(ServerName::IpAddress(addr.ip().into())),
    }
}

fn endpoint_display(endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Ip(addr) => addr.to_string(),
        Endpoint::Domain(host, port) => format!("{}:{}", host, port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::generate_simple_self_signed;
    use rustls::ServerConfig;
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, PrivateKeyDer};
    use skadi_core::Endpoint;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio_rustls::TlsAcceptor;

    #[tokio::test]
    async fn tls_outbound_connects_and_echoes() {
        let cert = generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let cert_pem = cert.cert.pem();
        let key_pem = cert.key_pair.serialize_pem();

        let ca_path = std::env::temp_dir().join("skadi-tls-outbound-test-ca.pem");
        std::fs::write(&ca_path, &cert_pem).unwrap();

        let certs: Vec<CertificateDer<'static>> =
            CertificateDer::pem_slice_iter(cert_pem.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        let key = PrivateKeyDer::from_pem_slice(key_pem.as_bytes()).unwrap();
        let server_config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut tls = acceptor.accept(tcp).await.unwrap();
            let mut buf = [0u8; 64];
            let n = tls.read(&mut buf).await.unwrap();
            tls.write_all(&buf[..n]).await.unwrap();
        });

        let client = TlsOutboundTransport::new(
            Duration::from_secs(5),
            &TlsClientConfig {
                ca_file: Some(ca_path.to_string_lossy().into_owned()),
                client_cert: None,
                client_key: None,
            },
        )
        .unwrap();

        let target = Endpoint::Domain("localhost".to_string(), port);
        let mut stream = client.connect(&target).await.unwrap();
        stream.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
    }
}
