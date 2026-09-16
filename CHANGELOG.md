# Changelog

Все значимые изменения в SkadiCore фиксируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект следует [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

Текущая ветка разработки. Всё, что ниже, ещё не выпущено в релиз.

### Added

- **DoH в TUN** — `client.tun.dns.mode = "doh"`: DNS-over-HTTPS (RFC 8484)
  через VLESS TCP+TLS к upstream (`https://cloudflare-dns.com/dns-query` и др.)

- **DNS-утечки (аудит 6.2)** — предупреждения при старте клиента
  (`socks5` vs `socks5h`, TUN `dns.hijack`); исправлены примеры `curl`
  на `--socks5-hostname`

- **MTU / фрагментация (аудит 6.3)** — константа `RECOMMENDED_TUN_MTU`
  (1400), предупреждение при `client.tun.mtu >= 1500`; документация PMTUD

- **Пост-квантовые риски (аудит 6.4)** — `docs/RISKS.md` §1.5,
  обновлён `SECURITY.md` (SN-DL, X25519 в REALITY/TLS)

- **Релизы (аудит)** — подпись артефактов `minisign` в CI (секрет
  `MINISIGN_SECRET_KEY`); `scripts/sign-release.sh` и
  `scripts/verify-release.sh`; инструкция в README / `docs/DEVELOPMENT.md`

- **IPv6 listen (аудит)** — `server.listen` как строка или массив
  (`["0.0.0.0:443", "[::]:443"]`); несколько accept-loop на разных сокетах;
  предупреждение при `0.0.0.0` без `[::]`; E2E `ipv6_listen_e2e`

- **Безопасность (аудит)** — per-IP rate limiting на неудачные аутентификации
  (`[server.auth_rate_limit]`); SSRF-защита outbound (`[outbound].allow_private`);
  UUID не логируется в plaintext при auth failure

- **Инфраструктура (аудит)** — `rust-toolchain.toml` (pinned `1.98.1`);
  multi-stage `Dockerfile` (musl static, `FROM scratch`); CI job `docker`

- **Тестирование (аудит)** — property-based тесты (`proptest`) для парсеров
  SOCKS5, VLESS и Mux: roundtrip encode/decode, инварианты `Incomplete`,
  no-panic на произвольных байтах (`tests/proptest_*.rs`)

- **Soak-тест (аудит)** — `scripts/soak.sh` (24h / `--quick`), бинарник
  `soak_load`, режимы `native` / `iperf3` / `wrk`; CSV с RSS, FD,
  `active_connections`; пример `examples/soak/server.toml`

- **TUN inbound (Linux MVP)** — `[client.tun]` в клиентском режиме: IP-туннель
  поверх VLESS (TCP + UDP через userspace netstack); `VlessClient::handshake_udp`;
  `[client.tun.routing]` (auto `ip rule`/`ip route`, bypass прокси);
  `[client.tun.dns]` (UDP/53 hijack → upstream DNS через VLESS);
  пример `examples/client-vless-tls-tun/`

- **Клиентский режим (MVP)** — `skadicore client`: локальный SOCKS5 → VLESS+TLS
  к удалённому прокси; крейт `skadi-client`, `VlessClient` в `skadi-protocol`;
  пример `examples/client-vless-tls/`, E2E `client_socks5_vless_e2e`

- **XHTTP packet-up** — sequenced POST (`/xhttp/{id}/{seq}`) + GET downlink;
  reassembly в `XhttpSession`, E2E `xhttp_packet_up_e2e`

- **XHTTP stream-up** — GET downlink + POST uplink с session id (`/xhttp/{id}`);
  `XhttpSessionManager`, E2E `xhttp_stream_up_e2e`

- **XHTTP inbound (stream-one MVP)**
  - `skadi-transport::xhttp` — HTTP upgrade поверх TLS/REALITY/plain TCP
  - `[transport.xhttp]` в конфиге: `path`, `mode`, `host`, `x_padding_bytes`
  - E2E: `xhttp_vless_e2e` (VLESS handshake over TLS + XHTTP POST)

- **Кросс-компиляция Windows / macOS**
  - `scripts/build-cross.sh` — Windows GNU (`x86_64-pc-windows-gnu`), macOS (`aarch64`/`x86_64-apple-darwin`)
  - CI job `cross-platform`; release workflow: `.zip` (Windows) + `.tar.gz` (macOS)
  - `.cargo/config.toml` — линкер mingw-w64

- **`skadi_core::Protocol`** — enum SOCKS5/VLESS, wire bytes, `EnabledProtocols::detect()`
  для sniffing; `skadi-server` использует вместо локального `ProtocolKind`

- **Soak-тесты** (`connection_soak_e2e`) — 120 sequential VLESS sessions;
  проверка `skadicore_active_connections == 0` и роста VmRSS; long-вариант `#[ignore]`

- **gRPC API TLS + rate limiting**
  - `[api.tls]` — TLS 1.3 (PEM cert/key) для management API
  - `api.rate_limit_per_sec` — fixed-window лимит RPC/сек (`RESOURCE_EXHAUSTED`)
  - E2E: `grpc_api_over_tls`, `grpc_api_rate_limit`

- **GitHub Releases** — workflow `release.yml`: статические musl-бинарники
  (x86_64 + aarch64), `SHA256SUMS`, публикация при push тега `v*`
- **CI: aarch64 musl** — matrix job `musl` собирает `aarch64-unknown-linux-musl`
  через `cargo-zigbuild` + Zig; smoke test через `qemu-aarch64-static`

- **Примеры конфигурации** (`examples/`):
  - `vless-tls/` — VLESS over TLS + Xray client JSON + `generate-certs.sh`
  - `socks5-tls/` — SOCKS5 user-pass over TLS
  - `README.md` — индекс всех примеров

- **XUDP hit/reconnect** (`skadi-transport::xudp`):
  - GlobalID registry с TTL 60s, reuse cone socket при reconnect
  - E2E `vless_xudp_hit_reconnect`

### Changed

- **Документация актуализирована** (2026-09-11):
  - `TODO.md` — чеклисты этапов 0–10 приведены к текущему коду.
  - `README.md` — TLS/SNI, поток соединения, тесты, дорожная карта.
  - `docs/ARCHITECTURE.md`, `docs/DEVELOPMENT.md` — TLS inbound, e2e-тесты.
  - `.cursor/context.md`, `.cursor/priorities.md` — снимок статуса.

### Added

- **VLESS e2e over TLS**
  - `build_tcp_request` / `build_tcp_domain_request` в `skadi-protocol`.
  - Интеграционные тесты `tls_vless_e2e`: relay и отказ при неверном UUID.

- **SNI-роутинг TLS**
  - `[[transport.tls.certificates]]` с `server_names`, `cert`, `key`.
  - `cert`/`key` верхнего уровня — fallback при неизвестном SNI.
  - `SniCertResolver` в `skadi-transport`.
  - Интеграционные тесты `tls_sni_e2e`.

- **TLS inbound в `skadi-server`**
  - Секция `[transport.tls]` в конфиге: `enabled`, `cert`, `key`, `alpn`.
  - TLS handshake после `accept()`, до протокольного handshake.
  - TLS 1.3 only, загрузка PEM, автоматическая установка ring crypto provider.
  - Интеграционный тест `tls_socks5_e2e`: SOCKS5 CONNECT поверх TLS.

- **`skadi-server` как библиотека**
  - `skadi_server::run()` и `run_server()` для тестов.

- **`.cursor/` — вспомогательные файлы для AI-агентов**
  - `rules.md` — архитектурные ограничения, стиль, антипаттерны.
  - `context.md` — снимок структуры крейтов и текущего статуса.
  - `priorities.md` — синхронизация с `TODO.md`.
  - `workflow.md` — процесс разработки и проверки.

- **CI (GitHub Actions)**
  - `cargo fmt --check`, `clippy`, `test`, `cargo audit`.

- **`skadi-server`**
  - Подключение VLESS наряду с SOCKS5.
  - Sniffing первого байта при включённых обоих протоколах.
  - Валидация конфига (listen addr, UUID, пользователи).

- **`skadi-transport`**
  - Заготовка `TlsTransport` на `rustls` + `tokio-rustls` (загрузка PEM).

### Fixed

- Дублирование `MAX_METHODS` в `socks5.rs` (ошибка компиляции).
- Экспорт модуля `vless` из `skadi-protocol`.

### Changed

- Handlers SOCKS5 и VLESS обобщены: `AsyncRead + AsyncWrite` вместо `TcpStream`.

- **Workspace и структура проекта**
  - Cargo workspace с крейтами `skadi-core`, `skadi-transport`,
    `skadi-protocol`, `skadi-server`.
  - Фиксация версии Rust через `rust-toolchain.toml`.
  - Единый стиль форматирования через `rustfmt.toml`.

- **`skadi-core`**
  - Тип `Endpoint` для представления целевого адреса (IP или домен).
  - Тип `UserId` для идентификации пользователя.
  - Тип `Session` и `SessionId` для отслеживания активных соединений.
  - Иерархия ошибок через `thiserror`.

- **`skadi-transport`**
  - `TcpTransport` с настраиваемым таймаутом подключения.
  - Отключение алгоритма Нейгла (`TCP_NODELAY`) для снижения latency.

- **`skadi-protocol`**
  - **SOCKS5** (RFC 1928):
    - Greeting и выбор метода аутентификации.
    - Метод no-auth (`0x00`).
    - Метод user/pass (RFC 1929).
    - Команда CONNECT для IPv4, IPv6 и доменов.
    - Reply-коды, соответствующие спецификации.
    - Константное сравнение паролей через `subtle::ConstantTimeEq`.
    - Защита от timing-атак на существование пользователя.
    - Лимит на количество методов аутентификации (16).
    - Таймаут на всю фазу переговоров (10 секунд).

  - **VLESS** (version 0):
    - Парсинг заголовка запроса: version, UUID, addons, command,
      port, ATYP, address.
    - UUID-аутентификация за постоянное время.
    - Команда TCP.
    - Ответный заголовок (2 байта).
    - Молчаливое закрытие при неверном UUID — как требует
      спецификация для защиты от активного зондирования.
    - Лимит на размер addons (512 байт).
    - Таймаут на handshake (10 секунд).

  - **Чистые парсеры** для SOCKS5 и VLESS без I/O:
    - `parse_greeting`, `parse_auth`, `parse_request` (SOCKS5).
    - `parse_request` (VLESS).
    - Пригодны для fuzz-тестирования и юнит-тестов без сети.

- **`skadi-server`**
  - Бинарник `skadicore` с CLI на `clap`.
  - Конфигурация через TOML с валидацией.
  - Structured logging через `tracing` в JSON-формате.
  - Graceful shutdown по `Ctrl+C` и `SIGTERM` через `watch`-канал.
  - Таймаут 5 секунд на завершение accept-loop.

- **Тестирование**
  - 13 юнит-тестов для парсеров SOCKS5.
  - 7 юнит-тестов для парсеров VLESS.
  - 3 fuzz-таргета для SOCKS5 (`parse_greeting`, `parse_auth`,
    `parse_request`).
  - 1 fuzz-таргет для VLESS (`parse_vless_request`).

- **Документация**
  - `README.md` с описанием проекта, сборки и конфигурации.
  - `SECURITY.md` с моделью угроз и политикой раскрытия
    уязвимостей.
  - `docs/ARCHITECTURE.md` с описанием слоёв и принятых решений.
  - `docs/PROTOCOLS.md` с деталями SOCKS5 и VLESS.
  - `CHANGELOG.md` (этот файл).

### Changed

- Парсеры SOCKS5 вынесены в отдельный модуль `parse.rs` для
  fuzz-тестирования. Публичный API `Socks5Handler` не изменился.
- `handle_client` в `skadi-server` теперь разветвляется по
  протоколу. На MVP активен только VLESS; SOCKS5 временно
  отключён из-за асимметрии в порядке отправки reply.

### Deprecated

Пока ничего.

### Removed

Пока ничего.

### Fixed

Пока ничего.

### Security

- Реализовано константное сравнение паролей и UUID.
- Добавлены лимиты на все поля парсеров.
- Добавлен таймаут на фазу переговоров.
- VLESS при неверном UUID не отправляет ответ — защита от
  активного зондирования.

**Известные ограничения безопасности** (см. `SECURITY.md`):

- VLESS работает **без TLS**. Трафик идёт в открытом виде.
  Не использовать в реальных условиях.
- Нет rate limiting на подключения.
- Нет изоляции пользователей друг от друга.
- Нет REALITY.

---

## Планы

Ближайшие крупные вещи, которые появятся в следующих записях:

- Интеграция `rustls` для TLS-транспорта.
- Интеграционные тесты полного цикла.
- Prometheus-метрики.
- REALITY через `rustls-reality`.
- gRPC API для управления пользователями.
- UDP over VLESS.
- TUN-режим.
- XHTTP.

---

## Формат записей

### Added
Для новых функций.

### Changed
Для изменений в существующей функциональности.

### Deprecated
Для функций, которые планируется удалить.

### Removed
Для удалённых функций.

### Fixed
Для исправленных багов.

### Security
Для всего, что связано с безопасностью. В том числе — для
описания известных ограничений, если они ещё не закрыты.

---

## Как вести этот файл

1. **Добавляйте запись при каждом значимом изменении.** Не ждите
   релиза — пишите в `[Unreleased]`.
2. **При релизе** — переименуйте `[Unreleased]` в
   `[X.Y.Z] - YYYY-MM-DD` и создайте новый пустой `[Unreleased]`.
3. **Пишите с точки зрения пользователя.** Не «отрефакторил
   парсер», а «парсер теперь не падает на битых пакетах».
4. **Ссылайтесь на issues и PR**, если они есть:
   `- Исправлена утечка памяти в handle_client ([#42])`.
5. **Security-записи** — обязательны. Даже если это «известное
   ограничение», оно должно быть зафиксировано.

<!-- Ссылки на будущие релизы добавляются сюда по мере выхода:
[Unreleased]: https://github.com/yourname/skadicore/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourname/skadicore/releases/tag/v0.1.0
-->