# SkadiCore — контекст для AI-агентов

Краткий снимок проекта. Обновляйте при существенных изменениях архитектуры.

## Назначение

Серверное ядро прокси для обхода блокировок. Замена/альтернатива Xray и sing-box:

- От Xray: gRPC API, REALITY, VLESS + XHTTP
- От sing-box: низкое потребление, TUN, множество протоколов в одном бинарнике

## Workspace

```
skadicore/
├── Cargo.toml              # workspace root, release profile
├── config/skadi.toml       # пример конфига
├── crates/
│   ├── skadi-core/         # Endpoint, Session, UserId, Error
│   ├── skadi-transport/    # TcpTransport (+ TLS в разработке)
│   ├── skadi-protocol/     # SOCKS5, VLESS (+ fuzz workspace)
│   └── skadi-server/       # бинарник skadicore
└── docs/                   # ARCHITECTURE, DEVELOPMENT, PROTOCOLS, …
```

## Зависимости между крейтами

```
skadi-server
    ├── skadi-core
    ├── skadi-transport ──► skadi-core
    └── skadi-protocol ──► skadi-core
```

## Поток соединения

```
accept → spawn → handle_client
    → handshake (SOCKS5 negotiate / VLESS handshake) → Endpoint
    → TcpTransport::connect(endpoint)
    → (SOCKS5 only) send_reply
    → copy_bidirectional
```

## Текущий статус (MVP)

| Компонент | Статус |
|-----------|--------|
| Workspace + крейты | ✅ |
| TCP transport + таймауты | ✅ |
| SOCKS5 CONNECT + auth | ✅ |
| VLESS парсер + handler | ✅ (код есть) |
| VLESS в skadi-server | 🚧 подключается |
| Fuzz-таргеты SOCKS5/VLESS | ✅ |
| Graceful shutdown | ✅ |
| TLS (rustls) | ⏳ этап 3 |
| CI (GitHub Actions) | 🚧 настраивается |
| REALITY | ⏳ этап 5 |
| gRPC API | ⏳ этап 6 |
| Prometheus метрики | ⏳ этап 7 |
| TUN | ⏳ |

## Ключевые файлы

| Файл | Роль |
|------|------|
| `crates/skadi-server/src/main.rs` | accept loop, handle_client, shutdown |
| `crates/skadi-server/src/config.rs` | загрузка TOML |
| `crates/skadi-protocol/src/socks5.rs` | SOCKS5 handler + config |
| `crates/skadi-protocol/src/socks5/parse.rs` | чистые парсеры SOCKS5 |
| `crates/skadi-protocol/src/vless/handler.rs` | VLESS handshake |
| `crates/skadi-protocol/src/vless/parse.rs` | чистые парсеры VLESS |
| `crates/skadi-transport/src/tcp.rs` | исходящий TCP с таймаутом |

## Конфиг (TOML)

```toml
[server]
listen = "0.0.0.0:1080"

[protocol.socks5]
enabled = true
auth = "no-auth"   # или "user-pass"

[protocol.vless]
enabled = false
# [[protocol.vless.users]]
# id = "uuid"
```

Хотя бы один протокол должен быть `enabled = true`.

## Дискриминация протоколов на одном порту

- SOCKS5: первый байт `0x05`
- VLESS v0: первый байт `0x00`

При включённых обоих протоколах — sniff первого байта.

## Команды разработки

```bash
cargo build --release
cargo test --workspace
cargo run --bin skadicore -- --config config/skadi.toml
RUST_LOG=debug cargo run --bin skadicore -- --log-level debug
```

Бинарник: `target/release/skadicore`

## Внешние референсы

- Xray-core — gRPC API, REALITY
- sing-box — TUN, минимализм
- rustls-reality — REALITY для Rust
- fast-socks5 — зрелая SOCKS5 на Rust
