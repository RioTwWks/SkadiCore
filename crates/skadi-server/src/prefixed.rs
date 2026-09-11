//! Обёртка над TcpStream, возвращающая один «заглянутый» байт при первом чтении.
//! Нужна для дискриминации протокола на общем порту (SOCKS5 vs VLESS).

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;

/// TCP-поток с опциональным префиксным байтом (уже прочитанным при sniffing).
pub struct PrefixedTcpStream {
    inner: TcpStream,
    prefix: Option<u8>,
}

impl PrefixedTcpStream {
    pub fn new(inner: TcpStream, prefix: Option<u8>) -> Self {
        Self { inner, prefix }
    }
}

impl AsyncRead for PrefixedTcpStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Some(byte) = this.prefix {
            if buf.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }
            buf.put_slice(&[byte]);
            this.prefix = None;
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedTcpStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
