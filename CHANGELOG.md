# Changelog

Все значимые изменения в SkadiCore фиксируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект следует [Semantic Versioning](https://semver.org/lang/ru/).

## [Unreleased]

Текущая ветка разработки. Всё, что ниже, ещё не выпущено в релиз.

### Added

- **TLS inbound в `skadi-server`**
  - Секция `[transport.tls]` в конфиге: `enabled`, `cert`, `key`, `alpn`.
  - TLS handshake после `accept()`, до протокольного handshake.
  - TLS 1.3 only, загрузка PEM, автоматическая установка ring crypto provider.
  - Интеграционный тест `tls_socks5_e2e`: SOCKS5 CONNECT поверх TLS.

- **`skadi-server` как библиотека**
  - `skadi_server::run()` и `run_server()` для тестов.

- **`.cursor/` — вспомогательные файлы для AI-агентов**
  - `rules.md` — архитектурные ограничения, стиль, антипаттерны.
  - `context.md` — снимок структуры крейтов и текущего статуса.
  - `priorities.md` — синхронизация с `TODO.md`.
  - `workflow.md` — процесс разработки и проверки.

- **CI (GitHub Actions)**
  - `cargo fmt --check`, `clippy`, `test`, `cargo audit`.

- **`skadi-server`**
  - Подключение VLESS наряду с SOCKS5.
  - Sniffing первого байта при включённых обоих протоколах.
  - Валидация конфига (listen addr, UUID, пользователи).

- **`skadi-transport`**
  - Заготовка `TlsTransport` на `rustls` + `tokio-rustls` (загрузка PEM).

### Fixed

- Дублирование `MAX_METHODS` в `socks5.rs` (ошибка компиляции).
- Экспорт модуля `vless` из `skadi-protocol`.

### Changed

- Handlers SOCKS5 и VLESS обобщены: `AsyncRead + AsyncWrite` вместо `TcpStream`.

- **Workspace и структура проекта**
  - Cargo workspace с крейтами `skadi-core`, `skadi-transport`,
    `skadi-protocol`, `skadi-server`.
  - Фиксация версии Rust через `rust-toolchain.toml`.
  - Единый стиль форматирования через `rustfmt.toml`.

- **`skadi-core`**
  - Тип `Endpoint` для представления целевого адреса (IP или домен).
  - Тип `UserId` для идентификации пользователя.
  - Тип `Session` и `SessionId` для отслеживания активных соединений.
  - Иерархия ошибок через `thiserror`.

- **`skadi-transport`**
  - `TcpTransport` с настраиваемым таймаутом подключения.
  - Отключение алгоритма Нейгла (`TCP_NODELAY`) для снижения latency.

- **`skadi-protocol`**
  - **SOCKS5** (RFC 1928):
    - Greeting и выбор метода аутентификации.
    - Метод no-auth (`0x00`).
    - Метод user/pass (RFC 1929).
    - Команда CONNECT для IPv4, IPv6 и доменов.
    - Reply-коды, соответствующие спецификации.
    - Константное сравнение паролей через `subtle::ConstantTimeEq`.
    - Защита от timing-атак на существование пользователя.
    - Лимит на количество методов аутентификации (16).
    - Таймаут на всю фазу переговоров (10 секунд).

  - **VLESS** (version 0):
    - Парсинг заголовка запроса: version, UUID, addons, command,
      port, ATYP, address.
    - UUID-аутентификация за постоянное время.
    - Команда TCP.
    - Ответный заголовок (2 байта).
    - Молчаливое закрытие при неверном UUID — как требует
      спецификация для защиты от активного зондирования.
    - Лимит на размер addons (512 байт).
    - Таймаут на handshake (10 секунд).

  - **Чистые парсеры** для SOCKS5 и VLESS без I/O:
    - `parse_greeting`, `parse_auth`, `parse_request` (SOCKS5).
    - `parse_request` (VLESS).
    - Пригодны для fuzz-тестирования и юнит-тестов без сети.

- **`skadi-server`**
  - Бинарник `skadicore` с CLI на `clap`.
  - Конфигурация через TOML с валидацией.
  - Structured logging через `tracing` в JSON-формате.
  - Graceful shutdown по `Ctrl+C` и `SIGTERM` через `watch`-канал.
  - Таймаут 5 секунд на завершение accept-loop.

- **Тестирование**
  - 13 юнит-тестов для парсеров SOCKS5.
  - 7 юнит-тестов для парсеров VLESS.
  - 3 fuzz-таргета для SOCKS5 (`parse_greeting`, `parse_auth`,
    `parse_request`).
  - 1 fuzz-таргет для VLESS (`parse_vless_request`).

- **Документация**
  - `README.md` с описанием проекта, сборки и конфигурации.
  - `SECURITY.md` с моделью угроз и политикой раскрытия
    уязвимостей.
  - `docs/ARCHITECTURE.md` с описанием слоёв и принятых решений.
  - `docs/PROTOCOLS.md` с деталями SOCKS5 и VLESS.
  - `CHANGELOG.md` (этот файл).

### Changed

- Парсеры SOCKS5 вынесены в отдельный модуль `parse.rs` для
  fuzz-тестирования. Публичный API `Socks5Handler` не изменился.
- `handle_client` в `skadi-server` теперь разветвляется по
  протоколу. На MVP активен только VLESS; SOCKS5 временно
  отключён из-за асимметрии в порядке отправки reply.

### Deprecated

Пока ничего.

### Removed

Пока ничего.

### Fixed

Пока ничего.

### Security

- Реализовано константное сравнение паролей и UUID.
- Добавлены лимиты на все поля парсеров.
- Добавлен таймаут на фазу переговоров.
- VLESS при неверном UUID не отправляет ответ — защита от
  активного зондирования.

**Известные ограничения безопасности** (см. `SECURITY.md`):

- VLESS работает **без TLS**. Трафик идёт в открытом виде.
  Не использовать в реальных условиях.
- Нет rate limiting на подключения.
- Нет изоляции пользователей друг от друга.
- Нет REALITY.

---

## Планы

Ближайшие крупные вещи, которые появятся в следующих записях:

- Интеграция `rustls` для TLS-транспорта.
- Интеграционные тесты полного цикла.
- Prometheus-метрики.
- REALITY через `rustls-reality`.
- gRPC API для управления пользователями.
- UDP over VLESS.
- TUN-режим.
- XHTTP.

---

## Формат записей

### Added
Для новых функций.

### Changed
Для изменений в существующей функциональности.

### Deprecated
Для функций, которые планируется удалить.

### Removed
Для удалённых функций.

### Fixed
Для исправленных багов.

### Security
Для всего, что связано с безопасностью. В том числе — для
описания известных ограничений, если они ещё не закрыты.

---

## Как вести этот файл

1. **Добавляйте запись при каждом значимом изменении.** Не ждите
   релиза — пишите в `[Unreleased]`.
2. **При релизе** — переименуйте `[Unreleased]` в
   `[X.Y.Z] - YYYY-MM-DD` и создайте новый пустой `[Unreleased]`.
3. **Пишите с точки зрения пользователя.** Не «отрефакторил
   парсер», а «парсер теперь не падает на битых пакетах».
4. **Ссылайтесь на issues и PR**, если они есть:
   `- Исправлена утечка памяти в handle_client ([#42])`.
5. **Security-записи** — обязательны. Даже если это «известное
   ограничение», оно должно быть зафиксировано.

<!-- Ссылки на будущие релизы добавляются сюда по мере выхода:
[Unreleased]: https://github.com/yourname/skadicore/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/yourname/skadicore/releases/tag/v0.1.0
-->