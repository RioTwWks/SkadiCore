//! Двунаправленный relay с таймаутом неактивности.

use std::future;
use std::io;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Скопировать данные между двумя потоками, прерывая сессию при простое.
///
/// Возвращает `(a→b, b→a)` байт, как `tokio::io::copy_bidirectional`.
/// При превышении `idle` без чтения/записи в любой из сторон — `ErrorKind::TimedOut`.
pub async fn copy_bidirectional_with_idle_timeout<A, B>(
    a: &mut A,
    b: &mut B,
    idle: Duration,
) -> io::Result<(u64, u64)>
where
    A: AsyncRead + AsyncWrite + Unpin,
    B: AsyncRead + AsyncWrite + Unpin,
{
    let mut a_to_b = 0u64;
    let mut b_to_a = 0u64;
    let mut buf_a = [0u8; 8192];
    let mut buf_b = [0u8; 8192];
    let mut a_read_done = false;
    let mut b_read_done = false;
    let idle_deadline = tokio::time::sleep(idle);
    tokio::pin!(idle_deadline);

    loop {
        if a_read_done && b_read_done {
            return Ok((a_to_b, b_to_a));
        }

        tokio::select! {
            _ = &mut idle_deadline => {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "idle timeout"));
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
                        idle_deadline
                            .as_mut()
                            .reset(tokio::time::Instant::now() + idle);
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
                        idle_deadline
                            .as_mut()
                            .reset(tokio::time::Instant::now() + idle);
                    }
                    Err(e) => return Err(e),
                }
            }
        }
    }
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
        let result =
            copy_bidirectional_with_idle_timeout(&mut left, &mut right, idle).await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn relays_data_before_idle_timeout() {
        let (mut client_end, mut relay_left) = tokio::io::duplex(1024);
        let (mut relay_right, mut server_end) = tokio::io::duplex(1024);

        let relay = tokio::spawn(async move {
            copy_bidirectional_with_idle_timeout(
                &mut relay_left,
                &mut relay_right,
                Duration::from_millis(300),
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
