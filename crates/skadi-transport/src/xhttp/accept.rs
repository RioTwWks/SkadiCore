//! XHTTP accept: stream-one и stream-up.

use crate::xhttp::common::{self, BoxBody, ChannelWriter, MpscBody, UploadReader};
use crate::xhttp::config::{PaddingRange, XhttpConfig, XhttpMode};
use crate::xhttp::session::{XhttpSessionManager, DEFAULT_MAX_POST_BYTES};
use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use http::{header, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
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

/// Ошибки XHTTP upgrade.
#[derive(Debug, Error)]
pub enum XhttpError {
    #[error("HTTP serve failed: {0}")]
    Serve(String),
    #[error("client disconnected before XHTTP upgrade")]
    Disconnected,
}

/// Результат accept: либо duplex для протокола, либо POST-only (stream-up).
pub enum XhttpAcceptResult {
    Upgraded(XhttpIo),
    PostHandled,
}

/// Дуплекс-поток после успешного XHTTP handshake.
pub struct XhttpIo {
    reader: Pin<Box<dyn AsyncRead + Send>>,
    writer: ChannelWriter,
}

impl AsyncRead for XhttpIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.reader).poll_read(cx, buf)
    }
}

impl AsyncWrite for XhttpIo {
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

/// Принять XHTTP-соединение (stream-one или stream-up GET).
pub async fn accept_xhttp<S>(
    stream: S,
    config: XhttpConfig,
    sessions: Arc<XhttpSessionManager>,
) -> Result<XhttpAcceptResult, XhttpError>
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
        let sessions = Arc::clone(&sessions);
        async move {
            handle_request(
                req, &path, &host, mode, padding, no_sse, upgrade_tx, sessions,
            )
            .await
        }
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

    match rx.await {
        Ok(XhttpAcceptResult::Upgraded(io)) => Ok(XhttpAcceptResult::Upgraded(io)),
        Ok(XhttpAcceptResult::PostHandled) => Ok(XhttpAcceptResult::PostHandled),
        Err(_) => Err(XhttpError::Disconnected),
    }
}

/// Обратная совместимость: stream-one only.
pub async fn accept_stream_one<S>(stream: S, config: XhttpConfig) -> Result<XhttpIo, XhttpError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let sessions = Arc::new(XhttpSessionManager::new());
    match accept_xhttp(stream, config, sessions).await? {
        XhttpAcceptResult::Upgraded(io) => Ok(io),
        XhttpAcceptResult::PostHandled => Err(XhttpError::Disconnected),
    }
}

async fn handle_request(
    req: Request<Incoming>,
    path: &str,
    host: &Option<String>,
    mode: XhttpMode,
    padding: PaddingRange,
    no_sse: bool,
    upgrade_tx: Arc<Mutex<Option<oneshot::Sender<XhttpAcceptResult>>>>,
    sessions: Arc<XhttpSessionManager>,
) -> Result<Response<BoxBody>, Infallible> {
    if req.method() == Method::OPTIONS {
        return Ok(common::options_response());
    }

    if !req.uri().path().starts_with(path) {
        return Ok(common::error_response(StatusCode::NOT_FOUND));
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
                return Ok(common::error_response(StatusCode::NOT_FOUND));
            }
        }
    }

    if !common::padding_valid(req.headers()) {
        return Ok(common::error_response(StatusCode::BAD_REQUEST));
    }

    let (session_id, seq) = common::extract_meta(req.uri().path(), path);

    if session_id.is_empty() {
        return handle_stream_one(req, mode, padding, no_sse, upgrade_tx).await;
    }

    if !seq.is_empty() {
        return handle_packet_up(
            req, mode, padding, no_sse, upgrade_tx, sessions, session_id, seq,
        )
        .await;
    }

    handle_stream_up(req, mode, padding, no_sse, upgrade_tx, sessions, session_id).await
}

async fn handle_stream_one(
    req: Request<Incoming>,
    mode: XhttpMode,
    padding: PaddingRange,
    no_sse: bool,
    upgrade_tx: Arc<Mutex<Option<oneshot::Sender<XhttpAcceptResult>>>>,
) -> Result<Response<BoxBody>, Infallible> {
    if !mode.allows_stream_one() {
        return Ok(common::error_response(StatusCode::BAD_REQUEST));
    }

    let (body_tx, body_rx) = mpsc::channel::<Bytes>(64);
    let body_stream = req.into_body().into_data_stream();
    let reader: Pin<Box<dyn AsyncRead + Send>> = Box::pin(StreamReader::new(
        body_stream.map_err(std::io::Error::other),
    ));
    let writer = ChannelWriter::new(body_tx);
    let upgraded = XhttpIo { reader, writer };

    send_upgrade(upgrade_tx, XhttpAcceptResult::Upgraded(upgraded));
    Ok(streaming_response(body_rx, padding, no_sse))
}

async fn handle_packet_up(
    req: Request<Incoming>,
    mode: XhttpMode,
    _padding: PaddingRange,
    _no_sse: bool,
    upgrade_tx: Arc<Mutex<Option<oneshot::Sender<XhttpAcceptResult>>>>,
    sessions: Arc<XhttpSessionManager>,
    session_id: String,
    seq_str: String,
) -> Result<Response<BoxBody>, Infallible> {
    if !mode.allows_packet_up() {
        return Ok(common::error_response(StatusCode::BAD_REQUEST));
    }

    if *req.method() != Method::POST {
        return Ok(common::error_response(StatusCode::METHOD_NOT_ALLOWED));
    }

    let seq = match seq_str.parse::<u64>() {
        Ok(n) => n,
        Err(_) => return Ok(common::error_response(StatusCode::BAD_REQUEST)),
    };

    let body = req.into_body();
    let payload = match read_body_limited(body, DEFAULT_MAX_POST_BYTES).await {
        Ok(p) => p,
        Err(status) => return Ok(common::error_response(status)),
    };

    let session = sessions.get_or_create(&session_id);
    session.push_packet(seq, payload);

    send_upgrade(upgrade_tx, XhttpAcceptResult::PostHandled);
    let mut response = Response::new(Full::new(Bytes::new()).boxed_unsync());
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    Ok(response)
}

async fn read_body_limited(body: Incoming, max_bytes: usize) -> Result<Bytes, StatusCode> {
    let mut stream = body.into_data_stream();
    let mut out = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| StatusCode::BAD_REQUEST)?;
        if out.len() + chunk.len() > max_bytes {
            return Err(StatusCode::from_u16(413).unwrap_or(StatusCode::BAD_REQUEST));
        }
        out.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(out))
}

async fn handle_stream_up(
    req: Request<Incoming>,
    mode: XhttpMode,
    padding: PaddingRange,
    no_sse: bool,
    upgrade_tx: Arc<Mutex<Option<oneshot::Sender<XhttpAcceptResult>>>>,
    sessions: Arc<XhttpSessionManager>,
    session_id: String,
) -> Result<Response<BoxBody>, Infallible> {
    let session = sessions.get_or_create(&session_id);

    match *req.method() {
        Method::GET => {
            if !mode.allows_stream_up() && !mode.allows_packet_up() {
                return Ok(common::error_response(StatusCode::BAD_REQUEST));
            }
            let upload_rx = session
                .take_upload_rx()
                .ok_or(())
                .map_err(|_| StatusCode::CONFLICT);
            let upload_rx = match upload_rx {
                Ok(rx) => rx,
                Err(status) => return Ok(common::error_response(status)),
            };

            let (body_tx, body_rx) = mpsc::channel::<Bytes>(64);
            let reader: Pin<Box<dyn AsyncRead + Send>> = Box::pin(UploadReader::new(upload_rx));
            let writer = ChannelWriter::new(body_tx);
            let upgraded = XhttpIo { reader, writer };

            send_upgrade(upgrade_tx, XhttpAcceptResult::Upgraded(upgraded));
            Ok(streaming_response(body_rx, padding, no_sse))
        }
        Method::POST => {
            if !mode.allows_stream_up() {
                return Ok(common::error_response(StatusCode::BAD_REQUEST));
            }
            let body = req.into_body();
            let sessions = Arc::clone(&sessions);
            let sid = session_id.clone();
            tokio::spawn(async move {
                session.attach_post_body(body).await;
                sessions.remove(&sid);
            });
            send_upgrade(upgrade_tx, XhttpAcceptResult::PostHandled);
            let mut response = Response::new(Full::new(Bytes::new()).boxed_unsync());
            *response.status_mut() = StatusCode::OK;
            response
                .headers_mut()
                .insert("X-Accel-Buffering", "no".parse().unwrap());
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            Ok(response)
        }
        _ => Ok(common::error_response(StatusCode::METHOD_NOT_ALLOWED)),
    }
}

fn send_upgrade(
    upgrade_tx: Arc<Mutex<Option<oneshot::Sender<XhttpAcceptResult>>>>,
    result: XhttpAcceptResult,
) {
    if let Some(tx) = upgrade_tx.lock().unwrap().take() {
        let _ = tx.send(result);
    }
}

fn streaming_response(
    body_rx: mpsc::Receiver<Bytes>,
    padding: PaddingRange,
    no_sse: bool,
) -> Response<BoxBody> {
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
    response
}
