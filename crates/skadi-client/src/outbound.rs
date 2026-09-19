//! Исходящее VLESS+TLS соединение к удалённому прокси.

use crate::config::ClientConfig;
use anyhow::{Context, Result};
use skadi_core::Endpoint;
use skadi_protocol::vless::VlessClient;
use skadi_transport::{OutboundTcpTransport, TcpUpstream};

#[derive(Clone)]
pub struct Outbound {
    transport: OutboundTcpTransport,
    proxy_tls: Endpoint,
    proxy_tcp: Endpoint,
    uuid: [u8; 16],
}

impl Outbound {
    pub fn from_config(config: &ClientConfig) -> Result<Self> {
        let remote = config
            .remote
            .as_ref()
            .context("remote section not configured")?;
        let transport = if remote.reality.enabled {
            OutboundTcpTransport::reality(
                config.connect_timeout(),
                &config.reality_tls_client_config()?,
            )?
        } else if remote.tls.enabled {
            OutboundTcpTransport::tls(config.connect_timeout(), &config.tls_client_config()?)?
        } else {
            OutboundTcpTransport::plain(config.connect_timeout())
        };

        let proxy_tls = if remote.reality.enabled {
            // REALITY: TCP dial — `remote.server`; SNI — `remote.reality.server_name` (в транспорте).
            config.proxy_endpoint()?
        } else {
            config.tls_sni_endpoint()?
        };

        Ok(Self {
            transport,
            proxy_tls,
            proxy_tcp: config.proxy_endpoint()?,
            uuid: config.uuid_bytes()?,
        })
    }

    pub async fn open_tcp(&self, target: &Endpoint) -> Result<TcpUpstream> {
        let mut stream = self.connect_proxy().await?;
        VlessClient::handshake_tcp(&mut stream, &self.uuid, target)
            .await
            .context("VLESS TCP handshake failed")?;
        Ok(stream)
    }

    pub async fn open_udp(&self, target: &Endpoint) -> Result<TcpUpstream> {
        let mut stream = self.connect_proxy().await?;
        VlessClient::handshake_udp(&mut stream, &self.uuid, target)
            .await
            .context("VLESS UDP handshake failed")?;
        Ok(stream)
    }

    async fn connect_proxy(&self) -> Result<TcpUpstream> {
        let endpoint = match &self.transport {
            OutboundTcpTransport::Tls(_) => &self.proxy_tls,
            OutboundTcpTransport::Reality(_) | OutboundTcpTransport::Plain(_) => &self.proxy_tcp,
        };
        self.transport
            .connect(endpoint)
            .await
            .context("remote proxy connect failed")
    }
}
