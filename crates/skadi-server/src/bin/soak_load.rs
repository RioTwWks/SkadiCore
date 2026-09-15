//! Генератор нагрузки VLESS для длительных soak-прогонов (`scripts/soak.sh`).

use anyhow::{Context, Result};
use clap::Parser;
use skadi_protocol::vless::{build_response_header, build_tcp_request, Uuid, VLESS_VERSION};
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, info, warn};

#[derive(Parser, Debug)]
#[command(name = "soak_load", about = "VLESS load generator for soak tests")]
struct Args {
    /// Адрес VLESS-прокси (host:port).
    #[arg(long, default_value = "127.0.0.1:10800")]
    proxy: String,

    /// UUID пользователя VLESS.
    #[arg(long, default_value = "b831381d-6324-4d53-ad4f-8cda48b30811")]
    uuid: String,

    /// IPv4 upstream (echo / iperf3).
    #[arg(long, default_value = "127.0.0.1")]
    target_host: String,

    #[arg(long, default_value = "5201")]
    target_port: u16,

    /// Параллельных сессий.
    #[arg(long, default_value = "32")]
    concurrency: usize,

    /// Размер payload на сессию (байт).
    #[arg(long, default_value = "65536")]
    payload_bytes: usize,

    /// Длительность (0 = до SIGTERM).
    #[arg(long, default_value = "0")]
    duration_secs: u64,

    /// Интервал отчёта статистики (сек).
    #[arg(long, default_value = "30")]
    report_interval_secs: u64,
}

struct Stats {
    sessions_ok: AtomicU64,
    sessions_fail: AtomicU64,
    bytes_transferred: AtomicU64,
    latency_us_sum: AtomicU64,
    latency_us_max: AtomicU64,
}

impl Stats {
    fn new() -> Self {
        Self {
            sessions_ok: AtomicU64::new(0),
            sessions_fail: AtomicU64::new(0),
            bytes_transferred: AtomicU64::new(0),
            latency_us_sum: AtomicU64::new(0),
            latency_us_max: AtomicU64::new(0),
        }
    }

    fn record_ok(&self, bytes: u64, latency_us: u64) {
        self.sessions_ok.fetch_add(1, Ordering::Relaxed);
        self.bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
        self.latency_us_sum.fetch_add(latency_us, Ordering::Relaxed);
        let prev = self.latency_us_max.load(Ordering::Relaxed);
        if latency_us > prev {
            self.latency_us_max.store(latency_us, Ordering::Relaxed);
        }
    }

    fn record_fail(&self) {
        self.sessions_fail.fetch_add(1, Ordering::Relaxed);
    }
}

async fn vless_session(
    proxy: &str,
    uuid: &[u8; 16],
    target: Ipv4Addr,
    port: u16,
    payload_bytes: usize,
) -> Result<(u64, u64)> {
    let started = Instant::now();
    let mut stream = TcpStream::connect(proxy).await.context("connect proxy")?;

    stream
        .write_all(&build_tcp_request(uuid, target, port))
        .await
        .context("write vless request")?;

    let mut response = [0u8; 2];
    stream
        .read_exact(&mut response)
        .await
        .context("read vless response")?;
    if response != build_response_header(VLESS_VERSION) {
        anyhow::bail!("unexpected vless response header");
    }

    let chunk = vec![0xABu8; payload_bytes.min(64 * 1024)];
    let mut remaining = payload_bytes;
    let mut transferred = 0u64;
    while remaining > 0 {
        let n = chunk.len().min(remaining);
        stream
            .write_all(&chunk[..n])
            .await
            .context("write payload")?;
        let mut buf = vec![0u8; n];
        stream.read_exact(&mut buf).await.context("read echo")?;
        transferred += n as u64;
        remaining -= n;
    }

    stream.shutdown().await.ok();
    let latency_us = started.elapsed().as_micros() as u64;
    Ok((transferred, latency_us))
}

async fn worker(
    proxy: String,
    uuid: [u8; 16],
    target: Ipv4Addr,
    port: u16,
    payload_bytes: usize,
    stats: Arc<Stats>,
    deadline: Option<Instant>,
) {
    loop {
        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                break;
            }
        }

        match vless_session(&proxy, &uuid, target, port, payload_bytes).await {
            Ok((bytes, latency_us)) => stats.record_ok(bytes, latency_us),
            Err(e) => {
                stats.record_fail();
                debug!(error = %e, "soak session failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn reporter(stats: Arc<Stats>, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.tick().await;
    loop {
        ticker.tick().await;
        let ok = stats.sessions_ok.load(Ordering::Relaxed);
        let fail = stats.sessions_fail.load(Ordering::Relaxed);
        let bytes = stats.bytes_transferred.load(Ordering::Relaxed);
        let avg_us = if ok > 0 {
            stats.latency_us_sum.load(Ordering::Relaxed) / ok
        } else {
            0
        };
        let max_us = stats.latency_us_max.load(Ordering::Relaxed);
        info!(
            sessions_ok = ok,
            sessions_fail = fail,
            bytes_transferred = bytes,
            latency_avg_ms = avg_us as f64 / 1000.0,
            latency_max_ms = max_us as f64 / 1000.0,
            "soak_load stats"
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let uuid = *Uuid::parse(&args.uuid).context("invalid uuid")?.as_bytes();
    let target = args
        .target_host
        .parse::<Ipv4Addr>()
        .context("target_host must be IPv4")?;

    if args.concurrency == 0 {
        anyhow::bail!("concurrency must be > 0");
    }

    let deadline = if args.duration_secs > 0 {
        Some(Instant::now() + Duration::from_secs(args.duration_secs))
    } else {
        None
    };

    let stats = Arc::new(Stats::new());
    let report_interval = Duration::from_secs(args.report_interval_secs.max(1));

    let reporter_handle = tokio::spawn(reporter(stats.clone(), report_interval));

    let mut workers = Vec::with_capacity(args.concurrency);
    for _ in 0..args.concurrency {
        let proxy = args.proxy.clone();
        let st = stats.clone();
        workers.push(tokio::spawn(worker(
            proxy,
            uuid,
            target,
            args.target_port,
            args.payload_bytes,
            st,
            deadline,
        )));
    }

    for handle in workers {
        if let Err(e) = handle.await {
            warn!(error = %e, "worker task join error");
        }
    }

    reporter_handle.abort();

    let fail = stats.sessions_fail.load(Ordering::Relaxed);
    let ok = stats.sessions_ok.load(Ordering::Relaxed);
    info!(sessions_ok = ok, sessions_fail = fail, "soak_load finished");

    if ok == 0 {
        anyhow::bail!("no successful sessions");
    }
    if fail > ok {
        anyhow::bail!("failure rate too high: {fail} failures vs {ok} successes");
    }

    Ok(())
}
