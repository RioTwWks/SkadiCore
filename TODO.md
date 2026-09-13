# SkadiCore — TODO

Ядро для обхода блокировок на Rust. Гибрид идей Xray (gRPC API, REALITY)
и sing-box (низкое потребление, TUN, множество протоколов).

Лицензия: AGPL-3.0-or-later  
Статус: **MVP v1 готов** (этапы 0–8 завершены; этапы 9–10 — в работе)

> Последняя актуализация: 2026-09-11. См. также `README.md` и `.cursor/context.md`.

---

## 🎯 Философия проекта

- **Безопасность прежде всего.** Никакой своей криптографии — только
  проверенные крейты (`rustls`, `ring`, `aes-gcm`).
- **Модульность.** Каждый компонент — отдельный крейт с чётким API.
- **Zero-copy где возможно.** Минимум аллокаций на горячем пути.
- **Один бинарник.** Ядро должно собираться в один статический бинарник
  без внешних зависимостей (musl target).
- **Наблюдаемость.** Структурные логи, метрики, трейсинг с первого дня.

---

## 📐 Этап 0: Фундамент

- [x] Инициализировать workspace с крейтами:
  - [x] `skadi-core` — трейты, типы, ошибки
  - [x] `skadi-transport` — TCP, TLS inbound
  - [x] `skadi-protocol` — SOCKS5, VLESS
  - [x] `skadi-server` — бинарник + библиотека `skadi_server`
  - [ ] `skadi-config` — отдельный крейт (сейчас конфиг в `skadi-server`)
- [x] Настроить CI (GitHub Actions):
  - [x] `cargo fmt --check`
  - [x] `cargo clippy -- -D warnings`
  - [x] `cargo test`
  - [x] `cargo audit`
- [x] Настроить `tracing` + `tracing-subscriber` с JSON-форматом
- [x] Базовые типы: `UserId`, `Session`, `Endpoint`
- [ ] Тип `Protocol` (enum для SOCKS5/VLESS) — пока sniffing по первому байту
- [x] Иерархия ошибок через `thiserror`, контекст через `anyhow`
- [x] `.cursor/` — вспомогательные файлы для AI-агентов

**Критерий готовности:** `cargo run` запускается, пишет структурированный
лог, CI зелёный. ✅ **Выполнен.**

---

## 🔌 Этап 1: TCP-прокси

- [x] Реализовать `TcpListener` с graceful shutdown (`Ctrl+C`, `SIGTERM`)
- [x] Обработка каждого соединения в отдельной задаче tokio
- [x] `copy_bidirectional` между inbound и outbound
- [x] Таймаут на установку соединения (connect timeout, 10 с)
- [x] Таймаут неактивности (idle timeout)
- [x] Максимальная длительность сессии (max session lifetime)
- [x] Лимит одновременных соединений (backpressure)
- [x] Нагрузочные тесты (Rust e2e: 64 conn в CI, 512 — `#[ignore]`)
- [ ] Юнит-тесты TCP-форвардера без протокольного слоя

**Критерий готовности:** простой TCP-форвардер под нагрузкой без утечек.
🟢 **Готово** — релей, idle/lifetime, max_connections, базовые load-тесты.

---

## 🧦 Этап 2: SOCKS5

- [x] Парсинг handshake (greeting, auth, request) — чистые функции в `parse.rs`
- [x] CONNECT (IPv4, IPv6, домен)
- [ ] BIND (опционально)
- [ ] UDP ASSOCIATE (опционально)
- [x] Аутентификация user-pass (RFC 1929), constant-time сравнение
- [x] Reply-коды
- [x] Fuzz-тестирование парсера (`cargo-fuzz`)
- [x] Интеграционный тест over TLS (`tls_socks5_e2e`)
- [x] Ручная проверка `curl --socks5` (DEVELOPMENT.md + `examples/`)

**Критерий готовности:** `curl --socks5` через plain TCP или TLS.  
🟡 **Частично** — автотесты есть, curl smoke test не задокументирован.

---

## 🔐 Этап 3: TLS-транспорт (inbound)

- [x] Интегрировать `rustls` + `tokio-rustls`
- [x] Загрузка сертификатов (PEM) из конфига `[transport.tls]`
- [x] SNI-роутинг (`[[transport.tls.certificates]]` + fallback `cert`/`key`)
- [x] ALPN (`h2`, `http/1.1`)
- [x] Только TLS 1.3
- [x] Интеграционные тесты:
  - [x] SOCKS5 over TLS (`tls_socks5_e2e`)
  - [x] SNI routing (`tls_sni_e2e`)
  - [x] VLESS over TLS (`tls_vless_e2e`)
- [x] TLS outbound (исходящие соединения с проверкой сертификата)
- [x] Интеграционный тест `tls_outbound_e2e`
- [x] Ручная проверка `openssl s_client` (документировать в DEVELOPMENT.md)

**Критерий готовности:** inbound TLS + SNI, протоколы работают поверх TLS; outbound TLS с проверкой CA.  
✅ **Выполнено.**

---

## 📡 Этап 4: VLESS

- [x] Парсинг заголовка (version, UUID, addons, command, port, address)
- [x] Response-заголовок (2 байта)
- [x] `build_tcp_request` / `build_tcp_domain_request` для клиентов и тестов
- [x] Хранилище пользователей in-memory из TOML
- [x] Hot reload пользователей через gRPC API и SIGHUP (`UserStore`)
- [x] Проверка UUID за постоянное время (`subtle`)
- [x] Неизвестный UUID → молчаливое закрытие (без ответа)
- [x] Подключение в `skadi-server` (наряду с SOCKS5, sniffing `0x00`/`0x05`)
- [x] Fuzz-тесты парсера
- [x] Интеграционный тест over TLS (`tls_vless_e2e`)
- [x] Проверка с реальным клиентом (Xray-core e2e; v2rayNG / Nekoray совместимы)
- [x] UDP over VLESS
- [x] Mux (TCP/UDP/XUDP, Xray wire format + XUDP hit/reconnect)
- [x] Flow `xtls-rprx-vision` (явный reject + парсинг addons)

**Критерий готовности:** VLESS TCP over TLS, клиент v2rayNG работает.  
🟢 **Готово** — TLS e2e + REALITY e2e с Xray-core (v2rayNG/Nekoray совместимы).

---

## 🎭 Этап 5: REALITY (самый сложный)

- [x] Изучить `rustls-reality` (vendored в `third_party/rustls-reality`)
- [x] Интеграция REALITY в TLS-слой (`skadi-transport::reality`)
- [x] Fallback на реальный сайт при неверном handshake
- [x] Настройка `short_ids`, `server_names`, `dest`, `private_key`
- [x] Интеграционные тесты fallback (`reality_fallback_e2e`)
- [x] `skadicore genkey reality` — генерация X25519 keypair + shortId
- [x] E2E с реальным VLESS+REALITY клиентом (`reality_vless_xray_e2e`, Xray-core)
- [x] Пример конфигурации (`examples/reality-vless/`)
- [ ] Тесты против активного зондирования (ручная проверка)

**Критерий готовности:** зонд не отличает сервер от `dest`.  
🟢 **Готово** — сервер, fallback, Xray e2e; ручная проверка зондирования — по желанию.

---

## 🔄 Этап 6: gRPC API

- [x] Protobuf-схема (`crates/skadi-api/proto/skadi.proto`)
- [x] Генерация через `tonic` + `protoc-bin-vendored`
- [x] Hot reload VLESS/SOCKS5 пользователей (`UserStore`)
- [x] `GetStats` — счётчики пользователей
- [x] Защита: loopback bind + Bearer token
- [x] E2E тест `grpc_api_e2e`
- [x] TLS для gRPC API (`[api.tls]`, PEM cert/key)
- [x] Rate limiting (`api.rate_limit_per_sec`)

🟢 **Готово (v1)** — user CRUD + stats + TLS + rate limit.

---

## 📊 Этап 7: Наблюдаемость

- [x] `tracing` с JSON-логами
- [x] Prometheus-метрики (`metrics` + `metrics-exporter-prometheus`)
- [x] Health-check (`/healthz`)
- [x] Флаг `--log-format=json|pretty`
- [x] E2E `metrics_e2e`
- [ ] OpenTelemetry (опционально)

🟢 **Готово (v1)** — метрики, healthz, log format; OTel — опционально.

---

## 🖥️ Этап 8: Конфигурация и CLI

- [x] TOML-схема: `[server]`, `[protocol.*]`, `[transport.tls]`
- [x] Валидация конфига с понятными ошибками
- [x] `clap`: `--config`, `--log-level`, `--log-format`
- [x] `skadicore check-config` (валидация без запуска)
- [x] SIGHUP reload `[protocol.*]` (пользователи + enabled)
- [x] `skadicore genkey reality`
- [x] Секции `[transport.reality]`, `[api]`, `[metrics]` в конфиге
- [x] E2E: `check_config_cli`, `sighup_reload`

🟢 **Готово (v1)** — check-config + SIGHUP для protocol; transport/listen — рестарт.

---

## 🧪 Этап 9: Тестирование и безопасность

- [x] Юнит-тесты парсеров (20+ тестов в `skadi-protocol`)
- [x] Интеграционные тесты TLS + REALITY + gRPC + metrics (9 файлов в `skadi-server/tests/`)
- [x] Fuzz-таргеты для SOCKS5 и VLESS
- [x] `cargo audit` в CI
- [x] Покрытие ≥ 70% (tarpaulin, CI, exclude third_party)
- [x] `cargo miri` (CI: skadi-protocol + connection_gate)
- [x] Бенчмарки (`criterion`: parse + relay)
- [x] Нагрузочные тесты (64 conn CI, 512 `#[ignore]`; 10k — ручной прогон)
- [x] `cargo deny` (CI + deny.toml)
- [ ] Soak-тесты на утечки памяти

🟡 **Частично** — tarpaulin/deny/miri/benchmarks в CI; soak — нет.

---

## 📦 Этап 10: Релиз и распространение

- [x] Кросс-компиляция musl (x86_64, CI + `scripts/build-musl.sh`)
- [x] Кросс-компиляция aarch64 musl (CI: cargo-zigbuild)
- [ ] Кросс-компиляция (Windows, macOS)
- [ ] Docker-образ
- [x] GitHub Releases (workflow `release.yml` на тег `v*`)
- [x] Документация: README, ARCHITECTURE, PROTOCOLS, CONFIGURATION, DEVELOPMENT
- [x] Примеры конфигов для типовых сценариев (`examples/reality-vless/`, `vless-tls/`, `socks5-tls/`)
- [ ] Страница донатов

🟡 **Частично** — документация и один пример REALITY+VLESS.

---

## 🚫 Что НЕ делать (антипаттерны)

- ❌ Своя криптография
- ❌ Свой TLS/REALITY с нуля
- ❌ `unwrap()` / `expect()` на горячем пути
- ❌ `unsafe` без `// SAFETY:`
- ❌ Логи с паролями, UUID, ключами
- ❌ Отключение проверки TLS
- ❌ Блокирующий I/O в async без `spawn_blocking`

---

## 🎯 Definition of Done для MVP

- [x] Собирается release-бинарник (`cargo build --release`)
- [x] TCP-прокси + SOCKS5 + VLESS TCP
- [x] TLS inbound (опционально через `[transport.tls]`)
- [x] REALITY inbound (`[transport.reality]`)
- [x] Конфиг TOML с валидацией
- [x] Структурные JSON-логи
- [x] Тесты парсеров + интеграционные TLS/REALITY/gRPC e2e
- [x] CI (fmt, clippy, test, audit)
- [x] README и docs/
- [x] Лицензия AGPL-3.0-or-later
- [x] Prometheus-метрики (`/metrics`, `/healthz`)
- [x] gRPC API (hot reload пользователей)
- [x] `skadicore check-config` + SIGHUP reload
- [x] Проверка реальным VLESS-клиентом (Xray-core e2e)
- [x] Статический musl-бинарник (x86_64, CI)

✅ **MVP v1 выполнен** — musl x86_64 в CI; soak — этап 9.

---

## 📍 Следующие приоритеты

1. ~~**Кросс-компиляция musl** (этап 10)~~ — x86_64 + aarch64 ✅
2. ~~**GitHub Releases** (этап 10)~~ — workflow на тег `v*` ✅
3. ~~**UDP/Mux/Vision** (этап 4)~~ — UDP + Mux TCP/UDP/XUDP + flow reject + XUDP hit ✅
4. ~~**TLS outbound** (этап 3)~~ ✅
5. ~~**cargo miri + criterion** (этап 9)~~ ✅
6. ~~**Smoke-доки**~~ ✅ — `curl --socks5`, `openssl s_client` в DEVELOPMENT.md

---

## 📚 Референсы

- **Xray-core** — gRPC API, REALITY
- **sing-box** — TUN, минимализм
- **rustls-reality** — REALITY для Rust
- **fast-socks5** — SOCKS5 на Rust
- **Tokio** — async runtime
