# SkadiCore — контекст для AI-агентов

Краткий снимок проекта. **Обновлено: 2026-09-11.**

## Назначение

Серверное ядро прокси для обхода блокировок (альтернатива Xray / sing-box).

## Workspace

```
skadicore/
├── .cursor/                # правила и приоритеты для AI
├── .github/workflows/ci.yml
├── config/skadi.toml
├── crates/
│   ├── skadi-core/         # Endpoint, Session, UserId, Error
│   ├── skadi-transport/    # TcpTransport, TlsTransport (+ SNI)
│   ├── skadi-protocol/     # SOCKS5, VLESS (+ fuzz/)
│   └── skadi-server/       # lib + bin skadicore, tests/
└── docs/
```

Конфиг и валидация — в `skadi-server/src/config.rs` (отдельного `skadi-config` нет).

## Поток соединения

```
accept(TCP)
  → [опционально] TlsTransport::accept()   # TLS 1.3, SNI
  → sniff 0x05/0x00 (если оба протокола)
  → SOCKS5 negotiate / VLESS handshake
  → TcpTransport::connect(upstream)
  → [SOCKS5] send_reply
  → copy_bidirectional
```

## Текущий статус

| Компонент | Статус |
|-----------|--------|
| TCP + connect timeout | ✅ |
| Graceful shutdown | ✅ |
| SOCKS5 CONNECT + auth | ✅ |
| VLESS TCP + UUID auth | ✅ |
| VLESS/SOCKS5 в server | ✅ |
| TLS inbound + SNI | ✅ |
| Fuzz SOCKS5/VLESS | ✅ |
| CI (fmt/clippy/test/audit) | ✅ |
| Интеграционные TLS-тесты | ✅ (5) |
| Prometheus / healthz | ⏳ |
| REALITY | ⏳ |
| gRPC API | ⏳ |
| TUN | ⏳ |
| VLESS UDP/Mux/flow | ⏳ |

**Тесты:** `cargo test --workspace` → 25 тестов (20 parser + 5 integration).

## Ключевые файлы

| Файл | Роль |
|------|------|
| `crates/skadi-server/src/lib.rs` | `run()`, `run_server()`, accept loop |
| `crates/skadi-server/src/config.rs` | TOML + валидация |
| `crates/skadi-transport/src/tls.rs` | TLS accept, SNI resolver |
| `crates/skadi-protocol/src/vless/parse.rs` | parse + build_tcp_request |
| `crates/skadi-server/tests/tls_*_e2e.rs` | интеграционные тесты |

## Конфиг (минимум)

```toml
[server]
listen = "0.0.0.0:443"

[transport.tls]
enabled = true
cert = "certs/default.pem"
key = "certs/default.key"

[protocol.vless]
enabled = true

[[protocol.vless.users]]
id = "uuid-here"
```

См. `docs/CONFIGURATION.md` для SNI и SOCKS5.

## Команды

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run --bin skadicore -- --config config/skadi.toml
```
