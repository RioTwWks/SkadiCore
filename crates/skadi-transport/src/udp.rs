//! UDP-транспорт и VLESS UDP relay (length-prefixed framing).

use anyhow::{Context, Result};
use skadi_core::Endpoint;
use std::io;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::UdpSocket;
use tracing::debug;

use crate::relay::{RelayLimits, IDLE_TIMEOUT_MSG, SESSION_LIFETIME_MSG};

/// Максимальный размер UDP payload в VLESS framing (u16).
pub const MAX_VLESS_UDP_PAYLOAD: usize = u16::MAX as usize;

/// Транспорт поверх UDP.
#[derive(Clone)]
pub struct UdpTransport {
    connect_timeout: Duration,
    allow_private: bool,
}

impl UdpTransport {
    pub fn new(connect_timeout: Duration) -> Self {
        Self::with_policy(connect_timeout, true)
    }

    pub fn with_policy(connect_timeout: Duration, allow_private: bool) -> Self {
        Self {
            connect_timeout,
            allow_private,
        }
    }

    pub fn allow_private(&self) -> bool {
        self.allow_private
    }

    /// Установить исходящее UDP-соединение (connected socket).
    pub async fn connect(&self, endpoint: &Endpoint) -> Result<UdpSocket> {
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("failed to bind UDP socket")?;

        let target = crate::outbound_policy::resolve_endpoint(endpoint, self.allow_private).await?;
        debug!(target = %target, "UDP connecting");

        tokio::time::timeout(self.connect_timeout, socket.connect(target))
            .await
            .map_err(|_| anyhow::anyhow!("UDP connect timeout to {}", target))?
            .with_context(|| format!("UDP connect failed to {}", target))?;

        Ok(socket)
    }
}

/// Relay VLESS UDP: length-prefixed кадры (2-byte BE) в обе стороны.
pub async fn relay_vless_udp_with_limits<C>(
    client: &mut C,
    upstream: &mut UdpSocket,
    limits: RelayLimits,
) -> io::Result<(u64, u64)>
where
    C: AsyncRead + AsyncWrite + Unpin,
{
    let mut client_to_upstream = 0u64;
    let mut upstream_to_client = 0u64;
    let mut udp_buf = vec![0u8; MAX_VLESS_UDP_PAYLOAD];

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

            read_result = read_vless_udp_frame(client) => {
                match read_result {
                    Ok(None) => return Ok((client_to_upstream, upstream_to_client)),
                    Ok(Some(payload)) => {
                        upstream.send(&payload).await?;
                        client_to_upstream += payload.len() as u64;
                        if idle_enabled {
                            idle_deadline
                                .as_mut()
                                .reset(tokio::time::Instant::now() + limits.idle.unwrap());
                        }
                    }
                    Err(e) => return Err(e),
                }
            }

            recv_result = upstream.recv(&mut udp_buf) => {
                match recv_result {
                    Ok(0) => continue,
                    Ok(n) => {
                        write_vless_udp_frame(client, &udp_buf[..n]).await?;
                        upstream_to_client += n as u64;
                        if idle_enabled {
                            idle_deadline
                                .as_mut()
                                .reset(tokio::time::Instant::now() + limits.idle.unwrap());
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
}

pub async fn read_vless_udp_frame<R>(reader: &mut R) -> io::Result<Option<Vec<u8>>>
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

    let len = u16::from_be_bytes(len_buf) as usize;
    if len > MAX_VLESS_UDP_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VLESS UDP frame too large",
        ));
    }

    let mut payload = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut payload).await?;
    }
    Ok(Some(payload))
}

pub async fn write_vless_udp_frame<W>(writer: &mut W, payload: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    if payload.len() > MAX_VLESS_UDP_PAYLOAD {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VLESS UDP frame too large",
        ));
    }
    let len = (payload.len() as u16).to_be_bytes();
    writer.write_all(&len).await?;
    writer.write_all(payload).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn relay_exchanges_udp_frames() {
        let echo = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let echo_addr = echo.local_addr().unwrap();

        let relay_task = tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            loop {
                let (n, peer) = echo.recv_from(&mut buf).await.unwrap();
                echo.send_to(&buf[..n], peer).await.unwrap();
            }
        });

        let mut upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        upstream.connect(echo_addr).await.unwrap();

        let (mut client_left, mut client_right) = tokio::io::duplex(4096);

        let relay = tokio::spawn(async move {
            relay_vless_udp_with_limits(&mut client_left, &mut upstream, RelayLimits::default())
                .await
        });

        // Клиент шлёт length-prefixed пакет.
        let msg = b"ping-udp";
        let len = (msg.len() as u16).to_be_bytes();
        client_right.write_all(&len).await.unwrap();
        client_right.write_all(msg).await.unwrap();

        // Читаем ответ.
        let mut len_buf = [0u8; 2];
        client_right.read_exact(&mut len_buf).await.unwrap();
        let resp_len = u16::from_be_bytes(len_buf) as usize;
        let mut resp = vec![0u8; resp_len];
        client_right.read_exact(&mut resp).await.unwrap();
        assert_eq!(resp, msg);

        client_right.shutdown().await.unwrap();
        relay.await.unwrap().unwrap();
        relay_task.abort();
    }

    #[tokio::test]
    async fn idle_timeout_closes_udp_relay() {
        let mut upstream = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let (mut client_left, _client_right) = tokio::io::duplex(256);

        let start = Instant::now();
        let result = relay_vless_udp_with_limits(
            &mut client_left,
            &mut upstream,
            RelayLimits {
                idle: Some(Duration::from_millis(50)),
                max_lifetime: None,
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), IDLE_TIMEOUT_MSG);
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
