# SkadiCore — текущие приоритеты

Синхронизировано с `TODO.md` (2026-09-19).

## Выполнено (не трогать без причины)

- [x] Workspace, крейты, типы, ошибки
- [x] TCP-прокси, graceful shutdown, connect timeout, idle/lifetime, max_connections
- [x] SOCKS5 CONNECT + user-pass + fuzz (+ BIND / UDP ASSOCIATE опционально)
- [x] VLESS TCP/UDP/Mux/XUDP + flow reject
- [x] TLS inbound/outbound, REALITY inbound + **native REALITY client**, gRPC API, metrics
- [x] Примеры конфигов: REALITY, VLESS TLS, SOCKS5 TLS, `client-reality-vless`
- [x] CI: fmt, clippy, test, audit, deny, miri, musl, cross-platform, tarpaulin, smoke-transports
- [x] GitHub Releases workflow + **первый тег `v0.1.0`**
- [x] `CONTRIBUTING.md`, CHANGELOG Keep a Changelog, `docs/RELEASING.md`

## Сейчас (ближайшие задачи)

1. **Транспорт:** клиентский XHTTP (сервер уже есть).
2. **Релизный цикл:** накопить `[Unreleased]` → **`v0.1.1`** по `docs/RELEASING.md` (kex_mode + TUN REALITY example в очереди на релиз).
3. **Ops:** `MINISIGN_SECRET_KEY` в GitHub для подписанных артефактов.

## Позже

| Задача | Описание |
|--------|----------|
| ~~XHTTP (server)~~ | ✅ stream-one + stream-up + packet-up |
| ~~AmneziaWG~~ | ✅ MVP |
| ~~Client MVP~~ | ✅ SOCKS5 → VLESS+TLS/REALITY |
| ~~TUN~~ | ✅ Linux MVP |
| Трейты `InboundHandler` / `Transport` | рефакторинг `handle_client` |
| Sync `third_party/rustls-reality` | Action / Renovate |
| CI examples + Xray в Docker | совместимость до релиза |

## Что НЕ делать сейчас

- OpenTelemetry до стабилизации метрик Prometheus
- NFQUEUE / sonicdpi в ядре (документировать клиентские утилиты)

## При завершении задачи

1. Обновить `TODO.md` и этот файл
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
