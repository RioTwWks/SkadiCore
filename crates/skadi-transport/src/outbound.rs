//! Исходящий TCP: plain или TLS.

use anyhow::Result;
use skadi_core::Endpoint;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::tcp::TcpTransport;
use crate::tls_client::{TlsClientConfig, TlsOutboundTransport};

/// Исходящее TCP-соединение (plain или TLS).
pub enum TcpUpstream {
    Plain(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl TcpUpstream {
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        match self {
            Self::Plain(s) => s.local_addr(),
            Self::Tls(s) => s.get_ref().0.local_addr(),
        }
    }
}

impl AsyncRead for TcpUpstream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            TcpUpstream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            TcpUpstream::Tls(s) => Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TcpUpstream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            TcpUpstream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            TcpUpstream::Tls(s) => Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            TcpUpstream::Plain(s) => Pin::new(s).poll_flush(cx),
            TcpUpstream::Tls(s) => Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            TcpUpstream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            TcpUpstream::Tls(s) => Pin::new(s).poll_shutdown(cx),
        }
    }
}

/// Исходящий TCP-транспорт: plain или TLS (в зависимости от конфига).
#[derive(Clone)]
pub enum OutboundTcpTransport {
    Plain(TcpTransport),
    Tls(TlsOutboundTransport),
}

impl OutboundTcpTransport {
    pub fn plain(connect_timeout: Duration) -> Self {
        Self::plain_with_policy(connect_timeout, true)
    }

    pub fn plain_with_policy(connect_timeout: Duration, allow_private: bool) -> Self {
        Self::Plain(TcpTransport::with_policy(connect_timeout, allow_private))
    }

    pub fn tls(connect_timeout: Duration, config: &TlsClientConfig) -> Result<Self> {
        Self::tls_with_policy(connect_timeout, config, true)
    }

    pub fn tls_with_policy(
        connect_timeout: Duration,
        config: &TlsClientConfig,
        allow_private: bool,
    ) -> Result<Self> {
        Ok(Self::Tls(TlsOutboundTransport::with_policy(
            connect_timeout,
            config,
            allow_private,
        )?))
    }

    pub async fn connect(&self, endpoint: &Endpoint) -> Result<TcpUpstream> {
        match self {
            Self::Plain(tcp) => Ok(TcpUpstream::Plain(tcp.connect(endpoint).await?)),
            Self::Tls(tls) => Ok(TcpUpstream::Tls(tls.connect(endpoint).await?)),
        }
    }
}
