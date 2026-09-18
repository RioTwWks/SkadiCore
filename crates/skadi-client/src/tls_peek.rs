//! Чтение префикса TLS ClientHello для SNI-инспекции.

use anyhow::{bail, Result};
use skadi_transport::parse_client_hello;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};

/// Прочитать начало TLS record (ClientHello) без потери байтов для relay.
pub async fn peek_tls_client_hello<S>(stream: &mut S) -> Result<Vec<u8>>
where
    S: AsyncRead + Unpin,
{
    let mut buffer = Vec::with_capacity(2048);
    let handshake_timeout = Duration::from_secs(2);

    let read_task = async {
        while buffer.len() < 5 {
            let mut chunk = [0u8; 512];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                bail!("connection closed before TLS record header");
            }
            buffer.extend_from_slice(&chunk[..n]);
        }

        if buffer[0] != 0x16 {
            return Ok(());
        }

        let needed = 5 + u16::from_be_bytes([buffer[3], buffer[4]]) as usize;
        while buffer.len() < needed && buffer.len() < 16_384 {
            let mut chunk = [0u8; 512];
            let n = stream.read(&mut chunk).await?;
            if n == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..n]);
        }
        Ok(())
    };

    match tokio::time::timeout(handshake_timeout, read_task).await {
        Ok(result) => result?,
        Err(_) => bail!("TLS ClientHello peek timeout"),
    }

    Ok(buffer)
}

/// Извлечь SNI из буфера TLS ClientHello.
pub fn tls_client_hello_sni(buffer: &[u8]) -> Option<String> {
    parse_client_hello(buffer)
        .ok()
        .flatten()
        .and_then(|info| info.server_name)
}
