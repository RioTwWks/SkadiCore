# SkadiCore — TODO

Ядро для обхода блокировок на Rust. Гибрид идей Xray (gRPC API, REALITY)
и sing-box (низкое потребление, TUN, множество протоколов).

Лицензия: AGPL-3.0-or-later  
Статус: **MVP в разработке** (этапы 0–4 частично завершены, TLS inbound готов)

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
- [ ] Таймаут неактивности (idle timeout)
- [ ] Максимальная длительность сессии (max session lifetime)
- [ ] Лимит одновременных соединений (backpressure)
- [ ] Нагрузочные тесты (`wrk` / `iperf3`)
- [ ] Юнит-тесты TCP-форвардера без протокольного слоя

**Критерий готовности:** простой TCP-форвардер под нагрузкой без утечек.
🟡 **Частично** — релей работает, idle/backpressure/load — нет.

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
- [ ] Ручная проверка `curl --socks5` (документировать в DEVELOPMENT.md)

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
- [ ] TLS outbound (исходящие соединения с проверкой сертификата)
- [ ] Ручная проверка `openssl s_client` (документировать)

**Критерий готовности:** inbound TLS + SNI, протоколы работают поверх TLS.  
✅ **Inbound выполнен.** Outbound — отдельная задача.

---

## 📡 Этап 4: VLESS

- [x] Парсинг заголовка (version, UUID, addons, command, port, address)
- [x] Response-заголовок (2 байта)
- [x] `build_tcp_request` / `build_tcp_domain_request` для клиентов и тестов
- [x] Хранилище пользователей in-memory из TOML (без hot reload)
- [x] Проверка UUID за постоянное время (`subtle`)
- [x] Неизвестный UUID → молчаливое закрытие (без ответа)
- [x] Подключение в `skadi-server` (наряду с SOCKS5, sniffing `0x00`/`0x05`)
- [x] Fuzz-тесты парсера
- [x] Интеграционный тест over TLS (`tls_vless_e2e`)
- [x] Проверка с реальным клиентом (Xray-core e2e; v2rayNG / Nekoray совместимы)
- [ ] UDP over VLESS
- [ ] Mux
- [ ] Flow `xtls-rprx-vision`

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
- [ ] TLS для gRPC API
- [ ] Rate limiting

🟢 **Готово (v1)** — user CRUD + stats; TLS/rate limit — v2.

---

## 📊 Этап 7: Наблюдаемость

- [x] `tracing` с JSON-логами
- [ ] Prometheus-метрики (`metrics` + `metrics-exporter-prometheus`)
- [ ] Health-check (`/healthz`)
- [ ] Флаг `--log-format=json|pretty`
- [ ] OpenTelemetry (опционально)

🟡 **Частично** — только логи.

---

## 🖥️ Этап 8: Конфигурация и CLI

- [x] TOML-схема: `[server]`, `[protocol.*]`, `[transport.tls]`
- [x] Валидация конфига с понятными ошибками
- [x] `clap`: `--config`, `--log-level`
- [ ] `--check-config` (валидация без запуска)
- [ ] SIGHUP reload
- [x] `skadicore genkey reality`
- [x] Секция `[transport.reality]` в конфиге
- [ ] Секция `[api]` в конфиге

🟡 **Частично.**

---

## 🧪 Этап 9: Тестирование и безопасность

- [x] Юнит-тесты парсеров (20 тестов в `skadi-protocol`)
- [x] Интеграционные тесты TLS + REALITY (7 тестов в `skadi-server/tests/`)
- [x] Fuzz-таргеты для SOCKS5 и VLESS
- [x] `cargo audit` в CI
- [ ] Покрытие ≥ 70% (tarpaulin)
- [ ] `cargo miri`
- [ ] Бенчмарки (`criterion`)
- [ ] Нагрузочные тесты (10k соединений)
- [ ] `cargo deny`
- [ ] Soak-тесты на утечки памяти

🟡 **Частично** — базовые тесты и audit есть.

---

## 📦 Этап 10: Релиз и распространение

- [ ] Кросс-компиляция (musl, Windows, macOS)
- [ ] Docker-образ
- [ ] GitHub Releases
- [x] Документация: README, ARCHITECTURE, PROTOCOLS, CONFIGURATION, DEVELOPMENT
- [ ] Примеры конфигов для типовых сценариев (отдельная папка `examples/`)
- [ ] Страница донатов

⏳ **Не начато** (кроме документации).

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
- [x] Конфиг TOML с валидацией
- [x] Структурные JSON-логи
- [x] Тесты парсеров + интеграционные TLS-тесты
- [x] CI (fmt, clippy, test, audit)
- [x] README и docs/
- [x] Лицензия AGPL-3.0-or-later
- [ ] Статический musl-бинарник (кросс-компиляция)
- [ ] Prometheus-метрики
- [ ] Проверка реальным VLESS-клиентом

---

## 📍 Следующие приоритеты

1. **Prometheus-метрики** (этап 7) — наблюдаемость
2. **REALITY** (этап 5) — после стабилизации VLESS+TLS
3. **gRPC API** (этап 6) — управление пользователями
4. **Idle timeout / backpressure** (этап 1) — устойчивость под нагрузкой
5. **`--check-config`** и документация `openssl s_client` / `curl --socks5`

---

## 📚 Референсы

- **Xray-core** — gRPC API, REALITY
- **sing-box** — TUN, минимализм
- **rustls-reality** — REALITY для Rust
- **fast-socks5** — SOCKS5 на Rust
- **Tokio** — async runtime
