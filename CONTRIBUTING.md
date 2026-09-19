# Contributing to SkadiCore

Спасибо за интерес к проекту. SkadiCore — AGPL-3.0-or-later; вклад принимается через GitHub pull requests.

Участники соблюдают [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

## Перед началом

1. Прочитайте `README.md`, `docs/DEVELOPMENT.md` и `SECURITY.md`.
2. Убедитесь, что локально проходят проверки из CI (см. ниже).
3. Для крупных изменений лучше сначала обсудить задачу в issue, чтобы не дублировать работу.

## Окружение

- Rust из `rust-toolchain.toml` (`rustup` + `rustup show`).
- Опционально: `cargo-fuzz`, `cargo-audit`, `cargo-deny` — как в `docs/DEVELOPMENT.md`.

```bash
cargo build
cargo test -p skadi-core -p skadi-transport -p skadi-protocol -p skadi-client -p skadi-server
cargo fmt --all
cargo clippy -p skadi-core -p skadi-transport -p skadi-protocol -p skadi-api -p skadi-client -p skadi-server --all-targets
```

## Стиль и архитектура

- Один крейт — одна зона ответственности; не смешивайте парсеры, API и бинарник без причины.
- Type hints / явные типы в Rust; `thiserror` + `anyhow` по образцу существующего кода.
- Логирование только через `tracing`; без `print()` в production-коде.
- Минимальный diff: не рефакторить «заодно» соседние модули.
- Секреты и пути — только из конфига / `.env`, не в репозитории.

## Тесты

- Юнит-тесты рядом с кодом; интеграционные — в `crates/skadi-server/tests/`.
- E2E с Xray-core могут скачивать бинарник при первом запуске (см. `tests/common/xray.rs`); в CI установлены `curl` и `unzip`.
- Fuzz: `cargo fuzz run <target>` (nightly + `cargo-fuzz`).

## Документация

При изменении поведения обновляйте:

- `CHANGELOG.md` (секция `[Unreleased]`, формат Keep a Changelog)
- `TODO.md` и `.cursor/priorities.md` при сдвиге дорожной карты
- `docs/CONFIGURATION.md` / `docs/DEVELOPMENT.md` при новых опциях конфига или CLI
- При релизе — `docs/RELEASING.md` и версия в корневом `Cargo.toml`

## Pull requests

1. Ветка от `main` (или от ветки maintainer, если указано в issue).
2. Один логический PR — одна тема (фича, фикс, docs).
3. Описание: что сделано, зачем, как проверить (команды).
4. CI должен быть зелёным (`fmt`, `clippy`, `test`, `audit`, `deny` и прочие jobs из `.github/workflows/ci.yml`).

## Безопасность

Уязвимости не публикуйте в открытых issue. См. `SECURITY.md` (контакт и ожидания по раскрытию).

## Лицензия

Отправляя патч, вы соглашаетесь лицензировать его под **AGPL-3.0-or-later**, как и остальной код репозитория.
