# Changelog

Все значимые изменения в SkadiCore фиксируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект следует [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Added

- **`rustls::reality::RealityServerCertVerifier`** — проверка REALITY leaf по HMAC-SHA512
  (не «доверять всему» на data plane); re-export в `skadi_transport::reality`
- **`skadi-config`** — server TOML + validation вынесены из `skadi-server`
- **SOCKS5 BIND / UDP ASSOCIATE** — опционально (`protocol.socks5.bind`, `udp_associate`)
- **gRPC audit log** — `api.audit_log` (default `true`), target `skadi.grpc.audit`
- **`scripts/probe-test.sh`** — TLS summary, опциональный JA3, сравнение таймингов dest vs Skadi
- **`scripts/smoke-transports.sh`** — `check-config` для AWG / Hysteria2 / TUIC
- **`CONTRIBUTING.md`** — как вносить изменения, проверки CI, документация
- **REALITY client (частично)** — `RealityClientAuth` (HKDF + session_id seal), схема `[remote.reality]` в `skadi-client`, пример `examples/client-reality-vless/`
- **CI** — optional job `smoke-transports` (`scripts/smoke-transports.sh`)

### Changed

- **Пример AWG** — плейсхолдеры ключей; `check-config` пропускает криптопроверку для `<...>`

## [0.1.0] - 2026-09-11

Первый MVP: сервер `skadicore`, VLESS/SOCKS5, TLS/REALITY, клиент, TUN, observability, CI.

### Added

#### Сервер и конфигурация

- Workspace: `skadi-core`, `skadi-protocol`, `skadi-transport`, `skadi-server`, `skadi-api`
- TOML-конфиг, `skadicore check-config`, SIGHUP reload `[protocol.*]`
- `skadicore genkey reality` / `genkey awg`
- `skadi_server::run()` для интеграционных тестов
- `skadi_core::Protocol`, sniffing SOCKS5/VLESS

#### Протоколы

- **SOCKS5** — CONNECT, user/pass (RFC 1929), fuzz; CONNECT over TLS (e2e)
- **VLESS** — TCP/UDP, Mux, XUDP hit/reconnect, flow `xtls-rprx-vision` (reject)
- Hot reload пользователей (`UserStore`), gRPC API + Bearer, rate limit, опциональный TLS API

#### Транспорт

- **TLS inbound** — PEM, SNI (`[[transport.tls.certificates]]`), ALPN, TLS 1.3
- **TLS outbound** — проверка CA
- **REALITY** — `rustls-reality`, fallback на `dest`, short_ids, e2e с Xray-core
- **REALITY-rkn-fix** — per-connection Ed25519, `impersonate_cert` / fetch с `dest`
- **XHTTP** — stream-one, stream-up, packet-up; пример REALITY+XHTTP + e2e
- **AmneziaWG** — `[transport.awg]`, client mode, NAT, export, `examples/awg-vpn/`
- **Hysteria2 / TUIC** — MVP через внешние бинарники + examples

#### Клиент

- `skadicore client` — локальный SOCKS5 → VLESS+TLS
- **TUN (Linux)** — routing, DNS hijack, DoH/DoT upstream, block system DoT/DoH, PMTUD
- Предупреждения DNS-утечек (`socks5` vs `socks5h`)

#### Безопасность и аудит

- Per-IP auth rate limit, SSRF `outbound.allow_private`, маскирование Bearer в логах
- UUID не в plaintext при auth failure
- `deny.toml` (supply chain), документация PQ/SN-DL (`docs/RISKS.md`, `SECURITY.md`)
- PQ-KEX `kex_mode = "hybrid_pq"` для TLS и REALITY

#### Наблюдаемость и ops

- Prometheus `/metrics`, `/healthz`, structured logging (`json` / `pretty`)

#### Тестирование и CI

- fmt, clippy, test, audit, deny, miri, tarpaulin (≥70%), proptest, soak (`scripts/soak.sh`)
- Кросс-сборка musl (x86_64, aarch64), Windows/macOS, Docker, GitHub Releases + minisign
- IPv6 `listen` (dual-stack), property/load/soak e2e

#### Документация и примеры

- `README`, `docs/*`, `examples/` (REALITY, VLESS TLS, SOCKS5 TLS, client, soak)
- `.cursor/` для AI-агентов

### Changed

- Handlers на `AsyncRead + AsyncWrite` вместо привязки к `TcpStream`
- Парсеры SOCKS5/VLESS вынесены в `parse.rs` для fuzz и юнит-тестов
- Документация синхронизирована с кодом (2026-09-11)

### Fixed

- Дублирование `MAX_METHODS` в SOCKS5
- Экспорт модуля `vless` из `skadi-protocol`

### Security

- Constant-time сравнение паролей и UUID; лимиты парсеров; таймауты handshake
- VLESS: молчаливое закрытие при неверном UUID (anti-probe)

---

## Как вести этот файл

1. Значимые изменения — в `[Unreleased]` сразу, не ждать релиза.
2. При релизе: переименовать `[Unreleased]` в `[X.Y.Z] - YYYY-MM-DD`, открыть новый пустой `[Unreleased]`.
3. Писать с точки зрения пользователя; для security — отдельная секция **Security**.
4. Ссылки на PR/issue по возможности: `([#42](https://github.com/RioTwWks/SkadiCore/pull/42))`.

Типы секций: **Added**, **Changed**, **Deprecated**, **Removed**, **Fixed**, **Security**.

[Unreleased]: https://github.com/RioTwWks/SkadiCore/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/RioTwWks/SkadiCore/releases/tag/v0.1.0
