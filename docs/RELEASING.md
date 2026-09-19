# Релизы SkadiCore

Проект следует [Semantic Versioning](https://semver.org/) и [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/).

## Версия в коде

- Workspace-версия: корневой `Cargo.toml`, поле `[workspace.package] version`.
- Все крейты используют `version.workspace = true`.
- Тег `vX.Y.Z` **должен совпадать** с версией в `Cargo.toml` (проверяется в `.github/workflows/release.yml`).

## Подготовка релиза

1. Собрать изменения в `CHANGELOG.md`: секция `[Unreleased]` → новая `[X.Y.Z] - YYYY-MM-DD`, оставить пустой `[Unreleased]`.
2. При необходимости обновить `README.md`, `docs/CONFIGURATION.md`, примеры.
3. Убедиться, что `main` зелёный в CI.
4. Закоммитить подготовку (без тега в том же коммите — тег ставится на уже смерженный `main`).

## Публикация (GitHub Releases)

Workflow **Release** (`.github/workflows/release.yml`) запускается при push тега `v*`:

```bash
git checkout main
git pull origin main
git tag -a v0.1.0 -m "SkadiCore 0.1.0"
git push origin v0.1.0
```

Артефакты: `skadicore-<platform>.tar.gz` / `.zip`, `SHA256SUMS`, при наличии секрета `MINISIGN_SECRET_KEY` — подписи `.minisig`.

### Подпись релизов (maintainer)

1. Один раз: `minisign -G -p minisign.pub -s minisign.key` (публичный ключ можно положить в репозиторий).
2. В GitHub: **Settings → Secrets and variables → Actions** → `MINISIGN_SECRET_KEY` = содержимое `minisign.key` (не коммитить).
3. Workflow подпишет артефакты и загрузит `*.minisig` на Release.

Пользователи проверяют: `./scripts/verify-release.sh minisign.pub dist`.

Повторная сборка без нового тега (ручной запуск):

- GitHub → Actions → **Release** → **Run workflow** → указать существующий тег, например `v0.1.0`.

## Проверка артефактов локально

```bash
./scripts/verify-release.sh minisign.pub dist
```

См. также `README.md` (раздел про GitHub Releases).

## После релиза

- Дальнейшие изменения — только в `[Unreleased]` в `CHANGELOG.md`.
- Для патчей: `0.1.1` и т.д.; для несовместимых API — мажор по semver.
