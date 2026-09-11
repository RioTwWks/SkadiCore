//! Двунаправленный relay с лимитами сессии.

use std::future;
use std::io;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Сообщение `io::Error` при idle timeout.
pub const IDLE_TIMEOUT_MSG: &str = "idle timeout";
/// Сообщение `io::Error` при превышении max session lifetime.
pub const SESSION_LIFETIME_MSG: &str = "max session lifetime";

/// Ограничения relay-сессии. `None` — лимит выключен.
#[derive(Debug, Clone, Copy, Default)]
pub struct RelayLimits {
    pub idle: Option<Duration>,
    pub max_lifetime: Option<Duration>,
}

impl RelayLimits {
    pub fn has_limits(self) -> bool {
        self.idle.is_some() || self.max_lifetime.is_some()
    }
}

/// Скопировать данные между двумя потоками с опциональными лимитами.
///
/// Возвращает `(a→b, b→a)` байт, как `tokio::io::copy_bidirectional`.
pub async fn copy_bidirectional_with_limits<A, B>(
    a: &mut A,
    b: &mut B,
    limits: RelayLimits,
) -> io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    if !limits.has_limits() {
        return tokio::io::copy_bidirectional(a, b).await;
    }

    let mut a_to_b = 0u64;
    let mut b_to_a = 0u64;
    let mut buf_a = [0u8; 8192];
    let mut buf_b = [0u8; 8192];
    let mut a_read_done = false;
    let mut b_read_done = false;

    let idle_enabled = limits.idle.is_some();
    let lifetime_enabled = limits.max_lifetime.is_some();

    let idle_deadline = tokio::time::sleep(limits.idle.unwrap_or(Duration::MAX));
    tokio::pin!(idle_deadline);

    let lifetime_deadline = tokio::time::sleep(limits.max_lifetime.unwrap_or(Duration::MAX));
    tokio::pin!(lifetime_deadline);

    loop {
        if a_read_done && b_read_done {
            return Ok((a_to_b, b_to_a));
        }

        tokio::select! {
            _ = &mut idle_deadline, if idle_enabled => {
                return Err(io::Error::new(io::ErrorKind::TimedOut, IDLE_TIMEOUT_MSG));
            }

            _ = &mut lifetime_deadline, if lifetime_enabled => {
                return Err(io::Error::new(io::ErrorKind::TimedOut, SESSION_LIFETIME_MSG));
            }

            res = read_if_open(a, &mut buf_a, a_read_done), if !a_read_done => {
                match res {
                    Ok(0) => {
                        a_read_done = true;
                        b.shutdown().await?;
                    }
                    Ok(n) => {
                        b.write_all(&buf_a[..n]).await?;
                        a_to_b += n as u64;
                        if idle_enabled {
                            idle_deadline
                                .as_mut()
                                .reset(tokio::time::Instant::now() + limits.idle.unwrap());
                        }
                    }
                    Err(e) => return Err(e),
                }
            }

            res = read_if_open(b, &mut buf_b, b_read_done), if !b_read_done => {
                match res {
                    Ok(0) => {
                        b_read_done = true;
                        a.shutdown().await?;
                    }
                    Ok(n) => {
                        a.write_all(&buf_b[..n]).await?;
                        b_to_a += n as u64;
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

/// Скопировать данные с таймаутом неактивности (обратная совместимость).
pub async fn copy_bidirectional_with_idle_timeout<A, B>(
    a: &mut A,
    b: &mut B,
    idle: Duration,
) -> io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    copy_bidirectional_with_limits(
        a,
        b,
        RelayLimits {
            idle: Some(idle),
            max_lifetime: None,
        },
    )
    .await
}

async fn read_if_open<R>(reader: &mut R, buf: &mut [u8], closed: bool) -> io::Result<usize>
where
    R: AsyncRead + Unpin,
{
    if closed {
        future::pending().await
    } else {
        reader.read(buf).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn closes_when_both_sides_idle() {
        let (mut left, mut right) = tokio::io::duplex(256);
        let idle = Duration::from_millis(50);

        let start = Instant::now();
        let result = copy_bidirectional_with_idle_timeout(&mut left, &mut right, idle).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn closes_when_session_lifetime_exceeded() {
        let (mut left, mut right) = tokio::io::duplex(256);

        let result = copy_bidirectional_with_limits(
            &mut left,
            &mut right,
            RelayLimits {
                idle: None,
                max_lifetime: Some(Duration::from_millis(50)),
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert_eq!(err.to_string(), SESSION_LIFETIME_MSG);
    }

    #[tokio::test]
    async fn lifetime_limit_applies_despite_traffic() {
        let (mut client_end, mut relay_left) = tokio::io::duplex(1024);
        let (mut relay_right, mut server_end) = tokio::io::duplex(1024);

        let relay = tokio::spawn(async move {
            copy_bidirectional_with_limits(
                &mut relay_left,
                &mut relay_right,
                RelayLimits {
                    idle: Some(Duration::from_secs(60)),
                    max_lifetime: Some(Duration::from_millis(100)),
                },
            )
            .await
        });

        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            loop {
                let n = server_end.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                if server_end.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });

        for _ in 0..3 {
            client_end.write_all(b"ping").await.unwrap();
            let mut out = [0u8; 4];
            client_end.read_exact(&mut out).await.unwrap();
            assert_eq!(&out, b"ping");
            tokio::time::sleep(Duration::from_millis(40)).await;
        }

        let result = relay.await.unwrap();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().to_string(), SESSION_LIFETIME_MSG);
    }

    #[tokio::test]
    async fn relays_data_before_idle_timeout() {
        let (mut client_end, mut relay_left) = tokio::io::duplex(1024);
        let (mut relay_right, mut server_end) = tokio::io::duplex(1024);

        let relay = tokio::spawn(async move {
            copy_bidirectional_with_limits(
                &mut relay_left,
                &mut relay_right,
                RelayLimits {
                    idle: Some(Duration::from_millis(300)),
                    max_lifetime: None,
                },
            )
            .await
        });

        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            loop {
                let n = server_end.read(&mut buf).await.unwrap();
                if n == 0 {
                    break;
                }
                if server_end.write_all(&buf[..n]).await.is_err() {
                    break;
                }
            }
        });

        client_end.write_all(b"hello").await.unwrap();
        let mut out = [0u8; 5];
        client_end.read_exact(&mut out).await.unwrap();
        assert_eq!(&out, b"hello");
        client_end.shutdown().await.unwrap();
        relay.await.unwrap().unwrap();
    }
}
