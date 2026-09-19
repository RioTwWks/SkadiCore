//! XHTTP (SplitHTTP) client: stream-one, stream-up, packet-up.

use crate::xhttp::config::{PaddingRange, XhttpMode};
use bytes::Bytes;
use rand::RngCore;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::sync::mpsc;

/// Ошибки XHTTP client handshake / I/O.
#[derive(Debug, Error)]
pub enum XhttpClientError {
    #[error("XHTTP handshake failed: {0}")]
    Handshake(String),
    #[error("unexpected HTTP status {0}")]
    BadStatus(u16),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Runtime-конфиг XHTTP outbound.
#[derive(Debug, Clone)]
pub struct XhttpClientConfig {
    pub path: String,
    pub host: String,
    pub mode: XhttpMode,
    pub padding: PaddingRange,
}

impl XhttpClientConfig {
    pub fn normalized_path(&self) -> String {
        let path = self.path.split('?').next().unwrap_or(&self.path);
        if path.is_empty() || !path.starts_with('/') {
            format!("/{}", path.trim_start_matches('/'))
        } else {
            path.to_string()
        }
    }

    /// Клиентский `auto` → stream-one (как типичный Xray REALITY+XHTTP).
    pub fn resolved_mode(&self) -> XhttpMode {
        match self.mode {
            XhttpMode::Auto => XhttpMode::StreamOne,
            other => other,
        }
    }
}

/// Дуплекс после XHTTP upgrade (для VLESS поверх HTTP).
pub struct XhttpClientIo {
    reader: Pin<Box<dyn AsyncRead + Send>>,
    writer: Pin<Box<dyn AsyncWrite + Send>>,
}

impl AsyncRead for XhttpClientIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for XhttpClientIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.writer).poll_shutdown(cx)
    }
}

/// Установить XHTTP поверх уже открытого dialer'а (TCP / TLS / REALITY).
pub async fn connect_xhttp<S, E, F, Fut>(
    mut dial: F,
    cfg: &XhttpClientConfig,
) -> Result<XhttpClientIo, XhttpClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: std::fmt::Display,
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = Result<S, E>> + Send,
{
    match cfg.resolved_mode() {
        XhttpMode::StreamOne => {
            let stream = dial()
                .await
                .map_err(|e| XhttpClientError::Handshake(format!("dial failed: {e}")))?;
            connect_stream_one(stream, cfg).await
        }
        XhttpMode::StreamUp => {
            let down = dial()
                .await
                .map_err(|e| XhttpClientError::Handshake(format!("dial downlink failed: {e}")))?;
            let up = dial()
                .await
                .map_err(|e| XhttpClientError::Handshake(format!("dial uplink failed: {e}")))?;
            connect_stream_up(down, up, cfg).await
        }
        XhttpMode::PacketUp => connect_packet_up(dial, cfg).await,
        XhttpMode::Auto => unreachable!("resolved_mode maps Auto"),
    }
}

/// stream-one: один POST с chunked body = uplink, response body = downlink.
pub async fn connect_stream_one<S>(
    mut stream: S,
    cfg: &XhttpClientConfig,
) -> Result<XhttpClientIo, XhttpClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let path = cfg.normalized_path();
    let headers = build_post_headers(&path, &cfg.host, cfg.padding, true);
    stream.write_all(headers.as_bytes()).await?;
    stream.flush().await?;

    let leftover = read_status_ok(&mut stream).await?;
    let (reader, writer) = tokio::io::split(stream);
    Ok(XhttpClientIo {
        reader: Box::pin(ChunkedDecoder::new(reader, leftover)),
        writer: Box::pin(ChunkedEncoder::new(writer)),
    })
}

/// stream-up: GET downlink + POST uplink (два соединения, общий session id).
pub async fn connect_stream_up<S>(
    mut down: S,
    mut up: S,
    cfg: &XhttpClientConfig,
) -> Result<XhttpClientIo, XhttpClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let path = cfg.normalized_path();
    let session = random_session_id();
    let get_path = format!("{path}/{session}");
    let post_path = get_path.clone();

    let get_req = format!(
        "GET {get_path} HTTP/1.1\r\nHost: {host}\r\n{pad}\r\n",
        host = cfg.host,
        pad = padding_header_line(cfg.padding),
    );
    down.write_all(get_req.as_bytes()).await?;
    down.flush().await?;
    let down_leftover = read_status_ok(&mut down).await?;

    let post_headers = build_post_headers(&post_path, &cfg.host, cfg.padding, true);
    up.write_all(post_headers.as_bytes()).await?;
    up.flush().await?;
    // POST response (empty 200) may arrive after body starts; drain headers first.
    let _ = read_status_ok(&mut up).await?;

    let (down_r, _down_w) = tokio::io::split(down);
    let (_up_r, up_w) = tokio::io::split(up);

    Ok(XhttpClientIo {
        reader: Box::pin(ChunkedDecoder::new(down_r, down_leftover)),
        writer: Box::pin(ChunkedEncoder::new(up_w)),
    })
}

/// packet-up: GET downlink + sequenced POST uplink (новое соединение на каждый write).
pub async fn connect_packet_up<S, E, F, Fut>(
    mut dial: F,
    cfg: &XhttpClientConfig,
) -> Result<XhttpClientIo, XhttpClientError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    E: std::fmt::Display,
    F: FnMut() -> Fut + Send + 'static,
    Fut: Future<Output = Result<S, E>> + Send,
{
    let path = cfg.normalized_path();
    let session = random_session_id();
    let get_path = format!("{path}/{session}");

    let mut down = dial()
        .await
        .map_err(|e| XhttpClientError::Handshake(format!("dial downlink failed: {e}")))?;
    let get_req = format!(
        "GET {get_path} HTTP/1.1\r\nHost: {host}\r\n{pad}\r\n",
        host = cfg.host,
        pad = padding_header_line(cfg.padding),
    );
    down.write_all(get_req.as_bytes()).await?;
    down.flush().await?;
    let down_leftover = read_status_ok(&mut down).await?;

    let (down_r, _down_w) = tokio::io::split(down);
    let (tx, rx) = mpsc::channel::<Bytes>(64);
    let seq = Arc::new(AtomicU64::new(0));
    let host = cfg.host.clone();
    let padding = cfg.padding;
    let base_path = path;
    let session_id = session;

    tokio::spawn(async move {
        let mut rx = rx;
        while let Some(payload) = rx.recv().await {
            let n = seq.fetch_add(1, Ordering::Relaxed);
            let post_path = format!("{base_path}/{session_id}/{n}");
            let result = async {
                let mut stream = dial().await.map_err(|e| {
                    io::Error::other(format!("packet-up dial failed: {e}"))
                })?;
                let headers = format!(
                    "POST {post_path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {len}\r\n{pad}\r\n",
                    len = payload.len(),
                    pad = padding_header_line(padding),
                );
                stream.write_all(headers.as_bytes()).await?;
                stream.write_all(&payload).await?;
                stream.flush().await?;
                read_status_ok(&mut stream).await?;
                let _ = stream.shutdown().await;
                Ok::<(), XhttpClientError>(())
            }
            .await;
            if let Err(e) = result {
                tracing::debug!(error = %e, seq = n, "xhttp packet-up POST failed");
                break;
            }
        }
    });

    Ok(XhttpClientIo {
        reader: Box::pin(ChunkedDecoder::new(down_r, down_leftover)),
        writer: Box::pin(PacketUpWriter { tx, pending: None }),
    })
}

fn build_post_headers(path: &str, host: &str, padding: PaddingRange, chunked: bool) -> String {
    let te = if chunked {
        "Transfer-Encoding: chunked\r\n"
    } else {
        ""
    };
    format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/grpc\r\n{te}{pad}\r\n",
        pad = padding_header_line(padding),
    )
}

fn padding_header_line(padding: PaddingRange) -> String {
    let len = padding.sample_len();
    if len == 0 {
        return String::new();
    }
    format!("X-Padding: {}\r\n", "X".repeat(len))
}

fn random_session_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

async fn read_status_ok<S: AsyncRead + Unpin>(stream: &mut S) -> Result<Vec<u8>, XhttpClientError> {
    let (status, leftover) = read_http_headers(stream).await?;
    if status != 200 {
        return Err(XhttpClientError::BadStatus(status));
    }
    Ok(leftover)
}

async fn read_http_headers<S: AsyncRead + Unpin>(
    stream: &mut S,
) -> Result<(u16, Vec<u8>), XhttpClientError> {
    let mut buf = Vec::with_capacity(512);
    let mut tmp = [0u8; 256];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            return Err(XhttpClientError::Handshake(
                "connection closed before HTTP headers".into(),
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = find_header_end(&buf) {
            let header_bytes = &buf[..pos];
            let leftover = buf[pos..].to_vec();
            let status = parse_status_code(header_bytes)?;
            return Ok((status, leftover));
        }
        if buf.len() > 64 * 1024 {
            return Err(XhttpClientError::Handshake("HTTP headers too large".into()));
        }
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn parse_status_code(headers: &[u8]) -> Result<u16, XhttpClientError> {
    let text = std::str::from_utf8(headers)
        .map_err(|_| XhttpClientError::Handshake("HTTP status line is not UTF-8".into()))?;
    let line = text.lines().next().unwrap_or("");
    let code = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| XhttpClientError::Handshake(format!("bad status line: {line}")))?;
    Ok(code)
}

/// HTTP/1.1 chunked body writer.
struct ChunkedEncoder<W> {
    inner: W,
    pending: Vec<u8>,
    pending_report: usize,
    closing: bool,
    closed: bool,
}

impl<W: AsyncWrite + Unpin> ChunkedEncoder<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            pending: Vec::new(),
            pending_report: 0,
            closing: false,
            closed: false,
        }
    }

    fn poll_write_pending(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while !self.pending.is_empty() {
            match Pin::new(&mut self.inner).poll_write(cx, &self.pending) {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "xhttp chunked write zero",
                    )));
                }
                Poll::Ready(Ok(n)) => {
                    self.pending.drain(..n);
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        Poll::Ready(Ok(()))
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for ChunkedEncoder<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.closed || self.closing {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "xhttp chunked writer closed",
            )));
        }
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.pending.is_empty() {
            let mut framed = Vec::with_capacity(buf.len() + 16);
            use std::io::Write as _;
            let _ = write!(&mut framed, "{:x}\r\n", buf.len());
            framed.extend_from_slice(buf);
            framed.extend_from_slice(b"\r\n");
            self.pending = framed;
            self.pending_report = buf.len();
        }
        match self.poll_write_pending(cx) {
            Poll::Ready(Ok(())) => {
                let n = self.pending_report;
                self.pending_report = 0;
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.poll_write_pending(cx) {
            Poll::Ready(Ok(())) => Pin::new(&mut self.inner).poll_flush(cx),
            other => other,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if self.closed {
            return Pin::new(&mut self.inner).poll_shutdown(cx);
        }
        if !self.closing {
            match self.poll_write_pending(cx) {
                Poll::Ready(Ok(())) => {
                    self.pending = b"0\r\n\r\n".to_vec();
                    self.pending_report = 0;
                    self.closing = true;
                }
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }
        match self.poll_write_pending(cx) {
            Poll::Ready(Ok(())) => {
                self.closed = true;
                Pin::new(&mut self.inner).poll_shutdown(cx)
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// HTTP/1.1 chunked body reader.
struct ChunkedDecoder<R> {
    inner: R,
    leftover: Vec<u8>,
    chunk_remaining: usize,
    state: ChunkState,
    eof: bool,
}

#[derive(Clone, Copy)]
enum ChunkState {
    SizeLine,
    Data,
    DataCrlf,
}

impl<R> ChunkedDecoder<R> {
    fn new(inner: R, leftover: Vec<u8>) -> Self {
        Self {
            inner,
            leftover,
            chunk_remaining: 0,
            state: ChunkState::SizeLine,
            eof: false,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ChunkedDecoder<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.eof {
            return Poll::Ready(Ok(()));
        }

        loop {
            match self.state {
                ChunkState::SizeLine => {
                    if let Some(idx) = self.leftover.windows(2).position(|w| w == b"\r\n") {
                        let line = self.leftover[..idx].to_vec();
                        self.leftover.drain(..idx + 2);
                        let line = std::str::from_utf8(&line)
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                        let size_hex = line.split(';').next().unwrap_or(line).trim();
                        let size = usize::from_str_radix(size_hex, 16)
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                        if size == 0 {
                            self.eof = true;
                            return Poll::Ready(Ok(()));
                        }
                        self.chunk_remaining = size;
                        self.state = ChunkState::Data;
                        continue;
                    }
                    // Need more bytes for size line.
                    let mut tmp = [0u8; 128];
                    let mut read_buf = ReadBuf::new(&mut tmp);
                    match Pin::new(&mut self.inner).poll_read(cx, &mut read_buf) {
                        Poll::Ready(Ok(())) => {
                            let n = read_buf.filled().len();
                            if n == 0 {
                                self.eof = true;
                                return Poll::Ready(Ok(()));
                            }
                            self.leftover.extend_from_slice(read_buf.filled());
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                ChunkState::Data => {
                    if self.chunk_remaining == 0 {
                        self.state = ChunkState::DataCrlf;
                        continue;
                    }
                    if !self.leftover.is_empty() {
                        let take = self
                            .chunk_remaining
                            .min(buf.remaining())
                            .min(self.leftover.len());
                        if take > 0 {
                            buf.put_slice(&self.leftover[..take]);
                            self.leftover.drain(..take);
                            self.chunk_remaining -= take;
                            return Poll::Ready(Ok(()));
                        }
                    }
                    let want = self.chunk_remaining.min(buf.remaining());
                    if want == 0 {
                        return Poll::Ready(Ok(()));
                    }
                    let mut tmp = vec![0u8; want];
                    let mut read_buf = ReadBuf::new(&mut tmp);
                    match Pin::new(&mut self.inner).poll_read(cx, &mut read_buf) {
                        Poll::Ready(Ok(())) => {
                            let n = read_buf.filled().len();
                            if n == 0 {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    "eof mid chunk",
                                )));
                            }
                            buf.put_slice(read_buf.filled());
                            self.chunk_remaining -= n;
                            return Poll::Ready(Ok(()));
                        }
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                        Poll::Pending => return Poll::Pending,
                    }
                }
                ChunkState::DataCrlf => {
                    while self.leftover.len() < 2 {
                        let mut tmp = [0u8; 2];
                        let mut read_buf = ReadBuf::new(&mut tmp);
                        match Pin::new(&mut self.inner).poll_read(cx, &mut read_buf) {
                            Poll::Ready(Ok(())) => {
                                let n = read_buf.filled().len();
                                if n == 0 {
                                    return Poll::Ready(Err(io::Error::new(
                                        io::ErrorKind::UnexpectedEof,
                                        "eof after chunk",
                                    )));
                                }
                                self.leftover.extend_from_slice(read_buf.filled());
                            }
                            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                            Poll::Pending => return Poll::Pending,
                        }
                    }
                    if &self.leftover[..2] != b"\r\n" {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "missing CRLF after chunk",
                        )));
                    }
                    self.leftover.drain(..2);
                    self.state = ChunkState::SizeLine;
                }
            }
        }
    }
}

struct PacketUpWriter {
    tx: mpsc::Sender<Bytes>,
    pending: Option<Bytes>,
}

impl AsyncWrite for PacketUpWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if self.pending.is_none() {
            self.pending = Some(Bytes::copy_from_slice(buf));
        }
        let chunk = self.pending.as_ref().unwrap().clone();
        match self.tx.try_send(chunk) {
            Ok(()) => {
                let len = self.pending.take().unwrap().len();
                Poll::Ready(Ok(len))
            }
            Err(mpsc::error::TrySendError::Full(chunk)) => {
                self.pending = Some(chunk);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "xhttp packet-up writer closed",
            ))),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Dropping `self` (and thus `tx`) signals the POST worker to exit.
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xhttp::config::XhttpMode;

    #[test]
    fn resolved_auto_is_stream_one() {
        let cfg = XhttpClientConfig {
            path: "/xhttp".into(),
            host: "localhost".into(),
            mode: XhttpMode::Auto,
            padding: PaddingRange::default(),
        };
        assert_eq!(cfg.resolved_mode(), XhttpMode::StreamOne);
    }

    #[test]
    fn parse_status() {
        let h = b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\n\r\n";
        assert_eq!(parse_status_code(h).unwrap(), 200);
    }
}
