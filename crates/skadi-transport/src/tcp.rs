use anyhow::Result;
use skadi_core::Endpoint;
use std::time::Duration;
use tokio::net::TcpStream;
use tracing::debug;

use crate::outbound_policy::resolve_endpoint_addrs;

/// Транспорт поверх TCP.
#[derive(Clone)]
pub struct TcpTransport {
    connect_timeout: Duration,
    /// По умолчанию `true` (без фильтрации) — для подключения к явно заданному прокси.
    allow_private: bool,
}

impl TcpTransport {
    pub fn new(connect_timeout: Duration) -> Self {
        Self::with_policy(connect_timeout, true)
    }

    pub fn with_policy(connect_timeout: Duration, allow_private: bool) -> Self {
        Self {
            connect_timeout,
            allow_private,
        }
    }

    /// Установить исходящее соединение с таймаутом.
    pub async fn connect(&self, endpoint: &Endpoint) -> Result<TcpStream> {
        let addrs = resolve_endpoint_addrs(endpoint, self.allow_private).await?;
        let mut last_err = None;

        for addr in addrs {
            debug!(target = %addr, "connecting");
            match tokio::time::timeout(self.connect_timeout, TcpStream::connect(addr)).await {
                Ok(Ok(stream)) => {
                    stream.set_nodelay(true)?;
                    return Ok(stream);
                }
                Ok(Err(e)) => last_err = Some(e),
                Err(_) => {
                    last_err = Some(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!("connect timeout to {}", addr),
                    ))
                }
            }
        }

        Err(last_err
            .map(|e| anyhow::anyhow!(e))
            .unwrap_or_else(|| anyhow::anyhow!("no addresses to connect")))
    }
}
