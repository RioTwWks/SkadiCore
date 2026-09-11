//! Минимальный HTTP-сервер для `/metrics` и `/healthz`.

use crate::config::MetricsConfig;
use anyhow::{Context, Result};
use metrics_exporter_prometheus::PrometheusHandle;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::info;

/// Запуск HTTP observability endpoint до shutdown.
pub async fn run_metrics_server(
    config: &MetricsConfig,
    prom: PrometheusHandle,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let listen: SocketAddr = config
        .listen
        .parse()
        .with_context(|| format!("invalid metrics.listen: {}", config.listen))?;

    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("failed to bind metrics on {}", config.listen))?;

    info!(addr = %config.listen, "metrics HTTP listening");

    loop {
        tokio::select! {
            biased;

            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    info!("metrics HTTP stopping");
                    return Ok(());
                }
            }

            accept = listener.accept() => {
                match accept {
                    Ok((mut stream, _)) => {
                        let prom = prom.clone();
                        tokio::spawn(async move {
                            if let Err(e) = serve_connection(&mut stream, &prom).await {
                                tracing::debug!(error = %e, "metrics HTTP request failed");
                            }
                        });
                    }
                    Err(e) => tracing::warn!(error = %e, "metrics accept failed"),
                }
            }
        }
    }
}

async fn serve_connection(
    stream: &mut tokio::net::TcpStream,
    prom: &PrometheusHandle,
) -> Result<()> {
    let mut buf = [0u8; 2048];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");

    let (status, body, content_type) = match path {
        "/healthz" => (
            "200 OK".to_string(),
            "ok\n".to_string(),
            "text/plain; charset=utf-8".to_string(),
        ),
        "/metrics" => (
            "200 OK".to_string(),
            prom.render(),
            "text/plain; version=0.0.4; charset=utf-8".to_string(),
        ),
        _ => (
            "404 Not Found".to_string(),
            String::new(),
            "text/plain; charset=utf-8".to_string(),
        ),
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        content_type,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}
