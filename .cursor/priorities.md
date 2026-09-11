# SkadiCore — текущие приоритеты

Синхронизировано с `TODO.md` (2026-09-11).

## Выполнено (не трогать без причины)

- [x] Workspace, крейты, типы, ошибки
- [x] TCP-прокси, graceful shutdown, connect timeout
- [x] SOCKS5 CONNECT + user-pass + fuzz
- [x] VLESS TCP: парсер, handler, wired в `skadi-server`
- [x] Protocol sniffing (`0x05` SOCKS5 / `0x00` VLESS)
- [x] Конфиг TOML + валидация (listen, UUID, TLS PEM)
- [x] CI: fmt, clippy, test, audit
- [x] TLS inbound: PEM, TLS 1.3, ALPN, SNI routing
- [x] Интеграционные тесты: SOCKS5/VLESS/SNI over TLS (5 тестов)
- [x] `.cursor/` для AI-агентов

## Сейчас (ближайшие задачи)

### 1. Наблюдаемость (этап 7)

- [ ] Prometheus-метрики (`metrics` + exporter)
- [ ] `/healthz`
- [ ] `--log-format=json|pretty`

### 2. Устойчивость (этап 1, доработка)

- [ ] Idle timeout на сессию
- [ ] Лимит одновременных соединений
- [ ] Нагрузочные тесты

### 3. Конфиг/CLI (этап 8)

- [ ] `--check-config`
- [ ] Документация ручных проверок (`openssl s_client`, `curl --socks5`)

## Позже (зависимости)

| Этап | Зависит от | Описание |
|------|------------|----------|
| 5 REALITY | 3 ✅ | `rustls-reality` |
| 6 gRPC API | 4 🟡 | Управление пользователями |
| 4 VLESS UDP/Mux/flow | 3 ✅ | Расширение протокола |
| 10 Релиз | все | musl, releases |

## Что НЕ делать сейчас

- REALITY до метрик и стабилизации VLESS+TLS в проде
- gRPC API до REALITY (или явного решения обойтись без него)
- TUN — клиентский режим, после серверного MVP
- Отдельный крейт `skadi-config` — низкий приоритет

## При завершении задачи

1. Обновить `TODO.md` и этот файл
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
