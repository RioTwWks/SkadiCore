//! SOCKS5 BIND и UDP ASSOCIATE (RFC 1928).

use anyhow::{Context, Result};
use skadi_protocol::socks5::parse::{encode_udp_datagram, parse_udp_datagram};
use skadi_protocol::{Socks5Handler, REP_SUCCEEDED};
use skadi_transport::{copy_bidirectional_with_limits, RelayLimits, UdpTransport};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::net::{TcpListener, UdpSocket};
use tracing::debug;

const MAX_UDP_PAYLOAD: usize = 65_507;
const UDP_REPLY_WAIT: Duration = Duration::from_secs(5);

/// BIND: слушаем ephemeral порт, второй reply после входящего connect, затем relay.
pub async fn handle_socks5_bind<S>(
    mut client: S,
    accept_timeout: Duration,
    relay_limits: RelayLimits,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let listener = TcpListener::bind("0.0.0.0:0")
        .await
        .context("SOCKS5 BIND listen failed")?;
    let bound = listener.local_addr()?;
    Socks5Handler::send_reply(&mut client, REP_SUCCEEDED, bound).await?;

    let (mut remote, remote_addr) = tokio::time::timeout(accept_timeout, listener.accept())
        .await
        .context("SOCKS5 BIND accept timeout")?
        .map_err(|e| e)
        .context("SOCKS5 BIND accept failed")?;

    debug!(bound = %bound, remote = %remote_addr, "SOCKS5 BIND accepted");
    Socks5Handler::send_reply(&mut client, REP_SUCCEEDED, remote_addr).await?;

    copy_bidirectional_with_limits(&mut client, &mut remote, relay_limits)
        .await
        .context("SOCKS5 BIND relay")?;
    Ok(())
}

/// UDP ASSOCIATE: TCP control + UDP relay на `relay` socket.
pub async fn handle_socks5_udp_associate<S>(
    mut control: S,
    outbound_udp: UdpTransport,
    relay_limits: RelayLimits,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let relay = UdpSocket::bind("0.0.0.0:0")
        .await
        .context("SOCKS5 UDP relay bind failed")?;
    let relay_addr = relay.local_addr()?;
    Socks5Handler::send_reply(&mut control, REP_SUCCEEDED, relay_addr).await?;
    debug!(relay = %relay_addr, "SOCKS5 UDP ASSOCIATE ready");

    let mut buf = vec![0u8; MAX_UDP_PAYLOAD + 512];
    let idle = relay_limits.idle.unwrap_or(Duration::MAX);
    let lifetime = relay_limits.max_lifetime.unwrap_or(Duration::MAX);

    let idle_deadline = tokio::time::sleep(idle);
    tokio::pin!(idle_deadline);
    let lifetime_deadline = tokio::time::sleep(lifetime);
    tokio::pin!(lifetime_deadline);

    let mut control_eof = [0u8; 1];

    loop {
        tokio::select! {
            _ = &mut lifetime_deadline => {
                break;
            }
            _ = &mut idle_deadline => {
                break;
            }
            read = control.read(&mut control_eof) => {
                match read {
                    Ok(0) => break,
                    Ok(_) => {
                        // RFC: клиент не должен слать данные по control; игнорируем.
                        idle_deadline.as_mut().reset(tokio::time::Instant::now() + idle);
                    }
                    Err(_) => break,
                }
            }
            recv = relay.recv_from(&mut buf) => {
                let (n, client_addr) = recv.context("SOCKS5 UDP recv")?;
                idle_deadline.as_mut().reset(tokio::time::Instant::now() + idle);
                if n < 4 {
                    continue;
                }
                let (target, header_len) = match parse_udp_datagram(&buf[..n]) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let payload = &buf[header_len..n];
                if payload.is_empty() {
                    continue;
                }
                let upstream = match outbound_udp.connect(&target).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if upstream.send(payload).await.is_err() {
                    continue;
                }
                let mut resp = vec![0u8; MAX_UDP_PAYLOAD];
                if let Ok(Ok(m)) =
                    tokio::time::timeout(UDP_REPLY_WAIT, upstream.recv(&mut resp)).await
                {
                    if m > 0 {
                        let packet = encode_udp_datagram(&target, &resp[..m]);
                        let _ = relay.send_to(&packet, client_addr).await;
                    }
                }
            }
        }
    }

    Ok(())
}
