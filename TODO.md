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
  - [x] `skadi-config` — отдельный крейт (server TOML + validation)
- [x] Настроить CI (GitHub Actions):
  - [x] `cargo fmt --check`
  - [x] `cargo clippy -- -D warnings`
  - [x] `cargo test`
  - [x] `cargo audit`
- [x] Настроить `tracing` + `tracing-subscriber` с JSON-форматом
- [x] Базовые типы: `UserId`, `Session`, `Endpoint`
- [x] Тип `Protocol` (enum для SOCKS5/VLESS) + `EnabledProtocols` в `skadi-core`
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
- [x] BIND (опционально, `protocol.socks5.bind`)
- [x] UDP ASSOCIATE (опционально, `protocol.socks5.udp_associate`)
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
- [x] Soak-тесты на утечки памяти (`connection_soak_e2e`, RSS + active_connections)

🟢 **Готово** — tarpaulin/deny/miri/benchmarks/soak в CI.

---

## 📦 Этап 10: Релиз и распространение

- [x] Кросс-компиляция musl (x86_64, CI + `scripts/build-musl.sh`)
- [x] Кросс-компиляция aarch64 musl (CI: cargo-zigbuild)
- [x] Кросс-компиляция (Windows GNU, macOS aarch64/x86_64; CI + release)
- [x] Docker-образ
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

✅ **MVP v1 выполнен** — musl x86_64/aarch64 в CI; soak-тесты в CI.

---

## 📍 Следующие приоритеты

1. ~~**Кросс-компиляция musl** (этап 10)~~ — x86_64 + aarch64 ✅
2. ~~**Кросс-компиляция Windows/macOS** (этап 10)~~ — CI + release ✅
3. ~~**GitHub Releases** (этап 10)~~ — workflow на тег `v*` ✅
4. ~~**UDP/Mux/Vision** (этап 4)~~ — UDP + Mux TCP/UDP/XUDP + flow reject + XUDP hit ✅
5. ~~**TLS outbound** (этап 3)~~ ✅
6. ~~**cargo miri + criterion** (этап 9)~~ ✅
7. ~~**Smoke-доки**~~ ✅ — `curl --socks5`, `openssl s_client` в DEVELOPMENT.md

---

## 📚 Референсы

- **Xray-core** — gRPC API, REALITY
- **sing-box** — TUN, минимализм
- **rustls-reality** — REALITY для Rust
- **fast-socks5** — SOCKS5 на Rust
- **Tokio** — async runtime

---

## 💬 Обратная связь (внешний аудит, 2026-09-15)

> Сохранено для последующей проработки. Часть пунктов может пересекаться с уже
> выполненными задачами в этапах 9–10 — сверять с актуальным кодом перед работой.

### Чего не хватает, помимо того, что уже указано в доках?

---

Я изучил ваш репозиторий на GitHub. Вы проделали огромную работу: реализованы TCP-транспорт, SOCKS5, VLESS, TLS с SNI-роутингом, REALITY, gRPC API, метрики, CI и даже клиентский режим с TUN. Многие вещи, которые я бы отметил как отсутствующие, у вас уже есть.

Поэтому я сфокусируюсь на том, чего **не хватает за пределами уже заявленного в документации**. Я разделю ответ на несколько категорий: архитектура и код, безопасность, тестирование, документация и процессы, инфраструктура и релизы, а также упущенные угрозы.

---

### 1. Архитектура и код

#### 1.1. `skadi-core` как «тонкий» слой типов

В README заявлено, что `skadi-core` не знает ничего про протоколы и транспорты, только типы: `Endpoint`, `Session`, `UserId`, `Error`. Это хороший принцип, но на практике такой «тонкий» слой часто становится «мусорным»: в него начинают протекать зависимости от `tokio`, `serde`, `tracing` и т.д. В `Cargo.toml` `skadi-core` уже зависят от `tokio`, `serde`, `tracing` и `thiserror`.

**Чего не хватает:** явного **`Cargo.toml` с фиксацией feature flags**. Если `skadi-core` не должен знать про `tokio`, то `tokio` там быть не должно. Если он используется только для `SocketAddr` — можно взять `std::net::SocketAddr` и не тащить весь runtime. Это важно, потому что `skadi-core` — единственный крейт, который потенциально переиспользуется в клиенте, и чем меньше у него зависимостей, тем лучше.

- [ ] Убрать лишние зависимости из `skadi-core` (tokio → `std::net::SocketAddr` где возможно)
- [ ] Зафиксировать feature flags в `Cargo.toml` для `skadi-core`

#### 1.2. Отсутствие трейтов для протоколов и транспортов

Сейчас `VlessHandler::handshake` и `Socks5Handler::negotiate` — это конкретные типы. В `handle_client` жёстко прописаны ветки `if vless { ... } else if socks5 { ... }`. Это работает для двух протоколов, но при добавлении Trojan, Shadowsocks или Hysteria придётся каждый раз править `handle_client`.

**Чего не хватает:** трейта `InboundHandler` с методом `handshake<S: AsyncRead + AsyncWrite + Unpin>(stream: &mut S, config: &Self::Config) -> Result<Endpoint>` и трейта `Transport` с методом `connect(endpoint) -> Result<impl AsyncRead + AsyncWrite>`. Тогда `handle_client` становится generic-функцией, а добавление нового протокола — это новый impl трейта, а не правка ядра.

- [x] Трейт `InboundHandler` для протоколов (`skadi-protocol::inbound`)
- [x] Трейт `OutboundTransport` для транспортов (`skadi-transport::connect`)
- [x] Рефакторинг `handle_connection` под `handshake_inbound` + `OutboundTransport::connect`

#### 1.3. `rustls-reality` в `third_party/`

Вы вендорите `rustls-reality` в `third_party/rustls-reality`. Это разумно, потому что `rustls-reality` — это форк `rustls`, и его нужно синхронизировать с upstream. Но вендоринг создаёт **supply chain risk**: вы фиксируете конкретный коммит, но не отслеживаете CVE в `rustls`, которые исправляются в upstream. В `Cargo.lock` `rustls-webpki` обновлён до 0.103.15, но что с остальными транзитивными зависимостями?

**Статус:** pin в `third_party/rustls-reality/UPSTREAM.toml`, weekly workflow
`sync-rustls-reality.yml` (`scripts/check-rustls-reality-upstream.sh`),
корневые `renovate.json` (regex → `rustls/rustls` releases) и
`.github/dependabot.yml` (cargo + github-actions). Полный rebase дерева
на `0.23.x` — отдельная задача (см. `UPSTREAM.md`).

- [x] CI/Action для синхронизации `third_party/rustls-reality` с upstream
- [x] `renovate.json` / `dependabot.yml` для `third_party/`

#### 1.4. `examples/` без тестов

В репозитории есть директория `examples/`. Судя по коммитам, там лежат примеры конфигов для UDP/Mux/XUDP и TLS outbound. Но нет **интеграционных тестов**, которые проверяют, что эти примеры действительно работают с реальными клиентами (Xray-core, v2rayNG, Nekoray).

**Чего не хватает:** CI-шага, который поднимает `xray-core` в Docker и прогоняет через SkadiCore реальный трафик. Это единственный способ поймать несовместимость с клиентами до релиза.

- [x] CI: интеграционные тесты examples/ с xray-core (Docker) — `scripts/examples-xray-docker-e2e.sh`, job `examples-xray-e2e`

---

### 2. Безопасность

#### 2.1. Rate limiting на аутентификацию

В документации указано, что нет rate limiting на подключения. Но отдельно не сказано про **rate limiting на неудачные аутентификации**. UUID — 128 бит, перебор невозможен, но если UUID генерируются слабым RNG или утекают через логи, атакующий может перебирать их с одного IP.

**Чего не хватает:** per-IP счётчика неудачных аутентификаций с экспоненциальным backoff. Даже простой `HashMap<IpAddr, u32>` с TTL в 10 минут закроет эту дыру.

- [x] Per-IP rate limiting на неудачные аутентификации (экспоненциальный backoff)

#### 2.2. Фильтрация внутренних адресов

В `Endpoint` может быть `127.0.0.1`, `10.0.0.0/8`, `::1` и т.д. Если пользователь подключится к `127.0.0.1:22` через ваш прокси, он получит доступ к SSH на самом сервере.

**Чего не хватает:** функции `is_forbidden(ip: IpAddr) -> bool`, которая блокирует loopback, private, link-local и unique-local адреса. Это должно быть **по умолчанию**, с возможностью отключить через конфиг (`allow_private = false`).

- [x] `is_forbidden(ip)` — блокировка loopback/private/link-local по умолчанию
- [x] Опция конфига `allow_private`

#### 2.3. Утечка UUID в логи

В `VlessHandler::handshake` при неудачной аутентификации логируется `uuid = ?uuid`. UUID — это идентификатор, но в контексте обхода блокировок он фактически является **паролем**. Если логи утекают (например, через неправильно настроенный ELK), атакующий получает список валидных UUID.

**Чего не хватает:** логирования хеша UUID вместо самого UUID. Или, как минимум, уровня `debug`, а не `warn`.

- [x] Не логировать UUID в plaintext (хеш или debug-уровень)

#### 2.4. TLS: `dangerous_configuration` и проверка сертификатов

При использовании `rustls` на клиентской стороне есть соблазн отключить проверку сертификата через `dangerous_configuration`. В `rustls-reality` это делается через кастомный `ServerCertVerifier`. Если этот `Verifier` реализован неверно (например, возвращает `Ok` для любого сертификата), REALITY превращается в открытый релей.

**Чего не хватает:** явного аудита `ServerCertVerifier` в `rustls-reality`. Нужно убедиться, что он проверяет подпись временного ключа, а не просто «доверяет всему».

- [x] Аудит `ServerCertVerifier` в `rustls-reality` — `RealityServerCertVerifier` (HMAC-SHA512 хвост)

#### 2.5. gRPC API: аутентификация и авторизация

gRPC API слушает на `127.0.0.1` с Bearer-токеном. Но что если токен утекает через `/metrics`? Или через `--log-format=json`? В README не сказано, что токен маскируется в логах.

**Чего не хватает:** маскирования токена в логах и метриках. А также **аудита всех изменений** через gRPC: кто, когда, какого пользователя добавил/удалил.

- [x] Маскирование Bearer-токена в логах и метриках
- [x] Аудит изменений через gRPC API (`api.audit_log`, `skadi.grpc.audit`)

#### 2.6. Supply chain: `cargo deny` и `cargo audit`

В CI есть `audit` и `deny`. Но `cargo deny` по умолчанию проверяет только лицензии и дубликаты. Он не проверяет **источники крейтов** (git vs crates.io), **минимальные версии** и **запрещённые зависимости**.

**Чего не хватает:** `deny.toml` с правилами:
- `[bans]` — запрет на конкретные крейты (например, `openssl`, `native-tls`).
- `[sources]` — разрешить только `crates.io` и конкретные git-репозитории.
- `[advisories]` — уровень `deny` для всех уязвимостей.

- [x] Расширить `deny.toml`: `[bans]`, `[sources]`, `[advisories]` (сверить с текущим)

---

### 3. Тестирование

#### 3.1. Property-based тестирование

Юнит-тесты и fuzz-тесты — это хорошо. Но есть класс ошибок, которые они не ловят: **инварианты протокола**. Например, «если парсер вернул `Ok`, то количество потреблённых байт не превышает длину входа» или «если парсер вернул `Incomplete`, то он не аллоцировал память».

**Чего не хватает:** `proptest` или `quickcheck` для property-based тестов.

- [x] Property-based тесты (`proptest`) для парсеров SOCKS5/VLESS/Mux

#### 3.2. Тесты на утечки памяти

Fuzzer может найти uтечки, но только если запущен с AddressSanitizer. В CI fuzz-таргеты, скорее всего, запускаются без санитайзеров.

**Чего не хватает:** отдельного CI-шага с `RUSTFLAGS="-Zsanitizer=address"` на nightly.

- [ ] CI-шаг с AddressSanitizer (nightly)

#### 3.3. Soak-тесты

README упоминает, что есть 29+ автотестов. Но нет **24-часового soak-теста** под нагрузкой, который проверяет, что ядро не течёт по памяти, не исчерпывает файловые дескрипторы и не деградирует по latency.

**Чего не хватает:** скрипта `scripts/soak.sh`, который запускает SkadiCore, подаёт трафик через `wrk` или `iperf3` в течение 24 часов и снимает метрики RSS, FD count, latency.

- [x] 24-часовой soak-тест (`scripts/soak.sh`, `soak_load`, wrk/iperf3, RSS/FD/latency)

#### 3.4. Тесты на активное зондирование

REALITY реализован, но нет тестов, которые проверяют, что **активный зонд не может отличить SkadiCore от настоящего сайта**.

**Чего не хватает:** скрипта `scripts/probe-test.sh` (JA3/JA4, тайминги handshake ±10%).

- [x] `scripts/probe-test.sh` — JA3 (опц.) и тайминги handshake vs реальный dest

---

### 4. Документация и процессы

#### 4.1. `CHANGELOG.md` не следует Keep a Changelog

**Чего не хватает:** переформатирования `CHANGELOG.md` под [Keep a Changelog](https://keepachangelog.com/).

- [x] `CHANGELOG.md` → формат Keep a Changelog (`[Unreleased]` / `[X.Y.Z]`)

#### 4.2. `CONTRIBUTING.md` отсутствует

- [x] Минимальный `CONTRIBUTING.md` (тесты, протоколы, ревью, коммиты)
- [x] `docs/RELEASING.md` — теги `v*` и GitHub Releases (`v0.1.0`)

#### 4.3. `CODE_OF_CONDUCT.md` отсутствует

- [x] `CODE_OF_CONDUCT.md` (Contributor Covenant)

#### 4.4. Документация для AI-агентов

- [ ] Отдельный документ с правилами для AI-агентов (файлы, тесты, коммиты)

---

### 5. Инфраструктура и релизы

#### 5.1. `Dockerfile` отсутствует

- [x] Multi-stage `Dockerfile` (musl, `FROM scratch`)

#### 5.2. `rust-toolchain.toml` не зафиксирован

- [x] Явный `rust-toolchain.toml` (channel, rustfmt, clippy)

#### 5.3. Подпись релизов

- [x] Подпись релизов (`minisign`) + `scripts/sign-release.sh` / `verify-release.sh` + README

#### 5.4. Воспроизводимые сборки

- [x] Dockerfile для воспроизводимой musl-сборки (фиксированные musl + rustc)

---

### 6. Упущенные угрозы

#### 6.1. ТСПУ и IPv6

- [x] IPv6 в `listen` (`[::]:443`, dual-stack массив) + предупреждение при `0.0.0.0` без `[::]`

#### 6.2. DNS-over-HTTPS и DNS-утечки

- [x] Предупреждения при старте клиента (`socks5` vs `socks5h`, TUN `dns.hijack`)
- [x] DoH-резолвер в TUN (`client.tun.dns.mode = "doh"`, RFC 8484 через VLESS)
- [x] DoT upstream в TUN (`client.tun.dns.mode = "dot"`, RFC 7858 через VLESS)
- [x] Блокировка системного DoT/DoH (`block_system_dot`, `block_system_doh`)

#### 6.3. MTU и фрагментация

- [x] MTU в конфиге TUN + рекомендация `1400` + предупреждение при `mtu >= 1500`
- [x] Path MTU Discovery Phase 1 (default MTU 1400, `pmtud = "probe"` до прокси)
- [x] Runtime ICMP PMTUD в userspace netstack (Phase 2)

#### 6.4. Квантовые атаки на REALITY

- [x] Документация: REALITY не защищён от store-now-decrypt-later (`docs/RISKS.md` §1.5, `SECURITY.md`)
- [x] Гибридный KEX X25519MLKEM768 для обычного TLS (`kex_mode = "hybrid_pq"`, RFC 10024)
- [x] PQ-KEX для REALITY (`transport.reality.kex_mode = "hybrid_pq"`)
- [ ] Расширенная crypto-agility (доп. KEM / провайдеры)

---

### Приоритеты (из аудита)

1. **Rate limiting на аутентификацию**
2. **Фильтрация внутренних адресов**
3. **`rust-toolchain.toml` и `Dockerfile`**
4. **Property-based тесты**
5. ~~**Soak-тест (24 ч)**~~ — ✅ `scripts/soak.sh`
6. ~~**Подпись релизов**~~ — ✅ minisign
7. ~~**IPv6 в `listen`**~~ — ✅ dual-stack

---

### Что ещё можно добавить для обхода блокировок?

Судя по репозиторию, фундамент уже силён: VLESS+REALITY, TLS, gRPC API, TUN. Ниже — направления устойчивости к методам ТСПУ (2025–2026).

#### 🚀 Транспорт и маскировка: XHTTP вместо чистого TCP

Классический VLESS+REALITY через TCP деградирует: ТСПУ анализирует TLS-хендшейки и троттлит потоки. **XHTTP** разбивает туннель на HTTP-запросы, имитируя браузер. С REALITY — обычный HTTPS. При XHTTP `flow` (xtls-rprx-vision) должен быть пустым.

**AmneziaWG** — обфусцированный WireGuard, стабильно проходит ТСПУ (2026).

- [x] XHTTP-транспорт (+ REALITY, `password` в realitySettings; пример + E2E)
- [x] XHTTP **клиент** (`[remote.xhttp]`, stream-one/stream-up/packet-up, native e2e)
- [x] AmneziaWG как альтернативный транспорт (MVP: `[transport.awg]` + amneziawg-go backend)

#### 🛡️ REALITY-rkn-fix

ТСПУ детектирует REALITY по статическому отпечатку сертификата (SerialNumber=0, пустые Subject/Issuer и т.д.).

**REALITY-rkn-fix** (`fwflunky/REALITY-rkn-fix`):
1. Свежий ed25519-сертификат на каждое соединение (реалистичные X.509 поля).
2. `ImpersonateCert` — повтор DER целевого сайта.

- [x] REALITY-rkn-fix: per-connection certs + `ImpersonateCert`

#### 🌐 Hysteria2 и TUIC

QUIC/UDP — другой профиль трафика, альтернатива при деградации TCP.

- [x] Hysteria2 (MVP: `[transport.hysteria2]` + hysteria binary backend)
- [x] TUIC (MVP: `[transport.tuic]` + tuic-server binary backend)
- [x] Hysteria2 / TUIC **client** (`[hysteria2]` / `[tuic]` → локальный SOCKS5)
- [x] Traffic e2e Hy2/TUIC в CI (`sidecar-traffic-e2e`, реальные бинарники)
- [x] Traffic e2e AWG в CI (netns + `amneziawg-go` / `awg`)

#### ⚡ Обход троттлинга и фрагментация (sonicdpi)

Примитивы: `fake`+`fooling`, `multisplit`, `fake,multidisorder`, `hostfakesplit`. Требуют raw sockets / NFQUEUE — для userspace TCP-прокси неэффективны; либо NFQUEUE/SO_ORIGINAL_DST, либо рекомендация `zapret` / `ByeDPI` / `SpoofDPI`.

- [ ] Документировать/интегрировать обход троттлинга (клиентские утилиты или NFQUEUE)

#### 🌍 IPv6-блокировки

- [x] `[::]:443` + dual-stack `listen` на сервере
- [ ] Корректные клиентские конфиги IPv6

#### 🔐 Пост-квантовая устойчивость

X25519 + ML-KEM-768 (гибрид, напр. Qeli / Chrome). Долгосрочно — заложить смену криптопримитивов.

- [ ] Архитектура для замены KEX; документация PQ-рисков

#### 💎 Приоритеты (обход блокировок)

1. ~~**XHTTP-транспорт**~~ — ✅
2. ~~**REALITY-rkn-fix**~~ — ✅
3. ~~**AmneziaWG**~~ — ✅ MVP
4. ~~**Hysteria2 / TUIC**~~ — ✅ server + client sidecar
