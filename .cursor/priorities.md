# SkadiCore — текущие приоритеты

Синхронизировано с `TODO.md` (2026-09-13).

## Выполнено (не трогать без причины)

- [x] Workspace, крейты, типы, ошибки
- [x] TCP-прокси, graceful shutdown, connect timeout, idle/lifetime, max_connections
- [x] SOCKS5 CONNECT + user-pass + fuzz
- [x] VLESS TCP/UDP/Mux/XUDP + flow reject
- [x] TLS inbound/outbound, REALITY, gRPC API, metrics
- [x] Примеры конфигов: REALITY, VLESS TLS, SOCKS5 TLS
- [x] CI: fmt, clippy, test, audit, deny, miri, musl, cross-platform, tarpaulin
- [x] GitHub Releases (Linux musl + Windows + macOS)

## Сейчас (ближайшие задачи)

Обратная связь аудита (2026-09-15): приоритеты 1–5 (auth, SSRF, toolchain/Docker, proptest, soak) — ✅.
Далее: подпись релизов, IPv6 в `listen`.

## Позже

| Задача | Описание |
|--------|----------|
| ~~XHTTP~~ | ✅ stream-one + stream-up + packet-up (`[transport.xhttp]`) |
| ~~Client MVP~~ | ✅ `skadicore client` (SOCKS5 → VLESS+TLS) |
| ~~TUN~~ | ✅ Linux MVP (`[client.tun]`, routing/DNS, TCP/UDP → VLESS) |
| `skadi-config` | Выделить конфиг в отдельный крейт |
| ~~Protocol enum~~ | ✅ `skadi_core::Protocol` |

## Что НЕ делать сейчас

- ~~Docker-образ~~ — ✅ `Dockerfile` (musl scratch)
- OpenTelemetry до стабилизации метрик Prometheus
- BIND / UDP ASSOCIATE для SOCKS5 — низкий приоритет

## При завершении задачи

1. Обновить `TODO.md` и этот файл
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
