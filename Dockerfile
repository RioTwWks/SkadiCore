# syntax=docker/dockerfile:1
# Статический musl-бинарник в минимальном образе (FROM scratch).
#
# Сборка:
#   docker build -t skadicore:local .
#
# Запуск (конфиг и сертификаты монтируются с хоста):
#   docker run --rm -p 443:443 \
#     -v "$(pwd)/config:/config:ro" \
#     -v "$(pwd)/certs:/certs:ro" \
#     skadicore:local --config /config/skadi.toml

ARG RUST_VERSION=1.98.1

FROM rust:${RUST_VERSION}-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends musl-tools \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml Cargo.lock rust-toolchain.toml deny.toml ./
COPY .cargo .cargo
COPY crates crates
COPY third_party third_party

RUN cargo build --release -p skadi-server --target x86_64-unknown-linux-musl \
    && file target/x86_64-unknown-linux-musl/release/skadicore

FROM scratch

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/skadicore /skadicore

EXPOSE 443

ENTRYPOINT ["/skadicore"]
CMD ["--config", "/config/skadi.toml"]
