# SkadiCore — правила для AI-агентов

> Ядро прокси на Rust. Гибрид идей Xray (gRPC API, REALITY) и sing-box (TUN, минимализм).
> Лицензия: AGPL-3.0-or-later. Статус: MVP в разработке.

## Обязательно читать перед работой

1. `README.md` — текущее состояние и быстрый старт
2. `TODO.md` — дорожная карта и критерии готовности этапов
3. `docs/ARCHITECTURE.md` — слои, поток данных, принятые решения
4. `.cursor/context.md` — снимок структуры крейтов и статуса
5. `.cursor/priorities.md` — что делать сейчас

## Архитектурные ограничения

| Правило | Детали |
|---------|--------|
| **Слои** | `skadi-core` → `skadi-transport` / `skadi-protocol` → `skadi-server` |
| **skadi-core** | Только типы (`Endpoint`, `Session`, `UserId`, `Error`). Без `tokio::net`, без протоколов |
| **skadi-transport** | TCP/TLS/XHTTP. Не знает про SOCKS5/VLESS |
| **skadi-protocol** | Парсеры — чистые функции в `parse.rs`. I/O — в `handler.rs` |
| **skadi-server** | Сборка: accept loop, выбор протокола, релей |

Нарушение границ крейтов — сигнал, что архитектура поплыла.

## Безопасность

- **Никакой своей криптографии.** Только `rustls`, `ring`, `aes-gcm`, `chacha20poly1305`
- **REALITY** — через `rustls-reality`, не с нуля
- **Секреты** — `subtle::ConstantTimeEq` для паролей и UUID
- **VLESS auth fail** — молчаливое закрытие, без ответа клиенту
- **Логи** — не писать пароли, UUID, ключи, содержимое трафика
- **TLS** — никогда не отключать проверку сертификата

## Стиль кода (Rust)

- Type hints обязательны; `edition = "2021"`
- Библиотеки: `thiserror` для ошибок. Бинарник: `anyhow` для контекста
- Логирование: `tracing`, не `println!` / `eprintln!`
- `unwrap()` / `expect()` — запрещены на горячем пути и в парсерах
- `unsafe` — только с `// SAFETY:` и обоснованием
- `rustfmt.toml`: `max_width = 100`
- Перед коммитом: `cargo fmt`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`

## Парсеры (критично)

```rust
pub fn parse_request(input: &[u8]) -> Result<(T, usize), ParseError>
```

- Чистые функции, без I/O
- Лимиты на все поля переменной длины (`MAX_DOMAIN`, `MAX_ADDONS`, …)
- Проверка лимита **до** аллокации
- Юнит-тесты + fuzz-таргет для каждого парсера
- Никаких паник: `checked_add`, проверка длины перед индексацией

## Протоколы: асимметрия

- **SOCKS5**: `send_reply` **после** `connect()` — reply содержит bound-адрес
- **VLESS**: ответный заголовок (2 байта) **внутри** `handshake()`, до connect

Не унифицировать преждевременно.

## Добавление нового протокола

1. `crates/skadi-protocol/src/<proto>/` — `parse.rs`, `handler.rs`, `config.rs`
2. `impl InboundHandler` + ветка в `handshake_inbound`
3. Fuzz-таргет в `fuzz/fuzz_targets/`
4. Секция в конфиге + `Protocol` / sniff при необходимости
5. Обновить `docs/PROTOCOLS.md`, `docs/CONFIGURATION.md`, `CHANGELOG.md`

## Добавление транспорта

1. `crates/skadi-transport/src/<name>.rs`
2. `impl OutboundTransport` (`connect` → `Self::Stream: AsyncRead + AsyncWrite + Unpin`)
3. Handlers обобщить: `S: AsyncRead + AsyncWrite + Unpin` вместо `TcpStream`

## Антипаттерны

- ❌ Redis, Celery, Kafka, внешние очереди
- ❌ `native-tls` (только `rustls`)
- ❌ Блокирующий I/O в async без `spawn_blocking`
- ❌ JSON из `/ui/*` или HTML из `/api/*` (когда появится API — только JSON)
- ❌ Хардкод путей, ключей, паролей
- ❌ Docker в v1 (по TODO этап 10 — позже)

## Коммиты и PR

- Ветки: `cursor/<описание>-ca71`
- Сообщения коммитов — полные предложения, описывают «что» и «зачем»
- Не коммитить `target/`, `.env`, секреты
- `Cargo.lock` — коммитить для приложения (бинарник)

## Тестирование

```bash
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cd crates/skadi-protocol && cargo +nightly fuzz run parse_greeting -- -max_total_time=60
```

## Ссылки

- `docs/DEVELOPMENT.md` — окружение, fuzz, отладка
- `docs/PROTOCOLS.md` — детали SOCKS5/VLESS
- `docs/CONFIGURATION.md` — TOML-схема
- `docs/RISKS.md` — риски по этапам
- `SECURITY.md` — политика безопасности
