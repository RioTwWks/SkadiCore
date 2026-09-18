//! REALITY inbound: sniff ClientHello → verify → TLS или fallback на dest.

use super::auth::verify_client_reality;
use super::cert::generate_reality_cert;
use super::hello_parser::{parse_client_hello, ClientHelloInfo};
use super::prefixed::BufferedPrefixStream;
use crate::crypto::TlsKexMode;
use crate::relay::{copy_bidirectional_with_limits, RelayLimits};
use crate::tls::crypto_provider_for_kex;
use anyhow::{bail, Context, Result};
use rustls::reality::RealityConfig;
use rustls::ServerConfig;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

/// Ошибки REALITY-слоя.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RealityError {
    #[error("REALITY handshake failed: {0}")]
    Handshake(String),

    #[error("REALITY handshake timeout")]
    HandshakeTimeout,

    #[error("connection closed during REALITY sniff")]
    EarlyClose,

    #[error("fallback to dest completed")]
    FallbackHandled,
}

/// Конфигурация REALITY-сервера.
#[derive(Debug, Clone)]
pub struct RealityServerConfig {
    /// X25519 private key (32 bytes).
    pub private_key: [u8; 32],
    /// Fallback destination `host:port`.
    pub dest: String,
    /// Разрешённые SNI.
    pub server_names: Vec<String>,
    /// Short IDs (raw bytes, обычно 8 байт).
    pub short_ids: Vec<Vec<u8>>,
    /// DER leaf-сертификата dest для ImpersonateCert (rkn-fix).
    pub impersonate_cert: Option<Vec<u8>>,
    /// Таймаут подключения к fallback `dest`.
    pub connect_timeout: Duration,
    /// Таймаут неактивности relay после fallback.
    pub idle_timeout: Option<Duration>,
    /// Максимальная длительность relay после fallback.
    pub max_session_lifetime: Option<Duration>,
    /// Режим key exchange для REALITY TLS (после verify).
    pub kex_mode: TlsKexMode,
}

/// REALITY inbound transport.
#[derive(Clone)]
pub struct RealityTransport {
    reality_config: Arc<RealityConfig>,
    server_names: Vec<String>,
    private_key: [u8; 32],
    short_ids: Vec<Vec<u8>>,
    impersonate_cert: Option<Vec<u8>>,
    connect_timeout: Duration,
    relay_limits: RelayLimits,
    kex_mode: TlsKexMode,
}

impl RealityTransport {
    /// Создать REALITY transport из конфигурации.
    pub fn new(config: &RealityServerConfig) -> Result<Self> {
        let mut short_ids_bytes = Vec::new();
        for id in &config.short_ids {
            if id.is_empty() || id.len() > 8 {
                bail!("REALITY short_id must be 1..8 bytes");
            }
            short_ids_bytes.push(id.clone());
        }
        if short_ids_bytes.is_empty() {
            bail!("REALITY requires at least one short_id");
        }

        let reality_config = RealityConfig::new(config.private_key.to_vec())
            .with_verify_client(true)
            .with_short_ids(short_ids_bytes.clone())
            .with_dest(config.dest.clone());

        reality_config
            .validate()
            .map_err(|e| anyhow::anyhow!("REALITY config validation failed: {:?}", e))?;

        Ok(Self {
            reality_config: Arc::new(reality_config),
            server_names: config
                .server_names
                .iter()
                .map(|s| s.trim().to_ascii_lowercase())
                .collect(),
            private_key: config.private_key,
            short_ids: short_ids_bytes,
            impersonate_cert: config.impersonate_cert.clone(),
            connect_timeout: config.connect_timeout,
            relay_limits: RelayLimits {
                idle: config.idle_timeout,
                max_lifetime: config.max_session_lifetime,
            },
            kex_mode: config.kex_mode,
        })
    }

    /// Принять соединение: REALITY TLS или transparent fallback.
    pub async fn accept<S>(&self, mut stream: S) -> Result<TlsStream<BufferedPrefixStream<S>>>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let buffer = read_client_hello_prefix(&mut stream).await?;

        if let Ok(Some(info)) = parse_client_hello(&buffer) {
            if self.sni_allowed(&info) {
                if let Some(auth_key) =
                    verify_client_reality(&info, &buffer, &self.private_key, &self.short_ids)
                {
                    return self
                        .accept_reality_client(stream, buffer, &info, auth_key)
                        .await;
                }
            } else {
                warn!(
                    sni = ?info.server_name,
                    allowed = ?self.server_names,
                    "REALITY SNI mismatch"
                );
            }
        }

        let dest = self
            .reality_config
            .dest
            .as_deref()
            .unwrap_or("www.microsoft.com:443");
        debug!(dest, "REALITY fallback to dest");
        fallback(
            stream,
            &buffer,
            dest,
            self.connect_timeout,
            self.relay_limits,
        )
        .await?;
        Err(RealityError::FallbackHandled.into())
    }

    fn sni_allowed(&self, info: &ClientHelloInfo) -> bool {
        if self.server_names.is_empty() {
            return true;
        }
        info.server_name
            .as_ref()
            .map(|s| {
                self.server_names
                    .iter()
                    .any(|n| n == &s.to_ascii_lowercase())
            })
            .unwrap_or(false)
    }

    async fn accept_reality_client<S>(
        &self,
        stream: S,
        buffer: Vec<u8>,
        info: &ClientHelloInfo,
        auth_key: [u8; 32],
    ) -> Result<TlsStream<BufferedPrefixStream<S>>>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let server_name = info.server_name.as_deref().unwrap_or("localhost");

        info!(
            sni = server_name,
            impersonate = self.impersonate_cert.is_some(),
            "REALITY client verified, generating dynamic certificate"
        );

        let (cert, key) =
            generate_reality_cert(&auth_key, server_name, self.impersonate_cert.as_deref())?;

        let mut conn_reality_config = (*self.reality_config).clone();
        conn_reality_config.private_key = auth_key.to_vec();
        conn_reality_config.verify_client = false;

        let provider = crypto_provider_for_kex(self.kex_mode)?;
        let mut config = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("unsupported TLS protocol versions")?
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .map_err(|e| anyhow::anyhow!(RealityError::Handshake(e.to_string())))?;
        config.reality_config = Some(Arc::new(conn_reality_config));

        let acceptor = TlsAcceptor::from(Arc::new(config));
        let prefixed = BufferedPrefixStream::new(buffer, stream);

        match tokio::time::timeout(Duration::from_secs(5), acceptor.accept(prefixed)).await {
            Ok(Ok(tls)) => {
                info!("REALITY TLS handshake successful");
                Ok(tls)
            }
            Ok(Err(e)) => {
                error!(error = %e, "REALITY TLS handshake failed");
                Err(anyhow::anyhow!(RealityError::Handshake(e.to_string())))
            }
            Err(_) => Err(anyhow::anyhow!(RealityError::HandshakeTimeout)),
        }
    }
}

async fn read_client_hello_prefix<S>(stream: &mut S) -> Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = Vec::with_capacity(2048);
    let handshake_timeout = Duration::from_secs(5);

    let read_task = async {
        while buffer.len() < 5 {
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                bail!(RealityError::EarlyClose);
            }
            buffer.extend_from_slice(&chunk[..n]);
        }

        let needed = if buffer[0] == 0x16 {
            5 + u16::from_be_bytes([buffer[3], buffer[4]]) as usize
        } else {
            buffer.len()
        };
        while buffer.len() < needed && buffer.len() < 16384 {
            let mut chunk = [0u8; 1024];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
        }
        Ok(())
    };

    match tokio::time::timeout(handshake_timeout, read_task).await {
        Ok(result) => result?,
        Err(_) => bail!(RealityError::HandshakeTimeout),
    }

    Ok(buffer)
}

async fn fallback<S>(
    mut stream: S,
    prefix: &[u8],
    dest: &str,
    connect_timeout: Duration,
    relay_limits: RelayLimits,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let mut dest_stream =
        match tokio::time::timeout(connect_timeout, TcpStream::connect(dest)).await {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => bail!("fallback connection timeout"),
        };
    dest_stream.write_all(prefix).await?;
    copy_bidirectional_with_limits(&mut stream, &mut dest_stream, relay_limits).await?;
    Ok(())
}
