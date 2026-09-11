use anyhow::Result;
use skadi_core::Endpoint;
use std::time::Duration;
use tokio::net::TcpStream;
use tracing::debug;

/// Транспорт поверх TCP.
#[derive(Clone)]
pub struct TcpTransport {
    connect_timeout: Duration,
}

impl TcpTransport {
    pub fn new(connect_timeout: Duration) -> Self {
        Self { connect_timeout }
    }

    /// Установить исходящее соединение с таймаутом.
    pub async fn connect(&self, endpoint: &Endpoint) -> Result<TcpStream> {
        let addr = match endpoint {
            Endpoint::Ip(a) => a.to_string(),
            Endpoint::Domain(host, port) => format!("{}:{}", host, port),
        };

        debug!(target = %addr, "connecting");

        let stream = tokio::time::timeout(self.connect_timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| anyhow::anyhow!("connect timeout to {}", addr))??;

        // Отключаем алгоритм Нейгла — снижает latency для мелких пакетов.
        stream.set_nodelay(true)?;

        Ok(stream)
    }
}
