//! Общие типы и утилиты XHTTP.

use bytes::Bytes;
use http::{header, Response, StatusCode};
use http_body::Body;
use http_body_util::{BodyExt, Full};
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncWrite, ReadBuf};
use tokio::sync::mpsc;

pub type BoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Infallible>;

pub fn extract_meta(request_path: &str, base_path: &str) -> (String, String) {
    let suffix = request_path.strip_prefix(base_path).unwrap_or(request_path);
    let trimmed = suffix.trim_matches('/');
    if trimmed.is_empty() {
        return (String::new(), String::new());
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    let session = parts.first().map(|s| s.to_string()).unwrap_or_default();
    let seq = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
    (session, seq)
}

pub fn padding_valid(headers: &http::HeaderMap) -> bool {
    if let Some(pad) = headers.get("X-Padding") {
        let len = pad.as_bytes().len();
        return (100..=1000).contains(&len);
    }
    if let Some(referer) = headers.get(header::REFERER) {
        let s = referer.to_str().unwrap_or("");
        if let Some(idx) = s.find("x_padding=") {
            let rest = &s[idx + "x_padding=".len()..];
            let len = rest.split('&').next().unwrap_or(rest).len();
            return (100..=1000).contains(&len);
        }
    }
    true
}

pub fn options_response() -> Response<BoxBody> {
    let body = Full::new(Bytes::new()).boxed_unsync();
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    response
}

pub fn error_response(status: StatusCode) -> Response<BoxBody> {
    let body = Full::new(Bytes::new()).boxed_unsync();
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
}

pub struct MpscBody {
    pub rx: mpsc::Receiver<Bytes>,
}

impl Body for MpscBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => Poll::Ready(Some(Ok(http_body::Frame::data(chunk)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Пишет downlink в HTTP response body.
pub struct ChannelWriter {
    tx: mpsc::Sender<Bytes>,
    pending: Vec<u8>,
}

impl ChannelWriter {
    pub fn new(tx: mpsc::Sender<Bytes>) -> Self {
        Self {
            tx,
            pending: Vec::new(),
        }
    }
}

impl AsyncWrite for ChannelWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }

        if !self.pending.is_empty() {
            match self.as_mut().poll_flush(cx) {
                Poll::Ready(Ok(())) => {}
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        }

        let chunk = Bytes::copy_from_slice(buf);
        match self.tx.try_send(chunk) {
            Ok(()) => Poll::Ready(Ok(buf.len())),
            Err(mpsc::error::TrySendError::Full(chunk)) => {
                self.pending.extend_from_slice(&chunk);
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "xhttp response body closed",
            ))),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        while !self.pending.is_empty() {
            let chunk = Bytes::from(self.pending.clone());
            match self.tx.try_send(chunk) {
                Ok(()) => self.pending.clear(),
                Err(mpsc::error::TrySendError::Full(_)) => {
                    cx.waker().wake_by_ref();
                    return Poll::Pending;
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "xhttp response body closed",
                    )));
                }
            }
        }
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

/// Читает uplink из mpsc-очереди (stream-up POST → GET).
pub struct UploadReader {
    rx: mpsc::Receiver<Bytes>,
    buf: Bytes,
}

impl UploadReader {
    pub fn new(rx: mpsc::Receiver<Bytes>) -> Self {
        Self {
            rx,
            buf: Bytes::new(),
        }
    }
}

impl tokio::io::AsyncRead for UploadReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if !self.buf.is_empty() {
            let n = std::cmp::min(buf.remaining(), self.buf.len());
            buf.put_slice(&self.buf[..n]);
            self.buf = self.buf.slice(n..);
            return Poll::Ready(Ok(()));
        }

        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => {
                let n = std::cmp::min(buf.remaining(), chunk.len());
                buf.put_slice(&chunk[..n]);
                if n < chunk.len() {
                    self.buf = chunk.slice(n..);
                }
                Poll::Ready(Ok(()))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}
