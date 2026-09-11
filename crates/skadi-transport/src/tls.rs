//! TLS-транспорт на базе rustls + tokio-rustls.

use anyhow::{Context, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::version::TLS13;
use rustls::ServerConfig;
use rustls_pemfile::{certs, private_key};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tracing::debug;

/// Ошибки TLS-слоя.
#[derive(Debug, Error)]
pub enum TlsError {
    #[error("no certificate found in PEM file")]
    NoCertificate,

    #[error("no private key found in PEM file")]
    NoPrivateKey,

    #[error("TLS handshake failed: {0}")]
    Handshake(String),
}

/// Конфигурация TLS-сервера (пути к PEM-файлам).
#[derive(Debug, Clone)]
pub struct TlsServerConfig {
    pub cert_path: String,
    pub key_path: String,
    /// ALPN-протоколы, например `h2`, `http/1.1`.
    pub alpn: Vec<String>,
}

/// TLS-транспорт для входящих соединений.
#[derive(Clone)]
pub struct TlsTransport {
    acceptor: TlsAcceptor,
}

impl TlsTransport {
    /// Создать acceptor из PEM-сертификата и приватного ключа.
    pub fn new(config: &TlsServerConfig) -> Result<Self> {
        ensure_crypto_provider()?;

        let cert_chain = load_certs(&config.cert_path)?;
        let key = load_private_key(&config.key_path)?;

        let mut server_config = ServerConfig::builder_with_protocol_versions(&[&TLS13])
            .with_no_client_auth()
            .with_single_cert(cert_chain, key)
            .context("invalid TLS certificate/key pair")?;

        if !config.alpn.is_empty() {
            server_config.alpn_protocols = config
                .alpn
                .iter()
                .map(|proto| proto.as_bytes().to_vec())
                .collect();
        }

        Ok(Self {
            acceptor: TlsAcceptor::from(Arc::new(server_config)),
        })
    }

    /// Выполнить TLS handshake поверх уже принятого TCP-соединения.
    pub async fn accept(&self, stream: TcpStream) -> Result<TlsStream<TcpStream>> {
        let peer = stream
            .peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".into());

        debug!(peer = %peer, "TLS handshake starting");

        self.acceptor
            .accept(stream)
            .await
            .map_err(|e| anyhow::anyhow!(TlsError::Handshake(e.to_string())))
    }
}

fn ensure_crypto_provider() -> Result<()> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
        && rustls::crypto::CryptoProvider::get_default().is_none()
    {
        anyhow::bail!("failed to install rustls ring crypto provider");
    }
    Ok(())
}

fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let file = File::open(Path::new(path))
        .with_context(|| format!("cannot open certificate file: {}", path))?;
    let mut reader = BufReader::new(file);

    let certs: Vec<CertificateDer<'static>> = certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificates from {}", path))?;

    if certs.is_empty() {
        anyhow::bail!(TlsError::NoCertificate);
    }

    Ok(certs)
}

fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    let file = File::open(Path::new(path))
        .with_context(|| format!("cannot open private key file: {}", path))?;
    let mut reader = BufReader::new(file);

    private_key(&mut reader)
        .with_context(|| format!("failed to parse private key from {}", path))?
        .ok_or_else(|| anyhow::anyhow!(TlsError::NoPrivateKey))
}
