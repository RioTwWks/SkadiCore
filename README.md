# SkadiCore

Ядро для обхода блокировок на Rust. Гибрид идей Xray (gRPC API, REALITY)
и sing-box (низкое потребление ресурсов, встроенный TUN, поддержка
множества протоколов).

**Статус**: ранний MVP. Не использовать в продакшене.

---

## Содержание

- [Что это](#что-это)
- [Текущее состояние](#текущее-состояние)
- [Архитектура](#архитектура)
- [Сборка и запуск](#сборка-и-запуск)
- [Конфигурация](#конфигурация)
- [Поддерживаемые протоколы](#поддерживаемые-протоколы)
- [Разработка](#разработка)
- [Безопасность](#безопасность)
- [Дорожная карта](#дорожная-карта)
- [Лицензия](#лицензия)

---

## Что это

SkadiCore — это попытка собрать в одном ядре сильные стороны двух
популярных проектов и не тащить их слабые места:

| Что берём у Xray | Что берём у sing-box |
|---|---|
| gRPC API для динамического управления пользователями | Низкое потребление памяти и CPU |
| REALITY как транспорт маскировки | Встроенный TUN-режим |
| Зрелый VLESS + XHTTP | Поддержка множества протоколов в одном бинарнике |

Цели проекта:

- **Один статический бинарник** без внешних зависимостей.
- **Безопасность прежде всего**: никакой своей криптографии, только
  проверенные крейты (`rustls`, `ring`, `aes-gcm`).
- **Наблюдаемость**: структурные логи, метрики, трейсинг.
- **Модульность**: каждый компонент — отдельный крейт с чётким API.

---

## Текущее состояние

**Готово:**

- [x] Workspace: `skadi-core`, `skadi-transport`, `skadi-protocol`,
      `skadi-server` (бинарник + библиотека для тестов).
- [x] TCP-транспорт: connect timeout, `TCP_NODELAY`, graceful shutdown.
- [x] SOCKS5: CONNECT (IPv4/IPv6/домен), no-auth, user-pass, reply-коды.
- [x] VLESS: TCP CONNECT, UUID-аутентификация, ответный заголовок.
- [x] Оба протокола подключены в `skadi-server`; sniffing `0x05`/`0x00`
      при одновременном включении.
- [x] TLS inbound: `rustls`, TLS 1.3, PEM, ALPN, **SNI-роутинг**.
- [x] **REALITY inbound**: sniff ClientHello, X25519 auth, dynamic cert,
      fallback на `dest` при невалидном клиенте.
- [x] Конфиг TOML с валидацией (listen, UUID, TLS/REALITY).
- [x] `skadicore genkey reality` — генерация ключей REALITY.
- [x] gRPC API: hot reload VLESS/SOCKS5 users (`[api]`, Bearer token).
- [x] Observability: Prometheus `/metrics`, `/healthz`, `--log-format`.
- [x] CI: `fmt`, `clippy`, `test`, `audit`.
- [x] Fuzz-таргеты SOCKS5/VLESS; 29+ автотестов (парсеры + TLS/REALITY e2e).
- [x] Документация: `docs/`, `.cursor/` для AI-агентов.

**В работе:**

- [x] Prometheus `/metrics` и `/healthz` (`[metrics]`).
- [ ] Idle timeout, лимит соединений.
- [x] E2E VLESS+REALITY с Xray-core (совместим с v2rayNG / Nekoray).

**Не начато:**

- [x] gRPC API — hot reload пользователей VLESS/SOCKS5 (`[api]`).
- [ ] TUN, UDP/Mux VLESS, XHTTP, TLS outbound.

⚠️ **Без `[transport.tls]` или `[transport.reality]` трафик идёт в открытом виде.**
Для продакшена включайте REALITY (рекомендуется) или TLS.

---

## Архитектура

```
skadicore/
├── Cargo.toml              # workspace root
├── crates/
│   ├── skadi-core/         # общие типы, трейты, ошибки
│   ├── skadi-transport/    # TCP, TLS inbound (+ SNI), позже XHTTP
│   ├── skadi-protocol/     # SOCKS5, VLESS, позже REALITY
│   └── skadi-server/       # бинарник: сборка всего вместе
└── config/
    └── skadi.toml
```

### Принципы разделения

- **`skadi-core`** не знает ничего про протоколы и транспорты. Только
  типы: `Endpoint`, `Session`, `UserId`, `Error`.
- **`skadi-transport`** умеет устанавливать соединения, но не знает,
  что по ним пойдёт.
- **`skadi-protocol`** парсит байты, но не занимается I/O напрямую.
  Все парсеры — чистые функции, пригодные для fuzz-тестирования.
- **`skadi-server`** связывает всё вместе: слушает порт, вызывает
  handshake нужного протокола, устанавливает upstream, релеит байты.

### Поток обработки соединения

```
accept(TCP)
  → [transport.tls] TlsTransport::accept()
  → sniff 0x05 (SOCKS5) / 0x00 (VLESS), если оба включены
  → VlessHandler::handshake() / Socks5Handler::negotiate() → Endpoint
  → TcpTransport::connect(Endpoint)
  → [SOCKS5] send_reply()
  → copy_bidirectional()
```

---

## Сборка и запуск

### Требования

- Rust stable (см. `rust-toolchain.toml`)
- Linux, macOS или Windows

### Сборка

```bash
cargo build --release
```

Бинарник: `target/release/skadicore`.

### Запуск

```bash
./target/release/skadicore --config config/skadi.toml
```

Опции CLI:

```
-c, --config <PATH>       Путь к конфигу [default: config/skadi.toml]
    --log-level <LEVEL>   trace | debug | info | warn | error [default: info]
-h, --help                Справка
-V, --version             Версия
```

Логи пишутся в JSON. Для читаемого вывода:

```bash
RUST_LOG=debug ./target/release/skadicore
```

(без флага `--json` формат переключается на человекочитаемый)

---

## Конфигурация

Формат — TOML. Подробности: `docs/CONFIGURATION.md`.

```toml
[server]
listen = "0.0.0.0:443"

# REALITY (рекомендуется; не включать вместе с transport.tls)
[transport.reality]
enabled = true
dest = "www.microsoft.com:443"
server_names = ["www.microsoft.com"]
private_key = "BASE64_X25519_PRIVATE_KEY"  # skadicore genkey reality
short_ids = ["0123456789abcdef"]

[protocol.vless]
enabled = true

[[protocol.vless.users]]
id = "b831381d-6324-4d53-ad4f-8cda48b30811"

[protocol.socks5]
enabled = false
```

Генерация ключей REALITY:

```bash
cargo run --bin skadicore -- genkey reality
```

SNI (несколько сертификатов на одном порту):

```toml
[[transport.tls.certificates]]
server_names = ["example.com"]
cert = "certs/example.pem"
key = "certs/example-key.pem"
```

### Валидация

- `server.listen` — валидный `SocketAddr`.
- Хотя бы один протокол (`socks5` или `vless`) включён.
- VLESS UUID — канонический формат; при TLS — файлы `cert`/`key` существуют и парсятся.

---

## Поддерживаемые протоколы

### SOCKS5 (RFC 1928)

- [x] No-auth
- [x] User/pass (RFC 1929)
- [x] CONNECT (IPv4, IPv6, домен)
- [ ] BIND
- [ ] UDP ASSOCIATE

Reply-коды соответствуют спецификации. Пароли сравниваются за
постоянное время, чтобы не утекало существование пользователя.

### VLESS (version 0)

- [x] Заголовок запроса (UUID, addons, command, address)
- [x] UUID-аутентификация за постоянное время
- [x] TCP CONNECT
- [x] Ответный заголовок (2 байта)
- [ ] UDP
- [ ] Mux
- [ ] Flow `xtls-rprx-vision`

VLESS рекомендуется поверх `[transport.tls]`. Без TLS — только для отладки.

---

## Разработка

### Тесты

```bash
# Все тесты (25: парсеры + TLS e2e)
cargo test --workspace

# Только парсеры
cargo test -p skadi-protocol

# Интеграционные TLS-тесты
cargo test -p skadi-server --test tls_vless_e2e
cargo test -p skadi-server --test tls_socks5_e2e
cargo test -p skadi-server --test tls_sni_e2e
```

### Fuzz-тесты

Требуется nightly Rust и `cargo-fuzz`:

```bash
cargo install cargo-fuzz
rustup toolchain install nightly

cd crates/skadi-protocol

cargo +nightly fuzz run parse_greeting
cargo +nightly fuzz run parse_auth
cargo +nightly fuzz run parse_request
cargo +nightly fuzz run parse_vless_request
```

Fuzzer сохраняет падения в `fuzz/artifacts/`. Воспроизвести:

```bash
cargo +nightly fuzz run parse_request fuzz/artifacts/parse_request/crash-<hash>
```

### Линтеры

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo audit
```

### Стиль кода

- `rustfmt.toml` фиксирует форматирование.
- Никаких `unwrap()` и `expect()` на горячем пути.
- `unsafe` — только с комментарием `// SAFETY:` и обоснованием.
- Логи не должны содержать пароли, UUID и ключи.

---

## Безопасность

### Что уже сделано

- Пароли и UUID сравниваются за постоянное время (`subtle`).
- Лимиты на все поля парсеров (количество методов, длина домена,
  размер addons).
- Таймаут на всю фазу переговоров — защита от Slowloris.
- Неверный UUID в VLESS не вызывает ответа — соединение молча
  закрывается, чтобы активный зонд не мог отличить сервер от
  закрытого порта.
- Graceful shutdown не обрывает активные сессии.

### Что ещё не сделано

- TLS outbound, REALITY.
- Нет rate limiting и idle timeout на сессии.
- Нет изоляции пользователей друг от друга.
- gRPC API (когда появится) должен слушать только `127.0.0.1` и
  требовать токен.

### Сообщить об уязвимости

Не открывайте публичный issue. Напишите на `security@example.com`
(заменить на реальный адрес). Мы ответим в течение 72 часов.

---

## Дорожная карта

| Этап | Статус | Описание |
|---|---|---|
| 0. Фундамент | ✅ | Workspace, CI, типы, `.cursor/` |
| 1. TCP-прокси | 🟡 | Релей есть; idle/backpressure — TODO |
| 2. SOCKS5 | ✅ | CONNECT + auth + fuzz + TLS e2e |
| 3. TLS inbound | ✅ | PEM, SNI, ALPN, TLS 1.3 |
| 4. VLESS | 🟡 | TCP + server + TLS e2e; UDP/flow — TODO |
| 5. REALITY | 🟢 | Inbound + fallback + Xray e2e |
| 6. gRPC API | 🟢 | Hot reload пользователей |
| 7. Метрики | 🟢 | Prometheus, `/healthz`, `--log-format` |
| 8. TUN | ⏳ | Клиентский режим |

Легенда: ✅ готово, 🚧 в работе, 🟡 частично, ⏳ не начато.

---

## Лицензия

AGPL-3.0-or-later. См. `LICENSE`.

Это означает: вы можете свободно использовать, модифицировать и
распространять SkadiCore, но если вы запускаете модифицированную
версию как сетевой сервис, вы обязаны раскрыть исходный код своих
изменений.

---

## Благодарности

Проект вдохновлён:

- [Xray-core](https://github.com/XTLS/Xray-core) — архитектура,
  REALITY, gRPC API.
- [sing-box](https://github.com/SagerNet/sing-box) — TUN, минимализм.
- [rustls-reality](https://github.com/rustls-reality) — REALITY для
  Rust.
- [Tokio](https://tokio.rs) — асинхронный runtime.
