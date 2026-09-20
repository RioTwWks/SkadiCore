# SkadiCore — текущие приоритеты

Синхронизировано с `TODO.md` (2026-09-20).

## Выполнено (не трогать без причины)

- [x] Workspace, крейты, типы, ошибки
- [x] TCP-прокси, graceful shutdown, connect timeout, idle/lifetime, max_connections
- [x] SOCKS5 CONNECT + user-pass + fuzz (+ BIND / UDP ASSOCIATE опционально)
- [x] VLESS TCP/UDP/Mux/XUDP + flow reject
- [x] TLS inbound/outbound, REALITY inbound + **native REALITY client**, gRPC API, metrics
- [x] Примеры конфигов: REALITY, VLESS TLS, SOCKS5 TLS, `client-reality-vless`
- [x] CI: fmt, clippy, test, audit, deny, miri, musl, cross-platform, tarpaulin, smoke-transports
- [x] GitHub Releases workflow + теги **`v0.1.0`**, **`v0.1.1`**
- [x] `CONTRIBUTING.md`, CHANGELOG Keep a Changelog, `docs/RELEASING.md`
- [x] XHTTP server + client
- [x] AmneziaWG / Hysteria2 / TUIC server MVP (binary backends)
- [x] Hysteria2 / TUIC **client** sidecar modes (`skadicore client`)

## Сейчас (ближайшие задачи)

1. Накопить `[Unreleased]` → **`v0.1.2`** (Hy2/TUIC clients + docs).
2. Опционально: traffic e2e для Hy2/TUIC/AWG в CI (реальные бинарники).

## Позже

| Задача | Описание |
|--------|----------|
| ~~XHTTP (server/client)~~ | ✅ |
| ~~AWG / Hy2 / TUIC server~~ | ✅ MVP binary |
| ~~Hy2 / TUIC client~~ | ✅ sidecar SOCKS5 |
| Трейты `InboundHandler` / `Transport` | рефакторинг `handle_client` |
| Sync `third_party/rustls-reality` | Action / Renovate |
| CI examples + Xray в Docker | совместимость |
| IPv6 клиентские конфиги | |
| Дока по троттлингу (zapret/ByeDPI) | не NFQUEUE в ядре |

## Что НЕ делать сейчас

- OpenTelemetry до стабилизации метрик Prometheus
- NFQUEUE / sonicdpi в ядре
- Нативный QUIC rewrite Hy2/TUIC (пока binary wrappers достаточны)

## При завершении задачи

1. Обновить `TODO.md` и этот файл
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
