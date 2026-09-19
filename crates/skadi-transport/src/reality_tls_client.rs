//! REALITY TLS outbound (Xray-compatible ClientHello + HMAC cert verify).

use anyhow::{Context, Result};
use rand::RngCore;
use rustls::reality::{DeferredRealityServerCertVerifier, RealityClientSettings};
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use skadi_core::Endpoint;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;
use tracing::debug;
use x25519_dalek::StaticSecret;

use crate::crypto::TlsKexMode;
use crate::tcp::TcpTransport;
use crate::tls::{crypto_provider_for_kex, ensure_crypto_provider};

/// Параметры REALITY outbound.
#[derive(Debug, Clone)]
pub struct RealityTlsClientConfig {
    pub server_public_key: [u8; 32],
    pub short_id: Vec<u8>,
    pub server_name: String,
    pub kex_mode: TlsKexMode,
}

/// TCP + REALITY TLS.
#[derive(Clone)]
pub struct RealityTlsOutboundTransport {
    tcp: TcpTransport,
    connector: TlsConnector,
    connect_timeout: Duration,
    /// TLS SNI (REALITY `serverName`); TCP dial uses the endpoint passed to [`Self::connect`].
    sni: ServerName<'static>,
    sni_log: String,
}

impl RealityTlsOutboundTransport {
    pub fn new(connect_timeout: Duration, config: &RealityTlsClientConfig) -> Result<Self> {
        Self::with_policy(connect_timeout, config, true)
    }

    pub fn with_policy(
        connect_timeout: Duration,
        config: &RealityTlsClientConfig,
        allow_private: bool,
    ) -> Result<Self> {
        ensure_crypto_provider()?;
        let mut eph_secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut eph_secret);
        let _ = StaticSecret::from(eph_secret);

        let reality_settings = RealityClientSettings::new(
            config.server_public_key,
            config.short_id.clone(),
            eph_secret,
        );
        let auth_slot = reality_settings.auth_key_slot();
        let verifier = Arc::new(DeferredRealityServerCertVerifier::new(auth_slot));

        let provider = crypto_provider_for_kex(config.kex_mode)?;
        let mut client_config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("unsupported TLS protocol versions")?
            .dangerous()
            .with_custom_certificate_verifier(verifier)
            .with_no_client_auth();

        client_config.reality_client = Some(Arc::new(reality_settings));
        client_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        client_config.resumption = rustls::client::Resumption::disabled();

        let sni = ServerName::try_from(config.server_name.clone())
            .map_err(|_| anyhow::anyhow!("invalid REALITY server_name: {}", config.server_name))?;

        Ok(Self {
            tcp: TcpTransport::with_policy(connect_timeout, allow_private),
            connector: TlsConnector::from(Arc::new(client_config)),
            connect_timeout,
            sni,
            sni_log: config.server_name.clone(),
        })
    }

    /// `tcp_endpoint` — адрес из `remote.server` (IP или hostname); SNI берётся из конфига REALITY.
    pub async fn connect(&self, tcp_endpoint: &Endpoint) -> Result<TlsStream<TcpStream>> {
        let target = endpoint_display(tcp_endpoint);
        debug!(target = %target, sni = %self.sni_log, "REALITY TLS outbound connecting");
        let tcp = self.tcp.connect(tcp_endpoint).await?;
        let tls = tokio::time::timeout(
            self.connect_timeout,
            self.connector.connect(self.sni.clone(), tcp),
        )
        .await
        .map_err(|_| anyhow::anyhow!("REALITY TLS handshake timeout to {}", target))?
        .with_context(|| format!("REALITY TLS handshake failed to {}", target))?;
        Ok(tls)
    }
}

fn endpoint_display(endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Ip(addr) => addr.to_string(),
        Endpoint::Domain(host, port) => format!("{}:{}", host, port),
    }
}
