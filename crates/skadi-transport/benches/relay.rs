//! Criterion-бенчмарки TCP relay (горячий путь copy_bidirectional).

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use skadi_transport::relay::{copy_bidirectional_with_limits, RelayLimits};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;

const PAYLOAD: usize = 64 * 1024;

async fn echo_server(mut server_end: tokio::io::DuplexStream) {
    let mut buf = vec![0u8; 8192];
    loop {
        let n = server_end.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }
        if server_end.write_all(&buf[..n]).await.is_err() {
            break;
        }
    }
}

fn relay_throughput(c: &mut Criterion) {
    let rt = Runtime::new().expect("tokio runtime");

    let mut group = c.benchmark_group("relay/copy_bidirectional");
    group.throughput(Throughput::Bytes(PAYLOAD as u64));
    group.sample_size(20);

    group.bench_function("unlimited", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (mut client_end, mut relay_left) = tokio::io::duplex(PAYLOAD + 4096);
                let (mut relay_right, server_end) = tokio::io::duplex(PAYLOAD + 4096);

                let server = tokio::spawn(echo_server(server_end));
                let relay = tokio::spawn(async move {
                    copy_bidirectional_with_limits(
                        &mut relay_left,
                        &mut relay_right,
                        RelayLimits::default(),
                    )
                    .await
                });

                let payload = vec![0xABu8; PAYLOAD];
                client_end.write_all(&payload).await.unwrap();
                let mut received = vec![0u8; PAYLOAD];
                client_end.read_exact(&mut received).await.unwrap();
                assert_eq!(received, payload);
                client_end.shutdown().await.unwrap();

                black_box(relay.await.unwrap().unwrap());
                server.abort();
            });
        });
    });

    group.bench_function("idle_timeout_60s", |b| {
        b.iter(|| {
            rt.block_on(async {
                let (mut client_end, mut relay_left) = tokio::io::duplex(PAYLOAD + 4096);
                let (mut relay_right, server_end) = tokio::io::duplex(PAYLOAD + 4096);

                let server = tokio::spawn(echo_server(server_end));
                let relay = tokio::spawn(async move {
                    copy_bidirectional_with_limits(
                        &mut relay_left,
                        &mut relay_right,
                        RelayLimits {
                            idle: Some(Duration::from_secs(60)),
                            max_lifetime: None,
                        },
                    )
                    .await
                });

                let payload = vec![0xCDu8; PAYLOAD];
                client_end.write_all(&payload).await.unwrap();
                let mut received = vec![0u8; PAYLOAD];
                client_end.read_exact(&mut received).await.unwrap();
                client_end.shutdown().await.unwrap();

                black_box(relay.await.unwrap().unwrap());
                server.abort();
            });
        });
    });

    group.finish();
}

criterion_group!(benches, relay_throughput);
criterion_main!(benches);
