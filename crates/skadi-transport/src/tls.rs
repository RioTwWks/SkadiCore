//! TLS-транспорт на базе rustls + tokio-rustls.

use anyhow::{Context, Result};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use rustls::ServerConfig;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::collections::HashMap;
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

    #[error("no TLS certificates configured")]
    NoCertificates,

    #[error("SNI entry has no server_names")]
    EmptyServerNames,

    #[error("duplicate SNI server name: {0}")]
    DuplicateServerName(String),
}

/// Пути к PEM-сертификату и ключу.
#[derive(Debug, Clone)]
pub struct TlsCertPaths {
    pub cert_path: String,
    pub key_path: String,
}

/// Сертификат, привязанный к одному или нескольким SNI-именам.
#[derive(Debug, Clone)]
pub struct TlsSniCert {
    pub server_names: Vec<String>,
    pub cert_path: String,
    pub key_path: String,
}

/// Конфигурация TLS-сервера.
#[derive(Debug, Clone)]
pub struct TlsServerConfig {
    /// Сертификат по умолчанию (без SNI или при неизвестном имени).
    pub default: Option<TlsCertPaths>,
    /// Сертификаты по SNI.
    pub sni_certs: Vec<TlsSniCert>,
    /// ALPN-протоколы, например `h2`, `http/1.1`.
    pub alpn: Vec<String>,
}

impl TlsServerConfig {
    /// Один сертификат без SNI-роутинга (обратная совместимость).
    pub fn single(cert_path: String, key_path: String, alpn: Vec<String>) -> Self {
        Self {
            default: Some(TlsCertPaths {
                cert_path,
                key_path,
            }),
            sni_certs: Vec::new(),
            alpn,
        }
    }
}

/// TLS-транспорт для входящих соединений.
#[derive(Clone)]
pub struct TlsTransport {
    acceptor: TlsAcceptor,
}

impl TlsTransport {
    /// Создать acceptor из конфигурации (single-cert или SNI).
    pub fn new(config: &TlsServerConfig) -> Result<Self> {
        ensure_crypto_provider()?;
        let provider = crypto_provider()?;
        let resolver = build_resolver(config, &provider)?;

        let mut server_config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("unsupported TLS protocol versions")?
            .with_no_client_auth()
            .with_cert_resolver(Arc::new(resolver));

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

/// Резолвер сертификатов: SNI → cert, fallback на default.
#[derive(Debug)]
struct SniCertResolver {
    by_name: HashMap<String, Arc<CertifiedKey>>,
    default: Option<Arc<CertifiedKey>>,
}

impl ResolvesServerCert for SniCertResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        if let Some(name) = client_hello.server_name() {
            let normalized = name.to_ascii_lowercase();
            if let Some(ck) = self.by_name.get(&normalized) {
                return Some(Arc::clone(ck));
            }
        }
        self.default.as_ref().map(Arc::clone)
    }
}

fn build_resolver(
    config: &TlsServerConfig,
    provider: &Arc<rustls::crypto::CryptoProvider>,
) -> Result<SniCertResolver> {
    if config.default.is_none() && config.sni_certs.is_empty() {
        anyhow::bail!(TlsError::NoCertificates);
    }

    let mut by_name = HashMap::new();
    let mut default = None;

    if let Some(paths) = &config.default {
        default = Some(load_certified_key(paths, provider)?);
    }

    for entry in &config.sni_certs {
        if entry.server_names.is_empty() {
            anyhow::bail!(TlsError::EmptyServerNames);
        }

        let ck = load_certified_key(
            &TlsCertPaths {
                cert_path: entry.cert_path.clone(),
                key_path: entry.key_path.clone(),
            },
            provider,
        )?;

        for name in &entry.server_names {
            let normalized = normalize_server_name(name)?;
            if by_name.contains_key(&normalized) {
                anyhow::bail!(TlsError::DuplicateServerName(normalized));
            }
            by_name.insert(normalized, Arc::clone(&ck));
        }
    }

    Ok(SniCertResolver { by_name, default })
}

fn load_certified_key(
    paths: &TlsCertPaths,
    _provider: &Arc<rustls::crypto::CryptoProvider>,
) -> Result<Arc<CertifiedKey>> {
    let cert_chain = load_certs(&paths.cert_path)?;
    let key = load_private_key(&paths.key_path)?;
    let signing_key = rustls::crypto::ring::sign::any_supported_type(&key)
        .with_context(|| format!("invalid private key: {}", paths.key_path))?;
    Ok(Arc::new(CertifiedKey::new(cert_chain, signing_key)))
}

fn normalize_server_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty SNI server name");
    }
    if trimmed.parse::<std::net::IpAddr>().is_ok() {
        anyhow::bail!(
            "IP addresses are not supported in server_names: {}",
            trimmed
        );
    }
    Ok(trimmed.to_ascii_lowercase())
}

pub(crate) fn crypto_provider() -> Result<Arc<rustls::crypto::CryptoProvider>> {
    Ok(Arc::new(rustls::crypto::ring::default_provider()))
}

pub(crate) fn ensure_crypto_provider() -> Result<()> {
    Ok(())
}

pub(crate) fn load_certs(path: &str) -> Result<Vec<CertificateDer<'static>>> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(Path::new(path))
        .map_err(|e| anyhow::anyhow!("cannot open certificate file {}: {}", path, e))?
        .collect::<Result<Vec<_>, _>>()
        .with_context(|| format!("failed to parse certificates from {}", path))?;

    if certs.is_empty() {
        anyhow::bail!(TlsError::NoCertificate);
    }

    Ok(certs)
}

pub(crate) fn load_private_key(path: &str) -> Result<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(Path::new(path))
        .map_err(|e| anyhow::anyhow!("failed to parse private key from {}: {}", path, e))
}
