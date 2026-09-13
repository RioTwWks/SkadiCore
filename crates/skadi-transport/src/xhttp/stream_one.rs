//! XHTTP stream-one: bidirectional HTTP body stream (Xray splithttp).

use crate::xhttp::config::{PaddingRange, XhttpConfig, XhttpMode};
use bytes::Bytes;
use futures_util::TryStreamExt;
use http::{header, Method, Request, Response, StatusCode};
use http_body::Body;
use http_body_util::{BodyExt, Full};
use hyper::body::{Frame, Incoming};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{mpsc, oneshot};
use tokio_util::io::StreamReader;

type BoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Infallible>;

/// Ошибки XHTTP upgrade.
#[derive(Debug, Error)]
pub enum XhttpError {
    #[error("HTTP serve failed: {0}")]
    Serve(String),
    #[error("client disconnected before stream-one upgrade")]
    Disconnected,
}

/// Дуплекс-поток после успешного XHTTP stream-one handshake.
pub struct StreamOneIo {
    reader: Pin<Box<dyn AsyncRead + Send>>,
    writer: ChannelWriter,
}

impl AsyncRead for StreamOneIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for StreamOneIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.writer).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.writer).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        self.poll_flush(cx)
    }
}

/// Принять одно XHTTP stream-one соединение поверх TLS/REALITY/plain TCP.
pub async fn accept_stream_one<S>(stream: S, config: XhttpConfig) -> Result<StreamOneIo, XhttpError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let upgrade_tx = Arc::new(Mutex::new(Some(tx)));

    let path = config.normalized_path();
    let host = config.host.clone();
    let mode = config.mode;
    let padding = config.padding;
    let no_sse = config.no_sse_header;

    let upgrade_for_service = Arc::clone(&upgrade_tx);
    let service = service_fn(move |req: Request<Incoming>| {
        let path = path.clone();
        let host = host.clone();
        let upgrade_tx = Arc::clone(&upgrade_for_service);
        async move { handle_stream_one(req, &path, &host, mode, padding, no_sse, upgrade_tx).await }
    });

    let io = TokioIo::new(stream);
    let conn = hyper::server::conn::http1::Builder::new()
        .keep_alive(false)
        .serve_connection(io, service);

    tokio::spawn(async move {
        if let Err(e) = conn.await {
            tracing::debug!(error = %e, "xhttp connection closed");
        }
    });

    rx.await.map_err(|_| XhttpError::Disconnected)
}

async fn handle_stream_one(
    req: Request<Incoming>,
    path: &str,
    host: &Option<String>,
    mode: XhttpMode,
    padding: PaddingRange,
    no_sse: bool,
    upgrade_tx: Arc<Mutex<Option<oneshot::Sender<StreamOneIo>>>>,
) -> Result<Response<BoxBody>, Infallible> {
    if req.method() == Method::OPTIONS {
        return Ok(options_response());
    }

    if !req.uri().path().starts_with(path) {
        return Ok(error_response(StatusCode::NOT_FOUND));
    }

    let request_host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if let Some(expected) = host {
        if !expected.is_empty() {
            let ok = request_host.eq_ignore_ascii_case(expected)
                || request_host
                    .split(':')
                    .next()
                    .is_some_and(|h| h.eq_ignore_ascii_case(expected));
            if !ok {
                return Ok(error_response(StatusCode::NOT_FOUND));
            }
        }
    }

    let (session_id, _seq) = extract_meta(req.uri().path(), path);
    if !session_id.is_empty() {
        return Ok(error_response(StatusCode::BAD_REQUEST));
    }

    if !mode.allows_stream_one() {
        return Ok(error_response(StatusCode::BAD_REQUEST));
    }

    if !padding_valid(req.headers()) {
        return Ok(error_response(StatusCode::BAD_REQUEST));
    }

    let (body_tx, body_rx) = mpsc::channel::<Bytes>(64);
    let body_stream = req.into_body().into_data_stream();
    let reader: Pin<Box<dyn AsyncRead + Send>> = Box::pin(StreamReader::new(
        body_stream.map_err(std::io::Error::other),
    ));
    let writer = ChannelWriter::new(body_tx);

    let upgraded = StreamOneIo { reader, writer };
    if let Some(tx) = upgrade_tx.lock().unwrap().take() {
        if tx.send(upgraded).is_err() {
            return Ok(error_response(StatusCode::INTERNAL_SERVER_ERROR));
        }
    } else {
        return Ok(error_response(StatusCode::CONFLICT));
    }

    let mut response = Response::new(MpscBody { rx: body_rx }.boxed_unsync());
    let headers = response.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    headers.insert("X-Accel-Buffering", "no".parse().unwrap());
    headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    if !no_sse {
        headers.insert(header::CONTENT_TYPE, "text/event-stream".parse().unwrap());
    }
    let pad_len = padding.sample_len();
    if pad_len > 0 {
        headers.insert(
            "X-Padding",
            http::HeaderValue::from_bytes(&vec![b'X'; pad_len])
                .unwrap_or_else(|_| http::HeaderValue::from_static("X")),
        );
    }
    *response.status_mut() = StatusCode::OK;

    Ok(response)
}

fn extract_meta(request_path: &str, base_path: &str) -> (String, String) {
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

fn padding_valid(headers: &http::HeaderMap) -> bool {
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

fn options_response() -> Response<BoxBody> {
    let body = Full::new(Bytes::new()).boxed_unsync();
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    response
}

fn error_response(status: StatusCode) -> Response<BoxBody> {
    let body = Full::new(Bytes::new()).boxed_unsync();
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
}

struct MpscBody {
    rx: mpsc::Receiver<Bytes>,
}

impl Body for MpscBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(chunk)) => Poll::Ready(Some(Ok(Frame::data(chunk)))),
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
    fn new(tx: mpsc::Sender<Bytes>) -> Self {
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
