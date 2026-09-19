# Changelog

Все значимые изменения в SkadiCore фиксируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект следует [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

### Added

- **Клиентский XHTTP** (`[remote.xhttp]`): stream-one / stream-up / packet-up поверх plain, TLS или REALITY
- Пример **`examples/client-reality-xhttp-vless/`** + e2e `client_socks5_vless_reality_xhttp_e2e`
- **`remote.reality.kex_mode`** на клиенте (`classic` / `hybrid_pq`, паритет с сервером)
- Пример **`examples/client-reality-vless-tun/`** (TUN + REALITY)
- **`CODE_OF_CONDUCT.md`** (Contributor Covenant 2.1), шаблоны GitHub PR/issue

### Changed

- При включённом `[remote.xhttp]` REALITY/TLS outbound использует ALPN только `http/1.1`
- **`docs/RELEASING.md`** — подробное руководство по релизам и minisign
- **`docs/CONFIGURATION.md`** — секции `[remote.reality]`, `[remote.xhttp]`
- **`SECURITY.md`** — политика поддержки для релизных тегов
- README: дорожная карта VLESS/REALITY, статус v0.1.0

### Fixed

- Release workflow: путь к бинарнику minisign в официальном tarball (`minisign-linux/x86_64/`)
- Release signing: поддержка `MINISIGN_KEY_PASSPHRASE` и sync `sign-release.sh` с default branch

## [0.1.0] - 2026-09-19

Первый публичный релиз: сервер `skadicore`, VLESS/SOCKS5, TLS/REALITY (inbound и нативный клиент), TUN, observability, CI и GitHub Releases.

### Added

#### Сервер и конфигурация

- Workspace: `skadi-core`, `skadi-protocol`, `skadi-transport`, `skadi-server`, `skadi-api`, `skadi-config`
- TOML-конфиг, `skadicore check-config`, SIGHUP reload `[protocol.*]`
- `skadicore genkey reality` / `genkey awg`
- `skadi_server::run()` для интеграционных тестов
- `skadi_core::Protocol`, sniffing SOCKS5/VLESS

#### Протоколы

- **SOCKS5** — CONNECT, user/pass (RFC 1929), fuzz; CONNECT over TLS (e2e); опционально BIND и UDP ASSOCIATE
- **VLESS** — TCP/UDP, Mux, XUDP hit/reconnect, flow `xtls-rprx-vision` (reject)
- Hot reload пользователей (`UserStore`), gRPC API + Bearer, rate limit, опциональный TLS API
- **gRPC audit log** — `api.audit_log` (default `true`), target `skadi.grpc.audit`

#### Транспорт

- **TLS inbound** — PEM, SNI (`[[transport.tls.certificates]]`), ALPN, TLS 1.3
- **TLS outbound** — проверка CA
- **REALITY** — `rustls-reality`, fallback на `dest`, short_ids, e2e с Xray-core
- **`rustls::reality::RealityServerCertVerifier`** — проверка REALITY leaf по HMAC-SHA512; re-export в `skadi_transport::reality`
- **REALITY-rkn-fix** — per-connection Ed25519, `impersonate_cert` / fetch с `dest`
- **XHTTP** — stream-one, stream-up, packet-up; пример REALITY+XHTTP + e2e
- **AmneziaWG** — `[transport.awg]`, client mode, NAT, export, `examples/awg-vpn/`
- **Hysteria2 / TUIC** — MVP через внешние бинарники + examples

#### Клиент

- `skadicore client` — локальный SOCKS5 → VLESS+TLS
- **REALITY outbound (native)** — vendored rustls ClientHello seal, `RealityTlsOutboundTransport`, `[remote.reality]`; TCP на `remote.server`, SNI на `remote.reality.server_name`
- E2E: `client_socks5_vless_reality_e2e`, `reality_native_tls_e2e`
- **TUN (Linux)** — routing, DNS hijack, DoH/DoT upstream, block system DoT/DoH, PMTUD (Phase 1 + runtime ICMP Phase 2)
- Предупреждения DNS-утечек (`socks5` vs `socks5h`)
- Пример `examples/client-reality-vless/`

#### Безопасность и аудит

- Per-IP auth rate limit, SSRF `outbound.allow_private`, маскирование Bearer в логах
- UUID не в plaintext при auth failure
- `deny.toml` (supply chain), документация PQ/SN-DL (`docs/RISKS.md`, `SECURITY.md`)
- PQ-KEX `kex_mode = "hybrid_pq"` для TLS и REALITY

#### Наблюдаемость и ops

- Prometheus `/metrics`, `/healthz`, structured logging (`json` / `pretty`)
- **`scripts/probe-test.sh`** — TLS summary, опциональный JA3, тайминги dest vs Skadi
- **`scripts/smoke-transports.sh`** — `check-config` для AWG / Hysteria2 / TUIC

#### Тестирование и CI

- fmt, clippy, test, audit, deny, miri, tarpaulin (≥70%), proptest, soak (`scripts/soak.sh`)
- Кросс-сборка musl (x86_64, aarch64), Windows/macOS, Docker, GitHub Releases + minisign
- IPv6 `listen` (dual-stack), property/load/soak e2e
- CI: optional job `smoke-transports`

#### Документация и примеры

- `README`, `docs/*`, `examples/` (REALITY, VLESS TLS, SOCKS5 TLS, client, soak)
- **`CONTRIBUTING.md`**, `.cursor/` для AI-агентов
- **`docs/RELEASING.md`** — теги и GitHub Releases

### Changed

- Handlers на `AsyncRead + AsyncWrite` вместо привязки к `TcpStream`
- Парсеры SOCKS5/VLESS вынесены в `parse.rs` для fuzz и юнит-тестов
- **`skadi-config`** — server TOML + validation вынесены из `skadi-server`
- Пример AWG — плейсхолдеры ключей; `check-config` пропускает криптопроверку для `<...>`

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
