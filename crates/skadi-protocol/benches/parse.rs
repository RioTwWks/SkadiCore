//! Criterion-бенчмарки парсеров SOCKS5 и VLESS (горячий путь).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use skadi_protocol::socks5::parse::{parse_greeting, parse_request};
use skadi_protocol::vless::addons::{build_addons_with_flow, parse_addons, FLOW_XTLS_VISION};
use skadi_protocol::vless::parse::{
    build_tcp_domain_request, build_tcp_request, parse_request as parse_vless_request,
};
use std::net::Ipv4Addr;

fn socks5_greeting(c: &mut Criterion) {
    let buf = [0x05, 0x01, 0x00];
    c.bench_function("socks5/greeting", |b| {
        b.iter(|| parse_greeting(black_box(&buf)));
    });
}

fn socks5_request(c: &mut Criterion) {
    let mut group = c.benchmark_group("socks5/request");

    let ipv4 = [0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x01, 0xBB];
    group.throughput(Throughput::Bytes(ipv4.len() as u64));
    group.bench_with_input(BenchmarkId::new("ipv4", ipv4.len()), &ipv4, |b, buf| {
        b.iter(|| parse_request(black_box(buf)));
    });

    let mut domain = vec![0x05, 0x01, 0x00, 0x03, 15];
    domain.extend_from_slice(b"www.example.com");
    domain.extend_from_slice(&[0x01, 0xBB]);
    group.throughput(Throughput::Bytes(domain.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("domain", domain.len()),
        &domain,
        |b, buf| {
            b.iter(|| parse_request(black_box(buf)));
        },
    );

    group.finish();
}

fn vless_request(c: &mut Criterion) {
    let uuid = [0xAB; 16];
    let ipv4 = build_tcp_request(&uuid, Ipv4Addr::new(10, 0, 0, 1), 443);
    let domain = build_tcp_domain_request(&uuid, "cdn.example.com", 443);

    let mut group = c.benchmark_group("vless/request");

    group.throughput(Throughput::Bytes(ipv4.len() as u64));
    group.bench_with_input(BenchmarkId::new("ipv4", ipv4.len()), &ipv4, |b, buf| {
        b.iter(|| parse_vless_request(black_box(buf)));
    });

    group.throughput(Throughput::Bytes(domain.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("domain", domain.len()),
        &domain,
        |b, buf| {
            b.iter(|| parse_vless_request(black_box(buf)));
        },
    );

    group.finish();
}

fn vless_addons(c: &mut Criterion) {
    let empty: &[u8] = &[];
    let vision = build_addons_with_flow(FLOW_XTLS_VISION);

    let mut group = c.benchmark_group("vless/addons");

    group.bench_function("empty", |b| {
        b.iter(|| parse_addons(black_box(empty)));
    });

    group.throughput(Throughput::Bytes(vision.len() as u64));
    group.bench_with_input(
        BenchmarkId::new("vision", vision.len()),
        &vision,
        |b, buf| {
            b.iter(|| parse_addons(black_box(buf)));
        },
    );

    group.finish();
}

criterion_group!(
    benches,
    socks5_greeting,
    socks5_request,
    vless_request,
    vless_addons
);
criterion_main!(benches);
