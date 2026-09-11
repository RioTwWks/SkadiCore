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

- [x] Workspace с крейтами `skadi-core`, `skadi-transport`,
      `skadi-protocol`, `skadi-server`.
- [x] TCP-транспорт с таймаутами и `TCP_NODELAY`.
- [x] SOCKS5: CONNECT, no-auth, user-pass, reply-коды,
      константное сравнение паролей.
- [x] VLESS: парсинг заголовка, UUID-аутентификация, TCP CONNECT.
- [x] Чистые парсеры без I/O, пригодные для fuzz-тестов.
- [x] Fuzz-таргеты для SOCKS5 и VLESS.
- [x] Graceful shutdown по `Ctrl+C` и `SIGTERM`.

**В работе:**

- [x] TLS inbound (rustls) — PEM из конфига, TLS 1.3.
- [x] TLS SNI-роутинг (несколько сертификатов на порту).
- [ ] TLS outbound.
- [x] Интеграционный тест SOCKS5 over TLS.
- [ ] Prometheus-метрики.

**Не начато:**

- [ ] REALITY.
- [ ] gRPC API.
- [ ] TUN-режим.
- [ ] UDP over VLESS.
- [ ] XHTTP.

⚠️ **VLESS сейчас работает без TLS**. Это означает, что трафик идёт
в открытом виде. Для отладки это удобно, но использовать в реальных
условиях нельзя. Следующий шаг — интеграция `rustls`.

---

## Архитектура

```
skadicore/
├── Cargo.toml              # workspace root
├── crates/
│   ├── skadi-core/         # общие типы, трейты, ошибки
│   ├── skadi-transport/    # TCP, позже TLS и XHTTP
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
accept() → handle_client()
             │
             ├─► VlessHandler::handshake()  ─► Endpoint
             │     или
             └─► Socks5Handler::negotiate() ─► Endpoint
             │
             ├─► TcpTransport::connect(Endpoint)
             │
             ├─► (для SOCKS5) Socks5Handler::send_reply()
             │
             └─► copy_bidirectional() — релей в обе стороны
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

Формат — TOML. Полный пример:

```toml
[server]
listen = "0.0.0.0:1080"

[protocol.socks5]
enabled = true
auth = "no-auth"          # или "user-pass"

# Раскомментировать для user-pass:
# [[protocol.socks5.users]]
# username = "alice"
# password = "change-me"

[protocol.vless]
enabled = false

# [[protocol.vless.users]]
# id = "b831381d-6324-4d53-ad4f-8cda48b30811"
# email = "alice@example.com"
```

### Валидация

- `server.listen` должен быть валидным `SocketAddr`.
- Хотя бы один протокол должен быть включён.
- UUID в VLESS-секции должен быть каноническим (32 hex-цифры с
  дефисами или без).

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

⚠️ VLESS работает поверх plain TCP. TLS/REALITY — в дорожной карте.

---

## Разработка

### Тесты

```bash
# Юнит-тесты
cargo test --workspace

# Только парсеры (быстро)
cargo test -p skadi-protocol
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

- Нет TLS: весь трафик в открытом виде.
- Нет rate limiting на подключения.
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
| 0. Фундамент | ✅ | Workspace, CI, типы |
| 1. TCP-прокси | ✅ | Базовый релей |
| 2. SOCKS5 | ✅ | CONNECT + auth |
| 3. TLS | 🚧 | `rustls` + `tokio-rustls` |
| 4. VLESS | 🟡 | TCP готов, UDP и flow — TODO |
| 5. REALITY | ⏳ | Через `rustls-reality` |
| 6. gRPC API | ⏳ | Динамическое управление |
| 7. Метрики | ⏳ | Prometheus |
| 8. TUN | ⏳ | Для клиентского режима |

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
