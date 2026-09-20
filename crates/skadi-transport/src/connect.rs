//! Унифицированный исходящий dial: [`Endpoint`] → stream.
//!
//! Новый TCP-транспорт = `impl OutboundTransport`, без правки ядра
//! `handle_connection` на этапе `connect` (UDP остаётся отдельным API).

use anyhow::Result;
use skadi_core::Endpoint;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::outbound::{OutboundTcpTransport, TcpUpstream};
use crate::reality_tls_client::RealityTlsOutboundTransport;
use crate::tcp::TcpTransport;
use crate::tls_client::TlsOutboundTransport;

/// Трейт исходящего TCP-транспорта.
///
/// Реализации: [`TcpTransport`], [`TlsOutboundTransport`],
/// [`RealityTlsOutboundTransport`], [`OutboundTcpTransport`].
pub trait OutboundTransport: Send + Sync {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send;

    fn connect(
        &self,
        endpoint: &Endpoint,
    ) -> impl std::future::Future<Output = Result<Self::Stream>> + Send;
}

impl OutboundTransport for TcpTransport {
    type Stream = TcpStream;

    async fn connect(&self, endpoint: &Endpoint) -> Result<Self::Stream> {
        TcpTransport::connect(self, endpoint).await
    }
}

impl OutboundTransport for TlsOutboundTransport {
    type Stream = TlsStream<TcpStream>;

    async fn connect(&self, endpoint: &Endpoint) -> Result<Self::Stream> {
        TlsOutboundTransport::connect(self, endpoint).await
    }
}

impl OutboundTransport for RealityTlsOutboundTransport {
    type Stream = TlsStream<TcpStream>;

    async fn connect(&self, endpoint: &Endpoint) -> Result<Self::Stream> {
        RealityTlsOutboundTransport::connect(self, endpoint).await
    }
}

impl OutboundTransport for OutboundTcpTransport {
    type Stream = TcpUpstream;

    async fn connect(&self, endpoint: &Endpoint) -> Result<Self::Stream> {
        OutboundTcpTransport::connect(self, endpoint).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn outbound_transport_implemented_for_plain() {
        fn assert_impl<T: OutboundTransport>() {}
        assert_impl::<TcpTransport>();
        assert_impl::<OutboundTcpTransport>();
        let _ = Duration::from_secs(1);
    }
}
