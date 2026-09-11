# SkadiCore — workflow для AI-агентов

## Перед началом задачи

1. Прочитать `.cursor/rules.md` и `.cursor/priorities.md`
2. Убедиться, что задача не нарушает зависимости этапов (`TODO.md`)
3. Создать ветку: `cursor/<описание>-ca71`
4. `cargo test --workspace` — baseline должен быть зелёным

## Во время разработки

### Парсеры

1. Логика в `parse.rs` — чистая функция
2. I/O в `handler.rs`
3. Юнит-тесты рядом с парсером
4. Fuzz-таргет в `crates/skadi-protocol/fuzz/`
5. 60 сек fuzz локально перед коммитом

### Транспорты

1. Новый файл в `skadi-transport/src/`
2. Реэкспорт в `lib.rs`
3. Не импортировать `skadi-protocol`

### Сервер

1. Конфиг → `skadi-server/src/config.rs`
2. Wiring → `main.rs` / отдельный модуль `session.rs` при росте
3. Валидация при `Config::load()`

## Проверка перед коммитом

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

## Коммит и PR

```bash
git add -A
git commit -m "Краткое описание изменения"
git push -u origin cursor/<ветка>-ca71
```

PR: draft по умолчанию, base `main`.

## Отладка

```bash
# Логи
RUST_LOG=skadi_protocol=debug,skadi_server=debug cargo run --bin skadicore

# SOCKS5 вручную
curl --socks5 127.0.0.1:1080 https://example.com

# Захват трафика
sudo tcpdump -i lo -X 'tcp port 1080'
```

## Документация

Обновлять при изменении поведения:

| Изменение | Файлы |
|-----------|-------|
| Новый протокол | `docs/PROTOCOLS.md`, `docs/CONFIGURATION.md` |
| Архитектура | `docs/ARCHITECTURE.md`, `.cursor/context.md` |
| Любая фича | `CHANGELOG.md` |
| Публичный API | `README.md` |

## Эскалация

- Неясность по REALITY/TLS — читать `docs/RISKS.md`, не изобретать
- Спорное архитектурное решение — ADR в `docs/adr/` (когда появится)
- Уязвимость — `SECURITY.md`, не публичный issue
