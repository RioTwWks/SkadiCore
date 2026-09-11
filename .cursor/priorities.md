# SkadiCore — текущие приоритеты

Синхронизировано с `TODO.md`. Агенты должны работать сверху вниз, не перескакивая зависимости.

## Сейчас (ближайшие задачи)

### 1. Стабилизация MVP (этапы 0–2)

- [x] Workspace, крейты, типы
- [x] TCP-прокси, graceful shutdown
- [x] SOCKS5 CONNECT + user-pass
- [ ] **Подключить VLESS в `skadi-server`** (handler есть, не wired)
- [ ] **Валидация конфига** (listen addr, ≥1 протокол, UUID format)
- [ ] **CI**: fmt, clippy, test, audit
- [ ] Интеграционные тесты SOCKS5 e2e

### 2. TLS-транспорт (этап 3) — следующий крупный шаг

Без TLS VLESS небезопасен. Порядок:

1. `rustls` + `tokio-rustls` в `skadi-transport`
2. Загрузка PEM из конфига
3. TLS accept на inbound
4. Обобщить handlers: `AsyncRead + AsyncWrite` вместо `TcpStream`
5. Тесты с `openssl s_client`

### 3. Наблюдаемость (этап 7, параллельно)

- Prometheus-метрики (`metrics` + exporter)
- `/healthz`
- Флаг `--log-format=json|pretty`

## Позже (не начинать без зависимостей)

| Этап | Зависит от | Описание |
|------|------------|----------|
| 4 VLESS UDP/Mux/flow | 2, 3 | Расширение протокола |
| 5 REALITY | 3, 4 | `rustls-reality` |
| 6 gRPC API | 4 | Управление пользователями |
| 8 Конфиг/CLI | 0 | SIGHUP reload, genkey |
| 9 Тесты/безопасность | все | miri, criterion, soak |
| 10 Релиз | все | musl cross-compile, releases |

## Definition of Done для MVP

- [ ] Один статический бинарник
- [ ] TCP-прокси + SOCKS5 работает
- [ ] VLESS TCP работает
- [ ] Конфиг через TOML с валидацией
- [ ] Структурные логи
- [ ] Тесты покрывают парсеры
- [ ] CI зелёный
- [ ] README с быстрым стартом ✅

## Что НЕ делать сейчас

- REALITY до TLS
- gRPC API до стабильного VLESS
- TUN (клиентский режим) — после серверного MVP
- Своя криптография

## При завершении задачи

1. Обновить чекбоксы в `TODO.md` (если этап завершён)
2. Запись в `CHANGELOG.md` под `[Unreleased]`
3. Обновить `.cursor/context.md` при архитектурных изменениях
