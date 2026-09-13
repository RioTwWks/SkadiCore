# SkadiCore — текущие приоритеты

Синхронизировано с `TODO.md` (2026-09-13).

## Выполнено (не трогать без причины)

- [x] Workspace, крейты, типы, ошибки
- [x] TCP-прокси, graceful shutdown, connect timeout, idle/lifetime, max_connections
- [x] SOCKS5 CONNECT + user-pass + fuzz
- [x] VLESS TCP/UDP/Mux/XUDP + flow reject
- [x] TLS inbound/outbound, REALITY, gRPC API, metrics
- [x] Примеры конфигов: REALITY, VLESS TLS, SOCKS5 TLS
- [x] CI: fmt, clippy, test, audit, deny, miri, musl, tarpaulin

## Сейчас (ближайшие задачи)

### 1. Релиз и распространение (этап 10)

- [x] Кросс-компиляция aarch64 musl (CI: cargo-zigbuild)
- [x] GitHub Releases (workflow на тег `v*`, musl x86_64 + aarch64)
- [ ] Кросс-компиляция Windows / macOS

### 2. Качество (этап 9)

- [ ] Soak-тесты на утечки памяти

## Позже

| Задача | Описание |
|--------|----------|
| XHTTP | Транспорт поверх HTTP |
| TUN | Клиентский режим |
| `skadi-config` | Выделить конфиг в отдельный крейт |
| Protocol enum | Вместо sniffing первого байта |

## Что НЕ делать сейчас

- Docker-образ (антипаттерн для проекта на текущем этапе)
- OpenTelemetry до стабилизации метрик Prometheus
- BIND / UDP ASSOCIATE для SOCKS5 — низкий приоритет

## При завершении задачи

1. Обновить `TODO.md` и этот файл
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
