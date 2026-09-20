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
- [x] GitHub Releases workflow + теги **`v0.1.0`**, **`v0.1.1`**, **`v0.1.2`**
- [x] `CONTRIBUTING.md`, CHANGELOG Keep a Changelog, `docs/RELEASING.md`
- [x] XHTTP server + client
- [x] AmneziaWG / Hysteria2 / TUIC server MVP (binary backends)
- [x] Hysteria2 / TUIC **client** sidecar modes (`skadicore client`)
- [x] Hy2 / TUIC traffic e2e в CI
- [x] AWG traffic e2e (netns + real bins)
- [x] Трейты `InboundHandler` / `OutboundTransport` + рефакторинг `handle_connection`
- [x] Sync monitor `third_party/rustls-reality` (Action + Renovate + Dependabot)
- [x] CI examples × Xray (Docker) — `examples-xray-e2e`
- [x] IPv6 клиентские конфиги (`[host]:port`, examples) + `docs/THROTTLING.md`

## Сейчас (ближайшие задачи)

1. (опционально) rebase vendored rustls 0.22.4 → 0.23.x — по issue от sync Action / `UPSTREAM.md`.
2. Расширенная crypto-agility (доп. KEM) — по необходимости.

## Позже

| Задача | Описание |
|--------|----------|
| ~~XHTTP (server/client)~~ | ✅ |
| ~~AWG / Hy2 / TUIC server~~ | ✅ MVP binary |
| ~~Hy2 / TUIC client~~ | ✅ sidecar SOCKS5 |
| ~~Hy2 / TUIC traffic e2e~~ | ✅ CI `sidecar-traffic-e2e` |
| ~~AWG traffic e2e~~ | ✅ netns + real bins |
| ~~Трейты `InboundHandler` / `Transport`~~ | ✅ |
| ~~Sync `third_party/rustls-reality`~~ | ✅ |
| ~~CI examples + Xray в Docker~~ | ✅ |
| ~~IPv6 клиентские конфиги~~ | ✅ |
| ~~Дока по троттлингу (zapret/ByeDPI)~~ | ✅ `docs/THROTTLING.md` |
| Rebase rustls-reality → 0.23.x | по `UPSTREAM.md` |
| Crypto-agility (доп. KEM) | |

## Что НЕ делать сейчас

- OpenTelemetry до стабилизации метрик Prometheus
- NFQUEUE / sonicdpi в ядре
- Нативный QUIC rewrite Hy2/TUIC (пока binary wrappers достаточны)

## При завершении задачи

1. Обновить `TODO.md` и этот файл
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
