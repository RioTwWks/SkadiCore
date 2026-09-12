//! VLESS Mux relay (Xray `common/mux` server worker, TCP/UDP sessions).

use skadi_protocol::vless::mux::{
    encode_data_frame, encode_end_frame, parse_meta_body, MuxError, MuxMeta, NETWORK_TCP,
    NETWORK_UDP, OPTION_DATA, SESSION_STATUS_END, SESSION_STATUS_KEEP, SESSION_STATUS_KEEP_ALIVE,
    SESSION_STATUS_NEW,
};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, warn};

use crate::outbound::{OutboundTcpTransport, TcpUpstream};
use crate::relay::{RelayLimits, IDLE_TIMEOUT_MSG, SESSION_LIFETIME_MSG};
use crate::udp::{UdpTransport, MAX_VLESS_UDP_PAYLOAD};

struct TcpMuxSession {
    upstream: Arc<Mutex<TcpUpstream>>,
    reader_task: JoinHandle<()>,
}

struct UdpMuxSession {
    upstream: Arc<Mutex<UdpSocket>>,
    reader_task: JoinHandle<()>,
}

enum MuxSession {
    Tcp(TcpMuxSession),
    Udp(UdpMuxSession),
}

impl MuxSession {
    fn abort(self) {
        match self {
            Self::Tcp(s) => s.reader_task.abort(),
            Self::Udp(s) => s.reader_task.abort(),
        }
    }
}

struct MuxWriter<W> {
    inner: Arc<Mutex<W>>,
}

impl<W> MuxWriter<W>
where
    W: AsyncWrite + Unpin + Send,
{
    fn new(inner: Arc<Mutex<W>>) -> Self {
        Self { inner }
    }

    async fn write_frame(&self, frame: &[u8]) -> io::Result<()> {
        let mut writer = self.inner.lock().await;
        writer.write_all(frame).await?;
        writer.flush().await?;
        Ok(())
    }

    async fn write_data(&self, session_id: u16, status: u8, payload: &[u8]) -> io::Result<()> {
        let meta = MuxMeta {
            session_id,
            status,
            option: OPTION_DATA,
            network: None,
            target: None,
        };
        let frame = encode_data_frame(&meta, payload)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.write_frame(&frame).await
    }

    async fn write_end(&self, session_id: u16) -> io::Result<()> {
        self.write_frame(&encode_end_frame(session_id, false)).await
    }
}

/// Relay VLESS Mux: мультиплексирование TCP/UDP-сессий поверх одного VLESS-соединения.
pub async fn relay_vless_mux_with_limits<C>(
    client: C,
    outbound_tcp: OutboundTcpTransport,
    outbound_udp: UdpTransport,
    limits: RelayLimits,
) -> io::Result<(u64, u64)>
where
    C: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut reader, writer) = tokio::io::split(client);
    let writer = MuxWriter::new(Arc::new(Mutex::new(writer)));
    let sessions: Arc<Mutex<HashMap<u16, MuxSession>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut client_to_upstream = 0u64;
    let mut upstream_to_client = 0u64;

    let idle_enabled = limits.idle.is_some();
    let lifetime_enabled = limits.max_lifetime.is_some();
    let idle_deadline = tokio::time::sleep(limits.idle.unwrap_or(Duration::MAX));
    tokio::pin!(idle_deadline);
    let lifetime_deadline = tokio::time::sleep(limits.max_lifetime.unwrap_or(Duration::MAX));
    tokio::pin!(lifetime_deadline);

    loop {
        tokio::select! {
            _ = &mut idle_deadline, if idle_enabled => {
                return Err(io::Error::new(io::ErrorKind::TimedOut, IDLE_TIMEOUT_MSG));
            }

            _ = &mut lifetime_deadline, if lifetime_enabled => {
                return Err(io::Error::new(io::ErrorKind::TimedOut, SESSION_LIFETIME_MSG));
            }

            read_result = read_mux_frame(&mut reader) => {
                match read_result? {
                    None => break,
                    Some(frame) => {
                        if idle_enabled {
                            idle_deadline
                                .as_mut()
                                .reset(tokio::time::Instant::now() + limits.idle.unwrap());
                        }

                        let bytes = match handle_mux_frame(
                            &frame,
                            &outbound_tcp,
                            &outbound_udp,
                            &writer,
                            &sessions,
                        ).await {
                            Ok(n) => n,
                            Err(e) => {
                                warn!(error = %e, session = frame.meta.session_id, "mux frame handling failed");
                                return Err(e);
                            }
                        };
                        client_to_upstream += bytes.client_to_upstream;
                        upstream_to_client += bytes.upstream_to_client;
                    }
                }
            }
        }
    }

    {
        let mut guard = sessions.lock().await;
        for (_, session) in guard.drain() {
            session.abort();
        }
    }

    Ok((client_to_upstream, upstream_to_client))
}

struct RelayBytes {
    client_to_upstream: u64,
    upstream_to_client: u64,
}

async fn handle_mux_frame<W>(
    frame: &skadi_protocol::vless::mux::MuxFrame,
    outbound_tcp: &OutboundTcpTransport,
    outbound_udp: &UdpTransport,
    writer: &MuxWriter<W>,
    sessions: &Arc<Mutex<HashMap<u16, MuxSession>>>,
) -> io::Result<RelayBytes>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut bytes = RelayBytes {
        client_to_upstream: 0,
        upstream_to_client: 0,
    };

    match frame.meta.status {
        SESSION_STATUS_NEW => {
            let target = frame.meta.target.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "mux new frame without target")
            })?;
            let network = frame.meta.network.unwrap_or(NETWORK_TCP);

            match network {
                NETWORK_TCP => {
                    open_tcp_session(frame, target, outbound_tcp, writer, sessions, &mut bytes)
                        .await?;
                }
                NETWORK_UDP => {
                    open_udp_session(frame, target, outbound_udp, writer, sessions, &mut bytes)
                        .await?;
                }
                other => {
                    warn!(network = other, "unknown mux network");
                    writer.write_end(frame.meta.session_id).await?;
                }
            }
        }

        SESSION_STATUS_KEEP => {
            forward_keep_frame(frame, writer, sessions, &mut bytes).await?;
        }

        SESSION_STATUS_END => {
            if let Some(session) = sessions.lock().await.remove(&frame.meta.session_id) {
                session.abort();
            }
        }

        SESSION_STATUS_KEEP_ALIVE => {}

        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown mux status: 0x{:02x}", other),
            ));
        }
    }

    Ok(bytes)
}

async fn open_tcp_session<W>(
    frame: &skadi_protocol::vless::mux::MuxFrame,
    target: &skadi_core::Endpoint,
    outbound_tcp: &OutboundTcpTransport,
    writer: &MuxWriter<W>,
    sessions: &Arc<Mutex<HashMap<u16, MuxSession>>>,
    bytes: &mut RelayBytes,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut upstream = outbound_tcp
        .connect(target)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    if let Some(payload) = &frame.payload {
        upstream.write_all(payload).await?;
        bytes.client_to_upstream += payload.len() as u64;
    }

    let upstream = Arc::new(Mutex::new(upstream));
    let session_id = frame.meta.session_id;
    let writer_clone = writer.inner.clone();
    let sessions_clone = Arc::clone(sessions);
    let upstream_for_reader = Arc::clone(&upstream);
    let reader_task = tokio::spawn(async move {
        let mux_writer = MuxWriter::new(writer_clone);
        if let Err(e) =
            pump_tcp_upstream_to_client(session_id, upstream_for_reader, &mux_writer).await
        {
            debug!(session = session_id, error = %e, "mux tcp upstream reader finished");
        }
        sessions_clone.lock().await.remove(&session_id);
    });

    sessions.lock().await.insert(
        session_id,
        MuxSession::Tcp(TcpMuxSession {
            upstream,
            reader_task,
        }),
    );

    debug!(session = session_id, target = %target, network = "tcp", "mux session opened");
    Ok(())
}

async fn open_udp_session<W>(
    frame: &skadi_protocol::vless::mux::MuxFrame,
    target: &skadi_core::Endpoint,
    outbound_udp: &UdpTransport,
    writer: &MuxWriter<W>,
    sessions: &Arc<Mutex<HashMap<u16, MuxSession>>>,
    bytes: &mut RelayBytes,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let upstream = outbound_udp
        .connect(target)
        .await
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

    if let Some(payload) = &frame.payload {
        if !payload.is_empty() {
            upstream.send(payload).await?;
            bytes.client_to_upstream += payload.len() as u64;
        }
    }

    let upstream = Arc::new(Mutex::new(upstream));
    let session_id = frame.meta.session_id;
    let writer_clone = writer.inner.clone();
    let sessions_clone = Arc::clone(sessions);
    let upstream_for_reader = Arc::clone(&upstream);
    let reader_task = tokio::spawn(async move {
        let mux_writer = MuxWriter::new(writer_clone);
        if let Err(e) =
            pump_udp_upstream_to_client(session_id, upstream_for_reader, &mux_writer).await
        {
            debug!(session = session_id, error = %e, "mux udp upstream reader finished");
        }
        sessions_clone.lock().await.remove(&session_id);
    });

    sessions.lock().await.insert(
        session_id,
        MuxSession::Udp(UdpMuxSession {
            upstream,
            reader_task,
        }),
    );

    debug!(session = session_id, target = %target, network = "udp", "mux session opened");
    Ok(())
}

async fn forward_keep_frame<W>(
    frame: &skadi_protocol::vless::mux::MuxFrame,
    writer: &MuxWriter<W>,
    sessions: &Arc<Mutex<HashMap<u16, MuxSession>>>,
    bytes: &mut RelayBytes,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let session_id = frame.meta.session_id;
    let payload = frame.payload.as_deref().filter(|p| !p.is_empty());

    let upstream = {
        let guard = sessions.lock().await;
        match guard.get(&session_id) {
            Some(MuxSession::Tcp(s)) => UpstreamHandle::Tcp(Arc::clone(&s.upstream)),
            Some(MuxSession::Udp(s)) => UpstreamHandle::Udp(Arc::clone(&s.upstream)),
            None => UpstreamHandle::Missing,
        }
    };

    match upstream {
        UpstreamHandle::Missing => {
            writer.write_end(session_id).await?;
        }
        UpstreamHandle::Tcp(upstream) if payload.is_some() => {
            let mut upstream = upstream.lock().await;
            upstream.write_all(payload.unwrap()).await?;
            bytes.client_to_upstream += payload.unwrap().len() as u64;
        }
        UpstreamHandle::Udp(upstream) if payload.is_some() => {
            let upstream = upstream.lock().await;
            upstream.send(payload.unwrap()).await?;
            bytes.client_to_upstream += payload.unwrap().len() as u64;
        }
        _ => {}
    }

    Ok(())
}

enum UpstreamHandle {
    Missing,
    Tcp(Arc<Mutex<TcpUpstream>>),
    Udp(Arc<Mutex<UdpSocket>>),
}

async fn pump_tcp_upstream_to_client<W>(
    session_id: u16,
    upstream: Arc<Mutex<TcpUpstream>>,
    writer: &MuxWriter<W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut buf = [0u8; 8192];

    loop {
        let n = {
            let mut upstream = upstream.lock().await;
            upstream.read(&mut buf).await?
        };

        if n == 0 {
            break;
        }

        writer
            .write_data(session_id, SESSION_STATUS_KEEP, &buf[..n])
            .await?;
    }

    writer.write_end(session_id).await?;
    Ok(())
}

async fn pump_udp_upstream_to_client<W>(
    session_id: u16,
    upstream: Arc<Mutex<UdpSocket>>,
    writer: &MuxWriter<W>,
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut buf = vec![0u8; MAX_VLESS_UDP_PAYLOAD];

    loop {
        let recv = {
            let upstream = upstream.lock().await;
            upstream.recv(&mut buf).await
        };

        match recv {
            Ok(n) => {
                if writer
                    .write_data(session_id, SESSION_STATUS_KEEP, &buf[..n])
                    .await
                    .is_err()
                {
                    break;
                }
            }
            Err(e) => {
                debug!(session = session_id, error = %e, "mux udp recv ended");
                break;
            }
        }
    }

    writer.write_end(session_id).await?;
    Ok(())
}

async fn read_mux_frame<R>(
    reader: &mut R,
) -> io::Result<Option<skadi_protocol::vless::mux::MuxFrame>>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 2];
    if let Err(e) = reader.read_exact(&mut len_buf).await {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            return Ok(None);
        }
        return Err(e);
    }

    let meta_len = u16::from_be_bytes(len_buf) as usize;
    if meta_len > skadi_protocol::vless::mux::MAX_META_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            MuxError::MetaTooLarge(meta_len).to_string(),
        ));
    }

    let mut meta_body = vec![0u8; meta_len];
    reader.read_exact(&mut meta_body).await?;

    let meta = parse_meta_body(&meta_body).map_err(map_mux_error)?;

    let payload = if meta.has_data() {
        let mut chunk_len_buf = [0u8; 2];
        reader.read_exact(&mut chunk_len_buf).await?;
        let chunk_len = u16::from_be_bytes(chunk_len_buf) as usize;
        if chunk_len > skadi_protocol::vless::mux::MAX_CHUNK_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                MuxError::ChunkTooLarge(chunk_len).to_string(),
            ));
        }
        let mut payload = vec![0u8; chunk_len];
        if chunk_len > 0 {
            reader.read_exact(&mut payload).await?;
        }
        Some(payload)
    } else {
        None
    };

    Ok(Some(skadi_protocol::vless::mux::MuxFrame { meta, payload }))
}

fn map_mux_error(err: MuxError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err.to_string())
}
